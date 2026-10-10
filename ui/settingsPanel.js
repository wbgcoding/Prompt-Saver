// Settings dialog: export and import, data wipe, scale presets, settings apply and reload.
import { $, appVersion, armButton, assignSettings, DISARM_MS, disarmButton, errText, errToast, flag, invoke, LANG, libraryEl, LOCK_SVG, modal, optFlag, setLang, setLangSelect, setStatusText, settings, settingsEl, SIZE_MAX, SIZE_MIN, SIZE_STEP, t, TAKEOVER_SVG, toast, val, withTaskbarBusy } from "./core.js";
import { confirmSecretExport, renderLibrary } from "./library.js";
import { favViewEnabled, refreshFavViewUi, renderGrid, seedExamplesIfNeeded } from "./views.js";
import { applyBars, applyI18n, wireClearButtons } from "./windowChrome.js";
import { applyTheme, applyTileStyle, fillThemeSelect, loadPalette, openThemeEditor, THEME_EDITOR_ENTRY } from "./theme.js";
import { fmtBytes } from "./floatShared.js";
import { confirmDialog, infoDialog } from "./dialog.js";
import { isHistoryTabOpen, renderJournal } from "./journal.js";
import { attachSelectPicker } from "./selectpop.js";
import { placePopover } from "./popover.js";
import { FONT_LABELS, FONTS } from "./fonts.js";
import { fitAllTiles, fitCache } from "./tiles.js";
import { prettyAccel } from "./hotkeys.js";
import { applyFlags, applyValues, refreshSettingsLocked, showExpertPage } from "./expertMenu.js";
import { maybeDupImportToast } from "./management.js";
import { openBackups } from "./backups.js";
import { withDialog } from "./editor.js";

export let deleteAllTimer = null;
export let resetSettingsTimer = null;

// Whatever had focus right before the settings/expert panel opened, so
// closing it returns focus there (same reasoning as modalOpener above).
export let settingsOpener = null;
export function assignSettingsOpener(v) { settingsOpener = v; }

// Hide the settings modal and reset the Updates status line back to the version
// number: unless a real update is pending (the "update available" state stays).
// So a transient "already newest version" message doesn't linger after reopening.
// The Import button's menu (from a file or from a folder).
export function closeImportMenu() {
  const pop = $("import-menu-pop");
  if (pop.classList.contains("hidden")) return;
  const hadFocus = pop.contains(document.activeElement);
  pop.classList.add("hidden");
  $("import-btn").setAttribute("aria-expanded", "false");
  if (hadFocus) $("import-btn").focus();
}
export function hideSettings() {
  settingsEl.classList.add("hidden");
  closeImportMenu();
  const uv = $("update-version");
  if (uv && appVersion && !uv.classList.contains("update-avail")) setStatusText(uv, "versionInstalled", "v", appVersion);
  settingsOpener?.focus?.();
  settingsOpener = null;
}

// ---- Settings actions ----
export async function runExport(format) {
  const includeSecrets = await confirmSecretExport(null);
  if (includeSecrets === null) return;
  try {
    await withDialog(() => invoke("export_prompts", { format, includeSecrets }));
  } catch (err) {
    if (String(err) !== "canceled") toast(t("exportFailed"));
  }
}

// Same shape as runExport, fed by the statistics dashboard's own format
// select + button instead of the settings export row. No success toast, same
// as runExport: the save dialog closing is confirmation enough.
export async function runStatsExport(format) {
  try {
    await withDialog(() => invoke("export_stats", { format }));
  } catch (err) {
    if (String(err) !== "canceled") toast(t("exportFailed"));
  }
}

// Shows what an import would do (new vs. skipped-as-duplicate) before it
// touches anything. Resolves the picked-file path if the user confirms, or
// null if they canceled anywhere along the way (no file picked, no items,
// or the preview itself dismissed).
export async function runImportPreview() {
  let preview;
  try {
    preview = await withDialog(() => invoke("preview_import"));
  } catch (err) {
    if (String(err) !== "canceled") toast(t("importFailed"));
    return null;
  }
  if (!preview) return null;
  const newCount = preview.items.filter((i) => !i.duplicate).length;
  const dupCount = preview.items.length - newCount;
  $("import-preview-summary").textContent = t("importPreviewSummary")
    .replace("{new}", newCount).replace("{dup}", dupCount);
  const list = $("import-preview-list");
  list.innerHTML = "";
  for (const item of preview.items) {
    const row = document.createElement("div");
    row.className = "import-preview-row" + (item.duplicate ? " dup" : "");
    const name = document.createElement("span");
    name.className = "import-preview-name";
    name.textContent = item.name;
    row.appendChild(name);
    if (item.duplicate) {
      const badge = document.createElement("span");
      badge.className = "import-preview-badge";
      badge.textContent = t("importPreviewDuplicate");
      row.appendChild(badge);
    }
    list.appendChild(row);
  }
  const modal = $("import-preview-modal");
  const confirmBtn = $("import-preview-confirm");
  confirmBtn.disabled = newCount === 0;
  modal.classList.remove("hidden");
  const confirmed = await new Promise((resolve) => {
    const done = (v) => {
      modal.classList.add("hidden");
      confirmBtn.removeEventListener("click", onOk);
      $("import-preview-cancel").removeEventListener("click", onCancel);
      $("import-preview-close").removeEventListener("click", onCancel);
      resolve(v);
    };
    const onOk = () => done(true);
    const onCancel = () => done(false);
    confirmBtn.addEventListener("click", onOk);
    $("import-preview-cancel").addEventListener("click", onCancel);
    $("import-preview-close").addEventListener("click", onCancel);
  });
  return confirmed ? { path: preview.path, hash: preview.hash } : null;
}
export async function runImport() {
  const picked = await runImportPreview();
  if (!picked) return;
  try {
    // No withDialog here: import_prompts re-reads the already-picked file, it
    // never opens a native dialog itself (that guard is preview_import's job).
    // hash is preview_import's digest of that same file -- import_prompts
    // refuses if the file changed underneath the preview.
    const count = await withTaskbarBusy("import", () => invoke("import_prompts", { path: picked.path, hash: picked.hash }));
    if (count == null) return;
    await renderGrid(); // refreshes prompts AND settings
    // Imported preferences apply on the spot: no restart needed.
    await setLang(settings.language);
    applyI18n();
    fillSizeSelects();
    fillFontSelects();
    setLangSelect();
    fillThemeSelect();
    $("tile-font").value = settings.tile_font || "system";
    $("tile-size").value = String(normSize(Number(settings.tile_size ?? 0)));
    fillScaleSelect("ui-scale", "uiScale");
    fillScaleSelect("icon-scale", "iconScale");
    $("opt-minimize").checked = settings.minimize_to_tray === true;
    $("opt-screenshot-folder").checked = optFlag("screenshotSave");
    $("opt-tooltips").checked = flag("tooltipsEnabled");
    $("opt-autopaste").checked = settings.ui_flags?.autoPaste !== false;
    $("opt-autoupdate").checked = settings.auto_update !== false;
    fillUpdateIntervalSelect();
    applyTheme(await invoke("current_theme"));
    applyTileStyle();
    applyBars();
    await renderGrid(true); // re-render with the new tile style
    toast(t("imported").replace("{n}", count.toLocaleString(LANG)));
    maybeDupImportToast(); // Flag near-duplicates the import may have introduced
  } catch (err) {
    if (String(err) === "import-changed") toast(t("importChanged"));
    else if (String(err) !== "canceled") toast(t("importFailed"));
  }
}

// Maps the backend's plain skip-reason codes to their translated row badge.
export const IMPORT_FOLDER_REASON_KEY = {
  "too-large": "importFolderReasonTooLarge",
  "bad-encoding": "importFolderReasonBadEncoding",
  "read-error": "importFolderReasonReadError",
};

// Same "review, then confirm" shape as runImportPreview, but for a whole
// folder of .md/.txt files instead of one export file: every readable item
// starts checked (a folder import is opt-out, not opt-in, since picking the
// folder at all already means "these are the notes I want"), tags/size/a
// per-row checkbox are shown, and an unreadable file is grayed out with why
// rather than silently dropped.
export async function runImportFolder() {
  let preview;
  try {
    preview = await withDialog(() => invoke("preview_import_folder", { title: t("importFolderTitle") }));
  } catch (err) {
    if (String(err) !== "canceled") toast(t("importFolderFailed"));
    return;
  }
  if (!preview) return;
  const modal = $("import-folder-modal");
  const list = $("import-folder-list");
  const summary = $("import-folder-summary");
  const confirmBtn = $("import-folder-confirm");
  const checks = new Map(); // relPath -> checkbox, readable items only
  const updateSummary = () => {
    const n = [...checks.values()].filter((cb) => cb.checked).length;
    let text = t("importFolderSummary").replace("{n}", n).replace("{total}", preview.items.length);
    if (preview.truncated) text += " " + t("importFolderTruncated");
    summary.textContent = text;
    confirmBtn.disabled = n === 0;
  };
  list.innerHTML = "";
  for (const item of preview.items) {
    const row = document.createElement("div");
    row.className = "import-preview-row" + (item.readable ? "" : " dup");
    if (item.readable) {
      const cb = document.createElement("input");
      cb.type = "checkbox";
      cb.checked = true;
      cb.addEventListener("change", updateSummary);
      checks.set(item.rel_path, cb);
      row.appendChild(cb);
    }
    const name = document.createElement("span");
    name.className = "import-preview-name";
    name.textContent = item.name;
    row.appendChild(name);
    for (const tag of item.tags) {
      const tagBadge = document.createElement("span");
      tagBadge.className = "import-preview-badge";
      tagBadge.textContent = tag;
      row.appendChild(tagBadge);
    }
    const sizeBadge = document.createElement("span");
    sizeBadge.className = "import-preview-badge";
    sizeBadge.textContent = fmtBytes(item.size, LANG);
    row.appendChild(sizeBadge);
    if (!item.readable) {
      const reasonBadge = document.createElement("span");
      reasonBadge.className = "import-preview-badge";
      reasonBadge.textContent = t(IMPORT_FOLDER_REASON_KEY[item.skip_reason] || "importFolderReasonReadError");
      row.appendChild(reasonBadge);
    }
    list.appendChild(row);
  }
  updateSummary();
  modal.classList.remove("hidden");
  const confirmed = await new Promise((resolve) => {
    const done = (v) => {
      modal.classList.add("hidden");
      confirmBtn.removeEventListener("click", onOk);
      $("import-folder-cancel").removeEventListener("click", onCancel);
      $("import-folder-close").removeEventListener("click", onCancel);
      $("import-folder-select-all").removeEventListener("click", onAll);
      $("import-folder-select-none").removeEventListener("click", onNone);
      resolve(v);
    };
    const onOk = () => done(true);
    const onCancel = () => done(false);
    const onAll = () => { for (const cb of checks.values()) cb.checked = true; updateSummary(); };
    const onNone = () => { for (const cb of checks.values()) cb.checked = false; updateSummary(); };
    confirmBtn.addEventListener("click", onOk);
    $("import-folder-cancel").addEventListener("click", onCancel);
    $("import-folder-close").addEventListener("click", onCancel);
    $("import-folder-select-all").addEventListener("click", onAll);
    $("import-folder-select-none").addEventListener("click", onNone);
  });
  if (!confirmed) return;
  const relPaths = [...checks.entries()].filter(([, cb]) => cb.checked).map(([rel]) => rel);
  if (!relPaths.length) return;
  try {
    const count = await withTaskbarBusy("import", () => invoke("import_folder_files", { dir: preview.dir, relPaths }));
    if (count == null) return;
    await renderGrid(true);
    toast(t("imported").replace("{n}", count.toLocaleString(LANG)));
    maybeDupImportToast();
  } catch {
    toast(t("importFolderFailed"));
  }
}

// "Everything" backup: prompts + all settings in one JSON file.
export async function runExportAll() {
  try {
    await withDialog(() => invoke("export_all"));
  } catch (err) {
    if (String(err) !== "canceled") toast(t("exportFailed"));
  }
}

export async function runImportAll() {
  const path = await withDialog(() => invoke("pick_backup_file")).catch(() => null);
  if (!path) return;
  // Same two checkboxes as restoreBackup, both preselected -- the dialog
  // itself refuses zero checked.
  const ok = await confirmDialog({
    title: t("backupImport"), message: t("importAllConfirm"), confirmLabel: t("backupImport"),
    checkboxes: [
      { key: "data", label: t("backupRestoreData"), checked: true },
      { key: "settings", label: t("backupRestoreSettings"), checked: true },
    ],
  });
  if (!ok) return;
  const dataOn = $("confirm-cb-data").checked;
  const settingsOn = $("confirm-cb-settings").checked;
  const scope = dataOn && settingsOn ? "all" : dataOn ? "data" : "settings";
  // The file stays picked while the password is retried, so a typo does not send
  // the user back through the file picker.
  let password = null;
  let asked = false;
  for (;;) {
    try {
      const res = await withTaskbarBusy("import", () => invoke("import_all", { path, password, scope }));
      await reloadEverything();
      let msg = t("importAllDone").replace("{f}", res.name).replace("{n}", Number(res.count).toLocaleString(LANG));
      // A hotkey the restore could not keep is named, never silently dropped.
      if (res.dropped_hotkeys) msg += "\n" + t("importHotkeysDropped").replace("{n}", Number(res.dropped_hotkeys).toLocaleString(LANG));
      // The file's own backup password is a foreign-machine DPAPI blob and was
      // discarded: say so, or a following export silently ships unprotected.
      if (res.backup_pw_dropped) msg += "\n" + t("importBackupPwKept");
      // Confidential entries left in the trash were sealed with the vault key this import replaced.
      if (res.vault_mismatch) msg += "\n" + t("importVaultMismatch");
      await infoDialog(t("backupImport"), msg);
      return;
    } catch (err) {
      const m = String(err);
      if (!/password/i.test(m)) {
        await infoDialog(t("importFailed"), errText(err)); // the reason, not just "failed"
        return;
      }
      password = await confirmDialog({
        title: t("backupPwTitle"),
        message: asked ? t("backupPwWrong") : t("backupPwBody"),
        confirmLabel: t("backupRestore"),
        cancelLabel: t("cancel"),
        icon: LOCK_SVG,
        input: { placeholder: t("backupPasswordLabel") },
        danger: false,
      });
      asked = true;
      if (password === null) return; // dismissed
    }
  }
}

// Portable start with an installed copy's store present: offer to adopt it. Runs
// after the window is up, so the dialog already has the theme and language.
// Returns whether it actually showed a dialog: the first-run tour must not
// open on top of (or right after burying) this more urgent prompt; it retries next
// launch instead, since tourSeen was never written this time.
export async function offerTakeover() {
  const offer = await invoke("takeover_offer").catch(() => null);
  if (!offer) return false;
  const countText = offer.prompts != null ? t("takeoverPromptCount").replace("{n}", offer.prompts) : t("takeoverCountUnknown");
  const details = `${fmtBytes(offer.size, LANG)}, ${countText}`;
  const adopt = await confirmDialog({
    title: t("takeoverTitle"),
    message: t("takeoverBody").replace("{path}", offer.path).replace("{details}", details).replace("{skip}", t("takeoverSkip")),
    confirmLabel: t("takeoverAdopt"),
    cancelLabel: t("takeoverSkip"),
    icon: TAKEOVER_SVG,
    danger: false,
    checkboxes: [{ key: "remember", label: t("takeoverRemember"), checked: false }],
    checkboxesOptional: true,
  });
  // Read regardless of how the dialog closed (Adopt, Skip, Escape or a
  // background click all leave the checkbox's own DOM state behind). The box starts
  // unticked: only an explicit tick settles the folder, otherwise it is offered
  // again next launch.
  const remember = $("confirm-cb-remember")?.checked === true;
  try {
    await invoke("takeover_apply", { adopt, remember });
  } catch (e) {
    errToast(e);
    return true;
  }
  if (adopt) {
    await reloadEverything();
    const newDir = await invoke("current_data_dir").catch(() => null);
    if (newDir) {
      toast(t("takeoverDoneBody").replace("{path}", newDir), { label: t("backupOpenFolder"), onClick: () => invoke("open_storage_folder", { path: newDir }).catch((e) => errToast(e)) }, "", null, 8000);
    }
  }
  return true;
}

// Delete all DATA (prompts, versions, copy history, clip inbox): settings kept.
export async function deleteData() {
  const btn = $("delete-data");
  if (!armButton(btn, t("deleteDataConfirm"))) {
    clearTimeout(deleteAllTimer);
    deleteAllTimer = setTimeout(() => disarmButton(btn, t("deleteData")), DISARM_MS);
    return;
  }
  clearTimeout(deleteAllTimer);
  disarmButton(btn, t("deleteData"));
  await maybeBackupBeforeWipe("delete");
  try {
    await invoke("delete_all_data");
  } catch (e) {
    // The in-memory store is already cleared either way (see reloadEverything
    // below) -- this means some ON-DISK table could not be, so the user must
    // still be told rather than believe the wipe fully succeeded.
    errToast(e);
  }
  await reloadEverything(); // in place: a page reload repaints the whole window
}

// Expert opt (Standard-an, split so a user who only wants one of the two can
// switch the other off). Take a safety backup before a destructive reset/delete.
export async function maybeBackupBeforeWipe(kind) {
  if (!flag(kind === "reset" ? "backupBeforeReset" : "backupBeforeDelete")) return;
  try { await invoke("create_backup", { keep: Math.round(val("backupKeep")) }); } catch { /* best-effort */ }
}

// Reset all SETTINGS to defaults (theme, views, toggles, expert values): prompts kept.
export async function resetSettings() {
  const btn = $("reset-settings");
  if (!armButton(btn, t("resetSettingsConfirm"))) {
    clearTimeout(resetSettingsTimer);
    resetSettingsTimer = setTimeout(() => disarmButton(btn, t("resetSettings")), DISARM_MS);
    return;
  }
  clearTimeout(resetSettingsTimer);
  disarmButton(btn, t("resetSettings"));
  await maybeBackupBeforeWipe("reset");
  await invoke("reset_settings");
  await reloadEverything(); // in place: a page reload repaints the whole window
}

export function bindSettingsPanel() {
  $("gear").addEventListener("click", () => {
    settingsOpener = document.activeElement;
    showExpertPage(false); // always open on the main page
    settingsEl.classList.remove("hidden");
  });

  $("settings-close").addEventListener("click", hideSettings);
  // pointerdown (not click): selecting text that ends outside an input must not close.
  settingsEl.addEventListener("pointerdown", (e) => { if (e.target === settingsEl) hideSettings(); });

  $("opt-minimize").addEventListener("change", (e) => {
    invoke("set_minimize_on_close", { enabled: e.target.checked }).catch((err) => errToast(err));
  });
  $("opt-screenshot-folder").addEventListener("change", (e) => {
    settings.ui_flags = { ...(settings.ui_flags || {}), screenshotSave: e.target.checked };
    invoke("set_ui_flag", { key: "screenshotSave", enabled: e.target.checked }).catch((err) => errToast(err));
  });
  // Master tooltip switch (default on): off = no hover tooltips anywhere.
  $("opt-tooltips").addEventListener("change", (e) => {
    settings.ui_flags = { ...(settings.ui_flags || {}), tooltipsEnabled: e.target.checked };
    invoke("set_ui_flag", { key: "tooltipsEnabled", enabled: e.target.checked }).catch((err) => errToast(err));
  });
  // Auto-paste (default on): normal setting; enter-after stays in the expert menu.
  $("opt-autopaste").addEventListener("change", (e) => {
    settings.ui_flags = { ...(settings.ui_flags || {}), autoPaste: e.target.checked };
    invoke("set_ui_flag", { key: "autoPaste", enabled: e.target.checked }).catch((err) => errToast(err));
  });
  // Recording copies from other apps is switched in the expert menu and in the copy history tab.
  $("opt-autostart").addEventListener("change", async (e) => {
    try {
      await invoke("set_autostart", { enabled: e.target.checked });
    } catch (err) {
      e.target.checked = !e.target.checked;
      errToast(err);
    }
    syncStartMinRow(); // a failed call flips the box back and re-hides the row
  });
  $("opt-startmin").addEventListener("change", async (e) => {
    try {
      await invoke("set_start_minimized", { enabled: e.target.checked });
    } catch (err) {
      e.target.checked = !e.target.checked;
      errToast(err);
    }
  });
  // Always-on-top: the header pin and the settings switch stay in sync; the
  // pin tints itself (accent) while active.
  const setOnTop = (on) => {
    settings.always_on_top = on;
    $("opt-ontop").checked = on;
    $("pin-top").classList.toggle("active", on);
    $("pin-top").setAttribute("aria-pressed", String(on));
    invoke("set_always_on_top", { enabled: on }).catch((err) => errToast(err));
  };
  $("opt-ontop").addEventListener("change", (e) => setOnTop(e.target.checked));
  $("pin-top").addEventListener("click", () => setOnTop(!settings.always_on_top));

  $("import-btn").addEventListener("click", () => {
    const pop = $("import-menu-pop");
    if (!pop.classList.contains("hidden")) { closeImportMenu(); return; }
    placePopover(pop, $("import-btn").getBoundingClientRect());
    $("import-btn").setAttribute("aria-expanded", "true");
    pop.querySelector("button")?.focus();
  });
  // Capture phase: any menu item closes the menu first, then runs its own listener.
  $("import-menu-pop").addEventListener("click", (e) => { if (e.target.closest("button")) closeImportMenu(); }, true);
  document.addEventListener("pointerdown", (e) => {
    const pop = $("import-menu-pop");
    if (pop.classList.contains("hidden")) return;
    if (pop.contains(e.target) || $("import-btn").contains(e.target)) return;
    closeImportMenu();
  });
  $("import-file-btn").addEventListener("click", runImport);
  $("import-folder-btn").addEventListener("click", runImportFolder);
  $("export-btn").addEventListener("click", () => runExport($("export-format").value));
  $("reset-settings").addEventListener("click", resetSettings);
  $("delete-data").addEventListener("click", deleteData);
  $("settings-backup-btn").addEventListener("click", openBackups);

  const themeSelect = $("theme-select");
  themeSelect.addEventListener("change", async () => {
    if (themeSelect.value === THEME_EDITOR_ENTRY) { fillThemeSelect(); openThemeEditor(); return; }
    settings.theme = themeSelect.value; // applyTheme reads the custom overrides from it
    applyTheme(await invoke("set_theme", { theme: themeSelect.value }));
  });

  // Language: persist, then re-render every translated string in place;
  // no restart, no reload.
  $("lang-select").addEventListener("change", async (e) => {
    await invoke("set_language", { lang: e.target.value });
    await setLang(e.target.value);
    applyI18n();
    // Re-fill the JS-built selects, then restore their current values.
    fillSizeSelects();
    fillUpdateIntervalSelect();
    fillFontSelects();
    fillThemeSelect(); // the custom-theme separator is a translated label too
    $("tile-font").value = settings.tile_font || "system";
    $("tile-size").value = String(normSize(Number(settings.tile_size ?? 0)));
    await renderGrid(); // fresh state: tooltips + a renamed default view
    if (!libraryEl.classList.contains("hidden")) renderLibrary();
    if (isHistoryTabOpen()) renderJournal();
  });

  // Prompt-tile font + size.
  const tileStyleChanged = async () => {
    settings.tile_font = $("tile-font").value;
    settings.tile_size = Number($("tile-size").value); // 0 = auto-fit
    applyTileStyle();
    invoke("set_tile_style", { font: settings.tile_font, size: settings.tile_size }).catch(() => {});
    await renderGrid(true); // re-render so auto-fit (or fixed size) applies cleanly
  };
  $("tile-font").addEventListener("change", tileStyleChanged);
  $("tile-size").addEventListener("change", tileStyleChanged);
  // Every dropdown uses the same popup style (scrollbar only when needed).
  attachSelectPicker($("tile-size"));

  attachSelectPicker($("tile-font"));

  attachSelectPicker($("theme-select"));
  attachSelectPicker($("lang-select"));

  attachSelectPicker($("export-format"));

  // UI size + icon size: percentage-preset selects mirroring the expert scales.
  fillScaleSelect("ui-scale", "uiScale");
  fillScaleSelect("icon-scale", "iconScale");
  const scaleChanged = (key) => async (e) => {
    const v = Number(e.target.value);
    settings.ui_values = { ...(settings.ui_values || {}), [key]: v };
    await invoke("set_ui_value", { key, value: v }).catch((err) => errToast(err));
    applyValues();
  };
  $("ui-scale").addEventListener("change", scaleChanged("uiScale"));
  $("icon-scale").addEventListener("change", scaleChanged("iconScale"));
  attachSelectPicker($("ui-scale"));
  attachSelectPicker($("icon-scale"));
}

// Fill a UI/icon size select with percentage presets (plus the current value if it
// was fine-tuned in the expert menu), then select the current value.
export const SCALE_PRESETS = [75, 90, 100, 110, 125, 150];
export function fillScaleSelect(id, key) {
  const sel = $(id);
  if (!sel) return;
  const cur = Math.round(val(key));
  sel.innerHTML = "";
  for (const p of [...new Set([...SCALE_PRESETS, cur])].sort((a, b) => a - b)) {
    const o = document.createElement("option");
    o.value = String(p);
    o.textContent = `${p} %`;
    sel.appendChild(o);
  }
  sel.value = String(cur);
}

// Interval select beside the Auto update switch. It only shows while the switch is on
// (the switch is the one way to turn checks off); a stored interval that is not on the
// list gets its own option.
export const UPDATE_INTERVALS = [[6, "updateEvery6h"], [12, "updateEvery12h"], [24, "updateDaily"], [48, "updateEvery2d"], [168, "updateWeekly"]];
export function fillUpdateIntervalSelect() {
  const sel = $("opt-update-interval");
  if (!sel) return;
  // A stored 0 or an out-of-range number shows as the interval the backend actually uses.
  const raw = Number(val("updateCheckH"));
  const cur = Number.isFinite(raw) ? Math.min(168, Math.max(1, raw)) : 24;
  const rows = UPDATE_INTERVALS.map(([h, key]) => [h, t(key)]);
  if (!rows.some(([h]) => h === cur)) rows.push([cur, `${cur}h`]);
  rows.sort((a, b) => a[0] - b[0]);
  sel.innerHTML = "";
  for (const [h, label] of rows) {
    const o = document.createElement("option");
    o.value = String(h);
    o.textContent = label;
    sel.appendChild(o);
  }
  sel.value = String(cur);
  sel.closest(".update-interval").classList.toggle("hidden", settings.auto_update === false);
}

// Fill both text-size selects: special options + 10..40 in steps of 2.
export function fillSizeSelects() {
  const fill = (sel, specials) => {
    sel.innerHTML = "";
    const add = (value, label) => {
      const o = document.createElement("option");
      o.value = value;
      o.textContent = label;
      sel.appendChild(o);
    };
    for (const [value, key] of specials) add(value, t(key));
    for (let s = SIZE_MIN; s <= SIZE_MAX; s += SIZE_STEP) add(String(s), s);
  };
  fill($("tile-size"), [["0", "langAuto"]]);
  fill(modal.sizeSel, [["0", "styleDefault"], ["1", "langAuto"]]);
  fill(modal.captionSize, [["0", "styleDefault"], ["1", "langAuto"]]);
}

// Fill both font selects from the shared catalog; every entry carries its own
// font family so the popup can preview it.
export function fillFontSelects() {
  const fill = (sel, withDefault) => {
    sel.innerHTML = "";
    if (withDefault) {
      const o = document.createElement("option");
      o.value = "";
      o.textContent = t("styleDefault");
      sel.appendChild(o);
    }
    for (const [key, stack] of Object.entries(FONTS)) {
      const o = document.createElement("option");
      o.value = key;
      o.textContent =
        FONT_LABELS[key] ?? t(key === "script" ? "fontScript" : "fontSystem");
      o.style.fontFamily = stack;
      sel.appendChild(o);
    }
  };
  fill($("tile-font"), false);
  fill(modal.fontSel, true);
}

// Snap legacy sizes (13/15/18/22) onto the new 10..40 grid.
export const normSize = (v) =>
  v <= 1 ? v : Math.min(SIZE_MAX, Math.max(SIZE_MIN, Math.round(v / 2) * 2));

// ---- Init ----
// Re-read settings from the backend and apply the global, order-sensitive parts
// (palette, language, sizing, theme). Shared by init() and reloadEverything() so a
// new global settings value only needs adding here, not in both places.
export async function applySettingsCore() {
  assignSettings(await invoke("get_settings"));
  loadPalette();
  await setLang(settings.language);
  applyI18n();
  fillSizeSelects();
  fillFontSelects();
  applyTheme(await invoke("current_theme").catch(() => settings.theme));
}

// The rest of applying loaded settings: flags, toolbar, the grid itself and the
// settings-dialog controls. Split from applySettingsCore() because init() has to
// wire event handlers in between; reloadEverything() calls both back to back.
export async function applySettingsUi() {
  applyFlags();
  applyBars();
  await renderGrid();
  syncSettingsControls();
}

// Push the loaded settings into the settings-dialog controls.
// "Start in the background" only means something while autostart is on. The
// stored choice stays untouched while the row is hidden.
export function syncStartMinRow() {
  $("opt-startmin").closest(".field").classList.toggle("hidden", !$("opt-autostart").checked);
}
export function syncSettingsControls() {
    fillThemeSelect();
    setLangSelect();
    $("opt-minimize").checked = settings.minimize_to_tray === true;
    $("opt-screenshot-folder").checked = optFlag("screenshotSave");
    $("opt-tooltips").checked = flag("tooltipsEnabled");
    $("opt-autopaste").checked = settings.ui_flags?.autoPaste !== false;
    $("opt-autostart").checked = settings.autostart === true;
    $("opt-startmin").checked = settings.start_minimized === true;
    syncStartMinRow();
    $("opt-ontop").checked = settings.always_on_top === true;
    $("opt-favview").checked = favViewEnabled();
    $("opt-favview").closest(".field").classList.toggle("hidden", !flag("favorites"));
    refreshFavViewUi();
    $("opt-hotkey").value = prettyAccel(settings.hotkey);
    $("opt-voice-hotkey").value = prettyAccel(settings.voice_hotkey);
    $("pin-top").classList.toggle("active", settings.always_on_top === true);
    $("pin-top").setAttribute("aria-pressed", String(settings.always_on_top === true));
    $("opt-autoupdate").checked = settings.auto_update !== false;
    fillUpdateIntervalSelect();
    $("tile-font").value = settings.tile_font || "system";
    $("tile-size").value = String(normSize(Number(settings.tile_size ?? 0)));
    applyTileStyle();
}

// Re-read the store and re-apply everything a full import replaced (theme, language,
// flags, views, prompts, controls). Reloading the page would do the same but repaints
// the whole window, which reads as a flicker.
export async function reloadEverything() {
  await refreshSettingsLocked(); // reset_settings/import_all may have just cleared the lock
  await applySettingsCore();
  await applySettingsUi(); // renderGrid() WITH the fetch: the prompts were replaced too
  // A wipe (delete_all_data) leaves the library empty, so the backend reseeds the
  // examples here; adopt/reset leave it non-empty, so this stays a no-op for them.
  const seeded = await seedExamplesIfNeeded();
  if (seeded && seeded.length) await renderGrid();
  wireClearButtons();
  fitCache.clear();
  fitAllTiles();
}
