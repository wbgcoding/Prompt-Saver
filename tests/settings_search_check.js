// Settings search (ui/settingsSearch.js): i18nHit, settingHit, settingsDirectoryHit and the
// SETTINGS_DIRECTORY table. Runs the real file in a node vm with its imports stubbed (a small DICT
// stands in for the active language, the gate test is a switchable fake), over the real English
// texts (ui/i18n/en.js) and the real tip list (ui/expertConfig.js). Also checks that every
// directory entry names real texts, a real gate and a target that exists in the markup.
// Run:  node tests/settings_search_check.js      Exit: 0 = clean, 1 = problems.
const fs = require("fs");
const path = require("path");
const vm = require("vm");
const { pathToFileURL } = require("url");

const uiDir = path.join(__dirname, "..", "ui");
const problems = [];
let count = 0;
const check = (label, actual, expected) => {
  count++;
  const a = JSON.stringify(actual), e = JSON.stringify(expected);
  if (a !== e) problems.push(`${label}: got ${a}, expected ${e}`);
};

async function main() {
  globalThis.document = globalThis.document || { addEventListener() {}, removeEventListener() {} };
  const EN = (await import(pathToFileURL(path.join(uiDir, "i18n", "en.js")).href)).default;
  const cfg = await import(pathToFileURL(path.join(uiDir, "expertConfig.js")).href);

  const src = fs.readFileSync(path.join(uiDir, "settingsSearch.js"), "utf8")
    .replace(/\r\n/g, "\n")
    .replace(/^import .*$/gm, "")
    .replace(/^export /gm, "");
  const sb = {
    DICT: {},
    EN,
    EXPERT_TIP_SET: cfg.EXPERT_TIP_SET,
    GATES: {},
    gateOpen: (g) => (g ? sb.GATES[g] !== false : true),
    settings: { auto_update: true },
    $: () => ({ checked: false }),
    settingsEl: null, assignLibraryTab() {}, hideSettings() {}, openBackups() {}, showExpertPage() {},
  };
  vm.createContext(sb);
  vm.runInContext(src + "\nthis.api = { i18nHit, settingHit, settingsDirectoryHit, SETTINGS_DIRECTORY, settingsDirectoryTarget };", sb);
  const api = sb.api;

  // ---- i18nHit: the active language or English
  sb.DICT = { theme: "Farbschema", language: "Ärger", emptyOne: "" };
  check("hit in the active language", api.i18nHit("theme", "farb"), true);
  check("hit in the middle of a text", api.i18nHit("theme", "schem"), true);
  check("hit in English although the language is German", api.i18nHit("theme", "theme"), true);
  check("miss in both", api.i18nHit("theme", "zzz"), false);
  check("a key unknown everywhere never hits", api.i18nHit("noSuchKeyAnywhere", "a"), false);
  check("language text is compared lower-cased (non-ascii)", api.i18nHit("language", "ärger"), true);
  check("English text is compared lower-cased", api.i18nHit("theme", "th"), true);
  check("a key present only in English still hits", api.i18nHit("tileFont", "tile"), true);
  check("an empty language text does not break the English hit", api.i18nHit("emptyOne", "x"), false);
  sb.DICT = {};
  check("an empty dictionary falls back to English", api.i18nHit("theme", "them"), true);

  // ---- settingHit: label first, then the tip of a setting that has one
  sb.DICT = {};
  check("label hit", api.settingHit("copyCooldownMs", "theme", "them"), true);
  check("a setting with a tip is found by a tip word", api.settingHit("copyCooldownMs", "theme", "double copies"), true);
  check("a tip word is found in lower case", api.settingHit("historyMax", "theme", "older ones are dropped"), true);
  check("no label hit, no tip hit", api.settingHit("copyCooldownMs", "theme", "zzzz"), false);
  check("a setting without a tip is not found by a tip word", api.settingHit("notAnExpertSetting", "theme", "double copies"), false);
  check("a setting without a tip still hits on its label", api.settingHit("notAnExpertSetting", "theme", "them"), true);
  check("a missing tip text does not throw and misses", api.settingHit("copyCooldownMs", "theme", "qqqqq"), false);
  sb.DICT = { tipCopyCooldownMs: "Doppelte Kopien verhindern" };
  check("the tip hits in the active language", api.settingHit("copyCooldownMs", "theme", "doppelte"), true);
  sb.DICT = {};

  // ---- the tip list: every member has an English tip text under the key settingHit derives
  const tipKey = (k) => `tip${k[0].toUpperCase()}${k.slice(1)}`;
  const missingTips = [...cfg.EXPERT_TIP_SET].filter((k) => !EN[tipKey(k)]);
  check("every tip-listed setting has an English tip text", missingTips, []);
  check("the tip list is not empty", cfg.EXPERT_TIP_SET.size > 50, true);

  // ---- settingsDirectoryHit (callers only read the result as true or false)
  const hit = (e, q) => !!api.settingsDirectoryHit(e, q);
  const entry = (over) => Object.assign({ key: "x", area: "normal", label: "theme" }, over);
  check("plain entry hits on its label", hit(entry(), "them"), true);
  check("plain entry misses", hit(entry(), "zzz"), false);
  check("entry tip hits", hit(entry({ label: "language", tip: "autoUpdateHint" }), "background"), true);
  check("entry without a tip does not hit on tip words", hit(entry({ label: "language" }), "background"), false);
  check("a false when() hides the entry", hit(entry({ when: () => false }), "them"), false);
  check("a true when() keeps the entry", hit(entry({ when: () => true }), "them"), true);
  sb.GATES = { quickGrid: false };
  check("a closed gate hides the entry", hit(entry({ gate: "quickGrid" }), "them"), false);
  sb.GATES = { quickGrid: true };
  check("an open gate shows the entry", hit(entry({ gate: "quickGrid" }), "them"), true);
  sb.GATES = {};
  check("when() is checked before the label", hit(entry({ when: () => false, label: "theme" }), "theme"), false);
  const real = (key) => api.SETTINGS_DIRECTORY.find((e) => e.key === key);
  sb.DICT = {};
  check("real entry: theme by its English name", hit(real("theme-select"), "theme"), true);
  check("real entry: opt-autostart found by 'start'", hit(real("opt-autostart"), "start"), true);
  sb.$ = () => ({ checked: false });
  check("real entry: start-minimized is hidden while autostart is off", hit(real("opt-startmin"), "in the background"), false);
  sb.$ = () => ({ checked: true });
  check("real entry: start-minimized shows while autostart is on", hit(real("opt-startmin"), "in the background"), true);
  sb.settings = { auto_update: false };
  check("real entry: update interval hidden while updates are off", hit(real("opt-update-interval"), "update"), false);
  sb.settings = { auto_update: true };
  check("real entry: update interval shown while updates are on", hit(real("opt-update-interval"), "update"), true);
  sb.settings = {};
  check("real entry: update interval shown when the setting was never stored", hit(real("opt-update-interval"), "update"), true);

  // ---- the directory itself
  const dir = api.SETTINGS_DIRECTORY;
  check("the directory is not empty", dir.length > 20, true);
  const keys = dir.map((e) => e.key);
  check("keys are unique", keys.filter((k, i) => keys.indexOf(k) !== i), []);
  check("every area is one of the four known", dir.filter((e) => !["normal", "toolbar", "library", "backup"].includes(e.area)).map((e) => e.key), []);
  check("every label is a real English text", dir.filter((e) => !EN[e.label]).map((e) => e.key), []);
  check("every tip is a real English text", dir.filter((e) => e.tip && !EN[e.tip]).map((e) => e.key), []);
  check("every entry has a key and a label", dir.filter((e) => !e.key || !e.label).length, 0);
  const flagNames = new Set([...cfg.ALL_FLAG_KEYS, ...cfg.OPT_FLAG_KEYS, ...Object.keys(cfg.EXPERT_VALUES)]);
  check("every gate names a real flag or value", dir.filter((e) => e.gate && !flagNames.has(e.gate.replace(/^!/, ""))).map((e) => e.key), []);
  check("when, if present, is a function", dir.filter((e) => e.when && typeof e.when !== "function").map((e) => e.key), []);
  sb.DICT = {};
  check("every entry hits on its own English label text", dir.filter((e) => !api.i18nHit(e.label, EN[e.label].toLowerCase())).map((e) => e.key), []);

  // Every entry resolves to a real target: an element id in the markup, or a row hook in the backup panel code.
  const html = fs.readFileSync(path.join(uiDir, "index.html"), "utf8");
  const backups = fs.readFileSync(path.join(uiDir, "backups.js"), "utf8");
  const noTarget = dir.filter((e) => {
    if (e.area === "backup") return !backups.includes(`"${e.key}"`);
    return !new RegExp(`\\bid="${e.key}"`).test(html);
  }).map((e) => e.key);
  check("every entry resolves to a real target", noTarget, []);

  // ---- settingsDirectoryTarget: backup rows are found by their row hook, the others by id
  const queries = [];
  globalThis.document.querySelector = (q) => { queries.push(q); return { hook: true }; };
  sb.document = globalThis.document;
  check("backup target is found by its row hook", api.settingsDirectoryTarget({ area: "backup", key: "backup-keep" }), { hook: true });
  check("backup target query", queries[0], '#backup-modal [data-setting-row="backup-keep"]');
  const field = { id: "field" };
  sb.$ = () => ({ closest: (s) => (s === ".field" ? field : null) });
  check("normal target is the surrounding field", api.settingsDirectoryTarget({ area: "normal", key: "theme-select" }), field);
  const bare = { id: "bare", closest: () => null };
  sb.$ = () => bare;
  check("a target without a field wrapper is the element itself", api.settingsDirectoryTarget({ area: "toolbar", key: "quick-grid" }), bare);
  sb.$ = () => null;
  check("a missing element gives null", api.settingsDirectoryTarget({ area: "normal", key: "nope" }), null);
}

main().then(() => {
  if (problems.length) {
    console.log(`${problems.length} problem(s):`);
    for (const p of problems) console.log(" - " + p);
    process.exit(1);
  }
  console.log(`settings_search_check: ${count} assertions OK`);
}).catch((e) => { console.log(e.stack || e); process.exit(1); });
