use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

fn temporary_path(path: &Path) -> Result<PathBuf> {
    let parent = path
        .parent()
        .context("Target path has no parent directory")?;
    let name = path
        .file_name()
        .context("Target path has no file name")?
        .to_string_lossy();
    Ok(parent.join(format!(".voxray-{name}-{}.tmp", std::process::id())))
}

pub fn atomic_copy(source: &Path, target: &Path) -> Result<()> {
    if target.exists() {
        bail!("Refusing to overwrite existing file {}", target.display());
    }
    let temp = temporary_path(target)?;
    let result = (|| {
        let copied = fs::copy(source, &temp).with_context(|| {
            format!("Failed to copy {} to {}", source.display(), temp.display())
        })?;
        let source_size = fs::metadata(source)?.len();
        if copied == 0 || copied != source_size {
            bail!(
                "Copy verification failed: source={} bytes, target={} bytes",
                source_size,
                copied
            );
        }
        fs::rename(&temp, target).with_context(|| {
            format!(
                "Failed to publish {} as {}",
                temp.display(),
                target.display()
            )
        })?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

pub fn temporary_sibling(path: &Path) -> Result<PathBuf> {
    temporary_path(path)
}

pub fn publish_temp(temp: &Path, target: &Path) -> Result<()> {
    if target.exists() {
        bail!("Refusing to overwrite existing file {}", target.display());
    }
    let size = fs::metadata(temp)
        .with_context(|| format!("Failed to inspect {}", temp.display()))?
        .len();
    if size == 0 {
        bail!("Temporary output is empty: {}", temp.display());
    }
    fs::rename(temp, target).with_context(|| {
        format!(
            "Failed to publish {} as {}",
            temp.display(),
            target.display()
        )
    })
}

pub fn atomic_write_pair(
    first_path: &Path,
    first_bytes: &[u8],
    second_path: &Path,
    second_bytes: &[u8],
) -> Result<()> {
    let first_temp = temporary_path(first_path)?;
    let second_temp = temporary_path(second_path)?;
    if let Err(error) = write_synced(&first_temp, first_bytes)
        .and_then(|_| write_synced(&second_temp, second_bytes))
    {
        let _ = fs::remove_file(&first_temp);
        let _ = fs::remove_file(&second_temp);
        return Err(error);
    }

    let first_backup = backup_path(first_path)?;
    let second_backup = backup_path(second_path)?;
    let had_first = first_path.exists();
    let had_second = second_path.exists();
    if had_first {
        fs::rename(first_path, &first_backup)?;
    }
    if had_second && let Err(error) = fs::rename(second_path, &second_backup) {
        if had_first {
            let _ = fs::rename(&first_backup, first_path);
        }
        let _ = fs::remove_file(&first_temp);
        let _ = fs::remove_file(&second_temp);
        return Err(error.into());
    }

    let publish = (|| {
        fs::rename(&first_temp, first_path)?;
        fs::rename(&second_temp, second_path)?;
        Ok::<_, anyhow::Error>(())
    })();
    if let Err(error) = publish {
        let _ = fs::remove_file(first_path);
        let _ = fs::remove_file(second_path);
        if had_first {
            let _ = fs::rename(&first_backup, first_path);
        }
        if had_second {
            let _ = fs::rename(&second_backup, second_path);
        }
        let _ = fs::remove_file(&first_temp);
        let _ = fs::remove_file(&second_temp);
        return Err(error);
    }
    let _ = fs::remove_file(first_backup);
    let _ = fs::remove_file(second_backup);
    Ok(())
}

fn write_synced(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file =
        fs::File::create(path).with_context(|| format!("Failed to create {}", path.display()))?;
    file.write_all(bytes)
        .with_context(|| format!("Failed to write {}", path.display()))?;
    file.sync_all()
        .with_context(|| format!("Failed to sync {}", path.display()))
}

fn backup_path(path: &Path) -> Result<PathBuf> {
    let parent = path
        .parent()
        .context("Target path has no parent directory")?;
    let name = path.file_name().context("Target path has no file name")?;
    Ok(parent.join(format!(
        ".voxray-backup-{}-{}.tmp",
        name.to_string_lossy(),
        std::process::id()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        std::env::temp_dir().join(format!(
            "voxray-storage-test-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ))
    }

    #[test]
    fn publishes_and_replaces_a_pair() {
        let dir = temp_dir();
        fs::create_dir_all(&dir).unwrap();
        let analysis = dir.join("analysis.json");
        let feedback = dir.join("feedback.txt");

        atomic_write_pair(&analysis, b"one", &feedback, b"first").unwrap();
        atomic_write_pair(&analysis, b"two", &feedback, b"second").unwrap();

        assert_eq!(fs::read(&analysis).unwrap(), b"two");
        assert_eq!(fs::read(&feedback).unwrap(), b"second");
        fs::remove_dir_all(dir).unwrap();
    }
}
