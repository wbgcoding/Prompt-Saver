"""UI-logic suite: one fresh boot of the main window per group of checks, for behaviour that is
decided in the page (dialog guards, counters, cooldowns, races) and needs no layout sweep."""
import json
import re

from .harness import REPO, unescape, run_chrome


HEAD = """
<script>
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const $ = (id) => document.getElementById(id);
  const visible = (el) => !!el && !el.classList.contains('hidden');
  const until = async (fn, n) => { for (let i = 0; i < (n || 60) && !fn(); i++) await sleep(50); return fn(); };
  const argsOf = (cmd) => (window.__SMOKE_ARGS__ || []).filter((c) => c[0] === cmd);
  try {
    await until(() => document.querySelectorAll('#grid .tile').length > 0, 120);
    // Startup questions (the data takeover offer) are closed with Cancel first.
    for (let i = 0; i < 25; i++) {
      if (visible($('confirm-modal'))) { $('confirm-cancel').click(); await sleep(300); } else await sleep(100);
    }
"""

TAIL = """
  } catch (e) { errs.push('probe: ' + e.message); }
  errs.push(...(window.__SMOKE_ERRORS__ || []));
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""

# 852: an "edit-prompt" event while the editor holds unsaved typing asks before it replaces
# the open session; Keep editing leaves the typed text where it is.
EDIT_EVENT_PROBE = """
    // 852:
    {
      const cm = $('confirm-modal');
      window.__FIRE__('edit-prompt', 'p0');
      await until(() => visible($('modal')));
      if (!visible($('modal'))) errs.push('852: the editor did not open for the first prompt');
      const nameA = $('modal-name').value;
      $('modal-text').value = $('modal-text').value + ' typed by the user';
      $('modal-text').dispatchEvent(new Event('input', { bubbles: true }));
      const textA = $('modal-text').value;
      window.__FIRE__('edit-prompt', 'p4');
      await until(() => visible(cm), 20);
      if (!visible(cm)) errs.push('852: a second edit-prompt over a dirty editor did not ask');
      else if ($('confirm-title').textContent.trim() !== 'Discard changes?') errs.push('852: unexpected dialog: ' + $('confirm-title').textContent);
      else $('confirm-cancel').click();
      await sleep(300);
      if ($('modal-name').value !== nameA || $('modal-text').value !== textA) {
        errs.push('852: Keep editing did not leave the first session as it was: ' + $('modal-name').value);
      }
      // Discard replaces the session with the requested prompt.
      window.__FIRE__('edit-prompt', 'p4');
      await until(() => visible(cm), 20);
      if (visible(cm)) $('confirm-ok').click();
      await until(() => $('modal-name').value === 'Prompt 5', 40);
      if ($('modal-name').value !== 'Prompt 5') errs.push('852: Discard did not open the requested prompt: ' + $('modal-name').value);
      // A clean editor is replaced without asking.
      window.__FIRE__('edit-prompt', 'p0');
      await until(() => $('modal-name').value === 'Prompt 1', 40);
      if (visible(cm)) { errs.push('852: a clean editor asked before it was replaced'); $('confirm-cancel').click(); }
      if ($('modal-name').value !== 'Prompt 1') errs.push('852: a clean editor was not replaced: ' + $('modal-name').value);
    }
"""

# 853: archiving a placed tile takes it off the grid without counting it as overflow.
ARCHIVE_OVERFLOW_PROBE = """
    // 853:
    {
      const banner = $('grid-overflow');
      if (visible(banner)) errs.push('853: the overflow banner is up before anything was archived');
      const tile = document.querySelector('#grid .tile');
      if (!tile) {
        errs.push('853: no placed tile to archive');
      } else {
        const id = tile.dataset.id;
        await window.__TAURI__.core.invoke('set_ui_text', { key: 'archiveTag', value: 'Archive' });
        await window.__TAURI__.core.invoke('batch_prompts', { ids: [id], action: 'tag-add', tag: 'Archive' });
        window.__FIRE__('store-synced');
        await until(() => !document.querySelector('#grid .tile[data-id="' + id + '"]'), 40);
        await sleep(200);
        if (document.querySelector('#grid .tile[data-id="' + id + '"]')) errs.push('853: the archived tile is still on the grid');
        if (visible(banner)) errs.push('853: an archived tile counts as overflow: ' + $('grid-overflow-text').textContent);
      }
    }
"""

def _boot(browser, tmp, tag, body, budget=45000):
    side = tmp / "ui" / f"logic-{tag}.html"
    side.write_text(
        (REPO / "ui" / "index.html").read_text(encoding="utf-8")
        .replace("<script", '<script src="tauri_stub.js"></script>\n<script', 1)
        .replace("</body>", HEAD + body + TAIL + "</body>"),
        encoding="utf-8")
    proc = run_chrome(browser, tmp / ("profile-logic-%s" % tag), "1000,780", budget, side.as_uri(), 180)
    m = re.search(r'<pre id="SMOKE">(\[.*?\])</pre>', proc.stdout, re.S)
    return json.loads(unescape(m.group(1))) if m else ["no probe result"]


def run(browser, tmp):
    failed = 0
    for tag, body in (
            ("edit-event", EDIT_EVENT_PROBE),
            ("archive-overflow", ARCHIVE_OVERFLOW_PROBE)):
        errs = _boot(browser, tmp, tag, body)
        print(f"-- ui logic ({tag})")
        for e in errs:
            print(f"   FAIL: {e}")
        failed += len(errs)
        if not errs:
            print("   ok")
    return failed
