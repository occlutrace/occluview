//! Deterministic placeholder thumbnails for shell fallback paths.

use occluview_render::ThumbnailSpec;

/// Build an opaque RGBA placeholder thumbnail.
///
/// The placeholder is intentionally pure and deterministic: it does not touch
/// GPU, filesystem, fonts, or platform APIs, so the COM layer can use it as a
/// last-resort shell-safe fallback.
#[must_use]
pub fn placeholder_thumbnail(spec: ThumbnailSpec) -> Vec<u8> {
    let size = usize::from(spec.size_px.max(1));
    let mut pixels = Vec::with_capacity(size * size * 4);
    for y in 0..size {
        for x in 0..size {
            pixels.extend_from_slice(&placeholder_pixel(x, y, size));
        }
    }
    pixels
}

fn placeholder_pixel(x: usize, y: usize, size: usize) -> [u8; 4] {
    let border = x == 0 || y == 0 || x + 1 == size || y + 1 == size;
    if border {
        return [72, 86, 101, 255];
    }

    let center = size / 2;
    let diamond = x.abs_diff(center) + y.abs_diff(center) <= (size / 5).max(1);
    if diamond {
        return [86, 165, 190, 255];
    }

    if ((x / 4) + (y / 4)) % 2 == 0 {
        [24, 28, 33, 255]
    } else {
        [31, 36, 42, 255]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_has_requested_rgba_size() {
        let spec = ThumbnailSpec {
            size_px: 16,
            ..Default::default()
        };
        assert_eq!(placeholder_thumbnail(spec).len(), 16 * 16 * 4);
    }

    #[test]
    fn placeholder_is_opaque_and_deterministic() {
        let spec = ThumbnailSpec {
            size_px: 12,
            ..Default::default()
        };
        let first = placeholder_thumbnail(spec);
        let second = placeholder_thumbnail(spec);
        assert_eq!(first, second);
        assert!(first.chunks_exact(4).all(|px| px[3] == 255));
    }

    #[test]
    fn placeholder_has_frame_and_interior_contrast() {
        let spec = ThumbnailSpec {
            size_px: 16,
            ..Default::default()
        };
        let pixels = placeholder_thumbnail(spec);
        let top_left = &pixels[0..4];
        let center_offset = ((8 * 16) + 8) * 4;
        let center = &pixels[center_offset..center_offset + 4];
        assert_ne!(top_left, center);
    }
}
