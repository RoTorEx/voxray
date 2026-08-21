use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use chrono::NaiveDateTime;
use serde::Serialize;

use crate::config::{Mode, Profile};
use crate::inbox::{extract_datetime, is_media_file, is_video_file};
use crate::logs;
use crate::storage;
use crate::transcribe::extract_audio;

#[derive(Debug, Serialize)]
pub struct ListenResult {
    pub recording: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub derived_audio: Option<PathBuf>,
    pub source_action: String,
}

impl ListenResult {
    pub fn transcription_input(&self) -> &Path {
        self.derived_audio.as_deref().unwrap_or(&self.recording)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ListenPlan {
    pub recording: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub derived_audio: Option<PathBuf>,
}

pub fn plan(source: &Path, name: &str, profile: &Profile) -> Result<ListenPlan> {
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
    let (recording, derived_audio) = match profile.mode.unwrap_or_default() {
        Mode::Folder => {
            let folder = profile.calls_dir.join(&base_name);
            (
                folder.join(format!("record.{extension}")),
                video.then(|| folder.join("audio.m4a")),
            )
        }
        Mode::File => (
            profile
                .calls_dir
                .join(format!("{base_name}.record.{extension}")),
            video.then(|| profile.calls_dir.join(format!("{base_name}.audio.m4a"))),
        ),
    };
    Ok(ListenPlan {
        recording,
        derived_audio,
    })
}

pub fn run(
    source: &Path,
    name: &str,
    move_source: bool,
    profile: &Profile,
) -> Result<ListenResult> {
    validate_source(source)?;
    fs::create_dir_all(&profile.calls_dir)
        .with_context(|| format!("Failed to create {}", profile.calls_dir.display()))?;

    let planned = plan(source, name, profile)?;
    let recording = planned.recording;
    let derived_audio = planned.derived_audio;
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
            if recording.exists() || derived_audio.as_ref().is_some_and(|path| path.exists()) {
                bail!("Call target already exists: {}", recording.display());
            }
        }
    }

    let publication = (|| {
        storage::atomic_copy(source, &recording)?;
        if let Some(audio) = &derived_audio {
            let temp = storage::temporary_sibling(audio)?;
            let result =
                extract_audio(source, &temp).and_then(|_| storage::publish_temp(&temp, audio));
            if result.is_err() {
                let _ = fs::remove_file(&temp);
            }
            result?;
        }
        Ok::<_, anyhow::Error>(())
    })();
    if let Err(error) = publication {
        let _ = fs::remove_file(&recording);
        if let Some(audio) = &derived_audio {
            let _ = fs::remove_file(audio);
        }
        if matches!(profile.mode.unwrap_or_default(), Mode::Folder)
            && let Some(folder) = recording.parent()
        {
            let _ = fs::remove_dir(folder);
        }
        return Err(error);
    }

    verify_copy(source, &recording)?;
    if move_source {
        fs::remove_file(source)
            .with_context(|| format!("Failed to remove source {}", source.display()))?;
    }

    logs::event(&format!(
        "listen_done source=\"{}\" recording=\"{}\" action={}",
        source.display(),
        recording.display(),
        if move_source { "move" } else { "copy" }
    ));
    Ok(ListenResult {
        recording,
        derived_audio,
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
    fn prefers_derived_audio_for_transcription() {
        let result = ListenResult {
            recording: PathBuf::from("call.record.mov"),
            derived_audio: Some(PathBuf::from("call.audio.m4a")),
            source_action: "copy".to_string(),
        };
        assert_eq!(result.transcription_input(), Path::new("call.audio.m4a"));
    }

    #[test]
    fn uses_recording_when_no_audio_was_derived() {
        let result = ListenResult {
            recording: PathBuf::from("call.record.m4a"),
            derived_audio: None,
            source_action: "copy".to_string(),
        };
        assert_eq!(result.transcription_input(), Path::new("call.record.m4a"));
    }
}
