use super::*;

// ---------- folder commands ----------

// Repositions a folder pill the way a mouse drag does, for the folder window's arrow-key move
// (folder.js), which has no native OS drag to hand off to. The Moved handler on the folder
// window (build_folder_window) persists the new position, as it does for a drag.
#[tauri::command]
pub(crate) async fn set_folder_position(app: AppHandle, id: String, x: f64, y: f64, clamp: Option<bool>) -> Result<(), String> {
    if [x, y].iter().all(|v: &f64| v.is_finite()) {
        if let Some(win) = app.get_webview_window(&folder_label(&id)) {
            let (x, y) = if clamp.unwrap_or(false) {
                let dpi = win.scale_factor().unwrap_or(1.0);
                let size = window_rect(&win).map(|r| (r.w, r.h)).unwrap_or((0.0, 0.0));
                let work = work_rect_at(&app, x * dpi, y * dpi);
                let (cx, cy) = clamp_to_work(work, size.0, size.1, x * dpi, y * dpi);
                (cx / dpi, cy / dpi)
            } else {
                (x, y)
            };
            let _ = win.set_position(tauri::LogicalPosition::new(x, y));
        }
    }
    Ok(())
}

// What the dragged pill covers: the id it would merge into, every other pill/folder
// box (for the live highlight), and the dragged pill's own rect.
pub(crate) type DragHit = (String, Vec<(String, Rect)>, Rect);

// What the dragged pill currently covers: every other pill/folder box plus the one
// `merge_target` picks. ONE hit function for both the live highlight and the drop, so
// what lights up while dragging is exactly what letting go would merge.
pub(crate) fn drag_hit(app: &AppHandle, id: &str, radius: f64) -> Option<DragHit> {
    let win = app.get_webview_window(&flabel(id))?;
    let dragged = window_rect(&win)?;
    let dpi = win.scale_factor().unwrap_or(1.0);
    let mut targets: Vec<(String, Rect)> = Vec::new();
    for (label, other) in app.webview_windows() {
        if label == flabel(id) {
            continue;
        }
        let rect = match window_rect(&other) {
            Some(r) => r,
            None => continue,
        };
        if let Some(pid) = label.strip_prefix("float-") {
            targets.push((format!("p{}", pid), rect));
        } else if let Some(fid) = label.strip_prefix("folder-") {
            // An OPEN folder is hit-tested on its icon, never on the grid it grew.
            targets.push((format!("f{}", fid), folder_icon_rect(app, fid, rect)));
        }
    }
    // The radius is a logical distance, so it follows the dragged pill's monitor.
    let hit = merge_target(dragged, &targets, radius * dpi)?.to_string();
    Some((hit, targets, dragged))
}

// Which window a hit key belongs to ("p<prompt>" = pill, "f<folder>" = folder).
pub(crate) fn hit_label(hit: &str) -> String {
    let (kind, key) = hit.split_at(1);
    if kind == "f" {
        folder_label(key)
    } else {
        flabel(key)
    }
}

// Live merge feedback while a pill is dragged. The OS move loop owns the pointer, so
// no webview sees the motion: the backend polls the dragged window instead and lights
// up the window that would swallow it. (generation, currently lit window label)
pub(crate) struct DragHint(pub(crate) Mutex<(u64, Option<String>)>);

pub(crate) const HINT_TICK_MS: u64 = 70;
pub(crate) const HINT_MAX_TICKS: u32 = 700; // ~50 s guard: no drag runs longer

// Move the highlight, telling the old target to drop it. Emits only on a change.
pub(crate) fn light_hint(app: &AppHandle, label: Option<String>) {
    let Some(state) = app.try_state::<DragHint>() else { return };
    let mut cur = state.0.lock().unwrap_or_else(|e| e.into_inner());
    if cur.1 == label {
        return;
    }
    if let Some(old) = cur.1.take() {
        let _ = app.emit_to(old.as_str(), "merge-hint", false);
    }
    if let Some(new) = &label {
        let _ = app.emit_to(new.as_str(), "merge-hint", true);
    }
    cur.1 = label;
}

// The pill window says when its drag starts and ends. Ending also clears the
// highlight, so a drag that dies with the window never leaves one lit.
#[tauri::command]
pub(crate) async fn float_drag_hint(app: AppHandle, state: State<'_, Db>, id: String, active: bool) -> Result<(), String> {
    let generation = {
        let hint: State<DragHint> = app.state();
        let mut cur = hint.0.lock().unwrap_or_else(|e| e.into_inner());
        cur.0 = cur.0.wrapping_add(1);
        cur.0
    };
    if !active {
        light_hint(&app, None);
        return Ok(());
    }
    let (on, radius) = {
        let store = lock(&state);
        (folders_on(&store.settings), MERGE_RADIUS)
    };
    if !on {
        return Ok(());
    }
    let handle = app;
    tauri::async_runtime::spawn(async move {
        for _ in 0..HINT_MAX_TICKS {
            tokio::time::sleep(std::time::Duration::from_millis(HINT_TICK_MS)).await;
            {
                let hint: State<DragHint> = handle.state();
                let cur = hint.0.lock().unwrap_or_else(|e| e.into_inner());
                if cur.0 != generation {
                    return; // a newer drag (or its end) owns the highlight now
                }
            }
            if handle.get_webview_window(&flabel(&id)).is_none() {
                break;
            }
            let hit = drag_hit(&handle, &id, radius).map(|(h, _, _)| hit_label(&h));
            light_hint(&handle, hit);
        }
        light_hint(&handle, None);
    });
    Ok(())
}

// A pill was dropped after a drag: did it land on another pill or on a folder? The
// check has to run HERE: the pills are separate OS windows, so only the backend knows
// where they all are. Called ONCE per drag END, never while dragging.
#[tauri::command]
pub(crate) async fn float_folder_drop(app: AppHandle, state: State<'_, Db>, id: String) -> Result<bool, String> {
    // The drag is over: whatever was lit goes dark, hit or no hit.
    {
        let hint: State<DragHint> = app.state();
        let mut cur = hint.0.lock().unwrap_or_else(|e| e.into_inner());
        cur.0 = cur.0.wrapping_add(1);
    }
    light_hint(&app, None);
    let (on, radius, full) = {
        let store = lock(&state);
        (
            folders_on(&store.settings),
            MERGE_RADIUS,
            store.settings.float_folders.len() >= MAX_FLOAT_FOLDERS,
        )
    };
    if !on {
        return Ok(false);
    }
    let win = match app.get_webview_window(&flabel(&id)) {
        Some(w) => w,
        None => return Ok(false),
    };
    let dpi = win.scale_factor().unwrap_or(1.0);
    let (hit, targets, dragged) = match drag_hit(&app, &id, radius) {
        Some(h) => h,
        None => return Ok(false),
    };
    let (kind, key) = hit.split_at(1);
    // A new folder needs its position first: working out which monitor the drop landed
    // on is a Tauri call, and one of those under the store mutex is the deadlock shape.
    let fresh = if kind == "f" {
        None
    } else {
        if full {
            return Ok(false);
        }
        let scale = {
            let store = lock(&state);
            float_scale_of(&store.settings, key)
        };
        // The new folder takes the target pill's place: centred on it, so it lands
        // where the user aimed: and its icon size mirrors that pill's size.
        let target = targets
            .iter()
            .find(|(t, _)| t == &hit)
            .map(|(_, r)| *r)
            .unwrap_or(dragged);
        let side = folder_box(scale) * dpi;
        let (cx, cy) = target.center();
        let work = work_rect_at(&app, cx, cy);
        let (x, y) = clamp_to_work(work, side, side, cx - side / 2.0, cy - side / 2.0);
        Some(FloatFolder {
            id: gen_id(),
            name: String::new(),
            pos: Pos { x: x.round() as i32, y: y.round() as i32, ..Default::default() },
            scale,
            members: vec![key.to_string(), id.clone()],
        })
    };
    {
        let mut store = lock(&state);
        match fresh {
            Some(folder) => store.settings.float_folders.push(folder),
            None => match store.settings.float_folders.iter_mut().find(|f| f.id == key) {
                Some(folder) => {
                    if !folder.members.iter().any(|m| m == &id) {
                        folder.members.push(id);
                    }
                }
                None => return Ok(false),
            },
        }
        let ids: Vec<String> = store.prompts.iter().map(|p| p.id.clone()).collect();
        normalize_float_folders(&mut store.settings.float_folders, &ids);
        save_settings(&app, &store.settings);
    }
    sync_float_windows(&app);
    Ok(true)
}

// Everything the folder window needs to draw itself.
#[derive(Serialize)]
pub(crate) struct FolderView {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) scale: f64,
    pub(crate) cols: u32,
    pub(crate) members: Vec<Prompt>,
}

#[tauri::command(async)]
pub(crate) fn float_folder_state(state: State<Db>, id: String) -> Option<FolderView> {
    let store = lock(&state);
    let folder = store.settings.float_folders.iter().find(|f| f.id == id)?;
    let members = folder
        .members
        .iter()
        .filter_map(|m| store.prompts.iter().find(|p| &p.id == m).cloned())
        .collect();
    Some(FolderView {
        id: folder.id.clone(),
        name: folder.name.clone(),
        scale: folder.scale,
        cols: FOLDER_COLS,
        members,
    })
}

// The layout an expanded folder ended up with, in LOGICAL pixels for the webview.
#[derive(Serialize)]
pub(crate) struct FolderGeom {
    pub(crate) side: String,
    pub(crate) icon_x: f64,
    pub(crate) icon_y: f64,
    pub(crate) pop_x: f64,
    pub(crate) pop_y: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
}

// Open the folder by growing its own window around the icon, so z-order and focus stay as
// simple as a single pill's. The popover size comes in as logical pixels and is scaled with
// this window's factor, so the call is correct on a monitor with a different DPI.
#[tauri::command]
pub(crate) async fn expand_float_folder(
    app: AppHandle,
    id: String,
    width: f64,
    height: f64,
    gap: f64,
) -> Result<FolderGeom, String> {
    if ![width, height, gap].iter().all(|v| v.is_finite() && *v >= 0.0) {
        return Err("bad popover size".to_string());
    }
    let win = app
        .get_webview_window(&folder_label(&id))
        .ok_or_else(|| "unknown folder".to_string())?;
    let dpi = win.scale_factor().unwrap_or(1.0);
    let current = window_rect(&win).ok_or_else(|| "no window geometry".to_string())?;
    let icon = folder_icon_rect(&app, &id, current);
    let (cx, cy) = icon.center();
    let work = work_rect_at(&app, cx, cy);
    let layout = folder_layout(
        icon,
        (width * dpi).clamp(1.0, 20000.0),
        (height * dpi).clamp(1.0, 20000.0),
        gap * dpi,
        work,
    );
    set_folder_expansion(
        &app,
        &id,
        Some(Rect { x: layout.icon.0, y: layout.icon.1, w: icon.w, h: icon.h }),
    );
    set_bounds_phys(&win, layout.win);
    Ok(FolderGeom {
        side: layout.side.as_str().to_string(),
        icon_x: layout.icon.0 / dpi,
        icon_y: layout.icon.1 / dpi,
        pop_x: layout.pop.0 / dpi,
        pop_y: layout.pop.1 / dpi,
        width: layout.win.w / dpi,
        height: layout.win.h / dpi,
    })
}

// Shrink the window back to the icon, wherever the icon currently is.
#[tauri::command]
pub(crate) async fn collapse_float_folder(app: AppHandle, state: State<'_, Db>, id: String) -> Result<(), String> {
    let win = match app.get_webview_window(&folder_label(&id)) {
        Some(w) => w,
        None => return Ok(()),
    };
    let current = match window_rect(&win) {
        Some(r) => r,
        None => return Ok(()),
    };
    let icon = folder_icon_rect(&app, &id, current);
    set_folder_expansion(&app, &id, None);
    set_bounds_phys(&win, icon);
    // The window may have been dragged while it was open, so store where the icon
    // ended up rather than where the folder was opened.
    let mut store = lock(&state);
    if let Some(folder) = store.settings.float_folders.iter_mut().find(|f| f.id == id) {
        folder.pos = Pos { x: icon.x.round() as i32, y: icon.y.round() as i32, ..Default::default() };
    }
    save_settings(&app, &store.settings);
    Ok(())
}

// Per-folder size, mirroring the per-pill float_scale.
#[tauri::command]
pub(crate) async fn set_float_folder_scale(
    app: AppHandle,
    state: State<'_, Db>,
    id: String,
    scale: f64,
) -> Result<(), String> {
    let scale = if scale.is_finite() { scale.clamp(0.5, 3.0) } else { 1.0 };
    let pos = {
        let mut store = lock(&state);
        let found = store
            .settings
            .float_folders
            .iter_mut()
            .find(|f| f.id == id)
            .map(|f| {
                f.scale = scale;
                f.pos.clone()
            });
        match found {
            Some(p) => {
                save_settings(&app, &store.settings);
                p
            }
            None => return Err("unknown folder".to_string()),
        }
    };
    if let Some(win) = app.get_webview_window(&folder_label(&id)) {
        let dpi = win.scale_factor().unwrap_or(1.0);
        let side = folder_box(scale) * dpi;
        set_folder_expansion(&app, &id, None); // a size change closes the open grid
        set_bounds_phys(&win, Rect { x: pos.x as f64, y: pos.y as f64, w: side, h: side });
        let _ = app.emit_to(folder_label(&id).as_str(), "folder-changed", ());
    }
    Ok(())
}

// The optional label under the folder icon. Control characters are stripped: the name
// also ends up as the window title.
#[tauri::command(async)]
pub(crate) fn rename_float_folder(app: AppHandle, state: State<Db>, id: String, name: String) -> Result<(), String> {
    let name = folder_name_of(&name)?;
    {
        let mut store = lock(&state);
        match store.settings.float_folders.iter_mut().find(|f| f.id == id) {
            Some(folder) => folder.name.clone_from(&name),
            None => return Err("unknown folder".to_string()),
        }
        save_settings(&app, &store.settings);
    }
    if let Some(win) = app.get_webview_window(&folder_label(&id)) {
        let _ = win.set_title(&name);
    }
    let _ = app.emit_to(folder_label(&id).as_str(), "folder-changed", ());
    Ok(())
}

// A folder window has no themed dialog of its own, so the rename is asked for in the
// MAIN window (confirmDialog) and applied from there: never a native message box.
#[tauri::command]
pub(crate) async fn folder_rename_request(app: AppHandle, state: State<'_, Db>, id: String) -> Result<(), String> {
    let name = {
        let store = lock(&state);
        store
            .settings
            .float_folders
            .iter()
            .find(|f| f.id == id)
            .map(|f| f.name.clone())
    };
    let name = name.ok_or_else(|| "unknown folder".to_string())?;
    show_main(&app);
    let _ = app.emit_to(
        "main",
        "folder-rename",
        serde_json::json!({ "id": id, "name": name }),
    );
    Ok(())
}

// Dissolve a folder: every member goes back to being a free pill. Nothing is deleted.
// Each one asks for the position it kept all along, but they are fanned apart first:
// merging put them on top of each other, so restoring the wishes unchanged would leave
// a stack that looks like a single pill.
#[tauri::command]
pub(crate) async fn dissolve_float_folder(app: AppHandle, state: State<'_, Db>, id: String) -> Result<(), String> {
    let dpi = app
        .get_webview_window(&folder_label(&id))
        .and_then(|w| w.scale_factor().ok())
        .unwrap_or(1.0);
    // Wished-for box of every member, in physical pixels.
    let (members, folder_pos) = {
        let store = lock(&state);
        let folder = match store.settings.float_folders.iter().find(|f| f.id == id) {
            Some(f) => f.clone(),
            None => return Ok(()),
        };
        let members: Vec<(String, Rect)> = folder
            .members
            .iter()
            .filter_map(|mid| {
                let prompt = store.prompts.iter().find(|p| &p.id == mid)?;
                let (w, h) = pill_dims(is_image_prompt(prompt), float_scale_of(&store.settings, mid));
                let pos = store.settings.floating.get(mid).cloned().unwrap_or_else(|| folder.pos.clone());
                Some((
                    mid.clone(),
                    Rect { x: pos.x as f64, y: pos.y as f64, w: w * dpi, h: h * dpi },
                ))
            })
            .collect();
        (members, folder.pos)
    };
    // Pills and folders already on screen are obstacles: this folder's own window is
    // about to close, so it is not one.
    let taken: Vec<Rect> = app
        .webview_windows()
        .iter()
        .filter(|(label, _)| {
            (label.starts_with("float-") || label.starts_with("folder-")) && *label != &folder_label(&id)
        })
        .filter_map(|(_, win)| window_rect(win))
        .collect();
    let side = folder_box(1.0) * dpi;
    let work = work_rect_at(&app, folder_pos.x as f64 + side / 2.0, folder_pos.y as f64 + side / 2.0);
    let wishes: Vec<Rect> = members.iter().map(|(_, r)| *r).collect();
    let spread = spread_free_positions(&wishes, &taken, work);
    {
        let mut store = lock(&state);
        let before = store.settings.float_folders.len();
        store.settings.float_folders.retain(|f| f.id != id);
        if store.settings.float_folders.len() == before {
            return Ok(());
        }
        for ((mid, _), (x, y)) in members.iter().zip(spread) {
            store
                .settings
                .floating
                .insert(mid.clone(), Pos { x: x.round() as i32, y: y.round() as i32, ..Default::default() });
        }
        save_settings(&app, &store.settings);
    }
    set_folder_expansion(&app, &id, None);
    sync_float_windows(&app);
    Ok(())
}

// A member was dragged out of the open folder: it becomes a free pill at the drop
// point. x/y are the pointer's screen position in the webview's CSS pixels, converted
// with the folder window's own scale factor and then clamped onto the work area it was
// dropped on. Dropping the second-to-last member dissolves the folder (normalize).
#[tauri::command]
pub(crate) async fn remove_folder_member(
    app: AppHandle,
    state: State<'_, Db>,
    folder_id: String,
    id: String,
    x: f64,
    y: f64,
) -> Result<(), String> {
    if !x.is_finite() || !y.is_finite() {
        return Err("bad drop position".to_string());
    }
    let dpi = app
        .get_webview_window(&folder_label(&folder_id))
        .and_then(|w| w.scale_factor().ok())
        .unwrap_or(1.0);
    let (px, py) = (x * dpi, y * dpi);
    let (pw, ph) = {
        let store = lock(&state);
        let prompt = match store.prompts.iter().find(|p| p.id == id) {
            Some(p) => p,
            None => return Err(format!("unknown {}", brand::ITEM)),
        };
        if !store
            .settings
            .float_folders
            .iter()
            .any(|f| f.id == folder_id && f.members.contains(&id))
        {
            return Err("not a member of this folder".to_string());
        }
        pill_dims(is_image_prompt(prompt), float_scale_of(&store.settings, &id))
    };
    let work = work_rect_at(&app, px, py);
    let (nx, ny) = clamp_to_work(work, pw * dpi, ph * dpi, px - pw * dpi / 2.0, py - ph * dpi / 2.0);
    {
        let mut store = lock(&state);
        for folder in store.settings.float_folders.iter_mut() {
            if folder.id == folder_id {
                folder.members.retain(|m| m != &id);
            }
        }
        store
            .settings
            .floating
            .insert(id, Pos { x: nx.round() as i32, y: ny.round() as i32, ..Default::default() });
        let ids: Vec<String> = store.prompts.iter().map(|p| p.id.clone()).collect();
        normalize_float_folders(&mut store.settings.float_folders, &ids);
        save_settings(&app, &store.settings);
    }
    sync_float_windows(&app);
    Ok(())
}

// Persist the per-prompt video player state (volume, mute, loop).
// Async: can fire while scrubbing; keep the settings write off the UI thread.
#[tauri::command]
pub(crate) async fn set_video_prefs(app: AppHandle, state: State<'_, Db>, id: String, volume: u32, muted: bool, looped: bool) -> Result<(), String> {
    let mut store = lock(&state);
    store
        .settings
        .video_prefs
        .insert(id, VideoPrefs { volume: volume.min(100), muted, looped });
    save_settings(&app, &store.settings);
    Ok(())
}
