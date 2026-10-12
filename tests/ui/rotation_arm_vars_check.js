// Auto-advancing prompt whose entries carry placeholders (ui/library.js copyResolved,
// ui/promptVars.js resolveVarsList): every entry is filled from ONE dialog and handed
// to rotation_arm_session, so later pastes of the session deliver filled text. A
// backend refusal falls back to the plain peek/copy/commit path.
//
// The real source text of the helpers is extracted and run in a vm against stubs.
// Run:  node tests/ui/rotation_arm_vars_check.js      Exit: 0 = clean, 1 = problems.
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
  extractBlock("async function resolveVarsList("),
  extractBlock("async function copyResolved("),
].join("\n");

const VAR_RE = /\{\?([^}\n]+)\}/g;
function make({ armResult = true, filled = { who: "Ana", tone: "kind" } } = {}) {
  const calls = [];
  const dialogs = [];
  const sandbox = {
    calls, dialogs,
    prompts: [],
    flag: () => true,
    toast: () => {},
    t: (k) => k,
    CLIP_VAR_MAX: 1000,
    isDynVar: () => false,
    parseVar: (tok) => ({ name: tok }),
    dynName: (n) => n,
    dynValue: () => "",
    nextVarCounter: () => 1,
    extractVars: (text) => [...new Set([...(text || "").matchAll(VAR_RE)].map((m) => m[1].trim()))],
    fillVars: (text, values) => text.replace(VAR_RE, (_, k) => values[k.trim()] ?? ""),
    varTokens: (p, text) => [...(text ?? p.text).matchAll(VAR_RE)],
    anyEntryHasVars: (p) => [p.text, ...(p.variants || [])].some((e) => VAR_RE.test(e) && !(VAR_RE.lastIndex = 0)),
    resolveVars: async (text) => (await sandbox.resolveVarsList([text]))?.[0] ?? null,
    promptVarsWindow: async (ask) => { dialogs.push(ask.slice()); return filled; },
    recordToolUsage: () => {},
    renderGrid: async () => {},
    ensureSecretVaultUnlocked: async () => true,
    errToast: () => {},
    invoke: async (cmd, args) => {
      calls.push([cmd, args]);
      if (cmd === "rotation_arm_session") return armResult;
      if (cmd === "rotation_peek") return { text: "Hi {?who}" };
      if (cmd === "copy_text") return true;
      return null;
    },
  };
  vm.createContext(sandbox);
  vm.runInContext(code + "\nthis.copyResolved = copyResolved; this.resolveVarsList = resolveVarsList;", sandbox);
  return sandbox;
}

const problems = [];
const check = (label, actual, expected) => {
  const a = JSON.stringify(actual), e = JSON.stringify(expected);
  if (a !== e) problems.push(`${label}: got ${a}, expected ${e}`);
};
const prompt = { id: "p1", text: "Hi {?who}", variants: ["Be {?tone}, {?who}", "plain"], rotate_auto: true };

(async () => {
  {
    const s = make();
    check("union of placeholders across texts", await s.resolveVarsList(["a {?x}", "b {?y} {?x}"]), ["a ", "b  "]);
    check("one dialog asks every name once", s.dialogs, [["x", "y"]]);
    check("texts without placeholders come back unchanged", await s.resolveVarsList(["a", "b"]), ["a", "b"]);
  }
  {
    const s = make();
    check("armed copy succeeds", await s.copyResolved({ ...prompt }), true);
    check("a single dialog for all entries", s.dialogs.length, 1);
    const arm = s.calls.find(([c]) => c === "rotation_arm_session");
    check("arm gets every entry filled, in order", arm && arm[1], { id: "p1", resolved: ["Hi Ana", "Be kind, Ana", "plain"] });
    check("the backend advances, so no commit follows", s.calls.some(([c]) => c === "rotation_commit"), false);
    check("no plain copy next to the session", s.calls.some(([c]) => c === "copy_text"), false);
  }
  {
    const s = make({ armResult: false });
    check("refused arm still copies", await s.copyResolved({ ...prompt }), true);
    check("fallback copies the peeked entry and commits", s.calls.filter(([c]) => c === "copy_text" || c === "rotation_commit").map(([c, a]) => [c, a.text ?? a.id]), [["copy_text", "Hi Ana"], ["rotation_commit", "p1"]]);
  }
  {
    const s = make({ filled: null });
    check("a cancelled dialog copies nothing", await s.copyResolved({ ...prompt }), false);
    check("nothing armed after a cancel", s.calls.some(([c]) => c === "rotation_arm_session"), false);
  }
  {
    const s = make();
    await s.copyResolved({ ...prompt, rotate_auto: false });
    check("without Auto no session is armed", s.calls.some(([c]) => c === "rotation_arm_session"), false);
  }
  if (problems.length) {
    console.log(problems.join("\n"));
    process.exit(1);
  }
  console.log("rotation_arm_vars_check: ok");
})();
