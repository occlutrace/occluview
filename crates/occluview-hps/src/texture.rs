use crate::{
    base64, crypto,
    parser::{encrypted_element_uses_scrambled_key, original_size_attr},
    xml, DecodedTexture, HpsError,
};
use image::GenericImageView;
use std::io::Cursor;
use std::mem::size_of;
use zeroize::Zeroizing;

const MAX_TEXTURE_DIMENSION_PX: u32 = 8_192;
const MAX_TEXTURE_RGBA_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Default)]
pub(super) struct SurfaceTexture {
    pub(super) corner_uvs: Option<Vec<Option<[f32; 2]>>>,
    pub(super) texture: Option<DecodedTexture>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum RawTextureLayout {
    Rgb,
    Bgr,
    Rgba,
    Bgra,
    Argb,
    Abgr,
}

pub(super) fn parse_texture_data(
    text: &str,
    schema: &str,
    key: Option<&[u8]>,
    vertex_count: usize,
    indices: &[u32],
) -> Result<Option<SurfaceTexture>, HpsError> {
    let corner_uvs = parse_texture_coordinates(text, schema, key, vertex_count, indices)?;
    let texture = parse_texture_image(text)?;
    if corner_uvs.is_none() && texture.is_none() {
        return Ok(None);
    }
    Ok(Some(SurfaceTexture {
        corner_uvs,
        texture,
    }))
}

fn parse_texture_coordinates(
    text: &str,
    schema: &str,
    key: Option<&[u8]>,
    vertex_count: usize,
    indices: &[u32],
) -> Result<Option<Vec<Option<[f32; 2]>>>, HpsError> {
    let Some(element) = xml::find_elements(text, "PerVertexTextureCoord")
        .into_iter()
        .next()
    else {
        return Ok(None);
    };

    let mut raw = Zeroizing::new(base64::decode(element.body)?);
    if schema == "CE" {
        let key = key.ok_or(HpsError::KeyMissing)?;
        raw = crypto::decrypt_hps_data(
            &raw,
            key,
            original_size_attr(element.open_tag)?,
            encrypted_element_uses_scrambled_key(element.open_tag)?,
        )?;
    }

    decode_per_vertex_texture_coordinates(&raw, vertex_count, indices).map(Some)
}

fn parse_texture_image(text: &str) -> Result<Option<DecodedTexture>, HpsError> {
    let Some(element) = xml::find_elements(text, "TextureImage").into_iter().next() else {
        return Ok(None);
    };
    let bytes = base64::decode(element.body)?;
    let texture = if let Some(texture) = parse_raw_texture_image(element.open_tag, &bytes)? {
        texture
    } else {
        decode_embedded_raster(&bytes)?
    };
    Ok(Some(correct_channel_order_for_dental(texture)))
}

fn decode_embedded_raster(bytes: &[u8]) -> Result<DecodedTexture, HpsError> {
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|error| texture_malformed(format!("texture format detection failed: {error}")))?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_TEXTURE_DIMENSION_PX);
    limits.max_image_height = Some(MAX_TEXTURE_DIMENSION_PX);
    limits.max_alloc = Some(MAX_TEXTURE_RGBA_BYTES);
    reader.limits(limits);
    let decoded = reader
        .decode()
        .map_err(|error| texture_malformed(format!("texture image decode failed: {error}")))?;
    let (width, height) = decoded.dimensions();
    validate_texture_dimensions(width, height)?;
    DecodedTexture::new(width, height, decoded.to_rgba8().into_raw())
}

fn validate_texture_dimensions(width: u32, height: u32) -> Result<(), HpsError> {
    if width == 0 || height == 0 {
        return Err(texture_malformed("texture dimensions must be non-zero"));
    }
    if width > MAX_TEXTURE_DIMENSION_PX || height > MAX_TEXTURE_DIMENSION_PX {
        return Err(texture_malformed(format!(
            "texture dimensions {width}x{height} exceed {MAX_TEXTURE_DIMENSION_PX}px limit"
        )));
    }
    let rgba_bytes = u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or(HpsError::ResourceLimit {
            resource: "texture RGBA size",
            limit: MAX_TEXTURE_RGBA_BYTES,
        })?;
    if rgba_bytes > MAX_TEXTURE_RGBA_BYTES {
        return Err(HpsError::ResourceLimit {
            resource: "texture RGBA size",
            limit: MAX_TEXTURE_RGBA_BYTES,
        });
    }
    Ok(())
}

fn texture_malformed(reason: impl Into<String>) -> HpsError {
    HpsError::TextureMalformed {
        reason: reason.into(),
    }
}

/// Final domain-prior guard against a swapped R/B channel order.
///
/// A dental scan is physically warm: enamel is cream/white and gingiva is red or
/// pink, so the mean RED channel always meets or exceeds the mean BLUE. A decoded
/// texture whose blue clearly dominates red is impossible for a real scan — it is
/// the fingerprint of a swapped R<->B order. HPS texture channel metadata
/// is inconsistent across exporter versions and is often absent, so the declared
/// or defaulted layout above can be wrong for a given file (the symptom: red
/// gingiva renders bright blue). When that fingerprint is present, swap R<->B so
/// the scan reads warm. A texture that is already warm or neutral is left exactly
/// as-is, so this can never cool a correct texture.
fn correct_channel_order_for_dental(mut texture: DecodedTexture) -> DecodedTexture {
    if texture_is_implausibly_blue(texture.rgba()) {
        for pixel in texture.rgba_mut().chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
    }
    texture
}

/// Whether the texture's opaque, saturated pixels are blue-dominant by a clear
/// margin — a physical impossibility for a dental scan and hence a reliable
/// swapped-channel fingerprint. Near-gray pixels (`R≈B`) carry no channel-order
/// signal and are ignored; a wholly gray/neutral texture is never flagged.
fn texture_is_implausibly_blue(rgba: &[u8]) -> bool {
    let pixel_count = rgba.len() / 4;
    if pixel_count == 0 {
        return false;
    }
    // Cap the scan at ~4096 evenly-strided pixels regardless of texture size.
    let stride = (pixel_count / 4096).max(1);
    let mut red_sum = 0u64;
    let mut blue_sum = 0u64;
    let mut counted = 0u64;
    for pixel in rgba.chunks_exact(4).step_by(stride) {
        if pixel[3] < 8 {
            continue; // transparent: no color signal
        }
        let red = u32::from(pixel[0]);
        let blue = u32::from(pixel[2]);
        if red.abs_diff(blue) < 16 {
            continue; // near-gray: no channel-order signal
        }
        red_sum += u64::from(red);
        blue_sum += u64::from(blue);
        counted += 1;
    }
    if counted == 0 {
        return false;
    }
    let red_mean = red_sum / counted;
    let blue_mean = blue_sum / counted;
    // Blue must beat red by a clear margin (≥25% of red, at least 24 levels) to
    // trip — conservative, so a merely cool-but-valid texture is never flipped.
    blue_mean > red_mean + (red_mean / 4).max(24)
}

fn parse_raw_texture_image(
    open_tag: &str,
    bytes: &[u8],
) -> Result<Option<DecodedTexture>, HpsError> {
    let Some(width) = optional_u32_attr(open_tag, "Width")? else {
        return Ok(None);
    };
    let Some(height) = optional_u32_attr(open_tag, "Height")? else {
        return Ok(None);
    };
    let Some(bytes_per_pixel) = optional_u32_attr(open_tag, "BytesPerPixel")? else {
        return Ok(None);
    };

    validate_texture_dimensions(width, height)?;

    let raw_len = raw_texture_len(width, height, bytes_per_pixel)?;
    if bytes.len() != raw_len {
        return Ok(None);
    }

    // Deterministic decode (no pixel-content guessing): honor an explicit,
    // unambiguous pixel-format declaration when present; otherwise fall back to
    // the HPS/DirectX default. HPS raw textures are DirectX surfaces
    // (D3DFMT_A8R8G8B8 / D3DFMT_R8G8B8), whose little-endian MEMORY byte order
    // is B,G,R(,A) — so the correct default is BGRA / BGR (swap R<->B).
    let layout =
        raw_texture_layout(open_tag)?.unwrap_or_else(|| default_raw_layout(bytes_per_pixel));
    let Some(rgba) = decode_raw_texture_pixels(bytes, bytes_per_pixel, layout) else {
        return Ok(None);
    };
    DecodedTexture::new(width, height, rgba).map(Some)
}

/// The default channel layout for a format-less raw HPS texture.
///
/// HPS emits DirectX surfaces (`D3DFMT_A8R8G8B8` for 32-bit, `D3DFMT_R8G8B8`
/// for 24-bit). A D3DFMT name lists channels from the high byte of a 32-bit pixel
/// to the low byte, so the little-endian MEMORY byte order is the reverse:
/// `B,G,R,A` for `A8R8G8B8` and `B,G,R` for `R8G8B8`. Decoding as BGRA/BGR (swap
/// R<->B) is the verified-correct behavior; treating the bytes as RGBA turns warm
/// dental whites (R>=G>B) into cool blue (B>R).
fn default_raw_layout(bytes_per_pixel: u32) -> RawTextureLayout {
    match bytes_per_pixel {
        3 => RawTextureLayout::Bgr,
        _ => RawTextureLayout::Bgra,
    }
}

fn raw_texture_layout(open_tag: &str) -> Result<Option<RawTextureLayout>, HpsError> {
    const FORMAT_ATTRS: [&str; 8] = [
        "PixelFormat",
        "pixelFormat",
        "Format",
        "format",
        "ChannelOrder",
        "channelOrder",
        "ColorFormat",
        "colorFormat",
    ];

    for attr in FORMAT_ATTRS {
        let Some(value) = xml::attr_value(open_tag, attr)? else {
            continue;
        };
        let normalized: String = value
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .flat_map(char::to_lowercase)
            .collect();
        // DirectX D3DFMT names (digit forms) list channels high-byte-first, so
        // the little-endian MEMORY byte order is the REVERSE. Map them to the
        // memory layout the bytes actually arrive in — the historic mismap of
        // A8R8G8B8 -> literal ARGB is exactly what rendered white scans blue.
        if normalized.contains("a8r8g8b8") || normalized.contains("x8r8g8b8") {
            return Ok(Some(RawTextureLayout::Bgra)); // 0xAARRGGBB -> [B,G,R,A]
        }
        if normalized.contains("a8b8g8r8") || normalized.contains("x8b8g8r8") {
            return Ok(Some(RawTextureLayout::Rgba)); // 0xAABBGGRR -> [R,G,B,A]
        }
        // DXGI-style names (digit forms) already list MEMORY byte order.
        if normalized.contains("b8g8r8a8") {
            return Ok(Some(RawTextureLayout::Bgra));
        }
        if normalized.contains("r8g8b8a8") {
            return Ok(Some(RawTextureLayout::Rgba));
        }
        if normalized.contains("b8g8r8") {
            return Ok(Some(RawTextureLayout::Bgr));
        }
        if normalized.contains("r8g8b8") {
            return Ok(Some(RawTextureLayout::Rgb));
        }
        // Bare tokens state the MEMORY byte order literally.
        if normalized.contains("abgr") {
            return Ok(Some(RawTextureLayout::Abgr));
        }
        if normalized.contains("argb") {
            return Ok(Some(RawTextureLayout::Argb));
        }
        if normalized.contains("bgra") {
            return Ok(Some(RawTextureLayout::Bgra));
        }
        if normalized.contains("rgba") {
            return Ok(Some(RawTextureLayout::Rgba));
        }
        if normalized.contains("bgr") {
            return Ok(Some(RawTextureLayout::Bgr));
        }
        if normalized.contains("rgb") {
            return Ok(Some(RawTextureLayout::Rgb));
        }
    }

    Ok(None)
}

fn decode_raw_texture_pixels(
    bytes: &[u8],
    bytes_per_pixel: u32,
    layout: RawTextureLayout,
) -> Option<Vec<u8>> {
    let mut rgba = Vec::with_capacity((bytes.len() / bytes_per_pixel as usize) * 4);
    for pixel in bytes.chunks_exact(bytes_per_pixel as usize) {
        rgba.extend_from_slice(&decode_raw_texture_pixel(pixel, bytes_per_pixel, layout)?);
    }
    Some(rgba)
}

fn decode_raw_texture_pixel(
    pixel: &[u8],
    bytes_per_pixel: u32,
    layout: RawTextureLayout,
) -> Option<[u8; 4]> {
    match (bytes_per_pixel, layout) {
        (3, RawTextureLayout::Rgb) => Some([pixel[0], pixel[1], pixel[2], 255]),
        (3, RawTextureLayout::Bgr) => Some([pixel[2], pixel[1], pixel[0], 255]),
        (4, RawTextureLayout::Rgb | RawTextureLayout::Rgba) => {
            Some([pixel[0], pixel[1], pixel[2], pixel[3]])
        }
        (4, RawTextureLayout::Bgr | RawTextureLayout::Bgra) => {
            Some([pixel[2], pixel[1], pixel[0], pixel[3]])
        }
        (4, RawTextureLayout::Argb) => Some([pixel[1], pixel[2], pixel[3], pixel[0]]),
        (4, RawTextureLayout::Abgr) => Some([pixel[3], pixel[2], pixel[1], pixel[0]]),
        _ => None,
    }
}

fn optional_u32_attr(open_tag: &str, attr: &str) -> Result<Option<u32>, HpsError> {
    xml::attr_value(open_tag, attr)?
        .map(|value| xml::parse_u32_attr(value, attr))
        .transpose()
}

fn raw_texture_len(width: u32, height: u32, bytes_per_pixel: u32) -> Result<usize, HpsError> {
    width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(bytes_per_pixel))
        .and_then(|len| usize::try_from(len).ok())
        .ok_or_else(|| texture_malformed("raw texture image size overflow"))
}

fn decode_per_vertex_texture_coordinates(
    bytes: &[u8],
    vertex_count: usize,
    indices: &[u32],
) -> Result<Vec<Option<[f32; 2]>>, HpsError> {
    let corners_by_vertex = build_vertex_corner_map(indices, vertex_count)?;
    let mut corner_uvs = vec![None; indices.len()];
    let mut offset = 0;

    for corners in corners_by_vertex.iter().take(vertex_count) {
        let flag = read_u8(bytes, &mut offset)?;
        let degree = corners.len();
        if flag == 0 {
            if degree != 0 {
                return Err(texture_malformed(
                    "texture coordinate stream degree mismatch",
                ));
            }
            continue;
        }

        let uv_count = if flag == 1 {
            1
        } else if usize::from(flag) == degree {
            degree
        } else {
            return Err(texture_malformed("texture coordinate stream flag mismatch"));
        };

        let mut decoded = Vec::with_capacity(uv_count);
        for _ in 0..uv_count {
            decoded.push(decode_packed_texture_coordinate(read_u32_le(
                bytes,
                &mut offset,
            )?));
        }

        if degree == 0 {
            continue;
        }
        if flag == 1 {
            for &corner in corners {
                corner_uvs[corner] = decoded[0];
            }
        } else {
            for (corner_ordinal, &corner) in corners.iter().enumerate() {
                corner_uvs[corner] = decoded[corner_ordinal];
            }
        }
    }

    Ok(corner_uvs)
}

fn build_vertex_corner_map(
    indices: &[u32],
    vertex_count: usize,
) -> Result<Vec<Vec<usize>>, HpsError> {
    let mut corners_by_vertex = vec![Vec::new(); vertex_count];
    for (corner, &vertex_index) in indices.iter().enumerate() {
        let vertex_index = vertex_index as usize;
        let Some(corners) = corners_by_vertex.get_mut(vertex_index) else {
            return Err(texture_malformed(
                "texture coordinate vertex index out of bounds",
            ));
        };
        corners.push(corner);
    }
    Ok(corners_by_vertex)
}

fn read_u8(bytes: &[u8], offset: &mut usize) -> Result<u8, HpsError> {
    let Some(&value) = bytes.get(*offset) else {
        return Err(texture_malformed("texture coordinate stream truncated"));
    };
    *offset += 1;
    Ok(value)
}

fn read_u32_le(bytes: &[u8], offset: &mut usize) -> Result<u32, HpsError> {
    let end = offset
        .checked_add(size_of::<u32>())
        .ok_or_else(|| texture_malformed("texture coordinate stream offset overflow"))?;
    let chunk = bytes
        .get(*offset..end)
        .ok_or_else(|| texture_malformed("texture coordinate stream truncated"))?;
    *offset = end;
    Ok(u32::from_le_bytes(chunk.try_into().map_err(|_| {
        texture_malformed("bad texture coordinate")
    })?))
}

fn decode_packed_texture_coordinate(packed: u32) -> Option<[f32; 2]> {
    if packed == u32::MAX {
        return None;
    }
    Some([
        decode_packed_texture_component((packed & 0xffff) as u16),
        decode_packed_texture_component(((packed >> 16) & 0xffff) as u16),
    ])
}

fn decode_packed_texture_component(component: u16) -> f32 {
    let value = f32::from(component & 0x7fff);
    if component & 0x8000 == 0 {
        value / 32767.0
    } else {
        value * (512.0 / 32767.0) - 256.0
    }
}

#[cfg(test)]
mod channel_prior_tests {
    #![allow(clippy::expect_used)]

    use super::{correct_channel_order_for_dental, texture_is_implausibly_blue};
    use crate::DecodedTexture;

    fn solid(width: u32, height: u32, rgb: [u8; 3]) -> DecodedTexture {
        let n = (width * height) as usize;
        let mut rgba = Vec::with_capacity(n * 4);
        for _ in 0..n {
            rgba.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
        }
        DecodedTexture::new(width, height, rgba).expect("solid texture is valid")
    }

    #[test]
    fn swapped_red_gingiva_is_corrected_back_to_warm() {
        // Red gingiva (200,60,55) rendered bright blue by a swapped R<->B order
        // (55,60,200) — the owner's exact symptom. The prior must swap it back.
        let blue = solid(8, 8, [55, 60, 200]);
        assert!(texture_is_implausibly_blue(blue.rgba()));
        let fixed = correct_channel_order_for_dental(blue);
        assert_eq!(&fixed.rgba()[0..3], &[200, 60, 55], "red must be restored");
    }

    #[test]
    fn already_warm_texture_is_left_untouched() {
        // Warm enamel/gingiva mix: red dominant. Must never be cooled.
        let warm = solid(8, 8, [210, 170, 150]);
        assert!(!texture_is_implausibly_blue(warm.rgba()));
        let same = correct_channel_order_for_dental(warm.clone());
        assert_eq!(same.rgba(), warm.rgba());
    }

    #[test]
    fn neutral_gray_texture_is_never_flagged() {
        // A near-gray texture carries no channel-order signal — leave it alone.
        let gray = solid(8, 8, [128, 130, 129]);
        assert!(!texture_is_implausibly_blue(gray.rgba()));
        let same = correct_channel_order_for_dental(gray.clone());
        assert_eq!(same.rgba(), gray.rgba());
    }

    #[test]
    fn a_faintly_cool_but_valid_texture_is_not_flipped() {
        // Slightly cool (blue a touch above red) but within the conservative
        // margin — must NOT be treated as a swap.
        let cool = solid(8, 8, [150, 150, 168]);
        assert!(!texture_is_implausibly_blue(cool.rgba()));
    }
}
