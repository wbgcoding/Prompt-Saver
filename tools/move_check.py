"""Proves that a staged change under ui/ only moved code.

Run:  python tools/move_check.py [--base REV] [--max N]

Stage the change first. The tool diffs the index against REV (default HEAD) for ui/,
normalises every removed and added line (strip, drop one leading "export "), and
compares them as multisets: every line that left a file must reappear as an added
line somewhere. Ignored on both sides: blank lines, import lines, link lines
(a name like linkSomething) and lines ending in "// linked from main.js".

Prints   move_check: removed=R added=A unmatched_removed=U unmatched_added=V
then every unmatched removed line with its old file and line number.
Exit 1 when U is greater than N (default 0), else 0.
"""
import argparse
import re
import subprocess
import sys
from collections import Counter
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
HUNK = re.compile(r"^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@")
LINK = re.compile(r"\blink[A-Z]\w*\b")


def normalise(line):
    text = line.strip()
    if text.startswith("export "):
        text = text[len("export "):].strip()
    if not text or text.startswith("import ") or LINK.search(text) or text.endswith("// linked from main.js"):
        return None
    return text


def staged_diff(base):
    out = subprocess.run(
        ["git", "diff", "--cached", "-U0", "--no-color", base, "--", "ui"], cwd=REPO,
        capture_output=True, text=True, encoding="utf-8", errors="replace")
    if out.returncode != 0:
        sys.exit("move_check: git diff failed: " + out.stderr.strip())
    return out.stdout


def parse(diff):
    removed, added = [], []
    old_file, old_no = "", 0
    for raw in diff.split("\n"):
        line = raw.rstrip("\r")
        if line.startswith("--- "):
            old_file = line[4:].removeprefix("a/")
            continue
        if line.startswith("+++ "):
            continue
        m = HUNK.match(line)
        if m:
            old_no = int(m.group(1))
            continue
        if line.startswith("-"):
            norm = normalise(line[1:])
            if norm is not None:
                removed.append((norm, old_file, old_no))
            old_no += 1
        elif line.startswith("+"):
            norm = normalise(line[1:])
            if norm is not None:
                added.append(norm)
    return removed, added


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--base", default="HEAD")
    ap.add_argument("--max", type=int, default=0)
    args = ap.parse_args()
    removed, added = parse(staged_diff(args.base))
    pool = Counter(added)
    unmatched_removed = []
    for norm, name, no in removed:
        if pool[norm] > 0:
            pool[norm] -= 1
        else:
            unmatched_removed.append((norm, name, no))
    unmatched_added = sum(pool.values())
    print(f"move_check: removed={len(removed)} added={len(added)} "
          f"unmatched_removed={len(unmatched_removed)} unmatched_added={unmatched_added}")
    for norm, name, no in unmatched_removed:
        print(f"  {name}:{no}: {norm}")
    return 1 if len(unmatched_removed) > args.max else 0


if __name__ == "__main__":
    sys.exit(main())
