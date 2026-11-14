// Dictation stop pressed before voice_start returned its session id (ui/dictation.js): the
// stop is remembered whether the state still says "arming" or a first partial already flipped it
// to "listening", and startVoice stops the session when the id arrives.
// Runs the real extracted source in a node vm. Run:  node tests/voice_stop_race_check.js
const vm = require("vm");
const src = require("./ui_sources.js").mainWindowSource().replace(/\r\n/g, "\n");

function extractBlock(marker) {
  const start = src.indexOf(marker);
  if (start === -1) throw new Error(`could not find ${marker} (source text changed?)`);
  const braceStart = src.indexOf("{", start);
  let depth = 0;
  for (let i = braceStart; i < src.length; i++) {
    if (src[i] === "{") depth++;
    else if (src[i] === "}" && --depth === 0) return src.slice(start, i + 1);
  }
  throw new Error(`unbalanced braces extracting ${marker}`);
}

const problems = [];
const check = (label, actual, expected) => {
  const a = JSON.stringify(actual), e = JSON.stringify(expected);
  if (a !== e) problems.push(`${label}: got ${a}, expected ${e}`);
};

async function scenario(label, flipToListening) {
  let release;
  const sent = [];
  const sb = {
    voiceState: "idle", voiceSessionId: null, voiceToken: 0, voiceTargetEl: null, voiceActiveBtn: null,
    voiceInterim: null, voiceArmCancelled: false, voiceResolvedLang: "", ended: 0,
    setVoiceBtnState() {}, endVoiceUi() { sb.ended++; sb.voiceState = "idle"; },
    routeVoiceError() {}, recordToolUsage() {}, txt: () => "auto", flag: () => false, val: () => 0,
    invoke: (cmd, args) => {
      if (cmd === "voice_start") return new Promise((r) => { release = () => r({ id: 7, lang: "en-US" }); });
      sent.push([cmd, args.id]);
      return Promise.resolve();
    },
  };
  vm.createContext(sb);
  vm.runInContext(extractBlock("async function startVoice(target, btn) {"), sb);
  vm.runInContext(extractBlock("async function stopVoice() {"), sb);
  const starting = sb.startVoice({}, {});
  if (flipToListening) sb.voiceState = "listening"; // the first partial arrived before the id
  await sb.stopVoice();
  check(`${label}: nothing to send yet`, sent, []);
  release();
  await starting;
  check(`${label}: the session is stopped when its id arrives`, sent, [["voice_stop", 7]]);
}

(async () => {
  await scenario("stop while arming", false);
  await scenario("stop after the first partial", true);
  for (const p of problems) console.log("FAIL", p);
  console.log(problems.length ? `${problems.length} problem(s)` : "PASS");
  process.exit(problems.length ? 1 : 0);
})();
