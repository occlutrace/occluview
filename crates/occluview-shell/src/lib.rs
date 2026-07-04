//! `occluview-shell` — Windows COM shell extension for OccluView.
//!
//! Implements `IThumbnailProvider` (CLSID
//! `{E357FCCD-A995-4576-B01F-234630154E96}`) as an **out-of-process** COM
//! server: Windows hosts this DLL in `dllhost.exe`, so a bug or a malicious
//! file cannot crash `explorer.exe` (ADR-0005, `docs/SHELL_INTEGRATION.md`).
//!
//! The thumbnail reuses [`occluview_render`] offscreen path, so it is
//! pixel-identical to the in-app frame — one shader, one camera, one loader.
//!
//! ## Status
//!
//! Stub. The COM class factory, `DllRegisterServer`/`DllUnregisterServer`, the
//! `IThumbnailProvider` impl, and the registry script that maps each extension
//! to our CLSID land in a dedicated PR per the roadmap. The only thing this
//! crate does today is expose a safe Rust entry point the implementation will
//! call from behind the COM boundary — so the logic is testable without Windows.

#![cfg_attr(not(test), forbid(unsafe_code))]

pub mod error;
pub mod render_thumb;

pub use error::ShellError;
pub use render_thumb::render_thumbnail;

/// The CLSID string for the OccluView thumbnail provider.
///
/// Registered under
/// `HKCR\.<ext>\ShellEx\{E357FCCD-A995-4576-B01F-234630154E96}` for each
/// supported extension. (The literal `{E357FCCD-A995-4576-B01F-234630154E96}`
/// is the shell's `IThumbnailProvider` category, not our own CLSID — our own
/// CLSID is generated when the COM class lands.)
pub const THUMBNAIL_PROVIDER_CATEGORY: &str = "{E357FCCD-A995-4576-B01F-234630154E96}";

/// File extensions OccluView registers a thumbnail provider for.
pub const SUPPORTED_EXTENSIONS: &[&str] = &["stl", "ply", "obj", "gltf", "glb", "3mf"];
