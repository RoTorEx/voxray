use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::PathBuf;

pub const REPORT_LANGUAGE: &str = "en";
pub const TRANSCRIPTION_LANGUAGE: &str = "auto";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_profile")]
    pub default: Profile,
    #[serde(default)]
    pub profiles: HashMap<String, Profile>,
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
}

impl Default for TranscriptionConfig {
    fn default() -> Self {
        Self {
            model: default_transcription_model(),
        }
    }
}

fn default_transcription_model() -> String {
    "whisperkit:openai_whisper-large-v3".to_string()
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
    }
}

fn dummy_profile() -> Profile {
    Profile {
        inbox_dir: PathBuf::from("/path/to/inbox"),
        calls_dir: PathBuf::from("/path/to/calls"),
        date_format: Some("%Y-%m-%d %H-%M".to_string()),
        mode: Some(Mode::Folder),
        modules: Vec::new(),
    }
}

impl Config {
    pub fn path() -> Result<PathBuf> {
        Ok(app_home()?.join("config.toml"))
    }

    pub fn load() -> Result<Self> {
        let path = Self::path()?;
        let content = match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                anyhow::bail!(
                    "Configuration is required at {}; run `voxray setup-config`",
                    path.display()
                )
            }
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("Failed to read config from {}", path.display()));
            }
        };
        toml::from_str(&content)
            .with_context(|| format!("Failed to parse config from {}", path.display()))
    }

    pub fn starter() -> Self {
        Config {
            default: default_profile(),
            profiles: HashMap::from([("dummy".to_string(), dummy_profile())]),
            analysis: AnalysisConfig::default(),
            transcription: TranscriptionConfig::default(),
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::path()?;
        fs::create_dir_all(
            path.parent()
                .context("Config path has no parent directory")?,
        )
        .with_context(|| format!("Failed to create config directory {}", path.display()))?;
        fs::set_permissions(
            path.parent()
                .context("Config path has no parent directory")?,
            fs::Permissions::from_mode(0o700),
        )?;
        let content = toml::to_string_pretty(self).context("Failed to serialize config")?;
        let temporary = path.with_file_name(format!(".config-{}.tmp", std::process::id()));
        let result = (|| {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&temporary)
                .with_context(|| format!("Failed to create {}", temporary.display()))?;
            file.write_all(content.as_bytes())?;
            file.sync_all()?;
            fs::rename(&temporary, &path)
                .with_context(|| format!("Failed to publish {}", path.display()))?;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(temporary);
        }
        result
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

    pub fn analysis_enabled(&self) -> bool {
        !self.default.modules.is_empty()
            || self
                .profiles
                .values()
                .any(|profile| !profile.modules.is_empty())
    }
}

pub fn app_home() -> Result<PathBuf> {
    let exe = std::env::current_exe().context("Failed to determine executable path")?;
    app_home_from_executable(&exe)
}

fn app_home_from_executable(exe: &std::path::Path) -> Result<PathBuf> {
    let dir = exe.parent().context("Executable has no parent directory")?;
    if dir.file_name().is_some_and(|name| name == "bin") {
        return dir
            .parent()
            .map(std::path::Path::to_path_buf)
            .context("Executable bin directory has no parent");
    }
    Ok(dir.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_binary_uses_parent_of_bin_as_app_home() {
        assert_eq!(
            app_home_from_executable(std::path::Path::new("/Users/you/.x-cli-voxray/bin/voxray"))
                .unwrap(),
            PathBuf::from("/Users/you/.x-cli-voxray")
        );
    }

    #[test]
    fn development_binary_uses_its_direct_parent() {
        assert_eq!(
            app_home_from_executable(std::path::Path::new("/tmp/target/debug/voxray")).unwrap(),
            PathBuf::from("/tmp/target/debug")
        );
    }

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

    #[test]
    fn analysis_is_enabled_by_any_profile_modules() {
        let mut config = Config::starter();
        assert!(!config.analysis_enabled());
        config.default.modules.push("communication".to_string());
        assert!(config.analysis_enabled());
    }

    #[test]
    fn starter_contains_a_complete_safe_dummy_profile() {
        let config = Config::starter();
        let dummy = config.profiles.get("dummy").unwrap();
        assert_eq!(dummy.mode, Some(Mode::Folder));
        assert_eq!(dummy.date_format.as_deref(), Some("%Y-%m-%d %H-%M"));
        assert!(dummy.modules.is_empty());
        assert!(!config.analysis_enabled());
    }

    #[test]
    fn starter_omits_removed_no_op_options() {
        let serialized = toml::to_string(&Config::starter()).unwrap();
        assert!(!serialized.contains("call_type"));
        assert!(!serialized.contains("subject_name"));
        assert!(!serialized.contains("subject_role"));
        assert!(!serialized.contains("source_language"));
        assert!(!serialized.contains("call_goal"));
        assert!(!serialized.contains("timestamps"));
        assert!(!serialized.contains("speakers"));
    }
}
