// The prompt library: search, filters, batch selection, list and tile rendering.
import { $, assignDrag, assignPrompts, assignSettings, cellKey, convertFileSrc, debounced, DRAG_THRESHOLD, errToast, favView, flag, GRID_PLUS, gridEl, gridSelected, ICON_EDIT, ICON_TRASH, IMG_WAIT, invoke, LANG, libraryEl, libraryTab, libThumbCache, makeKeyboardActivatable, openColorPop, optFlag, PDF_EXT, prompts, putIcon, settings, settingsEl, t, toast, txt, undoToast, val } from "./core.js";
import { mediaKind } from "./media.js";
import { armDragOut, autoPasteDouble, autoPasteNow, fitAllTiles, hasImage, imageOf, notesOn, placeTile } from "./tiles.js";
import { openLightbox, wireSearchSuggest } from "./windowChrome.js";
import { fillVideoFormatSelect, fmtBytes, varPreview } from "./floatShared.js";
import { anyEntryHasVars, dialogTokens, extractVars, parseVar, peekRotatedText, resolveVars, resolveVarsList, varTokens } from "./promptVars.js";
import { confirmDialog } from "./dialog.js";
import { activeView, firstFree, layoutOf, renderGrid, renderViews } from "./views.js";
import { copyOnCooldown, peekResolveCommit, recordCopy } from "./copyFlow.js";
import { attachSelectPicker } from "./selectpop.js";
import { allTags, renderTagFilter, tagColor, tagKey, tagsOn } from "./tags.js";
import { refreshClipInbox, setLibraryTab, tileTimeLines } from "./journal.js";
import { closeStats, openCompareView, recordToolUsage, renderStatsPanel } from "./management.js";
import { closeBackups } from "./backups.js";
import { buildSwatches, editPrompt, ensureSecretVaultUnlocked } from "./editor.js";
import { foldersOfView } from "./gridFolders.js";
import { openCtx } from "./gridMenus.js";

export let libQuery = ""; // prompt library search text
export function assignLibQuery(v) { libQuery = v; }
// A search to open the library with: its open handler resets the query after its own
// awaits, so a query set right after the click would be wiped.
let libPresetQuery = null;
export function presetLibQuery(q) { libPresetQuery = q; }
// Ids whose recognized image text matched the last query sent to search_image_text, and the
// (trimmed, lowercased) query they were fetched for. image_text never reaches the frontend,
// so this is the only trace of a hit computeLibSearchScore sees.
export let imageHitIds = new Set();
export let imageHitQuery = "";
// Type filters are ADDITIVE and identical in both windows: an empty set means every
// type, each chip toggles its own, and the pick is stored per window.
export const libTypes = new Set();
export let libFav = false; // prompt library "favorites only" filter
export let libShowArchive = false; // show archived prompts even while browsing (not persisted)
export let libViewMode = "list"; // "list" or "tiles" -- kept in sync with the type filter by syncLibViewMode()
export const libTags = new Set(); // Selected tag filters, as lowercase tag keys
// Batch operations state.
export let libSelectMode = false;
export let libSelected = new Set();
export let libLastId = null;          // anchor for shift-click range select
export let libDisplayOrder = [];      // ids in current display order (for range select)
export let gridDirty = false;         // batch ops changed prompts; rebuild grid on library close

// ---- Prompt library (all prompts, click to edit) ----
// Classify a prompt for the library type filter and thumbnails.
export function promptType(p) {
  if (PDF_EXT.test(p.file_path)) return "pdf";
  const kind = p.file_path ? mediaKind(p.file_path) : p.icon_path ? mediaKind(p.icon_path) : "";
  if (kind === "video") return "video";
  if (kind === "image" || kind === "gif") return "image";
  if (p.copy_image && hasImage(p)) return "image";
  if (p.file_path) return "file";
  return "text";
}

// Library display name: falls back to a type label so saved images, screenshots and files
// are never shown blank.
export const LIB_TYPE_LABEL = { image: "filterImage", video: "filterVideo", pdf: "filterPdf", file: "filterFile" };
export function libLabel(p) {
  if (p.name) return p.name;
  const key = LIB_TYPE_LABEL[promptType(p)];
  return key ? t(key) : p.text;
}

// Best preview source for a prompt's library thumbnail; "" = no image.
export function thumbSrc(p) {
  const stored = imageOf(p);
  if (stored) return stored;
  // A stored image outranks any path preview: while it loads, show nothing rather than briefly
  // swapping in the attachment's own thumbnail.
  if (hasImage(p)) return "";
  const path = mediaKind(p.file_path) === "image" || mediaKind(p.file_path) === "gif"
    ? p.file_path
    : mediaKind(p.icon_path) === "image" || mediaKind(p.icon_path) === "gif"
      ? p.icon_path
      : "";
  return path ? convertFileSrc(path) : "";
}

export function videoSrc(p) {
  if (mediaKind(p.file_path) === "video") return p.file_path;
  if (mediaKind(p.icon_path) === "video") return p.icon_path;
  return "";
}

// Only videos scrolled into view play, which keeps a long list cheap. Library and journal
// each get their own observer so one panel never pauses the other's mounted video thumbnails.
export let _libObs = null;
export let _journalObs = null;
export function assignJournalObs(v) { _journalObs = v; }
export let _activeVidObs = null; // observer the current render attaches new videos to
export function assignActiveVidObs(v) { _activeVidObs = v; }
export function makeVideoObserver() {
  return new IntersectionObserver((entries) => {
    for (const e of entries) {
      if (e.isIntersecting) e.target.play().catch(() => {});
      else e.target.pause();
    }
  }, { threshold: 0.1 });
}

export function thumbDot(p) {
  const dot = document.createElement("span");
  dot.className = "dot";
  if (p.color) dot.style.setProperty("--dot-color", p.color);
  return dot;
}

// Shared row preview for library and journal: a looping muted inline video for video
// prompts, an image for image/gif/stored-image prompts, else a color dot. `cache` reuses the
// element across re-renders of the same panel so the same base64 blob is not re-decoded on
// every keystroke.
export function thumbEl(p, cache) {
  if (!flag("imagePreview")) return thumbDot(p);
  const vsrc = flag("libraryVideoPreview") ? videoSrc(p) : "";
  const src = vsrc ? "" : thumbSrc(p);
  const key = vsrc ? `v:${vsrc}` : src ? `i:${src}` : "";
  if (!key) {
    const dot = thumbDot(p);
    // A stored image still on its way: the dot stands in and swaps itself out.
    if (!vsrc && hasImage(p)) dot.dataset[IMG_WAIT] = p.id;
    return dot;
  }
  const cached = cache.get(p.id);
  if (cached && cached.key === key) {
    // Each render builds a fresh IntersectionObserver (disconnecting the old one), so a cache hit
    // must re-observe with the current one or a surviving video stops following scroll.
    if (vsrc && _activeVidObs) _activeVidObs.observe(cached.el);
    return cached.el;
  }
  let el;
  if (vsrc) {
    el = document.createElement("video");
    el.className = "lib-thumb";
    el.muted = true;
    el.loop = true;
    el.playsInline = true;
    el.preload = "metadata";
    el.draggable = false;
    el.src = convertFileSrc(vsrc);
    if (_activeVidObs) _activeVidObs.observe(el);
    el.addEventListener("click", (e) => { e.stopPropagation(); openLightbox(el.src, true); });
  } else {
    el = document.createElement("img");
    el.className = "lib-thumb";
    el.src = src;
    el.draggable = false;
    el.addEventListener("click", (e) => { e.stopPropagation(); openLightbox(el.src, false); });
  }
  cache.set(p.id, { key, el });
  return el;
}

// ---- Media metadata: duration badge + resolution/size tooltip ----
// Library and copy history only; buildTile (the main grid) never wraps its thumbnail with
// this and never reads the media_* fields.

export const mediaDurationLabel = (ms) => {
  const s = Math.round(ms / 1000);
  const m = Math.floor(s / 60);
  return `${m}:${String(s % 60).padStart(2, "0")}`;
};

// Wraps a thumbnail with its duration badge when the entry carries a measured video length,
// otherwise returns it unchanged, so a caller can always write
// `row.append(mediaThumb(p, thumbEl(p, cache)), body)`.
export function mediaThumb(p, thumb) {
  if (!flag("mediaMeta") || !p.media_duration_ms) return thumb;
  const wrap = document.createElement("span");
  wrap.className = "lib-thumb-wrap";
  const badge = document.createElement("span");
  badge.className = "lib-thumb-duration";
  badge.textContent = mediaDurationLabel(p.media_duration_ms);
  wrap.append(thumb, badge);
  return wrap;
}

// Resolution and file size line for the tooltip. "" = nothing to add: a value that could not
// be measured shows nothing, never a placeholder.
export function mediaTipLine(p) {
  if (!flag("mediaMeta")) return "";
  const parts = [];
  if (p.media_width && p.media_height) parts.push(`${p.media_width}×${p.media_height}`);
  if (p.media_size_bytes) parts.push(fmtBytes(p.media_size_bytes, LANG));
  return parts.join(" · ");
}

// The one-time catch-up for a row never measured, or whose first measurement failed: at most
// once per id per session, fire-and-forget, and the panel re-renders only once it comes back
// with something to show.
export const mediaMetaBackfilled = new Set();
// One pending repaint per render function: a library full of unmeasured rows answers N
// times, and each answer would otherwise rebuild the whole list.
const backfillRepaint = new Map();
function scheduleBackfillRepaint(rerender) {
  clearTimeout(backfillRepaint.get(rerender));
  backfillRepaint.set(rerender, setTimeout(() => {
    backfillRepaint.delete(rerender);
    // A closed panel repaints on its next open anyway.
    if (!libraryEl.classList.contains("hidden")) rerender();
  }, 120));
}
export function maybeBackfillMediaMeta(p, rerender) {
  if (!flag("mediaMetaBackfill") || mediaMetaBackfilled.has(p.id)) return;
  if (p.media_duration_ms || p.media_width || p.media_size_bytes) return; // already measured
  if (!p.file_path && !p.icon_path) return; // nothing to measure
  mediaMetaBackfilled.add(p.id);
  invoke("backfill_media_meta", { id: p.id })
    .then((updated) => {
      if (!updated) return;
      const idx = prompts.findIndex((x) => x.id === p.id);
      if (idx === -1) return;
      prompts[idx] = { ...prompts[idx], ...updated };
      scheduleBackfillRepaint(rerender);
    })
    .catch(() => {}); // best-effort -- a failed catch-up just tries again next session
}

// ---- Additive type filters, shared by the library and the copy history ----
// Nothing picked = every type. "All" clears the set, every other chip toggles itself, so PDF
// and File can be shown together. The pick is stored per window.
export const typeFilterOf = (key) => new Set((settings.ui_texts?.[key] || "").split(",").filter(Boolean));

export function paintTypeChips(box, set) {
  for (const btn of box.querySelectorAll(".lib-type[data-type]")) {
    const type = btn.dataset.type;
    btn.classList.toggle("active", type === "all" ? set.size === 0 : set.has(type));
  }
}

export function toggleTypeFilter(set, type, box, key) {
  if (type === "all") set.clear();
  else if (!set.delete(type)) set.add(type);
  paintTypeChips(box, set);
  const value = [...set].join(",");
  settings.ui_texts = { ...(settings.ui_texts || {}), [key]: value };
  invoke("set_ui_text", { key, value }).catch(() => {});
}

// A prompt passes when no type is picked, or when its own type is among them.
export const typeAllowed = (set, p) => set.size === 0 || set.has(promptType(p));

// ---- Fuzzy search: tiered scoring (exact > prefix > fuzzy). Fuzzy only kicks in when no
// substring hit exists, so the common case stays cheap on 1000+ prompts. ----
export function fuzzyMaxTyposFor(term) {
  return term.length <= 5 ? 1 : 2;
}
// Bounded Levenshtein: returns max+1 as soon as every cell exceeds max (early out).
export function editDistWithin(a, b, max) {
  const la = a.length, lb = b.length;
  if (Math.abs(la - lb) > max) return max + 1;
  let prev = Array.from({ length: lb + 1 }, (_, j) => j);
  for (let i = 1; i <= la; i++) {
    const cur = new Array(lb + 1);
    cur[0] = i;
    let rowMin = i;
    const ca = a.charCodeAt(i - 1);
    for (let j = 1; j <= lb; j++) {
      const cost = ca === b.charCodeAt(j - 1) ? 0 : 1;
      cur[j] = Math.min(prev[j] + 1, cur[j - 1] + 1, prev[j - 1] + cost);
      if (cur[j] < rowMin) rowMin = cur[j];
    }
    if (rowMin > max) return max + 1;
    prev = cur;
  }
  return prev[lb];
}
// Weighted match score for the active query. score 0 = no match; fuzzy = true when only the
// typo-tolerant tier matched (rendered dimmed).
// Per-render memo: libSearchScore runs for every prompt in the filter, twice per comparison
// in the sort and once more per row, and fuzzy mode runs bounded edit distance over every
// token. Cleared at the top of each renderLibrary.
export let libScoreCache = new Map();
export function libSearchScore(p) {
  const hit = libScoreCache.get(p.id);
  if (hit) return hit;
  const res = computeLibSearchScore(p);
  libScoreCache.set(p.id, res);
  return res;
}
// Whether a prompt satisfies one tag:/var: condition from the search query. tag: matches a
// tag's identity (tagKey) case-insensitively, not as a substring (the plain search already
// does that). var: matches a placeholder's name, read as the variable dialog reads it (VAR_RE
// over the body, then the "|options" split).
export function searchConditionMet(c, p) {
  let hit;
  if (c.prefix === "tag") {
    hit = (p.tags || []).some((name) => tagKey(name) === tagKey(c.value));
  } else {
    const names = [p.text, ...(p.variants || [])].flatMap((t) => extractVars(t || ""));
    hit = names.some((tok) => parseVar(tok).name.toLowerCase() === c.value);
  }
  return c.neg ? !hit : hit;
}
// `imageHits` ({ query, ids }) lets another search surface bring its own recognized-text hits
// instead of the library's; left out, the library's own lookup is used.
export function computeLibSearchScore(p, imageHits = null) {
  const rawInput = libQuery.trim().toLowerCase();
  if (!rawInput) return { score: 1, fuzzy: false, entryIndex: 0 };
  // tag:word and var:word (and their negated "-tag:"/"-var:" forms) are hard conditions pulled
  // out first: a prompt that fails one is out whatever the rest of the query would score.
  const { rest, conditions } = parseSearchQuery(rawInput);
  if (conditions.length && !conditions.every((c) => searchConditionMet(c, p))) {
    return { score: 0, fuzzy: false, entryIndex: 0 };
  }
  const raw = rest;
  if (!raw) return { score: 1, fuzzy: false, entryIndex: 0 }; // only tag:/var: terms given
  // entryIndex says which entry a body-field match came from (0 = p.text, 1+ =
  // variants[entryIndex-1]), so a rotating prompt is findable by any entry and the row can show
  // the one that matched. The fourth element names the field, so a row can say where its hit
  // came from.
  const fields = [[(p.name || "").toLowerCase(), 3, -1, "name"], [(p.text || "").toLowerCase(), 1, 0, "text"], [(p.file_path || "").toLowerCase(), 1, -1, "file"]];
  (p.variants || []).forEach((v, i) => fields.push([(v || "").toLowerCase(), 1, i + 1, "variant"]));
  // A tag is a deliberate label, so it ranks between the name and the body.
  if (tagsOn() && (p.tags || []).length) fields.push([p.tags.join(" ").toLowerCase(), 2, -1, "tags"]);
  // A note is private context, so it matches but ranks below the prompt body.
  if (notesOn() && p.notes) fields.push([p.notes.toLowerCase(), 0.5, -1, "notes"]);
  // A "-word" term excludes any prompt whose searchable text contains it, checked across every
  // field first.
  const terms = raw.split(/\s+/).filter(Boolean);
  const isExclude = (t) => t.length > 1 && t[0] === "-";
  const excludes = terms.filter(isExclude).map((t) => t.slice(1));
  if (excludes.some((ex) => fields.some(([val]) => val.includes(ex)))) return { score: 0, fuzzy: false, entryIndex: 0 };
  const q = terms.filter((t) => !isExclude(t)).join(" ");
  if (!q) return { score: 1, fuzzy: false, entryIndex: 0 }; // exclusion-only query matches everything not excluded
  let best = 0, bestEntry = 0, bestKind = "";
  for (const [val, w, entry, kind] of fields) {
    if (val && val.includes(q)) {
      const s = (val.startsWith(q) ? 120 : 100) * w;
      if (s > best) { best = s; bestEntry = entry >= 0 ? entry : 0; bestKind = kind; }
    }
  }
  if (best) return { score: best, fuzzy: false, entryIndex: bestEntry, field: bestKind, q };
  if (!flag("fuzzySearch")) return { score: 0, fuzzy: false, entryIndex: 0 };
  // Tokenize on any non-alphanumeric so "code," / "code." / code blocks split into
  // clean words (scripts without spaces, e.g. CJK, stay one token: substring wins).
  const qWords = fuzzyTokens(q);
  if (!qWords.length) return { score: 0, fuzzy: false, entryIndex: 0 };
  for (const [val, w, entry, kind] of fields) {
    if (!val) continue;
    const words = fuzzyTokens(val);
    let matched = 0;
    for (const qw of qWords) {
      const max = fuzzyMaxTyposFor(qw);
      if (words.some((vw) => fuzzyWordHit(qw, vw, max))) matched++;
    }
    if (matched === qWords.length) {
      const s = 40 * w;
      if (s > best) { best = s; bestEntry = entry >= 0 ? entry : 0; bestKind = kind; }
    }
  }
  if (best) return { score: best, fuzzy: true, entryIndex: bestEntry, field: bestKind, q };
  // Last resort: text recognized inside an attached picture. Only reached when nothing else
  // matched, so it never outranks a match on the name, body, a tag or a note.
  const hits = imageHits || { query: imageHitQuery, ids: imageHitIds };
  if (optFlag("ocrIndex") && hits.query === rawInput && hits.ids.has(p.id)) {
    return { score: 90, fuzzy: false, entryIndex: 0, fromImage: true };
  }
  return { score: 0, fuzzy: false, entryIndex: 0 };
}
// Fills `el` with `str`, every case-insensitive occurrence of `q` wrapped in a mark.
// Text nodes only, never markup, so a name or preview can not inject anything.
function markHits(el, str, q) {
  el.textContent = "";
  // Folded per character and only where the length stays: toLowerCase() on the whole
  // string turns U+0130 into two code units and would shift every later index.
  const low = Array.from(str, (ch) => { const l = ch.toLowerCase(); return l.length === ch.length ? l : ch; }).join("");
  let at = 0;
  for (let i = low.indexOf(q); q && i >= 0; i = low.indexOf(q, at)) {
    if (i > at) el.appendChild(document.createTextNode(str.slice(at, i)));
    const m = document.createElement("mark");
    m.className = "lib-hit";
    m.textContent = str.slice(i, i + q.length);
    el.appendChild(m);
    at = i + q.length;
  }
  if (at < str.length) el.appendChild(document.createTextNode(str.slice(at)));
}
// The only place image_text is read back, via a backend lookup (the state payload never
// carries it). Fired once per query change, never inside computeLibSearchScore (which runs
// several times per prompt per render), and cached until the query changes. A stubbed or
// failing call just means no image hits.
export async function refreshImageHits(rawInput) {
  if (!optFlag("ocrIndex") || !rawInput) {
    imageHitIds = new Set();
    imageHitQuery = rawInput;
    return;
  }
  let ids = [];
  try { ids = (await invoke("search_image_text", { query: rawInput })) || []; } catch { ids = []; }
  // A slower reply for an older query must not draw its hit set over the newer one.
  if (rawInput !== libQuery.trim().toLowerCase()) return;
  imageHitIds = new Set(ids);
  imageHitQuery = rawInput;
  libScoreCache.clear();
  renderLibrary();
}
// Split into alphanumeric tokens (Unicode-aware).
export function fuzzyTokens(s) {
  return s.split(/[^\p{L}\p{N}]+/u).filter(Boolean);
}
// A query word matches a field word by substring, whole-word edit distance, or, for longer
// field words, edit distance against its leading prefix ("cde" matches "code" and
// "codebase").
export function fuzzyWordHit(qw, vw, max) {
  if (vw.includes(qw)) return true;
  if (editDistWithin(qw, vw, max) <= max) return true;
  if (vw.length > qw.length + max) return editDistWithin(qw, vw.slice(0, qw.length + max), max) <= max;
  return false;
}

// The archive is a reserved tag, not a new prompt field, so every tag-aware path (search,
// export, backup, sync) carries it without knowing about it.
export function archiveTagName() {
  return txt("archiveTag") || t("archiveTagName");
}
// The resolved tag text is only persisted on first real use, so a fresh install
// never writes a setting it never needed.
export let archiveTagEnsuring = null;
export async function ensureArchiveTagName() {
  const name = archiveTagName();
  if (txt("archiveTag")) return name;
  if (!archiveTagEnsuring) {
    settings.ui_texts = { ...(settings.ui_texts || {}), archiveTag: name };
    archiveTagEnsuring = invoke("set_ui_text", { key: "archiveTag", value: name }).catch((err) => errToast(err));
  }
  await archiveTagEnsuring;
  return name;
}
export function isArchived(p) {
  const name = txt("archiveTag");
  if (!name) return false;
  const key = tagKey(name);
  return (p.tags || []).some((t2) => tagKey(t2) === key);
}

// Browsing without a query leaves an archived prompt out of the list; the show-archive toggle
// or a typed query brings it back. The header counts follow the same rule.
function archiveHidden() {
  return flag("archiveTag") && !libQuery.trim() && !libShowArchive;
}

export function libMatches(p) {
  if (libFav && !p.favorite) return false;
  if (!typeAllowed(libTypes, p)) return false;
  if (!tagMatches(p)) return false;
  // An archived prompt is hidden unless the show-archive toggle is on, but a typed query still
  // finds it.
  if (archiveHidden() && isArchived(p)) return false;
  return libSearchScore(p).score > 0;
}

// Saved searches: query text, active tag filters and sort, kept as one JSON blob in a single
// settings slot (the generic set_ui_text command persists any string).
const SAVED_SEARCH_MAX = 5; // saved searches kept
export function loadSavedSearches() {
  try {
    const list = JSON.parse(txt("savedSearches") || "[]");
    return Array.isArray(list) ? list : [];
  } catch {
    return [];
  }
}
export async function persistSavedSearches(list) {
  const value = JSON.stringify(list);
  settings.ui_texts = { ...(settings.ui_texts || {}), savedSearches: value };
  await invoke("set_ui_text", { key: "savedSearches", value }).catch((err) => errToast(err));
}
// A long query would blow up the chip row, so the suggested name is a plain truncation,
// editable in the same dialog before saving.
export function suggestedSearchName(query) {
  const s = query.trim();
  return s.length > 40 ? s.slice(0, 40).trim() + "…" : s;
}
// Reordering the saved-search chips uses pointer events, since the webview reserves HTML5
// drag-and-drop for file drops (same as the view tabs).
let savedChipDrag = null;
let savedChipDragBlockClick = false;
function endSavedChipDrag() {
  const d = savedChipDrag;
  savedChipDrag = null;
  if (!d) return null;
  d.el.classList.remove("tab-dragging");
  d.target?.classList.remove("drag-over");
  return d;
}
window.addEventListener("pointermove", (e) => {
  const d = savedChipDrag;
  if (!d) return;
  if (!d.moved) {
    if (Math.abs(e.clientX - d.startX) < DRAG_THRESHOLD && Math.abs(e.clientY - d.startY) < DRAG_THRESHOLD) return;
    d.moved = true;
    d.el.classList.add("tab-dragging");
  }
  const hit = document.elementFromPoint(e.clientX, e.clientY)?.closest(".lib-saved-search-chip");
  const next = hit && hit !== d.el ? hit : null;
  if (next === d.target) return;
  d.target?.classList.remove("drag-over");
  d.target = next;
  next?.classList.add("drag-over");
});
window.addEventListener("pointercancel", () => { endSavedChipDrag(); });
window.addEventListener("pointerup", async () => {
  const d = endSavedChipDrag();
  if (!d || !d.moved) return;
  savedChipDragBlockClick = true; // the click that follows the release must not apply the search
  setTimeout(() => { savedChipDragBlockClick = false; }, 0);
  const to = d.target ? Number(d.target.dataset.idx) : NaN;
  if (!Number.isFinite(to) || to === d.from) return;
  const cur = loadSavedSearches();
  const [moved] = cur.splice(d.from, 1);
  if (!moved) return;
  cur.splice(to, 0, moved);
  await persistSavedSearches(cur);
  renderSavedSearches();
});
export function renderSavedSearches() {
  const bar = $("library-saved-searches");
  if (!bar) return;
  bar.innerHTML = "";
  if (!flag("savedSearches")) return;
  const list = loadSavedSearches();
  list.forEach((entry, i) => {
    const chip = document.createElement("button");
    chip.type = "button";
    chip.className = "tag-chip lib-saved-search-chip";
    chip.title = entry.query || entry.name;
    const label = document.createElement("span");
    label.className = "tag-chip-label";
    label.textContent = entry.name;
    chip.appendChild(label);
    chip.dataset.idx = String(i);
    chip.addEventListener("click", () => {
      if (savedChipDragBlockClick) { savedChipDragBlockClick = false; return; } // the click that ends a drag
      applySavedSearch(entry);
    });
    chip.addEventListener("contextmenu", (e) => {
      e.preventDefault();
      e.stopPropagation();
      editSavedSearch(i);
    });
    chip.addEventListener("pointerdown", (e) => {
      if (e.button !== 0) return;
      savedChipDrag = { from: i, el: chip, startX: e.clientX, startY: e.clientY, moved: false, target: null };
    });
    bar.appendChild(chip);
  });
}
export function applySavedSearch(entry) {
  libQuery = entry.query || "";
  $("library-q").value = libQuery;
  libTags.clear();
  for (const k of entry.tags || []) libTags.add(k);
  if (entry.sort && $("library-sort")) {
    $("library-sort").value = entry.sort;
    settings.ui_texts = { ...(settings.ui_texts || {}), librarySort: entry.sort };
    invoke("set_ui_text", { key: "librarySort", value: entry.sort }).catch((err) => errToast(err));
  }
  renderTagFilter();
  renderLibrary();
}
export async function saveSavedSearch() {
  const list = loadSavedSearches();
  const max = SAVED_SEARCH_MAX;
  if (list.length >= max) {
    const oldest = list[0];
    toast(t("savedSearchLimitToast").replace("{max}", String(max)).replace("{name}", () => oldest ? oldest.name : ""));
    return;
  }
  const suggested = suggestedSearchName(libQuery);
  const name = await confirmDialog({
    title: t("savedSearchSaveTitle"),
    confirmLabel: t("save"),
    input: { type: "text", value: suggested, placeholder: t("savedSearchNamePh") },
    danger: false,
  });
  if (!name) return;
  const entry = { name: name.trim() || suggested, query: libQuery, tags: [...libTags], sort: txt("librarySort") || "" };
  list.push(entry);
  await persistSavedSearches(list);
  renderSavedSearches();
}
export async function editSavedSearch(index) {
  const list = loadSavedSearches();
  const entry = list[index];
  if (!entry) return;
  const answer = await confirmDialog({
    title: t("savedSearchEditTitle"),
    message: t("savedSearchEditMsg"),
    confirmLabel: t("save"),
    altLabel: t("delete"),
    input: { type: "text", value: entry.name },
    danger: false,
  });
  if (answer === null || answer === false) return;
  if (answer === "alt") {
    list.splice(index, 1);
  } else {
    const name = answer.trim();
    if (name) entry.name = name;
  }
  await persistSavedSearches(list);
  renderSavedSearches();
}

// Selected tag chips narrow the list, combined with search and the other filters. AND
// (default) needs every chip, OR any of them. The grid's own filter (gridTags) reuses this
// with its own Set.
export function tagMatches(p, tagSet = libTags) {
  if (!tagsOn() || !tagSet.size) return true;
  const own = new Set((p.tags || []).map(tagKey));
  const wanted = [...tagSet];
  return (txt("tagFilterMode") || "and") === "or"
    ? wanted.some((k) => own.has(k))
    : wanted.every((k) => own.has(k));
}

// "Last edited" per prompt, from prompt_versions (a version row is only written on a real
// name/text change, see update_prompt), refetched when the library opens or an edit lands
// while it is open. A prompt absent from the map has no version row and sorts to the end,
// like an unused prompt under "most used".
export let lastEdited = {};
export async function refreshLastEdited() {
  if (!flag("promptHistory")) { lastEdited = {}; return; }
  try { lastEdited = await invoke("last_edited_map"); } catch { lastEdited = {}; }
}
// Smart sort: order the library list. The usage-based options gate on smartSort.
export function libSortMode() {
  const raw = settings.ui_texts?.librarySort || "default";
  if ((raw === "mostUsed" || raw === "recentUsed") && !flag("smartSort")) return "default";
  if (raw === "lastEdited" && !flag("promptHistory")) return "default";
  return raw;
}
export function sortLibraryItems(items) {
  const mode = libSortMode();
  const q = libQuery.trim();
  const nameOf = (p) => libLabel(p).toLowerCase();
  // One Collator for the whole sort: localeCompare(str, locale) builds a fresh one on every
  // call.
  const collator = new Intl.Collator(LANG);
  const byName = (a, b) => collator.compare(nameOf(a), nameOf(b));
  if (mode === "default") {
    // Default + active query = relevance order (exact/prefix above fuzzy).
    if (q) items.sort((a, b) => (libSearchScore(b).score - libSearchScore(a).score) || byName(a, b));
    return;
  }
  if (mode === "name") { items.sort(byName); return; }
  // Usage/recency descending; unused (0) fall last, ties resolve by name.
  if (mode === "mostUsed") {
    items.sort((a, b) => ((settings.usage?.[b.id] || 0) - (settings.usage?.[a.id] || 0)) || byName(a, b));
  } else if (mode === "recentUsed") {
    items.sort((a, b) => ((settings.last_used?.[b.id] || 0) - (settings.last_used?.[a.id] || 0)) || byName(a, b));
  } else if (mode === "lastEdited") {
    items.sort((a, b) => ((lastEdited[b.id] || 0) - (lastEdited[a.id] || 0)) || byName(a, b));
  }
}
export function fillLibrarySort() {
  const sel = $("library-sort");
  if (!sel) return;
  const opts = [["default", "sortDefault"], ["name", "sortName"]];
  if (flag("smartSort")) opts.push(["mostUsed", "sortMostUsed"], ["recentUsed", "sortRecentUsed"]);
  if (flag("promptHistory")) opts.push(["lastEdited", "sortLastEdited"]);
  sel.innerHTML = "";
  for (const [v, k] of opts) {
    const o = document.createElement("option");
    o.value = v; o.textContent = t(k);
    sel.appendChild(o);
  }
  sel.value = libSortMode();
}

// The library was raised over the statistics dialog by jumpToPrompt. Put it back down;
// `closeBehind` also dismisses the dialogs underneath (dragging onto the grid needs the
// grid). Returns true if the library was raised at all.
export function dropLibraryFront(closeBehind) {
  if (!document.body.classList.contains("library-front")) return false;
  document.body.classList.remove("library-front");
  if (closeBehind) {
    closeBackups();
    closeStats();
    settingsEl.classList.add("hidden");
  }
  return true;
}
// Close the library and flush any deferred grid rebuild from batch edits.
export function hideLibrary() {
  libraryEl.classList.add("hidden");
  // Prompts may have been edited or deleted while the library was up; the statistics
  // underneath would show the old numbers.
  if (dropLibraryFront(false)) renderStatsPanel($("stats-panel"));
  if (gridDirty) { gridDirty = false; renderGrid(true); }
}

// List/tiles toggle, remembered per active type filter: picking tiles under "only images"
// sticks for that filter without forcing tiles everywhere.
export function libViewModeKey() {
  return [...libTypes].sort().join(",") || "all";
}
export function loadLibViewModes() {
  try {
    const v = JSON.parse(txt("libraryViewMode") || "{}");
    return v && typeof v === "object" && !Array.isArray(v) ? v : {};
  } catch {
    return {};
  }
}
export function syncLibViewMode() {
  libViewMode = flag("libraryTiles") ? (loadLibViewModes()[libViewModeKey()] || "list") : "list";
}
export async function toggleLibViewMode() {
  libViewMode = libViewMode === "tiles" ? "list" : "tiles";
  const modes = loadLibViewModes();
  modes[libViewModeKey()] = libViewMode;
  const value = JSON.stringify(modes);
  settings.ui_texts = { ...(settings.ui_texts || {}), libraryViewMode: value };
  await invoke("set_ui_text", { key: "libraryViewMode", value }).catch((err) => errToast(err));
  renderLibrary();
}
// Tiles build their real thumbnail only once scrolled into view; otherwise a mostly-picture
// library would fetch every image on open (the cost imageOf's cache exists to avoid).
export let _tileImgObs = null;
export function makeTileImgObserver(root) {
  return new IntersectionObserver((entries) => {
    for (const entry of entries) {
      if (!entry.isIntersecting) continue;
      const row = entry.target;
      _tileImgObs.unobserve(row);
      const placeholder = row.querySelector(".lib-tile-placeholder");
      if (!placeholder) continue;
      const p = prompts.find((x) => x.id === row.dataset.id);
      if (p) placeholder.replaceWith(mediaThumb(p, thumbEl(p, libThumbCache)));
    }
  }, { root, rootMargin: "300px" });
}

// How hard the filters are biting, in the header where the filters are.
function updateLibraryCount(shown) {
  const count = $("library-count");
  if (!count) return;
  // A prompt the archive toggle hides counts neither as shown nor in the total.
  const all = archiveHidden() ? prompts.filter((p) => !isArchived(p)).length : prompts.length;
  count.textContent = shown === all
    ? String(all)
    : t("libraryCount").replace("{n}", () => shown).replace("{v}", () => all);
  const tabCount = $("library-tab-prompts-count");
  if (tabCount) tabCount.textContent = `(${count.textContent})`;
}

export function renderLibrary() {
  // The archive chip names the tag it filters by, so a renamed archive tag shows where it is used.
  const archiveChip = $("library-archive");
  $("library-archive-label").textContent = archiveTagName();
  const archiveTip = t("showArchiveTagged").replace("{name}", () => archiveTagName());
  archiveChip.title = archiveTip;
  archiveChip.setAttribute("aria-label", archiveTip);
  const list = $("library-list");
  list.innerHTML = "";
  libScoreCache.clear(); // fresh scores each render (query / prompt content may have changed)
  fillLibrarySort();
  syncLibraryCols();
  if (_libObs) _libObs.disconnect();
  _libObs = makeVideoObserver();
  _activeVidObs = _libObs;
  if (_tileImgObs) _tileImgObs.disconnect();
  const tilesMode = flag("libraryTiles") && libViewMode === "tiles";
  _tileImgObs = tilesMode ? makeTileImgObserver(list) : null;
  list.classList.toggle("lib-tiles-view", tilesMode);
  list.style.setProperty("--lib-tile-size", `${val("libraryTileSize")}px`);
  const viewToggle = $("library-view-toggle");
  if (viewToggle) {
    viewToggle.classList.toggle("active", tilesMode);
    viewToggle.setAttribute("aria-pressed", String(tilesMode));
  }
  renderTagFilter();
  renderSavedSearches();
  $("library-search").classList.toggle("hidden", !prompts.length);
  $("library-filter-rows").classList.toggle("hidden", !(flag("libraryTypeFilter") || flag("favorites") || tagsOn()));
  if (!prompts.length) {
    const empty = document.createElement("div");
    empty.className = "hint";
    empty.textContent = t("libraryEmpty");
    list.appendChild(empty);
    libDisplayOrder = []; // nothing on screen: select-all and range select must not reuse the last render
    updateLibraryCount(0);
    return;
  }
  // Hidden filters keep their default state, so this is inert when they are off.
  const items = prompts.filter(libMatches);
  updateLibraryCount(items.length);
  sortLibraryItems(items); // chosen sort (stable), before the favorites float
  // Favorites float to the top (stable otherwise) when the feature is on.
  if (flag("favorites")) items.sort((a, b) => (b.favorite ? 1 : 0) - (a.favorite ? 1 : 0));
  if (!items.length) {
    const empty = document.createElement("div");
    empty.className = "hint";
    empty.textContent = t("libraryNoResults");
    list.appendChild(empty);
    libDisplayOrder = [];
    return;
  }
  const placed = new Set(Object.keys(layoutOf(activeView())));
  // A folder member has no layout entry of its own (the folder draws it), so without this it
  // would show as addable although renderGrid skips ids listed in a folder's members.
  for (const f of foldersOfView(activeView().id)) for (const id of f.members || []) placed.add(id);
  libDisplayOrder = items.map((p) => p.id); // Range-select uses display order
  // Batch through a fragment; appending into the live list forced a reflow per row.
  const frag = document.createDocumentFragment();
  for (const p of items) {
    const row = document.createElement("button");
    row.type = "button";
    row.className = "lib-item";
    row.dataset.id = p.id; // In-place selection toggle finds the row by id
    // Dim rows that only matched via the typo-tolerant tier.
    if (libQuery.trim() && libSearchScore(p).fuzzy) row.classList.add("lib-fuzzy");
    // Selection state (the checkbox is a CSS ::before shown while the list has .selecting, so
    // toggling select mode never re-renders).
    if (libSelectMode && libSelected.has(p.id)) row.classList.add("lib-selected");
    row.title = libSelectMode ? "" : t("copy");
    const rowTimes = tileTimeLines(p.id);
    if (rowTimes) row.dataset.tipTimes = rowTimes;
    const rowMedia = mediaTipLine(p);
    if (rowMedia) row.dataset.tipMedia = rowMedia;

    const body = document.createElement("span");
    body.className = "lib-body";
    const name = document.createElement("span");
    name.className = "lib-name";
    name.textContent = libLabel(p);
    // A search hit inside entry 2+ shows that entry (why the row matched) plus a small
    // "n/total" tag naming it.
    const hitEntry = libQuery.trim() ? libSearchScore(p).entryIndex : 0;
    const hitVariant = hitEntry > 0 ? (p.variants || [])[hitEntry - 1] : null;
    // file_path still wins over a variant hit: a file/image prompt never has variants (the
    // editor gates rotation to text-only prompts).
    const text = document.createElement("span");
    text.className = "lib-text";
    text.textContent = p.file_path || varPreview(hitVariant ?? p.text);
    // Literal hits are marked in the name and the preview; a typo-tolerant or picture hit has
    // no substring to mark. A hit that only lives in the private note gets a label instead.
    const scoreHit = libQuery.trim() ? libSearchScore(p) : null;
    const markQ = scoreHit && !scoreHit.fuzzy && !scoreHit.fromImage ? scoreHit.q : "";
    if (markQ) {
      markHits(name, libLabel(p), markQ);
      markHits(text, text.textContent, markQ);
    }
    const nameRow = document.createElement("span");
    nameRow.className = "lib-name-row";
    nameRow.appendChild(name);
    body.appendChild(nameRow);
    if (hitVariant != null) {
      const tag = document.createElement("span");
      tag.className = "lib-rotate-tag";
      tag.textContent = `${hitEntry + 1}/${1 + p.variants.length}`;
      body.appendChild(tag);
    }
    // Otherwise this hit would look like a random match, since neither the name nor the body
    // contains the query.
    if (libQuery.trim() && libSearchScore(p).fromImage) {
      const imgTag = document.createElement("span");
      imgTag.className = "lib-image-hit-tag";
      imgTag.textContent = t("libFoundInImage");
      body.appendChild(imgTag);
    }
    if (scoreHit && scoreHit.field === "notes" && !scoreHit.fuzzy) {
      const noteTag = document.createElement("span");
      noteTag.className = "lib-note-hit-tag";
      noteTag.textContent = t("libFoundInNote");
      body.appendChild(noteTag);
    }
    body.appendChild(text);
    // Tags are small flat labels beside the name, not controls: at most three, then "+N".
    if (tagsOn() && (p.tags || []).length) {
      const marks = document.createElement("span");
      marks.className = "lib-tag-marks";
      const mark = (label, colorOf) => {
        const m = document.createElement("span");
        m.className = "tag-mark";
        m.textContent = label;
        const c = colorOf ? tagColor(colorOf) : "";
        if (c) {
          m.style.setProperty("--chip-color", c);
          m.classList.add("tag-colored");
        }
        marks.appendChild(m);
      };
      for (const tn of p.tags.slice(0, 3)) mark(tn, tn);
      if (p.tags.length > 3) mark(`+${p.tags.length - 3}`, "");
      nameRow.appendChild(marks);
    }

    // Per-type preview: video plays inline, image/gif as a thumbnail, else a dot. In tiles mode a
    // not-yet-visible row gets a placeholder that _tileImgObs swaps for the real thumbnail.
    if (tilesMode) {
      const placeholder = document.createElement("span");
      placeholder.className = "lib-thumb lib-tile-placeholder dot";
      if (p.color) placeholder.style.background = p.color;
      row.appendChild(placeholder);
      _tileImgObs.observe(row);
    } else {
      row.appendChild(mediaThumb(p, thumbEl(p, libThumbCache)));
    }
    row.appendChild(body);
    maybeBackfillMediaMeta(p, renderLibrary);

    // Every row gets the same actions menu a grid tile has; the image and video editors are
    // entries of it.
    row.addEventListener("contextmenu", (e) => {
      e.preventDefault();
      e.stopPropagation();
      openCtx(p.id, e.clientX, e.clientY);
    });

    row.addEventListener("pointerdown", (e) => {
      if (libSelectMode) return; // no drag while multi-selecting
      if (e.button !== 0 || e.target.closest(".lib-add, .lib-edit, .lib-fav, .lib-delete")) return;
      const out = armDragOut(row, e, () => prompts.find((x) => x.id === p.id));
      assignDrag({ id: p.id, startX: e.clientX, startY: e.clientY, moved: false, el: row, fromLibrary: true, out });
    });
    // Add-to-view icon (leftmost of the action group): place on the current layout.
    if (!placed.has(p.id)) {
      const add = document.createElement("span");
      add.className = "icon-btn lib-add";
      add.title = t("addToLayout");
      putIcon(add, GRID_PLUS);
      add.addEventListener("pointerdown", (e) => e.stopPropagation());
      add.addEventListener("click", async (e) => {
        e.stopPropagation();
        const view = activeView();
        const occupied = new Map(
          Object.entries(layoutOf(view)).map(([id, c]) => [cellKey(...c), id])
        );
        // A folder's cell is not in the layout, but placing a tile onto it is refused.
        for (const f of foldersOfView(view.id)) occupied.set(cellKey(...(f.cell || [0, 0])), f.id);
        const free = firstFree(occupied, view.cols, view.rows);
        if (!free) { toast(t("gridFull")); return; }
        await placeTile(p.id, free[0], free[1]);
        renderLibrary();
      });
      makeKeyboardActivatable(add);
      row.appendChild(add);
    }
    // Star (middle): mark/unmark this prompt as a favorite (feature-gated).
    if (flag("favorites")) {
      const fav = document.createElement("span");
      fav.className = "icon-btn lib-fav" + (p.favorite ? " active" : "");
      fav.textContent = "★";
      fav.title = t(p.favorite ? "favoriteRemove" : "favoriteAdd");
      fav.addEventListener("pointerdown", (e) => e.stopPropagation());
      fav.addEventListener("click", async (e) => {
        e.stopPropagation();
        await toggleFavoritePrompt(p);
      });
      makeKeyboardActivatable(fav);
      row.appendChild(fav);
    }
    const edit = document.createElement("span");
    edit.className = "icon-btn lib-edit";
    edit.title = t("edit");
    putIcon(edit, ICON_EDIT);
    edit.addEventListener("pointerdown", (e) => e.stopPropagation());
    edit.addEventListener("click", (e) => { e.stopPropagation(); editPrompt(p.id); });
    makeKeyboardActivatable(edit);
    row.appendChild(edit);

    const del = document.createElement("span");
    del.className = "icon-btn lib-delete";
    del.title = t("delete");
    putIcon(del, ICON_TRASH);
    del.addEventListener("pointerdown", (e) => e.stopPropagation());
    del.addEventListener("click", async (e) => {
      e.stopPropagation();
      if (!(await trashPromptWithUndo(p.id, p.name))) return;
      assignPrompts(prompts.filter((x) => x.id !== p.id));
      renderLibrary();
      await renderGrid(); // the tile (or its folder) is gone from the grid as well
    });
    makeKeyboardActivatable(del);
    row.appendChild(del);

    row.addEventListener("click", (e) => {
      if (libSelectMode) { toggleLibSelect(p.id, e.shiftKey); return; }
      libraryCopy(p);
    });
    frag.appendChild(row);
  }
  list.appendChild(frag);
}

// ---- Batch operations ----
export function setLibSelectMode(on) {
  libSelectMode = on;
  if (!on) { libSelected.clear(); libLastId = null; }
  const btn = $("library-select-btn");
  btn?.classList.toggle("active", on);
  btn?.setAttribute("aria-pressed", String(on));
  $("lib-batch-bar").classList.toggle("hidden", !on);
  // Toggle the checkbox column via a class, with no list rebuild.
  $("library-list").classList.toggle("selecting", on);
  if (on) { buildBatchColors(); fillBatchViews(); }
  applyLibSelectionDom();
  updateBatchBar();
}
export function toggleLibSelect(id, shift) {
  if (shift && libLastId && libDisplayOrder.includes(libLastId) && libDisplayOrder.includes(id)) {
    const a = libDisplayOrder.indexOf(libLastId), b = libDisplayOrder.indexOf(id);
    const [lo, hi] = a < b ? [a, b] : [b, a];
    const add = !libSelected.has(id); // extend using the target's new state
    for (let i = lo; i <= hi; i++) { if (add) libSelected.add(libDisplayOrder[i]); else libSelected.delete(libDisplayOrder[i]); }
  } else {
    if (libSelected.has(id)) libSelected.delete(id); else libSelected.add(id);
  }
  libLastId = id;
  applyLibSelectionDom(); // in-place class toggle: no full re-render (fast on 1000s)
  updateBatchBar();
}
// Reflect the current selection onto the visible rows without rebuilding the list.
export function applyLibSelectionDom() {
  for (const row of $("library-list").querySelectorAll(".lib-item")) {
    row.classList.toggle("lib-selected", libSelected.has(row.dataset.id));
  }
}
export function updateBatchBar() {
  const n = libSelected.size;
  $("lib-batch-count").textContent = t("selectedCount").replace("{n}", n.toLocaleString(LANG));
  const bar = $("lib-batch-bar");
  const tagsEnabled = tagsOn();
  $("lib-batch-tag-add").classList.toggle("hidden", !tagsEnabled);
  $("lib-batch-tag-remove").classList.toggle("hidden", !tagsEnabled);
  bar.querySelectorAll("button, select").forEach((el) => {
    if (["lib-batch-all", "lib-batch-none"].includes(el.id)) return; // always usable
    el.disabled = n === 0;
  });
  // Comparing needs exactly two entries, not "one or more".
  $("lib-batch-compare").disabled = n !== 2;
}
// Same free-color palette as the editor (buildSwatches + openColorPop). No selection is
// marked, since a batch can hold prompts of different colors.
export let batchColorDebounce = 0;
export function buildBatchColors(wrapId = "lib-batch-colors") {
  const wrap = $(wrapId);
  if (!wrap) return;
  const run = wrapId === "grid-batch-colors"
    ? (color) => doGridBatch("color", { color })
    : (color) => doBatch("color", { color });
  // The free picker's SV field fires onPick on every drag frame (cheap for the editor's local
  // preview); a batch write per frame would spam the backend and clear the selection
  // mid-drag, so settle before committing.
  const debouncedRun = (color) => {
    clearTimeout(batchColorDebounce);
    batchColorDebounce = setTimeout(() => run(color), 200);
  };
  buildSwatches(wrap, "", run, (anchor, cur) => openColorPop(anchor, cur, debouncedRun));
}
export function fillBatchViews(selId = "lib-batch-view") {
  const sel = $(selId);
  if (!sel) return;
  sel.innerHTML = "";
  for (const v of settings.views || []) {
    const o = document.createElement("option");
    o.value = v.id; o.textContent = v.name;
    sel.appendChild(o);
  }
  if (settings.active_view) sel.value = settings.active_view;
}

// The one favorite toggle: the library star and the tile menu entry both land here.
export async function toggleFavoritePrompt(p) {
  p.favorite = !p.favorite;
  try {
    await invoke("set_favorite", { id: p.id, favorite: p.favorite });
  } catch (err) {
    p.favorite = !p.favorite; // backend rolled back on a failed save: mirror it locally
    errToast(err);
  }
  if (!libraryEl.classList.contains("hidden")) renderLibrary();
  if (favView) renderGrid(true); // favorites grid updates live underneath
}

// ---- Grid bulk selection: reuses the same batch_prompts backend, its own
// selection state and toolbar (folder tiles are excluded, see gridSelected). ----
export function toggleGridSelect(id) {
  if (gridSelected.has(id)) gridSelected.delete(id); else gridSelected.add(id);
  const was = document.body.classList.contains("grid-selecting");
  const now = gridSelected.size > 0;
  document.body.classList.toggle("grid-selecting", now);
  applyGridSelectionDom();
  updateGridBatchBar();
  // The composer folds away only at the 0<->1 transition: refit tile text then, not on every
  // checkbox click.
  if (was !== now) fitAllTiles();
}
export function applyGridSelectionDom() {
  for (const tile of gridEl.querySelectorAll(".tile")) {
    tile.classList.toggle("grid-selected", gridSelected.has(tile.dataset.id));
  }
}
export function updateGridBatchBar() {
  const n = gridSelected.size;
  $("grid-batch-bar")?.classList.toggle("hidden", n === 0);
  const count = $("grid-batch-count");
  if (count) count.textContent = t("selectedCount").replace("{n}", n.toLocaleString(LANG));
  // Favorites and archive hide through flag classes in the stylesheet; the tag buttons follow
  // the same gate as the rest of the tag UI.
  const tagsEnabled = tagsOn();
  $("grid-batch-tag-add").classList.toggle("hidden", !tagsEnabled);
  $("grid-batch-tag-remove").classList.toggle("hidden", !tagsEnabled);
  // buildBatchColors() no-ops once built; fillBatchViews() resets the dropdown to the active
  // view, the right default for a selection that just grew.
  if (n > 0) { buildBatchColors("grid-batch-colors"); fillBatchViews("grid-batch-view"); }
}
// True when a dialog other than `selfId` is open, so Escape belongs to the main
// dispatcher and a selection must stay alive. A library raised over the backup,
// statistics or duplicate dialogs is the topmost thing, so those do not count.
export function otherOverlayOpen(selfId) {
  const under = document.body.classList.contains("library-front") ? ["backup-modal", "stats-modal", "dupes-modal"] : [];
  return [...document.querySelectorAll(".overlay:not(.hidden)")].some((o) => o.id !== selfId && !under.includes(o.id));
}
export function clearGridSelection() {
  const was = document.body.classList.contains("grid-selecting");
  gridSelected.clear();
  document.body.classList.remove("grid-selecting");
  applyGridSelectionDom();
  updateGridBatchBar();
  if (was) fitAllTiles(); // composer reappeared: refit tile text to the new height
}
// addView is all-or-nothing on the backend: a full target view rejects the whole batch with
// one error string instead of silently dropping ids.
export function batchErrorToast(e) {
  if (String(e).includes("view-full")) toast(t("gridFull")); else errToast(e);
}
// The backend call of a batch action plus the same change on the local prompt list, shared
// by the grid and the library selection. Returns whether the batch went through (same
// contract as doBatch); false means the backend refused (already toasted) and nothing
// changed locally.
async function mirrorBatch(action, ids, extra = {}) {
  const idset = new Set(ids);
  try {
    // The backend returns only settings (no prompt echo), so a batch over image-heavy prompts
    // does not re-serialise every base64 blob over IPC.
    assignSettings(await invoke("batch_prompts", {
      ids, action,
      color: extra.color ?? null,
      favorite: extra.favorite ?? null,
      viewId: extra.viewId ?? null,
      tag: extra.tag ?? null,
    }));
  } catch (e) { batchErrorToast(e); return false; }
  if (action === "delete") assignPrompts(prompts.filter((p) => !idset.has(p.id)));
  else if (action === "color") { for (const p of prompts) if (idset.has(p.id)) p.color = extra.color ?? ""; }
  else if (action === "favorite") { for (const p of prompts) if (idset.has(p.id)) p.favorite = extra.favorite ?? true; }
  else if (action === "tag-add") {
    const key = tagKey(extra.tag);
    for (const p of prompts) {
      if (idset.has(p.id) && !(p.tags || []).some((t) => tagKey(t) === key)) p.tags = [...(p.tags || []), extra.tag];
    }
  } else if (action === "tag-remove") {
    const key = tagKey(extra.tag);
    for (const p of prompts) if (idset.has(p.id)) p.tags = (p.tags || []).filter((t) => tagKey(t) !== key);
  }
  return true;
}

export async function doGridBatch(action, extra = {}) {
  const ids = [...gridSelected];
  if (!ids.length) return false;
  if (!(await mirrorBatch(action, ids, extra))) return false;
  clearGridSelection();
  await renderGrid(true);
  return true;
}
// A real move, not just "also in": drop the current view's placement, then place into the
// target, keeping the invariant that a prompt lives in exactly one view.
export async function doGridMove(targetViewId) {
  const ids = [...gridSelected];
  if (!ids.length || !targetViewId) return;
  try {
    // One moveView call changes both layouts in the same backend settings write, so a crash
    // mid-call cannot leave a tile placed in both views.
    assignSettings(await invoke("batch_prompts", { ids, action: "moveView", color: null, favorite: null, viewId: targetViewId, fromViewId: settings.active_view }));
  } catch (e) { batchErrorToast(e); return; }
  clearGridSelection();
  await renderGrid(true);
}

// Returns whether the batch went through, so a caller showing its own "done" toast never
// reports success over a failure batchErrorToast already surfaced.
export async function doBatch(action, extra = {}) {
  const ids = [...libSelected];
  if (!ids.length) return false;
  const idset = new Set(ids);
  if (!(await mirrorBatch(action, ids, extra))) return false;
  // The grid sits hidden behind the library modal; rebuilding it is deferred until the library
  // closes.
  gridDirty = true;
  if (action === "delete") {
    // Remove the deleted rows in place instead of rebuilding the whole list.
    for (const id of ids) $("library-list").querySelector(`.lib-item[data-id="${CSS.escape(id)}"]`)?.remove();
    libDisplayOrder = libDisplayOrder.filter((x) => !idset.has(x));
    libSelected.clear(); libLastId = null;
    // An emptied list needs its hint; otherwise only the counts above it are stale.
    if (!libDisplayOrder.length) renderLibrary();
    else updateLibraryCount(prompts.filter(libMatches).length);
  } else {
    // favorite/color/view can reorder or change row content → one rebuild.
    if (action === "addView" || action === "removeView") renderViews();
    renderLibrary();
  }
  updateBatchBar();
  return true;
}

// Undo for a batch tag add/remove: only the ids the original action touched, inverted, so a
// prompt that already carried the tag before the batch is left alone by the undo too.
export async function restoreBatchTag(inverseAction, ids, tag) {
  if (!ids.length) return;
  try {
    assignSettings(await invoke("batch_prompts", { ids, action: inverseAction, color: null, favorite: null, viewId: null, tag }));
  } catch (err) { errToast(err); return; }
  const key = tagKey(tag);
  const idset = new Set(ids);
  for (const p of prompts) {
    if (!idset.has(p.id)) continue;
    if (inverseAction === "tag-add" && !(p.tags || []).some((x) => tagKey(x) === key)) p.tags = [...(p.tags || []), tag];
    else if (inverseAction === "tag-remove") p.tags = (p.tags || []).filter((x) => tagKey(x) !== key);
  }
  if (libraryEl.classList.contains("hidden")) {
    gridDirty = false;
    await renderGrid(true); // the undo came from the grid's own selection bar
  } else {
    gridDirty = true;
    renderLibrary();
  }
}

// Deletes one prompt (it goes to the trash) and offers Undo, which puts it back where
// it was. Returns true on success; a failed save shows the message and returns false.
export async function trashPromptWithUndo(id, name) {
  const found = prompts.find((x) => x.id === id);
  const label = String(name || "").trim() || String(found?.text || "").trim().slice(0, 40);
  const placements = await invoke("placement_snapshot", { ids: [id] }).catch(() => null);
  try {
    await invoke("delete_prompt", { id });
  } catch (err) {
    errToast(err);
    return false;
  }
  undoToast(t("promptTrashedToast").replace("{name}", () => label), () => restoreBatchDelete([id], placements));
  return true;
}

// Undo for a batch delete: every deleted prompt went to the trash, not gone for good.
// The caller hands in the placement snapshot it took before deleting.
export async function restoreBatchDelete(ids, placements) {
  // With the placement snapshot taken before the delete, one command puts every prompt back
  // where it was (views, cells, pill, folders); without it (the snapshot call failed) each one
  // falls back to a fresh cell in the active view.
  if (placements && placements.length) {
    try {
      const st = await invoke("restore_prompts_batch", { placements });
      assignPrompts(st.prompts);
      assignSettings(st.settings);
      gridDirty = true;
      await renderGrid(true);
      if (!libraryEl.classList.contains("hidden")) renderLibrary();
      return;
    } catch (err) { errToast(err); }
  }
  for (const id of ids) {
    try {
      const st = await invoke("restore_prompt", { id });
      assignPrompts(st.prompts);
      assignSettings(st.settings);
    } catch (err) { errToast(err); }
  }
  gridDirty = true;
  await renderGrid(true);
  if (!libraryEl.classList.contains("hidden")) renderLibrary();
}

// The batch tag and archive actions, shared by the library bar and the grid bar: `ids` is the
// selection of whichever window asked, `run(action, extra)` its own runner (doBatch or
// doGridBatch), returning whether the batch went through.
export async function batchTagAdd(ids, run) {
  if (!ids.length) return;
  const raw = await confirmDialog({
    title: t("batchTagAdd"),
    message: t("batchTagAddMsg").replace("{n}", String(ids.length)),
    confirmLabel: t("batchTagAdd"),
    input: { type: "text", placeholder: t("tagsPh"), datalist: allTags() },
    danger: false,
  });
  const tag = (raw || "").trim();
  if (!tag) return;
  const key = tagKey(tag);
  const idset = new Set(ids);
  const changedIds = prompts.filter((p) => idset.has(p.id) && !(p.tags || []).some((x) => tagKey(x) === key)).map((p) => p.id);
  if (await run("tag-add", { tag })) {
    undoToast(t("batchTagAddDone").replace("{n}", String(changedIds.length)), () => restoreBatchTag("tag-remove", changedIds, tag));
  }
}
export async function batchTagRemove(ids, run) {
  if (!ids.length) return;
  const raw = await confirmDialog({
    title: t("batchTagRemove"),
    message: t("batchTagRemoveMsg").replace("{n}", String(ids.length)),
    confirmLabel: t("batchTagRemove"),
    input: { type: "text", placeholder: t("tagsPh"), datalist: allTags() },
    danger: false,
  });
  const tag = (raw || "").trim();
  if (!tag) return;
  const key = tagKey(tag);
  const idset = new Set(ids);
  const changedIds = prompts.filter((p) => idset.has(p.id) && (p.tags || []).some((x) => tagKey(x) === key)).map((p) => p.id);
  if (await run("tag-remove", { tag })) {
    undoToast(t("batchTagRemoveDone").replace("{n}", String(changedIds.length)), () => restoreBatchTag("tag-add", changedIds, tag));
  }
}
export async function batchArchive(ids, run) {
  if (!ids.length) return;
  const tag = await ensureArchiveTagName();
  const key = tagKey(tag);
  const idset = new Set(ids);
  const changedIds = prompts.filter((p) => idset.has(p.id) && !(p.tags || []).some((x) => tagKey(x) === key)).map((p) => p.id);
  if (await run("tag-add", { tag })) {
    undoToast(t("batchArchiveDone").replace("{n}", String(changedIds.length)), () => restoreBatchTag("tag-remove", changedIds, tag));
  }
}

// Asked only when the export set holds a confidential entry. `null` means the whole export
// was cancelled here; otherwise a boolean, default false matching the OK button/Enter key.
export async function confirmSecretExport(idFilter, format) {
  const n = prompts.filter((p) => p.secret && (!idFilter || idFilter.has(p.id))).length;
  if (!n) return false;
  // Only the JSON file carries the confidential flag; other formats can only leave these
  // entries out.
  if (format !== "json") {
    const go = await confirmDialog({
      title: t("secretExportTitle"),
      message: t("secretExportSkipMsg").replace("{n}", String(n)),
      confirmLabel: t("secretExportExclude"),
      danger: false,
    });
    return go ? false : null;
  }
  const choice = await confirmDialog({
    title: t("secretExportTitle"),
    message: t("secretExportMsg").replace("{n}", String(n)),
    confirmLabel: t("secretExportExclude"),
    altLabel: t("secretExportInclude"),
    danger: false,
  });
  if (choice === false) return null; // Cancel: abort the export entirely
  return choice === "alt";
}

export async function batchExport(format, ids) {
  if (!ids.length) return;
  const includeSecrets = await confirmSecretExport(new Set(ids), format);
  if (includeSecrets === null) return;
  try {
    const n = await invoke("export_prompts", { format, ids, includeSecrets });
    toast(t("exportDone").replace("{n}", String(n)));
  } catch (e) {
    if (String(e) !== "canceled") errToast(e);
  }
}

// Copy a prompt, filling {?variables} first when applicable: reserved names resolve
// silently, the rest go through the dialog. Returns false if the copy failed or the user
// cancelled. The single copy entry point for every path that skips the dialog's own
// double-click handling (grid, library, journal).
export async function copyResolved(p) {
  // Confidential and the vault has no key right now: ask for it first, as for a click on a
  // locked tile. A cancelled or wrong password fails the copy like any cancelled copy.
  // The cached copy can say "unlocked" after the backend dropped the key (idle timeout), so
  // ask the backend before trusting it and fall back to the locked path.
  if (p.secret && !p.locked) {
    const st = await invoke("secret_vault_status").catch(() => null);
    if (st && !st.unlocked) {
      await renderGrid();
      const now = prompts.find((x) => x.id === p.id);
      if (!now) return false;
      p = now;
    }
  }
  if (p.locked) {
    if (!(await ensureSecretVaultUnlocked())) return false;
    await renderGrid(); // pulls the now-decrypted text into `prompts`
    const fresh = prompts.find((x) => x.id === p.id);
    if (!fresh || fresh.locked) return false;
    p = fresh;
  }
  // Rotating prompt: peek the entry copy_prompt would use, resolve variables on that text, and
  // commit only once the copy reaches the clipboard, so a cancelled variable dialog never burns
  // an entry.
  if ((p.variants || []).length) {
    // Auto-advance with placeholders in any entry: every entry is filled from one dialog up
    // front so later pastes of the session deliver filled text. The backend moves the pointer
    // itself here, so no commit follows.
    if (p.rotate_auto && flag("rotatingPrompts") && flag("rotateAuto") && anyEntryHasVars(p)) {
      const resolved = await resolveVarsList([p.text, ...p.variants]);
      if (!resolved) return false;
      if (await invoke("rotation_arm_session", { id: p.id, resolved }).catch(() => false)) {
        recordToolUsage("rotation_advance");
        return true;
      }
    }
    // Handing the id over lets the backend open an auto-advance session for an eligible prompt;
    // without it the Auto switch would do nothing on this path, unlike the floating buttons
    // (which copy through copy_prompt).
    const armId = p.rotate_auto && !anyEntryHasVars(p) ? p.id : undefined;
    return peekResolveCommit(p, (text) => invoke("copy_text", { text, id: armId }).catch(() => false), {
      resolve: (raw) => (varTokens(p, raw).length ? resolveVars(raw) : raw),
    });
  }
  if (varTokens(p).length) {
    const text = await resolveVars(p.text);
    if (text === null) return false;
    return invoke("copy_text", { text }).catch(() => false);
  }
  return invoke("copy_prompt", { id: p.id }).catch((e) => { errToast(e); return false; });
}

// A blink says "something happened"; the character count says the whole prompt reached the
// clipboard, the doubt a long one leaves.
export function toastCopied(p) {
  const n = (p?.text || "").length;
  toast(n > 0 ? t("copiedChars").replace("{n}", n.toLocaleString(LANG)) : t("copied"));
}

export async function libraryCopy(p) {
  // Prompts that still need input skip the double-click shortcut: they must reach the dialog
  // (copyResolved) so variables are asked before copy and auto-paste.
  if (!dialogTokens(p, await peekRotatedText(p)).length && autoPasteDouble(p.id)) { await autoPasteNow(null); return; } // 2nd fast click pastes
  if (copyOnCooldown(p.id)) return;
  if (!(await copyResolved(p))) return;
  recordCopy(p.id);
  toastCopied(p);
  if (flag("closeAfterCopy")) hideLibrary();
}

// Reflect the close-after-copy flag on the library header checkbox.
export function syncLibraryToggle() {
  $("library-close-toggle").checked = flag("closeAfterCopy");
}

// Library list columns: the expert value is the one place that decides it.
export const libCols = () => Math.min(6, Math.max(1, Math.round(val("libraryCols"))));
export function syncLibraryCols() {
  $("library-list").style.setProperty("--lib-cols", libCols());
}

export function bindLibrary() {
  $("library-btn").addEventListener("click", async () => {
    const preset = libPresetQuery ?? "";
    libPresetQuery = null;
    await refreshLastEdited();
    await refreshClipInbox(); // decides whether the history tab is offered
    libQuery = preset;
    $("library-q").value = preset;
    // The type pick is remembered per window; search, colour and favourites are not.
    libTypes.clear();
    for (const type of typeFilterOf("libTypeFilter")) libTypes.add(type);
    paintTypeChips($("library-types"), libTypes);
    syncLibViewMode();
    libFav = false;
    $("library-fav").classList.remove("active");
    libShowArchive = false;
    $("library-archive").classList.remove("active");
    // Always open in normal (non-select) mode.
    libSelectMode = false; libSelected.clear(); libLastId = null;
    $("library-select-btn").classList.remove("active");
    $("library-select-btn").setAttribute("aria-pressed", "false");
    $("lib-batch-bar").classList.add("hidden");
    syncLibraryToggle();
    libraryEl.classList.remove("hidden");
    setLibraryTab(libraryTab);
    if (libraryTab === "prompts" && flag("librarySearch") && flag("searchAutofocus")) $("library-q").focus();
  });
  $("library-tab-prompts").addEventListener("click", () => setLibraryTab("prompts"));
  $("library-tab-history").addEventListener("click", () => setLibraryTab("history"));
  // "Close after copy" checkbox in the library header (mirrors the expert flag).
  $("library-close-toggle").addEventListener("change", async (e) => {
    const enabled = e.target.checked;
    settings.ui_flags = { ...(settings.ui_flags || {}), closeAfterCopy: enabled };
    try { await invoke("set_ui_flag", { key: "closeAfterCopy", enabled }); } catch (err) { errToast(err); }
    syncLibraryToggle();
  });
  const renderLibraryDebounced = debounced(renderLibrary);
  const refreshImageHitsDebounced = debounced(() => refreshImageHits(libQuery.trim().toLowerCase()));
  $("library-q").addEventListener("input", (e) => {
    libQuery = e.target.value;
    // With picture text search on and a query typed, the lookup renders once its answer is in;
    // rendering first too would draw the library twice.
    if (optFlag("ocrIndex") && libQuery.trim()) {
      renderLibraryDebounced.cancel(); // a render queued by an earlier blank query
      refreshImageHitsDebounced();
    } else {
      renderLibraryDebounced();
      refreshImageHitsDebounced();
    }
  });
  wireSearchSuggest($("library-q"), "recentSearchLib");
  $("library-search-save").addEventListener("click", () => saveSavedSearch());
  $("library-sort").addEventListener("change", (e) => {
    settings.ui_texts = { ...(settings.ui_texts || {}), librarySort: e.target.value };
    invoke("set_ui_text", { key: "librarySort", value: e.target.value }).catch((err) => errToast(err));
    renderLibrary();
  });
  $("library-types").addEventListener("click", (e) => {
    if (e.target.closest("#library-fav")) {
      libFav = !libFav;
      $("library-fav").classList.toggle("active", libFav);
      renderLibrary();
      return;
    }
    if (e.target.closest("#library-archive")) {
      libShowArchive = !libShowArchive;
      $("library-archive").classList.toggle("active", libShowArchive);
      renderLibrary();
      return;
    }
    const btn = e.target.closest(".lib-type");
    if (btn) {
      toggleTypeFilter(libTypes, btn.dataset.type, $("library-types"), "libTypeFilter");
      syncLibViewMode();
      renderLibrary();
    }
  });
  $("library-view-toggle").addEventListener("click", () => toggleLibViewMode());
  // Batch operations wiring.
  $("library-select-btn").addEventListener("click", () => setLibSelectMode(!libSelectMode));
  $("lib-batch-all").addEventListener("click", () => { for (const id of libDisplayOrder) libSelected.add(id); applyLibSelectionDom(); updateBatchBar(); });
  $("lib-batch-none").addEventListener("click", () => { libSelected.clear(); libLastId = null; applyLibSelectionDom(); updateBatchBar(); });
  $("lib-batch-fav").addEventListener("click", () => doBatch("favorite", { favorite: true }));
  $("lib-batch-unfav").addEventListener("click", () => doBatch("favorite", { favorite: false }));
  $("lib-batch-addview").addEventListener("click", () => doBatch("addView", { viewId: $("lib-batch-view").value }));
  $("lib-batch-removeview").addEventListener("click", () => doBatch("removeView", { viewId: $("lib-batch-view").value }));
  $("lib-batch-tag-add").addEventListener("click", () => batchTagAdd([...libSelected], doBatch));
  $("lib-batch-tag-remove").addEventListener("click", () => batchTagRemove([...libSelected], doBatch));
  $("lib-batch-archive").addEventListener("click", () => batchArchive([...libSelected], doBatch));
  $("lib-batch-unarchive").addEventListener("click", async () => {
    if (!libSelected.size) return;
    const tag = archiveTagName();
    const key = tagKey(tag);
    const changedIds = prompts.filter((p) => libSelected.has(p.id) && (p.tags || []).some((x) => tagKey(x) === key)).map((p) => p.id);
    if (await doBatch("tag-remove", { tag })) {
      undoToast(t("batchUnarchiveDone").replace("{n}", String(changedIds.length)), () => restoreBatchTag("tag-add", changedIds, tag));
    }
  });
  // Only meaningful at exactly two selected (updateBatchBar disables it otherwise); opens the
  // same compare view as a tile's context menu.
  $("lib-batch-compare").addEventListener("click", () => {
    if (libSelected.size !== 2) return;
    openCompareView([...libSelected]);
  });
  $("lib-batch-export").addEventListener("click", () => batchExport($("lib-batch-format").value, [...libSelected]));
  $("lib-batch-delete").addEventListener("click", async () => {
    const n = libSelected.size;
    if (!n) return;
    const ids = [...libSelected];
    const placements = await invoke("placement_snapshot", { ids }).catch(() => null);
    if (await doBatch("delete")) {
      undoToast(t("batchDeleteDone").replace("{n}", String(n)), () => restoreBatchDelete(ids, placements));
    }
  });
  // Esc leaves selection mode before it closes the library (capture beats the overlay).
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && libSelectMode && !libraryEl.classList.contains("hidden") && !otherOverlayOpen("library")) {
      e.stopPropagation();
      setLibSelectMode(false);
    }
  }, true);
  // Grid batch toolbar wiring, same shape as the library's with its own state. "All" means
  // every prompt tile on screen; folder tiles carry no select button and stay out.
  $("grid-batch-all").addEventListener("click", () => {
    for (const tile of gridEl.querySelectorAll(".tile")) {
      if (tile.dataset.id && tile.querySelector(":scope > .tile-select")) gridSelected.add(tile.dataset.id);
    }
    document.body.classList.toggle("grid-selecting", gridSelected.size > 0);
    applyGridSelectionDom();
    updateGridBatchBar();
  });
  $("grid-batch-none").addEventListener("click", clearGridSelection);
  $("grid-batch-fav").addEventListener("click", () => doGridBatch("favorite", { favorite: true }));
  $("grid-batch-unfav").addEventListener("click", () => doGridBatch("favorite", { favorite: false }));
  $("grid-batch-tag-add").addEventListener("click", () => batchTagAdd([...gridSelected], doGridBatch));
  $("grid-batch-tag-remove").addEventListener("click", () => batchTagRemove([...gridSelected], doGridBatch));
  $("grid-batch-archive").addEventListener("click", () => batchArchive([...gridSelected], doGridBatch));
  $("grid-batch-move").addEventListener("click", () => doGridMove($("grid-batch-view").value));
  $("grid-batch-export").addEventListener("click", () => batchExport($("grid-batch-format").value, [...gridSelected]));
  $("grid-batch-delete").addEventListener("click", async () => {
    const n = gridSelected.size;
    if (!n) return;
    const ids = [...gridSelected];
    const placements = await invoke("placement_snapshot", { ids }).catch(() => null);
    if (await doGridBatch("delete")) {
      undoToast(t("batchDeleteDone").replace("{n}", String(n)), () => restoreBatchDelete(ids, placements));
    }
  });
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && gridSelected.size && !otherOverlayOpen("")) {
      e.stopPropagation();
      clearGridSelection();
    }
  }, true);

  $("library-close").addEventListener("click", hideLibrary);
  libraryEl.addEventListener("pointerdown", (e) => {
    if (e.target === libraryEl) hideLibrary();
  });

  attachSelectPicker($("grid-batch-view")); // themed popup, options refreshed by fillBatchViews()
  attachSelectPicker($("lib-batch-view"));
  attachSelectPicker($("lib-batch-format"));
  attachSelectPicker($("grid-batch-format"));

  attachSelectPicker($("library-sort"));
  fillVideoFormatSelect($("batch-export-format"));
  attachSelectPicker($("batch-export-format"));
  attachSelectPicker($("batch-export-quality"));
}
