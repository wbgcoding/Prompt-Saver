"""A feature gate that can never fire looks exactly like one that works.

Two mirrored conventions live side by side: a default-ON feature reaches CSS as
`noflag-<key>`, emitted while the feature is OFF, and a default-OFF feature as
`flag-<key>`, emitted while it is ON. Pick the wrong one and the selector still
parses, every test still passes, and the rule simply never applies -- because the
class it waits for is emitted by nothing.

Only a static comparison against the two registries tells the two apart, so that
is what this does: every `flag-`/`noflag-` class used in a stylesheet or in the
markup must belong to the registry its prefix implies.

    python tests/flag_polarity.py [--unused]

Exit code 1 on an inverted or unknown gate. `--unused` additionally lists
registry keys no stylesheet mentions -- information, not a defect: plenty of
flags are read only from JavaScript.
"""

import re
import sys
from pathlib import Path

from ui_sources import main_window_source

ROOT = Path(__file__).resolve().parent.parent
UI = ROOT / "ui"

# prefix -> the registry a key must be in for that prefix to be legitimate
GATES = {"noflag-": "FLAG_LABELS", "flag-": "OPT_FLAG_LABELS"}


def registry_keys(source, name):
    """Top-level keys of `const <name> = { ... }`, ignoring nested objects."""
    start = source.index("const %s = {" % name)
    i = source.index("{", start)
    depth, keys, key_at_depth_one = 0, [], re.compile(r"([A-Za-z_$][\w$]*)\s*:")
    while i < len(source):
        c = source[i]
        if c == "{":
            depth += 1
            if depth == 1:
                line_start = i + 1
        elif c == "}":
            depth -= 1
            if depth == 0:
                break
        elif depth == 1 and c == "\n":
            line = source[line_start:i]
            match = key_at_depth_one.match(line.strip())
            if match:
                keys.append(match.group(1))
            line_start = i + 1
        i += 1
    if not keys:
        raise SystemExit("FAIL %s: no keys parsed -- the registry moved or changed shape" % name)
    return set(keys)


def used_classes(paths):
    """Every flag class in a selector or a class attribute, with where it came from."""
    css_class = re.compile(r"\.((?:no)?flag-[A-Za-z0-9_-]+)")
    attr_class = re.compile(r'class\s*=\s*"([^"]*)"')
    found = []
    for path in paths:
        text = path.read_text(encoding="utf-8")
        if path.suffix == ".html":
            for match in attr_class.finditer(text):
                for token in match.group(1).split():
                    if token.startswith(("flag-", "noflag-")):
                        found.append((token, path))
        else:
            for match in css_class.finditer(text):
                found.append((match.group(1), path))
    return found


def main():
    source = main_window_source()
    registries = {name: registry_keys(source, name) for name in set(GATES.values())}

    files = sorted((UI / "styles").glob("*.css")) + sorted(UI.glob("*.html"))
    uses = used_classes(files)
    if not uses:
        raise SystemExit("FAIL: no flag gate found at all -- the check itself is broken")
    problems, seen = [], set()
    for cls, path in uses:
        # noflag- first: every noflag- class also starts with flag- one char in
        prefix = "noflag-" if cls.startswith("noflag-") else "flag-"
        key = cls[len(prefix):]
        seen.add(key)
        expected = GATES[prefix]
        if key in registries[expected]:
            continue
        other = next(n for n in registries if n != expected)
        where = path.relative_to(ROOT).as_posix()
        if key in registries[other]:
            problems.append(
                "INVERTED  %s in %s -- %s belongs to %s, so the class it ever gets is %s"
                % (cls, where, key, other, ("flag-" if prefix == "noflag-" else "noflag-") + key)
            )
        else:
            problems.append("UNKNOWN   %s in %s -- %s is in no flag registry" % (cls, where, key))

    if "--unused" in sys.argv:
        for name, keys in sorted(registries.items()):
            for key in sorted(keys - seen):
                print("unused    %s (%s) -- no stylesheet or markup gates on it" % (key, name))

    for line in sorted(set(problems)):
        print(line)
    total = sum(len(k) for k in registries.values())
    print(
        "FAIL: %d bad flag gate(s)" % len(problems)
        if problems
        else "PASS (%d gates checked against %d flag keys)" % (len(uses), total)
    )
    return 1 if problems else 0


if __name__ == "__main__":
    raise SystemExit(main())
