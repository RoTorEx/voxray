//! Bounded metadata events, excluding recordings, paths and transcript content.
use serde_json::Value;
pub fn init() {
    if let Ok(home) = crate::config::app_home() {
        crate::usage::init("voxray", home.join("voxray.log"));
    }
}
pub fn event(action: &str, result: &str, fields: Value) {
    crate::usage::event(action, result, fields);
}
