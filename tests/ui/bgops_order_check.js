// Background jobs badge (ui/bgOps.js, real extracted source): when two list_bg_ops calls are
// in flight (last progress tick + done event) and the older answer lands last, that stale
// "running" answer must not overwrite the newer one, or the badge keeps a finished job.
// Run:  node tests/ui/bgops_order_check.js      Exit: 0 = clean, 1 = problems.
const fs = require("fs");
const path = require("path");
const vm = require("vm");

const uiDir = path.join(__dirname, "..", "..", "ui");
const src = fs.readFileSync(path.join(uiDir, "bgOps.js"), "utf8").replace(/\r\n/g, "\n");
const start = src.indexOf("export const bgOpsToasted");
const end = src.indexOf("export function wireBgOps");
if (start < 0 || end < 0) { console.error("bgops_order_check: source markers not found"); process.exit(1); }
const code = src.slice(start, end).replace(/^export /gm, "");

function fakeEl() {
  const cls = new Set();
  return {
    textContent: "",
    classList: {
      toggle: (c, on) => { if (on === undefined ? !cls.has(c) : on) cls.add(c); else cls.delete(c); },
      contains: (c) => cls.has(c),
    },
  };
}

async function main() {
  const els = {};
  const replies = [];
  const sandbox = {
    $: (id) => (els[id] ||= fakeEl()),
    invoke: () => new Promise((r) => replies.push(r)),
    t: (k) => k, toast() {}, errToast() {}, bgKindNoun: () => "x",
    bgOpsPop: { classList: { contains: () => true } },
    renderBgOpsPop() {},
  };
  vm.createContext(sandbox);
  vm.runInContext(code + "\nthis.refreshBgOps = refreshBgOps;", sandbox);
  const problems = [];
  const running = [{ id: "export-1", kind: "export", state: "running", started_at_ms: 1 }];
  const done = [{ id: "export-1", kind: "export", state: "done", started_at_ms: 1 }];
  const first = sandbox.refreshBgOps();   // older call
  const second = sandbox.refreshBgOps();  // newer call
  replies[1](done);                       // the newer answer lands first
  await second;
  replies[0](running);                    // the stale one lands last
  await first;
  if (!els["bg-ops-btn"].classList.contains("hidden")) problems.push("a stale running answer showed the badge again after the job finished");
  if (els["bg-ops-badge"].textContent !== "0") problems.push(`badge count ${els["bg-ops-badge"].textContent}, expected 0`);
  if (problems.length) {
    console.error("bgops_order_check: FAILED\n  " + problems.join("\n  "));
    process.exit(1);
  }
  console.log("bgops_order_check: ok");
}
main().catch((e) => { console.error(e); process.exit(1); });
