//! Creates a .lnk shortcut through IShellLinkW, the only supported way.
use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr::null_mut;
use windows_sys::core::{GUID, HRESULT, PCWSTR};
use windows_sys::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};

const CLSID_SHELL_LINK: GUID = GUID::from_u128(0x00021401_0000_0000_c000_000000000046);
const IID_SHELL_LINK_W: GUID = GUID::from_u128(0x000214f9_0000_0000_c000_000000000046);
const IID_PERSIST_FILE: GUID = GUID::from_u128(0x0000010b_0000_0000_c000_000000000046);

fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(std::iter::once(0)).collect()
}

/// A COM pointer released exactly once.
struct Com(*mut c_void);

impl Com {
    /// Method `slot` of this object's vtable.
    unsafe fn slot(&self, slot: usize) -> *const c_void {
        let vtable = *(self.0 as *const *const *const c_void);
        *vtable.add(slot)
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        // SAFETY: slot 2 is IUnknown::Release on every COM interface.
        unsafe {
            let release: extern "system" fn(*mut c_void) -> u32 = std::mem::transmute(self.slot(2));
            release(self.0);
        }
    }
}

fn check(hr: HRESULT) -> Result<(), i32> {
    if hr < 0 { Err(hr) } else { Ok(()) }
}

pub fn create(link: &Path, target: &Path) -> Result<(), i32> {
    // SAFETY: COM is initialised and uninitialised on this thread around the work.
    unsafe {
        check(CoInitializeEx(null_mut(), COINIT_APARTMENTTHREADED as u32))?;
        let result = create_inner(link, target);
        CoUninitialize();
        result
    }
}

unsafe fn create_inner(link: &Path, target: &Path) -> Result<(), i32> {
    let mut raw = null_mut();
    check(CoCreateInstance(&CLSID_SHELL_LINK, null_mut(), CLSCTX_INPROC_SERVER, &IID_SHELL_LINK_W, &mut raw))?;
    let shell_link = Com(raw);
    let target_w = wide(target);
    let dir_w = wide(target.parent().ok_or(-1)?);
    let description = openicue_core::win::wide("Change your RAM lighting");

    type SetText = extern "system" fn(*mut c_void, PCWSTR) -> HRESULT;
    let set_path: SetText = std::mem::transmute(shell_link.slot(20));
    check(set_path(shell_link.0, target_w.as_ptr()))?;
    let set_dir: SetText = std::mem::transmute(shell_link.slot(9));
    check(set_dir(shell_link.0, dir_w.as_ptr()))?;
    let set_description: SetText = std::mem::transmute(shell_link.slot(7));
    check(set_description(shell_link.0, description.as_ptr()))?;

    type Query = extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> HRESULT;
    let query: Query = std::mem::transmute(shell_link.slot(0));
    let mut raw = null_mut();
    check(query(shell_link.0, &IID_PERSIST_FILE, &mut raw))?;
    let persist = Com(raw);
    type Save = extern "system" fn(*mut c_void, PCWSTR, i32) -> HRESULT;
    let save: Save = std::mem::transmute(persist.slot(6));
    let link_w = wide(link);
    check(save(persist.0, link_w.as_ptr(), 1))
}
