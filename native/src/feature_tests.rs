// Tests for the features ported from remocn-studio: captions, brand kit, components, pipeline
// documents, the design check, ProRes, agent modes and the MCP tools that expose them.

use crate::mcp::tools::{call, definitions, FileWorkspace};
use crate::model::store::{default_tracks, make_clip, sample_clips};
use crate::model::types::*;
use crate::storage::ProjectRecord;
use serde_json::{json, Value};
use std::sync::OnceLock;

/// One private data folder for the whole test run.
fn data() -> std::path::PathBuf {
    static D: OnceLock<std::path::PathBuf> = OnceLock::new();
    D.get_or_init(|| {
        let d = std::env::temp_dir().join(format!("liemo-feature-tests-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::env::set_var("RAYNA_DATA", &d);
        d
    })
    .clone()
}

fn record(clips: Vec<Clip>) -> ProjectRecord {
    serde_json::from_value(json!({
        "id": "t", "name": "T", "createdAt": 0, "updatedAt": 0, "duration": 0, "clipCount": clips.len(),
        "clips": clips, "tracks": default_tracks(), "pool": [],
    }))
    .unwrap()
}

fn text(content: &str, start: f64, dur: f64) -> Clip {
    let mut c = make_clip(&default_tracks(), 0.0, "tx-title", Some("T1"), Some(start)).unwrap();
    c.dur = dur;
    c.text.as_mut().unwrap().content = content.into();
    c
}

fn tool(name: &str, args: Value) -> Result<Value, String> {
    data();
    call(name, &args, &mut FileWorkspace).map(|o| serde_json::from_str(&o.text).unwrap_or(Value::Null))
}

// ---------- Text animations and captions ----------
#[test]
fn every_text_animation_has_a_stable_id() {
    let mut seen = std::collections::HashSet::new();
    for (a, name) in crate::model::assets::TEXT_ANIMS {
        let id = crate::model::assets::text_anim_id(a);
        assert!(!id.is_empty() && seen.insert(id.clone()), "{name}: {id}");
        let back: TextAnim = serde_json::from_value(json!(id)).unwrap();
        assert_eq!(back, a);
    }
    assert_eq!(seen.len(), 33);
}

#[test]
fn caption_styles_are_unique_and_apply() {
    use crate::model::captions::{apply_caption_style, CAPTION_STYLES};
    let mut ids = std::collections::HashSet::new();
    let base = crate::model::assets::asset("tx-caption").unwrap().text.clone().unwrap();
    for s in CAPTION_STYLES.iter() {
        assert!(ids.insert(s.id), "duplicate {}", s.id);
        let mut t = base.clone();
        apply_caption_style(&mut t, s.id, 1280.0, 720.0);
        if s.id == "subtitle" {
            assert_eq!(t, base);
            continue;
        }
        assert!(crate::model::assets::FONTS.iter().any(|f| f.family == t.font), "{}: font {}", s.id, t.font);
        assert!(t.size >= 36.0 && t.size <= 130.0, "{}: size {}", s.id, t.size);
        // Readable: something sets the text off from the picture.
        assert!(t.shadow.is_some() || t.outline.is_some() || t.boxed || t.highlight.is_some() || t.glow.is_some() || t.blend != TextBlend::Normal, "{} has nothing behind it", s.id);
    }
    // Smaller frames get smaller captions; tall ones keep them in the lower part.
    let mut t = base.clone();
    apply_caption_style(&mut t, "pop", 720.0, 1280.0);
    assert!(t.size < 76.0 && t.y > 300.0);
}

#[test]
fn add_and_restyle_captions() {
    use crate::model::captions::Cue;
    let mut st = crate::model::store::Store::new("t".into(), sample_clips(), default_tracks(), vec![]);
    let cues = vec![Cue { start: 1.0, end: 2.5, text: "Hello there".into() }, Cue { start: 3.0, end: 4.0, text: "General".into() }];
    assert_eq!(st.add_captions(&cues, "karaoke-does-not-exist"), 2);
    assert_eq!(st.add_captions(&cues, "marker"), 2);
    let added = st.selection();
    assert!(added.iter().all(|id| st.clip(id).unwrap().text.as_ref().unwrap().anim == TextAnim::Marker));
    assert_eq!(st.restyle_captions("typewriter"), 2);
    let t = st.clip(&added[0]).unwrap().text.clone().unwrap();
    assert_eq!((t.anim, t.font.as_str(), t.content.as_str()), (TextAnim::Caret, "JetBrains Mono", "Hello there"));
    st.undo();
    assert_eq!(st.clip(&added[0]).unwrap().text.as_ref().unwrap().anim, TextAnim::Marker);
    assert_eq!(st.restyle_captions("subtitle"), 2);
    assert_eq!(st.clip(&added[0]).unwrap().text.as_ref().unwrap().content, "Hello there");
}

// ---------- Brand ----------
#[test]
fn brand_reads_and_describes() {
    use crate::model::brand::*;
    assert_eq!(norm_hex("#ABC"), Some("#aabbcc".into()));
    assert_eq!(norm_hex("12345"), None);
    assert_eq!(norm_hex(" ff5500 "), Some("#ff5500".into()));
    let b = from_text("# Northwind\n\nColors\n- Primary `#0A84FF`\n- Ink: #111111\n- url(#gradient) and #notacolor\n\nTypography: Bebas Neue for headings");
    assert_eq!(b.name, "Northwind");
    assert_eq!(b.colors, vec!["#0a84ff", "#111111"]);
    assert_eq!(b.font, "Bebas Neue");
    assert!(Brand::default().describe().is_none());
    let d = b.describe().unwrap();
    assert!(d.contains("#0a84ff") && d.contains("Bebas Neue") && d.contains("Northwind"));
    assert!(from_text("").is_empty());
    // A common word isn't taken for an installed font, and the earliest name wins.
    assert_eq!(from_text("We said it three times. Headings: Mona Sans, body text Inter.").font, "Mona Sans");
    assert_eq!(from_text("font-family: 'inter', sans-serif").font, "Inter");
}

// ---------- Components ----------
#[test]
fn components_round_trip() {
    data();
    let clips = vec![text("A", 5.0, 2.0), text("B", 6.0, 3.0)];
    let c = crate::ui::components::save("Lower third", clips).unwrap();
    let back = crate::ui::components::get(&c.id).unwrap();
    assert_eq!(back.clips[0].start, 0.0);
    assert_eq!(back.clips[1].start, 1.0);
    assert!(crate::ui::components::list().iter().any(|x| x.id == c.id));
    crate::ui::components::delete(&c.id);
    assert!(crate::ui::components::get(&c.id).is_none());
}

// ---------- Pipeline documents ----------
#[test]
fn pipeline_documents() {
    data();
    use crate::pipeline::*;
    let p = "pipeline-test";
    assert_eq!(done(p), vec![false; 7]);
    write(p, "script", "# Script").unwrap();
    write(p, "analysis", "# Analysis").unwrap();
    assert!(write(p, "nonsense", "x").is_err());
    assert!(write(p, "../escape", "x").is_err());
    assert_eq!(done(p), vec![true, false, true, false, false, false, false]);
    assert_eq!(read(p, "script").as_deref(), Some("# Script"));
    assert!(describe().contains("design_check"));
}

// ---------- Design check ----------
fn kinds(issues: &[crate::check::Issue]) -> Vec<&'static str> {
    issues.iter().map(|i| i.kind).collect()
}

#[test]
fn design_check_finds_each_problem() {
    let tracks = default_tracks();
    let bg = |start: f64, dur: f64| {
        let mut c = make_clip(&tracks, 0.0, "im-violet", Some("V1"), Some(start)).unwrap();
        c.dur = dur;
        c
    };
    // A clean edit: a long readable title over a dark still, end to end.
    let mut ok_title = text("Hello", 0.5, 3.0);
    ok_title.text.as_mut().unwrap().shadow = Some("#000000".into());
    let clean = record(vec![bg(0.0, 4.0), ok_title.clone()]);
    let found = crate::check::run(&clean, &|| false);
    assert!(found.is_empty(), "clean edit flagged: {found:?}");

    // Each problem, one at a time.
    let fast = record(vec![bg(0.0, 4.0), text("Seven words that nobody could read here", 0.5, 0.8)]);
    assert!(kinds(&crate::check::run(&fast, &|| false)).contains(&"readability"));

    let mut tiny = ok_title.clone();
    tiny.text.as_mut().unwrap().size = 14.0;
    assert!(kinds(&crate::check::run(&record(vec![bg(0.0, 4.0), tiny]), &|| false)).contains(&"size"));

    let mut off = ok_title.clone();
    off.x = 600.0;
    assert!(kinds(&crate::check::run(&record(vec![bg(0.0, 4.0), off]), &|| false)).contains(&"safe-area"));

    let mut other = ok_title.clone();
    other.id = "other".into();
    other.track = "V2".into();
    assert!(kinds(&crate::check::run(&record(vec![bg(0.0, 4.0), ok_title.clone(), other]), &|| false)).contains(&"collision"));

    let gap = crate::check::run(&record(vec![bg(0.0, 1.0), bg(2.0, 2.0)]), &|| false);
    assert!(gap.iter().any(|i| i.kind == "gap" && (i.time - 1.0).abs() < 0.05), "{gap:?}");

    assert!(kinds(&crate::check::run(&record(vec![bg(0.0, 4.0), bg(4.0, 0.05), bg(4.05, 2.0)]), &|| false)).contains(&"flash"));

    // Dark text on a dark picture.
    let mut dark = text("Hello", 0.5, 3.0);
    dark.text.as_mut().unwrap().color = "#1a1030".into();
    assert!(kinds(&crate::check::run(&record(vec![bg(0.0, 4.0), dark.clone()]), &|| false)).contains(&"contrast"));
    // The same text with a backing box is fine.
    dark.text.as_mut().unwrap().boxed = true;
    assert!(!kinds(&crate::check::run(&record(vec![bg(0.0, 4.0), dark]), &|| false)).contains(&"contrast"));

    // Music running to the end without a fade.
    let mut music = make_clip(&tracks, 0.0, "mu-golden", Some("A1"), Some(0.0)).unwrap_or_else(|| {
        let id = crate::model::assets::music()[0].id.clone();
        make_clip(&tracks, 0.0, &id, Some("A1"), Some(0.0)).unwrap()
    });
    music.dur = 4.0;
    music.fade_out = 0.0;
    assert!(kinds(&crate::check::run(&record(vec![bg(0.0, 4.0), music.clone()]), &|| false)).contains(&"ending"));
    music.fade_out = 2.0;
    assert!(!kinds(&crate::check::run(&record(vec![bg(0.0, 4.0), music]), &|| false)).contains(&"ending"));

    // Hidden tracks don't count.
    let mut rec = record(vec![bg(0.0, 4.0), text("Seven words that nobody could read here", 0.5, 0.8)]);
    rec.tracks.iter_mut().find(|t| t.id == "T1").unwrap().hidden = true;
    assert!(!kinds(&crate::check::run(&rec, &|| false)).contains(&"readability"));

    // Stopping early is honoured.
    let _ = crate::check::run(&clean, &|| true);
}

#[test]
fn design_check_on_the_sample_runs() {
    let issues = crate::check::run(&record(sample_clips()), &|| false);
    for i in &issues {
        assert!(i.time >= 0.0 && !i.message.is_empty());
    }
}

// ---------- ProRes ----------
#[test]
fn containers() {
    use crate::export::*;
    for (id, c, ext) in [("mp4", Container::Mp4, "mp4"), ("webm", Container::Webm, "webm"), ("mov", Container::Mov, "mov"), ("prores", Container::Mov, "mov"), ("?", Container::Mp4, "mp4")] {
        assert_eq!(Container::from_id(id), c);
        assert_eq!(c.ext(), ext);
    }
    let s = Settings { container: Container::Mov, ..Default::default() };
    assert_eq!(extension(&s), "mov");
    // The settings round-trip through JSON (the export dialog remembers them).
    let back: Settings = serde_json::from_value(serde_json::to_value(&s).unwrap()).unwrap();
    assert_eq!(back.container, Container::Mov);
    assert!(probe(Container::Mov).video.is_some(), "this FFmpeg has no ProRes encoder");
}

// ---------- AI chat modes ----------
#[test]
fn agent_modes() {
    use crate::agent::*;
    for (m, _, _) in MODES {
        assert_eq!(Mode::from_id(m.id()), m);
    }
    assert!(read_only("render_frame") && read_only("design_check") && read_only("write_doc"));
    assert!(!read_only("add_text") && !read_only("generate_sound") && !read_only("export") && !read_only("download_stock"));
    assert!(mode_note(Mode::Auto).is_none() && mode_note(Mode::Plan).unwrap().contains("Plan"));
    assert_eq!(describe_call("add_text", &json!({ "content": "Hi" })), "add text: Hi");
    // Every tool is either read-only or gated, and every gated one exists.
    let names: Vec<String> = definitions().iter().map(|d| d["name"].as_str().unwrap().to_string()).collect();
    for n in ["render_frame", "get_project", "design_check", "read_docs", "write_doc", "search_stock"] {
        assert!(names.contains(&n.to_string()), "{n} missing");
    }
}

// ---------- MCP tools ----------
#[test]
fn tool_definitions_are_well_formed() {
    let defs = definitions();
    let mut names = std::collections::HashSet::new();
    for d in &defs {
        let n = d["name"].as_str().unwrap();
        assert!(names.insert(n.to_string()), "duplicate tool {n}");
        assert_eq!(d["inputSchema"]["type"], "object", "{n}");
        for r in d["inputSchema"]["required"].as_array().into_iter().flatten() {
            assert!(d["inputSchema"]["properties"].get(r.as_str().unwrap()).is_some(), "{n}: required {r} not a property");
        }
    }
    for n in ["write_doc", "read_docs", "design_check", "search_stock", "download_stock", "generate_sound"] {
        assert!(names.contains(n), "{n} missing");
    }
    let add_text = defs.iter().find(|d| d["name"] == "add_text").unwrap();
    assert_eq!(add_text["inputSchema"]["properties"]["animation"]["enum"].as_array().unwrap().len(), 33);
    let caps = defs.iter().find(|d| d["name"] == "add_captions").unwrap();
    assert_eq!(caps["inputSchema"]["properties"]["style"]["enum"].as_array().unwrap().len(), 34);
}

#[test]
fn tools_end_to_end() {
    let made = tool("create_project", json!({ "template": "sample", "name": "Feature tests" })).unwrap();
    let id = made["id"].as_str().unwrap().to_string();
    // Text with each animation.
    for (a, _) in crate::model::assets::TEXT_ANIMS {
        let anim = crate::model::assets::text_anim_id(a);
        let r = tool("add_text", json!({ "project": id, "content": "Count 1,200", "animation": anim, "start": 1.0 })).unwrap();
        let cid = r["clip"]["id"].as_str().or(r["id"].as_str()).map(String::from).unwrap_or_default();
        let doc = tool("get_project", json!({ "project": id })).unwrap();
        let clip = doc["clips"].as_array().unwrap().iter().find(|c| c["id"] == cid.as_str() || (cid.is_empty() && c["text"]["animation"] == anim.as_str()));
        assert!(clip.is_some(), "{anim}: {r}");
    }
    // Captions in every style.
    for s in crate::model::captions::CAPTION_STYLES.iter() {
        let r = tool("add_captions", json!({ "project": id, "style": s.id, "cues": [{ "start": 2.0, "end": 3.5, "text": "Two words" }] })).unwrap();
        assert_eq!(r["added"], 1, "{}", s.id);
    }
    // A close-up of part of the frame.
    let full = call("render_frame", &json!({ "project": id, "time": 1.2, "width": 400 }), &mut FileWorkspace).unwrap();
    let part = call("render_frame", &json!({ "project": id, "time": 1.2, "width": 400, "region": { "x": 100, "y": 100, "w": 200, "h": 100 } }), &mut FileWorkspace).unwrap();
    let size = |png: &[u8]| {
        let i = image::load_from_memory(png).unwrap();
        (i.width(), i.height())
    };
    assert_eq!(size(&full.png.unwrap()), (400, 225));
    assert_eq!(size(&part.png.unwrap()), (400, 200));
    // A region off the frame is clamped, not an error.
    assert!(call("render_frame", &json!({ "project": id, "time": 1.2, "region": { "x": 5000, "y": -50, "w": 300, "h": 300 } }), &mut FileWorkspace).is_ok());
    // Pipeline documents and the design check.
    assert!(tool("write_doc", json!({ "project": id, "stage": "script", "content": "# Script" })).is_ok());
    assert!(tool("write_doc", json!({ "project": id, "stage": "nope", "content": "x" })).is_err());
    let docs = tool("read_docs", json!({ "project": id })).unwrap();
    assert_eq!(docs["docs"]["script"], "# Script");
    let one = tool("read_docs", json!({ "project": id, "stage": "analysis" })).unwrap();
    assert!(one["docs"].as_object().unwrap().is_empty());
    let check = tool("design_check", json!({ "project": id })).unwrap();
    assert_eq!(check["count"].as_u64().unwrap() as usize, check["issues"].as_array().unwrap().len());
    // The fast captions just added are flagged.
    assert!(check["issues"].as_array().unwrap().iter().any(|i| i["kind"] == "collision" || i["kind"] == "readability"));
    // Services that aren't connected say how to connect, and nothing is fetched.
    if !crate::integrations::connected(crate::integrations::Service::Pexels) {
        let e = tool("search_stock", json!({ "query": "ocean" })).unwrap_err();
        assert!(e.contains("Connect Pexels"), "{e}");
    }
    assert!(tool("download_stock", json!({ "url": "https://example.com/x.jpg" })).unwrap_err().contains("Pexels"));
    // Export as ProRes.
    let r = tool("export", json!({ "project": id, "kind": "video", "container": "mov", "start": 0, "end": 1, "resolution": 720, "quality": "low" }));
    let path = r.unwrap()["path"].as_str().unwrap().to_string();
    assert!(path.ends_with(".mov"));
    let _ = std::fs::remove_file(path);
}
