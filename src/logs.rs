use anyhow::{Context, Result};
use chrono::{Duration, NaiveDate};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

const RETENTION_DAYS: i64 = 30;

pub fn path() -> Result<PathBuf> {
    Ok(crate::config::app_home()?.join("voxray.log"))
}

fn write(level: &str, message: &str) -> Result<()> {
    let path = path()?;
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
    let line = format!("[{}] {} {}\n", now, level, message);
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("Failed to open log file {}", path.display()))?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    file.write_all(line.as_bytes())
        .with_context(|| format!("Failed to write to log file {}", path.display()))?;
    Ok(())
}

pub fn info(message: &str) {
    let _ = write("INFO", message);
}

pub fn error(message: &str) {
    let _ = write("ERROR", message);
}

pub fn command_start(args: &[String]) {
    let _ = prune_old_entries();
    let command = args.get(1).map(String::as_str).unwrap_or("unknown");
    info(&format!("command_start command={command}"));
}

pub fn event(message: &str) {
    info(message);
}

fn prune_old_entries() -> Result<()> {
    let path = path()?;
    let contents = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(error).with_context(|| format!("Failed to read {}", path.display()));
        }
    };
    let cutoff = chrono::Local::now().date_naive() - Duration::days(RETENTION_DAYS);
    let retained = retain_since(&contents, cutoff);
    if retained == contents {
        return Ok(());
    }
    let temporary = path.with_file_name(format!(".voxray-log-{}.tmp", std::process::id()));
    fs::write(&temporary, retained)
        .with_context(|| format!("Failed to write {}", temporary.display()))?;
    fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))?;
    fs::rename(&temporary, &path).with_context(|| format!("Failed to prune {}", path.display()))
}

fn retain_since(contents: &str, cutoff: NaiveDate) -> String {
    let mut retained = contents
        .lines()
        .filter(|line| {
            line.strip_prefix('[')
                .and_then(|value| value.get(..10))
                .and_then(|value| NaiveDate::parse_from_str(value, "%Y-%m-%d").ok())
                .is_none_or(|date| date >= cutoff)
        })
        .collect::<Vec<_>>()
        .join("\n");
    if !retained.is_empty() {
        retained.push('\n');
    }
    retained
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_entries_older_than_cutoff() {
        let contents = "[2026-06-01 10:00:00.000] INFO old\n[2026-07-01 10:00:00.000] INFO edge\n[2026-07-02 10:00:00.000] INFO new\n";
        assert_eq!(
            retain_since(contents, NaiveDate::from_ymd_opt(2026, 7, 1).unwrap()),
            "[2026-07-01 10:00:00.000] INFO edge\n[2026-07-02 10:00:00.000] INFO new\n"
        );
    }
}
