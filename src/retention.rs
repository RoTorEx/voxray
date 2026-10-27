//! Explicit media removal requires a proof tied to the exact transcription input.
use crate::{call::CallPaths, storage};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Serialize, Deserialize)]
struct Proof {
    recording_md5: String,
    transcript_md5: String,
}

pub fn checksum(path: &Path) -> Result<String> {
    let output = Command::new("/sbin/md5")
        .arg("-q")
        .arg(path)
        .output()
        .context("Failed to checksum artifact")?;
    let digest = String::from_utf8(output.stdout)?.trim().to_owned();
    if !output.status.success()
        || digest.len() != 32
        || !digest.bytes().all(|b| b.is_ascii_hexdigit())
    {
        bail!("Could not checksum {}", path.display());
    }
    Ok(digest)
}

fn proof_path(paths: &CallPaths) -> PathBuf {
    paths.transcript.with_extension("proof.json")
}

pub fn prove(recording: &Path, paths: &CallPaths) -> Result<()> {
    validate_text(&paths.transcript)?;
    let proof = Proof {
        recording_md5: checksum(recording)?,
        transcript_md5: checksum(&paths.transcript)?,
    };
    storage::atomic_write(&proof_path(paths), &serde_json::to_vec(&proof)?)
}

pub fn validate_text(path: &Path) -> Result<()> {
    let text = fs::read_to_string(path).context("Transcript must be saved UTF-8 text")?;
    if text.trim().is_empty() {
        bail!("Transcript is empty; media is preserved");
    }
    File::open(path)?.sync_all()?;
    File::open(path.parent().context("Missing transcript directory")?)?.sync_all()?;
    Ok(())
}

pub fn verify(recording: &Path, paths: &CallPaths) -> Result<()> {
    validate_text(&paths.transcript)?;
    let proof: Proof = serde_json::from_slice(&fs::read(proof_path(paths)).context(
        "Transcript has no verification proof; transcribe again with --force before removing media",
    )?)?;
    if proof.transcript_md5 != checksum(&paths.transcript)?
        || (recording.exists() && proof.recording_md5 != checksum(recording)?)
    {
        bail!("Recording or transcript changed since transcription; media is preserved");
    }
    Ok(())
}

/// Only the supplied recording and recognized call video siblings can be removed.
pub fn remove_media(recording: &Path, paths: &CallPaths) -> Result<()> {
    verify(recording, paths)?;
    for path in media_paths(recording, paths)? {
        fs::remove_file(path)?;
    }
    File::open(&paths.root)?.sync_all()?;
    Ok(())
}

pub fn media_paths(recording: &Path, paths: &CallPaths) -> Result<Vec<PathBuf>> {
    let mut result = Vec::new();
    if recording.exists() {
        result.push(recording.to_path_buf());
    }
    let prefix = match paths.mode {
        crate::config::Mode::Folder => "video.".to_owned(),
        crate::config::Mode::File => format!("{}.video.", paths.name),
    };
    for entry in fs::read_dir(&paths.root)? {
        let path = entry?.path();
        if path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with(&prefix))
            && crate::media::is_video_file(&path)
        {
            result.push(path);
        }
    }
    for path in &result {
        if !fs::symlink_metadata(path)?.file_type().is_file() {
            bail!("Refusing non-regular media {}", path.display());
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn removal_requires_unchanged_recording_and_transcript() {
        let root = std::env::temp_dir().join(format!("voxray-retention-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let recording = root.join("record.wav");
        let paths = CallPaths::from_recording(&recording).unwrap();
        fs::write(&recording, b"samples").unwrap();
        fs::write(&paths.transcript, "Speaker 1: hello").unwrap();
        assert!(remove_media(&recording, &paths).is_err());
        prove(&recording, &paths).unwrap();
        fs::write(&paths.transcript, "changed").unwrap();
        assert!(remove_media(&recording, &paths).is_err());
        assert!(recording.exists());
        fs::write(&paths.transcript, "Speaker 1: hello").unwrap();
        fs::write(root.join("video.mov"), b"video").unwrap();
        fs::write(root.join("unrelated.mov"), b"unrelated").unwrap();
        remove_media(&recording, &paths).unwrap();
        assert!(!recording.exists());
        assert!(!root.join("video.mov").exists());
        assert!(root.join("unrelated.mov").exists());
        remove_media(&recording, &paths).unwrap();
        fs::remove_dir_all(root).unwrap();
    }
}
