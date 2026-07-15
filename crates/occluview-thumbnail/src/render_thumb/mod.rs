//! The safe, Windows-agnostic core of thumbnail generation.
//!
//! The COM class calls into
//! [`render_thumbnail`] - this function does all the work and is unit-testable
//! without Windows. It loads the file via `occluview-formats`, frames the
//! camera with the dental occlusal default, and renders an offscreen frame via
//! `occluview-render`.
//!
//! Thumbnails intentionally use the same canonical occlusal framing as the app
//! viewport. Explorer preview should be a small version of what opens in the
//! viewer, not a separately auto-rotated interpretation of the mesh.

#![cfg_attr(
    test,
    allow(
        clippy::cast_possible_wrap,
        clippy::cast_precision_loss,
        clippy::expect_used
    )
)]

use crate::placeholder::{placeholder_thumbnail, placeholder_thumbnail_kind, PlaceholderKind};
use crate::ThumbnailError;
use occluview_formats::FormatError;
use occluview_render::ThumbnailSpec;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

mod cache;
mod concurrency;
mod loading;
mod rendering;

#[cfg(test)]
mod tests;

use cache::{
    oversize_input_error, thumbnail_file_cache, thumbnail_file_content_cache,
    thumbnail_file_content_key, thumbnail_setup_timeout, thumbnail_stream_cache,
    FileThumbnailPreflightError, StreamThumbnailPreflightError, ThumbnailFileCacheKey,
    ThumbnailFileContentKey, ThumbnailFileMetadata, ThumbnailRequestKey,
};
use concurrency::{
    render_coalesced_thumbnail, run_thumbnail_job_with_permit, run_thumbnail_job_with_timeouts,
    ThumbnailJobOutcome, ThumbnailJobPermit, ThumbnailJobProgress, ThumbnailRendererPool,
};
use loading::{
    load_thumbnail_mesh_from_bytes, load_thumbnail_mesh_from_bytes_kind,
    load_thumbnail_mesh_from_file, prepare_file_thumbnail_render, prepare_stream_thumbnail_render,
};

/// Default maximum wall-clock wait for a shell thumbnail request.
pub const DEFAULT_THUMBNAIL_TIMEOUT: Duration = Duration::from_millis(6_000);
/// Maximum stream size the shell thumbnail path will parse.
pub const MAX_THUMBNAIL_INPUT_BYTES: usize = 192 * 1024 * 1024;
/// Maximum local-file thumbnail input size. File-backed thumbnails use mmap,
/// so this policy can be higher than the stream cap without duplicating the
/// file into the COM surrogate's heap.
pub const MAX_THUMBNAIL_FILE_BYTES: usize = 512 * 1024 * 1024;
// Ceiling on how long a request may wait for a free render slot (the "setup"
// phase) before it gives up and returns a placeholder. This is the longest a
// single `GetThumbnail` can tie up one of Explorer's thumbnail worker threads
// while queued, so it must stay well under Explorer's own extraction window:
// hold the thread too long and Explorer abandons the call, shows the format
// icon, AND negative-caches it. A timed-out render keeps going in the
// background and caches its result (see the job workers), so returning a
// placeholder here is never the final word — the real thumbnail lands on the
// next repaint.
const MAX_THUMBNAIL_SETUP_TIMEOUT: Duration = Duration::from_secs(8);

static THUMBNAIL_INFLIGHT: OnceLock<
    Mutex<std::collections::HashMap<ThumbnailRequestKey, Arc<concurrency::InflightThumbnail>>>,
> = OnceLock::new();
static THUMBNAIL_FILE_CACHE: OnceLock<Mutex<cache::ThumbnailFileCache>> = OnceLock::new();
static THUMBNAIL_FILE_CONTENT_CACHE: OnceLock<Mutex<cache::ThumbnailFileContentCache>> =
    OnceLock::new();
static THUMBNAIL_STREAM_CACHE: OnceLock<Mutex<cache::ThumbnailStreamCache>> = OnceLock::new();
static THUMBNAIL_RENDERER_POOL: OnceLock<ThumbnailRendererPool> = OnceLock::new();
static THUMBNAIL_JOB_GATE: OnceLock<concurrency::ThumbnailJobGate> = OnceLock::new();

/// Capacity reserved before a shell stream is copied into memory.
///
/// Explorer's isolated thumbnail path initializes providers with `IStream`.
/// Reserving first keeps a mixed folder from materializing every large file in
/// `dllhost` before the ordinary decode/render gate can apply.
#[cfg_attr(not(windows), allow(dead_code))]
pub struct ThumbnailJobReservation(ThumbnailJobPermit);

#[must_use]
#[cfg_attr(not(windows), allow(dead_code))]
/// Reserve one bounded stream thumbnail job before copying shell bytes.
pub fn reserve_thumbnail_stream_job(timeout: Duration) -> Option<ThumbnailJobReservation> {
    concurrency::ThumbnailJobGate::shared()
        .acquire_timeout(thumbnail_setup_timeout(timeout))
        .map(ThumbnailJobReservation)
}

/// Load `bytes` (a file with the given lowercase extension, no dot) and render
/// a thumbnail per `spec`. Returns RGBA8 pixels in row-major order, length
/// `spec.size_px * spec.size_px * 4`, top-to-bottom.
///
/// Blocking: runs the offscreen render to completion on the calling thread.
/// The COM stub invokes this on a worker thread under a
/// Job Object with a watchdog.
///
/// # Errors
/// See [`ThumbnailError`]. The shell layer translates any error into a branded
/// placeholder returned to Windows.
pub fn render_thumbnail(
    extension: &str,
    bytes: &[u8],
    spec: ThumbnailSpec,
) -> Result<Vec<u8>, ThumbnailError> {
    render_thumbnail_bytes(Some(extension), bytes, spec)
}

/// Load `bytes` with an optional file extension hint and render a thumbnail.
///
/// This is the entry point for shell streams where Windows may not provide a
/// file path. It never falls back to a fake default extension.
///
/// # Errors
/// Returns [`ThumbnailError::Format`] if inference or parsing fails, and
/// [`ThumbnailError::Render`] if offscreen rendering fails.
pub fn render_thumbnail_bytes(
    extension: Option<&str>,
    bytes: &[u8],
    spec: ThumbnailSpec,
) -> Result<Vec<u8>, ThumbnailError> {
    let mesh = load_thumbnail_mesh_from_bytes(extension, bytes)?;
    rendering::render_mesh_thumbnail(mesh, spec)
}

/// Load a local file via the shared mmap-backed reader and render a thumbnail.
///
/// This path is preferred for Explorer `IInitializeWithFile` /
/// `IInitializeWithItem` initialization because it keeps the extension hint
/// for HPS and avoids an extra full-file copy for large STL/PLY/OBJ files.
///
/// # Errors
/// Returns [`ThumbnailError::Format`] for unsupported/malformed inputs and
/// [`ThumbnailError::Render`] for GPU/offscreen failures.
pub fn render_thumbnail_file(path: &Path, spec: ThumbnailSpec) -> Result<Vec<u8>, ThumbnailError> {
    let metadata = cache::thumbnail_file_metadata(path)?;
    let mesh = load_thumbnail_mesh_from_file(path, metadata)?;
    rendering::render_mesh_thumbnail(mesh, spec)
}

/// Render a thumbnail or return the deterministic fallback placeholder.
///
/// This is the COM-facing safe path: Explorer receives a bitmap even when the
/// file is malformed, unsupported, or rendering fails.
#[must_use]
pub fn render_thumbnail_or_placeholder(
    extension: Option<&str>,
    bytes: &[u8],
    spec: ThumbnailSpec,
) -> Vec<u8> {
    let extension = extension.map(ToOwned::to_owned);
    let bytes = Arc::<[u8]>::from(bytes.to_vec());
    render_thumbnail_shared_or_placeholder_with_timeout(
        extension,
        bytes,
        spec,
        DEFAULT_THUMBNAIL_TIMEOUT,
    )
}

/// Render a local file thumbnail or return the deterministic fallback
/// placeholder.
#[must_use]
pub fn render_thumbnail_file_or_placeholder(path: &Path, spec: ThumbnailSpec) -> Vec<u8> {
    render_thumbnail_file_or_placeholder_with_timeout(path, spec, DEFAULT_THUMBNAIL_TIMEOUT)
}

/// Render with a bounded wait or return the deterministic placeholder.
///
/// The worker thread may finish after the caller has returned; that is still
/// better than blocking Explorer's thumbnail worker beyond the time budget.
#[must_use]
pub fn render_thumbnail_or_placeholder_with_timeout(
    extension: Option<&str>,
    bytes: &[u8],
    spec: ThumbnailSpec,
    timeout: Duration,
) -> Vec<u8> {
    let extension = extension.map(ToOwned::to_owned);
    let bytes = Arc::<[u8]>::from(bytes.to_vec());
    render_thumbnail_shared_or_placeholder_with_timeout(extension, bytes, spec, timeout)
}

/// Render a local file with a bounded wait or return the deterministic
/// placeholder.
#[must_use]
pub fn render_thumbnail_file_or_placeholder_with_timeout(
    path: &Path,
    spec: ThumbnailSpec,
    timeout: Duration,
) -> Vec<u8> {
    let plan = match prepare_file_thumbnail_render(path, timeout) {
        Ok(plan) => plan,
        Err(FileThumbnailPreflightError::UnsupportedExtension) => {
            tracing::warn!(
                path = %path.display(),
                "thumbnail file extension is not registered for OccluView; returning placeholder"
            );
            return placeholder_thumbnail(spec);
        }
        Err(FileThumbnailPreflightError::Metadata(error)) => {
            tracing::warn!(?error, path = %path.display(), "thumbnail file metadata failed");
            return placeholder_thumbnail(spec);
        }
        Err(FileThumbnailPreflightError::Oversize { byte_len }) => {
            return placeholder_for_oversize_input(spec, byte_len);
        }
    };

    if let Ok(mut cache) = thumbnail_file_cache().lock() {
        if let Some(pixels) = cache.get(&plan.cache_key, spec.size_px) {
            return pixels;
        }
    }

    let (content_key, content_hit) = file_content_cache_lookup(path, &plan, spec);
    if let Some(pixels) = content_hit {
        return pixels;
    }

    let inflight_key = match content_key.clone() {
        Some(cache_key) => ThumbnailRequestKey::FileContent {
            cache_key,
            size_px: spec.size_px,
        },
        None => ThumbnailRequestKey::File {
            cache_key: plan.cache_key.clone(),
            size_px: spec.size_px,
        },
    };
    let path_cache_key = plan.cache_key;
    let metadata = plan.metadata;
    let wait_timeout = plan.wait_timeout;
    let path = path.to_path_buf();
    let cache_keys = FileThumbnailCacheKeys {
        path: path_cache_key,
        content: content_key,
    };
    render_coalesced_thumbnail(
        inflight_key,
        wait_timeout,
        move || render_file_thumbnail_job(path, metadata, cache_keys, spec, timeout),
        move || placeholder_thumbnail(spec),
    )
}

fn file_content_cache_lookup(
    path: &Path,
    plan: &cache::FileThumbnailRenderPlan,
    spec: ThumbnailSpec,
) -> (Option<ThumbnailFileContentKey>, Option<Vec<u8>>) {
    // If the file changes while Explorer is probing it, hashing is merely an
    // optimization failure: fall back to the path key and keep the contract.
    let content_key = thumbnail_file_content_key(path, &plan.metadata).ok();
    let Some(key) = content_key.as_ref() else {
        return (None, None);
    };
    let Some(pixels) = thumbnail_file_content_cache()
        .lock()
        .ok()
        .and_then(|mut cache| cache.get(key, spec.size_px))
    else {
        return (content_key, None);
    };
    if let Ok(mut path_cache) = thumbnail_file_cache().lock() {
        path_cache.insert(plan.cache_key.clone(), spec.size_px, &pixels);
    }
    (content_key, Some(pixels))
}

struct FileThumbnailCacheKeys {
    path: ThumbnailFileCacheKey,
    content: Option<ThumbnailFileContentKey>,
}

fn render_file_thumbnail_job(
    path: PathBuf,
    metadata: ThumbnailFileMetadata,
    cache_keys: FileThumbnailCacheKeys,
    spec: ThumbnailSpec,
    timeout: Duration,
) -> Vec<u8> {
    let setup_timeout = thumbnail_setup_timeout(timeout);
    let result = run_thumbnail_job_with_timeouts(setup_timeout, timeout, move |progress| {
        let result = (|| -> Result<Vec<u8>, ThumbnailError> {
            let mesh = load_thumbnail_mesh_from_file(&path, metadata)?;
            let _ = progress.send(ThumbnailJobProgress::Prepared);
            rendering::render_mesh_thumbnail(mesh, spec)
        })();
        if let Ok(pixels) = &result {
            cache_file_thumbnail(cache_keys.path, cache_keys.content, spec.size_px, pixels);
        }
        let _ = progress.send(ThumbnailJobProgress::Finished(result));
    });

    match result {
        ThumbnailJobOutcome::Finished(Ok(pixels)) => pixels,
        ThumbnailJobOutcome::Finished(Err(error)) => {
            let kind = placeholder_kind_for_error(&error);
            tracing::warn!(
                ?error,
                ?kind,
                "thumbnail file render failed; returning placeholder"
            );
            placeholder_thumbnail_kind(spec, kind)
        }
        ThumbnailJobOutcome::SetupTimedOut => {
            tracing::warn!(
                ?setup_timeout,
                "thumbnail file preparation timed out before a renderer became available; returning placeholder"
            );
            placeholder_thumbnail(spec)
        }
        ThumbnailJobOutcome::RenderTimedOut => {
            tracing::warn!(
                ?timeout,
                "thumbnail file render timed out after renderer checkout; returning placeholder"
            );
            placeholder_thumbnail(spec)
        }
        ThumbnailJobOutcome::Failed => {
            tracing::warn!("thumbnail file worker failed; returning placeholder");
            placeholder_thumbnail(spec)
        }
    }
}

fn cache_file_thumbnail(
    path_cache_key: ThumbnailFileCacheKey,
    content_cache_key: Option<ThumbnailFileContentKey>,
    size_px: u16,
    pixels: &[u8],
) {
    if let Ok(mut cache) = thumbnail_file_cache().lock() {
        cache.insert(path_cache_key, size_px, pixels);
    }
    if let Some(content_key) = content_cache_key {
        if let Ok(mut cache) = thumbnail_file_content_cache().lock() {
            cache.insert(content_key, size_px, pixels);
        }
    }
}

#[must_use]
/// Render shared stream bytes with a bounded wait and placeholder fallback.
pub fn render_thumbnail_shared_or_placeholder_with_timeout(
    extension: Option<String>,
    bytes: Arc<[u8]>,
    spec: ThumbnailSpec,
    timeout: Duration,
) -> Vec<u8> {
    render_thumbnail_shared_or_placeholder_with_timeout_impl(extension, bytes, spec, timeout, None)
}

#[must_use]
#[cfg_attr(not(windows), allow(dead_code))]
/// Render shared stream bytes using a previously acquired job reservation.
pub fn render_thumbnail_shared_or_placeholder_with_reservation(
    extension: Option<String>,
    bytes: Arc<[u8]>,
    spec: ThumbnailSpec,
    timeout: Duration,
    reservation: ThumbnailJobReservation,
) -> Vec<u8> {
    render_thumbnail_shared_or_placeholder_with_timeout_impl(
        extension,
        bytes,
        spec,
        timeout,
        Some(reservation),
    )
}

fn render_thumbnail_shared_or_placeholder_with_timeout_impl(
    extension: Option<String>,
    bytes: Arc<[u8]>,
    spec: ThumbnailSpec,
    timeout: Duration,
    reservation: Option<ThumbnailJobReservation>,
) -> Vec<u8> {
    let plan = match prepare_stream_thumbnail_render(extension.as_deref(), bytes.as_ref(), timeout)
    {
        Ok(plan) => plan,
        Err(StreamThumbnailPreflightError::Oversize { byte_len }) => {
            return placeholder_for_oversize_input(spec, byte_len);
        }
        Err(StreamThumbnailPreflightError::Format(error)) => {
            tracing::warn!(
                ?error,
                "thumbnail stream format inference failed before worker startup; returning placeholder"
            );
            return placeholder_thumbnail(spec);
        }
    };
    if let Ok(mut cache) = thumbnail_stream_cache().lock() {
        if let Some(pixels) = cache.get(&plan.cache_key, spec.size_px) {
            return pixels;
        }
    }

    let inflight_key = ThumbnailRequestKey::Stream {
        cache_key: plan.cache_key.clone(),
        size_px: spec.size_px,
    };
    render_coalesced_thumbnail(
        inflight_key,
        plan.wait_timeout,
        move || {
            let setup_timeout = thumbnail_setup_timeout(timeout);
            let cache_key_for_store = plan.cache_key.clone();
            let kind = plan.kind;
            let work = move |progress: std::sync::mpsc::SyncSender<
                ThumbnailJobProgress<Result<Vec<u8>, ThumbnailError>>,
            >| {
                let result = (|| -> Result<Vec<u8>, ThumbnailError> {
                    let mesh = load_thumbnail_mesh_from_bytes_kind(kind, bytes.as_ref())?;
                    let _ = progress.send(ThumbnailJobProgress::Prepared);
                    rendering::render_mesh_thumbnail(mesh, spec)
                })();
                // See the file path: cache from the worker so a render that
                // outran the caller's deadline still lands in the cache for the
                // next repaint instead of being thrown away.
                if let Ok(pixels) = &result {
                    if let Ok(mut cache) = thumbnail_stream_cache().lock() {
                        cache.insert(cache_key_for_store, spec.size_px, pixels);
                    }
                }
                let _ = progress.send(ThumbnailJobProgress::Finished(result));
            };
            let result = match reservation {
                Some(ThumbnailJobReservation(permit)) => {
                    run_thumbnail_job_with_permit(permit, setup_timeout, timeout, work)
                }
                None => run_thumbnail_job_with_timeouts(setup_timeout, timeout, work),
            };

            match result {
                ThumbnailJobOutcome::Finished(Ok(pixels)) => pixels,
                ThumbnailJobOutcome::Finished(Err(error)) => {
                    let kind = placeholder_kind_for_error(&error);
                    tracing::warn!(
                        ?error,
                        ?kind,
                        "thumbnail render failed; returning placeholder"
                    );
                    placeholder_thumbnail_kind(spec, kind)
                }
                ThumbnailJobOutcome::SetupTimedOut => {
                    tracing::warn!(
                        ?setup_timeout,
                        "thumbnail preparation timed out before a renderer became available; returning placeholder"
                    );
                    placeholder_thumbnail(spec)
                }
                ThumbnailJobOutcome::RenderTimedOut => {
                    tracing::warn!(
                        ?timeout,
                        "thumbnail render timed out after renderer checkout; returning placeholder"
                    );
                    placeholder_thumbnail(spec)
                }
                ThumbnailJobOutcome::Failed => {
                    tracing::warn!("thumbnail worker failed; returning placeholder");
                    placeholder_thumbnail(spec)
                }
            }
        },
        move || placeholder_thumbnail(spec),
    )
}

/// Return the policy placeholder for an input that exceeds the size ceiling.
pub fn placeholder_for_oversize_input(spec: ThumbnailSpec, byte_len: usize) -> Vec<u8> {
    let error = oversize_input_error(byte_len);
    tracing::warn!(
        ?error,
        byte_len,
        "thumbnail input exceeded size policy; returning placeholder"
    );
    // Over-budget is a policy decision, not a broken file: quiet plain cube.
    placeholder_thumbnail(spec)
}

/// Pick the placeholder flavor for a thumbnail failure.
///
/// A *recognized* format that fails to decode (truncated / malformed / bad
/// signature / core-geometry error) gets the [`PlaceholderKind::Corrupt`] badge
/// — the file itself looks broken. Everything else (unsupported payloads,
/// encrypted HPS without a key = [`FormatError::Deferred`], oversize sentinel
/// errors, I/O, and GPU/renderer/timeout failures) gets the quiet
/// [`PlaceholderKind::Plain`] cube.
fn placeholder_kind_for_error(error: &ThumbnailError) -> PlaceholderKind {
    match error {
        ThumbnailError::Format(format_error) => match format_error {
            // Oversize inputs surface as a synthetic `Malformed` with a
            // "thumbnail …" format tag; that is a budget decision, not a broken
            // file, so keep it plain.
            FormatError::Malformed { format, .. } if format.starts_with("thumbnail") => {
                PlaceholderKind::Plain
            }
            FormatError::BadSignature { .. }
            | FormatError::Truncated { .. }
            | FormatError::Malformed { .. }
            | FormatError::Core(_) => PlaceholderKind::Corrupt,
            FormatError::Unsupported { .. }
            | FormatError::Deferred { .. }
            | FormatError::UnsafePath { .. }
            | FormatError::Io(_) => PlaceholderKind::Plain,
        },
        ThumbnailError::Render(_) => PlaceholderKind::Plain,
    }
}
