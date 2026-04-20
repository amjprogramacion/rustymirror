use std::collections::HashSet;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub static IMAGE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "webp", "bmp", "gif", "tiff", "tif", "heic", "heif", "avif",
];
/// Formats that cannot be decoded by the `image` crate and require ImageMagick conversion.
pub(super) static MAGICK_EXTENSIONS: &[&str] = &["heic", "heif", "avif"];

pub(super) fn is_image(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str())
        .map(|e| IMAGE_EXTENSIONS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

pub(super) fn is_heic(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str())
        .map(|e| MAGICK_EXTENSIONS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

pub fn collect_images(directories: &[PathBuf]) -> Vec<PathBuf> {
    let single_pass = || -> HashSet<PathBuf> {
        directories.iter().flat_map(|dir| {
            WalkDir::new(dir).follow_links(false).into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file())
                .map(|e| e.into_path())
                .filter(|p| is_image(p))
        }).collect()
    };

    // Two passes: on SMB/NAS drives the first WalkDir traversal warms the
    // server-side directory cache, so a second pass consistently finds all
    // files that the cold-cache first pass may have missed.  Always take
    // the union so neither pass can silently drop files.
    let first  = single_pass();
    let second = single_pass();

    let mut all: Vec<PathBuf> = first.into_iter().chain(second)
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();

    all.sort();
    all
}
