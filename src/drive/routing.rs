//! Resolve the existing local library path beneath My Drive without configured IDs.
use super::{CallLock, Client};
use crate::config::DriveOptions;
use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::fs::OpenOptions;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Component, Path};

pub fn mirror_path(path: &Path) -> Result<Vec<String>> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let home = dirs::home_dir().context("Cannot locate the user home directory")?;
    let documents = dirs::document_dir().unwrap_or_else(|| home.join("Documents"));
    let relative = absolute
        .strip_prefix(&documents)
        .ok()
        .filter(|p| !p.as_os_str().is_empty())
        .or_else(|| {
            absolute
                .strip_prefix(&home)
                .ok()
                .filter(|p| !p.as_os_str().is_empty())
        })
        .unwrap_or(&absolute);
    let mut parts = Vec::new();
    for component in relative.components() {
        match component {
            Component::RootDir | Component::CurDir => {}
            Component::Normal(name) => {
                let name = name
                    .to_str()
                    .context("Drive mirror path must be valid UTF-8")?;
                if name.chars().any(char::is_control) {
                    bail!("Invalid Drive mirror path component");
                }
                parts.push(name.to_owned());
            }
            _ => bail!("Drive mirror path cannot contain parent traversal"),
        }
    }
    if parts.is_empty() {
        bail!("Drive mirror needs a named local directory");
    }
    Ok(parts)
}

trait Folders {
    fn find(&self, parent: &str, name: &str) -> Result<Vec<String>>;
    fn ensure(&self, parent: &str, name: &str) -> Result<String>;
}

fn unique(ids: Vec<String>) -> Result<Option<String>> {
    if ids.len() > 1 {
        bail!("Ambiguous duplicate Drive mirror folder; resolve duplicate names before retrying");
    }
    Ok(ids.into_iter().next())
}

fn resolve(store: &impl Folders, parts: &[String], create: bool) -> Result<Option<String>> {
    let mut parent = "root".to_owned();
    for name in parts {
        parent = match unique(store.find(&parent, name)?)? {
            Some(id) => id,
            None if create => store.ensure(&parent, name)?,
            None => return Ok(None),
        };
    }
    Ok(Some(parent))
}

impl Folders for Client {
    fn find(&self, parent: &str, name: &str) -> Result<Vec<String>> {
        super::validate_id(parent)?;
        let escaped = name.replace('\\', "\\\\").replace('\'', "\\'");
        let query = format!(
            "'{parent}' in parents and trashed=false and name='{escaped}' and mimeType='application/vnd.google-apps.folder'"
        );
        let mut ids = Vec::new();
        let mut page = String::new();
        loop {
            let mut request = self
                .request("GET", &format!("{}/files", self.api))
                .query("q", &query)
                .query("fields", "nextPageToken,files(id)")
                .query("pageSize", "1000");
            if !page.is_empty() {
                request = request.query("pageToken", &page);
            }
            let response: Value = request.call()?.into_json()?;
            for item in response["files"]
                .as_array()
                .context("Invalid Drive mirror listing")?
            {
                ids.push(
                    item["id"]
                        .as_str()
                        .context("Drive folder has no identity")?
                        .to_owned(),
                );
            }
            match response["nextPageToken"].as_str() {
                Some(next) if !next.is_empty() => page = next.into(),
                _ => break,
            }
        }
        Ok(ids)
    }

    fn ensure(&self, parent: &str, name: &str) -> Result<String> {
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .mode(0o600)
            .open(crate::config::app_home()?.join("drive-routing.lock"))?;
        lock.lock()?;
        let _lock = CallLock(lock);
        if let Some(id) = unique(self.find(parent, name)?)? {
            return Ok(id);
        }
        let id = self.generate_id()?;
        self.ensure_folder(&id, parent, name)?;
        if unique(self.find(parent, name)?)?.as_deref() != Some(&id) {
            bail!(
                "Drive mirror folder changed while being created; retry after resolving duplicates"
            );
        }
        Ok(id)
    }
}

impl Client {
    pub(super) fn destination(
        &self,
        options: &DriveOptions,
        create: bool,
    ) -> Result<Option<String>> {
        match &options.calls_dir {
            Some(path) => resolve(self, &mirror_path(path)?, create),
            // Accepted pre-mirror jobs retain their frozen destination.
            None => Ok(Some(
                options.folder_id.clone().unwrap_or_else(|| "root".into()),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    #[derive(Default)]
    struct Memory(RefCell<BTreeMap<(String, String), Vec<String>>>);
    impl Folders for Memory {
        fn find(&self, parent: &str, name: &str) -> Result<Vec<String>> {
            Ok(self
                .0
                .borrow()
                .get(&(parent.into(), name.into()))
                .cloned()
                .unwrap_or_default())
        }
        fn ensure(&self, parent: &str, name: &str) -> Result<String> {
            let id = format!("id{}", self.0.borrow().len());
            self.0
                .borrow_mut()
                .insert((parent.into(), name.into()), vec![id.clone()]);
            Ok(id)
        }
    }
    #[test]
    fn mirrors_local_library_names_and_nested_paths_literally() {
        let documents =
            dirs::document_dir().unwrap_or_else(|| dirs::home_dir().unwrap().join("Documents"));
        assert_eq!(
            mirror_path(&documents.join("Calls/26-10 Innowise/Internal/Internal Interviews"))
                .unwrap(),
            vec!["Calls", "26-10 Innowise", "Internal", "Internal Interviews"]
        );
        assert_eq!(
            mirror_path(&documents.join("Calls/26-10 Aveni")).unwrap(),
            vec!["Calls", "26-10 Aveni"]
        );
        assert!(mirror_path(&documents.join("Calls/../Elsewhere")).is_err());
    }
    #[test]
    fn preview_does_not_create_and_repeated_resolution_reuses_folders() {
        let store = Memory::default();
        let path = vec!["Calls".into(), "26-10 Aveni".into()];
        assert_eq!(resolve(&store, &path, false).unwrap(), None);
        assert!(store.0.borrow().is_empty());
        let first = resolve(&store, &path, true).unwrap();
        assert_eq!(resolve(&store, &path, true).unwrap(), first);
        assert_eq!(store.0.borrow().len(), 2);
        store.0.borrow_mut().insert(
            ("root".into(), "Calls".into()),
            vec!["one".into(), "two".into()],
        );
        assert!(resolve(&store, &path, true).is_err());
    }
}
