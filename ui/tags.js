// Tags: editor chips, the type-ahead list, the colour palette, the library filter row
// and the tag manager.
import { $, assignSettings, COLORS, confirmIfNoUndo, errToast, flag, invoke, LANG, modalState, openColorPop, prompts, settings, t, toast, undoToast } from "./core.js";
import { placePopover } from "./popover.js";
import { libTags, renderLibrary } from "./library.js";
import { renderGrid } from "./views.js";
import { confirmDialog } from "./dialog.js";
import { buildSwatches } from "./editor.js";

// ---- Tags: chips in the editor, filter row + manager in the library ----
// Names are compared case-insensitively; the backend keeps one canonical casing
// and stores chip colors under the lowercase key, so a recased tag keeps its color.
export const tagsOn = () => flag("tags");
export const tagKey = (name) => String(name ?? "").trim().toLowerCase();
export const tagColor = (name) => settings.tag_colors?.[tagKey(name)] || "";
export const TAG_MAX_LEN = 60; // keep in sync with MAX_TAG_LEN in views.rs

// Every tag in the library, first-seen casing, sorted for a stable filter row.
// tagSort switches alphabetical (default) for usage-count order, ties broken
// alphabetically either way.
export function allTags() {
  const seen = new Map();
  for (const p of prompts) {
    for (const name of p.tags || []) {
      const key = tagKey(name);
      if (key && !seen.has(key)) seen.set(key, name);
    }
  }
  const names = [...seen.values()];
  if (settings.ui_texts?.tagSort === "usage") {
    const counts = tagUsage();
    names.sort((a, b) => (counts.get(tagKey(b)) || 0) - (counts.get(tagKey(a)) || 0) || a.localeCompare(b, LANG));
  } else {
    names.sort((a, b) => a.localeCompare(b, LANG));
  }
  return names;
}

export function tagUsage() {
  const counts = new Map();
  for (const p of prompts) {
    for (const key of new Set((p.tags || []).map(tagKey))) {
      counts.set(key, (counts.get(key) || 0) + 1);
    }
  }
  return counts;
}

// One chip. `as` picks the element so filter chips stay real buttons.
export function tagChipEl(name, as = "span") {
  const chip = document.createElement(as);
  chip.className = "tag-chip";
  if (as === "button") chip.type = "button";
  const color = tagColor(name);
  if (color) {
    chip.style.setProperty("--chip-color", color);
    chip.classList.add("tag-colored");
  }
  const label = document.createElement("span");
  label.className = "tag-chip-label";
  label.textContent = name;
  chip.append(label);
  return chip;
}

// Shared floating layers: the type-ahead list and the per-chip palette.
export const tagSuggest = document.createElement("div");

export const tagPop = document.createElement("div");

export const placePop = (el, anchor) => placePopover(el, anchor.getBoundingClientRect());
export const closeTagSuggest = () => tagSuggest.classList.add("hidden");
export const closeTagPop = () => tagPop.classList.add("hidden");

// Palette for one tag. Colors are global (settings), so this writes through at once.
export function openTagColorPop(anchor, name, after) {
  const box = tagPop.querySelector(".swatches");
  const apply = async (hex) => {
    try {
      assignSettings(await invoke("set_tag_color", { name, color: hex }));
    } catch (err) { errToast(err); return; }
    after?.();
  };
  buildSwatches(box, tagColor(name), (hex) => { closeTagPop(); apply(hex); },
    (swatch, cur) => openColorPop(swatch, cur, apply));
  placePop(tagPop, anchor);
}

// ---- Editor: assigned chips + type-ahead input ----
export function renderTagChips() {
  const box = $("modal-tag-chips");
  if (!box || !modalState) return;
  box.innerHTML = "";
  for (const name of modalState.tags || []) {
    const chip = tagChipEl(name);
    const dot = document.createElement("button");
    dot.type = "button";
    dot.className = "tag-dot";
    dot.title = t("tagColor");
    dot.addEventListener("click", (e) => {
      e.stopPropagation();
      openTagColorPop(dot, name, renderTagChips);
    });
    const del = document.createElement("button");
    del.type = "button";
    del.className = "tag-x";
    del.title = t("tagRemove");
    del.setAttribute("aria-label", t("tagRemove"));
    del.textContent = "×";
    del.addEventListener("click", () => {
      modalState.tags = modalState.tags.filter((n) => tagKey(n) !== tagKey(name));
      renderTagChips();
    });
    chip.prepend(dot);
    chip.append(del);
    box.appendChild(chip);
  }
  renderTagQuickAdd();
}

// The 3 most-used tags across the library, minus ones already on this
// prompt: click to add without typing.
export function renderTagQuickAdd() {
  const box = $("modal-tag-quickadd");
  if (!box || !modalState) return;
  box.innerHTML = "";
  if (!tagsOn()) { box.classList.add("hidden"); return; }
  const counts = tagUsage();
  const assigned = new Set((modalState.tags || []).map(tagKey));
  const top = allTags()
    .filter((n) => !assigned.has(tagKey(n)))
    .sort((a, b) => (counts.get(tagKey(b)) || 0) - (counts.get(tagKey(a)) || 0))
    .slice(0, 3);
  box.classList.toggle("hidden", !top.length);
  if (!top.length) return;
  const label = document.createElement("span");
  label.className = "tag-quickadd-label";
  label.textContent = t("tagQuickAddLabel");
  box.appendChild(label);
  for (const name of top) {
    const chip = tagChipEl(name, "button");
    chip.addEventListener("click", () => { addTagToModal(name); renderTagChips(); });
    box.appendChild(chip);
  }
}

// Add a typed name to the open editor. Returns false when it is unusable, so the
// input can keep the text instead of silently swallowing it.
export function addTagToModal(raw) {
  if (!modalState) return false;
  const name = String(raw || "").trim();
  if (!name || name.length > TAG_MAX_LEN || name.includes(",")) return false;
  const key = tagKey(name);
  if ((modalState.tags || []).some((n) => tagKey(n) === key)) return true; // already on
  // Join the casing the library already uses instead of forking a near-duplicate.
  const known = allTags().find((n) => tagKey(n) === key);
  modalState.tags = [...(modalState.tags || []), known || name];
  renderTagChips();
  return true;
}

// `all` ignores what is typed: that is what the chevron next to the field opens,
// so an existing tag can be picked without knowing its first letters.
export function renderTagSuggest(all = false) {
  const input = $("modal-tag-input");
  if (!input || !modalState) return;
  const q = all ? "" : input.value.trim().toLowerCase();
  const taken = new Set((modalState.tags || []).map(tagKey));
  const hits = allTags()
    .filter((n) => !taken.has(tagKey(n)) && (!q || tagKey(n).includes(q)))
    .slice(0, all ? 40 : 8);
  // Typing with no match just closes: a dropdown that keeps saying "nothing" while
  // the user is still typing a NEW tag is in the way.
  if (!hits.length && !all) { closeTagSuggest(); return; }
  tagSuggest.innerHTML = "";
  if (!hits.length) {
    // Opened by the handle with nothing to offer. This belongs IN the dropdown:
    // a toast for "there is nothing here yet" reads like an error and is gone
    // before it is understood.
    const empty = document.createElement("div");
    empty.className = "tag-suggest-empty";
    empty.textContent = t(allTags().length ? "tagAllTaken" : "tagNone");
    tagSuggest.appendChild(empty);
  }
  for (const name of hits) {
    const item = document.createElement("button");
    item.type = "button";
    item.className = "tag-suggest-item";
    item.append(tagChipEl(name));
    item.addEventListener("click", () => {
      addTagToModal(name);
      input.value = "";
      closeTagSuggest();
      input.focus();
    });
    tagSuggest.appendChild(item);
  }
  // A dropdown belongs to its field, so it takes the field's width.
  tagSuggest.style.minWidth = `${Math.round(input.getBoundingClientRect().width)}px`;
  placePop(tagSuggest, input);
}

export function wireTagInput() {
  const input = $("modal-tag-input");
  if (!input) return;
  const commit = () => {
    // A pasted "a, b, c" splits into chips rather than being rejected as one name.
    const parts = input.value.split(",");
    const rest = parts.filter((p) => !addTagToModal(p));
    input.value = rest.join(",").trim();
    if (input.value) toast(t("tagInvalid"));
    closeTagSuggest();
  };
  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter") { e.preventDefault(); commit(); return; }
    if (e.key === "Escape" && !tagSuggest.classList.contains("hidden")) {
      e.stopPropagation(); // keep Escape from closing the whole editor
      closeTagSuggest();
      return;
    }
    if (e.key === "Backspace" && !input.value && modalState?.tags?.length) {
      modalState.tags = modalState.tags.slice(0, -1);
      renderTagChips();
    }
  });
  input.addEventListener("input", () => {
    if (input.value.includes(",")) { commit(); return; }
    renderTagSuggest();
  });
  input.addEventListener("focus", () => renderTagSuggest());
  input.addEventListener("blur", () => setTimeout(closeTagSuggest, 150)); // let a click land first
  const pick = $("modal-tag-pick");
  pick?.addEventListener("mousedown", (e) => e.preventDefault()); // keep the field focused
  pick?.addEventListener("click", () => {
    if (!tagSuggest.classList.contains("hidden")) { closeTagSuggest(); return; }
    input.value = "";
    renderTagSuggest(true);
    input.focus();
  });
}

// ---- Library: filter chips + manager ----
// The bar (and every chip's click listener) doesn't change while the user is
// just typing a search query: skip rebuilding it unless the tag set or the active
// filter actually changed since the last render.
export let _lastTagFilterSig = null;
export function renderTagFilter() {
  const bar = $("library-tagbar");
  if (!bar) return;
  if (!tagsOn()) {
    bar.innerHTML = "";
    _lastTagFilterSig = null;
    libTags.clear();
    closeTagManager();
    return;
  }
  const names = allTags();
  // A tag renamed or deleted elsewhere must not keep filtering the list.
  const live = new Set(names.map(tagKey));
  for (const key of [...libTags]) if (!live.has(key)) libTags.delete(key);
  const sig = LANG + "|" + names.join("") + "" + [...libTags].sort().join("") + JSON.stringify(settings.tag_colors || {});
  if (sig === _lastTagFilterSig) return;
  _lastTagFilterSig = sig;
  renderTagBar(bar, libTags, renderLibrary);
}

// Shared markup for the library's tag filter bar (#library-tagbar): label + chips +
// manage tags button.
// Only the container, the active set and what to re-render differ per caller.
// "Manage tags" stays reachable even before the first tag exists -- the bar
// used to disappear entirely with nothing to filter by, hiding the only way
// to find out tags exist at all.
export function renderTagBar(bar, activeSet, rerender) {
  bar.innerHTML = "";
  const names = allTags();
  if (names.length) {
    // A bare row of chips reads as decoration; the label is what makes it a filter.
    const label = document.createElement("span");
    label.className = "lib-tagbar-label";
    label.textContent = t("tagFilterLabel");
    bar.appendChild(label);
    for (const name of names) {
      const chip = tagChipEl(name, "button");
      chip.classList.add("tag-filter");
      const on = activeSet.has(tagKey(name));
      chip.classList.toggle("active", on);
      chip.setAttribute("aria-pressed", String(on));
      chip.addEventListener("click", () => {
        const key = tagKey(name);
        if (activeSet.has(key)) activeSet.delete(key); else activeSet.add(key);
        rerender();
      });
      bar.appendChild(chip);
    }
    if (activeSet.size) {
      const clear = document.createElement("button");
      clear.type = "button";
      clear.className = "tag-action";
      clear.textContent = t("tagClearFilter");
      clear.addEventListener("click", () => { activeSet.clear(); rerender(); });
      bar.appendChild(clear);
    }
  }
  const manage = document.createElement("button");
  manage.type = "button";
  manage.className = "tag-action";
  manage.textContent = t("tagManager");
  manage.addEventListener("click", openTagManager);
  bar.appendChild(manage);
}

// Merging is just rename_tag with a target that already exists -- its own
// rename_tag_in dedupes per-prompt and its color rule (keep the target's color,
// only borrow the source's if the target has none) already matches what a merge
// needs. Shared by the explicit Merge button and a rename that collides with an
// existing tag. It runs at once: the snapshot taken here is what the undo toast
// puts back.
export async function mergeTagWithUndo(from, into, refresh) {
  const sure = await confirmIfNoUndo({
    title: t("tagMergeTargetTitle"),
    message: t("tagMergeConfirmMsg").replace("{from}", from).replace("{into}", into),
    confirmLabel: t("tagMerge"),
    danger: false,
  });
  if (!sure) return;
  const fromKey = tagKey(from);
  const intoKey = tagKey(into);
  const hasTag = (p, key) => (p.tags || []).some((x) => tagKey(x) === key);
  const fromIds = prompts.filter((p) => hasTag(p, fromKey)).map((p) => p.id);
  const intoIds = prompts.filter((p) => hasTag(p, intoKey)).map((p) => p.id);
  const fromColor = tagColor(from);
  const intoColor = tagColor(into);
  try { assignSettings(await invoke("rename_tag", { old: from, new: into })); }
  catch (err) { errToast(err); return; }
  await refresh();
  undoToast(
    t("tagMergeDone").replace("{from}", from).replace("{into}", into),
    () => undoTagMerge(from, into, fromIds, intoIds, fromColor, intoColor, refresh),
  );
}

// Undo for a merge: the source tag goes back onto its prompts, the target leaves
// the prompts that only got it through the merge, and the colours return to what
// they were (the merge lends the source's colour to a target that had none).
export async function undoTagMerge(from, into, fromIds, intoIds, fromColor, intoColor, refresh) {
  try {
    if (fromIds.length) {
      assignSettings(await invoke("batch_prompts", { ids: fromIds, action: "tag-add", tag: from, color: null, favorite: null, viewId: null }));
      const intoSet = new Set(intoIds);
      const onlyMerged = fromIds.filter((id) => !intoSet.has(id));
      if (onlyMerged.length) {
        assignSettings(await invoke("batch_prompts", { ids: onlyMerged, action: "tag-remove", tag: into, color: null, favorite: null, viewId: null }));
      }
    }
    if (fromColor) assignSettings(await invoke("set_tag_color", { name: from, color: fromColor }));
    if (tagColor(into) !== intoColor) assignSettings(await invoke("set_tag_color", { name: into, color: intoColor }));
  } catch (err) { errToast(err); }
  await refresh();
}

export function openTagManager() {
  $("tag-modal").classList.remove("hidden");
  renderTagManager();
}
export function closeTagManager() {
  $("tag-modal").classList.add("hidden");
  closeTagPop();
}

// Undo for a deleted tag: re-adds the tag text to every prompt that had it and
// restores its color, through the same batch/tag-color commands the rest of the
// tag UI uses: the same "rewrite through the store, not prompt by prompt" rule
// as the tag manager's own refresh().
export async function restoreDeletedTag(name, ids, color) {
  const idset = new Set(ids);
  if (ids.length) {
    try {
      assignSettings(await invoke("batch_prompts", { ids, action: "tag-add", tag: name, color: null, favorite: null, viewId: null }));
    } catch (err) { errToast(err); return; }
    const key = tagKey(name);
    for (const p of prompts) {
      if (idset.has(p.id) && !(p.tags || []).some((x) => tagKey(x) === key)) p.tags = [...(p.tags || []), name];
    }
  }
  if (color) {
    try { assignSettings(await invoke("set_tag_color", { name, color })); } catch (err) { errToast(err); }
  }
  await renderGrid();
  renderTagManager();
  renderLibrary();
}

export function renderTagManager() {
  const list = $("tag-modal-list");
  if (!list) return;
  list.innerHTML = "";
  const counts = tagUsage();
  const names = allTags();
  if (!names.length) {
    const empty = document.createElement("div");
    empty.className = "hint";
    empty.textContent = t("tagNone");
    list.appendChild(empty);
    return;
  }
  // One backend pass per action, then a full state refetch: a tag on hundreds of
  // prompts is rewritten in the store, never prompt by prompt from here.
  const refresh = async () => {
    await renderGrid();
    renderTagManager();
    renderLibrary();
  };
  for (const name of names) {
    const row = document.createElement("div");
    row.className = "tag-manage-row";
    const dot = document.createElement("button");
    dot.type = "button";
    dot.className = "tag-dot";
    dot.title = t("tagColor");
    const color = tagColor(name);
    if (color) dot.style.setProperty("--chip-color", color);
    dot.classList.toggle("tag-colored", !!color);
    dot.addEventListener("click", (e) => {
      e.stopPropagation();
      openTagColorPop(dot, name, refresh);
    });
    const label = document.createElement("span");
    label.className = "tag-manage-name";
    label.textContent = name;
    const count = document.createElement("span");
    count.className = "tag-manage-count";
    count.textContent = String(counts.get(tagKey(name)) || 0);
    count.title = t("tagUsageCount");
    const rename = document.createElement("button");
    rename.type = "button";
    rename.className = "ghost-btn";
    rename.textContent = t("tagRename");
    rename.addEventListener("click", async () => {
      const next = await confirmDialog({
        title: t("tagRenameTitle"),
        message: name,
        confirmLabel: t("save"),
        danger: false,
        input: { type: "text", value: name, placeholder: t("tagsPh") },
      });
      if (next == null || !next.trim()) return;
      const nextTrimmed = next.trim();
      if (nextTrimmed === name) return; // unchanged (a casing change is a rename)
      // Renaming onto an existing tag is really a merge. The name was typed on
      // purpose, so it runs at once and the undo toast covers a slip.
      const existing = tagKey(nextTrimmed) === tagKey(name) ? null : allTags().find((n2) => tagKey(n2) === tagKey(nextTrimmed));
      if (existing) { await mergeTagWithUndo(name, existing, refresh); return; }
      try { assignSettings(await invoke("rename_tag", { old: name, new: nextTrimmed })); }
      catch (err) { errToast(err); return; }
      await refresh();
    });
    const merge = document.createElement("button");
    merge.type = "button";
    merge.className = "ghost-btn";
    merge.textContent = t("tagMerge");
    merge.addEventListener("click", async () => {
      const others = allTags().filter((n2) => tagKey(n2) !== tagKey(name));
      const target = await confirmDialog({
        title: t("tagMergeTargetTitle"),
        message: t("tagMergeTargetMsg").replace("{name}", name).replace("{n}", String(counts.get(tagKey(name)) || 0)),
        confirmLabel: t("tagMerge"),
        danger: false,
        input: { type: "text", placeholder: t("tagsPh"), datalist: others },
      });
      const trimmed = (target || "").trim();
      if (!trimmed || tagKey(trimmed) === tagKey(name)) return;
      const existing = others.find((n2) => tagKey(n2) === tagKey(trimmed)) || trimmed;
      await mergeTagWithUndo(name, existing, refresh);
    });
    const del = document.createElement("button");
    del.type = "button";
    del.className = "ghost-btn danger-btn";
    del.textContent = t("delete");
    del.addEventListener("click", async () => {
      const ok = await confirmDialog({
        title: t("tagDeleteTitle"),
        message: t("tagDeleteMsg").replace("{name}", name),
        confirmLabel: t("delete"),
      });
      if (!ok) return;
      const key = tagKey(name);
      const hadIds = prompts.filter((p) => (p.tags || []).some((x) => tagKey(x) === key)).map((p) => p.id);
      const color = tagColor(name);
      try { assignSettings(await invoke("delete_tag", { name })); }
      catch (err) { errToast(err); return; }
      libTags.delete(key);
      for (const p of prompts) p.tags = (p.tags || []).filter((x) => tagKey(x) !== key);
      await refresh();
      undoToast(t("tagDeletedToast").replace("{name}", name), () => restoreDeletedTag(name, hadIds, color));
    });
    row.append(dot, label, count, rename, merge, del);
    list.appendChild(row);
  }
}

export function wireTags() {
  tagSuggest.className = "tag-suggest hidden";
  document.body.appendChild(tagSuggest);
  tagPop.className = "tag-pop hidden";
  tagPop.innerHTML = '<div class="swatches"></div>';
  document.body.appendChild(tagPop);
  document.addEventListener("pointerdown", (e) => {
    if (!tagSuggest.classList.contains("hidden") &&
        !tagSuggest.contains(e.target) && e.target !== $("modal-tag-input") &&
        !e.target.closest?.(".tag-pick")) closeTagSuggest();
    if (!tagPop.classList.contains("hidden") &&
        !tagPop.contains(e.target) && !e.target.closest(".tag-dot") && !e.target.closest(".color-pop")) closeTagPop();
  });
  wireTagInput();
  $("tag-modal-close")?.addEventListener("click", closeTagManager);
  // One click assigns the palette to every tag that has no color yet, in the
  // order the palette lists them: round-robin once past the end.
  $("tag-colorize")?.addEventListener("click", async () => {
    const palette = COLORS.slice(1);
    if (!palette.length) return;
    const uncolored = allTags().filter((n) => !tagColor(n));
    if (!uncolored.length) return;
    const entries = uncolored.map((name, i) => ({ name, color: palette[i % palette.length] }));
    try { assignSettings(await invoke("set_tag_colors", { entries })); }
    catch (err) { errToast(err); return; }
    renderTagManager();
    renderLibrary();
    await renderGrid();
  });
  $("tag-modal")?.addEventListener("pointerdown", (e) => {
    if (e.target === $("tag-modal")) closeTagManager();
  });
}
