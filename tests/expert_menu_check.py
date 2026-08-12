"""Consistency check for the expert menu (ui/main.js).

Catches the mistakes that are invisible until a user opens the menu:
  * a tab references a flag/value/select/dropdown that does not exist,
  * the same setting listed in two groups (renders twice),
  * a `gate:` that points at a flag that does not exist,
  * a setting that exists but is not reachable from any tab,
  * a setting that depends on another feature but carries no gate — the rule is
    absolute: nothing dependent may stay on screen while its parent is off,
  * a gate whose parent flag lives in a different group (it would render far away
    from the toggle it belongs to, or not at all).

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
    "backupDaily", "backupWeekly", "backupMonthly", "backupYearly",
    # Statistics, rendered by renderStatsPanel.
    "usageStats", "statsTopN",
}

# Every setting that only makes sense while another feature is on, and the gate it
# must carry ("!x" = only while x is OFF). Maintained by hand on purpose: whether one
# feature needs another is a product decision, not something text matching can guess.
# Adding a dependent setting without adding it here (and gating it) is the mistake
# this file exists to catch — so add both together.
DEPENDENCIES = {
    # Creating prompts
    "storeFiles": "fileAttach",
    "cleanupFiles": "storeFiles",
    "screenshotDir": "screenshot",
    "snipPreviewVh": "screenshot",
    "varDefaults": "promptVars",
    "dynVars": "promptVars",
    "choiceVars": "promptVars",
    "choiceRemember": "choiceVars",
    "varPopupWindow": "promptVars",
    "dateStyle": "dynVars",
    "timeStyle": "dynVars",
    "counterUnits": "lengthCounter",
    "historyPerPrompt": "promptHistory",
    "historyDiff": "promptHistory",
    "diffWordCap": "historyDiff",
    "historyPreviewLen": "promptHistory",
    "notesInTooltip": "promptNotes",
    "notesTipLen": "notesInTooltip",
    "pickerFormat": "colorPicker",
    "loupeZoom": "colorPicker",
    "pickSwatchPx": "colorPicker",
    "loupeCells": "colorPicker",
    "snipQuality": "screenshot",
    "snipDelayMs": "screenshot",
    # 32: rotating prompts
    "rotateDefaultMode": "rotatingPrompts",
    "rotateMaxEntries": "rotatingPrompts",
    "rotateBadge": "rotatingPrompts",
    "rotateTipLen": "rotatingPrompts",
    # Markdown preview
    "codeHighlight": "mdPreview",
    "codeAutoDetect": "mdPreview",
    "hlFontPx": "codeHighlight",
    "mdMaxChars": "mdPreview",
    "hlMaxChars": "codeHighlight",
    # Workspace
    "floatOpacity": "floating",
    "pillFolders": "floating",
    "pillMergeRadius": "pillFolders",
    "folderGridCols": "pillFolders",
    "tileFolderZone": "tileFolders",
    "tileFolderHoverMs": "tileFolders",
    "tileFolderCols": "tileFolders",
    "kbFocusFadeMs": "keyboardNav",
    "trayRecentN": "trayRecent",
    "trayFavN": "trayFavs",
    "checkUpdateOnStart": "showUpdates",
    # Tiles + copying
    "chainLock": "chainPrompts",
    "autoPasteEnter": "autoPaste",
    "autoPasteDelayMs": "autoPaste",
    "dblClickMs": "autoPaste",
    "promptHotkeyPaste": "promptHotkeys",
    "dragOutModifier": "dragOut",
    "dragOutInvert": "dragOut",
    "tooltipWidth": "tilePreview",
    "tooltipDelayMs": "tooltipDelay",
    "tooltipTimeoutMs": "tooltipTimeout",
    "tileHoverLift": "tileHover",
    "hoverLift": "tileHoverLift",
    "favViewButton": "favorites",
    "favViewReorder": "favViewButton",
    "favStarSize": "favViewButton",
    "favMaxCols": "favViewButton",
    # Appearance
    "animSpeed": "animations",
    "accentScrollbar": "!hideScrollbars",
    "scrollbarWidth": "!hideScrollbars",
    "frostedBlur": "frostedModals",
    "customThemeMax": "customThemes",
    # Library, history, privacy
    "searchRecentMax": "searchSuggest",
    "searchDebounceMs": "librarySearch",
    "fuzzyMaxTypos": "fuzzySearch",
    "libraryCols": "librarySearch",
    "tagChipPx": "tags",
    "tagFilterMode": "tags",
    "tagPopCols": "tags",
    "tagSort": "tags",
    "historyMax": "copyHistory",
    "journalLimit": "copyHistory",
    "journalWidth": "copyHistory",
    "historyDays": "copyHistory",
    "tileBlur": "blurTilesUntilHover",
    "mediaBlur": "blurMediaUntilHover",
    "dimOpacity": "dimUnhovered",
    # Media
    "videoVolume": "!videoMuted",
    "mediaDimOpacity": "dimMedia",
    "mediaRadius": "roundMedia",
    # Tools + data
    "viewGroups": "multiView",
    "viewBorder": "multiView",
    "viewGroupMaxRows": "viewGroups",
    "dupImportCheck": "dupFinder",
    "dupThreshold": "dupFinder",
    "clipMinChars": "clipWatcher",
    "clipInboxMax": "clipWatcher",
    # Backups + statistics (rendered outside the expert menu, gate still required)
    "backupKeep": "autoBackup",
    "backupIntervalH": "autoBackup",
    "backupDaily": "backupGfs",
    "backupWeekly": "backupGfs",
    "backupMonthly": "backupGfs",
    "backupYearly": "backupGfs",
    "statsTopN": "usageStats",
    "updateCheckH": "showUpdates",
}


def block(name, opener, closer, src=None):
    text = SRC if src is None else src
    i = text.index(name)
    return balanced(text, text.index(opener, i), opener, closer)


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


def gates_of(src):
    """key -> gate for every entry of a definition object that declares one."""
    out = {}
    for key in top_keys(src):
        body = balanced(src, src.index(key + ":", 0) + len(key) + 1, "{", "}") \
            if re.search(re.escape(key) + r':\s*\{', src) else ""
        m = re.search(r'\bgate:\s*"([^"]+)"', body)
        if m:
            out[key] = m.group(1)
    return out


flags = set(top_keys(block("const FLAG_LABELS", "{", "}")))
opt_flags = set(top_keys(block("const OPT_FLAG_LABELS", "{", "}")))
all_flags = flags | opt_flags
values_src = block("const EXPERT_VALUES", "{", "}")
values = set(top_keys(values_src))
selects_src = block("const EXPERT_SELECTS", "{", "}")
selects = set(top_keys(selects_src))
dropdowns_src = block("const EXPERT_DROPDOWNS", "{", "}")
dropdowns = set(top_keys(dropdowns_src))
paths_src = block("const EXPERT_PATHS", "{", "}")

problems = []

# ---- gates declared per setting kind ------------------------------------------
# Values/selects/paths spell `gate:` inside their own definition; the enum dropdowns
# are built by enumDropdown(key, label, opts, default, gate) and pass it positionally.
declared = {}
declared.update(gates_of(values_src))
declared.update(gates_of(selects_src))
declared.update(gates_of(paths_src))
declared.update(gates_of(dropdowns_src))
for m in re.finditer(r'(\w+):\s*enumDropdown\((.*?)\),\n', dropdowns_src, re.S):
    args = re.findall(r'"([^"]*)"', m.group(2).rsplit("]", 1)[-1])
    if len(args) >= 2 and args[-1]:
        declared[m.group(1)] = args[-1]

# ---- tabs ---------------------------------------------------------------------
tabs = block("const EXPERT_TABS", "[", "]")
groups = []  # (title, {kind: [keys]}, {flagKey: gate}, [action gates])
for m in re.finditer(r'\{\s*title:\s*"(expGroup\w+)"', tabs):
    text = balanced(tabs, m.start(), "{", "}")
    title = m.group(1)
    kinds, flag_gates = {}, {}
    for kind in ("flags", "values", "selects", "dropdowns", "paths"):
        km = re.search(kind + r':\s*\[', text)
        if not km:
            continue
        lst = balanced(text, km.end() - 1, "[", "]")
        keys = []
        for entry in re.finditer(r'\{\s*key:\s*"([^"]+)"\s*,\s*gate:\s*"([^"]+)"\s*\}|"([^"]+)"', lst):
            if entry.group(1):
                keys.append(entry.group(1))
                if kind == "flags":
                    flag_gates[entry.group(1)] = entry.group(2)
                else:
                    problems.append(f"{kind}: '{entry.group(1)}' declares its gate in the tab; "
                                    "put it in the setting's own definition")
            else:
                keys.append(entry.group(3))
        kinds[kind] = keys
    actions = re.findall(r'gate:\s*"([^"]+)"\s*\}', text.split("actions:")[1]) if "actions:" in text else []
    groups.append((title, kinds, flag_gates, actions))

for _, _, flag_gates, _ in groups:
    declared.update(flag_gates)

used = {}
group_of = {}
for title, kinds, _, _ in groups:
    for kind, keys in kinds.items():
        for key in keys:
            used.setdefault(kind, []).append(key)
            group_of.setdefault(key, title)

for kind, known in (("flags", all_flags), ("values", values),
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

# ---- gates --------------------------------------------------------------------
for key, gate in sorted(declared.items()):
    parent = gate.lstrip("!")
    if parent not in all_flags:
        problems.append(f"gate: '{gate}' (on '{key}') points at an unknown flag")
    elif key in group_of and parent in group_of and group_of[key] != group_of[parent]:
        problems.append(f"gate: '{key}' sits in {group_of[key]} but its parent "
                        f"'{parent}' is in {group_of[parent]} — keep them together")
    if key == parent:
        problems.append(f"gate: '{key}' gates itself")

for key, gate in sorted(DEPENDENCIES.items()):
    if declared.get(key) != gate:
        problems.append(f"dependency: '{key}' only makes sense while '{gate}' is on, "
                        f"but it declares gate {declared.get(key)!r}")

for gate in re.findall(r'gate:\s*"([^"]+)"', tabs):
    if gate.lstrip("!") not in all_flags:
        problems.append(f"gate: '{gate}' in a tab points at an unknown flag")

# The "(default off)" suffix is appended at render time from the `defaultOff`
# string, so no label may carry it inline — that would double up or, worse, stick
# to a feature that is on by default.
i18n = (Path(__file__).resolve().parents[1] / "ui" / "i18n.js").read_text(encoding="utf-8")
for line in i18n.splitlines():
    if "(Standard Aus)" in line and not line.lstrip().startswith("defaultOff:"):
        problems.append(f"i18n: label carries the default-off suffix inline: {line.strip()[:70]}")

# A key listed in EXPERT_TIPS but without a tip<Key> string shows no tooltip at
# all — withTip() drops it silently, so nothing on screen says the string is
# missing. Listing a key is a promise to write the explanation.
tips = re.search(r"const EXPERT_TIPS = \[(.*?)\];", SRC, re.S)
if not tips:
    problems.append("EXPERT_TIPS: list not found")
else:
    tip_keys = re.findall(r'"([A-Za-z0-9_]+)"', tips.group(1))
    en = re.search(r"\n  en: \{(.*?)\n  [a-z]{2}: \{", i18n, re.S)
    en_keys = set(re.findall(r"^    ([A-Za-z0-9_]+):", en.group(1), re.M)) if en else set()
    known_settings = all_flags | values | selects | dropdowns | set(top_keys(paths_src))
    for key in tip_keys:
        want = "tip" + key[0].upper() + key[1:]
        if want not in en_keys:
            problems.append(f"tooltip: '{key}' is in EXPERT_TIPS but '{want}' has no string")
        if key not in known_settings:
            problems.append(f"tooltip: EXPERT_TIPS lists '{key}', which is not a setting")

# ---- per-row reset (31) --------------------------------------------------------
# Every row kind must wire the single-value reset button; a row builder that lost
# its rowResetBtn() call would silently make that setting kind un-resettable.
for kind, fn in (("flags", "flagRow"), ("values", "valueRow"), ("selects", "selectRow"),
                 ("dropdowns", "dropdownRow"), ("paths", "pathRow")):
    body = block(f"function {fn}(", "{", "}")
    if f'rowResetBtn("{kind}"' not in body:
        problems.append(f"reset: {fn}() does not call rowResetBtn(\"{kind}\", ...)")

# A group with a colour palette has no single key for the generic per-kind loop to
# diff — changedKeysInGroup()/resetExpertGroup() need their own explicit palette
# branch, or that category can never show/act on its own reset button.
if re.search(r'palette:\s*true', tabs):
    changed_src = block("function changedKeysInGroup", "{", "}")
    if "group.palette" not in changed_src:
        problems.append("reset: changedKeysInGroup() does not detect a changed palette")
    reset_src = block("async function resetExpertGroup", "{", "}")
    if "changed.palette" not in reset_src:
        problems.append("reset: resetExpertGroup() does not reset a changed palette")


# ---- 83: EXPERT_DROPDOWNS def type vs get() return type -----------------------
# isRowChanged() compares val()/cfg.get() to cfg.def with strict !==. A numeric def
# on an entry whose get() returns a String() is permanently "changed" ("0" !== 0) —
# the copySize bug. Object-literal entries only (enumDropdown()'s default is a
# positional arg, already a string there).
for key in top_keys(dropdowns_src):
    m = re.search(r'\b' + re.escape(key) + r'\s*:\s*\{', dropdowns_src)
    if not m:
        continue
    entry = balanced(dropdowns_src, m.end() - 1, "{", "}")
    def_m = re.search(r'def:\s*-?\d+(\.\d+)?\s*,', entry)
    if def_m and "String(" in entry:
        problems.append(f"dropdowns: '{key}' has a numeric def but get() returns a String — "
                         "isRowChanged will always see it as changed")

for p in problems:
    print("FAIL", p)
print("PASS" if not problems else f"{len(problems)} problem(s)")
sys.exit(1 if problems else 0)
