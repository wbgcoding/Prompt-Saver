"""Full-resolution screenshots of the real UI against the test stub, for a visual review.

    python tests/ui/shots.py [--size 880x600] [--lang de] [--theme dark] [--scale 1] [--out .tmp/shots] [surface ...]

--size, --lang, --theme and --scale take a comma list and shoot every combination;
`--lang all` and `--theme all` mean every UI language and every built-in theme.

A surface is `main`, `settings`, `settings_end`, `expert`, `library`, `history`,
`library_pins`, `tilemenu`, `editor`, `editor_long`, `editor_tags`, `editor_adv`, `folder`, `folders_mix`, `folders_mix_small`, `help`, `help_capture`, `backup`, `recresult`, `snipedit`, `snipdraw`, `snipsel`, `snipcrop`, `snip_edit_ocr`, `sniptext`, `sniptextsel`, or another
page such as `video.html` (`video.html:vehelp_cut` shoots the help dialog at one section).
A group (`@settings`, `@library`, `@tiles`, `@recording`, `@snip`) shoots its surfaces.
Without a surface every one is shot. Not a test: it never fails on what it sees.
"""
import argparse
import http.server
import itertools
import re
import subprocess
import sys
import threading
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from smoke.harness import CHROME_CANDIDATES  # noqa: E402

REPO = Path(__file__).resolve().parents[2]
UI = REPO / "ui"
STUB = Path(__file__).resolve().parent / "tauri_stub.js"

# Runs once the page is up: answers the first-run take-over question, then opens a surface.
PRE = """
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const $ = (id) => document.getElementById(id);
for (let i = 0; i < 40; i++) {
  await sleep(100);
  const b = document.querySelector('#confirm-modal:not(.hidden) #confirm-cancel');
  if (b) { b.click(); break; }
}
await sleep(300);
"""
TILE_MENU = """
const t = document.querySelector('#grid .tile[data-id]'); const r = t.getBoundingClientRect();
t.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: r.left + 30, clientY: r.top + 30 }));
await sleep(300);
"""
# Opens the screenshot editor on a 1400x800 white canvas with lines of small text,
# the same way a real capture arrives.
SNIP_OPEN = """
const cv = document.createElement('canvas'); cv.width = 1400; cv.height = 800;
const c = cv.getContext('2d'); c.fillStyle = '#fff'; c.fillRect(0, 0, 1400, 800);
c.fillStyle = '#222'; c.font = '16px Segoe UI';
for (let i = 0; i < 30; i++) c.fillText('Sample text line ' + i + ' 0123456789', 20, 30 + i * 25);
window.__FIRE__('snip-captured', { data_url: cv.toDataURL('image/png'), path: '', name: 'x.png' });
await sleep(800);
"""
# The text recognition crop view: an image file picked with a right-click on the OCR button.
SNIP_OCR_OPEN = """
const cv = document.createElement('canvas'); cv.width = 1400; cv.height = 800;
const c = cv.getContext('2d'); c.fillStyle = '#fff'; c.fillRect(0, 0, 1400, 800);
c.fillStyle = '#222'; c.font = '16px Segoe UI';
for (let i = 0; i < 30; i++) c.fillText('Sample text line ' + i + ' 0123456789', 20, 30 + i * 25);
window.__OPEN_EDIT_RESULT__ = { kind: 'image', data_url: cv.toDataURL('image/png') };
$('ocr-btn').dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
await sleep(900);
"""
EDITOR_TAGS = (
    "const core = await import('./core.js'); const extra = Array.from({ length: 14 }, (_, i) => 'Tag ' + String.fromCharCode(65 + i) + ' sample');"
    "core.prompts[core.prompts.length - 1].tags = [...(core.prompts[core.prompts.length - 1].tags || []), ...extra];"
    + TILE_MENU + "document.querySelector('#ctx [data-act=\"edit\"]')?.click(); await sleep(500);"
    "$('modal-advanced-head').click(); await sleep(400); for (let e = $('modal-tag-quickadd').parentElement; e; e = e.parentElement) e.scrollTop = 1e6; await sleep(200);"
)

# Four folders of 2, 3, 6 and 12 members over a mix of coloured, plain and image prompts, drawn on
# the home grid after boot. `small` also shrinks the tiles to the densest grid used in practice.
def _folders_mix(small):
    grid = "view.cols = 8; view.rows = 6; view.layouts = {}; const cells = [[0, 0], [2, 0], [4, 0], [6, 0]];" if small else "const cells = [[0, 1], [2, 1], [0, 2], [2, 2]];"
    return (
        "const core = await import('./core.js'); const views = await import('./views.js');"
        "const view = core.settings.views.find((v) => v.id === core.settings.active_view);"
        + grid +
        "const names = ['Cold email draft', 'Bug report', 'Meeting notes', 'Translate', 'Release checklist', 'Standup', 'Idea', 'Weekly review', 'SQL helper', 'Code review', 'Refactor plan', 'Thank you note', 'Summary', 'Pitch'];"
        "const cols = ['#3b82f6', '', '#ef4444', '#10b981', '', '#f59e0b'];"
        "const svg = (c) => 'data:image/svg+xml;utf8,' + encodeURIComponent('<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"40\" height=\"40\"><rect width=\"40\" height=\"40\" fill=\"' + c + '\"/><circle cx=\"20\" cy=\"20\" r=\"10\" fill=\"#fff\"/></svg>');"
        "const mk = (i) => ({ id: 'fx' + i, name: names[i % names.length], text: 'Body ' + i, color: cols[i % cols.length], tags: [], notes: '', hotkey: '', file_path: '', variants: [], image: i === 4 ? svg('#8b5cf6') : '' });"
        "const ids = Array.from({ length: 23 }, (_, i) => mk(i)).map((p) => { core.prompts.push(p); return p.id; });"
        "core.settings.tile_folders = [2, 3, 6, 12].map((n, k) => ({ id: 'fm' + k, name: 'Folder ' + n, color: k === 3 ? '#8b5cf6' : '', view_id: view.id, cell: cells[k], members: ids.splice(0, n) }));"
        "await views.renderGrid(true); await sleep(400);"
    )


ACTS = {
    "main": "",
    # The media manager window (`manager.html:recs`, `manager.html:shots`): the recordings grid it
    # opens on, the screenshots after the main window switched it over, and the screenshots empty
    # state with saving to a folder off.
    "recs": "await sleep(1200);",
    "recs_sel": "await sleep(1000); const tiles = document.querySelectorAll('#mm-grid .mm-tile'); tiles[1]?.click(); tiles[3]?.dispatchEvent(new MouseEvent('click', { bubbles: true, ctrlKey: true })); await sleep(500);",
    "shots": "window.__FIRE__('manager-kind', 'screenshots'); await sleep(1200);",
    "shots_off": "window.__MM_FILES__ = []; SETTINGS.ui_flags.screenshotSave = false; window.__FIRE__('manager-kind', 'screenshots'); await sleep(1000);",
    # The video editor help dialog, one section per surface (`video.html:vehelp_cut`).
    **{f"vehelp_{k}": "await sleep(600); document.getElementById('ve-shortcuts').click(); await sleep(150); document.querySelector(\"#ve-help-nav .ve-help-tab[data-help='" + k + "']\").click(); await sleep(200);" for k in ("nav", "cut", "edit", "ov", "export", "keys")},
    # The overlay editor of a new focus area (shape select, darkening).
    "veovspot": "await sleep(600); document.getElementById('ve-tab-overlays').click(); await sleep(150); document.getElementById('ve-ov-add-spotlight').click(); await sleep(400); document.getElementById('ve-ov-editor').scrollIntoView({ block: 'center' }); await sleep(200);",
    # The export tab right after a file export: status line with the folder/file buttons under it.
    "veexportdone": "await sleep(600); document.getElementById('ve-tab-export').click(); await sleep(150); document.getElementById('ve-export-status').textContent = 'Exported: C:\\\\Users\\\\name\\\\Videos\\\\My clips\\\\recording-2026-11-07.mp4'; document.getElementById('ve-export-done-row').classList.remove('hidden'); document.getElementById('ve-export-saveas').classList.remove('hidden'); document.getElementById('ve-export-start').scrollIntoView({ block: 'end' }); await sleep(150);",
    "settings": "$('gear').click(); await sleep(300);",
    "settings_end": "$('gear').click(); await sleep(300); $('settings-main').lastElementChild.scrollIntoView(); await sleep(100);",
    "expert": "$('gear').click(); await sleep(300); $('expert-open').click(); await sleep(400);",
    # Prints the height of the settings list into the shot: the scroll content of the dialog and
    # what is visible of it, so a compaction of the dialog can be compared as numbers.
    "settings_probe": "$('gear').click(); await sleep(300); const sm = $('settings-main'); const pr = document.createElement('pre'); pr.style.cssText = 'position:fixed;left:8px;top:8px;z-index:99999;background:#fff;color:#000;font:700 22px monospace;padding:8px'; pr.textContent = 'scrollHeight=' + sm.scrollHeight + ' clientHeight=' + sm.clientHeight + ' width=' + $('settings').querySelector('.modal').clientWidth; document.body.appendChild(pr);",
    "tour": '$(\'gear\').click(); await sleep(300); $(\'expert-open\').click(); await sleep(400); const find = () => document.querySelector(".expert-action[data-action=\'tour-open\']"); let tb = find(); for (const tab of document.querySelectorAll(\'#expert-tabs-bar .expert-tab\')) { if (tb) break; tab.click(); await sleep(150); tb = find(); } tb.click(); await sleep(500); const hs = []; for (let i = 0; i < 4; i++) { hs.push(Math.round($(\'tour-body\').getBoundingClientRect().height) + \'/\' + Math.round(document.querySelector(\'#tour-body .tour-step\').offsetHeight)); if (i < 3) { $(\'tour-next\').click(); await sleep(150); } } $(\'tour-dots\').querySelector(\'.tour-dot\').click(); await sleep(200); const pr = document.createElement(\'pre\'); pr.style.cssText = \'position:fixed;left:8px;top:8px;z-index:99999;background:#fff;color:#000;font:700 18px monospace;padding:6px\'; pr.textContent = \'body/step per page: \' + hs.join(\'  \') + \'  box=\' + Math.round(document.querySelector(\'#tour-modal .tour-modal-box\').getBoundingClientRect().height); document.body.appendChild(pr);',
    "expert_system": "$('gear').click(); await sleep(300); $('expert-open').click(); await sleep(400); [...document.querySelectorAll('#expert-tabs-bar .expert-tab')].at(-1).click(); await sleep(300); $('expert-flags').scrollTop = 1e6; await sleep(200);",
    "expert_end": "$('gear').click(); await sleep(300); $('expert-open').click(); await sleep(400); $('expert-flags').scrollTop = 1e6; await sleep(200);",
    **{f"expert_t{i}": "$('gear').click(); await sleep(300); $('expert-open').click(); await sleep(400); [...document.querySelectorAll('#expert-tabs-bar .expert-tab')][" + str(i) + "].click(); await sleep(300);" for i in range(6)},
    "library": "$('library-btn').click(); await sleep(400);",
    "history": "$('library-btn').click(); await sleep(400); $('library-tab-history').click(); await sleep(400);",
    # The library search dropdown with two pinned searches (one with tags) over three recent terms.
    "library_pins": "const core = await import('./core.js'); core.settings.ui_texts = { ...core.settings.ui_texts, recentSearches: JSON.stringify(['Invoice', 'Weekly report', 'Beta']), pinnedSearches: JSON.stringify([{ query: 'meeting notes', tags: ['work', 'weekly', 'team'], sort: '' }, { query: '', tags: ['private'], sort: '' }]) };"
                      "$('library-btn').click(); await sleep(400); $('library-q').dispatchEvent(new Event('pointerdown')); await sleep(300);",
    "tilemenu": TILE_MENU,
    "movefolder": TILE_MENU + "document.querySelector('#ctx [data-act=\"moveToFolder\"]').click(); await sleep(300);",
    "folderpick": TILE_MENU + "document.querySelector('#ctx [data-act=\"moveToFolder\"]').click(); await sleep(300);"
                  "document.querySelector('.move-to-folder-pop button').click(); await sleep(300);"
                  "$('confirm-ok').click(); await sleep(400);",
    "editor": TILE_MENU + "document.querySelector('#ctx [data-act=\"edit\"]')?.click(); await sleep(500);",
    "editor_long": TILE_MENU + "document.querySelector('#ctx [data-act=\"edit\"]')?.click(); await sleep(500);"
                   "const ta = $('modal-text'); ta.value = Array.from({ length: 60 }, (_, i) => 'Line ' + (i + 1) + ' of a long prompt').join(String.fromCharCode(10));"
                   "ta.dispatchEvent(new Event('input', { bubbles: true })); await sleep(400);",
    # The prompt editor over a library with many unassigned tags: 12 chips plus the "+N more" chip,
    # and the same editor after the chip expanded the rest.
    "editor_tags": EDITOR_TAGS + "await sleep(200);",
    "editor_tags_all": EDITOR_TAGS + "document.querySelector('.tag-quickadd-more')?.click(); await sleep(300);",
    "editor_adv": TILE_MENU + "document.querySelector('#ctx [data-act=\"edit\"]')?.click(); await sleep(500);"
                  "$('modal-advanced-head').click(); await sleep(400);",
    "editor_rotate": TILE_MENU + "document.querySelector('#ctx [data-act=\"edit\"]')?.click(); await sleep(500);"
                     "$('modal-advanced-head').click(); await sleep(300); $('modal-rotate-switch').click(); await sleep(300);"
                     "$('modal-rotate-add').click(); await sleep(200); $('modal-advanced-head').click(); await sleep(400);",
    "folder": ("const t = document.querySelector('#grid .folder-tile');"
               "t.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0, clientX: 5, clientY: 5 }));"
               "window.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, button: 0, clientX: 5, clientY: 5 }));"
               "await sleep(600);"),
    # Four folders of 2, 3, 6 and 12 members on the grid (`folders_mix_small` on a denser grid).
    "folders_mix": _folders_mix(False),
    "folders_mix_small": _folders_mix(True),
    "help": "$('gear').click(); await sleep(300); $('help-open').click(); await sleep(500);",
    "help_capture": "$('gear').click(); await sleep(300); $('help-open').click(); await sleep(500);"
                    "document.querySelectorAll('#help-tabs-bar .expert-tab')[2].click(); await sleep(400);"
                    "const hb = $('help-body'); const gt = [...hb.querySelectorAll('.expert-group-title')].find((e) => /Screen recording|Bildschirmaufnahme/.test(e.textContent));"
                    "if (gt) hb.scrollTop = gt.offsetTop - hb.offsetTop; await sleep(300);",
    "backup": "$('gear').click(); await sleep(300); $('settings-backup-btn').click(); await sleep(600);",
    "voicepop": "$('voice-btn').dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true })); await sleep(400);",
    "stats": "$('gear').click(); await sleep(300); $('settings-stats-btn').click(); await sleep(600);",
    # The statistics dialog on each period pill (the default, 30 days, is "stats").
    **{f"stats_{k}": "$('gear').click(); await sleep(300); $('settings-stats-btn').click(); await sleep(600);"
                  f"document.querySelector('#stats-panel [data-period=\"{k}\"]').click(); await sleep(600);" for k in ("7", "365", "all")},
    "stats_off": "$('gear').click(); await sleep(300); $('expert-open').click(); await sleep(400);"
                 "const sw = () => document.querySelector('#settings-expert [data-setting-row=usageStats] input');"
                 "for (const tab of document.querySelectorAll('#expert-tabs-bar .expert-tab')) { if (sw()) break; tab.click(); await sleep(150); }"
                 "sw().click(); await sleep(300);"
                 "$('expert-back').click(); await sleep(300); $('settings-stats-btn').click(); await sleep(600);",
    "recresult": "window.__FIRE__('recording-captured', { path: 'C:/fake/raw.mp4' }); await sleep(900);",
    "snipedit": SNIP_OPEN,
    "snip_edit_ocr": SNIP_OCR_OPEN,
    "snipdraw": SNIP_OPEN + "document.querySelector('#snip-edit-tools [data-tool=\"rect\"]')?.click(); await sleep(300);"
                "const dv = $('snip-edit-live'); const dr = dv.getBoundingClientRect();"
                "const dp = (t, x, y, el) => (el || window).dispatchEvent(new PointerEvent(t, { bubbles: true, button: 0, pointerId: 1, clientX: dr.left + x, clientY: dr.top + y }));"
                "dp('pointerdown', 120, 80, dv); dp('pointermove', 260, 150, dv); dp('pointerup', 260, 150); await sleep(300);",
    # A drawn rectangle selected again by a click on its border: the handles around it.
    "snipsel": SNIP_OPEN + "document.querySelector('#snip-edit-tools [data-tool=\"rect\"]')?.click(); await sleep(300);"
               "const sv = $('snip-edit-live'); const sr = sv.getBoundingClientRect();"
               "const sp = (t, x, y, el) => (el || window).dispatchEvent(new PointerEvent(t, { bubbles: true, button: 0, pointerId: 1, clientX: sr.left + x, clientY: sr.top + y }));"
               "sp('pointerdown', 120, 80, sv); sp('pointermove', 260, 150, sv); sp('pointerup', 260, 150); await sleep(200);"
               "sp('pointerdown', 120, 110, sv); sp('pointerup', 120, 110); await sleep(300);",
    "snipcrop": SNIP_OPEN + "document.querySelector('#snip-edit-tools [data-tool=\"crop\"]')?.click(); await sleep(400);",
    "sniptext": SNIP_OPEN + "document.querySelector('#snip-edit-tools [data-tool=\"text\"]')?.click(); await sleep(300);"
                "const lv = $('snip-edit-live'); const lr = lv.getBoundingClientRect();"
                "lv.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0, pointerId: 1, clientX: lr.left + 200, clientY: lr.top + 120 }));"
                "window.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, button: 0, pointerId: 1, clientX: lr.left + 200, clientY: lr.top + 120 }));"
                "await sleep(200); const ti = $('snip-edit-text-input'); ti.value = 'Hi'; ti.dispatchEvent(new Event('input', { bubbles: true })); await sleep(200);",
    # A placed text, committed with Enter: selected, with a frame around it and its values in the open panel.
    "sniptextsel": SNIP_OPEN + "document.querySelector('#snip-edit-tools [data-tool=\"text\"]')?.click(); await sleep(300);"
                   "const xv = $('snip-edit-live'); const xr = xv.getBoundingClientRect();"
                   "xv.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0, pointerId: 1, clientX: xr.left + 200, clientY: xr.top + 120 }));"
                   "window.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, button: 0, pointerId: 1, clientX: xr.left + 200, clientY: xr.top + 120 }));"
                   "await sleep(200); const xi = $('snip-edit-text-input'); xi.value = 'Hi there'; xi.dispatchEvent(new Event('input', { bubbles: true }));"
                   "xi.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true })); await sleep(200);"
                   "document.querySelector('#snip-edit-tools [data-tool=\"text\"]')?.click(); await sleep(300);",
    # The composer with a 12 line prompt typed in: the tool buttons stack into rows beside the tall box.
    "composer_tall": "const core = await import('./core.js'); core.inputEl.value = Array.from({ length: 12 }, (_, i) => 'Line ' + (i + 1) + ' of a long prompt').join('\\n');"
                    "core.autoGrow(core.inputEl); await sleep(300);",
}
# Area groups, so a review of one area shoots only its surfaces.
GROUPS = {
    "@settings": ["settings", "settings_end", "expert", "expert_end", "expert_system", "expert_t0", "expert_t1", "expert_t2", "expert_t3", "expert_t4", "expert_t5", "help", "help_capture", "stats", "stats_7", "stats_365", "stats_all", "stats_off", "backup"],
    "@library": ["library", "library_pins", "history"],
    "@tiles": ["main", "tilemenu", "movefolder", "folderpick", "editor", "editor_long", "editor_tags", "editor_tags_all", "editor_adv", "editor_rotate", "folder", "folders_mix", "folders_mix_small", "voicepop"],
    "@recording": ["recresult"],
    "@manager": ["manager.html:recs", "manager.html:recs_sel", "manager.html:shots", "manager.html:shots_off"],
    "manager_shots": ["manager.html:shots"],
    "@snip": ["snipedit", "snip_edit_ocr", "snipdraw", "snipsel", "snipcrop", "sniptext", "sniptextsel"],
    "@composer": ["main", "composer_tall"],
}


class Handler(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *a, **k):
        super().__init__(*a, directory=str(UI), **k)

    def do_GET(self):
        path, _, query = self.path.partition("?")
        q = dict(p.split("=", 1) for p in query.split("&") if "=" in p)
        if path == "/tauri_stub.js":
            return self._send(STUB.read_bytes(), "text/javascript")
        if path == "/__frame.html":
            # The page runs in an iframe of exactly the shot size. A headless window is
            # smaller than its screenshot while the page loads, so a script that measures
            # the viewport (a menu placed at a point, a text fitted to its box) would
            # otherwise work on the wrong size.
            w, h = int(q.get("w", 880)), int(q.get("h", 600))
            src = q.get("src", "index.html").replace("%3F", "?").replace("%26", "&").replace("%3D", "=")
            page = (f'<!doctype html><meta charset="utf-8"><style>html,body{{margin:0;overflow:hidden;background:#000}}'
                    f'iframe{{border:0;display:block;width:{w}px;height:{h}px}}</style><iframe src="/{src}"></iframe>')
            return self._send(page.encode("utf-8"), "text/html; charset=utf-8")
        page = UI / path.lstrip("/")
        if path.endswith(".html") and page.exists():
            lang = q.get("lang", "")
            head = '<script src="tauri_stub.js"></script>\n'
            if lang.isalpha():
                head += f'<script>SETTINGS.language = "{lang}";</script>\n'
            theme = q.get("theme", "")
            if theme.isalpha():
                head += f'<script>SETTINGS.theme = "{theme}"; window.__SHOT_THEME__ = "{theme}";</script>\n'
            # A headless shot freezes colour transitions part way (the theme fades in from the
            # light default), which paints the whole page washed out; shoot the settled state.
            head += '<style>*, *::before, *::after { transition: none !important; animation: none !important; }</style>\n'
            html = page.read_text(encoding="utf-8").replace("<script", head + "<script", 1)
            act = ACTS.get(q.get("act", ""), "")
            html = html.replace("</body>", f"<script type=module>{PRE}{act}</script></body>")
            return self._send(html.encode("utf-8"), "text/html; charset=utf-8")
        return super().do_GET()

    def _send(self, body, ctype):
        self.send_response(200)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *a):
        pass


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--size", default="880x600")
    ap.add_argument("--lang", default="de")
    ap.add_argument("--theme", default="dark")
    ap.add_argument("--scale", default="1")
    ap.add_argument("--out", default=str(REPO / ".tmp" / "shots"))
    ap.add_argument("surfaces", nargs="*")
    args = ap.parse_args()
    sizes = args.size.split(",")
    langs = sorted(f.stem for f in (UI / "i18n").glob("*.js")) if args.lang == "all" else args.lang.split(",")
    themes = (re.search(r"BUILTIN_THEMES = \[([^\]]*)\]", (UI / "theme.js").read_text(encoding="utf-8"))
              .group(1).replace('"', "").replace(" ", "").split(",")) if args.theme == "all" else args.theme.split(",")
    scales = args.scale.split(",")
    chrome = next((c for c in CHROME_CANDIDATES if Path(c).exists()), None)
    if not chrome:
        print("no Chrome or Edge found")
        return 1
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    port = server.server_address[1]
    out = Path(args.out).resolve()
    out.mkdir(parents=True, exist_ok=True)
    picked = [x for s in args.surfaces for x in GROUPS.get(s, [s])] if args.surfaces else list(ACTS)
    for s, size, lang, theme, scale in itertools.product(dict.fromkeys(picked), sizes, langs, themes, scales):
        w, h = size.split("x")
        if ":" in s and s.split(":", 1)[0].endswith(".html"):
            page, act = s.split(":", 1)
        else:
            page, act = (s, "main") if s.endswith(".html") else ("index.html", s)
        # The default theme and scale keep the short file name the tool always wrote.
        extra = ("" if theme == "dark" else f"_{theme}") + ("" if scale == "1" else f"_x{scale}")
        png = out / f"{Path(page).stem}_{act}_{lang}_{w}x{h}{extra}.png"
        subprocess.run([chrome, "--headless=new", "--disable-gpu",
                        f"--force-device-scale-factor={scale}", f"--user-data-dir={out / 'profile'}",
                        f"--window-size={w},{h}", "--virtual-time-budget=6000", f"--screenshot={png}",
                        f"http://127.0.0.1:{port}/__frame.html?w={w}&h={h}&src={page}%3Flang%3D{lang}%26theme%3D{theme}%26act%3D{act}"],
                       capture_output=True, timeout=90)
        print(png)
    server.shutdown()
    return 0


if __name__ == "__main__":
    sys.exit(main())
