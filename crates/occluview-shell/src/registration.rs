//! `DllRegisterServer` / `DllUnregisterServer` — COM self-registration.
//!
//! `regsvr32 occluview_shell.dll` runs `DllRegisterServer`, which writes the
//! registry entries that make Windows Explorer activate our thumbnail provider
//! for each supported extension. `DllUnregisterServer` (via `regsvr32 /u`)
//! removes them.
//!
//! The registration layout (ADR-0005):
//!
//! ```text
//! HKCR\CLSID\{OCCLUVIEW_THUMBNAIL_CLSID}
//!     (default) = "OccluView Thumbnail Provider"
//!     InprocServer32
//!         (default) = <path to this DLL>
//!         ThreadingModel = "Both"
//! HKCR\.stl\ShellEx\{E357FCCD-A995-4576-B01F-234630154E96}
//!     (default) = "{OCCLUVIEW_THUMBNAIL_CLSID}"
//! ... (one entry per supported extension)
//! ```
//!
//! The shell's `IThumbnailProvider` *category* CLSID
//! `{E357FCCD-A995-4576-B01F-234630154E96}` is well-known and constant; the
//! entry's default value is *our* implementation CLSID.

// Registry FFI is `unsafe` by definition; this module shares com.rs's gate
// (the `cfg(windows)` lives on `pub mod registration;` in lib.rs). The
// pedantic lints below are inherent to FFI/registry glue (raw pointer
// derefs, casts across the ABI) and are relaxed here only.
#![allow(
    unsafe_code,
    clippy::missing_safety_doc,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::not_unsafe_ptr_arg_deref,
    clippy::ptr_as_ptr,
    clippy::borrow_as_ptr,
    clippy::unnecessary_cast,
    clippy::doc_markdown,
    missing_docs,
)]

use crate::com::OCCLUVIEW_THUMBNAIL_CLSID;
use crate::{SUPPORTED_EXTENSIONS, THUMBNAIL_PROVIDER_CATEGORY};
use windows::core::{h, HRESULT, HSTRING, PCWSTR};
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyW, RegDeleteTreeW, RegSetValueExW, HKEY, HKEY_CLASSES_ROOT, REG_SZ,
};

const S_OK: HRESULT = HRESULT(0);
const E_FAIL: HRESULT = HRESULT(-2_147_418_235);
/// `ERROR_SUCCESS`.
const ERROR_SUCCESS: u32 = 0;
/// `ERROR_FILE_NOT_FOUND` — already-deleted key, treat as success on unregister.
const ERROR_FILE_NOT_FOUND: u32 = 2;

/// Static HSTRINGs via the `h!` macro (the macro yields `&'static HSTRING`).
const FRIENDLY_NAME_H: &HSTRING = h!("OccluView Thumbnail Provider");
const THREADING_MODEL_H: &HSTRING = h!("Both");

/// `regsvr32 occluview_shell.dll` calls this. Creates all registry entries.
#[no_mangle]
pub extern "system" fn DllRegisterServer() -> HRESULT {
    match register_all() {
        Ok(()) => S_OK,
        Err(e) => {
            tracing::error!(error = ?e, "DllRegisterServer failed");
            E_FAIL
        }
    }
}

/// `regsvr32 /u occluview_shell.dll` calls this. Removes all entries.
#[no_mangle]
pub extern "system" fn DllUnregisterServer() -> HRESULT {
    match unregister_all() {
        Ok(()) => S_OK,
        Err(e) => {
            tracing::error!(error = ?e, "DllUnregisterServer failed");
            E_FAIL
        }
    }
}

/// Write every registry entry. Idempotent — re-running over existing entries
/// is a no-op (RegCreateKeyW opens existing keys).
fn register_all() -> windows::core::Result<()> {
    let dll_path = own_dll_path();
    register_clsid(&dll_path)?;
    let our_clsid = HSTRING::from(OCCLUVIEW_THUMBNAIL_CLSID);
    for &ext in SUPPORTED_EXTENSIONS {
        register_extension(ext, &our_clsid)?;
    }
    Ok(())
}

/// Remove every registry entry. Missing keys are not an error.
fn unregister_all() -> windows::core::Result<()> {
    unregister_clsid()?;
    for &ext in SUPPORTED_EXTENSIONS {
        // Missing extension entry is fine (user may have deleted it); ignore
        // the not-found result.
        let _ = unregister_extension(ext);
    }
    Ok(())
}

/// Path to this DLL on disk, via `GetModuleFileNameW` with our own handle.
fn own_dll_path() -> HSTRING {
    let mut buf = [0u16; 1024];
    // SAFETY: GetModuleFileNameW writes up to `buf.len()` wide chars into our
    // stack array; `None` means "this DLL's path".
    let n = unsafe { GetModuleFileNameW(None, buf.as_mut_slice()) };
    if n == 0 {
        return HSTRING::new();
    }
    HSTRING::from_wide(&buf[..n as usize]).unwrap_or_default()
}

/// Register `HKCR\CLSID\{clsid}\InprocServer32` with the DLL path + Both.
fn register_clsid(dll_path: &HSTRING) -> windows::core::Result<()> {
    let clsid_key = HSTRING::from(format!("CLSID\\{OCCLUVIEW_THUMBNAIL_CLSID}"));
    let inproc_key = HSTRING::from(format!("CLSID\\{OCCLUVIEW_THUMBNAIL_CLSID}\\InprocServer32"));

    // Top-level CLSID entry: friendly name.
    let hk = create_key(&clsid_key)?;
    set_string(hk, None, FRIENDLY_NAME_H)?;
    let _ = unsafe { RegCloseKey(hk) };

    // InprocServer32: DLL path + ThreadingModel.
    let hk_inproc = create_key(&inproc_key)?;
    set_string(hk_inproc, None, dll_path)?;
    set_string(hk_inproc, Some(h!("ThreadingModel")), THREADING_MODEL_H)?;
    let _ = unsafe { RegCloseKey(hk_inproc) };
    Ok(())
}

/// Register `HKCR\.{ext}\ShellEx\{category}` -> our CLSID.
fn register_extension(ext: &str, our_clsid: &HSTRING) -> windows::core::Result<()> {
    let key_path = HSTRING::from(format!(".{ext}\\ShellEx\\{THUMBNAIL_PROVIDER_CATEGORY}"));
    let hk = create_key(&key_path)?;
    set_string(hk, None, our_clsid)?;
    let _ = unsafe { RegCloseKey(hk) };
    Ok(())
}

/// Remove `HKCR\CLSID\{clsid}` entirely (cascades to InprocServer32).
fn unregister_clsid() -> windows::core::Result<()> {
    let clsid_key = HSTRING::from(format!("CLSID\\{OCCLUVIEW_THUMBNAIL_CLSID}"));
    delete_tree(&clsid_key)
}

/// Remove `HKCR\.{ext}\ShellEx\{category}`.
fn unregister_extension(ext: &str) -> windows::core::Result<()> {
    let key_path = HSTRING::from(format!(".{ext}\\ShellEx\\{THUMBNAIL_PROVIDER_CATEGORY}"));
    delete_tree(&key_path)
}

/// Create or open a registry key under HKCR, returning the handle. Errors
/// propagate as `windows::core::Error`.
fn create_key(subkey: &HSTRING) -> windows::core::Result<HKEY> {
    let mut hkey = HKEY::default();
    // SAFETY: `hkey` is a stack out-param; `subkey` is a valid PCWSTR.
    let r = unsafe { RegCreateKeyW(HKEY_CLASSES_ROOT, PCWSTR(subkey.as_ptr()), &mut hkey) };
    if r.0 == ERROR_SUCCESS {
        Ok(hkey)
    } else {
        Err(windows::core::Error::from_win32())
    }
}

/// Set a `REG_SZ` value. `name=None` sets the key's default value.
fn set_string(hkey: HKEY, name: Option<&HSTRING>, value: &HSTRING) -> windows::core::Result<()> {
    let name_pcwstr = match name {
        Some(n) => PCWSTR(n.as_ptr()),
        None => PCWSTR::null(),
    };
    // Include the trailing NUL wide char in the byte length.
    let bytes = value.as_wide();
    let byte_len = (bytes.len() + 1) * 2;
    // SAFETY: `value`'s wide buffer is valid for byte_len bytes; the trailing
    // NUL is implicit in HSTRING's allocation.
    let r = unsafe {
        RegSetValueExW(
            hkey,
            name_pcwstr,
            0,
            REG_SZ,
            Some(std::slice::from_raw_parts(bytes.as_ptr() as *const u8, byte_len)),
        )
    };
    if r.0 == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(windows::core::Error::from_win32())
    }
}

/// Recursively delete a key. `ERROR_FILE_NOT_FOUND` is treated as success.
fn delete_tree(subkey: &HSTRING) -> windows::core::Result<()> {
    // SAFETY: `subkey` is a valid PCWSTR; RegDeleteTreeW recursively removes
    // the key and all subkeys.
    let r = unsafe { RegDeleteTreeW(HKEY_CLASSES_ROOT, PCWSTR(subkey.as_ptr())) };
    if r.0 == ERROR_SUCCESS || r.0 == ERROR_FILE_NOT_FOUND {
        Ok(())
    } else {
        Err(windows::core::Error::from_win32())
    }
}
