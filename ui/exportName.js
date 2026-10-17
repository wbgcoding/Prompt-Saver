// Filename rule for the video export panel's "File name" field, applied to the stem only
// (video.js appends the format's extension). The Rust side (export_file_name_is_valid in
// video_editor.rs) re-checks the same rule on the dest_path that actually arrives, since
// this check only guards the live field, not the IPC call. Kept in its own tiny module so
// both can be read side by side without video.js's DOM/Tauri wiring.
export const EXPORT_NAME_BAD_CHARS = /[\\/:*?"<>|\x00-\x1f]/;

// Windows reserved device names, matched case-insensitively against the part
// of the stem before its first dot -- "CON.take2" is just as reserved as
// "CON" itself.
export const EXPORT_RESERVED_STEMS = new Set([
  "CON", "PRN", "AUX", "NUL",
  "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9",
  "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
]);

export function exportNameIsValid(name) {
  const trimmed = (name || "").trim();
  if (!trimmed || trimmed === "." || trimmed === "..") return false;
  if (EXPORT_NAME_BAD_CHARS.test(trimmed)) return false;
  // Trailing whitespace is already gone after the trim above; a trailing dot
  // survives it and Windows silently drops it from the real file name, so it
  // gets its own check here.
  if (trimmed.endsWith(".")) return false;
  // A bare ".." inside a longer name (e.g. "v1..2") cannot climb anywhere: it only means
  // "walk up" when set off by separators, which the exact "." / ".." checks above cover.
  const firstSegment = trimmed.split(".")[0].toUpperCase();
  return !EXPORT_RESERVED_STEMS.has(firstSegment);
}
