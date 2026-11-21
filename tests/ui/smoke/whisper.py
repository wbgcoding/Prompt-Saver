"""Local dictation probes: the listening state of a session that says nothing until a phrase is done."""
from .arprobe import make_ar_probe


def run(browser, tmp):
    ar_probe = make_ar_probe(browser, tmp)
    failed = 0
    # The local engine sends one empty partial when its microphone is open: the button moves from arming to listening, the field stays as it was.
    failed += ar_probe('bwh01', 'dictation: an empty partial moves the button to listening and inserts nothing', 'index', '880x600', '', r"""
    await bootReady();
    const btn = document.getElementById('voice-btn');
    const input = document.getElementById('input');
    input.value = 'Hello';
    btn.click(); await sleep(100);
    if (!btn.classList.contains('is-arming')) errs.push('bwh01: fixture: the button should be arming right after the click, classes ' + btn.className);
    window.__FIRE__('voice-partial', { id: 1, token: 1, text: '' });
    await sleep(80);
    if (!btn.classList.contains('is-listening')) errs.push('bwh01: an empty partial did not move the button to listening, classes ' + btn.className);
    if (btn.classList.contains('is-arming')) errs.push('bwh01: the button is still arming');
    if (input.value !== 'Hello') errs.push('bwh01: an empty partial changed the field to ' + JSON.stringify(input.value));
    window.__FIRE__('voice-text', { id: 1, token: 1, text: 'world' });
    await sleep(80);
    if (input.value !== 'Hello world') errs.push('bwh01: the first phrase did not land after the empty partial, field holds ' + JSON.stringify(input.value));
    """)
    return failed
