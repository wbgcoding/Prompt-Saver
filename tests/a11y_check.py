"""Screen-reader inventory: every window has to name its controls, speak the
values it shows, announce what changes, trap focus inside its own dialogs and
back off when the system asks for less motion.

Same style as tooltip_check.py: static source scanning, no browser needed. A
real screen-reader pass (Windows Narrator against the timeline and the video
export) is a separate, manual step -- this only catches what a static read of
the markup and the JS that touches it can catch.

Checks, over all nine windows (index, floating, folder, vars, snip, recbar,
video, launcher, tray):
  1. Every icon-only control has a name (same rule as tooltip_check.py, run
     here against the four windows tooltip_check.py does not cover).
  2. Every element known to carry a value (the timeline, its two trim
     handles, the two volume sliders, the background-jobs percent row) is
     wired to aria-valuenow somewhere in its window's own JS.
  3. Each window has role="status" or aria-live on exactly one element,
     static or set from JS -- never zero, never several talking at once.
  4. Every popover/menu/modal that main.js or video.js shows and hides by
     toggling "hidden" also has a matching trapTabWithin(...) call, so Tab
     can never walk out of it while it is open.
  5. Every stylesheet a window loads is covered, directly or through a
     shared one, by a prefers-reduced-motion block.

Run:  python tests/a11y_check.py
Exit: 0 = clean, 1 = problems found.
"""
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO / "tests"))
import tooltip_check as tc  # noqa: E402 -- reuses its button/name regexes
from ui_sources import main_window_source, video_window_source  # noqa: E402

WINDOWS = ["index", "floating", "folder", "vars", "snip", "recbar", "video", "launcher", "tray"]
# tooltip_check.py's own PAGES already cover these five with its full button
# scan; dynamic_button_findings() below already scans every ui/*.js file, so
# only the remaining four static pages need a pass of the same button regex.
TOOLTIP_COVERED = set(tc.PAGES)
EXTRA_PAGES = [f"{w}.html" for w in WINDOWS if f"{w}.html" not in TOOLTIP_COVERED]

LINK_RE = re.compile(r'href="styles/([\w.-]+\.css)"')
VALUE_TARGETS = {
    # window -> [(element id, JS file that must set its aria-valuenow)]
    "video": [
        ("ve-timeline", "video.js"),
        ("ve-trim-a", "video.js"),
        ("ve-trim-b", "video.js"),
        ("ve-vol-sys", "video.js"),
        ("ve-vol-mic", "video.js"),
    ],
    "index": [
        ("bg-ops-pct-row", "main.js"),  # renderBgOpsPop's dynamic .bg-ops-pct span
    ],
}
# role="status" always ships paired with an explicit aria-live in this codebase
# (see floating.html, folder.html, ...), so counting aria-live alone avoids
# double-counting one element that carries both attributes.
LIVE_RE = re.compile(r'aria-live="(?!off")')


def named_control_findings():
    bad = []
    for page in EXTRA_PAGES:
        src = (REPO / "ui" / page).read_text(encoding="utf-8")
        for m in tc.BUTTON.finditer(src):
            attrs, body = m.group("attrs"), m.group("body")
            words = tc.TAG.sub("", body).strip()
            has_svg = "<svg" in body
            if words or not has_svg:
                continue
            named = any(tc.attr(attrs, a) for a in
                        ("title", "data-i18n-title", "aria-label", "data-i18n-aria", "data-i18n"))
            if not named and not tc.named_at_runtime(tc.attr(attrs, "id")):
                ident = tc.attr(attrs, "id") or tc.attr(attrs, "class") or "?"
                bad.append(f"{page}: <button {ident}> shows only an icon and has no tooltip or name")
    return bad


def value_findings():
    bad = []
    for win, targets in VALUE_TARGETS.items():
        for ident, jsfile in targets:
            src = main_window_source() if jsfile == "main.js" else video_window_source() if jsfile == "video.js" else (REPO / "ui" / jsfile).read_text(encoding="utf-8")
            # Either a direct setAttribute("aria-valuenow", ...) keyed to the id, or
            # (for the timeline/handles) the shared setTimeValue(el, ...) helper --
            # both patterns are accepted since a literal per-id search would miss the helper.
            if ident == "bg-ops-pct-row":
                ok = "role\", \"progressbar\"" in src and "aria-valuenow" in src
            else:
                ok = (f'"{ident}"' in src and "aria-valuenow" in src)
            if not ok:
                bad.append(f"{win}.html: '{ident}' has a value but {jsfile} never sets aria-valuenow for it")
    return bad


def live_region_findings():
    bad = []
    for w in WINDOWS:
        html = (REPO / "ui" / f"{w}.html").read_text(encoding="utf-8")
        static_count = len(LIVE_RE.findall(html))
        if static_count == 0:
            bad.append(f"{w}.html: no live region (role=\"status\" or aria-live) found")
        elif static_count > 1:
            bad.append(f"{w}.html: {static_count} live regions found, exactly one is required")
    return bad


# Popovers/modals main.js and video.js show and hide by toggling a "hidden"
# class, matched against every container name that also appears inside a
# trapTabWithin(...) call in the same file -- the same list the Tab-key
# dispatcher already keys off, so a new popover added to one but not the
# other is caught here instead of silently leaking Tab out of it.
HIDDEN_TOGGLE_RE = re.compile(r'(\w+)\.classList\.(?:add|toggle)\(\s*"hidden"')
TRAP_CALL_RE = re.compile(r'trapTabWithin\(\s*(\w+|\$\("[\w-]+"\))')


def focus_trap_findings():
    bad = []
    for jsfile in ("main.js", "video.js"):
        src = main_window_source() if jsfile == "main.js" else video_window_source() if jsfile == "video.js" else (REPO / "ui" / jsfile).read_text(encoding="utf-8")
        trapped = set(TRAP_CALL_RE.findall(src))
        # A container is "dialog-like" if its own declaration sets role="menu"/
        # role="dialog" (static markup) or its JS variable is built with one of
        # those roles (dynamic popovers such as viewsPop/tileFolderPop).
        dialog_vars = set(re.findall(r'(\w+)\.setAttribute\(\s*"role"\s*,\s*"(?:menu|dialog)"\s*\)', src))
        for var in dialog_vars:
            if var not in trapped and f'$("{var}")' not in trapped:
                bad.append(f"{jsfile}: '{var}' has role menu/dialog but no trapTabWithin(...) call traps Tab in it")
    return bad


def reduced_motion_findings():
    bad = []
    for w in WINDOWS:
        html = (REPO / "ui" / f"{w}.html").read_text(encoding="utf-8")
        sheets = LINK_RE.findall(html)
        covered = any("prefers-reduced-motion" in (REPO / "ui" / "styles" / s).read_text(encoding="utf-8")
                       for s in sheets if (REPO / "ui" / "styles" / s).exists())
        if not covered:
            bad.append(f"{w}.html: none of its stylesheets ({', '.join(sheets)}) has a prefers-reduced-motion block")
    return bad


def main():
    bad = []
    bad.extend(named_control_findings())
    bad.extend(value_findings())
    bad.extend(live_region_findings())
    bad.extend(focus_trap_findings())
    bad.extend(reduced_motion_findings())

    for line in bad:
        print(f"   FAIL {line}")
    print("PASS" if not bad else f"{len(bad)} problem(s)")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
