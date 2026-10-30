// The one shared custom-colour popup (SV field + hue bar + hex input).
// Every window that needs a custom colour picker calls createColorPop() once
// and gets back { el, open, close } to wire into its own overlay dismiss/
// Escape/Tab-trap handling. Never a second implementation, never a native
// <input type=color>.
import { placePopover } from "./popover.js";

export function createColorPop() {
  const el = document.createElement("div");
  el.className = "color-pop hidden";
  el.innerHTML =
    '<div class="cp-sv"><div class="cp-knob"></div></div>' +
    '<input class="cp-hue" type="range" min="0" max="360" step="1" data-i18n-aria="colorHue" />' +
    '<div class="cp-row"><span class="cp-preview"></span><input class="cp-hex" type="text" maxlength="7" spellcheck="false" data-i18n-aria="colorHex" /></div>';
  document.body.appendChild(el);
  const sv = el.querySelector(".cp-sv");
  const knob = el.querySelector(".cp-knob");
  const hue = el.querySelector(".cp-hue");
  const preview = el.querySelector(".cp-preview");
  const hex = el.querySelector(".cp-hex");
  let cp = { h: 215, s: 0.85, v: 0.92 };
  let onPick = null;

  function hsvToHex({ h, s, v }) {
    const f = (n) => {
      const k = (n + h / 60) % 6;
      const c = v - v * s * Math.max(0, Math.min(k, 4 - k, 1));
      return Math.round(c * 255).toString(16).padStart(2, "0");
    };
    return `#${f(5)}${f(3)}${f(1)}`;
  }

  function hexToHsv(hexStr) {
    const m = /^#?([0-9a-f]{6})$/i.exec(hexStr || "");
    if (!m) return { h: 215, s: 0.85, v: 0.92 };
    const n = parseInt(m[1], 16);
    const r = (n >> 16) / 255;
    const g = ((n >> 8) & 255) / 255;
    const b = (n & 255) / 255;
    const max = Math.max(r, g, b);
    const d = max - Math.min(r, g, b);
    let h = 0;
    if (d) {
      h = max === r ? ((g - b) / d) % 6 : max === g ? (b - r) / d + 2 : (r - g) / d + 4;
      // Kept fractional: a whole-degree hue would shift the colour on the round trip back to hex.
      h *= 60;
      if (h < 0) h += 360;
    }
    return { h, s: max ? d / max : 0, v: max };
  }

  function render(notify = true) {
    sv.style.background =
      `linear-gradient(to top, #000, transparent), linear-gradient(to right, #fff, hsl(${cp.h}, 100%, 50%))`;
    knob.style.left = `${cp.s * 100}%`;
    knob.style.top = `${(1 - cp.v) * 100}%`;
    hue.value = cp.h;
    const hexVal = hsvToHex(cp);
    preview.style.background = hexVal;
    if (document.activeElement !== hex) hex.value = hexVal;
    if (notify) onPick?.(hexVal);
  }

  function open(anchor, current, pick) {
    cp = hexToHsv(current);
    onPick = null;
    render(false); // show the current color without re-triggering the pick
    onPick = pick;
    placePopover(el, anchor.getBoundingClientRect(), 6);
    window.__TAURI__.core.invoke("record_tool_usage", { tool: "color_pick" }).catch(() => {});
  }

  function close() {
    onPick = null;
    el.classList.add("hidden");
  }

  function drag(e) {
    const r = sv.getBoundingClientRect();
    cp.s = Math.min(1, Math.max(0, (e.clientX - r.left) / r.width));
    cp.v = Math.min(1, Math.max(0, 1 - (e.clientY - r.top) / r.height));
    render();
  }
  sv.addEventListener("pointerdown", (e) => {
    sv.setPointerCapture(e.pointerId);
    drag(e);
  });
  sv.addEventListener("pointermove", (e) => {
    if (sv.hasPointerCapture(e.pointerId)) drag(e);
  });
  hue.addEventListener("input", () => {
    cp.h = Number(hue.value);
    render();
  });
  hex.addEventListener("change", () => {
    if (/^#?[0-9a-f]{6}$/i.test(hex.value)) {
      cp = hexToHsv(hex.value);
      render();
    }
  });
  document.addEventListener("pointerdown", (e) => {
    if (el.classList.contains("hidden")) return;
    if (!el.contains(e.target) && !e.target.closest(".swatch.custom")) close();
  });

  return { el, open, close };
}
