#!/usr/bin/env python3
"""Fails when docs/automation.md's generated command table no longer matches
docs/automation-manifest.json -- the case a command was added to
src-tauri/src/cli.rs's MANIFEST_ENTRIES (and, with it, to `prompt-saver
describe`'s own output) without running tests/gen_automation_doc.py
afterwards. The block it checks is delimited by the same markers
gen_automation_doc.py writes, so the two never read a different shape.

`cargo test --lib` catches the fixture going stale against the Rust source
(automation_manifest_json_matches_the_generated_source); this script catches
the doc going stale against the fixture. Together they cover the whole chain:
Cmd::Describe -> docs/automation-manifest.json -> docs/automation.md.

Usage: python tests/automation_doc_check.py
"""
import importlib.util
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("gen_automation_doc", ROOT / "tests" / "gen_automation_doc.py")
gen = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gen)


def main():
    import json

    manifest = json.loads(gen.MANIFEST_PATH.read_text(encoding="utf-8"))
    doc = gen.DOC_PATH.read_text(encoding="utf-8")
    if gen.BEGIN not in doc or gen.END not in doc:
        print(f"FAIL: docs/automation.md is missing the generated-block markers")
        return 1
    current_block = gen.BEGIN + doc.split(gen.BEGIN, 1)[1].split(gen.END, 1)[0] + gen.END
    expected_block = gen.generated_block(manifest)
    if current_block != expected_block:
        print("FAIL: docs/automation.md's command table does not match docs/automation-manifest.json.")
        print("Run: python tests/gen_automation_doc.py")
        return 1
    print(f"OK: docs/automation.md's command table matches {len(manifest)} manifest entries")
    return 0


if __name__ == "__main__":
    sys.exit(main())
