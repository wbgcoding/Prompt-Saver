// Hotkey helpers (ui/hotkeys.js): the accelerator built from a key event (modifier rules, the
// plus key, arrows, space), the readable label, the comparison key that treats the backend's
// aliases as one, who owns a combination, the error-code mapper, and the commit paths that write
// a binding. The real source is run in a node vm with the imports stubbed.
// Run:  node tests/hotkeys_check.js      Exit: 0 = clean, 1 = problems.
const fs = require("fs");
const path = require("path");
const vm = require("vm");

const problems = [];
let count = 0;
const check = (label, actual, expected) => {
  count++;
  const a = JSON.stringify(actual), e = JSON.stringify(expected);
  if (a !== e) problems.push(`${label}: got ${a}, expected ${e}`);
};

const src = fs.readFileSync(path.join(__dirname, "..", "ui", "hotkeys.js"), "utf8")
  .replace(/^import .*$/gm, "")
  .replace(/^export /gm, "");

const EN = {
  keyCtrl: "Ctrl", keyShift: "Shift", keyAlt: "Alt", keySpace: "Space", keyUp: "Up", keyDown: "Down",
  keyLeft: "Left", keyRight: "Right", keyHome: "Home", keyEnd: "End",
  promptHotkeyInvalid: "Invalid", promptHotkeyTaken: "Taken", promptHotkeySystem: "System", promptHotkeyConflict: "Conflict",
  persistFailed: "Save failed", hotkeyFail: "Hotkey failed", hotkeySet: "Hotkey set", promptHotkeyFail: "Refused",
  promptHotkeyTakenApp: "Used by the app", promptHotkeyTakenVoice: "Used by voice", promptHotkeyTakenRecStop: "Used by stop",
  promptHotkeyTakenRecPause: "Used by pause", promptHotkeyTakenBy: "Used by {name}",
};
const calls = [];
const toasts = [];
const ctx = {
  console,
  document: { activeElement: null, createElement: () => ({}) },
  $: (id) => (id === "hotkeys-list" ? {} : { classList: { contains: () => true, toggle() {} } }),
  errToast() {}, EXPERT_ACTIONS: {}, flag: () => false, flagState: () => false,
  invoke: (cmd, args) => { calls.push([cmd, args]); return Promise.resolve(ctx.__ret ? ctx.__ret(cmd, args) : undefined); },
  modal: {}, modalState: null, prompts: [], settings: {},
  t: (k) => EN[k] || "K:" + k,
  toast: (m) => toasts.push(m),
  hotkeyClearX() {}, openAllShortcuts() {}, showExpertPage() {},
};
vm.createContext(ctx);
vm.runInContext(src + "\nthis.api = { accelFromEvent, prettyAccel, hotkeyMessage, accelKey, hotkeyOwner, hotkeyTakenMsg, setPromptHotkey, commitGlobalHotkey, commitVoiceHotkey, commitRecStopHotkey, commitRecPauseHotkey, commitPromptHotkeyDirect, hotkeyCapture, reassertHotkeyCapture, HOTKEYS_LIST_KINDS, HOTKEY_ERRORS, getCapture: () => hotkeyCaptureOn };", ctx);
const H = ctx.api;

const ev = (o) => Object.assign({ key: "", code: "", ctrlKey: false, altKey: false, shiftKey: false, metaKey: false }, o);

async function main() {
  // ---- accelerator from a key event
  check("ctrl + letter is upper-cased", H.accelFromEvent(ev({ ctrlKey: true, key: "p" })), "CommandOrControl+P");
  check("a capital letter stays one", H.accelFromEvent(ev({ ctrlKey: true, key: "P" })), "CommandOrControl+P");
  check("modifier order is ctrl, alt, shift, super", H.accelFromEvent(ev({ metaKey: true, shiftKey: true, altKey: true, ctrlKey: true, key: "k" })), "CommandOrControl+Alt+Shift+Super+K");
  check("alt alone is a modifier", H.accelFromEvent(ev({ altKey: true, key: "x" })), "Alt+X");
  check("shift alone is a modifier", H.accelFromEvent(ev({ shiftKey: true, key: "x" })), "Shift+X");
  check("meta becomes Super", H.accelFromEvent(ev({ metaKey: true, key: "x" })), "Super+X");
  check("no modifier gives null", H.accelFromEvent(ev({ key: "p" })), null);
  check("a bare function key gives null", H.accelFromEvent(ev({ key: "F5" })), null);
  check("Control itself is not a combo", H.accelFromEvent(ev({ ctrlKey: true, key: "Control" })), null);
  check("Alt itself is not a combo", H.accelFromEvent(ev({ altKey: true, key: "Alt" })), null);
  check("Shift itself is not a combo", H.accelFromEvent(ev({ shiftKey: true, key: "Shift" })), null);
  check("Meta itself is not a combo", H.accelFromEvent(ev({ metaKey: true, key: "Meta" })), null);
  check("space is named", H.accelFromEvent(ev({ ctrlKey: true, key: " " })), "CommandOrControl+Space");
  check("ArrowUp loses its prefix", H.accelFromEvent(ev({ ctrlKey: true, key: "ArrowUp" })), "CommandOrControl+Up");
  check("ArrowLeft loses its prefix", H.accelFromEvent(ev({ altKey: true, key: "ArrowLeft" })), "Alt+Left");
  check("a function key keeps its name", H.accelFromEvent(ev({ ctrlKey: true, key: "F12" })), "CommandOrControl+F12");
  check("a named key keeps its name", H.accelFromEvent(ev({ ctrlKey: true, key: "Home" })), "CommandOrControl+Home");
  check("a digit stays a digit", H.accelFromEvent(ev({ ctrlKey: true, key: "7" })), "CommandOrControl+7");
  check("the typed plus becomes Equal", H.accelFromEvent(ev({ ctrlKey: true, shiftKey: true, key: "+", code: "Equal" })), "CommandOrControl+Shift+Equal");
  check("the numpad plus becomes NumpadAdd", H.accelFromEvent(ev({ ctrlKey: true, key: "+", code: "NumpadAdd" })), "CommandOrControl+NumpadAdd");
  check("a plus with an unknown code becomes Equal", H.accelFromEvent(ev({ ctrlKey: true, key: "+", code: "" })), "CommandOrControl+Equal");
  check("the result never holds a bare plus key", (H.accelFromEvent(ev({ ctrlKey: true, key: "+", code: "Equal" })) || "").split("+").includes(""), false);
  check("a punctuation key is kept as typed", H.accelFromEvent(ev({ ctrlKey: true, key: "," })), "CommandOrControl+,");

  // ---- readable label
  check("empty gives an empty label", H.prettyAccel(""), "");
  check("null gives an empty label", H.prettyAccel(null), "");
  check("ctrl, shift, letter", H.prettyAccel("CommandOrControl+Shift+P"), "Ctrl + Shift + P");
  check("Control reads as Ctrl", H.prettyAccel("Control+P"), "Ctrl + P");
  check("Option reads as Alt", H.prettyAccel("Option+P"), "Alt + P");
  check("Super reads as Win", H.prettyAccel("Super+E"), "Win + E");
  check("Meta reads as Win", H.prettyAccel("Meta+E"), "Win + E");
  check("a named key is spelled out", H.prettyAccel("Alt+Space"), "Alt + Space");
  check("an arrow key is spelled out", H.prettyAccel("Alt+Up"), "Alt + Up");
  check("Equal reads as the character", H.prettyAccel("CommandOrControl+Equal"), "Ctrl + =");
  check("NumpadAdd reads as Num +", H.prettyAccel("CommandOrControl+NumpadAdd"), "Ctrl + Num +");
  check("an unknown key part is kept", H.prettyAccel("Ctrl+F9"), "Ctrl + F9");

  // ---- comparison key
  check("case and spaces do not matter", H.accelKey("  CommandOrControl + SHIFT + p "), "ctrl+p+shift");
  check("the parts are sorted", H.accelKey("Shift+Alt+P"), "alt+p+shift");
  check("CommandOrControl and Control are one", H.accelKey("CommandOrControl+P") === H.accelKey("Control+P"), true);
  check("CmdOrCtrl is ctrl", H.accelKey("CmdOrCtrl+P"), "ctrl+p");
  check("CommandOrCtrl is ctrl", H.accelKey("CommandOrCtrl+P"), "ctrl+p");
  check("CmdOrControl is ctrl", H.accelKey("CmdOrControl+P"), "ctrl+p");
  check("Command, Cmd, Meta and Win are super", [H.accelKey("Command+P"), H.accelKey("Cmd+P"), H.accelKey("Meta+P"), H.accelKey("Win+P")], ["p+super", "p+super", "p+super", "p+super"]);
  check("Super stays super", H.accelKey("Super+P"), "p+super");
  check("Option is alt", H.accelKey("Option+P"), "alt+p");
  check("an empty part is ignored", H.accelKey("Ctrl++P"), "ctrl+p");
  check("empty gives an empty key", H.accelKey(""), "");
  check("null gives an empty key", H.accelKey(null), "");
  check("two different keys differ", H.accelKey("Ctrl+P") === H.accelKey("Ctrl+O"), false);
  check("Equal is not the same as NumpadAdd", H.accelKey("Ctrl+Equal") === H.accelKey("Ctrl+NumpadAdd"), false);

  // ---- error mapping
  check("invalid", H.hotkeyMessage("hotkey-invalid"), "Invalid");
  check("taken", H.hotkeyMessage("hotkey-taken"), "Taken");
  check("system", H.hotkeyMessage("hotkey-system"), "System");
  check("conflict", H.hotkeyMessage("hotkey-conflict"), "Conflict");
  check("a failed save has its own text", H.hotkeyMessage("save-failed"), "Save failed");
  check("anything else is the generic text", H.hotkeyMessage("whatever"), "Hotkey failed");
  check("undefined is the generic text", H.hotkeyMessage(undefined), "Hotkey failed");

  // ---- who owns a combination
  ctx.settings = { hotkey: "CommandOrControl+Shift+P", voice_hotkey: "Alt+V", rec_stop_hotkey: "Alt+S", rec_pause_hotkey: "Alt+Q" };
  ctx.prompts = [
    { id: "a", name: "First", hotkey: "Ctrl+1" },
    { id: "b", name: "Second", hotkey: "Control+2" },
    { id: "c", name: "Third", hotkey: "" },
  ];
  check("the app hotkey owns its combination", H.hotkeyOwner("ctrl+shift+p"), { app: true });
  check("the voice hotkey", H.hotkeyOwner("Alt+V"), { voice: true });
  check("the stop hotkey", H.hotkeyOwner("alt+s"), { recStop: true });
  check("the pause hotkey", H.hotkeyOwner("Alt+Q"), { recPause: true });
  check("a prompt hotkey names its prompt", H.hotkeyOwner("CommandOrControl+2"), { name: "Second" });
  check("a free combination has no owner", H.hotkeyOwner("Ctrl+9"), null);
  check("an empty combination has no owner", H.hotkeyOwner(""), null);
  check("a prompt's own binding is not a conflict", H.hotkeyOwner("Ctrl+1", "a"), null);
  check("another prompt's binding still is", H.hotkeyOwner("Ctrl+1", "b"), { name: "First" });
  check("the app slot skips the app hotkey", H.hotkeyOwner("Ctrl+Shift+P", null, "app"), null);
  check("the voice slot skips the voice hotkey", H.hotkeyOwner("Alt+V", null, "voice"), null);
  check("the stop slot skips the stop hotkey", H.hotkeyOwner("Alt+S", null, "recStop"), null);
  check("the pause slot skips the pause hotkey", H.hotkeyOwner("Alt+Q", null, "recPause"), null);
  check("a slot still sees the other global hotkeys", H.hotkeyOwner("Alt+V", null, "app"), { voice: true });
  ctx.settings = {};
  check("an unset global hotkey owns nothing", H.hotkeyOwner("Alt+V"), null);
  ctx.settings = { hotkey: "Ctrl+Shift+P" };
  ctx.prompts = [{ id: "z", name: "Z" }];
  check("a prompt with no hotkey field owns nothing", H.hotkeyOwner("Ctrl+Z"), null);

  // ---- owner messages
  check("app message", H.hotkeyTakenMsg({ app: true }), "Used by the app");
  check("voice message", H.hotkeyTakenMsg({ voice: true }), "Used by voice");
  check("stop message", H.hotkeyTakenMsg({ recStop: true }), "Used by stop");
  check("pause message", H.hotkeyTakenMsg({ recPause: true }), "Used by pause");
  check("a prompt name fills the slot", H.hotkeyTakenMsg({ name: "My prompt" }), "Used by My prompt");
  check("a name with $& goes in literally", H.hotkeyTakenMsg({ name: "a$&b" }), "Used by a$&b");

  // ---- the hotkey list kinds
  ctx.settings = { hotkey: "Ctrl+G", voice_hotkey: "Ctrl+V", rec_stop_hotkey: "Ctrl+S", rec_pause_hotkey: "Ctrl+P" };
  ctx.prompts = [{ id: "p1", name: "One", hotkey: "Ctrl+1" }];
  const K = H.HOTKEYS_LIST_KINDS;
  check("the list kinds", Object.keys(K), ["global", "prompt", "voice", "recStop", "recPause"]);
  check("the global kind reads the app hotkey", K.global.current(), "Ctrl+G");
  check("its slot", [K.global.slot, K.voice.slot, K.recStop.slot, K.recPause.slot], ["app", "voice", "recStop", "recPause"]);
  check("the prompt kind reads its prompt's hotkey", K.prompt.current("p1"), "Ctrl+1");
  check("an unknown prompt reads empty", K.prompt.current("nope"), "");
  check("the prompt kind has no slot", K.prompt.slot, undefined);
  check("voice, stop and pause read their own settings", [K.voice.current(), K.recStop.current(), K.recPause.current()], ["Ctrl+V", "Ctrl+S", "Ctrl+P"]);

  // ---- commits
  calls.length = 0; toasts.length = 0;
  check("global commit returns the accel", await H.commitGlobalHotkey("Ctrl+H"), "Ctrl+H");
  check("global commit calls set_hotkey", calls[0], ["set_hotkey", { hotkey: "Ctrl+H" }]);
  check("global commit stores the accel", ctx.settings.hotkey, "Ctrl+H");
  check("global commit toasts", toasts, ["Hotkey set"]);
  toasts.length = 0;
  await H.commitGlobalHotkey("");
  check("clearing stores an empty accel", ctx.settings.hotkey, "");
  check("clearing does not toast", toasts, []);
  await H.commitVoiceHotkey("Ctrl+J");
  check("voice commit command and setting", [calls[calls.length - 1], ctx.settings.voice_hotkey], [["set_voice_hotkey", { hotkey: "Ctrl+J" }], "Ctrl+J"]);
  await H.commitRecStopHotkey("Ctrl+K");
  check("stop commit command and setting", [calls[calls.length - 1], ctx.settings.rec_stop_hotkey], [["set_rec_stop_hotkey", { hotkey: "Ctrl+K" }], "Ctrl+K"]);
  await H.commitRecPauseHotkey("Ctrl+L");
  check("pause commit command and setting", [calls[calls.length - 1], ctx.settings.rec_pause_hotkey], [["set_rec_pause_hotkey", { hotkey: "Ctrl+L" }], "Ctrl+L"]);
  ctx.__ret = (cmd) => { if (cmd === "set_hotkey") throw "hotkey-taken"; };
  let thrown = null;
  const before = ctx.settings.hotkey;
  try { await H.commitGlobalHotkey("Ctrl+X"); } catch (e) { thrown = e; }
  check("a refused global commit throws the code", thrown, "hotkey-taken");
  check("a refused global commit keeps the old setting", ctx.settings.hotkey, before);
  ctx.__ret = null;

  const full = { id: "p1", name: "One", color: "#112233", show_image: true, copy_image: false, file_path: "f", pdf_page: 2, icon_path: "i", caption: "c", caption_size: 3, font: "x", font_size: 9, favorite: true, tags: ["t"], notes: "n", hotkey: "Ctrl+1", hotkey_paste: "paste", rotate_mode: "", rotate_auto: false };
  ctx.prompts = [full];
  calls.length = 0; toasts.length = 0;
  ctx.__ret = (cmd, a) => (cmd === "update_prompt" ? { hotkey: a.hotkey, hotkey_paste: a.hotkeyPaste } : undefined);
  check("a prompt commit returns the stored accel", await H.commitPromptHotkeyDirect("p1", "Ctrl+5"), "Ctrl+5");
  check("the conflict check runs first", calls[0], ["check_hotkey", { hotkey: "Ctrl+5", id: "p1" }]);
  const upd = calls[1];
  check("then update_prompt is called", upd[0], "update_prompt");
  check("it sends the new hotkey and keeps the paste choice", [upd[1].hotkey, upd[1].hotkeyPaste], ["Ctrl+5", "paste"]);
  check("it carries the other fields over", [upd[1].id, upd[1].name, upd[1].color, upd[1].filePath, upd[1].pdfPage, upd[1].favorite, upd[1].tags, upd[1].notes], ["p1", "One", "#112233", "f", 2, true, ["t"], "n"]);
  check("it sends no text, entries or image", ["text" in upd[1], "entries" in upd[1], "image" in upd[1]], [false, false, false]);
  check("the prompt copy follows the stored answer", [full.hotkey, full.hotkey_paste], ["Ctrl+5", "paste"]);
  calls.length = 0;
  await H.commitPromptHotkeyDirect("p1", "");
  check("clearing skips the conflict check", calls.map((c) => c[0]), ["update_prompt"]);
  check("clearing also clears the paste choice", [calls[0][1].hotkey, calls[0][1].hotkeyPaste], ["", ""]);
  check("the prompt copy is cleared", [full.hotkey, full.hotkey_paste], ["", ""]);
  toasts.length = 0;
  ctx.__ret = (cmd) => (cmd === "update_prompt" ? { hotkey: "", hotkey_paste: "" } : undefined);
  check("an OS refusal after the check returns an empty accel", await H.commitPromptHotkeyDirect("p1", "Ctrl+6"), "");
  check("an OS refusal after the check shows the notice", toasts, ["Refused"]);
  ctx.__ret = (cmd) => (cmd === "update_prompt" ? null : undefined);
  thrown = null;
  try { await H.commitPromptHotkeyDirect("p1", "Ctrl+7"); } catch (e) { thrown = e; }
  check("an empty save answer throws save-failed", thrown, "save-failed");
  thrown = null;
  calls.length = 0;
  try { await H.commitPromptHotkeyDirect("missing", "Ctrl+7"); } catch (e) { thrown = e; }
  check("an unknown prompt throws save-failed without a call", [thrown, calls.length], ["save-failed", 0]);
  ctx.__ret = (cmd) => { if (cmd === "check_hotkey") throw "hotkey-system"; };
  thrown = null;
  calls.length = 0;
  try { await H.commitPromptHotkeyDirect("p1", "Ctrl+8"); } catch (e) { thrown = e; }
  check("a failed conflict check stops before the save", [thrown, calls.map((c) => c[0])], ["hotkey-system", ["check_hotkey"]]);
  ctx.__ret = null;

  // ---- the editor's staged hotkey
  ctx.modalState = null;
  H.setPromptHotkey("Ctrl+3");
  check("no open editor changes nothing", ctx.modalState, null);
  ctx.modalState = { hotkey: "Ctrl+1", hotkeyPaste: "paste" };
  ctx.modal = { hotkeyInput: {}, hotkeyBadge: { textContent: "", classList: { toggle() {} } }, hotkeyErr: { textContent: "", classList: { toggle() {} } }, hotkeyAction: null };
  H.setPromptHotkey("Ctrl+3");
  check("a new hotkey is staged and the paste choice kept", ctx.modalState, { hotkey: "Ctrl+3", hotkeyPaste: "paste" });
  check("the field shows the readable label", ctx.modal.hotkeyInput.value, "Ctrl + 3");
  H.setPromptHotkey("");
  check("clearing the hotkey clears the paste choice", ctx.modalState, { hotkey: "", hotkeyPaste: "" });
  check("the field is empty again", ctx.modal.hotkeyInput.value, "");

  // ---- capture switch
  calls.length = 0;
  H.hotkeyCapture(true);
  check("capture on calls the backend", [calls[0], H.getCapture()], [["set_hotkey_capture", { active: true }], true]);
  H.hotkeyCapture(true);
  check("the same state is not sent twice", calls.length, 1);
  H.reassertHotkeyCapture();
  check("reassert sends the on state again", calls.map((c) => c[1].active), [true, true]);
  H.hotkeyCapture(false);
  check("capture off calls the backend", [calls[calls.length - 1][1].active, H.getCapture()], [false, false]);
  const n = calls.length;
  H.reassertHotkeyCapture();
  check("reassert while off does nothing", calls.length, n);
}

main().then(() => {
  if (problems.length) {
    console.log(`${problems.length} problem(s):`);
    for (const p of problems) console.log(" - " + p);
    process.exit(1);
  }
  console.log(`hotkeys_check: ${count} assertions OK`);
}).catch((e) => { console.log(e.stack || e); process.exit(1); });
