// The brand kit: a name, colours, a typeface and a logo, set once and used in every project. Brand
// colours lead the colour swatches, "Apply brand" dresses text in them, the logo goes on as a
// watermark, and the AI chat is told about the brand so its titles stay on it. A DESIGN.md (or any
// text) can fill it in.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Brand {
    pub name: String,
    /// #rrggbb, most important first: the first is the main text colour.
    pub colors: Vec<String>,
    /// A font family: bundled, or installed on the computer.
    pub font: String,
    /// An image file for the logo.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logo: Option<PathBuf>,
}

fn path() -> PathBuf {
    crate::storage::root().join("brand.json")
}

fn cache() -> &'static parking_lot::RwLock<Option<Brand>> {
    static C: std::sync::OnceLock<parking_lot::RwLock<Option<Brand>>> = std::sync::OnceLock::new();
    C.get_or_init(|| parking_lot::RwLock::new(None))
}

/// The brand kit (read from disk once, then kept).
pub fn load() -> Brand {
    if let Some(b) = cache().read().clone() {
        return b;
    }
    let b: Brand = std::fs::read(path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    *cache().write() = Some(b.clone());
    b
}

pub fn save(b: &Brand) -> anyhow::Result<()> {
    crate::storage::write_atomic(&path(), &serde_json::to_vec_pretty(b)?)?;
    *cache().write() = Some(b.clone());
    Ok(())
}

impl Brand {
    pub fn is_empty(&self) -> bool {
        self.name.is_empty() && self.colors.is_empty() && self.font.is_empty() && self.logo.is_none()
    }
    /// One paragraph for the AI chat, or nothing without a brand.
    pub fn describe(&self) -> Option<String> {
        if self.is_empty() {
            return None;
        }
        let mut s = String::from("The user's brand kit");
        if !self.name.is_empty() {
            s += &format!(" ({})", self.name);
        }
        s += ":";
        if !self.colors.is_empty() {
            s += &format!(" colours {} (the first for text and titles);", self.colors.join(", "));
        }
        if !self.font.is_empty() {
            s += &format!(" typeface {};", self.font);
        }
        if let Some(l) = &self.logo {
            s += &format!(" logo image {} (import_media to use it);", l.display());
        }
        s += " Use these for titles and colour choices unless asked otherwise.";
        Some(s)
    }
}

/// Valid "#rrggbb" (or "#rgb", expanded), lower case.
pub fn norm_hex(s: &str) -> Option<String> {
    let h = s.trim().trim_start_matches('#');
    let h = if h.len() == 3 { h.chars().flat_map(|c| [c, c]).collect::<String>() } else { h.to_string() };
    (h.len() == 6 && h.chars().all(|c| c.is_ascii_hexdigit())).then(|| format!("#{}", h.to_ascii_lowercase()))
}

/// Read a brand out of a DESIGN.md or similar: its first heading as the name, its hex colours in
/// the order they appear (up to six), and the first font family named that Liemo can draw.
pub fn from_text(src: &str) -> Brand {
    let mut b = Brand::default();
    b.name = src.lines().find_map(|l| l.trim().strip_prefix("# ")).map(|s| s.trim().to_string()).unwrap_or_default();
    let cs: Vec<char> = src.chars().collect();
    let mut i = 0;
    while i < cs.len() && b.colors.len() < 6 {
        if cs[i] == '#' {
            let hex: String = cs[i + 1..].iter().take_while(|c| c.is_ascii_hexdigit()).collect();
            if hex.len() == 6 || hex.len() == 3 {
                if let Some(h) = norm_hex(&hex) {
                    if !b.colors.contains(&h) {
                        b.colors.push(h);
                    }
                }
            }
            i += 1 + hex.len();
        } else {
            i += 1;
        }
    }
    // The typeface: the earliest family named that Liemo can draw. Bundled fonts match in any
    // case; installed ones only as written (so "times" in a sentence isn't Times).
    let lower = src.to_lowercase();
    let at_word = |text: &str, i: usize, len: usize| {
        let before = text[..i].chars().next_back().is_none_or(|c| !c.is_alphanumeric());
        let after = text[i + len..].chars().next().is_none_or(|c| !c.is_alphanumeric());
        before && after
    };
    let find = |text: &str, name: &str| text.match_indices(name).map(|(i, _)| i).find(|i| at_word(text, *i, name.len()));
    let mut best: Option<(usize, String)> = None;
    let mut consider = |pos: Option<usize>, name: &str| {
        if let Some(p) = pos {
            // Earliest wins; at the same place, the longer name ("Inter Display" over "Inter").
            if best.as_ref().is_none_or(|(bp, bn)| p < *bp || (p == *bp && name.len() > bn.len())) {
                best = Some((p, name.to_string()));
            }
        }
    };
    for f in super::assets::FONTS.iter() {
        consider(find(&lower, &f.family.to_lowercase()), f.family);
    }
    for fam in crate::render::text::system_families() {
        if fam.len() >= 4 {
            consider(find(src, &fam), &fam);
        }
    }
    if let Some((_, f)) = best {
        b.font = f;
    }
    b
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_design_md() {
        let b = super::from_text("# Acme\n\nPrimary: #FF5500\nInk #111\n\nType: \"Inter\", sans-serif\n#ff5500 again");
        assert_eq!(b.name, "Acme");
        assert_eq!(b.colors, vec!["#ff5500", "#111111"]);
        assert_eq!(b.font, "Inter");
    }
}
