// Projects live in ~/Library/Application Support/Liemo: one JSON file per project plus a card
// thumbnail, and one record per imported media file (pointing at the file where it is).

use crate::model::types::{AssetType, Clip, Track};
use crate::render::canvas::to_straight;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tiny_skia::Pixmap;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRecord {
    pub id: String,
    pub name: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub duration: f64,
    pub clip_count: usize,
    pub clips: Vec<Clip>,
    pub tracks: Vec<Track>,
    /// Imported media this project uses (ids into the media store).
    pub pool: Vec<String>,
    /// Canvas format (see model::format). Older projects are landscape.
    #[serde(default = "crate::model::format::default_format")]
    pub format: String,
    /// Pixel size for a custom format (width, height).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<[u32; 2]>,
    #[serde(default = "default_fps")]
    pub fps: u32,
    /// Short side of the default export, in pixels.
    #[serde(default = "default_resolution")]
    pub resolution: u32,
    #[serde(default = "default_background")]
    pub background: String,
}

fn default_fps() -> u32 {
    30
}
fn default_resolution() -> u32 {
    1080
}
fn default_background() -> String {
    "#000000".into()
}

/// A design canvas for a pixel size: the same shape, with a 720-unit short side.
pub fn design_canvas(w: u32, h: u32) -> (f32, f32) {
    let (w, h) = (w.max(1) as f32, h.max(1) as f32);
    if w <= h { (720.0, (720.0 * h / w).round()) } else { ((720.0 * w / h).round(), 720.0) }
}
/// "1080 × 1350" → "4:5"; ratios that don't reduce neatly become "1.85:1".
pub fn ratio_key(w: u32, h: u32) -> String {
    fn gcd(a: u32, b: u32) -> u32 {
        if b == 0 { a } else { gcd(b, a % b) }
    }
    let g = gcd(w.max(1), h.max(1));
    let (a, b) = (w.max(1) / g, h.max(1) / g);
    if a <= 32 && b <= 32 {
        format!("{a}:{b}")
    } else if w >= h {
        format!("{:.2}:1", w as f32 / h as f32)
    } else {
        format!("1:{:.2}", h as f32 / w as f32)
    }
}

impl ProjectRecord {
    pub fn env(&self) -> crate::model::time::Env {
        let (w, h) = match (self.format.as_str(), self.size) {
            ("custom", Some([pw, ph])) => design_canvas(pw, ph),
            _ => {
                let f = crate::model::format::format(&self.format);
                (f.w, f.h)
            }
        };
        let [r, g, b] = crate::render::canvas::hex_rgb(&self.background).map(|v| v as u8);
        crate::model::time::Env { w, h, fps: self.fps.clamp(1, 240) as f64, bg: [r, g, b] }
    }
    /// Export aspect key for this project's shape.
    pub fn aspect_key(&self) -> String {
        match (self.format.as_str(), self.size) {
            ("custom", Some([w, h])) => ratio_key(w, h),
            _ => crate::model::format::format(&self.format).aspect.into(),
        }
    }
    pub fn format_label(&self) -> String {
        match (self.format.as_str(), self.size) {
            ("custom", Some([w, h])) => format!("Custom {w} × {h}"),
            _ => crate::model::format::format(&self.format).name.into(),
        }
    }
}

#[derive(Clone)]
pub struct ProjectSummary {
    pub id: String,
    pub name: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub duration: f64,
    pub clip_count: usize,
    pub thumb: Option<PathBuf>,
    pub format: String,
    pub label: String,
    pub aspect: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaRecord {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: AssetType,
    pub name: String,
    pub sub: String,
    pub dur: f64,
    pub path: PathBuf,
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
    #[serde(default)]
    pub has_audio: bool,
}

pub fn root() -> PathBuf {
    // RAYNA_DATA points tests (or a second profile) at another folder.
    if let Ok(dir) = std::env::var("RAYNA_DATA") {
        return PathBuf::from(dir);
    }
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("Rayna")
}
fn projects_dir() -> PathBuf {
    root().join("projects")
}
fn media_dir() -> PathBuf {
    root().join("media")
}
pub fn project_thumb_path(id: &str) -> PathBuf {
    projects_dir().join(format!("{id}.jpg"))
}
pub fn media_thumb_path(id: &str) -> PathBuf {
    media_dir().join(format!("{id}.jpg"))
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}
pub fn new_id() -> String {
    format!("{}{}", crate::media::import::radix36(now_ms() as u64), crate::media::import::radix36(crate::media::import::rand_u32() as u64 % 2176782336))
}

/// Write via a temporary file and rename, so a crash never leaves half a project.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

pub fn save_jpeg(pm: &Pixmap, path: &Path) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let rgba = to_straight(pm);
    let rgb: Vec<u8> = rgba.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).collect();
    let mut out = vec![];
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 82).encode(&rgb, pm.width(), pm.height(), image::ExtendedColorType::Rgb8)?;
    write_atomic(path, &out)
}
pub fn load_image(path: &Path) -> Option<Pixmap> {
    let img = image::open(path).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    Some(crate::render::canvas::from_straight(img.as_raw(), w, h))
}

// ---------- Projects ----------
pub fn list_projects() -> Result<Vec<ProjectSummary>> {
    let dir = projects_dir();
    std::fs::create_dir_all(&dir)?;
    let mut out = vec![];
    for e in std::fs::read_dir(&dir)?.flatten() {
        let p = e.path();
        if p.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let Ok(bytes) = std::fs::read(&p) else { continue };
        let Ok(r) = serde_json::from_slice::<ProjectRecord>(&bytes) else { continue };
        let thumb = project_thumb_path(&r.id);
        let (label, aspect) = (r.format_label(), r.aspect_key());
        out.push(ProjectSummary {
            thumb: thumb.exists().then_some(thumb),
            id: r.id,
            name: r.name,
            created_at: r.created_at,
            updated_at: r.updated_at,
            duration: r.duration,
            clip_count: r.clip_count,
            label,
            aspect,
            format: r.format,
        });
    }
    Ok(out)
}
pub fn get_project(id: &str) -> Option<ProjectRecord> {
    let bytes = std::fs::read(projects_dir().join(format!("{id}.json"))).ok()?;
    serde_json::from_slice(&bytes).ok()
}
pub fn put_project(p: &ProjectRecord) -> Result<()> {
    write_atomic(&projects_dir().join(format!("{}.json", p.id)), &serde_json::to_vec(p)?)
}
pub fn delete_project(id: &str) -> Result<()> {
    let p = get_project(id);
    std::fs::remove_file(projects_dir().join(format!("{id}.json")))?;
    let _ = std::fs::remove_file(project_thumb_path(id));
    if let Some(p) = p {
        collect_media(&p.pool);
    }
    Ok(())
}
pub fn rename_project(id: &str, name: &str) -> Result<()> {
    if let Some(mut p) = get_project(id) {
        p.name = name.to_string();
        p.updated_at = now_ms();
        put_project(&p)?;
    }
    Ok(())
}
pub fn duplicate_project(id: &str) -> Result<Option<String>> {
    let Some(p) = get_project(id) else { return Ok(None) };
    let copy = ProjectRecord { id: new_id(), name: format!("{} copy", p.name), created_at: now_ms(), updated_at: now_ms(), ..p };
    put_project(&copy)?;
    let _ = std::fs::copy(project_thumb_path(id), project_thumb_path(&copy.id));
    Ok(Some(copy.id))
}

// ---------- Media ----------
pub fn put_media(m: &MediaRecord) -> Result<()> {
    write_atomic(&media_dir().join(format!("{}.json", m.id)), &serde_json::to_vec(m)?)
}
pub fn get_media(id: &str) -> Option<MediaRecord> {
    let bytes = std::fs::read(media_dir().join(format!("{id}.json"))).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Forget media records no remaining project uses. The files themselves are left alone.
fn collect_media(candidates: &[String]) {
    let used: std::collections::HashSet<String> = std::fs::read_dir(projects_dir())
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| std::fs::read(e.path()).ok())
        .filter_map(|b| serde_json::from_slice::<ProjectRecord>(&b).ok())
        .flat_map(|p| p.pool)
        .collect();
    for id in candidates.iter().filter(|id| !used.contains(*id)) {
        let _ = std::fs::remove_file(media_dir().join(format!("{id}.json")));
        let _ = std::fs::remove_file(media_thumb_path(id));
    }
}

pub fn relative_time(ts: i64) -> String {
    let s = ((now_ms() - ts) as f64 / 1000.0).round() as i64;
    if s < 45 {
        return "just now".into();
    }
    let m = (s as f64 / 60.0).round() as i64;
    if m < 60 {
        return format!("{m} min ago");
    }
    let h = (m as f64 / 60.0).round() as i64;
    if h < 24 {
        return format!("{h} hr ago");
    }
    let d = (h as f64 / 24.0).round() as i64;
    if d < 7 {
        return if d == 1 { "yesterday".into() } else { format!("{d} days ago") };
    }
    // Older: a date, like "5 Nov" (with the year when it isn't this one).
    let days = ts / 86_400_000;
    let (y, mo, dd) = civil(days);
    let (ny, ..) = civil(now_ms() / 86_400_000);
    const MON: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    if y == ny { format!("{dd} {}", MON[mo as usize - 1]) } else { format!("{dd} {} {y}", MON[mo as usize - 1]) }
}
/// Days since 1970-01-01 → (year, month, day).
fn civil(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}
