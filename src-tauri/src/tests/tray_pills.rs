use super::*;

// ---- Tray quick menu ----

#[test]
fn importing_duplicate_prompt_ids_gives_each_one_its_own() {
    // `id` is the prompts table PRIMARY KEY: a duplicate made the whole rewrite
    // fail, and the failure was swallowed into the ignored JSON fallback, so the
    // import "succeeded" and the old prompts came back after a restart.
    let mut ps = sample_prompts();
    ps[1].id = ps[0].id.clone();
    ps.push({
        let mut p = ps[0].clone();
        p.id = String::new(); // a hand-written backup may leave it out entirely
        p
    });
    migrate_prompts(&mut ps);
    let ids: std::collections::HashSet<&str> = ps.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids.len(), ps.len(), "every prompt must end up with a unique id");
    assert!(ps.iter().all(|p| !p.id.is_empty()), "no prompt may be left without an id");
}

#[test]
fn turning_the_old_hiding_off_keeps_screenshots_working() {
    // Stored "captureExclusion: false" meant "let screenshots through"; that has to
    // survive the switch to the positively-phrased flag.
    let mut s = Settings::default();
    s.ui_flags.insert("captureExclusion".into(), false);
    s.migrate();
    assert!(!capture_excluded(&s), "an explicit allow must keep allowing");
    assert!(!s.ui_flags.contains_key("captureExclusion"), "the old key is retired");

    // The old default (hiding on) and a fresh install both stay protected.
    let mut had = Settings::default();
    had.ui_flags.insert("captureExclusion".into(), true);
    had.migrate();
    assert!(capture_excluded(&had));
    let mut fresh = Settings::default();
    fresh.migrate();
    assert!(capture_excluded(&fresh), "the protection is the shipped default");

    // The mapping the live toggle in set_ui_flag has to follow: allow = do NOT
    // exclude. Passing the flag straight to set_capture_exclusion inverts it.
    let mut on = Settings::default();
    on.ui_flags.insert(ALLOW_SCREENSHOT_KEY.into(), true);
    assert!(!capture_excluded(&on), "allowing screenshots must not exclude");
    let mut off = Settings::default();
    off.ui_flags.insert(ALLOW_SCREENSHOT_KEY.into(), false);
    assert!(capture_excluded(&off), "refusing screenshots must exclude");
}

#[test]
fn an_imported_view_cannot_exceed_the_grid_ceiling() {
    // A hand-edited backup used to be able to ask for a grid of u32::MAX cells,
    // which the renderer then tried to build: the window never painted again.
    let mut s = Settings {
        views: vec![test_view("v1"), test_view("v2")],
        ..Default::default()
    };
    s.views[0].cols = u32::MAX;
    s.views[0].rows = 999_999;
    s.views[1].cols = 0; // and the other direction: a grid with no cells at all
    s.views[1].rows = 0;
    s.migrate();
    assert_eq!(s.views[0].cols, GRID_MAX);
    assert_eq!(s.views[0].rows, GRID_MAX);
    assert_eq!(s.views[1].cols, GRID_MIN);
    assert_eq!(s.views[1].rows, GRID_MIN);
    // More views than the ceiling are cut, not carried.
    let mut s = Settings {
        views: (0..MAX_VIEWS + 20).map(|i| test_view(&format!("v{i}"))).collect(),
        ..Default::default()
    };
    s.migrate();
    assert_eq!(s.views.len(), MAX_VIEWS);
}

pub(super) fn tray_prompt(id: &str, name: &str, favorite: bool) -> Prompt {
    let mut p = sample_prompts().remove(1);
    p.id = id.into();
    p.name = name.into();
    p.favorite = favorite;
    p.hotkey = String::new();
    p
}

pub(super) fn tray_all_on(recent_n: usize, fav_n: usize) -> TrayCfg {
    TrayCfg { recent: true, recent_n, favs: true, fav_n, label_max: TRAY_LABEL_MAX }
}

// The entries of one section of the plan, or an empty list if it is not drawn.
pub(super) fn tray_part(plan: &[(TraySection, Vec<TrayEntry>)], want: TraySection) -> Vec<TrayEntry> {
    plan.iter().find(|(s, _)| *s == want).map(|(_, e)| e.clone()).unwrap_or_default()
}
pub(super) fn tray_names(entries: &[TrayEntry]) -> Vec<&str> {
    entries.iter().map(|e| e.label.as_str()).collect()
}

#[test]
fn tray_settings_default_to_both_sections() {
    let cfg = tray_cfg(&Settings::default());
    assert!(cfg.recent, "the recent section ships on");
    assert!(cfg.favs, "the favourites section ships on");
    assert_eq!(cfg.recent_n, 5);
    assert_eq!(cfg.fav_n, 10);

    // Both features off = a menu with no dynamic entries at all.
    let mut s = Settings::default();
    s.ui_flags.insert("trayRecent".into(), false);
    s.ui_flags.insert("trayFavs".into(), false);
    let off = tray_cfg(&s);
    assert!(!off.recent && !off.favs);
    let prompts = vec![tray_prompt("a", "A", true)];
    let mut used = HashMap::new();
    used.insert("a".to_string(), 10u64);
    assert!(tray_plan(&prompts, &used, &off).is_empty());
}

#[test]
fn tray_counts_are_clamped_to_the_slider_range() {
    let mut s = Settings::default();
    s.ui_values.insert("trayRecentN".into(), 999.0);
    s.ui_values.insert("trayFavN".into(), 0.0);
    let cfg = tray_cfg(&s);
    assert_eq!(cfg.recent_n, 10);
    assert_eq!(cfg.fav_n, 1);
}

#[test]
fn tray_search_defaults_on() {
    let s = Settings::default();
    assert!(tray_search_on(&s), "the filter row ships on");

    let mut off = Settings::default();
    off.ui_flags.insert("traySearch".into(), false);
    assert!(!tray_search_on(&off));
}

#[test]
fn launcher_prefill_is_capped_at_200_characters() {
    let long: String = "x".repeat(250);
    assert_eq!(launcher_prefill_text(&long).chars().count(), 200);
    assert_eq!(launcher_prefill_text(""), "");
    assert_eq!(launcher_prefill_text("ab"), "ab");
}

#[test]
fn zoom_min_size_grows_with_zoom_and_shrinks_back() {
    let big_screen = (10_000, 10_000);
    assert_eq!(zoom_min_size(1.0, big_screen), (680, 460));
    assert_eq!(zoom_min_size(2.5, big_screen), (1700, 1150));
    assert_eq!(zoom_min_size(0.75, big_screen), (510, 345));
}

#[test]
fn zoom_min_size_never_exceeds_the_monitor() {
    let small_screen = (1000, 800);
    assert_eq!(zoom_min_size(3.0, small_screen), (1000, 800));
}

#[test]
fn tray_recent_is_newest_first_and_capped() {
    let prompts = vec![
        tray_prompt("p1", "One", false),
        tray_prompt("p2", "Two", false),
        tray_prompt("p3", "Three", false),
        tray_prompt("p4", "Never copied", false),
    ];
    let used: HashMap<String, u64> = [("p1", 100u64), ("p2", 300), ("p3", 200)]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
    let cfg = tray_all_on(2, 10);
    let recent = tray_part(&tray_plan(&prompts, &used, &cfg), TraySection::Recent);
    assert_eq!(tray_names(&recent), vec!["Two", "Three"], "newest first, cut at trayRecentN");
    // A prompt that was never copied has no entry in last_used and stays out.
    let all = tray_part(&tray_plan(&prompts, &used, &tray_all_on(10, 10)), TraySection::Recent);
    assert_eq!(all.len(), 3);
    assert!(all.iter().all(|e| e.id != tray_item_id("p4")));
}

#[test]
fn tray_recent_breaks_ties_by_name() {
    let prompts = vec![tray_prompt("p1", "Beta", false), tray_prompt("p2", "Alpha", false)];
    let used: HashMap<String, u64> =
        [("p1", 50u64), ("p2", 50)].into_iter().map(|(k, v)| (k.to_string(), v)).collect();
    let recent = tray_part(&tray_plan(&prompts, &used, &tray_all_on(10, 10)), TraySection::Recent);
    assert_eq!(tray_names(&recent), vec!["Alpha", "Beta"]);
}

#[test]
fn tray_favorites_are_alphabetical_and_capped() {
    let prompts = vec![
        tray_prompt("p1", "zebra", true),
        tray_prompt("p2", "Apple", true),
        tray_prompt("p3", "Middle", true),
        tray_prompt("p4", "Not starred", false),
    ];
    let favs =
        tray_part(&tray_plan(&prompts, &HashMap::new(), &tray_all_on(5, 2)), TraySection::Favorites);
    assert_eq!(tray_names(&favs), vec!["Apple", "Middle"]);
    let all =
        tray_part(&tray_plan(&prompts, &HashMap::new(), &tray_all_on(5, 10)), TraySection::Favorites);
    assert_eq!(all.len(), 3, "only starred prompts");
    assert_eq!(all[2].id, tray_item_id("p1"), "case-insensitive sort puts zebra last");
}

#[test]
fn the_pinned_section_is_drawn_first_and_never_repeated_below() {
    let prompts = vec![
        tray_prompt("p1", "Starred", true),
        tray_prompt("p2", "Plain", false),
        tray_prompt("p3", "Older", false),
    ];
    let used: HashMap<String, u64> = [("p1", 300u64), ("p2", 200), ("p3", 100)]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
    let plan = tray_plan(&prompts, &used, &tray_all_on(2, 10));
    assert_eq!(
        plan.iter().map(|(s, _)| *s).collect::<Vec<_>>(),
        vec![TraySection::Favorites, TraySection::Recent],
        "pinned favourites sit above the churning recents"
    );
    assert_eq!(tray_names(&plan[0].1), vec!["Starred"]);
    // p1 is the most recent too, but it is already pinned: the recents skip it and
    // backfill with the next unlisted prompt instead of showing a duplicate.
    assert_eq!(tray_names(&plan[1].1), vec!["Plain", "Older"]);
}

#[test]
fn a_tray_entry_carries_the_media_kind_its_icon_is_drawn_from() {
    let mut plain = tray_prompt("p1", "Plain", false);
    plain.file_path = String::new();
    plain.icon_path = String::new();
    plain.image = String::new();
    plain.color = String::new();
    assert_eq!(tray_kind(&plain), TrayKind::Text);

    let mut decorated = plain.clone();
    decorated.image = "data:image/png;base64,AAA".into();
    decorated.show_image = false;
    decorated.copy_image = false;
    assert_eq!(tray_kind(&decorated), TrayKind::Text, "a decorated text prompt still copies text");

    let mut shown = decorated.clone();
    shown.show_image = true;
    assert_eq!(tray_kind(&shown), TrayKind::Image);
    let mut copied = decorated.clone();
    copied.copy_image = true;
    assert_eq!(tray_kind(&copied), TrayKind::Image);

    let mut gif = shown.clone();
    gif.icon_path = "C:/clip.gif".into();
    assert_eq!(tray_kind(&gif), TrayKind::Media, "a gif/video icon outranks a still");

    let mut doc = gif.clone();
    doc.file_path = "C:/report.pdf".into();
    assert_eq!(tray_kind(&doc), TrayKind::File, "an attachment is what the click delivers");

    // The plan carries kind and colour through, so the menu never re-reads the store.
    let mut coloured = doc.clone();
    coloured.color = "#123456".into();
    let plan = tray_plan(&[coloured], &HashMap::new(), &tray_all_on(5, 5));
    assert!(plan.is_empty(), "not starred, never used");
    let mut starred = doc.clone();
    starred.favorite = true;
    starred.color = "#123456".into();
    let plan = tray_plan(&[starred], &HashMap::new(), &tray_all_on(5, 5));
    assert_eq!(plan[0].1[0].kind, TrayKind::File);
    assert_eq!(plan[0].1[0].color, "#123456");
}

#[test]
fn tray_custom_menu_defaults_on_and_the_flag_turns_it_off() {
    let mut settings = Settings::default();
    assert!(tray_custom_menu_on(&settings), "ships on until a user turns it off");
    settings.ui_flags.insert(TRAY_CUSTOM_MENU_KEY.to_string(), false);
    assert!(!tray_custom_menu_on(&settings));
    settings.ui_flags.insert(TRAY_CUSTOM_MENU_KEY.to_string(), true);
    assert!(tray_custom_menu_on(&settings), "explicitly re-enabling it must also work");
}

// The right-click on the tray icon prepares its plan from ONE store reference, off
// the event-loop thread. No red exists for the old shape: the old code took the lock
// twice on the main thread, and a hang needs a second thread holding the store, which
// a unit test cannot stage deterministically. This pins the new contract instead.
#[test]
fn tray_open_prep_skips_while_the_custom_window_is_off_and_carries_the_plan_when_on() {
    let mut store = Store { prompts: Vec::new(), trash: Vec::new(), settings: Settings::default(), prompts_locked: false, written: Default::default() };
    store.settings.ui_flags.insert(ALLOW_SCREENSHOT_KEY.to_string(), true);
    let (plan, excluded) = tray_open_prep_from_store(&store, None).expect("custom window ships on");
    assert!(!excluded, "a window that may be captured is not excluded");
    assert!(!plan.show_label.is_empty() && !plan.quit_label.is_empty());
    assert!(plan.rec_entries.is_empty(), "no recording running, no recording rows");
    let (with_rec, _) = tray_open_prep_from_store(&store, Some(RecPhase::Paused)).unwrap();
    assert_eq!(with_rec.rec_entries.len(), 2, "stop + pause rows while a recording runs");
    store.settings.ui_flags.insert(ALLOW_SCREENSHOT_KEY.to_string(), false);
    assert!(tray_open_prep_from_store(&store, None).unwrap().1, "default keeps the window out of captures");
    store.settings.ui_flags.insert(TRAY_CUSTOM_MENU_KEY.to_string(), false);
    assert!(tray_open_prep_from_store(&store, None).is_none(), "native popup owns the click");
}

// The menu window is built once and reused: a right-click only moves and refills the
// warm window. Building a WebView2 window per click took up to seconds and made the
// menu feel hung. No red exists as a unit test (window creation needs a running app),
// so this pins the shape: the open path builds only when no window exists, and the page
// reveals the window itself after drawing.
#[test]
fn tray_menu_window_is_reused_and_revealed_by_its_page() {
    let src = production_sources();
    let a = src.find("pub(crate) fn open_tray_window").expect("open_tray_window");
    let b = a + src[a..].find("
}
").expect("end of open_tray_window");
    let body = &src[a..b];
    assert!(!body.contains("WebviewWindowBuilder"), "open_tray_window must not build a window itself");
    assert!(!body.contains(".close()"), "the warm window must not be closed on open");
    let get = body.find("get_webview_window(TRAY_WIN_LABEL)").expect("reuses the existing window");
    let build = body.find("build_tray_window(app)").expect("builds only as fallback");
    assert!(get < build);
    // A hidden WebView2 is throttled: the open wakes it off-screen at full size, and
    // only the reveal moves it into place.
    let wake = body.find("set_bounds_phys(&win, tray_wake_rect(rect))").expect("wakes the window off-screen");
    let show = body.find("win.show()").expect("shows the window to wake it");
    assert!(wake < show, "the window must be off-screen before it is shown");
    let r = tray_wake_rect(Rect { x: 100.0, y: 200.0, w: 240.0, h: 300.0 });
    assert!(r.x <= -30000.0 && r.y <= -30000.0 && r.w == 240.0 && r.h == 300.0);
    let c = src.find("pub(crate) fn tray_window_close").expect("tray_window_close");
    let d = c + src[c..].find("
}
").expect("end of tray_window_close");
    assert!(!src[c..d].contains(".close()"), "closing the menu only hides the warm window");
    let js = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../ui/tray.js")).expect("ui/tray.js");
    assert!(js.contains("listen(\"tray-open\"") && js.contains("invoke(\"tray_window_reveal\")"));
}

#[test]
fn tray_pending_plan_is_handed_out_once() {
    let plan = TrayWinPlan { rec_entries: Vec::new(), sections: Vec::new(), show_label: "Show".into(), quit_label: "Quit".into(), search_on: false, search_ph: String::new() };
    *TRAY_WIN_PENDING.lock().unwrap() = Some(plan);
    assert_eq!(tray_window_plan().map(|p| p.show_label), Some("Show".to_string()));
    assert!(tray_window_plan().is_none(), "a second take for the same open gets nothing");
}

#[test]
fn tray_kind_str_round_trips_every_kind_to_a_distinct_lowercase_word() {
    let all = [
        (TrayKind::Text, "text"),
        (TrayKind::Image, "image"),
        (TrayKind::Media, "media"),
        (TrayKind::File, "file"),
    ];
    for (kind, word) in all {
        assert_eq!(tray_kind_str(kind), word);
    }
}

#[test]
fn tray_window_height_accounts_for_every_row_header_and_separator() {
    let entry = |id: &str| TrayWinEntry { id: id.into(), label: id.into(), kind: "text".into(), color: String::new() };
    // No sections at all: just the two terminal rows with the separator between
    // them, plus the card's own padding.
    let empty = TrayWinPlan { rec_entries: Vec::new(), sections: Vec::new(), show_label: "Show".into(), quit_label: "Quit".into(), search_on: false, search_ph: String::new() };
    assert_eq!(tray_window_logical_height(&empty), TRAY_WIN_CHROME + 2.0 * TRAY_WIN_ROW_H + TRAY_WIN_SEP_H);

    // One section, two entries: its own header, both rows, and the one separator
    // that always follows a section (matching push_tray_section's native shape).
    let one_section = TrayWinPlan {
        rec_entries: Vec::new(),
        sections: vec![TrayWinSection { title: "Favorites".into(), entries: vec![entry("a"), entry("b")] }],
        show_label: "Show".into(),
        quit_label: "Quit".into(),
        search_on: false,
        search_ph: String::new(),
    };
    let want_one = TRAY_WIN_CHROME + 2.0 * TRAY_WIN_ROW_H + TRAY_WIN_SEP_H + TRAY_WIN_HEAD_H + 2.0 * TRAY_WIN_ROW_H + TRAY_WIN_SEP_H;
    assert_eq!(tray_window_logical_height(&one_section), want_one);

    // Two sections must each add their own header, rows and separator: never
    // shared, since the native menu (push_tray_section) never shares one either.
    let two_sections = TrayWinPlan {
        rec_entries: Vec::new(),
        sections: vec![
            TrayWinSection { title: "Favorites".into(), entries: vec![entry("a")] },
            TrayWinSection { title: "Recent".into(), entries: vec![entry("b"), entry("c"), entry("d")] },
        ],
        show_label: "Show".into(),
        quit_label: "Quit".into(),
        search_on: false,
        search_ph: String::new(),
    };
    let want_two = TRAY_WIN_CHROME
        + 2.0 * TRAY_WIN_ROW_H + TRAY_WIN_SEP_H // show + quit and the line between them
        + (TRAY_WIN_HEAD_H + 1.0 * TRAY_WIN_ROW_H + TRAY_WIN_SEP_H) // 1-entry section
        + (TRAY_WIN_HEAD_H + 3.0 * TRAY_WIN_ROW_H + TRAY_WIN_SEP_H); // 3-entry section
    assert_eq!(tray_window_logical_height(&two_sections), want_two);
}

#[test]
fn tray_window_height_adds_one_row_for_search_when_on() {
    // search_on toggles ONE extra row (the filter field), on top of whatever
    // sections/rec_entries already contribute: proven here against the
    // otherwise-identical plan so a future change to the row math cannot
    // silently forget the search row.
    let base = TrayWinPlan {
        rec_entries: Vec::new(),
        sections: vec![TrayWinSection { title: "Favorites".into(), entries: vec![TrayWinEntry { id: "a".into(), label: "a".into(), kind: "text".into(), color: String::new() }] }],
        show_label: "Show".into(),
        quit_label: "Quit".into(),
        search_on: false,
        search_ph: String::new(),
    };
    let with_search = TrayWinPlan { search_on: true, ..base.clone() };
    assert_eq!(
        tray_window_logical_height(&with_search) - tray_window_logical_height(&base),
        TRAY_WIN_ROW_H,
        "turning the search row on must add exactly one row of height, no more"
    );
}

#[test]
fn tray_window_capped_height_never_exceeds_the_work_area() {
    let entry = |id: &str| TrayWinEntry { id: id.into(), label: id.into(), kind: "text".into(), color: String::new() };
    // Max entries in both sections (the supported ceiling): uncapped this is
    // ~742 logical px, comfortably past a 1366x768 @150% work area.
    let full = TrayWinPlan {
        rec_entries: Vec::new(),
        sections: vec![
            TrayWinSection { title: "Favorites".into(), entries: (0..10).map(|i| entry(&i.to_string())).collect() },
            TrayWinSection { title: "Recent".into(), entries: (0..10).map(|i| entry(&i.to_string())).collect() },
        ],
        show_label: "Show".into(),
        quit_label: "Quit".into(),
        search_on: false,
        search_ph: String::new(),
    };
    let dpi = 1.5;
    let uncapped = tray_window_logical_height(&full) * dpi;
    let small_work_h = 680.0 * dpi; // a real reported 1366x768@150% work-area height
    let h = tray_window_capped_height(&full, small_work_h, dpi);
    assert!(uncapped > small_work_h, "the fixture must actually be taller than the work area, or this test proves nothing");
    assert!(h <= small_work_h, "the window must never be sized past the work area it opens in: got {h}, work area {small_work_h}");
    assert!(h > 0.0, "must still leave a usable window, not collapse to nothing");

    // A tall enough monitor must not clip content that would otherwise fit.
    let roomy = tray_window_capped_height(&full, 2000.0, dpi);
    assert_eq!(roomy, uncapped, "plenty of room must not shrink the window below its real content height");
}

#[test]
fn a_menu_icon_is_a_full_opaque_16x16_chip() {
    // Opaque everywhere is not cosmetic: the Win32 icon->bitmap path only builds a
    // correct mask when it is, so a transparent pixel would show up as a stripe.
    for kind in [TrayKind::Text, TrayKind::Image, TrayKind::Media, TrayKind::File] {
        let px = tray_icon_pixels(kind, "");
        assert_eq!(px.len(), (TRAY_ICON_PX * TRAY_ICON_PX * 4) as usize);
        assert!(px.chunks_exact(4).all(|p| p[3] == 255), "{kind:?} has a transparent pixel");
    }
    // Each media kind is visibly a different chip.
    let mut seen: Vec<Vec<u8>> = Vec::new();
    for kind in [TrayKind::Text, TrayKind::Image, TrayKind::Media, TrayKind::File] {
        let px = tray_icon_pixels(kind, "");
        assert!(!seen.contains(&px), "{kind:?} draws the same chip as another kind");
        seen.push(px);
    }
    // The prompt's own colour wins over the kind tint, and the glyph flips to stay
    // readable on it: dark ink on a near-white chip, light ink on a near-black one.
    let white = tray_icon_pixels(TrayKind::Text, "#FFFFFF");
    let black = tray_icon_pixels(TrayKind::Text, "#000000");
    let ink = |px: &[u8], x: usize, y: usize| px[(y * 16 + x) * 4];
    assert_eq!(ink(&white, 5, 4), 24, "dark glyph on a light chip");
    assert_eq!(ink(&black, 5, 4), 255, "light glyph on a dark chip");
    // The same crossover as the web tray and the tiles: the ink with the higher contrast wins.
    for light in ["#f59e0b", "#eab308", "#22c55e"] {
        assert_eq!(ink(&tray_icon_pixels(TrayKind::Text, light), 5, 4), 24, "dark glyph on {light}");
    }
    for dark in ["#64748b", "#7c3aed", "#1e3a8a"] {
        assert_eq!(ink(&tray_icon_pixels(TrayKind::Text, dark), 5, 4), 255, "light glyph on {dark}");
    }
    // A junk colour must not blank the icon; it falls back to the kind tint.
    assert_eq!(tray_icon_pixels(TrayKind::Text, "not a colour"), tray_icon_pixels(TrayKind::Text, ""));
}

#[test]
fn tray_labels_are_cut_by_character_not_by_byte() {
    // Every char is 3 bytes: a byte cut would split one and panic.
    let cjk = "字".repeat(80);
    let cut = tray_label(&cjk, TRAY_LABEL_MAX);
    assert_eq!(cut.chars().count(), TRAY_LABEL_MAX);
    assert!(cut.ends_with('…'));
    for name in ["Привет мир ".repeat(20), "नमस्ते दुनिया ".repeat(20), "🙂🙃".repeat(60)] {
        let out = tray_label(&name, TRAY_LABEL_MAX);
        assert!(out.chars().count() <= TRAY_LABEL_MAX, "{out:?} is too long");
        assert!(!out.is_empty());
    }
    // Short names pass through untouched.
    assert_eq!(tray_label("Mail", TRAY_LABEL_MAX), "Mail");
    // Control characters collapse into single spaces, the name never spans lines.
    assert_eq!(tray_label("a\nb\tc\u{7}d", TRAY_LABEL_MAX), "a b c d");
    // A Win32 menu reads a lone & as a mnemonic marker.
    assert_eq!(tray_label("R&D", TRAY_LABEL_MAX), "R&&D");
    // A nameless prompt still gets something clickable.
    assert!(!tray_label("   ", TRAY_LABEL_MAX).is_empty());
    // An over-long name is cut back to the last word break near the cap, so the
    // entry ends on a whole word instead of mid-word.
    let cut = tray_label("Summarise this article into five concise bullet points", TRAY_LABEL_MAX);
    assert_eq!(cut, "Summarise this article into five…");
    // A single very long word has no usable break and is still cut hard.
    let word = tray_label(&"x".repeat(60), TRAY_LABEL_MAX);
    assert_eq!(word.chars().count(), TRAY_LABEL_MAX);
}

#[test]
fn tray_label_respects_a_custom_max() {
    // A single long word has no word break to back up to, so it is cut to
    // exactly whatever max is passed in: not the TRAY_LABEL_MAX const.
    let word = "x".repeat(60);
    assert_eq!(tray_label(&word, 20).chars().count(), 20);
    assert_eq!(tray_label(&word, 80).chars().count(), 60); // fits under 80, untouched
}

#[test]
fn ink_prefs_default_on_with_a_clamped_threshold() {
    let s = Settings::default();
    let prefs = ink_prefs_of(&s);
    assert!(prefs.auto, "the automatic ink rule ships on");
    assert_eq!(prefs.threshold, 25.0);

    let mut off = Settings::default();
    off.ui_flags.insert("autoInk".into(), false);
    assert!(!ink_prefs_of(&off).auto);

    let mut clamped_hi = Settings::default();
    clamped_hi.ui_values.insert("inkThreshold".into(), 999.0);
    assert_eq!(ink_prefs_of(&clamped_hi).threshold, 90.0);

    let mut clamped_lo = Settings::default();
    clamped_lo.ui_values.insert("inkThreshold".into(), 0.0);
    assert_eq!(ink_prefs_of(&clamped_lo).threshold, 20.0);
}

#[test]
fn ink_halo_default_on_with_a_clamped_strength() {
    let s = Settings::default();
    let prefs = ink_prefs_of(&s);
    assert!(prefs.halo, "the ink halo ships on");
    assert_eq!(prefs.halo_strength, 75.0);

    let mut off = Settings::default();
    off.ui_flags.insert("inkHalo".into(), false);
    assert!(!ink_prefs_of(&off).halo);

    let mut clamped_hi = Settings::default();
    clamped_hi.ui_values.insert("inkHaloStrength".into(), 999.0);
    assert_eq!(ink_prefs_of(&clamped_hi).halo_strength, 100.0);

    let mut clamped_lo = Settings::default();
    clamped_lo.ui_values.insert("inkHaloStrength".into(), 0.0);
    assert_eq!(ink_prefs_of(&clamped_lo).halo_strength, 10.0);
}

#[test]
fn tray_menu_ids_round_trip_and_reject_junk() {
    assert_eq!(tray_copy_target(&tray_item_id("abc123")), Some("abc123"));
    // A prompt id is opaque, so the parser must not stop at the first separator.
    assert_eq!(tray_copy_target(&tray_item_id("a:b:c")), Some("a:b:c"));
    for bad in ["show", "quit", "", "psc", "psc:", "PSC:x", "copy:x", ":x", "tray-head-recent"] {
        assert_eq!(tray_copy_target(bad), None, "accepted a bad menu id: {bad:?}");
    }
}

// ---- Pill folders ----

// A pill 200x80 at (x, y). Physical pixels, like every folder computation.
pub(super) fn pill_at(x: f64, y: f64) -> Rect {
    Rect { x, y, w: 200.0, h: 80.0 }
}

pub(super) fn folder_of(id: &str, members: &[&str]) -> FloatFolder {
    FloatFolder {
        id: id.into(),
        name: String::new(),
        pos: Pos { x: 100, y: 100, ..Default::default() },
        scale: 1.0,
        members: members.iter().map(|s| s.to_string()).collect(),
    }
}

pub(super) fn ids(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

#[test]
fn a_drop_on_top_of_another_pill_merges() {
    // Dropped 30px away: overlapping and inside the default 40px radius.
    let dragged = pill_at(130.0, 100.0);
    let others = vec![("other".to_string(), pill_at(100.0, 100.0))];
    assert_eq!(merge_target(dragged, &others, 40.0), Some("other"));
}

#[test]
fn a_small_drag_never_merges() {
    // The pill moved four pixels and touches nothing else: the case that must
    // never turn a nudge into a folder.
    let others = vec![
        ("a".to_string(), pill_at(600.0, 100.0)),
        ("b".to_string(), pill_at(100.0, 400.0)),
    ];
    assert_eq!(merge_target(pill_at(104.0, 100.0), &others, 40.0), None);
    // And a pill that stayed exactly where it was matches nothing either.
    assert_eq!(merge_target(pill_at(100.0, 100.0), &others, 40.0), None);
}

#[test]
fn merge_needs_both_the_radius_and_a_real_overlap() {
    let target = pill_at(100.0, 100.0);
    let others = vec![("other".to_string(), target)];
    // Centre distance exactly on the radius still counts as a hit ...
    assert_eq!(merge_target(pill_at(140.0, 100.0), &others, 40.0), Some("other"));
    // ... one pixel further does not.
    assert_eq!(merge_target(pill_at(141.0, 100.0), &others, 40.0), None);
    // Overlapping but far apart at the centres (a long text pill whose end just
    // touches) is not a merge either.
    let wide = vec![("wide".to_string(), Rect { x: 0.0, y: 100.0, w: 260.0, h: 80.0 })];
    assert!(pill_at(250.0, 100.0).overlaps(&wide[0].1));
    assert_eq!(merge_target(pill_at(250.0, 100.0), &wide, 40.0), None);
    // Two small pills close enough by centre distance but with a gap between them:
    // near is not on top of, so nothing merges.
    let small = Rect { x: 100.0, y: 100.0, w: 10.0, h: 10.0 };
    let apart = vec![("apart".to_string(), Rect { x: 130.0, y: 100.0, w: 10.0, h: 10.0 })];
    assert!(!small.overlaps(&apart[0].1));
    assert_eq!(merge_target(small, &apart, 40.0), None);
}

#[test]
fn the_nearest_pill_wins_a_crowded_drop() {
    let dragged = pill_at(100.0, 100.0);
    let others = vec![
        ("far".to_string(), pill_at(130.0, 100.0)),
        ("near".to_string(), pill_at(110.0, 100.0)),
        ("out".to_string(), pill_at(400.0, 100.0)),
    ];
    assert_eq!(merge_target(dragged, &others, 40.0), Some("near"));
}

#[test]
fn folder_invariants_are_restored_and_idempotent() {
    let known = ids(&["a", "b", "c", "d", "e"]);
    let mut folders = vec![
        folder_of("f1", &["a", "gone", "b"]),
        folder_of("f2", &["b", "c", "d"]), // b already belongs to f1
        folder_of("f3", &["e"]),           // a single member is not a folder
        folder_of("f4", &["ghost", "phantom"]),
    ];
    normalize_float_folders(&mut folders, &known);
    assert_eq!(folders.len(), 2);
    assert_eq!(folders[0].members, ids(&["a", "b"]));
    assert_eq!(folders[1].id, "f2");
    assert_eq!(folders[1].members, ids(&["c", "d"]));
    // Running it again changes nothing.
    let before: Vec<Vec<String>> = folders.iter().map(|f| f.members.clone()).collect();
    normalize_float_folders(&mut folders, &known);
    let after: Vec<Vec<String>> = folders.iter().map(|f| f.members.clone()).collect();
    assert_eq!(before, after);
}

#[test]
fn a_prompt_lives_in_at_most_one_folder() {
    let known = ids(&["a", "b", "c"]);
    let mut folders = vec![folder_of("f1", &["a", "b"]), folder_of("f2", &["a", "c"])];
    normalize_float_folders(&mut folders, &known);
    assert_eq!(folders[0].members, vec!["a", "b"]);
    // f2 lost the duplicate and is down to one member, so it dissolves.
    assert_eq!(folders.len(), 1);
}

#[test]
fn a_broken_folder_scale_is_clamped() {
    let known = ids(&["a", "b"]);
    let mut folders = vec![folder_of("f1", &["a", "b"])];
    folders[0].scale = f64::NAN;
    normalize_float_folders(&mut folders, &known);
    assert_eq!(folders[0].scale, 1.0);
    folders[0].scale = 99.0;
    normalize_float_folders(&mut folders, &known);
    assert_eq!(folders[0].scale, 3.0);
}

#[test]
fn deleting_a_prompt_purges_it_from_its_folder() {
    let mut settings = Settings::default();
    settings.ui_flags.insert("folders".into(), true);
    settings.float_folders = vec![folder_of("f1", &["a", "b", "c"]), folder_of("f2", &["d", "e"])];
    // delete_prompt needs an AppHandle, so exercise the same steps it performs.
    let mut alive = ids(&["a", "b", "c", "d", "e"]);
    alive.retain(|p| p != "c");
    normalize_float_folders(&mut settings.float_folders, &alive);
    assert_eq!(settings.float_folders[0].members, vec!["a", "b"]);
    // Losing a second member leaves one behind, so that folder dissolves and its
    // pill goes back to the free position it never gave up.
    alive.retain(|p| p != "d");
    normalize_float_folders(&mut settings.float_folders, &alive);
    assert_eq!(settings.float_folders.len(), 1);
    assert_eq!(settings.float_folders[0].id, "f1");
}

#[test]
fn the_window_plan_follows_the_feature_flag() {
    let mut settings = Settings::default();
    for id in ["a", "b", "c"] {
        settings.floating.insert(id.to_string(), Pos { x: 0, y: 0, ..Default::default() });
    }
    settings.float_folders = vec![folder_of("f1", &["a", "b"])];
    let known = ids(&["a", "b", "c"]);

    // On (the shipped default): the members lose their own window, the folder
    // gets one.
    let (pills, folders) = float_window_plan(&settings, &known);
    assert_eq!(pills, ids(&["c"]));
    assert_eq!(folders, ids(&["f1"]));

    // Off: every member is a plain pill again, no folder; switching the feature
    // off must put every pill back.
    settings.ui_flags.insert("folders".into(), false);
    let (pills, folders) = float_window_plan(&settings, &known);
    assert_eq!(pills, ids(&["a", "b", "c"]));
    assert!(folders.is_empty());
    settings.ui_flags.remove("folders");

    // An unpinned prompt never gets a window, folder member or not.
    settings.floating.remove("c");
    let (pills, _) = float_window_plan(&settings, &known);
    assert!(pills.is_empty());
}

#[test]
fn folder_layout_keeps_its_fixed_defaults() {
    let s = Settings::default();
    assert!(folders_on(&s), "the feature ships on");
    assert_eq!(MERGE_RADIUS, 40.0);
    assert_eq!(FOLDER_COLS, 3);
}

#[test]
fn folder_names_are_trimmed_and_stripped() {
    assert_eq!(folder_name_of("  Work  ").unwrap(), "Work");
    assert_eq!(folder_name_of("").unwrap(), "");
    // A pasted newline or control byte never reaches the window title.
    assert_eq!(folder_name_of("Wo\nrk\u{7}").unwrap(), "Work");
    assert!(folder_name_of(&"a".repeat(MAX_FOLDER_NAME)).is_ok());
    assert!(folder_name_of(&"a".repeat(MAX_FOLDER_NAME + 1)).is_err());
}

// A 1920x1080 primary screen with a 40px taskbar at the bottom.
pub(super) fn primary_work() -> Rect {
    Rect { x: 0.0, y: 0.0, w: 1920.0, h: 1040.0 }
}

pub(super) const ICON: f64 = 96.0;

pub(super) fn icon_at(x: f64, y: f64) -> Rect {
    Rect { x, y, w: ICON, h: ICON }
}

#[test]
fn a_folder_opens_below_when_there_is_room() {
    let icon = icon_at(400.0, 200.0);
    let l = folder_layout(icon, 260.0, 180.0, 6.0, primary_work());
    assert_eq!(l.side, FolderSide::Below);
    // The icon never moves: it still sits at its own screen position.
    assert_eq!(l.win.x + l.icon.0, icon.x);
    assert_eq!(l.win.y + l.icon.1, icon.y);
    // The popover hangs under it with the gap in between, aligned with the icon's
    // left edge: so the window origin IS the icon and nothing shifts sideways
    // when it opens.
    assert_eq!(l.win.y + l.pop.1, icon.bottom() + 6.0);
    assert_eq!(l.win.x + l.pop.0, icon.x);
    assert_eq!(l.icon, (0.0, 0.0));
    // One window covers both.
    assert_eq!(l.win.h, ICON + 6.0 + 180.0);
    assert_eq!(l.win.w, 260.0);
}

#[test]
fn opening_a_folder_never_moves_the_window_origin_sideways() {
    // Anywhere with room to the right, the popover starts at the icon's own left
    // edge: the origin stays put, so the icon cannot jump left/right for a frame.
    for x in [0.0, 400.0, 1000.0] {
        let icon = icon_at(x, 200.0);
        let l = folder_layout(icon, 230.0, 140.0, 6.0, primary_work());
        assert_eq!(l.win.x, icon.x, "origin moved at x={}", x);
        assert_eq!(l.icon.0, 0.0);
    }
}

#[test]
fn a_folder_at_the_bottom_edge_flips_above() {
    let icon = icon_at(400.0, 900.0); // bottom at 996, work ends at 1040
    let l = folder_layout(icon, 260.0, 180.0, 6.0, primary_work());
    assert_eq!(l.side, FolderSide::Above);
    assert_eq!(l.win.y + l.pop.1, icon.y - 6.0 - 180.0);
    assert_eq!(l.win.x + l.icon.0, icon.x);
    assert_eq!(l.win.y + l.icon.1, icon.y);
    // Still fully inside the work area.
    assert!(l.win.y >= primary_work().y && l.win.bottom() <= primary_work().bottom());
}

#[test]
fn a_tall_grid_in_the_middle_flips_sideways() {
    // Neither below nor above fits a 900px grid, so it goes to the right ...
    let icon = icon_at(400.0, 500.0);
    let l = folder_layout(icon, 300.0, 900.0, 6.0, primary_work());
    assert_eq!(l.side, FolderSide::Right);
    assert_eq!(l.win.x + l.pop.0, icon.right() + 6.0);
    // ... and to the left when the right edge is too close.
    let icon = icon_at(1700.0, 500.0);
    let l = folder_layout(icon, 300.0, 900.0, 6.0, primary_work());
    assert_eq!(l.side, FolderSide::Left);
    assert_eq!(l.win.x + l.pop.0, icon.x - 6.0 - 300.0);
    assert!(l.win.x >= 0.0 && l.win.right() <= 1920.0);
}

#[test]
fn a_folder_in_a_corner_keeps_the_whole_popover_on_screen() {
    let work = primary_work();
    for icon in [icon_at(0.0, 0.0), icon_at(1824.0, 0.0), icon_at(0.0, 944.0), icon_at(1824.0, 944.0)] {
        let l = folder_layout(icon, 400.0, 300.0, 6.0, work);
        assert!(l.win.x >= work.x, "left edge escaped at {:?}", icon);
        assert!(l.win.y >= work.y, "top edge escaped at {:?}", icon);
        assert!(l.win.right() <= work.right(), "right edge escaped at {:?}", icon);
        assert!(l.win.bottom() <= work.bottom(), "bottom edge escaped at {:?}", icon);
        // Whatever had to move, it was never the icon.
        assert_eq!(l.win.x + l.icon.0, icon.x);
        assert_eq!(l.win.y + l.icon.1, icon.y);
    }
}

#[test]
fn a_second_monitor_at_another_dpi_flips_on_its_own_edges() {
    // A 150% 2560x1440 screen to the RIGHT of the primary one: its work area
    // starts at x=1920, and every size arrives multiplied by 1.5.
    let work = Rect { x: 1920.0, y: 0.0, w: 2560.0, h: 1380.0 };
    let icon = Rect { x: 2000.0, y: 1200.0, w: ICON * 1.5, h: ICON * 1.5 };
    let l = folder_layout(icon, 260.0 * 1.5, 180.0 * 1.5, 6.0 * 1.5, work);
    // Below would run past that monitor's work area, so it flips there too.
    assert_eq!(l.side, FolderSide::Above);
    assert!(l.win.x >= work.x && l.win.right() <= work.right());
    assert!(l.win.y >= work.y && l.win.bottom() <= work.bottom());
    // A folder in that screen's top-left corner opens below and stays on it:
    // the primary monitor to the left is never used as spare room.
    let icon = Rect { x: 1930.0, y: 20.0, w: ICON * 1.5, h: ICON * 1.5 };
    let l = folder_layout(icon, 600.0, 200.0, 9.0, work);
    assert_eq!(l.side, FolderSide::Below);
    assert!(l.win.x >= work.x, "the popover leaked onto the primary monitor");
}

#[test]
fn a_popover_larger_than_the_screen_still_starts_on_it() {
    let work = primary_work();
    let l = folder_layout(icon_at(900.0, 500.0), 4000.0, 4000.0, 6.0, work);
    assert_eq!(l.win.x + l.pop.0, work.x);
    assert_eq!(l.win.y + l.pop.1, work.y);
}

#[test]
fn dissolving_a_folder_never_stacks_its_pills() {
    let work = primary_work();
    let pill = |x: f64, y: f64| Rect { x, y, w: 200.0, h: 80.0 };
    // Two members whose stored position is the SAME spot: exactly what merging
    // one pill onto another leaves behind.
    let out = spread_free_positions(&[pill(500.0, 400.0), pill(500.0, 400.0)], &[], work);
    assert_eq!(out[0], (500.0, 400.0), "the first one keeps its wish");
    assert_ne!(out[0], out[1]);
    let a = pill(out[0].0, out[0].1);
    let b = pill(out[1].0, out[1].1);
    assert!(!a.overlaps(&b), "the pills still cover each other: {:?} {:?}", a, b);
    // Both stay on screen.
    for (x, y) in out {
        assert!(x >= work.x && x + 200.0 <= work.right());
        assert!(y >= work.y && y + 80.0 <= work.bottom());
    }
    // A pill already on screen is an obstacle too.
    let out = spread_free_positions(&[pill(500.0, 400.0)], &[pill(500.0, 400.0)], work);
    assert!(!pill(out[0].0, out[0].1).overlaps(&pill(500.0, 400.0)));
    // Idempotent: positions that are already free are left exactly where they are.
    let free = [pill(100.0, 100.0), pill(600.0, 600.0)];
    assert_eq!(
        spread_free_positions(&free, &[], work),
        vec![(100.0, 100.0), (600.0, 600.0)]
    );
}

#[test]
fn a_dropped_pill_is_clamped_onto_its_work_area() {
    let work = primary_work();
    // Dropped past the right/bottom edge: pulled back so the whole pill shows.
    assert_eq!(clamp_to_work(work, 200.0, 80.0, 1900.0, 1030.0), (1720.0, 960.0));
    // Dropped past the top/left edge.
    assert_eq!(clamp_to_work(work, 200.0, 80.0, -50.0, -20.0), (0.0, 0.0));
    // Inside: untouched.
    assert_eq!(clamp_to_work(work, 200.0, 80.0, 500.0, 400.0), (500.0, 400.0));
    // A second monitor's origin is respected, not the desktop's.
    let right = Rect { x: 1920.0, y: 0.0, w: 1280.0, h: 1000.0 };
    assert_eq!(clamp_to_work(right, 200.0, 80.0, 1900.0, 10.0), (1920.0, 10.0));
    // A window bigger than the screen is pinned to the corner instead of pushed off.
    assert_eq!(clamp_to_work(work, 5000.0, 5000.0, 300.0, 300.0), (0.0, 0.0));
}

#[test]
fn settings_without_folders_still_load() {
    // An older settings blob has no `float_folders` key at all.
    let s: Settings = serde_json::from_str(r#"{"theme":"dark"}"#).unwrap();
    assert!(s.float_folders.is_empty());
    // A folder from a newer build keeps the fields this version knows and fills in
    // the ones it does not carry.
    let s: Settings = serde_json::from_str(
        r##"{"float_folders":[{"id":"f","name":"Work","members":["a","b"],"future":1}]}"##,
    )
    .unwrap();
    assert_eq!(s.float_folders[0].name, "Work");
    assert_eq!(s.float_folders[0].scale, 1.0);
    assert_eq!(s.float_folders[0].pos.x, 0);
}

// ---- Colour picker ----

#[test]
fn hex_is_uppercase_and_padded() {
    assert_eq!(rgb_to_hex(26, 43, 60), "#1A2B3C");
    assert_eq!(rgb_to_hex(0, 0, 0), "#000000");
    assert_eq!(rgb_to_hex(255, 255, 255), "#FFFFFF");
    assert_eq!(rgb_to_hex(1, 2, 3), "#010203"); // single digits keep their zero
}

#[test]
fn hsl_handles_the_degenerate_cases() {
    // Pure grey: hue and saturation are undefined, so both report 0.
    assert_eq!(rgb_to_hsl(128, 128, 128), (0, 0, 50));
    assert_eq!(rgb_to_hsl(0, 0, 0), (0, 0, 0));
    assert_eq!(rgb_to_hsl(255, 255, 255), (0, 0, 100));
    // Pure red sits at 0°, never at 360°.
    assert_eq!(rgb_to_hsl(255, 0, 0), (0, 100, 50));
    // Just below red wraps back down instead of rounding up to 360.
    assert_eq!(rgb_to_hsl(255, 0, 1).0, 0);
    assert_eq!(rgb_to_hsl(0, 255, 0), (120, 100, 50));
    assert_eq!(rgb_to_hsl(0, 0, 255), (240, 100, 50));
    // Each of the three max branches, with rounded integer output.
    assert_eq!(rgb_to_hsl(26, 43, 60), (210, 40, 17));
    assert_eq!(rgb_to_hsl(255, 128, 0), (30, 100, 50));
    assert_eq!(rgb_to_hsl(128, 0, 255), (270, 100, 50));
}

#[test]
fn every_format_writes_its_own_notation() {
    assert_eq!(format_color("hex", 26, 43, 60), "#1A2B3C");
    assert_eq!(format_color("rgb", 26, 43, 60), "rgb(26,43,60)");
    assert_eq!(format_color("hsl", 26, 43, 60), "hsl(210,40%,17%)");
    // An unknown or empty stored notation falls back to hex, never to nothing.
    assert_eq!(format_color("", 26, 43, 60), "#1A2B3C");
    assert_eq!(format_color("cmyk", 26, 43, 60), "#1A2B3C");
}

#[test]
fn picker_settings_are_clamped_to_their_range() {
    let mut s = Settings::default();
    let d = pick_cfg(&s);
    assert_eq!((d.zoom, d.format.as_str()), (4, "hex"));
    s.ui_values.insert("loupeZoom".into(), 99.0);
    assert_eq!(pick_cfg(&s).zoom, 16);
    s.ui_values.insert("loupeZoom".into(), 0.0);
    assert_eq!(pick_cfg(&s).zoom, 4);
    s.ui_texts.insert("pickerFormat".into(), "hsl".into());
    assert_eq!(pick_cfg(&s).format, "hsl");
    s.ui_texts.insert("pickerFormat".into(), "nonsense".into());
    assert_eq!(pick_cfg(&s).format, "hex");
}

// A stitched frozen desktop for a mixed-DPI pair: a 2560x1440 monitor scaled to
// 150% sits LEFT of the 1920x1080 primary, so the virtual origin is negative.
// Physical pixels are what the capture holds: the scale factor never enters the
// mapping, which is exactly why the picker stays exact on either monitor.
pub(super) fn mixed_dpi_desktop() -> (Vec<MonitorCap>, i64, i64) {
    let (left_x, left_w, left_h) = (-2560i32, 2560u32, 1440u32);
    let (right_x, right_w, right_h) = (0i32, 1920u32, 1080u32);
    let origin_x = left_x.min(right_x);
    let origin_y = 0;
    let total_w = left_w + right_w;
    let total_h = left_h.max(right_h);
    let mut canvas = image::RgbaImage::from_pixel(total_w, total_h, image::Rgba([0, 0, 0, 255]));
    let lx = stitch_offset(left_x, origin_x);
    let rx = stitch_offset(right_x, origin_x);
    // One marker pixel per monitor, at the monitor's own top-left corner.
    canvas.put_pixel(lx as u32, 0, image::Rgba([26, 43, 60, 255]));
    canvas.put_pixel(rx as u32, 0, image::Rgba([255, 128, 0, 255]));
    // ...and one in the far bottom-right of the right-hand monitor.
    canvas.put_pixel(rx as u32 + right_w - 1, right_h - 1, image::Rgba([1, 2, 3, 255]));
    (
        vec![MonitorCap { image: Arc::new(canvas), x: origin_x, y: origin_y, width: total_w, height: total_h }],
        lx,
        rx,
    )
}

#[test]
fn stitch_offset_places_each_monitor_at_its_own_column() {
    // Left monitor starts the image; the primary starts one full width in.
    assert_eq!(stitch_offset(-2560, -2560), 0);
    assert_eq!(stitch_offset(0, -2560), 2560);
    // Vertical works the same, including a monitor above the primary.
    assert_eq!(stitch_offset(-300, -300), 0);
    assert_eq!(stitch_offset(0, -300), 300);
}

#[test]
fn sampling_hits_the_right_pixel_on_either_monitor() {
    let (caps, lx, rx) = mixed_dpi_desktop();
    assert_eq!(sample_frozen(&caps, lx as i32, 0), Some([26, 43, 60]));
    assert_eq!(sample_frozen(&caps, rx as i32, 0), Some([255, 128, 0]));
    assert_eq!(sample_frozen(&caps, rx as i32 + 1919, 1079), Some([1, 2, 3]));
    // The neighbouring pixel is a different colour: no off-by-one smearing.
    assert_eq!(sample_frozen(&caps, rx as i32 - 1, 0), Some([0, 0, 0]));
    // Outside the desktop nothing is sampled at all (never a clamped neighbour).
    assert_eq!(sample_frozen(&caps, -1, 0), None);
    assert_eq!(sample_frozen(&caps, 0, -1), None);
    assert_eq!(sample_frozen(&caps, 4480, 0), None);
    assert_eq!(sample_frozen(&caps, 0, 1440), None);
    assert_eq!(sample_frozen(&[], 0, 0), None);
}

#[test]
fn the_tile_handed_to_the_loupe_is_packed_rgb() {
    let mut img = image::RgbaImage::new(2, 1);
    img.put_pixel(0, 0, image::Rgba([26, 43, 60, 255]));
    img.put_pixel(1, 0, image::Rgba([255, 128, 0, 128]));
    // Alpha is dropped: three bytes per pixel, in row order.
    assert_eq!(crop_rgb(&img, 0, 0, 2, 1), vec![26, 43, 60, 255, 128, 0]);
}

#[test]
fn a_loupe_tile_reads_the_rows_it_was_asked_for() {
    // 3x3 where every pixel encodes its own coordinate, so a wrong stride shows up.
    let mut img = image::RgbaImage::new(3, 3);
    for y in 0..3u32 {
        for x in 0..3u32 {
            img.put_pixel(x, y, image::Rgba([x as u8, y as u8, 9, 255]));
        }
    }
    assert_eq!(
        crop_rgb(&img, 1, 1, 2, 2),
        vec![1, 1, 9, 2, 1, 9, 1, 2, 9, 2, 2, 9],
        "the tile must be the requested rect, not the top-left corner"
    );
}

#[test]
fn a_tile_that_leaves_the_frame_returns_nothing() {
    let img = image::RgbaImage::new(4, 4);
    assert!(crop_rgb(&img, 3, 0, 2, 1).is_empty()); // runs off the right edge
    assert!(crop_rgb(&img, 0, 3, 1, 2).is_empty()); // runs off the bottom edge
    assert!(crop_rgb(&img, 0, 0, 0, 4).is_empty()); // empty rect
    assert!(crop_rgb(&img, u32::MAX, 0, 4, 4).is_empty()); // would wrap
    assert_eq!(crop_rgb(&img, 0, 0, 4, 4).len(), 4 * 4 * 3); // the whole frame still fits
}

#[test]
fn snip_background_still_loads_without_the_picker_fields() {
    // The overlay reads the mode off SnipBg; serializing keeps the snake_case
    // names snip.js indexes.
    let bg = SnipBg {
        src: "x".into(), is_file: true, width: 4, height: 2,
        pick: true, pick_zoom: 12, pick_format: "rgb".into(), pick_cells: 21,
        pick_history: vec!["#FF0000".into()],
        record: false, origin_x: 0, origin_y: 0,
    };
    let j = serde_json::to_value(&bg).unwrap();
    assert_eq!(j["pick"], serde_json::json!(true));
    assert_eq!(j["pick_zoom"], serde_json::json!(12));
    assert_eq!(j["pick_format"], serde_json::json!("rgb"));
    assert_eq!(j["pick_cells"], serde_json::json!(21));
    assert_eq!(j["pick_history"], serde_json::json!(["#FF0000"]));
}

#[test]
fn tray_rec_entries_follow_both_flags() {
    let mut s = Settings::default();
    assert!(tray_rec_entries_on(&s), "on by default");
    s.ui_flags.insert("recTrayEntries".into(), false);
    assert!(!tray_rec_entries_on(&s));
    s.ui_flags.insert("recTrayEntries".into(), true);
    s.ui_flags.insert("screenRecord".into(), false);
    assert!(!tray_rec_entries_on(&s), "recording switched off hides the rows too");
}

#[test]
fn custom_tray_menu_needs_no_native_rebuild_unless_one_is_installed() {
    assert!(!tray_rebuild_due(true, true, false), "custom window is built fresh on open");
    assert!(tray_rebuild_due(true, false, true), "an installed native popup must still be detached");
    assert!(tray_rebuild_due(false, true, false));
    assert!(tray_rebuild_due(false, false, true));
    assert!(!tray_rebuild_due(false, false, false));
}

#[test]
fn a_scaled_folder_centre_follows_its_own_box() {
    let pos = Pos { x: 100, y: 200, ..Default::default() };
    let (x1, _) = folder_centre(&pos, 1.0, 1.0);
    let (x3, y3) = folder_centre(&pos, 3.0, 1.0);
    assert!(x3 > x1, "a 3x folder reaches further right than a 1x one");
    assert_eq!(x3, 100.0 + folder_box(3.0) / 2.0);
    assert_eq!(y3, 200.0 + folder_box(3.0) / 2.0);
    let (x2, _) = folder_centre(&pos, 1.0, 2.0);
    assert_eq!(x2, 100.0 + folder_box(1.0));
}
