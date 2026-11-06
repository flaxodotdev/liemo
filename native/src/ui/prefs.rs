// Small per-user conveniences (last library tab, sort order, export settings), like the browser
// version's localStorage. Losing them is harmless.

use serde_json::{Map, Value};

fn path() -> std::path::PathBuf {
    crate::storage::root().join("settings.json")
}
fn read() -> Map<String, Value> {
    std::fs::read(path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}
pub fn get(key: &str) -> Option<Value> {
    read().get(key).cloned()
}
pub fn get_str(key: &str) -> Option<String> {
    get(key).and_then(|v| v.as_str().map(String::from))
}
pub fn set(key: &str, v: Value) {
    let mut m = read();
    m.insert(key.into(), v);
    if let Ok(bytes) = serde_json::to_vec_pretty(&m) {
        let _ = std::fs::create_dir_all(crate::storage::root());
        let _ = std::fs::write(path(), bytes);
    }
}
