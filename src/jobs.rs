//! Versioned recording handoff with optional Drive archiving. Capture originals are never mutated.
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use chrono::DateTime;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::cli::{Commands, JobCommand};
use crate::config::{self, Config, Mode};
use crate::{call::CallPaths, drive, inbox, media, retention, transcribe};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub recording_id: String,
    pub recording: PathBuf,
    /// RFC3339 capture time, including the user's offset, not filesystem mtime.
    pub recorded_at: String,
    pub profile: String,
    pub name: String,
    /// Per-recording choice; omitted in legacy requests means transcribe.
    #[serde(default = "default_transcribe")]
    pub transcribe: bool,
    /// Retain video alongside canonical audio; legacy native requests are audio-only.
    #[serde(default)]
    pub keep_video: bool,
    #[serde(default)]
    pub google_drive: Option<bool>,
    #[serde(default)]
    pub drive_folder: Option<String>,
    #[serde(default)]
    pub transcript_only: bool,
}

fn default_transcribe() -> bool {
    true
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Job {
    pub schema_version: u32,
    pub id: String,
    pub request: Request,
    pub stage: String,
    pub recording: PathBuf,
    pub transcript: PathBuf,
    pub error: Option<String>,
    pub model: String,
    #[serde(default)]
    pub drive: crate::config::DriveOptions,
    #[serde(default)]
    pub drive_config: crate::config::GoogleDriveConfig,
    #[serde(default)]
    pub drive_url: Option<String>,
    #[serde(default)]
    pub media_removed: bool,
}

pub fn dispatch(command: &Commands) -> Result<Value> {
    let root = config::app_home()?.join("jobs");
    match command {
        Commands::Profiles => {
            let config = Config::load()?;
            Ok(json!(
                config
                    .profile_labels()
                    .into_iter()
                    .map(|id| {
                        let profile = if id == "default" {
                            &config.default
                        } else {
                            &config.profiles[&id]
                        };
                        json!({"id":id,"calls_dir":profile.calls_dir,"keep_video":profile.keep_video,"google_drive":profile.drive.enabled,"drive_folder":config.drive_options(profile).folder_id,"drive_supported":true})
                    })
                    .collect::<Vec<_>>()
            ))
        }
        Commands::Jobs(JobCommand::Submit { request }) => {
            let request: Request = serde_json::from_slice(&fs::read(request)?)?;
            job_value(submit(&root, request, &Config::load()?)?)
        }
        Commands::Jobs(JobCommand::Status { id }) => job_value(load(&directory(&root, id)?)?),
        Commands::Jobs(JobCommand::Run { id }) => job_value(run(&root, id)?),
        _ => bail!("Not a native contract"),
    }
}

// Ready is a fresh verification at the bridge boundary, not only a saved flag.
fn job_value(job: Job) -> Result<Value> {
    if job.media_removed {
        if !job.request.transcript_only || job.stage != "ready" {
            bail!("Invalid media removal state");
        }
        retention::verify(&job.recording, &CallPaths::from_recording(&job.recording)?)?;
    }
    Ok(serde_json::to_value(job)?)
}

fn directory(root: &Path, id: &str) -> Result<PathBuf> {
    if id.len() != 36
        || !id.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
    {
        bail!("recording_id must be a UUID");
    }
    Ok(root.join(id.to_ascii_lowercase()))
}

// Advisory OS locks disappear on process death; status reads need no lock.
fn lock(directory: &Path) -> Result<File> {
    fs::create_dir_all(directory)?;
    fs::set_permissions(directory, fs::Permissions::from_mode(0o700))?;
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .mode(0o600)
        .open(directory.join("lock"))?;
    file.try_lock()
        .context("Job is already running; read its status instead")?;
    Ok(file)
}

fn save(directory: &Path, job: &Job) -> Result<()> {
    let path = directory.join("job.json");
    let temp = directory.join(format!(".job-{}.tmp", std::process::id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&temp)?;
    file.write_all(&serde_json::to_vec_pretty(job)?)?;
    file.sync_all()?;
    // Single rename: a killed writer leaves either the previous or the new state.
    fs::rename(temp, path)?;
    File::open(directory)?.sync_all()?;
    Ok(())
}

fn load(directory: &Path) -> Result<Job> {
    let job: Job = serde_json::from_slice(&fs::read(directory.join("job.json"))?)
        .context("Job state is damaged; refusing to overwrite it")?;
    if job.schema_version != 1 {
        bail!("Unsupported job schema");
    }
    if directory.file_name().and_then(|v| v.to_str()) != Some(&job.id) {
        bail!("Job identity mismatch");
    }
    if ![
        "queued",
        "importing",
        "transcribing",
        "finalizing",
        "stored",
        "ready",
        "failed",
    ]
    .contains(&job.stage.as_str())
    {
        bail!("Invalid job stage");
    }
    if job.request.recording_id.to_ascii_lowercase() != job.id
        || !job.recording.is_absolute()
        || job.transcript != job.recording.with_file_name("transcript.txt")
        || !job.request.recording.is_absolute()
        || !job.model.contains(':')
    {
        bail!("Job state has invalid identity or artifact paths; refusing to overwrite it");
    }
    Ok(job)
}

fn submit(root: &Path, mut request: Request, config: &Config) -> Result<Job> {
    request.recording_id = request.recording_id.to_ascii_lowercase();
    let directory = directory(root, &request.recording_id)?;
    let _lock = lock(&directory)?;
    if directory.join("job.json").exists() {
        let job = load(&directory)?;
        if job.request != request {
            bail!("Recording ID already belongs to a different request");
        }
        return Ok(job);
    }
    // Never adopt an orphan source from an interrupted submit without validating it.
    if request.name.trim().is_empty() {
        bail!("Call name must not be empty");
    }
    if !request.recording.is_absolute()
        || !request.recording.is_file()
        || !media::is_media_file(&request.recording)
    {
        bail!("A finalized absolute media path is required");
    }
    if request.transcript_only && (!request.transcribe || request.keep_video) {
        bail!("Transcript-only requires transcription and cannot keep video");
    }
    let recorded_at = DateTime::parse_from_rfc3339(&request.recorded_at)?.naive_local();
    let mut profile = config
        .profile((request.profile != "default").then_some(request.profile.as_str()))?
        .clone();
    profile.drive = config.drive_options(&profile);
    if !profile.calls_dir.is_absolute() {
        bail!("Native call destination must be absolute");
    }
    if let Some(enabled) = request.google_drive {
        profile.drive.enabled = enabled;
    }
    if let Some(id) = &request.drive_folder {
        drive::validate_id(id)?;
        profile.drive.folder_id = Some(id.clone());
    }
    drive::preflight(&profile.drive)?;
    // Native calls always use the canonical call-folder contract.
    profile.mode = Some(Mode::Folder);
    let planned = inbox::plan_at(
        &request.recording,
        &request.name,
        request.keep_video,
        &profile,
        &recorded_at,
    )?;
    let job = Job {
        schema_version: 1,
        id: request.recording_id.to_ascii_lowercase(),
        request,
        stage: "queued".into(),
        transcript: planned
            .recording
            .parent()
            .context("Call has no folder")?
            .join("transcript.txt"),
        recording: planned.recording,
        error: None,
        model: config.transcription.model.clone(),
        drive: profile.drive.clone(),
        drive_config: config.google_drive.clone(),
        drive_url: None,
        media_removed: false,
    };
    // Persist identity before copying; retries can finish the same immutable source copy.
    save(&directory, &job)?;
    Ok(job)
}

fn run(root: &Path, id: &str) -> Result<Job> {
    let directory = directory(root, id)?;
    let _lock = lock(&directory)?;
    let mut job = load(&directory)?;
    if job.stage == "ready" || job.stage == "stored" {
        if !job.media_removed
            && job.request.keep_video
            && media::is_video_file(&job.request.recording)
        {
            let folder = job.recording.parent().context("Missing destination")?;
            let extension = job
                .request
                .recording
                .extension()
                .context("Missing extension")?;
            if !folder
                .join(format!("video.{}", extension.to_string_lossy()))
                .is_file()
            {
                bail!("Completed job video is missing");
            }
        }
        if (!job.media_removed && !job.recording.is_file())
            || (job.stage == "ready" && !job.transcript.is_file())
        {
            bail!("Completed job artifacts are missing");
        }
        return Ok(job);
    }
    job.error = None;
    let result = execute(&directory, &mut job);
    if let Err(error) = result {
        job.stage = "failed".into();
        job.error = Some(format!("{error:#}"));
        save(&directory, &job)?;
    }
    Ok(job)
}

fn execute(directory: &Path, job: &mut Job) -> Result<()> {
    let paths = CallPaths::from_recording(&job.recording)?;
    if job.request.transcript_only
        && !job.recording.exists()
        && paths.transcript.with_extension("proof.json").is_file()
    {
        return finish(directory, job);
    }
    let source = directory.join(format!(
        "source.{}",
        job.request
            .recording
            .extension()
            .context("Missing extension")?
            .to_string_lossy()
    ));
    if !source.exists() {
        let temp = directory.join("source.partial");
        fs::copy(&job.request.recording, &temp)?;
        let size = fs::metadata(&temp)?.len();
        if size == 0 || size != fs::metadata(&job.request.recording)?.len() {
            bail!("Source copy verification failed");
        }
        File::open(&temp)?.sync_all()?;
        fs::rename(&temp, &source)?;
        File::open(directory)?.sync_all()?;
    }
    job.stage = "importing".into();
    save(directory, job)?;
    let folder = job.recording.parent().context("Missing destination")?;
    fs::create_dir_all(folder.parent().context("Missing library")?)?;
    reserve_folder(folder, &job.id)?;
    if !job.recording.exists() {
        let temp = folder.join(format!(".record-{}.tmp", std::process::id()));
        if media::is_video_file(&source) {
            transcribe::extract_audio(&source, &temp)?;
        } else {
            fs::copy(&source, &temp)?;
        }
        if fs::metadata(&temp)?.len() == 0 {
            bail!("Imported audio is empty");
        }
        File::open(&temp)?.sync_all()?;
        // Only our locked job can publish inside its reserved folder.
        fs::rename(temp, &job.recording)?;
        File::open(folder)?.sync_all()?;
    }
    if fs::metadata(&job.recording)?.len() == 0 {
        bail!("Saved recording is empty");
    }
    if job.request.keep_video && media::is_video_file(&source) {
        let video = folder.join(format!(
            "video.{}",
            source.extension().unwrap().to_string_lossy()
        ));
        if !video.exists() {
            let temp = folder.join(format!(".video-{}.tmp", std::process::id()));
            fs::copy(&source, &temp)?;
            if fs::metadata(&temp)?.len() != fs::metadata(&source)?.len() {
                bail!("Video copy verification failed");
            }
            File::open(&temp)?.sync_all()?;
            fs::rename(temp, &video)?;
            File::open(folder)?.sync_all()?;
        }
        if fs::metadata(&video)?.len() != fs::metadata(&source)?.len() {
            bail!("Saved video size differs from source");
        }
    }
    if !job.request.transcribe {
        return finish(directory, job);
    }
    job.stage = "transcribing".into();
    save(directory, job)?;
    transcribe::run(
        &job.recording,
        &job.recording,
        transcribe::TranscribeOptions {
            model: &job.model,
            force: false,
        },
    )?;
    finish(directory, job)
}

fn finish(directory: &Path, job: &mut Job) -> Result<()> {
    job.stage = "finalizing".into();
    save(directory, job)?;
    let paths = CallPaths::from_recording(&job.recording)?;
    if job.request.transcript_only {
        retention::verify(&job.recording, &paths)?;
    }
    job.drive_url = drive::sync(
        &job.drive_config,
        &job.drive,
        &paths,
        Some(&job.recording),
        job.request.transcript_only,
    )?;
    if job.request.transcript_only {
        retention::remove_media(&job.recording, &paths)?;
        let source = directory.join(format!(
            "source.{}",
            job.request
                .recording
                .extension()
                .context("Missing extension")?
                .to_string_lossy()
        ));
        if source.exists() {
            fs::remove_file(source)?;
            File::open(directory)?.sync_all()?;
        }
        job.media_removed = true;
    }
    job.stage = if job.request.transcribe {
        "ready"
    } else {
        "stored"
    }
    .into();
    save(directory, job)
}

fn reserve_folder(folder: &Path, id: &str) -> Result<()> {
    if folder.exists() {
        if fs::read_to_string(folder.join(".voxray-recording-id"))
            .ok()
            .as_deref()
            == Some(id)
        {
            return Ok(());
        }
        bail!(
            "Call destination belongs to another recording: {}",
            folder.display()
        );
    }
    let parent = folder.parent().context("Call has no parent")?;
    let staged = parent.join(format!(".voxray-reservation-{id}"));
    fs::create_dir_all(&staged)?;
    let marker = staged.join(".voxray-recording-id");
    // This private staging folder contains no media and belongs to this locked job.
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(&marker)?;
    file.write_all(id.as_bytes())?;
    file.sync_all()?;
    File::open(&staged)?.sync_all()?;
    rename_exclusive(&staged, folder)?;
    File::open(parent)?.sync_all()?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn rename_exclusive(source: &Path, target: &Path) -> Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    unsafe extern "C" {
        fn renamex_np(
            source: *const std::ffi::c_char,
            target: *const std::ffi::c_char,
            flags: u32,
        ) -> i32;
    }
    let source = CString::new(source.as_os_str().as_bytes())?;
    let target = CString::new(target.as_os_str().as_bytes())?;
    // macOS sys/stdio.h: RENAME_EXCL refuses even an existing empty directory.
    // Both NUL-terminated paths remain alive throughout this synchronous call.
    if unsafe { renamex_np(source.as_ptr(), target.as_ptr(), 0x00000004) } != 0 {
        return Err(std::io::Error::last_os_error())
            .context("Failed to reserve call folder without overwriting");
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn rename_exclusive(_source: &Path, _target: &Path) -> Result<()> {
    bail!("Native recording jobs require macOS exclusive directory publication")
}

#[cfg(test)]
mod tests {
    use super::*;
    static FIXTURE_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    fn fixture() -> (PathBuf, Config, Request) {
        let root = std::env::temp_dir().join(format!(
            "voxray-jobs-{}-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap(),
            FIXTURE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        let source = root.join("input.wav");
        fs::write(&source, b"original samples").unwrap();
        let mut config = Config::starter();
        config.default.calls_dir = root.join("calls");
        let request = Request {
            recording_id: "12345678-1234-1234-1234-123456789abc".into(),
            recording: source,
            recorded_at: "2026-10-09T15:00:00+02:00".into(),
            profile: "default".into(),
            name: "Daily".into(),
            transcribe: true,
            keep_video: false,
            google_drive: None,
            drive_folder: None,
            transcript_only: false,
        };
        (root, config, request)
    }
    #[test]
    fn import_only_completes_without_transcript_and_preserves_source() {
        let (root, config, mut request) = fixture();
        request.transcribe = false;
        let jobs = root.join("jobs");
        let accepted = submit(&jobs, request.clone(), &config).unwrap();
        let completed = run(&jobs, &accepted.id).unwrap();
        assert_eq!(completed.stage, "stored");
        assert!(!completed.transcript.exists());
        assert_eq!(fs::read(&completed.recording).unwrap(), b"original samples");
        assert_eq!(fs::read(&request.recording).unwrap(), b"original samples");
        assert_eq!(run(&jobs, &accepted.id).unwrap().stage, "stored");
        let mut changed = request;
        changed.transcribe = true;
        assert!(submit(&jobs, changed, &config).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn legacy_requests_keep_transcription_enabled() {
        let (root, _, request) = fixture();
        let mut encoded = serde_json::to_value(&request).unwrap();
        encoded.as_object_mut().unwrap().remove("transcribe");
        encoded.as_object_mut().unwrap().remove("keep_video");
        let legacy = serde_json::from_value::<Request>(encoded.clone()).unwrap();
        assert!(!legacy.keep_video);
        assert!(
            serde_json::from_value::<Request>(encoded)
                .unwrap()
                .transcribe
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn duplicate_submission_and_corrupt_state_preserve_identity() {
        let (root, config, request) = fixture();
        let jobs = root.join("jobs");
        let first = submit(&jobs, request.clone(), &config).unwrap();
        assert_eq!(
            submit(&jobs, request.clone(), &config).unwrap().recording,
            first.recording
        );
        let mut other = request;
        other.name = "Different".into();
        assert!(submit(&jobs, other, &config).is_err());
        let path = directory(&jobs, &first.id).unwrap().join("job.json");
        fs::write(&path, b"broken").unwrap();
        assert!(run(&jobs, &first.id).is_err());
        assert_eq!(fs::read(path).unwrap(), b"broken");
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn interrupted_transcription_reuses_import_without_changing_original() {
        let (root, config, request) = fixture();
        let jobs = root.join("jobs");
        let mut job = submit(&jobs, request.clone(), &config).unwrap();
        let dir = directory(&jobs, &job.id).unwrap();
        let folder = job.recording.parent().unwrap();
        fs::create_dir_all(folder).unwrap();
        fs::write(folder.join(".voxray-recording-id"), &job.id).unwrap();
        fs::write(&job.recording, b"imported audio").unwrap();
        fs::write(&job.transcript, b"previously completed transcript").unwrap();
        job.stage = "transcribing".into();
        save(&dir, &job).unwrap();
        let completed = run(&jobs, &job.id).unwrap();
        assert_eq!(completed.stage, "ready");
        assert_eq!(fs::read(&completed.recording).unwrap(), b"imported audio");
        assert_eq!(fs::read(request.recording).unwrap(), b"original samples");
        assert_eq!(run(&jobs, &job.id).unwrap().stage, "ready");
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn collision_never_overwrites_another_call() {
        let (root, config, request) = fixture();
        let jobs = root.join("jobs");
        let job = submit(&jobs, request, &config).unwrap();
        fs::create_dir_all(job.recording.parent().unwrap()).unwrap();
        fs::write(&job.recording, b"someone else's call").unwrap();
        let failed = run(&jobs, &job.id).unwrap();
        assert_eq!(failed.stage, "failed");
        assert_eq!(fs::read(&job.recording).unwrap(), b"someone else's call");
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn exclusive_reservation_recovers_staging_and_rejects_empty_foreign_folder() {
        let (root, _, request) = fixture();
        let target = root.join("Call");
        let staged = root.join(format!(".voxray-reservation-{}", request.recording_id));
        fs::create_dir(&staged).unwrap();
        fs::write(staged.join(".voxray-recording-id"), b"interrupted marker").unwrap();
        reserve_folder(&target, &request.recording_id).unwrap();
        assert_eq!(
            fs::read_to_string(target.join(".voxray-recording-id")).unwrap(),
            request.recording_id
        );
        assert!(!staged.exists());
        let foreign = root.join("foreign");
        fs::create_dir(&foreign).unwrap();
        assert!(reserve_folder(&foreign, &request.recording_id).is_err());
        assert_eq!(fs::read_dir(foreign).unwrap().count(), 0);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn live_worker_lock_is_released_on_drop() {
        let (root, _, _) = fixture();
        let first = lock(&root).unwrap();
        assert!(lock(&root).is_err());
        drop(first);
        assert!(lock(&root).is_ok());
        fs::remove_dir_all(root).unwrap();
    }
}
