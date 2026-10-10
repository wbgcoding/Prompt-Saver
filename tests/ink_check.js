// Readable-ink rule checks (ui/floattheme.js: applyFloatTheme()'s pill ink,
// which mirrors ui/main.js's own readableInk()). The split used to be a fixed
// WCAG 0.179 luminance cutoff; it now moves with the expert menu's autoInk
// switch and inkThreshold value, fed in from a theme-changed event or one
// ink_prefs() fetch at load.
// Run:  node tests/ink_check.js      Exit: 0 = clean, 1 = problems.
const path = require("path");

// floattheme.js runs in a Tauri webview (window.__TAURI__, document) and
// wires its own event.listen()/invoke() at module load -- stub just enough to
// import it under plain Node, the same way eta_check.js stubs `document` for
// floatShared.js's own module-scope popover listener.
if (typeof document === "undefined") {
  const vars = new Map();
  global.document = {
    documentElement: {
      setAttribute() {},
      style: {
        colorScheme: "",
        setProperty: (k, v) => vars.set(k, v),
        removeProperty: (k) => vars.delete(k),
        getPropertyValue: (k) => vars.get(k) || "",
      },
    },
    addEventListener() {},
  };
}
let themeHandler = null;
let resolveInkPrefs = null;
if (typeof window === "undefined") {
  global.window = {
    __TAURI__: {
      event: {
        listen: (name, cb) => {
          if (name === "theme-changed") themeHandler = cb;
          return Promise.resolve(() => {});
        },
      },
      core: {
        // Left pending on purpose -- resolved explicitly below, once themeHandler
        // is already captured, so the test controls exactly when it lands.
        invoke: (cmd) => (cmd === "ink_prefs" ? new Promise((res) => { resolveInkPrefs = res; }) : Promise.resolve({})),
      },
    },
  };
}

const { applyFloatTheme, inkForColor } = require(path.join(__dirname, "..", "ui", "floattheme.js"));

const problems = [];
const check = (label, actual, expected) => {
  if (actual !== expected) problems.push(`${label}: got ${JSON.stringify(actual)}, expected ${JSON.stringify(expected)}`);
};
const pillFg = () => document.documentElement.style.getPropertyValue("--pill-fg");
const pillInkShadow = () => document.documentElement.style.getPropertyValue("--pill-ink-shadow");

// A microtask tick, so a `.then(setInk)` chain already scheduled gets to run
// before the next assertion reads its effect.
const tick = () => new Promise((r) => setImmediate(r));

// The shipped palette, read from the source of ui/core.js (which needs the main window to load).
const paletteHexes = () => {
  const src = require("fs").readFileSync(path.join(__dirname, "..", "ui", "core.js"), "utf8");
  const body = /export const DEFAULT_COLORS = \[([^\]]*)\]/.exec(src)[1];
  return [...body.matchAll(/"(#[0-9a-fA-F]{6})"/g)].map((m) => m[1]);
};
const lum = (hex) => {
  const n = parseInt(hex.slice(1), 16);
  const lin = (c) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
  return 0.2126 * lin(((n >> 16) & 255) / 255) + 0.7152 * lin(((n >> 8) & 255) / 255) + 0.0722 * lin((n & 255) / 255);
};
const HALO = "0 0 2px rgba(0, 0, 0, 0.75), 0 1px 3px rgba(0, 0, 0, 0.62)";

async function run() {
  // Palette table under the module default, before any theme payload or ink_prefs() answer:
  // black ink wherever it reads better than white (L above 0.25 with the default threshold of
  // 25 %), white ink with the halo between the 0.179 crossover and the threshold, white without
  // a halo on a dark colour.
  const palette = paletteHexes();
  check("palette: 18 swatches read from core.js", palette.length, 18);
  const WHITE_WITH_HALO = ["#ef4444", "#3b82f6", "#6366f1", "#8b5cf6", "#a855f7", "#ec4899", "#f43f5e"];
  for (const hex of palette) {
    const l = lum(hex);
    const own = inkForColor(hex);
    if (l > 0.25) {
      check(`default threshold: ${hex} (L ${l.toFixed(3)}) gets black text`, own.fg, "#000");
      check(`default threshold: ${hex} gets no halo on black text`, own.shadow, "none");
    } else if (l > 0.179) {
      check(`default threshold: ${hex} (L ${l.toFixed(3)}) keeps white text`, own.fg, "#fff");
      check(`default threshold: ${hex} keeps its halo`, own.shadow, HALO);
    } else {
      check(`default threshold: ${hex} (L ${l.toFixed(3)}) keeps white text`, own.fg, "#fff");
      check(`default threshold: ${hex} needs no halo`, own.shadow, "none");
    }
  }
  for (const hex of WHITE_WITH_HALO) check(`default threshold: ${hex} is white with a halo`, [inkForColor(hex).fg, inkForColor(hex).shadow].join("|"), "#fff|" + HALO);
  check("default threshold: #d946ef (L 0.254) is black", inkForColor("#d946ef").fg, "#000");
  check("default threshold: #f59e0b (orange) is black", inkForColor("#f59e0b").fg, "#000");
  check("default threshold: #64748b is white without a halo", [inkForColor("#64748b").fg, inkForColor("#64748b").shadow].join("|"), "#fff|none");

  // The module's own shipped default (on, 25) applies before ink_prefs() answers.
  applyFloatTheme("dark", { accent: "#4a7dd8" });
  check("pre-fetch default: #4a7dd8 (medium blue) stays white", pillFg(), "#fff");

  // ink_prefs() resolves with an explicit threshold of 60 (a stored value is kept).
  resolveInkPrefs({ auto: true, threshold: 60, halo: true, haloStrength: 75 });
  await tick();
  applyFloatTheme("dark", { accent: "#4a7dd8" });
  check("threshold 60: #4a7dd8 stays white (red under the old fixed 0.179 split)", pillFg(), "#fff");
  applyFloatTheme("dark", { accent: "#f59e0b" });
  check("threshold 60: #f59e0b stays white", pillFg(), "#fff");
  applyFloatTheme("dark", { accent: "#ffd54f" });
  check("threshold 60: #ffd54f (light amber) turns black", pillFg(), "#000");

  // The dark halo behind white text: none where black text was already
  // picked, none on a colour dark enough that white reads fine on its own,
  // and the halo value on a colour past the contrast crossover but still
  // under the threshold (so it keeps white text, just with help).
  applyFloatTheme("dark", { accent: "#ffffff" });
  check("threshold 60: #ffffff (black text) gets no halo", pillInkShadow(), "none");
  applyFloatTheme("dark", { accent: "#4fa3e0" });
  check("threshold 60, halo 75: #4fa3e0 (white text, past the crossover) gets the default halo",
    pillInkShadow(), "0 0 2px rgba(0, 0, 0, 0.75), 0 1px 3px rgba(0, 0, 0, 0.62)");
  themeHandler({ payload: { ink: { auto: true, threshold: 60, halo: true, haloStrength: 85 } } });
  applyFloatTheme("dark", { accent: "#4fa3e0" });
  check("halo strength 85 still gives the old look",
    pillInkShadow(), "0 0 2px rgba(0, 0, 0, 0.85), 0 1px 3px rgba(0, 0, 0, 0.7)");
  applyFloatTheme("dark", { accent: "#202020" });
  check("threshold 60: #202020 (dark enough for white on its own) gets no halo", pillInkShadow(), "none");

  // The halo can be switched off entirely, live.
  themeHandler({ payload: { ink: { auto: true, threshold: 60, halo: false, haloStrength: 75 } } });
  applyFloatTheme("dark", { accent: "#4fa3e0" });
  check("halo off: #4fa3e0 gets no halo", pillInkShadow(), "none");

  // A lower strength keeps the halo but with lower alphas.
  themeHandler({ payload: { ink: { auto: true, threshold: 60, halo: true, haloStrength: 40 } } });
  applyFloatTheme("dark", { accent: "#4fa3e0" });
  check("halo strength 40: #4fa3e0 gets a lighter halo",
    pillInkShadow(), "0 0 2px rgba(0, 0, 0, 0.4), 0 1px 3px rgba(0, 0, 0, 0.33)");

  // A later theme-changed event can still flip the auto switch off, live.
  themeHandler({ payload: { ink: { auto: false, threshold: 60, halo: true, haloStrength: 75 } } });
  applyFloatTheme("dark", { accent: "#4a7dd8" });
  check("switch off: #4a7dd8 stays white", pillFg(), "#fff");
  applyFloatTheme("dark", { accent: "#ffd54f" });
  check("switch off: #ffd54f stays white too", pillFg(), "#fff");

  // A button with its own colour is judged by that colour, whatever the
  // theme accent is.
  themeHandler({ payload: { ink: { auto: true, threshold: 60, halo: true, haloStrength: 75 } } });
  applyFloatTheme("dark", { accent: "#1e3a8a" });
  check("own colour #ffe066: black text", inkForColor("#ffe066").fg, "#000");
  check("own colour #ffe066: no halo on black text", inkForColor("#ffe066").shadow, "none");
  check("own colour #4fa3e0: white text", inkForColor("#4fa3e0").fg, "#fff");
  check("own colour #4fa3e0: default halo", inkForColor("#4fa3e0").shadow,
    "0 0 2px rgba(0, 0, 0, 0.75), 0 1px 3px rgba(0, 0, 0, 0.62)");
  check("no own colour: no ink override", inkForColor(""), null);
  check("non-hex own colour: no ink override", inkForColor("rgba(1,2,3,1)"), null);
  themeHandler({ payload: { ink: { auto: false, threshold: 60, halo: true, haloStrength: 75 } } });
  check("autoInk off: own colour #ffe066 stays white", inkForColor("#ffe066").fg, "#fff");

  if (problems.length) {
    console.log(`${problems.length} problem(s):`);
    for (const p of problems) console.log(" - " + p);
    process.exit(1);
  } else {
    console.log("PASS");
  }
}
run();
