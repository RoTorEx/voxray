//! Explicit artifact transfer with a shared receipt and three-way conflict detection.
use super::{CallLock, Client, Receipt, matches_content, state_path, verify_parent};
use crate::{
    call::CallPaths,
    config::{DriveOptions, GoogleDriveConfig, Mode, Profile},
    retention, storage,
};
use anyhow::{Context, Result, bail};
use serde::Serialize;
use serde_json::Value;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Direction {
    Push,
    Pull,
}
impl Direction {
    pub fn label(self) -> &'static str {
        match self {
            Self::Push => "push",
            Self::Pull => "pull",
        }
    }
}
#[derive(Debug, Serialize)]
pub struct Action {
    pub call: String,
    pub file: String,
    pub action: String,
    pub conflict_copy: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct Report {
    pub direction: &'static str,
    pub dry_run: bool,
    pub calls: usize,
    pub actions: Vec<Action>,
}

pub(super) trait Remote {
    fn metadata(&self, id: &str) -> Result<Option<Value>>;
    fn children(&self, parent: &str) -> Result<Vec<Value>>;
    fn generate_id(&self) -> Result<String>;
    fn ensure_folder(&self, id: &str, parent: &str, name: &str) -> Result<()>;
    fn upload(&self, id: &str, parent: &str, path: &Path, baseline: Option<&str>) -> Result<()>;
    fn download(&self, id: &str, output: &mut File) -> Result<()>;
    fn trash(&self, item: &Value) -> Result<()>;
    fn read_state(&self, id: &str) -> Result<Vec<u8>>;
}
impl Remote for Client {
    fn metadata(&self, id: &str) -> Result<Option<Value>> {
        Client::metadata(self, id)
    }
    fn children(&self, parent: &str) -> Result<Vec<Value>> {
        super::validate_id(parent)?;
        let mut result = Vec::new();
        let mut token = String::new();
        loop {
            let mut request = self
                .request("GET", &format!("{}/files", self.api))
                .query("q", &format!("'{parent}' in parents and trashed=false"))
                .query(
                    "fields",
                    "nextPageToken,files(id,name,mimeType,parents,size,md5Checksum,trashed)",
                )
                .query("pageSize", "1000");
            if !token.is_empty() {
                request = request.query("pageToken", &token);
            }
            let data: Value = request.call()?.into_json()?;
            result.extend(
                data["files"]
                    .as_array()
                    .context("Invalid Drive listing")?
                    .iter()
                    .cloned(),
            );
            match data["nextPageToken"].as_str() {
                Some(next) if !next.is_empty() => token = next.into(),
                _ => break,
            }
        }
        Ok(result)
    }
    fn generate_id(&self) -> Result<String> {
        Client::generate_id(self)
    }
    fn ensure_folder(&self, id: &str, parent: &str, name: &str) -> Result<()> {
        Client::ensure_folder(self, id, parent, name)
    }
    fn upload(&self, id: &str, parent: &str, path: &Path, baseline: Option<&str>) -> Result<()> {
        self.upload_file(id, parent, path, baseline)
    }
    fn read_state(&self, id: &str) -> Result<Vec<u8>> {
        use std::io::Read;
        super::validate_id(id)?;
        let mut bytes = Vec::new();
        self.request("GET", &format!("{}/files/{id}", self.api))
            .query("alt", "media")
            .call()?
            .into_reader()
            .take(4097)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 4096 {
            bail!("Call synchronization state is too large");
        }
        Ok(bytes)
    }
    fn trash(&self, item: &Value) -> Result<()> {
        let id = item["id"].as_str().context("Missing media ID")?;
        super::validate_id(id)?;
        let fresh = self
            .metadata(id)?
            .context("Media disappeared during cleanup")?;
        if fresh["name"] != item["name"]
            || fresh["parents"] != item["parents"]
            || fresh["md5Checksum"] != item["md5Checksum"]
            || fresh["trashed"] == true
        {
            bail!("Cloud media changed during cleanup; preserved");
        }
        let mut request = self.request("PATCH", &format!("{}/files/{id}", self.api));
        if let Some(etag) = fresh["_etag"].as_str() {
            request = request.set("If-Match", etag);
        }
        request.send_json(serde_json::json!({"trashed":true}))?;
        if !self.metadata(id)?.is_some_and(|v| v["trashed"] == true) {
            bail!("Cloud media removal verification failed");
        }
        Ok(())
    }
    fn download(&self, id: &str, output: &mut File) -> Result<()> {
        super::validate_id(id)?;
        let response = self
            .request("GET", &format!("{}/files/{id}", self.api))
            .query("alt", "media")
            .call()?;
        io::copy(&mut response.into_reader(), output)?;
        output.sync_all()?;
        Ok(())
    }
}

fn component(name: &str) -> Result<()> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains(['/', '\\'])
        || name.chars().any(char::is_control)
    {
        bail!("Unsafe call/artifact name; no transfer performed");
    }
    Ok(())
}
fn paths(root: &Path, name: &str, mode: Mode) -> Result<CallPaths> {
    component(name)?;
    let transcript = match mode {
        Mode::Folder => root.join(name).join("transcript.txt"),
        Mode::File => root.join(format!("{name}.transcript.txt")),
    };
    CallPaths::from_transcript(&transcript)
}
pub(super) fn artifact(paths: &CallPaths, name: &str) -> bool {
    if retention::sync_path(paths)
        .file_name()
        .and_then(|n| n.to_str())
        == Some(name)
    {
        return true;
    }
    let short = match paths.mode {
        Mode::Folder => name,
        Mode::File => match name.strip_prefix(&format!("{}.", paths.name)) {
            Some(rest) => rest,
            None => return false,
        },
    };
    matches!(
        short,
        "transcript.txt" | "feedback.txt" | "recap.txt" | "call.json"
    ) || ((short.starts_with("record.") || short.starts_with("video.") || short == "audio.m4a")
        && crate::media::is_media_file(Path::new(short)))
}
fn regular(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            bail!("Refusing symbolic link {}", path.display())
        }
        Ok(meta) if meta.is_file() => Ok(true),
        Ok(_) => bail!("Expected a regular artifact: {}", path.display()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}
pub(super) fn local_files(paths: &CallPaths) -> Result<BTreeMap<String, PathBuf>> {
    let mut files = BTreeMap::new();
    if fs::symlink_metadata(&paths.root).is_ok_and(|m| m.file_type().is_symlink()) {
        bail!("Call directory must not be a symbolic link");
    }
    if !paths.root.exists() {
        return Ok(files);
    }
    for entry in fs::read_dir(&paths.root)? {
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("Non UTF-8 artifact name"))?;
        if artifact(paths, &name) {
            component(&name)?;
            if regular(&entry.path())? {
                files.insert(name, entry.path());
            }
        }
    }
    Ok(files)
}
fn local_calls(profile: &Profile) -> Result<BTreeMap<String, CallPaths>> {
    let mut result = BTreeMap::new();
    if !profile.calls_dir.exists() {
        return Ok(result);
    }
    for entry in fs::read_dir(&profile.calls_dir)? {
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("Non UTF-8 call name"))?;
        let call = match profile.mode.unwrap_or_default() {
            Mode::Folder if entry.file_type()?.is_dir() => Some(name),
            Mode::File => [
                ".transcript.txt",
                ".feedback.txt",
                ".recap.txt",
                ".call.json",
            ]
            .iter()
            .find_map(|suffix| name.strip_suffix(suffix).map(str::to_owned))
            .or_else(|| name.split_once(".record.").map(|(name, _)| name.to_owned()))
            .or_else(|| {
                name.strip_prefix('.')
                    .and_then(|n| n.strip_suffix(".sync.json"))
                    .map(str::to_owned)
            })
            .or_else(|| {
                name.strip_prefix('.')
                    .and_then(|n| n.strip_suffix(".drive.json"))
                    .map(str::to_owned)
            }),
            _ => None,
        };
        if let Some(name) = call {
            let p = paths(&profile.calls_dir, &name, profile.mode.unwrap_or_default())?;
            if !local_files(&p)?.is_empty() || state_path(&p).exists() {
                result.insert(name, p);
            }
        }
    }
    Ok(result)
}
fn cloud_folders(remote: &impl Remote, parent: &str) -> Result<BTreeMap<String, Value>> {
    let mut result = BTreeMap::new();
    let mut names = std::collections::BTreeSet::new();
    for value in remote.children(parent)? {
        if value["mimeType"] != "application/vnd.google-apps.folder" {
            continue;
        }
        let name = value["name"].as_str().context("Unnamed cloud call")?;
        component(name)?;
        if !names.insert(name.to_lowercase()) {
            bail!("Ambiguous duplicate cloud call name: {name}");
        }
        verify_parent(&value, parent, name)?;
        result.insert(name.to_owned(), value);
    }
    Ok(result)
}
pub(super) fn remote_files(
    remote: &impl Remote,
    paths: &CallPaths,
    folder: &str,
) -> Result<BTreeMap<String, Value>> {
    let mut files = BTreeMap::new();
    for item in remote.children(folder)? {
        let name = item["name"].as_str().context("Unnamed cloud artifact")?;
        component(name)?;
        if artifact(paths, name) {
            verify_parent(&item, folder, name)?;
            if item["md5Checksum"].as_str().is_none() {
                bail!(
                    "Cloud artifact must be a binary/text file, not a converted Google document: {name}"
                );
            }
            if files.insert(name.to_owned(), item).is_some() {
                bail!("Duplicate cloud artifact name");
            }
        }
    }
    Ok(files)
}

pub(super) struct Download(pub(super) PathBuf);
impl Drop for Download {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
pub(super) fn download(remote: &impl Remote, item: &Value, target: &Path) -> Result<Download> {
    let temp_path = storage::temporary_sibling(target)?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temp_path)?;
    let temp = Download(temp_path);
    remote.download(
        item["id"].as_str().context("Missing cloud artifact ID")?,
        &mut output,
    )?;
    output.sync_all()?;
    if !matches_content(
        item,
        fs::metadata(&temp.0)?.len(),
        &retention::checksum(&temp.0)?,
    ) {
        bail!("Cloud download checksum failed; local file preserved");
    }
    Ok(temp)
}
fn conflict_target(path: &Path, checksum: &str) -> Result<PathBuf> {
    if checksum.len() != 32 || !checksum.bytes().all(|c| c.is_ascii_hexdigit()) {
        bail!("Invalid cloud checksum");
    }
    let name = path
        .file_name()
        .context("Missing local file name")?
        .to_string_lossy();
    Ok(path.with_file_name(format!("{name}.conflict-drive-{}", &checksum[..12])))
}
#[derive(Debug, PartialEq)]
enum Change {
    Same,
    Transfer,
    OtherSide,
    Conflict,
}
fn change(
    direction: Direction,
    local: Option<&str>,
    cloud: Option<&str>,
    baseline: Option<&str>,
) -> Change {
    if local == cloud {
        return Change::Same;
    }
    match direction {
        Direction::Push if cloud.is_none() => Change::Transfer,
        Direction::Pull if local.is_none() => Change::Transfer,
        Direction::Push if local.is_none() => Change::OtherSide,
        Direction::Pull if cloud.is_none() => Change::OtherSide,
        _ if baseline.is_none() => Change::Conflict,
        Direction::Push if cloud == baseline => Change::Transfer,
        Direction::Pull if local == baseline => Change::Transfer,
        Direction::Push if local == baseline => Change::OtherSide,
        Direction::Pull if cloud == baseline => Change::OtherSide,
        _ => Change::Conflict,
    }
}

pub fn synchronize(
    config: &GoogleDriveConfig,
    options: &DriveOptions,
    profile: &Profile,
    direction: Direction,
    dry_run: bool,
    only: Option<&str>,
) -> Result<Report> {
    if !options.enabled {
        bail!("Google Drive is disabled for this profile");
    }
    if fs::symlink_metadata(&profile.calls_dir)
        .is_ok_and(|metadata| metadata.file_type().is_symlink())
    {
        bail!("Call root must not be a symbolic link");
    }
    super::preflight(options)?;
    if let Some(name) = only {
        component(name)?;
    }
    let client = Client::connect(config)?;
    let Some(parent) = client.destination(options, !dry_run && direction == Direction::Push)?
    else {
        let mut calls = local_calls(profile)?;
        if let Some(name) = only {
            if !calls.contains_key(name) {
                bail!("Selected call was not found: {name}");
            }
            calls.retain(|key, _| key == name);
        }
        let mut report = Report {
            direction: direction.label(),
            dry_run,
            calls: 0,
            actions: Vec::new(),
        };
        for (name, paths) in calls {
            if state_path(&paths).exists() {
                bail!(
                    "Existing call receipt needs migration to the mirrored destination before synchronization"
                );
            }
            let files = local_files(&paths)?;
            if files.is_empty() {
                continue;
            }
            report.calls += 1;
            report.actions.extend(files.into_keys().map(|file| {
                Action {
                    call: name.clone(),
                    file,
                    action: if direction == Direction::Push {
                        "upload"
                    } else {
                        "local_changed"
                    }
                    .into(),
                    conflict_copy: None,
                }
            }));
        }
        return Ok(report);
    };
    let resolved = DriveOptions {
        folder_id: Some(parent),
        calls_dir: None,
        ..options.clone()
    };
    synchronize_with(&client, &resolved, profile, direction, dry_run, only)
}
fn synchronize_with(
    remote: &impl Remote,
    options: &DriveOptions,
    profile: &Profile,
    direction: Direction,
    dry_run: bool,
    only: Option<&str>,
) -> Result<Report> {
    if fs::symlink_metadata(&profile.calls_dir).is_ok_and(|m| m.file_type().is_symlink()) {
        bail!("Call root must not be a symbolic link");
    }
    let parent = options.folder_id.as_deref().unwrap_or("root");
    let destination = remote
        .metadata(parent)?
        .context("Drive destination is unavailable")?;
    if destination["mimeType"] != "application/vnd.google-apps.folder"
        || destination["trashed"] == true
    {
        bail!("Drive destination is not an active folder");
    }
    let parent = if parent == "root" {
        destination["id"].as_str().context("Root ID unavailable")?
    } else {
        parent
    };
    let cloud = cloud_folders(remote, parent)?;
    let mut calls = local_calls(profile)?;
    if direction == Direction::Pull {
        for name in cloud.keys() {
            calls.entry(name.clone()).or_insert(paths(
                &profile.calls_dir,
                name,
                profile.mode.unwrap_or_default(),
            )?);
        }
    }
    if let Some(name) = only {
        if !calls.contains_key(name) {
            bail!("Selected call was not found: {name}");
        }
        calls.retain(|key, _| key == name);
    }
    let mut report = Report {
        direction: direction.label(),
        dry_run,
        calls: 0,
        actions: Vec::new(),
    };
    for (name, paths) in calls {
        let local = local_files(&paths)?;
        let receipt_path = state_path(&paths);
        let receipt_before = if regular(&receipt_path)? {
            Some(fs::read(&receipt_path)?)
        } else {
            None
        };
        let mut state: Receipt = match &receipt_before {
            Some(bytes) => serde_json::from_slice(bytes)?,
            None => Receipt::default(),
        };
        if !state.parent.is_empty() && state.parent != parent {
            bail!("Call receipt belongs to a different Drive destination");
        }
        let by_name = cloud.get(&name).and_then(|v| v["id"].as_str());
        if !state.folder.is_empty() {
            let item = remote
                .metadata(&state.folder)?
                .context("Recorded cloud call is missing; refusing automatic recreation")?;
            verify_parent(&item, parent, &name)?;
            if by_name != Some(state.folder.as_str()) {
                bail!("Cloud call identity differs from its receipt");
            }
        } else if let Some(id) = by_name {
            state.parent = parent.into();
            state.folder = id.into();
        }
        let files = if state.folder.is_empty() {
            BTreeMap::new()
        } else {
            remote_files(remote, &paths, &state.folder)?
        };
        if local.is_empty() && files.is_empty() {
            continue;
        }
        report.calls += 1;
        if !dry_run {
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(&paths.root)?;
        }
        let _lock = if dry_run {
            None
        } else {
            let lock_path = receipt_path.with_extension("lock");
            if fs::symlink_metadata(&lock_path).is_ok_and(|m| m.file_type().is_symlink()) {
                bail!("Refusing symbolic link lock");
            }
            let lock = OpenOptions::new()
                .create(true)
                .truncate(false)
                .write(true)
                .mode(0o600)
                .open(lock_path)?;
            lock.try_lock()
                .context("This call is already synchronizing")?;
            let current_receipt = if regular(&receipt_path)? {
                Some(fs::read(&receipt_path)?)
            } else {
                None
            };
            if current_receipt != receipt_before {
                bail!("Call receipt changed before lock acquisition; retry");
            }
            Some(CallLock(lock))
        };
        if !dry_run && state.folder.is_empty() {
            state.parent = parent.into();
            state.folder = remote.generate_id()?;
            storage::atomic_write(&receipt_path, &serde_json::to_vec(&state)?)?;
            remote.ensure_folder(&state.folder, parent, &name)?;
        }
        if !dry_run {
            storage::atomic_write(&receipt_path, &serde_json::to_vec(&state)?)?;
        }
        let text_only = super::retention_sync::reconcile(remote, &paths, &mut state, dry_run)?;
        let selected: std::collections::BTreeSet<_> =
            local.keys().chain(files.keys()).cloned().collect();
        for filename in selected {
            if retention::sync_path(&paths)
                .file_name()
                .and_then(|n| n.to_str())
                == Some(&filename)
                || (text_only && crate::media::is_media_file(Path::new(&filename)))
            {
                continue;
            }
            let target = paths.root.join(&filename);
            let local_hash = if regular(&target)? {
                Some(retention::checksum(&target)?)
            } else {
                None
            };
            let item = files.get(&filename);
            let cloud_hash = item.and_then(|i| i["md5Checksum"].as_str());
            if let Some(id) = state.files.get(&filename) {
                if let Some(item) = item {
                    if item["id"].as_str() != Some(id) {
                        bail!("Cloud artifact ID differs from its receipt");
                    }
                } else if state.baselines.contains_key(&filename) && direction == Direction::Push {
                    bail!("Recorded cloud artifact was removed; refusing automatic recreation");
                }
            }
            let decision = change(
                direction,
                local_hash.as_deref(),
                cloud_hash,
                state.baselines.get(&filename).map(String::as_str),
            );
            let action = match decision {
                Change::Same => "unchanged",
                Change::Transfer if direction == Direction::Push => "upload",
                Change::Transfer => "download",
                Change::OtherSide if direction == Direction::Push => "remote_changed",
                Change::OtherSide => "local_changed",
                Change::Conflict => "conflict",
            };
            let mut record = Action {
                call: name.clone(),
                file: filename.clone(),
                action: action.into(),
                conflict_copy: None,
            };
            if decision == Change::Conflict {
                let copy = conflict_target(
                    &target,
                    cloud_hash.context("Conflict has no cloud version")?,
                )?;
                record.conflict_copy = Some(copy.display().to_string());
                if !dry_run {
                    if regular(&copy)? {
                        if retention::checksum(&copy)?.as_str() != cloud_hash.unwrap() {
                            bail!("Existing conflict copy differs; refusing overwrite");
                        }
                    } else {
                        let temp = download(remote, item.unwrap(), &copy)?;
                        if copy.exists() {
                            bail!("Conflict destination appeared during download");
                        }
                        fs::rename(&temp.0, &copy)?;
                    }
                }
            } else if !dry_run && decision == Change::Transfer && direction == Direction::Push {
                let id = match state.files.get(&filename) {
                    Some(id) => id.clone(),
                    None => {
                        let id = match item.and_then(|i| i["id"].as_str()) {
                            Some(id) => id.into(),
                            None => remote.generate_id()?,
                        };
                        state.files.insert(filename.clone(), id.clone());
                        storage::atomic_write(&receipt_path, &serde_json::to_vec(&state)?)?;
                        id
                    }
                };
                if let Some(fresh) = remote.metadata(&id)? {
                    verify_parent(&fresh, &state.folder, &filename)?;
                    if fresh["md5Checksum"].as_str() != cloud_hash {
                        bail!("Cloud artifact changed during planning; retry");
                    }
                }
                if Some(retention::checksum(&target)?) != local_hash {
                    bail!("Local artifact changed during planning; retry");
                }
                remote.upload(&id, &state.folder, &target, cloud_hash)?;
                let uploaded = remote.metadata(&id)?.context("Uploaded artifact missing")?;
                if !matches_content(
                    &uploaded,
                    fs::metadata(&target)?.len(),
                    local_hash.as_ref().unwrap(),
                ) {
                    bail!("Upload checksum verification failed");
                }
                verify_parent(&uploaded, &state.folder, &filename)?;
                state
                    .baselines
                    .insert(filename.clone(), local_hash.unwrap());
            } else if !dry_run && decision == Change::Transfer {
                let item = item.context("Missing cloud artifact")?;
                let temp = download(remote, item, &target)?;
                let current = if regular(&target)? {
                    Some(retention::checksum(&target)?)
                } else {
                    None
                };
                if current != local_hash {
                    bail!("Local artifact changed during download; preserved");
                }
                fs::rename(&temp.0, &target)?;
                state.files.insert(
                    filename.clone(),
                    item["id"].as_str().context("Missing artifact ID")?.into(),
                );
                state
                    .baselines
                    .insert(filename.clone(), cloud_hash.unwrap().into());
            } else if !dry_run
                && decision == Change::Same
                && let Some(item) = item
            {
                state.files.insert(
                    filename.clone(),
                    item["id"].as_str().context("Missing artifact ID")?.into(),
                );
                state
                    .baselines
                    .insert(filename.clone(), cloud_hash.unwrap().into());
            }
            if !dry_run {
                storage::atomic_write(&receipt_path, &serde_json::to_vec(&state)?)?;
            }
            report.actions.push(record);
        }
        if text_only
            && !report
                .actions
                .iter()
                .any(|a| a.call == name && a.action == "conflict")
        {
            for filename in super::retention_sync::cleanup(remote, &paths, &mut state, dry_run)? {
                report.actions.push(Action {
                    call: name.clone(),
                    file: filename,
                    action: "remove_media".into(),
                    conflict_copy: None,
                });
            }
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::{
        cell::{Cell, RefCell},
        io::Write,
        sync::atomic::{AtomicU64, Ordering},
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Fixture {
        root: PathBuf,
        remote: Memory,
        profile: Profile,
        options: DriveOptions,
    }
    struct Memory {
        objects: RefCell<BTreeMap<String, (Value, Vec<u8>)>>,
        sequence: Cell<usize>,
        uploads: Cell<usize>,
        downloads: Cell<usize>,
        corrupt: Cell<bool>,
        fail_trash: Cell<bool>,
        scratch: PathBuf,
    }
    impl Memory {
        fn put(&self, id: &str, parent: &str, name: &str, body: &[u8]) {
            let path = self.scratch.join("hash-input");
            fs::write(&path, body).unwrap();
            let hash = retention::checksum(&path).unwrap();
            fs::remove_file(path).unwrap();
            self.objects.borrow_mut().insert(
                id.into(),
                (
                    json!({"id":id,"parents":[parent],"name":name,
                "mimeType":"text/plain","size":body.len().to_string(),"md5Checksum":hash}),
                    body.to_vec(),
                ),
            );
        }
        fn change(&self, id: &str, body: &[u8]) {
            let item = self.metadata(id).unwrap().unwrap();
            self.put(
                id,
                item["parents"][0].as_str().unwrap(),
                item["name"].as_str().unwrap(),
                body,
            );
        }
    }
    impl Remote for Memory {
        fn metadata(&self, id: &str) -> Result<Option<Value>> {
            Ok(self.objects.borrow().get(id).map(|i| i.0.clone()))
        }
        fn children(&self, parent: &str) -> Result<Vec<Value>> {
            Ok(self
                .objects
                .borrow()
                .values()
                .filter(|v| {
                    v.0["parents"]
                        .as_array()
                        .is_some_and(|p| p.contains(&json!(parent)))
                })
                .map(|v| v.0.clone())
                .collect())
        }
        fn generate_id(&self) -> Result<String> {
            self.sequence.set(self.sequence.get() + 1);
            Ok(format!("generated{}", self.sequence.get()))
        }
        fn ensure_folder(&self, id: &str, parent: &str, name: &str) -> Result<()> {
            self.objects.borrow_mut().insert(id.into(), (json!({"id":id,"parents":[parent],"name":name,"mimeType":"application/vnd.google-apps.folder"}),Vec::new()));
            Ok(())
        }
        fn upload(
            &self,
            id: &str,
            parent: &str,
            path: &Path,
            baseline: Option<&str>,
        ) -> Result<()> {
            if let Some(item) = self.metadata(id)? {
                assert_eq!(item["md5Checksum"].as_str(), baseline);
            }
            self.uploads.set(self.uploads.get() + 1);
            self.put(
                id,
                parent,
                path.file_name().unwrap().to_str().unwrap(),
                &fs::read(path)?,
            );
            Ok(())
        }
        fn read_state(&self, id: &str) -> Result<Vec<u8>> {
            Ok(self.objects.borrow()[id].1.clone())
        }
        fn trash(&self, item: &Value) -> Result<()> {
            if self.fail_trash.replace(false) {
                bail!("simulated interruption");
            }
            self.objects
                .borrow_mut()
                .remove(item["id"].as_str().unwrap());
            Ok(())
        }
        fn download(&self, id: &str, output: &mut File) -> Result<()> {
            self.downloads.set(self.downloads.get() + 1);
            let objects = self.objects.borrow();
            let bytes = &objects.get(id).unwrap().1;
            output.write_all(bytes)?;
            if self.corrupt.get() {
                output.write_all(b"corrupt")?;
            }
            Ok(())
        }
    }
    impl Fixture {
        fn new(mode: Mode) -> Self {
            let root = std::env::temp_dir().join(format!(
                "voxray-sync-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&root).unwrap();
            let remote = Memory {
                objects: RefCell::default(),
                sequence: Cell::new(0),
                uploads: Cell::new(0),
                downloads: Cell::new(0),
                corrupt: Cell::new(false),
                fail_trash: Cell::new(false),
                scratch: root.clone(),
            };
            remote.objects.borrow_mut().insert("parent".into(),(json!({"id":"parent","name":"Calls","mimeType":"application/vnd.google-apps.folder"}),Vec::new()));
            let profile = Profile {
                inbox_dir: root.join("inbox"),
                calls_dir: root.join("calls"),
                date_format: None,
                mode: Some(mode),
                keep_video: false,
                drive: DriveOptions::default(),
                modules: Vec::new(),
            };
            Self {
                root,
                remote,
                profile,
                options: DriveOptions {
                    enabled: true,
                    folder_id: Some("parent".into()),
                    calls_dir: None,
                },
            }
        }
        fn paths(&self) -> CallPaths {
            paths(&self.profile.calls_dir, "Call", self.profile.mode.unwrap()).unwrap()
        }
        fn local(&self, body: &[u8]) -> CallPaths {
            let paths = self.paths();
            fs::create_dir_all(&paths.root).unwrap();
            fs::write(&paths.transcript, body).unwrap();
            paths
        }
        fn run(&self, direction: Direction, dry: bool) -> Result<Report> {
            synchronize_with(
                &self.remote,
                &self.options,
                &self.profile,
                direction,
                dry,
                None,
            )
        }
        fn id(&self) -> String {
            let state: Receipt =
                serde_json::from_slice(&fs::read(state_path(&self.paths())).unwrap()).unwrap();
            state.files[self
                .paths()
                .transcript
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()]
            .clone()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
    #[test]
    fn transcript_only_is_synced_and_stale_devices_cannot_resurrect_media() {
        for mode in [Mode::Folder, Mode::File] {
            let f = Fixture::new(mode);
            let p = f.local(b"verified transcript");
            let audio = p.root.join(if mode == Mode::Folder {
                "record.m4a"
            } else {
                "Call.record.m4a"
            });
            fs::write(&audio, b"audio").unwrap();
            f.run(Direction::Push, false).unwrap();
            retention::remember_transcript_only(&p).unwrap();
            let uploads = f.remote.uploads.get();
            let preview = f.run(Direction::Push, true).unwrap();
            assert!(preview.actions.iter().any(|a| a.action == "remove_media"));
            assert_eq!(f.remote.uploads.get(), uploads);
            assert!(audio.exists());
            f.remote.fail_trash.set(true);
            assert!(f.run(Direction::Push, false).is_err());
            assert!(audio.exists());
            assert!(retention::sync_path(&p).exists());
            f.run(Direction::Push, false).unwrap();
            assert!(!audio.exists());
            // An offline device with stale audio and no local policy learns the intent
            // even on push, never uploads the media, and completes its cleanup.
            fs::remove_file(retention::sync_path(&p)).unwrap();
            fs::write(&audio, b"stale offline copy").unwrap();
            let uploads = f.remote.uploads.get();
            f.run(Direction::Push, false).unwrap();
            assert_eq!(f.remote.uploads.get(), uploads);
            assert!(!audio.exists());
            assert!(retention::sync_path(&p).exists());
            // A fresh device downloads text + policy and cannot recover removed media.
            fs::remove_file(&p.transcript).unwrap();
            fs::remove_file(retention::sync_path(&p)).unwrap();
            fs::remove_file(state_path(&p)).unwrap();
            f.run(Direction::Pull, false).unwrap();
            assert_eq!(fs::read(&p.transcript).unwrap(), b"verified transcript");
            assert!(retention::sync_path(&p).exists());
            assert!(!audio.exists());
            let receipt: Receipt =
                serde_json::from_slice(&fs::read(state_path(&p)).unwrap()).unwrap();
            let marker = retention::sync_path(&p);
            let marker_name = marker.file_name().unwrap().to_str().unwrap();
            f.remote
                .objects
                .borrow_mut()
                .remove(&receipt.files[marker_name]);
            fs::remove_file(marker).unwrap();
            fs::write(&audio, b"stale copy").unwrap();
            assert!(f.run(Direction::Push, false).is_err());
            assert!(audio.exists());
        }
    }

    #[test]
    fn transcript_conflict_and_unsupported_state_preserve_media() {
        let f = Fixture::new(Mode::Folder);
        let p = f.local(b"original text");
        let audio = p.root.join("record.m4a");
        fs::write(&audio, b"audio").unwrap();
        f.run(Direction::Push, false).unwrap();
        let state: Receipt = serde_json::from_slice(&fs::read(state_path(&p)).unwrap()).unwrap();
        f.remote
            .change(&state.files["transcript.txt"], b"cloud edit");
        fs::write(&p.transcript, b"local edit").unwrap();
        retention::remember_transcript_only(&p).unwrap();
        let report = f.run(Direction::Push, false).unwrap();
        assert!(report.actions.iter().any(|a| a.action == "conflict"));
        assert!(audio.exists());
        assert!(
            f.remote
                .metadata(&state.files["record.m4a"])
                .unwrap()
                .is_some()
        );
        fs::write(
            retention::sync_path(&p),
            br#"{"version":999,"retention":"transcript_only"}"#,
        )
        .unwrap();
        assert!(f.run(Direction::Push, false).is_err());
        assert!(audio.exists());
    }

    #[test]
    fn push_is_incremental_and_does_not_upload_unrelated_files() {
        let f = Fixture::new(Mode::Folder);
        let paths = f.local(b"initial transcript");
        fs::write(paths.root.join("private-token.json"), b"never upload").unwrap();
        f.run(Direction::Push, false).unwrap();
        let id = f.id();
        f.run(Direction::Push, false).unwrap();
        assert_eq!(f.remote.uploads.get(), 1);
        fs::write(&paths.transcript, b"local edit").unwrap();
        f.run(Direction::Push, false).unwrap();
        assert_eq!(f.remote.uploads.get(), 2);
        assert_eq!(f.id(), id);
        assert_eq!(f.remote.objects.borrow()[&id].1, b"local edit");
    }
    #[test]
    fn preview_has_no_local_or_remote_writes_and_generates_no_ids() {
        let f = Fixture::new(Mode::Folder);
        let paths = f.local(b"preview only");
        let before = f.remote.objects.borrow().clone();
        let plan = f.run(Direction::Push, true).unwrap();
        assert_eq!(plan.actions[0].action, "upload");
        assert_eq!(*f.remote.objects.borrow(), before);
        assert_eq!(f.remote.sequence.get(), 0);
        assert!(!state_path(&paths).exists());
        assert!(!state_path(&paths).with_extension("lock").exists());
    }
    #[test]
    fn cloud_change_is_downloaded_and_local_only_change_is_preserved() {
        let f = Fixture::new(Mode::Folder);
        let paths = f.local(b"base");
        f.run(Direction::Push, false).unwrap();
        f.remote.change(&f.id(), b"cloud edit");
        assert_eq!(
            f.run(Direction::Push, true).unwrap().actions[0].action,
            "remote_changed"
        );
        f.run(Direction::Pull, false).unwrap();
        assert_eq!(fs::read(&paths.transcript).unwrap(), b"cloud edit");
        fs::write(&paths.transcript, b"local edit").unwrap();
        let plan = f.run(Direction::Pull, false).unwrap();
        assert_eq!(plan.actions[0].action, "local_changed");
        assert_eq!(fs::read(&paths.transcript).unwrap(), b"local edit");
    }
    #[test]
    fn both_changed_saves_cloud_conflict_copy_without_overwriting_either_side() {
        let f = Fixture::new(Mode::Folder);
        let paths = f.local(b"base");
        f.run(Direction::Push, false).unwrap();
        let id = f.id();
        f.remote.change(&id, b"cloud version");
        fs::write(&paths.transcript, b"local version").unwrap();
        let result = f.run(Direction::Pull, false).unwrap();
        let action = &result.actions[0];
        assert_eq!(action.action, "conflict");
        assert_eq!(
            fs::read(action.conflict_copy.as_ref().unwrap()).unwrap(),
            b"cloud version"
        );
        assert_eq!(fs::read(&paths.transcript).unwrap(), b"local version");
        assert_eq!(f.remote.objects.borrow()[&id].1, b"cloud version");
        f.run(Direction::Push, false).unwrap();
        assert_eq!(f.remote.downloads.get(), 1);
    }
    #[test]
    fn corrupt_download_never_replaces_existing_local_data_or_leaves_temporary_files() {
        let f = Fixture::new(Mode::Folder);
        let paths = f.local(b"base");
        f.run(Direction::Push, false).unwrap();
        f.remote.change(&f.id(), b"new cloud");
        f.remote.corrupt.set(true);
        assert!(f.run(Direction::Pull, false).is_err());
        assert_eq!(fs::read(&paths.transcript).unwrap(), b"base");
        assert!(
            !fs::read_dir(&paths.root).unwrap().any(|e| e
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".tmp"))
        );
    }
    #[test]
    fn pull_discovers_call_on_fresh_machine_and_reuses_cloud_ids() {
        let f = Fixture::new(Mode::Folder);
        f.local(b"restore me");
        f.run(Direction::Push, false).unwrap();
        let id = f.id();
        let fresh = Profile {
            calls_dir: f.root.join("fresh"),
            ..f.profile.clone()
        };
        synchronize_with(&f.remote, &f.options, &fresh, Direction::Pull, true, None).unwrap();
        assert!(!fresh.calls_dir.exists());
        synchronize_with(&f.remote, &f.options, &fresh, Direction::Pull, false, None).unwrap();
        let path = paths(&fresh.calls_dir, "Call", Mode::Folder).unwrap();
        assert_eq!(fs::read(&path.transcript).unwrap(), b"restore me");
        let state: Receipt = serde_json::from_slice(&fs::read(state_path(&path)).unwrap()).unwrap();
        assert_eq!(state.files["transcript.txt"], id);
    }
    #[test]
    fn file_mode_and_transcript_only_legacy_calls_are_supported() {
        let f = Fixture::new(Mode::File);
        let paths = f.local(b"text only");
        f.run(Direction::Push, false).unwrap();
        f.remote.change(&f.id(), b"remote text");
        f.run(Direction::Pull, false).unwrap();
        assert_eq!(fs::read(&paths.transcript).unwrap(), b"remote text");
    }
    #[test]
    fn deletions_are_not_propagated() {
        let f = Fixture::new(Mode::Folder);
        let paths = f.local(b"keep cloud");
        f.run(Direction::Push, false).unwrap();
        let id = f.id();
        fs::remove_file(&paths.transcript).unwrap();
        f.run(Direction::Push, false).unwrap();
        assert!(f.remote.metadata(&id).unwrap().is_some());
        f.run(Direction::Pull, false).unwrap();
        f.remote.objects.borrow_mut().remove(&id);
        f.run(Direction::Pull, false).unwrap();
        assert!(paths.transcript.exists());
        assert!(f.run(Direction::Push, false).is_err());
    }
    #[test]
    fn unsafe_remote_names_and_symbolic_links_are_rejected_without_outside_writes() {
        let f = Fixture::new(Mode::Folder);
        f.remote.ensure_folder("evil", "parent", "..").unwrap();
        assert!(f.run(Direction::Pull, false).is_err());
        f.remote.objects.borrow_mut().remove("evil");
        f.local(b"base");
        f.run(Direction::Push, false).unwrap();
        let outside = f.root.join("outside");
        fs::create_dir(&outside).unwrap();
        fs::remove_dir_all(f.profile.calls_dir.join("Call")).unwrap();
        std::os::unix::fs::symlink(&outside, f.profile.calls_dir.join("Call")).unwrap();
        assert!(f.run(Direction::Pull, false).is_err());
        assert!(fs::read_dir(outside).unwrap().next().is_none());
    }
    #[test]
    fn duplicate_cloud_calls_are_not_silently_combined() {
        let f = Fixture::new(Mode::Folder);
        f.remote.ensure_folder("a", "parent", "Call").unwrap();
        f.remote.ensure_folder("b", "parent", "call").unwrap();
        assert!(f.run(Direction::Pull, false).is_err());
        assert!(!f.profile.calls_dir.exists());
    }
    #[test]
    fn unknown_baseline_with_different_versions_is_a_conflict() {
        assert_eq!(
            change(Direction::Push, Some("local"), Some("remote"), None),
            Change::Conflict
        );
        assert_eq!(
            change(Direction::Pull, Some("local"), Some("remote"), None),
            Change::Conflict
        );
    }
}
