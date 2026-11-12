// Components: clips saved together (a lower third with its backing and animation, a branded end
// card) to reuse in any project. Each is a JSON file of clips with their relative timing.

use crate::model::types::Clip;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Component {
    pub id: String,
    pub name: String,
    pub created: i64,
    pub clips: Vec<Clip>,
}

fn dir() -> PathBuf {
    crate::storage::root().join("components")
}

/// Saved components, newest first.
pub fn list() -> Vec<Component> {
    let mut v: Vec<Component> = std::fs::read_dir(dir())
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| std::fs::read(e.path()).ok())
        .filter_map(|b| serde_json::from_slice(&b).ok())
        .collect();
    v.sort_by_key(|c: &Component| std::cmp::Reverse(c.created));
    v
}

pub fn get(id: &str) -> Option<Component> {
    std::fs::read(dir().join(format!("{id}.json"))).ok().and_then(|b| serde_json::from_slice(&b).ok())
}

pub fn save(name: &str, mut clips: Vec<Clip>) -> anyhow::Result<Component> {
    clips.sort_by(|a, b| a.start.total_cmp(&b.start));
    let first = clips.first().map(|c| c.start).unwrap_or(0.0);
    for c in &mut clips {
        c.start -= first;
    }
    let c = Component { id: crate::storage::new_id(), name: name.to_string(), created: crate::storage::now_ms(), clips };
    crate::storage::write_atomic(&dir().join(format!("{}.json", c.id)), &serde_json::to_vec_pretty(&c)?)?;
    Ok(c)
}

pub fn delete(id: &str) {
    let _ = std::fs::remove_file(dir().join(format!("{id}.json")));
}
