// Own Win32 window access for the screenshot tool: which windows the overlay can pick, one
// window's own pixels, the app name behind a window, and the monitor rectangles. It replaces
// the xcap crate and keeps the rules that crate applied, which the overlay's hit test relies
// on. Raw `extern "system"` declarations in the style of `capture_screen_rect`, no windows
// crate features, nothing for other platforms.
//
// What the replaced crate did, and what each part below keeps:
// - Listing: EnumWindows order, so top-most first (the hit test takes the first window under
//   the pointer). A window is listed when it passes IsWindow and IsWindowVisible, has a class
//   name, is not a tool window without a title (the task bar class Shell_TrayWnd is the one
//   tool window kept), does not belong to this process, is not the Program Manager (class
//   Progman) or a Start button (class Button), is not cloaked (DWMWA_CLOAKED) and has a
//   non-empty DWM extended frame rect. A listed window then needs GetWindowInfo to work. Its
//   id is the window handle cut to 32 bits, the title is GetWindowText, minimized is IsIconic
//   and the rect is the client area (rcClient) in global physical pixels. The caller filters
//   minimized and untitled windows.
// - Window pixels: PrintWindow with PW_RENDERFULLCONTENT, then PrintWindow without flags,
//   then a plain copy from the window DC, all into a bitmap of the whole window rect, then
//   the client area is cut out. The process is per-monitor DPI aware, so every rect is
//   physical and the old per-monitor scale factor is always 1.
// - App name: the first non-empty of the version resource strings FileDescription,
//   ProductName, ProductShortName, InternalName, OriginalFilename (for every translation in
//   the resource), else the file name of the process image without its extension.
//
// Deliberate differences: the process is opened with PROCESS_QUERY_LIMITED_INFORMATION, so
// elevated windows get a name too (the old full-access open failed for them and gave an empty
// name). A resource with several translations is read by byte length / 4 (the old code
// read four times too far). A title with invalid UTF-16 is kept lossily instead of dropping
// the window.

use std::ffi::c_void;

/// One top-level window the overlay may offer.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct WinInfo {
    pub(crate) id: u32,
    pub(crate) title: String,
    pub(crate) minimized: bool,
    /// Client area: x, y, width, height in global physical pixels.
    pub(crate) client: (i32, i32, i32, i32),
}

#[repr(C)]
#[derive(Default, Clone, Copy, Debug, PartialEq)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[repr(C)]
#[derive(Default)]
struct WindowInfoRaw {
    cb_size: u32,
    rc_window: Rect,
    rc_client: Rect,
    style: u32,
    ex_style: u32,
    window_status: u32,
    border_x: u32,
    border_y: u32,
    atom: u16,
    creator_version: u16,
}

#[repr(C)]
struct BmHeader {
    size: u32,
    width: i32,
    height: i32,
    planes: u16,
    bit_count: u16,
    compression: u32,
    size_image: u32,
    x_ppm: i32,
    y_ppm: i32,
    clr_used: u32,
    clr_important: u32,
}

#[link(name = "user32")]
extern "system" {
    fn EnumWindows(proc_: extern "system" fn(isize, isize) -> i32, lparam: isize) -> i32;
    fn IsWindow(hwnd: isize) -> i32;
    fn IsWindowVisible(hwnd: isize) -> i32;
    fn IsIconic(hwnd: isize) -> i32;
    fn GetClassNameW(hwnd: isize, out: *mut u16, max: i32) -> i32;
    fn GetWindowLongW(hwnd: isize, index: i32) -> i32;
    fn GetWindowTextLengthW(hwnd: isize) -> i32;
    fn GetWindowTextW(hwnd: isize, out: *mut u16, max: i32) -> i32;
    fn GetWindowThreadProcessId(hwnd: isize, pid: *mut u32) -> u32;
    fn GetWindowInfo(hwnd: isize, info: *mut WindowInfoRaw) -> i32;
    fn GetWindowDC(hwnd: isize) -> isize;
    fn ReleaseDC(hwnd: isize, hdc: isize) -> i32;
    fn PrintWindow(hwnd: isize, hdc: isize, flags: u32) -> i32;
    fn EnumDisplayMonitors(hdc: isize, clip: *const Rect, proc_: extern "system" fn(isize, isize, *const Rect, isize) -> i32, lparam: isize) -> i32;
}
#[link(name = "gdi32")]
extern "system" {
    fn CreateCompatibleDC(hdc: isize) -> isize;
    fn CreateDIBSection(hdc: isize, bmi: *const BmHeader, usage: u32, bits: *mut *mut c_void, section: isize, offset: u32) -> isize;
    fn SelectObject(hdc: isize, obj: isize) -> isize;
    fn DeleteObject(obj: isize) -> i32;
    fn DeleteDC(hdc: isize) -> i32;
    fn BitBlt(dst: isize, x: i32, y: i32, w: i32, h: i32, src: isize, sx: i32, sy: i32, rop: u32) -> i32;
    fn GdiFlush() -> i32;
}
#[link(name = "dwmapi")]
extern "system" {
    fn DwmGetWindowAttribute(hwnd: isize, attr: u32, out: *mut c_void, size: u32) -> i32;
}
#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentProcessId() -> u32;
    fn OpenProcess(access: u32, inherit: i32, pid: u32) -> isize;
    fn CloseHandle(h: isize) -> i32;
    fn QueryFullProcessImageNameW(process: isize, flags: u32, out: *mut u16, size: *mut u32) -> i32;
}
#[link(name = "version")]
extern "system" {
    fn GetFileVersionInfoSizeW(path: *const u16, handle: *mut u32) -> u32;
    fn GetFileVersionInfoW(path: *const u16, handle: u32, len: u32, data: *mut c_void) -> i32;
    fn VerQueryValueW(block: *const c_void, sub_block: *const u16, out: *mut *mut c_void, len: *mut u32) -> i32;
}

const WS_EX_TOOLWINDOW: u32 = 0x0000_0080;
const GWL_EXSTYLE: i32 = -20;
const DWMWA_CLOAKED: u32 = 14;
const DWMWA_EXTENDED_FRAME_BOUNDS: u32 = 9;
const PW_RENDERFULLCONTENT: u32 = 2;
const SRCCOPY: u32 = 0x00CC_0020;
const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
// A window larger than this is not a real window; it also bounds the bitmap allocation.
const MAX_WINDOW_PIXELS: u64 = 120_000_000;

// ---- pure parts ----

/// The window handle behind an id (the id is the handle cut to 32 bits; handles are 32 bit
/// significant, so sign extension restores them).
pub(crate) fn hwnd_of(id: u32) -> isize {
    id as i32 as isize
}

/// The id handed to the overlay for a window handle.
pub(crate) fn id_of(hwnd: isize) -> u32 {
    hwnd as u32
}

/// UTF-16 up to the first NUL as a string (invalid units become U+FFFD).
pub(crate) fn wide_to_string(wide: &[u16]) -> String {
    let end = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
    String::from_utf16_lossy(&wide[..end])
}

/// The listing rule for a window that is visible and not ours: false for a nameless class,
/// a tool window without a title (the task bar excepted), the Program Manager and a Start
/// button. Cloaking and an empty frame are separate system checks.
pub(crate) fn window_passes_filter(class: &str, ex_style: u32, title_empty: bool) -> bool {
    if class.is_empty() {
        return false;
    }
    if ex_style & WS_EX_TOOLWINDOW != 0 && class != "Shell_TrayWnd" && title_empty {
        return false;
    }
    !matches!(class, "Progman" | "Button")
}

/// The overlay offers a window only when it is not minimized and has a title.
pub(crate) fn offered_to_overlay(w: &WinInfo) -> bool {
    !w.minimized && !w.title.is_empty()
}

/// A rect (left, top, right, bottom) with no area (IsRectEmpty).
pub(crate) fn rect_is_empty(left: i32, top: i32, right: i32, bottom: i32) -> bool {
    right <= left || bottom <= top
}

/// The client area of a window inside a bitmap of the whole window: x, y, w, h, clamped to
/// the bitmap. None when nothing is left.
pub(crate) fn client_crop(window: (i32, i32, i32, i32), client: (i32, i32, i32, i32)) -> Option<(u32, u32, u32, u32)> {
    // Both are (left, top, right, bottom) in global pixels.
    let (wl, wt, wr, wb) = window;
    let (cl, ct, cr, cb) = client;
    let (bw, bh) = (wr - wl, wb - wt);
    if bw <= 0 || bh <= 0 {
        return None;
    }
    let x0 = (cl - wl).clamp(0, bw);
    let y0 = (ct - wt).clamp(0, bh);
    let x1 = (cr - wl).clamp(0, bw);
    let y1 = (cb - wt).clamp(0, bh);
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    Some((x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32))
}

/// GDI hands back blue, green, red, unused; the screenshot code wants red, green, blue, opaque.
pub(crate) fn bgra_to_rgba_opaque(px: &mut [u8]) {
    for p in px.chunks_exact_mut(4) {
        p.swap(0, 2);
        p[3] = 255;
    }
}

/// A bitmap size that is sane to allocate.
pub(crate) fn size_is_allowed(w: i32, h: i32) -> bool {
    w > 0 && h > 0 && (w as u64) * (h as u64) <= MAX_WINDOW_PIXELS
}

/// The file name of a path without directory and without a trailing ".exe" (any case).
pub(crate) fn file_stem_of_path(path: &str) -> String {
    let base = path.rsplit(['\\', '/']).next().unwrap_or(path);
    let cut = base.len().checked_sub(4).filter(|&i| base.is_char_boundary(i));
    match cut {
        Some(i) if base[i..].eq_ignore_ascii_case(".exe") => base[..i].to_string(),
        _ => base.to_string(),
    }
}

/// The name shown for a window's program: the first non-empty version resource value (in the
/// order the caller yields them), else the file stem of the image path. None when both are
/// empty.
pub(crate) fn pick_app_name<I: IntoIterator<Item = String>>(values: I, image_path: &str) -> Option<String> {
    for v in values {
        let t = v.trim();
        if !t.is_empty() {
            return Some(t.to_string());
        }
    }
    let stem = file_stem_of_path(image_path);
    let stem = stem.trim();
    (!stem.is_empty()).then(|| stem.to_string())
}

/// The version resource keys that can name a program, in priority order.
pub(crate) const VERSION_KEYS: [&str; 5] = ["FileDescription", "ProductName", "ProductShortName", "InternalName", "OriginalFilename"];

/// The query path of one string in a version resource for a language / code page pair.
pub(crate) fn version_query(lang: u16, code_page: u16, key: &str) -> String {
    format!("\\StringFileInfo\\{lang:04x}{code_page:04x}\\{key}")
}

/// Several translations come as language / code page pairs of two 16-bit values; the length
/// the API reports is in bytes.
pub(crate) fn translation_count(byte_len: u32) -> usize {
    (byte_len / 4) as usize
}

/// A pairing of every key (priority order) with every translation, in the order the values
/// are tried.
pub(crate) fn version_queries(translations: &[(u16, u16)]) -> Vec<String> {
    let mut out = Vec::with_capacity(VERSION_KEYS.len() * translations.len());
    for key in VERSION_KEYS {
        for &(lang, cp) in translations {
            out.push(version_query(lang, cp, key));
        }
    }
    out
}

/// The smallest rectangle width of the stitched desktop and where each monitor lands: input
/// are the monitor captures with their global origin, output the stitched image and its
/// global origin. None for no captures.
pub(crate) fn stitch_captures(caps: Vec<(image::RgbaImage, i32, i32)>) -> Option<(image::RgbaImage, i32, i32)> {
    if caps.is_empty() {
        return None;
    }
    let min_x = caps.iter().map(|c| c.1).min()?;
    let min_y = caps.iter().map(|c| c.2).min()?;
    let max_x = caps.iter().map(|c| c.1 as i64 + c.0.width() as i64).max()?;
    let max_y = caps.iter().map(|c| c.2 as i64 + c.0.height() as i64).max()?;
    let total_w = (max_x - min_x as i64).max(1) as u32;
    let total_h = (max_y - min_y as i64).max(1) as u32;
    let mut canvas = image::RgbaImage::new(total_w, total_h);
    for (img, x, y) in &caps {
        image::imageops::replace(&mut canvas, img, crate::snip::stitch_offset(*x, min_x), crate::snip::stitch_offset(*y, min_y));
    }
    Some((canvas, min_x, min_y))
}

// ---- system calls ----

fn window_text_len(hwnd: isize) -> i32 {
    unsafe { GetWindowTextLengthW(hwnd) }
}

fn window_title(hwnd: isize) -> String {
    let n = window_text_len(hwnd);
    if n <= 0 {
        return String::new();
    }
    let mut buf = vec![0u16; n as usize + 1];
    let got = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    if got <= 0 {
        return String::new();
    }
    wide_to_string(&buf[..got as usize])
}

fn window_class(hwnd: isize) -> String {
    let mut buf = [0u16; 260];
    let n = unsafe { GetClassNameW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    if n < 1 {
        return String::new();
    }
    wide_to_string(&buf[..n as usize])
}

fn window_pid(hwnd: isize) -> u32 {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    pid
}

fn is_cloaked(hwnd: isize) -> bool {
    let mut cloaked = 0u32;
    let hr = unsafe { DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, &mut cloaked as *mut u32 as *mut c_void, 4) };
    hr == 0 && cloaked != 0
}

fn frame_is_empty(hwnd: isize) -> bool {
    let mut r = Rect::default();
    let hr = unsafe { DwmGetWindowAttribute(hwnd, DWMWA_EXTENDED_FRAME_BOUNDS, &mut r as *mut Rect as *mut c_void, std::mem::size_of::<Rect>() as u32) };
    hr != 0 || rect_is_empty(r.left, r.top, r.right, r.bottom)
}

/// The visible frame of a window (DWM extended frame bounds: no invisible resize border, title
/// bar and borders included) as x, y, width, height in global physical pixels.
pub(crate) fn frame_rect(id: u32) -> Option<(i32, i32, i32, i32)> {
    let mut r = Rect::default();
    let hr = unsafe { DwmGetWindowAttribute(hwnd_of(id), DWMWA_EXTENDED_FRAME_BOUNDS, &mut r as *mut Rect as *mut c_void, std::mem::size_of::<Rect>() as u32) };
    if hr != 0 || rect_is_empty(r.left, r.top, r.right, r.bottom) {
        return None;
    }
    Some((r.left, r.top, r.right - r.left, r.bottom - r.top))
}

fn is_listable(hwnd: isize, own_pid: u32) -> bool {
    unsafe {
        if IsWindow(hwnd) == 0 || IsWindowVisible(hwnd) == 0 {
            return false;
        }
    }
    // Reading the title of a window of this process from another thread can wait on that
    // window's own message loop, so our own windows are dropped before anything is read.
    if window_pid(hwnd) == own_pid {
        return false;
    }
    let class = window_class(hwnd);
    let ex_style = unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) } as u32;
    let title_empty = ex_style & WS_EX_TOOLWINDOW != 0 && window_text_len(hwnd) <= 0;
    if !window_passes_filter(&class, ex_style, title_empty) {
        return false;
    }
    !is_cloaked(hwnd) && !frame_is_empty(hwnd)
}

extern "system" fn collect_window(hwnd: isize, lparam: isize) -> i32 {
    // SAFETY: lparam is the address of the Vec that list_windows keeps alive for the call.
    let state = unsafe { &mut *(lparam as *mut (Vec<isize>, u32)) };
    if is_listable(hwnd, state.1) {
        state.0.push(hwnd);
    }
    1
}

fn window_info(hwnd: isize) -> Option<WindowInfoRaw> {
    let mut info = WindowInfoRaw { cb_size: std::mem::size_of::<WindowInfoRaw>() as u32, ..Default::default() };
    (unsafe { GetWindowInfo(hwnd, &mut info) } != 0).then_some(info)
}

/// Every top-level window the overlay could pick, top-most first.
pub(crate) fn list_windows() -> Vec<WinInfo> {
    let mut state: (Vec<isize>, u32) = (Vec::new(), unsafe { GetCurrentProcessId() });
    unsafe { EnumWindows(collect_window, &mut state as *mut (Vec<isize>, u32) as isize) };
    let mut out = Vec::with_capacity(state.0.len());
    for hwnd in state.0 {
        let Some(info) = window_info(hwnd) else { continue };
        let c = info.rc_client;
        out.push(WinInfo {
            id: id_of(hwnd),
            title: window_title(hwnd),
            minimized: unsafe { IsIconic(hwnd) } != 0,
            client: (c.left, c.top, c.right - c.left, c.bottom - c.top),
        });
    }
    out
}

// Releases a window or screen DC on every path.
struct WindowDc {
    hwnd: isize,
    dc: isize,
}
impl Drop for WindowDc {
    fn drop(&mut self) {
        unsafe { ReleaseDC(self.hwnd, self.dc) };
    }
}
struct MemDc(isize);
impl Drop for MemDc {
    fn drop(&mut self) {
        unsafe { DeleteDC(self.0) };
    }
}
struct Bitmap(isize);
impl Drop for Bitmap {
    fn drop(&mut self) {
        unsafe { DeleteObject(self.0) };
    }
}
// Puts the old bitmap back so the DIB can be deleted (a selected bitmap is not deleted).
struct Selected {
    dc: isize,
    old: isize,
}
impl Drop for Selected {
    fn drop(&mut self) {
        unsafe { SelectObject(self.dc, self.old) };
    }
}

/// Runs `paint(memory_dc, base_dc)` against a fresh top-down 32 bit DIB of w x h pixels and
/// returns its bytes (blue, green, red, unused) when paint reports success. Every GDI object
/// is released on every path (declaration order = reverse of the release order).
fn paint_dib(base_hwnd: isize, w: i32, h: i32, paint: impl FnOnce(isize, isize) -> bool) -> Option<Vec<u8>> {
    if !size_is_allowed(w, h) {
        return None;
    }
    let base = unsafe { GetWindowDC(base_hwnd) };
    if base == 0 {
        return None;
    }
    let _base = WindowDc { hwnd: base_hwnd, dc: base };
    let mem = unsafe { CreateCompatibleDC(base) };
    if mem == 0 {
        return None;
    }
    let _mem = MemDc(mem);
    let header = BmHeader {
        size: std::mem::size_of::<BmHeader>() as u32,
        width: w,
        height: -h, // negative: top-down rows
        planes: 1,
        bit_count: 32,
        compression: 0,
        size_image: 0,
        x_ppm: 0,
        y_ppm: 0,
        clr_used: 0,
        clr_important: 0,
    };
    let mut bits: *mut c_void = std::ptr::null_mut();
    let bmp = unsafe { CreateDIBSection(base, &header, 0, &mut bits, 0, 0) };
    if bmp == 0 || bits.is_null() {
        if bmp != 0 {
            unsafe { DeleteObject(bmp) };
        }
        return None;
    }
    let _bmp = Bitmap(bmp);
    let old = unsafe { SelectObject(mem, bmp) };
    if old == 0 {
        return None;
    }
    let _sel = Selected { dc: mem, old };
    if !paint(mem, base) {
        return None;
    }
    unsafe { GdiFlush() };
    let len = (w as usize) * (h as usize) * 4;
    // SAFETY: the section holds exactly w * h * 4 bytes and stays alive until the guards drop.
    Some(unsafe { std::slice::from_raw_parts(bits as *const u8, len) }.to_vec())
}

/// The window's own pixels (client area), as the window draws itself even when other windows
/// cover it. None when the window is gone, empty, too big or refuses to draw.
pub(crate) fn capture_window(id: u32) -> Option<image::RgbaImage> {
    let hwnd = hwnd_of(id);
    let info = window_info(hwnd)?;
    let (rw, rc) = (info.rc_window, info.rc_client);
    let (w, h) = (rw.right - rw.left, rw.bottom - rw.top);
    let (cx, cy, cw, ch) = client_crop((rw.left, rw.top, rw.right, rw.bottom), (rc.left, rc.top, rc.right, rc.bottom))?;
    let mut bytes = paint_dib(hwnd, w, h, |mem, window_dc| unsafe {
        PrintWindow(hwnd, mem, PW_RENDERFULLCONTENT) != 0
            || PrintWindow(hwnd, mem, 0) != 0
            || BitBlt(mem, 0, 0, w, h, window_dc, 0, 0, SRCCOPY) != 0
    })?;
    bgra_to_rgba_opaque(&mut bytes);
    let full = image::RgbaImage::from_raw(w as u32, h as u32, bytes)?;
    Some(image::imageops::crop_imm(&full, cx, cy, cw, ch).to_image())
}

// Owns a process handle until dropped.
struct ProcessHandle(isize);
impl Drop for ProcessHandle {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

/// The full path of the program image of a process.
fn image_path_of_pid(pid: u32) -> Option<String> {
    if pid == 0 {
        return None;
    }
    let h = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if h == 0 {
        return None;
    }
    let _h = ProcessHandle(h);
    // 32767 characters is the longest path Windows allows.
    let mut buf = vec![0u16; 32768];
    let mut len = buf.len() as u32;
    if unsafe { QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut len) } == 0 || len == 0 {
        return None;
    }
    Some(wide_to_string(&buf[..(len as usize).min(buf.len())]))
}

fn to_wide_nul(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

// `count` UTF-16 units starting at `ptr`, which need not be aligned.
unsafe fn read_wide(ptr: *const u8, count: usize) -> Vec<u16> {
    (0..count).map(|i| std::ptr::read_unaligned((ptr as *const u16).add(i))).collect()
}

/// The version resource of a file, read once; None when the file has none.
struct VersionBlock {
    // u32 elements keep the block aligned for the structures inside it.
    data: Vec<u32>,
}

impl VersionBlock {
    fn load(path: &str) -> Option<VersionBlock> {
        let wide = to_wide_nul(path);
        let size = unsafe { GetFileVersionInfoSizeW(wide.as_ptr(), std::ptr::null_mut()) };
        if size == 0 {
            return None;
        }
        let mut data = vec![0u32; (size as usize).div_ceil(4)];
        if unsafe { GetFileVersionInfoW(wide.as_ptr(), 0, size, data.as_mut_ptr() as *mut c_void) } == 0 {
            return None;
        }
        Some(VersionBlock { data })
    }

    fn query(&self, sub_block: &str) -> Option<(*const u8, u32)> {
        let sub = to_wide_nul(sub_block);
        let mut p: *mut c_void = std::ptr::null_mut();
        let mut len = 0u32;
        let ok = unsafe { VerQueryValueW(self.data.as_ptr() as *const c_void, sub.as_ptr(), &mut p, &mut len) };
        (ok != 0 && !p.is_null()).then_some((p as *const u8, len))
    }

    fn translations(&self) -> Vec<(u16, u16)> {
        let Some((p, byte_len)) = self.query("\\VarFileInfo\\Translation") else { return Vec::new() };
        (0..translation_count(byte_len))
            .map(|i| unsafe {
                let pair = p.add(i * 4);
                (std::ptr::read_unaligned(pair as *const u16), std::ptr::read_unaligned(pair.add(2) as *const u16))
            })
            .collect()
    }

    fn string(&self, query: &str) -> Option<String> {
        let (p, chars) = self.query(query)?;
        // SAFETY: VerQueryValueW reports `chars` UTF-16 units that live inside the block.
        Some(wide_to_string(&unsafe { read_wide(p, chars as usize) }))
    }
}

/// The name for a program image: its version resource strings, else its file stem.
pub(crate) fn app_name_of_image(path: &str) -> Option<String> {
    let block = VersionBlock::load(path);
    let values = block.iter().flat_map(|b| {
        let queries = version_queries(&b.translations());
        queries.into_iter().filter_map(move |q| b.string(&q))
    });
    pick_app_name(values, path)
}

/// The program name behind a window id (version resource first, else the file stem).
pub(crate) fn app_name(id: u32) -> Option<String> {
    let path = image_path_of_pid(window_pid(hwnd_of(id)))?;
    app_name_of_image(&path)
}

extern "system" fn collect_monitor(_hmon: isize, _hdc: isize, rect: *const Rect, lparam: isize) -> i32 {
    if !rect.is_null() {
        // SAFETY: lparam is the address of the Vec that monitor_rects keeps alive; rect is
        // valid for the callback.
        let out = unsafe { &mut *(lparam as *mut Vec<(i32, i32, i32, i32)>) };
        let r = unsafe { *rect };
        out.push((r.left, r.top, r.right - r.left, r.bottom - r.top));
    }
    1
}

/// Every monitor as x, y, width, height in global physical pixels.
pub(crate) fn monitor_rects() -> Vec<(i32, i32, i32, i32)> {
    let mut out: Vec<(i32, i32, i32, i32)> = Vec::new();
    unsafe { EnumDisplayMonitors(0, std::ptr::null(), collect_monitor, &mut out as *mut Vec<(i32, i32, i32, i32)> as isize) };
    out.retain(|r| r.2 > 0 && r.3 > 0);
    out
}

#[cfg(test)]
#[link(name = "user32")]
extern "system" {
    fn GetGuiResources(process: isize, flags: u32) -> u32;
}
#[cfg(test)]
#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentProcess() -> isize;
}

/// The number of GDI objects this process holds (for leak tests).
#[cfg(test)]
pub(crate) fn gdi_object_count() -> u32 {
    const GR_GDIOBJECTS: u32 = 0;
    unsafe { GetGuiResources(GetCurrentProcess(), GR_GDIOBJECTS) }
}

/// Test door to the DIB path with a caller-supplied painter.
#[cfg(test)]
pub(crate) fn paint_dib_for_test(w: i32, h: i32, paint: impl FnOnce(isize, isize) -> bool) -> Option<Vec<u8>> {
    paint_dib(0, w, h, paint)
}
