"""Consistency check for SETTINGS_DIRECTORY (ui/main.js), the hand-maintained
list that lets the expert-menu search also find a hit in the normal settings
page, the always-visible toolbar, the library dialog or the backup manager.

Catches the mistakes that are invisible until someone actually searches:
  * a directory entry whose target element does not exist in the markup
    (area "normal"/"toolbar"/"library": no matching `id="..."` in
    ui/index.html; area "backup": no matching `dataset.settingRow = "..."`
    assignment in ui/main.js) -- a stale entry that would silently 404 on
    click,
  * a directory entry whose `label` (or `tip`) is not a real i18n key,
  * a real, findable control in the normal settings page (an <input> or
    <select> with an id, inside #settings-main) that is NOT in the
    directory and not explicitly whitelisted as "not a setting" -- a
    forgotten field, exactly what Ben's own spec for this check asked for.

Run:  python tests/settings_directory_check.py
Exit: 0 = clean, 1 = problems found.
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MAIN = ROOT / "ui" / "main.js"
INDEX = ROOT / "ui" / "index.html"
I18N = ROOT / "ui" / "i18n.js"

MAIN_SRC = MAIN.read_text(encoding="utf-8")
INDEX_SRC = INDEX.read_text(encoding="utf-8")
I18N_SRC = I18N.read_text(encoding="utf-8")

# Comments would otherwise read as directory entries or data-setting-row hits.
MAIN_NC = re.sub(r"/\*.*?\*/", "", MAIN_SRC, flags=re.S)
MAIN_NC = "\n".join(l for l in MAIN_NC.splitlines() if not l.lstrip().startswith("//"))

# Controls inside #settings-main that are actions/buttons/plain UI state, not
# a persisted setting -- deliberately excluded from SETTINGS_DIRECTORY, same
# reasoning expert_menu_check.py's own NOT_IN_EXPERT documents for the expert
# side. Kept here, by hand, for the same reason: whether something counts as
# "a setting worth finding" is a product judgement call, not text matching.
NOT_A_SETTING = {
    "import-btn", "export-format", "export-btn", "settings-backup-btn",
    "export-all", "import-all", "about-btn", "update-btn", "update-version",
    "help-open", "tour-open-btn", "expert-open", "reset-settings",
    "delete-data", "hotkey-clear", "hotkey-list-btn", "voice-hotkey-clear",
}


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


def block(name, src, opener="[", closer="]"):
    i = src.index(name)
    return balanced(src, src.index(opener, i), opener, closer)


ENTRY_RE = re.compile(
    r'\{\s*key:\s*"([^"]+)"\s*,\s*area:\s*"([^"]+)"\s*,\s*label:\s*"([^"]+)"'
    r'(?:\s*,\s*tip:\s*"([^"]+)")?(?:\s*,\s*gate:\s*"([^"]+)")?\s*,?\s*\}'
)


def directory_entries():
    text = block("const SETTINGS_DIRECTORY = [", MAIN_NC)
    out = []
    for m in ENTRY_RE.finditer(text):
        key, area, label, tip, gate = m.groups()
        out.append({"key": key, "area": area, "label": label, "tip": tip, "gate": gate})
    return out


def en_keys():
    # Same technique as expert_menu_check.py's top_keys(), scoped to just the
    # English block so a translation-only key in another language is not
    # mistaken for coverage.
    i = I18N_SRC.index("\n  en: {\n")
    obj = balanced(I18N_SRC, I18N_SRC.index("{", i), "{", "}")
    return set(re.findall(r'\n\s{4}([A-Za-z_$][\w$]*):', obj))


def main():
    problems = []
    entries = directory_entries()
    if not entries:
        problems.append("SETTINGS_DIRECTORY parsed to zero entries -- the regex or the array itself is broken")

    seen_keys = set()
    for e in entries:
        if e["key"] in seen_keys:
            problems.append(f"duplicate directory entry: {e['key']}")
        seen_keys.add(e["key"])

        if e["area"] in ("normal", "toolbar", "library"):
            if f'id="{e["key"]}"' not in INDEX_SRC:
                problems.append(f"{e['key']}: no element with this id in ui/index.html ({e['area']})")
        elif e["area"] == "backup":
            # Most rows set dataset.settingRow directly with the literal key;
            # the four tier rows go through a small tierRow(..., dataKey)
            # helper instead, so a direct-assignment match would miss them --
            # checking that the literal key string appears anywhere near a
            # dataset.settingRow use covers both without hardcoding the helper.
            if f'"{e["key"]}"' not in MAIN_NC or "dataset.settingRow" not in MAIN_NC:
                problems.append(f"{e['key']}: no matching dataset.settingRow assignment in renderBackupPanel")
        else:
            problems.append(f"{e['key']}: unknown area '{e['area']}'")

    keys = en_keys()
    for e in entries:
        if e["label"] not in keys:
            problems.append(f"{e['key']}: label key '{e['label']}' is not a real i18n key")
        if e["tip"] and e["tip"] not in keys:
            problems.append(f"{e['key']}: tip key '{e['tip']}' is not a real i18n key")

    # Forgotten-field scan: every real <input>/<select> id inside #settings-main
    # that is not a NOT_A_SETTING action and not already in the directory.
    main_page = INDEX_SRC[INDEX_SRC.index('<div id="settings-main">'):INDEX_SRC.index('<!-- /settings-main -->')]
    directory_normal_keys = {e["key"] for e in entries if e["area"] == "normal"}
    for tag_match in re.finditer(r'<(input|select)\b[^>]*\bid="([^"]+)"', main_page):
        el_id = tag_match.group(2)
        if el_id in NOT_A_SETTING or el_id in directory_normal_keys:
            continue
        problems.append(f"{el_id}: a real <{tag_match.group(1)}> in #settings-main is neither in "
                         "SETTINGS_DIRECTORY nor whitelisted in NOT_A_SETTING -- forgotten field?")

    if problems:
        print(f"settings_directory_check: {len(problems)} problem(s)")
        for p in problems:
            print(f"  FAIL: {p}")
        return 1
    print(f"settings_directory_check: clean ({len(entries)} entries)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
