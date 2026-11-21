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

    # The model manager, opened the way the expert menu does it.
    open_manager = r"""
    await bootReady();
    const core = await import('./core.js');
    core.EXPERT_ACTIONS['whisper-models-open']();
    await until(() => document.querySelectorAll('#whisper-models-body .whisper-card').length === 3, 100);
    const card = (id) => document.querySelector('#whisper-models-body .whisper-card[data-model="' + id + '"]');
    const btn = (id, i) => card(id) && card(id).querySelectorAll('.whisper-card-actions button')[i || 0];
    """
    failed += ar_probe('bwh02', 'whisper models: the recommended badge sits on the catalog recommendation only', 'index', '880x600', '', open_manager + r"""
    const badges = [...document.querySelectorAll('#whisper-models-body .whisper-badge')];
    if (badges.length !== 1) errs.push('bwh02: expected one recommended badge, found ' + badges.length);
    if (!card('small') || !card('small').querySelector('.whisper-badge')) errs.push('bwh02: the badge is not on the recommended model (small)');
    if (card('turbo') && card('turbo').querySelector('.whisper-badge')) errs.push('bwh02: the badge sits on turbo');
    if (card('base') && card('base').querySelector('.whisper-badge')) errs.push('bwh02: the badge sits on base');
    for (const id of ['base', 'small', 'turbo']) {
      const b = btn(id);
      if (!b || b.disabled) errs.push('bwh02: ' + id + ' should offer an enabled download button');
    }
    if (!document.getElementById('whisper-models-pc').textContent.includes('8')) errs.push('bwh02: the PC line misses the core count: ' + document.getElementById('whisper-models-pc').textContent);
    if (!document.getElementById('whisper-models-cpu').classList.contains('hidden')) errs.push('bwh02: the AVX2 warning shows on a capable PC');
    """)
    failed += ar_probe('bwh03', 'whisper models: a download shows progress, then the model is in use and the engine is local', 'index', '880x600',
                       '<script>window.__WHISPER_GATE__ = [];</script>\n', open_manager + r"""
    btn('small').click();
    await until(() => !document.getElementById('confirm-modal').classList.contains('hidden'), 60);
    document.getElementById('confirm-ok').click();
    await until(() => card('small') && card('small').querySelector('.whisper-progress'), 60);
    if (!card('small') || !card('small').querySelector('.whisper-progress')) errs.push('bwh03: no progress row on the downloading card');
    if (!btn('base').disabled || !btn('turbo').disabled) errs.push('bwh03: the other downloads stay enabled while one runs');
    window.__FIRE__('whisper-download-progress', { model: 'small', done: 95042744, total: 190085487 });
    await sleep(60);
    const bar = card('small').querySelector('[role="progressbar"]');
    if (!bar || bar.getAttribute('aria-valuenow') !== '50') errs.push('bwh03: progress should read 50, reads ' + (bar && bar.getAttribute('aria-valuenow')));
    if (!/MB/.test(card('small').querySelector('.whisper-progress-text').textContent)) errs.push('bwh03: the progress text shows no sizes');
    if (callsFor('grant_whisper_download_consent').length !== 1) errs.push('bwh03: consent was not recorded before the download');
    window.__WHISPER_GATE__.shift().finish();
    await until(() => card('small') && !card('small').querySelector('.whisper-progress') && card('small').classList.contains('is-active'), 80);
    if (!card('small').classList.contains('is-active')) errs.push('bwh03: the finished model is not marked active');
    if (!btn('small').disabled) errs.push('bwh03: the active model should show a disabled In use button');
    if (core.settings.ui_texts.voiceEngine !== 'whisper') errs.push('bwh03: the first model did not switch the engine to whisper');
    """)
    failed += ar_probe('bwh04', 'whisper models: cancelling a download is silent and restores the card', 'index', '880x600',
                       '<script>window.__WHISPER_GATE__ = [];</script>\n', open_manager + r"""
    btn('base').click();
    await until(() => !document.getElementById('confirm-modal').classList.contains('hidden'), 60);
    document.getElementById('confirm-ok').click();
    await until(() => card('base') && card('base').querySelector('.whisper-progress-cancel'), 60);
    card('base').querySelector('.whisper-progress-cancel').click();
    await until(() => card('base') && !card('base').querySelector('.whisper-progress'), 80);
    await sleep(100);
    if (card('base').querySelector('.whisper-progress')) errs.push('bwh04: the progress row stays after a cancel');
    if (callsFor('cancel_whisper_download').length !== 1) errs.push('bwh04: cancel_whisper_download was not called once');
    if (btn('base').disabled || btn('base').textContent !== core.t('whisperDownload')) errs.push('bwh04: the card did not return to a download button');
    if (core.toastEl.classList.contains('show') && /fail|Fehler/i.test(core.toastEl.textContent)) errs.push('bwh04: a cancel raised a failure toast: ' + core.toastEl.textContent);
    """)
    failed += ar_probe('bwh05', 'whisper models: use switches the model, delete removes it after a confirmation', 'index', '880x600',
                       "<script>window.__WHISPER_HAVE__ = ['base', 'small'];</script>\n", open_manager + r"""
    if (!card('small').classList.contains('is-active')) errs.push('bwh05: the recommended downloaded model should be active by default');
    btn('base').click();
    await until(() => card('base') && card('base').classList.contains('is-active'), 60);
    if (!card('base').classList.contains('is-active')) errs.push('bwh05: Use did not make base active');
    if (core.settings.ui_texts.whisperModel !== 'base') errs.push('bwh05: whisperModel was not saved, holds ' + core.settings.ui_texts.whisperModel);
    btn('small', 1).click();
    await until(() => !document.getElementById('confirm-modal').classList.contains('hidden'), 60);
    document.getElementById('confirm-ok').click();
    await until(() => btn('small') && btn('small').textContent === core.t('whisperDownload'), 80);
    if (callsFor('delete_whisper_model').length !== 1) errs.push('bwh05: delete_whisper_model was not called once');
    if (btn('small').textContent !== core.t('whisperDownload')) errs.push('bwh05: the deleted model does not offer a download again');
    """)
    failed += ar_probe('bwh06', 'whisper models: a PC without AVX2 gets a warning, no downloads and a greyed engine option', 'index', '880x600',
                       '<script>window.__WHISPER_SUPPORTED__ = false;</script>\n', open_manager + r"""
    if (document.getElementById('whisper-models-cpu').classList.contains('hidden')) errs.push('bwh06: no AVX2 warning in the manager');
    for (const id of ['base', 'small', 'turbo']) if (!btn(id).disabled) errs.push('bwh06: ' + id + ' can be downloaded on an unsupported PC');
    document.getElementById('whisper-models-close-x').click();
    document.getElementById('gear').click(); await sleep(300);
    document.getElementById('expert-open').click(); await sleep(400);
    const row = () => document.querySelector('[data-setting-row="voiceEngine"]');
    for (const tab of document.querySelectorAll('#expert-tabs-bar .expert-tab')) { if (row()) break; tab.click(); await sleep(150); }
    const opt = row() && row().querySelector('option[value="whisper"]');
    if (!opt) errs.push('bwh06: the recognition row has no whisper option');
    else if (!opt.disabled) errs.push('bwh06: the whisper option is selectable on an unsupported PC');
    """)
    return failed
