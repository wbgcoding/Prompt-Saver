"""The main window's JavaScript as one text, for the tests that scan its source.

The main window is split into several modules under ui/. The list lives in
tests/ui_sources.json (key main_window_modules, in concatenation order); a module
created for the main window is appended there. Tests read the code through here,
never by opening ui/main.js directly.
"""
import json
import subprocess
from pathlib import Path

TESTS = Path(__file__).resolve().parent
REPO = TESTS.parent
UI = REPO / "ui"

_SOURCES = json.loads((TESTS / "ui_sources.json").read_text(encoding="utf-8"))
MAIN_WINDOW_MODULES = _SOURCES["main_window_modules"]
# The video editor window: video.js plus the modules it is split into.
VIDEO_WINDOW_MODULES = _SOURCES["video_window_modules"]


def main_window_files():
    return [UI / name for name in MAIN_WINDOW_MODULES]


def video_window_source():
    return "\n".join((UI / name).read_text(encoding="utf-8") for name in VIDEO_WINDOW_MODULES)


def _git_show(rev, name):
    out = subprocess.run(
        ["git", "show", f"{rev}:ui/{name}"], cwd=REPO,
        capture_output=True, text=True, encoding="utf-8", errors="replace")
    return out.stdout if out.returncode == 0 else None


def main_window_source(rev=None):
    """The module texts joined with a newline. With rev, the texts at that git
    revision (modules that do not exist there are skipped); None when main.js
    itself does not exist at that revision."""
    if rev is None:
        return "\n".join(p.read_text(encoding="utf-8") for p in main_window_files())
    parts = []
    for name in MAIN_WINDOW_MODULES:
        text = _git_show(rev, name)
        if text is None:
            if name == "main.js":
                return None
            continue
        parts.append(text)
    return "\n".join(parts)
