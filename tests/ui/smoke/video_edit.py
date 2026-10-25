"""Video editor probes added with the editor rework: title bar, selection, overlays. Each probe has its own boot."""
import json
import re

from .harness import REPO, unescape, run_chrome


SCAFFOLD = r"""
<script>
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const until = async (fn, tries) => { for (let i = 0; i < (tries || 100) && !fn(); i++) await sleep(50); return fn(); };
  try {
    await until(() => {
      const hint = document.querySelector('#ve-ops-list p');
      return document.getElementById('ve-stage-msg').classList.contains('hidden') &&
        document.querySelectorAll('#ve-filmstrip .ve-thumb').length > 0 &&
        !!hint && hint.textContent && hint.textContent !== 'veOpsEmpty';
    }, 300);
    const mod = await import('./video.js');
    const { VE } = mod;
    const ptr = (type, x, y, el, extra) => (el || window).dispatchEvent(new PointerEvent(type, Object.assign({
      bubbles: true, cancelable: true, button: 0, isPrimary: true, pointerId: 7, clientX: x, clientY: y,
    }, extra || {})));
%s
  } catch (e) { errs.push('probe threw: ' + e.message + ' ' + (e.stack || '').split('\n')[1]); }
  errs.push(...(window.__SMOKE_ERRORS__ || []));
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""


def run(browser, tmp):
    failed = 0

    def vprobe(tag, label, body, pre="", size="1130,860"):
        side = tmp / "ui" / f"ve-{tag}.html"
        side.write_text(
            (REPO / "ui" / "video.html").read_text(encoding="utf-8")
            .replace("<script", pre + '<script src="tauri_stub.js"></script>\n<script', 1)
            .replace("</body>", SCAFFOLD % body + "</body>"),
            encoding="utf-8")
        proc = run_chrome(browser, tmp / ("profile-ve-" + tag), size, 30000, side.as_uri(), 180)
        m = re.search(r'<pre id="SMOKE">(\[.*?\])</pre>', proc.stdout, re.S)
        errs = json.loads(unescape(m.group(1))) if m else ["no probe result"]
        print(f"-- video.html {label}")
        for e in errs:
            print(f"   FAIL: {e}")
        if not errs:
            print("   ok")
        return len(errs)

    # The camera window's bar is a deep drag region too (its markup is static, nothing to boot).
    cam = (REPO / "ui" / "camera.html").read_text(encoding="utf-8")
    print("-- camera.html title bar drags everywhere")
    if 'class="cam-titlebar" data-tauri-drag-region="deep"' in cam:
        print("   ok")
    else:
        print('   FAIL: the camera title bar is not data-tauri-drag-region="deep"')
        failed += 1

    # 4021: a bare drag attribute moves only from the bar itself; the file name and label need "deep".
    failed += vprobe('bar', 'title bar is a deep drag region, buttons opt out', r"""
    // 4021: the video editor title bar.
    const bar = document.querySelector('.ve-titlebar');
    if (bar.getAttribute('data-tauri-drag-region') !== 'deep') errs.push('4021: the bar drag region is "' + bar.getAttribute('data-tauri-drag-region') + '", want deep');
    for (const b of bar.querySelectorAll('button')) {
      if (b.getAttribute('data-tauri-drag-region') !== 'false') errs.push('4021: ' + b.id + ' does not opt out of the drag region');
    }
    """)

    # 4022: minimize and maximize/restore sit between the shortcuts and close buttons.
    failed += vprobe('winbtn', 'minimize and maximize buttons follow the window state', r"""
    // 4022: window buttons in the video editor bar.
    const mn = document.getElementById('ve-minimize');
    const mx = document.getElementById('ve-maximize');
    const sh = document.getElementById('ve-shortcuts');
    const cl = document.getElementById('ve-close');
    if (!mn || !mx) { errs.push('4022: a window button is missing'); }
    else {
      for (const b of [mn, mx]) {
        if (b.getAttribute('data-tauri-drag-region') !== 'false') errs.push('4022: ' + b.id + ' does not opt out of the drag region');
        const sv = b.querySelector('svg');
        if (!sv || sv.getAttribute('width') !== '18' || sv.getAttribute('stroke-width') !== '1.5') errs.push('4022: ' + b.id + ' icon is not 18px with stroke 1.5');
        if (!b.title || !b.getAttribute('aria-label') || /^win[A-Z]/.test(b.title)) errs.push('4022: ' + b.id + ' has no label');
      }
      const r = (e) => e.getBoundingClientRect();
      if (!(r(sh).right <= r(mn).left + 1 && r(mn).right <= r(mx).left + 1 && r(mx).right <= r(cl).left + 1)) errs.push('4022: the order is not shortcuts, minimize, maximize, close');
      if (Math.abs(r(mn).height - r(cl).height) > 1) errs.push('4022: the minimize button is not as tall as the close button');
      const iconBefore = mx.innerHTML, titleBefore = mx.title;
      mx.click(); await sleep(150);
      if (mx.dataset.maximized !== '1' || mx.innerHTML === iconBefore) errs.push('4022: a click did not switch to the restore icon');
      if (mx.title === titleBefore) errs.push('4022: the title did not change to Restore');
      mx.click(); await sleep(150);
      if (mx.dataset.maximized !== '0' || mx.innerHTML !== iconBefore) errs.push('4022: a second click did not switch back');
    }
    """)
    return failed
