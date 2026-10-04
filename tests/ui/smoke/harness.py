"""Shared setup for the smoke suites: paths, window sizes, the Chrome call, helpers."""
import hashlib
import json
import os
import subprocess
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
HERE = Path(__file__).resolve().parents[1]
# A dynamic import that was not preloaded never resolves under --virtual-time-budget,
# and the language passes switch the UI language at run time, so every language file
# is preloaded in the staged main page.
LANG_PRELOADS = "".join(
    f'<link rel="modulepreload" href="i18n/{c}.js" />'
    for c in ("en", "de", "es", "it", "pt", "pl", "zh", "ja", "nl", "tr", "ro"))

# The floor is the app's own minimum window size: read it, so this sweep follows
# tauri.conf.json instead of drifting away from it.
_WIN = json.loads((REPO / "src-tauri" / "tauri.conf.json").read_text(encoding="utf-8"))["app"]["windows"][0]
MIN_W, MIN_H = _WIN["minWidth"], _WIN["minHeight"]
SIZES = [(1280, 800), (1366, 768), (900, 650), (620, 500), (MIN_W + 60, MIN_H + 40), (MIN_W, MIN_H)]

# Headless Chrome refuses to make its window narrower than ~500px, so the page is
# loaded in an iframe of the exact size instead; the wrapper republishes the probe
# result because --dump-dom only prints the top document.
WRAPPER = """<!doctype html><meta charset="utf-8">
<style>html,body{margin:0}iframe{border:0;display:block}</style>
<iframe id="f" src="ui/index.html" style="width:%dpx;height:%dpx"></iframe><pre id="SMOKE"></pre>
<script>
setInterval(() => {
  try {
    const p = f.contentDocument.getElementById('SMOKE');
    const own = document.getElementById('SMOKE');
    if (p && !own.textContent) own.textContent = p.textContent;
  } catch (e) {}
}, 200);
</script>"""

CHROME_CANDIDATES = [
    r"C:\Program Files\Google\Chrome\Application\chrome.exe",
    r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
    r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
    r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
]

# The secondary windows are small by nature, so they get the same "nothing escapes the
# window" rule as the main one. Their own scroll containers are allowed to clip.
SIDE_SCAN = """
  const sideScan = (errs, where) => {
    for (const el of document.querySelectorAll('body *')) {
      const s = getComputedStyle(el);
      if (s.display === 'none' || s.visibility === 'hidden' || s.position === 'fixed' ||
          !el.getClientRects().length) continue;
      let clipped = false;
      for (let p = el.parentElement; p; p = p.parentElement) {
        const ps = getComputedStyle(p);
        if (ps.overflowX !== 'visible' || ps.overflowY !== 'visible') { clipped = true; break; }
      }
      if (clipped) continue;
      const r = el.getBoundingClientRect();
      if (!r.width || !r.height) continue;
      if (r.right > innerWidth + 1 || r.left < -1) {
        errs.push(where + ': ' + (el.id || el.className || el.tagName) + ' is outside the window (x ' +
          Math.round(r.left) + '..' + Math.round(r.right) + ' of ' + innerWidth + ')');
      }
    }
  };
"""


def install_manifest_recorder(tmp):
    """Refactor proof, off unless SMOKE_MANIFEST names a file: one line per Chrome call
    (masked args, timeout, hash of the staged temp tree). SMOKE_MANIFEST_DRY=1 skips
    the browser, so a run finishes in seconds."""
    target = os.environ.get("SMOKE_MANIFEST")
    if not target:
        return
    target = Path(target)
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text("", encoding="utf-8")
    dry = os.environ.get("SMOKE_MANIFEST_DRY") == "1"
    original = subprocess.run
    count = [0]

    def tree_hash():
        digest = hashlib.sha256()
        for f in sorted(p for p in tmp.rglob("*") if p.is_file()):
            rel = f.relative_to(tmp)
            if any(part.startswith("profile-") for part in rel.parts):
                continue
            digest.update(rel.as_posix().encode("utf-8") + b"\0" + f.read_bytes())
        return digest.hexdigest()

    def mask(arg):
        arg = str(arg)
        return arg.replace(tmp.as_uri(), "<TMP-URI>").replace(str(tmp), "<TMP>").replace(tmp.as_posix(), "<TMP>")

    def recorded(*args, **kwargs):
        argv = args[0] if args else kwargs.get("args")
        line = json.dumps({"i": count[0], "args": [mask(a) for a in argv],
                           "timeout": kwargs.get("timeout"), "tree": tree_hash()}, sort_keys=True)
        count[0] += 1
        with open(target, "a", encoding="utf-8", newline="\n") as fh:
            fh.write(line + "\n")
        if dry:
            return subprocess.CompletedProcess(argv, 0, stdout="", stderr="")
        return original(*args, **kwargs)

    subprocess.run = recorded


def unescape(s):
    for a, b in (("&quot;", '"'), ("&lt;", "<"), ("&gt;", ">"), ("&amp;", "&")):
        s = s.replace(a, b)
    return s


def run_chrome(browser, profile, window_size, budget_ms, uri, timeout):
    return subprocess.run(
        [browser, "--headless=new", "--disable-gpu", "--no-first-run",
         f"--user-data-dir={profile}", "--allow-file-access-from-files",
         f"--window-size={window_size}", f"--virtual-time-budget={budget_ms}",
         "--dump-dom", uri],
        capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=timeout)
