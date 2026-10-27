//! Personal Google Drive uploads. No credentials are needed until upload is requested.
use crate::{
    call::CallPaths,
    config::{DriveOptions, GoogleDriveConfig, Mode},
    retention, storage,
};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub fn validate_id(id: &str) -> Result<()> {
    if id.is_empty()
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        bail!(
            "Google Drive folder must be a folder ID (letters, digits, _ or -), not a path or URL"
        );
    }
    Ok(())
}

#[derive(Deserialize)]
struct Credentials {
    client_id: String,
    #[serde(default)]
    client_secret: String,
    refresh_token: String,
}

struct Client {
    agent: ureq::Agent,
    token: String,
    api: String,
    upload: String,
}

impl Client {
    fn connect(config: &GoogleDriveConfig) -> Result<Self> {
        let path = config
            .credentials_file
            .clone()
            .unwrap_or(crate::config::app_home()?.join("google-drive-oauth.json"));
        let metadata = fs::metadata(&path).context("Google Drive credentials are not configured; provide a private google-drive-oauth.json with personal OAuth credentials (client_id, client_secret, refresh_token). Service accounts cannot upload to personal Drive")?;
        if metadata.permissions().mode() & 0o077 != 0 {
            bail!("Google Drive credentials file must be private (chmod 600)");
        }
        let credentials: Credentials = serde_json::from_slice(&fs::read(&path)?)
            .map_err(|_| anyhow::anyhow!("Google Drive credentials must contain client_id, client_secret and refresh_token; service accounts are unsupported for personal Drive"))?;
        if credentials.client_id.is_empty() || credentials.refresh_token.is_empty() {
            bail!("Incomplete Google Drive OAuth credentials");
        }
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(300))
            .redirects(0)
            .build();
        let response: Value = agent
            .post("https://oauth2.googleapis.com/token")
            .send_form(&[
                ("grant_type", "refresh_token"),
                ("client_id", &credentials.client_id),
                ("client_secret", &credentials.client_secret),
                ("refresh_token", &credentials.refresh_token),
            ])
            .map_err(|_| {
                anyhow::anyhow!(
                    "Google Drive authorization failed; reconnect the personal Google account"
                )
            })?
            .into_json()?;
        let token = response["access_token"]
            .as_str()
            .filter(|s| !s.is_empty())
            .context("Google returned no access token")?
            .to_owned();
        Ok(Self {
            agent,
            token,
            api: "https://www.googleapis.com/drive/v3".into(),
            upload: "https://www.googleapis.com/upload/drive/v3".into(),
        })
    }
    fn request(&self, method: &str, url: &str) -> ureq::Request {
        self.agent
            .request(method, url)
            .set("Authorization", &format!("Bearer {}", self.token))
    }
    fn metadata(&self, id: &str) -> Result<Option<Value>> {
        match self
            .request("GET", &format!("{}/files/{id}", self.api))
            .query(
                "fields",
                "id,name,mimeType,parents,size,md5Checksum,trashed,capabilities(canAddChildren)",
            )
            .call()
        {
            Ok(response) => Ok(Some(response.into_json()?)),
            Err(ureq::Error::Status(404, _)) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }
    fn generate_id(&self) -> Result<String> {
        let value: Value = self
            .request("GET", &format!("{}/files/generateIds", self.api))
            .query("count", "1")
            .query("space", "drive")
            .query("type", "files")
            .call()?
            .into_json()?;
        let id = value["ids"][0]
            .as_str()
            .context("Drive returned no file ID")?
            .to_owned();
        validate_id(&id)?;
        Ok(id)
    }
    fn ensure_folder(&self, id: &str, parent: &str, name: &str) -> Result<()> {
        if self.metadata(id)?.is_none() {
            // A lost response is recovered by GET on the same preallocated ID.
            self.request("POST", &format!("{}/files", self.api)).send_json(json!({
                "id": id, "name": name, "parents": [parent], "mimeType": "application/vnd.google-apps.folder"
            }))?;
        }
        let value = self.metadata(id)?.context("Uploaded folder is missing")?;
        verify_parent(&value, parent, name)?;
        if value["mimeType"] != "application/vnd.google-apps.folder" {
            bail!("Drive destination is not a folder");
        }
        Ok(())
    }
    fn upload_file(&self, id: &str, parent: &str, path: &Path) -> Result<()> {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .context("Artifact has no UTF-8 name")?;
        let digest = retention::checksum(path)?;
        let size = fs::metadata(path)?.len();
        let existing = self.metadata(id)?;
        if let Some(value) = &existing {
            verify_parent(value, parent, name)?;
            if matches_content(value, size, &digest) {
                return Ok(());
            }
        }
        let url = match existing {
            Some(_) => format!("{}/files/{id}", self.upload),
            None => format!("{}/files", self.upload),
        };
        let metadata = if existing.is_some() {
            json!({"name": name})
        } else {
            json!({"id": id, "name": name, "parents": [parent]})
        };
        let response = self
            .request(if existing.is_some() { "PATCH" } else { "POST" }, &url)
            .query("uploadType", "resumable")
            .set("X-Upload-Content-Length", &size.to_string())
            .set("X-Upload-Content-Type", "application/octet-stream")
            .send_json(metadata)?;
        let location = response
            .header("Location")
            .context("Drive returned no upload session")?;
        // Never forward bearer credentials to an untrusted redirect/session host.
        if !location.starts_with(&format!("{}/", self.upload)) {
            bail!("Unexpected Google Drive upload session URL");
        }
        self.request("PUT", location)
            .set("Content-Length", &size.to_string())
            .set("Content-Type", "application/octet-stream")
            .send(File::open(path)?)?;
        let value = self.metadata(id)?.context("Uploaded artifact is missing")?;
        verify_parent(&value, parent, name)?;
        if !matches_content(&value, size, &digest) || retention::checksum(path)? != digest {
            bail!("Google Drive verification failed; local media is preserved");
        }
        Ok(())
    }
}

fn verify_parent(value: &Value, parent: &str, name: &str) -> Result<()> {
    if value["trashed"] == true
        || value["name"] != name
        || !value["parents"]
            .as_array()
            .is_some_and(|p| p.iter().any(|p| p == parent))
    {
        bail!("Google Drive artifact was moved, renamed or trashed; refusing to overwrite it");
    }
    Ok(())
}
fn matches_content(value: &Value, size: u64, digest: &str) -> bool {
    value["size"].as_str().and_then(|s| s.parse::<u64>().ok()) == Some(size)
        && value["md5Checksum"] == digest
}

#[derive(Default, Serialize, Deserialize)]
struct Receipt {
    parent: String,
    folder: String,
    files: BTreeMap<String, String>,
}

fn state_path(paths: &CallPaths) -> PathBuf {
    paths.root.join(match paths.mode {
        Mode::Folder => ".voxray-drive.json".to_owned(),
        Mode::File => format!(".{}.drive.json", paths.name),
    })
}

pub fn preflight(options: &DriveOptions) -> Result<()> {
    if options.enabled {
        validate_id(
            options
                .folder_id
                .as_deref()
                .context("Google Drive is enabled but no folder_id is configured")?,
        )?;
    }
    Ok(())
}

pub fn sync(
    config: &GoogleDriveConfig,
    options: &DriveOptions,
    paths: &CallPaths,
    recording: Option<&Path>,
    transcript_only: bool,
) -> Result<Option<String>> {
    if !options.enabled {
        return Ok(None);
    }
    preflight(options)?;
    if transcript_only {
        retention::validate_text(&paths.transcript)?;
    }
    let client = Client::connect(config)?;
    sync_with(&client, options, paths, recording, transcript_only).map(Some)
}

fn sync_with(
    client: &Client,
    options: &DriveOptions,
    paths: &CallPaths,
    recording: Option<&Path>,
    transcript_only: bool,
) -> Result<String> {
    let parent = options
        .folder_id
        .as_deref()
        .context("Missing Drive folder")?;
    let destination = client
        .metadata(parent)?
        .context("Google Drive parent folder is unavailable")?;
    if destination["mimeType"] != "application/vnd.google-apps.folder"
        || destination["trashed"] == true
        || destination["capabilities"]["canAddChildren"] != true
    {
        bail!("Google Drive parent must be a writable folder");
    }
    let state_path = state_path(paths);
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .mode(0o600)
        .open(state_path.with_extension("lock"))?;
    lock.try_lock().context("This call is already uploading")?;
    let mut state: Receipt = if state_path.exists() {
        serde_json::from_slice(&fs::read(&state_path)?)
            .context("Drive upload receipt is damaged; refusing to create duplicate uploads")?
    } else {
        Receipt::default()
    };
    // Keep separate receipts per destination so an intentional route change is explicit.
    if !state.parent.is_empty() && state.parent != parent {
        bail!(
            "Call was already routed to a different Drive folder; preserve its receipt before deliberately uploading another copy"
        );
    }
    if state.folder.is_empty() {
        state.parent = parent.to_owned();
        state.folder = client.generate_id()?;
        storage::atomic_write(&state_path, &serde_json::to_vec(&state)?)?;
    }
    client.ensure_folder(&state.folder, parent, &paths.name)?;
    let mut artifacts = vec![
        paths.transcript.clone(),
        paths.feedback.clone(),
        paths.manifest.clone(),
    ];
    if !transcript_only && let Some(recording) = recording {
        artifacts.extend(retention::media_paths(recording, paths)?);
    }
    for path in artifacts.into_iter().filter(|p| p.is_file()) {
        if fs::symlink_metadata(&path)?.file_type().is_symlink() {
            bail!("Refusing to upload symbolic links");
        }
        let name = path
            .file_name()
            .context("Missing artifact name")?
            .to_string_lossy()
            .into_owned();
        let id = match state.files.get(&name) {
            Some(id) => id.clone(),
            None => {
                let id = client.generate_id()?;
                state.files.insert(name, id.clone());
                storage::atomic_write(&state_path, &serde_json::to_vec(&state)?)?;
                id
            }
        };
        client.upload_file(&id, &state.folder, &path)?;
    }
    Ok(format!(
        "https://drive.google.com/drive/folders/{}",
        state.folder
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    // Scripted real HTTP transport; assertions exercise the requests sent to Drive.
    fn server(steps: Vec<(&'static str, u16, Value)>) -> (String, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let location = format!("{base}/upload/session");
        let handle = std::thread::spawn(move || {
            for (expected, status, body) in steps {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut byte = [0];
                while !bytes.ends_with(b"\r\n\r\n") {
                    stream.read_exact(&mut byte).unwrap();
                    bytes.push(byte[0]);
                }
                let headers = String::from_utf8(bytes).unwrap();
                assert!(
                    headers.starts_with(expected),
                    "expected {expected}, got {headers}"
                );
                assert!(
                    headers
                        .to_lowercase()
                        .contains("authorization: bearer test-token")
                );
                let length = headers
                    .lines()
                    .find_map(|l| {
                        l.to_lowercase()
                            .strip_prefix("content-length:")
                            .map(|n| n.trim().parse::<usize>().unwrap())
                    })
                    .unwrap_or(0);
                let mut payload = vec![0; length];
                stream.read_exact(&mut payload).unwrap();
                if expected.starts_with("PUT") {
                    assert_eq!(payload, b"Speaker 1: hello");
                }
                let encoded = body.to_string();
                write!(stream, "HTTP/1.1 {status} Response\r\nContent-Type: application/json\r\nContent-Length: {}\r\nLocation: {location}\r\nConnection: close\r\n\r\n{encoded}", encoded.len()).unwrap();
            }
        });
        (base, handle)
    }

    fn parent() -> Value {
        json!({"mimeType":"application/vnd.google-apps.folder", "capabilities":{"canAddChildren":true}})
    }
    fn folder() -> Value {
        json!({"name":"Call", "mimeType":"application/vnd.google-apps.folder", "parents":["parent"]})
    }
    fn file() -> Value {
        json!({"name":"transcript.txt", "parents":["folder1"], "size":"16", "md5Checksum":"bad"})
    }

    #[test]
    fn retries_lost_upload_response_using_persisted_ids_and_verifies_content() {
        let root = std::env::temp_dir().join(format!("voxray-drive-test-{}", std::process::id()));
        let call = root.join("Call");
        fs::create_dir_all(&call).unwrap();
        let paths = CallPaths::from_recording(&call.join("record.wav")).unwrap();
        fs::write(&paths.transcript, b"Speaker 1: hello").unwrap();
        let mut verified = file();
        verified["size"] = json!(16); // Wrong type is intentionally not accepted.
        assert!(!matches_content(&verified, 16, "bad"));
        verified["size"] = json!("16");
        verified["md5Checksum"] = json!(retention::checksum(&paths.transcript).unwrap());
        let steps = vec![
            ("GET /api/files/parent?", 200, parent()),
            (
                "GET /api/files/generateIds?",
                200,
                json!({"ids":["folder1"]}),
            ),
            ("GET /api/files/folder1?", 404, json!({})),
            ("POST /api/files ", 200, json!({})),
            ("GET /api/files/folder1?", 200, folder()),
            ("GET /api/files/generateIds?", 200, json!({"ids":["file1"]})),
            ("GET /api/files/file1?", 404, json!({})),
            ("POST /upload/files?", 200, json!({})),
            ("PUT /upload/session ", 500, json!({})),
            ("GET /api/files/parent?", 200, parent()),
            ("GET /api/files/folder1?", 200, folder()),
            ("GET /api/files/folder1?", 200, folder()),
            ("GET /api/files/file1?", 200, verified),
        ];
        let (base, handle) = server(steps);
        let client = Client {
            agent: ureq::AgentBuilder::new()
                .timeout(Duration::from_secs(5))
                .build(),
            token: "test-token".into(),
            api: format!("{base}/api"),
            upload: format!("{base}/upload"),
        };
        let options = DriveOptions {
            enabled: true,
            folder_id: Some("parent".into()),
        };
        assert!(sync_with(&client, &options, &paths, None, true).is_err());
        let receipt = fs::read(state_path(&paths)).unwrap();
        let url = sync_with(&client, &options, &paths, None, true).unwrap();
        assert!(url.ends_with("/folder1"));
        assert_eq!(fs::read(state_path(&paths)).unwrap(), receipt);
        handle.join().unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_destination_and_moved_artifacts_are_rejected() {
        for value in [
            "",
            "https://drive.google.com/folders/abc",
            "../folder",
            "a'b",
        ] {
            assert!(validate_id(value).is_err());
        }
        assert!(validate_id("abc_123-XYZ").is_ok());
        assert!(verify_parent(&folder(), "other-parent", "Call").is_err());
        let mut trashed = folder();
        trashed["trashed"] = json!(true);
        assert!(verify_parent(&trashed, "parent", "Call").is_err());
    }
}
