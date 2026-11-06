// Editor (ui/editor.js, real extracted source in a vm): the length counter and the Markdown
// preview timer test the wrapper that is really hidden and shown (modal.mdWrap), not the pane
// inside it. For an image or file prompt (textarea and wrapper hidden) the counter stays off,
// and while the person types in the textarea (wrapper hidden) no hidden re-render is queued.
// Run:  node tests/ui/editor_md_hidden_check.js      Exit: 0 = clean, 1 = problems.
const path = require("path");
const vm = require("vm");

const src = require(path.join(__dirname, "..", "ui_sources.js")).mainWindowSource().replace(/\r\n/g, "\n");
function extractBlock(marker) {
  const start = src.indexOf(marker);
  if (start === -1) throw new Error(`could not find (source text changed?): ${marker}`);
  const braceStart = src.indexOf("{", src.indexOf(")", start));
  let depth = 0;
  for (let i = braceStart; i < src.length; i++) {
    if (src[i] === "{") depth++;
    else if (src[i] === "}" && --depth === 0) return src.slice(start, i + 1);
  }
  throw new Error(`unbalanced braces after: ${marker}`);
}
const code = [extractBlock("function updateLenCounter("), extractBlock("function scheduleMdPreview(")].join("\n");

function box(hidden) {
  const cls = new Set(hidden ? ["hidden"] : []);
  return {
    textContent: "", value: "",
    classList: {
      toggle: (c, on) => { if (on === undefined ? !cls.has(c) : on) cls.add(c); else cls.delete(c); },
      contains: (c) => cls.has(c),
    },
  };
}

function run(state) {
  const counter = box(false);
  const timers = [];
  const modal = { text: box(state.textHidden), mdWrap: box(state.wrapHidden), mdPreview: box(false) };
  modal.text.value = "hello world";
  const sandbox = {
    modal, $: () => counter, flag: () => true, rotationUiOn: () => false, txt: () => "all",
    t: (k) => k, LANG: "en", mdLastText: "stale", mdTimer: 0,
    setTimeout: (fn) => { timers.push(fn); return timers.length; }, clearTimeout() {},
    renderMdPreview() {},
  };
  vm.createContext(sandbox);
  vm.runInContext(code, sandbox);
  vm.runInContext("updateLenCounter(); scheduleMdPreview();", sandbox);
  return { counterHidden: counter.classList.contains("hidden"), queued: timers.length };
}

const problems = [];
// Image or file prompt: textarea and Markdown wrapper both hidden.
let r = run({ textHidden: true, wrapHidden: true });
if (!r.counterHidden) problems.push("the length counter shows under a field that is not there (image or file prompt)");
// Resting state of a Markdown text prompt: preview stands in for the textarea.
r = run({ textHidden: true, wrapHidden: false });
if (r.counterHidden) problems.push("the length counter is gone while the Markdown preview is shown");
if (r.queued !== 1) problems.push("the visible preview was not queued for a re-render");
// Typing: textarea visible, wrapper hidden.
r = run({ textHidden: false, wrapHidden: true });
if (r.counterHidden) problems.push("the length counter is gone while typing");
if (r.queued !== 0) problems.push("a hidden preview was re-rendered while typing");

if (problems.length) {
  console.error("editor_md_hidden_check: FAILED\n  " + problems.join("\n  "));
  process.exit(1);
}
console.log("editor_md_hidden_check: ok");
