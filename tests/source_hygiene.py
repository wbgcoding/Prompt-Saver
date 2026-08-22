#!/usr/bin/env python3
"""Guards the shipped source tree against process traces: numbered work-item
comments, AI-tool name-drops, references to internal planning files, real
names / local paths, and the old copyright spelling.
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

NUMBERED_COMMENT = re.compile(r"^\s*(?://|/\*)\s*\d+(\s*/\s*\d+)*:")
NUMBERED_COMMENT_PAREN = re.compile(r"^\s*(?://|/\*)\s*\d+(\s*/\s*\d+)*\s*\(")
# A work-item code buried in a comment anywhere, not just at the start -- e.g. a
# section-divider comment like "// ---------- F25: folder commands ----------".
WORK_ITEM_CODE = re.compile(r"\b[A-Z]\d+:")

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
]


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
    lines = text.splitlines()
    for lineno, line in enumerate(lines, start=1):
        if is_excepted(rel_path, line):
            continue
        if NUMBERED_COMMENT.match(line) or NUMBERED_COMMENT_PAREN.match(line):
            violations.append((lineno, "numbered work-item comment", line.strip()))
            continue
        comment_at = line.find("//")
        if comment_at != -1:
            wm = WORK_ITEM_CODE.search(line, comment_at)
            if wm:
                violations.append((lineno, f"work-item code '{wm.group(0)}'", line.strip()))
                continue
        m = FORBIDDEN_WORDS.search(line)
        if m:
            violations.append((lineno, f"AI-tool reference '{m.group(0)}'", line.strip()))
            continue
        for name in FORBIDDEN_FILENAMES:
            if name in line:
                violations.append((lineno, f"planning-file reference '{name}'", line.strip()))
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
        violations = check_file(rel_path, text)
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
