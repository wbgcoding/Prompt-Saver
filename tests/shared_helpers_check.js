// Helpers the windows share instead of keeping their own copies (ui/floatShared.js,
// ui/shortcuts.js): the ffmpeg download button of the expert menu and the video editor,
// the hex colour reader of the tray and the screenshot overlay, and the shortcuts overlay
// open/close with its focus hand-back, and the layout-aware letter of a key event.
// Run:  node tests/shared_helpers_check.js      Exit: 0 = clean, 1 = problems.
const path = require("path");
const { pathToFileURL } = require("url");

const uiDir = path.join(__dirname, "..", "ui");
const problems = [];
const expect = (label, got, want) => {
  const a = JSON.stringify(got), b = JSON.stringify(want);
  if (a !== b) problems.push(`${label}: got ${a}, want ${b}`);
};

function fakeButton(text) {
  const attrs = {};
  return {
    textContent: text, title: "Download",
    setAttribute(k, v) { attrs[k] = String(v); },
    removeAttribute(k) { delete attrs[k]; },
    attrs,
  };
}

async function main() {
  globalThis.document = globalThis.document || { addEventListener() {}, removeEventListener() {} };
  const shared = await import(pathToFileURL(path.join(uiDir, "floatShared.js")).href);
  const shortcuts = await import(pathToFileURL(path.join(uiDir, "shortcuts.js")).href);

  // ---- hexToRgb: six hex digits with or without the hash, null for anything else
  expect("hexToRgb #rrggbb", shared.hexToRgb("#102030"), [16, 32, 48]);
  expect("hexToRgb without hash", shared.hexToRgb("FF0080"), [255, 0, 128]);
  expect("hexToRgb trims", shared.hexToRgb(" #0a0b0c "), [10, 11, 12]);
  expect("hexToRgb short form", shared.hexToRgb("#fff"), null);
  expect("hexToRgb junk", shared.hexToRgb("blue"), null);
  expect("hexToRgb empty", shared.hexToRgb(""), null);
  expect("hexToRgb undefined", shared.hexToRgb(undefined), null);

  // ---- runFfmpegDownload against a scripted backend
  const t = (k) => ({ ffmpegCancelHint: "CANCEL", ffmpegDownloading: "Downloading {pct}%" }[k] || k);
  let listener = null;
  let unlistened = 0;
  const calls = [];
  let downloadOutcome = { ok: true };
  let release = null;
  globalThis.window = {
    __TAURI__: {
      core: {
        invoke: async (cmd) => {
          calls.push(cmd);
          if (cmd === "download_ffmpeg") {
            await new Promise((r) => { release = r; });
            if (!downloadOutcome.ok) throw downloadOutcome.err;
          }
          return null;
        },
      },
      event: { listen: async (name, fn) => { listener = fn; return () => { unlistened++; }; } },
    },
  };
  const wait = (ms = 20) => new Promise((r) => setTimeout(r, ms));

  // a finished download: progress text, cancel hint while running, restored afterwards
  let btn = fakeButton("Get ffmpeg");
  let done = 0, failed = [];
  const opts = { t, onDone: () => { done++; }, onError: (e) => failed.push(String(e)) };
  const run1 = shared.runFfmpegDownload(btn, opts);
  await wait();
  expect("running: in-flight flag", shared.ffmpegDownloadActive(), true);
  expect("running: cancel hint", btn.title, "CANCEL");
  expect("running: progressbar role", btn.attrs.role, "progressbar");
  listener({ payload: { downloaded: 50, total: 200 } });
  expect("running: progress text", btn.textContent, "Downloading 25%");
  expect("running: valuenow", btn.attrs["aria-valuenow"], "25");
  // a second click while it runs cancels, it never starts a second download
  await shared.runFfmpegDownload(btn, opts);
  expect("second click cancels", calls.filter((c) => c === "cancel_ffmpeg_download").length, 1);
  expect("second click starts no second download", calls.filter((c) => c === "download_ffmpeg").length, 1);
  release();
  await run1;
  expect("finished: onDone ran once", done, 1);
  expect("finished: no error", failed, []);
  expect("finished: flag released", shared.ffmpegDownloadActive(), false);
  expect("finished: label restored", btn.textContent, "Get ffmpeg");
  expect("finished: title restored", btn.title, "Download");
  expect("finished: aria cleaned", Object.keys(btn.attrs), []);
  expect("finished: listener removed", unlistened, 1);

  // a cancelled download is swallowed
  calls.length = 0; done = 0; failed = [];
  downloadOutcome = { ok: false, err: "cancelled" };
  btn = fakeButton("Get ffmpeg");
  const run2 = shared.runFfmpegDownload(btn, opts);
  await wait();
  release();
  await run2;
  expect("cancelled: no error reported", failed, []);
  expect("cancelled: onDone not run", done, 0);
  expect("cancelled: flag released", shared.ffmpegDownloadActive(), false);

  // any other failure reaches onError once
  downloadOutcome = { ok: false, err: "disk full" };
  const run3 = shared.runFfmpegDownload(btn, opts);
  await wait();
  release();
  await run3;
  expect("failure: reported once", failed, ["disk full"]);
  expect("failure: label restored", btn.textContent, "Get ffmpeg");

  // ---- the shortcuts overlay: rows and the focus hand-back
  const made = [];
  const mk = (tag) => {
    const el = { tag, className: "", textContent: "", children: [], isConnected: true, focused: 0 };
    el.append = (...c) => el.children.push(...c);
    el.appendChild = (c) => { el.children.push(c); return c; };
    el.focus = () => { el.focused++; };
    el.classList = { _s: new Set(["hidden"]), add(c) { this._s.add(c); }, remove(c) { this._s.delete(c); }, contains(c) { return this._s.has(c); } };
    made.push(el);
    return el;
  };
  const body = mk("body");
  const opener = mk("button");
  globalThis.document = { body, activeElement: opener, createElement: mk, addEventListener() {}, removeEventListener() {} };
  const list = mk("div");
  shortcuts.buildShortcutRows(list, [{ desc: "Undo", keys: "Ctrl+Z" }, { desc: "Redo", keys: "Ctrl+Y" }]);
  expect("rows: two built", list.children.length, 2);
  expect("rows: class", list.children[0].className, "shortcuts-row");
  expect("rows: desc and keys", list.children[1].children.map((c) => c.textContent), ["Redo", "Ctrl+Y"]);

  const modal = mk("div");
  shortcuts.openShortcutsOverlay(modal);
  expect("overlay: shown", modal.classList.contains("hidden"), false);
  shortcuts.closeShortcutsOverlay(modal);
  expect("overlay: hidden again", modal.classList.contains("hidden"), true);
  expect("overlay: focus back on the opener", opener.focused, 1);
  expect("overlay: body untouched while the opener is there", body.focused, 0);

  shortcuts.openShortcutsOverlay(modal);
  opener.isConnected = false;
  shortcuts.closeShortcutsOverlay(modal);
  expect("overlay: focus falls back to the body", body.focused, 1);
  expect("overlay: a removed opener is not focused", opener.focused, 1);

  // ---- letterCode: the printed letter wins over the physical key, non-Latin falls back
  expect("letterCode German Z key", shortcuts.letterCode({ key: "z", code: "KeyY" }), "KeyZ");
  expect("letterCode German Y key", shortcuts.letterCode({ key: "y", code: "KeyZ" }), "KeyY");
  expect("letterCode shifted", shortcuts.letterCode({ key: "Z", code: "KeyZ" }), "KeyZ");
  expect("letterCode Cyrillic", shortcuts.letterCode({ key: "я", code: "KeyZ" }), "KeyZ");
  expect("letterCode non-letter", shortcuts.letterCode({ key: "Enter", code: "Enter" }), "Enter");

  if (problems.length) {
    console.error("shared_helpers_check: FAILED\n  " + problems.join("\n  "));
    process.exit(1);
  }
  console.log("shared_helpers_check: ok");
  process.exit(0);
}
main().catch((e) => { console.error(e); process.exit(1); });
