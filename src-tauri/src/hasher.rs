use std::path::Path;
use anyhow::Result;
use image::GenericImageView;
use image_hasher::{HashAlg, HasherConfig, ImageHash};

/// Reads the file once and returns Blake3 hash + raw bytes.
pub fn read_file_data(path: &Path) -> Result<(String, u64, Vec<u8>)> {
    let bytes = std::fs::read(path)?;
    let size  = bytes.len() as u64;
    let hash  = blake3::hash(&bytes).to_hex().to_string();
    Ok((hash, size, bytes))
}

/// Extracts the raw JPEG thumbnail bytes embedded in a JPEG's EXIF APP1 segment.
///
/// Parses: JPEG SOI → APP1 (FF E1 + "Exif\0\0") → TIFF header → IFD0 → IFD1
/// → JPEGInterchangeFormat (0x0201) offset + JPEGInterchangeFormatLength (0x0202).
/// All accesses are bounds-checked; returns None for any malformed/missing data.
fn extract_exif_thumbnail(data: &[u8]) -> Option<Vec<u8>> {
    // Must start with JPEG SOI (FF D8)
    if data.len() < 4 || data[0] != 0xFF || data[1] != 0xD8 {
        return None;
    }

    // Walk JPEG segments until we find APP1 with an EXIF header
    let mut pos = 2usize;
    let tiff: &[u8] = loop {
        if pos + 4 > data.len() { return None; }
        if data[pos] != 0xFF    { return None; }
        let marker  = data[pos + 1];
        // SOS / EOI mean we've passed the header area
        if marker == 0xDA || marker == 0xD9 { return None; }
        let seg_len = u16::from_be_bytes([data[pos + 2], data[pos + 3]]) as usize;
        let seg_end = pos + 2 + seg_len; // marker(2) + length(includes itself)

        if marker == 0xE1 && seg_end <= data.len() {
            // data[pos+4..pos+10] should be "Exif\0\0"
            if data.get(pos + 4..pos + 10) == Some(b"Exif\x00\x00") {
                break &data[pos + 10..seg_end]; // TIFF data starts here
            }
        }
        if seg_len < 2 { return None; } // guard against infinite loop
        pos += 2 + seg_len;
    };

    if tiff.len() < 8 { return None; }

    // Determine byte order from TIFF header ("II" = little-endian, "MM" = big-endian)
    let le = match tiff.get(..2)? {
        b"II" => true,
        b"MM" => false,
        _     => return None,
    };

    // Bounds-checked TIFF integer readers
    let r16 = |o: usize| -> Option<u16> {
        let b = tiff.get(o..o + 2)?;
        Some(if le { u16::from_le_bytes([b[0], b[1]]) }
             else  { u16::from_be_bytes([b[0], b[1]]) })
    };
    let r32 = |o: usize| -> Option<u32> {
        let b = tiff.get(o..o + 4)?;
        Some(if le { u32::from_le_bytes([b[0], b[1], b[2], b[3]]) }
             else  { u32::from_be_bytes([b[0], b[1], b[2], b[3]]) })
    };

    // IFD0: offset at bytes 4-7 of TIFF header
    let ifd0 = r32(4)? as usize;
    let ifd0_entries = r16(ifd0)? as usize;

    // IFD1 pointer sits right after the last IFD0 entry (12 bytes each)
    let ifd1 = r32(ifd0 + 2 + ifd0_entries * 12)? as usize;
    if ifd1 == 0 { return None; }

    let ifd1_entries = r16(ifd1)? as usize;
    let mut thumb_off: Option<usize> = None;
    let mut thumb_len: Option<usize> = None;

    for i in 0..ifd1_entries {
        let e = ifd1 + 2 + i * 12;
        match r16(e)? {
            0x0201 => thumb_off = Some(r32(e + 8)? as usize), // JPEGInterchangeFormat
            0x0202 => thumb_len = Some(r32(e + 8)? as usize), // JPEGInterchangeFormatLength
            _      => {}
        }
    }

    let off = thumb_off?;
    let len = thumb_len?;
    if len == 0 { return None; }

    tiff.get(off..off + len).map(<[u8]>::to_vec)
}

/// Computes pHash from already-loaded bytes.
///
/// `use_thumbnail = true`  (fast mode):
///   Tries to extract the embedded EXIF JPEG thumbnail first (~20 KB, ~1 ms decode).
///   Falls back to full image decode if no thumbnail is present or it is corrupt.
///
/// `use_thumbnail = false` (precise mode):
///   Always decodes the full image. Slower but uses the highest-quality source,
///   making the hash more reliable for borderline similarity cases.
///
/// The image is passed directly to image_hasher without any pre-resize so the
/// library can apply its own high-quality filter when scaling down to the 8×8
/// hash grid. Pre-resizing with Nearest beforehand discarded too much information
/// and caused visually different images to collide at Hamming distance 0.
///
/// Returns `Err` if the image cannot be decoded; callers that want to skip
/// undecoded files can call `.ok()` at the use site.
pub fn perceptual_hash_from_bytes(bytes: &[u8], use_thumbnail: bool) -> Result<ImageHash, image::ImageError> {
    let hasher = HasherConfig::new()
        .hash_size(8, 8)
        .hash_alg(HashAlg::Gradient)
        .to_hasher();

    if use_thumbnail {
        if let Some(thumb) = extract_exif_thumbnail(bytes) {
            if let Ok(img) = image::load_from_memory(&thumb) {
                return Ok(hasher.hash_image(&img));
            }
        }
    }

    // Full image decode (always used in precise mode; fallback in fast mode)
    let img = image::load_from_memory(bytes)?;
    Ok(hasher.hash_image(&img))
}

/// Sharpness score via Laplacian variance.
///
/// Decodes to luma and downsamples to at most 256 px on the longest side
/// before applying the 4-neighbour Laplacian kernel — cheap enough to run on
/// every image during the scan without meaningfully slowing it down.
///
/// Returns `None` if the image cannot be decoded or is smaller than 3×3 px.
/// Higher values mean a sharper image (typical range: ~10 for blurry, ~500+
/// for very sharp; depends on image content and resolution).
pub fn laplacian_variance(bytes: &[u8]) -> Option<f64> {
    let img = image::load_from_memory(bytes).ok()?;
    let (w, h) = img.dimensions();

    // Downsample to ≤256 px on the longest side so computation stays O(1) in practice.
    let scale = (256.0_f32 / w.max(h) as f32).min(1.0_f32);
    let sw = ((w as f32 * scale) as u32).max(3);
    let sh = ((h as f32 * scale) as u32).max(3);

    let gray = img
        .resize_exact(sw, sh, image::imageops::FilterType::Nearest)
        .to_luma8();

    let pixels = gray.as_raw();
    let w = sw as usize;
    let h = sh as usize;
    let n = (w - 2) * (h - 2);

    // Apply 4-neighbour Laplacian: [0,1,0; 1,-4,1; 0,1,0]
    // Compute variance = E[X²] - E[X]² in a single pass.
    let mut sum    = 0.0_f64;
    let mut sum_sq = 0.0_f64;
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let c = pixels[y * w + x] as f64;
            let v = pixels[(y - 1) * w + x] as f64
                  + pixels[(y + 1) * w + x] as f64
                  + pixels[y * w + (x - 1)] as f64
                  + pixels[y * w + (x + 1)] as f64
                  - 4.0 * c;
            sum    += v;
            sum_sq += v * v;
        }
    }
    let n = n as f64;
    let mean = sum / n;
    Some(sum_sq / n - mean * mean)
}
