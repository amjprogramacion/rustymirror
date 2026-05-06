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
    let max_passes = if directories.iter().any(|dir| is_probably_network_path(dir)) {
        3
    } else {
        1
    };

    let mut all = HashSet::new();
    for pass in 0..max_passes {
        let before = all.len();
        all.extend(single_pass(directories, &filter));

        if pass > 0 && all.len() == before {
            break;
        }
    }

    let mut all: Vec<PathBuf> = all.into_iter().collect();

    all.sort();
    all
}

fn single_pass(
    directories: &[PathBuf],
    filter: &impl Fn(&std::path::Path) -> bool,
) -> HashSet<PathBuf> {
    directories.iter().flat_map(|dir| {
        WalkDir::new(dir)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .map(|e| e.into_path())
            .filter(|p| filter(p))
    }).collect()
}

fn is_probably_network_path(path: &Path) -> bool {
    #[cfg(target_os = "windows")]
    {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;

        let path_str = path.to_string_lossy();
        if path_str.starts_with("\\\\") || path_str.starts_with("//") {
            return true;
        }

        if path_str.len() >= 2 && path_str.as_bytes()[1] == b':' {
            let root = format!("{}:\\", &path_str[..1].to_uppercase());
            let wide: Vec<u16> = OsStr::new(&root)
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            let drive_type =
                unsafe { windows_sys::Win32::Storage::FileSystem::GetDriveTypeW(wide.as_ptr()) };
            return drive_type == 4;
        }

        false
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = path;
        false
    }
}

pub fn collect_images(directories: &[PathBuf]) -> Vec<PathBuf> {
    collect_paths(directories, is_image)
}

pub fn collect_media(directories: &[PathBuf]) -> Vec<PathBuf> {
    collect_paths(directories, |p| is_image(p) || is_video(p))
}
