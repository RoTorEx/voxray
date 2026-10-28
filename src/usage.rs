//! Local, bounded product events. Never pass content, paths, argv or raw errors.
use fs2::FileExt;
use serde_json::{Value, json};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::{
        OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const SEGMENT_BYTES: u64 = 2 * 1024 * 1024;
const SEGMENTS: usize = 5;
const RETENTION_SECONDS: u64 = 30 * 86400;
const RECORD_BYTES: usize = 4096;
static LOGGER: OnceLock<Logger> = OnceLock::new();
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
static WARNED: AtomicBool = AtomicBool::new(false);
struct Logger {
    path: PathBuf,
    app: &'static str,
    run: String,
}

pub fn init(app: &'static str, path: PathBuf) {
    let _ = LOGGER.set(Logger {
        path,
        app,
        run: format!("{}-{}", std::process::id(), nanos()),
    });
}
fn nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}
fn seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
}

/// Only scalar counts, booleans and explicitly permitted categorical fields survive.
pub fn safe_fields(fields: Value) -> Value {
    let mut safe = serde_json::Map::new();
    if let Value::Object(fields) = fields {
        for (key, value) in fields.into_iter().take(32) {
            if !token(&key) {
                continue;
            }
            match &value {
                Value::Number(_) | Value::Bool(_) => {
                    safe.insert(key, value);
                }
                Value::String(s)
                    if matches!(
                        key.as_str(),
                        "stage"
                            | "mode"
                            | "provider"
                            | "model"
                            | "error_code"
                            | "reason"
                            | "source"
                            | "status"
                    ) && token(s) =>
                {
                    safe.insert(key, value);
                }
                _ => {}
            }
        }
    }
    Value::Object(safe)
}

pub fn event(action: &str, result: &str, fields: Value) {
    emit(action, result, fields, None, None);
}
fn emit(
    action: &str,
    result: &str,
    fields: Value,
    operation: Option<&str>,
    duration: Option<u128>,
) {
    let Some(logger) = LOGGER.get() else {
        return;
    };
    if !token(action) || !token(result) {
        return;
    }
    let now = SystemTime::now();
    let timestamp = chrono::DateTime::<chrono::Utc>::from(now)
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let record = json!({"schema_version":1,"timestamp":timestamp,"epoch_seconds":seconds(),"app":logger.app,"app_version":env!("CARGO_PKG_VERSION"),"run_id":logger.run,"operation_id":operation,"action":action,"result":result,"duration_ms":duration,"fields":safe_fields(fields)});
    let write = serde_json::to_vec(&record)
        .map_err(io::Error::other)
        .and_then(|mut line| {
            line.push(b'\n');
            append(
                &logger.path,
                &line,
                seconds(),
                SEGMENT_BYTES,
                SEGMENTS,
                RETENTION_SECONDS,
            )
        });
    if write.is_err() && !WARNED.swap(true, Ordering::Relaxed) {
        eprintln!(
            "{}: usage logging unavailable; the operation continues",
            logger.app
        );
    }
}

pub struct Operation {
    action: &'static str,
    id: String,
    start: Instant,
    finished: bool,
}
impl Operation {
    pub fn start(action: &'static str) -> Self {
        let id = format!(
            "{}-{}-{}",
            std::process::id(),
            nanos(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        );
        emit(action, "started", json!({}), Some(&id), None);
        Self {
            action,
            id,
            start: Instant::now(),
            finished: false,
        }
    }
    pub fn finish(mut self, result: &str, fields: Value) {
        emit(
            self.action,
            result,
            fields,
            Some(&self.id),
            Some(self.start.elapsed().as_millis()),
        );
        self.finished = true;
    }
}
impl Drop for Operation {
    fn drop(&mut self) {
        if !self.finished {
            emit(
                self.action,
                if std::thread::panicking() {
                    "interrupted"
                } else {
                    "error"
                },
                json!({"error_code":"operation_incomplete"}),
                Some(&self.id),
                Some(self.start.elapsed().as_millis()),
            );
        }
    }
}
fn regular(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) if !meta.file_type().is_file() => {
            Err(io::Error::other("log destination is not a regular file"))
        }
        Ok(_) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}
fn private_open(path: &Path, truncate: bool) -> io::Result<File> {
    regular(path)?;
    let mut options = OpenOptions::new();
    options.create(true).write(true).read(true);
    if truncate {
        options.truncate(true);
    } else {
        options.append(true);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    Ok(file)
}
fn segment(path: &Path, index: usize) -> PathBuf {
    if index == 0 {
        path.to_owned()
    } else {
        path.with_file_name(format!(
            "{}.{}",
            path.file_name().unwrap().to_string_lossy(),
            index
        ))
    }
}
fn remove(path: &Path) -> io::Result<()> {
    regular(path)?;
    match fs::remove_file(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}
fn append(
    path: &Path,
    line: &[u8],
    now: u64,
    limit: u64,
    count: usize,
    retention: u64,
) -> io::Result<()> {
    if line.len() > RECORD_BYTES || line.len() as u64 > limit {
        return Err(io::Error::other("usage event too large"));
    }
    let directory = path
        .parent()
        .ok_or_else(|| io::Error::other("missing log directory"))?;
    fs::create_dir_all(directory)?;
    if fs::symlink_metadata(directory)?.file_type().is_symlink() {
        return Err(io::Error::other("symlink log directory"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(directory, fs::Permissions::from_mode(0o700))?;
    }
    // A separate inode survives renames; the OS releases its lock after a crash.
    let lock = private_open(&path.with_extension("lock"), false)?;
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        match lock.try_lock_exclusive() {
            Ok(()) => break,
            Err(e) if e.kind() == io::ErrorKind::WouldBlock && Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(2))
            }
            Err(e) => return Err(e),
        }
    }
    for index in 0..count {
        regular(&segment(path, index))?;
    }
    let mut first = [0u8; 1];
    if File::open(path)
        .and_then(|mut f| f.read_exact(&mut first))
        .is_ok()
        && first[0] == b'['
    {
        let legacy = path.with_file_name("voxray.legacy.log");
        if legacy.exists() {
            return Err(io::Error::other("legacy log already exists"));
        }
        fs::rename(path, legacy)?;
    }
    // Prune by record time once daily, not file mtime (an active file may span months).
    let marker = path.with_extension("retention");
    regular(&marker)?;
    let last = fs::read_to_string(&marker)
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    if last / 86400 != now / 86400 || last > now {
        for index in 0..count {
            let current = segment(path, index);
            if !current.exists() {
                continue;
            }
            let contents = fs::read_to_string(&current)?;
            let kept = contents
                .lines()
                .filter(|s| {
                    serde_json::from_str::<Value>(s)
                        .ok()
                        .and_then(|v| v["epoch_seconds"].as_u64())
                        .is_some_and(|t| t >= now.saturating_sub(retention))
                })
                .collect::<Vec<_>>()
                .join("\n");
            let temp = path.with_extension("retention-tmp");
            let mut file = private_open(&temp, true)?;
            if !kept.is_empty() {
                writeln!(file, "{kept}")?;
            }
            file.sync_data()?;
            fs::rename(temp, current)?;
        }
        let mut file = private_open(&marker, true)?;
        write!(file, "{now}")?;
    }
    if fs::metadata(path).map(|m| m.len()).unwrap_or(0) + line.len() as u64 > limit {
        remove(&segment(path, count - 1))?;
        for index in (0..count - 1).rev() {
            let old = segment(path, index);
            if old.exists() {
                fs::rename(old, segment(path, index + 1))?;
            }
        }
    }
    let mut file = private_open(path, false)?;
    file.write_all(line)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn scratch() -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "usage-test-{}-{}-{}",
            std::process::id(),
            nanos(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&p).unwrap();
        p
    }
    #[test]
    fn rotation_and_age_preserve_recent_records_and_bound_bytes() {
        let root = scratch();
        let p = root.join("events.jsonl");
        for n in 0..30 {
            let line = format!("{{\"epoch_seconds\":1000000,\"n\":{n}}}\n");
            append(&p, line.as_bytes(), 1000000, 120, 3, 100).unwrap();
        }
        let total: u64 = (0..3)
            .map(|i| fs::metadata(segment(&p, i)).map(|m| m.len()).unwrap_or(0))
            .sum();
        assert!(total <= 360);
        append(&p, b"{\"epoch_seconds\":2000000}\n", 2000000, 120, 3, 100).unwrap();
        for i in 0..3 {
            if let Ok(s) = fs::read_to_string(segment(&p, i)) {
                assert!(!s.contains("1000000"));
            }
        }
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn concurrent_writers_keep_complete_unique_records() {
        let root = scratch();
        let p = root.join("events.jsonl");
        let writers = (0..8)
            .map(|i| {
                let p = p.clone();
                std::thread::spawn(move || {
                    for n in 0..30 {
                        let line = format!("{{\"epoch_seconds\":1000000,\"id\":{}}}\n", i * 30 + n);
                        append(&p, line.as_bytes(), 1000000, 100000, 5, 100).unwrap();
                    }
                })
            })
            .collect::<Vec<_>>();
        for writer in writers {
            writer.join().unwrap();
        }
        let contents = fs::read_to_string(p).unwrap();
        let ids = contents
            .lines()
            .map(|s| {
                serde_json::from_str::<Value>(s).unwrap()["id"]
                    .as_u64()
                    .unwrap()
            })
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(ids.len(), 240);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn content_is_not_serialized_and_invalid_storage_fails_safely() {
        let v = safe_fields(
            json!({"count":3,"mode":"plain","title":"SENSITIVE_MARKER","error":"/private/secret","payload":{"token":"SECRET"},"stage":"secret/path"}),
        );
        assert_eq!(v, json!({"count":3,"mode":"plain"}));
        let root = scratch();
        let p = root.join("events.jsonl");
        fs::create_dir(&p).unwrap();
        assert!(append(&p, b"{}\n", 1000000, 120, 3, 100).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
