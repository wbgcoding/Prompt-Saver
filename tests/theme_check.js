// Themes (ui/theme.js): cssColorToHex, customThemeId / findCustomTheme / customThemes,
// activeThemeVars, themeBase (including a retired base), selectChevronVar, applyThemeVars /
// applyTheme (inline overrides, derived glow and chevron) and loadPalette. Runs the real file in a
// node vm with its imports stubbed (a fake document element, a flag and settings), over the real
// HEX_RE and default palette taken from ui/core.js. Also holds the stylesheet in step with the
// lists in the script: every theme token and built-in theme has a rule, and each theme's baked
// select chevron matches selectChevronVar of that theme's own muted colour.
// Run:  node tests/theme_check.js      Exit: 0 = clean, 1 = problems.
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
  const shared = await import(pathToFileURL(path.join(uiDir, "floatShared.js")).href);
  const EN = (await import(pathToFileURL(path.join(uiDir, "i18n", "en.js")).href)).default;

  const coreSrc = fs.readFileSync(path.join(uiDir, "core.js"), "utf8").replace(/\r\n/g, "\n");
  const grab = (re, what) => {
    const m = re.exec(coreSrc);
    if (!m) throw new Error("core.js: " + what + " not found");
    return m[0].replace(/^export /, "");
  };
  const coreBits = [
    grab(/export const HEX_RE = .*;/, "HEX_RE"),
    grab(/export const DEFAULT_COLORS = \[[\s\S]*?\];/, "DEFAULT_COLORS"),
    grab(/export const PALETTE_LEN = .*;/, "PALETTE_LEN"),
  ].join("\n");

  const src = fs.readFileSync(path.join(uiDir, "theme.js"), "utf8")
    .replace(/\r\n/g, "\n")
    .replace(/^import .*$/gm, "")
    .replace(/^export /gm, "");

  const props = new Map();
  const root = {
    attrs: {},
    style: {
      colorScheme: "",
      setProperty: (k, v) => props.set(k, v),
      removeProperty: (k) => props.delete(k),
      getPropertyValue: (k) => props.get(k) || "",
    },
    setAttribute(k, v) { root.attrs[k] = v; },
    getAttribute(k) { return k in root.attrs ? root.attrs[k] : null; },
  };
  let palette = null;
  const sb = {
    FLAGS: { customThemes: true },
    flag: (k) => !!sb.FLAGS[k],
    settings: {},
    themeColorScheme: shared.themeColorScheme,
    document: { documentElement: root },
    assignCOLORS: (v) => { palette = v; },
  };
  vm.createContext(sb);
  vm.runInContext(
    coreBits + "\n" + src + "\nthis.api = { THEME_VARS, THEME_VAR_GROUPS, BUILTIN_THEMES, THEME_LABELS, THEME_VAR_LABELS, customThemes, customThemeId, findCustomTheme, activeThemeVars, applyTheme, applyThemeVars, themeBase, selectChevronVar, loadPalette, cssColorToHex, DEFAULT_COLORS, PALETTE_LEN, HEX_RE };",
    sb
  );
  const api = sb.api;
  const themes = [
    { id: "t1", base: "ocean", vars: { accent: "#112233", bg: "#000000" } },
    { id: "t2", base: "mono", vars: {} },
    { id: "t3", base: "nonsense", vars: { fg: "#ffffff" } },
  ];

  // ---- cssColorToHex
  const hex = (v) => api.cssColorToHex(v);
  check("hex six digits", hex("#A1B2C3"), "#a1b2c3");
  check("hex three digits expand", hex("#abc"), "#aabbcc");
  check("hex three digits upper case", hex("#FFF"), "#ffffff");
  check("hex blanks around are trimmed", hex("  #ABC  "), "#aabbcc");
  check("hex of four digits is not a colour here", hex("#abcd"), "#808080");
  check("hex of five digits is not a colour here", hex("#12345"), "#808080");
  check("hex without a hash is not a colour here", hex("abcdef"), "#808080");
  check("hex of eight digits is not a colour here", hex("#aabbccdd"), "#808080");
  check("rgb with commas", hex("rgb(255, 0, 128)"), "#ff0080");
  check("rgba drops the alpha", hex("rgba(0, 0, 0, 0.5)"), "#000000");
  check("rgb keyword in capitals", hex("RGB(1,2,3)"), "#010203");
  check("rgb with spaces and a slash alpha", hex("rgb(0 0 0 / 50%)"), "#000000");
  check("rgb with percentages", hex("rgb(100% 50% 0%)"), "#ff8000");
  check("rgb values above the range are capped", hex("rgb(300, -5, 10)"), "#ff000a");
  check("rgb fractions are rounded", hex("rgb(12.6, 0, 0)"), "#0d0000");
  check("rgb of two parts is unreadable", hex("rgb(1,2)"), "#808080");
  check("rgb of words reads as zero", hex("rgb(a,b,c)"), "#000000");
  check("a colour name is unreadable", hex("red"), "#808080");
  check("an empty value is unreadable", hex(""), "#808080");
  check("null is unreadable", hex(null), "#808080");
  check("undefined is unreadable", hex(undefined), "#808080");
  check("rgb with an unclosed bracket is unreadable", hex("rgb(1,2,3"), "#808080");

  // ---- custom theme lookup
  check("customThemeId of a custom value", api.customThemeId("custom:abc"), "abc");
  check("customThemeId keeps a colon in the id", api.customThemeId("custom:a:b"), "a:b");
  check("customThemeId of a built-in", api.customThemeId("dark"), "");
  check("customThemeId of an empty id", api.customThemeId("custom:"), "");
  check("customThemeId of null", api.customThemeId(null), "");
  check("customThemeId of undefined", api.customThemeId(undefined), "");
  check("customThemeId is case-sensitive", api.customThemeId("Custom:abc"), "");
  sb.settings = { custom_themes: themes, theme: "custom:t1" };
  check("customThemes with the feature on", api.customThemes().length, 3);
  check("findCustomTheme finds by preference", api.findCustomTheme("custom:t3").id, "t3");
  check("findCustomTheme for an unknown id", api.findCustomTheme("custom:zz"), null);
  check("findCustomTheme for a built-in", api.findCustomTheme("dark"), null);
  check("activeThemeVars of the active custom theme", api.activeThemeVars(), { accent: "#112233", bg: "#000000" });
  sb.settings.theme = "dark";
  check("activeThemeVars of a built-in is empty", api.activeThemeVars(), {});
  sb.settings.theme = "custom:gone";
  check("activeThemeVars of a deleted theme is empty", api.activeThemeVars(), {});
  sb.FLAGS = { customThemes: false };
  sb.settings.theme = "custom:t1";
  check("customThemes with the feature off is empty", api.customThemes(), []);
  check("findCustomTheme with the feature off", api.findCustomTheme("custom:t1"), null);
  check("activeThemeVars with the feature off", api.activeThemeVars(), {});
  sb.FLAGS = { customThemes: true };
  sb.settings = {};
  check("customThemes without stored themes", api.customThemes(), []);

  // ---- themeBase
  sb.settings = { custom_themes: themes };
  check("themeBase of a built-in", api.themeBase("dark"), "dark");
  check("themeBase of system", api.themeBase("system"), "system");
  check("themeBase of null passes through", api.themeBase(null), null);
  check("themeBase of a custom theme is its base", api.themeBase("custom:t1"), "ocean");
  check("themeBase maps the retired mono base to light", api.themeBase("custom:t2"), "light");
  check("themeBase of an unknown base falls back to system", api.themeBase("custom:t3"), "system");
  check("themeBase of an unknown id falls back to system", api.themeBase("custom:zz"), "system");
  sb.FLAGS = { customThemes: false };
  check("themeBase keeps the base while the feature is off", api.themeBase("custom:t1"), "ocean");
  sb.FLAGS = { customThemes: true };
  sb.settings = {};
  check("themeBase without stored themes", api.themeBase("custom:t1"), "system");
  check("every built-in base resolves to itself", api.BUILTIN_THEMES.filter((b) => {
    sb.settings = { custom_themes: [{ id: "x", base: b }] };
    return api.themeBase("custom:x") !== b;
  }), []);

  // ---- selectChevronVar
  const chev = api.selectChevronVar("#aabbcc");
  check("chevron starts as a data uri", chev.startsWith('url("data:image/svg+xml,%3Csvg'), true);
  check("chevron ends the url", chev.endsWith('%3C/svg%3E")'), true);
  check("chevron carries the colour without its hash, escaped", chev.includes("stroke='%23aabbcc'"), true);
  check("chevron has no raw hash", chev.includes("#"), false);

  // ---- applyThemeVars
  sb.settings = { custom_themes: themes };
  props.clear();
  api.applyThemeVars({ bg: "#101010", fg: "#fafafa" });
  check("valid tokens are set inline", [props.get("--bg"), props.get("--fg")], ["#101010", "#fafafa"]);
  check("no accent, no glow", props.has("--shadow"), false);
  check("no muted, no chevron", props.has("--select-chevron"), false);
  api.applyThemeVars({ accent: "#2563eb" });
  check("an earlier override is cleared", props.has("--bg"), false);
  check("the accent derives the window glow", props.get("--shadow"), "0 8px 28px color-mix(in srgb, #2563eb 32%, transparent)");
  api.applyThemeVars({ muted: "#666d7a" });
  check("the muted colour derives the chevron", props.get("--select-chevron"), api.selectChevronVar("#666d7a"));
  check("the glow of the earlier call is cleared", props.has("--shadow"), false);
  api.applyThemeVars({ bg: "red", fg: "#fff", accent: "#12345", border: "#GGGGGG" });
  check("non-hex values are ignored", [...props.keys()], []);
  api.applyThemeVars({ "not-a-token": "#123456", "accent-fg": "#ffffff", "surface-2": "#222222" });
  check("unknown tokens are ignored, known dashed ones set", [...props.keys()].sort(), ["--accent-fg", "--surface-2"]);
  api.applyThemeVars(null);
  check("null clears everything", [...props.keys()], []);
  api.applyThemeVars({ bg: "#101010" });
  api.applyThemeVars(undefined);
  check("undefined clears everything", [...props.keys()], []);
  api.applyThemeVars({ bg: "#ABCDEF" });
  check("upper case hex is accepted as written", props.get("--bg"), "#ABCDEF");

  // ---- applyTheme
  props.clear();
  sb.settings = { custom_themes: themes, theme: "custom:t1" };
  api.applyTheme("ocean");
  check("applyTheme sets the data attribute", root.attrs["data-theme"], "ocean");
  check("applyTheme layers the active custom theme", props.get("--accent"), "#112233");
  check("applyTheme derives the glow of the custom accent", props.has("--shadow"), true);
  check("applyTheme sets the colour scheme (ocean is light)", root.style.colorScheme, "light");
  api.applyTheme("dark", {});
  check("applyTheme with explicit empty vars clears the overrides", [...props.keys()], []);
  check("applyTheme dark sets the dark scheme", root.style.colorScheme, "dark");
  api.applyTheme("cyberpunk", { fg: "#eeeeee" });
  check("applyTheme explicit vars win over the active custom theme", [props.get("--fg"), props.has("--accent")], ["#eeeeee", false]);
  root.attrs = { "data-theme": "keep" };
  api.applyTheme("", {});
  check("applyTheme with no theme leaves the attribute", root.attrs["data-theme"], "keep");
  check("applyTheme with no theme falls to the light scheme", root.style.colorScheme, "light");

  // ---- loadPalette
  const good = api.DEFAULT_COLORS.slice(1).map((c) => c.toUpperCase());
  sb.settings = { ui_texts: { palette: JSON.stringify(good) } };
  api.loadPalette();
  check("a stored palette is used with the empty first slot", palette, ["", ...good]);
  sb.settings = {};
  api.loadPalette();
  check("no palette gives the defaults", palette, api.DEFAULT_COLORS);
  check("the defaults are a copy", palette === api.DEFAULT_COLORS, false);
  sb.settings = { ui_texts: { palette: JSON.stringify(good.slice(1)) } };
  api.loadPalette();
  check("a too short palette gives the defaults", palette, api.DEFAULT_COLORS);
  sb.settings = { ui_texts: { palette: JSON.stringify([...good, "#ffffff"]) } };
  api.loadPalette();
  check("a too long palette gives the defaults", palette, api.DEFAULT_COLORS);
  sb.settings = { ui_texts: { palette: JSON.stringify([...good.slice(1), "red"]) } };
  api.loadPalette();
  check("a palette with a bad colour gives the defaults", palette, api.DEFAULT_COLORS);
  sb.settings = { ui_texts: { palette: "{broken" } };
  api.loadPalette();
  check("a malformed palette gives the defaults", palette, api.DEFAULT_COLORS);
  sb.settings = { ui_texts: { palette: JSON.stringify({ a: 1 }) } };
  api.loadPalette();
  check("a palette that is no array gives the defaults", palette, api.DEFAULT_COLORS);
  sb.settings = { ui_texts: { palette: "" } };
  api.loadPalette();
  check("an empty palette text gives the defaults", palette, api.DEFAULT_COLORS);
  check("the palette length is the editable hue count", api.PALETTE_LEN, 18);

  // ---- the lists agree with each other and with the texts
  check("every theme token has a label key", api.THEME_VARS.filter((k) => !api.THEME_VAR_LABELS[k]), []);
  check("every theme token label is a real English text", Object.values(api.THEME_VAR_LABELS).filter((k) => !EN[k]), []);
  check("every built-in theme has a label key", api.BUILTIN_THEMES.filter((k) => !api.THEME_LABELS[k]), []);
  check("every built-in theme label is a real English text", Object.values(api.THEME_LABELS).filter((k) => !EN[k]), []);
  check("every group title is a real English text", api.THEME_VAR_GROUPS.map((g) => g[0]).filter((k) => !EN[k]), []);
  const grouped = api.THEME_VAR_GROUPS.flatMap((g) => g[1]);
  check("the groups hold every token exactly once", [...grouped].sort(), [...api.THEME_VARS].sort());

  // ---- the stylesheet
  const css = require("./ui_sources.js").mainCss().replace(/\r\n/g, "\n");
  const blockOf = (selector) => {
    const i = css.indexOf("\n" + selector + " {");
    if (i < 0 && !css.startsWith(selector + " {")) return null;
    const from = i < 0 ? 0 : i + 1;
    return css.slice(from, css.indexOf("\n}", from));
  };
  const rootBlock = blockOf(":root");
  check("the base block exists", !!rootBlock, true);
  check("every theme token is defined in the base block", api.THEME_VARS.filter((k) => !new RegExp(`--${k}:`).test(rootBlock)), []);
  const blockFor = (theme) => (theme === "light" ? rootBlock : blockOf(`:root[data-theme="${theme}"]`));
  check("every built-in theme has a rule block", api.BUILTIN_THEMES.filter((b) => !blockFor(b)), []);
  check("every theme block defines every token", api.BUILTIN_THEMES.filter((b) => {
    const blk = blockFor(b);
    return !blk || api.THEME_VARS.some((k) => !new RegExp(`--${k}:`).test(blk));
  }), []);
  const outOfStep = api.BUILTIN_THEMES.filter((b) => {
    const blk = blockFor(b);
    if (!blk) return true;
    const muted = /--muted:\s*(#[0-9a-fA-F]{6})/.exec(blk);
    const chevron = /--select-chevron:\s*(url\("[^"]*"\))/.exec(blk);
    return !muted || !chevron || chevron[1] !== api.selectChevronVar(muted[1]);
  });
  check("each theme's baked chevron equals selectChevronVar of its muted colour", outOfStep, []);
  const colorSchemeOff = api.BUILTIN_THEMES.filter((b) => shared.themeColorScheme(b) !== (["dark", "programmer", "neon", "cyberpunk"].includes(b) ? "dark" : "light"));
  check("the dark themes get the dark colour scheme", colorSchemeOff, []);
}

main().then(() => {
  if (problems.length) {
    console.log(`${problems.length} problem(s):`);
    for (const p of problems) console.log(" - " + p);
    process.exit(1);
  }
  console.log(`theme_check: ${count} assertions OK`);
}).catch((e) => { console.log(e.stack || e); process.exit(1); });
