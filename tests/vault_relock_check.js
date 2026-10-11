// Confidential prompts and the vault timeout (ui/views.js armVaultRelock, ui/library.js
// copyResolved). The backend drops the key lazily and tells no window, so:
//   * while decrypted confidential text is shown, one refetch is timed for the moment
//     the key expires, and a later, shorter timeout moves that timer forward,
//   * a copy of a confidential prompt asks the backend first and goes the locked way
//     when the key is gone, instead of trusting the cached "unlocked" copy.
// Runs the real extracted source in a node vm. Run:  node tests/vault_relock_check.js
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

function sandbox(status) {
  const sb = {
    now: 1_000_000,
    timers: [], // { id, delay, fn, live }
    calls: [],
    status,
    prompts: [{ id: "s", secret: true, locked: false, text: "plain", variants: [] }],
    renders: 0,
    unlockAsked: 0,
    unlockAnswer: false,
  };
  sb.Date = { now: () => sb.now };
  sb.setTimeout = (fn, delay) => { const t = { id: sb.timers.length + 1, delay, fn, live: true }; sb.timers.push(t); return t.id; };
  sb.clearTimeout = (id) => { const t = sb.timers.find((x) => x.id === id); if (t) t.live = false; };
  sb.invoke = async (cmd) => {
    sb.calls.push(cmd);
    if (cmd === "secret_vault_status") return sb.status;
    if (cmd === "copy_prompt") return true;
    return null;
  };
  sb.renderGrid = async () => {
    sb.renders++;
    // The refetch brings the locked shape back when the key is gone.
    if (!sb.status.unlocked) sb.prompts = sb.prompts.map((p) => ({ ...p, locked: true, text: "" }));
  };
  sb.renderLibrary = () => {};
  sb.libraryEl = { classList: { contains: () => true } };
  sb.ensureSecretVaultUnlocked = async () => { sb.unlockAsked++; return sb.unlockAnswer; };
  sb.varTokens = () => [];
  sb.errToast = () => {};
  vm.createContext(sb);
  vm.runInContext("let vaultRelockTimer = 0; let vaultRelockAt = 0; let vaultRelockAsking = false;", sb);
  vm.runInContext(extractBlock("async function armVaultRelock() {"), sb);
  vm.runInContext(extractBlock("async function copyResolved(p) {"), sb);
  return sb;
}

const live = (sb) => sb.timers.filter((t) => t.live);

(async () => {
  // The relock refetch is timed for the moment the key expires (plus a second).
  {
    const sb = sandbox({ unlocked: true, configured: true, remaining_secs: 300 });
    await sb.armVaultRelock();
    check("one timer armed", live(sb).map((t) => t.delay), [301_000]);
    // Same expiry again: the waiting timer stays.
    await sb.armVaultRelock();
    check("an unchanged expiry keeps the timer", live(sb).length, 1);
    check("no second timer", sb.timers.length, 1);
    // The timeout was shortened: the key now expires sooner, the timer moves forward.
    sb.status = { unlocked: true, configured: true, remaining_secs: 20 };
    await sb.armVaultRelock();
    check("a shorter expiry replaces the timer", live(sb).map((t) => t.delay), [21_000]);
    // A longer one never pushes it back: the early refetch just re-arms itself.
    sb.status = { unlocked: true, configured: true, remaining_secs: 900 };
    await sb.armVaultRelock();
    check("a longer expiry leaves the sooner timer", live(sb).map((t) => t.delay), [21_000]);
    // The timer fires: the refetch runs and a new round may be armed.
    sb.status = { unlocked: false, configured: true, remaining_secs: null };
    await live(sb)[0].fn();
    check("the refetch ran", sb.renders, 1);
    check("the locked shape is back", sb.prompts[0].locked, true);
  }
  // Nothing decrypted shown, "until quit" and a locked vault arm nothing.
  {
    const sb = sandbox({ unlocked: true, configured: true, remaining_secs: null });
    await sb.armVaultRelock();
    check("no timeout, no timer", sb.timers.length, 0);
    const locked = sandbox({ unlocked: false, configured: true, remaining_secs: null });
    await locked.armVaultRelock();
    check("locked vault, no timer", locked.timers.length, 0);
    const plain = sandbox({ unlocked: true, configured: true, remaining_secs: 60 });
    plain.prompts = [{ id: "a", secret: false }];
    await plain.armVaultRelock();
    check("nothing confidential shown, no status call", plain.calls, []);
  }
  // A copy re-checks the vault: the cached copy says unlocked, the backend says no.
  {
    const sb = sandbox({ unlocked: false, configured: true, remaining_secs: null });
    const ok = await sb.copyResolved(sb.prompts[0]);
    check("expired key: the grid is refetched", sb.renders, 1);
    check("expired key: the password is asked for", sb.unlockAsked, 1);
    check("expired key: no copy without it", sb.calls.includes("copy_prompt"), false);
    check("expired key: the copy fails", ok, false);
  }
  {
    const sb = sandbox({ unlocked: true, configured: true, remaining_secs: 100 });
    const ok = await sb.copyResolved(sb.prompts[0]);
    check("live key: no refetch", sb.renders, 0);
    check("live key: no password question", sb.unlockAsked, 0);
    check("live key: copied", [ok, sb.calls.includes("copy_prompt")], [true, true]);
  }

  for (const p of problems) console.log("FAIL", p);
  console.log(problems.length ? `${problems.length} problem(s)` : "PASS");
  process.exit(problems.length ? 1 : 0);
})().catch((e) => { console.error(e); process.exit(1); });
