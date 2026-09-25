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
     ever shows is dead weight in the table.

Run:  python tests/shortcut_check.py
Exit: 0 = clean, 1 = problems found.
"""
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]

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
    src = (REPO / "ui" / "i18n.js").read_text(encoding="utf-8")
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
        src = (REPO / "ui" / js).read_text(encoding="utf-8")
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

    # Every non-alt action needs at least one button that shows it.
    for action in sorted(actions):
        if action.endswith("Alt"):
            continue
        if action not in referenced:
            bad.append(f"shortcuts.js: action '{action}' is never shown by a button (static or dynamic)")

    for line in bad:
        print(f"   FAIL {line}")
    print("PASS" if not bad else f"{len(bad)} problem(s)")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
