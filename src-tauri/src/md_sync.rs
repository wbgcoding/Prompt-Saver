use super::*;

// ---- Prompt folders as Markdown (opt-in expert feature, default off) ----
//
// A text-only prompt (no attached file/image/icon: those stay DB-only, binary
// content does not belong in a git-friendly text file) round-trips through one
// `<id>.md` per prompt in a chosen folder (default `<data dir>/<slug>-sync`). The id is
// the filename, so renaming a prompt in the app never orphans its file. Hooked at
// save_prompts(): the single choke point every prompt mutation already funnels
// through: so no individual command (add/update/delete/batch/import) needs to know
// this feature exists.
//
// Deliberately simplified vs. a fuller original ambition:
//   - polling (SYNC_POLL_SECS) instead of a filesystem-watcher crate: no new
//     dependency, and a few seconds of latency is fine for a background sync;
//   - no per-view position sync: a prompt imported from a new file lands in the
//     first free cell of the active view, exactly like a normal add;
//   - no hotkey sync: keeps a background task from re-running hotkey conflict/
//     registration logic, which has its own careful invariants elsewhere;
//   - conflict handling is "whichever side changed since the last successful sync
//     wins this cycle, the other is retried next cycle" rather than an interactive
//     merge dialog with per-conflict choices.
// None of these lose data outright: worst case a change waits one more poll
// interval: but this has NOT had the multi-instance crash-simulation stress test
// the original spec calls for as a "done" gate. Treat it as a beta feature.
pub(crate) const SYNC_POLL_SECS: u64 = 5;

// A lock is only ever held around one small file write, so 15 s is already far
// past any real one. The age is deliberately NOT tied to the poll interval: the
// second half of the staleness check is whether the process that wrote the lock
// is still alive, so a genuinely slow write by a running instance is never
// stolen however long it takes, and a lock left by a crashed one is picked up as
// soon as the age passes.
pub(crate) const SYNC_LOCK_STALE_SECS: u64 = 15;

// After this many consecutive failed rounds (an unreachable folder, a network
// share that is not mounted) the poll pauses instead of retrying every tick
// forever, says so once, and from then on only tries again at
// SYNC_PAUSED_FACTOR times the interval.
pub(crate) const SYNC_FAIL_MAX: u32 = 5;
pub(crate) const SYNC_PAUSED_FACTOR: u64 = 10;

pub(crate) fn sync_enabled(settings: &Settings) -> bool {
    settings.ui_flags.get("promptSync").copied().unwrap_or(false)
}

// The shipped folder, built from the brand slug rather than written out: the name
// lives in exactly one place, and a fork gets its own folder without a code change.
pub(crate) fn sync_dir_leaf(slug: &str) -> String {
    format!("{slug}-sync")
}

pub(crate) fn sync_default_dir(app: &AppHandle) -> PathBuf {
    data_dir(app).join(sync_dir_leaf(brand::SLUG))
}

// What earlier versions used before the folder was named after the brand. Adopted
// once for anyone who already syncs (sync_adopt_legacy_dir), never written to again.
pub(crate) const SYNC_LEGACY_DIR_NAME: &str = "Prompts";
// Set to "1" the first time the legacy folder was considered, so the adoption
// below can never run twice and move a folder the user has since changed.
pub(crate) const SYNC_LEGACY_MARK: &str = "syncDirLegacyChecked";

pub(crate) fn sync_dir_path(app: &AppHandle, settings: &Settings) -> PathBuf {
    let custom = settings.ui_texts.get("syncDir").cloned().unwrap_or_default();
    let resolved = if custom.is_empty() { sync_default_dir(app) } else { PathBuf::from(custom) };
    // Recorded for `append_crash_log`'s scrubbing, see `LAST_KNOWN_SYNC_DIR`.
    *LAST_KNOWN_SYNC_DIR.lock().unwrap_or_else(|e| e.into_inner()) = Some(resolved.clone());
    resolved
}

// Whether a folder holds at least one file this feature could have written. Used
// by the one-time legacy adoption, which must not claim an empty leftover folder.
pub(crate) fn sync_dir_has_md(dir: &Path) -> bool {
    let Ok(entries) = fs::read_dir(dir) else { return false };
    entries.flatten().any(|e| {
        let name = e.file_name().to_string_lossy().to_string();
        name != "AGENTS.md"
            && name.ends_with(".md")
            && Path::new(&name).file_stem().and_then(|s| s.to_str()).is_some_and(sync_safe_id)
    })
}

// The default folder changed name. Anyone who synced into the old one without ever
// picking a folder of their own would silently start over in an empty new one, so
// on the first start after the change their old folder becomes their explicit
// choice. Runs once (marked in ui_texts), only when no custom folder is set and the
// old folder actually holds our files. Returns the adopted path for the log line.
pub(crate) fn sync_adopt_legacy_dir(settings: &mut Settings, legacy: &Path) -> Option<String> {
    if settings.ui_texts.contains_key(SYNC_LEGACY_MARK) {
        return None;
    }
    settings.ui_texts.insert(SYNC_LEGACY_MARK.into(), "1".into());
    let custom = settings.ui_texts.get("syncDir").cloned().unwrap_or_default();
    if !custom.is_empty() || !sync_dir_has_md(legacy) {
        return None;
    }
    let path = legacy.to_string_lossy().into_owned();
    settings.ui_texts.insert("syncDir".into(), path.clone());
    Some(path)
}

// A media prompt syncs too, but only ever as a REFERENCE: the `file`/`icon` headers
// carry the paths the user picked, so the button keeps its name, colour, tags, notes
// and placement in step across machines while whoever set it up makes sure the paths
// resolve there. Embedded binary data (a pasted screenshot lives base64-encoded in
// `Prompt.image`) has no path to write and is never put in the file: it stays
// DB-only and survives untouched, since apply_md_to_prompt only ever overwrites the
// fields the header actually carries.
// A prompt whose id is not a plain, filename-safe token stays out entirely: the id
// becomes the FILE NAME here, and ids can arrive from an imported backup, i.e. from
// outside this app.
// A confidential entry never leaves an unencrypted trace outside the app, and
// the Markdown sync folder is a plain text file with no dialog moment to ask
// "encrypted or left out" the way a one-shot export has: so it is always left
// out, never written encrypted. An entry that was already synced and is later
// marked confidential drops out of `wanted` the same way a deleted prompt does,
// so sync_sweep_orphans removes the stale plaintext file on the next sync.
pub(crate) fn sync_eligible(p: &Prompt) -> bool {
    sync_safe_id(&p.id) && !p.secret
}

// Refused rather than escaped: the id is also how the inbound half finds the prompt
// again, so a rewritten name would silently orphan the file.
// A prompt written out as Markdown is text a person typed; 8 MB is far past any
// real one and still small enough that re-reading the folder on a timer costs
// nothing. A bigger file in the sync folder is simply not one of ours and is
// skipped instead of being pulled into memory every cycle.
pub(crate) const MAX_SYNC_MD_BYTES: u64 = 8 * 1024 * 1024;

// Text as an editor may have saved it: plain UTF-8, UTF-8 with a byte order mark, or
// UTF-16 (Windows PowerShell's default for `>`). Anything else is not text.
pub(crate) fn sync_decode_text(bytes: Vec<u8>) -> Option<String> {
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]).or_else(|| bytes.strip_prefix(&[0xFE, 0xFF])) {
        if rest.len() % 2 != 0 {
            return None;
        }
        let little = bytes[0] == 0xFF;
        let units: Vec<u16> = rest
            .chunks_exact(2)
            .map(|c| if little { u16::from_le_bytes([c[0], c[1]]) } else { u16::from_be_bytes([c[0], c[1]]) })
            .collect();
        return String::from_utf16(&units).ok();
    }
    let text = String::from_utf8(bytes).ok()?;
    Some(text.strip_prefix('\u{feff}').map(str::to_string).unwrap_or(text))
}

pub(crate) fn sync_safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

// Header fields are one line each: flattening a stray newline is simpler and safer
// than a real multi-line frontmatter parser for what are normally short values.
pub(crate) fn md_escape_line(s: &str) -> String {
    s.replace(['\n', '\r'], " ")
}

// Notes are typed into a multi-line box, so they are the one header field where a
// line break is real content. A note with one is written escaped (\n, \r, \\) and
// flagged with a `notes_enc` line; one without stays exactly as it always was, so
// older files and single-line notes (which may hold a path with backslashes) read
// back unchanged.
pub(crate) fn md_escape_notes(s: &str) -> Option<String> {
    if !s.contains(['\n', '\r']) {
        return None;
    }
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            _ => out.push(c),
        }
    }
    Some(out)
}

pub(crate) fn md_unescape_notes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

// What a prompt's synced .md file's placement header lines should say:
// derived from Settings (view.layouts / tile_folders / float_folders), never
// stored on Prompt itself. An empty field means "not applicable".
#[derive(Default)]
pub(crate) struct SyncPlacement {
    pub(crate) view: String,
    pub(crate) grid: String,
    pub(crate) cell: String,
    pub(crate) tile_folder: String,
    pub(crate) tile_folder_color: String,
    pub(crate) float_folder: String,
}

// A grid-folder member has no view.layouts entry of its own: save_tile_folder
// drops it the moment it joins: so its view/grid/cell describe the
// FOLDER's own placement instead. Every member's file ends up saying the same
// (redundant, but self-consistent and self-healing: sync_write_all_impl rewrites
// EVERY member's file the moment the folder moves, since prompt_to_md's output
// changes for each of them). Float-folder membership is a fully separate axis:
// a prompt can be a floating pill grouped into a pill-folder independently of
// whatever grid tile it also is: and only membership syncs there, never the
// folder's own desktop pixel position/scale, which is inherently per-machine
// (a coordinate valid on one monitor layout can be off-screen on another).
pub(crate) fn sync_placement_for(p: &Prompt, settings: &Settings) -> SyncPlacement {
    let float_folder = settings
        .float_folders
        .iter()
        .find(|f| f.members.iter().any(|m| m == &p.id))
        .map(|f| f.name.clone())
        .unwrap_or_default();
    if let Some(tf) = settings.tile_folders.iter().find(|f| f.members.iter().any(|m| m == &p.id)) {
        let view = settings.views.iter().find(|v| v.id == tf.view_id);
        return SyncPlacement {
            view: view.map(|v| v.name.clone()).unwrap_or_default(),
            grid: view.map(|v| grid_key(v.cols, v.rows)).unwrap_or_default(),
            cell: format!("{},{}", tf.cell[0], tf.cell[1]),
            tile_folder: tf.name.clone(),
            tile_folder_color: tf.color.clone(),
            float_folder,
        };
    }
    // Only the CURRENT grid size's saved layout: a view can hold one placement
    // per size it has ever been resized to, but only the one matching its live
    // cols/rows is "where the user sees it right now."
    for view in &settings.views {
        let key = grid_key(view.cols, view.rows);
        if let Some(cell) = view.layouts.get(&key).and_then(|m| m.get(&p.id)) {
            return SyncPlacement {
                view: view.name.clone(),
                grid: key,
                cell: format!("{},{}", cell[0], cell[1]),
                tile_folder: String::new(),
                tile_folder_color: String::new(),
                float_folder,
            };
        }
    }
    SyncPlacement {
        view: String::new(),
        grid: String::new(),
        cell: String::new(),
        tile_folder: String::new(),
        tile_folder_color: String::new(),
        float_folder,
    }
}

pub(crate) fn prompt_to_md(p: &Prompt, placement: &SyncPlacement) -> String {
    prompt_to_md_impl(p, placement, false)
}

// Whether a file's content is what this machine would write for the prompt, in
// today's format or in the one before notes and a trailing newline round-tripped
// (notes flattened to one line, no extra newline after a text that ends with one).
// Without the second form, every file an older version wrote would read as an edit
// by someone else right after the update.
pub(crate) fn sync_md_matches(p: &Prompt, placement: &SyncPlacement, content: &str) -> bool {
    prompt_to_md_impl(p, placement, false) == content || prompt_to_md_impl(p, placement, true) == content
}

pub(crate) fn prompt_to_md_impl(p: &Prompt, placement: &SyncPlacement, legacy: bool) -> String {
    let (notes, notes_enc) = match (legacy, md_escape_notes(&p.notes)) {
        (false, Some(escaped)) => (escaped, "notes_enc: escaped\n"),
        _ => (md_escape_line(&p.notes), ""),
    };
    let tags = p.tags.iter().map(|t| md_escape_line(t)).collect::<Vec<_>>().join(", ");
    // `variants` is informational only (a human glancing at the header sees the
    // entry count at a glance): NEVER read back by apply_md_to_prompt, which always
    // re-derives the real list from the body's own VARIANT_SEP markers. The four
    // REQUIRED fields (name/color/tags/notes, md_is_ours()) are unchanged; `variants`
    // and the six placement fields below (2) are additive, which md_is_ours()
    // already tolerates.
    let mut out = format!(
        "---\nname: {}\ncolor: {}\ntags: {}\nnotes: {}\n{}variants: {}\nview: {}\ngrid: {}\ncell: {}\ntile_folder: {}\ntile_folder_color: {}\nfloat_folder: {}\nfile: {}\nicon: {}\npdf_page: {}\n---\n",
        md_escape_line(&p.name),
        md_escape_line(&p.color),
        tags,
        notes,
        notes_enc,
        p.variants.len(),
        md_escape_line(&placement.view),
        placement.grid,
        placement.cell,
        md_escape_line(&placement.tile_folder),
        placement.tile_folder_color,
        md_escape_line(&placement.float_folder),
        md_escape_line(&p.file_path),
        md_escape_line(&p.icon_path),
        p.pdf_page,
    );
    let text = join_variants(&p.text, &p.variants);
    out.push_str(&text);
    // The newline closing the body always belongs to the file: a text that ends with
    // one gets a second, and apply_md_to_prompt takes exactly one off again.
    if !text.is_empty() && (!legacy || !text.ends_with('\n')) {
        out.push('\n');
    }
    out
}

// `None` means "not a file this feature wrote" (no opening `---` header): the
// caller leaves such files alone rather than guessing at a malformed one.
pub(crate) fn parse_frontmatter(content: &str) -> Option<(HashMap<String, String>, String)> {
    let content = content.strip_prefix('\u{feff}').unwrap_or(content);
    let mut lines = content.split('\n');
    if lines.next()?.trim_end_matches('\r') != "---" {
        return None;
    }
    let mut fields = HashMap::new();
    let mut body_lines: Vec<&str> = Vec::new();
    let mut in_header = true;
    for line in lines {
        if in_header {
            if line.trim_end_matches('\r') == "---" {
                in_header = false;
                continue;
            }
            if let Some(idx) = line.find(':') {
                fields.insert(line[..idx].trim().to_string(), line[idx + 1..].trim().to_string());
            }
        } else {
            body_lines.push(line);
        }
    }
    if in_header {
        return None; // closing --- never found
    }
    Some((fields, body_lines.join("\n")))
}

// A file only counts as ours when it carries the FULL header this feature writes.
// A user's own note (Obsidian, Jekyll, …) also opens with `---`; importing one and
// then rewriting it would destroy their own frontmatter fields.
pub(crate) fn md_is_ours(fields: &HashMap<String, String>) -> bool {
    ["name", "color", "tags", "notes"].iter().all(|k| fields.contains_key(*k))
}

pub(crate) fn apply_md_to_prompt(base: &Prompt, content: &str) -> Option<Prompt> {
    let (fields, body) = parse_frontmatter(content)?;
    if !md_is_ours(&fields) {
        return None;
    }
    let mut p = base.clone();
    if let Some(v) = fields.get("name") {
        p.name.clone_from(v);
    }
    if let Some(v) = fields.get("color") {
        p.color.clone_from(v);
    }
    if let Some(v) = fields.get("tags") {
        // Same rules every other way into the app enforces (length, character set,
        // how many per prompt): a hand-written file is an entry point like any
        // other, and a tag that breaks them would sit in the library unfixable.
        // Leniently: a bad tag is dropped, it never costs the whole file its text.
        let mut seen = std::collections::HashSet::new();
        p.tags = v
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && s.chars().count() <= MAX_TAG_LEN && tag_chars_ok(s))
            .filter(|s| seen.insert(tag_key(s)))
            .take(MAX_TAGS_PER_PROMPT)
            .collect();
    }
    if let Some(v) = fields.get("notes") {
        p.notes = if fields.contains_key("notes_enc") { md_unescape_notes(v) } else { v.clone() };
    }
    // Media by reference. Absent field = an older or hand-written file that does not
    // describe media at all: leave whatever the prompt already has. Present but
    // empty = "this button carries no file", which really does clear it. `image`
    // (embedded binary) is deliberately not in this list; nothing here can express it.
    if let Some(v) = fields.get("file") {
        p.file_path.clone_from(v);
    }
    if let Some(v) = fields.get("icon") {
        p.icon_path.clone_from(v);
    }
    if let Some(v) = fields.get("pdf_page") {
        // A garbled page number is ignored rather than reset to 0: page 0 is a valid
        // value, so guessing it would silently move the reader to the front page.
        if let Ok(n) = v.trim().parse::<u32>() {
            p.pdf_page = n;
        }
    }
    // The writer closes the body with a newline; keeping it out of the stored text is
    // what stops every round trip through the folder from adding another blank line.
    let body = body.strip_suffix('\n').unwrap_or(&body);
    // Entries 2..n ride along inside the body, split back out here: the
    // `variants:` header count above is never consulted, so a stale count left by a
    // hand-edit (added/removed an entry without touching the header) is harmless.
    // `rotate_mode` is deliberately NOT part of this sync (same partial-field
    // philosophy as favorite/hotkey/font, which this function also never touches).
    let (text, variants) = split_variants(body);
    // A file written by an older version still holds the old placeholder spelling.
    p.text = upgrade_placeholders(&text);
    p.variants = sanitize_variants(variants.iter().map(|v| upgrade_placeholders(v)).collect());
    // A hand-edit that removed every `<!-- next -->` marker must not leave a
    // "random" mode stranded on an empty list: the same invariant
    // sanitize_rotate_mode already enforces for every other entry point.
    p.rotate_mode = sanitize_rotate_mode(p.rotate_mode.clone(), &p.variants);
    p.rotate_auto = sanitize_rotate_auto(p.rotate_auto, &p.variants);
    Some(p)
}

// The incoming half of the six placement fields. Unlike
// apply_md_to_prompt (Prompt fields), these land on Settings: a view/folder is
// found by NAME (an id is meaningless across machines) and created here if this
// machine has never seen it. Every part below is independently best-effort, same
// "never lose data outright" philosophy as content sync: nothing here aborts the
// whole file over one malformed field. Returns (grid_placed, folders_touched):
// grid_placed tells the caller whether to skip today's "active view, first free
// cell" fallback (only when the grid fields were absent or unusable: an older
// version's file, a hand-created one, or a folder name this machine rejected);
// folders_touched tells it whether a tile/float folder was created, joined or
// LEFT, which needs a normalize pass: the two are independent, since a
// float_folder join/leave carries no grid fields at all.
//
// A field that is PRESENT but empty ("tile_folder: ") is a deliberate "not in a
// folder any more" from the writer: prompt_to_md always writes all six fields,
// so an empty one is never ambiguous with "this file predates the feature." That
// case must actively leave whatever folder this machine still has the prompt in,
// or a folder left on one machine reappears there forever: this machine's own
// next outgoing write would keep re-asserting the stale membership, and the
// machine that dropped it would re-join on the next poll.
pub(crate) fn apply_md_placement(settings: &mut Settings, prompt_id: &str, fields: &HashMap<String, String>) -> (bool, bool) {
    let mut folders_touched = false;
    if let Some(raw) = fields.get("float_folder") {
        let name = raw.trim();
        let touched = if name.is_empty() {
            sync_leave_float_folder(settings, prompt_id)
        } else {
            sync_join_float_folder(settings, prompt_id, name)
        };
        if touched {
            folders_touched = true;
        }
    }
    let (view_name, cols, rows, cell) = sync_parse_view_grid_cell(fields);
    if let Some(raw) = fields.get("tile_folder") {
        let name = raw.trim();
        if name.is_empty() {
            if sync_leave_tile_folder(settings, prompt_id) {
                folders_touched = true;
            }
        } else {
            let color = fields.get("tile_folder_color").map(|s| s.as_str()).unwrap_or("");
            let view_id = if view_name.trim().is_empty() {
                settings.active_view.clone()
            } else {
                sync_find_or_create_view(settings, view_name, cols, rows)
            };
            if sync_join_tile_folder(settings, prompt_id, name, color, &view_id, cell) {
                folders_touched = true;
                // The key is always written, so an empty value means the folder has no
                // colour on that machine: a cleared colour has to spread as well.
                if fields.get("tile_folder_color").is_some_and(|c| c.trim().is_empty()) {
                    if let Some(f) = settings.tile_folders.iter_mut().find(|f| f.members.iter().any(|m| m == prompt_id)) {
                        f.color.clear();
                    }
                }
                return (true, folders_touched);
            }
            // An unusable folder name (too long, control characters only) falls
            // through to plain grid placement below instead of leaving a
            // brand-new prompt with nowhere to land at all.
        }
    }
    if view_name.trim().is_empty() || cell.is_none() {
        return (false, folders_touched);
    }
    let view_id = sync_find_or_create_view(settings, view_name, cols, rows);
    sync_place_in_view(settings, prompt_id, &view_id, cell.unwrap());
    (true, folders_touched)
}

// Parses the "view" / "grid" / "cell" trio out of an incoming file's header. An
// unparsable `grid` falls back to the app's own default size (only matters when
// the view has to be created); an unparsable `cell` is None, not a wrong guess.
pub(crate) fn sync_parse_view_grid_cell(fields: &HashMap<String, String>) -> (&str, u32, u32, Option<[u32; 2]>) {
    let view = fields.get("view").map(|s| s.as_str()).unwrap_or("");
    let (cols, rows) = fields
        .get("grid")
        .and_then(|g| g.split_once('x'))
        .and_then(|(c, r)| Some((c.trim().parse().ok()?, r.trim().parse().ok()?)))
        .unwrap_or((default_cols(), default_rows()));
    let cell = fields
        .get("cell")
        .and_then(|c| c.split_once(','))
        .and_then(|(x, y)| Some([x.trim().parse().ok()?, y.trim().parse().ok()?]));
    (view, cols, rows, cell)
}

pub(crate) fn folder_cells_in_view(settings: &Settings, view_id: &str) -> Vec<[u32; 2]> {
    settings.tile_folders.iter().filter(|f| f.view_id == view_id).map(|f| f.cell).collect()
}

// Every id already living inside one of this view's grid folders -- these sit
// on the folder's own cell (see folder_cells_in_view), never in the view's own
// layout, so place_ids_in_view/view_capacity_deficit must not count them as
// needing a cell of their own.
pub(crate) fn folder_members_in_view(settings: &Settings, view_id: &str) -> std::collections::HashSet<String> {
    settings
        .tile_folders
        .iter()
        .filter(|f| f.view_id == view_id)
        .flat_map(|f| f.members.iter().cloned())
        .collect()
}

// The exact incoming cell wins when nothing ELSE sits there yet on THIS machine;
// otherwise the first free cell stands in, the same fallback a brand-new tile
// gets today. `prompt_id`'s OWN existing entry at that cell never counts as a
// collision: without this, re-importing a placement that already matches
// (e.g. only the prompt's text changed on the other machine) would read as
// "occupied by ourselves" and bounce the tile to a different free cell on every
// unrelated content edit. Bounds-checks against the view's OWN size, which may
// differ from the machine that sent the file: a view only takes its incoming
// grid size once, at creation (sync_find_or_create_view), never a forced resize
// afterward.
pub(crate) fn sync_resolve_cell(settings: &Settings, view_id: &str, prompt_id: &str, wanted: [u32; 2]) -> Option<[u32; 2]> {
    let view = settings.views.iter().find(|v| v.id == view_id)?;
    let folder_cells = folder_cells_in_view(settings, view_id);
    let key = grid_key(view.cols, view.rows);
    let fits = wanted[0] < view.cols
        && wanted[1] < view.rows
        && !folder_cells.contains(&wanted)
        && !view
            .layouts
            .get(&key)
            .map(|m| m.iter().any(|(pid, c)| pid != prompt_id && *c == wanted))
            .unwrap_or(false);
    if fits { Some(wanted) } else { first_free_cell_excluding(view, &folder_cells) }
}

// Mirrors the frontend's placeTile(): with multiViewPrompt off (default), a prompt lives in exactly
// ONE view. Without stripping the others here, a machine re-importing a placement
// that already matches (only the prompt's content changed elsewhere) left the
// prompt duplicated across every view it was ever placed in on ANY machine. With
// the flag on, multiple placements are legitimate and deliberately left alone.
pub(crate) fn sync_place_in_view(settings: &mut Settings, prompt_id: &str, view_id: &str, wanted: [u32; 2]) {
    let Some(cell) = sync_resolve_cell(settings, view_id, prompt_id, wanted) else { return };
    let multi_view = settings.ui_flags.get("multiViewPrompt") == Some(&true);
    if !multi_view {
        for v in settings.views.iter_mut() {
            if v.id != view_id {
                for layout in v.layouts.values_mut() {
                    layout.remove(prompt_id);
                }
                v.members.retain(|m| m != prompt_id);
            }
        }
    }
    let Some(view) = settings.views.iter_mut().find(|v| v.id == view_id) else { return };
    let key = grid_key(view.cols, view.rows);
    view.layouts.entry(key).or_default().insert(prompt_id.to_string(), cell);
    if !view.members.iter().any(|m| m == prompt_id) {
        view.members.push(prompt_id.to_string());
    }
}

// Finds a view by NAME or creates one at the incoming grid size (clamped like
// every other view creation). Ids never travel between machines: the name is the
// only thing an incoming file and a local view can agree on.
pub(crate) fn sync_find_or_create_view(settings: &mut Settings, name: &str, cols: u32, rows: u32) -> String {
    if let Some(v) = settings.views.iter().find(|v| v.name == name) {
        return v.id.clone();
    }
    if settings.views.len() >= MAX_VIEWS {
        return settings.active_view.clone();
    }
    let id = gen_id();
    settings.views.push(View {
        id: id.clone(),
        name: name.to_string(),
        cols: cols.clamp(GRID_MIN, GRID_MAX),
        rows: rows.clamp(GRID_MIN, GRID_MAX),
        layouts: HashMap::new(),
        color: String::new(),
        members: Vec::new(),
    });
    id
}

// Removes a prompt from every grid folder it happens to be in: the "leave" half
// of an explicit-empty tile_folder field, and the shared first step both a leave
// and a re-join take (moving folders is a leave-then-join). Returns whether it
// actually removed anything, so a no-op leave (never was a member) doesn't count
// as a change worth persisting.
pub(crate) fn sync_leave_tile_folder(settings: &mut Settings, prompt_id: &str) -> bool {
    let mut removed = false;
    for folder in settings.tile_folders.iter_mut() {
        let before = folder.members.len();
        folder.members.retain(|m| m != prompt_id);
        if folder.members.len() != before {
            removed = true;
        }
    }
    removed
}

pub(crate) fn sync_leave_float_folder(settings: &mut Settings, prompt_id: &str) -> bool {
    let mut removed = false;
    for folder in settings.float_folders.iter_mut() {
        let before = folder.members.len();
        folder.members.retain(|m| m != prompt_id);
        if folder.members.len() != before {
            removed = true;
        }
    }
    removed
}

// Finds a grid folder by NAME or creates one: every member's file redundantly
// carries the same name/color/place, so any one of them can recreate it. Mirrors
// save_tile_folder: pulled out of every OTHER folder first, and a folder member is
// not a tile of its own, so its own layout entry is dropped the moment it joins.
pub(crate) fn sync_join_tile_folder(settings: &mut Settings, prompt_id: &str, name: &str, color: &str, view_id: &str, cell: Option<[u32; 2]>) -> bool {
    let Ok(name) = folder_name_of(name) else { return false };
    if name.is_empty() {
        return false;
    }
    let color = valid_tag_color(color).unwrap_or_default();
    // Match on (name, view_id): a bare name match merges same-named folders
    // from DIFFERENT views into whichever one the scan happens to find first.
    let exists = settings.tile_folders.iter().any(|f| f.name == name && f.view_id == view_id);
    let incoming = cell;
    let wanted = cell.unwrap_or([0, 0]);
    let cell = if exists { wanted } else { sync_resolve_cell(settings, view_id, prompt_id, wanted).unwrap_or(wanted) };
    sync_leave_tile_folder(settings, prompt_id);
    // An existing folder is not moved or recoloured by a plain join, so two machines
    // that disagree about its cell or colour would rewrite every member file forever.
    // Adopt what the file says when this machine can: the cell only if it is free in
    // that view (the folder's own cell excluded), the colour only if it is a valid one.
    let mut adopt_cell = None;
    if let (true, Some(want)) = (exists, incoming) {
        let idx = settings.tile_folders.iter().position(|f| f.name == name && f.view_id == view_id);
        if let Some(i) = idx.filter(|&i| settings.tile_folders[i].cell != want) {
            let own = std::mem::replace(&mut settings.tile_folders[i].cell, [u32::MAX, u32::MAX]);
            if sync_resolve_cell(settings, view_id, prompt_id, want) == Some(want) {
                adopt_cell = Some(want);
            }
            settings.tile_folders[i].cell = own;
        }
    }
    match settings.tile_folders.iter_mut().find(|f| f.name == name && f.view_id == view_id) {
        Some(folder) => {
            folder.members.push(prompt_id.to_string());
            if let Some(c) = adopt_cell {
                folder.cell = c;
            }
            if !color.is_empty() {
                folder.color.clone_from(&color);
            }
        }
        None => settings.tile_folders.push(TileFolder {
            id: gen_id(),
            name,
            color,
            view_id: view_id.to_string(),
            cell,
            members: vec![prompt_id.to_string()],
        }),
    }
    let inside: Vec<String> = settings.tile_folders.iter().flat_map(|f| f.members.iter().cloned()).collect();
    for view in settings.views.iter_mut() {
        for layout in view.layouts.values_mut() {
            layout.retain(|pid, _| !inside.contains(pid));
        }
    }
    true
}

// Same for pill folders, but only ever joins a prompt that is ALREADY a floating
// pill on this machine (has a `floating` entry): a folder can group pills, but
// conjuring a brand-new OS pill window out of a background sync poll is exactly
// the kind of machine-specific side effect the sync deliberately keeps out of scope
// (position/monitor layout differ per machine, and so does whether the feature is
// even in use here).
pub(crate) fn sync_join_float_folder(settings: &mut Settings, prompt_id: &str, name: &str) -> bool {
    if !settings.floating.contains_key(prompt_id) {
        return false;
    }
    let Ok(name) = folder_name_of(name) else { return false };
    if name.is_empty() {
        return false;
    }
    sync_leave_float_folder(settings, prompt_id);
    match settings.float_folders.iter_mut().find(|f| f.name == name) {
        Some(folder) => folder.members.push(prompt_id.to_string()),
        None => settings.float_folders.push(FloatFolder {
            id: gen_id(),
            name,
            pos: Pos { x: 100, y: 100, ..Default::default() },
            scale: default_folder_scale(),
            members: vec![prompt_id.to_string()],
        }),
    }
    true
}

// The general normalize_tile_folders/normalize_float_folders dissolve any folder
// under 2 members: correct after a UI-driven removal (the user just took the
// last-but-one member out), but wrong straight after an INCOMING sync batch: a
// folder's members can legitimately arrive across SEPARATE poll cycles (a cloud
// provider propagates several file writes with no ordering or batching guarantee
// between machines). Dissolving a still-forming folder the moment it has only 1
// member means it can never reach 2 at all: the next member's file finds no
// folder to join, creates its own 1-member one, that gets dissolved too, forever.
// These run every other invariant the general versions do (drop unknown/duplicate
// members, drop a folder whose view no longer exists, resolve cell collisions,
// cap the count) but only dissolve at 0 members, not 1: a folder that genuinely
// never grows past 1 still gets cleaned up the next time a NON-sync path (any
// UI-driven folder command) touches it, since those keep calling the general,
// stricter version.
pub(crate) fn sync_normalize_tile_folders(folders: &mut Vec<TileFolder>, prompt_ids: &[String], view_ids: &[String]) {
    let mut taken: Vec<String> = Vec::new();
    for folder in folders.iter_mut() {
        // See normalize_tile_folders: an incoming sync file's `cell:` header is
        // exactly as untrusted as a hand-edited backup.
        folder.cell[0] = folder.cell[0].min(GRID_MAX);
        folder.cell[1] = folder.cell[1].min(GRID_MAX);
        folder.members.retain(|id| {
            let ok = prompt_ids.iter().any(|p| p == id) && !taken.contains(id);
            if ok {
                taken.push(id.clone());
            }
            ok
        });
    }
    folders.retain(|f| !f.members.is_empty() && view_ids.iter().any(|id| id == &f.view_id));
    let mut cells: Vec<(String, [u32; 2])> = Vec::new();
    for folder in folders.iter_mut() {
        for _ in 0..=GRID_MAX {
            if !cells.iter().any(|(v, c)| v == &folder.view_id && *c == folder.cell) {
                break;
            }
            folder.cell[0] = folder.cell[0].saturating_add(1);
        }
        cells.push((folder.view_id.clone(), folder.cell));
    }
    folders.truncate(MAX_TILE_FOLDERS);
}

pub(crate) fn sync_normalize_float_folders(folders: &mut Vec<FloatFolder>, prompt_ids: &[String]) {
    let mut taken: Vec<String> = Vec::new();
    for folder in folders.iter_mut() {
        folder.scale = if folder.scale.is_finite() { folder.scale.clamp(0.5, 3.0) } else { 1.0 };
        folder.members.retain(|id| {
            let ok = prompt_ids.iter().any(|p| p == id) && !taken.contains(id);
            if ok {
                taken.push(id.clone());
            }
            ok
        });
    }
    folders.retain(|f| !f.members.is_empty());
    folders.truncate(MAX_FLOAT_FOLDERS);
}

// The note's first line, which is also how this folder's own file is recognised
// again later (a note from an older version still counts as ours).
pub(crate) fn sync_agents_md_head() -> String {
    format!("# {} sync folder", brand::DISPLAY_NAME)
}

// Written into the user's own folder, so it obeys the fork rule like every other
// visible text: the product name and the word for what it stores both come from
// the brand, never from a literal here.
pub(crate) fn sync_agents_md() -> String {
    let head = sync_agents_md_head();
    let app = brand::DISPLAY_NAME;
    let item = brand::ITEM;
    format!(
        "{head}

Each `<id>.md` file here mirrors one {item}: a short header (`name`, `color`, `tags`,
`notes`) between two `---` lines, then the {item}'s text below. A {item} that carries a
file, an icon or a PDF page lists those as paths in the header - the files themselves
stay where they are.

Edit the text (or a header field) and save - {app} checks this folder every
few seconds and picks up the change. A file is read once it has stopped changing,
so an edit can take one extra round to arrive.

Do not rename a file; the filename is the {item}'s permanent id. A renamed file
arrives as a NEW {item} and the original is written back out under its own name,
so you end up with two.

Deleting a file here does not delete the {item}: it is written again from the app.
Delete the {item} in the app and its file goes with it. Anything in this folder
that {app} did not write itself is never changed and never deleted.

While a file is being written, a matching `<file>.md.lock` may briefly appear next
to it - leave it alone, it clears itself within seconds. If one lingers (e.g. after
a crash), it is safe to delete by hand. A `<id>.conflict-<number>.md` is a copy
{app} kept because the file and the {item} had both changed; nothing is lost, and
the file is yours to keep or delete.
"
    )
}

pub(crate) fn sync_write_agents_md(dir: &Path) {
    let path = dir.join("AGENTS.md");
    if !path.exists() {
        let _ = fs::write(&path, sync_agents_md());
    }
}

// ---- What each file in the folder looked like the last time it was seen ----
//
// Two problems share one answer. A file is only read when its modification time or
// size differs from the last round, which keeps a thousand-entry folder from being
// read end to end every few seconds; and a file whose stamp CHANGED since the last
// round is deliberately not read yet, because an editor that writes straight into
// the file (rather than the usual temp-file-and-rename) can be caught mid-write.
// One round later the stamp has settled and what is read is a whole file.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) struct SyncStamp {
    pub(crate) mtime_ns: u128,
    pub(crate) len: u64,
}

// A file the app wrote itself is recorded as already handled: its content is known,
// so the next round has nothing to read.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SyncSeen {
    pub(crate) stamp: SyncStamp,
    pub(crate) handled: bool,
    /// Hash of the file's bytes when the app wrote or read them, 0 when unknown. A
    /// cloud client re-touching the modification time does not change it.
    pub(crate) hash: u64,
}

pub(crate) fn sync_hash(bytes: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish().max(1)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum SyncScan {
    /// Changed (or never seen): remember the stamp and look again next round.
    Settle,
    /// Unchanged since the last round and not read yet: this one is safe to read.
    Read,
    /// Unchanged and already handled: nothing to do.
    Skip,
}

pub(crate) fn sync_scan_decision(prev: Option<SyncSeen>, now: SyncStamp) -> SyncScan {
    match prev {
        Some(p) if p.stamp == now => {
            if p.handled {
                SyncScan::Skip
            } else {
                SyncScan::Read
            }
        }
        _ => SyncScan::Settle,
    }
}

pub(crate) fn sync_stamp_of(path: &Path) -> Option<SyncStamp> {
    let meta = fs::metadata(path).ok()?;
    let mtime_ns = meta
        .modified()
        .ok()
        .and_then(|m| m.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    Some(SyncStamp { mtime_ns, len: meta.len() })
}

pub(crate) type SyncSeenMap = HashMap<String, SyncSeen>;
pub(crate) static SYNC_SEEN: std::sync::OnceLock<Mutex<SyncSeenMap>> = std::sync::OnceLock::new();

pub(crate) fn sync_seen() -> &'static Mutex<SyncSeenMap> {
    SYNC_SEEN.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(crate) fn sync_seen_lock() -> std::sync::MutexGuard<'static, SyncSeenMap> {
    sync_seen().lock().unwrap_or_else(|e| e.into_inner())
}

// Keyed by the whole path, never by the file name alone: the same `<id>.md` exists
// in the old and the new folder after a folder change, and a stamp from one of them
// must never be read as the other one's.
pub(crate) fn sync_seen_key(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

// Called right after the app writes a file itself: the content is ours and known,
// so the poll neither has to re-read it nor may mistake it for someone's edit.
#[cfg(test)]
pub(crate) fn sync_seen_record_own_write(path: &Path) {
    sync_seen_record_own_write_of(path, 0);
}

pub(crate) fn sync_seen_record_own_write_of(path: &Path, hash: u64) {
    let Some(stamp) = sync_stamp_of(path) else { return };
    sync_seen_lock().insert(sync_seen_key(path), SyncSeen { stamp, handled: true, hash });
}

// The files this process knows belong to a prompt of this library: written for one,
// or read back as one. Only these may ever be swept as orphans. A file whose id the
// app has never known (another machine's folder the user just pointed us at, a file
// made or renamed by hand, one that arrived a moment ago) is not ours to delete, even
// when it carries our header: the poll has to import it first.
pub(crate) static SYNC_KNOWN: std::sync::OnceLock<Mutex<std::collections::HashSet<String>>> = std::sync::OnceLock::new();

pub(crate) fn sync_known_lock() -> std::sync::MutexGuard<'static, std::collections::HashSet<String>> {
    SYNC_KNOWN
        .get_or_init(|| Mutex::new(std::collections::HashSet::new()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

pub(crate) fn sync_known_note(path: &Path) {
    sync_known_lock().insert(sync_seen_key(path));
}

// What the app last knew to be in each synced file: a hash of the content it
// rendered and the file's stamp right then. When the next render hashes the same
// and the file still carries that stamp, the file is known to hold exactly that
// content and is not read back at all. An outside edit changes the stamp, so it
// is never skipped.
pub(crate) type SyncWrittenMap = HashMap<String, (u64, SyncStamp)>;
pub(crate) static SYNC_WRITTEN: std::sync::OnceLock<Mutex<SyncWrittenMap>> = std::sync::OnceLock::new();

pub(crate) fn sync_written_lock() -> std::sync::MutexGuard<'static, SyncWrittenMap> {
    SYNC_WRITTEN.get_or_init(|| Mutex::new(HashMap::new())).lock().unwrap_or_else(|e| e.into_inner())
}

pub(crate) fn sync_content_hash(content: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    content.hash(&mut h);
    h.finish()
}

// True when `path` is known to already hold the content with this hash.
pub(crate) fn sync_file_known_current(path: &Path, hash: u64) -> bool {
    let Some(now) = sync_stamp_of(path) else { return false };
    sync_written_lock().get(&sync_seen_key(path)).is_some_and(|(h, stamp)| *h == hash && *stamp == now)
}

// Remember that `path` holds the content with this hash (call after a write, or
// after a read that found it already equal).
// A stamp younger than SYNC_STAMP_TRUST_NS is not trusted: on a file system with a
// coarse clock (FAT/exFAT/SMB) an outside edit of the same length inside the same tick
// would carry the very same stamp. Such a file is simply compared by content next round.
pub(crate) const SYNC_STAMP_TRUST_NS: u128 = 2_000_000_000;

pub(crate) fn sync_stamp_is_settled(stamp: &SyncStamp) -> bool {
    let now_ns = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    now_ns.saturating_sub(stamp.mtime_ns) >= SYNC_STAMP_TRUST_NS
}

pub(crate) fn sync_file_note_current(path: &Path, hash: u64) {
    if let Some(stamp) = sync_stamp_of(path) {
        if sync_stamp_is_settled(&stamp) {
            sync_written_lock().insert(sync_seen_key(path), (hash, stamp));
        } else {
            sync_written_lock().remove(&sync_seen_key(path));
        }
    }
}

// Whether the file changed since the app last wrote or read it. `None` (never seen)
// is not decided here: sync_edit_unsaved judges such a file by its content, so a
// fresh start does not litter the folder with copies of what the app wrote itself.
//
// A stamp the poll noted but has not read yet (handled: false) counts as changed
// too: the new stamp is already in the map, so comparing stamps alone would call an
// edit the app never saw "unchanged" and overwrite it without keeping a copy.
//
// With a recorded hash the bytes decide: a changed stamp alone (a cloud client
// touching the file, an identical rewrite by another instance) is no edit.
pub(crate) fn sync_changed_since_seen(path: &Path, on_disk: &[u8]) -> bool {
    let seen = sync_seen_lock();
    match (seen.get(&sync_seen_key(path)), sync_stamp_of(path)) {
        (Some(prev), Some(_)) if prev.hash != 0 => sync_hash(on_disk) != prev.hash,
        (Some(prev), Some(now)) => prev.stamp != now || !prev.handled,
        _ => false,
    }
}

// Whether the bytes about to be replaced hold something the app has not saved
// itself. A file that is not readable as text at all (an ANSI editor, a broken
// encoding) is always worth a copy: the poll cannot import it, so the file is the
// only place that version exists.
//
// A file the app has no record of at all (activation, a switch to a folder that
// already holds files, a save ahead of the startup seed) is judged by its content: a
// different one is somebody's edit, one equal to this prompt's own text, in today's
// form or an older one, is not.
// Writes `bytes` to a fresh `<id>.conflict-<secs>[-<n>]<tag>.md` in `dir`. The
// name is claimed with create_new first, so two conflicts in the same second
// (or an earlier copy) never overwrite each other.
pub(crate) fn sync_write_conflict_copy(dir: &Path, id: &str, tag: &str, bytes: &[u8]) -> Option<PathBuf> {
    let secs = now_secs();
    for n in 0..1000u32 {
        let suffix = if n == 0 { String::new() } else { format!("-{n}") };
        let copy = dir.join(format!("{id}.conflict-{secs}{suffix}{tag}.md"));
        match fs::OpenOptions::new().write(true).create_new(true).open(&copy) {
            Ok(_) => {
                if write_bytes_atomic(&copy, bytes).is_ok() {
                    return Some(copy);
                }
                let _ = fs::remove_file(&copy);
                return None;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return None,
        }
    }
    None
}

pub(crate) fn sync_edit_unsaved(path: &Path, on_disk: &[u8], p: &Prompt, placement: &SyncPlacement) -> bool {
    if on_disk.is_empty() {
        return false;
    }
    let Some(text) = sync_decode_text(on_disk.to_vec()) else { return true };
    if !sync_seen_lock().contains_key(&sync_seen_key(path)) {
        return !sync_md_matches(p, placement, &text);
    }
    // Seen but never known as a live prompt's file (it arrived while the prompt sat in
    // the trash and was only skipped): a restore must not overwrite it silently.
    if !sync_known_lock().contains(&sync_seen_key(path)) {
        return true;
    }
    sync_changed_since_seen(path, on_disk)
}

// ---- How the folder itself is doing (shown in the expert menu) ----
#[derive(Clone, Default)]
pub(crate) struct SyncHealth {
    /// Unix seconds of the last round that could read the folder.
    pub(crate) last_ok: u64,
    /// Files seen in that round.
    pub(crate) files: usize,
    pub(crate) fails: u32,
    pub(crate) paused: bool,
    /// Plain-language reason the poll is paused, for the status line.
    pub(crate) reason: String,
}

pub(crate) static SYNC_HEALTH: std::sync::OnceLock<Mutex<SyncHealth>> = std::sync::OnceLock::new();

pub(crate) fn sync_health() -> &'static Mutex<SyncHealth> {
    SYNC_HEALTH.get_or_init(|| Mutex::new(SyncHealth::default()))
}

pub(crate) fn sync_health_lock() -> std::sync::MutexGuard<'static, SyncHealth> {
    sync_health().lock().unwrap_or_else(|e| e.into_inner())
}

// Returns true exactly on the round that flips the pause on, so the log line is
// written once instead of on every failed round.
pub(crate) fn sync_health_note_fail(h: &mut SyncHealth, max: u32, reason: &str) -> bool {
    h.fails = h.fails.saturating_add(1);
    h.reason = reason.to_string();
    if h.paused || h.fails < max {
        return false;
    }
    h.paused = true;
    true
}

pub(crate) fn sync_health_note_ok(h: &mut SyncHealth, files: usize, now: u64) {
    h.fails = 0;
    h.paused = false;
    h.reason.clear();
    h.files = files;
    h.last_ok = now;
}

// Switching to a folder that was written fine ends whatever the old one caused: the
// pause and its reason describe a folder the app no longer uses.
pub(crate) fn sync_health_after_dir_change(h: &mut SyncHealth) {
    h.fails = 0;
    h.paused = false;
    h.reason.clear();
}

// Wakes the poll loop out of its (possibly stretched) sleep, so a new folder is read
// at once instead of after up to ten intervals.
pub(crate) static SYNC_WAKE: tokio::sync::Notify = tokio::sync::Notify::const_new();

// A paused poll still comes back, just far less often.
pub(crate) fn sync_wait_secs(paused: bool, poll: u64) -> u64 {
    if paused {
        poll.saturating_mul(SYNC_PAUSED_FACTOR)
    } else {
        poll
    }
}

pub(crate) fn sync_lock_path(md_path: &Path) -> PathBuf {
    let mut s = md_path.as_os_str().to_os_string();
    s.push(".lock");
    PathBuf::from(s)
}

pub(crate) fn pid_from_lock(lock_path: &Path) -> Option<u32> {
    fs::read_to_string(lock_path).ok()?.trim().parse().ok()
}

#[cfg(windows)]
pub(crate) fn pid_is_alive(pid: u32) -> bool {
    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> isize;
        fn CloseHandle(h: isize) -> i32;
    }
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h == 0 {
            false
        } else {
            CloseHandle(h);
            true
        }
    }
}
#[cfg(not(windows))]
pub(crate) fn pid_is_alive(_pid: u32) -> bool {
    true // conservative off Windows: never treat a lock as stale
}

pub(crate) fn sync_lock_is_stale(lock_path: &Path) -> bool {
    let Ok(meta) = fs::metadata(lock_path) else { return true }; // gone already
    let old_enough = meta
        .modified()
        .and_then(|m| m.elapsed().map_err(std::io::Error::other))
        .map(|d| d.as_secs() >= SYNC_LOCK_STALE_SECS)
        .unwrap_or(true);
    if !old_enough {
        return false;
    }
    match pid_from_lock(lock_path) {
        Some(pid) => !pid_is_alive(pid),
        None => true, // unreadable/garbage lock content: treat as abandoned
    }
}

// Atomic create (never "check then create": that would race between two
// instances). A held, non-stale lock means "leave this file for the next poll
// cycle", never a blocking wait: this runs in a background task with no UI to
// block, so retrying later is the safe choice over hanging the poll loop.
pub(crate) fn sync_lock_acquire(md_path: &Path) -> bool {
    let lock_path = sync_lock_path(md_path);
    let try_create = |p: &Path| {
        fs::OpenOptions::new().write(true).create_new(true).open(p).map(|mut f| {
            let _ = std::io::Write::write_all(&mut f, std::process::id().to_string().as_bytes());
        })
    };
    if try_create(&lock_path).is_ok() {
        return true;
    }
    if sync_lock_is_stale(&lock_path) {
        let _ = fs::remove_file(&lock_path);
        return try_create(&lock_path).is_ok();
    }
    false
}

pub(crate) fn sync_lock_release(md_path: &Path) {
    let _ = fs::remove_file(sync_lock_path(md_path));
}

// One writer at a time into the sync folder: the background flush, the activation
// write, a folder change and the inbound poll's file scan all take this around their
// file I/O, never the store lock. A slow share then only delays the next sync run;
// it can no longer freeze every window that wants the library.
pub(crate) static SYNC_RUN: Mutex<()> = Mutex::new(());

pub(crate) fn sync_run_lock() -> std::sync::MutexGuard<'static, ()> {
    SYNC_RUN.lock().unwrap_or_else(|e| e.into_inner())
}

// Split from the old sync_write_all(app, store) so the per-prompt file I/O
// below (a read to compare content, sometimes a write, once per eligible prompt)
// can run on owned data instead of a &Store borrow - every call site already
// holds the store mutex, and this loop's cost scales with the whole library on
// every single save. spawn_sync_write_all below clones what it needs and runs
// this on a blocking task after the lock is dropped; the one genuinely
// synchronous caller (activating the feature) calls this directly.
// Returns how many files could not be written. A sync folder that silently stays
// empty is the worst outcome this feature has: the user sees the switch on, the
// files never appear, and nothing anywhere says why. Every failure is logged, and
// the count lets the one caller the user is actually waiting on say so out loud.
pub(crate) fn sync_write_all_impl(app: &AppHandle, prompts: &[Prompt], settings: &Settings) -> usize {
    if !sync_enabled(settings) {
        return 0;
    }
    // See strip_data_dir_prefix's own comment: an attachment reference written
    // to a synced .md file must never carry an absolute demo-dir path either.
    let sync_source_dir = data_dir(app);
    let prompts = strip_data_dir_prefix(&sync_source_dir, data_dir_is_demo(app, &sync_source_dir), prompts);
    let prompts = prompts.as_ref();
    let dir = sync_dir_path(app, settings);
    // `prompts` is a snapshot and the write can take a while: right before a file is
    // swept, ask whether the library holds that id by now (the poll may have imported
    // it meanwhile). A store that is busy right now counts as "maybe", which only
    // postpones the sweep to the next write.
    let live = |id: &str| match app.try_state::<Db>() {
        Some(state) => match state.try_lock() {
            Ok(store) => store.prompts.iter().any(|p| p.id == id),
            Err(std::sync::TryLockError::Poisoned(e)) => e.into_inner().prompts.iter().any(|p| p.id == id),
            Err(std::sync::TryLockError::WouldBlock) => true,
        },
        None => true,
    };
    let _run = sync_run_lock();
    sync_write_dir_checked(&dir, prompts, settings, &live)
}

// The file half of sync_write_all_impl: one call writes every eligible prompt into
// `dir`, then sweeps what this process knows to be orphaned. Free of any window or
// state access so a test can run it against a temp folder.
#[cfg(test)]
pub(crate) fn sync_write_dir(dir: &Path, prompts: &[Prompt], settings: &Settings) -> usize {
    sync_write_dir_checked(dir, prompts, settings, &|_| false)
}

// `is_live` is asked right before a file would be swept: true means the library holds
// that id now, so the file is not an orphan after all.
pub(crate) fn sync_write_dir_checked(dir: &Path, prompts: &[Prompt], settings: &Settings, is_live: &dyn Fn(&str) -> bool) -> usize {
    let dir = dir.to_path_buf();
    if let Err(e) = fs::create_dir_all(&dir) {
        append_crash_log(&format!("prompt-sync: the sync folder could not be created: {e}"));
        return prompts.iter().filter(|p| sync_eligible(p)).count().max(1);
    }
    sync_write_agents_md(&dir);
    let mut failed = 0usize;
    let mut wanted: std::collections::HashSet<String> = std::collections::HashSet::new();
    for p in prompts {
        if !sync_eligible(p) {
            continue;
        }
        let filename = format!("{}.md", p.id);
        let path = dir.join(&filename);
        let placement = sync_placement_for(p, settings);
        let content = prompt_to_md(p, &placement);
        let hash = sync_content_hash(&content);
        if sync_file_known_current(&path, hash) {
            sync_known_note(&path);
            wanted.insert(filename);
            continue; // unchanged since it was last written: no read, no compare
        }
        // Raw bytes, never decoded text: a UTF-16 or ANSI file would read as empty
        // through read_to_string and be overwritten without a trace.
        let on_disk = fs::read(&path).unwrap_or_default();
        if on_disk == content.as_bytes() {
            sync_file_note_current(&path, hash);
        } else if sync_lock_acquire(&path) {
            // Both sides changed: the file was edited outside the app since the app
            // last touched it, and the prompt in the app has since changed too. The
            // app's version wins the file, but the one being replaced is kept beside
            // it instead of disappearing. A conflict is rare, a silent loss is not
            // acceptable. The copy's name carries a dot, so it is neither a valid
            // prompt id (the poll skips it) nor something the orphan sweep may touch.
            if sync_edit_unsaved(&path, &on_disk, p, &placement) {
                if let Some(copy) = sync_write_conflict_copy(&dir, &p.id, "", &on_disk) {
                    append_crash_log(&format!(
                        "prompt-sync: {filename} was edited outside the app and in the app at the same time; the outside version was kept as {}",
                        copy.file_name().unwrap_or_default().to_string_lossy()
                    ));
                }
            }
            // One more look right before the replace: the lock file is only ours, an
            // external editor never honors it, so a save that landed after the first
            // read has to end up in the conflict copy as well, not be overwritten.
            let latest = fs::read(&path).unwrap_or_default();
            if latest != on_disk && latest != content.as_bytes() && !latest.is_empty() {
                if let Some(late_copy) = sync_write_conflict_copy(&dir, &p.id, "-late", &latest) {
                    append_crash_log(&format!(
                        "prompt-sync: {filename} was edited outside the app while it was being written; the outside version was kept as {}",
                        late_copy.file_name().unwrap_or_default().to_string_lossy()
                    ));
                }
            }
            let ok = write_file_atomic(&path, &content);
            sync_lock_release(&path);
            if ok {
                sync_seen_record_own_write_of(&path, sync_hash(content.as_bytes()));
                sync_file_note_current(&path, hash);
            } else {
                failed += 1;
                append_crash_log(&format!("prompt-sync: writing {filename} failed"));
            }
        }
        sync_known_note(&path);
        wanted.insert(filename);
    }
    sync_sweep_orphans_checked(&dir, &wanted, is_live);
    // Entries for files that are gone (deleted prompts, swept orphans) go too.
    sync_written_lock().retain(|key, _| Path::new(key).exists());
    failed
}

// A queue rather than one independent spawn_blocking task per call - two rapid
// saves used to race as two unrelated tasks, and whichever finished LAST won even
// if it started from the OLDER snapshot, silently reverting the newer save's files
// (and letting the orphan sweep delete/recreate files based on that stale state).
// Only a dirty flag is queued, not a data snapshot: the worker re-fetches prompts/
// settings fresh right before every write. A second review round of this fix
// caught that queuing a snapshot at save time let an already-queued write still
// land after the user disabled the feature (the snapshot's OWN captured settings
// still read "enabled"); re-checking the live settings immediately before writing
// closes that gap, and also means the actual file content written is never staler
// than it has to be even if the worker was mid-write for a while.
pub(crate) struct SyncWriteQueue(pub(crate) Mutex<SyncWriteState>);
#[derive(Default)]
pub(crate) struct SyncWriteState {
    pub(crate) pending: bool,
    pub(crate) active: bool,
}

// Called from save_prompts()/save_prompts_and_trash() after every successful
// persist, all of which already hold the store mutex - this only ever sets two
// bools under a brief lock, never touching the filesystem or cloning the library
// itself, so it keeps that mutex from being held for however long a large or slow
// sync folder takes to read and compare.
pub(crate) fn spawn_sync_write_all(app: &AppHandle, store: &Store) {
    if !sync_enabled(&store.settings) {
        return; // common case (feature off): nothing to queue
    }
    let Some(q) = app.try_state::<SyncWriteQueue>() else { return };
    let should_spawn = {
        let mut s = q.0.lock().unwrap_or_else(|e| e.into_inner());
        s.pending = true;
        if s.active {
            false // a worker is already draining the queue and will pick this up
        } else {
            s.active = true;
            true
        }
    };
    if !should_spawn {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        // Fetch the queue state ONCE for this task's whole lifetime rather than
        // re-fetching every loop iteration: a `try_state` miss mid-loop used to
        // `return` without ever clearing `active`, which permanently latches
        // `should_spawn` to false above and disables Markdown sync writes for the
        // rest of the process. `Db` state (below) is refetched per-iteration on
        // purpose -- only this queue's own liveness needs to be pinned down.
        let Some(q) = app.try_state::<SyncWriteQueue>() else { return };
        loop {
            {
                // Check-and-clear (or check-and-go-inactive) as ONE critical section -
                // a deposit between "queue looked empty" and "mark inactive" would
                // otherwise sit unprocessed until some later unrelated save.
                let mut s = q.0.lock().unwrap_or_else(|e| e.into_inner());
                if !s.pending {
                    s.active = false;
                    return;
                }
                s.pending = false;
            }
            let Some(state) = app.try_state::<Db>() else { continue };
            let job = {
                let store = lock(&state);
                if !sync_enabled(&store.settings) {
                    None // disabled since this was queued - nothing to write
                } else {
                    Some((store.prompts.clone(), store.settings.clone()))
                }
            };
            if let Some((prompts, settings)) = job {
                let app2 = app.clone();
                let _ = tauri::async_runtime::spawn_blocking(move || {
                    // Background flush: no one is waiting on it, so a failure goes
                    // to the crash log inside and the next save tries again.
                    let _ = sync_write_all_impl(&app2, &prompts, &settings);
                })
                .await;
            }
        }
    });
}

// A prompt that left the library (deleted, trashed, or given an id this feature
// refuses) takes its file with it: otherwise a stale file would come back as a "new"
// prompt on the next inbound poll. ONLY files this feature could have written are
// swept: the sync folder is a directory the USER picks, so it can hold their own
// documents. A document without our `---` frontmatter is never imported by
// sync_poll_once either, so deleting it would destroy data the app does not own.
#[cfg(test)]
pub(crate) fn sync_sweep_orphans(dir: &Path, wanted: &std::collections::HashSet<String>) {
    sync_sweep_orphans_checked(dir, wanted, &|_| false);
}

pub(crate) fn sync_sweep_orphans_checked(dir: &Path, wanted: &std::collections::HashSet<String>, is_live: &dyn Fn(&str) -> bool) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if !name.ends_with(".md") || name == "AGENTS.md" || wanted.contains(&name) {
            continue;
        }
        // Only a name this feature could have written itself, i.e. one that is a
        // valid prompt id. A copy the user made of one of our files ("notes copy.md",
        // "plan (1).md") carries our header too, and deleting that as an orphan would
        // destroy a file the user deliberately put there.
        if !Path::new(&name).file_stem().and_then(|s| s.to_str()).is_some_and(sync_safe_id) {
            continue;
        }
        let path = e.path();
        // Never a file the app has not known before (see SYNC_KNOWN).
        let key = sync_seen_key(&path);
        if !sync_known_lock().contains(&key) {
            continue;
        }
        // Nor one with an edit the app has noted but not read back: it changed since
        // the app last wrote or read it, so it is somebody's work, and the poll will
        // read it first.
        let seen = sync_seen_lock().get(&key).copied();
        if !matches!((seen, sync_stamp_of(&path)), (Some(s), Some(now)) if s.handled && s.stamp == now) {
            continue;
        }
        let ours = read_text_capped(&path, MAX_SYNC_MD_BYTES)
            .and_then(|c| parse_frontmatter(&c))
            .is_some_and(|(fields, _)| md_is_ours(&fields));
        let stem = name.trim_end_matches(".md");
        if ours && !is_live(stem) {
            let _ = fs::remove_file(&path);
            sync_known_lock().remove(&sync_seen_key(&path));
        }
    }
}

pub(crate) fn sync_blank_prompt(id: String) -> Prompt {
    Prompt {
        id,
        trashed_at: 0,
        name: String::new(),
        text: String::new(),
        color: String::new(),
        image: String::new(),
        show_image: false,
        copy_image: false,
        file_path: String::new(),
        pdf_page: 0,
        icon_path: String::new(),
        caption: String::new(),
        caption_size: 0,
        font: String::new(),
        font_size: 0,
        favorite: false,
        tags: Vec::new(),
        notes: String::new(),
        hotkey: String::new(),
        hotkey_paste: String::new(),
        variants: Vec::new(),
        rotate_mode: String::new(),
        rotate_auto: false,
        media_duration_ms: 0,
        media_width: 0,
        media_height: 0,
        media_size_bytes: 0,
        image_text: String::new(),
        // The Markdown mirror never carries a confidential entry at all (see
        // sync_eligible), so a blank starting point is always non-confidential.
        secret: false,
    }
}

// The apply half of sync_poll_once, free of any window or file access so a test can
// drive it: takes the candidates the scan collected (stem, file content, is_new)
// and folds them into the store. `on_replace` sees a prompt right before a file
// replaces it (the caller keeps a restore point). Returns (prompts_changed,
// settings_changed); the caller persists whichever is set.
pub(crate) fn sync_poll_apply(
    store: &mut Store,
    candidates: Vec<(String, String, bool)>,
    on_replace: &mut dyn FnMut(&Settings, &Prompt, &Prompt),
) -> (bool, bool) {
    let mut changed = false;
    let mut settings_changed = false;
    let mut placements_touched = false;
    for (stem, content, is_new) in candidates {
        // Fields lives alongside content/name/etc: parse_frontmatter is cheap
        // (string splitting) and apply_md_to_prompt already re-parses it itself, so
        // a second pass here isn't worth threading through that function's own
        // signature just to share one HashMap.
        let fields = parse_frontmatter(&content).filter(|(f, _)| md_is_ours(f)).map(|(f, _)| f);
        if is_new {
            if store.prompts.iter().any(|p| p.id == stem) {
                continue; // added by something else in the meantime
            }
            if store.trash.iter().any(|p| p.id == stem) {
                // Trashed here, written again by another machine that does not know:
                // a second row with the same id would break restore, delete and purge.
                // The file stays: the sweep only takes files this process wrote or read
                // for a live prompt, and a later restore keeps this one as a copy.
                continue;
            }
            let Some(np) = apply_md_to_prompt(&sync_blank_prompt(stem), &content) else { continue };
            let id = np.id.clone();
            store.prompts.push(np);
            let (placed, folders_touched_here) = fields
                .as_ref()
                .map(|f| apply_md_placement(&mut store.settings, &id, f))
                .unwrap_or((false, false));
            if folders_touched_here {
                placements_touched = true;
            }
            // A placed file changed the settings itself; an unplaced one gets a
            // view membership below. Either way the settings are dirty.
            settings_changed = true;
            if !placed {
                let folder_cells = active_view_folder_cells(&store.settings);
                let view = store.settings.active_view_mut();
                if let Some(cell) = first_free_cell_excluding(view, &folder_cells) {
                    let key = grid_key(view.cols, view.rows);
                    view.layouts.entry(key).or_default().insert(id.clone(), cell);
                }
                // Members is the authoritative list: a prompt with a cell but no
                // membership is purged by the next grid rebuild, and one without a
                // free cell still belongs to the view (shown as overflow).
                if !view.members.contains(&id) {
                    view.members.push(id);
                }
            }
            changed = true;
        } else if let Some(idx) = store.prompts.iter().position(|p| p.id == stem) {
            if store.prompts[idx].secret {
                // A confidential entry never has a file of its own: its text here is
                // ciphertext, and a stray plaintext file with the same id (written by a
                // machine that does not know it is confidential) must not replace it.
                continue;
            }
            let placement = sync_placement_for(&store.prompts[idx], &store.settings);
            let unchanged = sync_md_matches(&store.prompts[idx], &placement, &content);
            if unchanged {
                continue; // a concurrent edit already made this file's content current
            }
            if let Some(updated) = apply_md_to_prompt(&store.prompts[idx], &content) {
                // A file that only moved or recoloured a tile leaves the prompt itself
                // as it was: replacing it would count as a change, save the prompts and
                // write every file again for nothing.
                let blank = SyncPlacement::default();
                if prompt_to_md(&store.prompts[idx], &blank) != prompt_to_md(&updated, &blank) {
                    on_replace(&store.settings, &store.prompts[idx], &updated);
                    store.prompts[idx] = updated;
                    changed = true;
                }
            }
            if let Some(f) = fields.as_ref() {
                let (placed, folders_touched_here) = apply_md_placement(&mut store.settings, &stem, f);
                if placed {
                    settings_changed = true;
                }
                if folders_touched_here {
                    placements_touched = true;
                    settings_changed = true;
                }
            }
        } // else: deleted while we were reading files; nothing left to update
    }
    if placements_touched {
        let prompt_ids: Vec<String> = store.prompts.iter().map(|p| p.id.clone()).collect();
        let view_ids: Vec<String> = store.settings.views.iter().map(|v| v.id.clone()).collect();
        sync_normalize_tile_folders(&mut store.settings.tile_folders, &prompt_ids, &view_ids);
        sync_normalize_float_folders(&mut store.settings.float_folders, &prompt_ids);
    }
    (changed, settings_changed)
}

// The inbound half: files -> DB. Runs on a timer (see start_sync_poll) rather than
// a filesystem watcher, see the module note above for why.
//
// Used to hold the store mutex for the entire fs::read_dir + per-file
// fs::read_to_string scan below - every other command needing that mutex (which
// is nearly all of them) blocked for as long as reading the user's sync folder
// took, and a slow or unreachable path (network share, unmounted drive) could
// have blocked the whole app indefinitely. Now takes a brief lock only to check
// the two guards and clone the prompt list, does every bit of file I/O against
// that snapshot with no lock held, then takes a second brief lock to apply
// whatever it actually found. Deliberately does NOT compute apply_md_to_prompt
// during the unlocked scan (an earlier version of this fix did, and a review
// caught that it would silently discard a UI edit made during the scan - the
// snapshot-derived prompt would win over the real, more current one when
// written back). Only the cheap "does this file differ from the snapshot" check
// and the file-level lock happen unlocked; apply_md_to_prompt runs once the
// store lock is back, against each prompt's CURRENT state.
// Puts the files of candidates that were never applied back to "noted, not read".
pub(crate) fn sync_requeue_candidates(dir: &Path, candidates: &[(String, String, bool)]) {
    let mut seen = sync_seen_lock();
    for (stem, _, _) in candidates {
        if let Some(entry) = seen.get_mut(&sync_seen_key(&dir.join(format!("{stem}.md")))) {
            entry.handled = false;
            entry.hash = 0;
        }
    }
}

// A file the stat scan wants read this round.
pub(crate) struct SyncToRead {
    pub(crate) path: PathBuf,
    pub(crate) key: String,
    pub(crate) stem: String,
    pub(crate) name: String,
    pub(crate) stamp: SyncStamp,
}

// Stat-only pass over the folder: decides which files changed and have settled,
// without reading a file or touching the store. A folder nothing changed in
// costs one stat per file, and the caller copies no prompts or settings.
//
// Also returns (stem, key) of every file in the folder: the caller marks the files of
// known prompts as known (SYNC_KNOWN) once it has the library's ids, every round and
// for every such file, changed or not, so the sweep's gate stays as strict as before.
pub(crate) type SyncScanOut = (Vec<SyncToRead>, usize, Vec<(String, String)>);
pub(crate) fn sync_scan_folder(dir: &Path) -> std::io::Result<SyncScanOut> {
    let entries = fs::read_dir(dir)?;
    let mut to_read = Vec::new();
    let mut all: Vec<(String, String)> = Vec::new();
    let mut present: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut files = 0usize;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()).map(str::to_string) else { continue };
        // The stem becomes a prompt id, and every id is written back out as a file
        // name: so it has to pass the same gate an outgoing id does.
        if !sync_safe_id(&stem) {
            continue;
        }
        let name = format!("{stem}.md");
        if name == "AGENTS.md" {
            continue; // the folder's own readme, never a prompt (the sweep skips it too)
        }
        files += 1;
        let key = sync_seen_key(&path);
        all.push((stem.clone(), key.clone()));
        present.insert(key.clone());
        let Some(stamp) = sync_stamp_of(&path) else { continue };
        // Metadata only, for every file in the folder: a folder whose files have not
        // changed since the last round costs one stat each and not a single read.
        let decision = sync_scan_decision(sync_seen_lock().get(&key).copied(), stamp);
        if decision == SyncScan::Read {
            to_read.push(SyncToRead { path, key, stem, name, stamp });
        } else if decision == SyncScan::Settle {
            sync_seen_lock().insert(key, SyncSeen { stamp, handled: false, hash: 0 });
        }
    }
    // Files that are gone leave no entry behind: this map would otherwise grow for
    // the life of the process in a folder with a lot of coming and going. Only this
    // folder's entries: the map also holds other folders' files (the previous folder
    // after a switch), whose records a scan of this one has no business dropping.
    sync_seen_lock().retain(|key, _| Path::new(key).parent() != Some(dir) || present.contains(key));
    Ok((to_read, files, all))
}

pub(crate) fn sync_poll_once(app: &AppHandle) {
    let Some(state) = app.try_state::<Db>() else { return };
    let dir = {
        let store = lock(&state);
        if store.prompts_locked || !sync_enabled(&store.settings) {
            return;
        }
        sync_dir_path(app, &store.settings)
    };
    let run = sync_run_lock();
    let (to_read, files, all) = match sync_scan_folder(&dir) {
        Ok(r) => r,
        Err(e) => {
            // An unreachable folder (a network share that is not mounted, a removed
            // drive) must not turn into a failure every few seconds for the rest of
            // the session. After enough failed rounds the poll pauses and says so.
            let max = SYNC_FAIL_MAX;
            let just_paused = {
                let mut h = sync_health_lock();
                sync_health_note_fail(&mut h, max, &e.to_string())
            };
            if just_paused {
                append_crash_log(&format!(
                    "prompt-sync: paused after {max} rounds that could not read {}: {e}",
                    dir.display()
                ));
            }
            return;
        }
    };
    {
        let mut h = sync_health_lock();
        sync_health_note_ok(&mut h, files, now_secs());
    }
    {
        // The files of known prompts count as known (SYNC_KNOWN) every round, changed
        // or not, taken against the library's ids (no prompt copy). The scan is stat
        // only and has no snapshot, so the step lives here; the guards are rechecked
        // under the same lock as before.
        let store = lock(&state);
        if store.prompts_locked || !sync_enabled(&store.settings) {
            return;
        }
        let ids: std::collections::HashSet<&str> = store.prompts.iter().map(|p| p.id.as_str()).collect();
        let mut known = sync_known_lock();
        for (stem, key) in &all {
            if ids.contains(stem.as_str()) {
                known.insert(key.clone());
            }
        }
    }
    if to_read.is_empty() {
        return; // nothing changed: no prompt or settings copy is made
    }
    // Only now, with at least one file to read, is the library copied for the
    // comparison against what each file would be written as.
    let (snapshot, settings_snapshot) = {
        let store = lock(&state);
        if store.prompts_locked || !sync_enabled(&store.settings) {
            return;
        }
        (store.prompts.clone(), store.settings.clone())
    };
    let mut candidates: Vec<(String, String, bool)> = Vec::new(); // (stem, content, is_new)
    for SyncToRead { path, key, stem, name, stamp } in to_read {
        // Acquire the file lock BEFORE reading, not after - the
        // lock is meant to protect the read from a concurrent writer, which it did
        // not while the read happened first and the lock was only taken afterward
        // for the (much cheaper) bookkeeping. This is unchanged from the original
        // pre-item-9 code, carried forward without noticing until flagged.
        if !sync_lock_acquire(&path) {
            continue; // someone else is mid-write; retry next cycle
        }
        let bytes = read_file_capped(&path, MAX_SYNC_MD_BYTES).ok();
        sync_lock_release(&path);
        let hash = bytes.as_deref().map(sync_hash).unwrap_or(0);
        let content = bytes.and_then(sync_decode_text);
        // Whatever the read said, this stamp has now been dealt with: a file that
        // is too big or unreadable must not be retried (and re-logged) every round.
        sync_seen_lock().insert(key, SyncSeen { stamp, handled: true, hash });
        let Some(content) = content else {
            // Refused, never truncated: a half file would be imported as the whole
            // prompt. The user only ever learns why from the error log, so say it.
            if stamp.len > MAX_SYNC_MD_BYTES {
                append_crash_log(&format!(
                    "prompt-sync: {name} is {} bytes, past the {} byte limit, skipped and not truncated",
                    stamp.len, MAX_SYNC_MD_BYTES
                ));
            } else {
                append_crash_log(&format!("prompt-sync: {name} could not be read as UTF-8 text, skipped"));
            }
            continue;
        };
        if let Some(idx) = snapshot.iter().position(|p| p.id == stem) {
            if snapshot[idx].secret {
                append_crash_log(&format!("prompt-sync: {name} belongs to a confidential prompt, skipped"));
                continue;
            }
            let placement = sync_placement_for(&snapshot[idx], &settings_snapshot);
            if sync_md_matches(&snapshot[idx], &placement, &content) {
                continue; // matches what we would write ourselves: not a real change
            }
            candidates.push((stem, content, false));
        } else {
            candidates.push((stem, content, true));
        }
    }
    // The file reads are done; applying touches only the in-memory library.
    drop(run);
    if candidates.is_empty() {
        return;
    }
    let mut store = lock(&state);
    if store.prompts_locked || !sync_enabled(&store.settings) {
        // Not applied, so not dealt with: the files are read again once the library is
        // free, instead of being dropped and later overwritten without a copy.
        sync_requeue_candidates(&dir, &candidates);
        return;
    }
    let (changed, settings_changed) = sync_poll_apply(&mut store, candidates, &mut |settings, old, updated| {
        // The file wins this round, so whatever the prompt said before is about to
        // be gone. If both sides changed since the last sync this is the losing
        // side of a conflict, so keep it as a restore point instead of dropping it,
        // the same way an edit in the app keeps one.
        if !history_enabled(settings)
            || (old.name == updated.name && old.text == updated.text && old.variants == updated.variants)
        {
            return;
        }
        let cap = history_cap(settings);
        match db_conn(app) {
            Some(c) => {
                if let Err(e) = db_add_version(&c, &old.id, now_secs(), &old.name, &old.text, &old.variants, cap) {
                    append_crash_log(&format!("prompt-sync: no restore point kept for {}: {e}", old.id));
                }
            }
            None => append_crash_log(&format!("prompt-sync: no restore point kept for {}: no DB connection", old.id)),
        }
    });
    // Prompts and Settings are separate tables/files (save_prompts never touches
    // settings): a placement-only change with no content change would otherwise be
    // applied in memory, echoed straight back out to the sync folder by
    // save_prompts' own spawn_sync_write_all, and then silently lost on the next
    // restart while the file on disk kept claiming the new placement forever.
    if settings_changed {
        save_settings(app, &store.settings);
    }
    if changed {
        save_prompts(app, &store);
    }
    drop(store);
    if changed || settings_changed {
        // Every open window still holds what it fetched last. Tell them, or a stale
        // grid shows old names and a later drag in it would write its old layout
        // back over what the other machine just moved.
        let _ = app.emit_to("main", "store-synced", ());
        notify_float_settings(app);
        rebuild_tray(app);
    }
}

// ---- Letting go of the old folder when the user picks a new one ----
//
// The sync folder belongs to the USER. Deleting one is the single most one-way
// thing this feature can do, so it is built to refuse rather than to guess: it
// only ever removes files this app wrote itself, in one folder, never into
// sub-folders, never by pattern, and the folder itself only if it ends up empty.
#[derive(Default)]
pub(crate) struct SyncDirScan {
    /// Files this app wrote and may therefore remove.
    pub(crate) ours: Vec<PathBuf>,
    /// Everything else, in the order found. One of these aborts a delete.
    pub(crate) foreign: Vec<String>,
}

pub(crate) fn sync_dir_scan(dir: &Path) -> SyncDirScan {
    let mut scan = SyncDirScan::default();
    let Ok(entries) = fs::read_dir(dir) else { return scan };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let path = e.path();
        // A sub-folder is never ours: this feature writes flat files and nothing
        // else, and a folder would need a recursive delete to go away at all.
        if path.is_dir() {
            scan.foreign.push(format!("{name}\\"));
            continue;
        }
        let ours = if name == "AGENTS.md" {
            // Only the note this feature writes itself, recognised by its first line
            // rather than by the whole text, so a note written by an older version is
            // still ours. Once the user has rewritten it, it is their text and stays.
            fs::read_to_string(&path).is_ok_and(|c| c.starts_with(&sync_agents_md_head()))
        } else if let Some(stem) = name.strip_suffix(".md") {
            // Only a file this process wrote or read back for a prompt of the library:
            // another machine's new file or a hand-made one carries our header too,
            // is not in the new folder, and would be lost with the old one.
            sync_safe_id(stem)
                && sync_known_lock().contains(&sync_seen_key(&path))
                && read_text_capped(&path, MAX_SYNC_MD_BYTES)
                    .and_then(|c| parse_frontmatter(&c))
                    .is_some_and(|(fields, _)| md_is_ours(&fields))
        } else if let Some(rest) = name.strip_suffix(".md.lock") {
            sync_safe_id(rest) // our own write lock, cleared by whoever holds it
        } else {
            // Deliberately NOT the `.tmp` half of an interrupted write of ours: a
            // file called "notes.tmp" could just as easily be the user's own, and
            // guessing wrong here deletes something nobody can get back. A leftover
            // temp file stops the delete and is named, like any other stranger.
            false
        };
        if ours {
            scan.ours.push(path);
        } else {
            scan.foreign.push(name);
        }
    }
    scan
}

// Refuses a path that is not a plain, existing folder of its own. The caller
// already only ever passes the folder the app itself is syncing to; this is the
// second lock on the door, not the first.
pub(crate) fn sync_dir_is_deletable(dir: &Path, data: &Path) -> Result<(), String> {
    if !dir.is_absolute() || dir.parent().is_none() {
        return Err("not a folder that can be removed".into());
    }
    if !dir.is_dir() {
        return Err("the folder is no longer there".into());
    }
    if dir == data || data.starts_with(dir) {
        return Err("this folder holds the app's own data".into());
    }
    Ok(())
}

// Returns how many files were removed, or a plain-language reason it did not
// touch anything. Never partially deletes on a refusal: the scan decides first.
pub(crate) fn sync_delete_dir(dir: &Path, data: &Path) -> Result<usize, String> {
    sync_dir_is_deletable(dir, data)?;
    let scan = sync_dir_scan(dir);
    if !scan.foreign.is_empty() {
        let shown: Vec<String> = scan.foreign.iter().take(5).cloned().collect();
        let more = scan.foreign.len().saturating_sub(shown.len());
        let list = shown.join(", ");
        return Err(if more > 0 {
            format!("{list} and {more} more")
        } else {
            list
        });
    }
    let mut removed = 0usize;
    for path in &scan.ours {
        if fs::remove_file(path).is_ok() {
            removed += 1;
        }
    }
    // Only ever the one folder, and only when nothing is left in it. A failure here
    // (something appeared in the meantime) leaves the folder standing, which is the
    // harmless outcome.
    let _ = fs::remove_dir(dir);
    Ok(removed)
}

// What the window needs to phrase its question: how many of the files in the
// current folder are ours, and whether anything else is in there.
#[derive(Serialize)]
pub(crate) struct SyncDirInfo {
    pub(crate) path: String,
    pub(crate) ours: usize,
    pub(crate) foreign: usize,
}

#[tauri::command]
pub(crate) fn sync_dir_info(app: AppHandle, state: State<Db>) -> SyncDirInfo {
    let dir = {
        let store = lock(&state);
        sync_dir_path(&app, &store.settings)
    };
    let scan = sync_dir_scan(&dir);
    SyncDirInfo {
        path: dir.to_string_lossy().into_owned(),
        // Only the prompt files: the folder's own readme and a leftover lock are
        // ours as well, but neither is what the question is about.
        ours: scan
            .ours
            .iter()
            .filter(|p| {
                p.extension().and_then(|e| e.to_str()) == Some("md")
                    && p.file_name().and_then(|n| n.to_str()) != Some("AGENTS.md")
            })
            .count(),
        foreign: scan.foreign.len(),
    }
}

// Live state of the feature for the expert menu: the last round that could read the
// folder, how many files it saw, and why it is paused if it is.
#[derive(Serialize)]
pub(crate) struct SyncStatus {
    pub(crate) enabled: bool,
    pub(crate) last_ok: u64,
    pub(crate) files: usize,
    pub(crate) paused: bool,
    pub(crate) reason: String,
}

#[tauri::command(async)]
pub(crate) fn sync_status(state: State<Db>) -> SyncStatus {
    let enabled = sync_enabled(&lock(&state).settings);
    let h = sync_health_lock().clone();
    SyncStatus { enabled, last_ok: h.last_ok, files: h.files, paused: h.paused, reason: h.reason }
}

// Whether two paths name the same folder, or one lies inside the other. Compared by
// identity (the resolved path), never by spelling: another letter case, a mapped
// drive against its share path or a junction all reach the same files, and deleting
// "the old folder" after writing into it would wipe what was just written.
//
// Case is ignored (this is a Windows-only app, and a share's server part can keep the
// case it was typed in). A path that cannot be resolved right now (permissions, a
// share briefly gone) counts as "maybe the same": the only thing this answer ever
// gates is a delete, and a delete that wrongly does not happen loses nothing.
pub(crate) fn sync_same_dir(a: &Path, b: &Path) -> bool {
    let (ca, cb) = match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => (x, y),
        _ => return true,
    };
    let fold = |p: &Path| PathBuf::from(p.to_string_lossy().to_lowercase());
    let (ca, cb) = (fold(&ca), fold(&cb));
    ca == cb || ca.starts_with(&cb) || cb.starts_with(&ca)
}

// The state half of a folder change, free of any window so a test can drive it:
// under the store lock only the setting is changed and saved and the library is
// copied; the (possibly slow) file write runs on the copies after the lock is
// gone. Returns (old folder, new folder, files that failed, feature enabled).
pub(crate) fn set_sync_dir_core(
    db: &Mutex<Store>,
    dir: &str,
    save: &dyn Fn(&Settings),
    dir_of: &dyn Fn(&Settings) -> PathBuf,
    write: impl Fn(&[Prompt], &Settings) -> usize,
) -> (PathBuf, PathBuf, usize, bool) {
    let (old, new_dir, enabled, snapshot) = {
        let mut store = db.lock().unwrap_or_else(|e| e.into_inner());
        let old = dir_of(&store.settings);
        store.settings.ui_texts.insert("syncDir".into(), dir.to_string());
        save(&store.settings);
        let enabled = sync_enabled(&store.settings);
        let snapshot = enabled.then(|| (store.prompts.clone(), store.settings.clone()));
        (old, dir_of(&store.settings), enabled, snapshot)
    };
    let failures = match snapshot {
        Some((prompts, settings)) => write(&prompts, &settings),
        None => 0,
    };
    (old, new_dir, failures, enabled)
}

// Point the sync at another folder. `delete_old` is an explicit decision the WINDOW
// made after asking the user. A backend that deleted a folder on its own initiative
// is exactly the kind of surprise this app does not do. The folder that may be
// deleted is never passed in: it is the one the app itself was syncing to a moment
// ago, read from the settings here. Async so the one-off full write runs off the
// main thread, and on a copy of the library so no window waits on it.
#[tauri::command(async)]
pub(crate) fn set_sync_dir(app: AppHandle, state: State<Db>, dir: String, delete_old: bool) -> Result<usize, String> {
    let trimmed = dir.trim();
    if !trimmed.is_empty() && !consume_granted_dir(Path::new(trimmed), "sync") {
        return Err("dir-not-picked".to_string());
    }
    // Same one-off treatment activation gets: when this returns, the files are in the
    // new folder.
    let (old, new_dir, write_failures, enabled) = set_sync_dir_core(
        &state,
        trimmed,
        &|settings| {
            save_settings(&app, settings);
        },
        &|settings| sync_dir_path(&app, settings),
        |prompts, settings| sync_write_all_impl(&app, prompts, settings),
    );
    // Whenever the folder just took a full write (also when the same folder is picked
    // again after its drive came back) it demonstrably works: the pause and its old
    // reason end now, not after up to ten intervals.
    if enabled && write_failures == 0 {
        sync_health_after_dir_change(&mut sync_health_lock());
        SYNC_WAKE.notify_one();
    }
    notify_float_settings(&app);
    if !delete_old {
        return Ok(0);
    }
    // The new folder is written FIRST and the old one only goes once that worked.
    // Deleting after a failed write would take the only copy of those files with it.
    if write_failures > 0 {
        return Err(format!("the new folder could not be written ({write_failures} files), so nothing was deleted"));
    }
    let new = new_dir;
    if sync_same_dir(&old, &new) {
        return Ok(0); // nothing moved, so there is no old folder to let go of
    }
    sync_delete_dir(&old, &data_dir(&app))
}

// Guards start_sync_poll against a second loop: the feature can be switched on
// again before the previous loop has noticed it was switched off (or the setup
// call and an immediate activation race), and either way only one timer may
// ever be ticking at once.
pub(crate) static SYNC_POLL_RUNNING: AtomicBool = AtomicBool::new(false);

// Claims the single poll slot. False means a loop already holds it and this
// caller must not start a second one.
pub(crate) fn sync_poll_try_claim() -> bool {
    !SYNC_POLL_RUNNING.swap(true, Ordering::SeqCst)
}

pub(crate) fn sync_poll_release() {
    SYNC_POLL_RUNNING.store(false, Ordering::SeqCst);
}

// Run once before the first save of a session: every file that already sits in the
// folder under the id of a library prompt is noted as known, and its stamp is
// recorded. One that matches what this machine would write is handled; one that
// differs (edited while the app was closed, or by another machine through the
// cloud) stays unhandled, so a save landing before the poll has imported it keeps
// a copy of it instead of overwriting it.
pub(crate) fn sync_seed_seen(dir: &Path, prompts: &[Prompt], settings: &Settings) {
    for p in prompts.iter().filter(|p| sync_eligible(p)) {
        let path = dir.join(format!("{}.md", p.id));
        if !path.is_file() {
            continue;
        }
        sync_known_note(&path);
        let key = sync_seen_key(&path);
        if sync_seen_lock().contains_key(&key) {
            continue;
        }
        let Some(stamp) = sync_stamp_of(&path) else { continue };
        let bytes = read_file_capped(&path, MAX_SYNC_MD_BYTES).ok();
        let hash = bytes.as_deref().map(sync_hash).unwrap_or(0);
        let handled = bytes
            .and_then(sync_decode_text)
            .is_some_and(|c| sync_md_matches(p, &sync_placement_for(p, settings), &c));
        // A file that differs stays unhandled with no hash, so it counts as changed.
        let hash = if handled { hash } else { 0 };
        sync_seen_lock().entry(key).or_insert(SyncSeen { stamp, handled, hash });
    }
}

pub(crate) fn start_sync_poll(app: AppHandle) {
    if !sync_poll_try_claim() {
        return; // already running - set_ui_flag calls this again on every activation
    }
    tauri::async_runtime::spawn(async move {
        // Learn which files on disk belong to prompts we already have before the
        // first save can sweep anything: a prompt deleted in the first seconds of a
        // session must still take its file along.
        if let Some(state) = app.try_state::<Db>() {
            let (dir, prompts, settings) = {
                let s = lock(&state);
                (sync_dir_path(&app, &s.settings), s.prompts.clone(), s.settings.clone())
            };
            let _ = tauri::async_runtime::spawn_blocking(move || sync_seed_seen(&dir, &prompts, &settings)).await;
        }
        loop {
            let Some(state) = app.try_state::<Db>() else {
                sync_poll_release();
                return;
            };
            // Read fresh each round (not once at spawn time) so the switch is
            // honoured on the very next wait, no scheduler restart needed.
            let (enabled, secs) = {
                let s = lock(&state);
                (sync_enabled(&s.settings), SYNC_POLL_SECS)
            };
            // The feature is off: end the task instead of waking up forever for
            // nothing. Whoever switches it back on (set_ui_flag) starts a fresh
            // loop, guarded by the claim above against ever running two at once.
            if !enabled {
                sync_poll_release();
                return;
            }
            // A paused poll (an unreachable folder) keeps the same one task and only
            // slows down: never a second timer, and never a thread that has to be
            // stopped and started again when the interval changes.
            let paused = sync_health_lock().paused;
            tokio::select! {
                _ = tokio::time::sleep(std::time::Duration::from_secs(sync_wait_secs(paused, secs))) => {}
                _ = SYNC_WAKE.notified() => {}
            }
            sync_poll_once(&app);
        }
    });
}
