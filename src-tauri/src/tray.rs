use super::*;

// ---------- tray quick-copy sections ----------

// A prompt name ends up in an OS menu, so it is stripped and capped first.
pub(crate) const TRAY_LABEL_MAX: usize = 40;
// How far back from the cap tray_label may look for a word break.
pub(crate) const TRAY_LABEL_WORD_BACK: usize = 12;
// Namespace for the dynamic entries. Every other menu id is a fixed keyword, so a
// click can be routed back to a prompt without ever guessing.
pub(crate) const TRAY_COPY_PREFIX: &str = "psc:";
// How long a burst of store changes is collected before the menu is rebuilt.
pub(crate) const TRAY_DEBOUNCE_MS: u64 = 400;

// Whether the installed menu currently carries quick-copy entries, and whether a
// rebuild is already scheduled. Both flags off + nothing installed = no work at all.
pub(crate) static TRAY_EXTRAS: AtomicBool = AtomicBool::new(false);
pub(crate) static TRAY_PENDING: AtomicBool = AtomicBool::new(false);

// What the two sections should contain, read from the expert settings.
pub(crate) struct TrayCfg {
    pub(crate) recent: bool,
    pub(crate) recent_n: usize,
    pub(crate) favs: bool,
    pub(crate) fav_n: usize,
    pub(crate) label_max: usize,
}

pub(crate) fn tray_cfg(settings: &Settings) -> TrayCfg {
    let count = |key: &str, def: f64| {
        settings.ui_values.get(key).copied().unwrap_or(def).clamp(1.0, 10.0) as usize
    };
    TrayCfg {
        recent: settings.ui_flags.get("trayRecent") != Some(&false), // on by default
        recent_n: count("trayRecentN", 5.0),
        favs: settings.ui_flags.get("trayFavs") != Some(&false), // on by default
        fav_n: count("trayFavN", 10.0),
        label_max: tray_label_max(settings),
    }
}

// Expert-tunable cap for tray_label, clamped the same way every other
// ui_values-backed expert row is: an absent/out-of-range stored value falls
// back to the shipped default rather than reaching tray_label unclamped.
pub(crate) fn tray_label_max(settings: &Settings) -> usize {
    settings.ui_values.get("trayLabelMax").copied().unwrap_or(TRAY_LABEL_MAX as f64).clamp(20.0, 80.0) as usize
}

// Whether the tray menu's filter row is drawn at all.
pub(crate) fn tray_search_on(settings: &Settings) -> bool {
    settings.ui_flags.get("traySearch") != Some(&false) // on by default
}

// Menu text for a prompt. Control characters (a pasted newline lands in the name)
// collapse to spaces, `&` is doubled because a Win32 menu reads a single one as a
// mnemonic, and the cut counts CHARACTERS: a byte cut would split a multi-byte
// name in the middle of a character. An over-long name is cut back to the last word
// break in the tail rather than mid-word, so the entry still reads as words.
pub(crate) fn tray_label(name: &str, max: usize) -> String {
    let flat: String = name.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let mut text = flat.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() > max {
        let mut cut: String = text.chars().take(max - 1).collect();
        if let Some(space) = cut.rfind(' ') {
            // Only when the break is near the end: otherwise a single long word
            // would shrink the label to almost nothing.
            if cut[..space].chars().count() >= max.saturating_sub(TRAY_LABEL_WORD_BACK) {
                cut.truncate(space);
            }
        }
        text = cut.trim_end().to_string();
        text.push('…');
    }
    if text.is_empty() {
        text.push('\u{2014}'); // a nameless prompt still needs something clickable
    }
    text.replace('&', "&&")
}

pub(crate) fn tray_item_id(prompt_id: &str) -> String {
    format!("{TRAY_COPY_PREFIX}{prompt_id}")
}

// Route a clicked menu id back to a prompt. Anything that is not one of our own
// quick-copy ids is ignored instead of being looked up.
pub(crate) fn tray_copy_target(menu_id: &str) -> Option<&str> {
    match menu_id.strip_prefix(TRAY_COPY_PREFIX) {
        Some(id) if !id.is_empty() => Some(id),
        _ => None,
    }
}

// What a quick-copy entry actually delivers. It decides the entry's menu icon, so
// the media kind of a prompt is readable straight from the tray.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum TrayKind {
    Text,
    Image,
    Media,
    File,
}

// The image fields are not exclusive: a picture can be the payload (copy_image), the
// tile's face (show_image) or just a decoration next to the text. Only the first two
// make the entry a picture: a decorated text prompt still copies text.
pub(crate) fn tray_kind(p: &Prompt) -> TrayKind {
    if !p.file_path.trim().is_empty() {
        TrayKind::File
    } else if !p.icon_path.trim().is_empty() {
        TrayKind::Media
    } else if !p.image.is_empty() && (p.copy_image || p.show_image) {
        TrayKind::Image
    } else {
        TrayKind::Text
    }
}

// One quick-copy entry, ready to become a menu item.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct TrayEntry {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) kind: TrayKind,
    // The prompt's own tile colour ("" = tint the icon by kind instead).
    pub(crate) color: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum TraySection {
    Favorites,
    Recent,
}

// The menu's dynamic part, in the order it is drawn: pinned favourites first, then
// recents: the Windows jump-list idiom, so the curated section never moves under
// the user while the recents below it churn. A favourite is not repeated in the
// recents; those backfill with the next unlisted prompt instead of leaving a gap.
// Pure, so ordering, caps, dedupe and truncation stay testable: the menu API cannot.
pub(crate) fn tray_plan(
    prompts: &[Prompt],
    last_used: &HashMap<String, u64>,
    cfg: &TrayCfg,
) -> Vec<(TraySection, Vec<TrayEntry>)> {
    let entry = |p: &Prompt| TrayEntry {
        id: tray_item_id(&p.id),
        label: tray_label(&p.name, cfg.label_max),
        kind: tray_kind(p),
        color: p.color.clone(),
    };
    let mut out = Vec::new();
    let mut listed: Vec<&str> = Vec::new();
    if cfg.favs {
        let mut starred: Vec<&Prompt> = prompts.iter().filter(|p| p.favorite).collect();
        starred.sort_by_key(|p| (p.name.to_lowercase(), p.id.clone()));
        starred.truncate(cfg.fav_n);
        listed.extend(starred.iter().map(|p| p.id.as_str()));
        let favs: Vec<TrayEntry> = starred.into_iter().map(entry).collect();
        if !favs.is_empty() {
            out.push((TraySection::Favorites, favs));
        }
    }
    if cfg.recent {
        let mut used: Vec<(&Prompt, u64)> = prompts
            .iter()
            .filter(|p| !listed.contains(&p.id.as_str()))
            .filter_map(|p| last_used.get(&p.id).map(|ts| (p, *ts)))
            .collect();
        // Same second for two prompts: fall back to the name so the order is stable.
        used.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.name.cmp(&b.0.name)));
        used.truncate(cfg.recent_n);
        let recent: Vec<TrayEntry> = used.into_iter().map(|(p, _)| entry(p)).collect();
        if !recent.is_empty() {
            out.push((TraySection::Recent, recent));
        }
    }
    out
}

// ---------- menu icons ----------
// A menu icon is a 16x16 chip: the prompt's tile colour (or the media kind's own
// tint) with a glyph on top. It is FULLY OPAQUE on purpose. The Win32 menu path
// turns an icon into a bitmap through a monochrome AND mask that is only built
// correctly when every pixel is opaque, so a glyph with transparent padding comes
// out striped instead of clean.
pub(crate) const TRAY_ICON_PX: u32 = 16;

pub(crate) fn hex_rgb(hex: &str) -> Option<[u8; 3]> {
    let c = hex_to_color(hex.trim())?;
    Some([c.0, c.1, c.2])
}

// Perceived brightness, 0..255: decides whether the glyph is drawn light or dark.
pub(crate) fn tray_luma(c: [u8; 3]) -> u32 {
    (c[0] as u32 * 299 + c[1] as u32 * 587 + c[2] as u32 * 114) / 1000
}

// Blend `c` toward `to` by `amount` percent.
pub(crate) fn tray_mix(c: [u8; 3], to: [u8; 3], amount: u32) -> [u8; 3] {
    let f = |a: u8, b: u8| ((a as u32 * (100 - amount) + b as u32 * amount) / 100) as u8;
    [f(c[0], to[0]), f(c[1], to[1]), f(c[2], to[2])]
}

pub(crate) fn tray_kind_tint(kind: TrayKind) -> [u8; 3] {
    match kind {
        TrayKind::Text => [90, 100, 114],
        TrayKind::Image => [47, 111, 237],
        TrayKind::Media => [139, 92, 246],
        TrayKind::File => [224, 138, 30],
    }
}

// A 16x16 chip under construction. Every write is bounds-checked, so a glyph that
// runs over an edge is clipped instead of wrapping onto the next row.
pub(crate) struct TrayChip {
    pub(crate) px: Vec<u8>,
}

impl TrayChip {
    pub(crate) fn new(fill: [u8; 3]) -> Self {
        let n = (TRAY_ICON_PX * TRAY_ICON_PX) as usize;
        let mut px = Vec::with_capacity(n * 4);
        for _ in 0..n {
            px.extend_from_slice(&[fill[0], fill[1], fill[2], 255]);
        }
        Self { px }
    }
    pub(crate) fn dot(&mut self, x: i32, y: i32, c: [u8; 3]) {
        let n = TRAY_ICON_PX as i32;
        if x < 0 || y < 0 || x >= n || y >= n {
            return;
        }
        let i = ((y * n + x) * 4) as usize;
        self.px[i] = c[0];
        self.px[i + 1] = c[1];
        self.px[i + 2] = c[2];
    }
    pub(crate) fn row(&mut self, x0: i32, x1: i32, y: i32, c: [u8; 3]) {
        for x in x0..=x1 {
            self.dot(x, y, c);
        }
    }
    pub(crate) fn bar(&mut self, x0: i32, x1: i32, y0: i32, y1: i32, c: [u8; 3]) {
        for y in y0..=y1 {
            self.row(x0, x1, y, c);
        }
    }
    pub(crate) fn frame(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, c: [u8; 3]) {
        self.row(x0, x1, y0, c);
        self.row(x0, x1, y1, c);
        for y in y0..=y1 {
            self.dot(x0, y, c);
            self.dot(x1, y, c);
        }
    }
}

// The 16x16 RGBA chip for one entry. `color` is the prompt's tile colour; anything
// unparseable falls back to the kind's own tint.
pub(crate) fn tray_icon_pixels(kind: TrayKind, color: &str) -> Vec<u8> {
    let fill = hex_rgb(color).unwrap_or_else(|| tray_kind_tint(kind));
    let dark = tray_luma(fill) < 150;
    let ink = if dark { [255, 255, 255] } else { [24, 26, 31] };
    let edge = tray_mix(fill, if dark { [255, 255, 255] } else { [0, 0, 0] }, 28);
    let mut chip = TrayChip::new(fill);
    chip.frame(0, 0, 15, 15, edge);
    match kind {
        // Three text lines, the last one short: a paragraph at 16 px.
        TrayKind::Text => {
            chip.bar(4, 11, 4, 5, ink);
            chip.bar(4, 11, 7, 8, ink);
            chip.bar(4, 8, 10, 11, ink);
        }
        // Picture frame with a sun and a hill inside.
        TrayKind::Image => {
            chip.frame(3, 4, 12, 11, ink);
            chip.bar(5, 6, 6, 7, ink);
            for y in 7..=10 {
                let half = y - 7;
                chip.row((9 - half).max(4), (9 + half).min(11), y, ink);
            }
        }
        // Play triangle: rows taper symmetrically toward the tip on the right.
        TrayKind::Media => {
            for y in 3i32..=12 {
                let off = (2 * y - 15).abs(); // distance from the 7.5 centre, doubled
                chip.row(5, 11 - (off * 6) / 9, y, ink);
            }
        }
        // Page with a folded top-right corner and two ruled lines.
        TrayKind::File => {
            for y in 3..=13 {
                let right = if y < 7 { 7 + (y - 3) } else { 11 };
                chip.row(4, right, y, ink);
            }
            chip.bar(6, 9, 8, 8, fill);
            chip.bar(6, 9, 11, 11, fill);
        }
    }
    chip.px
}

pub(crate) fn tray_icon(kind: TrayKind, color: &str) -> tauri::image::Image<'static> {
    tauri::image::Image::new_owned(tray_icon_pixels(kind, color), TRAY_ICON_PX, TRAY_ICON_PX)
}

// One labelled section: a greyed header, the entries with their icons, a separator.
pub(crate) fn push_tray_section(
    app: &AppHandle,
    items: &mut Vec<Box<dyn IsMenuItem<tauri::Wry>>>,
    head_id: &str,
    title: &str,
    entries: &[TrayEntry],
) {
    if entries.is_empty() {
        return;
    }
    if let Ok(head) = MenuItem::with_id(app, head_id, title, false, None::<&str>) {
        items.push(Box::new(head));
    }
    for e in entries {
        if let Ok(item) = IconMenuItem::with_id(
            app,
            e.id.as_str(),
            e.label.as_str(),
            true,
            Some(tray_icon(e.kind, &e.color)),
            None::<&str>,
        ) {
            items.push(Box::new(item));
        }
    }
    if let Ok(sep) = PredefinedMenuItem::separator(app) {
        items.push(Box::new(sep));
    }
}

// The custom Windows-11-style tray window replaces the native right-click
// popup. Ships on (a rounder, themed look was the whole point of building it);
// switching it off is the escape hatch back to the plain OS menu if the
// custom window ever misbehaves.
pub(crate) const TRAY_CUSTOM_MENU_KEY: &str = "trayCustomMenu";

pub(crate) fn tray_custom_menu_on(settings: &Settings) -> bool {
    settings.ui_flags.get(TRAY_CUSTOM_MENU_KEY) != Some(&false)
}

// Build the tray menu from the current store and install it. The store lock is
// dropped before the first menu call: building a menu while holding it is the
// classic deadlock shape here.
pub(crate) fn apply_tray_menu(app: &AppHandle) {
    let (lang, plan, custom, rec_entries_on) = match app.try_state::<Db>() {
        Some(state) => {
            let store = lock(&state);
            let cfg = tray_cfg(&store.settings);
            let plan = tray_plan(&store.prompts, &store.settings.last_used, &cfg);
            let rec_entries_on = store.settings.ui_flags.get("screenRecord") != Some(&false)
                && store.settings.ui_flags.get("recTrayEntries") != Some(&false);
            (resolve_lang(&store.settings.language), plan, tray_custom_menu_on(&store.settings), rec_entries_on)
        }
        None => return,
    };
    if custom {
        // The custom window builds its own content live from tray_window_plan()
        // on every right-click, so there is nothing to precompute here: just
        // make sure no native popup is left attached to fight it for the
        // right-click. Detaching also skips every rebuild this function would
        // otherwise do while the toggle stays on.
        if let Some(tray) = app.tray_by_id("tray") {
            let _ = tray.set_menu::<Menu<tauri::Wry>>(None);
        }
        return;
    }
    TRAY_EXTRAS.store(!plan.is_empty(), Ordering::SeqCst);
    let (open_label, quit_label) = tray_labels(lang);
    let (recent_title, favs_title) = tray_section_labels(lang);
    let mut items: Vec<Box<dyn IsMenuItem<tauri::Wry>>> = Vec::new();
    // While a recording runs, two extra entries at the very top -- the
    // tray-menu counterpart of the bar's own Stop/Pause buttons, same commands.
    if rec_entries_on {
        if let Some(phase) = recording_phase(app) {
            let paused = phase == RecPhase::Paused;
            if let (Ok(stop), Ok(pause)) = (
                MenuItem::with_id(app, "rec-stop", tray_rec_stop_label(lang), true, None::<&str>),
                MenuItem::with_id(app, "rec-pause-toggle", tray_rec_pause_label(lang, paused), true, None::<&str>),
            ) {
                items.push(Box::new(stop));
                items.push(Box::new(pause));
                if let Ok(sep) = PredefinedMenuItem::separator(app) {
                    items.push(Box::new(sep));
                }
            }
        }
    }
    for (section, entries) in &plan {
        let (head_id, title) = match section {
            TraySection::Favorites => ("tray-head-favs", favs_title),
            TraySection::Recent => ("tray-head-recent", recent_title),
        };
        push_tray_section(app, &mut items, head_id, title, entries);
    }
    let (Ok(show), Ok(quit)) = (
        MenuItem::with_id(app, "show", open_label, true, None::<&str>),
        MenuItem::with_id(app, "quit", quit_label, true, None::<&str>),
    ) else {
        return;
    };
    items.push(Box::new(show));
    items.push(Box::new(quit));
    let refs: Vec<&dyn IsMenuItem<tauri::Wry>> = items.iter().map(|b| b.as_ref()).collect();
    if let (Some(tray), Ok(menu)) = (app.tray_by_id("tray"), Menu::with_items(app, &refs)) {
        let _ = tray.set_menu(Some(menu));
    }
}

// A tray menu is static, so anything that changes prompts, stars or usage has to
// rebuild it. Bursts (a batch action, an import, a run of copies) are collected
// into a single rebuild off the UI thread; with both features off and no entries
// installed there is nothing to do at all.
pub(crate) fn rebuild_tray(app: &AppHandle) {
    let wanted = match app.try_state::<Db>() {
        Some(state) => {
            let cfg = tray_cfg(&lock(&state).settings);
            cfg.recent || cfg.favs
        }
        None => false,
    };
    if !wanted && !TRAY_EXTRAS.load(Ordering::SeqCst) {
        return; // the legacy menu is already what the settings ask for
    }
    if TRAY_PENDING.swap(true, Ordering::SeqCst) {
        return; // a rebuild is already due and will pick these changes up
    }
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(TRAY_DEBOUNCE_MS));
        TRAY_PENDING.store(false, Ordering::SeqCst);
        apply_tray_menu(&app);
    });
}

// A tray entry runs the very same activation a tile click does (copy, usage,
// chains, placeholder dialog): but never the hotkey auto-paste: the click leaves
// the tray in the foreground, so there is no text field to paste into.
pub(crate) fn fire_tray_copy(app: &AppHandle, id: &str) {
    let _ = app.emit_to("main", "prompt-tray-copy", id.to_string());
}

// One place for what a tray menu id DOES, shared by the native popup's
// on_menu_event and the custom tray window's tray_window_activate command --
// the two menus must never drift into behaving differently for the same id.
pub(crate) fn handle_tray_menu_id(app: &AppHandle, id: &str) {
    match id {
        "show" => show_main(app),
        "quit" => request_app_quit(app),
        "rec-stop" => trigger_rec_stop_hotkey(app),
        "rec-pause-toggle" => trigger_rec_pause_hotkey(app),
        other => {
            if let Some(id) = tray_copy_target(other) {
                fire_tray_copy(app, id);
            }
        }
    }
}

// ---------- Custom Windows-11-style tray window ----------
// The native TrackPopupMenu cannot be rounded or restyled -- that is a shell/XAML
// look, not something Win32 menus support. apply_tray_menu() detaches the native
// menu entirely while trayCustomMenu is on (see there), and this window takes
// over the right-click instead, drawing its OWN content from the plan below.

pub(crate) const TRAY_WIN_LABEL: &str = "tray-menu";
pub(crate) const TRAY_WIN_W: f64 = 240.0; // logical px; keep in sync with .tray-menu-card in floating.css
pub(crate) const TRAY_WIN_ROW_H: f64 = 30.0;
pub(crate) const TRAY_WIN_HEAD_H: f64 = 22.0;
pub(crate) const TRAY_WIN_SEP_H: f64 = 9.0; // 1px line + 4px margin top/bottom, one per section
pub(crate) const TRAY_WIN_CHROME: f64 = 20.0; // card padding, independent of row count

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TrayWinEntry {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) kind: String,
    pub(crate) color: String,
}

#[derive(Serialize, Clone)]
pub(crate) struct TrayWinSection {
    pub(crate) title: String,
    pub(crate) entries: Vec<TrayWinEntry>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TrayWinPlan {
    // 0 or 2 entries (Stop, Pause/Resume) -- no header, own separator,
    // rendered before every other row. ui/tray.js draws them with no icon,
    // exactly like show/quit below.
    pub(crate) rec_entries: Vec<TrayWinEntry>,
    pub(crate) sections: Vec<TrayWinSection>,
    pub(crate) show_label: String,
    pub(crate) quit_label: String,
    // Whether the search row at the top of the menu is drawn at all (traySearch,
    // default on) -- tray_window_logical_height only reserves a row for it when
    // this is true, so an unrelated off switch never grows the window for nothing.
    pub(crate) search_on: bool,
    pub(crate) search_ph: String,
}

pub(crate) fn tray_kind_str(kind: TrayKind) -> &'static str {
    match kind {
        TrayKind::Text => "text",
        TrayKind::Image => "image",
        TrayKind::Media => "media",
        TrayKind::File => "file",
    }
}

// Same content, same order as the native menu (tray_plan/tray_labels) -- this is
// a restyle, not a second menu with its own rules. Built fresh on every open, so
// (unlike the native HMENU) there is no separate rebuild-on-change path to keep
// in sync.
pub(crate) fn build_tray_win_plan(app: &AppHandle) -> TrayWinPlan {
    let state: State<Db> = match app.try_state::<Db>() {
        Some(s) => s,
        None => {
            return TrayWinPlan { rec_entries: Vec::new(), sections: Vec::new(), show_label: String::new(), quit_label: String::new(), search_on: false, search_ph: String::new() };
        }
    };
    let phase = recording_phase(app);
    let store = lock(&state);
    tray_win_plan_from_store(&store, phase)
}

// The plan from plain data, so the store lock is the only thing the caller holds
// while this runs and the content stays testable without an app handle. The
// recording phase is read before the lock is taken (it has its own mutex).
pub(crate) fn tray_win_plan_from_store(store: &Store, phase: Option<RecPhase>) -> TrayWinPlan {
    let cfg = tray_cfg(&store.settings);
    let plan = tray_plan(&store.prompts, &store.settings.last_used, &cfg);
    let lang = resolve_lang(&store.settings.language);
    let (open_label, quit_label) = tray_labels(lang);
    let (recent_title, favs_title) = tray_section_labels(lang);
    let rec_entries_on = store.settings.ui_flags.get("screenRecord") != Some(&false)
        && store.settings.ui_flags.get("recTrayEntries") != Some(&false);
    let search_on = tray_search_on(&store.settings);
    let rec_entries = if rec_entries_on {
        match phase {
            Some(phase) => {
                let paused = phase == RecPhase::Paused;
                vec![
                    TrayWinEntry { id: "rec-stop".to_string(), label: tray_rec_stop_label(lang).to_string(), kind: "text".to_string(), color: String::new() },
                    TrayWinEntry { id: "rec-pause-toggle".to_string(), label: tray_rec_pause_label(lang, paused).to_string(), kind: "text".to_string(), color: String::new() },
                ]
            }
            None => Vec::new(),
        }
    } else {
        Vec::new()
    };
    let sections = plan
        .into_iter()
        .map(|(section, entries)| TrayWinSection {
            title: match section {
                TraySection::Favorites => favs_title,
                TraySection::Recent => recent_title,
            }
            .to_string(),
            entries: entries
                .into_iter()
                .map(|e| TrayWinEntry { id: e.id, label: e.label, kind: tray_kind_str(e.kind).to_string(), color: e.color })
                .collect(),
        })
        .collect();
    TrayWinPlan {
        rec_entries,
        sections,
        show_label: open_label.to_string(),
        quit_label: quit_label.to_string(),
        search_on,
        search_ph: tray_search_placeholder(lang).to_string(),
    }
}

// Everything a right-click on the tray icon needs from the store, taken under ONE
// lock: None while the custom window is switched off (the native popup owns the
// click then), otherwise the plan and the capture-exclusion flag. Runs on a worker
// thread: the store lock can be held for a long time by a backup or an index run,
// and waiting for it on the event-loop thread froze every window.
pub(crate) fn tray_open_prep(app: &AppHandle) -> Option<(TrayWinPlan, bool)> {
    let phase = recording_phase(app);
    let state = app.try_state::<Db>()?;
    let store = lock(&state);
    tray_open_prep_from_store(&store, phase)
}

pub(crate) fn tray_open_prep_from_store(store: &Store, phase: Option<RecPhase>) -> Option<(TrayWinPlan, bool)> {
    if !tray_custom_menu_on(&store.settings) {
        return None;
    }
    Some((tray_win_plan_from_store(store, phase), capture_excluded(&store.settings)))
}

// Pure, so the row/header/separator arithmetic stays testable without an app
// handle -- mirrors tray_plan's own reasoning for why it takes plain data.
pub(crate) fn tray_window_logical_height(plan: &TrayWinPlan) -> f64 {
    let mut logical_h = TRAY_WIN_CHROME + 2.0 * TRAY_WIN_ROW_H; // show + quit always present
    if plan.search_on {
        logical_h += TRAY_WIN_ROW_H; // the filter row at the very top
    }
    if !plan.rec_entries.is_empty() {
        logical_h += plan.rec_entries.len() as f64 * TRAY_WIN_ROW_H + TRAY_WIN_SEP_H;
    }
    for s in &plan.sections {
        logical_h += TRAY_WIN_HEAD_H + s.entries.len() as f64 * TRAY_WIN_ROW_H + TRAY_WIN_SEP_H;
    }
    logical_h
}

// clamp_to_work only ever repositions -- it cannot shrink a window taller than
// the work area, so at the supported 10-favorites/10-recents limits (both
// sections on by default) the uncapped content could reach ~742 logical px and
// overhang the bottom of a small or heavily-scaled display, with the card's own
// (former) "overflow: hidden" then making the lower rows -- Quit included --
// impossible to reach. Capped to the work area with an 8px margin so it never
// sits flush against the taskbar/screen edge; floating.css makes the card
// scroll internally once its content no longer fits the capped height. Split
// out from tray_window_rect so the cap math is testable without an AppHandle.
pub(crate) fn tray_window_capped_height(plan: &TrayWinPlan, work_h: f64, dpi: f64) -> f64 {
    (tray_window_logical_height(plan) * dpi).min((work_h - 8.0 * dpi).max(TRAY_WIN_ROW_H * dpi))
}

pub(crate) fn tray_window_rect(app: &AppHandle, plan: &TrayWinPlan) -> Rect {
    let (work, dpi, cursor) = work_and_dpi_at_cursor(app);
    let w = TRAY_WIN_W * dpi;
    let h = tray_window_capped_height(plan, work.h, dpi);
    let (x, y) = clamp_to_work(work, w, h, cursor.x, cursor.y);
    Rect { x, y, w, h }
}

// Runs on the event-loop thread, so it takes the plan prepared by tray_open_prep and
// touches no lock. Opens at the cursor, clamped into that monitor's work area -- same helper every
// other cursor-anchored popup (launcher, var prompt) already uses. Replaces an
// already-open one instead of stacking a second, matching those windows too.
pub(crate) fn open_tray_window(app: &AppHandle, plan: TrayWinPlan, excluded: bool) {
    if let Some(win) = app.get_webview_window(TRAY_WIN_LABEL) {
        let _ = win.close();
    }
    let rect = tray_window_rect(app, &plan);
    let win = match WebviewWindowBuilder::new(app, TRAY_WIN_LABEL, WebviewUrl::App("tray.html".into()))
        .title(brand::DISPLAY_NAME)
        .inner_size(TRAY_WIN_W, TRAY_WIN_W) // placeholder: set_bounds_phys owns the real box
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .shadow(false)
        .focused(true)
        .visible(false) // sized in physical pixels first, so nothing flashes at the wrong size
        .build()
    {
        Ok(w) => w,
        Err(_) => return,
    };
    let _ = win.set_background_color(Some(tauri::webview::Color(0, 0, 0, 0)));
    set_capture_exclusion(&win, excluded);
    set_bounds_phys(&win, rect);
    let _ = win.show();
    let _ = win.set_focus();
}

// Async on purpose: a sync command runs on the event-loop thread, and the plan
// waits for the store lock.
#[tauri::command]
pub(crate) async fn tray_window_plan(app: AppHandle) -> TrayWinPlan {
    build_tray_win_plan(&app)
}

#[tauri::command]
pub(crate) fn tray_window_activate(app: AppHandle, id: String) {
    handle_tray_menu_id(&app, &id);
    if let Some(win) = app.get_webview_window(TRAY_WIN_LABEL) {
        let _ = win.close();
    }
}

#[tauri::command]
pub(crate) fn tray_window_close(app: AppHandle) {
    if let Some(win) = app.get_webview_window(TRAY_WIN_LABEL) {
        let _ = win.close();
    }
}

// The tray filter row hands its first keystroke to the quick launcher: what was typed
// becomes the launcher's starting query and the launcher is built, but it only appears
// (launcher_ready) once its page is up. The tray window keeps the focus and every key
// until then, passes on the complete text (tray_launcher_prefill) the moment the launcher
// takes the focus away, and closes itself after that.
// Must stay async: it builds the launcher window, and window creation from a sync
// command deadlocks on Windows (sync commands run on the main thread).
#[tauri::command]
pub(crate) async fn tray_open_launcher(app: AppHandle, query: String) {
    *LAUNCHER_PREFILL.lock().unwrap_or_else(|e| e.into_inner()) = launcher_prefill_text(&query);
    open_launcher_window(&app, true);
}

// The launcher page has loaded and is listening: now the window may appear and take the
// focus. The tray notices it lost the focus and passes on everything typed so far.
#[tauri::command]
pub(crate) fn launcher_ready(app: AppHandle) {
    reveal_launcher(&app);
}

// Everything typed in the tray before the launcher took the focus: the newer text replaces
// the pending starting query and is pushed to the launcher, which puts it in front of any
// key it received on its own since.
#[tauri::command]
pub(crate) fn tray_launcher_prefill(app: AppHandle, query: String) {
    let text = launcher_prefill_text(&query);
    *LAUNCHER_PREFILL.lock().unwrap_or_else(|e| e.into_inner()) = text.clone();
    let _ = app.emit_to(LAUNCHER_WIN_LABEL, "launcher-prefill", text);
}

// The starting query is user input crossing a window boundary: capped like the launcher's own field.
pub(crate) fn launcher_prefill_text(q: &str) -> String {
    q.chars().take(200).collect()
}

// A result picked in the quick-launcher runs the very same activation the tray
// does (fire_tray_copy) -- placeholder dialog, cooldown, chains, everything already
// tested for "a copy that started outside the grid" -- then the launcher itself closes.
#[tauri::command]
pub(crate) fn launcher_select(app: AppHandle, id: String) -> Result<(), String> {
    fire_tray_copy(&app, &id);
    if let Some(win) = app.get_webview_window(LAUNCHER_WIN_LABEL) {
        let _ = win.close();
    }
    Ok(())
}

#[tauri::command]
pub(crate) fn launcher_close(app: AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window(LAUNCHER_WIN_LABEL) {
        let _ = win.close();
    }
    Ok(())
}
