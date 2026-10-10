"""413's own hard rule, made mechanical: no existing settings row may change
which expert-menu group (and therefore which tab) it lives in, and the
backup window's sections must stay in the order backups, storage, retention
(the usage statistics have their own dialog). New rows are fine; a MOVED or
REMOVED one is not.

Compares the current `ui/main.js` / `ui/index.html` against the git HEAD
versions (the tree as it stood before this round's retention-overview work
started) rather than a hand-maintained snapshot, so the test never goes stale
on its own.

Run:  python tests/layout/expert_menu_no_field_moved_test.py
Exit: 0 = nothing moved, 1 = a field's group changed or disappeared.
"""
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "tests"))
from ui_sources import main_window_source  # noqa: E402
INDEX = REPO / "ui" / "index.html"

KINDS = ("flags", "values", "selects", "dropdowns", "paths")


def balanced(text, start, opener, closer):
    depth, j = 0, start
    while True:
        if text[j] == opener:
            depth += 1
        elif text[j] == closer:
            depth -= 1
            if depth == 0:
                return text[start:j + 1]
        j += 1


def strip_comments(src):
    src = re.sub(r"/\*.*?\*/", "", src, flags=re.S)
    return "\n".join(l for l in src.splitlines() if not l.lstrip().startswith("//"))


def group_membership(src):
    """(kind, key) -> group title, for every key EXPERT_TABS lists."""
    src = strip_comments(src)
    i = src.index("const EXPERT_TABS")
    tabs = balanced(src, src.index("[", i), "[", "]")
    membership = {}
    for m in re.finditer(r'\{\s*title:\s*"(expGroup\w+)"', tabs):
        title = m.group(1)
        text = balanced(tabs, m.start(), "{", "}")
        for kind in KINDS:
            km = re.search(kind + r':\s*\[', text)
            if not km:
                continue
            lst = balanced(text, km.end() - 1, "[", "]")
            for entry in re.finditer(r'\{\s*key:\s*"([^"]+)"[^}]*\}|"([^"]+)"', lst):
                key = entry.group(1) or entry.group(2)
                membership[(kind, key)] = title
    return membership


def panel_order(html):
    ids = re.findall(r'id="(storage-panel|retention-panel|backup-panel)"', html)
    return ids


def main():
    problems = []

    current_main = main_window_source()
    baseline_main = main_window_source("HEAD")
    if baseline_main is None:
        print("SKIP: no git HEAD version of ui/main.js to compare against")
        return 0

    baseline = group_membership(baseline_main)
    current = group_membership(current_main)

    for (kind, key), old_group in baseline.items():
        new_group = current.get((kind, key))
        if new_group is None:
            problems.append(f"{kind} '{key}' existed in group {old_group} at HEAD and is now gone entirely")
        elif new_group != old_group:
            problems.append(f"{kind} '{key}' moved from group {old_group} to {new_group}")

    current_html = INDEX.read_text(encoding="utf-8")
    order = panel_order(current_html)
    if order != ["backup-panel", "storage-panel", "retention-panel"]:
        problems.append(f"the backup window's panels are not backups, storage, retention: {order}")
    if 'id="stats-panel"' not in current_html[current_html.index('id="stats-modal"'):]:
        problems.append("the usage statistics panel is not inside the statistics dialog")

    added = sorted(k for k in current if k not in baseline)
    print(f"expert-menu keys at HEAD: {len(baseline)}, now: {len(current)} ({len(added)} added)")
    for kind, key in added:
        print(f"   + {kind} '{key}' -> {current[(kind, key)]}")

    if problems:
        for p in problems:
            print(f"FAIL: {p}")
        print(f"{len(problems)} problem(s)")
        return 1
    print("PASS: every pre-existing field kept its group, panel order unchanged")
    return 0


if __name__ == "__main__":
    sys.exit(main())
