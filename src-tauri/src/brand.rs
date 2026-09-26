// Product identity. GENERATED: run `node tools/brand.js <brand>` to change it;
// the values live in brands/<brand>.json. Nothing else in the Rust code may spell
// the product name.
//
// These strings decide where an installation keeps its data and how Windows knows
// the app. Changing one strands every existing install; brand_values_are_pinned in
// lib.rs asserts them for exactly that reason.

pub const DISPLAY_NAME: &str = "Prompt Saver";
pub const SLUG: &str = "prompt-saver";
pub const IDENTIFIER: &str = "com.bgcoding.promptsaver";
/// Store folder under %APPDATA%\Roaming. Deliberately not the bundle identifier.
pub const DATA_FOLDER: &str = "Prompt-Saver";
/// Value name under HKCU\...\Run.
pub const AUTOSTART_KEY: &str = "PromptSaver";
/// Sub-folder under %TEMP% for unpacked runtime files.
pub const TEMP_DIR: &str = "prompt-saver";
/// Leading part of loose temp files (snip previews, update staging).
pub const TEMP_PREFIX: &str = "promptsaver";
/// Win32 class of the hidden clipboard-watcher window.
pub const WINDOW_CLASS: &str = "PromptSaverClipWatch";
/// User-Agent for the update check and the WebView2 bootstrapper download.
pub const USER_AGENT: &str = "PromptSaver";
/// GitHub `owner/name` the update check is pinned to.
pub const REPO: &str = "wbgcoding/Prompt-Saver";

// Historical identifiers. Empty means this brand never had one, so the matching
// migration is skipped. They are frozen: a value here describes what an older
// version of THIS product wrote, so it can never be re-derived from the name.
/// Store folder an earlier version used under %APPDATA%.
pub const LEGACY_DATA_FOLDER: &str = "com.bgcoding.promptsaver";
/// Autostart value name an earlier version registered.
pub const LEGACY_AUTOSTART_KEY: &str = "";
/// File-name prefix of screenshots an earlier version saved.
pub const LEGACY_SCREENSHOT_PREFIX: &str = "PromptSaver-";

// What this brand calls the thing it stores. The backend only needs it for the few
// English messages it hands back to the UI; every translated string is handled by
// TERMS in ui/brand.js.
/// Singular, lowercase.
pub const ITEM: &str = "prompt";
/// Plural, lowercase.
pub const ITEM_PLURAL: &str = "prompts";
