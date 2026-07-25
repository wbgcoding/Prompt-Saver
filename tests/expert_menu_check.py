"""Consistency check for the expert menu (ui/main.js).

Catches the mistakes that are invisible until a user opens the menu:
  * a tab references a flag/value/select/dropdown that does not exist,
  * the same setting listed in two groups (renders twice),
  * a value whose `gate:` points at a flag that does not exist,
  * a setting that exists but is not reachable from any tab.

Run:  python tests/expert_menu_check.py
Exit: 0 = clean, 1 = problems found.
"""
import re
import sys
from pathlib import Path

MAIN = Path(__file__).resolve().parents[1] / "ui" / "main.js"
SRC = MAIN.read_text(encoding="utf-8")
# Comments carry words like "// Duplicate finder: threshold" that the key scanner
# would otherwise read as setting names.
SRC = re.sub(r"/\*.*?\*/", "", SRC, flags=re.S)
SRC = "\n".join(l for l in SRC.splitlines() if not l.lstrip().startswith("//"))

# Settings deliberately absent from the tab list: normal settings in the settings
# dialog, and the ones the backups dialog renders itself (renderBackupPanel /
# renderStatsPanel, opened from settings — not from the expert menu).
NOT_IN_EXPERT = {
    "autoPaste",
    # Backup settings, rendered by renderBackupPanel.
    "autoBackup", "backupGfs", "backupKeep", "backupIntervalH",
    "backupDaily", "backupWeekly", "backupMonthly",
    # Statistics, rendered by renderStatsPanel.
    "usageStats", "statsTopN",
}


def block(name, opener, closer):
    i = SRC.index(name)
    start = SRC.index(opener, i)
    depth, j = 0, start
    while True:
        if SRC[j] == opener:
            depth += 1
        elif SRC[j] == closer:
            depth -= 1
            if depth == 0:
                return SRC[start:j + 1]
        j += 1


def top_keys(text):
    """Keys of an object literal at nesting depth 1."""
    keys, depth, i = [], 0, 0
    while i < len(text):
        c = text[i]
        if c in "{[":
            depth += 1
        elif c in "}]":
            depth -= 1
        elif depth == 1:
            m = re.match(r'\s*([A-Za-z_$][\w$]*)\s*:', text[i:])
            if m:
                keys.append(m.group(1))
                i += m.end() - 1
        i += 1
    return keys


flags = set(top_keys(block("const FLAG_LABELS", "{", "}")))
opt_flags = set(top_keys(block("const OPT_FLAG_LABELS", "{", "}")))
values_src = block("const EXPERT_VALUES", "{", "}")
values = set(top_keys(values_src))
selects = set(top_keys(block("const EXPERT_SELECTS", "{", "}")))
dropdowns = set(top_keys(block("const EXPERT_DROPDOWNS", "{", "}")))

tabs = block("const EXPERT_TABS", "[", "]")
used = {}
for kind in ("flags", "values", "selects", "dropdowns", "paths"):
    for lst in re.findall(kind + r':\s*\[(.*?)\]', tabs, re.S):
        for key in re.findall(r'"([^"]+)"', lst):
            used.setdefault(kind, []).append(key)

problems = []

for kind, known in (("flags", flags | opt_flags), ("values", values),
                    ("selects", selects), ("dropdowns", dropdowns)):
    seen = set()
    for key in used.get(kind, []):
        if key not in known:
            problems.append(f"{kind}: '{key}' is in a tab but not defined")
        if key in seen:
            problems.append(f"{kind}: '{key}' is listed in more than one group")
        seen.add(key)
    missing = known - seen - NOT_IN_EXPERT
    for key in sorted(missing):
        problems.append(f"{kind}: '{key}' is defined but unreachable in the menu")

# The "(default off)" suffix is appended at render time from the `defaultOff`
# string, so no label may carry it inline — that would double up or, worse, stick
# to a feature that is on by default.
i18n = (Path(__file__).resolve().parents[1] / "ui" / "i18n.js").read_text(encoding="utf-8")
for line in i18n.splitlines():
    if "(Standard Aus)" in line and not line.lstrip().startswith("defaultOff:"):
        problems.append(f"i18n: label carries the default-off suffix inline: {line.strip()[:70]}")

# Gates must point at a real flag (an "!" prefix inverts it).
for gate in re.findall(r'gate:\s*"([^"]+)"', values_src):
    name = gate.lstrip("!")
    if name not in flags and name not in opt_flags:
        problems.append(f"gate: '{gate}' points at an unknown flag")

for p in problems:
    print("FAIL", p)
print("PASS" if not problems else f"{len(problems)} problem(s)")
sys.exit(1 if problems else 0)
