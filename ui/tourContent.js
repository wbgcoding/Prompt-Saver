// Tiny hand-drawn illustrations, ~110x110, currentColor + var(--accent) on the one
// interactive element each drawing highlights (never a raster asset, matches the
// toolbar icon convention just at a bigger size).
export const TOUR_SVG_WELCOME = `<svg viewBox="0 0 100 100" width="110" height="110" aria-hidden="true">
  <rect x="8" y="14" width="84" height="72" rx="6" fill="none" stroke="currentColor" stroke-width="3"/>
  <line x1="8" y1="30" x2="92" y2="30" stroke="currentColor" stroke-width="3"/>
  <circle cx="16" cy="22" r="2" fill="currentColor"/>
  <circle cx="24" cy="22" r="2" fill="currentColor"/>
  <circle cx="32" cy="22" r="2" fill="currentColor"/>
  <rect x="16" y="38" width="20" height="16" rx="3" fill="none" stroke="currentColor" stroke-width="2.5"/>
  <rect x="40" y="38" width="20" height="16" rx="3" fill="var(--accent)" stroke="var(--accent)" stroke-width="2.5"/>
  <rect x="64" y="38" width="20" height="16" rx="3" fill="none" stroke="currentColor" stroke-width="2.5"/>
  <rect x="16" y="60" width="20" height="16" rx="3" fill="none" stroke="currentColor" stroke-width="2.5"/>
  <rect x="40" y="60" width="20" height="16" rx="3" fill="none" stroke="currentColor" stroke-width="2.5"/>
  <rect x="64" y="60" width="20" height="16" rx="3" fill="none" stroke="currentColor" stroke-width="2.5"/>
  <line x1="46" y1="43" x2="54" y2="43" stroke="var(--accent-fg)" stroke-width="1.5" stroke-linecap="round"/>
  <line x1="46" y1="48" x2="54" y2="48" stroke="var(--accent-fg)" stroke-width="1.5" stroke-linecap="round"/>
</svg>`;
// A click on a tile sends its text to the clipboard: the tile itself, a cursor over it, a
// motion arc for the copy, and the clipboard the line of text lands in.
export const TOUR_SVG_USAGE = `<svg viewBox="0 0 100 100" width="110" height="110" aria-hidden="true">
  <rect x="6" y="32" width="36" height="28" rx="4" fill="none" stroke="currentColor" stroke-width="2.2"/>
  <line x1="13" y1="42" x2="35" y2="42" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/>
  <line x1="13" y1="50" x2="29" y2="50" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/>
  <path d="M34 46 L34 64 L38.2 60.4 L41.5 67.5 L45 65.8 L41.5 58.8 L47.5 58.8 Z" fill="currentColor"/>
  <path d="M48 44 Q64 26 80 36" fill="none" stroke="var(--accent)" stroke-width="2.2" stroke-dasharray="4 3" stroke-linecap="round"/>
  <path d="M73 35 L80 36 L76 30" fill="none" stroke="var(--accent)" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"/>
  <rect x="64" y="48" width="30" height="36" rx="3" fill="none" stroke="currentColor" stroke-width="2.2"/>
  <rect x="72" y="43" width="14" height="8" rx="2" fill="none" stroke="currentColor" stroke-width="2.2"/>
  <line x1="71" y1="64" x2="89" y2="64" stroke="var(--accent)" stroke-width="2" stroke-linecap="round"/>
  <line x1="71" y1="72" x2="85" y2="72" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" opacity="0.6"/>
  <line x1="71" y1="80" x2="81" y2="80" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" opacity="0.4"/>
</svg>`;
export const TOUR_SVG_TILES = `<svg viewBox="0 0 100 100" width="110" height="110" aria-hidden="true">
  <rect x="8" y="14" width="22" height="18" rx="3" fill="none" stroke="currentColor" stroke-width="2.2"/>
  <rect x="8" y="41" width="22" height="18" rx="3" fill="none" stroke="currentColor" stroke-width="2.2" stroke-dasharray="3 3" opacity="0.55"/>
  <rect x="8" y="68" width="22" height="18" rx="3" fill="none" stroke="currentColor" stroke-width="2.2"/>
  <rect x="70" y="14" width="22" height="18" rx="3" fill="none" stroke="currentColor" stroke-width="2.2"/>
  <rect x="70" y="41" width="22" height="18" rx="3" fill="var(--accent)" stroke="var(--accent)" stroke-width="2.5"/>
  <rect x="70" y="68" width="22" height="18" rx="3" fill="none" stroke="currentColor" stroke-width="2.2"/>
  <path d="M32 23 Q50 6 68 23" fill="none" stroke="var(--accent)" stroke-width="2" stroke-dasharray="4 3"/>
  <path d="M62 21 L68 23 L66 17" fill="none" stroke="var(--accent)" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
  <line x1="13" y1="19" x2="25" y2="19" stroke="currentColor" stroke-width="1.4"/>
  <line x1="13" y1="73" x2="25" y2="73" stroke="currentColor" stroke-width="1.4"/>
  <line x1="75" y1="19" x2="87" y2="19" stroke="currentColor" stroke-width="1.4"/>
  <line x1="75" y1="73" x2="87" y2="73" stroke="currentColor" stroke-width="1.4"/>
</svg>`;
export const TOUR_SVG_LIBRARY = `<svg viewBox="0 0 100 100" width="110" height="110" aria-hidden="true">
  <line x1="10" y1="18" x2="68" y2="18" stroke="currentColor" stroke-width="2.5" stroke-linecap="round"/>
  <line x1="10" y1="32" x2="58" y2="32" stroke="currentColor" stroke-width="2.5" stroke-linecap="round"/>
  <line x1="10" y1="46" x2="64" y2="46" stroke="currentColor" stroke-width="2.5" stroke-linecap="round"/>
  <rect x="10" y="60" width="18" height="10" rx="5" fill="none" stroke="currentColor" stroke-width="2"/>
  <rect x="32" y="60" width="18" height="10" rx="5" fill="none" stroke="currentColor" stroke-width="2"/>
  <circle cx="4" cy="65" r="2" fill="currentColor"/>
  <circle cx="26" cy="65" r="2" fill="currentColor"/>
  <circle cx="64" cy="52" r="17" fill="none" stroke="var(--accent)" stroke-width="3"/>
  <line x1="76" y1="64" x2="90" y2="78" stroke="var(--accent)" stroke-width="3.5" stroke-linecap="round"/>
  <line x1="56" y1="46" x2="72" y2="46" stroke="var(--accent)" stroke-width="1.6" opacity="0.6"/>
  <line x1="56" y1="58" x2="66" y2="58" stroke="var(--accent)" stroke-width="1.6" opacity="0.6"/>
</svg>`;

// The steps drawing + text (step 5, the settings table, is handled separately below).
export const TOUR_STEPS = [
  { titleKey: "tourStep1Title", lineKeys: ["tourStep1Line1", "tourStep1Line2", "tourStep1Line3"], svg: TOUR_SVG_WELCOME },
  { titleKey: "tourStep2Title", lineKeys: ["tourStep2Line1", "tourStep2Line2", "tourStep2Line3"], svg: TOUR_SVG_USAGE },
  { titleKey: "tourStep3Title", lineKeys: ["tourStep3Line1", "tourStep3Line2", "tourStep3Line3"], svg: TOUR_SVG_TILES },
  { titleKey: "tourStep4Title", lineKeys: ["tourStep4Line1", "tourStep4Line2", "tourStep4Line3"], svg: TOUR_SVG_LIBRARY },
];
export const TOUR_STEP_COUNT = TOUR_STEPS.length + 1; // + the settings step
export const TOUR_LAST_STEP = TOUR_STEP_COUNT - 1;

// Step 5's table: a data structure, not markup, so a later feature is one new
// row. Each row names the flag it switches, the text describing it, and the
// help article it points at (ui/help.js, matched by `id`).
export const TOUR_FEATURES = [
  { flagKey: "screenshot", labelKey: "flagScreenshot", textKey: "tourOptScreenshot", helpAnchor: "screenshotEditor" },
  { flagKey: "ocr", labelKey: "flagOcr", textKey: "tourOptOcr", helpAnchor: "ocr" },
  { flagKey: "screenRecord", labelKey: "flagScreenRecord", textKey: "tourOptRecord", helpAnchor: "screenRecording" },
  { flagKey: "voice", labelKey: "flagVoice", textKey: "tourOptVoice", helpAnchor: "voiceDictation" },
  { flagKey: "colorPicker", labelKey: "flagColorPicker", textKey: "tourOptPicker", helpAnchor: "colorPicker" },
];
