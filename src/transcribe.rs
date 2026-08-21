use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::call::{self, CallManifest, CallPaths};
use crate::config::{Profile, REPORT_LANGUAGE};
use crate::{logs, storage, transcript};

pub fn extract_audio(video_path: &Path, output_audio: &Path) -> Result<()> {
    let status = Command::new("ffmpeg")
        .arg("-y")
        .arg("-i")
        .arg(video_path)
        .arg("-vn")
        .arg("-map")
        .arg("0:a")
        .arg("-c:a")
        .arg("aac")
        .arg("-b:a")
        .arg("128k")
        .arg(output_audio)
        .stdout(Stdio::null())
        .status()
        .with_context(|| "Failed to run ffmpeg; install it and ensure it is on PATH")?;
    if !status.success() {
        bail!("ffmpeg audio extraction failed with status {status}");
    }
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct TranscribeResult {
    pub result: String,
    pub transcript: PathBuf,
    pub call_json: PathBuf,
}

pub fn run(
    recording: &Path,
    transcription_input: &Path,
    profile_name: &str,
    profile: &Profile,
    model: &str,
    force: bool,
    interactive: bool,
) -> Result<TranscribeResult> {
    call::validate_file(recording, "Recording")?;
    call::validate_file(transcription_input, "Transcription input")?;
    if !crate::inbox::is_media_file(transcription_input) {
        bail!(
            "Unsupported transcription input type: {}",
            transcription_input.display()
        );
    }
    if model.trim().is_empty() || !model.contains(':') {
        bail!("Transcription model must use engine:model-id format");
    }

    let paths = CallPaths::from_recording(recording)?;
    if paths.transcript.exists() && !force {
        return Ok(TranscribeResult {
            result: "already_exists".to_string(),
            transcript: paths.transcript,
            call_json: paths.manifest,
        });
    }

    let mut manifest = CallManifest::load(&paths.manifest)?
        .unwrap_or_else(|| CallManifest::new(&paths, profile_name, profile));
    manifest.schema_version = 3;
    manifest.name = paths.name.clone();
    manifest.mode = paths.mode;
    manifest.profile = profile_name.to_string();
    manifest.call_type = profile.call_type.clone();
    manifest.subject_name = profile.subject_name.clone();
    manifest.subject_role = profile.subject_role.clone();
    manifest.call_goal = profile.call_goal.clone();
    manifest.source_language = profile.source_language.clone();
    manifest.report_language = REPORT_LANGUAGE.to_string();
    let original_video = (transcription_input != recording).then_some(recording);
    manifest.set_recording(transcription_input, original_video, &paths.root)?;

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
        .arg(model)
        .arg("--language")
        .arg(&profile.source_language)
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
        model,
        &profile.source_language,
        profile,
        started.elapsed().as_millis(),
        interactive,
    )?;

    manifest.speaker_mapping = canonical.speaker_mapping.clone();
    manifest.metrics = Some(canonical.metrics("target"));
    manifest.transcript = Some(canonical.clone());
    manifest.analysis = None;
    manifest.set_artifact("recording", recording, &paths.root);
    if transcription_input != recording {
        manifest.set_artifact("audio", transcription_input, &paths.root);
    }
    manifest.set_artifact("transcript", &paths.transcript, &paths.root);

    storage::atomic_write_pair(
        &paths.manifest,
        &manifest.to_bytes()?,
        &paths.transcript,
        canonical.render_text().as_bytes(),
    )?;
    call::remove_legacy_files(&paths)?;
    logs::event(&format!(
        "transcribe_done recording=\"{}\" input=\"{}\" transcript=\"{}\"",
        recording.display(),
        transcription_input.display(),
        paths.transcript.display()
    ));
    Ok(TranscribeResult {
        result: "created".to_string(),
        transcript: paths.transcript,
        call_json: paths.manifest,
    })
}
