use windows::{
    core::{PCWSTR, PWSTR},
    Win32::{Foundation::ERROR_SUCCESS, System::Registry::*},
};

pub fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub struct Key(HKEY);

impl Drop for Key {
    fn drop(&mut self) {
        unsafe {
            let _ = RegCloseKey(self.0);
        }
    }
}

impl Key {
    pub fn open(path: &str, write: bool) -> Option<Key> {
        let p = w(path);
        let mut h = HKEY::default();
        let sam = if write { KEY_READ | KEY_WRITE } else { KEY_READ };
        let r = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR(p.as_ptr()), Some(0), sam, &mut h) };
        if r != ERROR_SUCCESS {
            return None;
        }
        Some(Key(h))
    }

    pub fn create(path: &str) -> Option<Key> {
        let p = w(path);
        let mut h = HKEY::default();
        let r = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(p.as_ptr()),
                Some(0),
                PCWSTR::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_READ | KEY_WRITE,
                None,
                &mut h,
                None,
            )
        };
        if r != ERROR_SUCCESS {
            return None;
        }
        Some(Key(h))
    }

    pub fn dword(&self, name: &str) -> Option<u32> {
        let n = w(name);
        let mut v: u32 = 0;
        let mut sz = 4u32;
        let mut ty = REG_VALUE_TYPE(0);
        let r = unsafe {
            RegQueryValueExW(
                self.0,
                PCWSTR(n.as_ptr()),
                None,
                Some(&mut ty),
                Some(&mut v as *mut u32 as *mut u8),
                Some(&mut sz),
            )
        };
        if r == ERROR_SUCCESS && ty == REG_DWORD {
            Some(v)
        } else {
            None
        }
    }

    pub fn set_dword(&self, name: &str, v: u32) {
        let n = w(name);
        let r = unsafe {
            RegSetValueExW(self.0, PCWSTR(n.as_ptr()), Some(0), REG_DWORD, Some(&v.to_le_bytes()))
        };
        if r != ERROR_SUCCESS {
            lg!("registry write failed: {name} (error {})", r.0);
        }
    }

    pub fn bytes(&self, name: &str) -> Option<Vec<u8>> {
        let n = w(name);
        let mut ty = REG_VALUE_TYPE(0);
        let mut sz = 0u32;
        let r = unsafe {
            RegQueryValueExW(self.0, PCWSTR(n.as_ptr()), None, Some(&mut ty), None, Some(&mut sz))
        };
        if r != ERROR_SUCCESS || ty != REG_BINARY {
            return None;
        }
        let mut buf = vec![0u8; sz as usize];
        let mut sz2 = sz;
        let r = unsafe {
            RegQueryValueExW(
                self.0,
                PCWSTR(n.as_ptr()),
                None,
                None,
                Some(buf.as_mut_ptr()),
                Some(&mut sz2),
            )
        };
        if r != ERROR_SUCCESS {
            return None;
        }
        buf.truncate(sz2 as usize);
        Some(buf)
    }

    /// Returns (registry type, text). Name "" = default value.
    pub fn raw_str(&self, name: &str) -> Option<(u32, String)> {
        let n = w(name);
        let mut ty = REG_VALUE_TYPE(0);
        let mut sz = 0u32;
        let r = unsafe {
            RegQueryValueExW(self.0, PCWSTR(n.as_ptr()), None, Some(&mut ty), None, Some(&mut sz))
        };
        if r != ERROR_SUCCESS || (ty != REG_SZ && ty != REG_EXPAND_SZ) {
            return None;
        }
        let mut buf = vec![0u8; sz as usize + 2];
        let mut sz2 = buf.len() as u32;
        let r = unsafe {
            RegQueryValueExW(
                self.0,
                PCWSTR(n.as_ptr()),
                None,
                None,
                Some(buf.as_mut_ptr()),
                Some(&mut sz2),
            )
        };
        if r != ERROR_SUCCESS {
            return None;
        }
        let u: Vec<u16> = buf[..sz2 as usize]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        let end = u.iter().position(|&c| c == 0).unwrap_or(u.len());
        Some((ty.0, String::from_utf16_lossy(&u[..end])))
    }

    pub fn set_str(&self, name: &str, ty: u32, s: &str) {
        let n = w(name);
        let d: Vec<u8> = w(s).iter().flat_map(|c| c.to_le_bytes()).collect();
        let r = unsafe {
            RegSetValueExW(self.0, PCWSTR(n.as_ptr()), Some(0), REG_VALUE_TYPE(ty), Some(&d[..]))
        };
        if r != ERROR_SUCCESS {
            lg!("registry write failed: {name} (error {})", r.0);
        }
    }

    pub fn del_value(&self, name: &str) {
        let n = w(name);
        let _ = unsafe { RegDeleteValueW(self.0, PCWSTR(n.as_ptr())) };
    }

    pub fn subkeys(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut i = 0u32;
        loop {
            let mut buf = [0u16; 256];
            let mut len = 256u32;
            let r = unsafe {
                RegEnumKeyExW(
                    self.0,
                    i,
                    Some(PWSTR(buf.as_mut_ptr())),
                    &mut len,
                    None,
                    None,
                    None,
                    None,
                )
            };
            if r != ERROR_SUCCESS {
                break;
            }
            out.push(String::from_utf16_lossy(&buf[..len as usize]));
            i += 1;
        }
        out
    }
}