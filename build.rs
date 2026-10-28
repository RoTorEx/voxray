use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=VOXRAY_GOOGLE_CLIENT_JSON");
    println!("cargo:rerun-if-env-changed=HOME");
    let local = env::var_os("HOME")
        .map(PathBuf::from)
        .map(|p| p.join(".toolbelt/voxray/google-drive-client.json"));
    if let Some(path) = &local {
        println!("cargo:rerun-if-changed={}", path.display());
    }
    // Desktop clients are public OAuth clients. Never embed a user's refresh token.
    let client = env::var("VOXRAY_GOOGLE_CLIENT_JSON")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| local.and_then(|p| fs::read_to_string(p).ok()))
        .unwrap_or_else(|| "{}".into());
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo output directory"));
    let parsed: serde_json::Value =
        serde_json::from_str(&client).expect("Invalid desktop OAuth client JSON");
    let bundled = if let Some(installed) = parsed.get("installed") {
        assert!(
            installed["client_id"]
                .as_str()
                .is_some_and(|s| !s.is_empty()),
            "Desktop OAuth client ID is required"
        );
        assert!(
            installed["client_secret"]
                .as_str()
                .is_some_and(|s| !s.is_empty()),
            "Desktop OAuth client secret is required"
        );
        serde_json::json!({"installed": {
            "client_id": installed["client_id"],
            "client_secret": installed["client_secret"]
        }})
    } else {
        assert!(
            parsed == serde_json::json!({}),
            "Expected a desktop OAuth client, never user credentials"
        );
        parsed
    };
    fs::write(out.join("google-client.json"), bundled.to_string())
        .expect("Write bundled desktop client");
}
