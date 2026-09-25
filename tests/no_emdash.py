#!/usr/bin/env python3
"""Guards visible, shipped text against em dash / en dash / horizontal bar /
figure dash characters (item 345: every visible text reads without a dash).

Scope is the user-facing text surface: the i18n dictionary, the in-app help
content and its markdown mirror, every UI script and markup file, and the
top-level README. Source-code comments are deliberately left alone here --
that cleanup is item 350's job, which runs later for a reason stated there --
so this guard strips `//` and `/* */` / `<!-- -->` comments before scanning a
JS or HTML file and never flags a dash that lives only in one.

Range display keeps a plain ASCII hyphen with no spaces (e.g. `00:12-00:34`,
`{low}-{high}`, `(A-Z)`) -- that is not a dash character this guard looks
for, so it needs no exception.
"""
import re
import subprocess
import sys

SCOPE_GLOBS = [
    "ui/*.js",
    "ui/*.html",
    "docs/help.md",
    "README.md",
]

DASH_CHARS = "—–―‒"
DASH_RE = re.compile("[" + DASH_CHARS + "]")

# Comment syntax per file type: (line marker, block open, block close).
COMMENT_SYNTAX = {
    ".js": ("//", "/*", "*/"),
    ".html": (None, "<!--", "-->"),
    ".md": (None, None, None),
}

STRING_SPAN = re.compile(r"""(?<!\\)(?:"[^"\n]*"|'[^'\n]*')""")

# Explicit exceptions, each with its own reason. Content-matched rather than
# line-pinned so they survive edits that shift line numbers.
EXCEPTIONS = [
    {
        "file": None,
        "when": lambda line: '"—"' in line or "'—'" in line,
        "reason": "The lone em dash used as the app's 'no value yet' "
                  "placeholder (stats, backups, OCR sizes, dates, char "
                  "counts, and every other ? val : \"—\" spot sharing "
                  "that convention) -- Ben's decision on item 325, kept "
                  "as-is here too.",
    },
    {
        "file": "ui/main.js",
        "when": lambda line: "update hash mismatch" in line,
        "reason": "Matches the Rust backend's raw Err(...) string verbatim "
                  "(src-tauri/src/lib.rs) so the ERROR_MSG_KEYS lookup "
                  "still finds it; it is an internal dictionary key, never "
                  "rendered to the user directly, and not shipped UI text.",
    },
    {
        "file": "ui/voice.js",
        "when": lambda line: "VOICE_PUNCT" in line or '"dash"' in line
                  or '"gedankenstrich"' in line or '"raya"' in line
                  or '"trattino lungo"' in line or '"travessão"' in line
                  or '"myślnik"' in line or '"破折号"' in line
                  or '"ダッシュ"' in line or '"streepje"' in line
                  or '"uzun çizgi"' in line or '"linie de pauză"' in line,
        "reason": "The en dash here is the literal character voice "
                  "dictation types when the user says the word for "
                  "\"dash\" in their language -- a dictation target, not "
                  "displayed app text, so it stays untouched.",
    },
]


def is_excepted(rel_path, line):
    return any(
        (exc["file"] is None or exc["file"] == rel_path) and exc["when"](line)
        for exc in EXCEPTIONS
    )


def code_segments(text, suffix):
    """Yield (line number, code text) with comments blanked out.

    Mirrors tests/source_hygiene.py's comment_segments, but keeps the code
    instead of the comment: a dash inside a `//` or `/* */` / `<!-- -->`
    comment is invisible to this guard, a dash inside a string literal or
    plain markup text is not.
    """
    line_marker, block_open, block_close = COMMENT_SYNTAX.get(suffix, (None, None, None))
    if line_marker is None and block_open is None:
        # Plain text (markdown): nothing is a comment.
        for lineno, raw in enumerate(text.split("\n"), start=1):
            yield lineno, raw
        return

    in_block = False
    for lineno, raw in enumerate(text.split("\n"), start=1):
        rest = raw
        kept = []
        while rest:
            if in_block:
                end = rest.find(block_close) if block_close else -1
                if end == -1:
                    rest = ""
                    break
                rest = rest[end + len(block_close):]
                in_block = False
                continue
            code = STRING_SPAN.sub(lambda m: '"' + " " * (len(m.group(0)) - 2) + '"', rest)
            line_at = code.find(line_marker) if line_marker else -1
            block_at = code.find(block_open) if block_open else -1
            if line_at != -1 and (block_at == -1 or line_at < block_at):
                kept.append(rest[:line_at])
                rest = ""
                break
            if block_at != -1:
                kept.append(rest[:block_at])
                rest = rest[block_at + len(block_open):]
                in_block = True
                continue
            kept.append(rest)
            rest = ""
        yield lineno, "".join(kept)


def scope_files():
    out = subprocess.run(
        ["git", "ls-files", "--"] + SCOPE_GLOBS,
        cwd=ROOT, capture_output=True, text=True, check=True,
    )
    return [line for line in out.stdout.splitlines() if line.strip()]


def check_file(rel_path, text):
    violations = []
    lines = text.split("\n")
    suffix = "." + rel_path.rsplit(".", 1)[-1]

    for lineno, code in code_segments(text, suffix):
        if not DASH_RE.search(code):
            continue
        line = lines[lineno - 1]
        if is_excepted(rel_path, line):
            continue
        for m in DASH_RE.finditer(code):
            violations.append((lineno, "dash character %r" % m.group(0), line.strip()))
            break
    return violations


def main():
    global ROOT
    ROOT = subprocess.run(
        ["git", "rev-parse", "--show-toplevel"],
        capture_output=True, text=True, check=True,
    ).stdout.strip()

    total = 0
    for rel_path in scope_files():
        try:
            with open(f"{ROOT}/{rel_path}", "r", encoding="utf-8") as f:
                text = f.read()
        except (FileNotFoundError, UnicodeDecodeError):
            continue
        violations = sorted(check_file(rel_path, text))
        for lineno, kind, snippet in violations:
            print(f"{rel_path}:{lineno}: {kind}: {snippet}")
        total += len(violations)

    if total:
        print(f"\nno_emdash: {total} violation(s)")
        return 1
    print("no_emdash: clean")
    return 0


if __name__ == "__main__":
    sys.exit(main())
