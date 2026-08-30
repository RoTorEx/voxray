use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::analysis::AnalysisDocument;
use crate::config::{Mode, REPORT_LANGUAGE};
use crate::transcript::{Metrics, Transcript};

#[derive(Debug, Clone)]
pub struct CallPaths {
    pub root: PathBuf,
    pub manifest: PathBuf,
    pub transcript: PathBuf,
    pub feedback: PathBuf,
    pub mode: Mode,
    pub name: String,
}

impl CallPaths {
    pub fn from_recording(recording: &Path) -> Result<Self> {
        let parent = recording
            .parent()
            .context("Recording path has no parent directory")?;
        let file_name = recording
            .file_name()
            .and_then(|value| value.to_str())
            .context("Recording path has no UTF-8 file name")?;
        let folder_layout = file_name.starts_with("record.")
            || (file_name == "audio.m4a" && contains_folder_recording(parent));
        if folder_layout {
            let name = parent
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("call")
                .to_string();
            return Ok(Self {
                root: parent.to_path_buf(),
                manifest: parent.join("call.json"),
                transcript: parent.join("transcript.txt"),
                feedback: parent.join("feedback.txt"),
                mode: Mode::Folder,
                name,
            });
        }

        let stem = recording
            .file_stem()
            .and_then(|value| value.to_str())
            .context("Recording path has no UTF-8 stem")?;
        let name = stem.strip_suffix(".record").unwrap_or(stem).to_string();
        Ok(Self::file(parent, name))
    }

    pub fn from_transcript(transcript: &Path) -> Result<Self> {
        let parent = transcript
            .parent()
            .context("Transcript path has no parent directory")?;
        let file_name = transcript
            .file_name()
            .and_then(|value| value.to_str())
            .context("Transcript path has no UTF-8 file name")?;
        if file_name == "transcript.txt" {
            let name = parent
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("call")
                .to_string();
            return Ok(Self {
                root: parent.to_path_buf(),
                manifest: parent.join("call.json"),
                transcript: transcript.to_path_buf(),
                feedback: parent.join("feedback.txt"),
                mode: Mode::Folder,
                name,
            });
        }
        let stem = transcript
            .file_stem()
            .and_then(|value| value.to_str())
            .context("Transcript path has no UTF-8 stem")?;
        let name = stem.strip_suffix(".transcript").unwrap_or(stem).to_string();
        let mut paths = Self::file(parent, name);
        paths.transcript = transcript.to_path_buf();
        Ok(paths)
    }

    fn file(parent: &Path, name: String) -> Self {
        Self {
            root: parent.to_path_buf(),
            manifest: parent.join(format!("{name}.call.json")),
            transcript: parent.join(format!("{name}.transcript.txt")),
            feedback: parent.join(format!("{name}.feedback.txt")),
            mode: Mode::File,
            name,
        }
    }
}

fn contains_folder_recording(parent: &Path) -> bool {
    fs::read_dir(parent).is_ok_and(|entries| {
        entries.filter_map(Result::ok).any(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with("record."))
        })
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallManifest {
    pub schema_version: u32,
    pub call_id: String,
    #[serde(default)]
    pub name: String,
    pub profile: String,
    #[serde(default)]
    pub mode: Mode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    pub report_language: String,
    pub created_at: String,
    #[serde(default)]
    pub artifacts: BTreeMap<String, String>,
    #[serde(default)]
    pub speaker_mapping: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recording: Option<RecordingRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript: Option<Transcript>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metrics: Option<Metrics>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub analysis: Option<AnalysisDocument>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingRecord {
    pub path: String,
    pub media_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_video_path: Option<String>,
    pub size_bytes: u64,
    #[serde(alias = "ingested_at")]
    pub recorded_at: String,
}

impl CallManifest {
    pub fn new(paths: &CallPaths, profile_name: &str) -> Self {
        Self {
            schema_version: 4,
            call_id: format!(
                "{}-{}",
                chrono::Utc::now().timestamp_millis(),
                slug(&paths.name)
            ),
            name: paths.name.clone(),
            profile: profile_name.to_string(),
            mode: paths.mode,
            context: None,
            report_language: REPORT_LANGUAGE.to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            artifacts: BTreeMap::new(),
            speaker_mapping: BTreeMap::new(),
            recording: None,
            transcript: None,
            metrics: None,
            analysis: None,
        }
    }

    pub fn load(path: &Path) -> Result<Option<Self>> {
        if !path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        let manifest = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse {}", path.display()))?;
        Ok(Some(manifest))
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        serde_json::to_vec_pretty(self).context("Failed to serialize call manifest")
    }

    pub fn set_artifact(&mut self, name: &str, path: &Path, root: &Path) {
        self.artifacts.insert(
            name.to_string(),
            path.strip_prefix(root)
                .unwrap_or(path)
                .to_string_lossy()
                .into_owned(),
        );
    }
}

pub fn remove_legacy_files(paths: &CallPaths) -> Result<()> {
    let legacy = if paths.mode == Mode::Folder {
        vec![
            paths.root.join("analysis.json"),
            paths.root.join("transcript.json"),
            paths.root.join("transcript.raw.json"),
            paths.root.join("feedback.md"),
            paths.root.join("feedback_stat.json"),
            paths.root.join("feedback_sales.txt"),
            paths.root.join("feedback_english.txt"),
            paths.root.join("feedback_common.txt"),
        ]
    } else {
        vec![
            paths.root.join(format!("{}.analysis.json", paths.name)),
            paths.root.join(format!("{}.transcript.json", paths.name)),
            paths
                .root
                .join(format!("{}.transcript.raw.json", paths.name)),
            paths.root.join(format!("{}.feedback.md", paths.name)),
            paths
                .root
                .join(format!("{}.feedback_stat.json", paths.name)),
        ]
    };
    for path in legacy {
        if path.exists() {
            fs::remove_file(&path)
                .with_context(|| format!("Failed to remove legacy {}", path.display()))?;
        }
    }
    Ok(())
}

fn slug(value: &str) -> String {
    let value: String = value
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();
    if value.is_empty() {
        "call".to_string()
    } else {
        value
    }
}

pub fn validate_file(path: &Path, label: &str) -> Result<()> {
    if !path.exists() {
        bail!("{label} does not exist: {}", path.display());
    }
    if !path.is_file() {
        bail!("{label} is not a file: {}", path.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_folder_recording_paths() {
        let paths = CallPaths::from_recording(Path::new("/tmp/Call/record.m4a")).unwrap();
        assert_eq!(paths.transcript, Path::new("/tmp/Call/transcript.txt"));
        assert_eq!(paths.feedback, Path::new("/tmp/Call/feedback.txt"));
        assert_eq!(paths.manifest, Path::new("/tmp/Call/call.json"));
    }

    #[test]
    fn resolves_file_recording_paths() {
        let paths = CallPaths::from_recording(Path::new("/tmp/Call.record.m4a")).unwrap();
        assert_eq!(paths.transcript, Path::new("/tmp/Call.transcript.txt"));
        assert_eq!(paths.feedback, Path::new("/tmp/Call.feedback.txt"));
        assert_eq!(paths.manifest, Path::new("/tmp/Call.call.json"));
    }

    #[test]
    fn resolves_plain_transcript_paths() {
        let paths = CallPaths::from_transcript(Path::new("/tmp/custom.txt")).unwrap();
        assert_eq!(paths.feedback, Path::new("/tmp/custom.feedback.txt"));
    }
}
