"""Headless smoke test: boot the real UI with a stubbed Tauri backend.

Catches the class of defect that only shows when the app actually runs:
  * a JS error during startup or while opening a dialog,
  * an element that overflows the window horizontally (layout bug),
  * a control parked outside the window, covered, or crushed to no height,
  * a string that renders as its raw i18n key (missing translation).

It walks the library, the copy history, the tile menu and every expert tab, down
to the smallest window the app itself allows, and once more with the appearance
scales raised. Two extra boots cover the store-sealing warning in both
directions, since it only ever appears during startup. Skips cleanly when no
Chromium browser is installed.

The suites live in the smoke/ folder next to this file: main_walk, side_windows,
video, boots, video_expert (video.run_expert) and findings.

Run:  python tests/ui/smoke_test.py [--only SUITE]...
Exit: 0 = clean, 1 = problems found.
"""
import argparse
import os
import shutil
import sys
import tempfile
from pathlib import Path

from smoke import boots, findings, main_walk, side_windows, video
from smoke.harness import REPO, HERE, CHROME_CANDIDATES, install_manifest_recorder

# Run order: the single boots come before the video expert boot, the generated findings last.
SUITES = (
    ("main_walk", main_walk.run),
    ("side_windows", side_windows.run),
    ("video", video.run),
    ("boots", boots.run),
    ("video_expert", video.run_expert),
    ("findings", findings.run),
)


def main():
    ap = argparse.ArgumentParser(description="Headless smoke test of the real UI.")
    ap.add_argument("--only", action="append", choices=[name for name, _ in SUITES],
                    help="run only this suite (repeatable); default is every suite")
    only = ap.parse_args().only

    browser = next((p for p in CHROME_CANDIDATES if Path(p).exists()), None)
    if not browser:
        # CI's runner image always ships Chrome (see ci.yml); a missing browser there
        # means the whole UI-smoke job silently stopped covering anything, which must
        # fail loudly instead of passing green with zero coverage.
        if os.environ.get("CI"):
            print("FAIL: no Chromium browser found on a CI runner")
            return 1
        print("SKIP: no Chromium browser found")
        return 0

    tmp = Path(tempfile.mkdtemp(prefix="ps-smoke-"))
    install_manifest_recorder(tmp)
    failed = 0
    try:
        shutil.copytree(REPO / "ui", tmp / "ui")
        shutil.copy(HERE / "tauri_stub.js", tmp / "ui" / "tauri_stub.js")
        for name, run in SUITES:
            if only and name not in only:
                continue
            failed += run(browser, tmp)
        print("PASS" if not failed else f"{failed} problem(s)")
        return 1 if failed else 0
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
