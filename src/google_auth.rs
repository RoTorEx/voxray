//! Device-local Google OAuth. Authorization URLs, codes and tokens never enter logs.
use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use url::Url;

use crate::{
    cli::GoogleAuthCommand,
    config::{self, Config, GoogleDriveConfig},
};

const DRIVE_SCOPE: &str = "https://www.googleapis.com/auth/drive";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
// Three minutes allows account selection/consent while bounding the local listener.
const LOGIN_TIMEOUT: Duration = Duration::from_secs(180);

#[derive(Deserialize)]
struct DesktopFile {
    installed: DesktopClient,
}

#[derive(Deserialize)]
struct DesktopClient {
    client_id: String,
    client_secret: String,
}

#[derive(Serialize, Deserialize)]
struct Credentials {
    client_id: String,
    #[serde(default)]
    client_secret: String,
    refresh_token: String,
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(20))
        .redirects(0)
        .build()
}

fn drive_config() -> Result<GoogleDriveConfig> {
    if !Config::path()?.try_exists()? {
        return Ok(GoogleDriveConfig::default());
    }
    Ok(Config::load()?.google_drive)
}

fn credentials_path(config: &GoogleDriveConfig) -> Result<PathBuf> {
    Ok(config
        .credentials_file
        .clone()
        .unwrap_or(config::app_home()?.join("google-drive-oauth.json")))
}

fn read_credentials(path: &Path) -> Result<Option<Credentials>> {
    let metadata = match fs::metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => bail!("Cannot read Google credentials file"),
    };
    if !metadata.is_file() || metadata.permissions().mode() & 0o077 != 0 {
        bail!("Google credentials file must be private (chmod 600)");
    }
    let data = fs::read(path).context("Cannot read Google credentials file")?;
    let credentials: Credentials = serde_json::from_slice(&data)
        .map_err(|_| anyhow::anyhow!("Invalid Google credentials; run voxray google-auth login"))?;
    if credentials.client_id.is_empty() || credentials.refresh_token.is_empty() {
        bail!("Incomplete Google credentials; run voxray google-auth login");
    }
    Ok(Some(credentials))
}

fn desktop_client() -> Result<DesktopClient> {
    let path = config::app_home()?.join("google-drive-client.json");
    let raw = if path.try_exists()? {
        fs::read_to_string(path).context("Cannot read Google desktop client configuration")?
    } else {
        include_str!(concat!(env!("OUT_DIR"), "/google-client.json")).into()
    };
    let file: DesktopFile = serde_json::from_str(&raw).map_err(|_| {
        anyhow::anyhow!(
            "This build has no Google Desktop OAuth client; install an official configured build"
        )
    })?;
    if file.installed.client_id.is_empty() || file.installed.client_secret.is_empty() {
        bail!("Incomplete Google Desktop OAuth client configuration");
    }
    Ok(file.installed)
}

fn random_string() -> Result<String> {
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes)
        .map_err(|_| anyhow::anyhow!("Secure randomness unavailable"))?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

fn authorization_url(client_id: &str, redirect: &str, state: &str, verifier: &str) -> Url {
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let mut url =
        Url::parse("https://accounts.google.com/o/oauth2/v2/auth").expect("Google OAuth URL");
    url.query_pairs_mut().extend_pairs([
        ("client_id", client_id),
        ("redirect_uri", redirect),
        ("response_type", "code"),
        ("scope", DRIVE_SCOPE),
        ("access_type", "offline"),
        ("prompt", "consent select_account"),
        ("state", state),
        ("code_challenge", &challenge),
        ("code_challenge_method", "S256"),
    ]);
    url
}

fn callback_code(target: &str, expected_state: &str) -> Result<String> {
    let url = Url::parse(&format!("http://127.0.0.1{target}"))
        .map_err(|_| anyhow::anyhow!("Invalid Google callback"))?;
    if !target.starts_with("/callback?") || url.path() != "/callback" {
        bail!("Unexpected Google callback path");
    }
    let pairs: Vec<_> = url.query_pairs().collect();
    let states: Vec<_> = pairs.iter().filter(|(k, _)| k == "state").collect();
    if states.len() != 1 || states[0].1 != expected_state {
        bail!("Google callback state mismatch; existing credentials were preserved");
    }
    if pairs.iter().any(|(k, _)| k == "error") {
        bail!("Google login was declined; existing credentials were preserved");
    }
    let codes: Vec<_> = pairs
        .iter()
        .filter(|(k, v)| k == "code" && !v.is_empty())
        .collect();
    if codes.len() != 1 {
        bail!("Google returned no authorization code");
    }
    Ok(codes[0].1.to_string())
}

fn read_callback(stream: &mut TcpStream, state: &str) -> Result<String> {
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let mut request = Vec::new();
    let mut buffer = [0u8; 1024];
    while request.len() < 16384 {
        let n = stream
            .read(&mut buffer)
            .context("Cannot read browser callback")?;
        if n == 0 {
            break;
        }
        request.extend_from_slice(&buffer[..n]);
        if request.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }
    let line = std::str::from_utf8(&request)
        .unwrap_or("")
        .lines()
        .next()
        .unwrap_or("");
    let parts: Vec<_> = line.split_whitespace().collect();
    if parts.len() != 3 || parts[0] != "GET" {
        bail!("Invalid browser callback request");
    }
    callback_code(parts[1], state)
}

fn wait_for_code(listener: &TcpListener, state: &str, timeout: Duration) -> Result<String> {
    listener.set_nonblocking(true)?;
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        match listener.accept() {
            Ok((mut stream, _)) => {
                let result = read_callback(&mut stream, state);
                let (status, message) = if result.is_ok() {
                    (
                        "200 OK",
                        "Google responded. Return to Voxray to check the connection result.",
                    )
                } else {
                    (
                        "400 Bad Request",
                        "Authorization was not accepted. Return to Voxray.",
                    )
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=utf-8\r\nCache-Control: no-store\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{message}",
                    message.len()
                );
                return result;
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(50))
            }
            Err(_) => bail!("Cannot receive browser authorization"),
        }
    }
    bail!("Google login timed out after 180 seconds; existing credentials were preserved")
}

fn token_request(agent: &ureq::Agent, endpoint: &str, fields: &[(&str, &str)]) -> Result<Value> {
    // HTTP errors can contain authorization URLs or token response bodies: never propagate them.
    agent
        .post(endpoint)
        .send_form(fields)
        .map_err(|_| {
            anyhow::anyhow!("Google token request failed; retry voxray google-auth login")
        })?
        .into_json()
        .map_err(|_| anyhow::anyhow!("Invalid Google token response"))
}

fn access_token(response: &Value) -> Result<&str> {
    response["access_token"]
        .as_str()
        .filter(|s| !s.is_empty())
        .context("Google returned no access token")
}

fn identity(agent: &ureq::Agent, token: &str, config: &GoogleDriveConfig) -> Result<Value> {
    let bearer = format!("Bearer {token}");
    let user: Value = agent
        .get("https://www.googleapis.com/drive/v3/about")
        .query("fields", "user(emailAddress)")
        .set("Authorization", &bearer)
        .call()
        .map_err(|_| anyhow::anyhow!("Cannot verify Google Drive account"))?
        .into_json()
        .map_err(|_| anyhow::anyhow!("Invalid Google Drive account response"))?;
    let account = user["user"]["emailAddress"]
        .as_str()
        .context("Google returned no account email")?;
    let folder = config.folder_id.as_deref().unwrap_or("root");
    crate::drive::validate_id(folder)?;
    let destination: Value = agent
        .get(&format!(
            "https://www.googleapis.com/drive/v3/files/{folder}"
        ))
        .query(
            "fields",
            "name,mimeType,trashed,capabilities(canAddChildren)",
        )
        .set("Authorization", &bearer)
        .call()
        .map_err(|_| {
            anyhow::anyhow!("Configured Google Drive folder is unavailable to this account")
        })?
        .into_json()
        .map_err(|_| anyhow::anyhow!("Invalid Google Drive folder response"))?;
    if destination["mimeType"] != "application/vnd.google-apps.folder"
        || destination["trashed"] == true
        || destination["capabilities"]["canAddChildren"] != true
    {
        bail!("Configured Google Drive destination is not a writable folder");
    }
    Ok(json!({"connected":true,"account":account,"folder":destination["name"],"folder_id":folder}))
}

fn save_credentials(path: &Path, credentials: &Credentials) -> Result<()> {
    let parent = path
        .parent()
        .context("Google credentials path has no parent")?;
    fs::create_dir_all(parent)?;
    let temp = parent.join(format!(".google-oauth-{}.tmp", random_string()?));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temp)?;
        file.write_all(&serde_json::to_vec(credentials)?)?;
        file.sync_all()?;
        fs::rename(&temp, path)?;
        File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}

pub fn run(command: &GoogleAuthCommand, non_interactive: bool) -> Result<Value> {
    let config = drive_config()?;
    let path = credentials_path(&config)?;
    let home = config::app_home()?;
    fs::create_dir_all(&home)?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(home.join(".google-auth.lock"))?;
    lock.try_lock_exclusive()
        .context("Another google-auth command is running")?;
    match command {
        GoogleAuthCommand::Logout => {
            match fs::remove_file(&path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => bail!("Cannot remove this computer's Google credentials"),
            }
            Ok(
                json!({"connected":false,"message":"Local Google access removed; other computers remain connected"}),
            )
        }
        GoogleAuthCommand::Status => {
            let Some(c) = read_credentials(&path)? else {
                return Ok(
                    json!({"connected":false,"message":"Not connected; run voxray google-auth login"}),
                );
            };
            let agent = agent();
            let token = token_request(
                &agent,
                TOKEN_URL,
                &[
                    ("grant_type", "refresh_token"),
                    ("client_id", &c.client_id),
                    ("client_secret", &c.client_secret),
                    ("refresh_token", &c.refresh_token),
                ],
            )?;
            identity(&agent, access_token(&token)?, &config)
        }
        GoogleAuthCommand::Login => {
            if non_interactive {
                bail!("google-auth login requires browser interaction; omit --non-interactive");
            }
            let client = desktop_client()?;
            let listener = TcpListener::bind("127.0.0.1:0")
                .context("Cannot open local OAuth callback listener")?;
            let redirect = format!(
                "http://127.0.0.1:{}/callback",
                listener.local_addr()?.port()
            );
            let state = random_string()?;
            let verifier = random_string()?;
            let url = authorization_url(&client.client_id, &redirect, &state, &verifier);
            eprintln!(
                "Opening Google consent in your browser. Choose your account and review the requested Drive access."
            );
            let opened = Command::new("open")
                .arg(url.as_str())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .context("Cannot open your browser")?;
            if !opened.success() {
                bail!("Cannot open your browser; existing credentials were preserved");
            }
            let code = wait_for_code(&listener, &state, LOGIN_TIMEOUT)?;
            let agent = agent();
            let token = token_request(
                &agent,
                TOKEN_URL,
                &[
                    ("grant_type", "authorization_code"),
                    ("code", &code),
                    ("redirect_uri", &redirect),
                    ("client_id", &client.client_id),
                    ("client_secret", &client.client_secret),
                    ("code_verifier", &verifier),
                ],
            )?;
            let granted = token["scope"].as_str().unwrap_or("");
            if !granted.split_whitespace().any(|s| s == DRIVE_SCOPE) {
                bail!(
                    "Google Drive permission was not granted; existing credentials were preserved"
                );
            }
            let refresh_token = token["refresh_token"]
                .as_str()
                .filter(|s| !s.is_empty())
                .context("Google returned no refresh token; existing credentials were preserved")?;
            let status = identity(&agent, access_token(&token)?, &config)?;
            save_credentials(
                &path,
                &Credentials {
                    client_id: client.client_id,
                    client_secret: client.client_secret,
                    refresh_token: refresh_token.into(),
                },
            )?;
            Ok(status)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_matches_rfc7636_vector_and_requests_offline_consent() {
        let url = authorization_url(
            "client",
            "http://127.0.0.1:1234/callback",
            "state",
            "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk",
        );
        let query: std::collections::HashMap<_, _> = url.query_pairs().collect();
        assert_eq!(
            query["code_challenge"],
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
        assert_eq!(query["access_type"], "offline");
        assert_eq!(query["state"], "state");
    }

    #[test]
    fn callback_rejects_forgery_denial_and_duplicate_parameters() {
        for target in [
            "/callback?state=wrong&code=secret",
            "/callback?state=ok&error=access_denied",
            "/callback?state=ok&state=ok&code=x",
            "/callback?state=ok&code=x&code=y",
            "/other?state=ok&code=x",
        ] {
            let error = callback_code(target, "ok").unwrap_err().to_string();
            assert!(!error.contains("secret"));
        }
        assert_eq!(
            callback_code("/callback?state=ok&code=a%2Bb", "ok").unwrap(),
            "a+b"
        );
    }

    #[test]
    fn token_endpoint_failure_never_exposes_the_response_body() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/token", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = [0; 8192];
            let _ = stream.read(&mut request).unwrap();
            let body = "sensitive-token-provider-body";
            write!(
                stream,
                "HTTP/1.1 400 Bad Request\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        });
        let error =
            token_request(&agent(), &endpoint, &[("grant_type", "refresh_token")]).unwrap_err();
        assert!(!format!("{error:#}").contains("sensitive-token-provider-body"));
        assert!(!format!("{error:#}").contains(&endpoint));
        server.join().unwrap();
    }

    #[test]
    fn abandoned_login_times_out() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        assert!(wait_for_code(&listener, "state", Duration::from_millis(1)).is_err());
    }

    #[test]
    fn replacement_is_private_and_readable_by_the_uploader() {
        let dir = std::env::temp_dir().join(format!("voxray-auth-{}", random_string().unwrap()));
        fs::create_dir(&dir).unwrap();
        let path = dir.join("credentials.json");
        for token in ["first", "replacement"] {
            save_credentials(
                &path,
                &Credentials {
                    client_id: "client".into(),
                    client_secret: "desktop".into(),
                    refresh_token: token.into(),
                },
            )
            .unwrap();
            assert_eq!(
                read_credentials(&path).unwrap().unwrap().refresh_token,
                token
            );
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn loopback_receiver_handles_real_browser_request() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let browser = thread::spawn(move || {
            let mut stream = TcpStream::connect(address).unwrap();
            stream.write_all(b"GET /callback?state=expected&code=accepted HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
            let mut response = String::new();
            stream.read_to_string(&mut response).unwrap();
            assert!(response.starts_with("HTTP/1.1 200"));
            assert!(!response.contains("accepted"));
        });
        assert_eq!(
            wait_for_code(&listener, "expected", Duration::from_secs(2)).unwrap(),
            "accepted"
        );
        browser.join().unwrap();
    }
}
