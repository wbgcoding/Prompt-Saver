#!/usr/bin/env python3
"""Guards the shipped source tree against process traces: numbered work-item
comments, AI-tool name-drops, references to internal planning files, real
names / local paths, and the old copyright spelling.

A comment states the fact or the reason. It never states where the fact came
from -- not the plan item that ordered it, not the round it was decided in, not
the tool that helped write it. Every rule below is one shape that leaks that.
"""
import re
import subprocess
import sys

TRACKED_GLOBS = [
    "src-tauri/src/*.rs",
    "ui/*.js",
    "ui/*.html",
    "ui/styles/*.css",
    "tools/*.js",
    "build.bat",
    "installer.nsi",
]

# Comment syntax per file type: (line marker, block open, block close).
COMMENT_SYNTAX = {
    ".rs": ("//", "/*", "*/"),
    ".js": ("//", "/*", "*/"),
    ".css": (None, "/*", "*/"),
    ".html": (None, "<!--", "-->"),
    ".nsi": (";", "/*", "*/"),
    ".bat": ("rem ", None, None),
}

# "// 51:", "// 90-94:", "// 123.3:" -- a plan item wearing a colon.
NUMBERED_COMMENT = re.compile(r"^\s*\d+(?:\s*[-,/.]\s*\d+)*\s*[:)]")
# The same thing spelled out, in either language the plans were written in.
WORK_ITEM_WORD = re.compile(
    r"\b(?:points?|items?|punkte?|entscheidung|runde|round|phase)\s+(?:\d+|[A-Z]\b)",
    re.IGNORECASE,
)
# A work-item code buried anywhere in a comment -- e.g. a section divider like
# "---------- F25: folder commands ----------", or a bare "// F23" tag.
WORK_ITEM_CODE = re.compile(r"(?<![\w+])[FSGPE]\d{1,3}\b")
# The date a decision was taken. Only ever process bookkeeping.
DECISION_DATE = re.compile(r"\(\s*\d{1,2}\.\s*\d{1,2}\.\s*(?:\d{2,4})?\s*\)")

FORBIDDEN_WORDS = re.compile(
    r"\b(codex|claude|chatgpt|copilot|gemini|llm|prompt-engineer)\b",
    re.IGNORECASE,
)

FORBIDDEN_FILENAMES = [
    "CLAUDE.md",
    "WORK.md",
    "AGENTS.md",
    "GEMINI.md",
    "INVARIANTS.md",
    "FUTUREUPDATES.md",
    "SUGGESTIONS.md",
    "DESIGN.md",
]

# Real name to guard against once Ben supplies one; empty until then (not
# present anywhere in the tracked tree today, verified via git log/manifests).
FORBIDDEN_NAMES = []

FORBIDDEN_PATH = "C:\\Users\\"
OLD_COPYRIGHT_SPELLING = "BG Coding"

# Explicit exceptions, each with its own reason. Content-matched rather than
# line-pinned so they survive edits that shift line numbers.
EXCEPTIONS = [
    {
        "file": "src-tauri/src/lib.rs",
        "when": lambda line: "AGENTS.md" in line,
        "reason": "AGENTS.md here is the read-me the markdown-sync folder "
                  "writes for the user -- a product file name, not a "
                  "planning-doc reference.",
    },
    {
        "file": "src-tauri/src/lib.rs",
        "when": lambda line: "Alt+F4" in line or "Ctrl+F4" in line,
        "reason": "A keyboard key, not a work-item code.",
    },
    {
        "file": "ui/main.js",
        "when": lambda line: "P3" in line and "cubic" in line.lower(),
        "reason": "An easing control point, not a work-item code.",
    },
]

STRING_SPAN = re.compile(r"""(?<!\\)(?:"[^"\n]*"|'[^'\n]*')""")


def comment_segments(text, suffix):
    """Yield (line number, comment text) for every comment in the file.

    Only what a reader sees as prose is handed to the rules: string literals are
    blanked first, so a URL in code never reads as the start of a comment.
    """
    line_marker, block_open, block_close = COMMENT_SYNTAX.get(suffix, ("//", "/*", "*/"))
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


def tracked_files():
    out = subprocess.run(
        ["git", "ls-files", "--"] + TRACKED_GLOBS,
        cwd=ROOT, capture_output=True, text=True, check=True,
    )
    return [line for line in out.stdout.splitlines() if line.strip()]


def is_excepted(rel_path, line):
    return any(
        exc["file"] == rel_path and exc["when"](line) for exc in EXCEPTIONS
    )


def check_file(rel_path, text):
    violations = []
    lines = text.split("\n")
    suffix = "." + rel_path.rsplit(".", 1)[-1]

    for lineno, comment in comment_segments(text, suffix):
        line = lines[lineno - 1]
        if is_excepted(rel_path, line):
            continue
        body = comment.lstrip(" \t*-")
        found = None
        if NUMBERED_COMMENT.match(body):
            found = "numbered work-item comment"
        elif WORK_ITEM_CODE.search(comment):
            found = "work-item code '%s'" % WORK_ITEM_CODE.search(comment).group(0)
        elif WORK_ITEM_WORD.search(comment):
            found = "work-item reference '%s'" % WORK_ITEM_WORD.search(comment).group(0)
        elif DECISION_DATE.search(comment):
            found = "decision date '%s'" % DECISION_DATE.search(comment).group(0)
        elif FORBIDDEN_WORDS.search(comment):
            found = "AI-tool reference '%s'" % FORBIDDEN_WORDS.search(comment).group(0)
        if found:
            violations.append((lineno, found, line.strip()))

    # The remaining rules are about the whole line, comment or not: a planning
    # file, a real name or a build-machine path has no place in code either.
    for lineno, line in enumerate(lines, start=1):
        if is_excepted(rel_path, line):
            continue
        for name in FORBIDDEN_FILENAMES:
            if name in line:
                violations.append((lineno, "planning-file reference '%s'" % name, line.strip()))
                break
        else:
            for name in FORBIDDEN_NAMES:
                if name in line:
                    violations.append((lineno, "real name in source", line.strip()))
                    break
            if FORBIDDEN_PATH in line:
                violations.append((lineno, "local build-machine path", line.strip()))
            elif OLD_COPYRIGHT_SPELLING in line:
                violations.append((lineno, "old copyright spelling", line.strip()))
    return violations


def main():
    global ROOT
    ROOT = subprocess.run(
        ["git", "rev-parse", "--show-toplevel"],
        capture_output=True, text=True, check=True,
    ).stdout.strip()

    total = 0
    for rel_path in tracked_files():
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
        print(f"\nsource_hygiene: {total} violation(s)")
        return 1
    print("source_hygiene: clean")
    return 0


if __name__ == "__main__":
    sys.exit(main())
