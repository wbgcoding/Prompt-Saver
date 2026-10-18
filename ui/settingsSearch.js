// Settings search: matches a query against setting names in the UI language and in English,
// and opens a hit that lives outside the expert menu.
import { $, assignLibraryTab, DICT, gateOpen, settings, settingsEl } from "./core.js";
import { EXPERT_TIP_SET } from "./expertConfig.js";
import { hideSettings } from "./settingsPanel.js";
import EN from "./i18n/en.js";
import { openStats } from "./management.js";
import { openBackups } from "./backups.js";
import { showExpertPage } from "./expertMenu.js";

// A search matches if the term is in the active language OR in English: English
// is the lingua franca, so a non-native speaker's query still finds the setting.
export function i18nHit(key, q) {
  return (DICT[key] || "").toLowerCase().includes(q) || (EN[key] || "").toLowerCase().includes(q);
}
// Expert-menu search also matches a setting's tooltip text, not just its
// label: a setting is often findable only by a word that lives in the tip.
export function settingHit(settingKey, labelKey, q) {
  if (i18nHit(labelKey, q)) return true;
  if (!EXPERT_TIP_SET.has(settingKey)) return false;
  const tipKey = `tip${settingKey[0].toUpperCase()}${settingKey.slice(1)}`;
  return i18nHit(tipKey, q);
}

// A hand-maintained directory of every findable setting outside the expert menu: the normal
// settings page, the toolbar, the library dialog and the backup manager. The expert search
// covers EXPERT_TABS itself; this lets the same search surface a hit in these areas, badge it,
// and jump to it on click. `key` is the element's DOM id for "normal"/"toolbar"/"library"
// entries; for "backup" entries (built by renderBackupPanel(), otherwise id-less) it is the
// `data-setting-row` value stamped onto that row. tests/settings_directory_check.py
// cross-checks this list against the real markup.
export const SETTINGS_DIRECTORY = [
  { key: "theme-select", area: "normal", label: "theme" },
  { key: "lang-select", area: "normal", label: "language" },
  { key: "tile-font", area: "normal", label: "tileFont" },
  { key: "ui-scale", area: "normal", label: "valUiScale" },
  { key: "tile-size", area: "normal", label: "tileSize" },
  { key: "opt-ontop", area: "normal", label: "alwaysOnTop" },
  { key: "opt-minimize", area: "normal", label: "minimizeOnClose" },
  { key: "opt-autostart", area: "normal", label: "autostart" },
  { key: "opt-startmin", area: "normal", label: "startMinimized", when: () => $("opt-autostart").checked },
  { key: "opt-autopaste", area: "normal", label: "autoPasteSetting", tip: "tipAutoPaste" },
  { key: "opt-tooltips", area: "normal", label: "showTooltips", tip: "tipTooltips" },
  { key: "opt-screenshot-folder", area: "normal", label: "screenshotToFolder", tip: "tipScreenshotFolder" },
  { key: "opt-favview", area: "normal", label: "favViewTitle", tip: "favViewOption" },
  { key: "opt-hotkey", area: "normal", label: "hotkeyLabel" },
  { key: "opt-voice-hotkey", area: "normal", label: "voiceHotkeyLabel", gate: "voiceHotkey" },
  { key: "opt-autoupdate", area: "normal", label: "autoUpdate", tip: "autoUpdateHint" },
  { key: "opt-update-interval", area: "normal", label: "valUpdateCheckH", when: () => settings.auto_update !== false },
  { key: "quick-grid", area: "toolbar", label: "gridSize", gate: "quickGrid" },
  { key: "library-sort", area: "library", label: "sortBy" },
  { key: "library-close-toggle", area: "library", label: "flagCloseAfterCopy" },
  { key: "backup-auto", area: "backup", label: "flagAutoBackup" },
  { key: "backup-interval", area: "backup", label: "valBackupInterval", gate: "autoBackup" },
  { key: "backup-start-delay", area: "backup", label: "valBackupStartDelay", gate: "autoBackup" },
  { key: "backup-gfs", area: "backup", label: "flagBackupGfs", tip: "tipBackupGfs" },
  { key: "backup-keep", area: "backup", label: "valBackupKeep" },
  { key: "backup-daily", area: "backup", label: "valBackupDaily" },
  { key: "backup-weekly", area: "backup", label: "valBackupWeekly" },
  { key: "backup-monthly", area: "backup", label: "valBackupMonthly" },
  { key: "backup-yearly", area: "backup", label: "valBackupYearly" },
  { key: "backup-password", area: "backup", label: "backupPasswordLabel" },
  { key: "usage-stats", area: "stats", label: "statsEnable", tip: "tipUsageStats" },
];

export function settingsDirectoryHit(entry, q) {
  if (entry.when && !entry.when()) return false;
  if (!gateOpen(entry.gate)) return false;
  return i18nHit(entry.label, q) || (entry.tip && i18nHit(entry.tip, q));
}

// The DOM node to scroll to and flash for one directory entry. Only valid
// once that entry's own area is already open (see openSettingsDirectoryEntry).
export function settingsDirectoryTarget(entry) {
  if (entry.area === "backup") return document.querySelector(`#backup-modal [data-setting-row="${entry.key}"]`);
  if (entry.area === "stats") return document.querySelector(`#stats-modal [data-setting-row="${entry.key}"]`);
  const el = $(entry.key);
  return el ? (el.closest(".field") || el) : null;
}

export function flashSettingsRow(el) {
  if (!el) return;
  el.scrollIntoView({ block: "center" });
  el.classList.add("settings-search-hit");
  setTimeout(() => el.classList.remove("settings-search-hit"), 1800);
}

// A click on a foreign hit: close the expert menu, open whichever dialog/tab
// actually owns the row, then scroll it into view and flash it, so the
// setting that was found is the one the eye lands on.
export async function openSettingsDirectoryEntry(entry) {
  if (entry.area === "normal") {
    settingsEl.classList.remove("hidden");
    showExpertPage(false);
  } else if (entry.area === "toolbar") {
    hideSettings();
  } else if (entry.area === "library") {
    hideSettings();
    assignLibraryTab("prompts");
    $("library-btn").click();
  } else if (entry.area === "backup") {
    hideSettings();
    await openBackups();
  } else if (entry.area === "stats") {
    hideSettings();
    await openStats();
  }
  flashSettingsRow(settingsDirectoryTarget(entry));
}
