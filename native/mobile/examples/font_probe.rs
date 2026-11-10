// Draws a line of text in several installed fonts, inside the iOS simulator, into one image:
// `xcrun simctl spawn booted <binary> <out.png>`.
use liemo_mobile::render::canvas::{hex, Canvas};
use liemo_mobile::render::text::{fill_text, face_index, Align, Baseline, TextStyle};
fn main() {
    let out = std::env::args().nth(1).unwrap();
    let fonts = ["Avenir Next", "Avenir Next Condensed", "Georgia", "Didot", "Futura", "Marker Felt", "Snell Roundhand", "Courier New", "Helvetica Neue", "Chalkboard SE"];
    let mut c = Canvas::new(900, 60 * fonts.len() as u32);
    c.fill_all(hex("#ffffff"));
    for (i, f) in fonts.iter().enumerate() {
        let st = TextStyle { family: f, weight: 800.0, size: 34.0, spacing: 0.0 };
        println!("{f}: face {}", face_index(f, 800.0));
        fill_text(&mut c, &format!("{f} — Hamburgefonts"), 20.0, 30.0 + 60.0 * i as f32, &st, Align::Left, Baseline::Middle, hex("#000000"));
    }
    let pm = c.into_pixmap();
    image::save_buffer(&out, &liemo_mobile::render::canvas::to_straight(&pm), pm.width(), pm.height(), image::ExtendedColorType::Rgba8).unwrap();
}
