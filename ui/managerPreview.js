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
    if (e.key === "Enter" && !onButton) { ctx.actions.edit(current); return true; }
    if (e.key === " " && !onButton && !onVideo) return togglePlay();
    return false;
  }

  $("mm-view-back").addEventListener("click", close);
  $("mm-view-prev").addEventListener("click", () => step(-1));
  $("mm-view-next").addEventListener("click", () => step(1));
  $("mm-view-edit").addEventListener("click", () => ctx.actions.edit(current));
  $("mm-view-copy").addEventListener("click", () => ctx.actions.copy(current));
  $("mm-view-player").addEventListener("click", () => {
    const v = $("mm-view-stage").querySelector("video");
    if (v) v.pause();
    ctx.actions.player(current);
  });
  $("mm-view-export").addEventListener("click", () => ctx.actions.exportOne(current));
  $("mm-view-reveal").addEventListener("click", () => ctx.actions.reveal(current));
  $("mm-view-del").addEventListener("click", discardCurrent);

  return { open, close, isOpen, sync, onKey, current: () => current };
}
