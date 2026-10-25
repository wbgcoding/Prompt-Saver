// Video editor: the keystroke overlay, key presses recorded with a take and drawn over the
// export.
import { FONT_FILES, fontsDir, outMs, t, VE } from "./video.js";
import { commitOverlay, liveOverlay } from "./videoOverlays.js";
import { ffPath, ffText, hexToFf } from "./videoGraph.js";

// ---------------------------------------------------------------- keys overlay
//
// The "keys" overlay burns VE.keySidecar's timestamped key-down/key-up events into a static or
// semi-static keyboard picture. Every key gets equal width (no real keycap proportions),
// enough for "which key".

function keyLabel(vk) {
  if (vk >= 0x41 && vk <= 0x5a) return String.fromCharCode(vk);
  if (vk >= 0x30 && vk <= 0x39) return String.fromCharCode(vk);
  if (vk >= 0x70 && vk <= 0x7b) return `F${vk - 0x6f}`;
  const named = {
    0x08: "Back", 0x09: "Tab", 0x0d: "Enter", 0x10: "Shift", 0x11: "Ctrl", 0x12: "Alt",
    0x14: "Caps", 0x1b: "Esc", 0x20: "Space", 0x25: "←", 0x26: "↑", 0x27: "→", 0x28: "↓",
    0x2e: "Del", 0xa0: "Shift", 0xa1: "Shift", 0xa2: "Ctrl", 0xa3: "Ctrl", 0xa4: "Alt", 0xa5: "Alt",
    0xba: ";", 0xbb: "=", 0xbc: ",", 0xbd: "-", 0xbe: ".", 0xbf: "/", 0xc0: "`",
    0xdb: "[", 0xdc: "\\", 0xdd: "]", 0xde: "'",
  };
  return named[vk] || `#${vk}`;
}

// Rows of virtual-key codes. "half" and "game" are proper subsets of "full"
// so a key highlighted in one is recognisable in the others too.
const KEY_LAYOUT_FULL = [
  [0x1b, 0x70, 0x71, 0x72, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7a, 0x7b],
  [0xc0, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x30, 0xbd, 0xbb, 0x08],
  [0x09, 0x51, 0x57, 0x45, 0x52, 0x54, 0x59, 0x55, 0x49, 0x4f, 0x50, 0xdb, 0xdd],
  [0x14, 0x41, 0x53, 0x44, 0x46, 0x47, 0x48, 0x4a, 0x4b, 0x4c, 0xba, 0xde, 0x0d],
  [0x10, 0x5a, 0x58, 0x43, 0x56, 0x42, 0x4e, 0x4d, 0xbc, 0xbe, 0xbf],
  [0x11, 0x12, 0x20, 0x25, 0x26, 0x27, 0x28],
];
const KEY_LAYOUT_HALF = [
  [0xc0, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x30, 0xbd, 0xbb, 0x08],
  [0x09, 0x51, 0x57, 0x45, 0x52, 0x54, 0x59, 0x55, 0x49, 0x4f, 0x50],
  [0x41, 0x53, 0x44, 0x46, 0x47, 0x48, 0x4a, 0x4b, 0x4c, 0x0d],
  [0x10, 0x5a, 0x58, 0x43, 0x56, 0x42, 0x4e, 0x4d, 0x20],
];
const KEY_LAYOUT_GAME = [
  [0x31, 0x32, 0x33, 0x34, 0x35],
  [0x51, 0x57, 0x45, 0x52],
  [0x41, 0x53, 0x44, 0x46],
  [0x10, 0x20, 0x11],
  [0x25, 0x26, 0x27, 0x28],
];

function keysLayoutRows(op) {
  if (op.preset === "half") return KEY_LAYOUT_HALF;
  if (op.preset === "full") return KEY_LAYOUT_FULL;
  if (op.preset === "game") return KEY_LAYOUT_GAME;
  // An empty pick draws nothing (the editor says so) instead of a stray "A".
  if (op.preset === "custom") return op.keys && op.keys.length ? [op.keys] : [];
  return null; // "pressed" has no fixed grid
}

// Every key rendered at equal width, rows centered against the widest one.
function keysGridRects(rows, rect) {
  if (!rows.length) return [];
  const maxCols = Math.max(...rows.map((r) => r.length));
  const cellW = rect.w / maxCols;
  const cellH = rect.h / rows.length;
  const out = [];
  rows.forEach((row, ri) => {
    const rowW = row.length * cellW;
    const xOff = rect.x + (rect.w - rowW) / 2;
    row.forEach((vk, ci) => {
      out.push({ vk, x: Math.round(xOff + ci * cellW), y: Math.round(rect.y + ri * cellH), w: Math.round(cellW) - 2, h: Math.round(cellH) - 2 });
    });
  });
  return out;
}

// Every [down, up] span for one key across the whole sidecar, held visible for holdMs past
// release; a key still down when the log ends stays visible until the last known timestamp
// plus the hold, never forever.
function keyPressSpans(vk, holdMs) {
  const events = (VE.keySidecar && VE.keySidecar.events) || [];
  const spans = [];
  let downT = null;
  for (const e of events) {
    if (e.vk !== vk) continue;
    if (e.down) { if (downT == null) downT = e.t_ms; } else if (downT != null) { spans.push([downT, e.t_ms + holdMs]); downT = null; }
  }
  if (downT != null) {
    const lastMs = events.length ? events[events.length - 1].t_ms : downT;
    spans.push([downT, lastMs + holdMs]);
  }
  return spans;
}

// Clips every span into this overlay's active range and onto the output timeline, like
// enableExpr() does for every other overlay kind (the sidecar's timestamps are source-relative,
// like op.startMs/op.endMs).
function keysEnable(op, spans) {
  const clipped = spans
    .map(([s, e]) => [Math.max(s, op.startMs), Math.min(e, op.endMs)])
    .filter(([s, e]) => e > s)
    .map(([s, e]) => [outMs(s) / 1000, outMs(e, true) / 1000]);
  if (!clipped.length) return null;
  return `enable='${clipped.map(([a, b]) => `between(t,${a.toFixed(3)},${b.toFixed(3)})`).join("+")}'`;
}

function keysFontFile() {
  const font = FONT_FILES.find(([id]) => id === (VE.ovFontDefault || FONT_FILES[0][0])) || FONT_FILES[0];
  return ffPath(`${fontsDir}/${font[1]}`);
}

// Greedy interval assignment: each press span gets the first slot free since its start, or (an
// overlapping run past maxSlots) the oldest slot again rather than being dropped: a crowded
// timeline beats missing keys.
function pressedSpans(op, maxSlots) {
  const holdMs = (VE.keySidecar && VE.keySidecar.hold_ms) || 400;
  const events = (VE.keySidecar && VE.keySidecar.events) || [];
  const open = new Map();
  const spans = [];
  for (const e of events) {
    if (e.down) { if (!open.has(e.vk)) open.set(e.vk, e.t_ms); } else if (open.has(e.vk)) {
      spans.push({ vk: e.vk, start: open.get(e.vk), end: e.t_ms + holdMs });
      open.delete(e.vk);
    }
  }
  const lastMs = events.length ? events[events.length - 1].t_ms : 0;
  for (const [vk, start] of open) spans.push({ vk, start, end: lastMs + holdMs });
  spans.sort((a, b) => a.start - b.start);
  const slotEnd = new Array(maxSlots).fill(-Infinity);
  for (const s of spans) {
    let slot = slotEnd.findIndex((end) => end <= s.start);
    if (slot === -1) slot = 0;
    s.slot = slot;
    slotEnd[slot] = s.end;
  }
  return spans;
}

// Label size of one grid key: a share of the key height, shrunk so a long name such as "Space"
// or "Shift" still fits the key's width.
function keyFontSize(k, label) {
  const byHeight = Math.round(k.h * 0.4);
  const byWidth = Math.floor(k.w / (label.length * 0.6 + 0.4));
  return Math.max(6, Math.min(byHeight, byWidth));
}

// The bar a pressed key gets in the "pressed" preset, slot 0 being the top one.
function pressedBar(op, slot) {
  const maxSlots = Math.min(4, Math.max(1, Math.floor(op.rect.h / 28)));
  const rowH = op.rect.h / maxSlots;
  const y = Math.round(op.rect.y + slot * rowH);
  const h = Math.max(12, Math.round(rowH) - 4);
  return { maxSlots, y, h, fontSize: Math.max(10, Math.round(h * 0.6)) };
}

// What the preview draws for an overlay, in overlay-local source pixels: the same grid the export
// burns in, with the W key shown as pressed so the colours can be judged. `bar` is the single
// sample bar of the "pressed" preset; `empty` flags a custom pick without keys.
export function keysPreviewModel(op) {
  if (op.preset === "pressed") {
    const bar = pressedBar(op, 0);
    return { cells: [], bar: { x: 0, y: bar.y - op.rect.y, w: op.rect.w, h: bar.h, fontSize: bar.fontSize, label: "A" }, empty: false };
  }
  const rows = keysLayoutRows(op);
  const rects = keysGridRects(rows, { x: 0, y: 0, w: op.rect.w, h: op.rect.h });
  // W is the sample "pressed" key (the usual movement key); a layout without it shows its first.
  const W_VK = 0x57;
  const sample = Math.max(0, rects.findIndex((k) => k.vk === W_VK));
  const cells = rects.map((k, i) => {
    const label = keyLabel(k.vk);
    return { x: k.x, y: k.y, w: k.w, h: k.h, label, fontSize: keyFontSize(k, label), active: i === sample };
  });
  return { cells, bar: null, empty: cells.length === 0 };
}

// Produces the drawbox/drawtext filter strings this overlay adds to the chain; called once per
// op from buildVideoChain(), factored out for length.
export function buildKeysFilters(op) {
  const filters = [];
  const font = keysFontFile();
  const color = hexToFf(op.color, op.opacity ?? 100);
  const activeColor = hexToFf(op.activeColor, op.opacity ?? 100);
  const textColor = hexToFf("#ffffff", 100);
  if (op.preset === "pressed") {
    const maxSlots = pressedBar(op, 0).maxSlots;
    for (const s of pressedSpans(op, maxSlots)) {
      const enable = keysEnable(op, [[s.start, s.end]]);
      if (!enable) continue;
      const bar = pressedBar(op, s.slot);
      filters.push(`drawbox=x=${op.rect.x}:y=${bar.y}:w=${op.rect.w}:h=${bar.h}:color=${activeColor}:t=fill:${enable}`);
      filters.push(`drawtext=fontfile='${font}':x=${op.rect.x + 8}:y=${bar.y + Math.round(bar.h / 4)}:fontsize=${bar.fontSize}:fontcolor=${textColor}:expansion=none:${enable}:text='${ffText(keyLabel(s.vk))}'`);
    }
    return filters;
  }
  const rows = keysLayoutRows(op);
  const holdMs = (VE.keySidecar && VE.keySidecar.hold_ms) || 400;
  for (const k of keysGridRects(rows, op.rect)) {
    const label = keyLabel(k.vk);
    filters.push(`drawbox=x=${k.x}:y=${k.y}:w=${k.w}:h=${k.h}:color=${color}:t=2`);
    // The press fill goes down before the label so the label stays readable on top of it, and the
    // label is centred on the key by its own measured size rather than by a guessed offset.
    const enable = keysEnable(op, keyPressSpans(k.vk, holdMs));
    if (enable) filters.push(`drawbox=x=${k.x}:y=${k.y}:w=${k.w}:h=${k.h}:color=${activeColor}:t=fill:${enable}`);
    filters.push(`drawtext=fontfile='${font}':x=${k.x}+(${k.w}-text_w)/2:y=${k.y}+(${k.h}-text_h)/2:fontsize=${keyFontSize(k, label)}:fontcolor=${color}:expansion=none:text='${ffText(label)}'`);
  }
  return filters;
}

// The free-selection field for the "custom" preset: one checkbox per key of
// the full layout, toggling membership in op.keys.
export function keysPickerRow(op) {
  const wrap = document.createElement("div");
  wrap.className = "ve-keys-picker";
  // Without a key the overlay draws nothing, so say so right where the keys are picked.
  const hint = document.createElement("p");
  hint.className = "modal-hint ve-keys-empty";
  hint.textContent = t("veKeysEmpty");
  hint.hidden = !!(op.keys && op.keys.length);
  wrap.appendChild(hint);
  for (const row of KEY_LAYOUT_FULL) {
    for (const vk of row) {
      const label = document.createElement("label");
      label.className = "ve-keys-pick-item";
      const box = document.createElement("input");
      box.type = "checkbox";
      box.checked = (op.keys || []).includes(vk);
      box.addEventListener("change", () => {
        const keys = new Set(liveOverlay(op).keys || []);
        if (box.checked) keys.add(vk); else keys.delete(vk);
        hint.hidden = keys.size > 0;
        commitOverlay(op, { keys: [...keys] });
      });
      label.append(box, document.createTextNode(keyLabel(vk)));
      wrap.appendChild(label);
    }
  }
  return wrap;
}
