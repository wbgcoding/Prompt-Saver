"""TOUR_FEATURES (ui/main.js, first-run tour step 5) consistency check.

Each row names a feature flag and a text key; nothing catches a row that points
at a flag or a translation that does not exist -- the row would just render a
raw i18n key, or throw trying to read an undefined flag, both invisible until
someone actually opens the tour. This is the static check that stands in.

    python tests/tour_check.py

Exit code 1 on an unknown flagKey or textKey.
"""

import re
import sys
from pathlib import Path

from i18n_files import combined_source
from ui_sources import main_window_source

ROOT = Path(__file__).resolve().parent.parent
MAIN = ROOT / "ui" / "main.js"


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
                pass
        elif c == "}":
            depth -= 1
            if depth == 0:
                break
        elif depth == 1:
            m = key_at_depth_one.match(source, i)
            if m:
                keys.append(m.group(1))
        i += 1
    return keys


def tour_features(source):
    start = source.index("const TOUR_FEATURES = [")
    end = source.index("];", start)
    block = source[start:end]
    rows = []
    for m in re.finditer(r"\{\s*flagKey:\s*\"([^\"]+)\".*?textKey:\s*\"([^\"]+)\".*?\}", block):
        rows.append((m.group(1), m.group(2)))
    return rows


def main():
    src = main_window_source()
    src_nc = re.sub(r"/\*.*?\*/", "", src, flags=re.S)
    src_nc = "\n".join(l for l in src_nc.splitlines() if not l.lstrip().startswith("//"))

    flag_labels = set(registry_keys(src_nc, "FLAG_LABELS"))
    opt_flag_labels = set(registry_keys(src_nc, "OPT_FLAG_LABELS"))
    known_flags = flag_labels | opt_flag_labels

    i18n_src = combined_source()
    en_start = i18n_src.index("\n  en: {")
    en_end = i18n_src.index("\n  de: {", en_start)
    en_keys = set(re.findall(r"\n\s+([A-Za-z_$][\w$]*):", i18n_src[en_start:en_end]))

    rows = tour_features(src_nc)
    if not rows:
        print("TOUR_FEATURES not found or empty in ui/main.js")
        sys.exit(1)

    problems = []
    for flag_key, text_key in rows:
        if flag_key not in known_flags:
            problems.append(f"TOUR_FEATURES row {flag_key!r}: flagKey is not in FLAG_LABELS or OPT_FLAG_LABELS")
        if text_key not in en_keys:
            problems.append(f"TOUR_FEATURES row {flag_key!r}: textKey {text_key!r} has no en i18n entry")

    if problems:
        for p in problems:
            print(p)
        sys.exit(1)
    print(f"TOUR_FEATURES: {len(rows)} rows, all flagKeys and textKeys resolve. PASS")


if __name__ == "__main__":
    main()
