use tauri::Manager;
use super::{AppError, cache_data_dir, to_base64_data_uri};

#[tauri::command]
pub async fn get_thumbnail(path: String, app: tauri::AppHandle) -> Result<String, AppError> {
    let resource_dir    = app.path().resource_dir().ok();
    let thumb_cache_dir = cache_data_dir(&app).ok().map(|d| d.join("thumb_cache"));

    tokio::task::spawn_blocking(move || -> Result<String, AppError> {
        use image::imageops::FilterType;
        use std::io::{Cursor, Seek, SeekFrom};

        let thumb_err = |msg: String| AppError::Thumbnail { message: msg };

        let lower   = path.to_lowercase();
        let is_heic = lower.ends_with(".heic") || lower.ends_with(".heif") || lower.ends_with(".avif");

        if is_heic {
            // Read once — used for the cache key.
            let heic_bytes = std::fs::read(&path).map_err(|e| thumb_err(e.to_string()))?;

            let cache_path = thumb_cache_dir.as_ref().map(|dir| {
                let hash = blake3::hash(&heic_bytes);
                let name = format!("{}.jpg", &hash.to_hex()[..16]);
                dir.join(name)
            });

            if let Some(ref cp) = cache_path {
                if cp.exists() {
                    if let Ok(cached) = std::fs::read(cp) {
                        tracing::debug!("thumb HIT (heic): {}", path);
                        return Ok(to_base64_data_uri(&cached, "image/jpeg"));
                    }
                }
            }

            tracing::debug!("thumb MISS (heic): {}", path);

            let (tmp, _, _) = crate::heic::heic_to_temp_jpeg(
                std::path::Path::new(&path),
                resource_dir.as_deref(),
                None, // full resolution for thumbnail/viewer
            ).ok_or_else(|| thumb_err("HEIC converter not available".to_string()))?;

            let jpeg_bytes = std::fs::read(&tmp).map_err(|e| thumb_err(e.to_string()))?;
            let _ = std::fs::remove_file(&tmp);

            let img = image::load_from_memory(&jpeg_bytes).map_err(|e| thumb_err(e.to_string()))?;
            // Normalise to 8-bit RGB — mirrors the PNG fix; prevents JPEG encoder
            // failures when magick/sips produces output with an unusual bit depth
            // or colour space (e.g. HDR/wide-gamut HEICs from iPhone Pro models).
            let img   = image::DynamicImage::ImageRgb8(img.into_rgb8());
            let thumb = img.resize(180, 180, FilterType::Nearest);
            let mut buf = Cursor::new(Vec::<u8>::new());
            thumb.write_to(&mut buf, image::ImageFormat::Jpeg).map_err(|e| thumb_err(e.to_string()))?;
            buf.seek(SeekFrom::Start(0)).map_err(|e| thumb_err(e.to_string()))?;
            let thumb_bytes = buf.into_inner();

            if let Some(ref cp) = cache_path {
                if let Some(parent) = cp.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::fs::write(cp, &thumb_bytes);
                tracing::debug!("thumb SAVE (heic): {}", path);
            }

            return Ok(to_base64_data_uri(&thumb_bytes, "image/jpeg"));
        }

        // Non-HEIC: handles local PNGs (WebView2 struggles with some variants)
        // and network paths (which cannot use convertFileSrc).
        let bytes = std::fs::read(&path).map_err(|e| thumb_err(e.to_string()))?;

        let cache_path = thumb_cache_dir.as_ref().and_then(|dir| {
            let hash = blake3::hash(&bytes);
            let name = format!("jpg_{}.jpg", &hash.to_hex()[..16]);
            Some(dir.join(name))
        });

        if let Some(ref cp) = cache_path {
            if cp.exists() {
                if let Ok(cached) = std::fs::read(cp) {
                    tracing::debug!("thumb HIT (jpg/net): {}", path);
                    return Ok(to_base64_data_uri(&cached, "image/jpeg"));
                }
            }
        }

        tracing::debug!("thumb MISS (jpg/net): {}", path);

        let img = match image::load_from_memory(&bytes) {
            Ok(img) => img,
            Err(e) => {
                // The image crate failed to decode the file (unusual PNG variant,
                // unsupported bit depth, etc.). Return the raw bytes as a data URI
                // so the browser can still try to render it natively.
                tracing::warn!(error = %e, path = %path, "thumb decode failed, returning raw");
                let ext = std::path::Path::new(&path)
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                let mime = match ext.as_str() {
                    "png"        => "image/png",
                    "gif"        => "image/gif",
                    "webp"       => "image/webp",
                    "bmp"        => "image/bmp",
                    "tiff"|"tif" => "image/tiff",
                    _            => "image/jpeg",
                };
                return Ok(to_base64_data_uri(&bytes, mime));
            }
        };
        let img   = apply_exif_orientation(&bytes, img);
        // Normalise to 8-bit RGB before resize: JPEG does not support 16-bit colour
        // depth, so 48-bit (16bpc) PNGs would cause write_to to fail otherwise.
        let img   = image::DynamicImage::ImageRgb8(img.into_rgb8());
        let thumb = img.resize(180, 180, FilterType::Nearest);
        let mut buf = Cursor::new(Vec::<u8>::new());
        thumb.write_to(&mut buf, image::ImageFormat::Jpeg).map_err(|e| thumb_err(e.to_string()))?;
        buf.seek(SeekFrom::Start(0)).map_err(|e| thumb_err(e.to_string()))?;
        let thumb_bytes = buf.into_inner();

        if let Some(ref cp) = cache_path {
            if let Some(parent) = cp.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(cp, &thumb_bytes);
            tracing::debug!("thumb SAVE (jpg/net): {}", path);
        }

        Ok(to_base64_data_uri(&thumb_bytes, "image/jpeg"))
    })
    .await?
}

#[tauri::command]
pub async fn get_full_image(path: String, app: tauri::AppHandle) -> Result<String, AppError> {
    let resource_dir = app.path().resource_dir().ok();

    tokio::task::spawn_blocking(move || -> Result<String, AppError> {
        use std::io::{Cursor, Seek, SeekFrom};

        let thumb_err = |msg: String| AppError::Thumbnail { message: msg };

        let lower   = path.to_lowercase();
        let is_heic = lower.ends_with(".heic") || lower.ends_with(".heif") || lower.ends_with(".avif");

        let bytes = if is_heic {
            let (tmp, _, _) = crate::heic::heic_to_temp_jpeg(
                std::path::Path::new(&path),
                resource_dir.as_deref(),
                None, // full resolution for thumbnail/viewer
            ).ok_or_else(|| thumb_err("HEIC converter not available".to_string()))?;
            let b = std::fs::read(&tmp).map_err(|e| thumb_err(e.to_string()))?;
            let _ = std::fs::remove_file(&tmp);
            b
        } else {
            std::fs::read(&path).map_err(|e| thumb_err(e.to_string()))?
        };

        let img = image::load_from_memory(&bytes).map_err(|e| thumb_err(e.to_string()))?;
        let img = if !is_heic { apply_exif_orientation(&bytes, img) } else { img };
        // Normalise to 8-bit RGB — prevents JPEG encoder failures on HDR/wide-gamut
        // or CMYK output from ImageMagick (mirrors the same fix in get_thumbnail).
        let img = image::DynamicImage::ImageRgb8(img.into_rgb8());

        let mut buf = Cursor::new(Vec::<u8>::new());
        img.write_to(&mut buf, image::ImageFormat::Jpeg).map_err(|e| thumb_err(e.to_string()))?;
        buf.seek(SeekFrom::Start(0)).map_err(|e| thumb_err(e.to_string()))?;

        Ok(to_base64_data_uri(&buf.into_inner(), "image/jpeg"))
    })
    .await?
}

fn apply_exif_orientation(bytes: &[u8], img: image::DynamicImage) -> image::DynamicImage {
    let orientation = read_jpeg_orientation(bytes).unwrap_or(1);
    match orientation {
        2 => img.fliph(),
        3 => img.rotate180(),
        4 => img.flipv(),
        5 => img.rotate90().fliph(),
        6 => img.rotate90(),
        7 => img.rotate270().fliph(),
        8 => img.rotate270(),
        _ => img,
    }
}

/// Minimal inline JPEG EXIF orientation reader.
/// Avoids a subprocess call per thumbnail by parsing the JPEG APP1 segment
/// directly from the in-memory bytes already loaded for the image decode.
fn read_jpeg_orientation(bytes: &[u8]) -> Option<u32> {
    if bytes.len() < 4 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return None;
    }
    let mut pos = 2usize;
    while pos + 4 <= bytes.len() {
        if bytes[pos] != 0xFF {
            break;
        }
        let marker = bytes[pos + 1];
        // Stand-alone markers have no length field
        if matches!(marker, 0xD8 | 0xD9 | 0xD0..=0xD7) {
            pos += 2;
            continue;
        }
        if pos + 4 > bytes.len() {
            break;
        }
        let seg_len = u16::from_be_bytes([bytes[pos + 2], bytes[pos + 3]]) as usize;
        if seg_len < 2 {
            break;
        }
        let seg_end = pos + 2 + seg_len;
        // APP1 with EXIF header
        if marker == 0xE1 && seg_len >= 8 {
            let data_start = pos + 4;
            if data_start + 6 <= bytes.len()
                && &bytes[data_start..data_start + 6] == b"Exif\0\0"
            {
                let tiff = &bytes[data_start + 6..];
                return parse_tiff_orientation(tiff);
            }
        }
        // Stop at SOS — scan data follows, no more APPn segments
        if marker == 0xDA {
            break;
        }
        pos = seg_end;
    }
    None
}

fn parse_tiff_orientation(tiff: &[u8]) -> Option<u32> {
    if tiff.len() < 8 {
        return None;
    }
    let le = &tiff[0..2] == b"II";
    let read_u16 = |pos: usize| -> Option<u16> {
        let b: [u8; 2] = tiff.get(pos..pos + 2)?.try_into().ok()?;
        Some(if le { u16::from_le_bytes(b) } else { u16::from_be_bytes(b) })
    };
    let read_u32 = |pos: usize| -> Option<u32> {
        let b: [u8; 4] = tiff.get(pos..pos + 4)?.try_into().ok()?;
        Some(if le { u32::from_le_bytes(b) } else { u32::from_be_bytes(b) })
    };
    if read_u16(2)? != 0x002A {
        return None; // not a TIFF
    }
    let ifd0 = read_u32(4)? as usize;
    let count = read_u16(ifd0)? as usize;
    for i in 0..count {
        let e = ifd0 + 2 + i * 12;
        if e + 12 > tiff.len() {
            break;
        }
        if read_u16(e)? == 0x0112 {
            // Orientation tag, type SHORT (3)
            if read_u16(e + 2)? == 3 {
                return read_u16(e + 8).map(|v| v as u32);
            }
        }
    }
    None
}
