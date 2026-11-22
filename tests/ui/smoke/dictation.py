"""Dictation button probes: the quick voice settings menu a right-click opens, and the error messages."""
from .arprobe import make_ar_probe


def run(browser, tmp):
    ar_probe = make_ar_probe(browser, tmp)
    failed = 0
    # Right-click on the microphone opens the quick voice settings menu.
    failed += ar_probe('bvq01', 'voice input: right-click on the microphone opens the quick settings menu', 'index', '880x600', '<script>window.__SEED_SETTINGS__ = { ui_flags: { voice: true, voiceAutoStop: false } };</script>\n', r"""
    await bootReady();
    const core = await import('./core.js');
    const pop = () => document.querySelector('.voice-pop:not(.hidden)');
    const rowOf = (k) => document.querySelector('.voice-pop [data-setting-row="' + k + '"]');
    const rightClick = async (id) => {
      document.getElementById(id).dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
      await sleep(400);
    };
    await rightClick('voice-btn');
    const p = pop();
    if (!p) errs.push('bvq01: no menu after the right-click on the microphone');
    else {
      const r = p.getBoundingClientRect();
      if (r.left < 0 || r.top < 0 || r.right > innerWidth || r.bottom > innerHeight) errs.push('bvq01: the menu leaves the window: ' + JSON.stringify([r.left, r.top, r.right, r.bottom]));
      for (const k of ['voiceLang', 'voiceInsert', 'voiceAutoStop', 'voicePunct', 'voiceInterim']) if (!rowOf(k)) errs.push('bvq01: the row ' + k + ' is missing');
      const sec = rowOf('voiceAutoStopSec');
      if (!sec || !sec.classList.contains('hidden')) errs.push('bvq01: the silence seconds row should be hidden while auto-stop is off');
      if (p.querySelector('.voice-pop-mic, .voice-pop-sound, .voice-pop-sep')) errs.push('bvq01: the menu still has the microphone row or the sound settings button');
      if (!p.firstElementChild || !p.firstElementChild.classList.contains('voice-pop-title')) errs.push('bvq01: the menu does not start with its title');
      // The local engine rows: the engine choice always, microphone and model manager only for Whisper.
      if (!rowOf('voiceEngine')) errs.push('bvq01: the engine row is missing');
      const mic = rowOf('voiceMic');
      const models = p.querySelector('.voice-pop-models');
      if (!mic || !mic.classList.contains('hidden')) errs.push('bvq01: the microphone row should be hidden for the Windows engine');
      if (!models || !models.classList.contains('hidden')) errs.push('bvq01: the models button should be hidden for the Windows engine');
      const eng = rowOf('voiceEngine') && rowOf('voiceEngine').querySelector('select');
      if (eng) {
        eng.value = 'whisper';
        eng.dispatchEvent(new Event('change', { bubbles: true }));
        await sleep(300);
        if (mic && mic.classList.contains('hidden')) errs.push('bvq01: the microphone row stays hidden after picking Whisper');
        if (models && models.classList.contains('hidden')) errs.push('bvq01: the models button stays hidden after picking Whisper');
        eng.value = 'windows';
        eng.dispatchEvent(new Event('change', { bubbles: true }));
        await sleep(300);
        if (mic && !mic.classList.contains('hidden')) errs.push('bvq01: the microphone row stays visible after picking Windows again');
      }
      const sw = rowOf('voiceAutoStop') && rowOf('voiceAutoStop').querySelector('input');
      if (sw) {
        sw.click(); await sleep(300);
        if (sec && sec.classList.contains('hidden')) errs.push('bvq01: the silence seconds row stays hidden after auto-stop is switched on');
        if (core.settings.ui_flags.voiceAutoStop !== true) errs.push('bvq01: the auto-stop switch did not reach the settings');
      }
    }
    await rightClick('voice-btn');
    if (!pop()) errs.push('bvq01: the menu did not reopen');
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    await sleep(200);
    if (pop()) errs.push('bvq01: Escape did not close the menu');
    // Every dictation failure is shown: the nothing-heard toast names the device and offers the
    // sound settings, the other codes map to their own message.
    const dict = await import('./dictation.js');
    const toastMsg = () => (document.querySelector('#toast .toast-msg') || {}).textContent || '';
    dict.routeVoiceError({ code: 'voice-nothing-heard', mic: 'Test Microphone' });
    await sleep(100);
    if (toastMsg().indexOf('Test Microphone') < 0) errs.push('bvq01: the nothing-heard toast does not name the microphone: ' + toastMsg());
    const act = document.querySelector('#toast .toast-btn');
    if (!act) errs.push('bvq01: the nothing-heard toast has no sound settings button');
    else {
      act.click(); await sleep(150);
      if (window.__SOUND_SETTINGS_OPENED__ !== true) errs.push('bvq01: the sound settings button did not call the backend');
    }
    for (const code of ['voice-network', 'voice-mic', 'voice-audio', 'voice-no-topic', 'voice-failed']) {
      dict.routeVoiceError({ code });
      await sleep(60);
      if (!toastMsg() || toastMsg().indexOf('voice-') >= 0) errs.push('bvq01: ' + code + ' shows no message of its own: ' + toastMsg());
    }
    // From the editor: the menu sits above the dialog.
    document.querySelector('#grid .tile') && document.querySelector('#grid .tile').dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: 40, clientY: 40 }));
    await sleep(300);
    const ed = document.querySelector('#ctx [data-act="edit"]');
    if (!ed) errs.push('bvq01: no edit entry in the tile menu (fixture)');
    else {
      ed.click(); await sleep(500);
      await rightClick('modal-voice-btn');
      const p2 = pop();
      if (!p2) errs.push('bvq01: no menu from the editor microphone');
      else {
        const zp = parseInt(getComputedStyle(p2).zIndex, 10);
        const zm = parseInt(getComputedStyle(document.getElementById('modal')).zIndex, 10);
        if (!(zp > zm)) errs.push('bvq01: the menu (z ' + zp + ') is not above the editor (z ' + zm + ')');
      }
    }
""")
    failed += ar_probe('v2', 'dictation: a final text that lands late replaces the live partial instead of doubling it', 'index', '1000x780', '<script>window.__SEED_SETTINGS__ = { ui_flags: { voiceInterim: true } };</script>\n', r"""
    await bootReady();
    const btn = document.getElementById('voice-btn');
    const input = document.getElementById('input');
    input.value = '';
    btn.click(); await sleep(100);
    window.__FIRE__('voice-partial', { id: 1, token: 1, text: 'hello' });
    await sleep(50);
    if (input.value.trim() !== 'hello') errs.push('564-v2: fixture: the partial did not land, got ' + JSON.stringify(input.value));
    window.__FIRE__('voice-stopped', { id: 1, token: 1, reason: 'manual' });
    await sleep(50);
    window.__FIRE__('voice-text', { id: 1, token: 1, text: 'hello world' });
    await sleep(100);
    if (input.value.trim() !== 'hello world') errs.push('564-v2: the late final text should replace the partial, field holds ' + JSON.stringify(input.value));
""")
    # An automatic end (pause or time limit) plays the short stop tone, a manual stop does not;
    # the pause slider starts at 30 s and reaches ten minutes.
    failed += ar_probe('bvs01', 'dictation: the pause slider goes from 30 s to 10 min (the stop chime is played by Windows, see the stop_tone unit tests)', 'index', '880x600', '<script>window.__SEED_SETTINGS__ = { ui_flags: { voice: true, voiceAutoStop: true } };</script>\n', r"""
    await bootReady();
    const btn = document.getElementById('voice-btn');
    btn.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
    await sleep(400);
    const row = document.querySelector('.voice-pop [data-setting-row="voiceAutoStopSec"]');
    const slider = row && row.querySelector('input[type=range]');
    const out = row && row.querySelector('.value-out');
    if (!slider || !out) errs.push('bvs01: the pause slider is missing');
    else {
      if (!/^30\s?s$/.test(out.textContent.trim())) errs.push('bvs01: the pause readout starts at ' + JSON.stringify(out.textContent));
      slider.value = slider.max;
      slider.dispatchEvent(new Event('input', { bubbles: true }));
      await sleep(50);
      if (out.textContent.indexOf('10') < 0 || /s$/.test(out.textContent.trim())) errs.push('bvs01: the top of the pause slider reads ' + JSON.stringify(out.textContent));
    }
""")
    return failed
