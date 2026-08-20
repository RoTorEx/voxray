use anyhow::{Context, Result};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

pub fn path() -> Result<PathBuf> {
    let exe = std::env::current_exe().context("Failed to determine executable path")?;
    let dir = exe.parent().context("Executable has no parent directory")?;
    Ok(dir.join("voxray.log"))
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
    let command = args.get(1).map(String::as_str).unwrap_or("unknown");
    info(&format!("command_start command={command}"));
}

pub fn event(message: &str) {
    info(message);
}
