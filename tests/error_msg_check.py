"""Every error string the Rust side can return must either have a translation in
ERROR_MSG_KEYS (ui/core.js) or sit in the allowlist below with the reason it never
reaches a user as text. An unmapped string would otherwise be the one place an English
sentence slips into a German (or any other) UI.

Run:  python tests/error_msg_check.py      Exit: 0 = clean, 1 = problems.
"""
import glob
import os
import re
import sys

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
SRC = os.path.join(ROOT, "src-tauri", "src")

# reason -> error strings (as written in the Rust source, placeholders included).
ALLOW = {
    "the caller branches on the code and shows its own text": [
        "canceled", "cancelled", "demo-mode-active", "focus-failed", "no-target", "import-changed",
        "ocr-no-language", "ocr-index-busy", "voice-no-language", "voice-busy", "voice-online-required",
        "recording-too-young-to-continue", "not-media", "no-source",
        "sync-backup-failed: {e}", "sync-write-failed: {failed}", "too-large:{}",
        "view-full:{deficit}", "view-full", "invalid-exe",
    ],
    "an export graph only this app's own UI builds; a refusal shows the generic export failure": [
        "export graph not allowed: {why}", "export graph not allowed: media file type",
        "unexpected character in the graph", "unterminated link label",
    ],
    "an input only this app's own UI builds, never a user-typed value": [
        "bad drop position", "bad height", "bad payload", "bad popover size", "bad values",
        "bad-action", "no window geometry", "invalid recording target", "invalid update source",
        "invalid number", "invalid OCR capability name", "unknown OCR capability name",
        "invalid speech capability name", "unknown speech capability name",
        "unknown audio source: {source}", "unknown-tool", "no pick session",
        "point outside the captured desktop", "empty path", "bad pinned hash",
        "path does not match the active camera recording",
        "unknown scope {scope:?}, expected all, data or settings",
        "not a folder that can be removed", "the elevated script is too long for a command line",
        "unexpected PowerShell output shape", "invalid camera chunk",
    ],
    "raw reason for the crash log or the technical details row, wrapped in a mapped code": [
        "serialize failed", "write failed ({:?})", "timeout",
        "copy {sub}/: {e}", "the new folder lies inside {sub}/, which has to be copied",
        "the stored prompts are unreadable and cannot be rewritten",
        "the updated paths could not be saved",
    ],
    "background save of editor state, the caller ignores the failure": [
        "db unavailable",
    ],
    "command line tool only, printed in the terminal": [
        "name is empty", "name longer than {MAX_NAME_LEN} characters",
        "text longer than {MAX_TEXT_LEN} characters",
        "path is empty, contains a NUL byte, or is too long",
        '"{raw}" is relative to a drive; give the full path with a backslash after the drive letter',
        "could not open the restored store", "could not write the restored store",
    ],
    "demo mode switch, only reachable from the command line": [
        "a folder already exists at the demo data path and was not created by demo mode; refusing to touch it",
        "could not prepare the demo store",
    ],
}

# Which Rust constructs produce an error string: Err("..".to_string()), Err(format!(".."..)),
# ok_or("..") and the map_err forms of the same.
LIT = r'"((?:[^"\\]|\\.)*)"'
CONV = r"\s*\.(?:to_string|into|to_owned)\(\)"
PATTERNS = [re.compile(p, re.S) for p in (
    r"Err\(\s*" + LIT + CONV + r"\s*\)",
    r"Err\(\s*format!\(\s*" + LIT,
    r"ok_or(?:_else)?\(\s*(?:\|\|\s*)?" + LIT + CONV,
    r"ok_or\(\s*" + LIT + r"\s*\)",
    r"map_err\(\s*\|[^|]*\|\s*" + LIT + CONV,
    r"map_err\(\s*\|[^|]*\|\s*format!\(\s*" + LIT,
)]


def unescape(s):
    return s.replace('\\"', '"').replace("\\\\", "\\")


def rust_errors():
    out = {}
    for path in sorted(glob.glob(os.path.join(SRC, "*.rs"))):
        name = os.path.basename(path)
        if name.endswith("_tests.rs") or name == "test_env.rs":
            continue
        with open(path, encoding="utf8") as fh:
            text = fh.read()
        mod = re.search(r"\n#\[cfg\(test\)\]\s*\nmod \w+ \{", text)  # inline unit-test module
        if mod:
            text = text[: mod.start()]
        for pat in PATTERNS:
            for m in pat.finditer(text):
                line = text.count("\n", 0, m.start()) + 1
                out.setdefault(unescape(m.group(1)), []).append(f"{name}:{line}")
    return out


def mapped_keys():
    with open(os.path.join(ROOT, "ui", "errors.js"), encoding="utf8") as fh:
        core = fh.read()
    start = core.index("export const ERROR_MSG_KEYS = {")
    end = core.index("\n};", start)
    return dict(re.findall(r'^\s*"((?:[^"\\]|\\.)*)":\s*"(\w+)"', core[start:end], re.M)), core


# A raw error (String(err), err.message) handed to a toast or put into the page is English
# whatever the app language is: it has to go through errMessage/errText (ui/errors.js).
ERR_VAR = r"(?:err|e|e2|error|ex|reason)"
# String(err) used as a value; String(err).includes(..) / .slice(..) only inspects it.
RAW_ERR = re.compile(rf"\bString\({ERR_VAR}\)(?!\.)|\b{ERR_VAR}\.message\b")
# hotkeyMessage() turns the code into a translated text.
MAPPED_ERR = re.compile(rf"hotkeyMessage\(String\({ERR_VAR}\)\)")
SINKS = re.compile(
    r"(?<![A-Za-z_.])(?:toast|showError|infoDialog|renderLangsRetry|alert)\(|\.(?:textContent|innerText)\s*=")
# (file, text the offending line contains) -> why a raw error is fine there.
RAW_OK = []


def raw_error_display():
    out = []
    for path in sorted(glob.glob(os.path.join(ROOT, "ui", "*.js"))):
        name = "ui/" + os.path.basename(path)
        if name in ("ui/help.js", "ui/errors.js"):
            continue
        with open(path, encoding="utf8") as fh:
            lines = fh.read().split("\n")
        for no, line in enumerate(lines, 1):
            code = line.split("//")[0] if line.lstrip().startswith("//") else line
            code = MAPPED_ERR.sub("hotkeyMessage()", code)
            m = SINKS.search(code)
            if not m or not RAW_ERR.search(code[m.start():]):
                continue
            if any(f == name and frag in line for f, frag, _ in RAW_OK):
                continue
            out.append(f"{name}:{no}: a raw error reaches the user, route it through errMessage/errText: {line.strip()[:110]}")
    return out


def main():
    problems = []
    keys, core = mapped_keys()
    unescaped = {unescape(k): v for k, v in keys.items()}
    with open(os.path.join(ROOT, "ui", "i18n", "en.js"), encoding="utf8") as fh:
        en = fh.read()
    for lit, key in unescaped.items():
        if not re.search(rf"^\s*{key}:", en, re.M):
            problems.append(f"ERROR_MSG_KEYS maps {lit!r} to {key}, which is no en string")
    fb = re.search(r'"errUnexpected"', core)
    if not fb:
        problems.append("ui/errors.js has no errUnexpected fallback")
    problems.extend(raw_error_display())

    allowed = {}
    for reason, lits in ALLOW.items():
        for lit in lits:
            if lit in allowed:
                problems.append(f"allowlist lists {lit!r} twice")
            if lit in unescaped:
                problems.append(f"{lit!r} is both mapped and allowlisted")
            allowed[lit] = reason

    found = rust_errors()
    for lit, where in sorted(found.items()):
        if lit not in unescaped and lit not in allowed:
            problems.append(f"{lit!r} ({', '.join(where[:3])}) is in neither ERROR_MSG_KEYS nor the allowlist")
    for lit in allowed:
        if lit not in found:
            problems.append(f"allowlisted {lit!r} no longer exists in src-tauri/src")

    for p in problems:
        print("FAIL:", p)
    print(f"{len(found)} Rust error strings checked, {len(unescaped)} mapped, {len(allowed)} allowlisted, {len(problems)} problem(s)")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
