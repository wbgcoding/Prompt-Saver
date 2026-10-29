use crate::win_windows as ww;

#[test]
fn wide_strings_stop_at_the_first_nul() {
    let w: Vec<u16> = "Notepad\0junk".encode_utf16().collect();
    assert_eq!(ww::wide_to_string(&w), "Notepad");
    let no_nul: Vec<u16> = "Edge".encode_utf16().collect();
    assert_eq!(ww::wide_to_string(&no_nul), "Edge");
    assert_eq!(ww::wide_to_string(&[]), "");
    assert_eq!(ww::wide_to_string(&[0, 65]), "");
}

#[test]
fn wide_strings_keep_non_latin_text_and_survive_broken_units() {
    let w: Vec<u16> = "Übersicht \u{4e2d}\u{6587}".encode_utf16().collect();
    assert_eq!(ww::wide_to_string(&w), "Übersicht \u{4e2d}\u{6587}");
    // A lone surrogate must not drop the whole title.
    assert_eq!(ww::wide_to_string(&[0x41, 0xD800, 0x42]), "A\u{FFFD}B");
}

#[test]
fn the_listing_filter_follows_the_old_rules() {
    const TOOL: u32 = 0x80;
    // A plain window passes, a nameless class does not.
    assert!(ww::window_passes_filter("Notepad", 0, false));
    assert!(ww::window_passes_filter("Notepad", 0, true));
    assert!(!ww::window_passes_filter("", 0, false));
    // Tool windows pass only with a title, the task bar always.
    assert!(!ww::window_passes_filter("Palette", TOOL, true));
    assert!(ww::window_passes_filter("Palette", TOOL, false));
    assert!(ww::window_passes_filter("Shell_TrayWnd", TOOL, true));
    // Shell furniture never.
    assert!(!ww::window_passes_filter("Progman", 0, false));
    assert!(!ww::window_passes_filter("Button", 0, false));
}

#[test]
fn the_overlay_offers_only_titled_windows_that_are_not_minimized() {
    let w = |title: &str, minimized: bool| ww::WinInfo { id: 1, title: title.to_string(), minimized, client: (0, 0, 10, 10) };
    assert!(ww::offered_to_overlay(&w("Editor", false)));
    assert!(!ww::offered_to_overlay(&w("", false)));
    assert!(!ww::offered_to_overlay(&w("Editor", true)));
}

#[test]
fn rect_emptiness_matches_isrectempty() {
    assert!(ww::rect_is_empty(0, 0, 0, 10));
    assert!(ww::rect_is_empty(0, 0, 10, 0));
    assert!(ww::rect_is_empty(5, 5, 4, 9));
    assert!(!ww::rect_is_empty(-5, -5, -4, -4));
}

#[test]
fn the_client_area_is_cut_out_of_the_window_bitmap() {
    // Window 100,100..500,400 with a 8 px border and a 30 px title bar.
    assert_eq!(ww::client_crop((100, 100, 500, 400), (108, 130, 492, 392)), Some((8, 30, 384, 262)));
    // Client equal to window.
    assert_eq!(ww::client_crop((0, 0, 50, 40), (0, 0, 50, 40)), Some((0, 0, 50, 40)));
    // Negative global coordinates (monitor left of the primary).
    assert_eq!(ww::client_crop((-1000, 50, -600, 350), (-992, 80, -608, 342)), Some((8, 30, 384, 262)));
    // A client rect poking out of the bitmap is clamped, never negative or too wide.
    assert_eq!(ww::client_crop((10, 10, 110, 110), (0, 0, 200, 200)), Some((0, 0, 100, 100)));
    // Nothing left: empty client, client outside, empty window.
    assert_eq!(ww::client_crop((10, 10, 110, 110), (20, 20, 20, 50)), None);
    assert_eq!(ww::client_crop((10, 10, 110, 110), (200, 200, 300, 300)), None);
    assert_eq!(ww::client_crop((10, 10, 10, 110), (10, 10, 10, 110)), None);
}

#[test]
fn gdi_bytes_become_opaque_rgba() {
    let mut px = vec![1u8, 2, 3, 0, 10, 20, 30, 7, 255, 0, 0, 255];
    ww::bgra_to_rgba_opaque(&mut px);
    assert_eq!(px, vec![3, 2, 1, 255, 30, 20, 10, 255, 0, 0, 255, 255]);
    // A trailing partial pixel is left alone and nothing panics.
    let mut odd = vec![1u8, 2, 3, 4, 5, 6];
    ww::bgra_to_rgba_opaque(&mut odd);
    assert_eq!(odd, vec![3, 2, 1, 255, 5, 6]);
    let mut empty: Vec<u8> = Vec::new();
    ww::bgra_to_rgba_opaque(&mut empty);
    assert!(empty.is_empty());
}

#[test]
fn bitmap_sizes_are_bounded() {
    assert!(ww::size_is_allowed(1, 1));
    assert!(ww::size_is_allowed(7680, 4320));
    assert!(!ww::size_is_allowed(0, 100));
    assert!(!ww::size_is_allowed(100, 0));
    assert!(!ww::size_is_allowed(-5, 100));
    assert!(!ww::size_is_allowed(20000, 20000));
    assert!(!ww::size_is_allowed(i32::MAX, i32::MAX));
}

#[test]
fn window_ids_round_trip_through_the_handle() {
    for h in [0x0001_0ABCisize, 0x7FFF_FFFF, 1, 0x00A4_0F2E] {
        assert_eq!(ww::hwnd_of(ww::id_of(h)), h);
    }
    // A handle with the upper half of the 32 bits set comes back sign extended, the same value
    // the system accepts, and the id itself stays the plain low 32 bits.
    let id = ww::id_of(0xFFFF_FFFF);
    assert_eq!(id, 0xFFFF_FFFF);
    assert_eq!(ww::hwnd_of(id), -1);
    // A 64 bit handle keeps only its low half in the id.
    assert_eq!(ww::id_of(0x1_0000_0042), 0x42);
}

#[test]
fn file_stems_drop_the_directory_and_the_exe_ending() {
    assert_eq!(ww::file_stem_of_path("C:\\Program Files\\App\\Tool.EXE"), "Tool");
    assert_eq!(ww::file_stem_of_path("C:\\x\\tool.exe"), "tool");
    assert_eq!(ww::file_stem_of_path("C:/x/other.exe"), "other");
    assert_eq!(ww::file_stem_of_path("Plain"), "Plain");
    assert_eq!(ww::file_stem_of_path("C:\\x\\lib.dll"), "lib.dll");
    assert_eq!(ww::file_stem_of_path(""), "");
    // A multi-byte name shorter than the ending never panics.
    assert_eq!(ww::file_stem_of_path("\u{4e2d}"), "\u{4e2d}");
    assert_eq!(ww::file_stem_of_path("C:\\\u{4e2d}\u{6587}.exe"), "\u{4e2d}\u{6587}");
}

#[test]
fn the_app_name_prefers_the_first_filled_version_value() {
    let vals = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert_eq!(ww::pick_app_name(vals(&["", "  ", "Product", "Internal"]), "C:\\a\\b.exe"), Some("Product".to_string()));
    assert_eq!(ww::pick_app_name(vals(&["  Padded  "]), "C:\\a\\b.exe"), Some("Padded".to_string()));
    assert_eq!(ww::pick_app_name(vals(&["", ""]), "C:\\a\\b.exe"), Some("b".to_string()));
    assert_eq!(ww::pick_app_name(Vec::<String>::new(), "C:\\a\\Name.exe"), Some("Name".to_string()));
    assert_eq!(ww::pick_app_name(Vec::<String>::new(), ""), None);
    assert_eq!(ww::pick_app_name(Vec::<String>::new(), "C:\\a\\.exe"), None);
}

#[test]
fn version_queries_cover_every_key_and_translation_in_priority_order() {
    assert_eq!(ww::version_query(0x0409, 0x04b0, "FileDescription"), "\\StringFileInfo\\040904b0\\FileDescription");
    assert_eq!(ww::version_query(0x0007, 0x0000, "X"), "\\StringFileInfo\\00070000\\X");
    // The length the API reports is in bytes, four per language / code page pair.
    assert_eq!(ww::translation_count(0), 0);
    assert_eq!(ww::translation_count(4), 1);
    assert_eq!(ww::translation_count(8), 2);
    assert_eq!(ww::translation_count(7), 1);
    let q = ww::version_queries(&[(0x0409, 0x04b0), (0x0407, 0x04b0)]);
    assert_eq!(q.len(), ww::VERSION_KEYS.len() * 2);
    assert!(q[0].ends_with("\\FileDescription") && q[0].contains("040904b0"));
    assert!(q[1].ends_with("\\FileDescription") && q[1].contains("040704b0"));
    assert!(q[2].ends_with("\\ProductName"));
    assert!(q.last().unwrap().ends_with("\\OriginalFilename"));
    assert!(ww::version_queries(&[]).is_empty());
}

#[test]
fn stitched_monitors_land_at_their_global_offsets() {
    let solid = |w: u32, h: u32, c: [u8; 4]| image::RgbaImage::from_pixel(w, h, image::Rgba(c));
    // Left monitor at a negative origin, primary at 0,0 and a taller one above-right.
    let caps = vec![(solid(4, 3, [1, 0, 0, 255]), -4, 0), (solid(5, 3, [0, 2, 0, 255]), 0, 0), (solid(2, 2, [0, 0, 3, 255]), 5, -2)];
    let (img, ox, oy) = ww::stitch_captures(caps).unwrap();
    assert_eq!((ox, oy), (-4, -2));
    assert_eq!((img.width(), img.height()), (11, 5));
    assert_eq!(img.get_pixel(0, 2).0, [1, 0, 0, 255]);
    assert_eq!(img.get_pixel(4, 2).0, [0, 2, 0, 255]);
    assert_eq!(img.get_pixel(9, 0).0, [0, 0, 3, 255]);
    // Gaps stay transparent.
    assert_eq!(img.get_pixel(0, 0).0, [0, 0, 0, 0]);
    assert!(ww::stitch_captures(Vec::new()).is_none());
}

#[test]
fn a_dead_or_foreign_window_id_yields_nothing() {
    assert!(ww::capture_window(u32::MAX).is_none());
    assert!(ww::capture_window(0).is_none());
    assert!(ww::app_name(u32::MAX).is_none());
    assert!(ww::app_name(0).is_none());
}

#[test]
fn the_window_listing_is_consistent() {
    let ws = ww::list_windows();
    let mut ids: Vec<u32> = ws.iter().map(|w| w.id).collect();
    let n = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), n, "a window is listed once");
    let own = std::process::id();
    for w in &ws {
        assert!(w.client.2 >= 0 && w.client.3 >= 0, "client size is never negative: {:?}", w);
        let pid = {
            #[link(name = "user32")]
            extern "system" {
                fn GetWindowThreadProcessId(hwnd: isize, pid: *mut u32) -> u32;
            }
            let mut p = 0u32;
            unsafe { GetWindowThreadProcessId(ww::hwnd_of(w.id), &mut p) };
            p
        };
        assert_ne!(pid, own, "this process's own windows are never listed");
    }
}

#[test]
fn monitors_have_a_size() {
    for (_, _, w, h) in ww::monitor_rects() {
        assert!(w > 0 && h > 0);
    }
}

#[test]
fn the_app_name_of_a_system_program_comes_from_its_version_resource() {
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
    let cmd = format!("{root}\\System32\\cmd.exe");
    if !std::path::Path::new(&cmd).exists() {
        return;
    }
    let name = ww::app_name_of_image(&cmd).expect("cmd.exe has a name");
    assert!(!name.trim().is_empty());
    // The shipped description is not the bare file stem.
    assert_ne!(name.to_lowercase(), "cmd.exe");
}

#[test]
fn an_image_without_a_resource_falls_back_to_its_file_stem() {
    assert_eq!(ww::app_name_of_image("C:\\definitely\\not\\here\\Some Tool.EXE"), Some("Some Tool".to_string()));
    assert_eq!(ww::app_name_of_image(""), None);
}

#[test]
fn the_dib_helper_returns_the_pixel_buffer_or_nothing() {
    // Sizes that must be refused before any GDI object exists.
    assert!(ww::paint_dib_for_test(0, 10, |_, _| true).is_none());
    assert!(ww::paint_dib_for_test(10, -1, |_, _| true).is_none());
    assert!(ww::paint_dib_for_test(30000, 30000, |_, _| true).is_none());
    // A painter that reports failure gives nothing back.
    assert!(ww::paint_dib_for_test(8, 8, |_, _| false).is_none());
    // With a desktop session the buffer is exactly w * h * 4 bytes and starts zeroed.
    if let Some(bytes) = ww::paint_dib_for_test(7, 5, |_, _| true) {
        assert_eq!(bytes.len(), 7 * 5 * 4);
        assert!(bytes.iter().all(|&b| b == 0));
    }
}

#[test]
fn the_dib_helper_releases_every_gdi_object() {
    // Warm up so one-time allocations are not counted.
    for _ in 0..3 {
        let _ = ww::paint_dib_for_test(64, 64, |_, _| true);
    }
    let before = ww::gdi_object_count();
    for i in 0..300 {
        // Success and failure paths both.
        let _ = ww::paint_dib_for_test(64, 64, |_, _| i % 2 == 0);
    }
    let after = ww::gdi_object_count();
    // Other tests run in parallel and may hold a few handles; a leak here would add about 600.
    assert!(after < before + 40, "GDI objects grew from {before} to {after}");
}

#[test]
fn no_source_names_the_replaced_capture_crate_outside_the_module_header() {
    let word = ["xc", "ap"].concat();
    assert!(!include_str!("../../Cargo.toml").contains(&word));
    for (name, src) in [("capture.rs", include_str!("../capture.rs")), ("snip.rs", include_str!("../snip.rs"))] {
        assert!(!src.contains(&word), "{name} still mentions the replaced crate");
    }
}

#[test]
fn the_frame_rect_of_a_dead_window_is_none() {
    assert!(ww::frame_rect(u32::MAX).is_none());
    assert!(ww::frame_rect(0).is_none());
}

#[test]
fn the_file_version_comes_from_the_fixed_version_info() {
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".to_string());
    let v = ww::file_version_of(&format!("{windir}\\System32\\kernel32.dll")).expect("kernel32 has a version");
    let parts: Vec<u32> = v.split('.').map(|p| p.parse().unwrap()).collect();
    assert_eq!(parts.len(), 3, "major.minor.build, got {v}");
    assert!(parts[0] >= 6, "a Windows system file reports its real major version, got {v}");
    assert_eq!(ww::file_version_of("C:\\no\\such\\file.exe"), None);
}
