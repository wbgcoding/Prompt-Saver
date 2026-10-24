"""Shortcut inventory: SHORTCUTS is the one place a keyboard shortcut lives, and
every button that fires one has to say so in its tooltip.

Same style as tooltip_check.py: static source scanning, no browser needed.

Checks:
  1. Every SHORTCUTS entry's descKey resolves to a real i18n key.
  2. No two entries in the same scope share a key combo (code + mods).
  3. Every `data-shortcut="action"` in the static HTML names a real action, and
     the element it sits on also carries `data-i18n-title` -- the one thing
     that makes applyI18n() route it through withShortcut() and show the key.
  4. Every dynamically-built button's `withShortcut(el, "action")` call (or the
     snip-tool factory's template-literal form) names a real action.
  5. Every action that is not an alt binding (does not end in "Alt") is
     referenced by at least one button, static or dynamic -- an entry nobody
     ever shows is dead weight in the table. Exempt: entries whose scopes are
     all list-only (windows with their own key handling) and the view
     switching keys.
  6. Every `openShortcutsModal(...)` call site names a scope that at least one
     SHORTCUTS entry actually uses -- catches a typo'd scope name silently
     opening an empty table.

Run:  python tests/shortcut_check.py
Exit: 0 = clean, 1 = problems found.
"""
import re
import sys
from pathlib import Path

from i18n_files import combined_source
from ui_sources import main_window_source, video_window_source

REPO = Path(__file__).resolve().parents[1]

# Windows that handle their own keys: their entries are documented in the overview
# only, so no button has to show them. Ctrl+Tab has no button either.
LIST_ONLY_SCOPES = {"launcher", "tray", "vars", "floating", "snipOverlay", "recBar", "camera"}
# snipApply is Enter on the editor's Close button, whose tooltip shows Esc (snipCancel).
LIST_ONLY_ACTIONS = {"mainNextView", "mainPrevView", "snipApply"}

ENTRY_RE = re.compile(
    r'\{\s*action:\s*"(?P<action>[^"]+)"\s*,\s*code:\s*"(?P<code>[^"]+)"\s*,\s*'
    r'mods:\s*\[(?P<mods>[^\]]*)\]\s*,\s*scopes:\s*\[(?P<scopes>[^\]]*)\]\s*,\s*'
    r'descKey:\s*"(?P<descKey>[^"]+)"\s*\}'
)


def parse_shortcuts():
    src = (REPO / "ui" / "shortcuts.js").read_text(encoding="utf-8")
    entries = []
    for m in ENTRY_RE.finditer(src):
        mods = [x.strip().strip('"') for x in m.group("mods").split(",") if x.strip()]
        scopes = [x.strip().strip('"') for x in m.group("scopes").split(",") if x.strip()]
        entries.append({
            "action": m.group("action"),
            "code": m.group("code"),
            "mods": mods,
            "scopes": scopes,
            "descKey": m.group("descKey"),
        })
    return entries


def i18n_has_key(key):
    src = combined_source()
    return re.search(rf'(?<![\w.]){re.escape(key)}:\s*"', src) is not None


def static_shortcut_buttons():
    """{action: [(page, has_title_hook)]} for every data-shortcut in markup."""
    found = []
    for page in ("index.html", "video.html"):
        src = (REPO / "ui" / page).read_text(encoding="utf-8")
        for m in re.finditer(r"<[a-zA-Z][^>]*data-shortcut=\"(?P<action>[^\"]+)\"[^>]*>", src):
            tag = m.group(0)
            has_title_hook = 'data-i18n-title="' in tag
            found.append((page, m.group("action"), has_title_hook))
    return found


DYNAMIC_CALL = re.compile(r'withShortcut\(\s*\w+\s*,\s*(?P<expr>[^)]+)\)')


def dynamic_shortcut_actions():
    """Actions (or action prefixes, for a template literal) referenced by a
    withShortcut() call in application code."""
    literal = set()
    prefixes = set()
    for js in ("main.js", "video.js"):
        src = main_window_source() if js == "main.js" else video_window_source()
        for m in DYNAMIC_CALL.finditer(src):
            expr = m.group("expr").strip()
            if expr.startswith('"') and expr.endswith('"'):
                literal.add(expr.strip('"'))
                continue
            # `snipTool.${tdef.id}` -- a template literal naming a whole family
            # of actions sharing one static prefix before the interpolation.
            tm = re.match(r"`([^`$]+)\$\{", expr)
            if tm:
                prefixes.add(tm.group(1))
    return literal, prefixes


OPEN_MODAL_CALL = re.compile(r'openShortcutsModal\(\s*(?P<expr>[^)]+)\)')


def open_modal_scopes():
    """Every scope name (or array of them) passed to an openShortcutsModal()
    call in application code."""
    found = []
    for js in ("main.js", "video.js"):
        src = main_window_source() if js == "main.js" else video_window_source()
        for m in OPEN_MODAL_CALL.finditer(src):
            expr = m.group("expr").strip()
            for s in re.findall(r'"([^"]+)"', expr):
                found.append((js, s))
        # The all-windows overview takes its scopes from one named list.
        for m in re.finditer(r'ALL_SHORTCUT_SCOPES\s*=\s*\[(?P<list>[^\]]*)\]', src):
            for s in re.findall(r'"([^"]+)"', m.group("list")):
                found.append((js, s))
    return found


def main():
    bad = []
    entries = parse_shortcuts()
    if not entries:
        print("   FAIL no SHORTCUTS entries parsed from ui/shortcuts.js")
        print("1 problem(s)")
        return 1

    actions = set()
    for e in entries:
        if e["action"] in actions:
            bad.append(f"shortcuts.js: duplicate action '{e['action']}'")
        actions.add(e["action"])
        if not i18n_has_key(e["descKey"]):
            bad.append(f"shortcuts.js: action '{e['action']}' descKey '{e['descKey']}' is not a real i18n key")

    # No duplicate key combo within one scope.
    seen = {}
    for e in entries:
        combo = e["code"] + "+" + "+".join(sorted(e["mods"]))
        for scope in e["scopes"]:
            k = (scope, combo)
            if k in seen:
                bad.append(f"shortcuts.js: '{combo}' is bound twice in scope '{scope}' ({seen[k]} and {e['action']})")
            else:
                seen[k] = e["action"]

    # Static buttons.
    referenced = set()
    for page, action, has_title_hook in static_shortcut_buttons():
        if action not in actions:
            bad.append(f"{page}: data-shortcut=\"{action}\" does not match any SHORTCUTS action")
            continue
        referenced.add(action)
        if not has_title_hook:
            bad.append(f"{page}: data-shortcut=\"{action}\" button has no data-i18n-title, so applyI18n() never shows its key")

    # Dynamic buttons.
    literal, prefixes = dynamic_shortcut_actions()
    for action in literal:
        if action not in actions:
            bad.append(f"withShortcut() call names unknown action '{action}'")
        referenced.add(action)
    for action in actions:
        if any(action.startswith(p) for p in prefixes):
            referenced.add(action)

    # Every non-alt action needs at least one button that shows it, except the
    # list-only windows and view switching (see LIST_ONLY_* above).
    by_action = {e["action"]: e for e in entries}
    for action in sorted(actions):
        if action.endswith("Alt"):
            continue
        if action in LIST_ONLY_ACTIONS or set(by_action[action]["scopes"]) <= LIST_ONLY_SCOPES:
            continue
        if action not in referenced:
            bad.append(f"shortcuts.js: action '{action}' is never shown by a button (static or dynamic)")

    # openShortcutsModal() call sites. "library" is a reserved scope name with
    # no entry yet (see the comment at the top of shortcuts.js) -- allowed
    # alongside a real one, but nothing else unknown is.
    known_scopes = {s for e in entries for s in e["scopes"]} | {"library"}
    for js, scope in open_modal_scopes():
        if scope not in known_scopes:
            bad.append(f"{js}: openShortcutsModal() names scope '{scope}', which no SHORTCUTS entry uses")

    for line in bad:
        print(f"   FAIL {line}")
    print("PASS" if not bad else f"{len(bad)} problem(s)")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
