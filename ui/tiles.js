// Grid tiles: building, auto-fit, images, video thumbs, drag-out and auto-paste.
import { assignDrag, assignMemberDrag, assignSettings, BUBBLE_MS, convertFileSrc, drag, DRAG_THRESHOLD, errToast, favView, flag, folderArm, gateOpen, gridEl, gridSelected, gridTags, ICON_FILE, ICON_IMAGE, ICON_KEYBOARD, ICON_PDF, ICON_VIDEO, IMG_WAIT, inkShadow, invoke, journalThumbCache, LANG, libraryEl, libThumbCache, LOCK_SVG, mediaBarLabels, memberDrag, optFlag, PDF_EXT, prompts, readableInk, ROTATE_SHORT_MAX, settings, t, toast, txt, undoToast, val, videoCache } from "./core.js";
import { applyChainToTile, chainMode, toggleChainPick } from "./chain.js";
import { applyVideoPrefs, buildMediaBar, mediaKind } from "./media.js";
import { fitText, FONTS } from "./fonts.js";
import { FLAG_GATES } from "./expertConfig.js";
import { applyCopySizeLevel, DRAG_OUT_MODS, fileUri, rotateBadgeLabel } from "./floatShared.js";
import { activeView, clearViewDropHover, dropTilesOnView, gridKeyOf, layoutOf, openGridSizeFor, openMiniMenu, placeFav, renderGrid, resolveViewDropTarget, updateViewDropHover, viewsContaining } from "./views.js";
import { dialogTokens, peekRotatedText } from "./promptVars.js";
import { blockedByCooldown, copyTextWithVars, maybeCloseOnCopy, recordCopy } from "./copyFlow.js";
import { copyResolved, dropLibraryFront, tagMatches, thumbEl, toggleGridSelect } from "./library.js";
import { tileTimeLines } from "./journal.js";
import { prettyAccel, promptHotkeysOn } from "./hotkeys.js";
import { recordToolUsage } from "./management.js";
import { openModal } from "./editor.js";
import { armFolderDrop, disarmFolder, dropIntoFolder, endMemberDragLayers, folderById, openTileFolderPop, saveFolder } from "./gridFolders.js";
import { openCtx } from "./gridMenus.js";

// Auto-fit cache per (text, cell size); cleared on font changes.
export const fitCache = new Map();
export const FIT_QUANT = 8; // measurement-box bucket size (see fitAllTiles)

// All fitting is measured inside one off-screen ruler pinned to 0/0, so the result never
// depends on a tile's own (sub)pixel position, cell or DPI.
export let ruler = null;
export function getRuler() {
  if (!ruler) {
    ruler = document.createElement("span");
    ruler.className = "tile-name fit tile-ruler";
    document.body.appendChild(ruler);
  }
  return ruler;
}

// Largest font size where the text fits the shared measurement box, wrapped or (oneLine) on a
// single line.
function measureFit(name, maxW, maxH, oneLine) {
  const key = `${oneLine ? "1|" : ""}${name.textContent}|${name.style.fontFamily}|${maxW}x${maxH}`;
  let size = fitCache.get(key);
  if (size == null) {
    const r = getRuler();
    r.style.fontFamily = name.style.fontFamily;
    r.style.whiteSpace = oneLine ? "nowrap" : "";
    r.textContent = name.textContent;
    r.style.width = `${maxW}px`;
    // scrollWidth exceeds maxW only when a single unbreakable word overflows (or, on one line,
    // when the whole text is wider than the box).
    const fits = (s) => {
      r.style.fontSize = `${s}px`;
      return r.scrollHeight <= maxH && r.scrollWidth <= maxW;
    };
    let lo = 8;
    let hi = Math.max(8, Math.min(96, maxH));
    while (lo < hi) {
      const mid = Math.ceil((lo + hi) / 2);
      if (fits(mid)) lo = mid;
      else hi = mid - 1;
    }
    r.style.whiteSpace = "";
    size = lo;
    if (fitCache.size > 1000) fitCache.clear();
    fitCache.set(key, size);
  }
  return size;
}

// Largest font size where the wrapped text fits the shared measurement box.
export function fitTileText(tile, maxW, maxH) {
  if (tile.classList.contains("has-image") || tile.dataset.fitMode === "fixed") return;
  const name = tile.querySelector(".tile-name");
  if (!name) return;
  name.classList.add("fit");
  name.classList.remove("fit-one-line");
  name.style.fontSize = `${measureFit(name, maxW, maxH, false)}px`;
}

// A one-line size must keep at least this share of the wrapping size to win.
export const ONE_LINE_SHARE = 0.8;

// The tiles of one grid: every name on a single line at one shared size when that size keeps
// at least ONE_LINE_SHARE of the largest wrapping size, else each name gets its wrapping size
// (a name never wraps while its neighbours stay on one line unless shrinking everything would
// cost more than a fifth of the text size).
export function fitTilesShared(tiles, maxW, maxH) {
  const names = [];
  for (const tile of tiles) {
    if (tile.classList.contains("has-image") || tile.dataset.fitMode === "fixed") continue;
    const name = tile.querySelector(".tile-name");
    if (name) names.push(name);
  }
  if (!names.length) return;
  let wrapMax = 0;
  let oneLine = Infinity;
  for (const name of names) {
    name.classList.add("fit");
    wrapMax = Math.max(wrapMax, measureFit(name, maxW, maxH, false));
    oneLine = Math.min(oneLine, measureFit(name, maxW, maxH, true));
  }
  const single = oneLine >= ONE_LINE_SHARE * wrapMax;
  for (const name of names) {
    name.classList.toggle("fit-one-line", single);
    name.style.fontSize = `${single ? oneLine : measureFit(name, maxW, maxH, false)}px`;
  }
}

// Measurement box cached per grid state: while the window and grid layout are unchanged every
// render uses the exact same box, so moving tiles never changes a fitted text size.
export const fitBox = { key: "", maxW: 0, maxH: 0 };

// `boxOnly` skips the per-tile work while the measurement box is unchanged. The resize
// handler uses it: dragging the window edge changes the pixel size every frame but the
// quantized box moves only every FIT_QUANT px, and re-fitting in between made the grid
// shimmer.
export function fitAllTiles(boxOnly) {
  const globalAuto = Number(settings.tile_size) === 0;
  const tiles = [...gridEl.querySelectorAll(".tile")].filter(
    (tile) =>
      (globalAuto || tile.dataset.fitMode === "auto") &&
      tile.dataset.fitMode !== "fixed" &&
      !tile.classList.contains("has-image")
  );
  if (!tiles.length) return;
  const key =
    `${gridEl.style.gridTemplateColumns}|${gridEl.style.gridTemplateRows}` +
    // Include the gap: it changes the cell size without changing the grid's own size or
    // template, so a gap change must not reuse a stale fit box.
    `|${gridEl.clientWidth}x${gridEl.clientHeight}|${getComputedStyle(gridEl).gap}`;
  let boxMoved = false;
  if (fitBox.key !== key) {
    const prevW = fitBox.maxW;
    const prevH = fitBox.maxH;
    // Shared box = smallest grid cell (cells exist for every slot) minus the tile chrome from
    // computed style: fractional-exact, no per-cell rounding.
    let cellW = Infinity;
    let cellH = Infinity;
    for (const cell of gridEl.children) {
      cellW = Math.min(cellW, cell.clientWidth);
      cellH = Math.min(cellH, cell.clientHeight);
    }
    if (!Number.isFinite(cellW)) return;
    const cs = getComputedStyle(tiles[0]);
    const chromeW =
      parseFloat(cs.borderLeftWidth) + parseFloat(cs.borderRightWidth) +
      parseFloat(cs.paddingLeft) + parseFloat(cs.paddingRight);
    const chromeH =
      parseFloat(cs.borderTopWidth) + parseFloat(cs.borderBottomWidth) +
      parseFloat(cs.paddingTop) + parseFloat(cs.paddingBottom);
    fitBox.key = key;
    // Quantize to an 8px bucket: DPI handoffs and monitor drags shift cell rounding by ±1px, and
    // within one bucket the fitted size cannot change; the slack keeps text from clipping after
    // such a shift. A little air around the text, and on a normal-size tile enough above and
    // below to keep it clear of the corner badges (16px icon at 6px).
    const airX = 2 * Math.round(cellW * 0.04);
    const airY = cellH >= 80 ? 2 * 16 : 2 * 4;
    fitBox.maxW = Math.floor((cellW - chromeW - airX) / FIT_QUANT) * FIT_QUANT;
    fitBox.maxH = Math.floor((cellH - chromeH - 2 - airY) / FIT_QUANT) * FIT_QUANT;
    boxMoved = fitBox.maxW !== prevW || fitBox.maxH !== prevH;
  }
  if (boxOnly && !boxMoved) return;
  if (fitBox.maxW <= 0 || fitBox.maxH <= 0) return;
  fitTilesShared(tiles, fitBox.maxW, fitBox.maxH);
}

// Re-fit on window resize (cells change size with the window).
export let fitRaf = 0;

// Re-measure cells when the window moves to a monitor with a different scale factor. The fit
// cache is kept: sizes are DPI-independent CSS px and the quantized box leaves enough slack
// that ±1px rounding cannot clip, so a cross-monitor drag never changes a fitted size.
export function watchDpr() {
  matchMedia(`(resolution: ${devicePixelRatio}dppx)`).addEventListener(
    "change",
    () => {
      fitBox.key = ""; // cell rounding differs at the new DPI
      fitAllTiles();
      watchDpr();
    },
    { once: true }
  );
}

// ---- Private notes: context the user keeps next to a prompt. Read in the editor, the tile
// tooltip and the library search; never in any copy path.
export const notesOn = () => flag("promptNotes");
// Cap what the tooltip shows so one long note can never fill the screen.
export const clipNote = (s) => {
  const max = val("notesTipLen");
  return max > 0 && s.length > max ? `${s.slice(0, max)}…` : s;
};

// Client-side cache of each rotating prompt's "next entry" state (index/pos/total/text),
// kept in sync by the backend's rotation-changed event, which every copy path in every
// window fires after the pointer moves. Populated lazily when a tile for that id first
// renders (primeRotateState); the badge and tooltip stay blank for one frame.
export const rotateState = new Map(); // id -> { index, pos, total, text }
// Per-id request generation: a rotation-changed push (another window's copy) can land while
// this id's initial rotation_peek is in flight, and the peek's late reply would overwrite the
// fresher position with stale data (same shape as floating.js's rotateReq and folder.js's
// rotateReqs).
export const rotateReqs = new Map(); // id -> latest request number

// Pushes the cached state (however stale/empty) into one tile's badge + tooltip.
export function applyRotateState(tile, id) {
  const badge = tile.querySelector(".tile-rotate-badge");
  if (badge) badge.textContent = rotateBadgeLabel(rotateState.get(id));
  const s = rotateState.get(id);
  if (s) {
    const short = ROTATE_SHORT_MAX > 0 && s.text.length > ROTATE_SHORT_MAX
      ? `${s.text.slice(0, ROTATE_SHORT_MAX)}…` : s.text;
    tile.dataset.tipRotate = t("tileTipRotate").replace("{text}", () => short);
  }
}

// Ids with a live auto-advance session, kept by the backend's rotation-auto event (fired on
// arm/end by the clipboard watcher).
export const autoActiveIds = new Set();

export function applyAutoBadge(tile, id) {
  const badge = tile.querySelector(".tile-rotate-badge");
  if (!badge) return;
  const active = autoActiveIds.has(id);
  badge.classList.toggle("is-auto", active);
  if (active) badge.title = t("rotateAutoActive");
  else badge.removeAttribute("title");
}

export function primeRotateState(id) {
  if (rotateState.has(id)) return;
  const req = (rotateReqs.get(id) || 0) + 1;
  rotateReqs.set(id, req);
  invoke("rotation_peek", { id }).then((r) => {
    if (!r || rotateReqs.get(id) !== req) return;
    rotateState.set(id, r);
    document.querySelectorAll(`.tile[data-id="${CSS.escape(id)}"]`).forEach((tile) => applyRotateState(tile, id));
  }).catch(() => {});
}

// ---- Stored images, fetched one prompt at a time ----
// get_state hands the window `has_image` + `image_rev` instead of the picture: a stored image
// is a base64 PNG of up to a few hundred KB, and sending every one through IPC on each render
// was by far the most expensive thing the app did. A surface that wants one asks here; until
// the bytes arrive the tile keeps its plain name/colour face, so nothing flashes an empty
// frame.
export const imgCache = new Map(); // id -> data URL; insertion order = least recently used first
export const imgCacheRev = new Map(); // id -> the image_rev the cached data URL belongs to
export const imgPending = new Set(); // ids with a prompt_image call already in flight
// Several screens' worth of image tiles: enough that scrolling the library or hopping between
// views never refetches, low enough that the cache stays a few tens of MB even with
// full-size screenshots.
export const IMG_CACHE_MAX = 80;

// True for both shapes a prompt arrives in: the slim one from get_state and the
// full one from get_prompt/list_trash, which still carries its image inline.
export function hasImage(p) {
  return !!p && (p.has_image === true || !!p.image);
}

// What a prompt shows as media on a tile, derived the same way for a grid tile and a folder
// member: `has` (known from the prompt list) decides the layout, `stored` (the bytes, maybe
// still loading) whether the image can be drawn yet, `showKind` and `showPath` the
// gif/video/image taken from the icon path or the file path.
export function tileMedia(p) {
  const kind = p.file_path ? mediaKind(p.file_path) : "";
  const has = hasImage(p);
  const stored = has ? imageOf(p) : "";
  const iconKind = !has && p.icon_path ? mediaKind(p.icon_path) : "";
  const showPath = iconKind ? p.icon_path : p.file_path;
  const showKind = has ? "" : iconKind || kind;
  return { kind, has, stored, showPath, showKind };
}

// The type badge icon: what the button COPIES (file/image/video), "" for plain text.
export function tileTypeIcon(p, kind) {
  return p.file_path
    ? (kind === "video" ? ICON_VIDEO : kind ? ICON_IMAGE : PDF_EXT.test(p.file_path) ? ICON_PDF : ICON_FILE)
    : p.copy_image ? ICON_IMAGE : "";
}

// The prompt's stored image, or "" while it is still on its way (or absent).
export function imageOf(p) {
  if (!p) return "";
  if (p.image) return p.image; // a full prompt brought its own
  if (!p.has_image) return ""; // nothing to ask for: never spend a call on it
  const id = p.id;
  const rev = p.image_rev || 0;
  if (imgCache.has(id)) {
    if (imgCacheRev.get(id) === rev) {
      const data = imgCache.get(id);
      imgCache.delete(id); // re-insert, so the eviction order stays oldest-access-first
      imgCache.set(id, data);
      return data;
    }
    imgCache.delete(id); // the picture changed under us
    imgCacheRev.delete(id);
  }
  if (!imgPending.has(id)) {
    imgPending.add(id);
    invoke("prompt_image", { id })
      .then((data) => {
        imgPending.delete(id);
        // An empty answer is cached too, or a prompt whose image went away between the two calls
        // would be asked for again on every render.
        imgCache.set(id, data || "");
        imgCacheRev.set(id, rev);
        while (imgCache.size > IMG_CACHE_MAX) {
          const oldest = imgCache.keys().next().value;
          imgCache.delete(oldest);
          imgCacheRev.delete(oldest);
        }
        if (data) scheduleImagePaint();
      })
      .catch(() => { imgPending.delete(id); });
  }
  return "";
}

// One repaint for a whole batch of arrivals instead of one per image.
export let imagePaintQueued = false;
export function scheduleImagePaint() {
  if (imagePaintQueued) return;
  imagePaintQueued = true;
  setTimeout(() => {
    imagePaintQueued = false;
    if (drag) return; // a drag owns the tiles it moves; its end re-renders anyway
    for (const el of document.querySelectorAll(`[data-img-wait]`)) {
      const p = prompts.find((x) => x.id === el.dataset[IMG_WAIT]);
      if (!p || !imageOf(p)) continue;
      if (el.classList.contains("tile")) {
        el.replaceWith(buildTile(p));
      } else {
        // A library or copy-history row preview: its panel owns the element cache, so drop the
        // placeholder entry before asking for a fresh thumbnail.
        const cache = el.closest("#library-panel-history") ? journalThumbCache : libThumbCache;
        cache.delete(p.id);
        el.replaceWith(thumbEl(p, cache));
      }
    }
    fitAllTiles();
  }, 0);
}

export function buildTile(p) {
  const tile = document.createElement("div");
  tile.className = "tile";
  tile.dataset.id = p.id;
  // A tile acts as a button named after the prompt (a locked one only says that it is secret).
  tile.setAttribute("role", "button");
  tile.setAttribute("aria-label", p.locked ? t("editorSecretLabel") : p.name);
  if (gridSelected.has(p.id)) tile.classList.add("grid-selected");
  // A tag filter dims what does not match instead of hiding it: the cell stays occupied, so
  // drag, reorder and grid geometry never treat it as empty.
  if (gridTags.size && !tagMatches(p, gridTags)) tile.classList.add("tile-dimmed");
  // Confidential and no vault key right now: the name is redacted behind a bar and a lock
  // badge replaces the type badge. Text and variants are already blank in `p` (see
  // slim_prompts on the backend), so nothing here has the real text to show.
  if (p.locked) {
    tile.classList.add("tile-locked");
    const lock = document.createElement("span");
    lock.className = "tile-lock-badge";
    lock.title = t("editorSecretLabel");
    lock.setAttribute("aria-label", t("editorSecretLabel"));
    lock.innerHTML = LOCK_SVG;
    tile.appendChild(lock);
  }
  // Bulk-selection checkbox. Its own pointerdown/click stop propagation so a press here never
  // arms the tile's reorder drag or starts a copy.
  const selBtn = document.createElement("button");
  selBtn.type = "button";
  selBtn.className = "tile-select";
  selBtn.setAttribute("aria-label", t("selectPrompt"));
  selBtn.addEventListener("pointerdown", (e) => e.stopPropagation());
  selBtn.addEventListener("click", (e) => { e.stopPropagation(); toggleGridSelect(p.id); });
  tile.appendChild(selBtn);
  const preview = p.file_path || p.text || "";
  // Themed hover tooltip (rendered by wireTooltips' shared popup, not the native title): stash
  // name and raw text on the tile; the popup shortens it while it draws (fillVarHighlighted)
  // and grows tall so long prompts read comfortably.
  tile.dataset.tipName = p.name;
  if (preview && preview !== p.name) tile.dataset.tipBody = preview;
  // The prompt's own hotkey rides along as a line after the body; the corner badge cannot show
  // it (it ignores the pointer).
  if (p.hotkey && promptHotkeysOn()) tile.dataset.tipHotkey = t("tileHotkeyBadgeTitle").replace("{accel}", () => prettyAccel(p.hotkey));
  // Two prompts of very different length look identical on the tile: show the same
  // char/~token readout the editor shows, so there is one formula for "how long is this".
  if (p.text) {
    const chars = [...p.text].length;
    const tokens = Math.ceil(chars / 4);
    const nf = (n) => n.toLocaleString(LANG);
    tile.dataset.tipLen = `${nf(chars)} ${t("lenChars")} · ~${nf(tokens)} ${t("lenTokens")}`;
  }
  // The private note rides along as its own dimmed section (opt-out flag).
  const note = notesOn() && flag("notesInTooltip") ? (p.notes || "").trim() : "";
  if (note) tile.dataset.tipNotes = clipNote(note);
  const times = tileTimeLines(p.id);
  if (times) tile.dataset.tipTimes = times;

  // Media on the tile: stored still, or gif/video from the icon/file path. A tile waiting for
  // its picture falls back to the name/colour face instead of an empty frame or an unrelated
  // icon-path preview.
  const { kind, has, stored, showPath, showKind } = tileMedia(p);
  const pathVideo = p.show_image && !has && showKind === "video";
  const pathGif = p.show_image && !has && showKind === "gif";
  if (has && !stored) tile.dataset[IMG_WAIT] = p.id; // redrawn once the bytes land
  if (p.show_image && (stored || pathVideo || pathGif)) {
    tile.classList.add("has-image");
    // The chosen color tints the border area around the image.
    if (p.color) {
      tile.style.background = p.color;
      tile.style.borderColor = p.color;
    }
    if (pathVideo) {
      tile.appendChild(getVideoWrap(p, convertFileSrc(showPath)));
      // The control bar only appears while the mouse is in the lower part.
      tile.addEventListener("mousemove", (e) => {
        const r = tile.getBoundingClientRect();
        const zone = Math.max(48, r.height * 0.35);
        // Stay visible while on the bar itself (volume popup reaches higher).
        tile.classList.toggle(
          "media-hover",
          e.clientY > r.bottom - zone || !!e.target.closest(".media-bar")
        );
      });
      tile.addEventListener("mouseleave", () => {
        if (!tile.querySelector(".media-sound.dragging")) {
          tile.classList.remove("media-hover");
        }
      });
    } else {
      const img = document.createElement("img");
      img.className = "tile-img";
      img.src = stored || convertFileSrc(showPath);
      img.draggable = false;
      tile.appendChild(img);
    }
    // Optional caption overlay (0 = default, 1 = auto-scale, else fixed px).
    if (p.caption && flag("captions")) {
      const cap = document.createElement("span");
      cap.className = "tile-caption";
      cap.textContent = p.caption;
      if (p.caption_size === 1) cap.classList.add("auto");
      else if (p.caption_size > 1) cap.style.fontSize = `${p.caption_size}px`;
      tile.appendChild(cap);
    }
  } else if (p.color) {
    tile.classList.add("tinted");
    tile.style.background = p.color;
    tile.style.borderColor = p.color;
    tile.style.color = readableInk(p.color);
    tile.style.setProperty("--ink-shadow", inkShadow(p.color));
  }

  const name = document.createElement("span");
  name.className = "tile-name";
  name.textContent = p.name;
  // Per-tile style overrides (0 = follow settings, 1 = auto-fit, else fixed).
  if (p.font) name.style.fontFamily = FONTS[p.font] || "";
  if (p.font_size === 1) {
    tile.dataset.fitMode = "auto";
  } else if (p.font_size > 1) {
    name.style.fontSize = `${p.font_size}px`;
    tile.dataset.fitMode = "fixed";
  }
  tile.appendChild(name);

  // Subtle type badge in the top-left corner. It reflects what the button copies
  // (file/image/video); a decorative media icon never changes it.
  const typeIcon = tileTypeIcon(p, kind);
  if (typeIcon && flag("typeBadges")) {
    const badge = document.createElement("span");
    badge.className = "tile-type";
    badge.innerHTML = typeIcon;
    tile.appendChild(badge);
  }

  // A prompt's own hotkey (set in the editor's Advanced options) is otherwise invisible until
  // used, so it stays visible. Sits right of the type badge/checkbox slot so it never swaps
  // out with either on selection.
  if (p.hotkey && promptHotkeysOn() && flag("tileHotkeyBadge")) {
    const hkBadge = document.createElement("span");
    hkBadge.className = "tile-hotkey-badge";
    const hkLabel = t("tileHotkeyBadgeTitle").replace("{accel}", () => prettyAccel(p.hotkey));
    hkBadge.title = hkLabel;
    hkBadge.setAttribute("aria-label", hkLabel);
    hkBadge.innerHTML = ICON_KEYBOARD;
    tile.appendChild(hkBadge);
  }

  // A favorite shows a small star in the top-right corner, like the library row. Display only:
  // it takes no clicks, so pressing near the corner still copies. Hidden in the favourites
  // view, where every tile is a favorite.
  if (p.favorite && flag("favorites") && gateOpen(FLAG_GATES.tileFavBadge) && flag("tileFavBadge") && !favView) {
    const favBadge = document.createElement("span");
    favBadge.className = "tile-fav-badge";
    favBadge.setAttribute("aria-hidden", "true");
    favBadge.textContent = "★";
    tile.appendChild(favBadge);
  }

  // Rotating prompt: "next entry" position pill, bottom-left (a rotating prompt is text-only,
  // so that corner never competes with a media caption or control bar). Text fills in from the
  // cache once primed below. rotateBadge is a sub-toggle of rotatingPrompts: flag() alone
  // ignores the parent, gateOpen() walks the declared chain.
  const rotateUiOn = flag("rotateBadge") && gateOpen(FLAG_GATES.rotateBadge);
  if ((p.variants || []).length && rotateUiOn) {
    const rbadge = document.createElement("span");
    rbadge.className = "tile-rotate-badge";
    tile.appendChild(rbadge);
    applyAutoBadge(tile, p.id); // survives a re-render mid-session
  }

  // The actions menu (right-click) can be switched off entirely.
  if (flag("tileMenu")) {
    tile.addEventListener("contextmenu", (e) => {
      e.preventDefault();
      e.stopPropagation();
      openCtx(p.id, e.clientX, e.clientY);
    });
  }

  tile.addEventListener("pointerdown", (e) => {
    if (e.button !== 0 || e.target.closest(".tile-select")) return;
    // With the modifier held the press becomes a drag-out; `out` keeps the reorder machine out of
    // it. Ctrl+click without moving selects the tile.
    const out = armDragOut(tile, e, () => prompts.find((x) => x.id === p.id));
    assignDrag({ id: p.id, startX: e.clientX, startY: e.clientY, moved: false, el: tile, out });
  });

  // Prompts with an attached file or media icon get a persistent error banner while that file
  // is missing.
  if (p.file_path || p.icon_path) {
    const err = document.createElement("span");
    err.className = "tile-error";
    err.textContent = t("fileMissing");
    tile.appendChild(err);
    if (missingFiles.has(p.id)) tile.classList.add("file-missing");
  }
  // Gated like the badge above: an unconditional peek would fire one rotation_peek IPC call per
  // rotating tile even with the badge (or its parent feature) off.
  if ((p.variants || []).length && rotateUiOn) {
    primeRotateState(p.id); // async: patches this tile again once the peek lands
    applyRotateState(tile, p.id); // sync: reflects whatever is cached right now
  }
  if (chainMode) applyChainToTile(tile, p); // keep badges/skip across re-renders
  return tile;
}

export function getVideoWrap(p, src) {
  const cached = videoCache.get(p.id);
  if (cached && cached.src === src) return cached.wrap;
  const wrap = document.createElement("div");
  wrap.className = "tile-media";
  const video = document.createElement("video");
  video.className = "tile-video";
  video.src = src;
  video.muted = true;
  video.loop = true;
  video.autoplay = optFlag("videoAutoplay");
  video.playsInline = true;
  wrap.append(video);
  if (flag("videoControls")) {
    wrap.append(buildMediaBar(video, {
      onChange: (prefs) => invoke("set_video_prefs", { id: p.id, ...prefs }).catch(() => {}),
      labels: mediaBarLabels(),
    }));
  }
  applyVideoPrefs(video, settings.video_prefs?.[p.id]);
  // The autoplay attribute alone can be unreliable for dynamically created elements; muted
  // videos may play.
  if (optFlag("videoAutoplay")) video.play().catch(() => {});
  videoCache.set(p.id, { src, wrap });
  return wrap;
}

export function pruneVideoCache() {
  for (const id of videoCache.keys()) {
    if (!prompts.some((p) => p.id === id)) videoCache.delete(id);
  }
}

// ---- Missing-file watcher ----
// The ambient refresh is a backend-pushed "missing-files" event (see start_missing_files_poll
// in images.rs). pollMissingFiles stays for one-off "recheck now" calls (initial load, right
// after an edit).
export let missingFiles = new Set();

export function applyMissingFiles(ids) {
  missingFiles = new Set(ids);
  document.querySelectorAll(".tile, .folder-member-tile").forEach((tile) => {
    tile.classList.toggle("file-missing", missingFiles.has(tile.dataset.id));
  });
}

export async function pollMissingFiles() {
  if (!prompts.some((p) => p.file_path || p.icon_path)) {
    missingFiles.clear();
    return;
  }
  try {
    applyMissingFiles(await invoke("missing_files"));
  } catch {}
}

// ---- Pointer-based drag to any cell ----
export function cellAt(x, y) {
  return document.elementFromPoint(x, y)?.closest(".cell") || null;
}

// Track the hovered cell instead of querying all cells on every mousemove.
export let hoverCell = null;

export function setHoverCell(cell) {
  if (cell === hoverCell) return;
  hoverCell?.classList.remove("drag-over");
  hoverCell = cell;
  hoverCell?.classList.add("drag-over");
}

export function endDragVisuals() {
  document.body.classList.remove("drag-active");
  setHoverCell(null);
}

// ---- drag a prompt out into another app ----
// The webview's drag-drop handling only replaces the drop target (that is how a dropped file
// becomes a prompt); starting a drag is untouched, so a plain HTML5 dragstart hands the
// prompt to the OS. A modifier is required because every drag source already owns its plain
// drag: tiles reorder, library rows get placed on the grid, the pill moves its window.
export const dragOutMod = () => DRAG_OUT_MODS[txt("dragOutModifier")] || "ctrlKey";
export const dragOutWired = new WeakSet();

// A rotating prompt's peeked "next entry" text, prefetched on arm (see armDragOut) so
// dragOutText can stay synchronous: dataTransfer.setData only works inside dragstart, too
// late to await a peek there.
export const dragRotationPeek = new Map(); // prompt id -> peeked text

// What lands in the drop target. Placeholders stay as written: resolving them needs
// the fill-in dialog, and no dialog can open while the OS drag loop holds the pointer.
export function dragOutText(p) {
  if (p.file_path) return p.file_path;
  if ((p.variants || []).length) {
    const peeked = dragRotationPeek.get(p.id);
    if (peeked != null) return peeked;
  }
  return p.text || "";
}

// Called on pointerdown. Returns true when this press should become a drag-out; the caller
// then leaves the plain drag alone.
export function armDragOut(el, e, getPrompt) {
  // XOR: with dragOutInvert on, the plain drag is the one that leaves the app and
  // the modifier is what still moves the tile inside the grid.
  const on = optFlag("dragOut") && (e[dragOutMod()] === true) !== flag("dragOutInvert");
  el.draggable = on;
  if (on) {
    const p = getPrompt();
    // Peek is read-only, so it is safe to fire on every arm; it normally resolves well before
    // dragstart (a real drag needs motion past the threshold first).
    if (p && (p.variants || []).length) {
      invoke("rotation_peek", { id: p.id }).then((r) => { if (r) dragRotationPeek.set(p.id, r.text); }).catch(() => {});
    }
  }
  if (on && !dragOutWired.has(el)) {
    dragOutWired.add(el);
    el.addEventListener("dragstart", (ev) => startDragOut(ev, getPrompt()));
    el.addEventListener("dragend", (ev) => {
      el.draggable = false;
      const p = getPrompt();
      // Only an accepted drop of a text we actually peeked may consume an entry: a cancelled drag
      // reports "none", and a peek that never resolved fell back to p.text in dragOutText, so
      // committing would advance whatever the pointer's real position is, not the dragged entry.
      if (p && (p.variants || []).length) {
        if (ev.dataTransfer.dropEffect !== "none" && dragRotationPeek.has(p.id)) {
          invoke("rotation_commit", { id: p.id }).catch(() => {});
          recordToolUsage("rotation_advance");
        }
        dragRotationPeek.delete(p.id);
      }
    });
  }
  return on;
}

export function startDragOut(e, p) {
  const text = p ? dragOutText(p) : "";
  if (!optFlag("dragOut") || !text) { e.preventDefault(); return; }
  e.dataTransfer.effectAllowed = "copy";
  e.dataTransfer.setData("text/plain", text);
  if (p.file_path) e.dataTransfer.setData("text/uri-list", fileUri(p.file_path));
  // The OS owns the gesture from here: drop the pointer-drag bookkeeping so the ghost or hover
  // cell of a reorder is not left behind.
  assignDrag(null);
  endDragVisuals();
}

// Resume videos the drag paused (visibility pauses use the same marker).
export function resumeDragVideos(el) {
  el.querySelectorAll("video").forEach((v) => {
    if (v.dataset.resume) {
      delete v.dataset.resume;
      v.play().catch(() => {});
    }
  });
}

// Size a "Copied!" element: fit the text to its box, then scale it to the chosen
// level (large is the full fit).
export function styleCopyText(el, maxW, maxH, cap) {
  fitText(el, maxW, maxH, cap);
  applyCopySizeLevel(el, txt("copySizeLevel"));
}

// Separate from the pill and folder windows' shared copied duration (900 ms): this tile flash runs shorter.
export const TILE_COPIED_MS = 350;
// Flash + small "Copied!" bubble at the bottom of the tile.
export function showCopied(tile, label) {
  tile.classList.add("copied");
  setTimeout(() => tile.classList.remove("copied"), TILE_COPIED_MS);
  if (val("bubbleMs") <= 0) return; // border flash stays; bubble is optional
  const pop = document.createElement("div");
  pop.className = "copy-pop";
  // Match the fade animation to the (expert-tunable) bubble lifetime so it is neither cut off
  // early nor lingering after the CSS fade ends.
  pop.style.animationDuration = `${BUBBLE_MS}ms`;
  pop.textContent = label || t("copied");
  tile.appendChild(pop);
  styleCopyText(pop, tile.clientWidth * 0.8, tile.clientHeight * 0.45, 26);
  setTimeout(() => pop.remove(), BUBBLE_MS);
}

// ---- Auto-paste: a fast second activation of the same prompt pastes the just-copied text
// into the last external window instead of re-copying. Enter on a focused tile copies and
// pastes in one press. All gated by the autoPaste opt-flag.
export let lastActivate = { id: null, t: 0 };
export function autoPasteDouble(id) {
  if (!flag("autoPaste")) return false;
  const now = performance.now();
  const dbl = lastActivate.id === id && (now - lastActivate.t) <= val("dblClickMs");
  lastActivate = dbl ? { id: null, t: 0 } : { id, t: now };
  return dbl;
}
export async function autoPasteNow(el) {
  try {
    await invoke("paste_into_previous", { delayMs: Math.round(val("autoPasteDelayMs")), enter: optFlag("autoPasteEnter") });
    if (el) showCopied(el, t("pasted")); else toast(t("pasted"));
  } catch (e) {
    const m = String(e);
    if (m === "no-target") toast(t("pasteNoTarget"));
    else if (m === "focus-failed") toast(t("pasteFocusFail"));
    else errToast(e);
  }
}

// Pure layout move: the tile element is re-parented as is, so its fitted text size cannot
// change and a playing video keeps running. Falls back to a full render when the tile is not
// in the grid yet (placed from the library).
export function moveTileDom(id, col, row) {
  const tile = gridEl.querySelector(`.tile[data-id="${CSS.escape(id)}"]`);
  const target = gridEl.querySelector(`.cell[data-col="${col}"][data-row="${row}"]`);
  if (!tile || !target) return false;
  const source = tile.parentElement;
  if (source === target) return true;
  const occupant = target.firstElementChild;
  if (occupant) source.appendChild(occupant); // swap
  target.appendChild(tile);
  return true;
}

// Place a tile at [col,row]; swaps with the occupant. Renders from local
// state immediately; when the backend refuses a write the stored state is read back and redrawn,
// and the result is false.
export async function placeTile(id, col, row) {
  // A grid folder is not part of the layout: a tile dropped on its cell takes the cell and the
  // folder moves to the cell the tile leaves. A tile with no cell yet leaves nothing to swap
  // with, so that drop is refused.
  const onFolder = gridEl.querySelector(`.cell[data-col="${col}"][data-row="${row}"] > .folder-tile`);
  const folderThere = onFolder ? folderById(onFolder.dataset.folderId) : null;
  if (folderThere) {
    const from = layoutOf(activeView())[id];
    if (!from) return false;
    if (!(await saveFolder(folderThere, { cell: [from[0], from[1]] }))) return false;
  }
  const view = activeView();
  const writes = [];
  // Off by default: placing a prompt drops it from every other view it sits in first (one
  // placement). On, a prompt can sit in several views at once (same object, same data).
  if (!optFlag("multiViewPrompt")) {
    for (const other of viewsContaining(id, view.id)) {
      delete layoutOf(other)[id];
      other.members = (other.members || []).filter((m) => m !== id);
      writes.push(invoke("batch_prompts", { ids: [id], action: "removeView", color: null, favorite: null, viewId: other.id }));
    }
  }
  const layout = { ...layoutOf(view) };
  const old = layout[id];
  const occupant = Object.entries(layout).find(
    ([oid, c]) => oid !== id && c[0] === col && c[1] === row
  );
  if (occupant) {
    if (old) layout[occupant[0]] = old;
    else delete layout[occupant[0]];
  }
  layout[id] = [col, row];
  view.layouts[gridKeyOf(view)] = layout;
  // The backend seats a placed prompt as a member too (set_layout); mirror it here.
  if (!(view.members || []).includes(id)) (view.members ||= []).push(id);
  writes.push(invoke("set_layout", { layout }));
  const drawn = moveTileDom(id, col, row);
  if (!drawn) await renderGrid(true);
  const failed = (await Promise.allSettled(writes)).find((r) => r.status === "rejected");
  if (!failed) return true;
  errToast(failed.reason);
  try { assignSettings(await invoke("get_settings")); } catch (_) { /* the redraw below shows the local state */ }
  await renderGrid(true);
  return false;
}

export function wireTiles() {
  window.addEventListener("resize", () => {
    cancelAnimationFrame(fitRaf);
    fitRaf = requestAnimationFrame(() => fitAllTiles(true));
  });
  watchDpr();
  // Looping tile videos pause while the window is hidden (tray/minimised) and resume afterwards;
  // manual pauses stay paused.
  document.addEventListener("visibilitychange", () => {
    document.querySelectorAll(".tile-video").forEach((v) => {
      if (document.hidden) {
        if (!v.paused) {
          v.pause();
          v.dataset.resume = "1";
        }
      } else if (v.dataset.resume) {
        delete v.dataset.resume;
        v.play().catch(() => {});
      }
    });
  });
  window.addEventListener("pointermove", (e) => {
    if (!drag || drag.out) return;
    // No reorder while chaining (a click selects) or when reorder is switched off in the expert
    // menu, but a library-placed tile and a favorites-view reorder (intrinsic to that view) may
    // always finish.
    if (!drag.moved && (chainMode || (!flag("tileReorder") && !drag.fromLibrary && !favView))) return;
    if (!drag.moved) {
      if (Math.abs(e.clientX - drag.startX) < DRAG_THRESHOLD &&
          Math.abs(e.clientY - drag.startY) < DRAG_THRESHOLD) return;
      drag.moved = true;
      drag.el.classList.add("dragging");
      document.body.classList.add("drag-active");
      // Dragging out of the library: hide the overlay (and anything it was raised over) so the grid
      // is visible.
      if (drag.fromLibrary) {
        libraryEl.classList.add("hidden");
        dropLibraryFront(true);
      }
      // Live ghost: a clone of the tile follows the cursor.
      const r = drag.el.getBoundingClientRect();
      drag.offX = drag.startX - r.left;
      drag.offY = drag.startY - r.top;
      drag.ghost = drag.el.cloneNode(true);
      drag.ghost.classList.add("drag-ghost");
      drag.ghost.classList.remove("dragging");
      drag.ghost.style.width = `${r.width}px`;
      drag.ghost.style.height = `${r.height}px`;
      // A cloned <video> would autoplay and decode while following the cursor, which makes
      // video-tile drags stutter.
      drag.ghost.querySelectorAll("video").forEach((v) => {
        v.removeAttribute("autoplay");
        v.removeAttribute("src");
      });
      // Pause the original too for the duration of the drag.
      drag.el.querySelectorAll("video").forEach((v) => {
        if (!v.paused) {
          v.pause();
          v.dataset.resume = "1";
        }
      });
      document.body.appendChild(drag.ghost);
    }
    drag.ghost.style.transform =
      `translate(${e.clientX - drag.offX}px, ${e.clientY - drag.offY}px)`;
    setHoverCell(cellAt(e.clientX, e.clientY));
    armFolderDrop(e); // Hovering the middle of a tile offers a folder
    // An already-placed tile (not a folder, not fresh from the library, not in the free-form
    // favorites grid) can also be dropped on a view tab above.
    if (flag("dragToView") && flag("multiView") && !drag.folderId && !drag.fromLibrary && !favView) {
      updateViewDropHover(e.clientX, e.clientY, e.ctrlKey);
    } else if (drag.viewTarget) {
      clearViewDropHover();
    }
  });
  window.addEventListener("pointerup", async (e) => {
    if (!drag) return;
    const { id, moved, el, ghost, folderId, viewTarget, viewTargetTitle } = drag;
    assignDrag(null);
    ghost?.remove();
    el.classList.remove("dragging");
    resumeDragVideos(el);
    endDragVisuals();
    // A tile hovering a view tab never also reaches the grid below; restore the tab's own title
    // either way.
    if (viewTarget) {
      viewTarget.classList.remove("tab-drop");
      viewTarget.title = viewTargetTitle ?? viewTarget.title;
    }
    // What an armed centre would swallow, decided before the arm is dropped.
    const armed = folderArm && folderArm.armed ? folderArm.el : null;
    disarmFolder();

    // A folder tile: a click opens it, a drag moves it to another cell.
    if (folderId) {
      const f = folderById(folderId);
      if (!f) return;
      if (!moved) { openTileFolderPop(f, el); return; }
      const cell = cellAt(e.clientX, e.clientY);
      if (!cell) return;
      const target = [Number(cell.dataset.col), Number(cell.dataset.row)];
      // Onto an occupied cell the folder stays put: a folder never swaps a tile away (dropping
      // into it does that).
      if (cell.firstElementChild && cell.firstElementChild !== el) return;
      if (await saveFolder(f, { cell: target })) await renderGrid(true);
      return;
    }

    if (!moved) {
      if (el.classList.contains("tile")) {
        const p = prompts.find((x) => x.id === id);
        if (e.ctrlKey) { toggleGridSelect(id); return; } // Ctrl+click without moving selects
        if (chainMode) { if (p) await toggleChainPick(p); return; } // pick (asks vars), don't copy
        // A prompt that still needs input always routes through the dialog: the double-click paste
        // shortcut would otherwise fire on the 2nd click and paste stale clipboard without asking. A
        // click only copies, never pastes. Checks the entry rotation is about to copy, not entry 0.
        if (p && dialogTokens(p, await peekRotatedText(p)).length) {
          // The cooldown comes before the dialog: a copy that just happened must not ask again (a
          // cancelled dialog starts no cooldown, so it can be reopened).
          if (blockedByCooldown(id, el)) return;
          await copyTextWithVars(p, el);
          return;
        }
        if (autoPasteDouble(id)) { await autoPasteNow(el); return; } // 2nd fast click pastes
        if (blockedByCooldown(id, el)) return;
        if (p && await copyResolved(p)) {
          showCopied(el);
          recordCopy(id);
          maybeCloseOnCopy();
        }
      }
      return;
    }
    // Dropped on a view tab instead of a grid cell: move it there, or with Ctrl held copy it so
    // it stays in both views. The tab of the view already on screen is a no-op.
    if (viewTarget && flag("dragToView")) {
      const targetView = resolveViewDropTarget(viewTarget);
      if (targetView && targetView.id !== activeView().id) {
        const ids = gridSelected.has(id) && gridSelected.size > 1 ? [...gridSelected] : [id];
        await dropTilesOnView(ids, targetView.id, activeView().id, e.ctrlKey);
      }
      return;
    }
    const cell = cellAt(e.clientX, e.clientY);
    if (!cell) return;
    if (favView) { await placeFav(id, Number(cell.dataset.col), Number(cell.dataset.row)); return; }
    if (armed) { await dropIntoFolder(id, armed, cell); return; }
    // Only a genuine reorder of an already-placed tile gets an undo offer; a new placement has
    // nothing to move back from, so prevCell stays undefined and the toast is skipped.
    const prevCell = layoutOf(activeView())[id];
    const destCol = Number(cell.dataset.col);
    const destRow = Number(cell.dataset.row);
    const placed = await placeTile(id, destCol, destRow);
    if (placed && prevCell && (prevCell[0] !== destCol || prevCell[1] !== destRow)) {
      const [fromCol, fromRow] = prevCell;
      // The undo belongs to the view the tile was moved in: placeTile reads the view on screen, so
      // after a tab switch it would pull the tile out of its own view.
      const movedIn = activeView().id;
      undoToast(t("tileMoved"), () => {
        if (favView || activeView().id !== movedIn) { toast(t("undoNotHere")); return; }
        return placeTile(id, fromCol, fromRow);
      });
    }
  });
  window.addEventListener("pointercancel", () => {
    if (drag) {
      drag.el.classList.remove("dragging");
      drag.ghost?.remove();
      resumeDragVideos(drag.el);
      clearViewDropHover();
    }
    assignDrag(null);
    disarmFolder();
    if (memberDrag) {
      memberDrag.el.classList.remove("dragging");
      memberDrag.ghost?.remove();
    }
    assignMemberDrag(null);
    endMemberDragLayers();
    endDragVisuals();
  });
  // Double-click an empty cell to create a prompt right there. `.cell` has no children while
  // nothing occupies it (renderGrid appends a tile or folder tile only to a filled one).
  gridEl.addEventListener("dblclick", (e) => {
    if (favView) return; // free-form favorites layout has its own placement rules
    const cell = e.target.closest(".cell");
    if (!cell || cell.firstElementChild) return;
    openModal({
      mode: "create",
      title: t("nameModalTitle"),
      targetCell: [Number(cell.dataset.col), Number(cell.dataset.row)],
    });
  });
  // Right-click on an empty spot of the grid offers the grid size. Tiles and folder
  // tiles stop their own contextmenu event; the favorites grid sizes itself.
  gridEl.addEventListener("contextmenu", (e) => {
    if (favView || e.target.closest(".tile")) return;
    e.preventDefault();
    openMiniMenu([{ label: t("gridSizeMenu"), run: () => openGridSizeFor(activeView()) }], e.clientX, e.clientY);
  });
}
