// The prompt library: search, filters, batch selection, list and tile rendering.
import { $, assignDrag, assignPrompts, assignSettings, cellKey, COLORS, convertFileSrc, debounced, errToast, favView, flag, GRID_PLUS, gridEl, gridSelected, ICON_EDIT, ICON_TRASH, IMG_WAIT, invoke, LANG, libraryEl, libraryTab, libThumbCache, makeKeyboardActivatable, openColorPop, optFlag, PDF_EXT, prompts, putIcon, settings, settingsEl, t, toast, txt, undoToast, val } from "./core.js";
import { mediaKind } from "./media.js";
import { armDragOut, autoPasteDouble, autoPasteNow, fitAllTiles, hasImage, imageOf, notesOn, placeTile } from "./tiles.js";
import { openLightbox, wireSearchSuggest } from "./windowChrome.js";
import { fmtBytes } from "./floatShared.js";
import { anyEntryHasVars, dialogTokens, extractVars, parseVar, peekRotatedText, resolveVars, varTokens } from "./promptVars.js";
import { confirmDialog } from "./dialog.js";
import { activeView, firstFree, layoutOf, renderGrid, renderViews } from "./views.js";
import { copyOnCooldown, recordCopy } from "./copyFlow.js";
import { attachSelectPicker } from "./selectpop.js";
import { allTags, renderTagFilter, tagChipEl, tagKey, tagsOn } from "./tags.js";
import { refreshClipInbox, setLibraryTab, tileTimeLines } from "./journal.js";

let buildSwatches, closeBackups, editPrompt, ensureSecretVaultUnlocked, foldersOfView, openCompareView, openCtx, recordToolUsage, renderStatsPanel; // linked from main.js
export function linkLibrary(fns) { ({ buildSwatches, closeBackups, editPrompt, ensureSecretVaultUnlocked, foldersOfView, openCompareView, openCtx, recordToolUsage, renderStatsPanel } = fns); }

export let libQuery = ""; // prompt library search text
export function assignLibQuery(v) { libQuery = v; }
// Ids whose recognized image text matched the last query sent to
// search_image_text, and the (trimmed, lowercased) query they were fetched
// for -- image_text itself never reaches the frontend, so this is the only
// trace of a hit computeLibSearchScore ever sees.
export let imageHitIds = new Set();
export let imageHitQuery = "";
// Type filters are ADDITIVE and identical in both windows: an empty set means every
// type, each chip toggles its own, and the pick is stored per window.
export const libTypes = new Set();
export let libColor = "all"; // prompt library color filter
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
// Classify a prompt for the library type filter / thumbnails.
export function promptType(p) {
  if (PDF_EXT.test(p.file_path)) return "pdf";
  const kind = p.file_path ? mediaKind(p.file_path) : p.icon_path ? mediaKind(p.icon_path) : "";
  if (kind === "video") return "video";
  if (kind === "image" || kind === "gif") return "image";
  if (p.copy_image && hasImage(p)) return "image";
  if (p.file_path) return "file";
  return "text";
}

// Library display name: fall back to a type label ("Bild" etc.) so saved
// images/screenshots/files are never shown blank.
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
  // A stored image outranks any path preview: while it is still loading, show
  // nothing rather than briefly swapping in the attachment's own thumbnail.
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

// Only the videos currently scrolled into view play: keeps a long list cheap.
// Library and journal each get their own observer so opening one panel never
// pauses the other's still-mounted video thumbnails.
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
  if (p.color) dot.style.background = p.color;
  return dot;
}

// Shared row preview for library + journal: a looping muted inline video for
// video prompts, an image for image/gif/stored-image prompts, else a color dot.
// `cache` reuses the element across re-renders of the SAME panel instead of
// re-decoding the same base64 blob on every keystroke: appending an already-
// mounted node just reparents it, so an id whose thumb type stays the same never
// gets a fresh <img>/<video>.
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
    // renderLibrary/renderJournal build a FRESH IntersectionObserver every
    // render (disconnecting the old one) -- a cache hit must re-observe with the
    // current one, or a video that survives a re-render (e.g. the search field)
    // stops being paused/started by scroll after the first refresh.
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
// Library and copy history only, by explicit request -- buildTile (the main
// grid's own thumbEl caller) never wraps its thumbnail with this and never
// reads media_duration_ms/media_width/media_height/media_size_bytes.

export const mediaDurationLabel = (ms) => {
  const s = Math.round(ms / 1000);
  const m = Math.floor(s / 60);
  return `${m}:${String(s % 60).padStart(2, "0")}`;
};

// Wraps a library/history thumbnail with its duration badge when the entry
// carries a measured video length. Returns the thumb unchanged otherwise, so a
// caller can always write `row.append(mediaThumb(p, thumbEl(p, cache)), body)`.
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

// Resolution + file size line for the library/history tooltip. "" = nothing to
// add -- a value that could not be measured shows nothing, never a placeholder.
export function mediaTipLine(p) {
  if (!flag("mediaMeta")) return "";
  const parts = [];
  if (p.media_width && p.media_height) parts.push(`${p.media_width}×${p.media_height}`);
  if (p.media_size_bytes) parts.push(fmtBytes(p.media_size_bytes));
  return parts.join(" · ");
}

// The one-time catch-up for a row that has never been measured, or whose first
// measurement failed: fires at most once per id per session and never blocks
// the render -- the backend call is fire-and-forget, and the panel only
// re-renders once it actually comes back with something to show.
export const mediaMetaBackfilled = new Set();
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
      rerender();
    })
    .catch(() => {}); // best-effort -- a failed catch-up just tries again next session
}

// ---- Additive type filters, shared by the library and the copy history ----
// Nothing picked = every type. "All" clears the set, every other chip toggles itself,
// so PDF and File can be shown together. The pick is stored per window.
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

// ---- Fuzzy search: tiered scoring (exact > prefix > fuzzy) over the same
// fields the plain search used. Fuzzy only kicks in when no substring hit exists,
// so the common case short-circuits and stays cheap on 1000+ prompts.
export function fuzzyMaxTyposFor(term) {
  const raw = settings.ui_texts?.fuzzyMaxTypos || "auto";
  if (raw === "1") return 1;
  if (raw === "2") return 2;
  return term.length <= 5 ? 1 : 2; // auto
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
// Weighted match score for the active query. score 0 = no match; fuzzy = true when
// only the typo-tolerant tier matched (rendered dimmed).
// Per-render memo: libSearchScore is called for every prompt in the filter, TWICE
// per comparison in the sort, and once more per row for the fuzzy class. Fuzzy mode
// runs bounded edit-distance over every token, so recomputing 3×/keystroke lagged.
// Cache is cleared at the top of each renderLibrary (see below).
export let libScoreCache = new Map();
export function libSearchScore(p) {
  const hit = libScoreCache.get(p.id);
  if (hit) return hit;
  const res = computeLibSearchScore(p);
  libScoreCache.set(p.id, res);
  return res;
}
// Whether a prompt satisfies one tag:/var: condition from the search query.
// tag: matches a tag's own identity (tagKey), case-insensitive, not a
// substring -- the plain search already does substring matching over tag
// text. var: matches a placeholder's name, read the same way the variable
// dialog itself reads it (VAR_RE over the body, then the "|options" split).
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
export function computeLibSearchScore(p) {
  const rawInput = libQuery.trim().toLowerCase();
  if (!rawInput) return { score: 1, fuzzy: false, entryIndex: 0 };
  // tag:word and var:word (and their negated "-tag:"/"-var:" forms) are hard
  // conditions, pulled out before anything else runs -- a prompt that fails
  // one of them is out, whatever the rest of the query would otherwise have
  // scored it. parseSearchQuery leaves everything else untouched.
  const { rest, conditions } = parseSearchQuery(rawInput);
  if (conditions.length && !conditions.every((c) => searchConditionMet(c, p))) {
    return { score: 0, fuzzy: false, entryIndex: 0 };
  }
  const raw = rest;
  if (!raw) return { score: 1, fuzzy: false, entryIndex: 0 }; // only tag:/var: terms given
  // entryIndex tags which entry a body-field match came from (0 = p.text, the
  // library row's default preview; 1+ = variants[entryIndex-1]): a rotating
  // prompt is findable by ANY entry, and the row can then show the one that
  // actually matched instead of always entry 1 regardless of why it appeared.
  const fields = [[(p.name || "").toLowerCase(), 3, -1], [(p.text || "").toLowerCase(), 1, 0], [(p.file_path || "").toLowerCase(), 1, -1]];
  (p.variants || []).forEach((v, i) => fields.push([(v || "").toLowerCase(), 1, i + 1]));
  // A tag is a deliberate label, so it ranks between the name and the body.
  if (tagsOn() && (p.tags || []).length) fields.push([p.tags.join(" ").toLowerCase(), 2, -1]);
  // A note is private context, so it matches but ranks below the prompt body.
  if (notesOn() && p.notes) fields.push([p.notes.toLowerCase(), 0.5, -1]);
  // A "-word" term excludes any prompt whose searchable text contains it,
  // checked across every field before anything else runs.
  const terms = raw.split(/\s+/).filter(Boolean);
  const isExclude = (t) => t.length > 1 && t[0] === "-";
  const excludes = terms.filter(isExclude).map((t) => t.slice(1));
  if (excludes.some((ex) => fields.some(([val]) => val.includes(ex)))) return { score: 0, fuzzy: false, entryIndex: 0 };
  const q = terms.filter((t) => !isExclude(t)).join(" ");
  if (!q) return { score: 1, fuzzy: false, entryIndex: 0 }; // exclusion-only query matches everything not excluded
  let best = 0, bestEntry = 0;
  for (const [val, w, entry] of fields) {
    if (val && val.includes(q)) {
      const s = (val.startsWith(q) ? 120 : 100) * w;
      if (s > best) { best = s; bestEntry = entry >= 0 ? entry : 0; }
    }
  }
  if (best) return { score: best, fuzzy: false, entryIndex: bestEntry };
  if (!flag("fuzzySearch")) return { score: 0, fuzzy: false, entryIndex: 0 };
  // Tokenize on any non-alphanumeric so "code," / "code." / code blocks split into
  // clean words (scripts without spaces, e.g. CJK, stay one token: substring wins).
  const qWords = fuzzyTokens(q);
  if (!qWords.length) return { score: 0, fuzzy: false, entryIndex: 0 };
  for (const [val, w, entry] of fields) {
    if (!val) continue;
    const words = fuzzyTokens(val);
    let matched = 0;
    for (const qw of qWords) {
      const max = fuzzyMaxTyposFor(qw);
      if (words.some((vw) => fuzzyWordHit(qw, vw, max))) matched++;
    }
    if (matched === qWords.length) {
      const s = 40 * w;
      if (s > best) { best = s; bestEntry = entry >= 0 ? entry : 0; }
    }
  }
  if (best) return { score: best, fuzzy: true, entryIndex: bestEntry };
  // Last resort: text recognized inside an attached picture. Only reached
  // when nothing else -- not even a typo-tolerant match -- found this prompt,
  // so it never outranks a real match on the name, the body, a tag or a note.
  if (optFlag("ocrIndex") && imageHitQuery === rawInput && imageHitIds.has(p.id)) {
    return { score: 90, fuzzy: false, entryIndex: 0, fromImage: true };
  }
  return { score: 0, fuzzy: false, entryIndex: 0 };
}
// The only place image_text is ever read back: a backend lookup, since the
// state payload never carries it. Fired once per query change (never inside
// computeLibSearchScore itself, which runs several times per prompt per
// render) and cached in imageHitIds/imageHitQuery until the query changes
// again. A stubbed or failing call just means no image hits, never a search
// that throws.
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
// A query word matches a field word by substring, whole-word edit distance, or:
// for longer field words: edit distance against its leading prefix ("cde"→"code",
// "cde"→"codebase").
export function fuzzyWordHit(qw, vw, max) {
  if (vw.includes(qw)) return true;
  if (editDistWithin(qw, vw, max) <= max) return true;
  if (vw.length > qw.length + max) return editDistWithin(qw, vw.slice(0, qw.length + max), max) <= max;
  return false;
}

// The archive is a reserved tag, not a new prompt field or state -- every other
// tag-aware path (search, export, backup, sync) already carries it through
// without needing to know this concept exists.
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

export function libMatches(p) {
  if (libFav && !p.favorite) return false;
  if (!typeAllowed(libTypes, p)) return false;
  if (libColor !== "all" && (p.color || "") !== libColor) return false;
  if (!tagMatches(p)) return false;
  // Browsing without a search query hides an archived prompt unless the show-archive
  // toggle is on; typing an actual query still finds it, the list is only quieter.
  if (flag("archiveTag") && !libQuery.trim() && !libShowArchive && isArchived(p)) return false;
  return libSearchScore(p).score > 0;
}

// Saved searches: query text, active tag filters and sort, kept together as one
// JSON blob in a single settings slot -- no new backend concept needed, the
// generic set_ui_text command already persists any string.
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
// A long query would blow up the chip row, so the suggested name is a plain
// truncation of it -- still editable in the same dialog before it is saved.
export function suggestedSearchName(query) {
  const s = query.trim();
  return s.length > 40 ? s.slice(0, 40).trim() + "…" : s;
}
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
    chip.draggable = true;
    chip.title = entry.query || entry.name;
    const label = document.createElement("span");
    label.className = "tag-chip-label";
    label.textContent = entry.name;
    chip.appendChild(label);
    chip.addEventListener("click", () => applySavedSearch(entry));
    chip.addEventListener("contextmenu", (e) => {
      e.preventDefault();
      e.stopPropagation();
      editSavedSearch(i);
    });
    chip.addEventListener("dragstart", (e) => {
      e.dataTransfer.setData("text/plain", String(i));
      e.dataTransfer.effectAllowed = "move";
    });
    chip.addEventListener("dragover", (e) => { e.preventDefault(); chip.classList.add("drag-over"); });
    chip.addEventListener("dragleave", () => chip.classList.remove("drag-over"));
    chip.addEventListener("drop", async (e) => {
      e.preventDefault();
      chip.classList.remove("drag-over");
      const from = parseInt(e.dataTransfer.getData("text/plain"), 10);
      if (!Number.isFinite(from) || from === i) return;
      const cur = loadSavedSearches();
      const [moved] = cur.splice(from, 1);
      cur.splice(i, 0, moved);
      await persistSavedSearches(cur);
      renderSavedSearches();
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
  const max = val("savedSearchMax");
  if (list.length >= max) {
    const oldest = list[0];
    toast(t("savedSearchLimitToast").replace("{max}", String(max)).replace("{name}", oldest ? oldest.name : ""));
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

// Selected tag chips narrow the list, combined with search and the other
// filters. AND (default) needs every chip, OR any of them. The grid's own
// filter (gridTags) reuses this with its own Set instead of the library's.
export function tagMatches(p, tagSet = libTags) {
  if (!tagsOn() || !tagSet.size) return true;
  const own = new Set((p.tags || []).map(tagKey));
  const wanted = [...tagSet];
  return (txt("tagFilterMode") || "and") === "or"
    ? wanted.some((k) => own.has(k))
    : wanted.every((k) => own.has(k));
}

// Color-filter dots (once per wrap). "All" + every palette color. Shared by the
// library and the copy-history journal.
export function buildColorFilter(wrap, current) {
  if (!wrap || wrap.childElementCount) return; // built once; refreshColorUI clears first
  const mk = (color, cls) => {
    const b = document.createElement("button");
    b.type = "button";
    b.className = `lib-color ${cls}`.trim();
    b.dataset.color = color;
    if (color && color !== "all") {
      b.style.background = color;
      b.title = color;
      b.setAttribute("aria-label", color);
    }
    if (color === current) b.classList.add("active");
    return b;
  };
  wrap.appendChild(mk("all", "lib-color-all"));
  for (const c of COLORS) if (c) wrap.appendChild(mk(c, ""));
}
export function setActiveFilter(wrap, selector, attr, value) {
  for (const b of wrap.querySelectorAll(selector)) {
    b.classList.toggle("active", b.dataset[attr] === value);
  }
}
export function setLibColor(color) {
  libColor = color;
  setActiveFilter($("library-colors"), ".lib-color", "color", color);
}

// "last edited" per prompt: from prompt_versions (a version row is only
// ever written on a real name/text change, see update_prompt), refetched
// whenever the library opens or an edit lands while it's already open. A
// prompt absent from the map has no version row (history off, or never
// content-edited since creation) and sorts to the end, same as an unused
// prompt already does under "most used"/"recently used" above.
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
  // One Collator for the whole sort instead of one per pairwise comparison:
  // localeCompare(str, locale) builds a fresh one internally on every call.
  const collator = new Intl.Collator(LANG);
  const byName = (a, b) => collator.compare(nameOf(a), nameOf(b));
  if (mode === "default") {
    // Default + active query = relevance order (exact/prefix above fuzzy).
    if (q) items.sort((a, b) => (libSearchScore(b).score - libSearchScore(a).score) || byName(a, b));
    return;
  }
  if (mode === "name") { items.sort(byName); return; }
  // Usage/recency desc; unused (0) fall last, ties resolve by name: all stable.
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

// Close the library and flush any deferred grid rebuild from batch edits.
// The library was raised over the backups dialog by jumpToPrompt. Put it back down;
// `closeBehind` also dismisses the dialogs underneath (dragging onto the grid needs
// the grid). Returns true if the library was raised at all.
export function dropLibraryFront(closeBehind) {
  if (!document.body.classList.contains("library-front")) return false;
  document.body.classList.remove("library-front");
  if (closeBehind) {
    closeBackups();
    settingsEl.classList.add("hidden");
  }
  return true;
}
export function hideLibrary() {
  libraryEl.classList.add("hidden");
  // Prompts may have been edited or deleted while the library was up: the
  // statistics underneath would otherwise still show the old numbers.
  if (dropLibraryFront(false)) renderStatsPanel($("stats-panel"));
  if (gridDirty) { gridDirty = false; renderGrid(true); }
}

// List/tiles toggle: remembered per active type filter, so switching to
// "only images" and picking tiles there sticks the next time that same
// filter is chosen, without forcing tiles everywhere else too.
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
// Tiles only build their real thumbnail once scrolled into view -- otherwise a
// library that is mostly pictures would fetch every one of them the moment it
// opens, the exact "500 images at once" cost imageOf's own cache exists to avoid.
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

export function renderLibrary() {
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
  if (viewToggle) viewToggle.classList.toggle("active", tilesMode);
  renderTagFilter();
  renderSavedSearches();
  const anyLibFilter = flag("librarySearch") || flag("libraryTypeFilter") || flag("colorFilter") || flag("favorites") || tagsOn();
  $("library-search").classList.toggle("hidden", !prompts.length || !anyLibFilter);
  if (!prompts.length) {
    const empty = document.createElement("div");
    empty.className = "hint";
    empty.textContent = t("libraryEmpty");
    list.appendChild(empty);
    return;
  }
  // Hidden filters keep their default state, so this is inert when they are off.
  const items = prompts.filter(libMatches);
  // How hard the filters are biting, in the header where the filters are.
  const count = $("library-count");
  if (count) {
    const all = prompts.length;
    count.textContent = items.length === all
      ? String(all)
      : t("libraryCount").replace("{n}", items.length).replace("{v}", all);
  }
  sortLibraryItems(items); // chosen sort (stable), before the favorites float
  // Favorites float to the top (stable otherwise) when the feature is on.
  if (flag("favorites")) items.sort((a, b) => (b.favorite ? 1 : 0) - (a.favorite ? 1 : 0));
  if (!items.length) {
    const empty = document.createElement("div");
    empty.className = "hint";
    empty.textContent = t("libraryNoResults");
    list.appendChild(empty);
    return;
  }
  const placed = new Set(Object.keys(layoutOf(activeView())));
  // A folder member has no layout entry of its own (the folder draws it), so
  // without this it would show as addable: placing it would silently no-op since
  // renderGrid always skips ids listed in a folder's members.
  for (const f of foldersOfView(activeView().id)) for (const id of f.members || []) placed.add(id);
  libDisplayOrder = items.map((p) => p.id); // Range-select uses display order
  // Batch through a fragment -- appending straight into the live list forced
  // a reflow per row on a large library.
  const frag = document.createDocumentFragment();
  for (const p of items) {
    const row = document.createElement("button");
    row.type = "button";
    row.className = "lib-item";
    row.dataset.id = p.id; // In-place selection toggle finds the row by id
    // Dim rows that only matched via the typo-tolerant tier.
    if (libQuery.trim() && libSearchScore(p).fuzzy) row.classList.add("lib-fuzzy");
    // Selection state (the checkbox itself is a CSS ::before, shown while the
    // list carries the .selecting class: so toggling select mode never re-renders).
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
    // A search hit inside entry 2+ shows THAT entry (why the row matched at
    // all) instead of always entry 1, plus a small "n/total" tag naming it.
    const hitEntry = libQuery.trim() ? libSearchScore(p).entryIndex : 0;
    const hitVariant = hitEntry > 0 ? (p.variants || [])[hitEntry - 1] : null;
    // file_path still wins over a variant hit: a file/image prompt can never
    // actually HAVE variants (the editor gates rotation to text-only prompts), so
    // hitVariant is never non-null on a prompt that also has a file_path.
    const text = document.createElement("span");
    text.className = "lib-text";
    text.textContent = p.file_path || (hitVariant ?? p.text);
    body.appendChild(name);
    if (hitVariant != null) {
      const tag = document.createElement("span");
      tag.className = "lib-rotate-tag";
      tag.textContent = `${hitEntry + 1}/${1 + p.variants.length}`;
      body.appendChild(tag);
    }
    // Otherwise this hit would look like a random match, since nothing in
    // the visible name or body actually contains the query.
    if (libQuery.trim() && libSearchScore(p).fromImage) {
      const imgTag = document.createElement("span");
      imgTag.className = "lib-image-hit-tag";
      imgTag.textContent = t("libFoundInImage");
      body.appendChild(imgTag);
    }
    body.appendChild(text);
    if (tagsOn() && (p.tags || []).length) {
      const row2 = document.createElement("span");
      row2.className = "lib-tags";
      for (const tn of p.tags) row2.appendChild(tagChipEl(tn));
      body.appendChild(row2);
    }

    // Per-type preview: video plays inline, image/gif as a thumbnail, else a dot.
    // In tiles mode a not-yet-visible row gets a plain placeholder instead --
    // _tileImgObs swaps in the real thumbnail once it scrolls into view.
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

    // Every row, media rows included, gets the same actions menu a grid tile
    // has; the image and video editors are entries of that menu.
    row.addEventListener("contextmenu", (e) => {
      e.preventDefault();
      e.stopPropagation();
      openCtx(p.id, e.clientX, e.clientY);
    });

    // Place on the current layout: drag the row onto the grid, or one click.
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
        p.favorite = !p.favorite;
        try {
          await invoke("set_favorite", { id: p.id, favorite: p.favorite });
        } catch (err) {
          p.favorite = !p.favorite; // backend rolled back on a failed save: mirror it locally
          toast(String(err) === "save-failed" ? t("persistFailed") : String(err));
        }
        renderLibrary();
        if (favView) renderGrid(true); // favorites grid updates live underneath
      });
      makeKeyboardActivatable(fav);
      row.appendChild(fav);
    }
    // Edit icon (rightmost): opens the edit dialog without copying.
    const edit = document.createElement("span");
    edit.className = "icon-btn lib-edit";
    edit.title = t("edit");
    putIcon(edit, ICON_EDIT);
    edit.addEventListener("pointerdown", (e) => e.stopPropagation());
    edit.addEventListener("click", (e) => { e.stopPropagation(); editPrompt(p.id); });
    makeKeyboardActivatable(edit);
    row.appendChild(edit);

    // Delete (far right): goes to the trash and shows an Undo toast.
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

    // Click copies the prompt (or toggles selection in batch mode).
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
  // Toggle the checkbox column via a class: no list rebuild (instant on 1000s).
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
  // Same feature gate as the rest of the tag UI (filter chips, manager button).
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
// Same free-color palette as the editor (buildSwatches + openColorPop)
// instead of "none" + presets only: no selection is ever marked here since a
// batch can hold prompts of different colors.
export let batchColorDebounce = 0;
export function buildBatchColors(wrapId = "lib-batch-colors") {
  const wrap = $(wrapId);
  if (!wrap) return;
  const run = wrapId === "grid-batch-colors"
    ? (color) => doGridBatch("color", { color })
    : (color) => doBatch("color", { color });
  // The free picker's SV field fires onPick on every drag frame (fine for the
  // editor's cheap local-state preview): a batch write per frame would spam
  // the backend and clear the selection mid-drag, so settle before committing.
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

// ---- Grid bulk selection: reuses the same batch_prompts backend, its own
// selection state and toolbar (folder tiles are excluded, see gridSelected). ----
export function toggleGridSelect(id) {
  if (gridSelected.has(id)) gridSelected.delete(id); else gridSelected.add(id);
  const was = document.body.classList.contains("grid-selecting");
  const now = gridSelected.size > 0;
  document.body.classList.toggle("grid-selecting", now);
  applyGridSelectionDom();
  updateGridBatchBar();
  // The composer folds away only at the 0<->1 transition: refit tile text
  // to the grid's new height then, not on every further checkbox click.
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
  // Favorites and archive hide through their flag classes in the stylesheet; the
  // tag buttons follow the same gate as every other piece of tag UI.
  const tagsEnabled = tagsOn();
  $("grid-batch-tag-add").classList.toggle("hidden", !tagsEnabled);
  $("grid-batch-tag-remove").classList.toggle("hidden", !tagsEnabled);
  // buildBatchColors() no-ops once already built; fillBatchViews() harmlessly
  // resets the dropdown to the active view each time, which is the right default
  // for a selection that just grew.
  if (n > 0) { buildBatchColors("grid-batch-colors"); fillBatchViews("grid-batch-view"); }
}
export function clearGridSelection() {
  const was = document.body.classList.contains("grid-selecting");
  gridSelected.clear();
  document.body.classList.remove("grid-selecting");
  applyGridSelectionDom();
  updateGridBatchBar();
  if (was) fitAllTiles(); // composer reappeared: refit tile text to the new height
}
// addView is all-or-nothing on the backend now: a full target view rejects
// the whole batch with this one error string instead of silently dropping ids.
export function batchErrorToast(e) {
  toast(String(e).includes("view-full") ? t("gridFull") : String(e));
}
// Returns whether the batch went through (same contract as doBatch), so the
// shared tag/archive/delete helpers never report success over a failure.
export async function doGridBatch(action, extra = {}) {
  const ids = [...gridSelected];
  if (!ids.length) return false;
  const idset = new Set(ids);
  try {
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
  clearGridSelection();
  await renderGrid(true);
  return true;
}
// A real move (not just "also in"): drop the current view's placement first, then
// place into the target: matches the "a prompt lives in exactly one view"
// invariant everywhere else in the app relies on.
export async function doGridMove(targetViewId) {
  const ids = [...gridSelected];
  if (!ids.length || !targetViewId) return;
  try {
    // One moveView call - both layouts change in the same backend settings
    // write, so a crash mid-call can no longer leave a tile placed in both views
    // (the old addView-then-removeView pair was two separate IPC round-trips).
    assignSettings(await invoke("batch_prompts", { ids, action: "moveView", color: null, favorite: null, viewId: targetViewId, fromViewId: settings.active_view }));
  } catch (e) { batchErrorToast(e); return; }
  clearGridSelection();
  await renderGrid(true);
}

// Returns whether the batch actually went through, so a caller showing its own
// "done" toast (delete, tag add/remove) never reports success over a failure
// batchErrorToast already surfaced.
export async function doBatch(action, extra = {}) {
  const ids = [...libSelected];
  if (!ids.length) return false;
  const idset = new Set(ids);
  try {
    // Backend now returns ONLY settings (no prompt echo): so a batch over
    // image-heavy prompts no longer re-serialises every base64 blob over IPC.
    assignSettings(await invoke("batch_prompts", {
      ids, action,
      color: extra.color ?? null,
      favorite: extra.favorite ?? null,
      viewId: extra.viewId ?? null,
      tag: extra.tag ?? null,
    }));
  } catch (e) { batchErrorToast(e); return false; }
  // Mirror the change onto the local prompt list ourselves.
  if (action === "delete") assignPrompts(prompts.filter((p) => !idset.has(p.id)));
  else if (action === "favorite") { for (const p of prompts) if (idset.has(p.id)) p.favorite = extra.favorite ?? true; }
  else if (action === "color") { for (const p of prompts) if (idset.has(p.id)) p.color = extra.color ?? ""; }
  else if (action === "tag-add") {
    const key = tagKey(extra.tag);
    for (const p of prompts) {
      if (idset.has(p.id) && !(p.tags || []).some((t) => tagKey(t) === key)) {
        p.tags = [...(p.tags || []), extra.tag];
      }
    }
  } else if (action === "tag-remove") {
    const key = tagKey(extra.tag);
    for (const p of prompts) if (idset.has(p.id)) p.tags = (p.tags || []).filter((t) => tagKey(t) !== key);
  }
  // The grid sits hidden behind the library modal: rebuilding it (a full fit-text
  // pass) is deferred until the library closes.
  gridDirty = true;
  if (action === "delete") {
    // Remove the deleted rows in place instead of rebuilding the whole list.
    for (const id of ids) $("library-list").querySelector(`.lib-item[data-id="${CSS.escape(id)}"]`)?.remove();
    libDisplayOrder = libDisplayOrder.filter((x) => !idset.has(x));
    libSelected.clear(); libLastId = null;
  } else {
    // favorite/color/view can reorder or change row content → one rebuild.
    if (action === "addView" || action === "removeView") renderViews();
    renderLibrary();
  }
  updateBatchBar();
  return true;
}

// Undo for a batch tag add/remove: only the ids the original action actually
// touched, inverted, so a prompt that already carried the tag before the batch
// (never "changed") is left alone by the undo too.
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
    toast(String(err) === "save-failed" ? t("persistFailed") : String(err));
    return false;
  }
  undoToast(t("promptTrashedToast").replace("{name}", label), () => restoreBatchDelete([id], placements));
  return true;
}

// Undo for a batch delete: every deleted prompt went to the trash, not gone for good.
// The caller hands in the placement snapshot it took before deleting.
export async function restoreBatchDelete(ids, placements) {
  // With the placement snapshot taken before the delete, one command puts every
  // prompt back where it was (views, cells, pill, folders); without it (the snapshot
  // call failed) each one falls back to a fresh cell in the active view.
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

// The batch tag and archive actions, shared by the library bar and the grid bar:
// `ids` is the selection of whichever window asked, `run(action, extra)` is its own
// runner (doBatch or doGridBatch) and returns whether the batch went through.
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

// Asked only when the export set actually holds a confidential entry, so a
// store with none never sees this step. `null` means the whole export was
// cancelled here (never reaches export_prompts); otherwise a plain boolean,
// default false ("aus") matching the OK button/Enter key.
export async function confirmSecretExport(idFilter) {
  const n = prompts.filter((p) => p.secret && (!idFilter || idFilter.has(p.id))).length;
  if (!n) return false;
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
  const includeSecrets = await confirmSecretExport(new Set(ids));
  if (includeSecrets === null) return;
  try {
    const n = await invoke("export_prompts", { format, ids, includeSecrets });
    toast(t("exportDone").replace("{n}", String(n)));
  } catch (e) {
    if (String(e) !== "canceled") errToast(e);
  }
}

// Copy a prompt, filling {#{variables}#} first when applicable: reserved names
// resolve silently, the rest go through the dialog. Returns false if the copy
// failed or the user cancelled. The single copy entry point for every path that
// does not run the dialog's own double-click handling (grid, library, journal).
export async function copyResolved(p) {
  // Confidential and the vault has no key right now: ask for it before doing
  // anything else, the same first-access moment the spec describes for a
  // click on a locked tile. A cancelled/wrong-password prompt just fails the
  // copy, same as any other cancelled copy.
  if (p.locked) {
    if (!(await ensureSecretVaultUnlocked())) return false;
    await renderGrid(); // pulls the now-decrypted text into `prompts`
    const fresh = prompts.find((x) => x.id === p.id);
    if (!fresh || fresh.locked) return false;
    p = fresh;
  }
  // Rotating prompt: peek the entry copy_prompt would use, resolve variables
  // on THAT text (fixes {#{...}#} tokens, if any, in it), then commit only once the
  // copy actually reaches the clipboard, so a cancelled variable dialog never
  // burns an entry.
  if ((p.variants || []).length) {
    const peek = await invoke("rotation_peek", { id: p.id }).catch(() => null);
    const rawText = peek ? peek.text : p.text;
    const text = varTokens(p, rawText).length ? await resolveVars(rawText) : rawText;
    if (text === null || text === false) return false;
    // Handing the id over is what lets the backend open an auto-advance session for
    // an eligible prompt: without it this path copied plain text and the Auto
    // switch did nothing at all here, while the floating buttons (which copy through
    // copy_prompt) honoured it.
    const armId = p.rotate_auto && !anyEntryHasVars(p) ? p.id : undefined;
    const ok = await invoke("copy_text", { text, id: armId }).catch(() => false);
    // A failed peek already fell back to p.text above: committing anyway would
    // advance whatever the pointer's REAL position is, not the entry we actually
    // just copied. Only a successful peek may be followed by a commit.
    if (ok && peek) { invoke("rotation_commit", { id: p.id }).catch(() => {}); recordToolUsage("rotation_advance"); }
    return ok;
  }
  if (varTokens(p).length) {
    const text = await resolveVars(p.text);
    if (text === null) return false;
    return invoke("copy_text", { text }).catch(() => false);
  }
  return invoke("copy_prompt", { id: p.id }).catch((e) => { errToast(e); return false; });
}

// A blink says "something happened"; the character count says the whole prompt went
// to the clipboard, which is the doubt a long one leaves.
export function toastCopied(p) {
  const n = (p?.text || "").length;
  toast(n > 0 ? t("copiedChars").replace("{n}", n.toLocaleString(LANG)) : t("copied"));
}

export async function libraryCopy(p) {
  // Prompts that still need input skip the double-click shortcut: they must reach
  // the dialog (copyResolved) so the variables get asked before copy + auto-paste.
  if (!dialogTokens(p, await peekRotatedText(p)).length && autoPasteDouble(p.id)) { await autoPasteNow(null); return; } // 2nd fast click pastes
  if (copyOnCooldown(p.id)) return;
  if (!(await copyResolved(p))) return;
  recordCopy(p.id);
  toastCopied(p);
  if (flag("closeAfterCopy")) hideLibrary();
}

// Reflect the close-after-copy flag on the library header toggle.
export function syncLibraryToggle() {
  const btn = $("library-close-toggle");
  const on = flag("closeAfterCopy");
  btn.classList.toggle("active", on);
  btn.setAttribute("aria-pressed", String(on));
}

// Library list columns. ONE place decides it: the expert value. The segmented
// toggle in the library header was a third control for the same number, next to the
// flag that showed it and the value that capped it.
export const libCols = () => Math.min(6, Math.max(1, Math.round(val("libraryCols"))));
export function syncLibraryCols() {
  $("library-list").style.setProperty("--lib-cols", libCols());
}

export function bindLibrary() {
  buildColorFilter($("library-colors"), libColor);
  $("library-btn").addEventListener("click", async () => {
    await refreshLastEdited();
    await refreshClipInbox(); // decides whether the history tab is offered
    libQuery = "";
    $("library-q").value = "";
    // The type pick is remembered per window; search, colour and favourites are not.
    libTypes.clear();
    for (const type of typeFilterOf("libTypeFilter")) libTypes.add(type);
    paintTypeChips($("library-types"), libTypes);
    syncLibViewMode();
    setLibColor("all");
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
  // "Close after copy" toggle in the library header (mirrors the expert flag).
  $("library-close-toggle").addEventListener("click", async () => {
    const enabled = !flag("closeAfterCopy");
    settings.ui_flags = { ...(settings.ui_flags || {}), closeAfterCopy: enabled };
    try { await invoke("set_ui_flag", { key: "closeAfterCopy", enabled }); } catch (err) { errToast(err); }
    syncLibraryToggle();
  });
  const renderLibraryDebounced = debounced(renderLibrary, () => val("searchDebounceMs"));
  const refreshImageHitsDebounced = debounced(() => refreshImageHits(libQuery.trim().toLowerCase()), () => val("searchDebounceMs"));
  $("library-q").addEventListener("input", (e) => {
    libQuery = e.target.value;
    // With picture text search on and a query typed, the lookup renders once its
    // answer is in; rendering first as well would draw the whole library twice.
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
  $("library-colors").addEventListener("click", (e) => {
    const btn = e.target.closest(".lib-color");
    if (btn) { setLibColor(btn.dataset.color); renderLibrary(); }
  });
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
  // Only meaningful at exactly two selected (updateBatchBar disables it
  // otherwise) -- opens the same compare view a tile's own context menu does.
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
    if (e.key === "Escape" && libSelectMode && !libraryEl.classList.contains("hidden") && $("compare-modal").classList.contains("hidden")) {
      e.stopPropagation();
      setLibSelectMode(false);
    }
  }, true);
  // Grid batch toolbar wiring: same shape as the library's, its own state.
  // "All" means every prompt tile of the view on screen; folder tiles carry no
  // select button and stay out, exactly as a single click on them never selected.
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
    if (e.key === "Escape" && gridSelected.size && $("compare-modal").classList.contains("hidden")) {
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
  attachSelectPicker($("batch-export-format"));
  attachSelectPicker($("batch-export-quality"));
}
