use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use chrono::NaiveDateTime;
use serde::Serialize;

use crate::config::{Mode, Profile};
use crate::logs;
use crate::media::{extract_datetime, is_media_file, is_video_file};
use crate::storage;
use crate::transcribe::extract_audio;

#[derive(Debug, Serialize)]
pub struct InboxResult {
    pub recording: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video: Option<PathBuf>,
    pub source_action: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InboxPlan {
    pub recording: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video: Option<PathBuf>,
}

pub fn plan(source: &Path, name: &str, keep_video: bool, profile: &Profile) -> Result<InboxPlan> {
    validate_source(source)?;
    let clean_name = sanitize_call_name(name);
    let date_format = profile.date_format.as_deref().unwrap_or("%Y-%m-%d %H-%M");
    let datetime = extract_datetime(source)?;
    let base_name = build_base_name(&datetime, date_format, &clean_name);
    let extension = source
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("m4a")
        .to_ascii_lowercase();
    let video = is_video_file(source);
    let (recording, stored_video) = destination_paths(
        &profile.calls_dir,
        &base_name,
        &extension,
        profile.mode.unwrap_or_default(),
        video,
        keep_video,
    );
    Ok(InboxPlan {
        recording,
        video: stored_video,
    })
}

fn destination_paths(
    calls_dir: &Path,
    base_name: &str,
    extension: &str,
    mode: Mode,
    source_is_video: bool,
    keep_video: bool,
) -> (PathBuf, Option<PathBuf>) {
    match mode {
        Mode::Folder => {
            let folder = calls_dir.join(base_name);
            (
                folder.join(if source_is_video {
                    "record.m4a".to_string()
                } else {
                    format!("record.{extension}")
                }),
                (source_is_video && keep_video).then(|| folder.join(format!("video.{extension}"))),
            )
        }
        Mode::File => (
            calls_dir.join(if source_is_video {
                format!("{base_name}.record.m4a")
            } else {
                format!("{base_name}.record.{extension}")
            }),
            (source_is_video && keep_video)
                .then(|| calls_dir.join(format!("{base_name}.video.{extension}"))),
        ),
    }
}

pub fn run(
    source: &Path,
    name: &str,
    move_source: bool,
    keep_video: bool,
    profile: &Profile,
) -> Result<InboxResult> {
    validate_source(source)?;
    fs::create_dir_all(&profile.calls_dir)
        .with_context(|| format!("Failed to create {}", profile.calls_dir.display()))?;

    let source_is_video = is_video_file(source);
    let planned = plan(source, name, keep_video, profile)?;
    let recording = planned.recording;
    let video = planned.video;
    match profile.mode.unwrap_or_default() {
        Mode::Folder => {
            let folder = recording
                .parent()
                .context("Folder-mode recording has no parent")?;
            if folder.exists() {
                bail!("Call target already exists: {}", folder.display());
            }
            fs::create_dir(folder)
                .with_context(|| format!("Failed to create {}", folder.display()))?;
        }
        Mode::File => {
            if recording.exists() || video.as_ref().is_some_and(|path| path.exists()) {
                bail!("Call target already exists: {}", recording.display());
            }
        }
    }

    let publication = (|| {
        if source_is_video {
            let temp = storage::temporary_sibling(&recording)?;
            let result =
                extract_audio(source, &temp).and_then(|_| storage::publish_temp(&temp, &recording));
            if result.is_err() {
                let _ = fs::remove_file(&temp);
            }
            result?;
            verify_nonempty(&recording)?;
            if let Some(video) = &video {
                storage::atomic_copy(source, video)?;
                verify_copy(source, video)?;
            }
        } else {
            storage::atomic_copy(source, &recording)?;
            verify_copy(source, &recording)?;
        }
        Ok::<_, anyhow::Error>(())
    })();
    if let Err(error) = publication {
        let _ = fs::remove_file(&recording);
        if let Some(video) = &video {
            let _ = fs::remove_file(video);
        }
        if matches!(profile.mode.unwrap_or_default(), Mode::Folder)
            && let Some(folder) = recording.parent()
        {
            let _ = fs::remove_dir(folder);
        }
        return Err(error);
    }

    if move_source {
        fs::remove_file(source)
            .with_context(|| format!("Failed to remove source {}", source.display()))?;
    }

    logs::event(&format!(
        "inbox_done source=\"{}\" recording=\"{}\" video={} action={}",
        source.display(),
        recording.display(),
        video.is_some(),
        if move_source { "move" } else { "copy" }
    ));
    Ok(InboxResult {
        recording,
        video,
        source_action: if move_source { "move" } else { "copy" }.to_string(),
    })
}

fn validate_source(path: &Path) -> Result<()> {
    if !path.exists() {
        bail!("Recording does not exist: {}", path.display());
    }
    if !path.is_file() {
        bail!("Recording is not a file: {}", path.display());
    }
    if !is_media_file(path) {
        bail!("Unsupported recording type: {}", path.display());
    }
    Ok(())
}

fn verify_copy(source: &Path, target: &Path) -> Result<()> {
    let source_size = fs::metadata(source)?.len();
    let target_size = fs::metadata(target)?.len();
    if source_size == 0 || source_size != target_size {
        bail!("Copy verification failed: source={source_size} bytes, target={target_size} bytes");
    }
    Ok(())
}

fn verify_nonempty(path: &Path) -> Result<()> {
    let size = fs::metadata(path)?.len();
    if size == 0 {
        bail!("Generated recording is empty: {}", path.display());
    }
    Ok(())
}

pub fn sanitize_call_name(value: &str) -> String {
    let mut clean = String::new();
    let mut previous_space = false;
    for character in value.trim().chars() {
        let replacement =
            if character == '/' || character == '\\' || character == '\0' || character.is_control()
            {
                '-'
            } else {
                character
            };
        if replacement.is_whitespace() {
            if !previous_space {
                clean.push(' ');
            }
            previous_space = true;
        } else {
            clean.push(replacement);
            previous_space = false;
        }
    }
    let clean = clean.trim_start_matches('.').trim();
    let clean: String = clean.chars().take(120).collect();
    if clean.is_empty() {
        "recording".to_string()
    } else {
        clean
    }
}

fn build_base_name(datetime: &NaiveDateTime, date_format: &str, name: &str) -> String {
    if date_format.is_empty() {
        name.to_string()
    } else {
        format!("{} {}", datetime.format(date_format), name)
            .trim()
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn sanitizes_call_names_without_losing_unicode() {
        assert_eq!(
            sanitize_call_name(" ..Привет /  call\\x "),
            "Привет - call-x"
        );
        assert_eq!(sanitize_call_name("..."), "recording");
    }

    #[test]
    fn builds_dated_and_plain_names() {
        let dt = NaiveDate::from_ymd_opt(2026, 8, 7)
            .unwrap()
            .and_hms_opt(17, 30, 46)
            .unwrap();
        assert_eq!(
            build_base_name(&dt, "%Y-%m-%d %H-%M", "Session"),
            "2026-08-07 17-30 Session"
        );
        assert_eq!(build_base_name(&dt, "", "Session"), "Session");
    }

    #[test]
    fn video_uses_record_m4a_without_storing_video_by_default() {
        let (recording, video) = destination_paths(
            Path::new("/calls"),
            "Call",
            "mov",
            Mode::Folder,
            true,
            false,
        );

        assert_eq!(recording, Path::new("/calls/Call/record.m4a"));
        assert_eq!(video, None);
    }

    #[test]
    fn retained_video_is_a_secondary_artifact() {
        let (recording, video) =
            destination_paths(Path::new("/calls"), "Call", "mov", Mode::File, true, true);

        assert_eq!(recording, Path::new("/calls/Call.record.m4a"));
        assert_eq!(video.as_deref(), Some(Path::new("/calls/Call.video.mov")));
    }

    #[test]
    fn audio_input_keeps_its_original_extension() {
        let (recording, video) = destination_paths(
            Path::new("/calls"),
            "Call",
            "wav",
            Mode::Folder,
            false,
            true,
        );

        assert_eq!(recording, Path::new("/calls/Call/record.wav"));
        assert_eq!(video, None);
    }
}
