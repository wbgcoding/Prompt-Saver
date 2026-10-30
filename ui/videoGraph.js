// Video editor: compiles the operation list into the ffmpeg filter graph of the export.
import { contrastTextColor } from "./floatShared.js";
import { buildKeysFilters } from "./videoKeys.js";
import { $, clamp, FONT_FILES, fontsDir, formatHasAudio, MAX_SILENT_DIM, opOf, opsOf, outMs, overlayStack, SILENT_FORMATS, timelinePlan, totalOutMs, VE } from "./video.js";

// ---------------------------------------------------------------- filter graph

// A path inside a filter option passes two parsing levels and only one spelling survives both:
// wrapped in single quotes with exactly one backslash before the drive colon (checked against
// the shipped ffmpeg build; unquoted, unescaped and double-escaped all fail with "Invalid
// argument"). The caller supplies the quotes. An apostrophe cannot sit inside the quotes: it
// closes them, then a triple backslash plus quote survives the graph level as a backslash plus
// quote and the option level as the quote itself, and a new quote reopens the literal.
export function ffPath(path) {
  return String(path).replace(/\\/g, "/").replace(/:/g, "\\:").replace(/'/g, "'\\\\\\''");
}

// The apostrophe an exported text overlay actually shows (see ffText); the live preview uses the
// same substitution so it matches the result.
export const typographicApostrophe = (text) => String(text).replace(/'/g, "’");

// The same two levels for drawtext's own text, plus one thing the parser cannot express: an
// ASCII apostrophe is silently dropped by every escape form ffmpeg accepts (the
// quote-escape-requote spelling renders "its", not "it's"), so it becomes the typographic
// apostrophe. Backslashes, colons and the blanks at both ends are escaped in one pass, so no
// inserted backslash is ever escaped again. The option reader drops unescaped blanks at both
// ends of the value, quoted or not, while the preview keeps them (white-space: pre).
export function ffText(text) {
  return typographicApostrophe(String(text))
    .replace(/\r?\n/g, " ")
    .replace(/\\|:|^[ \t]+|[ \t]+$/g, (m) => [...m].map((c) => "\\" + c).join(""));
}

export const secs = (ms) => (ms / 1000).toFixed(3);
export const hexToFf = (hex, opacity) => `0x${String(hex).replace("#", "")}@${(clamp(opacity ?? 100, 0, 100) / 100).toFixed(2)}`;

// A stretch of source that survives trim + cuts, expressed on the OUTPUT
// timeline, which is what `enable=between(t,…)` reads.
export function enableExpr(op) {
  const a = outMs(clamp(op.startMs, 0, VE.dur)) / 1000;
  const b = outMs(clamp(op.endMs, 0, VE.dur), true) / 1000;
  if (b <= a) return null;
  return `enable='between(t,${a.toFixed(3)},${b.toFixed(3)})'`;
}

// A video overlay set to play once has nothing to show after its own footage ends; capping the
// configured end at its duration keeps `movie=` from being asked for frames past the end of its
// file (which broke the whole export). The footage plays at native speed on the output clock,
// so the cap is added to the start after both ends went through outMs(): a speed change or cut
// inside the range must not stretch or shrink the footage's length.
export function videoOverlayWindowOutMs(op) {
  const aOut = outMs(clamp(op.startMs, 0, VE.dur));
  const bOut = outMs(clamp(op.endMs, 0, VE.dur), true);
  const endOut = op.loop || !op.videoDurationMs ? bOut : Math.min(bOut, aOut + op.videoDurationMs);
  return { aOut, endOut };
}

function videoOverlayEnable(op) {
  const { aOut, endOut } = videoOverlayWindowOutMs(op);
  const a = aOut / 1000;
  const b = endOut / 1000;
  if (b <= a) return null;
  return `enable='between(t,${a.toFixed(3)},${b.toFixed(3)})'`;
}

// A finite repeat count rather than an unbounded loop: deterministic ffmpeg
// behaviour, and it needs no guess at a buffer size large enough to hold the
// whole thing. Native speed only: a video overlay's own clock is never
// adjusted to match a "speed" op on the main timeline.
function videoOverlayLoopArg(op) {
  if (!op.loop || !op.videoDurationMs) return "";
  const { aOut, endOut } = videoOverlayWindowOutMs(op);
  const neededMs = endOut - aOut;
  const count = Math.max(0, Math.ceil(neededMs / op.videoDurationMs) - 1);
  return count > 0 ? `:loop=${count}` : "";
}

// atempo only accepts 0.5-2.0 per stage, so a larger factor is split across
// several stages that multiply back to the requested rate.
function atempoChain(rate) {
  const stages = [];
  let remaining = rate;
  while (remaining > 2.0001) { stages.push(2); remaining /= 2; }
  while (remaining < 0.4999) { stages.push(0.5); remaining *= 2; }
  stages.push(Number(remaining.toFixed(6)));
  return stages.map((s) => `atempo=${s}`).join(",");
}

export const exportFormat = () => $("ve-export-format").value || "mp4";
export const exportQuality = () => $("ve-export-quality").value || "high";

// The size the picture has after the crop and before the scale: the crop rectangle when a
// crop exists, else the source. Scale presets, the percent field, the ratio lock, the size
// line and the bitrate estimate all read it, so they follow the crop.
export function effectiveSize() {
  const crop = opOf("crop");
  return crop ? { w: crop.rect.w, h: crop.rect.h } : { w: VE.w, h: VE.h };
}

// The scale op is the one resolution setting of the window: both the
// Edit-tab fields and the Export-tab summary read and write it directly.
export function scaleTarget() {
  const scaleOp = opOf("scale");
  return scaleOp ? { w: scaleOp.w, h: scaleOp.h } : null;
}

// ffmpeg autorotates a `-i` input from its display matrix but never a `movie=` source, so an
// inserted clip or video overlay shot in portrait on a phone would come out sideways. The probe
// reports the counter-clockwise quarter turns (ffprobe's rotation / 90); this applies the filter
// ffmpeg's own autorotate would (90 = transpose=cclock, 270 = transpose=clock, 180 = both flips),
// so the picture matches the preview and the main input. A missing value (an op saved before the
// probe reported it) is no turn.
function rotationFilter(quarters) {
  const q = (((Math.round(Number(quarters)) || 0) % 4) + 4) % 4;
  return q === 1 ? "transpose=cclock" : q === 2 ? "hflip,vflip" : q === 3 ? "transpose=clock" : "";
}

// Chains the rotation step after a movie= source and returns the label to continue from.
function turnedMovieLabel(op, src, label, parts) {
  const turn = rotationFilter(op.rotQuarters);
  if (!turn) return src;
  const out = label();
  parts.push(`[${src}]${turn}[${out}]`);
  return out;
}

// One inserted section clip's video, fps- and size-normalized to the main video's: two files
// rarely share a resolution or frame rate, and concat of mismatched streams either fails or
// drifts against its audio. `movie=` opens the file fresh, so no second `-i` is needed (see
// the Rust test for the fragment this produces against a real ffmpeg run).
function clipVideoLabel(op, label, parts, fps) {
  const src = label(), out = label();
  parts.push(`movie='${ffPath(op.path)}'[${src}]`);
  const turned = turnedMovieLabel(op, src, label, parts);
  parts.push(`[${turned}]fps=${fps},scale=${VE.w}:${VE.h}:force_original_aspect_ratio=decrease,pad=${VE.w}:${VE.h}:(ow-iw)/2:(oh-ih)/2:color=black,setpts=PTS-STARTPTS[${out}]`);
  return out;
}

// Every media file the graph reads besides the source (overlay pictures and
// videos, inserted clips, extra audio). The request names them so the backend can
// refuse a movie=/amovie= source that is anything else.
export const exportMediaPaths = () => [...new Set(VE.ops.map((o) => o.path).filter(Boolean))];

// The one place the operation list becomes ffmpeg. Produces a complete graph
// consuming [0:v] and ending on [vout]; the audio half is built separately below
// and ends on [aout], so the backend joins the two with ";" into a single
// -filter_complex and maps [vout]/[aout].
export function buildVideoChain() {
  const plan = timelinePlan();
  const mains = plan.filter((e) => e.type === "main");
  const hasClips = plan.some((e) => e.type === "clip");
  const crop = opOf("crop");
  const speed = opOf("speed");
  const fade = opOf("fade");
  const downscale = scaleTarget();
  const overlays = overlayStack();
  const gif = exportFormat() === "gif";
  const silentFormat = SILENT_FORMATS.has(exportFormat());
  const trimmed = mains.length !== 1 || hasClips || mains[0].a > 0 || mains[0].b < VE.dur;
  if (!trimmed && !crop && !speed && !fade && !downscale && !overlays.length && !silentFormat) return null;

  const parts = [];
  let counter = 0;
  const label = () => `v${counter++}`;
  let cur = label();

  // fps-normalize first: a screen capture is variable frame rate, and trimming
  // or concatenating a VFR stream drifts against its own audio.
  const fps = VE.fps > 0 ? Number(VE.fps.toFixed(3)) : 30;
  const mainLabels = [];
  if (mains.length > 1) {
    const splits = mains.map(() => label());
    parts.push(`[0:v]fps=${fps},split=${mains.length}${splits.map((l) => `[${l}]`).join("")}`);
    mains.forEach((seg, i) => {
      const out = label();
      parts.push(`[${splits[i]}]trim=start=${secs(seg.a)}:end=${secs(seg.b)},setpts=PTS-STARTPTS[${out}]`);
      mainLabels.push(out);
    });
  } else if (mains.length === 1) {
    const out = label();
    parts.push(`[0:v]fps=${fps},trim=start=${secs(mains[0].a)}:end=${secs(mains[0].b)},setpts=PTS-STARTPTS[${out}]`);
    mainLabels.push(out);
  }

  let mi = 0;
  const planLabels = plan.map((entry) => (entry.type === "main" ? mainLabels[mi++] : clipVideoLabel(entry.op, label, parts, fps)));
  if (planLabels.length > 1) {
    const out = label();
    parts.push(`${planLabels.map((l) => `[${l}]`).join("")}concat=n=${planLabels.length}:v=1:a=0[${out}]`);
    cur = out;
  } else if (planLabels.length === 1) {
    cur = planLabels[0];
  } else {
    // Everything trimmed away and no clip inserted: still needs a frame to
    // build the rest of the graph (scale, fades, ...) on.
    parts.push(`[0:v]fps=${fps},trim=start=0:end=0,setpts=PTS-STARTPTS[${cur}]`);
  }

  const chain = (filter) => {
    const out = label();
    parts.push(`[${cur}]${filter}[${out}]`);
    cur = out;
  };

  // Speed comes before the overlays because their `enable` ranges are already
  // expressed on the sped-up output timeline. The crop comes AFTER them: every
  // overlay position is a coordinate in the SOURCE frame, which is the frame the
  // preview shows them on: cropping first would shift each one by the crop
  // offset and push anything near an edge out of the picture entirely.
  if (speed && Math.abs(speed.rate - 1) > 0.001) chain(`setpts=PTS/${speed.rate}`);

  for (const op of overlays) {
    if (op.kind === "video" && op.missing) continue;
    const enable = op.kind === "video" ? videoOverlayEnable(op) : enableExpr(op);
    if (!enable) continue;
    if (op.kind === "image") {
      const src = label(), scaled = label(), out = label();
      parts.push(`movie='${ffPath(op.path)}'[${src}]`);
      parts.push(`[${src}]scale=${op.rect.w}:${op.rect.h},format=rgba,colorchannelmixer=aa=${(clamp(op.opacity, 5, 100) / 100).toFixed(2)}[${scaled}]`);
      parts.push(`[${cur}][${scaled}]overlay=${op.rect.x}:${op.rect.y}:${enable}[${out}]`);
      cur = out;
    } else if (op.kind === "video") {
      const src = label(), scaled = label(), out = label();
      const loopArg = videoOverlayLoopArg(op);
      // movie= decodes from its own t=0 in lockstep with the output timeline, so without a shift the
      // picture at the overlay's start would show the clip at that output time instead of its
      // beginning. The setpts shift moves the clip's first frame to the same startOutSec used for its
      // audio's adelay below, keeping picture and sound in sync.
      const startOutSec = (outMs(clamp(op.startMs, 0, VE.dur)) / 1000).toFixed(3);
      parts.push(`movie='${ffPath(op.path)}'${loopArg}[${src}]`);
      const turned = turnedMovieLabel(op, src, label, parts);
      parts.push(`[${turned}]setpts=PTS-STARTPTS+${startOutSec}/TB,scale=${op.rect.w}:${op.rect.h}:force_original_aspect_ratio=decrease,pad=${op.rect.w}:${op.rect.h}:(ow-iw)/2:(oh-ih)/2:color=black,format=rgba,colorchannelmixer=aa=${(clamp(op.opacity, 5, 100) / 100).toFixed(2)}[${scaled}]`);
      parts.push(`[${cur}][${scaled}]overlay=${op.rect.x}:${op.rect.y}:${enable}[${out}]`);
      cur = out;
    } else if (op.kind === "frame") {
      const color = hexToFf(op.color, op.opacity);
      chain(`drawbox=x=${op.rect.x}:y=${op.rect.y}:w=${op.rect.w}:h=${op.rect.h}:color=${color}:t=${op.thickness}:${enable}`);
      if (op.style === "double") {
        const gap = op.thickness * 2;
        chain(`drawbox=x=${op.rect.x + gap}:y=${op.rect.y + gap}:w=${Math.max(2, op.rect.w - 2 * gap)}:h=${Math.max(2, op.rect.h - 2 * gap)}:color=${color}:t=${op.thickness}:${enable}`);
      } else if (op.style === "soft") {
        // The preview shows a glow around the frame, so the softer box sits outside it.
        const halo = op.thickness * 3;
        chain(`drawbox=x=${op.rect.x - halo}:y=${op.rect.y - halo}:w=${op.rect.w + 2 * halo}:h=${op.rect.h + 2 * halo}:color=${hexToFf(op.color, (op.opacity || 100) * 0.4)}:t=${halo}:${enable}`);
      }
    } else if (op.kind === "text") {
      const font = FONT_FILES.find(([id]) => id === op.font) || FONT_FILES[0];
      const plate = op.back === "plate" ? `:box=1:boxcolor=black@0.5:boxborderw=${Math.max(4, Math.round(op.fontPx / 4))}` : "";
      const outline = op.back === "outline" ? `:borderw=${op.outlinePx || 3}:bordercolor=${hexToFf(contrastTextColor(op.color), 100)}` : "";
      // expansion=none keeps a caption containing % or {} literal instead of
      // running it through drawtext's own text expansion.
      chain(`drawtext=fontfile='${ffPath(`${fontsDir}/${font[1]}`)}':x=${op.x}:y=${op.y}:fontsize=${op.fontPx}:fontcolor=${hexToFf(op.color, 100)}:expansion=none${plate}${outline}:${enable}:text='${ffText(op.text)}'`);
    } else if (op.kind === "pixelate") {
      // Sampled down and blown back up with nearest-neighbour: a real mosaic,
      // not a blur that a sharpening pass could partly undo.
      const a = label(), b = label(), small = label(), out = label();
      parts.push(`[${cur}]split[${a}][${b}]`);
      parts.push(`[${b}]crop=${op.rect.w}:${op.rect.h}:${op.rect.x}:${op.rect.y},scale=${Math.max(1, Math.round(op.rect.w / op.blockPx))}:${Math.max(1, Math.round(op.rect.h / op.blockPx))},scale=${op.rect.w}:${op.rect.h}:flags=neighbor[${small}]`);
      parts.push(`[${a}][${small}]overlay=${op.rect.x}:${op.rect.y}:${enable}[${out}]`);
      cur = out;
    } else if (op.kind === "spotlight") {
      const a = label(), b = label(), clear = label(), dark = label(), out = label();
      parts.push(`[${cur}]split[${a}][${b}]`);
      parts.push(`[${b}]crop=${op.rect.w}:${op.rect.h}:${op.rect.x}:${op.rect.y}[${clear}]`);
      parts.push(`[${a}]drawbox=x=0:y=0:w=iw:h=ih:color=black@${op.dim.toFixed(2)}:t=fill:${enable}[${dark}]`);
      parts.push(`[${dark}][${clear}]overlay=${op.rect.x}:${op.rect.y}:${enable}[${out}]`);
      cur = out;
    } else if (op.kind === "keys") {
      for (const filter of buildKeysFilters(op)) chain(filter);
    }
  }

  if (crop) chain(`crop=${crop.rect.w}:${crop.rect.h}:${crop.rect.x}:${crop.rect.y}`);

  if (fade) {
    const total = totalOutMs() / 1000;
    if (fade.inMs > 0) chain(`fade=t=in:st=0:d=${(fade.inMs / 1000).toFixed(3)}`);
    if (fade.outMs > 0) chain(`fade=t=out:st=${Math.max(0, total - fade.outMs / 1000).toFixed(3)}:d=${(fade.outMs / 1000).toFixed(3)}`);
  }

  // The op itself already carries the decision (a preset, the percent field or
  // typed width/height, larger or smaller than the source). Enlarging gets the
  // sharper lanczos scaler instead of ffmpeg's default bicubic one.
  if (downscale) {
    const inW = crop ? crop.rect.w : VE.w;
    const inH = crop ? crop.rect.h : VE.h;
    const enlarging = downscale.w > inW || downscale.h > inH;
    chain(`scale=${downscale.w}:${downscale.h}${enlarging ? ":flags=lanczos" : ""}`);
  }

  // A GIF balloons in file size fast at a high resolution
  //: cap the long side regardless of any explicit scale op above.
  if (silentFormat) chain(`scale='min(iw,${MAX_SILENT_DIM})':'min(ih,${MAX_SILENT_DIM})':force_original_aspect_ratio=decrease`);

  if (gif) {
    // Palette generation and use in the SAME graph: two ffmpeg runs would mean
    // an intermediate file for nothing.
    const g0 = label(), g1 = label(), pal = label();
    parts.push(`[${cur}]fps=15,split[${g0}][${g1}]`);
    parts.push(`[${g0}]palettegen=stats_mode=diff[${pal}]`);
    parts.push(`[${g1}][${pal}]paletteuse=dither=bayer[vout]`);
    return parts.join(";");
  }

  parts.push(`[${cur}]null[vout]`);
  return parts.join(";");
}

// A silent placeholder matching one inserted clip's native (pre-speed) length; without it the
// sys/mic tracks would run short of the video by that length and drift out of sync with
// everything after.
function silentGapLabel(entry, label, parts) {
  return silentSpanLabel(entry.op.durationMs, label, parts);
}

function silentSpanLabel(ms, label, parts) {
  const out = label();
  parts.push(`anullsrc=channel_layout=stereo:sample_rate=48000,atrim=0:${secs(ms)},asetpts=PTS-STARTPTS[${out}]`);
  return out;
}

// An inserted clip's own sound, cut or padded to exactly its video length so
// everything after it stays in sync.
function clipAudioLabel(entry, label, parts) {
  const out = label();
  parts.push(`amovie='${ffPath(entry.op.path)}',aresample=48000,aformat=channel_layouts=stereo,apad,atrim=0:${secs(entry.op.durationMs)},asetpts=PTS-STARTPTS[${out}]`);
  return out;
}

// Audio half of the same graph: per track trim/cut/concat (with a silent gap
// standing in for every inserted clip, see silentGapLabel, except that one
// track, the clip host, plays the clips' own sound), volume, speed and
// fades, plus one delayed branch per audible video overlay, mixed down to one
// stream on [aout].
export function buildAudioChain() {
  if (!formatHasAudio(exportFormat())) return null;
  const plan = timelinePlan();
  const mains = plan.filter((e) => e.type === "main");
  const tracks = [];
  if (VE.hasSys) tracks.push({ index: 0, spec: "0:a:0", state: VE.audio.sys });
  if (VE.hasMic) tracks.push({ index: 1, spec: "0:a:1", state: VE.audio.mic });
  const videoOverlaysWithAudio = opsOf("overlay").filter((o) => o.kind === "video" && !o.missing && o.hasAudio && o.volume > 0);
  const clipsWithAudio = plan.filter((e) => e.type === "clip" && e.op.hasAudio);
  if (!tracks.length && !videoOverlaysWithAudio.length && !clipsWithAudio.length) return null;
  // Clip sound goes into ONE track, or it would be mixed in twice. The first
  // track that is not muted hosts it; with none (no recorded audio, or all
  // of it muted) a dedicated track (silent where the main video plays) carries it.
  let clipHost = null;
  if (clipsWithAudio.length) {
    clipHost = tracks.find((tr) => !tr.state.mute);
    if (!clipHost) {
      // No unmuted recorded track (or none at all): a muted host would zero
      // the clip's sound too, so it gets a track of its own.
      clipHost = { index: -1, spec: null, state: { mute: false, vol: 100 } };
      tracks.push(clipHost);
    }
  }

  const speed = opOf("speed");
  const fade = opOf("fade");
  const rate = speed ? speed.rate : 1;
  const trimmed = mains.length !== 1 || plan.length !== mains.length || mains[0].a > 0 || mains[0].b < VE.dur;
  const touched = trimmed || Math.abs(rate - 1) > 0.001 || !!fade
    || tracks.some((tr) => tr.state.mute || tr.state.vol !== 100) || tracks.length > 1 || videoOverlaysWithAudio.length > 0;
  if (!touched) return null;

  const parts = [];
  let counter = 0;
  const label = () => `a${counter++}`;
  const outs = [];

  for (const track of tracks) {
    const silentTrack = track.spec === null;
    const mainSplits = mains.length > 1 && !silentTrack ? mains.map(() => label()) : null;
    if (mainSplits) parts.push(`[${track.spec}]asplit=${mains.length}${mainSplits.map((l) => `[${l}]`).join("")}`);
    let mi = 0;
    const segLabels = plan.map((entry) => {
      if (entry.type !== "main") return track === clipHost && entry.op.hasAudio ? clipAudioLabel(entry, label, parts) : silentGapLabel(entry, label, parts);
      if (silentTrack) return silentSpanLabel(entry.b - entry.a, label, parts);
      const out = label();
      const src = mains.length > 1 ? mainSplits[mi++] : track.spec;
      parts.push(`[${src}]atrim=start=${secs(entry.a)}:end=${secs(entry.b)},asetpts=PTS-STARTPTS[${out}]`);
      return out;
    });
    let cur;
    if (segLabels.length > 1) {
      const out = label();
      parts.push(`${segLabels.map((l) => `[${l}]`).join("")}concat=n=${segLabels.length}:v=0:a=1[${out}]`);
      cur = out;
    } else {
      cur = segLabels[0];
    }
    const chain = (filter) => {
      const out = label();
      parts.push(`[${cur}]${filter}[${out}]`);
      cur = out;
    };
    const volume = track.state.mute ? 0 : clamp(track.state.vol, 0, 200) / 100;
    chain(`volume=${volume.toFixed(2)}`);
    if (Math.abs(rate - 1) > 0.001) {
      // Keeping the pitch is atempo's job; letting it move is what a plain
      // sample-rate change does, which is the "chipmunk" result some clips want.
      chain(speed && speed.pitch === false ? `asetrate=48000*${rate},aresample=48000` : atempoChain(rate));
    }
    if (fade) {
      const total = totalOutMs() / 1000;
      if (fade.inMs > 0) chain(`afade=t=in:st=0:d=${(fade.inMs / 1000).toFixed(3)}`);
      if (fade.outMs > 0) chain(`afade=t=out:st=${Math.max(0, total - fade.outMs / 1000).toFixed(3)}:d=${(fade.outMs / 1000).toFixed(3)}`);
    }
    outs.push(cur);
  }

  // Each video overlay's audio is a separate branch, delayed to land at its start on the output
  // timeline, rather than folded into the sys/mic tracks, which already carry a silent gap for
  // every inserted clip; folding it in would mix it twice.
  for (const op of videoOverlaysWithAudio) {
    const { aOut: startOutMs, endOut } = videoOverlayWindowOutMs(op);
    const durSec = (endOut - startOutMs) / 1000;
    if (durSec <= 0) continue;
    const loopArg = videoOverlayLoopArg(op);
    const src = label(), out = label();
    parts.push(`amovie='${ffPath(op.path)}'${loopArg}[${src}]`);
    const vol = clamp(op.volume, 0, 200) / 100;
    const delayMs = Math.max(0, Math.round(startOutMs));
    parts.push(`[${src}]aresample=48000,aformat=channel_layouts=stereo,atrim=0:${durSec.toFixed(3)},asetpts=PTS-STARTPTS,adelay=${delayMs}|${delayMs},volume=${vol.toFixed(2)}[${out}]`);
    outs.push(out);
  }

  if (!outs.length) return null;
  if (outs.length > 1) parts.push(`${outs.map((l) => `[${l}]`).join("")}amix=inputs=${outs.length}:normalize=0[aout]`);
  else parts.push(`[${outs[0]}]anull[aout]`);
  return parts.join(";");
}

// A pure trim with nothing else touched can be copied instead of re-encoded:
// instant, and the picture is bit-identical to the source. The result says what happens to the
// sound: "all" copies every audio stream, "first" keeps only the first one (the mic is muted),
// "mix" copies the picture and mixes both tracks into one AAC stream (a recording with a mic
// carries two streams, and most players only play the first). null = a real re-encode.
export function copyAudioMode() {
  // Only mp4 can carry an h264/aac stream untouched; the other three
  // formats always need a real re-encode to become themselves at all.
  if (exportFormat() !== "mp4") return null;
  if (exportQuality() === "lossless") return null; // an explicit re-encode was asked for
  // A source whose own streams are not already safe to copy (h264/hevc video,
  // aac-or-silent audio: e.g. a webm/VP9 file dragged in from elsewhere)
  // fails ffmpeg's "-c copy" outright instead of producing a trimmed export.
  if (VE.probe && VE.probe.stream_copy_safe === false) return null;
  if (opsOf("cut").length) return null;
  if (opOf("crop") || opOf("speed") || opOf("fade") || opOf("scale") || opsOf("overlay").length) return null;
  // An inserted section only exists as a movie= source in the filter graph,
  // which a plain copy of the main file would leave out.
  if (opsOf("clip").some((c) => !c.missing)) return null;
  const { sys, mic } = VE.audio;
  const plain = (track) => !track.mute && track.vol === 100;
  // A track the recording does not have has no state to compare: its mute button is
  // disabled and its default (muted) would otherwise rule the copy out for good.
  if (!VE.hasMic) return !VE.hasSys || plain(sys) ? "all" : null;
  // A mic without a system track is the only stream, so there is nothing to mix.
  if (!VE.hasSys) return plain(mic) ? "all" : null;
  if (!plain(sys)) return null;
  if (mic.mute) return "first";
  return plain(mic) ? "mix" : null;
}

export function isLosslessCopy() {
  return copyAudioMode() !== null;
}

// The audio graph of the "mix" copy. The trim is not part of it: the backend cuts the input at
// the start point, so the sound begins where the copied picture begins.
export function buildCopyMixChain() {
  return "[0:a:0][0:a:1]amix=inputs=2:normalize=0[aout]";
}
