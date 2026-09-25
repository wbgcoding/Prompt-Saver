#!/usr/bin/env python3
"""Guards every tracked source file against em dash / en dash / horizontal bar
/ figure dash characters, in code comments as well as shipped visible text
(item 350: every surviving comment gets the same treatment item 345 gave the
UI text).

Scope is the whole tracked source tree: the Rust backend, every UI script and
markup file plus its stylesheet, the test suites and their harness, and the
build tools. A dash inside a string literal is left alone here; that content
is code, not prose, and stays whatever it already needs to be (error strings
matched verbatim against the Rust backend, dictation targets, the stats
no-value placeholder, and so on -- see EXCEPTIONS below).

Range display keeps a plain ASCII hyphen with no spaces (e.g. `00:12-00:34`,
`{low}-{high}`, `(A-Z)`) -- that is not a dash character this guard looks
for, so it needs no exception.
"""
import re
import subprocess
import sys

SCOPE_GLOBS = [
    "src-tauri/src/*.rs",
    "ui/*.js",
    "ui/*.html",
    "ui/styles/*.css",
    "tests/*.py",
    "tests/*.js",
    "tests/layout/*.py",
    "tests/ui/*.py",
    "tests/ui/*.js",
    "tools/*.js",
]

DASH_CHARS = "—–―‒"
DASH_RE = re.compile("[" + DASH_CHARS + "]")

# Comment syntax per file type: (line marker, block open, block close).
COMMENT_SYNTAX = {
    ".rs": ("//", "/*", "*/"),
    ".js": ("//", "/*", "*/"),
    ".css": (None, "/*", "*/"),
    ".html": (None, "<!--", "-->"),
    ".py": ("#", None, None),
}

STRING_SPAN = re.compile(r"""(?<!\\)(?:"[^"\n]*"|'[^'\n]*')""")

# A line range excepted wholesale, keyed by file and inclusive (start, end).
# Temporary: mod keylog is being rewritten in a parallel lane and is left
# byte-identical here on purpose; cleaned up (removed from this list) once
# that lane merges and the module gets its own dash pass.
RANGE_EXCEPTIONS = [
    {
        "file": "src-tauri/src/lib.rs",
        "start": 20982,
        "end": 21357,
        "reason": "mod keylog -- rewritten in a parallel change, cleaned "
                  "after merge.",
    },
    {
        "file": "tools/brand.js",
        "start": 204,
        "end": 247,
        "reason": "brandRs()'s backtick template literal: JS string-literal "
                  "content in this file (it becomes real // comments only in "
                  "the GENERATED src-tauri/src/brand.rs output), so it is "
                  "code, not a comment, here.",
    },
    {
        "file": "tools/brand.js",
        "start": 251,
        "end": 265,
        "reason": "brandJs()'s backtick template literal: same as brandRs() "
                  "above, generates ui/brand.js's comments rather than being "
                  "one itself.",
    },
]

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
    {
        "file": "src-tauri/src/lib.rs",
        "when": lambda line: "no usable checksum published" in line
                  or "update hash mismatch" in line,
        "reason": "Raw Err(...) strings matched verbatim by the frontend's "
                  "ERROR_MSG_KEYS lookup (ui/main.js); internal dictionary "
                  "keys, never rendered to the user directly.",
    },
    {
        "file": "src-tauri/src/lib.rs",
        "when": lambda line: "sync_agents_md" not in line and (
                  "an icon or a PDF page lists those as paths" in line
                  or "Edit the text (or a header field) and save" in line
                  or "leave it alone, it clears itself within seconds" in line
                  ),
        "reason": "Shipped user-facing documentation text written into the "
                  "sync folder's own markdown file by sync_agents_md(), not "
                  "a source comment.",
    },
    {
        "file": "src-tauri/src/lib.rs",
        "when": lambda line: 'text.push(' in line
                  or "grid fields were valid" in line
                  or "a stale map really does skip the write" in line
                  or "the foreign backup password must not survive" in line
                  or "yesterday had nothing" in line
                  or "without the flush the disk is behind" in line,
        "reason": "String literal content (the stats no-value placeholder "
                  "char, or an assert!/assert_eq! test-failure message), "
                  "not a comment.",
    },
]


def is_excepted(rel_path, lineno, line):
    for exc in RANGE_EXCEPTIONS:
        if exc["file"] == rel_path and exc["start"] <= lineno <= exc["end"]:
            return True
    return any(
        (exc["file"] is None or exc["file"] == rel_path) and exc["when"](line)
        for exc in EXCEPTIONS
    )


def comment_segments(text, suffix):
    """Yield (line number, comment text) for every comment in the file.

    Mirrors tests/source_hygiene.py's comment_segments: string literals are
    blanked first, so a dash inside a quoted string never reads as the start
    of a comment and a dash after a comment marker inside a string is not
    mistaken for one either.
    """
    line_marker, block_open, block_close = COMMENT_SYNTAX.get(suffix, (None, None, None))
    if line_marker is None and block_open is None:
        return
    in_block = False
    for lineno, raw in enumerate(text.split("\n"), start=1):
        rest = raw
        while rest:
            if in_block:
                end = rest.find(block_close) if block_close else -1
                if end == -1:
                    yield lineno, rest
                    break
                yield lineno, rest[:end]
                rest = rest[end + len(block_close):]
                in_block = False
                continue
            code = STRING_SPAN.sub(lambda m: '"' + " " * (len(m.group(0)) - 2) + '"', rest)
            line_at = code.find(line_marker) if line_marker else -1
            block_at = code.find(block_open) if block_open else -1
            if line_at != -1 and (block_at == -1 or line_at < block_at):
                yield lineno, rest[line_at + len(line_marker):]
                break
            if block_at != -1:
                rest = rest[block_at + len(block_open):]
                in_block = True
                continue
            break


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

    for lineno, comment in comment_segments(text, suffix):
        if not DASH_RE.search(comment):
            continue
        line = lines[lineno - 1]
        if is_excepted(rel_path, lineno, line):
            continue
        for m in DASH_RE.finditer(comment):
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
