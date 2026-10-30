use super::*;

// ---- Free pill resize: the edge follows the cursor in physical pixels ----

fn r(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect { x, y, w, h }
}

const MIN: (f64, f64) = (48.0, 28.0);

// A 200x100 pill at (500, 400) on a 1920x1040 work area at the origin.
fn pill() -> Rect {
    r(500.0, 400.0, 200.0, 100.0)
}
fn work() -> Rect {
    r(0.0, 0.0, 1920.0, 1040.0)
}
fn drag(dir: (i32, i32), dx: f64, dy: f64) -> Rect {
    resize_rect(pill(), (600.0, 450.0), (600.0 + dx, 450.0 + dy), dir, MIN, work())
}

#[test]
fn each_edge_moves_only_its_own_side() {
    assert_eq!(drag((1, 0), 30.0, 70.0), r(500.0, 400.0, 230.0, 100.0));
    assert_eq!(drag((-1, 0), -30.0, 70.0), r(470.0, 400.0, 230.0, 100.0));
    assert_eq!(drag((0, 1), 70.0, 25.0), r(500.0, 400.0, 200.0, 125.0));
    assert_eq!(drag((0, -1), 70.0, -25.0), r(500.0, 375.0, 200.0, 125.0));
}

#[test]
fn each_corner_moves_both_of_its_sides_independently() {
    assert_eq!(drag((1, 1), 30.0, 25.0), r(500.0, 400.0, 230.0, 125.0));
    assert_eq!(drag((-1, -1), -30.0, -25.0), r(470.0, 375.0, 230.0, 125.0));
    assert_eq!(drag((1, -1), 30.0, -25.0), r(500.0, 375.0, 230.0, 125.0));
    assert_eq!(drag((-1, 1), -30.0, 25.0), r(470.0, 400.0, 230.0, 125.0));
}

#[test]
fn width_and_height_do_not_follow_a_common_ratio() {
    let out = drag((1, 1), 400.0, 10.0);
    assert_eq!((out.w, out.h), (600.0, 110.0));
}

#[test]
fn the_opposite_edge_never_moves() {
    for dir in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (-1, -1), (1, -1), (-1, 1)] {
        for (dx, dy) in [(-900.0, -900.0), (-40.0, 33.0), (0.0, 0.0), (45.0, -7.0), (3000.0, 3000.0)] {
            let out = drag(dir, dx, dy);
            if dir.0 > 0 {
                assert_eq!(out.x, 500.0);
            }
            if dir.0 < 0 {
                assert_eq!(out.right(), 700.0);
            }
            if dir.1 > 0 {
                assert_eq!(out.y, 400.0);
            }
            if dir.1 < 0 {
                assert_eq!(out.bottom(), 500.0);
            }
            if dir.0 == 0 {
                assert_eq!((out.x, out.w), (500.0, 200.0));
            }
            if dir.1 == 0 {
                assert_eq!((out.y, out.h), (400.0, 100.0));
            }
        }
    }
}

#[test]
fn no_move_keeps_the_box() {
    assert_eq!(drag((1, 1), 0.0, 0.0), pill());
    assert_eq!(drag((-1, -1), 0.0, 0.0), pill());
}

#[test]
fn the_minimum_box_holds_from_every_side() {
    assert_eq!(drag((1, 1), -900.0, -900.0), r(500.0, 400.0, 48.0, 28.0));
    // Left/top edges stop where the opposite edge minus the minimum is.
    assert_eq!(drag((-1, -1), 900.0, 900.0), r(652.0, 472.0, 48.0, 28.0));
}

#[test]
fn growing_stops_at_the_work_area() {
    let out = drag((1, 1), 5000.0, 5000.0);
    assert_eq!((out.right(), out.bottom()), (1920.0, 1040.0));
    let out = drag((-1, -1), -5000.0, -5000.0);
    assert_eq!((out.x, out.y), (0.0, 0.0));
    assert_eq!((out.right(), out.bottom()), (700.0, 500.0));
}

#[test]
fn negative_virtual_desktop_coordinates_work_like_any_other() {
    // A monitor left of and above the primary one.
    let work = r(-1920.0, -200.0, 1920.0, 1040.0);
    let start = r(-1500.0, -100.0, 300.0, 120.0);
    let out = resize_rect(start, (-1400.0, -50.0), (-1380.0, -20.0), (1, 1), MIN, work);
    assert_eq!(out, r(-1500.0, -100.0, 320.0, 150.0));
    let out = resize_rect(start, (-1400.0, -50.0), (-1430.0, -75.0), (-1, -1), MIN, work);
    assert_eq!(out, r(-1530.0, -125.0, 330.0, 145.0));
    let out = resize_rect(start, (-1400.0, -50.0), (-9000.0, -9000.0), (-1, -1), MIN, work);
    assert_eq!((out.x, out.y), (-1920.0, -200.0));
    let out = resize_rect(start, (-1400.0, -50.0), (9000.0, 9000.0), (1, 1), MIN, work);
    assert_eq!((out.right(), out.bottom()), (0.0, 840.0));
}

#[test]
fn an_edge_already_outside_the_work_area_is_not_pulled_back() {
    // Pill hanging 100 px over the right edge: shrinking works, growing is frozen, no snap.
    let start = r(1820.0, 400.0, 200.0, 100.0);
    let grow = resize_rect(start, (2000.0, 450.0), (2100.0, 450.0), (1, 0), MIN, work());
    assert_eq!(grow.right(), 2020.0);
    let shrink = resize_rect(start, (2000.0, 450.0), (1950.0, 450.0), (1, 0), MIN, work());
    assert_eq!(shrink.right(), 1970.0);
}

#[test]
fn the_result_depends_on_the_physical_delta_only() {
    // The same cursor delta anywhere on the desktop gives the same change of box.
    let a = resize_rect(pill(), (600.0, 450.0), (650.0, 480.0), (1, 1), MIN, work());
    let b = resize_rect(pill(), (-300.0, 9.0), (-250.0, 39.0), (1, 1), MIN, work());
    assert_eq!(a, b);
}

#[test]
fn the_free_size_is_optional_in_old_settings_files() {
    let s: Settings = serde_json::from_str("{}").expect("empty settings parse");
    assert!(s.float_size.is_empty());
    assert_eq!(float_size_of(&s, "p"), None);
    let s: Settings = serde_json::from_str(r#"{"float_size":{"p":[321.5,64.0]}}"#).expect("settings parse");
    assert_eq!(float_size_of(&s, "p"), Some((321.5, 64.0)));
    let round = serde_json::to_string(&s).expect("serialize");
    let again: Settings = serde_json::from_str(&round).expect("reparse");
    assert_eq!(again.float_size.get("p"), Some(&[321.5, 64.0]));
}

#[test]
fn a_stored_free_size_is_clamped_to_the_usable_range() {
    let mut s = Settings::default();
    s.float_size.insert("a".into(), [1.0, 1.0]);
    s.float_size.insert("b".into(), [99999.0, 99999.0]);
    s.float_size.insert("c".into(), [f64::NAN, 50.0]);
    assert_eq!(float_size_of(&s, "a"), Some((FLOAT_MIN_W, FLOAT_MIN_H)));
    assert_eq!(float_size_of(&s, "b"), Some((FLOAT_MAX_SIDE, FLOAT_MAX_SIDE)));
    assert_eq!(float_size_of(&s, "c"), None);
}
