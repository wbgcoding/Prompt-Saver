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
    ? { create: "Prompt anlegen", copy: "kopieren", paste: "einfügen", ways: ["Kachel", "Tastenkürzel", "Schwebeknopf", "Infobereich"] }
    : { create: "Create prompt", copy: "Copy", paste: "Paste", ways: ["Tile", "Hotkey", "Floating button", "Tray"] };
  const wayX = [60, 190, 320, 450];
  return `<svg viewBox="0 0 560 130" xmlns="http://www.w3.org/2000/svg" role="img">` +
    box(20, 15, 110, 34) + label(75, 36, words.create) +
    arrow(130, 32, 195) +
    box(195, 15, 170, 34) + label(280, 36, words.copy) +
    arrow(365, 32, 430) +
    box(430, 15, 110, 34) + label(485, 36, words.paste) +
    `<line x1="280" y1="49" x2="280" y2="70" stroke="currentColor" stroke-width="1" stroke-dasharray="2 2"/>` +
    words.ways.map((w, i) => box(wayX[i], 78, 115, 34) + label(wayX[i] + 57, 99, w, { size: 10 })).join("") +
    `</svg>`;
}

function folderTypes(lang) {
  const words = lang === "de"
    ? { view: "Ansichtsgruppe", pill: "Pill-Ordner", tile: "Kachel-Ordner" }
    : { view: "View group", pill: "Pill folder", tile: "Grid folder" };
  return `<svg viewBox="0 0 560 120" xmlns="http://www.w3.org/2000/svg" role="img">` +
    // View group: a tab strip inside a tab.
    box(20, 20, 150, 70) + box(30, 30, 40, 16) + box(74, 30, 40, 16) + box(30, 52, 120, 30) +
    label(95, 108, words.view) +
    // Pill folder: a row of small floating round buttons.
    `<circle cx="255" cy="45" r="16" fill="none" stroke="currentColor" stroke-width="1.5"/>` +
    `<circle cx="295" cy="45" r="16" fill="none" stroke="currentColor" stroke-width="1.5"/>` +
    `<circle cx="335" cy="45" r="16" fill="none" stroke="currentColor" stroke-width="1.5"/>` +
    label(295, 108, words.pill) +
    // Grid folder: one cell of a grid, marked as a folder.
    box(430, 20, 100, 70) +
    `<line x1="430" y1="55" x2="530" y2="55" stroke="currentColor" stroke-width="1"/>` +
    `<line x1="480" y1="20" x2="480" y2="90" stroke="currentColor" stroke-width="1"/>` +
    label(480, 108, words.tile) +
    `</svg>`;
}

function placeholderAnatomy(lang) {
  const words = lang === "de"
    ? { name: "Name", def: "Vorgabe", choices: "Auswahlliste" }
    : { name: "Name", def: "Default", choices: "Choice list" };
  const text = "{#{Kunde=Meier|Meier,Schulz}#}";
  return `<svg viewBox="0 0 560 130" xmlns="http://www.w3.org/2000/svg" role="img">` +
    label(30, 45, text, { anchor: "start", size: 15 }) +
    `<line x1="65" y1="55" x2="65" y2="80" stroke="currentColor" stroke-width="1"/>` +
    label(65, 95, words.name, { size: 11 }) +
    `<line x1="118" y1="55" x2="150" y2="80" stroke="currentColor" stroke-width="1"/>` +
    label(150, 95, words.def, { size: 11 }) +
    `<line x1="240" y1="55" x2="260" y2="80" stroke="currentColor" stroke-width="1"/>` +
    label(260, 95, words.choices, { size: 11 }) +
    `</svg>`;
}

function editorTimeline(lang) {
  const words = lang === "de"
    ? { timeline: "Zeitleiste", cut: "Schnittmarke", overlay: "Überlagerung", audio: "Tonspur" }
    : { timeline: "Timeline", cut: "Cut mark", overlay: "Overlay", audio: "Audio track" };
  return `<svg viewBox="0 0 560 130" xmlns="http://www.w3.org/2000/svg" role="img">` +
    box(60, 15, 120, 30) + label(120, 34, words.overlay, { size: 10 }) +
    box(20, 55, 520, 22) +
    `<line x1="180" y1="55" x2="180" y2="77" stroke="currentColor" stroke-width="1.5"/>` +
    `<line x1="340" y1="55" x2="340" y2="77" stroke="currentColor" stroke-width="1.5"/>` +
    label(180, 95, words.cut, { size: 10 }) +
    `<path d="M 20 115 q 15 -15 30 0 t 30 0 t 30 0 t 30 0 t 30 0 t 30 0 t 30 0 t 30 0 t 30 0 t 30 0 t 30 0 t 30 0 t 30 0" fill="none" stroke="currentColor" stroke-width="1"/>` +
    label(480, 95, words.audio, { size: 10 }) +
    label(290, 12, words.timeline, { size: 10 }) +
    `</svg>`;
}

function chainSteps(lang) {
  const words = lang === "de" ? { step: "Schritt" } : { step: "Step" };
  const stepX = [30, 230, 430];
  return `<svg viewBox="0 0 560 90" xmlns="http://www.w3.org/2000/svg" role="img">` +
    stepX.map((x, i) => box(x, 20, 100, 34) + label(x + 50, 41, `${words.step} ${i + 1}`)).join("") +
    arrow(130, 37, 225) + label(177, 25, "{#{X}#}", { size: 10 }) +
    arrow(330, 37, 425) + label(377, 25, "{#{Y}#}", { size: 10 }) +
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
