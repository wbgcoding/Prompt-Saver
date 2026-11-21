"""The shared probe runner of the generated findings suites: one Chrome boot of a page
per probe, the probe body appended to the page, its failures read back from the dump."""
import json
import re

from .harness import REPO, unescape, run_chrome


AR_SCAFFOLD = r"""
<script>
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const until = async (fn, tries) => { for (let i = 0; i < (tries || 100) && !fn(); i++) await sleep(50); return fn(); };
  try {
%s
%s
  } catch (e) { errs.push('probe threw: ' + e.message); }
  errs.push(...(window.__SMOKE_ERRORS__ || []));
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""

AR_COMMON = r"""
const visible = (el) => !!el && !el.classList.contains('hidden');
const callsFor = (cmd) => (window.__SMOKE_ARGS__ || []).filter(([c]) => c === cmd).map(([, a]) => a);
const tileEl = (id) => document.querySelector('#grid .tile[data-id="' + id + '"]');
const centre = (el) => { const r = el.getBoundingClientRect(); return [r.left + r.width / 2, r.top + r.height / 2]; };
const ptr = (type, x, y, el, extra) => (el || window).dispatchEvent(new PointerEvent(type, Object.assign({
  bubbles: true, cancelable: true, button: 0, isPrimary: true, pointerId: 6, clientX: x, clientY: y,
}, extra || {})));
const dragTo = async (tile, tx, ty) => {
  const [sx, sy] = centre(tile);
  ptr('pointerdown', sx, sy, tile);
  ptr('pointermove', sx + 8, sy - 8, tile);
  await sleep(60);
  ptr('pointermove', tx, ty, window);
  await sleep(60);
};
const endDrag = async (x, y) => { ptr('pointerup', x, y, window); await sleep(300); };
const bootReady = async () => {
  await until(() => document.querySelectorAll('#grid .tile').length > 0, 200);
  const cm = document.getElementById('confirm-modal');
  for (let i = 0; i < 60 && !visible(cm); i++) await sleep(50);
  if (visible(cm)) { document.getElementById('confirm-cancel').click(); await sleep(300); }
};
const cellEl = (c, r) => document.querySelector('#grid .cell[data-col="' + c + '"][data-row="' + r + '"]');
"""


def make_ar_probe(browser, tmp):
    """Returns ar_probe(tag, label, page, size, pre, body) -> number of failures."""
    def ar_probe(tag, label, page, size, pre, body):
        side = tmp / "ui" / f"ar-{tag}.html"
        side.write_text(
            (REPO / "ui" / f"{page}.html").read_text(encoding="utf-8")
            .replace("<script", pre + '<script src="tauri_stub.js"></script>\n<script', 1)
            .replace("</body>", AR_SCAFFOLD % (AR_COMMON, body) + "</body>"),
            encoding="utf-8")
        w, h = size.split("x")
        proc = run_chrome(browser, tmp / ('profile-ar-' + tag), f"{w},{h}", 45000, side.as_uri(), 180)
        m = re.search(r'<pre id="SMOKE">(\[.*?\])</pre>', proc.stdout, re.S)
        errs = json.loads(unescape(m.group(1))) if m else ["no probe result"]
        print(f"-- {label}")
        for e in errs:
            print(f"   FAIL: {e}")
        if not errs:
            print("   ok")
        return len(errs)

    return ar_probe
