// The expert switch for the receiver certificate lifetime (ui/main.js).
// Run:  node tests/remote_tls_switch_check.js      Exit: 0 = clean, 1 = problems.
const fs = require("fs");
const path = require("path");
const src = fs.readFileSync(path.join(__dirname, "..", "ui", "main.js"), "utf8").replace(/\r\n/g, "\n");
const i18n = fs.readFileSync(path.join(__dirname, "..", "ui", "i18n.js"), "utf8");
const problems = [];
const need = (ok, msg) => { if (!ok) problems.push(msg); };

// The entry: in the remote group, gated by the network switch, with label and tip.
need(/\{ key: "remoteTlsShortLived", gate: "remoteNetworkFilter" \}/.test(src), "expert entry with gate remoteNetworkFilter is missing");
need(/remoteTlsShortLived: "remoteTlsShortLived",/.test(src), "flag label mapping is missing");
const tips = src.match(/const EXPERT_TIPS = \[([\s\S]*?)\];/);
need(tips && /"remoteTlsShortLived"/.test(tips[1]), "the tip is not in EXPERT_TIPS");
const bulk = src.match(/const EXPERT_BULK_NEVER = \[([^\]]*)\]/);
need(bulk && /"remoteTlsShortLived"/.test(bulk[1]), "bulk writers must never touch the switch");
for (const key of ["remoteTlsShortLived", "tipRemoteTlsShortLived"]) {
  const n = (i18n.match(new RegExp(`^[ \\t]+${key}: "`, "gm")) || []).length;
  need(n === 11, `${key} must exist in all 11 languages (found ${n})`);
}
need(/key === "remoteTlsShortLived"[\s\S]{0,200}toggleShortLivedCert\(enabled\)[\s\S]{0,80}input\.checked = !enabled/.test(src), "the switch handler must restore the checkbox when the toggle is not applied");

// Behaviour of the toggle: take the function out of the source and run it.
const start = src.indexOf("async function toggleShortLivedCert");
need(start >= 0, "toggleShortLivedCert not found");
let depth = 0, end = start, seen = false;
for (let i = src.indexOf("{", start); i < src.length; i++) {
  if (src[i] === "{") { depth++; seen = true; } else if (src[i] === "}") depth--;
  if (seen && depth === 0) { end = i + 1; break; }
}
const build = (answer, invokeImpl) => {
  const log = { invoked: [], toasts: [], errs: [] };
  const settings = { ui_flags: {} };
  const fn = new Function("confirmDialog", "invoke", "t", "toast", "errToast", "settings",
    `${src.slice(start, end)}\nreturn toggleShortLivedCert;`)(
    async () => answer,
    async (cmd, args) => { log.invoked.push([cmd, args]); return invokeImpl ? invokeImpl() : "FP"; },
    (k) => k, (m) => log.toasts.push(m), (e) => log.errs.push(String(e)), settings);
  return { fn, log, settings };
};
(async () => {
  let r = build(false);
  need((await r.fn(true)) === false, "cancel must report not applied");
  need(r.log.invoked.length === 0 && r.settings.ui_flags.remoteTlsShortLived === undefined, "cancel must save nothing");
  r = build(true);
  need((await r.fn(true)) === true, "confirm must report applied");
  need(JSON.stringify(r.log.invoked) === JSON.stringify([["remote_tls_set_short_lived", { enabled: true }]]), "confirm must call the dedicated command once");
  need(r.settings.ui_flags.remoteTlsShortLived === true, "confirm must record the new state");
  r = build(true, () => { throw "save-failed"; });
  need((await r.fn(true)) === false && r.log.toasts.includes("persistFailed") && r.settings.ui_flags.remoteTlsShortLived === undefined, "a failed save must restore the switch and show the error");
  for (const p of problems) console.log("FAIL", p);
  console.log(problems.length ? `${problems.length} problem(s)` : "PASS");
  process.exit(problems.length ? 1 : 0);
})();
