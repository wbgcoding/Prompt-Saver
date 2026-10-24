#!/usr/bin/env python3
"""Checks that the third-party credits list names exactly what ships: nothing
missing, nothing extra, every version right.

The list is THIRD-PARTY-NOTICES.md (the same rows the app shows in About >
Third-party licenses). build.rs writes it from `cargo metadata`; this check
compares it against a second, independent source so a bug in that generator,
a stale file after a dependency change, or a hand edit is caught:

Rust crates: `cargo tree` for the Windows target, normal dependencies only,
without proc-macro crates (those run at compile time and are not linked). Every
crate in that tree must have a row with exactly its versions, and every crate
row must be in that tree.

Everything else (the hand-maintained rows from src-tauri/third-party-manual.json)
needs proof in the sources that it really ships, and everything the app embeds
or bundles from outside needs a row:
  - PDFium: the DLL compiled in with include_bytes!
  - SQLite: rusqlite built with its "bundled" feature
  - FFmpeg: ffmpeg.exe in the installer resources or the consented download
  - every bundle resource and every embedded .dll/.exe must belong to a row
"""
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TAURI = ROOT / "src-tauri"
TARGET = "x86_64-pc-windows-msvc"

# Bundle resources and embedded binaries, mapped to the row that credits them.
# A file here that is not in the list, or a row whose proof is gone, fails.
FILE_OWNERS = {
    "ffmpeg.exe": "FFmpeg",
    "ffmpeg-LICENSE": "FFmpeg",
    "pdfium.dll": "PDFium",
}


def notice_rows():
    text = (ROOT / "THIRD-PARTY-NOTICES.md").read_text(encoding="utf-8")
    rows = {}
    for line in text.splitlines():
        m = re.match(r"^\| (?!Name \||---)([^|]+?) \| ([^|]+?) \| ([^|]+?) \|$", line)
        if m:
            rows[m.group(1)] = {v.strip() for v in m.group(2).split(",")}
    return rows


def shipped_crates():
    out = subprocess.run(
        ["cargo", "tree", "--manifest-path", str(TAURI / "Cargo.toml"), "--locked",
         "--target", TARGET, "-e", "normal,no-proc-macro", "--prefix", "none", "-f", "{p}"],
        capture_output=True, text=True, encoding="utf-8",
    )
    if out.returncode != 0:
        sys.exit("credits_check: cargo tree failed:\n" + out.stderr[-2000:])
    crates = {}
    lines = out.stdout.splitlines()
    for line in lines[1:]:  # the first line is the app itself
        m = re.match(r"^(\S+) v(\S+)", line)
        if m:
            crates.setdefault(m.group(1), set()).add(m.group(2))
    return crates


def main():
    problems = []
    rows = notice_rows()
    manual = {e["name"] for e in json.loads((TAURI / "third-party-manual.json").read_text(encoding="utf-8"))}
    crates = shipped_crates()

    for name, versions in sorted(crates.items()):
        if name not in rows:
            problems.append(f"too few: crate {name} {', '.join(sorted(versions))} ships but has no row")
        elif rows[name] != versions:
            problems.append(f"wrong version: {name} is listed as {', '.join(sorted(rows[name]))}, ships as {', '.join(sorted(versions))}")
    for name in sorted(set(rows) - set(crates) - manual):
        problems.append(f"too many: row {name} is not linked into the app")

    for name in sorted(manual - set(rows)):
        problems.append(f"too few: {name} is in third-party-manual.json but not in the list (rebuild)")

    sources = "\n".join(p.read_text(encoding="utf-8") for p in (TAURI / "src").glob("*.rs"))
    cargo_toml = (TAURI / "Cargo.toml").read_text(encoding="utf-8")
    conf = json.loads((TAURI / "tauri.conf.json").read_text(encoding="utf-8"))
    resources = conf.get("bundle", {}).get("resources") or []
    if isinstance(resources, dict):
        resources = list(resources.keys())
    embedded = re.findall(r'include_bytes!\("([^"]+\.(?:dll|exe))"\)', sources)

    proofs = {
        "PDFium": bool(re.search(r'include_bytes!\("[^"]*pdfium\.dll"\)', sources)),
        "SQLite": bool(re.search(r'^rusqlite\s*=.*"bundled"', cargo_toml, re.M)) and "rusqlite" in crates,
        "FFmpeg": "ffmpeg.exe" in resources or "codexffmpeg/releases/download" in sources,
    }
    for name in sorted(manual):
        if name not in proofs:
            problems.append(f"too many: {name} has no ship proof in this check (add one, or drop the row)")
        elif not proofs[name]:
            problems.append(f"too many: {name} is listed but no longer ships")

    for f in [Path(r).name for r in resources] + [Path(e).name for e in embedded]:
        owner = FILE_OWNERS.get(f)
        if owner is None:
            problems.append(f"too few: {f} ships with the app but no row credits it (map it in FILE_OWNERS)")
        elif owner not in rows:
            problems.append(f"too few: {f} ships but its row {owner} is missing")

    for p in problems:
        print("FAIL:", p)
    print(f"credits_check: {len(crates)} crates, {len(manual)} other components, {len(problems)} problem(s)")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
