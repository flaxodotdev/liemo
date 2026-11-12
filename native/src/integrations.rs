// Connected services, each on the user's own account: Pexels for stock photos and video, and
// ElevenLabs for generated sound effects and music. Keys live in the macOS Keychain (elsewhere, a
// file in Liemo's data folder that only the user can read). Downloads land in the data folder's
// `stock/` and `generated/`, ready for import.

use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};
use std::io::Read;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Service {
    Pexels,
    ElevenLabs,
}

impl Service {
    pub fn id(self) -> &'static str {
        match self {
            Service::Pexels => "pexels",
            Service::ElevenLabs => "elevenlabs",
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Service::Pexels => "Pexels",
            Service::ElevenLabs => "ElevenLabs",
        }
    }
    /// Where the user gets a key.
    pub fn key_url(self) -> &'static str {
        match self {
            Service::Pexels => "https://www.pexels.com/api/new/",
            Service::ElevenLabs => "https://elevenlabs.io/app/settings/api-keys",
        }
    }
}

const KEYCHAIN_SERVICE: &str = "Liemo";

fn known() -> &'static parking_lot::Mutex<std::collections::HashMap<&'static str, bool>> {
    static K: std::sync::OnceLock<parking_lot::Mutex<std::collections::HashMap<&'static str, bool>>> = std::sync::OnceLock::new();
    K.get_or_init(Default::default)
}

/// Whether a key is set (looked up once, then remembered until it changes).
pub fn connected(s: Service) -> bool {
    if let Some(v) = known().lock().get(s.id()) {
        return *v;
    }
    let v = key(s).is_some();
    known().lock().insert(s.id(), v);
    v
}

/// Set or (with None) remove a service's key.
pub fn set_key(s: Service, new: Option<&str>) -> Result<()> {
    store_key(s, new)?;
    known().lock().insert(s.id(), new.is_some_and(|k| !k.trim().is_empty()));
    Ok(())
}

#[cfg(target_os = "macos")]
pub fn key(s: Service) -> Option<String> {
    let out = std::process::Command::new("/usr/bin/security").args(["find-generic-password", "-s", KEYCHAIN_SERVICE, "-a", s.id(), "-w"]).output().ok()?;
    let k = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (out.status.success() && !k.is_empty()).then_some(k)
}

#[cfg(target_os = "macos")]
fn store_key(s: Service, new: Option<&str>) -> Result<()> {
    match new.map(str::trim).filter(|k| !k.is_empty()) {
        Some(k) => {
            // Given to `security -i` on stdin, so the key never shows in the process list.
            if k.chars().any(|c| c == '"' || c == '\\' || c.is_control()) {
                bail!("That doesn't look like an API key");
            }
            use std::io::Write;
            let mut child = std::process::Command::new("/usr/bin/security").arg("-i").stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::null()).spawn()?;
            let cmd = format!("add-generic-password -U -s {KEYCHAIN_SERVICE} -a {} -w \"{k}\"\n", s.id());
            child.stdin.take().ok_or_else(|| anyhow!("security has no stdin"))?.write_all(cmd.as_bytes())?;
            if !child.wait()?.success() || key(s).as_deref() != Some(k) {
                bail!("The Keychain didn't accept the key");
            }
        }
        None => {
            let _ = std::process::Command::new("/usr/bin/security").args(["delete-generic-password", "-s", KEYCHAIN_SERVICE, "-a", s.id()]).status();
        }
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn key_file(s: Service) -> PathBuf {
    crate::storage::root().join("keys").join(s.id())
}

#[cfg(not(target_os = "macos"))]
pub fn key(s: Service) -> Option<String> {
    std::fs::read_to_string(key_file(s)).ok().map(|k| k.trim().to_string()).filter(|k| !k.is_empty())
}

#[cfg(not(target_os = "macos"))]
fn store_key(s: Service, new: Option<&str>) -> Result<()> {
    let p = key_file(s);
    match new.map(str::trim).filter(|k| !k.is_empty()) {
        Some(k) => {
            std::fs::create_dir_all(p.parent().unwrap())?;
            std::fs::write(&p, k)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600))?;
            }
        }
        None => {
            let _ = std::fs::remove_file(p);
        }
    }
    Ok(())
}

fn need(s: Service) -> Result<String> {
    key(s).ok_or_else(|| anyhow!("{} isn't connected. In Liemo, press ⌘K and choose Connect {}, then paste a key from {}.", s.name(), s.name(), s.key_url()))
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder().timeout_global(Some(std::time::Duration::from_secs(120))).http_status_as_error(false).build().into()
}

fn read_all(r: ureq::http::Response<ureq::Body>, what: &str) -> Result<Vec<u8>> {
    let status = r.status().as_u16();
    let mut bytes = vec![];
    r.into_body().into_reader().take(400 << 20).read_to_end(&mut bytes)?;
    if !(200..300).contains(&status) {
        let msg = serde_json::from_slice::<Value>(&bytes).ok().and_then(|v| v["detail"]["message"].as_str().or(v["error"].as_str()).map(String::from)).unwrap_or_else(|| String::from_utf8_lossy(&bytes).chars().take(200).collect());
        bail!("{what} answered {status}: {msg}");
    }
    Ok(bytes)
}

// ---------- Pexels ----------
/// Photos or videos for a query: each with its id, size, photographer and the file to download.
pub fn search_stock(query: &str, video: bool, count: u32, orientation: Option<&str>) -> Result<Vec<Value>> {
    let k = need(Service::Pexels)?;
    let base = if video { "https://api.pexels.com/videos/search" } else { "https://api.pexels.com/v1/search" };
    let mut req = agent().get(base).header("Authorization", &k).query("query", query).query("per_page", count.clamp(1, 40).to_string());
    if let Some(o) = orientation {
        req = req.query("orientation", o);
    }
    let v: Value = serde_json::from_slice(&read_all(req.call().map_err(|e| anyhow!("Pexels: {e}"))?, "Pexels")?)?;
    let items = if video { &v["videos"] } else { &v["photos"] };
    Ok(items
        .as_array()
        .into_iter()
        .flatten()
        .map(|it| {
            if video {
                // The largest file up to 1080p keeps downloads reasonable.
                let files = it["video_files"].as_array().cloned().unwrap_or_default();
                let best = files
                    .iter()
                    .filter(|f| f["file_type"].as_str() == Some("video/mp4") && f["height"].as_u64().unwrap_or(0).min(f["width"].as_u64().unwrap_or(0)) <= 1080)
                    .max_by_key(|f| f["width"].as_u64().unwrap_or(0))
                    .or(files.first());
                json!({ "id": it["id"], "kind": "video", "width": it["width"], "height": it["height"], "seconds": it["duration"], "by": it["user"]["name"], "page": it["url"], "preview": it["image"], "url": best.map(|f| f["link"].clone()).unwrap_or(Value::Null) })
            } else {
                json!({ "id": it["id"], "kind": "photo", "width": it["width"], "height": it["height"], "by": it["photographer"], "page": it["url"], "alt": it["alt"], "color": it["avg_color"], "preview": it["src"]["medium"], "url": it["src"]["large2x"] })
            }
        })
        .collect())
}

fn out_dir(sub: &str) -> Result<PathBuf> {
    let d = crate::storage::root().join(sub);
    std::fs::create_dir_all(&d)?;
    Ok(d)
}

/// A search result's small preview image, downloaded once into a cache folder.
pub fn preview(v: &Value) -> Option<PathBuf> {
    let url = v["preview"].as_str()?;
    if !url.starts_with("https://images.pexels.com/") {
        return None;
    }
    let p = out_dir("stock/previews").ok()?.join(format!("{}-{}.jpg", v["kind"].as_str().unwrap_or("photo"), v["id"]));
    if !p.exists() {
        let bytes = read_all(agent().get(url).call().ok()?, "Pexels").ok()?;
        std::fs::write(&p, bytes).ok()?;
    }
    Some(p)
}

/// Download a Pexels file (a `url` from search_stock) into the stock folder.
pub fn download_stock(url: &str, name: &str) -> Result<PathBuf> {
    let host_ok = url.starts_with("https://images.pexels.com/") || url.starts_with("https://videos.pexels.com/") || url.starts_with("https://player.vimeo.com/");
    if !host_ok {
        bail!("Only files from Pexels search results can be downloaded");
    }
    let ext = if url.contains("videos.pexels.com") || url.contains("vimeo") { "mp4" } else if url.contains(".png") { "png" } else { "jpg" };
    let bytes = read_all(agent().get(url).call().map_err(|e| anyhow!("Pexels: {e}"))?, "Pexels")?;
    let safe: String = name.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == ' ' { c } else { '-' }).collect();
    let p = crate::export::unique_path(&out_dir("stock")?, safe.trim(), ext);
    std::fs::write(&p, bytes)?;
    Ok(p)
}

// ---------- ElevenLabs ----------
/// A sound effect (up to 22 s) or a music track (10 s – 5 min) from a description, as an MP3 in
/// the generated folder. Each call uses the user's ElevenLabs credits.
pub fn generate_sound(prompt: &str, music: bool, seconds: Option<f64>) -> Result<PathBuf> {
    let k = need(Service::ElevenLabs)?;
    let (url, body) = if music {
        let ms = (seconds.unwrap_or(30.0).clamp(10.0, 300.0) * 1000.0) as u64;
        ("https://api.elevenlabs.io/v1/music", json!({ "prompt": prompt, "music_length_ms": ms }))
    } else {
        let mut b = json!({ "text": prompt });
        if let Some(s) = seconds {
            b["duration_seconds"] = json!(s.clamp(0.5, 22.0));
        }
        ("https://api.elevenlabs.io/v1/sound-generation", b)
    };
    let r = agent()
        .post(url)
        .header("xi-api-key", &k)
        .header("Accept", "audio/mpeg")
        .header("Content-Type", "application/json")
        .send(serde_json::to_vec(&body)?.as_slice())
        .map_err(|e| anyhow!("ElevenLabs: {e}"))?;
    let bytes = read_all(r, "ElevenLabs")?;
    let stem: String = prompt.chars().take(40).map(|c| if c.is_alphanumeric() || c == ' ' { c } else { ' ' }).collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ");
    let p = crate::export::unique_path(&out_dir("generated")?, if stem.is_empty() { "sound" } else { &stem }, "mp3");
    std::fs::write(&p, bytes)?;
    Ok(p)
}
