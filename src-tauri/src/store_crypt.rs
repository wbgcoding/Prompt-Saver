use std::ffi::c_void;
#[repr(C)]
struct Blob {
    cb: u32,
    pb: *mut u8,
}
#[link(name = "crypt32")]
extern "system" {
    fn CryptProtectData(d: *const Blob, desc: *const u16, ent: *const Blob, res: *mut c_void, prompt: *mut c_void, flags: u32, out: *mut Blob) -> i32;
    fn CryptUnprotectData(d: *const Blob, desc: *mut *mut u16, ent: *const Blob, res: *mut c_void, prompt: *mut c_void, flags: u32, out: *mut Blob) -> i32;
}
#[link(name = "kernel32")]
extern "system" {
    fn LocalFree(h: *mut c_void) -> *mut c_void;
}
const UI_FORBIDDEN: u32 = 0x1;

fn run(input: &[u8], protect: bool) -> Option<Vec<u8>> {
    unsafe {
        let in_blob = Blob { cb: input.len() as u32, pb: input.as_ptr() as *mut u8 };
        let mut out = Blob { cb: 0, pb: std::ptr::null_mut() };
        let ok = if protect {
            CryptProtectData(&in_blob, std::ptr::null(), std::ptr::null(), std::ptr::null_mut(), std::ptr::null_mut(), UI_FORBIDDEN, &mut out)
        } else {
            CryptUnprotectData(&in_blob, std::ptr::null_mut(), std::ptr::null(), std::ptr::null_mut(), std::ptr::null_mut(), UI_FORBIDDEN, &mut out)
        };
        if ok == 0 || out.pb.is_null() {
            return None;
        }
        let v = std::slice::from_raw_parts(out.pb, out.cb as usize).to_vec();
        // The unprotected buffer holds plain settings or sidecar data: wipe it before it is freed.
        std::ptr::write_bytes(out.pb, 0, out.cb as usize);
        LocalFree(out.pb as *mut c_void);
        Some(v)
    }
}
pub fn protect(d: &[u8]) -> Option<Vec<u8>> { run(d, true) }
pub fn unprotect(d: &[u8]) -> Option<Vec<u8>> { run(d, false) }
