//! Retention intent is synchronized before media decisions, independent of direction.
//! A missing file is not a tombstone. Only an explicit, versioned intent removes media.
use super::synchronization::{Remote, local_files, remote_files};
use super::{Receipt, matches_content, state_path, verify_parent};
use crate::{call::CallPaths, retention, storage};
use anyhow::{Context, Result, bail};
use std::{
    fs::{self, File},
    path::Path,
};

pub(super) fn reconcile(
    remote: &impl Remote,
    paths: &CallPaths,
    state: &mut Receipt,
    dry: bool,
) -> Result<bool> {
    let path = retention::sync_path(paths);
    let name = path
        .file_name()
        .unwrap()
        .to_str()
        .context("Invalid state filename")?;
    let local = local_files(paths)?.contains_key(name);
    if local {
        retention::validate_sync_state(&fs::read(&path)?)?;
    }
    let files = remote_files(remote, paths, &state.folder)?;
    let cloud = files.get(name);
    if !local && cloud.is_none() && state.files.contains_key(name) {
        bail!("Recorded retention intent is missing; restore it before synchronizing");
    }
    if let Some(item) = cloud {
        let id = item["id"].as_str().context("Missing sync state ID")?;
        if state.files.get(name).is_some_and(|known| known != id) {
            bail!("Call synchronization state identity changed");
        }
        let bytes = remote.read_state(id)?;
        retention::validate_sync_state(&bytes)?;
        if item["size"].as_str().and_then(|s| s.parse::<usize>().ok()) != Some(bytes.len()) {
            bail!("Call synchronization state size mismatch");
        }
        if !dry {
            // The state is monotonic: a stale device must accept the deletion intent
            // even during push, before it can consider uploading its old media.
            if !local {
                let temp = super::synchronization::download(remote, item, &path)?;
                retention::validate_sync_state(&fs::read(&temp.0)?)?;
                fs::rename(&temp.0, &path)?;
            }
            state.files.insert(name.into(), id.into());
            state.baselines.insert(
                name.into(),
                item["md5Checksum"]
                    .as_str()
                    .context("Missing state checksum")?
                    .into(),
            );
            storage::atomic_write(&state_path(paths), &serde_json::to_vec(state)?)?;
        }
    } else if local && !dry {
        let id = match state.files.get(name) {
            Some(_) if state.baselines.contains_key(name) => bail!(
                "Synced retention state was removed remotely; restore it before synchronizing"
            ),
            Some(id) => id.clone(),
            None => {
                let id = remote.generate_id()?;
                state.files.insert(name.into(), id.clone());
                storage::atomic_write(&state_path(paths), &serde_json::to_vec(state)?)?;
                id
            }
        };
        // Upload may have succeeded before its response was lost. Recover the same ID.
        let existing = remote.metadata(&id)?;
        if let Some(item) = &existing {
            verify_parent(item, &state.folder, name)?;
        }
        remote.upload(
            &id,
            &state.folder,
            &path,
            existing.as_ref().and_then(|v| v["md5Checksum"].as_str()),
        )?;
        let item = remote
            .metadata(&id)?
            .context("Retention state upload missing")?;
        verify_parent(&item, &state.folder, name)?;
        let hash = retention::checksum(&path)?;
        if !matches_content(&item, fs::metadata(&path)?.len(), &hash) {
            bail!("Retention state upload verification failed");
        }
        state.baselines.insert(name.into(), hash);
        storage::atomic_write(&state_path(paths), &serde_json::to_vec(state)?)?;
    }
    Ok(local || cloud.is_some())
}

pub(super) fn cleanup(
    remote: &impl Remote,
    paths: &CallPaths,
    state: &mut Receipt,
    dry: bool,
) -> Result<Vec<String>> {
    let local = local_files(paths)?;
    let cloud = remote_files(remote, paths, &state.folder)?;
    let media: std::collections::BTreeSet<_> = local
        .keys()
        .chain(cloud.keys())
        .filter(|name| crate::media::is_media_file(Path::new(name)))
        .cloned()
        .collect();
    if media.is_empty() {
        return Ok(Vec::new());
    }
    if dry {
        return Ok(media.into_iter().collect());
    }
    // No deletion until the same nonempty transcript is durably present on both sides.
    retention::validate_text(&paths.transcript)?;
    let name = paths.transcript.file_name().unwrap().to_str().unwrap();
    let transcript = cloud
        .get(name)
        .context("Cloud transcript is missing; media preserved")?;
    let hash = retention::checksum(&paths.transcript)?;
    if !matches_content(transcript, fs::metadata(&paths.transcript)?.len(), &hash) {
        bail!("Transcript is not synchronized; media preserved");
    }
    // Persist intent on disk before destructive work, including intent received from Drive.
    File::open(retention::sync_path(paths))?.sync_all()?;
    File::open(&paths.root)?.sync_all()?;
    let mut removed = Vec::new();
    for name in media {
        let path = paths.root.join(&name);
        let local_hash = if local.contains_key(&name) {
            Some(retention::checksum(&path)?)
        } else {
            None
        };
        if local_hash.as_ref().is_some_and(|hash| {
            state
                .baselines
                .get(&name)
                .is_some_and(|baseline| baseline != hash)
        }) {
            bail!("Local media changed since last synchronization; preserved");
        }
        if let Some(item) = cloud.get(&name) {
            // Never delete a different remote object merely because it shares a name.
            if state
                .files
                .get(&name)
                .is_some_and(|id| item["id"].as_str() != Some(id))
            {
                bail!("Cloud media identity changed; preserved");
            }
            if state
                .baselines
                .get(&name)
                .is_some_and(|hash| item["md5Checksum"].as_str() != Some(hash))
            {
                bail!("Cloud media changed since last synchronization; preserved");
            }
            remote.trash(item)?;
        }
        if let Some(hash) = local_hash {
            if retention::checksum(&path)? != hash {
                bail!("Local media changed during cleanup; preserved");
            }
            fs::remove_file(&path)?;
            File::open(&paths.root)?.sync_all()?;
        }
        // Retention intent survives receipt cleanup and prevents future resurrection.
        state.files.remove(&name);
        state.baselines.remove(&name);
        storage::atomic_write(&state_path(paths), &serde_json::to_vec(state)?)?;
        removed.push(name);
    }
    Ok(removed)
}
