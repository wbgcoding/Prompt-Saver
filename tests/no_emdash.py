#!/usr/bin/env python3
"""Guards every tracked source file against em dash / en dash / horizontal bar
/ figure dash characters, in code comments as well as shipped visible text
(every surviving comment gets the same treatment the UI text got).

Scope is the whole tracked source tree: the Rust backend, every UI script and
markup file plus its stylesheet, the test suites and their harness, and the
build tools. A dash inside a string literal is left alone here; that content
is code, not prose, and stays whatever it already needs to be (error strings
matched verbatim against the Rust backend, dictation targets, the stats
no-value placeholder, and so on -- see EXCEPTIONS below).

Two further checks cover shipped text: a pseudo dash (space, two hyphens,
space) inside a Rust or UI string literal, and any dash character anywhere in
the NSIS installer script and its language files (strings and comments alike).

Shipped release text is checked too, with no exceptions: docs/release-notes.md
in full, and CHANGELOG.md above its first dated version heading (the unreleased
section; published sections are history and stay as they are).

Range display keeps a plain ASCII hyphen with no spaces (e.g. `00:12-00:34`,
`{low}-{high}`, `(A-Z)`) -- that is not a dash character this guard looks
for, so it needs no exception.
"""
import fnmatch
import re
import subprocess
import sys

SCOPE_GLOBS = [
    "src-tauri/src/*.rs",
    "ui/*.js",
    "ui/i18n/*.js",
    "ui/*.html",
    "ui/styles/*.css",
    "tests/*.py",
    "tests/*.js",
    "tests/layout/*.py",
    "tests/ui/*.py",
    "tests/ui/smoke/*.py",
    "tests/ui/smoke/walk/*.py",
    "tests/ui/*.js",
    "tools/*.js",
    "src-tauri/*.nsi",
    "src-tauri/nsis-languages/*.nsh",
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
RANGE_EXCEPTIONS = []

# Explicit exceptions, each with its own reason. Content-matched rather than
# line-pinned so they survive edits that shift line numbers.
EXCEPTIONS = [
    {
        "file": None,
        "when": lambda line: '"—"' in line or "'—'" in line,
        "reason": "The lone em dash used as the app's 'no value yet' "
                  "placeholder (stats, backups, OCR sizes, dates, char "
                  "counts, and every other ? val : \"—\" spot sharing "
                  "that convention), a deliberate product decision, kept "
                  "as-is here too.",
    },
    {
        "file": "ui/errors.js",
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
        "file": "src-tauri/src/*.rs",
        "when": lambda line: "no usable checksum published" in line
                  or "update hash mismatch" in line,
        "reason": "Raw Err(...) strings matched verbatim by the frontend's "
                  "ERROR_MSG_KEYS lookup (ui/errors.js); internal dictionary "
                  "keys, never rendered to the user directly.",
    },
    {
        "file": "src-tauri/src/*.rs",
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
        "file": "src-tauri/src/*.rs",
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
        (exc["file"] is None or fnmatch.fnmatchcase(rel_path, exc["file"])) and exc["when"](line)
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


PSEUDO_DASH = " -- "

# String literals that legitimately contain " -- ": command lines that pass a
# flag after cargo's own separator, and a captured encoder log line used as a
# test fixture.
PSEUDO_DASH_OK = ("cargo test", "cargo run", "Error initializing output stream")


def string_literals(text, suffix):
    """Yield (line number, literal text) for every string literal in Rust or JS
    source. Comments are skipped, Rust char literals and lifetimes are not
    mistaken for string starts, raw strings and JS template literals are
    handled well enough for a hygiene check."""
    rust = suffix == ".rs"
    i, n, line = 0, len(text), 1
    while i < n:
        c = text[i]
        two = text[i:i + 2]
        if c == "\n":
            line += 1
            i += 1
        elif two == "//":
            j = text.find("\n", i)
            i = n if j == -1 else j
        elif two == "/*":
            j = text.find("*/", i + 2)
            j = n if j == -1 else j + 2
            line += text.count("\n", i, j)
            i = j
        elif not rust and c == "/" and re.search(r"(^|[(,=:\[!&|?{};+\-*%<>~^]|\breturn|\btypeof)\s*$", text[max(0, i - 40):i]):
            j = i + 1
            in_class = False
            while j < n and text[j] != "\n":
                if text[j] == chr(92):
                    j += 1
                elif text[j] == "[":
                    in_class = True
                elif text[j] == "]":
                    in_class = False
                elif text[j] == "/" and not in_class:
                    break
                j += 1
            i = j + 1
        elif rust and c == "'":
            if text[i + 1:i + 2] == "\\":
                j = text.find("'", i + 2)
                i = n if j == -1 else j + 1
            elif text[i + 2:i + 3] == "'":
                i += 3
            else:
                i += 1
        elif rust and c == "r" and re.match(r'r#*"', text[i:i + 8]) and (i == 0 or not (text[i - 1].isalnum() or text[i - 1] == "_")):
            hashes = len(re.match(r"r(#*)", text[i:]).group(1))
            start = i + 1 + hashes + 1
            end = text.find('"' + "#" * hashes, start)
            end = n if end == -1 else end
            yield line, text[start:end]
            line += text.count("\n", i, end)
            i = end + 1 + hashes
        elif c == '"' or (not rust and c in "'`"):
            j = i + 1
            while j < n and text[j] != c:
                if text[j] == "\\":
                    j += 1
                if j < n and text[j] == "\n":
                    if c == "'" or (c == '"' and not rust):
                        break
                    line += 1
                j += 1
            yield line, text[i + 1:j]
            if c == "`":
                line += text.count("\n", i, j)
            i = j + 1
        else:
            i += 1


def check_pseudo_dash(rel_path, text):
    suffix = "." + rel_path.rsplit(".", 1)[-1]
    if suffix not in (".rs", ".js"):
        return []
    lines = text.split("\n")
    out = []
    for lineno, lit in string_literals(text, suffix):
        if PSEUDO_DASH not in lit:
            continue
        shown = lines[min(lineno, len(lines)) - 1].strip()
        if any(ok in lit for ok in PSEUDO_DASH_OK):
            continue
        out.append((lineno, "pseudo dash ' -- ' in a string literal", shown))
    return out


def check_nsis(rel_path, text):
    """The installer script and its language files: no dash character on any
    line, and no ' -- ' inside a quoted string."""
    out = []
    for lineno, raw in enumerate(text.split("\n"), start=1):
        m = DASH_RE.search(raw)
        if m:
            out.append((lineno, "dash character %r" % m.group(0), raw.strip()))
            continue
        for sm in re.finditer(r'"[^"]*"', raw):
            if PSEUDO_DASH in sm.group(0):
                out.append((lineno, "pseudo dash ' -- ' in a string", raw.strip()))
                break
    return out


# Shipped release text: the whole release-notes page, and the part of the
# changelog above the first dated version heading (the unreleased section).
# Published sections are history and are never touched, so they are not scanned.
RELEASE_TEXT_FILES = ("docs/release-notes.md", "CHANGELOG.md")
DATED_HEADING = re.compile(r"^## \d+\.\d+\.\d+ \(\d{4}-\d{2}-\d{2}\)\s*$")


def check_release_text(rel_path, text):
    out = []
    for lineno, raw in enumerate(text.split("\n"), start=1):
        if rel_path == "CHANGELOG.md" and DATED_HEADING.match(raw):
            break
        m = DASH_RE.search(raw)
        if m:
            out.append((lineno, "dash character %r" % m.group(0), raw.strip()))
    return out


def main():
    global ROOT
    ROOT = subprocess.run(
        ["git", "rev-parse", "--show-toplevel"],
        capture_output=True, text=True, check=True,
    ).stdout.strip()

    files = list(scope_files())
    if not any(p.startswith("ui/i18n/") for p in files):
        print("no_emdash: scope lost ui/i18n/*.js")
        return 1

    total = 0
    for rel_path in files:
        try:
            with open(f"{ROOT}/{rel_path}", "r", encoding="utf-8") as f:
                text = f.read()
        except (FileNotFoundError, UnicodeDecodeError):
            continue
        if rel_path.endswith((".nsi", ".nsh")):
            violations = sorted(check_nsis(rel_path, text))
        else:
            violations = sorted(check_file(rel_path, text) + check_pseudo_dash(rel_path, text))
        for lineno, kind, snippet in violations:
            print(f"{rel_path}:{lineno}: {kind}: {snippet}")
        total += len(violations)

    for rel_path in RELEASE_TEXT_FILES:
        try:
            with open(f"{ROOT}/{rel_path}", "r", encoding="utf-8") as f:
                text = f.read()
        except FileNotFoundError:
            print(f"no_emdash: {rel_path} is missing")
            total += 1
            continue
        for lineno, kind, snippet in check_release_text(rel_path, text):
            print(f"{rel_path}:{lineno}: {kind}: {snippet}")
            total += 1

    if total:
        print(f"\nno_emdash: {total} violation(s)")
        return 1
    print("no_emdash: clean")
    return 0


if __name__ == "__main__":
    sys.exit(main())
