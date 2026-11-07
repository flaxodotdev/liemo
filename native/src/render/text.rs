// Canvas text: the six bundled faces (variable where the family is) and any font installed on
// the Mac, shaped with rustybuzz and filled as outlines, with system fallbacks for glyphs a face
// lacks (katakana, braille) and Apple Color Emoji for emoji.

use super::canvas::{pixmap, Canvas};
use parking_lot::Mutex;
use rustybuzz::ttf_parser::{self, GlyphId, OutlineBuilder, Tag};
use rustybuzz::{Face, UnicodeBuffer, Variation};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use tiny_skia::{Color, FillRule, LineJoin, Paint, Path, PathBuilder, Pixmap, Stroke, Transform};

pub const FONT_MONA: &[u8] = include_bytes!("../../assets/fonts/var/MonaSans.ttf");
pub const FONT_INTER: &[u8] = include_bytes!("../../assets/fonts/var/Inter.ttf");
pub const FONT_INSTRUMENT: &[u8] = include_bytes!("../../assets/fonts/var/InstrumentSerif.ttf");
pub const FONT_BEBAS: &[u8] = include_bytes!("../../assets/fonts/var/BebasNeue.ttf");
pub const FONT_JETBRAINS: &[u8] = include_bytes!("../../assets/fonts/var/JetBrainsMono.ttf");
pub const FONT_CAVEAT: &[u8] = include_bytes!("../../assets/fonts/var/Caveat.ttf");

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Baseline {
    Alphabetic,
    Middle,
}

pub struct TextStyle<'a> {
    pub family: &'a str,
    pub weight: f32,
    pub size: f32,
    /// Extra space after every character, in px (canvas letterSpacing).
    pub spacing: f32,
}

#[derive(Clone, Copy)]
struct FontData {
    data: &'static [u8],
    /// Face index inside a collection (.ttc).
    index: u32,
    variable: bool,
}

/// 0..6 are the bundled faces, the rest are system fallbacks found at startup.
fn fonts() -> &'static Vec<FontData> {
    static F: OnceLock<Vec<FontData>> = OnceLock::new();
    F.get_or_init(|| {
        let mut v = vec![
            FontData { index: 0, data: FONT_MONA, variable: true },
            FontData { index: 0, data: FONT_INTER, variable: true },
            FontData { index: 0, data: FONT_INSTRUMENT, variable: false },
            FontData { index: 0, data: FONT_BEBAS, variable: false },
            FontData { index: 0, data: FONT_JETBRAINS, variable: true },
            FontData { index: 0, data: FONT_CAVEAT, variable: true },
        ];
        for p in [
            "/System/Library/Fonts/Menlo.ttc",
            "/System/Library/Fonts/Apple Braille.ttf",
            "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc",
            "/System/Library/Fonts/Hiragino Sans GB.ttc",
            "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
            "/System/Library/Fonts/Helvetica.ttc",
            "/Library/Fonts/Arial Unicode.ttf",
        ] {
            if let Ok(bytes) = std::fs::read(p) {
                v.push(FontData { data: Box::leak(bytes.into_boxed_slice()), index: 0, variable: false });
            }
        }
        v
    })
}

fn bundled_index(family: &str) -> Option<usize> {
    Some(match family {
        "Mona Sans" => 0,
        "Inter" => 1,
        "Instrument Serif" => 2,
        "Bebas Neue" => 3,
        "JetBrains Mono" => 4,
        "Caveat" => 5,
        _ => return None,
    })
}

// ---------- Installed fonts ----------
/// Faces loaded from installed fonts live at SYSTEM_BASE + n, one per (family, weight).
const SYSTEM_BASE: usize = 1 << 20;

fn system_db() -> &'static fontdb::Database {
    static D: OnceLock<fontdb::Database> = OnceLock::new();
    D.get_or_init(|| {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        db
    })
}

/// Every installed font family, sorted, without the hidden system ones (".SF…").
pub fn system_families() -> &'static Vec<String> {
    static F: OnceLock<Vec<String>> = OnceLock::new();
    F.get_or_init(|| {
        let mut v: Vec<String> = system_db()
            .faces()
            .filter_map(|f| f.families.iter().find(|(_, l)| *l == fontdb::Language::English_UnitedStates).or(f.families.first()).map(|(n, _)| n.clone()))
            .filter(|n| !n.starts_with('.') && !n.starts_with("System Font"))
            .collect();
        v.sort_by_key(|n| n.to_lowercase());
        v.dedup();
        v
    })
}

pub fn is_installed(family: &str) -> bool {
    system_families().binary_search_by_key(&family.to_lowercase(), |n| n.to_lowercase()).is_ok()
}

struct Loaded {
    faces: Vec<FontData>,
    by_key: HashMap<(String, u32), usize>,
    files: HashMap<std::path::PathBuf, &'static [u8]>,
}
fn loaded() -> &'static Mutex<Loaded> {
    static L: OnceLock<Mutex<Loaded>> = OnceLock::new();
    L.get_or_init(|| Mutex::new(Loaded { faces: vec![], by_key: HashMap::new(), files: HashMap::new() }))
}

/// The installed face closest to `family` at `weight`, loaded once.
fn system_face(family: &str, weight: f32) -> Option<usize> {
    let w = ((weight / 100.0).round() * 100.0).clamp(100.0, 900.0) as u32;
    let key = (family.to_string(), w);
    if let Some(i) = loaded().lock().by_key.get(&key) {
        return Some(*i);
    }
    let db = system_db();
    let id = db.query(&fontdb::Query { families: &[fontdb::Family::Name(family)], weight: fontdb::Weight(w as u16), stretch: fontdb::Stretch::Normal, style: fontdb::Style::Normal })?;
    let face = db.face(id)?;
    let mut l = loaded().lock();
    let data: &'static [u8] = match &face.source {
        fontdb::Source::File(path) => match l.files.get(path) {
            Some(d) => d,
            None => {
                let d: &'static [u8] = Box::leak(std::fs::read(path).ok()?.into_boxed_slice());
                l.files.insert(path.clone(), d);
                d
            }
        },
        fontdb::Source::Binary(b) => Box::leak((**b).as_ref().to_vec().into_boxed_slice()),
        #[allow(unreachable_patterns)]
        _ => return None,
    };
    let variable = ttf_parser::Face::parse(data, face.index).map(|f| f.variation_axes().into_iter().any(|a| a.tag == Tag::from_bytes(b"wght"))).unwrap_or(false);
    l.faces.push(FontData { data, index: face.index, variable });
    let i = SYSTEM_BASE + l.faces.len() - 1;
    l.by_key.insert(key, i);
    Some(i)
}

fn font(idx: usize) -> Option<FontData> {
    if idx >= SYSTEM_BASE {
        loaded().lock().faces.get(idx - SYSTEM_BASE).copied()
    } else {
        fonts().get(idx).copied()
    }
}

/// The face that draws `family` at `weight`: a bundled one, an installed one, or Mona Sans.
pub fn face_index(family: &str, weight: f32) -> usize {
    bundled_index(family).or_else(|| system_face(family, weight)).unwrap_or(0)
}

thread_local! {
    static FACES: RefCell<HashMap<(usize, u32, u32), Face<'static>>> = RefCell::new(HashMap::new());
}

fn with_face<R>(idx: usize, weight: f32, size: f32, f: impl FnOnce(&Face<'static>) -> R) -> Option<R> {
    let fd = font(idx)?;
    let (w, opsz) = if fd.variable { (weight.round() as u32, if idx == 1 { size.clamp(14.0, 32.0).round() as u32 } else { 0 }) } else { (0, 0) };
    FACES.with(|faces| {
        let mut faces = faces.borrow_mut();
        let key = (idx, w, opsz);
        if !faces.contains_key(&key) {
            let mut face = Face::from_slice(fd.data, fd.index)?;
            if fd.variable {
                let mut vars = vec![Variation { tag: Tag::from_bytes(b"wght"), value: w as f32 }];
                if opsz > 0 {
                    vars.push(Variation { tag: Tag::from_bytes(b"opsz"), value: opsz as f32 });
                }
                face.set_variations(&vars);
            }
            faces.insert(key, face);
        }
        Some(f(faces.get(&key).unwrap()))
    })
}

fn has_glyph(idx: usize, ch: char) -> bool {
    with_face(idx, 400.0, 16.0, |f| f.glyph_index(ch).map(|g| g.0 != 0).unwrap_or(false)).unwrap_or(false)
}

/// Which face draws `ch`: the requested one when it has the glyph, otherwise a fallback.
fn face_for(primary: usize, ch: char) -> usize {
    if ch.is_whitespace() || has_glyph(primary, ch) {
        return primary;
    }
    (6..fonts().len()).find(|&i| has_glyph(i, ch)).unwrap_or(primary)
}

pub fn is_emoji(ch: char) -> bool {
    let c = ch as u32;
    (0x1F300..=0x1FAFF).contains(&c) || (0x2600..=0x27BF).contains(&c)
}

/// One shaped glyph: face, glyph id, pen x in px, and an optional emoji image.
struct Placed {
    face: usize,
    gid: u16,
    x: f32,
    emoji: Option<char>,
}

struct Shaped {
    glyphs: Vec<Placed>,
    width: f32,
}

fn shape(text: &str, st: &TextStyle) -> Shaped {
    let primary = face_index(st.family, st.weight);
    let mut glyphs = vec![];
    let mut pen = 0.0f32;
    // Split into runs by face.
    let mut runs: Vec<(usize, String)> = vec![];
    for ch in text.chars() {
        let fi = if is_emoji(ch) { usize::MAX } else { face_for(primary, ch) };
        match runs.last_mut() {
            Some((f, s)) if *f == fi && fi != usize::MAX => s.push(ch),
            _ => runs.push((fi, ch.to_string())),
        }
    }
    for (fi, run) in runs {
        if fi == usize::MAX {
            for ch in run.chars() {
                glyphs.push(Placed { face: usize::MAX, gid: 0, x: pen, emoji: Some(ch) });
                pen += st.size * 1.17 + st.spacing;
            }
            continue;
        }
        with_face(fi, st.weight, st.size, |face| {
            let upem = face.units_per_em() as f32;
            let k = st.size / upem;
            let mut buf = UnicodeBuffer::new();
            buf.push_str(&run);
            let out = rustybuzz::shape(face, &[], buf);
            let infos = out.glyph_infos();
            let pos = out.glyph_positions();
            let mut last_cluster = u32::MAX;
            for (i, p) in infos.iter().zip(pos) {
                glyphs.push(Placed { face: fi, gid: i.glyph_id as u16, x: pen + p.x_offset as f32 * k, emoji: None });
                pen += p.x_advance as f32 * k;
                if i.cluster != last_cluster {
                    pen += st.spacing;
                    last_cluster = i.cluster;
                }
            }
        });
    }
    Shaped { glyphs, width: pen }
}

pub fn measure(text: &str, st: &TextStyle) -> f32 {
    shape(text, st).width
}

/// Ascent and descent (both positive) in px, from the face's hhea metrics.
pub fn metrics(st: &TextStyle) -> (f32, f32) {
    with_face(face_index(st.family, st.weight), st.weight, st.size, |f| {
        let k = st.size / f.units_per_em() as f32;
        (f.ascender() as f32 * k, -(f.descender() as f32) * k)
    })
    .unwrap_or((st.size * 0.8, st.size * 0.2))
}

struct PathSink(PathBuilder);
impl OutlineBuilder for PathSink {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to(x, y)
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to(x, y)
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.0.quad_to(x1, y1, x, y)
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.0.cubic_to(x1, y1, x2, y2, x, y)
    }
    fn close(&mut self) {
        self.0.close()
    }
}

type GlyphKey = (usize, u16, u32, u32);
fn glyph_cache() -> &'static Mutex<HashMap<GlyphKey, Option<Arc<Path>>>> {
    static C: OnceLock<Mutex<HashMap<GlyphKey, Option<Arc<Path>>>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Glyph outline in font units (y up).
fn glyph_path(face: usize, gid: u16, weight: f32, size: f32) -> Option<Arc<Path>> {
    let variable = font(face).map(|f| f.variable).unwrap_or(false);
    let w = if variable { weight.round() as u32 } else { 0 };
    let o = if variable && face == 1 { size.clamp(14.0, 32.0).round() as u32 } else { 0 };
    let key = (face, gid, w, o);
    if let Some(p) = glyph_cache().lock().get(&key) {
        return p.clone();
    }
    let p = with_face(face, weight, size, |f| {
        let mut sink = PathSink(PathBuilder::new());
        f.outline_glyph(GlyphId(gid), &mut sink)?;
        sink.0.finish().map(Arc::new)
    })
    .flatten();
    let mut c = glyph_cache().lock();
    if c.len() > 20000 {
        c.clear();
    }
    c.insert(key, p.clone());
    p
}

fn emoji_font() -> Option<&'static [u8]> {
    static E: OnceLock<Option<&'static [u8]>> = OnceLock::new();
    *E.get_or_init(|| std::fs::read("/System/Library/Fonts/Apple Color Emoji.ttc").ok().map(|b| &*Box::leak(b.into_boxed_slice())))
}

fn emoji_image(ch: char, px: u16) -> Option<Arc<Pixmap>> {
    static C: OnceLock<Mutex<HashMap<(char, u16), Option<Arc<Pixmap>>>>> = OnceLock::new();
    let cache = C.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(v) = cache.lock().get(&(ch, px)) {
        return v.clone();
    }
    let img = (|| {
        let face = ttf_parser::Face::parse(emoji_font()?, 0).ok()?;
        let gid = face.glyph_index(ch)?;
        let raster = face.glyph_raster_image(gid, px.max(20))?;
        let rgba = image::load_from_memory(raster.data).ok()?.to_rgba8();
        let (w, h) = rgba.dimensions();
        Some(Arc::new(super::canvas::from_straight(rgba.as_raw(), w, h)))
    })();
    cache.lock().insert((ch, px), img.clone());
    img
}

/// fillText. `x, y` are in the canvas's user space; the current transform, alpha, blend and clip apply.
pub fn fill_text(c: &mut Canvas, text: &str, x: f32, y: f32, st: &TextStyle, align: Align, baseline: Baseline, color: Color) {
    if text.is_empty() {
        return;
    }
    let shaped = shape(text, st);
    let x0 = match align {
        Align::Left => x,
        Align::Center => x - shaped.width / 2.0,
        Align::Right => x - shaped.width,
    };
    let (asc, desc) = metrics(st);
    let base = match baseline {
        Baseline::Alphabetic => y,
        Baseline::Middle => y + (asc - desc) / 2.0,
    };
    let primary = face_index(st.family, st.weight);
    // Bundled static faces get a faux bold; installed families bring their own bold faces.
    let faux_bold = primary < SYSTEM_BASE && !font(primary).map(|f| f.variable).unwrap_or(false) && st.weight >= 600.0;
    let ts = c.transform();
    let mut pb = PathBuilder::new();
    let mut any = false;
    for g in &shaped.glyphs {
        if let Some(ch) = g.emoji {
            let px = (st.size * ts.sx.abs().max(ts.sy.abs())).clamp(8.0, 160.0) as u16;
            if let Some(img) = emoji_image(ch, px) {
                let h = st.size * 1.17;
                let w = h * img.width() as f32 / img.height() as f32;
                let saved = c.transform();
                c.draw_image_scaled(&img, x0 + g.x, base - st.size * 0.95, w, h);
                c.set_transform(saved);
            }
            continue;
        }
        let Some(path) = glyph_path(g.face, g.gid, st.weight, st.size) else { continue };
        let upem = with_face(g.face, st.weight, st.size, |f| f.units_per_em() as f32).unwrap_or(1000.0);
        let k = st.size / upem;
        let gt = ts.pre_translate(x0 + g.x, base).pre_scale(k, -k);
        if let Some(p) = (*path).clone().transform(gt) {
            pb.push_path(&p);
            any = true;
        }
    }
    if !any {
        return;
    }
    let Some(path) = pb.finish() else { return };
    let fill = super::canvas::Fill::Solid(color);
    c.fill_path_with(&path, &fill);
    if faux_bold {
        let mut color = color;
        color.apply_opacity(c.alpha());
        let paint = Paint { shader: tiny_skia::Shader::SolidColor(color), blend_mode: c.blend(), anti_alias: true, ..Default::default() };
        let k = (ts.sx * ts.sy - ts.kx * ts.ky).abs().sqrt();
        let stroke = Stroke { width: st.size * 0.035 * k, line_join: LineJoin::Round, ..Default::default() };
        c.pm.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
    let _ = FillRule::Winding;
}

/// Render a single glyph, centred, into a square tile (used by the ASCII atlas).
pub fn glyph_tile(text: &str, cell: u32, st: &TextStyle, dy: f32) -> Pixmap {
    let mut c = Canvas::from_pixmap(pixmap(cell, cell));
    fill_text(&mut c, text, cell as f32 / 2.0, cell as f32 / 2.0 + dy, st, Align::Center, Baseline::Middle, Color::WHITE);
    c.into_pixmap()
}
