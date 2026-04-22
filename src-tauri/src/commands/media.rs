use std::path::PathBuf;
use tauri::{AppHandle, Manager};

use super::AppError;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaFile {
    pub name:        String,
    pub path:        String,
    pub date_taken:  Option<String>,
    pub date_source: Option<String>,  // "exif" | "create"
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaCountResult {
    pub total:       usize,
    pub images:      usize,
    pub videos:      usize,
    pub image_exts:  std::collections::BTreeMap<String, usize>,
    pub video_exts:  std::collections::BTreeMap<String, usize>,
    pub files:       Vec<MediaFile>,
}

/// Counts all compatible media files in the given directories,
/// split by images and videos with per-extension breakdown and EXIF date.
#[tauri::command]
pub async fn count_media_files(paths: Vec<String>, config: crate::organizer::OrganizerConfig, app: AppHandle) -> Result<MediaCountResult, AppError> {
    use std::collections::{BTreeMap, HashMap, HashSet};

    const VIDEO_EXTS: &[&str] = &["mp4", "mov", "avi", "mpg", "mpeg", "mkv"];
    const CHUNK: usize = 500;

    let resource_dir = app.path().resource_dir().ok();

    tokio::task::spawn_blocking(move || {
        let directories: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();

        let files: HashSet<PathBuf> = directories.iter().flat_map(|dir| {
            walkdir::WalkDir::new(dir).follow_links(false).into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file())
                .map(|e| e.into_path())
                .filter(|p| {
                    let ext = p.extension().and_then(|e| e.to_str())
                        .map(|e| e.to_lowercase());
                    match ext.as_deref() {
                        Some(e) => crate::scanner::walk::IMAGE_EXTENSIONS.contains(&e)
                            || VIDEO_EXTS.contains(&e),
                        None => false,
                    }
                })
        }).collect();

        let mut image_exts: BTreeMap<String, usize> = BTreeMap::new();
        let mut video_exts: BTreeMap<String, usize> = BTreeMap::new();

        for p in &files {
            let ext = p.extension().and_then(|e| e.to_str())
                .map(|e| e.to_lowercase())
                .unwrap_or_default();
            if VIDEO_EXTS.contains(&ext.as_str()) {
                *video_exts.entry(ext).or_insert(0) += 1;
            } else {
                *image_exts.entry(ext).or_insert(0) += 1;
            }
        }

        // Read DateTimeOriginal + CreateDate for all files via ExifTool batch calls.
        let exiftool = resource_dir.as_deref().and_then(crate::exiftool::find_exiftool);
        let mut date_map: HashMap<String, (String, String)> = HashMap::new(); // path → (date, source)

        let mut sorted_paths: Vec<&PathBuf> = files.iter().collect();
        sorted_paths.sort();

        if let Some(ref et) = exiftool {
            for chunk in sorted_paths.chunks(CHUNK) {
                let owned: Vec<PathBuf> = chunk.iter().map(|p| (*p).clone()).collect();
                if let Ok(results) = crate::exiftool::batch_read_tags(et, &owned, &["-EXIF:DateTimeOriginal", "-QuickTime:CreateDate", "-File:FileModifyDate"]) {
                    for obj in results {
                        let src_path = obj.get("SourceFile")
                            .and_then(|v| v.as_str())
                            .map(|s| s.replace('\\', "/"));
                        let filename = src_path.as_deref()
                            .and_then(|p| p.rsplit('/').next())
                            .unwrap_or("");
                        let from_filename = crate::organizer::filename_date(filename);
                        let from_exif = obj.get("DateTimeOriginal")
                            .or_else(|| obj.get("CreateDate"))
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_owned());
                        let from_exif_source = if obj.get("DateTimeOriginal").and_then(|v| v.as_str()).is_some() {
                            "exif"
                        } else {
                            "create"
                        };
                        let from_modify = obj.get("FileModifyDate").and_then(|v| v.as_str()).map(|s| s.to_owned());

                        let entry = match config.date_priority {
                            crate::organizer::DatePriority::Filename => {
                                if let Some(d) = from_filename { Some((d, "filename".to_owned())) }
                                else if let Some(d) = from_exif { Some((d, from_exif_source.to_owned())) }
                                else if let Some(d) = from_modify { Some((d, "modify".to_owned())) }
                                else { None }
                            }
                            crate::organizer::DatePriority::Exif => {
                                if let Some(d) = from_exif { Some((d, from_exif_source.to_owned())) }
                                else if let Some(d) = from_modify { Some((d, "modify".to_owned())) }
                                else if let Some(d) = from_filename { Some((d, "filename".to_owned())) }
                                else { None }
                            }
                        };
                        if let (Some(src), Some(entry)) = (src_path, entry) {
                            date_map.insert(src, entry);
                        }
                    }
                }
            }
        }

        let images = image_exts.values().sum();
        let videos = video_exts.values().sum();

        let mut file_list: Vec<MediaFile> = files.iter().map(|p| {
            let norm = p.to_string_lossy().replace('\\', "/");
            let (date_taken, date_source) = match date_map.get(&norm) {
                Some((d, s)) => (Some(d.clone()), Some(s.clone())),
                None         => (None, None),
            };
            MediaFile {
                name: p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                path: p.to_string_lossy().into_owned(),
                date_taken,
                date_source,
            }
        }).collect();
        file_list.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

        Ok(MediaCountResult { total: files.len(), images, videos, image_exts, video_exts, files: file_list })
    })
    .await?
}
