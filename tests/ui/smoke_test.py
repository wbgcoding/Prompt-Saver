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
import re
import shutil
import sys
import tempfile
from pathlib import Path

from smoke import boots, findings, logic, main_walk, side_windows, video
from smoke.harness import REPO, HERE, CHROME_CANDIDATES, install_manifest_recorder

# Run order: the single boots come before the video expert boot, the generated findings last.
SUITES = (
    ("main_walk", main_walk.run),
    ("side_windows", side_windows.run),
    ("video", video.run),
    ("boots", boots.run),
    ("logic", logic.run),
    ("video_expert", video.run_expert),
    ("findings", findings.run),
)

# Ids reused before probes had to be unique. New probes take a fresh id.
LEGACY_DUP_IDS = {
    "17": 2, "21": 2, "28": 2, "32": 2, "33": 3, "43": 2, "58": 2, "67": 2, "70": 4, "76": 2,
    "79": 2, "81": 2, "108": 2, "125": 2, "171": 2, "181": 2, "188": 2, "230": 8, "275": 2,
    "277": 2, "339": 2, "343": 2, "363": 2, "365": 2, "389": 2, "405": 2, "414": 3, "425": 3,
    "427": 3, "429": 2, "434": 7, "456": 3, "500": 2, "508": 4, "510": 3, "516": 4, "521": 2,
    "677": 2,
}

PROBE_ID_RE = re.compile(r"^\s*// ([0-9]+[A-Za-z]?(?:-[A-Za-z0-9]+)*):", re.M)


def check_probe_ids():
    """A probe block starts with "// <id>:" and an id belongs to one block only.
    Returns the number of ids used more often than allowed."""
    counts = {}
    for f in sorted((HERE / "smoke").rglob("*.py")):
        for m in PROBE_ID_RE.finditer(f.read_text(encoding="utf-8")):
            counts[m.group(1)] = counts.get(m.group(1), 0) + 1
    bad = 0
    for pid, n in sorted(counts.items()):
        if n > LEGACY_DUP_IDS.get(pid, 1):
            print(f"FAIL probe id {pid} used {n} times")
            bad += 1
    return bad


def main():
    ap = argparse.ArgumentParser(description="Headless smoke test of the real UI.")
    ap.add_argument("--only", action="append", choices=[name for name, _ in SUITES],
                    help="run only this suite (repeatable); default is every suite")
    only = ap.parse_args().only

    id_problems = check_probe_ids()
    browser = next((p for p in CHROME_CANDIDATES if Path(p).exists()), None)
    if not browser:
        # CI's runner image always ships Chrome (see ci.yml); a missing browser there
        # means the whole UI-smoke job silently stopped covering anything, which must
        # fail loudly instead of passing green with zero coverage.
        if os.environ.get("CI"):
            print("FAIL: no Chromium browser found on a CI runner")
            return 1
        print("SKIP: no Chromium browser found")
        return 1 if id_problems else 0

    tmp = Path(tempfile.mkdtemp(prefix="ps-smoke-"))
    install_manifest_recorder(tmp)
    failed = id_problems
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
