use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use inquire::{Confirm, Password};

const TOKEN_FILE: &str = "openai-api-key";

pub struct SetupResult {
    pub path: PathBuf,
    pub changed: bool,
}

pub fn run() -> Result<SetupResult> {
    let home = crate::config::app_home()?;
    fs::create_dir_all(&home).with_context(|| format!("Failed to create {}", home.display()))?;
    fs::set_permissions(&home, fs::Permissions::from_mode(0o700))
        .with_context(|| format!("Failed to secure {}", home.display()))?;
    let path = home.join(TOKEN_FILE);

    if has_saved_token(&path)? {
        println!(
            "OpenAI API token is already configured at {}",
            path.display()
        );
        let replace = Confirm::new("Replace it?")
            .with_default(false)
            .prompt()
            .context("Failed to confirm token replacement")?;
        if !replace {
            return Ok(SetupResult {
                path,
                changed: false,
            });
        }
    }

    let token = Password::new("OpenAI API token:")
        .with_custom_confirmation_message("Confirm OpenAI API token:")
        .with_custom_confirmation_error_message("Tokens do not match")
        .prompt()
        .context("Failed to read OpenAI API token")?;
    let token = token.trim();
    if token.is_empty() {
        bail!("OpenAI API token must not be empty");
    }

    save_token(&path, token)?;
    Ok(SetupResult {
        path,
        changed: true,
    })
}

fn has_saved_token(path: &Path) -> Result<bool> {
    match fs::read_to_string(path) {
        Ok(value) => Ok(!value.trim().is_empty()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error).with_context(|| format!("Failed to read {}", path.display())),
    }
}

fn save_token(path: &Path, token: &str) -> Result<()> {
    let parent = path
        .parent()
        .context("Token path has no parent directory")?;
    let temporary = parent.join(format!(".{TOKEN_FILE}-{}.tmp", std::process::id()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)
            .with_context(|| format!("Failed to create {}", temporary.display()))?;
        file.write_all(token.as_bytes())?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        fs::rename(&temporary, path)
            .with_context(|| format!("Failed to publish {}", path.display()))?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_trimmed_token_with_private_permissions() {
        let directory = std::env::temp_dir().join(format!(
            "voxray-token-test-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        fs::create_dir(&directory).unwrap();
        let path = directory.join(TOKEN_FILE);

        save_token(&path, "sk-test").unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "sk-test\n");
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn detects_only_non_empty_saved_tokens() {
        let directory = std::env::temp_dir().join(format!(
            "voxray-token-presence-test-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        fs::create_dir(&directory).unwrap();
        let path = directory.join(TOKEN_FILE);
        assert!(!has_saved_token(&path).unwrap());
        fs::write(&path, "  \n").unwrap();
        assert!(!has_saved_token(&path).unwrap());
        fs::write(&path, "sk-existing\n").unwrap();
        assert!(has_saved_token(&path).unwrap());
        fs::remove_dir_all(directory).unwrap();
    }
}
