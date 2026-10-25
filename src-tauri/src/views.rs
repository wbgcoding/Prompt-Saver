use super::*;

// ---------- Grid / layout commands (per active view) ----------

// Replace the placement map of the active view's current grid size.
// Async: fires on every drag/hide; keep the settings write off the UI thread.
#[tauri::command]
pub(crate) async fn set_layout(app: AppHandle, state: State<'_, Db>, layout: HashMap<String, [u32; 2]>) -> Result<(), String> {
    let mut store = lock(&state);
    let known: std::collections::HashSet<String> = store.prompts.iter().map(|p| p.id.clone()).collect();
    let layout = layout_of_known(layout, &known);
    let view = store.settings.active_view_mut();
    let key = grid_key(view.cols, view.rows);
    seat_layout_ids_as_members(view, &layout, &known);
    view.layouts.insert(key, layout);
    save_settings(&app, &store.settings);
    Ok(())
}

// A layout sent after a delete still names the deleted prompt: stored as it came,
// it would recreate an occupied ghost cell, so only existing prompts are kept.
pub(crate) fn layout_of_known(
    mut layout: HashMap<String, [u32; 2]>,
    known: &std::collections::HashSet<String>,
) -> HashMap<String, [u32; 2]> {
    layout.retain(|id, _| known.contains(id));
    layout
}

// A prompt placed on the grid (dragged in from the library, or added with its plus)
// arrives here as a layout entry only; `members` is what decides belonging, and a
// resize rebuilds the layout from it, so a placed prompt that never became a member
// would vanish at the next resize. Only prompts that still exist count, so a stale
// layout sent after a delete cannot bring one back.
pub(crate) fn seat_layout_ids_as_members(
    view: &mut View,
    layout: &HashMap<String, [u32; 2]>,
    known: &std::collections::HashSet<String>,
) {
    let mut ids: Vec<&String> = layout.keys().filter(|id| known.contains(*id)).collect();
    ids.sort();
    for id in ids {
        if !view.members.iter().any(|m| m == id) {
            view.members.push(id.clone());
        }
    }
}

// Rebuilds one view's layout for its current grid size from `members`, the view's real
// membership list. Never removes a member: an id that finds no free cell gets no layout
// entry and shows up as overflow in the UI instead of silently disappearing. One
// unconditional recompute, with no grow/shrink branching to get wrong.
pub(crate) fn rebuild_view_layout(
    view: &mut View,
    folder_cells: &std::collections::HashSet<(u32, u32)>,
    folder_members: &std::collections::HashSet<String>,
) {
    let (cols, rows) = (view.cols, view.rows);
    let key = grid_key(cols, rows);
    // Keep what the current size has on record, dropping entries for ids that are no longer
    // members, now sit inside a grid folder, or no longer fit (the grid shrank).
    let mut layout: HashMap<String, [u32; 2]> = view.layouts.get(&key).cloned().unwrap_or_default();
    layout.retain(|pid, cell| {
        view.members.iter().any(|m| m == pid)
            && !folder_members.contains(pid)
            && cell[0] < cols
            && cell[1] < rows
    });
    let mut occupied: std::collections::HashSet<(u32, u32)> = folder_cells.clone();
    occupied.extend(layout.values().map(|c| (c[0], c[1])));
    // A member without a cell prefers the spot it is remembered at from any other saved size
    // (shrinking and growing back restores the old arrangement), then the first free cell.
    // Growing always finds room; shrinking may not, and the leftovers stay members without a
    // cell (overflow).
    let mut remembered: HashMap<&str, [u32; 2]> = HashMap::new();
    for (k, other) in &view.layouts {
        if *k == key {
            continue;
        }
        for (pid, cell) in other {
            remembered.entry(pid.as_str()).or_insert(*cell);
        }
    }
    for pid in view.members.clone() {
        // Lives inside a grid folder's own cell, not as an individual tile.
        if folder_members.contains(&pid) {
            continue;
        }
        if layout.contains_key(&pid) {
            continue;
        }
        if let Some(cell) = remembered.get(pid.as_str()) {
            if cell[0] < cols && cell[1] < rows && !occupied.contains(&(cell[0], cell[1])) {
                layout.insert(pid.clone(), *cell);
                occupied.insert((cell[0], cell[1]));
                continue;
            }
        }
        'find: for row in 0..rows {
            for col in 0..cols {
                if !occupied.contains(&(col, row)) {
                    layout.insert(pid.clone(), [col, row]);
                    occupied.insert((col, row));
                    break 'find;
                }
            }
        }
    }
    view.layouts.insert(key, layout);
}

// Moves every folder of `view_id` whose cell lies outside a cols x rows grid to the
// first cell no folder of that view holds. The cells of folders that already fit are
// taken up front, so a moved folder never lands on one that was not touched.
pub(crate) fn fit_folders_to_grid(folders: &mut [TileFolder], view_id: &str, cols: u32, rows: u32) {
    let mut taken_cells: Vec<[u32; 2]> = folders
        .iter()
        .filter(|f| f.view_id == view_id && f.cell[0] < cols && f.cell[1] < rows)
        .map(|f| f.cell)
        .collect();
    for folder in folders.iter_mut().filter(|f| f.view_id == view_id) {
        if folder.cell[0] >= cols || folder.cell[1] >= rows {
            'find: for row in 0..rows {
                for col in 0..cols {
                    if !taken_cells.contains(&[col, row]) {
                        folder.cell = [col, row];
                        taken_cells.push(folder.cell);
                        break 'find;
                    }
                }
            }
        }
    }
}

// Change the active view's grid dimensions. The arrangement saved for the new size is
// restored automatically (layouts are keyed per size); `members` never changes here, so
// nothing appears or disappears just because the grid did.
#[tauri::command(async)]
pub(crate) fn set_view_grid(app: AppHandle, state: State<Db>, id: String, cols: u32, rows: u32) -> Settings {
    let mut store = lock(&state);
    let cols = cols.clamp(GRID_MIN, GRID_MAX);
    let rows = rows.clamp(GRID_MIN, GRID_MAX);
    // A folder whose cell no longer fits the shrunk grid moves to the first free cell in this
    // view instead of landing off-grid and unreachable; normalize_tile_folders below resolves
    // any collision the move creates.
    fit_folders_to_grid(&mut store.settings.tile_folders, &id, cols, rows);
    let ids: Vec<String> = store.prompts.iter().map(|p| p.id.clone()).collect();
    let view_ids: Vec<String> = store.settings.views.iter().map(|v| v.id.clone()).collect();
    normalize_tile_folders(&mut store.settings.tile_folders, &ids, &view_ids);
    let (folder_cells, folder_members) = view_folder_sets(&store.settings, &id);
    let Some(view) = store.settings.views.iter_mut().find(|v| v.id == id) else {
        return webview_settings(&store.settings);
    };
    view.cols = cols;
    view.rows = rows;
    rebuild_view_layout(view, &folder_cells, &folder_members);
    save_settings(&app, &store.settings);
    webview_settings(&store.settings)
}

// ---------- Tag commands ----------

pub(crate) const MAX_TAG_LEN: usize = 60;
pub(crate) const MAX_TAGS_PER_PROMPT: usize = 50;

pub(crate) fn tag_key(name: &str) -> String {
    name.trim().to_lowercase()
}

// Whether a prompt carries the reserved archive tag: a plain tag match, not a state field,
// so every tag-aware path (search, export, backup, sync) treats an archived prompt like any
// other.
pub(crate) fn is_archived(prompt: &Prompt, archive_tag_key: Option<&str>) -> bool {
    match archive_tag_key {
        Some(key) => prompt.tags.iter().any(|t| tag_key(t) == key),
        None => false,
    }
}

// Commas separate tags in the TXT/CSV exports and control characters would break the
// line-based TXT format, so neither may appear inside a name.
pub(crate) fn tag_chars_ok(name: &str) -> bool {
    !name.contains(',') && !name.chars().any(|c| c.is_control())
}

// The casing each known tag is already stored with, so a tag typed as "work" joins
// an existing "Work" instead of splitting into a second chip.
pub(crate) fn canonical_tags(prompts: &[Prompt]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for p in prompts {
        for t in &p.tags {
            map.entry(tag_key(t)).or_insert_with(|| t.clone());
        }
    }
    map
}

// Trim, drop blank entries, fold case-insensitive duplicates onto the casing the
// library already uses. Over-long / over-many names are rejected, never truncated.
pub(crate) fn sanitize_tags(input: Vec<String>, known: &HashMap<String, String>) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for raw in input {
        let name = raw.trim();
        if name.is_empty() {
            continue;
        }
        if name.chars().count() > MAX_TAG_LEN {
            return Err(format!("tag name longer than {} characters", MAX_TAG_LEN));
        }
        if !tag_chars_ok(name) {
            return Err("tag name contains a comma or a control character".to_string());
        }
        let key = tag_key(name);
        if !seen.insert(key.clone()) {
            continue;
        }
        out.push(known.get(&key).cloned().unwrap_or_else(|| name.to_string()));
    }
    if out.len() > MAX_TAGS_PER_PROMPT {
        return Err(format!("more than {} tags on one prompt", MAX_TAGS_PER_PROMPT));
    }
    Ok(out)
}

// ---------- Private notes ----------

// Generous ceiling: notes are a scratchpad, not a second prompt body. Rejecting
// instead of truncating keeps the user's text from being silently cut in half.
pub(crate) const MAX_NOTES_LEN: usize = 10_000;

// Normalize line endings and enforce the ceiling. Notes may contain newlines:
// the TXT exporter escapes them (see notes_escape).
pub(crate) fn sanitize_notes(raw: &str) -> Result<String, String> {
    let text = raw.replace("\r\n", "\n").replace('\r', "\n");
    let text = text.trim().to_string();
    if text.chars().count() > MAX_NOTES_LEN {
        return Err(format!("note longer than {} characters", MAX_NOTES_LEN));
    }
    Ok(text)
}

// The TXT export is line-based, so a multi-line note is written on one line with
// its newlines escaped. Backslashes are escaped too, or a note ending in "\" plus
// a literal "n" would come back as a line break.
pub(crate) fn notes_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\n', "\\n")
}

pub(crate) fn notes_unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
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

// A single tag name argument: blank is an error here because the name IS the input.
pub(crate) fn valid_tag_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("tag name is empty".to_string());
    }
    if name.chars().count() > MAX_TAG_LEN {
        return Err(format!("tag name longer than {} characters", MAX_TAG_LEN));
    }
    if !tag_chars_ok(name) {
        return Err("tag name contains a comma or a control character".to_string());
    }
    Ok(name.to_string())
}

// "" clears the chip color back to neutral; anything else must be #rrggbb.
pub(crate) fn valid_tag_color(color: &str) -> Result<String, String> {
    let c = color.trim();
    if c.is_empty() {
        return Ok(String::new());
    }
    let ok = c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|ch| ch.is_ascii_hexdigit());
    if ok {
        Ok(c.to_lowercase())
    } else {
        Err("color must be #rrggbb".to_string())
    }
}

// Swap one tag key for a new name across every prompt. Renaming onto a tag a prompt
// already carries merges the two instead of duplicating it. Returns whether anything
// changed, so an unused name costs no write.
pub(crate) fn rename_tag_in(prompts: &mut [Prompt], old_key: &str, new: &str) -> bool {
    let mut changed = false;
    for p in prompts {
        if !p.tags.iter().any(|t| tag_key(t) == old_key) {
            continue;
        }
        let mut kept: Vec<String> = Vec::with_capacity(p.tags.len());
        for t in std::mem::take(&mut p.tags) {
            let name = if tag_key(&t) == old_key { new.to_string() } else { t };
            if !kept.iter().any(|k| tag_key(k) == tag_key(&name)) {
                kept.push(name);
            }
        }
        p.tags = kept;
        changed = true;
    }
    changed
}

// Move `old_key`'s color onto `new_key` unless `new_key` already has one: the surviving
// tag's color wins. This is what makes renaming onto an existing tag a merge; rename_tag
// handles both.
pub(crate) fn transfer_tag_color(colors: &mut HashMap<String, String>, old_key: &str, new_key: &str) {
    if let Some(color) = colors.remove(old_key) {
        colors.entry(new_key.to_string()).or_insert(color);
    }
}

// Drop one tag key from every prompt. Returns whether anything changed.
pub(crate) fn remove_tag_from(prompts: &mut [Prompt], key: &str) -> bool {
    let mut changed = false;
    for p in prompts {
        let before = p.tags.len();
        p.tags.retain(|t| tag_key(t) != key);
        changed |= p.tags.len() != before;
    }
    changed
}

// Batch "add tag": touches only the listed ids that do not already carry it
// (case-insensitive); already having it is a no-op, not an error. `known` supplies the
// stored casing, like the editor's sanitize_tags call.
pub(crate) fn add_tag_to_ids(
    prompts: &mut [Prompt],
    ids: &std::collections::HashSet<&str>,
    name: &str,
    known: &HashMap<String, String>,
) -> Result<(), String> {
    let key = tag_key(name);
    for p in prompts.iter_mut() {
        if !ids.contains(p.id.as_str()) || p.tags.iter().any(|t| tag_key(t) == key) {
            continue;
        }
        let mut next = p.tags.clone();
        next.push(name.to_string());
        p.tags = sanitize_tags(next, known)?;
    }
    Ok(())
}

// Batch "remove tag": only touches the listed ids. Not carrying the tag is not an
// error, just a no-op for that one prompt.
pub(crate) fn remove_tag_from_ids(prompts: &mut [Prompt], ids: &std::collections::HashSet<&str>, key: &str) {
    for p in prompts.iter_mut() {
        if ids.contains(p.id.as_str()) {
            p.tags.retain(|t| tag_key(t) != key);
        }
    }
}

// Replace `old` with `new` in every prompt and move the color entry, in ONE store
// pass: a tag on hundreds of prompts must not turn into hundreds of round trips.
// Idempotent, and renaming onto an existing tag merges the two.
#[tauri::command]
pub(crate) async fn rename_tag(
    app: AppHandle,
    state: State<'_, Db>,
    old: String,
    new: String,
) -> Result<Settings, String> {
    let old = valid_tag_name(&old)?;
    let new = valid_tag_name(&new)?;
    let (old_key, new_key) = (tag_key(&old), tag_key(&new));
    let mut store = lock(&state);
    rename_tag_in_store(
        &mut store,
        &old_key,
        &new,
        &new_key,
        &mut |st| save_prompts(&app, st),
        &mut |settings| save_settings(&app, settings),
    )?;
    Ok(webview_settings(&store.settings))
}

// The two-step write behind rename_tag with the saves injected, so the rollback is
// testable. The prompts are saved first; when the settings save then fails, the
// prompts are put back AND written again, because the rename is already on disk
// and the caller is told the whole call failed.
pub(crate) fn rename_tag_in_store(
    store: &mut Store,
    old_key: &str,
    new: &str,
    new_key: &str,
    save_prompts_fn: &mut dyn FnMut(&Store) -> bool,
    save_settings_fn: &mut dyn FnMut(&Settings) -> bool,
) -> Result<(), String> {
    // Snapshot everything this command can touch before mutating, so a failed
    // save can put it back exactly instead of reporting success for a rename that
    // never reached disk.
    let original_tags = snapshot_tags(&store.prompts, old_key);
    let prompts_changed = rename_tag_in(&mut store.prompts, old_key, new);
    if prompts_changed && !save_prompts_fn(store) {
        restore_tags(store, original_tags);
        return Err("save-failed".into());
    }
    let original_old_color = store.settings.tag_colors.get(old_key).cloned();
    let original_new_color = store.settings.tag_colors.get(new_key).cloned();
    transfer_tag_color(&mut store.settings.tag_colors, old_key, new_key);
    if !save_settings_fn(&store.settings) {
        match original_old_color {
            Some(c) => { store.settings.tag_colors.insert(old_key.to_string(), c); }
            None => { store.settings.tag_colors.remove(old_key); }
        }
        match original_new_color {
            Some(c) => { store.settings.tag_colors.insert(new_key.to_string(), c); }
            None => { store.settings.tag_colors.remove(new_key); }
        }
        if prompts_changed {
            restore_tags(store, original_tags);
            note_failed_undo(save_prompts_fn(store), "tag rename");
        }
        return Err("save-failed".into());
    }
    Ok(())
}

// The tag lists of every prompt carrying `key`, taken before a change so a failed save can
// put exactly those back.
fn snapshot_tags(prompts: &[Prompt], key: &str) -> Vec<(String, Vec<String>)> {
    prompts
        .iter()
        .filter(|p| p.tags.iter().any(|t| tag_key(t) == key))
        .map(|p| (p.id.clone(), p.tags.clone()))
        .collect()
}

fn restore_tags(store: &mut Store, original_tags: Vec<(String, Vec<String>)>) {
    for (id, tags) in original_tags {
        if let Some(p) = store.prompts.iter_mut().find(|p| p.id == id) {
            p.tags = tags;
        }
    }
}

// The write that puts the old prompts back after a failed settings write. When that one fails
// too, the disk keeps the new prompt rows although the command reports an error: leave a trace.
pub(crate) fn note_failed_undo(written: bool, operation: &str) {
    if !written {
        append_crash_log(&format!("persist failed: the undo of a {operation} could not be written"));
    }
}

// Remove a tag from every prompt plus its color entry. Idempotent.
#[tauri::command]
pub(crate) async fn delete_tag(app: AppHandle, state: State<'_, Db>, name: String) -> Result<Settings, String> {
    let key = tag_key(&valid_tag_name(&name)?);
    let mut store = lock(&state);
    delete_tag_in_store(
        &mut store,
        &key,
        &mut |st| save_prompts(&app, st),
        &mut |settings| save_settings(&app, settings),
    )?;
    Ok(webview_settings(&store.settings))
}

// The two-step write behind delete_tag with the saves injected, rolled back the same way as
// the rename: a failed settings save restores the prompts in memory and on disk.
pub(crate) fn delete_tag_in_store(
    store: &mut Store,
    key: &str,
    save_prompts_fn: &mut dyn FnMut(&Store) -> bool,
    save_settings_fn: &mut dyn FnMut(&Settings) -> bool,
) -> Result<(), String> {
    let original_tags = snapshot_tags(&store.prompts, key);
    let prompts_changed = remove_tag_from(&mut store.prompts, key);
    if prompts_changed && !save_prompts_fn(store) {
        restore_tags(store, original_tags);
        return Err("save-failed".into());
    }
    let original_color = store.settings.tag_colors.remove(key);
    if !save_settings_fn(&store.settings) {
        if let Some(c) = original_color {
            store.settings.tag_colors.insert(key.to_string(), c);
        }
        // The prompts without the tag are already on disk: put them back there too.
        if prompts_changed {
            restore_tags(store, original_tags);
            note_failed_undo(save_prompts_fn(store), "tag delete");
        }
        return Err("save-failed".into());
    }
    Ok(())
}

#[derive(Deserialize)]
pub(crate) struct TagColor {
    pub(crate) name: String,
    pub(crate) color: String,
}

// Every name and colour is validated before anything is stored, so one bad entry
// rejects the whole call instead of leaving half the tags coloured.
pub(crate) fn apply_tag_colors(settings: &mut Settings, entries: Vec<TagColor>) -> Result<(), String> {
    let checked: Vec<(String, String)> = entries
        .into_iter()
        .map(|e| Ok((tag_key(&valid_tag_name(&e.name)?), valid_tag_color(&e.color)?)))
        .collect::<Result<_, String>>()?;
    for (key, color) in checked {
        if color.is_empty() {
            settings.tag_colors.remove(&key);
        } else {
            settings.tag_colors.insert(key, color);
        }
    }
    Ok(())
}

// "Colour every tag at once": one lock and one settings write for the whole set, instead of
// an immediate write per tag through set_tag_color.
#[tauri::command(async)]
pub(crate) fn set_tag_colors(app: AppHandle, state: State<Db>, entries: Vec<TagColor>) -> Result<Settings, String> {
    let mut store = lock(&state);
    apply_tag_colors(&mut store.settings, entries)?;
    save_settings(&app, &store.settings);
    Ok(webview_settings(&store.settings))
}

#[tauri::command(async)]
pub(crate) fn set_tag_color(app: AppHandle, state: State<Db>, name: String, color: String) -> Result<Settings, String> {
    let key = tag_key(&valid_tag_name(&name)?);
    let color = valid_tag_color(&color)?;
    let mut store = lock(&state);
    if color.is_empty() {
        store.settings.tag_colors.remove(&key);
    } else {
        store.settings.tag_colors.insert(key, color);
    }
    save_settings(&app, &store.settings);
    Ok(webview_settings(&store.settings))
}

// ---------- View commands ----------

#[tauri::command(async)]
pub(crate) fn add_view(app: AppHandle, state: State<Db>, name: String, color: String) -> Result<Settings, String> {
    let mut store = lock(&state);
    if store.settings.views.len() >= MAX_VIEWS {
        return Err(format!("max {} views", MAX_VIEWS));
    }
    let trimmed = name.trim();
    let view = View {
        id: gen_id(),
        name: if trimmed.is_empty() {
            format!("View {}", store.settings.views.len() + 1)
        } else {
            trimmed.to_string()
        },
        cols: default_cols(),
        rows: default_rows(),
        layouts: HashMap::new(),
        color: color.trim().to_string(),
        members: Vec::new(),
        sync_ids: Vec::new(),
    };
    store.settings.active_view.clone_from(&view.id);
    store.settings.views.push(view);
    save_settings(&app, &store.settings);
    Ok(webview_settings(&store.settings))
}

#[tauri::command(async)]
pub(crate) fn rename_view(app: AppHandle, state: State<Db>, id: String, name: String) -> Settings {
    let mut store = lock(&state);
    if let Some(view) = store.settings.views.iter_mut().find(|v| v.id == id) {
        let trimmed = name.trim();
        if !trimmed.is_empty() {
            view.name = trimmed.to_string();
        }
    }
    save_settings(&app, &store.settings);
    // The view's name is part of every file in it: write them now, or the other machines only
    // see the new name once some prompt in the view is next saved.
    spawn_sync_write_all(&app, &store);
    webview_settings(&store.settings)
}

// Set a view's tab color (hex); empty string clears it back to default.
#[tauri::command(async)]
pub(crate) fn set_view_color(app: AppHandle, state: State<Db>, id: String, color: String) -> Settings {
    let mut store = lock(&state);
    if let Some(view) = store.settings.views.iter_mut().find(|v| v.id == id) {
        view.color = color.trim().to_string();
    }
    save_settings(&app, &store.settings);
    webview_settings(&store.settings)
}

// The tab bar's order and a group's member order are both just `settings.views`' order
// (groupViews on the frontend filters that array without sorting it), so reordering either
// is one reshuffle of this Vec and never touches a View's own fields. `order` is the
// caller's desired full id sequence: an unknown id is skipped, and a view left out (stale
// caller, or added by another window mid-drag) keeps its relative order after everything
// named, so a mismatch can reorder less than intended but never drop a view.
pub(crate) fn reorder_views_by_id(mut views: Vec<View>, order: &[String]) -> Vec<View> {
    let mut reordered: Vec<View> = Vec::with_capacity(views.len());
    for id in order {
        if let Some(pos) = views.iter().position(|v| &v.id == id) {
            reordered.push(views.remove(pos));
        }
    }
    reordered.append(&mut views);
    reordered
}

#[tauri::command(async)]
pub(crate) fn reorder_views(app: AppHandle, state: State<Db>, order: Vec<String>) -> Settings {
    let mut store = lock(&state);
    store.settings.views = reorder_views_by_id(std::mem::take(&mut store.settings.views), &order);
    save_settings(&app, &store.settings);
    webview_settings(&store.settings)
}

// Moves every tag colour equal to an old palette hex to its new one. Returns
// whether anything changed.
pub(crate) fn remap_tag_colors(colors: &mut HashMap<String, String>, map: &HashMap<String, String>) -> bool {
    let mut changed = false;
    for c in colors.values_mut() {
        if let Some(nc) = map.get(c.as_str()) {
            *c = nc.clone();
            changed = true;
        }
    }
    changed
}

// Recolor saved data when the global palette changes: every prompt/view whose
// color equals an old palette hex is moved to the new one (pairs = [[old,new],...]).
#[tauri::command(async)]
pub(crate) fn remap_colors(app: AppHandle, state: State<Db>, pairs: Vec<(String, String)>) -> Result<(), String> {
    if pairs.is_empty() {
        return Ok(());
    }
    let map: HashMap<String, String> = pairs.into_iter().filter(|(o, n)| o != n).collect();
    if map.is_empty() {
        return Ok(());
    }
    let mut store = lock(&state);
    // Snapshot every color this can touch before mutating, so a failed save can restore it
    // instead of reporting success for a remap that never reached disk.
    let original_prompt_colors: Vec<(String, String)> = store
        .prompts
        .iter()
        .filter(|p| map.contains_key(&p.color))
        .map(|p| (p.id.clone(), p.color.clone()))
        .collect();
    let mut prompts_changed = false;
    for p in &mut store.prompts {
        if let Some(nc) = map.get(&p.color) {
            p.color = nc.clone();
            prompts_changed = true;
        }
    }
    let restore_prompt_colors = |store: &mut Store| {
        for (id, c) in &original_prompt_colors {
            if let Some(p) = store.prompts.iter_mut().find(|p| p.id == *id) {
                p.color = c.clone();
            }
        }
    };
    if prompts_changed && !save_prompts(&app, &store) {
        restore_prompt_colors(&mut store);
        return Err("save-failed".into());
    }
    let original_view_colors: Vec<(String, String)> = store
        .settings
        .views
        .iter()
        .filter(|v| map.contains_key(&v.color))
        .map(|v| (v.id.clone(), v.color.clone()))
        .collect();
    let mut views_changed = false;
    for v in &mut store.settings.views {
        if let Some(nc) = map.get(&v.color) {
            v.color = nc.clone();
            views_changed = true;
        }
    }
    // Tag colours are picked from the same palette swatches, so a tag set to an old
    // palette hue would otherwise keep the old hex and read as a custom colour.
    let original_tag_colors = store.settings.tag_colors.clone();
    views_changed |= remap_tag_colors(&mut store.settings.tag_colors, &map);
    if views_changed && !save_settings(&app, &store.settings) {
        store.settings.tag_colors = original_tag_colors;
        for (id, c) in original_view_colors {
            if let Some(v) = store.settings.views.iter_mut().find(|v| v.id == id) {
                v.color = c;
            }
        }
        // The prompts reached disk already: put their colours back there too.
        if prompts_changed {
            restore_prompt_colors(&mut store);
            note_failed_undo(save_prompts(&app, &store), "palette change");
        }
        return Err("save-failed".into());
    }
    Ok(())
}

// The settings half of deleting a view, apart from the lock and the save so it is
// testable. A grid folder of the deleted view goes with it now: left behind it would still
// claim its members in normalize_tile_folders' one-folder-per-prompt rule and starve a new
// folder.
pub(crate) fn remove_view_from_settings(settings: &mut Settings, id: &str, prompt_ids: &[String]) -> Result<(), String> {
    if settings.views.len() <= 1 {
        return Err("cannot delete the last view".to_string());
    }
    settings.views.retain(|v| v.id != id);
    if settings.active_view == id {
        let first_id = settings.views[0].id.clone();
        settings.active_view.clone_from(&first_id);
    }
    let view_ids: Vec<String> = settings.views.iter().map(|v| v.id.clone()).collect();
    normalize_tile_folders(&mut settings.tile_folders, prompt_ids, &view_ids);
    Ok(())
}

#[tauri::command(async)]
pub(crate) fn delete_view(app: AppHandle, state: State<Db>, id: String) -> Result<Settings, String> {
    let mut store = lock(&state);
    let prompt_ids: Vec<String> = store.prompts.iter().map(|p| p.id.clone()).collect();
    remove_view_from_settings(&mut store.settings, &id, &prompt_ids)?;
    save_settings(&app, &store.settings);
    Ok(webview_settings(&store.settings))
}

#[tauri::command(async)]
pub(crate) fn set_active_view(app: AppHandle, state: State<Db>, id: String) -> Settings {
    let mut store = lock(&state);
    if store.settings.views.iter().any(|v| v.id == id) {
        store.settings.active_view = id;
    }
    save_settings(&app, &store.settings);
    webview_settings(&store.settings)
}

// ---------- Tile folder commands ----------

// Bring the grid folders back to their invariants: only real prompts and real views, no
// prompt in two folders (the first listing it wins), no two folders on one cell of a view
// (a later one is pushed right), and no folder with fewer than two members (it dissolves
// and its prompts become tiles again). Runs on load and after every mutation; idempotent.
// Takes `view_ids` rather than `&[View]` to spare a clone of every view's layout maps at
// each call site.
pub(crate) fn normalize_tile_folders(folders: &mut Vec<TileFolder>, prompt_ids: &[String], view_ids: &[String]) {
    normalize_tile_folders_min(folders, prompt_ids, view_ids, 2);
}

// The shared body; `min_members` is 2 everywhere except after a sync batch (see md_sync).
pub(crate) fn normalize_tile_folders_min(folders: &mut Vec<TileFolder>, prompt_ids: &[String], view_ids: &[String], min_members: usize) {
    let mut taken: Vec<String> = Vec::new();
    for folder in folders.iter_mut() {
        // A hand-edited or imported cell far outside any grid (up to u32::MAX) must not reach the
        // collision loop: saturating_add stops advancing at the cap and would spin forever with the
        // store mutex held.
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
    folders.retain(|f| f.members.len() >= min_members.max(1) && view_ids.iter().any(|id| id == &f.view_id));
    // One folder per cell: a later one on an occupied cell is pushed to the right,
    // which the renderer resolves to the first free cell anyway.
    let mut cells: Vec<(String, [u32; 2])> = Vec::new();
    for folder in folders.iter_mut() {
        // Bounded even if every cell in a full grid is already taken: the clamp
        // above keeps this from ever needing more than GRID_MAX steps in practice,
        // but the guard is what actually guarantees termination.
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

// Create (empty id) or update one grid folder in a single pass: name, colour, cell
// and the full member list. Prompts handed in are pulled out of every other folder.
// One parameter per field the grid-folder editor can set.
#[allow(clippy::too_many_arguments)]
#[tauri::command(async)]
pub(crate) fn save_tile_folder(
    app: AppHandle,
    state: State<Db>,
    id: String,
    name: String,
    color: String,
    view_id: String,
    cell: [u32; 2],
    members: Vec<String>,
) -> Result<Settings, String> {
    // A folder may stay unnamed: it then shows the member count alone.
    let name = folder_name_of(&name)?;
    let color = valid_tag_color(&color)?;
    let mut store = lock(&state);
    for pid in &members {
        if !store.prompts.iter().any(|p| &p.id == pid) {
            return Err(format!("unknown {}", brand::ITEM));
        }
    }
    for folder in store.settings.tile_folders.iter_mut() {
        if folder.id != id {
            folder.members.retain(|m| !members.contains(m));
        }
    }
    match store.settings.tile_folders.iter_mut().find(|f| f.id == id) {
        Some(folder) => {
            folder.name = name;
            folder.color = color;
            folder.cell = cell;
            folder.members = members;
        }
        None => {
            if members.len() < 2 {
                return Err(format!("a folder needs at least two {}", brand::ITEM_PLURAL));
            }
            if !store.settings.views.iter().any(|v| v.id == view_id) {
                return Err("unknown view".to_string());
            }
            if store.settings.tile_folders.len() >= MAX_TILE_FOLDERS {
                return Err(format!("max {} folders", MAX_TILE_FOLDERS));
            }
            store.settings.tile_folders.push(TileFolder {
                id: gen_id(),
                name,
                color,
                view_id,
                cell,
                members,
            });
        }
    }
    let ids: Vec<String> = store.prompts.iter().map(|p| p.id.clone()).collect();
    finish_tile_folder_edit(&mut store.settings, &ids);
    save_settings(&app, &store.settings);
    Ok(webview_settings(&store.settings))
}

// The shared tail of a folder edit. A prompt inside a folder is not a tile: its layout
// entries are dropped so its old cell is free. A folder left with one member is about to be
// dropped by normalize, so that member is seated on a cell first, as when a prompt is taken
// out of a folder.
pub(crate) fn finish_tile_folder_edit(settings: &mut Settings, prompt_ids: &[String]) {
    let inside: Vec<String> = settings.tile_folders.iter().flat_map(|f| f.members.iter().cloned()).collect();
    for view in settings.views.iter_mut() {
        for layout in view.layouts.values_mut() {
            layout.retain(|pid, _| !inside.contains(pid));
        }
    }
    settle_lone_folder_members(&mut settings.views, &settings.tile_folders);
    let view_ids: Vec<String> = settings.views.iter().map(|v| v.id.clone()).collect();
    normalize_tile_folders(&mut settings.tile_folders, prompt_ids, &view_ids);
}

// The layout step of taking a prompt out of a grid folder onto `cell`. Whoever
// already sits on that cell moves to the first free cell (folder cells count as
// taken); with no free cell left the drop is refused and nothing changes. A cell
// outside the grid places nothing, so the prompt stays library-only.
pub(crate) fn place_taken_out_member(
    view: &mut View,
    folder_cells: &[[u32; 2]],
    id: &str,
    cell: [u32; 2],
) -> Result<(), ()> {
    if cell[0] >= view.cols || cell[1] >= view.rows {
        return Ok(());
    }
    let key = grid_key(view.cols, view.rows);
    let occupants: Vec<String> = view
        .layouts
        .get(&key)
        .map(|l| l.iter().filter(|(_, c)| **c == cell).map(|(k, _)| k.clone()).collect())
        .unwrap_or_default();
    let mut moved: Vec<(String, [u32; 2])> = Vec::new();
    let mut scratch = view.clone();
    for occupant in occupants {
        let Some(free) = first_free_cell_excluding(&scratch, folder_cells) else {
            return Err(());
        };
        scratch.layouts.entry(key.clone()).or_default().insert(occupant.clone(), free);
        moved.push((occupant, free));
    }
    let layout = view.layouts.entry(key).or_default();
    for (occupant, free) in moved {
        layout.insert(occupant, free);
    }
    layout.insert(id.to_string(), cell);
    Ok(())
}

// Where a prompt taken out of a folder really lands. A cell a folder still holds after the
// member left is not free: folders live outside view.layouts, so the tile would save but
// never draw, and it takes the first free cell instead. The origin folder counts only while
// it keeps two or more members (below that it dissolves and its last member is settled
// afterwards).
pub(crate) fn take_out_target_cell(
    view: &View,
    folders: &[TileFolder],
    folder_cells: &[[u32; 2]],
    id: &str,
    cell: [u32; 2],
) -> Result<[u32; 2], ()> {
    let held = folders
        .iter()
        .any(|f| f.view_id == view.id && f.cell == cell && f.members.iter().filter(|m| m.as_str() != id).count() >= 2);
    if held {
        first_free_cell_excluding(view, folder_cells).ok_or(())
    } else {
        Ok(cell)
    }
}

// Take one prompt out of its folder and put it on `cell` of the folder's view. The
// second-to-last member leaving dissolves the folder (normalize), which is what makes
// "drag the last one out" work without a case of its own.
#[tauri::command(async)]
pub(crate) fn take_out_of_tile_folder(
    app: AppHandle,
    state: State<Db>,
    id: String,
    cell: [u32; 2],
) -> Result<Settings, String> {
    let mut store = lock(&state);
    let view_id = match store
        .settings
        .tile_folders
        .iter()
        .find(|f| f.members.contains(&id))
    {
        Some(f) => f.view_id.clone(),
        None => return Err("not in a folder".to_string()),
    };
    // Every folder cell of that view lives outside view.layouts; a displaced
    // occupant must not be handed one of them.
    let folder_cells: Vec<[u32; 2]> = store
        .settings
        .tile_folders
        .iter()
        .filter(|f| f.view_id == view_id)
        .map(|f| f.cell)
        .collect();
    // Decide the placement first: a drop that cannot be honoured leaves the
    // member in its folder instead of half-moving it.
    let folders = store.settings.tile_folders.clone();
    if let Some(view) = store.settings.views.iter_mut().find(|v| v.id == view_id) {
        let Ok(cell) = take_out_target_cell(view, &folders, &folder_cells, &id, cell) else {
            return Err("no free cell".to_string());
        };
        if place_taken_out_member(view, &folder_cells, &id, cell).is_err() {
            return Err("no free cell".to_string());
        }
    }
    for folder in store.settings.tile_folders.iter_mut() {
        folder.members.retain(|m| m != &id);
    }
    let settings = &mut store.settings;
    settle_lone_folder_members(&mut settings.views, &settings.tile_folders);
    let ids: Vec<String> = store.prompts.iter().map(|p| p.id.clone()).collect();
    let view_ids: Vec<String> = store.settings.views.iter().map(|v| v.id.clone()).collect();
    normalize_tile_folders(&mut store.settings.tile_folders, &ids, &view_ids);
    save_settings(&app, &store.settings);
    Ok(webview_settings(&store.settings))
}

// A folder left with one member is about to be dropped; that member lives outside
// view.layouts, so it gets the folder's own cell (or the first free one when the
// taken-out prompt was put there) instead of vanishing from the grid. A member the
// view's own `members` list does not name yet joins it, as when a folder dissolves.
pub(crate) fn settle_lone_folder_members(views: &mut [View], folders: &[TileFolder]) {
    for folder in folders.iter().filter(|f| f.members.len() == 1) {
        let Some(view) = views.iter_mut().find(|v| v.id == folder.view_id) else { continue };
        let other_cells: Vec<[u32; 2]> = folders
            .iter()
            .filter(|f| f.view_id == folder.view_id && f.id != folder.id && f.members.len() >= 2)
            .map(|f| f.cell)
            .collect();
        let key = grid_key(view.cols, view.rows);
        let in_grid = folder.cell[0] < view.cols && folder.cell[1] < view.rows;
        let taken = view.layouts.get(&key).is_some_and(|l| l.values().any(|c| *c == folder.cell));
        let cell = if in_grid && !taken { Some(folder.cell) } else { first_free_cell_excluding(view, &other_cells) };
        if let Some(cell) = cell {
            view.layouts.entry(key).or_default().insert(folder.members[0].clone(), cell);
            if !view.members.contains(&folder.members[0]) {
                view.members.push(folder.members[0].clone());
            }
        }
    }
}

// Gives each member of a dissolved folder the first free cell of its view. A member
// the view's own `members` list does not name yet (a folder can hold a prompt the
// view never listed) joins it, since it now sits on that view's grid.
pub(crate) fn seat_dissolved_members(view: &mut View, members: &[String], other_folder_cells: &[[u32; 2]]) {
    let key = grid_key(view.cols, view.rows);
    for member in members {
        // Grid full: this and any remaining member stay library-only, same as a
        // brand-new prompt would.
        let Some(cell) = first_free_cell_excluding(view, other_folder_cells) else { break };
        view.layouts.entry(key.clone()).or_default().insert(member.clone(), cell);
        if !view.members.contains(member) {
            view.members.push(member.clone());
        }
    }
}

// Dissolve a grid folder: its prompts become tiles again, nothing is deleted. Each member
// lands on the first free cell of the folder's view (usually including the folder's own,
// free by then). Cells held by other folders in that view are excluded too: they live
// outside view.layouts, so first_free_cell alone would hand a member a cell another folder
// owns and it would never appear.
#[tauri::command(async)]
pub(crate) fn delete_tile_folder(app: AppHandle, state: State<Db>, id: String) -> Settings {
    let mut store = lock(&state);
    let removed = store.settings.tile_folders.iter().find(|f| f.id == id).cloned();
    store.settings.tile_folders.retain(|f| f.id != id);
    if let Some(folder) = removed {
        let other_folder_cells: Vec<[u32; 2]> = store
            .settings
            .tile_folders
            .iter()
            .filter(|f| f.view_id == folder.view_id)
            .map(|f| f.cell)
            .collect();
        if let Some(view) = store.settings.views.iter_mut().find(|v| v.id == folder.view_id) {
            seat_dissolved_members(view, &folder.members, &other_folder_cells);
        }
    }
    save_settings(&app, &store.settings);
    webview_settings(&store.settings)
}
