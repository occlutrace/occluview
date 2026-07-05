//! `DllRegisterServer` / `DllUnregisterServer` — COM self-registration.
//!
//! `regsvr32 occluview_shell.dll` runs `DllRegisterServer`, which writes the
//! registry entries that make Windows Explorer activate our thumbnail provider
//! for each supported extension, AND registers the "Open with" ProgID so the
//! shell offers OccluView in the context menu. `DllUnregisterServer` (via
//! `regsvr32 /u`) removes them.
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
//!
//! HKCR\OccluView.Mesh                      (the ProgID)
//!     (default) = "OccluView 3D Mesh"
//!     DefaultIcon
//!         (default) = "<app.exe>,0"
//!     shell\open\command
//!         (default) = "<app.exe>" "%1"
//! HKCR\.stl\OpenWithProgids
//!     OccluView.Mesh = ""                  (REG_SZ, presence is what counts)
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
use windows::Win32::Storage::FileSystem::GetFileAttributesW;
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

/// The OccluView ProgID — the "Open with" association identifier.
///
/// Registered under `HKCR\OccluView.Mesh` with a `shell\open\command` that
/// launches the app, and referenced from each `.<ext>\OpenWithProgids` so
/// the shell offers "OccluView" in the context menu for supported files.
pub const OCCLUVIEW_PROGID: &str = "OccluView.Mesh";

/// Friendly name shown for the ProgID in `regedit` / `Open with`.
const PROGID_FRIENDLY_H: &HSTRING = h!("OccluView 3D Mesh");

/// The app binary name (looked up next to this DLL).
const APP_EXE_NAME: &str = "occluview-app.exe";

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
///
/// The "Open with" ProgID registration is only written when the app exe is
/// found next to this DLL (same directory); otherwise it's skipped silently
/// so the thumbnail registration still succeeds. The installer is expected
/// to place both binaries together.
fn register_all() -> windows::core::Result<()> {
    let dll_path = own_dll_path();
    register_clsid(&dll_path)?;
    let our_clsid = HSTRING::from(OCCLUVIEW_THUMBNAIL_CLSID);
    for &ext in SUPPORTED_EXTENSIONS {
        register_extension(ext, &our_clsid)?;
    }
    if let Some(app_path) = app_exe_path(&dll_path) {
        register_progid(&app_path)?;
        let progid = HSTRING::from(OCCLUVIEW_PROGID);
        for &ext in SUPPORTED_EXTENSIONS {
            register_open_with(ext, &progid)?;
        }
    } else {
        tracing::warn!(
            "app exe '{}' not found next to DLL; skipping Open-with ProgID registration",
            APP_EXE_NAME
        );
    }
    Ok(())
}

/// Remove every registry entry. Missing keys are not an error.
fn unregister_all() -> windows::core::Result<()> {
    unregister_clsid()?;
    for &ext in SUPPORTED_EXTENSIONS {
        // Missing entry is fine (user may have deleted it); ignore not-found.
        let _ = unregister_extension(ext);
        let _ = unregister_open_with(ext);
    }
    let _ = unregister_progid();
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

/// Resolve the app exe path: same directory as `dll_path`, with
/// [`APP_EXE_NAME`]. Returns `None` if the file does not exist.
fn app_exe_path(dll_path: &HSTRING) -> Option<HSTRING> {
    let wide = dll_path.as_wide();
    // Find the last path separator (backslash).
    let sep = wide.iter().rposition(|&c| c == u16::from(b'\\'))?;
    let dir = &wide[..=sep];
    let exe_name: Vec<u16> = APP_EXE_NAME.encode_utf16().chain(std::iter::once(0)).collect();
    let mut full = dir.to_vec();
    full.extend_from_slice(&exe_name);
    let full = HSTRING::from_wide(&full[..full.len() - 1]).ok()?;
    // Existence check via GetFileAttributesW (avoid pulling std::fs for cfg).
    // SAFETY: full is a valid NUL-terminated wide path.
    let attrs = unsafe { GetFileAttributesW(PCWSTR(full.as_ptr())) };
    // INVALID_FILE_ATTRIBUTES == 0xFFFFFFFF (-1 as u32).
    if attrs == u32::MAX {
        return None;
    }
    Some(full)
}

/// Register the ProgID `OccluView.Mesh` with `shell\open\command` and
/// `DefaultIcon` pointing at the app exe.
fn register_progid(app_path: &HSTRING) -> windows::core::Result<()> {
    let progid = HSTRING::from(OCCLUVIEW_PROGID);
    // Top-level: friendly name.
    let hk = create_key(&progid)?;
    set_string(hk, None, PROGID_FRIENDLY_H)?;
    let _ = unsafe { RegCloseKey(hk) };

    // DefaultIcon: "<app.exe>,0"
    let icon_key = HSTRING::from(format!("{OCCLUVIEW_PROGID}\\DefaultIcon"));
    let hk = create_key(&icon_key)?;
    let icon_val = HSTRING::from(format!("{},0", utf16_to_string(app_path)));
    set_string(hk, None, &icon_val)?;
    let _ = unsafe { RegCloseKey(hk) };

    // shell\open\command: "<app.exe>" "%1"
    let cmd_key = HSTRING::from(format!("{OCCLUVIEW_PROGID}\\shell\\open\\command"));
    let hk = create_key(&cmd_key)?;
    let cmd_val = HSTRING::from(format!("\"{}\" \"%1\"", utf16_to_string(app_path)));
    set_string(hk, None, &cmd_val)?;
    let _ = unsafe { RegCloseKey(hk) };
    Ok(())
}

/// Remove the ProgID entirely (cascades to DefaultIcon, shell\open\command).
fn unregister_progid() -> windows::core::Result<()> {
    let progid = HSTRING::from(OCCLUVIEW_PROGID);
    delete_tree(&progid)
}

/// Register `HKCR\.{ext}\OpenWithProgids` with the ProgID as a named value.
/// The value data is empty string; the shell keys on the value *name*.
fn register_open_with(ext: &str, progid: &HSTRING) -> windows::core::Result<()> {
    let key_path = HSTRING::from(format!(".{ext}\\OpenWithProgids"));
    let hk = create_key(&key_path)?;
    let empty = HSTRING::new();
    set_string(hk, Some(progid), &empty)?;
    let _ = unsafe { RegCloseKey(hk) };
    Ok(())
}

/// Remove `HKCR\.{ext}\OpenWithProgids`.
fn unregister_open_with(ext: &str) -> windows::core::Result<()> {
    let key_path = HSTRING::from(format!(".{ext}\\OpenWithProgids"));
    delete_tree(&key_path)
}

/// Best-effort HSTRING → String for assembling registry command lines. Lossy
/// on non-UTF-16 paths (uses replacement char), which is acceptable for the
/// diagnostics/assembly path; the actual registration uses the original
/// HSTRING bytes when it matters.
fn utf16_to_string(s: &HSTRING) -> String {
    String::from_utf16_lossy(s.as_wide())
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
