// Project formats: the canvas shape a project is made in. Every format keeps a 720-unit short
// side, so text and graphics are sized the same way whatever the shape.

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Vertical,
    Posts,
    Landscape,
    Cinema,
}
pub const CATEGORIES: [(Option<Category>, &str); 5] = [
    (None, "All"),
    (Some(Category::Vertical), "Vertical"),
    (Some(Category::Posts), "Square & portrait"),
    (Some(Category::Landscape), "Landscape"),
    (Some(Category::Cinema), "Cinema"),
];

pub struct Format {
    pub id: &'static str,
    pub name: &'static str,
    pub category: Category,
    /// Export aspect key (see export::ASPECTS).
    pub aspect: &'static str,
    /// Design canvas in units.
    pub w: f32,
    pub h: f32,
    /// The usual upload size.
    pub pixels: &'static str,
    pub best_for: &'static str,
    /// The platform's length limit, when it has one.
    pub length: &'static str,
}

const fn f(
    id: &'static str,
    name: &'static str,
    category: Category,
    aspect: &'static str,
    w: f32,
    h: f32,
    pixels: &'static str,
    best_for: &'static str,
    length: &'static str,
) -> Format {
    Format { id, name, category, aspect, w, h, pixels, best_for, length }
}

use Category::*;
pub const FORMATS: [Format; 16] = [
    f("youtube", "YouTube video", Landscape, "16:9", 1280.0, 720.0, "1920 × 1080", "Videos, vlogs, tutorials", "No limit"),
    f("reel", "Instagram Reel", Vertical, "9:16", 720.0, 1280.0, "1080 × 1920", "Reels on Instagram and Facebook", "Up to 3 min"),
    f("tiktok", "TikTok", Vertical, "9:16", 720.0, 1280.0, "1080 × 1920", "TikTok videos", "Up to 10 min"),
    f("shorts", "YouTube Shorts", Vertical, "9:16", 720.0, 1280.0, "1080 × 1920", "Shorts", "Up to 3 min"),
    f("story", "Instagram Story", Vertical, "9:16", 720.0, 1280.0, "1080 × 1920", "Stories on Instagram and Facebook", "60 s per card"),
    f("snapchat", "Snapchat Spotlight", Vertical, "9:16", 720.0, 1280.0, "1080 × 1920", "Spotlight and Stories", "Up to 60 s"),
    f("post", "Instagram post", Posts, "1:1", 720.0, 720.0, "1080 × 1080", "Feed posts and carousels", "Up to 60 min"),
    f("portrait", "Portrait post", Posts, "4:5", 720.0, 900.0, "1080 × 1350", "Instagram and Facebook feeds", "Up to 60 min"),
    f("pinterest", "Pinterest pin", Posts, "2:3", 720.0, 1080.0, "1000 × 1500", "Pins and idea boards", "Up to 15 min"),
    f("linkedin", "LinkedIn post", Posts, "1:1", 720.0, 720.0, "1080 × 1080", "Feed videos", "Up to 10 min"),
    f("x", "X post", Landscape, "16:9", 1280.0, 720.0, "1920 × 1080", "Posts on X and Threads", "Up to 2 min 20 s"),
    f("presentation", "Presentation", Landscape, "16:9", 1280.0, 720.0, "1920 × 1080", "Slides, demos, screen recordings", "No limit"),
    f("classic", "Classic 4:3", Landscape, "4:3", 960.0, 720.0, "1440 × 1080", "Retro, archival and TV looks", "No limit"),
    f("widescreen", "Widescreen", Cinema, "21:9", 1680.0, 720.0, "2560 × 1080", "Trailers, music videos", "No limit"),
    f("cinema", "Cinema 1.85", Cinema, "1.85:1", 1332.0, 720.0, "1998 × 1080", "Short films (flat)", "No limit"),
    f("vertical-cinema", "Vertical cinema", Cinema, "9:16", 720.0, 1280.0, "2160 × 3840", "Phone-first short films in 4K", "No limit"),
];

pub fn format(id: &str) -> &'static Format {
    FORMATS.iter().find(|f| f.id == id).unwrap_or(&FORMATS[0])
}
pub fn default_format() -> String {
    "youtube".into()
}
