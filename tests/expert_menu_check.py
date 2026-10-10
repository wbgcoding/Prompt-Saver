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
  * a notice that no group shows, or that also exists as a flag — a notice
    reports a state and must never become something the user can switch off.

Run:  python tests/expert_menu_check.py
Exit: 0 = clean, 1 = problems found.
"""
import re
import sys
from pathlib import Path

from i18n_files import combined_source
from ui_sources import main_window_source

SRC = main_window_source()
# Comments carry words like "// Duplicate finder: threshold" that the key scanner
# would otherwise read as setting names.
SRC = re.sub(r"/\*.*?\*/", "", SRC, flags=re.S)
SRC = "\n".join(l for l in SRC.splitlines() if not l.lstrip().startswith("//"))

# Settings deliberately absent from the tab list: normal settings in the settings
# dialog, and the ones the backups dialog renders itself (renderBackupPanel /
# renderStatsPanel, opened from settings, not from the expert menu).
NOT_IN_EXPERT = {
    "autoPaste",
    # Backup settings, rendered by renderBackupPanel.
    "autoBackup", "backupGfs", "backupKeep", "backupIntervalH", "backupStartDelayS",
    "backupDaily", "backupWeekly", "backupMonthly", "backupYearly",
    # Statistics, rendered by renderStatsPanel.
    "usageStats", "statsTopN",
    # Screenshot folder save: Settings page and the screenshot dialog.
    "screenshotSave",
    # Interface size, set by the select on the normal settings page.
    "uiScale",
    # Remembered by the library's own Close-after-copy button.
    "closeAfterCopy",
    # The tooltip switch on the normal settings page.
    "tooltipsEnabled",
    # Update interval, beside the auto-update switch on the normal settings page.
    "updateCheckH",
    # Favorites view switch: the normal settings page and the top-bar right-click menu.
    "favViewButton",
    # Switched from the header right-click menu.
    "barToggles", "showLogo", "showTitle",
}

# Every setting that only makes sense while another feature is on, and the gate it
# must carry ("!x" = only while x is OFF). Maintained by hand on purpose: whether one
# feature needs another is a product decision, not something text matching can guess.
# Adding a dependent setting without adding it here (and gating it) is the mistake
# this file exists to catch: so add both together.
DEPENDENCIES = {
    # Remote access
    "remoteTlsShortLived": "remoteNetworkFilter",
    # Creating prompts
    "storeFiles": "fileAttach",
    "cleanupFiles": "storeFiles",
    "screenshotDir": "screenshot",
    "varDefaults": "promptVars",
    "dynVars": "promptVars",
    "choiceVars": "promptVars",
    "choiceRemember": "choiceVars",
    "varFixed": "promptVars",
    "dateStyle": "dynVars",
    "timeStyle": "dynVars",
    "counterUnits": "lengthCounter",
    "historyPerPrompt": "promptHistory",
    "notesInTooltip": "promptNotes",
    "notesTipLen": "notesInTooltip",
    "pickerFormat": "colorPicker",
    "loupeZoom": "colorPicker",
    "screenshotFormat": "screenshot",
    "snipDelayMs": "screenshot",
    "snipSaveQuality": "screenshot",
    # 117/118: screenshot editor
    "snipEditor": "screenshot",
    "snipEditSnap": "snipEditor",
    "snipTextBack": "snipEditor",
    "snipEditDefaultTool": "snipEditor",
    # 121: OCR -- "ocr" itself stands alone (no parent flag; its group placement
    # right after the screenshot block is list order, not a gating relationship).
    "ocrAutoCopy": "ocr",
    "ocrJoinLines": "ocr",
    "ocrLang": "ocr",
    # 32: rotating prompts
    "rotateDefaultMode": "rotatingPrompts",
    "rotateMaxEntries": "rotatingPrompts",
    "rotateBadge": "rotatingPrompts",
    # 90-94: auto-advance
    "rotateAuto": "rotatingPrompts",
    # Markdown preview
    "codeHighlight": "mdPreview",
    "codeAutoDetect": "mdPreview",
    # Workspace
    "floatOpacity": "floating",
    "trayRecentN": "trayRecent",
    "trayFavN": "trayFavs",
    # Tiles + copying
    "chainLock": "chainPrompts",
    "chainQuickStart": "chainPrompts",
    "autoPasteEnter": "autoPaste",
    "autoPasteDelayMs": "autoPaste",
    "dblClickMs": "autoPaste",
    "promptHotkeyPaste": "promptHotkeys",
    "dragOutModifier": "dragOut",
    "dragOutInvert": "dragOut",
    "copySizeLevel": "bubbleMs",
    "tooltipDelayMs": "tooltipsEnabled",
    "favViewButton": "favorites",
    "favViewReorder": "favViewButton",
    "favMaxCols": "favViewButton",
    # Appearance
    "frostedBlur": "frostedModals",
    "customThemeMax": "customThemes",
    # Library, history, privacy
    "libraryCols": "librarySearch",
    "tagFilterMode": "tags",
    "tagSort": "tags",
    "historyMax": "copyHistory",
    "historyDays": "copyHistory",
    # Media
    "videoVolume": "!videoMuted",
    # Tools + data
    "maxViews": "multiView",
    "gridViewBorder": "multiView",
    # Recording upkeep lives with the recording switch, the video editor's temp
    # files with the editor, the error log's size with the log.
    "recSweepBroken": "screenRecord",
    "recDiscardKeepH": "screenRecord",
    "recordingRetentionDays": "screenRecord",
    "errorLogKeepKb": "errorLog",
    # Starting values and choices added with their feature's own switch
    "recOutline": "screenRecord",
    "recOutlineColor": "recOutline",
    "chainSeparator": "chainPrompts",
    "snipDefaultColor": "snipEditor",
    "dupImportCheck": "dupFinder",
    "similarThreshold": "similarHint",
    "clipMinChars": "clipWatcher",
    "clipInboxMax": "clipWatcher",
    # Backups + statistics (rendered outside the expert menu, gate still required)
    "backupKeep": "autoBackup",
    "backupIntervalH": "autoBackup",
    "backupStartDelayS": "autoBackup",
    "backupDaily": "backupGfs",
    "backupWeekly": "backupGfs",
    "backupMonthly": "backupGfs",
    "backupYearly": "backupGfs",
    "statsTopN": "usageStats",
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


def top_level_args(s):
    """Split a call's argument text at bracket depth 0."""
    args, depth, cur = [], 0, ""
    for ch in s:
        if ch in "([{":
            depth += 1
        elif ch in ")]}":
            depth -= 1
        if ch == "," and depth == 0:
            args.append(cur)
            cur = ""
        else:
            cur += ch
    if cur.strip():
        args.append(cur)
    return args


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
# A value whose 0 is its off stop gates its dependents like a flag (offAtZero: true).
off_at_zero = {k for k in values
               if re.search(r'\b' + re.escape(k) + r'\s*:\s*\{[^{}]*\boffAtZero:\s*true', values_src)}
selects_src = block("const EXPERT_SELECTS", "{", "}")
selects = set(top_keys(selects_src))
dropdowns_src = block("const EXPERT_DROPDOWNS", "{", "}")
dropdowns = set(top_keys(dropdowns_src))
paths_src = block("const EXPERT_PATHS", "{", "}")
colors_src = block("const EXPERT_COLORS", "{", "}")
colors = set(top_keys(colors_src))

problems = []

# ---- gates declared per setting kind ------------------------------------------
# Values/selects/paths spell `gate:` inside their own definition; the enum dropdowns
# are built by enumDropdown(key, label, opts, default, gate) and pass it positionally.
declared = {}
declared.update(gates_of(values_src))
declared.update(gates_of(selects_src))
declared.update(gates_of(paths_src))
declared.update(gates_of(dropdowns_src))
declared.update(gates_of(colors_src))
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
    for kind in ("flags", "values", "selects", "dropdowns", "paths", "colors"):
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

# A switch that lives outside the expert menu (Settings) keeps its gate for the rows
# that depend on it through an explicit assignment after FLAG_GATES is built.
for m in re.finditer(r'FLAG_GATES\.(\w+)\s*=\s*"([^"]+)"\s*;', SRC):
    declared[m.group(1)] = m.group(2)

used = {}
group_of = {}
for title, kinds, _, _ in groups:
    for kind, keys in kinds.items():
        for key in keys:
            used.setdefault(kind, []).append(key)
            group_of.setdefault(key, title)

for kind, known in (("flags", all_flags), ("values", values),
                    ("selects", selects), ("dropdowns", dropdowns), ("colors", colors)):
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
# 105: a group whose ONLY flag is itself (everything else it owns is gated on it,
# same group -- e.g. expGroupTags: flags=["tags"]) is a whole-subsystem switch, not
# a narrow feature toggle. A row in another group may legitimately depend on it
# (a tag row needs both its own toggle AND the global tags flag) without
# moving in beside it -- unlike a local flag (chainPrompts, promptHotkeys, ...)
# whose only sensible dependents are its own immediate parameters.
subsystem_toggles = {ks[0] for _, kinds, _, _ in groups for ks in [kinds.get("flags", [])] if len(ks) == 1}

for key, gate in sorted(declared.items()):
    parent = gate.lstrip("!")
    if parent not in all_flags | off_at_zero:
        problems.append(f"gate: '{gate}' (on '{key}') points at an unknown flag")
    elif (key in group_of and parent in group_of and group_of[key] != group_of[parent]
          and parent not in subsystem_toggles
          # The Troubleshooting group gathers diagnostics from every area on purpose;
          # their gates stay where they were.
          and group_of[key] != "expGroupDiagnostics"):
        problems.append(f"gate: '{key}' sits in {group_of[key]} but its parent "
                        f"'{parent}' is in {group_of[parent]} — keep them together")
    if key == parent:
        problems.append(f"gate: '{key}' gates itself")

for key, gate in sorted(DEPENDENCIES.items()):
    if declared.get(key) != gate:
        problems.append(f"dependency: '{key}' only makes sense while '{gate}' is on, "
                        f"but it declares gate {declared.get(key)!r}")

for gate in re.findall(r'gate:\s*"([^"]+)"', tabs):
    if gate.lstrip("!") not in all_flags | off_at_zero:
        problems.append(f"gate: '{gate}' in a tab points at an unknown flag")

# The "(default off)" suffix is appended at render time from the `defaultOff`
# string, so no label may carry it inline: that would double up or, worse, stick
# to a feature that is on by default.
i18n = combined_source()
for line in i18n.splitlines():
    if "(Standard Aus)" in line and not line.lstrip().startswith("defaultOff:"):
        problems.append(f"i18n: label carries the default-off suffix inline: {line.strip()[:70]}")

# A key listed in EXPERT_TIPS but without a tip<Key> string shows no tooltip at
# all: withTip() drops it silently, so nothing on screen says the string is
# missing. Listing a key is a promise to write the explanation.
en = re.search(r"\n  en: \{(.*?)\n  [a-z]{2}: \{", i18n, re.S)
en_keys = set(re.findall(r"^    ([A-Za-z0-9_]+):", en.group(1), re.M)) if en else set()
tips = re.search(r"const EXPERT_TIPS = \[(.*?)\];", SRC, re.S)
if not tips:
    problems.append("EXPERT_TIPS: list not found")
else:
    tip_keys = re.findall(r'"([A-Za-z0-9_]+)"', tips.group(1))
    known_settings = all_flags | values | selects | dropdowns | set(top_keys(paths_src)) | set(top_keys(colors_src))
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
# diff: changedKeysInGroup()/resetExpertGroup() need their own explicit palette
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
# on an entry whose get() returns a String() is permanently "changed" ("0" !== 0):
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

# ---- 99: every dropdown needs a `def`, or isRowChanged() can never flag it ----
for m in re.finditer(r'(\w+):\s*enumDropdown\(([\s\S]*?)\),\n', dropdowns_src):
    key, args = m.group(1), top_level_args(m.group(2))
    if len(args) < 4 or not args[3].strip():
        problems.append(f"dropdowns: '{key}' calls enumDropdown() without a def argument")
for key in top_keys(dropdowns_src):
    m = re.search(r'\b' + re.escape(key) + r'\s*:\s*\{', dropdowns_src)
    if not m:
        continue  # an enumDropdown() entry, checked above
    entry = balanced(dropdowns_src, m.end() - 1, "{", "}")
    if "def:" not in entry:
        problems.append(f"dropdowns: '{key}' has no def — its reset button can never appear")

# Same requirement for plain values/selects (a missing `def` would crash isRowChanged
# with "cannot read .def of undefined" before it could ever show a wrong result, but
# a static check catches it at review time instead of at first click).
for kind_name, src in (("values", values_src), ("selects", selects_src)):
    for key in top_keys(src):
        m = re.search(r'\b' + re.escape(key) + r'\s*:\s*\{', src)
        entry = balanced(src, m.end() - 1, "{", "}") if m else ""
        if "def:" not in entry:
            problems.append(f"{kind_name}: '{key}' has no def — its reset button can never appear")

# The backup/statistics panels (their own top-level window, not the expert menu,
# see NOT_IN_EXPERT above) reuse rowResetBtn()/resetOneExpertValue() through numRow()
# and the hand-rolled backupIntervalH select, rather than a second implementation.
numrow_body = block("function numRow(", "{", "}")
if 'rowResetBtn("values"' not in numrow_body:
    problems.append('reset: numRow() does not call rowResetBtn("values", ...)')
backup_panel_body = block("async function renderBackupPanel", "{", "}")
if 'rowResetBtn("selects", "backupIntervalH"' not in backup_panel_body:
    problems.append("reset: renderBackupPanel() does not wire a reset button for backupIntervalH")

# The colour palette's reset button used to show unconditionally; it must now follow
# the same "only while changed" rule every other row's rowResetBtn() already has:
# i.e. the button element is only ever appended inside a conditional.
palette_body = block("function paletteRow(", "{", "}")
if not re.search(r'if\s*\(\s*\w+\s*\)\s*\{[^}]*palette-reset', palette_body, re.S):
    problems.append("reset: paletteRow() shows its reset button even when unchanged")

# ---- notices ------------------------------------------------------------------
# A notice reports a state the app is in (the store being written unencrypted),
# not a setting. It has no default, nothing to reset and (the point of the whole
# construct) no switch: a warning that describes what is on disk right now cannot
# be turned off, or the one user who needs it is the one who silenced it. So:
# every id a group lists must be declared, every declared one must be placed, both
# its strings must exist, and none of them may ever turn up as a flag.
notices_src = block("const EXPERT_NOTICES", "{", "}")
declared = set(top_keys(notices_src))
placed = [k for m in re.finditer(r'notices:\s*\[([^\]]*)\]', tabs)
          for k in re.findall(r'"([^"]+)"', m.group(1))]
for key in sorted(declared - set(placed)):
    problems.append(f"notices: '{key}' is declared but no group shows it")
for key in placed:
    if key not in declared:
        problems.append(f"notices: a group lists '{key}', which is not in EXPERT_NOTICES")
    if placed.count(key) > 1:
        problems.append(f"notices: '{key}' is listed by more than one group")
    if key in all_flags:
        problems.append(f"notices: '{key}' also exists as a flag — a notice must not be switchable off")
for key in sorted(declared):
    m = re.search(r'\b' + re.escape(key) + r'\s*:\s*\{', notices_src)
    entry = balanced(notices_src, m.end() - 1, "{", "}") if m else ""
    if "when:" not in entry:
        problems.append(f"notices: '{key}' has no when() — it would show unconditionally")
    for field in ("title", "body"):
        s = re.search(field + r':\s*"([^"]+)"', entry)
        if not s:
            problems.append(f"notices: '{key}' has no {field} string key")
        elif s.group(1) not in en_keys:
            problems.append(f"notices: '{key}' points at '{s.group(1)}', which has no string")

# ---- bulk writers never flip the switches that carry their own consent step ----
# The resets write through the bare set_ui_flag path, which skips the
# password dialog (receiver), the plaintext warning (sync) and the confirmed restart
# (demo mode). Those keys stay out of every bulk write.
never_m = re.search(r'const EXPERT_BULK_NEVER\s*=\s*\[([^\]]*)\]', SRC)
if not never_m or "demoMode" not in re.findall(r'"([^"]+)"', never_m.group(1)):
    problems.append("bulk: EXPERT_BULK_NEVER must list demoMode")
for fn in ("function isRowChanged", "async function resetExpertGroup",
           "async function resetOneExpertValue"):
    if "EXPERT_BULK_NEVER" not in block(fn, "{", "}"):
        problems.append(f"bulk: {fn}() does not honour EXPERT_BULK_NEVER")
# A full reset has its own keep list: the backend clears the certificate lifetime
# flag itself, so only the demo marker is written back.
if "EXPERT_RESET_KEEP" not in block("async function onResetExpert", "{", "}"):
    problems.append("bulk: onResetExpert() does not honour EXPERT_RESET_KEEP")
if "demoMode" not in re.findall(r'"([^"]+)"', (re.search(r"const EXPERT_RESET_KEEP = \[([^\]]*)\]", SRC) or [None, ""])[1]):
    problems.append("bulk: EXPERT_RESET_KEEP must list demoMode")

for p in problems:
    print("FAIL", p)
print("PASS" if not problems else f"{len(problems)} problem(s)")
sys.exit(1 if problems else 0)
