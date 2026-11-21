"""Composer probes: the tool buttons beside the text box and its clear X."""
from .arprobe import make_ar_probe


def run(browser, tmp):
    ar_probe = make_ar_probe(browser, tmp)
    failed = 0
    # Composer button order: Screenshot, Recording, Text recognition, Pipette, File, Voice input.
    failed += ar_probe('bco01', 'composer: the buttons sit in the agreed order', 'index', '880x600', '', r"""
    await bootReady();
    const ids = [...document.querySelectorAll('footer.composer > .composer-tools > button.composer-img-btn')].map(b => b.id);
    const want = ['snip-btn', 'record-btn', 'ocr-btn', 'picker-btn', 'file-btn', 'voice-btn'];
    if (JSON.stringify(ids.slice(0, want.length)) !== JSON.stringify(want)) errs.push('bco01: composer order is ' + JSON.stringify(ids));
    """)
    # A tall composer text box stacks the tool buttons into rows (same order, bottom aligned); a single line puts them in one row again.
    failed += ar_probe('bco02', 'composer: the tool buttons stack beside a tall text box and return to one row', 'index', '880x600', '', r"""
    await bootReady();
    const core = await import('./core.js');
    const tools = document.querySelector('.composer-tools');
    const rowsOf = () => new Set([...tools.children].map((b) => Math.round(b.getBoundingClientRect().top))).size;
    core.inputEl.value = '';
    core.autoGrow(core.inputEl);
    await sleep(100);
    if (rowsOf() !== 1) errs.push('bco02: one line should give one row, got ' + rowsOf());
    core.inputEl.value = Array.from({ length: 12 }, (_, i) => 'line ' + i).join(String.fromCharCode(10));
    core.autoGrow(core.inputEl);
    await sleep(100);
    if (tools.style.getPropertyValue('--tool-cols') !== '2') errs.push('bco02: tall box should give 2 columns, got ' + tools.style.getPropertyValue('--tool-cols'));
    if (rowsOf() < 2) errs.push('bco02: tall box should stack the buttons, got ' + rowsOf() + ' row(s)');
    const ids = [...tools.children].map((b) => b.id).join();
    if (ids !== 'snip-btn,record-btn,ocr-btn,picker-btn,file-btn,voice-btn') errs.push('bco02: order changed: ' + ids);
    const lastBottom = Math.round(tools.lastElementChild.getBoundingClientRect().bottom);
    const inBottom = Math.round(core.inputEl.getBoundingClientRect().bottom);
    if (Math.abs(inBottom - lastBottom) > 6) errs.push('bco02: tools not bottom aligned: ' + lastBottom + ' vs ' + inBottom);
    core.inputEl.value = '';
    core.autoGrow(core.inputEl);
    await sleep(100);
    if (rowsOf() !== 1) errs.push('bco02: clearing should give one row again, got ' + rowsOf());
    """)
    # Typing a long sentence one character at a time: the tool grid only ever stacks further, it never jumps back and forth.
    failed += ar_probe('bco03', 'composer: typing never makes the tool buttons jump back and forth', 'index', '880x600', '', r"""
    await bootReady();
    const core = await import('./core.js');
    const tools = document.querySelector('.composer-tools');
    const text = 'Ein langer Beispieltext, der Zeichen fuer Zeichen getippt wird, bis das Feld mehrfach umbricht und weiter waechst. '.repeat(4);
    let prev = Number(tools.style.getPropertyValue('--tool-cols')) || 6, ups = 0, first = '';
    for (let i = 1; i <= text.length; i++) {
      core.inputEl.value = text.slice(0, i);
      core.inputEl.dispatchEvent(new Event('input', { bubbles: true }));
      const cols = Number(tools.style.getPropertyValue('--tool-cols'));
      if (cols > prev) { ups++; if (!first) first = 'char ' + i + ': ' + prev + ' -> ' + cols; }
      prev = cols;
    }
    if (prev >= 6) errs.push('bco03: precondition: the text never stacked the buttons');
    if (ups) errs.push('bco03: the grid went back to more columns ' + ups + ' time(s) while typing, first at ' + first);
    """)
    # The clear X of the composer text box sits in the middle of a one-line box at every icon scale.
    failed += ar_probe('bco04', 'composer: the clear X is centred on a one-line text box at every icon scale', 'index', '880x600', '', r"""
    await bootReady();
    const core = await import('./core.js');
    const ta = core.inputEl;
    for (const z of ['1', '1.25', '1.5']) {
      document.documentElement.style.setProperty('--icon-zoom', z);
      ta.value = 'abc';
      ta.dispatchEvent(new Event('input', { bubbles: true }));
      await sleep(60);
      const svg = ta.parentElement.querySelector('.clear-btn.show svg');
      if (!svg) { errs.push('bco04: precondition: no visible clear X at zoom ' + z); continue; }
      const r = ta.getBoundingClientRect(), s = svg.getBoundingClientRect();
      const dy = (s.top + s.height / 2) - (r.top + r.height / 2);
      if (Math.abs(dy) > 1) errs.push('bco04: clear X is ' + dy.toFixed(1) + 'px off the middle at icon zoom ' + z);
    }
    document.documentElement.style.removeProperty('--icon-zoom');
    """)
    return failed
