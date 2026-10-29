"""Tooltip inventory: every icon-only control has to say what it does.

A button whose whole content is an SVG shows no words, so without a tooltip and an
accessible name it is a mystery box. This walks the real markup of every window and
fails on any such control that carries neither.

Run:  python tests/tooltip_check.py
Exit: 0 = clean, 1 = problems found.
"""
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
PAGES = ["index.html", "floating.html", "folder.html", "vars.html", "snip.html"]

BUTTON = re.compile(r"<button\b(?P<attrs>[^>]*)>(?P<body>.*?)</button>", re.S | re.I)
TAG = re.compile(r"<[^>]+>")

# Dynamically created buttons: ui/*.js builds ~60 of them via createElement rather
# than static markup. Each one is scoped to the enclosing block: from its own
# creation line to wherever the brace depth first drops below where it started,
# i.e. the rest of the function/arrow body it was created in.
CREATE_BTN = re.compile(r'(?:const|let|var)\s+(\w+)\s*=\s*document\.createElement\("button"\)')


def attr(attrs, name):
    m = re.search(rf'{name}\s*=\s*"([^"]*)"', attrs)
    return m.group(1) if m else None


def named_at_runtime(ident):
    """True when JS gives this id a title or an accessible name after load.

    The small windows translate their menus from FLOAT_TXT instead of data-i18n, so
    their buttons are named in code: that counts, and only that counts: the id has
    to be captured in a variable that then gets a title or an aria-label.
    """
    if not ident:
        return False
    for js in (REPO / "ui").glob("*.js"):
        src = js.read_text(encoding="utf-8")
        for m in re.finditer(rf'(?:const|let|var)\s+(\w+)\s*=\s*document\.getElementById\("{re.escape(ident)}"\)', src):
            var = m.group(1)
            if re.search(rf"\b{var}\.title\s*=", src) or \
               re.search(rf'\b{var}\.setAttribute\(\s*"aria-label"', src):
                return True
    return False


def enclosing_block(lines, start):
    """Lines from `start` to where brace depth first drops below its starting level."""
    depth = 0
    for j in range(start, len(lines)):
        depth += lines[j].count("{") - lines[j].count("}")
        if depth < 0 and j > start:
            return lines[start:j + 1]
    return lines[start:]


def dynamic_button_findings():
    """Icon-only buttons built with createElement, never given a title/aria-label."""
    bad = []
    for js in sorted((REPO / "ui").glob("*.js")):
        src = js.read_text(encoding="utf-8")
        lines = src.split("\n")
        for i, l in enumerate(lines):
            m = CREATE_BTN.match(l.strip())
            if not m:
                continue
            var = m.group(1)
            scope = "\n".join(enclosing_block(lines, i))
            named = (
                re.search(rf"\b{re.escape(var)}\.title\s*=", scope)
                or re.search(rf'\b{re.escape(var)}\.setAttribute\(\s*["\']aria-label["\']', scope)
                or re.search(rf"\b{re.escape(var)}\.textContent\s*=", scope)
                or re.search(rf"\b{re.escape(var)}\.(?:append|appendChild)\(", scope)
                or re.search(rf'\b{re.escape(var)}\.(?:dataset\.i18n\w*|setAttribute\(\s*["\']data-i18n)', scope)
                # A factory that returns the bare element hands naming duty to
                # its caller (e.g. mkBtn(svg) in media.js), out of this check's reach.
                or re.search(rf"return\s+{re.escape(var)}\s*;", scope)
            )
            icon_only = re.search(rf"\b{re.escape(var)}\.innerHTML\s*=", scope)
            if icon_only and not named:
                bad.append(f"{js.name}:{i + 1}: button '{var}' gets only an icon and is never given a title or aria-label")
    return bad


def main():
    bad = []
    for page in PAGES:
        src = (REPO / "ui" / page).read_text(encoding="utf-8")
        for m in BUTTON.finditer(src):
            attrs, body = m.group("attrs"), m.group("body")
            words = TAG.sub("", body).strip()
            has_svg = "<svg" in body
            if words or not has_svg:
                continue  # it says something in writing
            named = any(attr(attrs, a) for a in
                        ("title", "data-i18n-title", "aria-label", "data-i18n-aria", "data-i18n"))
            if not named and not named_at_runtime(attr(attrs, "id")):
                ident = attr(attrs, "id") or attr(attrs, "class") or "?"
                bad.append(f"{page}: <button {ident}> shows only an icon and has no tooltip or name")

    bad.extend(dynamic_button_findings())

    for line in bad:
        print(f"   FAIL {line}")
    print("PASS" if not bad else f"{len(bad)} problem(s)")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
