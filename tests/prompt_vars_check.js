// Prompt placeholders (ui/promptVars.js): extractVars, fillVars, hasEmptyVarToken, parseVar,
// the reserved @names (dynName, isDynVar, dynValue, date helpers, the persistent counter),
// varTokens / dialogTokens / anyEntryHasVars, resolveVarsList, the remembered and fixed value
// stores, and the highlighted tooltip text. Runs the real file in a node vm with its core.js and
// theme.js imports stubbed (flags, settings, invoke, toast are fakes), over the real VAR_RE and
// varPreview of ui/floatShared.js.
// Run:  node tests/prompt_vars_check.js      Exit: 0 = clean, 1 = problems.
// Dates are built in local time; the run pins the time zone so the offset text is fixed.
process.env.TZ = "Europe/Berlin";
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

  const src = fs.readFileSync(path.join(uiDir, "promptVars.js"), "utf8")
    .replace(/\r\n/g, "\n")
    .replace(/^import .*$/gm, "")
    .replace(/^export /gm, "");

  const calls = [];
  const toasts = [];
  const sb = {
    FLAGS: { promptVars: true, dynVars: true, choiceVars: true },
    flag: (k) => !!sb.FLAGS[k],
    invoke: (cmd, args) => { calls.push([cmd, args]); return Promise.resolve(sb.INVOKE_RESULT[cmd]); },
    INVOKE_RESULT: {},
    LANG: "en",
    settings: {},
    t: (k) => "T:" + k,
    toast: (m) => toasts.push(m),
    TOOLTIP_PREVIEW_CHARS: 20,
    TXT: { dateStyle: "iso", timeStyle: "iso" },
    txt: (k) => sb.TXT[k] || "",
    activeThemeVars: () => ({}),
    varPreview: shared.varPreview,
    VAR_RE: shared.VAR_RE,
    crypto: globalThis.crypto,
    Intl,
  };
  vm.createContext(sb);
  vm.runInContext(
    src + "\nthis.api = { extractVars, fillVars, hasEmptyVarToken, fillVarHighlighted, DYN_VARS, DYN_SET, dynToken, dynName, DYN_LOCALES, pad2, dayShift, isoWeek, isoStamp, shortId, uuidV4, parseVar, isDynVar, dynLocale, fmtDynDate, fmtDynTime, nextVarCounter, dynValue, varTokens, dialogTokens, anyEntryHasVars, peekRotatedText, resolveVars, resolveVarsList, readRememberedVars, saveRememberedVars, readFixedVars, saveFixedVars };",
    sb
  );
  const api = sb.api;
  const flags = (o) => { sb.FLAGS = Object.assign({ promptVars: true, dynVars: true, choiceVars: true }, o); };

  // ---- extractVars
  check("extractVars empty text", api.extractVars(""), []);
  check("extractVars null", api.extractVars(null), []);
  check("extractVars undefined", api.extractVars(undefined), []);
  check("extractVars one", api.extractVars("Hi {?name}"), ["name"]);
  check("extractVars keeps order and drops repeats", api.extractVars("{?b} {?a} {?b}"), ["b", "a"]);
  check("extractVars trims the name", api.extractVars("{?n  }"), ["n"]);
  check("extractVars keeps a default and options in the token", api.extractVars("{?c=g|r,g,b}"), ["c=g|r,g,b"]);
  check("extractVars reserved name", api.extractVars("{?@date}"), ["@date"]);
  check("extractVars digit start", api.extractVars("{?1st}"), ["1st"]);
  check("extractVars underscore start", api.extractVars("{?_x}"), ["_x"]);
  check("extractVars unicode name", api.extractVars("{?\u540d\u524d} {?\u00fcber}"), ["\u540d\u524d", "\u00fcber"]);
  check("extractVars a space after the question mark is no variable", api.extractVars("{? x}"), []);
  check("extractVars a space before the question mark is no variable", api.extractVars("{ ?x}"), []);
  check("extractVars an empty token is none", api.extractVars("{?}"), []);
  check("extractVars an unclosed token is none", api.extractVars("{?name"), []);
  check("extractVars json is not a variable", api.extractVars('{"a": 1, "b": {"c": 2}}'), []);
  check("extractVars a token never spans a line", api.extractVars("{?a\nb}"), []);
  check("extractVars a name of 200 characters is found", api.extractVars("{?" + "a".repeat(200) + "}"), ["a".repeat(200)]);
  check("extractVars a name of 201 characters is not", api.extractVars("{?" + "a".repeat(201) + "}"), []);
  check("extractVars a double brace is a literal", api.extractVars("{{?x}}"), ["x"]);
  check("extractVars adjacent tokens", api.extractVars("{?a}{?b}"), ["a", "b"]);

  // ---- fillVars
  check("fillVars replaces", api.fillVars("Hi {?name}!", { name: "Bob" }), "Hi Bob!");
  check("fillVars missing value becomes empty", api.fillVars("Hi {?name}!", {}), "Hi !");
  check("fillVars empty value", api.fillVars("[{?a}]", { a: "" }), "[]");
  check("fillVars zero value is kept", api.fillVars("{?n}", { n: 0 }), "0");
  check("fillVars repeats fill every place", api.fillVars("{?a}-{?a}", { a: "x" }), "x-x");
  check("fillVars the value is taken literally (dollar patterns)", api.fillVars("{?a}", { a: "$& $1 $$" }), "$& $1 $$");
  check("fillVars a value that looks like a token is not filled again", api.fillVars("{?a}", { a: "{?a}" }), "{?a}");
  check("fillVars trims the key", api.fillVars("{?a  }", { a: "x" }), "x");
  check("fillVars full token key", api.fillVars("{?c=g|r,g}", { "c=g|r,g": "r" }), "r");
  check("fillVars text without tokens", api.fillVars("plain {x}", {}), "plain {x}");
  check("fillVars multi line value", api.fillVars("{?a}", { a: "l1\nl2" }), "l1\nl2");

  // ---- hasEmptyVarToken
  check("empty token {?}", api.hasEmptyVarToken("{?}"), true);
  check("empty token with a space", api.hasEmptyVarToken("{? x}"), true);
  check("empty token only blanks", api.hasEmptyVarToken("{? }"), true);
  check("empty token inside text", api.hasEmptyVarToken("a {?a} b {?} c"), true);
  check("a named token is fine", api.hasEmptyVarToken("{?x}"), false);
  check("a reserved token is fine", api.hasEmptyVarToken("{?@date}"), false);
  check("an underscore name is fine", api.hasEmptyVarToken("{?_}"), false);
  check("a digit name is fine", api.hasEmptyVarToken("{?1}"), false);
  check("a letter outside ascii is fine", api.hasEmptyVarToken("{?\u00e9}"), false);
  check("plain text", api.hasEmptyVarToken("nothing"), false);
  check("null", api.hasEmptyVarToken(null), false);
  check("empty text", api.hasEmptyVarToken(""), false);
  check("a token broken by a line break is not flagged", api.hasEmptyVarToken("{?\n}"), false);
  check("json braces are not flagged", api.hasEmptyVarToken('{"a": 1}'), false);

  // ---- parseVar
  flags({});
  check("parseVar name", api.parseVar("name"), { name: "name", options: [], default: "" });
  check("parseVar trims", api.parseVar("  name  "), { name: "name", options: [], default: "" });
  check("parseVar default", api.parseVar("name=Bob"), { name: "name", options: [], default: "Bob" });
  check("parseVar default keeps inner blanks", api.parseVar("n = a b "), { name: "n", options: [], default: "a b" });
  check("parseVar empty default", api.parseVar("name="), { name: "name", options: [], default: "" });
  check("parseVar options", api.parseVar("c|red,green"), { name: "c", options: ["red", "green"], default: "" });
  check("parseVar options and default", api.parseVar("c=green|red,green"), { name: "c", options: ["red", "green"], default: "green" });
  check("parseVar drops empty and blank options", api.parseVar("c|a, ,,b"), { name: "c", options: ["a", "b"], default: "" });
  check("parseVar options with unicode", api.parseVar("f|\u00e4,\u00f6"), { name: "f", options: ["\u00e4", "\u00f6"], default: "" });
  check("parseVar a bar without options keeps the whole token as the name", api.parseVar("c|"), { name: "c|", options: [], default: "" });
  check("parseVar only blank options", api.parseVar("c| , "), { name: "c| ,", options: [], default: "" });
  check("parseVar a bar without a name", api.parseVar("|a,b"), { name: "|a,b", options: [], default: "" });
  check("parseVar an equals sign without a name", api.parseVar("=x"), { name: "=x", options: [], default: "" });
  check("parseVar equals and bar without a name", api.parseVar("=x|a,b"), { name: "=x|a,b", options: [], default: "" });
  check("parseVar a later equals sign stays in the default", api.parseVar("a=b=c"), { name: "a", options: [], default: "b=c" });
  check("parseVar the first bar splits", api.parseVar("a|b|c"), { name: "a", options: ["b|c"], default: "" });
  check("parseVar reserved name", api.parseVar("@date"), { name: "@date", options: [], default: "" });
  check("parseVar non-string input", api.parseVar(5), { name: "5", options: [], default: "" });
  flags({ choiceVars: false });
  check("parseVar without choice lists: only the name before the bar", api.parseVar("c|red,green"), { name: "c", options: [], default: "" });
  check("parseVar without choice lists keeps a default", api.parseVar("c=g|red,g"), { name: "c", options: [], default: "g" });
  check("parseVar without choice lists and without a name", api.parseVar("|a"), { name: "|a", options: [], default: "" });
  flags({});

  // ---- reserved names
  check("DYN_VARS count", api.DYN_VARS.length, 16);
  check("DYN_VARS list", api.DYN_VARS.join(","), "date,time,datetime,weekday,yesterday,tomorrow,week,month,year,timestamp,epoch,clipboard,username,uuid,random,counter");
  check("DYN_SET agrees with the list", api.DYN_SET.size, 16);
  check("dynToken", api.dynToken("date"), "@date");
  check("dynName strips the at sign", api.dynName("@date"), "date");
  check("dynName of a plain word", api.dynName("date"), "");
  check("dynName of a lone at sign", api.dynName("@"), "");
  check("dynName of an empty text", api.dynName(""), "");
  check("dynName keeps a later at sign", api.dynName("@a@b"), "a@b");
  check("isDynVar reserved", api.isDynVar("@date"), true);
  check("isDynVar the plain word is a user variable", api.isDynVar("date"), false);
  check("isDynVar unknown reserved name", api.isDynVar("@bogus"), false);
  check("isDynVar is case-sensitive", api.isDynVar("@Date"), false);
  flags({ dynVars: false });
  check("isDynVar is off with the switch off", api.isDynVar("@date"), false);
  flags({});

  // ---- date helpers
  check("pad2 one digit", api.pad2(5), "05");
  check("pad2 two digits", api.pad2(12), "12");
  check("pad2 zero", api.pad2(0), "00");
  check("pad2 three digits stay", api.pad2(123), "123");
  const day = (y, m, d, h = 12) => new Date(y, m - 1, d, h, 0, 0);
  const ymd = (d) => `${d.getFullYear()}-${api.pad2(d.getMonth() + 1)}-${api.pad2(d.getDate())} ${d.getHours()}`;
  check("dayShift plus one", ymd(api.dayShift(day(2026, 7, 27), 1)), "2026-07-28 12");
  check("dayShift minus one", ymd(api.dayShift(day(2026, 7, 27), -1)), "2026-07-26 12");
  check("dayShift over a month end", ymd(api.dayShift(day(2026, 3, 31), 1)), "2026-04-01 12");
  check("dayShift over a year start", ymd(api.dayShift(day(2026, 1, 1), -1)), "2025-12-31 12");
  check("dayShift into a leap day", ymd(api.dayShift(day(2024, 2, 28), 1)), "2024-02-29 12");
  check("dayShift over the leap day", ymd(api.dayShift(day(2024, 3, 1), -1)), "2024-02-29 12");
  check("dayShift across the spring clock change keeps the hour", ymd(api.dayShift(day(2026, 3, 28), 1)), "2026-03-29 12");
  check("dayShift across the autumn clock change keeps the hour", ymd(api.dayShift(day(2026, 10, 24), 1)), "2026-10-25 12");
  check("dayShift zero", ymd(api.dayShift(day(2026, 7, 27), 0)), "2026-07-27 12");
  const original = day(2026, 7, 27);
  api.dayShift(original, 5);
  check("dayShift leaves its argument alone", ymd(original), "2026-07-27 12");

  check("isoWeek mid year", api.isoWeek(day(2026, 7, 27)), 31);
  check("isoWeek first day of 2026 is week 1", api.isoWeek(day(2026, 1, 1)), 1);
  check("isoWeek 2024-12-30 already belongs to 2025 week 1", api.isoWeek(day(2024, 12, 30)), 1);
  check("isoWeek 2021-01-03 still belongs to week 53", api.isoWeek(day(2021, 1, 3)), 53);
  check("isoWeek 2020-12-31 is week 53", api.isoWeek(day(2020, 12, 31)), 53);
  check("isoWeek 2022-01-01 is week 52", api.isoWeek(day(2022, 1, 1)), 52);
  check("isoWeek 2023-01-01 is week 52", api.isoWeek(day(2023, 1, 1)), 52);
  check("isoWeek 2026-12-31 is week 53", api.isoWeek(day(2026, 12, 31)), 53);
  check("isoWeek 2027-01-01 is week 53", api.isoWeek(day(2027, 1, 1)), 53);
  check("isoWeek a Sunday belongs to the week before", api.isoWeek(day(2026, 7, 26)), 30);
  check("isoWeek a Monday opens the week", api.isoWeek(day(2026, 7, 27, 0)), 31);
  check("isoWeek late on a Sunday", api.isoWeek(day(2026, 7, 26, 23)), 30);

  check("isoStamp summer", api.isoStamp(new Date(2026, 6, 27, 14, 3, 22)), "2026-07-27T14:03:22+02:00");
  check("isoStamp winter", api.isoStamp(new Date(2026, 0, 5, 9, 8, 7)), "2026-01-05T09:08:07+01:00");
  check("isoStamp pads small numbers", api.isoStamp(new Date(2026, 0, 1, 0, 0, 0)), "2026-01-01T00:00:00+01:00");
  check("isoStamp end of year", api.isoStamp(new Date(2026, 11, 31, 23, 59, 59)), "2026-12-31T23:59:59+01:00");

  check("uuidV4 shape", /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(api.uuidV4()), true);
  check("uuidV4 differs between calls", api.uuidV4() === api.uuidV4(), false);
  let allValid = true;
  for (let i = 0; i < 200; i++) if (!/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(api.uuidV4())) allValid = false;
  check("uuidV4 shape over many draws", allValid, true);
  check("shortId is eight characters of base 36", /^[0-9a-z]{8}$/.test(api.shortId()), true);
  check("shortId differs between calls", api.shortId() === api.shortId(), false);

  // ---- locale
  sb.LANG = "en";
  check("dynLocale plain code", api.dynLocale(), "en");
  sb.LANG = "pt";
  check("dynLocale Portuguese is the European one", api.dynLocale(), "pt-PT");
  sb.LANG = "de";
  check("dynLocale German", api.dynLocale(), "de");
  sb.LANG = "en";

  // ---- dynValue
  const now = new Date(2026, 6, 27, 14, 3, 22);
  const ctx = { clip: "CLIP", user: "alice", counter: 7 };
  sb.TXT = { dateStyle: "iso", timeStyle: "iso" };
  check("dynValue date iso", api.dynValue("date", now, ctx), "2026-07-27");
  check("dynValue time iso", api.dynValue("time", now, ctx), "14:03:22");
  check("dynValue datetime iso", api.dynValue("datetime", now, ctx), "2026-07-27 14:03:22");
  check("dynValue weekday", api.dynValue("weekday", now, ctx), "Monday");
  check("dynValue yesterday", api.dynValue("yesterday", now, ctx), "2026-07-26");
  check("dynValue tomorrow", api.dynValue("tomorrow", now, ctx), "2026-07-28");
  check("dynValue week", api.dynValue("week", now, ctx), "31");
  check("dynValue month", api.dynValue("month", now, ctx), "July");
  check("dynValue year", api.dynValue("year", now, ctx), "2026");
  check("dynValue timestamp", api.dynValue("timestamp", now, ctx), "2026-07-27T14:03:22+02:00");
  check("dynValue epoch", api.dynValue("epoch", new Date(Date.UTC(2026, 0, 1)), ctx), "1767225600");
  check("dynValue epoch rounds down", api.dynValue("epoch", new Date(Date.UTC(2026, 0, 1, 0, 0, 0, 999)), ctx), "1767225600");
  check("dynValue clipboard", api.dynValue("clipboard", now, ctx), "CLIP");
  check("dynValue username", api.dynValue("username", now, ctx), "alice");
  check("dynValue counter", api.dynValue("counter", now, ctx), "7");
  check("dynValue uuid", /^[0-9a-f-]{36}$/.test(api.dynValue("uuid", now, ctx)), true);
  check("dynValue random", /^[0-9a-z]{8}$/.test(api.dynValue("random", now, ctx)), true);
  check("dynValue unknown name is empty", api.dynValue("bogus", now, ctx), "");
  check("dynValue yesterday over a month start", api.dynValue("yesterday", new Date(2026, 2, 1, 10), ctx), "2026-02-28");
  check("dynValue tomorrow over a year end", api.dynValue("tomorrow", new Date(2026, 11, 31, 10), ctx), "2027-01-01");
  check("dynValue tomorrow into a leap day", api.dynValue("tomorrow", new Date(2024, 1, 28, 10), ctx), "2024-02-29");
  sb.TXT = {};
  check("dynValue date without a stored style is the short style", api.dynValue("date", now, ctx), "7/27/26");
  // Newer ICU data puts a narrow no-break space before PM; older data a plain one.
  check("dynValue time without a stored style is the short style", api.dynValue("time", now, ctx).replace(/\u202f/g, " "), "2:03 PM");
  sb.TXT = { dateStyle: "long", timeStyle: "iso" };
  check("dynValue date long", api.dynValue("date", now, ctx), "July 27, 2026");
  sb.LANG = "de";
  check("dynValue date long German", api.dynValue("date", now, ctx), "27. Juli 2026");
  check("dynValue weekday German", api.dynValue("weekday", now, ctx), "Montag");
  check("dynValue month German", api.dynValue("month", now, ctx), "Juli");
  sb.LANG = "pt";
  check("dynValue weekday Portuguese", api.dynValue("weekday", now, ctx), "segunda-feira");
  sb.LANG = "xx-invalid-@@";
  check("dynValue falls back to English for a language Intl rejects", api.dynValue("weekday", now, ctx), "Monday");
  sb.LANG = "en";
  sb.TXT = { dateStyle: "iso", timeStyle: "iso" };
  check("fmtDynDate iso pads", api.fmtDynDate(new Date(2026, 0, 5), "iso"), "2026-01-05");
  check("fmtDynTime iso pads", api.fmtDynTime(new Date(2026, 0, 5, 1, 2, 3), "iso"), "01:02:03");

  // ---- the persistent counter
  const counterOf = (stored) => {
    calls.length = 0;
    sb.settings = stored === undefined ? {} : { ui_texts: { varCounter: stored, other: "keep" } };
    const n = api.nextVarCounter();
    return { n, saved: sb.settings.ui_texts.varCounter, other: sb.settings.ui_texts.other, call: calls[0] };
  };
  check("counter starts at 1", counterOf(undefined).n, 1);
  check("counter is stored as text", counterOf(undefined).saved, "1");
  check("counter persists through the backend", counterOf("4").call, ["set_ui_text", { key: "varCounter", value: "5" }]);
  check("counter increments", counterOf("5").n, 6);
  check("counter keeps the other texts", counterOf("5").other, "keep");
  check("counter at the maximum wraps to 1", counterOf("1000000000").n, 1);
  check("counter just below the maximum counts on", counterOf("999999999").n, 1000000000);
  check("counter negative restarts", counterOf("-3").n, 1);
  check("counter garbage restarts", counterOf("abc").n, 1);
  check("counter empty text restarts", counterOf("").n, 1);
  check("counter fraction is floored", counterOf("3.7").n, 4);
  check("counter above the maximum restarts", counterOf("5000000000").n, 1);

  // ---- tokens a copy has to look at
  const prompt = (o) => Object.assign({ text: "{?a} {?@date}" }, o);
  check("varTokens", api.varTokens(prompt(), undefined), ["a", "@date"]);
  check("varTokens with an override text", api.varTokens(prompt(), "{?z}"), ["z"]);
  check("varTokens override with an empty text is empty, not the prompt text", api.varTokens(prompt(), ""), []);
  check("varTokens of an image prompt", api.varTokens(prompt({ copy_image: true }), undefined), []);
  check("varTokens of a file prompt", api.varTokens(prompt({ file_path: "C:\\a.txt" }), undefined), []);
  check("varTokens of no prompt", api.varTokens(null, undefined), []);
  flags({ promptVars: false });
  check("varTokens with the switch off", api.varTokens(prompt(), undefined), []);
  flags({});
  check("dialogTokens leave out the reserved names", api.dialogTokens(prompt(), undefined), ["a"]);
  check("dialogTokens a reserved name with options still counts as reserved", api.dialogTokens(prompt({ text: "{?@date|a,b} {?x}" }), undefined), ["x"]);
  flags({ dynVars: false });
  check("dialogTokens ask for a reserved name when reserved names are off", api.dialogTokens(prompt(), undefined), ["a", "@date"]);
  flags({});
  check("anyEntryHasVars in the text", api.anyEntryHasVars({ text: "{?a}" }), true);
  check("anyEntryHasVars in a later entry", api.anyEntryHasVars({ text: "plain", variants: ["x", "{?a}"] }), true);
  check("anyEntryHasVars none", api.anyEntryHasVars({ text: "plain", variants: ["x"] }), false);
  check("anyEntryHasVars without variants", api.anyEntryHasVars({ text: "plain" }), false);
  check("anyEntryHasVars an image prompt carries none", api.anyEntryHasVars({ text: "{?a}", copy_image: true }), false);

  // ---- peekRotatedText
  calls.length = 0;
  check("peek a plain prompt", await api.peekRotatedText({ id: 1, text: "base" }), "base");
  check("peek a plain prompt asks nobody", calls.length, 0);
  sb.INVOKE_RESULT = { rotation_peek: { text: "second" } };
  check("peek a rotating prompt", await api.peekRotatedText({ id: 1, text: "base", variants: ["x"] }), "second");
  sb.INVOKE_RESULT = { rotation_peek: null };
  check("peek without an answer falls back to the text", await api.peekRotatedText({ id: 1, text: "base", variants: ["x"] }), "base");
  sb.INVOKE_RESULT = {};

  // ---- resolveVarsList / resolveVars
  sb.settings = {};
  sb.INVOKE_RESULT = {};
  calls.length = 0;
  check("resolve without tokens hands the texts back", await api.resolveVarsList(["a", "b"]), ["a", "b"]);
  const same = ["x"];
  check("resolve without tokens returns a copy", (await api.resolveVarsList(same)) === same, false);
  check("resolve of nothing", await api.resolveVarsList([]), []);
  check("resolve reserved names fill themselves in", /^\d{4}-\d{10}$/.test((await api.resolveVarsList(["{?@year}-{?@epoch}"]))[0]), true);
  let asked = null;
  vm.runInContext("promptVarsWindow = async (tokens) => { askedTokens.push(tokens); return askedAnswer.v; };", Object.assign(sb, { askedTokens: [], askedAnswer: { v: { name: "Bob" } } }));
  check("resolve asks for the user variables and fills them", await api.resolveVarsList(["Hi {?name}"]), ["Hi Bob"]);
  check("resolve asked exactly the user tokens", sb.askedTokens, [["name"]]);
  sb.askedTokens.length = 0;
  check("resolve mixes asked and reserved values", await api.resolveVarsList(["{?name} {?@year}"]), [`Bob ${new Date().getFullYear()}`]);
  sb.askedTokens.length = 0;
  check("resolve asks once for the union of several texts", await api.resolveVarsList(["{?name}", "x {?name}"]), ["Bob", "x Bob"]);
  check("resolve union asked a single round", sb.askedTokens.length, 1);
  sb.askedAnswer.v = null;
  check("resolve returns null when the user cancels", await api.resolveVarsList(["{?name}"]), null);
  check("resolveVars returns null on cancel", await api.resolveVars("{?name}"), null);
  toasts.length = 0;
  sb.askedAnswer.v = false;
  check("resolve returns null when the window failed", await api.resolveVarsList(["{?name}"]), null);
  check("resolve says so when the window failed", toasts, ["T:varsWindowFailed"]);
  sb.askedAnswer.v = { name: "Bob" };
  check("resolveVars returns the single text", await api.resolveVars("Hi {?name}"), "Hi Bob");
  check("resolveVars without tokens", await api.resolveVars("none"), "none");

  // The counter only advances when the copy goes ahead.
  sb.settings = { ui_texts: { varCounter: "9" } };
  sb.askedAnswer.v = null;
  check("a cancelled dialog does not burn a counter value", await api.resolveVarsList(["{?name} {?@counter}"]), null);
  check("the counter stayed", sb.settings.ui_texts.varCounter, "9");
  sb.askedAnswer.v = { name: "Bob" };
  check("a finished copy takes the next counter value", await api.resolveVarsList(["{?name} {?@counter}"]), ["Bob 10"]);
  check("the counter moved once", sb.settings.ui_texts.varCounter, "10");
  check("one text with the counter twice gets one value", await api.resolveVarsList(["{?@counter} {?@counter}"]), ["11 11"]);
  check("two texts share one counter value", await api.resolveVarsList(["{?@counter}", "n{?@counter}"]), ["12", "n12"]);

  // Clipboard and user name come from the backend.
  sb.INVOKE_RESULT = { clipboard_text: "from clip", os_user_name: "carol" };
  check("the clipboard name resolves", await api.resolveVarsList(["[{?@clipboard}]"]), ["[from clip]"]);
  check("the user name resolves", await api.resolveVarsList(["{?@username}"]), ["carol"]);
  sb.INVOKE_RESULT = { clipboard_text: "x".repeat(100005) };
  check("a huge clipboard is cut at 100000 characters", (await api.resolveVarsList(["{?@clipboard}"]))[0].length, 100000);
  sb.INVOKE_RESULT = { clipboard_text: "" };
  toasts.length = 0;
  check("an empty clipboard fills nothing", await api.resolveVarsList(["[{?@clipboard}]"]), ["[]"]);
  check("an empty clipboard is said so", toasts, ["T:clipVarEmpty"]);
  sb.INVOKE_RESULT = { clipboard_text: null };
  check("a null clipboard fills nothing", await api.resolveVarsList(["[{?@clipboard}]"]), ["[]"]);
  sb.INVOKE_RESULT = {};
  flags({ dynVars: false });
  sb.askedTokens.length = 0;
  check("with reserved names off the token is asked like any other", await api.resolveVarsList(["{?@date}"]), [""]);
  check("with reserved names off the asked token list has it", sb.askedTokens, [["@date"]]);
  sb.askedAnswer.v = { "@date": "today" };
  check("with reserved names off the answer fills it", await api.resolveVarsList(["{?@date}"]), ["today"]);
  flags({});

  // ---- remembered and fixed value stores
  sb.settings = {};
  check("remembered vars default", api.readRememberedVars(), {});
  sb.settings = { ui_texts: { varDefaults: '{"a":"1"}' } };
  check("remembered vars read", api.readRememberedVars(), { a: "1" });
  sb.settings = { ui_texts: { varDefaults: "{broken" } };
  check("remembered vars garbage reads as empty", api.readRememberedVars(), {});
  sb.settings = { ui_texts: { varFixed: '{"b":"2"}' } };
  check("fixed vars read", api.readFixedVars(), { b: "2" });
  sb.settings = { ui_texts: { varFixed: "nope" } };
  check("fixed vars garbage reads as empty", api.readFixedVars(), {});
  sb.settings = {};
  calls.length = 0;
  api.saveRememberedVars({ a: "1" }, {});
  check("saveRememberedVars stores json", sb.settings.ui_texts.varDefaults, '{"a":"1"}');
  check("saveRememberedVars tells the backend", calls[0], ["set_ui_text", { key: "varDefaults", value: '{"a":"1"}' }]);
  calls.length = 0;
  api.saveRememberedVars({ a: "1" }, { a: "1" });
  check("saveRememberedVars with no change writes nothing", calls.length, 0);
  sb.settings = { ui_texts: { other: "keep" } };
  api.saveRememberedVars({ z: "9" }, {});
  check("saveRememberedVars keeps other texts", sb.settings.ui_texts.other, "keep");
  calls.length = 0;
  sb.settings = {};
  api.saveFixedVars({ b: "2" });
  check("saveFixedVars stores json", sb.settings.ui_texts.varFixed, '{"b":"2"}');
  check("saveFixedVars tells the backend", calls[0], ["set_ui_text", { key: "varFixed", value: '{"b":"2"}' }]);
  calls.length = 0;
  api.saveFixedVars({ b: "2" });
  check("saveFixedVars unchanged writes nothing", calls.length, 0);
  calls.length = 0;
  sb.settings = {};
  api.saveFixedVars({});
  check("saveFixedVars empty on empty writes nothing", calls.length, 0);

  // ---- fillVarHighlighted: tokens as accent spans, never markup
  const makeEl = () => {
    const el = { kids: [], textContent: "x", appendChild(k) { el.kids.push(k); } };
    Object.defineProperty(el, "textContent", { set(v) { if (v === "") el.kids = []; }, get() { return ""; } });
    return el;
  };
  sb.document = {
    createTextNode: (s) => ({ text: s }),
    createElement: () => ({ className: "", textContent: "" }),
  };
  const render = (text) => {
    const el = makeEl();
    api.fillVarHighlighted(el, text);
    return el.kids.map((k) => (k.className ? `<${k.className}>${k.textContent}` : k.text)).join("|");
  };
  check("highlight plain text", render("plain"), "plain");
  check("highlight a token", render("a {?name} b"), "a |<var-token>[name]| b");
  check("highlight a token with a default", render("{?c=g|r,g}"), "<var-token>[c]");
  check("highlight a markup-looking text stays text", render("<b>x</b>"), "<b>x</b>");
  check("highlight a long text is cut with an ellipsis", render("a".repeat(30)), "a".repeat(20) + "|\u2026");
  check("highlight stops at the limit inside text", render("12345678901234567890X{?a}"), "12345678901234567890|\u2026");
  check("highlight exact limit has no ellipsis", render("a".repeat(20)), "a".repeat(20));
  check("highlight empty text", render(""), "");
}

main().then(() => {
  if (problems.length) {
    console.log(`${problems.length} problem(s):`);
    for (const p of problems) console.log(" - " + p);
    process.exit(1);
  }
  console.log(`prompt_vars_check: ${count} assertions OK`);
}).catch((e) => { console.log(e.stack || e); process.exit(1); });
