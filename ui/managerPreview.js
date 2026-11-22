// Media manager large view: one recording or screenshot shown big inside the manager window,
// with the actions for that file (edit, copy, player, export, folder, delete) and stepping
// through the files the grid currently shows.
//
// The screenshot folder is not in the asset scope, so a screenshot arrives as a data URL from
// the backend; a recording plays straight from its file through the asset protocol.

// Formats the webview plays or draws itself.
const VIDEO_TAG_EXTS = new Set(["mp4", "webm"]);
const IMG_TAG_EXTS = new Set(["gif", "webp", "apng", "png", "jpg", "jpeg", "bmp"]);

export function createPreview(ctx) {
  const { $, t, fill, invoke, convertFileSrc } = ctx;
  let current = "";     // name of the file on show, "" while closed
  let loadedKey = "";   // what the stage holds, so a relabel does not reload the picture
  let seq = 0;          // bumped on every stage load: a late answer for an older file is dropped
  let lastAt = 0;       // grid position of the file on show, for moving on when it disappears

  const isOpen = () => !!current;

  // ---- zoom and pan of the picture or video on show
  // The media element carries `translate(x, y) scale(s)` around its own centre, which sits on the
  // stage centre; the stage clips it. Scale 1 is "fit", so the range is fit up to ZOOM_MAX times.
  const ZOOM_MAX = 8;
  const CONTROL_BAR_PX = 56; // the native control bar at the bottom of a video, in unzoomed pixels
  const PAN_START_PX = 3;    // a press that moves less than this stays a click (play/pause on a video)
  const zoom = { s: 1, x: 0, y: 0, drag: null };
  const stageEl = $("mm-view-stage");

  const mediaEl = () => stageEl.querySelector(":scope > img, :scope > video");

  // Size the media is drawn at when fitted (the video element fills the stage, its picture is letterboxed).
  function fittedSize(el) {
    if (el instanceof HTMLVideoElement && el.videoWidth > 0 && el.videoHeight > 0) {
      const k = Math.min(el.offsetWidth / el.videoWidth, el.offsetHeight / el.videoHeight);
      return { w: el.videoWidth * k, h: el.videoHeight * k };
    }
    return { w: el.offsetWidth, h: el.offsetHeight };
  }

  // Scale at which one media pixel is one screen pixel, never below 2 so the toggle always shows more.
  function actualScale(el) {
    const fit = fittedSize(el);
    const nw = el instanceof HTMLVideoElement ? el.videoWidth : el.naturalWidth;
    const k = nw > 0 && fit.w > 0 ? nw / fit.w : 0;
    return Math.min(ZOOM_MAX, Math.max(2, k));
  }

  function paintZoom() {
    const el = mediaEl();
    const zoomed = zoom.s > 1.001;
    stageEl.classList.toggle("zoomed", zoomed);
    stageEl.classList.toggle("panning", !!(zoom.drag && zoom.drag.active));
    if (!el) return;
    el.style.transform = zoomed ? `translate(${zoom.x}px, ${zoom.y}px) scale(${zoom.s})` : "";
    // The native control bar would scale off the stage with the picture: it is hidden while zoomed.
    if (el instanceof HTMLVideoElement) el.controls = !zoomed;
  }

  function resetZoom() {
    zoom.s = 1;
    zoom.x = 0;
    zoom.y = 0;
    zoom.drag = null;
    paintZoom();
  }

  // Keeps the zoomed media covering the stage: it can be moved until its edge meets the stage edge.
  function clampPan() {
    const el = mediaEl();
    if (!el) return;
    const fit = fittedSize(el);
    const limX = Math.max(0, (fit.w * zoom.s - stageEl.clientWidth) / 2);
    const limY = Math.max(0, (fit.h * zoom.s - stageEl.clientHeight) / 2);
    zoom.x = Math.min(limX, Math.max(-limX, zoom.x));
    zoom.y = Math.min(limY, Math.max(-limY, zoom.y));
  }

  // Sets the scale so the point (cx, cy), in client coordinates, stays under the cursor.
  function zoomTo(next, cx, cy) {
    if (!mediaEl()) return;
    const s = Math.min(ZOOM_MAX, Math.max(1, next));
    if (s === zoom.s) return;
    const r = stageEl.getBoundingClientRect();
    const qx = cx - (r.left + r.width / 2);
    const qy = cy - (r.top + r.height / 2);
    const k = s / zoom.s;
    zoom.x = qx - (qx - zoom.x) * k;
    zoom.y = qy - (qy - zoom.y) * k;
    zoom.s = s;
    if (s <= 1.001) { zoom.s = 1; zoom.x = 0; zoom.y = 0; } else clampPan();
    paintZoom();
  }

  function zoomAtCentre(next) {
    const r = stageEl.getBoundingClientRect();
    zoomTo(next, r.left + r.width / 2, r.top + r.height / 2);
  }

  // True when the pointer is on the media itself and not on a video's native control bar.
  function onMediaSurface(e) {
    const el = mediaEl();
    if (!el || e.target !== el) return false;
    if (el instanceof HTMLVideoElement && el.controls) {
      const r = el.getBoundingClientRect();
      const bar = CONTROL_BAR_PX * (r.height / Math.max(1, el.offsetHeight));
      if (e.clientY > r.bottom - bar) return false;
    }
    return true;
  }

  function toggleActual(cx, cy) {
    const el = mediaEl();
    if (!el) return;
    if (zoom.s > 1.001) zoomTo(1, cx, cy);
    else zoomTo(actualScale(el), cx, cy);
  }

  stageEl.addEventListener("wheel", (e) => {
    if (!onMediaSurface(e)) return;
    e.preventDefault();
    const unit = e.deltaMode === 1 ? 16 : e.deltaMode === 2 ? 400 : 1;
    zoomTo(zoom.s * Math.exp(-e.deltaY * unit * 0.0015), e.clientX, e.clientY);
  }, { passive: false });

  // Capture phase: a double click on a video would otherwise reach its native full-screen toggle.
  stageEl.addEventListener("dblclick", (e) => {
    if (!onMediaSurface(e)) return;
    e.preventDefault();
    e.stopPropagation();
    toggleActual(e.clientX, e.clientY);
  }, true);

  stageEl.addEventListener("pointerdown", (e) => {
    if (e.button !== 0 || zoom.s <= 1.001 || !onMediaSurface(e)) return;
    zoom.drag = { id: e.pointerId, x: e.clientX, y: e.clientY, lx: e.clientX, ly: e.clientY, active: false };
  });
  stageEl.addEventListener("pointermove", (e) => {
    const d = zoom.drag;
    if (!d || d.id !== e.pointerId) return;
    const dx = e.clientX - d.x;
    const dy = e.clientY - d.y;
    if (!d.active) {
      if (Math.hypot(dx, dy) < PAN_START_PX) return;
      d.active = true;
      try { stageEl.setPointerCapture(e.pointerId); } catch (_) { /* pointer already gone */ }
    }
    // Moves by what changed since the last event, so a zoom in the middle of a drag is kept.
    zoom.x += e.clientX - d.lx;
    zoom.y += e.clientY - d.ly;
    d.lx = e.clientX;
    d.ly = e.clientY;
    clampPan();
    paintZoom();
  });
  const endDrag = (e) => {
    if (!zoom.drag || zoom.drag.id !== e.pointerId) return;
    zoom.drag = null;
    paintZoom();
  };
  // Without the native controls a zoomed video no longer toggles play on a click itself.
  stageEl.addEventListener("click", (e) => {
    const el = mediaEl();
    if (zoom.s > 1.001 && el instanceof HTMLVideoElement && e.target === el) togglePlay();
  });
  stageEl.addEventListener("pointerup", endDrag);
  stageEl.addEventListener("pointercancel", endDrag);
  window.addEventListener("resize", () => { if (zoom.s > 1.001) { clampPan(); paintZoom(); } });

  const indexOf = (name) => ctx.view().findIndex((f) => f.name === name);

  function releaseStage() {
    const stage = $("mm-view-stage");
    // A playing video keeps its file open; drop the source so the file can be deleted or moved.
    for (const v of stage.querySelectorAll("video")) {
      v.pause();
      v.removeAttribute("src");
      v.load();
    }
    stage.textContent = "";
    loadedKey = "";
    resetZoom();
  }

  function message(text, f, withPlayer) {
    const stage = $("mm-view-stage");
    stage.textContent = "";
    const box = document.createElement("div");
    box.className = "mm-view-msg";
    const poster = f ? ctx.posterUrl(f) : "";
    if (poster) {
      const img = document.createElement("img");
      img.alt = "";
      img.draggable = false;
      img.src = poster;
      box.appendChild(img);
    }
    const p = document.createElement("p");
    p.textContent = text;
    box.appendChild(p);
    if (withPlayer) {
      const b = document.createElement("button");
      b.type = "button";
      b.className = "ghost-btn";
      b.textContent = t("mmOpenPlayer");
      b.addEventListener("click", () => ctx.actions.player(f.name));
      box.appendChild(b);
    }
    stage.appendChild(box);
  }

  async function loadStage(f) {
    const key = `${ctx.kind()}|${f.name}|${f.modified_ms}|${f.size}`;
    if (key === loadedKey) return;
    releaseStage();
    loadedKey = key;
    const mine = ++seq;
    const stage = $("mm-view-stage");
    if (ctx.kind() === "screenshots") {
      let url = "";
      try { url = await invoke("media_preview_image", { name: f.name }); } catch (_) { url = ""; }
      if (mine !== seq) return;
      if (!url) { message(t("mmPreviewFailed"), f, false); return; }
      const img = document.createElement("img");
      img.alt = f.name;
      img.draggable = false;
      img.addEventListener("error", () => { if (mine === seq) message(t("mmPreviewFailed"), f, false); });
      img.src = url;
      stage.appendChild(img);
      return;
    }
    const src = convertFileSrc(f.path);
    if (VIDEO_TAG_EXTS.has(f.ext)) {
      const v = document.createElement("video");
      v.controls = true;
      v.preload = "metadata";
      v.playsInline = true;
      const poster = ctx.posterUrl(f);
      if (poster) v.poster = poster;
      v.setAttribute("aria-label", f.name);
      v.addEventListener("error", () => { if (mine === seq && v.getAttribute("src")) message(t("mmNoInlinePlay"), f, true); });
      v.src = src;
      stage.appendChild(v);
      // Start right away; if the webview refuses sound-on autoplay, retry once muted (the controls let the user unmute).
      const started = v.play();
      if (started && started.catch) {
        started.catch(() => {
          if (mine !== seq || !v.getAttribute("src")) return;
          v.muted = true;
          v.play().catch(() => {});
        });
      }
    } else if (IMG_TAG_EXTS.has(f.ext)) {
      const img = document.createElement("img");
      img.alt = f.name;
      img.draggable = false;
      img.addEventListener("error", () => { if (mine === seq) message(t("mmNoInlinePlay"), f, true); });
      img.src = src;
      stage.appendChild(img);
    } else {
      message(t("mmNoInlinePlay"), f, true);
    }
  }

  function paintLabels() {
    const recs = ctx.kind() === "recordings";
    $("mm-view-copy").textContent = t(recs ? "mmCopyFile" : "mmCopyImage");
    $("mm-view-player").classList.toggle("hidden", !recs);
    $("mm-view-export").classList.toggle("hidden", !recs);
    $("mm-view-back").title = `${t("mmBack")} (Esc)`;
    $("mm-view-edit").title = `${t("mmOpenEditor")} (Enter)`;
    for (const [id, key, keyName] of [["mm-view-prev", "mmPrev", "←"], ["mm-view-next", "mmNext", "→"]]) {
      $(id).title = `${t(key)} (${keyName})`;
      $(id).setAttribute("aria-label", t(key));
    }
  }

  // Draws the bar for the file on show and loads its picture or video when it changed.
  function render() {
    if (!current) return;
    const f = ctx.byName(current);
    if (!f) { close(); return; }
    const list = ctx.view();
    const at = indexOf(current);
    if (at >= 0) lastAt = at;
    $("mm-view-name").textContent = f.name;
    $("mm-view-name").title = f.name;
    $("mm-view-meta").textContent = ctx.metaText(f);
    $("mm-view-pos").textContent = at >= 0 ? fill("mmPosOf", { n: at + 1, total: list.length }) : "";
    $("mm-view-prev").disabled = at <= 0;
    $("mm-view-next").disabled = at < 0 || at >= list.length - 1;
    paintLabels();
    loadStage(f);
  }

  function open(name) {
    if (!ctx.byName(name)) return;
    current = name;
    document.body.classList.add("mm-viewing");
    $("mm-view").classList.remove("hidden");
    render();
    $("mm-view-edit").focus({ preventScroll: true });
  }

  function close() {
    if (!current) return;
    const was = current;
    current = "";
    seq++;
    releaseStage();
    $("mm-view").classList.add("hidden");
    document.body.classList.remove("mm-viewing");
    ctx.onClose(was);
  }

  function step(d) {
    const list = ctx.view();
    const at = indexOf(current);
    const next = list[at + d];
    if (at < 0 || !next) return;
    current = next.name;
    render();
  }

  // After the list was reloaded: a file that is gone (deleted here or elsewhere) makes way for
  // the one that moved into its place, and the view closes when nothing is left.
  function sync() {
    if (!current) return;
    if (!ctx.byName(current) || indexOf(current) < 0) {
      const list = ctx.view();
      if (!list.length) { close(); return; }
      current = list[Math.min(lastAt, list.length - 1)].name;
    }
    render();
  }

  // The relist after the move runs sync(), which shows the next file; a file that could not be
  // moved is shown again.
  async function discardCurrent() {
    const name = current;
    releaseStage(); // the video must let go of the file before it can be moved away
    await ctx.actions.discard(name);
    if (current === name) render();
  }

  function togglePlay() {
    const v = $("mm-view-stage").querySelector("video");
    if (!v) return false;
    if (v.paused) v.play().catch(() => {}); else v.pause();
    return true;
  }

  // Keys while the view is open. Returns true when the key was used.
  function onKey(e) {
    if (!current) return false;
    const onButton = e.target instanceof HTMLButtonElement;
    const onVideo = e.target instanceof HTMLVideoElement;
    if (e.key === "Escape" || (e.key === "Backspace" && !onButton)) { close(); return true; }
    if (e.key === "ArrowLeft" && !onVideo) { step(-1); return true; }
    if (e.key === "ArrowRight" && !onVideo) { step(1); return true; }
    if (e.key === "Enter" && !onButton) { pauseStage(); ctx.actions.edit(current); return true; }
    if (e.key === " " && !onButton && !onVideo) return togglePlay();
    if (!e.ctrlKey && !e.metaKey && !e.altKey) {
      if (e.key === "+" || e.key === "=") { zoomAtCentre(zoom.s * 1.5); return true; }
      if (e.key === "-" || e.key === "_") { zoomAtCentre(zoom.s / 1.5); return true; }
      if (e.key === "0") { zoomAtCentre(1); return true; }
    }
    return false;
  }

  $("mm-view-back").addEventListener("click", close);
  $("mm-view-prev").addEventListener("click", () => step(-1));
  $("mm-view-next").addEventListener("click", () => step(1));
  // Anything that opens another window stops the large view's video first, so it does not
  // play on behind it.
  function pauseStage() {
    $("mm-view-stage").querySelector("video")?.pause();
  }
  $("mm-view-edit").addEventListener("click", () => { pauseStage(); ctx.actions.edit(current); });
  $("mm-view-copy").addEventListener("click", () => ctx.actions.copy(current));
  $("mm-view-player").addEventListener("click", () => {
    pauseStage();
    ctx.actions.player(current);
  });
  $("mm-view-export").addEventListener("click", () => { pauseStage(); ctx.actions.exportOne(current); });
  $("mm-view-reveal").addEventListener("click", () => ctx.actions.reveal(current));
  $("mm-view-del").addEventListener("click", discardCurrent);

  return { open, close, isOpen, sync, onKey, current: () => current };
}
