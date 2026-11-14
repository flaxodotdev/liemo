// No console window behind the app on Windows (stdio still works for `liemo mcp`).
#![cfg_attr(windows, windows_subsystem = "windows")]
mod audio;
mod export;
mod fx;
mod mcp;
mod media;
mod platform;
mod storage;
mod update;
#[cfg(target_os = "macos")]
mod vision_mac;
#[cfg(target_os = "macos")]
mod speech_mac;
mod agent;
mod check;
mod pipeline;
mod integrations;
mod ui;
mod model;
mod render;
#[cfg(test)]
mod feature_tests;

use model::store::{default_tracks, sample_clips};
use render::canvas::Canvas;
use render::compositor::{render_frame, NoFrames, Scene};

fn save_png(pm: &tiny_skia::Pixmap, path: &str) {
    let rgba = render::canvas::to_straight(pm);
    image::save_buffer(path, &rgba, pm.width(), pm.height(), image::ColorType::Rgba8).unwrap();
}

/// The app icon: the faceted L in white on a near-black macOS squircle-ish tile, 1024 px.
fn app_icon(out: &str) -> anyhow::Result<()> {
    let n = 1024.0f32;
    let mut c = Canvas::new(n as u32, n as u32);
    // macOS icon grid: the tile is 824 px with a ~185 px corner radius, centred.
    let (inset, size) = (100.0, 824.0);
    c.set_fill_color(render::canvas::hex("#0b0b0d"));
    c.begin_path();
    c.round_rect(inset, inset, size, size, 185.0);
    c.fill();
    // The mark's own box is 237 units (viewBox 258 163 237 237); draw it at ~55% of the tile.
    let k = size * 0.55 / 237.0;
    let (ox, oy) = (n / 2.0 - (258.0 + 118.5) * k, n / 2.0 - (163.0 + 118.5) * k);
    c.set_fill_color(render::canvas::hex("#ffffff"));
    for poly in [&[(371.0, 163.0), (400.0, 242.0), (325.0, 400.0), (296.0, 321.0)][..], &[(325.0, 400.0), (428.0, 321.0), (457.0, 400.0)][..]] {
        c.begin_path();
        for (i, (x, y)) in poly.iter().enumerate() {
            let (x, y) = (ox + x * k, oy + y * k);
            if i == 0 { c.move_to(x, y) } else { c.line_to(x, y) }
        }
        c.close_path();
        c.fill();
    }
    save_png(&c.into_pixmap(), out);
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    // Remove background finds the subject with Apple's Vision.
    #[cfg(target_os = "macos")]
    fx::cutout::set_segmenter(Box::new(vision_mac::segment));
    if args.get(1).map(String::as_str) == Some("mcp") {
        mcp::serve_stdio(args.get(2).map(String::as_str) == Some("--agent"));
        return;
    }
    if args.get(1).map(String::as_str) == Some("--version") {
        println!("Liemo {}", update::current_version());
        return;
    }
    // `liemo agent-test <provider> <project-id> <prompt> [session]`: one agent turn, events printed.
    if args.get(1).map(String::as_str) == Some("agent-test") {
        let (Some(p), Some(project), Some(prompt)) = (args.get(2).and_then(|p| agent::Provider::from_id(p)), args.get(3), args.get(4)) else {
            eprintln!("usage: liemo agent-test <claude|codex|gemini|opencode|grok> <project-id> <prompt> [session]");
            std::process::exit(2);
        };
        let (tx, rx) = std::sync::mpsc::channel();
        let turn = agent::Turn { provider: p, model: String::new(), prompt: prompt.clone(), session: args.get(5).cloned(), project_id: project.clone(), project_name: "Test".into() };
        agent::start(turn, move |e| {
            let _ = tx.send(e);
        });
        for e in rx {
            println!("{e:?}");
            if matches!(e, agent::Event::Done(_)) {
                break;
            }
        }
        return;
    }
    // Release tooling (see update.rs and scripts/).
    match args.get(1).map(String::as_str) {
        Some("update-keygen") | Some("sign-update") | Some("app-icon") => {
            let r = match args[1].as_str() {
                "update-keygen" => update::keygen(std::path::Path::new(args.get(2).map(String::as_str).unwrap_or("liemo-update.key"))),
                "sign-update" => match (args.get(2), args.get(3)) {
                    (Some(k), Some(z)) => update::sign(k, std::path::Path::new(z)),
                    _ => Err(anyhow::anyhow!("usage: liemo sign-update <key-file|-> <zip>")),
                },
                _ => app_icon(args.get(2).map(String::as_str).unwrap_or("AppIcon.png")),
            };
            if let Err(e) = r {
                eprintln!("{e:#}");
                std::process::exit(1);
            }
            return;
        }
        _ => {}
    }
    if args.len() <= 1 || !args[1].starts_with("--") {
        run_app();
        return;
    }
    if args.get(1).map(String::as_str) == Some("--text-test") {
        // Every text animation as a strip of four moments, one PNG each.
        let dir = args.get(2).cloned().unwrap_or_else(|| ".".into());
        let mut t = model::assets::asset("tx-title").and_then(|a| a.text.clone()).unwrap();
        t.content = "Ship 2,400 videos\nwithout a timeline".into();
        t.size = 96.0;
        for (anim, _) in model::assets::TEXT_ANIMS {
            t.anim = anim;
            let mut c = Canvas::new(1280, 720);
            c.fill_all(render::canvas::hex("#101012"));
            for (i, lt) in [0.08f32, 0.25, 0.5, 1.6].iter().enumerate() {
                c.save();
                c.translate((i % 2) as f32 * 640.0, (i / 2) as f32 * 360.0);
                c.scale(0.5, 0.5);
                render::textclip::draw_text(&mut c, &t, *lt, 1.0);
                c.restore();
            }
            save_png(&c.into_pixmap(), &format!("{dir}/text-{}.png", model::assets::text_anim_id(anim)));
        }
        if std::env::var("RAYNA_SETTLE").is_ok() {
            // The settled picture of each animation over the plain text: red where they differ.
            t.content = "Ship 2,400 videos\nwithout a timeline".into();
            for (anim, _) in model::assets::TEXT_ANIMS {
                t.anim = model::types::TextAnim::None;
                let mut a = Canvas::new(1280, 720);
                render::textclip::draw_text(&mut a, &t, 6.0, 1.0);
                t.anim = anim;
                let mut b = Canvas::new(1280, 720);
                render::textclip::draw_text(&mut b, &t, 6.0, 1.0);
                let (pa, mut pb) = (a.into_pixmap(), b.into_pixmap());
                let mut n = 0;
                for (x, y) in pa.data().chunks(4).zip(pb.data_mut().chunks_mut(4)) {
                    if (x[3] as i32 - y[3] as i32).abs() > 24 {
                        n += 1;
                        y.copy_from_slice(&[255, 0, 0, 255]);
                    }
                }
                if n > 0 {
                    println!("{:?}: {n} px differ", anim);
                    save_png(&pb, &format!("{dir}/settle-{}.png", model::assets::text_anim_id(anim)));
                }
            }
        }
        for o in model::assets::overlays() {
            let mut c = Canvas::new(1280, 720);
            c.fill_all(render::canvas::hex("#2a2f3a"));
            for (i, lt) in [0.2f32, 0.5, 0.9, 1.4].iter().enumerate() {
                c.save();
                c.translate((i % 2) as f32 * 640.0, (i / 2) as f32 * 360.0);
                c.scale(0.5, 0.5);
                (o.draw.unwrap())(&mut c, *lt);
                c.restore();
            }
            save_png(&c.into_pixmap(), &format!("{dir}/overlay-{}.png", o.id));
        }
        return;
    }
    if args.get(1).map(String::as_str) == Some("--render-test") {
        let dir = args.get(2).cloned().unwrap_or_else(|| ".".into());
        let clips = sample_clips();
        let tracks = default_tracks();
        for t in [0.3, 2.0, 6.0, 8.0, 11.0, 13.0, 17.0, 18.5] {
            let start = std::time::Instant::now();
            let mut c = Canvas::new(1280, 720);
            render_frame(&mut c, &Scene { clips: &clips, tracks: &tracks }, t, &mut NoFrames);
            println!("t={t}: {:?}", start.elapsed());
            save_png(&c.into_pixmap(), &format!("{dir}/frame-{t}.png"));
        }
        for id in ["ascii", "braille", "rain", "moon", "vhs", "crt", "kaleidoscope", "sketch", "thermal", "dither", "tilt-shift", "glitch", "blocks"] {
            save_png(&render::thumbs::effect_thumb(id), &format!("{dir}/fx-{id}.png"));
        }
        // Installed fonts: a few families at two weights.
        let fams = render::text::system_families();
        println!("{} installed font families", fams.len());
        let mut c = Canvas::new(1280, 720);
        c.fill_all(render::canvas::hex("#101012"));
        for (i, fam) in ["Georgia", "Avenir Next", "Futura", "Didot", "Snell Roundhand", "SF Pro", "Helvetica Neue", "Baskerville"].iter().enumerate() {
            let y = 70.0 + i as f32 * 82.0;
            for (j, w) in [400.0, 700.0].iter().enumerate() {
                let st = render::text::TextStyle { family: fam, weight: *w, size: 48.0, spacing: 0.0 };
                render::text::fill_text(&mut c, &format!("{fam} {w}"), 40.0 + j as f32 * 620.0, y, &st, render::text::Align::Left, render::text::Baseline::Alphabetic, render::canvas::hex("#ffffff"));
            }
        }
        save_png(&c.into_pixmap(), &format!("{dir}/fonts.png"));
        for b in fx::paper::backgrounds() {
            match fx::paper::render(&b.id, 2.0, 480, 270) {
                Some(p) => save_png(&p, &format!("{dir}/bg-{}.png", b.id)),
                None => println!("background {} failed", b.id),
            }
        }
        for f in &fx::remocn::FILTERS {
            save_png(&render::thumbs::effect_thumb(f.id), &format!("{dir}/fx-{}.png", f.id));
        }
        for l in fx::looks::LOOKS {
            save_png(&render::thumbs::effect_thumb(l.id), &format!("{dir}/look-{}.png", l.id));
        }
        for tr in fx::transitions::TRANSITIONS.iter().filter(|t| t.id.starts_with("remocn-")) {
            for (i, p) in [0.1f32, 0.3, 0.5, 0.7, 0.9].iter().enumerate() {
                save_png(&render::thumbs::transition_at(tr.id, *p), &format!("{dir}/trp-{}-{i}.png", tr.id));
            }
        }
        for id in ["clock", "soft-wipe", "blinds", "spin", "blur"] {
            save_png(&render::thumbs::transition_thumb(id), &format!("{dir}/tr-{id}.png"));
        }
        // Grid flash and its id variants (order and grid size ride in the id), at a moment
        // mid-transition so cells flash, hold and reveal.
        for id in ["grid-flash", "grid-flash-4x4-random", "grid-flash-3x3-up", "grid-flash-6x6"] {
            for (i, p) in [0.25f32, 0.5, 0.75].iter().enumerate() {
                save_png(&render::thumbs::transition_at(id, *p), &format!("{dir}/tr-{id}-{i}.png"));
            }
        }
        // Grid flash against synthetic frames: solid red out, solid blue in — the probe then
        // reads each cell's state unambiguously.
        let (ga, gb) = {
            let mut a = Canvas::new(384, 216);
            a.fill_all(render::canvas::hex("#d92b2b"));
            let mut b = Canvas::new(384, 216);
            b.fill_all(render::canvas::hex("#2b52d9"));
            (a.into_pixmap(), b.into_pixmap())
        };
        for (i, p) in [0.1f32, 0.3, 0.5, 0.7, 0.9].iter().enumerate() {
            let mut c = Canvas::new(384, 216);
            fx::transitions::grid_flash_p(&mut c, Some(&ga), &gb, *p, 1.0, 3, 3, fx::transitions::GridOrder::Down, 0);
            let pm = c.into_pixmap();
            // Which way each cell has flipped: its middle pixel reads red (a) or blue (b).
            let cell = |col: usize, row: usize| {
                let x = (col * 128 + 64) as usize;
                let y = (row * 72 + 36) as usize;
                let d = pm.data();
                let px = &d[(y * 384 + x) * 4..(y * 384 + x) * 4 + 3];
                if px[2] > px[0] { 'B' } else { 'R' }
            };
            let grid: String = (0..3).map(|row| (0..3).map(|col| cell(col, row)).collect::<String>()).collect::<Vec<_>>().join(" ");
            println!("gf p={p}: {grid} (R=a, B=b)");
            save_png(&pm, &format!("{dir}/gf-syn-{i}.png"));
        }
        // Up reads the other way; random is a stable shuffled permutation, and 4×4 is 16 cells.
        let probe = |order: fx::transitions::GridOrder, seed: u64, cols: u32, rows: u32, label: &str| {
            let mut c = Canvas::new(384, 216);
            fx::transitions::grid_flash_p(&mut c, Some(&ga), &gb, 0.5, 1.0, cols, rows, order, seed);
            let pm = c.into_pixmap();
            let (cw, ch) = (384.0 / cols as f32, 216.0 / rows as f32);
            let mut grid = String::new();
            for row in 0..rows as usize {
                for col in 0..cols as usize {
                    let d = pm.data();
                    let (x, y) = ((col as f32 * cw + cw / 2.0) as usize, (row as f32 * ch + ch / 2.0) as usize);
                    let px = &d[(y * 384 + x) * 4..(y * 384 + x) * 4 + 3];
                    grid.push(if px[2] > px[0] { 'B' } else { 'R' });
                }
                grid.push(' ');
            }
            println!("gf {label} p=0.5: {grid}");
        };
        probe(fx::transitions::GridOrder::Up, 0, 3, 3, "3x3-up");
        probe(fx::transitions::GridOrder::Random, 7, 3, 3, "3x3-random#7");
        probe(fx::transitions::GridOrder::Random, 7, 3, 3, "3x3-random#7 again");
        probe(fx::transitions::GridOrder::Random, 8, 3, 3, "3x3-random#8");
        probe(fx::transitions::GridOrder::Random, fx::transitions::id_seed("clip-x1") as u64, 4, 4, "4x4-random");
        // Liquid glass text over a photograph, mid entrance.
        let mut c = Canvas::new(1280, 720);
        c.draw_image(&render::thumbs::format_preview(16.0, 9.0, 720), 0.0, 0.0);
        let glass = model::types::TextProps {
            content: "LIQUID GLASS".into(),
            font: "Bebas Neue".into(),
            size: 110.0,
            weight: 400.0,
            color: "#ffffff".into(),
            align: model::types::TextAlign::Center,
            anim: model::types::TextAnim::None,
            boxed: false,
            y: 0.0,
            blend: model::types::TextBlend::Normal,
            outline: None,
            outline_width: 6.0,
            shadow: None,
            glow: None,
            highlight: None,
            glass: Some("#9fd0ff".into()),
            tilt_x: 0.0,
            tilt_y: 0.0,
        };
        render::textclip::draw_text(&mut c, &glass, 2.0, 1.0);
        save_png(&c.into_pixmap(), &format!("{dir}/glass-text.png"));
        for id in ["tx-lower", "tx-quote", "tx-poster", "tx-note", "ov-frame", "ov-grain"] {
            save_png(&render::thumbs::asset_thumb_id(id).unwrap(), &format!("{dir}/as-{id}.png"));
        }
        return;
    }
    if args.get(1).map(String::as_str) == Some("--media-test") {
        media_test(&args[2], &args[3]);
    }
}

gpui::actions!(rayna, [Quit, CheckForUpdates]);

fn run_app() {
    use gpui::{px, size, App, AppContext, Bounds, KeyBinding, Menu, MenuItem, WindowBounds, WindowOptions};
    use std::borrow::Cow;
    media::init();
    gpui_platform::application().with_assets(ui::assets::Assets).run(|cx: &mut App| {
        gpui_component::init(cx);
        let fonts: Vec<Cow<'static, [u8]>> = vec![
            Cow::Borrowed(include_bytes!("../assets/fonts/ui/inter-400.ttf")),
            Cow::Borrowed(include_bytes!("../assets/fonts/ui/inter-500.ttf")),
            Cow::Borrowed(include_bytes!("../assets/fonts/ui/inter-600.ttf")),
            Cow::Borrowed(include_bytes!("../assets/fonts/ui/inter-700.ttf")),
            Cow::Borrowed(include_bytes!("../assets/fonts/ui/mona-sans-600.ttf")),
            Cow::Borrowed(include_bytes!("../assets/fonts/ui/mona-sans-700.ttf")),
            Cow::Borrowed(include_bytes!("../assets/fonts/ui/jetbrains-mono-400.ttf")),
            Cow::Borrowed(include_bytes!("../assets/fonts/ui/jetbrains-mono-500.ttf")),
            Cow::Borrowed(include_bytes!("../assets/fonts/ui/jetbrains-mono-600.ttf")),
            Cow::Borrowed(include_bytes!("../assets/fonts/ui/instrument-serif-400.ttf")),
            Cow::Borrowed(include_bytes!("../assets/fonts/ui/bebas-neue-400.ttf")),
            Cow::Borrowed(include_bytes!("../assets/fonts/ui/caveat-700.ttf")),
        ];
        if let Err(e) = cx.text_system().add_fonts(fonts) {
            eprintln!("Liemo: couldn't load fonts: {e}");
        }
        ui::theme::install(cx);
        cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
        cx.on_action(|_: &Quit, cx: &mut App| cx.quit());
        cx.on_action(|_: &CheckForUpdates, _: &mut App| update::check(true));
        cx.set_menus(vec![Menu::new("Liemo").items([
            MenuItem::action("Check for Updates…", CheckForUpdates),
            MenuItem::separator(),
            MenuItem::action("Quit Liemo", Quit),
        ])]);
        update::check_on_launch();
        // Index installed fonts and find the AI tools off the main thread.
        std::thread::spawn(|| {
            render::text::system_families();
        });
        std::thread::spawn(|| {
            for (p, _) in agent::installed() {
                p.models();
            }
        });
        // Once an update is in place, quit so the relaunch can start the new version.
        cx.spawn(async move |cx| loop {
            cx.background_executor().timer(std::time::Duration::from_millis(250)).await;
            if matches!(update::state(), update::State::Done) {
                cx.background_executor().timer(std::time::Duration::from_millis(600)).await;
                let _ = cx.update(|cx| cx.quit());
                break;
            }
        })
        .detach();
        let background = std::env::var("RAYNA_BACKGROUND").is_ok();
        let bounds = Bounds::centered(None, size(px(1440.), px(900.)), cx);
        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    window_min_size: Some(size(px(1100.), px(700.))),
                    // RAYNA_BACKGROUND=1 (test runs) opens without taking focus from what you're doing.
                    focus: !background,
                    // No system title bar: the app's own header carries the traffic lights and moves the window.
                    titlebar: Some(gpui::TitlebarOptions {
                        title: Some("Liemo".into()),
                        appears_transparent: true,
                        traffic_light_position: Some(gpui::point(px(18.), px(20.))),
                    }),
                    ..Default::default()
                },
                |window, cx| {
                    let app = cx.new(|cx| ui::app::RaynaApp::new(window, cx));
                    cx.new(|cx| gpui_component::Root::new(app, window, cx))
                },
            )
            .expect("open window");
        let _ = window;
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        if !background {
            cx.activate(true);
        }
    });
}

fn media_test(file: &str, dir: &str) {
    use std::sync::atomic::AtomicBool;
    let path = std::path::PathBuf::from(file);
    let imp = media::import::read_file(&path, "utest".into()).expect("import");
    println!("dur={} {}x{} audio={}", imp.record.dur, imp.record.width, imp.record.height, imp.record.has_audio);
    media::import::register_media(&imp.record, imp.thumb, imp.bitmap);
    let t = std::time::Instant::now();
    let (_, ok) = media::import::load_payload(&imp.record);
    println!("audio decoded={ok} in {:?}, frames={}", t.elapsed(), media::audio::buffer("utest").map(|b| b.frames()).unwrap_or(0));
    let mut d = media::video::VideoDecoder::open(&path).unwrap();
    d.set_max(1280, 720);
    let t = std::time::Instant::now();
    for i in 0..60 { d.frame_at(i as f64 / 30.0).unwrap(); }
    println!("60 sequential frames in {:?}", t.elapsed());
    let t = std::time::Instant::now();
    for x in [3.5, 0.2, 2.0, 1.1] { d.frame_at(x).unwrap(); }
    println!("4 seeks in {:?}", t.elapsed());
    save_png(&d.frame_at(2.0).unwrap(), &format!("{dir}/decoded-2s.png"));

    let mut clips = sample_clips();
    let tracks = default_tracks();
    let mut c = model::store::make_clip(&tracks, 0.0, "utest", Some("V1"), Some(21.0)).unwrap();
    c.transition = Some(model::types::TransitionRef { id: "dissolve".into(), dur: 1.0 });
    clips.push(c);
    let cancel = AtomicBool::new(false);
    let report = |f: f64, l: String| { if (f * 100.0) as i64 % 25 == 0 { println!("  {:.0}% {l}", f * 100.0) } };
    let prog = export::Progress { cancel: &cancel, report: &report };
    for (name, kind, container, ext, extra) in [
        ("mp4", export::Kind::Video, export::Container::Mp4, "mp4", ""),
        ("webm", export::Kind::Video, export::Container::Webm, "webm", ""),
        ("prores", export::Kind::Video, export::Container::Mov, "mov", ""),
        ("vertical", export::Kind::Video, export::Container::Mp4, "mp4", "9:16"),
        ("gif", export::Kind::Gif, export::Container::Mp4, "gif", ""),
        ("png", export::Kind::Image, export::Container::Mp4, "png", ""),
        ("wav", export::Kind::Audio, export::Container::Mp4, "wav", ""),
        ("m4a", export::Kind::Audio, export::Container::Mp4, "m4a", "m4a"),
    ] {
        let mut s = export::Settings { kind, container, resolution: 720, ..Default::default() };
        if extra == "9:16" { s.aspect = "9:16".into(); }
        if extra == "m4a" { s.audio_wav = false; }
        let (start, end) = if kind == export::Kind::Gif { (5.0, 8.0) } else { (18.0, 25.0) };
        let job = export::Job { settings: s, start, end, image_time: 21.5, clips: clips.clone(), tracks: tracks.clone(), path: format!("{dir}/out-{name}.{ext}").into() };
        let t = std::time::Instant::now();
        match export::run(&job, &prog) {
            Ok(o) => println!("{name}: {} bytes audio={} in {:?}", o.bytes, o.with_audio, t.elapsed()),
            Err(e) => println!("{name}: ERROR {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn katakana_fallback() {
        let st = crate::render::text::TextStyle { family: "JetBrains Mono", weight: 600.0, size: 40.0, spacing: 0.0 };
        let tile = crate::render::text::glyph_tile("ｱ", 48, &st, 0.0);
        let ink: u32 = tile.data().chunks(4).map(|p| p[3] as u32).sum();
        println!("ink={ink}");
        assert!(ink > 0);
    }
}
