"""Run every suite listed in the table of tests/README.md and report the result.

The README table is the authority: this runner reads its commands instead of
keeping a second list. Commands that are tools rather than gates are skipped
(see SKIP). Node checks run in parallel; the Python suites (several of them
start a browser) run one after another.

Usage: python tests/run_all.py [--quick] [--filter text]
  --quick   leave out the slow browser walks (smoke, perf, layout)
  --filter  run only commands containing this text
Exit code 0 only when every command that ran passed.
"""
import argparse
import concurrent.futures
import os
import re
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# Commands in the table that are no pass/fail gate.
SKIP = {
    "python tests/vm/cdp.py list": "VM helper, needs the test VM",
    "python tests/contrast_check.py": "known findings, not part of the suite",
}
SLOW = ("smoke_test.py", "perf_hot_paths.py", "tests/layout/", "expert_rows_test.py",
        "select_picker_test.py", "minor_layout_test.py")


def table_commands():
    with open(os.path.join(ROOT, "tests", "README.md"), encoding="utf-8") as fh:
        text = fh.read()
    cmds = re.findall(r"^\| `([^`]+)`", text, re.M)
    seen, out = set(), []
    for c in cmds:
        if c not in seen:
            seen.add(c)
            out.append(c)
    return out


def run(cmd):
    argv = cmd.split()
    if argv[0] == "python":
        argv[0] = sys.executable
    t0 = time.time()
    try:
        p = subprocess.run(argv, cwd=ROOT, capture_output=True, text=True,
                           encoding="utf-8", errors="replace", timeout=1800)
        code, out = p.returncode, (p.stdout + p.stderr)
    except subprocess.TimeoutExpired:
        code, out = 124, "timeout after 1800 s"
    return cmd, code, time.time() - t0, out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--quick", action="store_true")
    ap.add_argument("--filter", default="")
    args = ap.parse_args()

    cmds = [c for c in table_commands() if c not in SKIP and args.filter in c]
    if args.quick:
        cmds = [c for c in cmds if not any(s in c for s in SLOW)]
    node = [c for c in cmds if c.startswith("node ")]
    py = [c for c in cmds if not c.startswith("node ")]

    results = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=6) as pool:
        for r in pool.map(run, node):
            results.append(r)
            print(("ok   " if r[1] == 0 else "FAIL ") + f"{r[2]:6.1f}s  {r[0]}", flush=True)
    for c in py:
        r = run(c)
        results.append(r)
        print(("ok   " if r[1] == 0 else "FAIL ") + f"{r[2]:6.1f}s  {r[0]}", flush=True)

    failed = [r for r in results if r[1] != 0]
    for cmd, code, _, out in failed:
        print(f"\n--- {cmd} (exit {code}) ---")
        print("\n".join(out.strip().splitlines()[-25:]))
    print(f"\n{len(results) - len(failed)}/{len(results)} passed, {len(SKIP)} skipped")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
