use std::{
    env, fs,
    io::Read,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};

const INSTALLER_URL: &str =
    "https://github.com/RoTorEx/voxray/releases/latest/download/voxray-install.sh";

pub fn run() -> Result<PathBuf> {
    let install_dir = env::current_exe()
        .context("Failed to determine the current executable")?
        .parent()
        .context("Executable has no parent directory")?
        .to_path_buf();
    let token_path = install_dir.join("gh-token");
    let token = github_token(&token_path)?;
    let temp_dir = temp_dir()?;
    let installer = temp_dir.join("voxray-install.sh");

    let result = (|| {
        download_installer(&installer, token.as_deref())?;
        let mut command = Command::new("sh");
        command
            .arg(&installer)
            .arg("--install-dir")
            .arg(&install_dir)
            .arg("--no-path-update");
        if let Some(token) = token.as_deref() {
            command.env("GH_INSTALLER_TOKEN", token);
        }
        let status = command.status().context("Failed to run the installer")?;
        if !status.success() {
            bail!("installer exited with {status}");
        }
        Ok(install_dir.join("voxray"))
    })();

    let _ = fs::remove_dir_all(temp_dir);
    result
}

fn download_installer(path: &Path, token: Option<&str>) -> Result<()> {
    let mut request = ureq::get(INSTALLER_URL);
    if let Some(token) = token {
        request = request.set("Authorization", &format!("Bearer {token}"));
    }
    let response = request.call().with_context(
        || "Failed to download the installer; set GH_INSTALLER_TOKEN for this private repository",
    )?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .read_to_end(&mut bytes)
        .context("Failed to read the installer")?;
    fs::write(path, bytes).with_context(|| format!("Failed to write {}", path.display()))
}

fn github_token(path: &Path) -> Result<Option<String>> {
    for name in ["GH_INSTALLER_TOKEN", "GH_TOKEN", "GITHUB_TOKEN"] {
        if let Ok(value) = env::var(name) {
            let value = value.trim();
            if !value.is_empty() {
                return Ok(Some(value.to_string()));
            }
        }
    }
    match fs::read_to_string(path) {
        Ok(value) => Ok((!value.trim().is_empty()).then(|| value.trim().to_string())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("Failed to read {}", path.display())),
    }
}

fn temp_dir() -> Result<PathBuf> {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("System time is before Unix epoch")?
        .as_nanos();
    let path = env::temp_dir().join(format!("voxray-update-{suffix}"));
    fs::create_dir(&path).with_context(|| format!("Failed to create {}", path.display()))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::INSTALLER_URL;

    #[test]
    fn updater_uses_latest_release_installer() {
        assert_eq!(
            INSTALLER_URL,
            "https://github.com/RoTorEx/voxray/releases/latest/download/voxray-install.sh"
        );
    }
}
