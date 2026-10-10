"""Main-window suite: the size walk over index.html plus the Italian pass. The probe is assembled from
the fragments in walk/."""
import json
import re

from .harness import LANG_PRELOADS, SIZES, WRAPPER, unescape, run_chrome
from .walk import SECTIONS


HEAD = """
<script>
(async () => {
  const out = { errors: [], overflow: [], tabs: [], rawKeys: [], layout: [] };
  // 434: an uncaught exception anywhere below used to kill the whole probe
  // silently ("no probe result", nothing runs) -- wrapping the walk means a
  // real regression shows up as a reported error instead of a bare timeout.
  try {
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const click = async (id) => { const e = document.getElementById(id); if (e) { e.click(); await sleep(200); } };
  // Placeholders are only ever asked in the standalone window: the stub keeps the
  // payload of open_var_window, and the window answers with a vars-filled event.
  const varReq = () => { try { return JSON.parse(window.__VAR_PAYLOAD__ || 'null'); } catch (e) { return null; } };
  const varAnswer = async (values) => {
    window.__FIRE__('vars-filled', JSON.stringify({ values, remember: values.map(() => false) }));
    await sleep(250);
    window.__VAR_PAYLOAD__ = undefined;
  };
  const varCancel = async () => { window.__FIRE__('vars-filled', null); await sleep(200); window.__VAR_PAYLOAD__ = undefined; };
  // The header padding stays at the former default of the removed setting (measured
  // before the setting went: 10px).
  { const hp = parseFloat(getComputedStyle(document.querySelector('.header')).paddingTop);
    if (Math.abs(hp - 10) > 0.5) out.errors.push('the header padding is ' + hp + 'px, expected 10px'); }
  // A name or body that looks like markup must render as literal text everywhere
  // it shows up (grid tile, library row), never as a real element.
  const XSS_NAME = 'Injected <img src=x onerror=alert(1)> name';
  const XSS_TEXT = 'body <script>alert(2)<' + '/script> tail';

  // ---- layout guards -------------------------------------------------------
  // Everything below answers one question: on a small window, can the user still
  // reach and read what is on screen?
  const onScreen = (el) => {
    const s = getComputedStyle(el);
    return s.display !== 'none' && s.visibility !== 'hidden' && s.pointerEvents !== 'none' &&
      Number(s.opacity) > 0.05 && el.getClientRects().length;
  };
  const sig = (el) => (el.id ? '#' + el.id : '') +
    (typeof el.className === 'string' && el.className.trim()
      ? '.' + el.className.trim().split(/\\s+/)[0] : el.tagName);
  // Content parked past the right edge of a SIDEWAYS-scrollable container (the view
  // tab strip) is reachable by scrolling. Behind an overflow-x:hidden ancestor it is
  // gone for good, so that still counts as an escape.
  // Only the NEAREST ancestor that controls horizontal overflow decides - an outer
  // scroller cannot reveal what an inner overflow-x:hidden box already cut off.
  const inScroller = (el) => {
    // Every SVG element computes overflow-x: hidden by the UA stylesheet, whether
    // or not its author ever clipped anything -- an icon <path> would otherwise
    // always "find" that default on its own <svg> parent before ever reaching a
    // real scroll container further up, permanently defeating this check for
    // every icon button (178 uncovered this on the screenshot editor toolbar,
    // whose icon buttons sit in a genuinely horizontally-scrollable row).
    // SVG's own viewport clipping is a distinct rendering concept from a CSS
    // overflow-driven scrollbar, so the walk starts above the whole SVG subtree.
    let start = el;
    while (start && start.namespaceURI === 'http://www.w3.org/2000/svg') start = start.parentElement;
    for (let p = start ? start.parentElement : null; p; p = p.parentElement) {
      const ox = getComputedStyle(p).overflowX;
      if (ox === 'auto' || ox === 'scroll') return true;
      if (ox === 'hidden' || ox === 'clip') return false;
    }
    return false;
  };
  // Hit-testable: a real pointer can land on it. Scrolling to it first is allowed;
  // sitting outside the window, or under something else, is not.
  const canHit = (el) => {
    try { el.scrollIntoView({ block: 'nearest', inline: 'nearest' }); } catch (e) {}
    const r = el.getBoundingClientRect();
    if (!r.width || !r.height) return false;
    const x = r.left + r.width / 2, y = r.top + r.height / 2;
    if (x < 0 || y < 0 || x > innerWidth || y > innerHeight) return false;
    const hit = document.elementFromPoint(x, y);
    if (!hit) return false;
    if (hit === el || el.contains(hit) || hit.contains(el)) return true;
    // Another dialog stacked on top is doing its job, not covering this one by accident.
    const theirs = hit.closest('.overlay');
    return !!theirs && theirs !== el.closest('.overlay');
  };
  const layoutScan = (where) => {
    const add = (m) => { const s = where + ': ' + m; if (!out.layout.includes(s)) out.layout.push(s); };
    if (document.documentElement.scrollWidth > innerWidth + 1) {
      add('the page scrolls horizontally (' + document.documentElement.scrollWidth + ' > ' + innerWidth + ')');
    }
    // A dialog holding more than it can show must be able to scroll to the rest -
    // on a short window the inner lists keep a height floor, so the dialog is where
    // the overflow ends up.
    for (const box of document.querySelectorAll('.overlay:not(.hidden) > .modal')) {
      const oy = getComputedStyle(box).overflowY;
      if (box.scrollHeight > box.clientHeight + 2 && oy !== 'auto' && oy !== 'scroll') {
        add(sig(box) + ' holds ' + box.scrollHeight + 'px of content in ' +
          box.clientHeight + 'px and cannot scroll to it');
      }
    }
    // Every popover goes through one placement rule and must land fully inside the
    // window - vertically too. A dropdown opening past the bottom edge is the theme
    // editor / value picker defect, and it is invisible to the horizontal scan.
    for (const el of document.querySelectorAll(
        '.num-pop, .color-pop, .tag-suggest, .tag-pop, .mini-menu, .ctx-menu, ' +
        '.tile-folder-pop, .toolbar-menu, .suggest-pop, .pick-history-pop')) {
      if (!onScreen(el)) continue;
      const r = el.getBoundingClientRect();
      if (!r.width || !r.height) continue;
      if (r.bottom > innerHeight + 1 || r.top < -1 || r.right > innerWidth + 1 || r.left < -1) {
        add(sig(el) + ' opens outside the window (' + Math.round(r.left) + ',' + Math.round(r.top) +
          '..' + Math.round(r.right) + ',' + Math.round(r.bottom) + ' of ' + innerWidth + 'x' + innerHeight + ')');
      }
    }
    for (const el of document.querySelectorAll('body *')) {
      if (!onScreen(el)) continue;
      const r = el.getBoundingClientRect();
      if (!r.width || !r.height) continue;
      if ((r.right > innerWidth + 1 || r.left < -1) && !inScroller(el)) {
        add(sig(el) + ' is outside the window (x ' + Math.round(r.left) + '..' +
          Math.round(r.right) + ' of ' + innerWidth + ')');
      }
      // Cut off sideways by a box that cannot scroll to it: the control is gone, not
      // merely off-view. Only interactive elements - labels legitimately ellipsise.
      if (el.matches('button, input, select, textarea')) {
        for (let p = el.parentElement; p; p = p.parentElement) {
          const ox = getComputedStyle(p).overflowX;
          if (ox === 'auto' || ox === 'scroll') break;
          if (ox === 'hidden' || ox === 'clip') {
            const pr = p.getBoundingClientRect();
            if (r.right > pr.right + 1 || r.left < pr.left - 1) {
              add(sig(el) + ' is clipped away by ' + sig(p));
            }
            break;
          }
        }
      }
      // A flex item with its own overflow bottoms out at zero height, which turns a
      // list into an unusable sliver instead of letting its dialog scroll.
      const s = getComputedStyle(el);
      if (/auto|scroll/.test(s.overflowY) && el.scrollHeight > 40 &&
          el.clientHeight > 0 && el.clientHeight < 40) {
        add(sig(el) + ' is crushed to ' + el.clientHeight + 'px around ' +
          el.scrollHeight + 'px of content');
      }
    }
    // Rows of an open dialog are stacked, never layered: a row squeezed below its
    // content is drawn OVER the row beneath it (the colour swatches used to end up
    // under the media block on a small window). Only in-flow children count.
    for (const box of document.querySelectorAll('.overlay:not(.hidden) .modal, .overlay:not(.hidden) .modal *')) {
      if (!(box instanceof HTMLElement)) continue; // an <svg> lays its own shapes out
      const st = getComputedStyle(box);
      if (st.display !== 'block' && st.display !== 'flex') continue;
      if (st.display === 'flex' && st.flexDirection.startsWith('row')) continue;
      const kids = [...box.children].filter((e) => e instanceof HTMLElement &&
        e.getClientRects().length && getComputedStyle(e).position === 'static');
      for (let i = 1; i < kids.length; i++) {
        const a = kids[i - 1].getBoundingClientRect(), b = kids[i].getBoundingClientRect();
        if (a.height > 0 && b.height > 0 && b.top < a.bottom - 1) {
          add(sig(kids[i - 1]) + ' and ' + sig(kids[i]) + ' overlap by ' +
            Math.round(a.bottom - b.top) + 'px');
        }
      }
    }
  };
  // A collapsible row is its own overflow box, so a flex parent can squeeze it below
  // its head and leave a 6px sliver where a clickable row belongs.
  const collapseRows = (where) => {
    for (const row of document.querySelectorAll(where === 'editor' ? '#modal .modal-collapse' : '.modal-collapse')) {
      const head = row.querySelector('.modal-collapse-head');
      if (!head || !onScreen(row)) continue;
      const h = row.getBoundingClientRect().height;
      if (h + 1 < head.scrollHeight) {
        out.layout.push('prompt editor: a collapsible row is squeezed to ' + Math.round(h) +
          'px around a ' + head.scrollHeight + 'px head');
      }
    }
  };
  const mustReach = (where, sel) => {
    for (const el of document.querySelectorAll(sel)) {
      if (onScreen(el) && !canHit(el)) {
        out.layout.push(where + ': "' + (el.id || el.textContent || sel).trim().slice(0, 20) + '" cannot be clicked');
      }
    }
  };

  // Startup reveals the window from a nested requestAnimationFrame. In a small
  // headless frame Chromium stops producing frames once the page is quiet, so that
  // second callback never runs and the walk below would start from a dead page.
  // Dirtying an unused custom property keeps frames coming; it touches no geometry.
  const heartbeat = setInterval(() => {
    document.documentElement.style.setProperty('--smoke-tick', String(Date.now()));
    document.body.getBoundingClientRect();
  }, 40);
  await sleep(700);
  try {
"""


TAIL = """\
  } catch (e) { out.errors.push('probe: ' + e.message); }
  for (const el of document.querySelectorAll('.overlay *, #expert-flags *')) {
    const r = el.getBoundingClientRect();
    if (r.width && r.right > innerWidth + 1) {
      out.overflow.push((el.id || el.className || el.tagName) + ' right=' + Math.round(r.right));
    }
  }
  // t() falls back to the key name, so an untranslated string shows up verbatim.
  for (const el of document.querySelectorAll('body *')) {
    const txt = (el.children.length ? '' : el.textContent || '').trim();
    if (/^(flag|val|tip|exp|stat|backup|help)[A-Z][A-Za-z]+$/.test(txt)) out.rawKeys.push(txt);
  }
  out.errors.push(...(window.__SMOKE_ERRORS__ || []));
  } catch (e) {
    out.errors.push('UNCAUGHT: ' + (e && e.message) + (e && e.stack ? ' | ' + String(e.stack).split(String.fromCharCode(10)).slice(0, 3).join(' <- ') : ''));
  }
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(out);
  document.body.appendChild(pre);
})();
</script>
"""


PROBE = HEAD + "".join(SECTIONS) + TAIL


def run(browser, tmp):
    failed = 0
    page = tmp / "ui" / "index.html"
    html = page.read_text(encoding="utf-8")
    html = html.replace("<script", '<script src="tauri_stub.js"></script>' + LANG_PRELOADS + '\n<script', 1)
    html = html.replace("</body>", PROBE + "</body>")
    page.write_text(html, encoding="utf-8")

    for w, h in SIZES:
        size_tag = f"{w}x{h}"
        wrapper = tmp / f"wrap-{size_tag}.html"
        wrapper.write_text(WRAPPER % (w, h), encoding="utf-8")
        # An isolated profile is required or Chrome hands the URL to a running
        # instance and prints nothing.
        # The probe walks more UI than it used to; 30s of virtual time ran
        # out mid-run on the smallest window and produced no result at all;
        # raised to 60s, then to 75s once the P10 quick-grid hover-only check
        # (two more expert-menu round trips) pushed past that too, then to 95s
        # once 91/51/70's regressions (trash dialog, multi-view popover, three
        # restore-scope round trips) added their own expert-menu detours, then
        # to 110s once 90-94's six new expert rows pushed past that too, then
        # to 120s once the injected-markup grid/library check added its own
        # create-and-verify round trip, then to 126s once 171's late-voice-text
        # probe added its own editor round trip plus the 3s late-delivery grace
        # window it has to sit through, then to 132s once the recording
        # result dialog's probe added its own four expert-menu round trips,
        # then to 138s once the tag-merge check added its own batch-select
        # round trip (two disposable tags added before the merge itself),
        # then to 146s once the copy-sound check added its own four
        # copy-and-listen round trips plus a volume-slider detour,
        # then to 149s once the number-field check added its own
        # click/hold/wheel/preset/disabled-state round trip, then to
        # 151s once the media-viewer zoom check added its own
        # prompt-editor-to-lightbox round trip, then to 154s once the
        # editor-scale check added its own draw/resize/undo round trip,
        # then to 172s once the sync-folder check added three full
        # question-and-answer rounds, each with its own tab walk, then
        # to 220s once the undo-toast checks (434) added six of their
        # own round trips (view delete, tag delete, copy history,
        # clipboard inbox, batch delete, batch tag add/remove).
        # then to 300s once the settings probes of the combined lanes (copy history,
        # expert menu, editor, settings head) pushed the walk just past 250s,
        # then to 360s once the walk measured 296 to 300s of virtual time against the 300s
        # budget (the smallest window ran out before it wrote its result, one run in two),
        # then to 400s once the statistics dialog got its own probe block.
        # The window has to hold the iframe plus Chrome's own frame.
        # The dumped DOM carries all 20 languages; without an explicit
        # codec Windows decodes it as cp1252 and the reader thread dies,
        # leaving stdout empty.
        proc = run_chrome(browser, tmp / ('profile-%s' % size_tag), f"{max(w + 30, 520)},{h + 140}", 400000, wrapper.as_uri(), 270)
        m = re.search(r'<pre id="SMOKE">(\{.*?\})</pre>', proc.stdout, re.S)
        if not m:
            print(f"FAIL {w}x{h}: the UI produced no probe result")
            failed += 1
            continue
        r = json.loads(unescape(m.group(1)))
        print(f"-- {w}x{h} - expert tabs: {', '.join(r['tabs']) or 'none'}")
        for kind, items in (("JS error", r["errors"]),
                            ("overflow", sorted(set(r["overflow"]))),
                            ("layout", r.get("layout", [])),
                            ("untranslated", sorted(set(r["rawKeys"])))):
            for it in items:
                print(f"   FAIL {kind}: {it}")
            failed += len(items)
        if not (r["errors"] or r["overflow"] or r["rawKeys"] or r.get("layout")):
            print("   ok")

    # 221: a pass in the longest-running UI language (Italian: averages ~42
    # chars/string across all 1231 keys, the longest of the 11 by total
    # character count) at the two tightest window sizes, where a long
    # translation is most likely to overflow, overlap or push a button
    # out of reach. Same page/probe as the English sweep above, only with
    # SETTINGS.language overridden before main.js ever runs.
    it_page = tmp / "ui" / "index-it.html"
    it_page.write_text(
        html.replace(
            '<script src="tauri_stub.js"></script>',
            '<script src="tauri_stub.js"></script>\n<script>SETTINGS.language = "it";</script>',
            1,
        ),
        encoding="utf-8",
    )
    for w, h in (SIZES[-1], SIZES[-2]):
        size_tag = f"it-{w}x{h}"
        wrapper = tmp / f"wrap-{size_tag}.html"
        wrapper.write_text(WRAPPER.replace("ui/index.html", "ui/index-it.html") % (w, h), encoding="utf-8")
        # Raised from 126s once the clipboard-secret expert-menu check
        # (password-skip switch plus its length slider, each gated on the
        # other) added its own round of tab walks and switch toggles, then
        # from 134s once the Capture tab split (372) added a 7th expert
        # tab -- lengthening every existing tab walk by one click -- plus
        # its own group-survives-the-split probe, itself two full walks.
        # Kept equal to the English sweep's own budget just above, since
        # this runs the exact same probe script and had fallen behind it.
        proc = run_chrome(browser, tmp / ('profile-%s' % size_tag), f"{max(w + 30, 520)},{h + 140}", 400000, wrapper.as_uri(), 270)
        m = re.search(r'<pre id="SMOKE">(\{.*?\})</pre>', proc.stdout, re.S)
        if not m:
            print(f"FAIL it {w}x{h}: the UI produced no probe result")
            failed += 1
            continue
        r = json.loads(unescape(m.group(1)))
        print(f"-- it {w}x{h} - expert tabs: {', '.join(r['tabs']) or 'none'}")
        # Only the language-agnostic categories: overflow/layout/untranslated
        # are exactly what 221 asks for (truncated text, overlaps,
        # unreachable buttons). The probe's own "errors" array also carries
        # its behavioral assertions (exact English button labels, toast
        # text, dialog titles), which are EXPECTED to read differently in
        # Italian and are not a layout bug -- checked once already by the
        # English sweep above, so re-checking them here would only be noise.
        for kind, items in (("overflow", sorted(set(r["overflow"]))),
                            ("layout", r.get("layout", [])),
                            ("untranslated", sorted(set(r["rawKeys"])))):
            for it_ in items:
                print(f"   FAIL {kind}: {it_}")
            failed += len(items)
        uncaught = [e for e in r["errors"] if e.startswith("error: ") or e.startswith("rejection: ")]
        for e in uncaught:
            print(f"   FAIL JS error: {e}")
        failed += len(uncaught)
        if not (uncaught or r["overflow"] or r["rawKeys"] or r.get("layout")):
            print("   ok")
    return failed
