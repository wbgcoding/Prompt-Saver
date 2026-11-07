// ---- Editor clipboard copy: formats and the CF_DIBV5 layout ----

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}

#[test]
fn transient_copy_carries_both_history_flags_and_not_the_monitor_flag() {
    let live = crate::images::editor_clipboard_formats(true);
    assert_eq!(live, ["PNG", "CF_DIBV5", "CanIncludeInClipboardHistory", "CanUploadToCloudClipboard"]);
    assert!(!live.contains(&"ExcludeClipboardContentFromMonitorProcessing"));
}

#[test]
fn final_copy_carries_no_history_flag() {
    let fin = crate::images::editor_clipboard_formats(false);
    assert_eq!(fin, ["PNG", "CF_DIBV5"]);
    assert!(!fin.iter().any(|f| f.starts_with("CanIncludeInClipboardHistory") || f.starts_with("CanUpload")));
}

#[test]
fn dibv5_header_fields_sit_at_their_offsets() {
    // 2x2: red, green / blue, white (top row first), all opaque except white at half alpha.
    let rgba = [
        255, 0, 0, 255, 0, 255, 0, 255, //
        0, 0, 255, 255, 255, 255, 255, 128,
    ];
    let d = crate::images::dibv5_bytes(2, 2, &rgba);
    assert_eq!(d.len(), 124 + 16);
    assert_eq!(u32_at(&d, 0), 124); // size
    assert_eq!(u32_at(&d, 4), 2); // width
    assert_eq!(u32_at(&d, 8), 2); // height, positive = bottom-up
    assert_eq!(u16::from_le_bytes([d[12], d[13]]), 1); // planes
    assert_eq!(u16::from_le_bytes([d[14], d[15]]), 32); // bit count
    assert_eq!(u32_at(&d, 16), 3); // BI_BITFIELDS
    assert_eq!(u32_at(&d, 20), 16); // image size
    assert_eq!(u32_at(&d, 40), 0x00FF_0000); // red mask
    assert_eq!(u32_at(&d, 44), 0x0000_FF00); // green mask
    assert_eq!(u32_at(&d, 48), 0x0000_00FF); // blue mask
    assert_eq!(u32_at(&d, 52), 0xFF00_0000); // alpha mask
    assert_eq!(u32_at(&d, 56), 0x7352_4742); // LCS_sRGB
    assert_eq!(u32_at(&d, 108), 4); // LCS_GM_IMAGES
}

#[test]
fn dibv5_rows_run_bottom_up_as_bgra() {
    let rgba = [
        255, 0, 0, 255, 0, 255, 0, 255, //
        0, 0, 255, 255, 255, 255, 255, 128,
    ];
    let d = crate::images::dibv5_bytes(2, 2, &rgba);
    // First data pixel is the bottom-left source pixel (blue), written B G R A.
    assert_eq!(&d[124..128], &[255, 0, 0, 255]);
    // Then the bottom-right (white, half alpha), then the top row.
    assert_eq!(&d[128..132], &[255, 255, 255, 128]);
    assert_eq!(&d[132..136], &[0, 0, 255, 255]);
    assert_eq!(&d[136..140], &[0, 255, 0, 255]);
}
