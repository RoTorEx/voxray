//! Media-file recognition and recording timestamp extraction.

use std::fs;
use std::path::Path;

use anyhow::Context;
use chrono::NaiveDateTime;

const VIDEO_EXTENSIONS: &[&str] = &["mov", "mp4", "mkv", "avi"];
const AUDIO_EXTENSIONS: &[&str] = &["m4a", "mp3", "wav", "aac"];

fn has_extension(path: &Path, extensions: &[&str]) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| {
            let lower = e.to_lowercase();
            extensions.contains(&lower.as_str())
        })
        .unwrap_or(false)
}

pub fn is_video_file(path: &Path) -> bool {
    has_extension(path, VIDEO_EXTENSIONS)
}

pub fn is_audio_file(path: &Path) -> bool {
    has_extension(path, AUDIO_EXTENSIONS)
}

pub fn is_media_file(path: &Path) -> bool {
    is_video_file(path) || is_audio_file(path)
}

pub(crate) fn extract_datetime(path: &Path) -> anyhow::Result<NaiveDateTime> {
    if let Some(dt) = parse_filename_datetime(path) {
        return Ok(dt);
    }

    let modified = fs::metadata(path)
        .and_then(|m| m.modified())
        .with_context(|| format!("Failed to read metadata for {}", path.display()))?;

    let dt: chrono::DateTime<chrono::Local> = modified.into();
    Ok(dt.naive_local())
}

fn parse_filename_datetime(path: &Path) -> Option<NaiveDateTime> {
    let stem = path.file_stem()?.to_str()?;
    let suffix = stem.rsplit('_').next()?;
    NaiveDateTime::parse_from_str(suffix, "%Y-%m-%d-%H.%M.%S").ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveDate, Timelike};

    #[test]
    fn detects_media_files() {
        assert!(is_media_file(Path::new("call.mov")));
        assert!(is_media_file(Path::new("call.MOV")));
        assert!(is_media_file(Path::new("call.mp4")));
        assert!(is_media_file(Path::new("call.m4a")));
        assert!(!is_media_file(Path::new("call.txt")));
        assert!(!is_media_file(Path::new("call")));
    }

    #[test]
    fn parses_bettercapture_filename_datetime() {
        let path = Path::new("BetterCapture_2026-08-07-17.30.46.mov");
        let dt = parse_filename_datetime(path).unwrap();
        assert_eq!(dt.date(), NaiveDate::from_ymd_opt(2026, 8, 7).unwrap());
        assert_eq!(dt.hour(), 17);
        assert_eq!(dt.minute(), 30);
        assert_eq!(dt.second(), 46);
    }
}
