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

const { applyFloatTheme } = require(path.join(__dirname, "..", "ui", "floattheme.js"));

const problems = [];
const check = (label, actual, expected) => {
  if (actual !== expected) problems.push(`${label}: got ${JSON.stringify(actual)}, expected ${JSON.stringify(expected)}`);
};
const pillFg = () => document.documentElement.style.getPropertyValue("--pill-fg");
const pillInkShadow = () => document.documentElement.style.getPropertyValue("--pill-ink-shadow");

// A microtask tick, so a `.then(setInk)` chain already scheduled gets to run
// before the next assertion reads its effect.
const tick = () => new Promise((r) => setImmediate(r));

async function run() {
  // The module's own shipped default (on, 60) applies before ink_prefs() answers.
  applyFloatTheme("dark", { accent: "#4a7dd8" });
  check("pre-fetch default: #4a7dd8 (medium blue) stays white", pillFg(), "#fff");

  // ink_prefs() resolves with the shipped default explicitly.
  resolveInkPrefs({ auto: true, threshold: 60 });
  await tick();
  applyFloatTheme("dark", { accent: "#4a7dd8" });
  check("threshold 60: #4a7dd8 stays white (red under the old fixed 0.179 split)", pillFg(), "#fff");
  applyFloatTheme("dark", { accent: "#ffd54f" });
  check("threshold 60: #ffd54f (light amber) turns black", pillFg(), "#000");

  // The dark halo behind white text: none where black text was already
  // picked, none on a colour dark enough that white reads fine on its own,
  // and the halo value on a colour past the contrast crossover but still
  // under the threshold (so it keeps white text, just with help).
  applyFloatTheme("dark", { accent: "#ffffff" });
  check("threshold 60: #ffffff (black text) gets no halo", pillInkShadow(), "none");
  applyFloatTheme("dark", { accent: "#4fa3e0" });
  check("threshold 60: #4fa3e0 (white text, past the crossover) gets a halo",
    pillInkShadow(), "0 0 2px rgba(0, 0, 0, 0.85), 0 1px 3px rgba(0, 0, 0, 0.7)");
  applyFloatTheme("dark", { accent: "#202020" });
  check("threshold 60: #202020 (dark enough for white on its own) gets no halo", pillInkShadow(), "none");

  // A later theme-changed event can still flip the switch off, live.
  themeHandler({ payload: { ink: { auto: false, threshold: 60 } } });
  applyFloatTheme("dark", { accent: "#4a7dd8" });
  check("switch off: #4a7dd8 stays white", pillFg(), "#fff");
  applyFloatTheme("dark", { accent: "#ffd54f" });
  check("switch off: #ffd54f stays white too", pillFg(), "#fff");

  if (problems.length) {
    console.log(`${problems.length} problem(s):`);
    for (const p of problems) console.log(" - " + p);
    process.exit(1);
  } else {
    console.log("PASS");
  }
}
run();
