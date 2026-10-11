// Voice hotkey dictation (ui/dictation.js): when the session stops while the buffer
// still carries the live partial of the last phrase, the final text for that phrase can
// land just after voice-stopped. The partial must not go out as one text and the final
// as a second one: the delivery waits for the final and sends the merged buffer once.
//
// The real source text of the helpers is extracted and run in a vm against fake timers.
// Run:  node tests/ui/voice_hotkey_late_check.js      Exit: 0 = clean, 1 = problems.
const path = require("path");
const vm = require("vm");

const src = require(path.join(__dirname, "..", "ui_sources.js")).mainWindowSource().replace(/\r\n/g, "\n");
function extractBlock(marker) {
  const start = src.indexOf(marker);
  if (start === -1) throw new Error(`could not find in the main window source (text changed?): ${marker}`);
  const braceStart = src.indexOf("{", start);
  let depth = 0;
  for (let i = braceStart; i < src.length; i++) {
    if (src[i] === "{") depth++;
    else if (src[i] === "}" && --depth === 0) return src.slice(start, i + 1);
  }
  throw new Error(`unbalanced braces after: ${marker}`);
}
const code = [
  "let voiceHotkeyHold = null;",
  "let voicePrev = null;",
  extractBlock("function flushVoiceHotkeyHold("),
  extractBlock("function holdVoiceHotkeyText("),
  extractBlock("function deliverLateVoiceText("),
].join("\n");

function make() {
  let nextId = 1;
  const timers = new Map();
  const delivered = [];
  const sandbox = {
    VOICE_LATE_GRACE_MS: 3000,
    LANG: "en",
    flag: () => false,
    txt: () => "",
    applyVoiceCommands: (text) => text,
    deliverVoiceHotkeyText: (text) => { delivered.push(text); },
    insertLateVoiceText: () => {},
    Date,
    setTimeout: (fn) => { const id = nextId++; timers.set(id, fn); return id; },
    clearTimeout: (id) => { timers.delete(id); },
  };
  vm.createContext(sandbox);
  vm.runInContext(code, sandbox);
  const fire = () => { for (const [id, fn] of [...timers]) { timers.delete(id); fn(); } };
  return { sandbox, delivered, fire, timers };
}

const problems = [];
const check = (ok, what) => { if (!ok) problems.push(what); };

// 1. Partial held, the final arrives late: one delivery, the final in the partial's place.
{
  const { sandbox, delivered } = make();
  vm.runInContext('holdVoiceHotkeyText("hello wor");', sandbox);
  check(delivered.length === 0, "the buffer with a live partial is not sent at once");
  sandbox.voicePrev = null;
  vm.runInContext(
    'voicePrev = {token: 5, el: null, caret: null, lang: "en", hotkey: true, endedAt: Date.now(), interim: {start: 6, len: 3, text: "wor"}};',
    sandbox,
  );
  vm.runInContext('deliverLateVoiceText({token: 5, text: "world"});', sandbox);
  check(delivered.length === 1 && delivered[0] === "hello world", `late final replaces the partial and goes out once (got ${JSON.stringify(delivered)})`);
}

// 2. Partial held, no final ever arrives: the grace timer sends the buffer.
{
  const { sandbox, delivered, fire } = make();
  vm.runInContext('holdVoiceHotkeyText("hello wor");', sandbox);
  fire();
  check(delivered.length === 1 && delivered[0] === "hello wor", "the held buffer goes out when the grace window ends");
  fire();
  check(delivered.length === 1, "and only once");
}

// 3. A hotkey session without a held buffer: a late final is new text and goes out on its own.
{
  const { sandbox, delivered } = make();
  vm.runInContext(
    'voicePrev = {token: 7, el: null, caret: null, lang: "en", hotkey: true, endedAt: Date.now(), interim: null};',
    sandbox,
  );
  vm.runInContext('deliverLateVoiceText({token: 7, text: "again"});', sandbox);
  check(delivered.length === 1 && delivered[0] === "again", "a late final with nothing held is delivered as it is");
}

if (problems.length) {
  console.error("voice_hotkey_late_check: " + problems.length + " problem(s)");
  for (const p of problems) console.error("  - " + p);
  process.exit(1);
}
console.log("voice_hotkey_late_check: clean");
