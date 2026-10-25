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
TEXT = re.compile(r">([^<>]+)<")


def attr(attrs, name):
    m = re.search(rf'{name}\s*=\s*"([^"]*)"', attrs)
    return m.group(1) if m else None


def named_at_runtime(ident):
    """True when JS gives this id a title or an accessible name after load.

    The small windows translate their menus from FLOAT_TXT instead of data-i18n, so
    their buttons are named in code, that counts, and only that counts: the id has
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


def main():
    bad = []
    for page in PAGES:
        src = (REPO / "ui" / page).read_text(encoding="utf-8")
        for m in BUTTON.finditer(src):
            attrs, body = m.group("attrs"), m.group("body")
            words = "".join(TEXT.findall(body)).strip()
            has_svg = "<svg" in body
            if words or not has_svg:
                continue  # it says something in writing
            named = any(attr(attrs, a) for a in
                        ("title", "data-i18n-title", "aria-label", "data-i18n-aria", "data-i18n"))
            if not named and not named_at_runtime(attr(attrs, "id")):
                ident = attr(attrs, "id") or attr(attrs, "class") or "?"
                bad.append(f"{page}: <button {ident}> shows only an icon and has no tooltip or name")

    for line in bad:
        print(f"   FAIL {line}")
    print("PASS" if not bad else f"{len(bad)} problem(s)")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
