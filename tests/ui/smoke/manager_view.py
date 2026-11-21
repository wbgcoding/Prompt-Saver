"""Media manager large view: a click on a tile shows the file big in the same window, with
stepping, the edit and copy actions, delete that moves on, and the way back to the grid."""
from .arprobe import make_ar_probe

# Waits for the manager grid of the kind asked for.
BOOT = r"""
    const tiles = () => document.querySelectorAll('#mm-grid .mm-tile');
    const kindReady = async (k) => {
      await until(() => tiles().length > 0, 200);
      if (k === 'screenshots') window.__FIRE__('manager-kind', 'screenshots');
      await until(() => tiles().length > 0 && tiles()[0].dataset.name.startsWith(k === 'screenshots' ? 'Screenshot' : 'Recording'), 200);
      await sleep(150);
    };
    const viewOpen = () => !document.getElementById('mm-view').classList.contains('hidden');
    const name = () => document.getElementById('mm-view-name').textContent;
    const key = (k, target) => (target || document.activeElement || document.body).dispatchEvent(new KeyboardEvent('keydown', { key: k, bubbles: true, cancelable: true }));
"""


def run(browser, tmp):
    ar_probe = make_ar_probe(browser, tmp)
    failed = 0
    # A click on a screenshot opens the large view in the window, with its picture, position and actions.
    failed += ar_probe('bmv01', 'manager: a click on a screenshot shows it large with its actions', 'manager', '880x600', '', BOOT + r"""
    await kindReady('screenshots');
    const second = tiles()[1].dataset.name;
    tiles()[1].click();
    await until(() => viewOpen(), 40);
    if (!viewOpen()) { errs.push('bmv01: the large view did not open'); return; }
    if (name() !== second) errs.push('bmv01: shows ' + name() + ' instead of ' + second);
    await until(() => document.querySelector('#mm-view-stage img'), 40);
    const img = document.querySelector('#mm-view-stage img');
    if (!img || !img.src.startsWith('data:')) errs.push('bmv01: no picture from media_preview_image in the stage');
    if (!callsFor('media_preview_image').some((a) => a.name === second)) errs.push('bmv01: media_preview_image was not asked for ' + second);
    if (!document.getElementById('mm-view-pos').textContent.includes('2')) errs.push('bmv01: position text is ' + document.getElementById('mm-view-pos').textContent);
    if (document.querySelector('.mm-toolbar').offsetParent || document.querySelector('.mm-scroll').offsetParent) errs.push('bmv01: the grid is still visible behind the large view');
    if (!document.getElementById('mm-view-player').classList.contains('hidden')) errs.push('bmv01: a screenshot offers the player button');
    if (callsFor('media_open_in_main').length) errs.push('bmv01: a plain click opened the editor');
    document.getElementById('mm-view-copy').click();
    await sleep(100);
    if (!callsFor('media_copy_image').some((a) => a.name === second)) errs.push('bmv01: Copy did not copy the picture');
    document.getElementById('mm-view-edit').click();
    await sleep(100);
    if (!callsFor('media_open_in_main').some((a) => a.name === second && a.kind === 'screenshots')) errs.push('bmv01: Edit did not open the screenshot in the editor');
    """)
    # Arrow keys step through the files, Escape goes back to the grid with the shown file focused.
    failed += ar_probe('bmv02', 'manager: arrows step through the large view, Escape returns to the grid', 'manager', '880x600', '', BOOT + r"""
    await kindReady('screenshots');
    const names = [...tiles()].map((t) => t.dataset.name);
    tiles()[0].click();
    await until(() => viewOpen(), 40);
    if (!document.getElementById('mm-view-prev').disabled) errs.push('bmv02: Previous is enabled on the first file');
    key('ArrowRight', document.body); await sleep(50);
    key('ArrowRight', document.body); await sleep(50);
    if (name() !== names[2]) errs.push('bmv02: two steps right show ' + name() + ', expected ' + names[2]);
    key('ArrowLeft', document.body); await sleep(50);
    if (name() !== names[1]) errs.push('bmv02: one step back shows ' + name());
    key('Escape', document.body); await sleep(100);
    if (viewOpen()) errs.push('bmv02: Escape did not close the large view');
    if (!document.querySelector('.mm-scroll').offsetParent) errs.push('bmv02: the grid did not come back');
    if (document.activeElement?.dataset?.name !== names[1]) errs.push('bmv02: focus is not on the last shown file');
    if (document.getElementById('mm-view-stage').childElementCount) errs.push('bmv02: the stage still holds content after closing');
    """)
    # A recording in a format the webview plays gets a video element; leaving the view drops its source.
    failed += ar_probe('bmv03', 'manager: a recording plays in the large view and lets go of its file on close', 'manager', '880x600', '', BOOT + r"""
    await kindReady('recordings');
    const mp4 = [...tiles()].find((t) => t.dataset.name.endsWith('.mp4'));
    const mkv = [...tiles()].find((t) => t.dataset.name.endsWith('.mkv'));
    mp4.click();
    await until(() => document.querySelector('#mm-view-stage video'), 40);
    const v = document.querySelector('#mm-view-stage video');
    if (!v) { errs.push('bmv03: no video element for an mp4'); return; }
    if (!v.controls) errs.push('bmv03: the video has no controls');
    if (document.getElementById('mm-view-player').classList.contains('hidden')) errs.push('bmv03: no player button for a recording');
    document.getElementById('mm-view-back').click();
    await sleep(100);
    if (v.getAttribute('src')) errs.push('bmv03: the video kept its source after closing');
    mkv.click();
    await until(() => document.querySelector('#mm-view-stage .mm-view-msg'), 40);
    if (!document.querySelector('#mm-view-stage .mm-view-msg button')) errs.push('bmv03: an mkv shows no way to open it in the player');
    """)
    # Delete in the large view moves the file away and shows the next one; the last one closes the view.
    failed += ar_probe('bmv04', 'manager: Delete in the large view moves on to the next file', 'manager', '880x600', '', BOOT + r"""
    await kindReady('screenshots');
    const names = [...tiles()].map((t) => t.dataset.name);
    const base = await window.__TAURI__.core.invoke('list_media', { kind: 'screenshots' });
    tiles()[0].click();
    await until(() => viewOpen(), 40);
    window.__MM_FILES__ = base.filter((f) => f.name !== names[0]);
    document.getElementById('mm-view-del').click();
    await until(() => name() === names[1], 60);
    if (!callsFor('media_discard').some((a) => (a.names || []).includes(names[0]))) errs.push('bmv04: Delete did not discard ' + names[0]);
    if (!viewOpen()) errs.push('bmv04: the view closed instead of moving on');
    if (name() !== names[1]) errs.push('bmv04: after Delete it shows ' + name() + ', expected ' + names[1]);
    window.__MM_FILES__ = [];
    document.getElementById('mm-view-del').click();
    await until(() => !viewOpen(), 60);
    if (viewOpen()) errs.push('bmv04: deleting the last file left the view open');
    """)
    # Opening a recording, and stepping to another one, starts playback.
    failed += ar_probe('bmv05', 'manager: a recording starts playing when opened and when stepped to', 'manager', '880x600', '', BOOT + r"""
    let plays = 0;
    HTMLMediaElement.prototype.play = function () { plays++; return Promise.resolve(); };
    await kindReady('recordings');
    const mp4s = [...tiles()].filter((t) => t.dataset.name.endsWith('.mp4'));
    mp4s[0].click();
    await until(() => document.querySelector('#mm-view-stage video'), 40);
    if (plays < 1) errs.push('bmv05: opening an mp4 did not call play()');
    const before = plays;
    document.getElementById('mm-view-next').click();
    await sleep(150);
    const nextName = document.getElementById('mm-view-name').textContent;
    if (!nextName.endsWith('.mp4')) errs.push('bmv05: the next file is ' + nextName + ', not a second mp4');
    if (plays < before + 1) errs.push('bmv05: stepping to a second mp4 did not call play()');
    """)
    return failed
