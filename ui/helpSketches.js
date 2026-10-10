// Five small diagrams for the in-app help. Plain shapes with theme-coloured
// lines and text (currentColor everywhere), never a screenshot, so they stay
// correct through any redesign of the window they describe. English and
// German labels only, matching help.js's own EN/DE scope; every other
// language falls back to the English sketch the same way the surrounding
// article text does.

const box = (x, y, w, h) => `<rect x="${x}" y="${y}" width="${w}" height="${h}" rx="6" fill="none" stroke="currentColor" stroke-width="1.5"/>`;
const arrow = (x1, y, x2) => `<line x1="${x1}" y1="${y}" x2="${x2 - 8}" y2="${y}" stroke="currentColor" stroke-width="1.5"/>` +
  `<path d="M ${x2 - 10} ${y - 4} L ${x2} ${y} L ${x2 - 10} ${y + 4}" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/>`;
const label = (x, y, text, opts = {}) =>
  `<text x="${x}" y="${y}" font-size="${opts.size || 11}" fill="currentColor" text-anchor="${opts.anchor || "middle"}">${text}</text>`;

function flowToTarget(lang) {
  const words = lang === "de"
    ? { create: "Prompt anlegen", copy: "kopieren", paste: "einfügen", ways: ["Kachel", "Tastenkürzel", "Schwebe-Button", "Infobereich"] }
    : { create: "Create prompt", copy: "Copy", paste: "Paste", ways: ["Tile", "Hotkey", "Floating button", "Tray"] };
  const wayX = [10, 147, 284, 421];
  return `<svg viewBox="0 0 560 124" xmlns="http://www.w3.org/2000/svg" role="img">` +
    box(10, 10, 140, 36) + label(80, 33, words.create, { size: 14 }) +
    arrow(150, 28, 210) +
    box(210, 10, 140, 36) + label(280, 33, words.copy, { size: 14 }) +
    arrow(350, 28, 410) +
    box(410, 10, 140, 36) + label(480, 33, words.paste, { size: 14 }) +
    `<line x1="280" y1="46" x2="280" y2="78" stroke="currentColor" stroke-width="1" stroke-dasharray="2 2"/>` +
    words.ways.map((w, i) => box(wayX[i], 78, 129, 36) + label(wayX[i] + 64.5, 101, w, { size: 13 })).join("") +
    `</svg>`;
}

function folderTypes(lang) {
  const words = lang === "de"
    ? { view: "Ansichtsgruppe", pill: "Desktop-Ordner", tile: "Raster-Ordner" }
    : { view: "View group", pill: "Desktop folder", tile: "Grid folder" };
  return `<svg viewBox="0 0 560 124" xmlns="http://www.w3.org/2000/svg" role="img">` +
    // View group: a tab strip inside a tab.
    box(20, 10, 150, 76) + box(30, 20, 40, 16) + box(74, 20, 40, 16) + box(30, 44, 130, 32) +
    label(95, 112, words.view, { size: 14 }) +
    // Desktop folder: a row of small floating round buttons.
    `<circle cx="240" cy="48" r="18" fill="none" stroke="currentColor" stroke-width="1.5"/>` +
    `<circle cx="285" cy="48" r="18" fill="none" stroke="currentColor" stroke-width="1.5"/>` +
    `<circle cx="330" cy="48" r="18" fill="none" stroke="currentColor" stroke-width="1.5"/>` +
    label(285, 112, words.pill, { size: 14 }) +
    // Grid folder: one tile holding a 2x2 preview of its members.
    box(420, 10, 110, 76) +
    box(447, 18, 26, 26) + box(477, 18, 26, 26) + box(447, 48, 26, 26) + box(477, 48, 26, 26) +
    label(475, 112, words.tile, { size: 14 }) +
    `</svg>`;
}

function placeholderAnatomy(lang) {
  const words = lang === "de"
    ? { name: "Name", def: "Vorgabe", choices: "Auswahlliste" }
    : { name: "Name", def: "Default", choices: "Choice list" };
  // Every piece of the markup is its own monospace run, stretched to exactly CH px per
  // character (textLength), so the bracket under each part sits on it in any font.
  const CH = 11;
  const parts = [["{?"], ["Kunde", words.name], ["="], ["Meier", words.def], ["|"], ["Meier, Schulz", words.choices], ["}"]];
  let x = 20;
  let out = "";
  for (const [text, name] of parts) {
    const w = text.length * CH;
    out += `<text x="${x}" y="34" font-size="17" font-family="Consolas, ui-monospace, monospace" textLength="${w}" lengthAdjust="spacingAndGlyphs" fill="currentColor"${name ? "" : ' opacity="0.55"'}>${text}</text>`;
    if (name) {
      const mid = x + w / 2;
      out += `<path d="M ${x + 1} 44 V 50 H ${x + w - 1} V 44" fill="none" stroke="currentColor" stroke-width="1.5"/>` +
        `<line x1="${mid}" y1="50" x2="${mid}" y2="64" stroke="currentColor" stroke-width="1.5"/>` +
        label(mid, 80, name, { size: 12 });
    }
    x += w;
  }
  // Same 560 wide canvas as the other sketches, so it is drawn at their scale.
  return `<svg viewBox="0 0 560 92" xmlns="http://www.w3.org/2000/svg" role="img">${out}</svg>`;
}

function editorTimeline(lang) {
  const words = lang === "de"
    ? { timeline: "Zeitleiste", cut: "Schnittmarke", overlay: "Überlagerung", audio: "Tonspur" }
    : { timeline: "Timeline", cut: "Cut mark", overlay: "Overlay", audio: "Audio track" };
  // 26 waves of 20 px span the same 520 px as the timeline above them.
  const wave = "M 20 128 q 10 -12 20 0" + " t 20 0".repeat(25);
  return `<svg viewBox="0 0 560 140" xmlns="http://www.w3.org/2000/svg" role="img">` +
    box(80, 8, 160, 30) + label(160, 28, words.overlay, { size: 14 }) +
    box(20, 50, 520, 28) + label(380, 69, words.timeline, { size: 13 }) +
    `<line x1="250" y1="44" x2="250" y2="84" stroke="currentColor" stroke-width="2"/>` +
    label(250, 100, words.cut, { size: 13 }) +
    `<path d="${wave}" fill="none" stroke="currentColor" stroke-width="1"/>` +
    label(540, 100, words.audio, { size: 13, anchor: "end" }) +
    `</svg>`;
}

function chainSteps(lang) {
  const words = lang === "de" ? { step: "Schritt" } : { step: "Step" };
  const stepX = [20, 220, 420];
  return `<svg viewBox="0 0 560 70" xmlns="http://www.w3.org/2000/svg" role="img">` +
    stepX.map((x, i) => box(x, 26, 120, 36) + label(x + 60, 49, `${words.step} ${i + 1}`, { size: 14 })).join("") +
    arrow(140, 44, 220) + label(180, 20, "{?X}", { size: 13 }) +
    arrow(340, 44, 420) + label(380, 20, "{?Y}", { size: 13 }) +
    `</svg>`;
}

// Keyed the same way HELP_CONTENT entries reference them (`cat.sketch`).
export const HELP_SKETCHES = {
  flowToTarget,
  folderTypes,
  placeholderAnatomy,
  editorTimeline,
  chainSteps,
};
