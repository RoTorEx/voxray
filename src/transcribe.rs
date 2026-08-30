use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::call::{self, CallPaths};
use crate::config::TRANSCRIPTION_LANGUAGE;
use crate::{logs, storage, transcript};

pub fn extract_audio(video_path: &Path, output_audio: &Path) -> Result<()> {
    let status = Command::new("ffmpeg")
        .args(audio_extraction_args(video_path, output_audio))
        .stdout(Stdio::null())
        .status()
        .with_context(|| "Failed to run ffmpeg; install it and ensure it is on PATH")?;
    if !status.success() {
        bail!("ffmpeg audio extraction failed with status {status}");
    }
    Ok(())
}

fn audio_extraction_args(video_path: &Path, output_audio: &Path) -> Vec<OsString> {
    [
        OsString::from("-y"),
        OsString::from("-i"),
        video_path.as_os_str().to_owned(),
        OsString::from("-vn"),
        OsString::from("-map"),
        OsString::from("0:a"),
        OsString::from("-c:a"),
        OsString::from("aac"),
        OsString::from("-b:a"),
        OsString::from("128k"),
        OsString::from("-f"),
        OsString::from("ipod"),
        output_audio.as_os_str().to_owned(),
    ]
    .into()
}

#[derive(Debug, Serialize)]
pub struct TranscribeResult {
    pub result: String,
    pub transcript: PathBuf,
}

pub struct TranscribeOptions<'a> {
    pub model: &'a str,
    pub force: bool,
}

pub fn run(
    recording: &Path,
    transcription_input: &Path,
    options: TranscribeOptions<'_>,
) -> Result<TranscribeResult> {
    call::validate_file(recording, "Recording")?;
    call::validate_file(transcription_input, "Transcription input")?;
    if !crate::media::is_media_file(transcription_input) {
        bail!(
            "Unsupported transcription input type: {}",
            transcription_input.display()
        );
    }
    if options.model.trim().is_empty() || !options.model.contains(':') {
        bail!("Transcription model must use engine:model-id format");
    }

    let paths = CallPaths::from_recording(recording)?;
    if paths.transcript.exists() && !options.force {
        return Ok(TranscribeResult {
            result: "already_exists".to_string(),
            transcript: paths.transcript,
        });
    }

    let raw_target = paths
        .root
        .join(format!(".voxray-mw-{}.tmp", std::process::id()));
    let started = Instant::now();
    let status = Command::new("mw")
        .arg("transcribe")
        .arg("--format")
        .arg("json")
        .arg("--speakers")
        .arg("--model")
        .arg(options.model)
        .arg("--language")
        .arg(TRANSCRIPTION_LANGUAGE)
        .arg("--output")
        .arg(&raw_target)
        .arg(transcription_input)
        .stdout(Stdio::null())
        .status()
        .with_context(
            || "Failed to run mw transcribe; install MacWhisper CLI and ensure mw is on PATH",
        )?;
    if !status.success() {
        let _ = std::fs::remove_file(&raw_target);
        bail!("MacWhisper transcription failed with status {status}");
    }

    let raw = std::fs::read(&raw_target)
        .with_context(|| format!("Failed to read {}", raw_target.display()))?;
    let _ = std::fs::remove_file(&raw_target);
    let value: serde_json::Value =
        serde_json::from_slice(&raw).context("MacWhisper returned invalid JSON")?;
    let canonical = transcript::Transcript::from_macwhisper_json(
        &value,
        options.model,
        TRANSCRIPTION_LANGUAGE,
        started.elapsed().as_millis(),
    )?;

    storage::atomic_write(&paths.transcript, canonical.render_text().as_bytes())?;
    logs::event(&format!(
        "transcribe_done recording=\"{}\" input=\"{}\" transcript=\"{}\"",
        recording.display(),
        transcription_input.display(),
        paths.transcript.display()
    ));
    Ok(TranscribeResult {
        result: "created".to_string(),
        transcript: paths.transcript,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tells_ffmpeg_to_write_m4a_when_temporary_path_ends_in_tmp() {
        let args = audio_extraction_args(
            Path::new("call.mov"),
            Path::new(".voxray-audio.m4a-123.tmp"),
        );
        let args: Vec<_> = args
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();

        assert_eq!(
            args,
            [
                "-y",
                "-i",
                "call.mov",
                "-vn",
                "-map",
                "0:a",
                "-c:a",
                "aac",
                "-b:a",
                "128k",
                "-f",
                "ipod",
                ".voxray-audio.m4a-123.tmp",
            ]
        );
    }
}
