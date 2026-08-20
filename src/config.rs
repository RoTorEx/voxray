use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

pub const REPORT_LANGUAGE: &str = "en";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_profile")]
    pub default: Profile,
    #[serde(default)]
    pub profiles: HashMap<String, Profile>,
    #[serde(default = "default_languages")]
    pub languages: Vec<String>,
    #[serde(default, alias = "feedback")]
    pub analysis: AnalysisConfig,
    #[serde(default)]
    pub transcription: TranscriptionConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisConfig {
    /// Model type+version passed to the Responses API
    #[serde(default = "default_feedback_model")]
    pub model: String,
    /// Optional reasoning effort (e.g. "low"); omitted from the request when None
    #[serde(default)]
    pub reasoning_effort: Option<String>,
    /// Responses API endpoint; override for OpenAI-compatible APIs
    #[serde(default = "default_feedback_api_url")]
    pub api_url: String,
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        AnalysisConfig {
            model: default_feedback_model(),
            reasoning_effort: Some("medium".to_string()),
            api_url: default_feedback_api_url(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionConfig {
    #[serde(default = "default_transcription_model")]
    pub model: String,
    #[serde(default = "default_true")]
    pub timestamps: bool,
    #[serde(default = "default_true")]
    pub speakers: bool,
}

impl Default for TranscriptionConfig {
    fn default() -> Self {
        Self {
            model: default_transcription_model(),
            timestamps: true,
            speakers: true,
        }
    }
}

fn default_transcription_model() -> String {
    "whisperkit:openai_whisper-large-v3".to_string()
}

fn default_true() -> bool {
    true
}

fn default_feedback_model() -> String {
    "gpt-5.6-terra".to_string()
}

fn default_feedback_api_url() -> String {
    "https://api.openai.com/v1/responses".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub inbox_dir: PathBuf,
    pub calls_dir: PathBuf,
    pub date_format: Option<String>,
    #[serde(default)]
    pub mode: Option<Mode>,
    /// Analysis modules enabled for this profile. `feedback` remains a read alias.
    #[serde(default, alias = "feedback")]
    pub modules: Vec<String>,
    #[serde(default = "default_call_type")]
    pub call_type: String,
    #[serde(default = "default_subject_name")]
    pub subject_name: String,
    #[serde(default = "default_subject_role")]
    pub subject_role: String,
    #[serde(default = "default_source_language")]
    pub source_language: String,
    #[serde(default)]
    pub call_goal: String,
    #[serde(default)]
    pub subject_speakers: Vec<String>,
}

fn default_call_type() -> String {
    "general".to_string()
}

fn default_subject_name() -> String {
    "Alex".to_string()
}

fn default_subject_role() -> String {
    "participant".to_string()
}

fn default_source_language() -> String {
    "auto".to_string()
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    #[default]
    Folder,
    File,
}

fn default_profile() -> Profile {
    let desktop = dirs::desktop_dir().unwrap_or_else(|| PathBuf::from("."));
    Profile {
        inbox_dir: desktop.clone(),
        calls_dir: desktop,
        date_format: None,
        mode: None,
        modules: Vec::new(),
        call_type: default_call_type(),
        subject_name: default_subject_name(),
        subject_role: default_subject_role(),
        source_language: default_source_language(),
        call_goal: String::new(),
        subject_speakers: Vec::new(),
    }
}

fn default_languages() -> Vec<String> {
    vec!["auto".to_string(), "en".to_string(), "ru".to_string()]
}

impl Config {
    pub fn path() -> Result<PathBuf> {
        let exe = std::env::current_exe().context("Failed to determine executable path")?;
        let dir = exe.parent().context("Executable has no parent directory")?;
        Ok(dir.join("config.toml"))
    }

    pub fn load() -> Result<Self> {
        let path = Self::path()?;
        if path.exists() {
            let content = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read config from {}", path.display()))?;
            let config: Config = toml::from_str(&content)
                .with_context(|| format!("Failed to parse config from {}", path.display()))?;
            Ok(config)
        } else {
            let config = Config {
                default: default_profile(),
                profiles: HashMap::new(),
                languages: default_languages(),
                analysis: AnalysisConfig::default(),
                transcription: TranscriptionConfig::default(),
            };
            config.save()?;
            Ok(config)
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::path()?;
        fs::create_dir_all(
            path.parent()
                .context("Config path has no parent directory")?,
        )
        .with_context(|| format!("Failed to create config directory {}", path.display()))?;
        let content = toml::to_string_pretty(self).context("Failed to serialize config")?;
        fs::write(&path, content)
            .with_context(|| format!("Failed to write config to {}", path.display()))?;
        Ok(())
    }

    pub fn profile(&self, name: Option<&str>) -> Result<&Profile> {
        match name {
            Some(n) => self
                .profiles
                .get(n)
                .with_context(|| format!("Profile '{}' not found", n)),
            None => Ok(&self.default),
        }
    }

    pub fn profile_labels(&self) -> Vec<String> {
        let mut labels = vec!["default".to_string()];
        let mut names: Vec<String> = self.profiles.keys().cloned().collect();
        names.sort();
        labels.extend(names);
        labels
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_legacy_feedback_aliases() {
        let input = r#"
[default]
inbox_dir = "/tmp"
calls_dir = "/tmp"
feedback = ["english", "common"]

[feedback]
model = "gpt-5.6-terra"
reasoning_effort = "medium"
"#;
        let config: Config = toml::from_str(input).unwrap();
        assert_eq!(config.default.modules, vec!["english", "common"]);
        assert_eq!(config.analysis.model, "gpt-5.6-terra");
    }
}
