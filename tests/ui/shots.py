"""Full-resolution screenshots of the real UI against the test stub, for a visual review.

    python tests/ui/shots.py [--size 880x600] [--lang de] [--theme dark] [--scale 1] [--out .tmp/shots] [surface ...]

--size, --lang, --theme and --scale take a comma list and shoot every combination;
`--lang all` and `--theme all` mean every UI language and every built-in theme.

A surface is `main`, `settings`, `settings_end`, `expert`, `library`, `history`,
`tilemenu`, `editor`, `folder`, `help`, `backup`, `snipedit`, `snipdraw`, or another
page such as `video.html`.
A group (`@settings`, `@library`, `@tiles`, `@snip`) shoots its surfaces.
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
  const b = [...document.querySelectorAll('#confirm-modal:not(.hidden) button')].find((x) => /leer|empty/i.test(x.textContent));
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
ACTS = {
    "main": "",
    "settings": "$('gear').click(); await sleep(300);",
    "settings_end": "$('gear').click(); await sleep(300); $('settings-main').lastElementChild.scrollIntoView(); await sleep(100);",
    "expert": "$('gear').click(); await sleep(300); $('expert-open').click(); await sleep(400);",
    "library": "$('library-btn').click(); await sleep(400);",
    "history": "$('library-btn').click(); await sleep(400); $('library-tab-history').click(); await sleep(400);",
    "tilemenu": TILE_MENU,
    "editor": TILE_MENU + "document.querySelector('#ctx [data-act=\"edit\"]')?.click(); await sleep(500);",
    "folder": ("const t = document.querySelector('#grid .folder-tile');"
               "t.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0, clientX: 5, clientY: 5 }));"
               "window.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, button: 0, clientX: 5, clientY: 5 }));"
               "await sleep(600);"),
    "help": "$('gear').click(); await sleep(300); $('help-open').click(); await sleep(500);",
    "backup": "$('gear').click(); await sleep(300); $('settings-backup-btn').click(); await sleep(600);",
    "snipedit": SNIP_OPEN,
    "snipdraw": SNIP_OPEN + "document.querySelector('#snip-edit-tools [data-tool=\"rect\"]')?.click(); await sleep(300);",
}
# Area groups, so a review of one area shoots only its surfaces.
GROUPS = {
    "@settings": ["settings", "settings_end", "expert", "help"],
    "@library": ["library", "history"],
    "@tiles": ["main", "tilemenu", "editor", "folder"],
    "@snip": ["snipedit", "snipdraw"],
}


class Handler(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *a, **k):
        super().__init__(*a, directory=str(UI), **k)

    def do_GET(self):
        path, _, query = self.path.partition("?")
        q = dict(p.split("=", 1) for p in query.split("&") if "=" in p)
        if path == "/tauri_stub.js":
            return self._send(STUB.read_bytes(), "text/javascript")
        page = UI / path.lstrip("/")
        if path.endswith(".html") and page.exists():
            lang = q.get("lang", "")
            head = '<script src="tauri_stub.js"></script>\n'
            if lang.isalpha():
                head += f'<script>SETTINGS.language = "{lang}";</script>\n'
            theme = q.get("theme", "")
            if theme.isalpha():
                head += f'<script>SETTINGS.theme = "{theme}"; window.__SHOT_THEME__ = "{theme}";</script>\n'
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
        page, act = (s, "main") if s.endswith(".html") else ("index.html", s)
        # The default theme and scale keep the short file name the tool always wrote.
        extra = ("" if theme == "dark" else f"_{theme}") + ("" if scale == "1" else f"_x{scale}")
        png = out / f"{Path(page).stem}_{act}_{lang}_{w}x{h}{extra}.png"
        subprocess.run([chrome, "--headless=new", "--disable-gpu", "--hide-scrollbars",
                        f"--force-device-scale-factor={scale}", f"--user-data-dir={out / 'profile'}",
                        f"--window-size={w},{h}", "--virtual-time-budget=6000", f"--screenshot={png}",
                        f"http://127.0.0.1:{port}/{page}?lang={lang}&theme={theme}&act={act}"],
                       capture_output=True, timeout=90)
        print(png)
    server.shutdown()
    return 0


if __name__ == "__main__":
    sys.exit(main())
