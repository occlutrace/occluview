/// The installed GUI binary name used by shell "Open with" registration.
pub const APP_EXE_NAME: &str = "occluview.exe";

/// The CLSID string for the OccluView thumbnail provider.
///
/// Registered under
/// `HKCR\.<ext>\ShellEx\{E357FCCD-A995-4576-B01F-234630154E96}` for each
/// supported extension. (The literal `{E357FCCD-A995-4576-B01F-234630154E96}`
/// is the shell's `IThumbnailProvider` category, not our own CLSID — our own
/// CLSID is generated when the COM class lands.)
pub const THUMBNAIL_PROVIDER_CATEGORY: &str = "{E357FCCD-A995-4576-B01F-234630154E96}";

/// The shell preview handler category used by Explorer's Preview Pane.
pub const PREVIEW_HANDLER_CATEGORY: &str = "{8895B1C6-B41F-4C1C-A562-0D564250836F}";

/// File extensions OccluView registers a thumbnail provider and Open-with
/// `ProgID` for.
///
/// JSON `.gltf` and `.3mf` are deliberately absent until their stream-safe
/// readers exist. HPS are included because private builds can provide the
/// HPS key at runtime while public builds safely fall back to placeholders for
/// encrypted CE sources.
pub const SUPPORTED_EXTENSIONS: &[&str] = occluview_formats::V1_OPEN_EXTENSIONS;

/// Formats that ship a dedicated file-type icon asset in the MSI.
pub const DEDICATED_FILE_ICON_EXTENSIONS: &[&str] = SUPPORTED_EXTENSIONS;
