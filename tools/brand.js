// Brand generator, stamps the product identity from brands/<slug>.json onto the
// files that carry it, so a rebrand is one command instead of ~100 hand edits.
//
//   node tools/brand.js <slug>                 apply in place (working tree)
//   node tools/brand.js <slug> --out <dir>     copy the tree, then stamp the copy
//   node tools/brand.js <slug> --icons         also regenerate src-tauri/icons/*
//   node tools/brand.js <slug> --docs          rewrite the prose files in place too
//   node tools/brand.js <slug> --changelog     ... and CHANGELOG.md with them
//   node tools/brand.js --list                 available brands
//
// Applying a brand is idempotent, and applying the previous one restores the tree
// byte for byte (verify with `git status`). Everything the app resolves at runtime
// comes from src-tauri/src/brand.rs and ui/brand.js, both written from scratch here.
//
// --out is the way to produce a branded product repo: it copies what git would
// commit, stamps the copy and rewrites the prose, leaving this tree untouched.
"use strict";

const fs = require("fs");
const path = require("path");
const { execFileSync } = require("child_process");

const ROOT = path.resolve(__dirname, "..");
const BRANDS = path.join(ROOT, "brands");

// ---------- helpers ----------

const read = (p) => fs.readFileSync(p, "utf8");
// Explicit UTF-8 without BOM: i18n.js and the docs carry CJK/Cyrillic/Devanagari.
const write = (p, s) => fs.writeFileSync(p, s, { encoding: "utf8" });

function fail(msg) {
  console.error("brand: " + msg);
  process.exit(1);
}

// Replace exactly one anchored occurrence. Anything else means the file drifted
// away from what this generator knows how to stamp, that must be loud, not silent.
function sub(text, re, replacement, what) {
  const hits = text.match(new RegExp(re.source, re.flags.includes("g") ? re.flags : re.flags + "g"));
  if (!hits || hits.length !== 1) {
    fail(`${what}: expected exactly one match for ${re}, found ${hits ? hits.length : 0}`);
  }
  return text.replace(re, replacement);
}

// ---------- brand config ----------

const RULES = {
  displayName: { re: /^[A-Za-z0-9][A-Za-z0-9 ._-]{0,39}$/, req: true },
  slug: { re: /^[a-z0-9]+(-[a-z0-9]+)*$/, max: 40, req: true },
  identifier: { re: /^[a-z0-9]+(\.[a-z0-9]+)+$/, max: 80, req: true },
  description: { re: /^[^"\\\x00-\x1f]{1,120}$/, req: true },
  npmDescription: { re: /^[^"\\\x00-\x1f]{1,160}$/, req: false },
  dataFolder: { re: /^[A-Za-z0-9._-]{1,40}$/, req: true },
  autostartKey: { re: /^[A-Za-z0-9._-]{1,40}$/, req: true },
  tempDir: { re: /^[A-Za-z0-9._-]{1,40}$/, req: true },
  tempPrefix: { re: /^[A-Za-z0-9._-]{1,40}$/, req: true },
  windowClass: { re: /^[A-Za-z0-9._-]{1,40}$/, req: true },
  userAgent: { re: /^[A-Za-z0-9._-]{1,40}$/, req: true },
  repo: { re: /^[A-Za-z0-9._-]{1,40}\/[A-Za-z0-9._-]{1,40}$/, req: true },
  legacyDataFolder: { re: /^([A-Za-z0-9._-]{1,40})?$/, req: false },
  legacyAutostartKey: { re: /^([A-Za-z0-9._-]{1,40})?$/, req: false },
  legacyScreenshotPrefix: { re: /^([A-Za-z0-9._-]{1,40})?$/, req: false },
  icon: { asset: true, req: true },
  installerIcon: { asset: true, req: true },
  // Feature switches this brand ships the other way round than the app does, e.g.
  // { "mdPreview": false }. Only the shipped default moves, a stored user choice
  // still wins. Absent means "every feature ships the way the app defines it".
  flagDefaults: { flags: true, req: false },
  // Fork vocabulary. The app's own texts keep saying "prompt"; a brand that calls
  // its unit something else lists, per language, every surface form to swap and what
  // to swap it for. Absent means "the texts already use this brand's word".
  //   "terms": { "item": "entry", "itemPlural": "entries",
  //              "forms": { "en": { "prompt": "entry", "prompts": "entries" }, … } }
  terms: { terms: true, req: false },
};

// Flag keys are read straight into ui/brand.js, so keep them to the shape a JS
// identifier has and the values to real booleans.
function safeFlagDefaults(v, field) {
  if (v === undefined || v === null || v === "") return {};
  if (typeof v !== "object" || Array.isArray(v)) fail(`${field}: must be an object`);
  const out = {};
  for (const key of Object.keys(v).sort()) {
    if (!/^[A-Za-z][A-Za-z0-9]{0,39}$/.test(key)) fail(`${field}: "${key}" is not a feature key`);
    if (typeof v[key] !== "boolean") fail(`${field}.${key}: must be true or false`);
    out[key] = v[key];
  }
  return out;
}

// The vocabulary is stamped straight into generated source, so both sides of every
// pair stay plain words and the language keys stay language codes.
function safeTerms(v, field) {
  if (v === undefined || v === null || v === "") return null;
  if (typeof v !== "object" || Array.isArray(v)) fail(`${field}: must be an object`);
  for (const key of Object.keys(v)) {
    if (!["item", "itemPlural", "forms"].includes(key)) fail(`${field}: unknown key "${key}"`);
  }
  const word = (w, where) => {
    if (typeof w !== "string" || !/^[^"\\\x00-\x1f]{1,40}$/.test(w)) fail(`${where}: not a usable word`);
    return w;
  };
  const out = { item: word(v.item, `${field}.item`), itemPlural: word(v.itemPlural, `${field}.itemPlural`), forms: {} };
  const forms = v.forms || {};
  if (typeof forms !== "object" || Array.isArray(forms)) fail(`${field}.forms: must be an object`);
  for (const lang of Object.keys(forms).sort()) {
    if (!/^[a-z]{2}$/.test(lang)) fail(`${field}.forms: "${lang}" is not a language code`);
    const map = forms[lang];
    if (typeof map !== "object" || Array.isArray(map)) fail(`${field}.forms.${lang}: must be an object`);
    const pairs = {};
    for (const from of Object.keys(map).sort((a, b) => b.length - a.length)) {
      pairs[word(from, `${field}.forms.${lang} key`)] = word(map[from], `${field}.forms.${lang}.${from}`);
    }
    out.forms[lang] = pairs;
  }
  return out;
}

// A path out of the config may never leave the repo or climb out of it.
function safeAsset(rel, field) {
  if (typeof rel !== "string" || !rel || rel.length > 200) fail(`${field}: must be a short relative path`);
  if (path.isAbsolute(rel) || /^[A-Za-z]:/.test(rel) || rel.includes("..") || rel.includes("\0")) {
    fail(`${field}: must be a relative path inside the repo (got "${rel}")`);
  }
  if (!rel.toLowerCase().endsWith(".svg")) fail(`${field}: must point at an .svg`);
  const abs = path.resolve(ROOT, rel);
  if (!(abs + path.sep).startsWith(ROOT + path.sep)) fail(`${field}: resolves outside the repo`);
  if (!fs.existsSync(abs)) fail(`${field}: file not found (${rel})`);
  return rel.split(path.sep).join("/");
}

function loadBrand(slug) {
  if (!/^[a-z0-9]+(-[a-z0-9]+)*$/.test(slug)) fail(`"${slug}" is not a valid brand name`);
  const file = path.join(BRANDS, slug + ".json");
  if (!fs.existsSync(file)) fail(`no such brand config: brands/${slug}.json`);
  let b;
  try {
    b = JSON.parse(read(file));
  } catch (e) {
    fail(`brands/${slug}.json is not valid JSON: ${e.message}`);
  }
  for (const key of Object.keys(b)) if (!RULES[key]) fail(`brands/${slug}.json: unknown field "${key}"`);
  for (const [key, rule] of Object.entries(RULES)) {
    let v = b[key];
    if (rule.flags) {
      b[key] = safeFlagDefaults(b[key], key);
      continue;
    }
    if (rule.terms) {
      b[key] = safeTerms(b[key], key);
      continue;
    }
    if (v === undefined || v === null) v = "";
    if (typeof v !== "string") fail(`${key}: must be a string`);
    if (rule.req && !v) fail(`${key}: required`);
    if (rule.asset) {
      b[key] = safeAsset(v, key);
      continue;
    }
    if (rule.max && v.length > rule.max) fail(`${key}: longer than ${rule.max} characters`);
    if (v && !rule.re.test(v)) fail(`${key}: "${v}" does not match ${rule.re}`);
    b[key] = v;
  }
  if (b.slug !== slug) fail(`brands/${slug}.json declares slug "${b.slug}"`);
  if (!b.npmDescription) b.npmDescription = b.description;
  return b;
}

// The brand the tree currently carries, read back from the generated Rust module.
// Prose files are token-mapped from these values, which is what makes switching
// back and forth lossless.
function readCurrentBrand(root) {
  const file = path.join(root, "src-tauri", "src", "brand.rs");
  if (!fs.existsSync(file)) return null;
  const src = read(file);
  const get = (name) => {
    const m = src.match(new RegExp(`pub const ${name}: &str = "([^"]*)";`));
    return m ? m[1] : null;
  };
  const cur = {
    displayName: get("DISPLAY_NAME"),
    slug: get("SLUG"),
    identifier: get("IDENTIFIER"),
    dataFolder: get("DATA_FOLDER"),
    autostartKey: get("AUTOSTART_KEY"),
    tempPrefix: get("TEMP_PREFIX"),
    repo: get("REPO"),
  };
  return cur.slug ? cur : null;
}

// ---------- generated files ----------

const rs = (v) => JSON.stringify(String(v)); // values are pre-validated; this only quotes

function brandRs(b) {
  return `// Product identity. GENERATED, run \`node tools/brand.js <brand>\` to change it;
// the values live in brands/<brand>.json. Nothing else in the Rust code may spell
// the product name.
//
// These strings decide where an installation keeps its data and how Windows knows
// the app. Changing one strands every existing install, brand_values_are_pinned in
// lib.rs asserts them for exactly that reason.

pub const DISPLAY_NAME: &str = ${rs(b.displayName)};
pub const SLUG: &str = ${rs(b.slug)};
pub const IDENTIFIER: &str = ${rs(b.identifier)};
/// Store folder under %APPDATA%\\Roaming. Deliberately not the bundle identifier.
pub const DATA_FOLDER: &str = ${rs(b.dataFolder)};
/// Value name under HKCU\\...\\Run.
pub const AUTOSTART_KEY: &str = ${rs(b.autostartKey)};
/// Sub-folder under %TEMP% for unpacked runtime files.
pub const TEMP_DIR: &str = ${rs(b.tempDir)};
/// Leading part of loose temp files (snip previews, update staging).
pub const TEMP_PREFIX: &str = ${rs(b.tempPrefix)};
/// Win32 class of the hidden clipboard-watcher window.
pub const WINDOW_CLASS: &str = ${rs(b.windowClass)};
/// User-Agent for the update check and the WebView2 bootstrapper download.
pub const USER_AGENT: &str = ${rs(b.userAgent)};
/// GitHub \`owner/name\` the update check is pinned to.
pub const REPO: &str = ${rs(b.repo)};

// Historical identifiers. Empty means this brand never had one, so the matching
// migration is skipped. They are frozen: a value here describes what an older
// version of THIS product wrote, so it can never be re-derived from the name.
/// Store folder an earlier version used under %APPDATA%.
pub const LEGACY_DATA_FOLDER: &str = ${rs(b.legacyDataFolder)};
/// Autostart value name an earlier version registered.
pub const LEGACY_AUTOSTART_KEY: &str = ${rs(b.legacyAutostartKey)};
/// File-name prefix of screenshots an earlier version saved.
pub const LEGACY_SCREENSHOT_PREFIX: &str = ${rs(b.legacyScreenshotPrefix)};

// What this brand calls the thing it stores. The backend only needs it for the few
// English messages it hands back to the UI; every translated string is handled by
// TERMS in ui/brand.js.
/// Singular, lowercase.
pub const ITEM: &str = ${rs((b.terms && b.terms.item) || "prompt")};
/// Plural, lowercase.
pub const ITEM_PLURAL: &str = ${rs((b.terms && b.terms.itemPlural) || "prompts")};
`;
}

function brandJs(b) {
  return `// Product name for the webview windows. GENERATED, run \`node tools/brand.js <brand>\`
// to change it; the values live in brands/<brand>.json.
//
// t() substitutes {app} in every translated string with this, and applyI18n fills
// each [data-brand] element with it, so no markup or translation spells the name.
export const APP_NAME = ${JSON.stringify(b.displayName)};

// Features this brand ships differently from the app's own default. shipsOn() in
// main.js reads it; a value the user has stored always wins over it.
export const FLAG_DEFAULTS = ${flagDefaultsJs(b.flagDefaults)};

// Fork vocabulary: per language, what to swap in every translated string before it
// reaches the screen. Empty for the brand whose own word the texts already use.
export const TERMS = ${termsJs(b.terms)};
`;
}

// Longest form first, so "prompts" is never eaten by the rule for "prompt".
function termsJs(terms) {
  const forms = (terms && terms.forms) || {};
  const langs = Object.keys(forms).sort();
  if (!langs.length) return "{}";
  const NL = "\n";
  const body = langs.map((lang) => {
    const map = forms[lang];
    const keys = Object.keys(map).sort((a, b) => b.length - a.length || a.localeCompare(b));
    const pairs = keys.map((k) => `    ${JSON.stringify(k)}: ${JSON.stringify(map[k])},`).join(NL);
    return `  ${lang}: {${NL}${pairs}${NL}  },`;
  }).join(NL);
  return `{${NL}${body}${NL}}`;
}

// One key per line, sorted, so the generated file is byte-identical every run.
function flagDefaultsJs(map) {
  const keys = Object.keys(map || {});
  if (!keys.length) return "{}";
  return "{\n" + keys.map((k) => `  ${k}: ${map[k]},\n`).join("") + "}";
}

// ---------- stamping ----------

function stampManifests(root, b, current) {
  // package.json
  const pkgPath = path.join(root, "package.json");
  let pkg = read(pkgPath);
  pkg = sub(pkg, /^(  "name": ")[^"]*(")/m, `$1${b.slug}$2`, "package.json name");
  pkg = sub(pkg, /^(  "description": ")[^"]*(")/m, `$1${b.npmDescription}$2`, "package.json description");
  pkg = sub(
    pkg,
    /^(    "release:publish": ").*(",?)$/m,
    `$1gh release create v%npm_package_version% \\"src-tauri/target/release/${b.slug}.exe\\" ` +
      `\\"dist/${b.displayName}_%npm_package_version%_x64-setup.exe\\" ` +
      `\\"dist/${b.displayName}_%npm_package_version%_x64-setup.exe.sha256\\" ` +
      `--title \\"${b.displayName} %npm_package_version%\\" --notes-file docs/release-notes.md$2`,
    "package.json release:publish"
  );
  write(pkgPath, pkg);

  // Cargo.toml, package name, lib name and description.
  const cargoPath = path.join(root, "src-tauri", "Cargo.toml");
  let cargo = read(cargoPath);
  cargo = sub(cargo, /^name = "[a-z0-9-]+"$/m, `name = "${b.slug}"`, "Cargo.toml package name");
  cargo = sub(cargo, /^name = "[a-z0-9_]+_lib"$/m, `name = "${b.slug.replace(/-/g, "_")}_lib"`, "Cargo.toml lib name");
  cargo = sub(cargo, /^description = ".*"$/m, `description = "${b.description}"`, "Cargo.toml description");
  write(cargoPath, cargo);

  // main.rs follows the lib name.
  const mainPath = path.join(root, "src-tauri", "src", "main.rs");
  let mainRs = read(mainPath);
  mainRs = sub(mainRs, /^    [a-z0-9_]+_lib::run\(\);$/m, `    ${b.slug.replace(/-/g, "_")}_lib::run();`, "main.rs");
  write(mainPath, mainRs);

  // tauri.conf.json, product name, identifier, window title, asset-protocol denies.
  const confPath = path.join(root, "src-tauri", "tauri.conf.json");
  let conf = read(confPath);
  conf = sub(conf, /^(  "productName": ")[^"]*(")/m, `$1${b.displayName}$2`, "tauri.conf productName");
  conf = sub(conf, /^(  "identifier": ")[^"]*(")/m, `$1${b.identifier}$2`, "tauri.conf identifier");
  conf = sub(conf, /^(        "title": ")[^"]*(")/m, `$1${b.displayName}$2`, "tauri.conf window title");
  conf = sub(
    conf,
    /"deny": \["\*\*\/data\.db", "\*\*\/data\.db-wal", "\*\*\/data\.db-shm", "[^"]*", "[^"]*"\]/,
    `"deny": ["**/data.db", "**/data.db-wal", "**/data.db-shm", "**/${b.dataFolder}/settings.json", "**/${b.dataFolder}/prompts.json"]`,
    "tauri.conf assetProtocol deny"
  );
  write(confPath, conf);

  // Lock files name this crate/package too. They are keyed by the OLD slug, so
  // this only runs when the tree already declares a brand.
  if (!current || current.slug === b.slug) return;
  const old = current.slug.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const lockJson = path.join(root, "package-lock.json");
  if (fs.existsSync(lockJson)) {
    let s = read(lockJson);
    const re = new RegExp(`"name": "${old}"`, "g");
    const n = (s.match(re) || []).length;
    if (n !== 2) fail(`package-lock.json: expected 2 name entries, found ${n}`);
    write(lockJson, s.replace(re, `"name": "${b.slug}"`));
  }
  const lockToml = path.join(root, "src-tauri", "Cargo.lock");
  if (fs.existsSync(lockToml)) {
    let s = read(lockToml);
    s = sub(s, new RegExp(`^name = "${old}"$`, "m"), `name = "${b.slug}"`, "Cargo.lock crate name");
    write(lockToml, s);
  }
}

function stampBuildFiles(root, b) {
  const batPath = path.join(root, "build.bat");
  let bat = read(batPath);
  bat = sub(bat, /^set "APP_NAME=.*"$/m, `set "APP_NAME=${b.displayName}"`, "build.bat APP_NAME");
  bat = sub(bat, /^set "APP_SLUG=.*"$/m, `set "APP_SLUG=${b.slug}"`, "build.bat APP_SLUG");
  write(batPath, bat);

  // The uninstaller wipes the store folder, which is brand-specific.
  const nsiPath = path.join(root, "src-tauri", "installer.nsi");
  let nsi = read(nsiPath);
  // Only the literal folder line, the sibling `${BUNDLEID}` lines stay untouched.
  nsi = sub(
    nsi,
    /^(    RmDir \/r "\$APPDATA\\)[^"${}]+(")$/m,
    `$1${b.dataFolder}$2`,
    "installer.nsi store folder"
  );
  nsi = sub(
    nsi,
    /^(!define AUTOSTARTKEY ")[^"${}]+(")$/m,
    `$1${b.autostartKey}$2`,
    "installer.nsi autostart key"
  );
  write(nsiPath, nsi);
}

// The pin test duplicates the brand literals on purpose, a refactor that moves a
// user's data must break a test, not an installation.
function stampPinTest(root, b) {
  const libPath = path.join(root, "src-tauri", "src", "lib.rs");
  const src = read(libPath);
  const start = "        // BRAND-PIN-START\n";
  const end = "        // BRAND-PIN-END\n";
  const i = src.indexOf(start);
  const j = src.indexOf(end);
  if (i < 0 || j < 0 || j < i) fail("lib.rs: BRAND-PIN markers not found");
  const body =
    `        assert_eq!(brand::DISPLAY_NAME, ${rs(b.displayName)});\n` +
    `        assert_eq!(brand::SLUG, ${rs(b.slug)});\n` +
    `        assert_eq!(brand::IDENTIFIER, ${rs(b.identifier)});\n` +
    `        assert_eq!(brand::DATA_FOLDER, ${rs(b.dataFolder)});\n` +
    `        assert_eq!(brand::AUTOSTART_KEY, ${rs(b.autostartKey)});\n` +
    `        assert_eq!(brand::TEMP_DIR, ${rs(b.tempDir)});\n` +
    `        assert_eq!(brand::TEMP_PREFIX, ${rs(b.tempPrefix)});\n` +
    `        assert_eq!(brand::WINDOW_CLASS, ${rs(b.windowClass)});\n` +
    `        assert_eq!(brand::USER_AGENT, ${rs(b.userAgent)});\n` +
    `        assert_eq!(brand::REPO, ${rs(b.repo)});\n` +
    `        assert_eq!(brand::LEGACY_DATA_FOLDER, ${rs(b.legacyDataFolder)});\n` +
    `        assert_eq!(brand::LEGACY_AUTOSTART_KEY, ${rs(b.legacyAutostartKey)});\n` +
    `        assert_eq!(brand::LEGACY_SCREENSHOT_PREFIX, ${rs(b.legacyScreenshotPrefix)});\n` +
    `        assert_eq!(env!("CARGO_PKG_NAME"), ${rs(b.slug)});\n` +
    `        let conf = include_str!("../tauri.conf.json");\n` +
    `        assert!(conf.contains(${rs(`"productName": ${rs(b.displayName)}`)}));\n` +
    `        assert!(conf.contains(${rs(`"identifier": ${rs(b.identifier)}`)}));\n` +
    `        let js = include_str!("../../ui/brand.js");\n` +
    `        assert!(js.contains(${rs(`APP_NAME = ${rs(b.displayName)};`)}));\n`;
  write(libPath, src.slice(0, i + start.length) + body + src.slice(j));
}

// Prose files carry the name in running text; map the old brand's tokens onto the
// new ones, longest first so a token never eats part of a longer one.
//
// This step is NOT reversible in general: two of a brand's tokens may be the same
// word (Clipboard-Saver's display name and store folder are), and once both have
// been written the way back cannot tell them apart. That is why in-place switching
// leaves prose alone unless --docs is passed, see stampDocs.
function tokenMap(oldB, newB) {
  const pairs = [
    [oldB.identifier, newB.identifier],
    [oldB.repo, newB.repo],
    [oldB.dataFolder, newB.dataFolder],
    [oldB.displayName, newB.displayName],
    [oldB.slug, newB.slug],
    [oldB.autostartKey, newB.autostartKey],
    [oldB.tempPrefix, newB.tempPrefix],
  ].filter(([o, n]) => o && n && o !== n);
  // Deduplicate, then apply the longest source token first.
  const seen = new Set();
  return pairs
    .filter(([o]) => (seen.has(o) ? false : seen.add(o)))
    .sort((a, b2) => b2[0].length - a[0].length);
}

function stampDocs(root, oldB, b, withChangelog, enabled) {
  if (!enabled) {
    console.log("  docs      left as they are (--out maps them; --docs forces it in place)");
    return;
  }
  if (!oldB) {
    console.warn("brand: no current brand found, leaving the prose files untouched");
    return;
  }
  const map = tokenMap(oldB, b);
  if (!map.length) return;
  const files = ["README.md", "docs/release-notes.md", "THIRD-PARTY-NOTICES.md"];
  if (withChangelog) files.push("CHANGELOG.md");
  for (const rel of files) {
    const p = path.join(root, rel);
    if (!fs.existsSync(p)) continue;
    let text = read(p);
    // Placeholders keep an already-replaced token from being matched again.
    const holes = [];
    for (const [from, to] of map) {
      text = text.split(from).join(`\u0000${holes.push(to) - 1}\u0000`);
    }
    text = text.replace(/\u0000(\d+)\u0000/g, (_, n) => holes[Number(n)]);
    write(p, text);
  }
}

// ---------- out-of-tree copy ----------

// What git would commit is exactly what belongs in a product repo: tracked files
// plus anything new that is not ignored, which already leaves out build output and
// local notes. Build inputs that are ignored on purpose (the pdfium DLL) are added
// back by name.
function gitFiles() {
  try {
    const args = ["-C", ROOT, "ls-files", "-z", "--cached", "--others", "--exclude-standard"];
    const out = execFileSync("git", args, { encoding: "utf8" });
    const list = out.split("\0").filter(Boolean);
    return list.length ? list : null;
  } catch {
    return null;
  }
}

const EXTRA_FILES = ["src-tauri/pdfium.dll", "src-tauri/pdfium-LICENSE"];

function copyTree(dst) {
  const tracked = gitFiles();
  // Without git there is no trustworthy answer to "what may leave this repo", and
  // guessing risks copying local notes or credentials into a public tree.
  if (!tracked) fail("git is required to copy a branded tree (it decides what ships)");
  for (const rel of tracked.concat(EXTRA_FILES)) {
    const from = path.join(ROOT, rel);
    if (!fs.existsSync(from)) continue;
    const to = path.join(dst, rel);
    fs.mkdirSync(path.dirname(to), { recursive: true });
    fs.copyFileSync(from, to);
  }
}

// A product repo carries its own brand only, the other configs stay behind.
function pruneBrands(dst, b) {
  const dir = path.join(dst, "brands");
  if (!fs.existsSync(dir)) return;
  for (const f of fs.readdirSync(dir)) {
    if (f === "icons" || f === b.slug + ".json") continue;
    fs.rmSync(path.join(dir, f), { recursive: true, force: true });
  }
  const icons = path.join(dir, "icons");
  if (!fs.existsSync(icons)) return;
  const keep = new Set([path.basename(b.icon), path.basename(b.installerIcon)]);
  for (const f of fs.readdirSync(icons)) if (!keep.has(f)) fs.rmSync(path.join(icons, f), { force: true });
}

function prepareOut(dir) {
  if (typeof dir !== "string" || !dir || dir.includes("\0")) fail("--out needs a directory");
  const abs = path.resolve(dir);
  if (abs === ROOT || (abs + path.sep).startsWith(ROOT + path.sep)) {
    fail("--out must point outside this repo");
  }
  if (fs.existsSync(abs)) {
    if (!fs.statSync(abs).isDirectory()) fail(`--out exists and is not a directory: ${abs}`);
    if (fs.readdirSync(abs).length) fail(`--out is not empty: ${abs}`);
  }
  copyTree(abs);
  return abs;
}

// ---------- icons ----------

function stampIcons(root, b, regenerate) {
  fs.copyFileSync(path.join(ROOT, b.icon), path.join(root, "ui", "assets", "icon.svg"));
  fs.copyFileSync(path.join(ROOT, b.installerIcon), path.join(root, "ui", "assets", "installer-icon.svg"));
  if (!regenerate) {
    console.log("  icons     ui/assets/*.svg swapped (pass --icons to rebuild src-tauri/icons)");
    return;
  }
  // Always driven from this repo, an --out copy has no node_modules of its own.
  // The Tauri CLI is invoked through node directly (not `npm run tauri`): no shell
  // is involved, so nothing from the config can be read as a command, and there is
  // no `--` for npm to swallow the path argument with.
  const cli = path.join(ROOT, "node_modules", "@tauri-apps", "cli", "tauri.js");
  if (!fs.existsSync(cli)) fail("--icons needs the Tauri CLI, run `npm install` first");
  const icons = path.join(root, "src-tauri", "icons");
  const run = (svg, outDir) =>
    execFileSync(process.execPath, [cli, "icon", path.join(ROOT, svg), "-o", outDir], {
      cwd: ROOT,
      stdio: "inherit",
    });
  run(b.icon, icons);
  const tmp = path.join(icons, ".installer");
  run(b.installerIcon, tmp);
  fs.copyFileSync(path.join(tmp, "icon.ico"), path.join(icons, "installer-icon.ico"));
  fs.rmSync(tmp, { recursive: true, force: true });
  // Windows-only project: drop the mobile/macOS/Store sets tauri icon also writes.
  for (const d of ["android", "ios"]) fs.rmSync(path.join(icons, d), { recursive: true, force: true });
  for (const f of fs.readdirSync(icons)) {
    if (f === "icon.icns" || f === "StoreLogo.png" || f.startsWith("Square")) {
      fs.rmSync(path.join(icons, f), { force: true });
    }
  }
  console.log("  icons     regenerated from " + b.icon);
}

// ---------- main ----------

function main(argv) {
  if (argv.includes("--list") || !argv.length) {
    const list = fs.readdirSync(BRANDS).filter((f) => f.endsWith(".json")).map((f) => f.slice(0, -5));
    console.log("brands: " + list.join(", "));
    console.log("usage:  node tools/brand.js <brand> [--out <dir>] [--icons] [--docs] [--changelog]");
    return;
  }
  const slug = argv[0];
  const flags = argv.slice(1);
  const outIdx = flags.indexOf("--out");
  const out = outIdx >= 0 ? flags[outIdx + 1] : null;
  if (outIdx >= 0 && !out) fail("--out needs a directory");
  for (const f of flags) {
    if (f.startsWith("--") && !["--out", "--icons", "--changelog", "--docs"].includes(f)) fail(`unknown flag ${f}`);
  }

  const b = loadBrand(slug);
  const root = out ? prepareOut(out) : ROOT;
  const current = readCurrentBrand(root);
  if (current && current.slug === b.slug && !out) console.log(`brand: ${b.displayName} is already applied, re-stamping`);

  write(path.join(root, "src-tauri", "src", "brand.rs"), brandRs(b));
  write(path.join(root, "ui", "brand.js"), brandJs(b));
  stampManifests(root, b, current);
  stampBuildFiles(root, b);
  stampPinTest(root, b);
  stampDocs(root, current, b, flags.includes("--changelog"), !!out || flags.includes("--docs"));
  stampIcons(root, b, flags.includes("--icons"));
  if (out) pruneBrands(root, b);

  console.log(`brand: ${b.displayName} (${b.slug}) applied to ${root}`);
  if (out) console.log("  the working tree was not touched");
}

main(process.argv.slice(2));
