// Video editor: the overlay list and inserting another video as an overlay or section.
import { numberField, parseFfmpegError } from "./floatShared.js";
import { attachSelectPicker } from "./selectpop.js";
import { placePopoverAt } from "./popover.js";
import { keysPickerRow } from "./videoKeys.js";
import { $, clamp, even, EXTRA_CLIPS_MAX, extraClipCount, fmtTime, FONT_FILES, invoke, keptSegments, openColorPop, opOf, opsOf, OVERLAY_COLORS, overlayLabelKey, parseTime, pushOp, pushReplacing, removeOps, renderRegions, settings, showPanel, speedRate, t, textsReady, toast, TXT_READY, updateNudgeHint, VE } from "./video.js";
import { renderRects, snapRect } from "./videoPreview.js";

// ---------------------------------------------------------------- overlays

let overlaySeq = 0;
const newId = () => `ov${++overlaySeq}-${Date.now().toString(36)}`;

function defaultRange() {
  if (VE.sel) return { startMs: Math.min(VE.sel.a, VE.sel.b), endMs: Math.max(VE.sel.a, VE.sel.b) };
  const trim = opOf("trim");
  return { startMs: trim ? trim.startMs : 0, endMs: trim ? trim.endMs : VE.dur };
}

export function centeredRect(fraction) {
  const w = even(VE.w * fraction), h = even(VE.h * fraction);
  return snapRect({ x: (VE.w - w) / 2, y: (VE.h - h) / 2, w, h });
}

export async function addOverlay(kind) {
  // Same race as startAddVideo: a button live from first paint that builds text with t() at
  // click time needs the lazy i18n import landed first.
  if (!TXT_READY) await textsReady;
  const range = defaultRange();
  const base = { tool: "overlay", id: newId(), kind, ...range };
  let op = null;
  if (kind === "image") {
    const path = await invoke("pick_file_path").catch(() => null);
    if (!path) return;
    // An apostrophe in the path has no escape ffmpeg's filter parser accepts, so
    // it is refused here with a reason instead of failing at export time.
    if (path.includes("'")) { toast(t("veOvImageQuote")); return; }
    const rect = centeredRect(0.3);
    op = { ...base, path, rect, opacity: VE.ovImageOpacityDefault ?? 100 };
  } else if (kind === "frame") {
    op = { ...base, rect: snapRect({ x: 0, y: 0, w: VE.w, h: VE.h }), color: VE.ovFrameColorDefault, thickness: VE.ovLastFrameThickness ?? Math.max(2, even(VE.h * 0.01)), style: "solid", opacity: VE.ovFrameOpacityDefault ?? 100 };
  } else if (kind === "text") {
    op = { ...base, text: t("veOvTextDefault"), x: even(VE.w * 0.08), y: even(VE.h * 0.08), fontPx: VE.ovLastTextFontPx ?? Math.max(12, Math.round(VE.h / 16)), font: VE.ovFontDefault || FONT_FILES[0][0], color: VE.ovTextColorDefault, back: "plate", outlinePx: VE.ovTextOutlineDefault ?? 3 };
  } else if (kind === "pixelate") {
    op = { ...base, rect: centeredRect(0.35), blockPx: VE.ovPixelateBlockDefault ?? 16 };
  } else if (kind === "spotlight") {
    op = { ...base, rect: centeredRect(0.45), dim: VE.ovSpotlightDimDefault ?? 0.6 };
  } else if (kind === "keys") {
    op = {
      ...base, rect: centeredRect(0.3), preset: "pressed", keys: [],
      color: VE.ovColorDefault, activeColor: VE.ovKeysActiveColorDefault ?? "#3fa7ff", opacity: 100,
    };
  }
  if (!op) return;
  VE.selOv = op.id;
  pushOp(op);
  showPanel("ve-panel-overlays");
}

// ---------------------------------------------------------------- add a video

// At most EXTRA_CLIPS_MAX extra sources, each checked with ffprobe once up front, so an
// unreadable file is rejected right away with the same wording an export failure uses.
async function pickExtraVideo() {
  if (extraClipCount() >= EXTRA_CLIPS_MAX) {
    toast(t("veClipLimit").replace("{n}", String(EXTRA_CLIPS_MAX)));
    return null;
  }
  const path = await invoke("pick_file_path").catch(() => null);
  if (!path) return null;
  if (path.includes("'")) { toast(t("veOvImageQuote")); return null; }
  try {
    const probe = await invoke("probe_extra_video_source", { path });
    return { path, probe };
  } catch (err) {
    const { message } = parseFfmpegError(err, t);
    toast(message);
    return null;
  }
}

// A camera recording opened alongside its companion screen recording lands here as an aligned
// overlay instead of through the file picker (createVideoOverlay above), placed bottom-right as
// picture-in-picture, the usual spot for a webcam layer.
//
// Alignment: the camera's sidecar (written by camera_finish_recording) already carries the
// final overlay offset, read live by the Rust side from the screen recording's pause-aware
// position when the camera started. Reconstructing it from two wall-clock stamps would inflate
// it by any paused span between the starts.
//
// `gen` is the load generation the call belongs to: a second recording opened while one of the
// awaits below is pending must not receive this overlay.
export async function addAlignedCameraOverlay(path, gen) {
  if (!path) return;
  if (VE.ops.some((o) => o.tool === "overlay" && o.kind === "video" && o.path === path)) return;
  let probe;
  try {
    probe = await invoke("probe_extra_video_source", { path });
  } catch {
    // The recording still exists on disk: the overlays panel's own "add a
    // video" picker reaches it by hand if this automatic probe fails.
    return;
  }
  if (VE.loadGen !== gen) return;
  let startMs = 0;
  try {
    const sidecar = await invoke("read_camera_sidecar", { path });
    if (sidecar && sidecar.overlay_start_ms != null) startMs = sidecar.overlay_start_ms;
  } catch {
    // No sidecar: 0 (both at the same point) is still a sane fallback,
    // never a missing overlay.
  }
  if (VE.loadGen !== gen) return;
  const durationMs = Number(probe.duration_ms) || 0;
  const w = even(VE.w * 0.3), h = even(VE.h * 0.3);
  const margin = even(Math.min(VE.w, VE.h) * 0.03);
  const rect = snapRect({ x: VE.w - w - margin, y: VE.h - h - margin, w, h });
  const op = {
    tool: "overlay", id: newId(), kind: "video",
    startMs, endMs: Math.min(VE.dur, startMs + durationMs || VE.dur),
    path, rect, opacity: VE.ovImageOpacityDefault ?? 100,
    volume: 0, loop: false,
    hasAudio: !!probe.has_audio, videoDurationMs: durationMs,
  };
  VE.selOv = op.id;
  pushOp(op);
}

function createVideoOverlay({ path, probe }) {
  const range = defaultRange();
  const rect = centeredRect(0.3);
  const op = {
    tool: "overlay", id: newId(), kind: "video", ...range,
    path, rect, opacity: VE.ovImageOpacityDefault ?? 100,
    // A freshly added video overlay carries no sound until asked to.
    volume: VE.ovVideoVolumeDefault ?? 0, loop: false,
    hasAudio: !!probe.has_audio, videoDurationMs: Number(probe.duration_ms) || 0,
  };
  VE.selOv = op.id;
  pushOp(op);
  showPanel("ve-panel-overlays");
}

// Projects VE.playhead (a source ms, possibly inside a cut region) onto the output-cumulative
// axis timelinePlan() splits a "cut"-position clip against, so "at the current position" means
// what it shows on screen. A playhead inside a cut region lands on the next kept segment's start.
export function clipAtMsFromPlayhead() {
  const segs = keptSegments();
  const rate = speedRate();
  let cum = 0;
  for (const s of segs) {
    if (VE.playhead <= s.b) return cum + (clamp(VE.playhead, s.a, s.b) - s.a) / rate;
    cum += (s.b - s.a) / rate;
  }
  return cum;
}

function createClip({ path, probe }, position) {
  const op = {
    tool: "clip", id: newId(), path, position,
    name: path.split(/[\\/]/).pop() || path,
    durationMs: Number(probe.duration_ms) || 0,
    hasAudio: !!probe.has_audio, missing: false,
  };
  if (position === "cut") op.atMs = clipAtMsFromPlayhead();
  pushOp(op);
}

// Right-click menu for duplicating/deleting an overlay row, mirroring the
// screenshot editor's own equivalent context menu one for one.
export const ovCtxPop = document.createElement("div");
ovCtxPop.className = "ctx hidden";
ovCtxPop.setAttribute("role", "menu");
const ovCtxDupBtn = document.createElement("button");
ovCtxDupBtn.type = "button";
const ovCtxDelBtn = document.createElement("button");
ovCtxDelBtn.type = "button";
ovCtxDelBtn.className = "danger";
ovCtxPop.append(ovCtxDupBtn, ovCtxDelBtn);
document.body.appendChild(ovCtxPop);
let ovCtxOp = null;
export function closeOvCtxPop() { ovCtxPop.classList.add("hidden"); ovCtxOp = null; }
function openOvCtxPop(op, x, y) {
  ovCtxOp = op;
  ovCtxDupBtn.textContent = t("duplicate");
  ovCtxDelBtn.textContent = t("delete");
  placePopoverAt(ovCtxPop, x, y);
}
ovCtxDupBtn.addEventListener("click", () => {
  const op = ovCtxOp;
  closeOvCtxPop();
  if (op) duplicateOverlay(op);
});
ovCtxDelBtn.addEventListener("click", () => {
  const op = ovCtxOp;
  closeOvCtxPop();
  if (!op) return;
  if (VE.selOv === op.id) VE.selOv = null;
  removeOps((o) => o.id === op.id);
});
document.addEventListener("pointerdown", (e) => {
  if (ovCtxPop.classList.contains("hidden") || ovCtxPop.contains(e.target)) return;
  closeOvCtxPop();
});

function menuButton(label, onClick, id) {
  const b = document.createElement("button");
  b.type = "button";
  if (id) b.id = id;
  b.textContent = label;
  b.addEventListener("click", onClick);
  return b;
}

// A second, generic popover for the "Add a video" flow, appended after ovCtxPop so
// document.querySelector('.ctx') still finds that one first while it is the only one open.
export const addVideoPop = document.createElement("div");
addVideoPop.id = "ve-add-video-pop";
addVideoPop.className = "ctx hidden";
addVideoPop.setAttribute("role", "menu");
document.body.appendChild(addVideoPop);
export function closeAddVideoPop() { addVideoPop.classList.add("hidden"); addVideoPop.innerHTML = ""; }
document.addEventListener("pointerdown", (e) => {
  if (addVideoPop.classList.contains("hidden") || addVideoPop.contains(e.target) || e.target.closest?.("#ve-add-video")) return;
  closeAddVideoPop();
});

function showClipPositionMenu(picked, x, y) {
  addVideoPop.innerHTML = "";
  addVideoPop.append(
    menuButton(t("veClipAtStart"), () => { closeAddVideoPop(); createClip(picked, "start"); }),
    menuButton(t("veClipAtEnd"), () => { closeAddVideoPop(); createClip(picked, "end"); }),
    menuButton(t("veClipAtCut"), () => { closeAddVideoPop(); createClip(picked, "cut"); }),
  );
  placePopoverAt(addVideoPop, x, y);
}

// Both destinations share one selection path: pick and validate the file once, then choose
// where it goes.
//
// The limit toast and the menu below are t() calls made at click time: the button is live from
// first paint, before the lazy i18n import has necessarily landed, so a fast click would
// otherwise freeze raw i18n keys into the toast and popup.
export async function startAddVideo(x, y) {
  if (!TXT_READY) await textsReady;
  const picked = await pickExtraVideo();
  if (!picked) return;
  addVideoPop.innerHTML = "";
  addVideoPop.append(
    menuButton(t("veAddAsOverlay"), () => { closeAddVideoPop(); createVideoOverlay(picked); }, "ve-add-video-as-overlay"),
    menuButton(t("veAddAsClip"), () => { showClipPositionMenu(picked, x, y); }, "ve-add-video-as-clip"),
  );
  placePopoverAt(addVideoPop, x, y);
}

// A fresh push at the end, never spliced next to the source: undo() pops the last entry, so a
// mid-array insert would make undo remove the wrong operation.
function duplicateOverlay(op) {
  const offset = VE.ovDupOffsetPx;
  const dup = { ...op, id: newId() };
  // A duplicate is a brand new op, not a continuation of the original's undo history: copying
  // the _prev chain would make a single undo step that chain back instead of removing the duplicate.
  delete dup._prev;
  delete dup.z;
  if (dup.rect) dup.rect = snapRect({ ...dup.rect, x: dup.rect.x + offset, y: dup.rect.y + offset });
  if (dup.kind === "text") { dup.x = clamp(dup.x + offset, 0, VE.w); dup.y = clamp(dup.y + offset, 0, VE.h); }
  VE.selOv = dup.id;
  pushOp(dup);
  showPanel("ve-panel-overlays");
}

export function renderOverlayList() {
  const list = $("ve-ov-list");
  list.innerHTML = "";
  const overlays = opsOf("overlay");
  $("ve-ov-empty").classList.toggle("hidden", overlays.length > 0);
  for (const op of overlays) {
    const row = document.createElement("button");
    row.type = "button";
    row.className = "ve-ov-row" + (op.id === VE.selOv ? " sel" : "");
    row.textContent = `${t(overlayLabelKey(op.kind))} · ${fmtTime(op.startMs)}-${fmtTime(op.endMs)}`;
    if (op.kind === "video") {
      const bits = [];
      if (!op.hasAudio || !(op.volume > 0)) bits.push(t("veClipMuted"));
      if (op.missing) bits.push(t("veClipMissing"));
      if (bits.length) row.textContent += ` · ${bits.join(" · ")}`;
    }
    row.addEventListener("click", () => {
      VE.selOv = VE.selOv === op.id ? null : op.id;
      renderOverlayList();
      renderOverlayEditor();
      renderRects();
      renderRegions();
      updateNudgeHint();
    });
    row.addEventListener("contextmenu", (e) => {
      e.preventDefault();
      openOvCtxPop(op, e.clientX, e.clientY);
    });
    list.appendChild(row);
  }
}

function clipPositionLabel(op) {
  if (op.position === "start") return t("veClipAtStart");
  if (op.position === "end") return t("veClipAtEnd");
  return `${t("veClipAtCut")} · ${fmtTime(op.atMs)}`;
}

// An inserted section shows as a placeholder with its name and length, never
// a second video picture in the preview.
export function renderClipList() {
  const list = $("ve-clip-list");
  if (!list) return;
  list.innerHTML = "";
  const clips = opsOf("clip");
  list.classList.toggle("hidden", clips.length === 0);
  for (const op of clips) {
    const row = document.createElement("div");
    row.className = "ve-op-row";
    const label = document.createElement("span");
    label.className = "ve-op-label";
    const bits = [op.name, fmtTime(op.durationMs), clipPositionLabel(op)];
    if (!op.hasAudio) bits.push(t("veClipMuted"));
    if (op.missing) bits.push(t("veClipMissing"));
    label.textContent = bits.join(" · ");
    row.appendChild(label);
    const del = document.createElement("button");
    del.type = "button";
    del.className = "icon-btn ve-op-del";
    del.innerHTML = '<svg viewBox="0 0 24 24" width="15" height="15" aria-hidden="true"><path fill="currentColor" d="M6.4 5 12 10.6 17.6 5 19 6.4 13.4 12 19 17.6 17.6 19 12 13.4 6.4 19 5 17.6 10.6 12 5 6.4Z"/></svg>';
    del.title = t("veOpRemove");
    del.setAttribute("aria-label", `${t("veOpRemove")}: ${op.name}`);
    del.addEventListener("click", () => removeOps((o) => o === op));
    row.appendChild(del);
    list.appendChild(row);
  }
}

// Small builders so every overlay field looks like the rest of the app's fields
// instead of each panel inventing its own row shape.
function fieldRow(labelText, control) {
  const label = document.createElement("label");
  label.className = "field";
  const span = document.createElement("span");
  span.textContent = labelText;
  label.append(span, control);
  return label;
}

// Thin positional-args shim over the shared numberField() (click/hold/wheel/preset behaviour
// lives in floatShared.js).
export function numberInput(value, min, max, step, onChange, presets) {
  return numberField({
    value, min, max, step, presets, onChange,
    decreaseLabel: t("numFieldDecrease"),
    increaseLabel: t("numFieldIncrease"),
    presetsLabel: t("numFieldPresets"),
    wheelEnabled: settings?.ui_flags?.numFieldWheel !== false,
  });
}

function textInput(value, onChange) {
  const input = document.createElement("input");
  input.type = "text";
  input.className = "modal-input";
  input.value = value;
  input.addEventListener("change", () => onChange(input.value));
  return input;
}

function selectInput(options, value, onChange) {
  const select = document.createElement("select");
  select.className = "modal-input";
  for (const [val, label] of options) {
    const option = document.createElement("option");
    option.value = val;
    option.textContent = label;
    select.appendChild(option);
  }
  select.value = value;
  select.addEventListener("change", () => onChange(select.value));
  attachSelectPicker(select);
  return select;
}

export function colorRow(current, onPick) {
  const wrap = document.createElement("div");
  wrap.className = "swatches ve-swatches";
  for (const hex of OVERLAY_COLORS) {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "swatch" + (hex.toLowerCase() === String(current).toLowerCase() ? " sel" : "");
    btn.style.background = hex;
    btn.title = hex;
    btn.setAttribute("aria-label", hex);
    btn.addEventListener("click", () => onPick(hex));
    wrap.appendChild(btn);
  }
  // The shared custom-colour popup: shows the current colour when it is not one of the fixed
  // presets above, using the same swatch pattern as editor.js's buildSwatches().
  const isCustom = !!current && !OVERLAY_COLORS.some((c) => c.toLowerCase() === String(current).toLowerCase());
  const custom = document.createElement("button");
  custom.type = "button";
  custom.className = "swatch custom" + (isCustom ? " sel" : "");
  if (isCustom) custom.style.background = current;
  custom.title = t("colorCustom");
  custom.setAttribute("aria-label", t("colorCustom"));
  custom.addEventListener("click", () => openColorPop(custom, isCustom ? current : "", onPick));
  wrap.appendChild(custom);
  return wrap;
}

// The editor panel keeps its field handlers (and the `op` they closed over)
// alive across edits, so a handler can hold a copy older than the overlay now in
// VE.ops. Every edit starts from the live one; a stale copy would write its old
// values back over fields changed since.
export const liveOverlay = (op) => VE.ops.find((o) => o.id === op.id) || op;

// True while `op` is still in the recording that was loaded when `gen` was read. A picker or a
// probe awaited in between can outlive the overlay (removed) or the recording (another one
// loaded), and committing then would push the old overlay back in.
export const overlayStillInEditor = (op, gen) => VE.loadGen === gen && VE.ops.some((o) => o.id === op.id);

export function commitOverlay(op, changes) {
  pushReplacing((o) => o.id === op.id, { ...liveOverlay(op), ...changes }, { keepEditor: true });
  rememberOverlayValues(op.kind, changes);
}

// Keeps the colour/thickness/opacity/font size a kind was last given as the starting point for
// the next overlay of that kind, in the settings store (set_ui_text/set_ui_value) rather than
// the per-file edit state, since it belongs to the user. Also updates the live VE.ov*Default
// fields at once: those commands only broadcast "settings-changed" to the floating-pill and
// pill-folder windows (see notify_float_settings), never to this one.
function rememberOverlayValues(kind, changes) {
  if (kind === "text") {
    if (changes.color !== undefined) {
      VE.ovTextColorDefault = changes.color;
      invoke("set_ui_text", { key: "veOvLastTextColor", value: changes.color }).catch(() => {});
    }
    if (changes.fontPx !== undefined) {
      VE.ovLastTextFontPx = changes.fontPx;
      invoke("set_ui_value", { key: "veOvLastTextFontPx", value: changes.fontPx }).catch(() => {});
    }
  } else if (kind === "frame") {
    if (changes.color !== undefined) {
      VE.ovFrameColorDefault = changes.color;
      invoke("set_ui_text", { key: "veOvLastFrameColor", value: changes.color }).catch(() => {});
    }
    if (changes.thickness !== undefined) {
      VE.ovLastFrameThickness = changes.thickness;
      invoke("set_ui_value", { key: "veOvLastFrameThickness", value: changes.thickness }).catch(() => {});
    }
    if (changes.opacity !== undefined) {
      VE.ovFrameOpacityDefault = changes.opacity;
      invoke("set_ui_value", { key: "veOvLastFrameOpacity", value: changes.opacity }).catch(() => {});
    }
  } else if (kind === "pixelate") {
    if (changes.blockPx !== undefined) {
      VE.ovPixelateBlockDefault = changes.blockPx;
      invoke("set_ui_value", { key: "veOvLastPixelateBlock", value: changes.blockPx }).catch(() => {});
    }
  } else if (kind === "spotlight") {
    if (changes.dim !== undefined) {
      VE.ovSpotlightDimDefault = changes.dim;
      invoke("set_ui_value", { key: "veOvLastSpotlightDim", value: Math.round(changes.dim * 100) }).catch(() => {});
    }
  } else if (kind === "image") {
    if (changes.opacity !== undefined) {
      VE.ovImageOpacityDefault = changes.opacity;
      invoke("set_ui_value", { key: "veOvLastImageOpacity", value: changes.opacity }).catch(() => {});
    }
  }
}

// Registers an overlay-editor input so a later keepEditor commit can refresh it in place (see
// syncOverlayFields) instead of rebuilding the panel. `control` is what a caller focuses or
// reads (the input itself or a numberField() wrapper), so the [data-ov-field] marker goes on
// the actual <input>, which is focusable and has a `.value`, unlike the wrapper div.
function ovTrack(key, control, getValue) {
  const focusEl = control.matches("input") ? control : control.querySelector("input") || control;
  focusEl.dataset.ovField = key;
  VE.ovFields[key] = { input: control, getValue };
  return control;
}

export function renderOverlayEditor() {
  const box = $("ve-ov-editor");
  box.innerHTML = "";
  VE.ovFields = {};
  const op = VE.ops.find((o) => o.id === VE.selOv);
  if (!op) { box.classList.add("hidden"); return; }
  box.classList.remove("hidden");

  const head = document.createElement("h3");
  head.className = "ve-h";
  head.textContent = t(overlayLabelKey(op.kind));
  box.appendChild(head);

  // Pixelate and spotlight cover one fixed rectangle for one time range. A target
  // that moves needs a second overlay with its own range and position.
  if (op.kind === "pixelate" || op.kind === "spotlight") {
    const note = document.createElement("p");
    note.className = "modal-hint";
    note.textContent = t("veOvRectTip");
    box.appendChild(note);
  }

  const range = document.createElement("div");
  range.className = "ve-row2";
  const startInput = ovTrack("startMs", textInput(fmtTime(op.startMs), (v) => {
    const ms = parseTime(v);
    if (ms != null) commitOverlay(op, { startMs: clamp(ms, 0, liveOverlay(op).endMs) });
    else renderOverlayEditor();
  }), (o) => fmtTime(o.startMs));
  const endInput = ovTrack("endMs", textInput(fmtTime(op.endMs), (v) => {
    const ms = parseTime(v);
    if (ms != null) commitOverlay(op, { endMs: clamp(ms, liveOverlay(op).startMs, VE.dur) });
    else renderOverlayEditor();
  }), (o) => fmtTime(o.endMs));
  range.append(fieldRow(t("veOvFrom"), startInput), fieldRow(t("veOvTo"), endInput));
  box.appendChild(range);

  const useSel = document.createElement("button");
  useSel.type = "button";
  useSel.className = "ghost-btn ve-wide ve-use-sel";
  useSel.textContent = t("veOvUseSelection");
  useSel.disabled = !VE.sel;
  useSel.addEventListener("click", () => {
    if (!VE.sel) return;
    commitOverlay(op, { startMs: Math.min(VE.sel.a, VE.sel.b), endMs: Math.max(VE.sel.a, VE.sel.b) });
  });
  box.appendChild(useSel);

  if (op.kind === "image") {
    const pathLine = document.createElement("p");
    pathLine.className = "ve-path";
    pathLine.textContent = op.path;
    pathLine.title = op.path;
    box.appendChild(pathLine);
    const change = document.createElement("button");
    change.type = "button";
    change.className = "ghost-btn ve-wide";
    change.textContent = t("pathChange");
    change.addEventListener("click", async () => {
      const gen = VE.loadGen;
      const path = await invoke("pick_file_path").catch(() => null);
      if (!path || !overlayStillInEditor(op, gen)) return;
      if (path.includes("'")) { toast(t("veOvImageQuote")); return; }
      commitOverlay(op, { path });
    });
    box.appendChild(change);
    box.appendChild(fieldRow(t("veOvOpacity"), ovTrack("opacity", numberInput(op.opacity, 5, 100, 5, (v) => commitOverlay(op, { opacity: v }), [25, 50, 75, 100]), (o) => String(o.opacity))));
  }

  if (op.kind === "video") {
    const pathLine = document.createElement("p");
    pathLine.className = "ve-path";
    pathLine.textContent = op.path;
    pathLine.title = op.path;
    box.appendChild(pathLine);
    if (op.missing) {
      const warn = document.createElement("p");
      warn.className = "modal-hint";
      warn.textContent = t("veClipMissing");
      box.appendChild(warn);
    }
    const change = document.createElement("button");
    change.type = "button";
    change.className = "ghost-btn ve-wide";
    change.textContent = t("pathChange");
    change.addEventListener("click", async () => {
      const gen = VE.loadGen;
      const path = await invoke("pick_file_path").catch(() => null);
      if (!path || !overlayStillInEditor(op, gen)) return;
      if (path.includes("'")) { toast(t("veOvImageQuote")); return; }
      try {
        const probe = await invoke("probe_extra_video_source", { path });
        if (!overlayStillInEditor(op, gen)) return;
        commitOverlay(op, { path, hasAudio: !!probe.has_audio, videoDurationMs: Number(probe.duration_ms) || 0, missing: false });
      } catch (err) {
        const { message } = parseFfmpegError(err, t);
        toast(message);
      }
    });
    box.appendChild(change);
    box.appendChild(fieldRow(t("veOvOpacity"), ovTrack("opacity", numberInput(op.opacity, 5, 100, 5, (v) => commitOverlay(op, { opacity: v }), [25, 50, 75, 100]), (o) => String(o.opacity))));
    box.appendChild(fieldRow(t("veOvVolume"), ovTrack("volume", numberInput(op.volume, 0, 200, 5, (v) => commitOverlay(op, { volume: v })), (o) => String(o.volume))));
    box.appendChild(fieldRow(t("veOvLoop"), ovTrack("loop", selectInput(
      [["once", t("veOvLoopOnce")], ["repeat", t("veOvLoopRepeat")]],
      op.loop ? "repeat" : "once", (v) => commitOverlay(op, { loop: v === "repeat" })), (o) => (o.loop ? "repeat" : "once"))));
  }

  if (op.kind === "frame") {
    box.appendChild(fieldRow(t("veOvThickness"), ovTrack("thickness", numberInput(op.thickness, 1, Math.max(4, Math.round(VE.h / 4)), 1, (v) => commitOverlay(op, { thickness: v }), [1, 2, 4, 8, 16]), (o) => String(o.thickness))));
    box.appendChild(fieldRow(t("veOvStyle"), ovTrack("style", selectInput(
      [["solid", t("veOvStyleSolid")], ["double", t("veOvStyleDouble")], ["soft", t("veOvStyleSoft")]],
      op.style, (v) => commitOverlay(op, { style: v })), (o) => o.style)));
    // A full rebuild instead of keepEditor: a swatch click never holds typing focus, and the
    // selected-swatch highlight only comes from a fresh render.
    box.appendChild(fieldRow(t("veOvColor"), colorRow(op.color, (hex) => { commitOverlay(op, { color: hex }); renderOverlayEditor(); })));
    box.appendChild(fieldRow(t("veOvOpacity"), ovTrack("opacity", numberInput(op.opacity, 5, 100, 5, (v) => commitOverlay(op, { opacity: v }), [25, 50, 75, 100]), (o) => String(o.opacity))));
  }

  if (op.kind === "text") {
    box.appendChild(fieldRow(t("veOvTextValue"), ovTrack("text", textInput(op.text, (v) => commitOverlay(op, { text: v })), (o) => o.text)));
    box.appendChild(fieldRow(t("veOvFont"), ovTrack("font", selectInput(FONT_FILES.map(([id]) => [id, id]), op.font, (v) => commitOverlay(op, { font: v })), (o) => o.font)));
    box.appendChild(fieldRow(t("veOvFontSize"), ovTrack("fontPx", numberInput(op.fontPx, 8, Math.max(16, VE.h), 1, (v) => commitOverlay(op, { fontPx: v }), [12, 16, 24, 32, 48, 64, 96]), (o) => String(o.fontPx))));
    const pos = document.createElement("div");
    pos.className = "ve-row2";
    pos.append(
      fieldRow(t("veCropX"), ovTrack("x", numberInput(op.x, 0, VE.w, 1, (v) => commitOverlay(op, { x: v })), (o) => String(o.x))),
      fieldRow(t("veCropY"), ovTrack("y", numberInput(op.y, 0, VE.h, 1, (v) => commitOverlay(op, { y: v })), (o) => String(o.y))));
    box.appendChild(pos);
    // A full rebuild instead of keepEditor: a swatch click never holds typing focus, and the
    // selected-swatch highlight only comes from a fresh render.
    box.appendChild(fieldRow(t("veOvColor"), colorRow(op.color, (hex) => { commitOverlay(op, { color: hex }); renderOverlayEditor(); })));
    box.appendChild(fieldRow(t("veOvTextBack"), selectInput(
      [["none", t("optSnipTextBackNone")], ["plate", t("optSnipTextBackPlate")], ["outline", t("optSnipTextBackOutline")]],
      op.back, (v) => { commitOverlay(op, { back: v }); renderOverlayEditor(); })));
    if (op.back === "outline") {
      box.appendChild(fieldRow(t("veOvTextOutline"), ovTrack("outlinePx", numberInput(op.outlinePx ?? 3, 1, 12, 1, (v) => commitOverlay(op, { outlinePx: v }), [1, 2, 3, 4, 6, 8]), (o) => String(o.outlinePx))));
    }
  }

  if (op.kind === "pixelate") {
    box.appendChild(fieldRow(t("veOvBlock"), ovTrack("blockPx", numberInput(op.blockPx, 4, 80, 2, (v) => commitOverlay(op, { blockPx: v }), [8, 12, 16, 24, 32]), (o) => String(o.blockPx))));
  }

  if (op.kind === "spotlight") {
    box.appendChild(fieldRow(t("veOvDim"), ovTrack("dim", numberInput(Math.round(op.dim * 100), 10, 95, 5, (v) => commitOverlay(op, { dim: v / 100 })), (o) => String(Math.round(o.dim * 100)))));
  }

  if (op.kind === "keys") {
    if (!VE.keySidecar) {
      const note = document.createElement("p");
      note.className = "modal-hint";
      note.textContent = t("veOvKeysNoData");
      box.appendChild(note);
    }
    box.appendChild(fieldRow(t("veOvKeysPreset"), selectInput(
      [["pressed", t("veOvKeysPressed")], ["half", t("veOvKeysHalf")], ["full", t("veOvKeysFull")],
        ["game", t("veOvKeysGame")], ["custom", t("veOvKeysCustom")]],
      op.preset, (v) => { commitOverlay(op, { preset: v }); renderOverlayEditor(); })));
    if (op.preset === "custom") box.appendChild(fieldRow(t("veOvKeysPick"), keysPickerRow(op)));
    box.appendChild(fieldRow(t("veOvColor"), colorRow(op.color, (hex) => { commitOverlay(op, { color: hex }); renderOverlayEditor(); })));
    box.appendChild(fieldRow(t("veOvActiveColor"), colorRow(op.activeColor, (hex) => { commitOverlay(op, { activeColor: hex }); renderOverlayEditor(); })));
    box.appendChild(fieldRow(t("veOvOpacity"), ovTrack("opacity", numberInput(op.opacity, 5, 100, 5, (v) => commitOverlay(op, { opacity: v }), [25, 50, 75, 100]), (o) => String(o.opacity))));
  }

  if (op.rect) {
    const grid = document.createElement("div");
    grid.className = "ve-row4";
    const setRect = (key) => (v) => commitOverlay(op, { rect: snapRect({ ...liveOverlay(op).rect, [key]: v }) });
    grid.append(
      fieldRow(t("veCropX"), ovTrack("rect.x", numberInput(op.rect.x, 0, VE.w, 2, setRect("x")), (o) => String(o.rect.x))),
      fieldRow(t("veCropY"), ovTrack("rect.y", numberInput(op.rect.y, 0, VE.h, 2, setRect("y")), (o) => String(o.rect.y))),
      fieldRow(t("veCropW"), ovTrack("rect.w", numberInput(op.rect.w, 2, VE.w, 2, setRect("w")), (o) => String(o.rect.w))),
      fieldRow(t("veCropH"), ovTrack("rect.h", numberInput(op.rect.h, 2, VE.h, 2, setRect("h")), (o) => String(o.rect.h))));
    box.appendChild(grid);
  }

  const remove = document.createElement("button");
  remove.type = "button";
  remove.className = "ghost-btn ve-wide";
  remove.textContent = t("veOvRemove");
  remove.addEventListener("click", () => { const id = op.id; VE.selOv = null; removeOps((o) => o.id === id); });
  box.appendChild(remove);
}
