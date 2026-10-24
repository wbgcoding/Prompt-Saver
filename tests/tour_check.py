"""Introduction tour (ui/tourContent.js) consistency check.

Every step names a title key and up to three text keys; nothing catches a step that
points at a translation that does not exist in one of the languages -- the tour would
just render a raw i18n key, invisible until someone opens it in that language. The step
count must also equal the number of drawing steps (there is no extra page after them).

    python tests/tour_check.py

Exit code 1 on a missing key, a step with more than three lines, or a step-count drift.
"""

import re
import sys
from pathlib import Path

from i18n_files import LANGS

ROOT = Path(__file__).resolve().parent.parent


def lang_keys(code):
    text = (ROOT / "ui" / "i18n" / f"{code}.js").read_text(encoding="utf-8")
    return set(re.findall(r"^\s+([A-Za-z_$][\w$]*):", text, flags=re.M))


def main():
    src = (ROOT / "ui" / "tourContent.js").read_text(encoding="utf-8")
    block = src[src.index("export const TOUR_STEPS = ["):]
    block = block[: block.index("];")]
    steps = []
    for m in re.finditer(r"titleKey:\s*\"([^\"]+)\",\s*lineKeys:\s*\[([^\]]*)\]", block):
        steps.append((m.group(1), re.findall(r"\"([^\"]+)\"", m.group(2))))
    if not steps:
        print("TOUR_STEPS not found or empty in ui/tourContent.js")
        sys.exit(1)

    problems = []
    if not re.search(r"export const TOUR_STEP_COUNT = TOUR_STEPS\.length;", src):
        problems.append("TOUR_STEP_COUNT must equal TOUR_STEPS.length (no extra page after the drawing steps)")
    for title, lines in steps:
        if not 1 <= len(lines) <= 3:
            problems.append(f"step {title!r}: {len(lines)} text lines, expected 1 to 3")
    for code in LANGS:
        keys = lang_keys(code)
        for title, lines in steps:
            for k in [title] + lines:
                if k not in keys:
                    problems.append(f"{code}: tour key {k!r} is missing")

    if problems:
        for p in problems:
            print(p)
        sys.exit(1)
    print(f"TOUR_STEPS: {len(steps)} steps, all keys present in {len(LANGS)} languages. PASS")


if __name__ == "__main__":
    main()
