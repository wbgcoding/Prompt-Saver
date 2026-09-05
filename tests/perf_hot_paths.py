"""Hot-path timing: grid, library, search and expert menu with 1000 prompts.

Point 219 -- measured, not guessed. Boots the real UI headless (same mechanism
as tests/ui/smoke_test.py) against a stubbed backend seeded with 1000 synthetic
prompts (100 of them with a stored image), then times, from the real DOM's own
perspective (click to fully-rendered), the four hot paths: opening the library
(1000 unfiltered rows), typing a search query, switching views (forces a full
grid re-render), and opening the expert menu.

This does not hard-fail on ordinary machine-to-machine speed variance -- only
on numbers far enough past a generous target to mean an actual regression, not
noise. Skips cleanly when no Chromium browser is installed, same as smoke_test.

Run:  python tests/perf_hot_paths.py
Exit: 0 = all under target (or skipped), 1 = something is genuinely slow.
"""
import json
import re
import shutil
import subprocess
import sys
import tempfile
from html import unescape
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
HERE = Path(__file__).parent

CHROME_CANDIDATES = [
    r"C:\Program Files\Google\Chrome\Application\chrome.exe",
    r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
    r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
    r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
]

# Generous on purpose: this catches an actual O(n^2)-class regression, not
# machine-to-machine noise. A number that stays comfortably under these on a
# throwaway headless run is nowhere near what a user would call "slow".
TARGETS_MS = {
    "librarySearch": 400,
    "libraryOpen1000": 800,
    "viewSwitch1000": 800,
    "expertMenuOpen": 800,
    # The video editor opens a full 10-minute 1080p30 recording (filmstrip
    # populated, both waveforms drawn) in well under 3s.
    "videoEditorOpen10Min1080p30": 3000,
}

# Appended after tauri_stub.js's own script tag -- overrides its 12-prompt
# fixture with 1000 synthetic prompts (100 carrying an image) across two views,
# and turns multiView on so the view tabs used for the grid-switch timing exist.
PERF_SEED = """
<script>
(function () {
  const N = 1000;
  const WITH_IMAGES = 100;
  const PROMPTS_1000 = Array.from({ length: N }, (_, i) => ({
    id: "perf" + i,
    name: "Prompt " + (i + 1),
    text: "Sample body number " + (i + 1) + ". Rewrite this so it reads clearly. ".repeat(4),
    color: ["", "#3b82f6", "#ef4444", "#10b981", "#f59e0b"][i % 5],
    favorite: i % 25 === 0,
    tags: i % 3 === 0 ? ["Work"] : i % 3 === 1 ? ["Draft"] : [],
    notes: "",
    hotkey: "",
    file_path: "",
    variants: [],
    rotate_mode: "",
    has_image: i < WITH_IMAGES,
    image_rev: i < WITH_IMAGES ? 1 : 0,
    show_image: i < WITH_IMAGES,
  }));
  const LAYOUT_HOME = {};
  // Only the first 150 cells of a 10x15 grid are placed -- the rest live in the
  // library only, same as a real library that outgrew its grid.
  PROMPTS_1000.slice(0, 150).forEach((p, i) => { LAYOUT_HOME[p.id] = [i % 10, Math.floor(i / 10)]; });

  SETTINGS.views = [
    { id: "home", name: "Home", cols: 10, rows: 15, layouts: { "10x15": LAYOUT_HOME }, color: "", members: PROMPTS_1000.map((p) => p.id) },
    { id: "work", name: "Work", cols: 10, rows: 15, layouts: {}, color: "", members: [] },
  ];
  SETTINGS.active_view = "home";
  SETTINGS.ui_flags.multiView = true;
  SETTINGS.tile_folders = [];
  SETTINGS.tag_colors = { work: "#3b82f6" };

  RESULTS.get_state = () => ({ prompts: PROMPTS_1000, settings: JSON.parse(JSON.stringify(SETTINGS)) });
  RESULTS.set_active_view = (a) => {
    if (SETTINGS.views.some((v) => v.id === (a && a.id))) SETTINGS.active_view = a.id;
    return JSON.parse(JSON.stringify(SETTINGS));
  };
})();
</script>
"""

PROBE = """
<script>
(async () => {
  const out = { timings: {}, errors: [], counts: {} };
  window.addEventListener("error", (e) => out.errors.push(String(e.message)));
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const waitFor = async (check, timeoutMs) => {
    const start = performance.now();
    while (performance.now() - start < timeoutMs) {
      if (check()) return true;
      await sleep(10);
    }
    return false;
  };
  const rowCount = () => document.querySelectorAll("#library-list .lib-item").length;

  await waitFor(() => document.querySelectorAll(".tile").length > 0, 20000);
  await sleep(300);

  // 1) Library open -- 1000 unfiltered rows.
  let t0 = performance.now();
  document.getElementById("library-btn").click();
  await waitFor(() => rowCount() >= 900, 15000);
  out.timings.libraryOpen1000 = performance.now() - t0;
  out.counts.libraryOpen1000 = rowCount();

  // 2) Search filter, library already open.
  const q = document.getElementById("library-q");
  const nativeSetter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, "value").set;
  t0 = performance.now();
  nativeSetter.call(q, "Sample body number 5");
  q.dispatchEvent(new Event("input", { bubbles: true }));
  await waitFor(() => rowCount() < 900, 15000);
  out.timings.librarySearch = performance.now() - t0;
  out.counts.librarySearch = rowCount();
  nativeSetter.call(q, "");
  q.dispatchEvent(new Event("input", { bubbles: true }));
  await waitFor(() => rowCount() >= 900, 15000);
  document.getElementById("library-close").click();
  await sleep(150);

  // 3) Grid re-render via a view switch (1000-prompt store, 150-tile grid).
  t0 = performance.now();
  const workTab = document.querySelector('.view-btn[data-view-id="work"]');
  workTab.click();
  await waitFor(() => document.querySelector('.view-btn[data-view-id="work"]').classList.contains("active"), 15000);
  await sleep(50); // renderGrid's own async tail (image lookups etc.)
  out.timings.viewSwitch1000 = performance.now() - t0;
  const homeTab = document.querySelector('.view-btn[data-view-id="home"]');
  homeTab.click();
  await waitFor(() => homeTab.classList.contains("active"), 15000);

  // 4) Expert menu open.
  t0 = performance.now();
  document.getElementById("gear").click();
  await sleep(50);
  document.getElementById("expert-open").click();
  await waitFor(() => document.getElementById("expert-flags").children.length > 0, 15000);
  out.timings.expertMenuOpen = performance.now() - t0;

  document.body.insertAdjacentHTML("beforeend", "<pre id=\\"PERF\\">" + JSON.stringify(out) + "</pre>");
})();
</script>
"""

# Video editor: no PERF_SEED override needed here -- tauri_stub.js's own
# probe_recording/generate_filmstrip/generate_waveform_peaks fixtures already
# model a 10-minute 1080p30 recording (600000 ms, 1920x1080, system + mic audio),
# which is exactly the non-degenerate case this timing wants. The window pulls
# its source on boot (video_editor_source), so the timer just has to start
# essentially at navigation and wait for the populated result -- same "click (or
# here, boot) to fully-rendered" contract the four cases above use.
VIDEO_PROBE = """
<script>
(async () => {
  const out = { timings: {}, errors: [], counts: {} };
  window.addEventListener("error", (e) => out.errors.push(String(e.message)));
  window.addEventListener("unhandledrejection", (e) =>
    out.errors.push(String((e.reason && e.reason.message) || e.reason)));
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const waitFor = async (check, timeoutMs) => {
    const start = performance.now();
    while (performance.now() - start < timeoutMs) {
      if (check()) return true;
      await sleep(10);
    }
    return false;
  };
  // Both waveform canvases carry a hardcoded placeholder width of 10 in the
  // markup, so "still 10" is what an un-rendered waveform looks like.
  const ready = () =>
    document.getElementById("ve-stage-msg").classList.contains("hidden") &&
    document.querySelectorAll("#ve-filmstrip .ve-thumb").length > 0 &&
    document.getElementById("ve-wave-sys").width > 10 &&
    document.getElementById("ve-wave-mic").width > 10;

  const t0 = performance.now();
  await waitFor(ready, 15000);
  out.timings.videoEditorOpen10Min1080p30 = performance.now() - t0;
  out.counts.videoEditorOpen10Min1080p30 = document.querySelectorAll("#ve-filmstrip .ve-thumb").length;

  document.body.insertAdjacentHTML("beforeend", "<pre id=\\"PERF\\">" + JSON.stringify(out) + "</pre>");
})();
</script>
"""


def main():
    browser = next((p for p in CHROME_CANDIDATES if Path(p).exists()), None)
    if not browser:
        print("SKIP: no Chromium browser found")
        return 0

    tmp = Path(tempfile.mkdtemp(prefix="ps-perf-"))
    try:
        shutil.copytree(REPO / "ui", tmp / "ui")
        shutil.copy(HERE / "ui" / "tauri_stub.js", tmp / "ui" / "tauri_stub.js")
        page = tmp / "ui" / "index.html"
        html = page.read_text(encoding="utf-8")
        html = html.replace(
            "<script",
            '<script src="tauri_stub.js"></script>\n' + PERF_SEED + "\n<script",
            1,
        )
        html = html.replace("</body>", PROBE + "</body>")
        page.write_text(html, encoding="utf-8")

        proc = subprocess.run(
            [browser, "--headless=new", "--disable-gpu", "--no-first-run",
             f"--user-data-dir={tmp / 'profile'}", "--allow-file-access-from-files",
             "--window-size=1280,900", "--virtual-time-budget=60000",
             "--dump-dom", page.as_uri()],
            capture_output=True, text=True, encoding="utf-8", errors="replace",
            timeout=120)
        m = re.search(r'<pre id="PERF">(\{.*?\})</pre>', proc.stdout, re.S)
        if not m:
            print("FAIL: the UI produced no perf result")
            return 1
        r = json.loads(unescape(m.group(1)))

        # Video editor: its own boot, direct-loaded the same way index.html is
        # above (no small-viewport iframe trick -- this window is never below
        # 760x520, well past whatever floor that trick works around).
        video_page = tmp / "ui" / "video.html"
        video_html = video_page.read_text(encoding="utf-8")
        video_html = video_html.replace(
            "<script", '<script src="tauri_stub.js"></script>\n<script', 1)
        video_html = video_html.replace("</body>", VIDEO_PROBE + "</body>")
        video_page.write_text(video_html, encoding="utf-8")

        video_proc = subprocess.run(
            [browser, "--headless=new", "--disable-gpu", "--no-first-run",
             f"--user-data-dir={tmp / 'profile-video'}", "--allow-file-access-from-files",
             "--window-size=1100,720", "--virtual-time-budget=30000",
             "--dump-dom", video_page.as_uri()],
            capture_output=True, text=True, encoding="utf-8", errors="replace",
            timeout=120)
        vm = re.search(r'<pre id="PERF">(\{.*?\})</pre>', video_proc.stdout, re.S)
        if not vm:
            print("FAIL: the video editor produced no perf result")
            return 1
        r_video = json.loads(unescape(vm.group(1)))
        r["timings"].update(r_video["timings"])
        r["counts"].update(r_video["counts"])
        r["errors"].extend(r_video["errors"])
    finally:
        shutil.rmtree(tmp, ignore_errors=True)

    if r["errors"]:
        for e in r["errors"]:
            print(f"FAIL JS error: {e}")
        return 1

    failed = 0
    for name, target in TARGETS_MS.items():
        ms = r["timings"].get(name)
        if ms is None:
            print(f"FAIL {name}: no timing recorded")
            failed += 1
            continue
        count = r["counts"].get(name)
        extra = f" ({count} rows)" if count is not None else ""
        status = "ok" if ms <= target else "SLOW"
        print(f"{status:4} {name}: {ms:.0f} ms (target <= {target} ms){extra}")
        if ms > target:
            failed += 1

    print("PASS" if not failed else f"FAIL: {failed} hot path(s) over target")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
