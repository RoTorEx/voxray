use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use anyhow::{Context, Result};
use inquire::Confirm;

pub struct SetupResult {
    pub path: PathBuf,
    pub changed: bool,
}

pub fn run() -> Result<SetupResult> {
    let home = crate::config::app_home()?;
    fs::create_dir_all(&home).with_context(|| format!("Failed to create {}", home.display()))?;
    fs::set_permissions(&home, fs::Permissions::from_mode(0o700))
        .with_context(|| format!("Failed to secure {}", home.display()))?;
    let path = crate::config::Config::path()?;

    if path.exists() {
        println!("Configuration is already present at {}", path.display());
        let replace = Confirm::new("Replace it with a starter configuration?")
            .with_default(false)
            .prompt()
            .context("Failed to confirm configuration replacement")?;
        if !replace {
            return Ok(SetupResult {
                path,
                changed: false,
            });
        }
    }

    crate::config::Config::starter().save()?;
    Ok(SetupResult {
        path,
        changed: true,
    })
}
