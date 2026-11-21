// Small pure helpers of ui/floatShared.js that no other check reaches: the copy-text size level,
// the video format list, the paste notice, file URIs, the ffmpeg progress percent, the screen
// reader progress text, the checked clipboard write, colour luminance and ink, the language pick
// and the hold-or-tap toggle. The real module is imported; document/navigator/Date are the only
// stubs.
// Run:  node tests/float_shared_check.js      Exit: 0 = clean, 1 = problems.
const path = require("path");
const { pathToFileURL } = require("url");

const problems = [];
let count = 0;
const check = (label, actual, expected) => {
  count++;
  const a = JSON.stringify(actual), e = JSON.stringify(expected);
  if (a !== e) problems.push(`${label}: got ${a}, expected ${e}`);
};
const round = (x, d = 4) => (x === null ? null : Math.round(x * 10 ** d) / 10 ** d);

async function main() {
  globalThis.document = { addEventListener() {}, removeEventListener() {} };
  const root = path.join(__dirname, "..", "ui");
  const S = await import(pathToFileURL(path.join(root, "floatShared.js")).href);
  const en = (await import(pathToFileURL(path.join(root, "i18n", "en.js")).href)).default;

  // ---- copy text size level
  check("size factors", S.COPY_SIZE_FACTORS, { small: 0.6, medium: 0.8, large: 1 });
  check("a known level is kept (small)", S.normCopySizeLevel("small"), "small");
  check("a known level is kept (medium)", S.normCopySizeLevel("medium"), "medium");
  check("a known level is kept (large)", S.normCopySizeLevel("large"), "large");
  check("an unknown level reads as large", S.normCopySizeLevel("huge"), "large");
  check("undefined reads as large", S.normCopySizeLevel(undefined), "large");
  check("a number reads as large", S.normCopySizeLevel(0.6), "large");
  check("an inherited name reads as large", S.normCopySizeLevel("toString"), "large");
  check("the prototype key reads as large", S.normCopySizeLevel("__proto__"), "large");
  check("a wrong-case level reads as large", S.normCopySizeLevel("Small"), "large");

  const sized = (px, level) => {
    const el = { style: { fontSize: px } };
    S.applyCopySizeLevel(el, level);
    return el.style.fontSize;
  };
  check("small shrinks 20px to 12px", sized("20px", "small"), "12px");
  check("medium shrinks 20px to 16px", sized("20px", "medium"), "16px");
  check("large leaves the size alone", sized("20px", "large"), "20px");
  check("an unknown level leaves the size alone", sized("20px", "huge"), "20px");
  check("small never goes below 10px", sized("15px", "small"), "10px");
  check("medium never goes below 10px", sized("13px", "medium"), "10px");
  check("a size under 10px is never raised", sized("8px", "small"), "8px");
  check("a fractional size is rounded", sized("20.5px", "small"), "12px");
  check("an empty size is left empty", sized("", "small"), "");
  check("a zero size is left alone", sized("0px", "small"), "0px");
  check("a text that is no number is left alone", sized("auto", "small"), "auto");

  // ---- video formats
  check("the format list", S.VIDEO_FORMATS, ["mp4", "webm", "gif", "avi"]);
  check("a stored format is kept", S.normRecFormat("webm"), "webm");
  check("gif is kept", S.normRecFormat("gif"), "gif");
  check("an unknown format reads as mp4", S.normRecFormat("mkv"), "mp4");
  check("a wrong-case format reads as mp4", S.normRecFormat("GIF"), "mp4");
  check("undefined reads as mp4", S.normRecFormat(undefined), "mp4");
  check("an empty value reads as mp4", S.normRecFormat(""), "mp4");
  check("label key of mp4", S.videoFormatLabelKey("mp4"), "optRecFormatMp4");
  check("label key of webm", S.videoFormatLabelKey("webm"), "optRecFormatWebm");
  check("every format has an English text for its label key", S.VIDEO_FORMATS.filter((f) => !en[S.videoFormatLabelKey(f)]), []);
  let filled = null;
  const prevCreate = globalThis.document.createElement;
  globalThis.document.createElement = (tag) => ({ tag, dataset: {} });
  S.fillVideoFormatSelect({ replaceChildren: (...kids) => { filled = kids; } });
  globalThis.document.createElement = prevCreate;
  check("the select is filled from the list, in order", filled, [
    { tag: "option", dataset: { i18n: "optRecFormatMp4" }, value: "mp4", textContent: "MP4" },
    { tag: "option", dataset: { i18n: "optRecFormatWebm" }, value: "webm", textContent: "WebM" },
    { tag: "option", dataset: { i18n: "optRecFormatGif" }, value: "gif", textContent: "GIF" },
    { tag: "option", dataset: { i18n: "optRecFormatAvi" }, value: "avi", textContent: "AVI" },
  ]);

  // ---- paste notice
  const txt = { pasteNoTarget: "No window to paste into", pasteFocusFail: "Could not focus the window" };
  check("no-target notice", S.pasteFailText("no-target", txt), "No window to paste into");
  check("focus-failed notice", S.pasteFailText("focus-failed", txt), "Could not focus the window");
  check("another code has no notice", S.pasteFailText("failed", txt), "");
  check("a missing text gives an empty notice", S.pasteFailText("no-target", {}), "");
  check("an Error object is not a code", S.pasteFailText(new Error("no-target"), txt), "");
  check("undefined has no notice", S.pasteFailText(undefined, txt), "");
  check("a code is matched whole", S.pasteFailText("no-target-found", txt), "");

  // ---- file URI
  check("a drive path", S.fileUri("C:\\Users\\Public\\a\\x.png"), "file:///C:/Users/Public/a/x.png");
  check("a space is encoded", S.fileUri("C:\\Users\\Public\\a b\\x.png"), "file:///C:/Users/Public/a%20b/x.png");
  check("a network path keeps two slashes", S.fileUri("\\\\server\\share\\f.png"), "file://server/share/f.png");
  check("? and # cannot start a query or fragment", S.fileUri("C:\\a#b?c.png"), "file:///C:/a%23b%3Fc.png");
  check("a percent sign is encoded", S.fileUri("C:\\100%.png"), "file:///C:/100%25.png");
  check("a non-ASCII name is encoded", S.fileUri("C:\\M\u00fcll\\x.png"), "file:///C:/M%C3%BCll/x.png");
  check("forward slashes pass through", S.fileUri("C:/a/b.png"), "file:///C:/a/b.png");

  // ---- ffmpeg download percent
  check("a quarter", S.ffmpegDownloadPct({ downloaded: 50, total: 200 }), 25);
  check("a third rounds down", S.ffmpegDownloadPct({ downloaded: 1, total: 3 }), 33);
  check("two thirds round up", S.ffmpegDownloadPct({ downloaded: 2, total: 3 }), 67);
  check("done", S.ffmpegDownloadPct({ downloaded: 200, total: 200 }), 100);
  check("never above 100", S.ffmpegDownloadPct({ downloaded: 300, total: 200 }), 100);
  check("nothing yet", S.ffmpegDownloadPct({ downloaded: 0, total: 200 }), 0);
  check("an unknown total gives 0", S.ffmpegDownloadPct({ downloaded: 50, total: 0 }), 0);
  check("a missing total gives 0", S.ffmpegDownloadPct({ downloaded: 50 }), 0);
  check("no payload gives 0", S.ffmpegDownloadPct(undefined), 0);
  check("a null payload gives 0", S.ffmpegDownloadPct(null), 0);
  check("the extract stage keeps the byte ratio", S.ffmpegDownloadPct({ downloaded: 10, total: 10, stage: "extract" }), 100);

  // ---- screen reader progress text
  const A11Y = {
    a11yPctLeft: "{pct}% done, {rest}",
    a11yPctOnly: "{pct}% done",
    etaAbout: "about {n} min left",
    etaOverHour: "over an hour left",
    dollar: "$& {n}",
  };
  const ta = (k) => A11Y[k] || k;
  check("percent and rest", S.a11yProgressText(ta, 41.6, { key: "etaAbout", value: 5 }), "42% done, about 5 min left");
  check("no estimate gives the percent only", S.a11yProgressText(ta, 41.6, null), "42% done");
  check("an undefined estimate gives the percent only", S.a11yProgressText(ta, 10, undefined), "10% done");
  check("over an hour counts as no known rest", S.a11yProgressText(ta, 7, { key: "etaOverHour", value: 1 }), "7% done");
  check("zero percent", S.a11yProgressText(ta, 0, null), "0% done");
  check("the rest text goes in literally ($& is no pattern)", S.a11yProgressText(ta, 50, { key: "dollar", value: 3 }), "50% done, $& 3");

  // ---- checked clipboard write
  const sent = [];
  const inv = (ret) => (cmd, args) => { sent.push([cmd, args]); return Promise.resolve(ret); };
  await S.copyTextChecked(inv(true), "hello");
  check("the text goes to copy_text", sent[0], ["copy_text", { text: "hello" }]);
  let thrown = null;
  try { await S.copyTextChecked(inv(false), "x"); } catch (e) { thrown = e; }
  check("a false answer throws the clipboard error", thrown, "clipboard write failed");
  thrown = null;
  try { await S.copyTextChecked(inv(undefined), "x"); } catch (e) { thrown = e; }
  check("only an explicit false throws", thrown, null);
  thrown = null;
  try { await S.copyTextChecked(() => Promise.reject("boom"), "x"); } catch (e) { thrown = e; }
  check("a rejected call reaches the caller", thrown, "boom");

  // ---- luminance and ink
  check("white", round(S.hexLuminance("#ffffff")), 1);
  check("black", S.hexLuminance("#000000"), 0);
  check("red", round(S.hexLuminance("#ff0000")), 0.2126);
  check("green", round(S.hexLuminance("#00ff00")), 0.7152);
  check("blue", round(S.hexLuminance("#0000ff")), 0.0722);
  check("mid grey", round(S.hexLuminance("#808080")), 0.2159);
  check("the hash is optional", round(S.hexLuminance("808080")), 0.2159);
  check("upper case is read", round(S.hexLuminance("#FFFFFF")), 1);
  check("three digits are not a colour", S.hexLuminance("#fff"), null);
  check("eight digits are not a colour", S.hexLuminance("#ffffff00"), null);
  check("a name is not a colour", S.hexLuminance("red"), null);
  check("empty is not a colour", S.hexLuminance(""), null);
  check("undefined is not a colour", S.hexLuminance(undefined), null);
  check("a non-hex digit is not a colour", S.hexLuminance("#gggggg"), null);
  check("the crossover constant", S.INK_CROSSOVER, 0.179);
  check("yellow takes black", S.inkFor("#ffff00"), "#000000");
  check("blue takes white", S.inkFor("#0000ff"), "#ffffff");
  check("white takes black", S.inkFor("#ffffff"), "#000000");
  check("black takes white", S.inkFor("#000000"), "#ffffff");
  check("just above the crossover takes black", S.inkFor("#777777"), "#000000");
  check("just below the crossover takes white", S.inkFor("#747474"), "#ffffff");
  check("a custom cut moves the split", S.inkFor("#808080", 0.5), "#ffffff");
  check("a low custom cut moves it the other way", S.inkFor("#0000ff", 0.05), "#000000");
  check("a non-hex value takes white", S.inkFor("not a colour"), "#ffffff");
  check("contrast text on yellow is near black", S.contrastTextColor("#ffff00"), "#111111");
  check("contrast text on navy is white", S.contrastTextColor("#000080"), "#ffffff");
  check("contrast text on a bad value is white", S.contrastTextColor("nope"), "#ffffff");
  check("contrast text is always 6-digit hex", /^#[0-9a-f]{6}$/.test(S.contrastTextColor("#abcdef")), true);

  // ---- language pick
  const LANGS = ["en", "de", "fr", "pt"];
  const realNav = Object.getOwnPropertyDescriptor(globalThis, "navigator");
  const setNav = (language) => Object.defineProperty(globalThis, "navigator", { value: { language }, configurable: true, writable: true });
  setNav("fr-CA");
  check("an explicit setting wins over the browser", S.resolveLang("de", LANGS), "de");
  check("auto follows the browser", S.resolveLang("auto", LANGS), "fr");
  check("an empty setting follows the browser", S.resolveLang("", LANGS), "fr");
  check("no setting follows the browser", S.resolveLang(undefined, LANGS), "fr");
  check("the setting is read in lower case", S.resolveLang("DE", LANGS), "de");
  check("a regional setting matches its language", S.resolveLang("pt-BR", LANGS), "pt");
  check("an unknown setting falls back to en", S.resolveLang("xx", LANGS), "en");
  setNav("ja-JP");
  check("an unknown browser language falls back to en", S.resolveLang("auto", LANGS), "en");
  setNav("");
  check("an empty browser language falls back to en", S.resolveLang("auto", LANGS), "en");
  setNav("DE-at");
  check("the browser language is read in lower case", S.resolveLang("auto", LANGS), "de");
  setNav("fr");
  check("an empty list falls back to en", S.resolveLang("auto", []), "en");
  if (realNav) Object.defineProperty(globalThis, "navigator", realNav); else delete globalThis.navigator;

  // ---- hold-or-tap toggle
  const realNow = Date.now;
  let clock = 1000;
  Date.now = () => clock;
  const make = () => {
    const calls = [];
    const tg = S.makeHoldOrTapToggle((v) => calls.push(v));
    return { tg, calls };
  };
  {
    const { tg, calls } = make();
    check("hidden at the start", tg.isShown(), false);
    tg.press();
    check("a press shows it", [tg.isShown(), calls], [true, [true]]);
    clock += 100;
    tg.release();
    check("a quick release keeps it shown", [tg.isShown(), calls], [true, [true]]);
    clock += 1000;
    tg.press();
    check("the next press changes nothing", [tg.isShown(), calls], [true, [true]]);
    tg.release();
    check("its release hides it", [tg.isShown(), calls], [false, [true, false]]);
  }
  {
    const { tg, calls } = make();
    tg.press();
    clock += 399;
    tg.release();
    check("a release at 399 ms still locks", tg.isShown(), true);
    tg.reset();
    check("reset clears a lock", [tg.isShown(), calls], [false, [true, false]]);
    tg.press();
    check("after a reset a press shows again", tg.isShown(), true);
    tg.release();
    tg.reset();
  }
  {
    const { tg, calls } = make();
    tg.press();
    clock += 400;
    tg.release();
    check("a release at 400 ms hides it", [tg.isShown(), calls], [false, [true, false]]);
  }
  {
    const { tg, calls } = make();
    tg.press();
    clock += 500;
    tg.press();
    clock += 0;
    tg.release();
    check("a repeated press keeps the first press time", [tg.isShown(), calls], [false, [true, false]]);
  }
  {
    const { tg, calls } = make();
    tg.release();
    check("a release with no press does nothing", [tg.isShown(), calls], [false, []]);
    tg.reset();
    check("a reset while hidden does not call back", calls, []);
  }
  {
    const { tg, calls } = make();
    tg.press();
    tg.reset();
    clock += 10;
    tg.release();
    check("a release after a reset does nothing", [tg.isShown(), calls], [false, [true, false]]);
  }
  Date.now = realNow;

  delete globalThis.document;
}

main().then(() => {
  if (problems.length) {
    console.log(`${problems.length} problem(s):`);
    for (const p of problems) console.log(" - " + p);
    process.exit(1);
  }
  console.log(`float_shared_check: ${count} assertions OK`);
}).catch((e) => { console.log(e.stack || e); process.exit(1); });
