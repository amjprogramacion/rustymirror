use std::collections::HashSet;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub static IMAGE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "webp", "bmp", "gif", "tiff", "tif", "heic", "heif", "avif",
];
pub static VIDEO_EXTENSIONS: &[&str] = &["mp4", "mov", "avi", "mpg", "mpeg", "mkv"];
/// Formats that cannot be decoded by the `image` crate and require ImageMagick conversion.
pub(super) static MAGICK_EXTENSIONS: &[&str] = &["heic", "heif", "avif"];

pub(super) fn is_image(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str())
        .map(|e| IMAGE_EXTENSIONS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

fn is_video(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str())
        .map(|e| VIDEO_EXTENSIONS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

pub(super) fn is_heic(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str())
        .map(|e| MAGICK_EXTENSIONS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

fn collect_paths(directories: &[PathBuf], filter: impl Fn(&std::path::Path) -> bool) -> Vec<PathBuf> {
    let single_pass = || -> HashSet<PathBuf> {
        directories.iter().flat_map(|dir| {
            WalkDir::new(dir).follow_links(false).into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file())
                .map(|e| e.into_path())
                .filter(|p| filter(p))
        }).collect()
    };

    let first  = single_pass();
    let second = single_pass();

    let mut all: Vec<PathBuf> = first.into_iter().chain(second)
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();

    all.sort();
    all
}

pub fn collect_images(directories: &[PathBuf]) -> Vec<PathBuf> {
    collect_paths(directories, is_image)
}

pub fn collect_media(directories: &[PathBuf]) -> Vec<PathBuf> {
    collect_paths(directories, |p| is_image(p) || is_video(p))
}

