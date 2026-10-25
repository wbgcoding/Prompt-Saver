// A small, safe layer over the Windows registry API for the few values this app reads and
// writes: the autostart entry, the old protocol key, and two system policy flags. Errors are
// plain `io::Error`s built from the Win32 code, so `ErrorKind::NotFound` means a missing key
// or value.

use std::io;
use windows::core::PCWSTR;
use windows::Win32::Foundation::WIN32_ERROR;
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW,
    RegSetValueExW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_SET_VALUE,
    REG_DWORD, REG_EXPAND_SZ, REG_OPTION_NON_VOLATILE, REG_SAM_FLAGS, REG_SZ, REG_VALUE_TYPE,
};

#[derive(Clone, Copy)]
pub(crate) enum Hive {
    CurrentUser,
    LocalMachine,
}

impl Hive {
    fn root(self) -> HKEY {
        match self {
            Hive::CurrentUser => HKEY_CURRENT_USER,
            Hive::LocalMachine => HKEY_LOCAL_MACHINE,
        }
    }
}

fn check(code: WIN32_ERROR) -> io::Result<()> {
    if code.0 == 0 {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(code.0 as i32))
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

// An open key; closed when dropped.
pub(crate) struct Key(HKEY);

impl Drop for Key {
    fn drop(&mut self) {
        unsafe {
            let _ = RegCloseKey(self.0);
        }
    }
}

impl Key {
    // Opens an existing subkey for reading.
    pub(crate) fn open_read(hive: Hive, path: &str) -> io::Result<Key> {
        Self::open(hive, path, KEY_READ)
    }

    // Opens an existing subkey for setting and deleting values.
    pub(crate) fn open_write(hive: Hive, path: &str) -> io::Result<Key> {
        Self::open(hive, path, KEY_SET_VALUE)
    }

    fn open(hive: Hive, path: &str, access: REG_SAM_FLAGS) -> io::Result<Key> {
        let path = wide(path);
        let mut out = HKEY::default();
        unsafe {
            check(RegOpenKeyExW(hive.root(), PCWSTR(path.as_ptr()), None, access, &mut out))?;
        }
        Ok(Key(out))
    }

    // Opens the subkey for writing values, creating it (and missing parents) first if needed.
    pub(crate) fn create_write(hive: Hive, path: &str) -> io::Result<Key> {
        let path = wide(path);
        let mut out = HKEY::default();
        unsafe {
            check(RegCreateKeyExW(
                hive.root(),
                PCWSTR(path.as_ptr()),
                None,
                PCWSTR::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_SET_VALUE,
                None,
                &mut out,
                None,
            ))?;
        }
        Ok(Key(out))
    }

    // Reads a value's type and raw bytes. The size is asked for first and the read repeated
    // if the value grew in between.
    fn query(&self, name: &str) -> io::Result<(REG_VALUE_TYPE, Vec<u8>)> {
        let name = wide(name);
        let mut size = 0u32;
        unsafe {
            check(RegQueryValueExW(self.0, PCWSTR(name.as_ptr()), None, None, None, Some(&mut size)))?;
        }
        loop {
            let mut buf = vec![0u8; size as usize + 2];
            let mut got = buf.len() as u32;
            let mut kind = REG_VALUE_TYPE::default();
            let code = unsafe {
                RegQueryValueExW(self.0, PCWSTR(name.as_ptr()), None, Some(&mut kind), Some(buf.as_mut_ptr()), Some(&mut got))
            };
            // ERROR_MORE_DATA: the value grew between the two calls.
            if code.0 == 234 {
                size = got.max(size + 2);
                continue;
            }
            check(code)?;
            buf.truncate(got as usize);
            return Ok((kind, buf));
        }
    }

    pub(crate) fn get_string(&self, name: &str) -> io::Result<String> {
        let (kind, bytes) = self.query(name)?;
        if kind != REG_SZ && kind != REG_EXPAND_SZ {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "registry value is not a string"));
        }
        let units: Vec<u16> = bytes.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect();
        let end = units.iter().position(|&u| u == 0).unwrap_or(units.len());
        Ok(String::from_utf16_lossy(&units[..end]))
    }

    pub(crate) fn get_u32(&self, name: &str) -> io::Result<u32> {
        let (kind, bytes) = self.query(name)?;
        match (kind == REG_DWORD, <[u8; 4]>::try_from(bytes.as_slice())) {
            (true, Ok(b)) => Ok(u32::from_le_bytes(b)),
            _ => Err(io::Error::new(io::ErrorKind::InvalidData, "registry value is not a 32-bit number")),
        }
    }

    pub(crate) fn set_string(&self, name: &str, value: &str) -> io::Result<()> {
        let name = wide(name);
        let data: Vec<u8> = wide(value).into_iter().flat_map(u16::to_le_bytes).collect();
        unsafe { check(RegSetValueExW(self.0, PCWSTR(name.as_ptr()), None, REG_SZ, Some(&data))) }
    }

    pub(crate) fn set_u32(&self, name: &str, value: u32) -> io::Result<()> {
        let name = wide(name);
        unsafe { check(RegSetValueExW(self.0, PCWSTR(name.as_ptr()), None, REG_DWORD, Some(&value.to_le_bytes()))) }
    }

    pub(crate) fn delete_value(&self, name: &str) -> io::Result<()> {
        let name = wide(name);
        unsafe { check(RegDeleteValueW(self.0, PCWSTR(name.as_ptr()))) }
    }
}

// Deletes a subkey with everything below it.
pub(crate) fn delete_tree(hive: Hive, path: &str) -> io::Result<()> {
    let path = wide(path);
    unsafe { check(RegDeleteTreeW(hive.root(), PCWSTR(path.as_ptr()))) }
}
