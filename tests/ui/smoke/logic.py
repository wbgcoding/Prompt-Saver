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
# 854: a tile or folder member with a placeholder starts the copy cooldown like any other
# copy: once its dialog was answered, an immediate second click asks nothing.
COOLDOWN_DIALOG_PROBE = """
    // 854:
    {
      const inv = window.__TAURI__.core.invoke;
      await inv('set_ui_value', { key: 'copyCooldownMs', value: 2000 });
      await inv('update_prompt', { id: 'p4', text: 'Folder note for {?who}' });
      window.__FIRE__('store-synced');
      await sleep(500);
      const asks = () => argsOf('open_var_window').length;
      const answer = async (n) => {
        window.__FIRE__('vars-filled', JSON.stringify({ values: Array.from({ length: n }, () => 'x'), remember: Array.from({ length: n }, () => false) }));
        await sleep(300);
      };
      const press = (el) => {
        for (const type of ['pointerdown', 'pointerup', 'click']) {
          el.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, button: 0, isPrimary: true, pointerId: 1, clientX: 5, clientY: 5 }));
        }
      };
      const tile = document.querySelector('#grid .tile[data-id="p1"]');
      if (!tile) {
        errs.push('854: the placeholder tile is missing');
      } else {
        press(tile);
        await until(() => asks() === 1, 40);
        if (asks() !== 1) errs.push('854: the first click did not ask: ' + asks());
        await answer(2);
        await sleep(300);
        const before = asks();
        press(tile);
        await sleep(500);
        if (asks() !== before) errs.push('854: a click right after a copy asked again (tile)');
      }
      const ftile = document.querySelector('#grid .folder-tile');
      if (!ftile) {
        errs.push('854: the folder tile is missing');
      } else {
        press(ftile);
        await until(() => document.querySelector('.tile-folder-pop:not(.hidden) .folder-member-tile[data-id="p4"]'), 40);
        const member = document.querySelector('.tile-folder-pop .folder-member-tile[data-id="p4"]');
        if (!member) {
          errs.push('854: the folder member with a placeholder is missing');
        } else {
          const base = asks();
          const memberClick = async () => {
            member.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, cancelable: true, button: 0, isPrimary: true, pointerId: 1, clientX: 5, clientY: 5 }));
            window.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, button: 0, isPrimary: true, pointerId: 1, clientX: 5, clientY: 5 }));
            await sleep(500);
          };
          await memberClick();
          if (asks() !== base + 1) errs.push('854: the first member click did not ask: ' + (asks() - base));
          await answer(1);
          await sleep(300);
          const before = asks();
          await memberClick();
          if (asks() !== before) errs.push('854: a click right after a copy asked again (folder member)');
        }
      }
    }
"""
# 855: opening the global search and closing it again before the history fetch answers
# must leave the overlay hidden, not stuck open past every close path.
SEARCH_RACE_PROBE = """
    // 855:
    {
      window.__STUB_DELAY_MS__ = { recent_copies: 300 };
      const gs = $('global-search');
      $('global-search-btn').click();
      await sleep(50);
      $('global-search-btn').click();
      await sleep(600);
      if (visible(gs)) errs.push('855: the search overlay was shown after it had been closed during the history fetch');
      window.__STUB_DELAY_MS__ = {};
      // A plain open still works and closes with Escape.
      $('global-search-btn').click();
      await until(() => visible(gs), 40);
      if (!visible(gs)) errs.push('855: the search overlay did not open');
      $('gs-input').dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
      await sleep(100);
      if (visible(gs)) errs.push('855: Escape did not close the search overlay');
    }
"""
# 856: a new screenshot arriving over an editor with edits asks before it replaces them. Keep
# editing leaves the session (and the file Apply will clean up) untouched; Replace cancels the
# old session once and opens the new shot.
SNIP_REPLACE_SEED = '<script>window.__SEED_SETTINGS__ = { ui_flags: { screenshotSave: true } };</script>'
SNIP_REPLACE_PROBE = """
    // 856:
    {
      const cm = $('confirm-modal'), edit = $('snip-edit'), undo = $('snip-edit-undo'), live = $('snip-edit-live');
      const shot = (w, h) => {
        const cv = document.createElement('canvas'); cv.width = w; cv.height = h;
        const g = cv.getContext('2d'); g.fillStyle = '#88aacc'; g.fillRect(0, 0, w, h);
        return cv.toDataURL('image/png');
      };
      window.__FIRE__('snip-captured', { data_url: shot(200, 120), path: 'C:/shots/first.png', name: 'first.png' });
      await until(() => visible(edit), 60);
      if (!visible(edit)) {
        errs.push('856: the editor did not open for the first shot');
      } else {
        const stroke = async () => {
          document.querySelector('#snip-edit-tools button[data-tool="pen"]').click();
          await sleep(50);
          const r = live.getBoundingClientRect();
          live.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0, pointerId: 1, clientX: r.left + 10, clientY: r.top + 10 }));
          live.dispatchEvent(new PointerEvent('pointermove', { bubbles: true, pointerId: 1, clientX: r.left + 40, clientY: r.top + 30 }));
          window.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 1, clientX: r.left + 40, clientY: r.top + 30 }));
          await sleep(100);
        };
        // An editor with no edits is replaced without a question.
        window.__FIRE__('snip-captured', { data_url: shot(160, 100), path: 'C:/shots/clean.png', name: 'clean.png' });
        await until(() => live.width === 160, 40);
        if (visible(cm)) errs.push('856: a clean editor asked before it was replaced');
        if (live.width !== 160) errs.push('856: a clean editor was not replaced by the new shot: ' + live.width);
        await stroke();
        if (undo.disabled) errs.push('856: fixture: the stroke left no operation');
        window.__FIRE__('snip-captured', { data_url: shot(120, 80), path: 'C:/shots/second.png', name: 'second.png' });
        await until(() => visible(cm), 40);
        if (!visible(cm)) {
          errs.push('856: a new shot over an edited editor did not ask');
        } else {
          if ($('confirm-title').textContent.trim() !== 'Replace the open screenshot?') errs.push('856: unexpected dialog: ' + $('confirm-title').textContent);
          $('confirm-cancel').click();
        }
        await sleep(300);
        if (!visible(edit) || undo.disabled || live.width !== 160) errs.push('856: Keep editing changed the open session: undo ' + undo.disabled + ' width ' + live.width);
        // Apply cleans up the file the OPEN session belongs to: still the clean shot's.
        window.__SMOKE_ARGS__.length = 0;
        $('snip-edit-apply').click();
        await until(() => argsOf('delete_screenshot_file').length > 0, 40);
        const del = argsOf('delete_screenshot_file').map((c) => c[1].path);
        if (del.join() !== 'C:/shots/clean.png') errs.push('856: after Keep editing Apply cleaned up ' + JSON.stringify(del) + ' instead of the open shot');
        for (const id of ['snip-modal-close', 'snip-modal-close-x']) { const b = $(id); if (b) b.click(); }
        await sleep(200);
        // Replace: the old session ends, the new shot opens with a fresh history.
        window.__FIRE__('snip-captured', { data_url: shot(200, 120), path: 'C:/shots/third.png', name: 'third.png' });
        await until(() => visible(edit) && live.width === 200, 40);
        await stroke();
        window.__FIRE__('snip-captured', { data_url: shot(120, 80), path: 'C:/shots/fourth.png', name: 'fourth.png' });
        await until(() => visible(cm), 40);
        if (visible(cm)) $('confirm-ok').click();
        await until(() => live.width === 120, 40);
        if (live.width !== 120) errs.push('856: Replace did not open the new shot: ' + live.width);
        if (!undo.disabled) errs.push('856: the replaced editor kept the old history');
        window.__SMOKE_ARGS__.length = 0;
        $('snip-edit-apply').click();
        await until(() => argsOf('delete_screenshot_file').length > 0, 40);
        const del2 = argsOf('delete_screenshot_file').map((c) => c[1].path);
        if (del2.join() !== 'C:/shots/fourth.png') errs.push('856: after Replace Apply cleaned up ' + JSON.stringify(del2) + ' instead of the new shot');
      }
    }
"""
# 857: while a recording is being converted the dialog's Save, Save as, Copy, Attach and
# Video editor actions are locked, and enabled again once it is done.
REC_CONVERT_SEED = '<script>window.__SEED_SETTINGS__ = { ui_texts: { recFormat: "webm" } };</script>'
REC_CONVERT_PROBE = """
    // 857:
    {
      const ids = ['rec-modal-save', 'rec-modal-saveas', 'rec-modal-copy', 'rec-modal-yes', 'rec-modal-edit'];
      window.__STUB_DELAY_MS__ = { convert_recording: 600 };
      window.__FIRE__('recording-captured', { path: 'C:/fake/raw.mp4' });
      await until(() => visible($('rec-modal')), 40);
      await sleep(200);
      if (!visible($('rec-modal'))) errs.push('857: the recording dialog did not open');
      const open = ids.filter((id) => !$(id).disabled);
      if (open.length) errs.push('857: still enabled while converting: ' + open.join(', '));
      await until(() => argsOf('convert_recording').length > 0, 20);
      await sleep(900);
      const stuck = ids.filter((id) => $(id).disabled);
      if (stuck.length) errs.push('857: still disabled after the conversion: ' + stuck.join(', '));
      // Discard ends the dialog; the next one must start with the actions unlocked or locked by its own conversion.
      window.__STUB_DELAY_MS__ = {};
    }
"""

# 3001-recedit: choosing Edit in the recording dialog hands the file to the editor and closes the
# dialog, so no Save, Save as or Copy can move it afterwards and nothing is discarded.
REC_EDIT_PROBE = """
    // 3001-recedit:
    {
      window.__FIRE__('recording-captured', { path: 'C:/fake/edit-me.mp4' });
      await until(() => visible($('rec-modal')), 40);
      if (!visible($('rec-modal'))) errs.push('3001-recedit: the recording dialog did not open');
      $('rec-modal-edit').click();
      await until(() => argsOf('open_video_editor').length > 0, 40);
      await until(() => !visible($('rec-modal')), 40);
      if (argsOf('open_video_editor').length !== 1) errs.push('3001-recedit: the editor was not opened once');
      if (visible($('rec-modal'))) errs.push('3001-recedit: the dialog stayed open after Edit');
      await sleep(200);
      for (const cmd of ['discard_recording_file', 'save_recording_now', 'save_recording_as', 'copy_recording_to_clipboard']) {
        if (argsOf(cmd).length) errs.push('3001-recedit: ' + cmd + ' ran after Edit');
      }
    }
"""

# 859: a resize keeps the earlier image for undo, but only as far back as the undo limit
# reaches; what lies behind it is released, and closing the editor releases the whole session.
SNIP_HOLD_SEED = ''
SNIP_HOLD_PROBE = """
    // 859:
    {
      const edit = $('snip-edit'), undo = $('snip-edit-undo'), redo = $('snip-edit-redo');
      const cv = document.createElement('canvas'); cv.width = 200; cv.height = 120;
      const g = cv.getContext('2d'); g.fillStyle = '#88aacc'; g.fillRect(0, 0, 200, 120);
      window.__FIRE__('snip-captured', { data_url: cv.toDataURL('image/png'), path: '', name: 'hold.png' });
      await until(() => visible(edit), 60);
      if (!visible(edit)) {
        errs.push('859: the editor did not open');
      } else {
        const resize = async (label) => {
          $('snip-edit-scale').click();
          await sleep(100);
          const pop = document.querySelector('.snip-scale-pop');
          [...pop.querySelectorAll('.snip-scale-presets button')].find((b) => b.textContent === label).click();
          await sleep(30);
          pop.querySelector('.primary-btn').click();
          await sleep(120);
        };
        for (let i = 0; i < 52; i++) await resize(i % 2 === 0 ? '50%' : '200%');
        const held = window.__snipEditHeld().resizes;
        if (held !== 50) errs.push('859: after 52 resizes with an undo limit of 50, ' + held + ' resizes still hold their earlier image, wanted 50');
        let steps = 0;
        while (!undo.disabled && steps < 80) { undo.click(); steps++; await sleep(20); }
        if (steps !== 50) errs.push('859: undo walked ' + steps + ' resizes, wanted 50');
        let redos = 0;
        while (!redo.disabled && redos < 80) { redo.click(); redos++; await sleep(20); }
        if (redos !== 50) errs.push('859: redo walked ' + redos + ' resizes, wanted 50');
        $('snip-edit-cancel').click();
        await sleep(200);
        const after = window.__snipEditHeld();
        if (after.ops || after.redo || after.img || after.raw || after.resizes) errs.push('859: closing the editor left the session alive: ' + JSON.stringify(after));
        // Strokes drawn after a resize push it below the undo floor: its earlier image goes too.
        window.__FIRE__('snip-captured', { data_url: cv.toDataURL('image/png'), path: '', name: 'hold2.png' });
        await until(() => visible(edit), 60);
        await resize('50%');
        if (window.__snipEditHeld().resizes !== 1) errs.push('859: a fresh resize does not hold its earlier image: ' + window.__snipEditHeld().resizes);
        document.querySelector('#snip-edit-tools button[data-tool="pen"]').click();
        await sleep(50);
        const live = $('snip-edit-live');
        const r = live.getBoundingClientRect();
        for (let i = 0; i < 52; i++) {
          const x = r.left + 4 + (i % 20) * 3, y = r.top + 4 + (i % 20) * 3;
          live.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0, pointerId: 1, clientX: x, clientY: y }));
          live.dispatchEvent(new PointerEvent('pointermove', { bubbles: true, pointerId: 1, clientX: x + 5, clientY: y + 5 }));
          window.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 1, clientX: x + 5, clientY: y + 5 }));
          await sleep(20);
        }
        const behind = window.__snipEditHeld().resizes;
        if (behind !== 0) errs.push('859: a resize behind the undo limit still holds its earlier image: ' + behind);
        $('snip-edit-cancel').click();
      }
    }
"""

# 860: "Back up now" is locked while its backup is being written, so a double click starts one.
BACKUP_NOW_PROBE = """
    // 860:
    {
      window.__STUB_DELAY_MS__ = { create_backup: 400 };
      (await import('./backups.js')).openBackups();
      await until(() => document.querySelector('#backup-panel .backup-now-btn'), 40);
      const btn = document.querySelector('#backup-panel .backup-now-btn');
      if (!btn) {
        errs.push('860: the Back up now button is missing');
      } else {
        btn.click();
        btn.click();
        await sleep(100);
        if (!btn.disabled) errs.push('860: the button stays enabled while the backup is written');
        await sleep(900);
        const n = argsOf('create_backup').length;
        if (n !== 1) errs.push('860: two quick clicks made ' + n + ' backups, wanted 1');
      }
      window.__STUB_DELAY_MS__ = {};
    }
"""

def _boot(browser, tmp, tag, body, budget=45000, seed=""):
    side = tmp / "ui" / f"logic-{tag}.html"
    side.write_text(
        (REPO / "ui" / "index.html").read_text(encoding="utf-8")
        .replace("<script", seed + '<script src="tauri_stub.js"></script>\n<script', 1)
        .replace("</body>", HEAD + body + TAIL + "</body>"),
        encoding="utf-8")
    proc = run_chrome(browser, tmp / ("profile-logic-%s" % tag), "1000,780", budget, side.as_uri(), 180)
    m = re.search(r'<pre id="SMOKE">(\[.*?\])</pre>', proc.stdout, re.S)
    return json.loads(unescape(m.group(1))) if m else ["no probe result"]


def run(browser, tmp):
    failed = 0
    for tag, body, seed in (
            ("edit-event", EDIT_EVENT_PROBE, ""),
            ("archive-overflow", ARCHIVE_OVERFLOW_PROBE, ""),
            ("cooldown-dialog", COOLDOWN_DIALOG_PROBE, ""),
            ("search-race", SEARCH_RACE_PROBE, ""),
            ("snip-replace", SNIP_REPLACE_PROBE, SNIP_REPLACE_SEED),
            ("rec-convert", REC_CONVERT_PROBE, REC_CONVERT_SEED),
            ("rec-edit", REC_EDIT_PROBE, ""),
            ("snip-hold", SNIP_HOLD_PROBE, SNIP_HOLD_SEED),
            ("backup-now", BACKUP_NOW_PROBE, "")):
        errs = _boot(browser, tmp, tag, body, seed=seed)
        print(f"-- ui logic ({tag})")
        for e in errs:
            print(f"   FAIL: {e}")
        failed += len(errs)
        if not errs:
            print("   ok")
    return failed
