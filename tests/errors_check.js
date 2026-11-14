// Error texts (ui/errors.js): errMessage maps a backend error string to the caller's translated
// text, fills the {n} / {m} slots of a templated message, and shows the generic text (never the
// raw string) for anything unmapped; errMessageLogged also sends an unmapped string to the log.
// The real module is imported; `t` is a small fake lookup that echoes the key or a slot template.
// Run:  node tests/errors_check.js      Exit: 0 = clean, 1 = problems.
const path = require("path");
const { pathToFileURL } = require("url");

const problems = [];
let count = 0;
const check = (label, actual, expected) => {
  count++;
  const a = JSON.stringify(actual), e = JSON.stringify(expected);
  if (a !== e) problems.push(`${label}: got ${a}, expected ${e}`);
};

const SLOTS = {
  errMaxFolders: "At most {n} folders",
  errImageTooLarge: "Image is {n} MB, limit {m} MB",
  errDownloadFailed: "Download failed: {n}",
  errNothingToImport: "No {n} found",
  errFolderNeedsTwo: "A folder needs two {n}",
  errFileTooLargeLimit: "File is over {n} MB",
};
const t = (k) => SLOTS[k] || "K:" + k;

async function main() {
  const mod = await import(pathToFileURL(path.join(__dirname, "..", "ui", "errors.js")).href);
  const { errMessage, errMessageLogged, ERROR_MSG_KEYS } = mod;

  // ---- exact strings
  check("a plain mapped string", errMessage("cannot delete the last view", t), "K:errLastView");
  check("another mapped string", errMessage("file-missing", t), "K:openFileMissing");
  check("two strings can share a key (colour)", errMessage("not a hex color", t), "K:errColorInvalid");
  check("two strings can share a key (colour, other)", errMessage("color must be #rrggbb", t), "K:errColorInvalid");
  check("a string with a slash in it", errMessage("installer too large / truncated", t), "K:errUpdaterTooLarge");
  check("a string with a colon", errMessage("refused: that path belongs to the program files", t), "K:errExportProgramFile");
  check("a string with an apostrophe", errMessage("this folder holds the app's own data", t), "K:errFolderIsAppData");
  check("an exact string wins over a template that also fits", errMessage("unknown view", t), "K:errGone");

  // ---- unmapped text is generic, never raw
  check("an unknown string gives the generic text", errMessage("something nobody mapped", t), "K:errUnexpected");
  check("the raw string is not shown", errMessage("secret internal detail 42", t).includes("secret"), false);
  check("an empty string gives the generic text", errMessage("", t), "K:errUnexpected");
  check("a code a caller branches on gives the generic text", errMessage("canceled", t), "K:errUnexpected");
  check("a match is case-sensitive", errMessage("Cannot delete the last view", t), "K:errUnexpected");
  check("a mapped string with a trailing blank is unmapped", errMessage("cannot delete the last view ", t), "K:errUnexpected");
  check("a mapped string inside a longer one is unmapped", errMessage("error: cannot delete the last view", t), "K:errUnexpected");
  check("an Error object is not a mapped string", errMessage(new Error("cannot delete the last view"), t), "K:errUnexpected");
  check("null gives the generic text", errMessage(null, t), "K:errUnexpected");
  check("undefined gives the generic text", errMessage(undefined, t), "K:errUnexpected");
  check("a number gives the generic text", errMessage(42, t), "K:errUnexpected");
  check("an object gives the generic text", errMessage({ a: 1 }, t), "K:errUnexpected");

  // ---- templates
  check("one slot", errMessage("max 12 folders", t), "At most 12 folders");
  check("one slot, other number", errMessage("max 3 folders", t), "At most 3 folders");
  check("two slots", errMessage("image is too large (12 MB); the limit is 10 MB", t), "Image is 12 MB, limit 10 MB");
  check("the other two-slot sentence", errMessage("file is too large (7 MB); the limit is 5 MB", t), "Image is 7 MB, limit 5 MB");
  check("the one-slot file sentence is its own key", errMessage("file is too large; the limit is 64 MB", t), "File is over 64 MB");
  check("a word fills the slot", errMessage("no prompts found", t), "No prompts found");
  check("a word fills the slot (folder)", errMessage("a folder needs at least two views", t), "A folder needs two views");
  check("the slot text goes in literally ($& is no pattern)", errMessage("download: $& and $1", t), "Download failed: $& and $1");
  check("a path with backslashes fills the slot", errMessage("download: C:\\temp\\x.zip", t), "Download failed: C:\\temp\\x.zip");
  check("a slot can hold a line break", errMessage("download: a\nb", t), "Download failed: a\nb");
  check("a slot can hold a colon and spaces", errMessage("download: error: 404 not found", t), "Download failed: error: 404 not found");
  check("a template is anchored at the start", errMessage("x max 12 folders", t), "K:errUnexpected");
  check("a template is anchored at the end", errMessage("max 12 folders more", t), "K:errUnexpected");
  check("an empty slot does not match", errMessage("max  folders", t), "K:errUnexpected");
  check("a template with no slot in the translated text still works", errMessage("max 5 views", (k) => "plain:" + k), "plain:errMaxViews");
  check("a missing {m} slot in the text leaves the text alone", errMessage("image is too large (12 MB); the limit is 10 MB", () => "Too big {n}"), "Too big 12");
  check("a text with {m} but only one capture keeps {m}", errMessage("max 5 folders", () => "{n} and {m}"), "5 and {m}");
  check("the longest template wins", errMessage("file is too large (7 MB); the limit is 5 MB", t), "Image is 7 MB, limit 5 MB");
  check("a key's literal parentheses do not act as a group", errMessage("image is too large 1 MB; the limit is 2 MB", t), "K:errUnexpected");
  check("the installer template with a code", errMessage("could not start the elevated process (ShellExecuteW code 5)", t), "K:errInstallerStart");

  // ---- the map itself
  const keys = Object.keys(ERROR_MSG_KEYS);
  check("the map is not empty", keys.length > 100, true);
  check("every value is a non-empty key", keys.filter((k) => !ERROR_MSG_KEYS[k] || typeof ERROR_MSG_KEYS[k] !== "string").length, 0);
  // Every exact (slot-free) string maps to its own key, whatever templates exist.
  const wrong = keys.filter((k) => !k.includes("{") && errMessage(k, t) !== (SLOTS[ERROR_MSG_KEYS[k]] || "K:" + ERROR_MSG_KEYS[k]));
  check("every exact string resolves to its own key", wrong, []);
  // Every templated string resolves when its slots are filled with a plain word.
  const badT = keys.filter((k) => k.includes("{")).filter((k) => {
    const sample = k.replace(/\{[^}]*\}/g, "w");
    return errMessage(sample, (key) => "K:" + key) === "K:errUnexpected";
  });
  check("every templated string resolves with a word in its slots", badT, []);

  // ---- errMessageLogged
  const calls = [];
  globalThis.window = { __TAURI__: { core: { invoke: (cmd, args) => { calls.push([cmd, args]); return Promise.resolve(); } } } };
  check("logged: a mapped string returns its text", errMessageLogged("file-missing", t, "video"), "K:openFileMissing");
  check("logged: a mapped string is not logged", calls.length, 0);
  check("logged: an unmapped string returns the generic text", errMessageLogged("odd failure", t, "video"), "K:errUnexpected");
  check("logged: the raw string goes to the log with its origin", calls[0], ["log_error", { text: "K:errUnexpected", origin: "video", raw: "odd failure" }]);
  errMessageLogged("odd failure two", t);
  check("logged: the origin defaults to main", calls[1][1].origin, "main");
  globalThis.window = { __TAURI__: { core: { invoke: () => { throw new Error("no ipc"); } } } };
  check("logged: a throwing invoke does not raise", errMessageLogged("odd", t, "snip"), "K:errUnexpected");
  globalThis.window = { __TAURI__: { core: { invoke: () => Promise.reject(new Error("no ipc")) } } };
  check("logged: a rejecting invoke does not raise", errMessageLogged("odd", t, "snip"), "K:errUnexpected");
  globalThis.window = {};
  check("logged: no Tauri handle does not raise", errMessageLogged("odd", t, "snip"), "K:errUnexpected");
  delete globalThis.window;

  // ---- parseFfmpegError: the "(code N)" part is only there when a code is known
  if (typeof globalThis.document === "undefined") globalThis.document = { addEventListener() {} };
  const { parseFfmpegError } = await import(pathToFileURL(path.join(__dirname, "..", "ui", "floatShared.js")).href);
  const OTHER = {
    en: "The conversion failed (code {code}).",
    de: "Die Konvertierung ist fehlgeschlagen (Code {code}).",
    ja: "変換に失敗しました（コード {code}）。",
    zh: "转换失败（代码 {code}）。",
  };
  const OTHER_EXPECT = {
    en: "The conversion failed.",
    de: "Die Konvertierung ist fehlgeschlagen.",
    ja: "変換に失敗しました。",
    zh: "转换失败。",
  };
  for (const lang of Object.keys(OTHER)) {
    const tl = (k) => (k === "ffErrOther" ? OTHER[lang] : "K:" + k);
    check("ffErrOther without a code drops the parenthetical (" + lang + ")", parseFfmpegError(JSON.stringify({ key: "ffErrOther", raw: "x" }), tl).message, OTHER_EXPECT[lang]);
    check("ffErrOther with a null code drops it too (" + lang + ")", parseFfmpegError(JSON.stringify({ key: "ffErrOther", code: null, raw: "x" }), tl).message, OTHER_EXPECT[lang]);
    check("ffErrOther keeps a known code (" + lang + ")", parseFfmpegError(JSON.stringify({ key: "ffErrOther", code: 7, raw: "x" }), tl).message, OTHER[lang].replace("{code}", "7"));
  }
}

main().then(() => {
  if (problems.length) {
    console.log(`${problems.length} problem(s):`);
    for (const p of problems) console.log(" - " + p);
    process.exit(1);
  }
  console.log(`errors_check: ${count} assertions OK`);
}).catch((e) => { console.log(e.stack || e); process.exit(1); });
