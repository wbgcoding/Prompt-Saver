// Minimize and maximize/restore buttons for a frameless window's own title bar.
// The markup, icons and wiring live here once, so every bar that needs them (the
// screenshot editor now, the video editor next) builds the same buttons.

// i18n keys of the labels; the maximize button swaps between the last two.
export const WIN_BUTTON_KEYS = { minimize: "winMinimize", maximize: "winMaximize", restore: "winRestore" };

const SVG_OPEN = '<svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">';

export const WIN_ICON_MINIMIZE = SVG_OPEN + '<path d="M6 12h12"/></svg>';
export const WIN_ICON_MAXIMIZE = SVG_OPEN + '<rect x="6" y="6" width="12" height="12" rx="1.5"/></svg>';
export const WIN_ICON_RESTORE = SVG_OPEN + '<path d="M9 9V7.5A1.5 1.5 0 0 1 10.5 6h6A1.5 1.5 0 0 1 18 7.5v6A1.5 1.5 0 0 1 16.5 15H15"/><rect x="6" y="9" width="9" height="9" rx="1.5"/></svg>';

// Builds the two buttons and puts them in front of `before` (the close button). The
// ids are `<idPrefix>-minimize` and `<idPrefix>-maximize`. They opt out of the bar's
// drag region, so a press on them is a click and not the start of a window drag.
export function insertWinButtons(before, idPrefix) {
  const make = (suffix, icon) => {
    const b = document.createElement("button");
    b.id = idPrefix + "-" + suffix;
    b.type = "button";
    b.className = "icon-btn win-bar-btn";
    b.dataset.tauriDragRegion = "false";
    b.innerHTML = icon;
    before.before(b);
    return b;
  };
  return { minBtn: make("minimize", WIN_ICON_MINIMIZE), maxBtn: make("maximize", WIN_ICON_MAXIMIZE) };
}

// Wires the buttons to `win` (the current Tauri window). The returned function
// re-reads the maximized state and the labels; call it when the bar becomes visible
// and after a language change. It also runs on every resize of the window.
export function wireWinButtons({ win, minBtn, maxBtn, translate }) {
  const sync = async () => {
    let max = false;
    try { max = !!(await win.isMaximized()); } catch (_) {}
    const minLabel = translate(WIN_BUTTON_KEYS.minimize);
    const maxLabel = translate(max ? WIN_BUTTON_KEYS.restore : WIN_BUTTON_KEYS.maximize);
    minBtn.title = minLabel;
    minBtn.setAttribute("aria-label", minLabel);
    maxBtn.title = maxLabel;
    maxBtn.setAttribute("aria-label", maxLabel);
    maxBtn.innerHTML = max ? WIN_ICON_RESTORE : WIN_ICON_MAXIMIZE;
    maxBtn.dataset.maximized = max ? "1" : "0";
  };
  minBtn.addEventListener("click", () => { win.minimize().catch(() => {}); });
  maxBtn.addEventListener("click", async () => {
    try { await win.toggleMaximize(); } catch (_) {}
    sync();
  });
  try { win.onResized(sync).catch(() => {}); } catch (_) {}
  sync();
  return sync;
}
