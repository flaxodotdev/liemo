// Drives the engine the way the iOS app does, with a media host that has nothing to offer.

use liemo_mobile::api::{Engine, MediaFrame, MediaHost, MediaProbe};
use std::sync::Arc;

struct NoMedia;
impl MediaHost for NoMedia {
    fn probe(&self, _: String) -> Option<MediaProbe> {
        None
    }
    fn video_frame(&self, _: String, _: f64, _: u32, _: u32) -> Option<MediaFrame> {
        None
    }
    fn decode_image(&self, _: String, _: u32) -> Option<MediaFrame> {
        None
    }
    fn decode_audio(&self, _: String) -> Option<Vec<f32>> {
        None
    }
    fn segment(&self, _: MediaFrame) -> Option<MediaFrame> {
        None
    }
}

/// The first clip of a type in the project.
fn clip_of(e: &Engine, project: &str, kind: &str) -> String {
    let doc: serde_json::Value = serde_json::from_str(&e.call("get_project".into(), format!(r#"{{"project":"{project}"}}"#)).text).unwrap();
    doc["clips"].as_array().unwrap().iter().find(|c| c["type"] == kind).expect(kind)["id"].as_str().unwrap().to_string()
}

#[test]
fn edit_undo_render() {
    let dir = std::env::temp_dir().join(format!("liemo-mobile-tests-{}", std::process::id()));
    let e = Engine::new(dir.display().to_string(), Arc::new(NoMedia));
    let made = e.call("create_project".into(), r#"{"template":"sample"}"#.into());
    assert!(made.ok, "{}", made.text);
    let id = serde_json::from_str::<serde_json::Value>(&made.text).unwrap()["id"].as_str().unwrap().to_string();

    assert!(e.open_project(id.clone()).ok);
    assert!(!e.can_undo());
    let title = clip_of(&e, &id, "text");
    let r = e.call("update_clip".into(), format!(r#"{{"project":"{id}","clip":"{title}","text":{{"content":"Hello iPhone"}}}}"#));
    assert!(r.ok, "{}", r.text);
    assert!(e.can_undo());
    let shown = |e: &Engine| e.call("get_project".into(), format!(r#"{{"project":"{id}"}}"#)).text;
    assert!(shown(&e).contains("Hello iPhone"));
    assert!(e.call("undo".into(), format!(r#"{{"project":"{id}"}}"#)).ok);
    assert!(!shown(&e).contains("Hello iPhone"));
    assert!(e.call("redo".into(), format!(r#"{{"project":"{id}"}}"#)).ok);

    let f = e.render_frame(2.0, 640).expect("a frame");
    assert_eq!((f.width, f.height), (640, 360));
    assert!(f.rgba.chunks(4).any(|p| p[0] > 40 || p[1] > 40 || p[2] > 40), "frame is black");
    let audio = e.render_audio(1.0, 4800);
    assert!(audio.iter().any(|s| s.abs() > 0.001), "no music");

    let defs: serde_json::Value = serde_json::from_str(&e.tool_definitions()).unwrap();
    assert!(defs.as_array().unwrap().iter().all(|t| t["name"] != "export"));

    e.close_project();
    assert!(std::path::Path::new(&e.project_thumb_path(id)).exists());
}

#[test]
fn waveform_peaks() {
    let dir = std::env::temp_dir().join(format!("liemo-mobile-tests-{}", std::process::id()));
    let e = Engine::new(dir.display().to_string(), Arc::new(NoMedia));
    let made = e.call("create_project".into(), r#"{"template":"sample"}"#.into());
    let id = serde_json::from_str::<serde_json::Value>(&made.text).unwrap()["id"].as_str().unwrap().to_string();
    e.open_project(id.clone());
    let p = e.audio_peaks(clip_of(&e, &id, "music"), 64);
    assert_eq!(p.len(), 64);
    assert!(p.iter().filter(|v| **v > 0.1).count() > 32, "{p:?}");
}

#[test]
fn new_styles_and_motion_render() {
    let dir = std::env::temp_dir().join(format!("liemo-mobile-tests-{}", std::process::id()));
    let e = Engine::new(dir.display().to_string(), Arc::new(NoMedia));
    let made = e.call("create_project".into(), r#"{"template":"sample"}"#.into());
    let id = serde_json::from_str::<serde_json::Value>(&made.text).unwrap()["id"].as_str().unwrap().to_string();
    e.open_project(id.clone());
    let doc: serde_json::Value = serde_json::from_str(&e.call("get_project".into(), format!(r#"{{"project":"{id}"}}"#)).text).unwrap();
    let title = doc["clips"].as_array().unwrap().iter().find(|c| c["type"] == "text").unwrap().clone();
    let (tid, t0) = (title["id"].as_str().unwrap().to_string(), title["start"].as_f64().unwrap() + 1.0);
    let main = doc["clips"].as_array().unwrap().iter().find(|c| c["track"] == "V1").unwrap().clone();
    let mid = main["id"].as_str().unwrap().to_string();
    let frame = |e: &Engine, t: f64| e.render_frame(t, 320).unwrap().rgba;
    let call = |e: &Engine, tool: &str, args: String| {
        let r = e.call(tool.into(), args);
        assert!(r.ok, "{tool}: {}", r.text);
        r
    };

    let plain = frame(&e, t0);
    for (k, v) in [("outline", "\"#ff0000\""), ("shadow", "\"#000000\""), ("glow", "\"#00ffcc\""), ("highlight", "\"#ffd400\"")] {
        call(&e, "update_clip", format!(r#"{{"project":"{id}","clip":"{tid}","text":{{"{k}":{v}}}}}"#));
    }
    assert_ne!(plain, frame(&e, t0), "text styles changed nothing");
    for anim in ["bounce", "zoom", "blur", "pop"] {
        call(&e, "update_clip", format!(r#"{{"project":"{id}","clip":"{tid}","text":{{"animation":"{anim}"}}}}"#));
        let start = title["start"].as_f64().unwrap();
        let _ = frame(&e, start + 0.1);
    }

    let before = frame(&e, main["start"].as_f64().unwrap() + 0.2);
    let dur = main["dur"].as_f64().unwrap();
    call(&e, "update_clip", format!(r#"{{"project":"{id}","clip":"{mid}","speed":2,"reverse":true,"flipH":true,"animIn":"zoom-in","animOut":"slide-left"}}"#));
    let doc: serde_json::Value = serde_json::from_str(&e.call("get_project".into(), format!(r#"{{"project":"{id}"}}"#)).text).unwrap();
    let m = doc["clips"].as_array().unwrap().iter().find(|c| c["id"] == mid.as_str()).unwrap().clone();
    assert!((m["dur"].as_f64().unwrap() - dur / 2.0).abs() < 0.05, "speed 2 should halve the length: {m}");
    assert_eq!(m["motion"]["reverse"], true);
    assert_ne!(before, frame(&e, m["start"].as_f64().unwrap() + 0.2), "motion changed nothing");

    let r = call(&e, "apply_effect", format!(r#"{{"project":"{id}","clip":"{mid}","effect":"adjust","params":{{"brightness":40,"vignette":60,"grain":30,"sharpen":50,"warmth":30}}}}"#));
    assert!(r.text.contains("uid"));
    let _ = frame(&e, m["start"].as_f64().unwrap() + 0.3);
}

#[test]
fn every_new_effect_and_animation_renders() {
    let dir = std::env::temp_dir().join(format!("liemo-mobile-tests-{}", std::process::id()));
    let e = Engine::new(dir.display().to_string(), Arc::new(NoMedia));
    let made = e.call("create_project".into(), r#"{"template":"sample"}"#.into());
    let id = serde_json::from_str::<serde_json::Value>(&made.text).unwrap()["id"].as_str().unwrap().to_string();
    e.open_project(id.clone());
    let doc: serde_json::Value = serde_json::from_str(&e.call("get_project".into(), format!(r#"{{"project":"{id}"}}"#)).text).unwrap();
    let main = doc["clips"].as_array().unwrap().iter().find(|c| c["track"] == "V1").unwrap().clone();
    let (mid, at) = (main["id"].as_str().unwrap().to_string(), main["start"].as_f64().unwrap() + 0.7);
    // Remove the sample's own effects so each new one is judged alone.
    for fx in main["effects"].as_array().unwrap() {
        e.call("remove_effect".into(), format!(r#"{{"project":"{id}","clip":"{mid}","uid":"{}"}}"#, fx["uid"].as_str().unwrap()));
    }
    let plain = e.render_frame(at, 320).unwrap().rgba;
    let ids = [
        "flash", "strobe", "invert-flash", "zoom-pulse", "push-in", "handheld", "speed-lines", "light-leak", "lens-flare", "bokeh", "film-burn",
        "film-8mm", "rainfall", "snow", "hearts", "confetti", "embers", "sparkles", "split-2", "split-3", "split-4", "border", "blur-fill", "color-pop", "fisheye",
    ];
    let mut same = vec![];
    for fx in ids {
        let r = e.call("apply_effect".into(), format!(r#"{{"project":"{id}","clip":"{mid}","effect":"{fx}"}}"#));
        assert!(r.ok, "{fx}: {}", r.text);
        let uid = serde_json::from_str::<serde_json::Value>(&r.text).unwrap()["uid"].as_str().unwrap().to_string();
        // Some pulse effects are idle at one instant; try a few.
        let changed = [0.0, 0.13, 0.31].iter().any(|d| e.render_frame(at + d, 320).unwrap().rgba != plain);
        if !changed {
            same.push(fx);
        }
        e.call("remove_effect".into(), format!(r#"{{"project":"{id}","clip":"{mid}","uid":"{uid}"}}"#));
    }
    assert!(same.is_empty(), "no visible change from {same:?}");
    for anim in ["whip-left", "drop", "rise", "swing", "flip", "stretch", "shake"] {
        assert!(e.call("update_clip".into(), format!(r#"{{"project":"{id}","clip":"{mid}","animIn":"{anim}"}}"#)).ok);
        let _ = e.render_frame(main["start"].as_f64().unwrap() + 0.1, 200);
    }
    for anim in ["breathe", "float", "wiggle", "pulse", "spin", "shake", "sway"] {
        assert!(e.call("update_clip".into(), format!(r#"{{"project":"{id}","clip":"{mid}","animLoop":"{anim}"}}"#)).ok);
        let _ = e.render_frame(at, 200);
    }
    let title = clip_of(&e, &id, "text");
    let start = serde_json::from_str::<serde_json::Value>(&e.call("get_project".into(), format!(r#"{{"project":"{id}"}}"#)).text).unwrap()["clips"]
        .as_array().unwrap().iter().find(|c| c["id"] == title.as_str()).unwrap()["start"].as_f64().unwrap();
    for anim in ["wave", "glitch", "flicker"] {
        assert!(e.call("update_clip".into(), format!(r##"{{"project":"{id}","clip":"{title}","text":{{"animation":"{anim}","outline":"#000000"}}}}"##)).ok);
        for d in [0.05, 0.2, 0.9] {
            let _ = e.render_frame(start + d, 200);
        }
    }
}

#[test]
fn cutout_tilt_and_thirds_render() {
    let dir = std::env::temp_dir().join(format!("liemo-mobile-tests-{}", std::process::id()));
    let e = Engine::new(dir.display().to_string(), Arc::new(NoMedia));
    let made = e.call("create_project".into(), r#"{"template":"sample"}"#.into());
    let id = serde_json::from_str::<serde_json::Value>(&made.text).unwrap()["id"].as_str().unwrap().to_string();
    e.open_project(id.clone());
    let call = |tool: &str, args: String| {
        let r = e.call(tool.into(), args);
        assert!(r.ok, "{tool}: {}", r.text);
        r
    };
    // The main track's clips, in order (ids depend on what else was made in this process).
    let doc: serde_json::Value = serde_json::from_str(&e.call("get_project".into(), format!(r#"{{"project":"{id}"}}"#)).text).unwrap();
    let mut main: Vec<(String, f64)> = doc["clips"].as_array().unwrap().iter().filter(|c| c["track"] == "V1")
        .map(|c| (c["id"].as_str().unwrap().to_string(), c["start"].as_f64().unwrap())).collect();
    main.sort_by(|a, b| a.1.total_cmp(&b.1));
    let (c1, c2, c3) = (main[0].0.clone(), main[1].0.clone(), main[2].0.clone());
    // Chroma key with a generous range takes something out of the first clip.
    let before = e.render_frame(3.0, 240).unwrap().rgba;
    call("apply_effect", format!(r##"{{"project":"{id}","clip":"{c1}","effect":"chroma-key","params":{{"color":"#3a6f9a","similarity":0.9,"outline":6}}}}"##));
    assert_ne!(before, e.render_frame(3.0, 240).unwrap().rgba, "chroma key changed nothing");
    // Without an AI on this machine, Remove background leaves the picture alone (and doesn't fail).
    call("apply_effect", format!(r#"{{"project":"{id}","clip":"{c2}","effect":"remove-bg","params":{{"background":"blur"}}}}"#));
    let _ = e.render_frame(7.0, 240);
    // Tilted text looks different from flat text.
    let title = clip_of(&e, &id, "text");
    let doc: serde_json::Value = serde_json::from_str(&e.call("get_project".into(), format!(r#"{{"project":"{id}"}}"#)).text).unwrap();
    let t = doc["clips"].as_array().unwrap().iter().find(|c| c["id"] == title.as_str()).unwrap()["start"].as_f64().unwrap() + 1.5;
    let flat = e.render_frame(t, 320).unwrap().rgba;
    call("update_clip", format!(r#"{{"project":"{id}","clip":"{title}","text":{{"tiltX":30,"tiltY":-35}}}}"#));
    assert_ne!(flat, e.render_frame(t, 320).unwrap().rgba, "tilt changed nothing");
    // Thirds, half way through, shows both clips.
    call("set_transition", format!(r#"{{"project":"{id}","clip":"{c3}","transition":"thirds","duration":1.0}}"#));
    let _ = e.render_frame(11.0, 240);
    // Titles at the same moment both stay where they were put.
    let a = call("add_text", format!(r#"{{"project":"{id}","content":"ONE","start":12.0,"duration":2.0}}"#));
    let b = call("add_text", format!(r#"{{"project":"{id}","content":"TWO","start":12.5,"duration":2.0}}"#));
    let start = |r: &liemo_mobile::api::ToolResult| serde_json::from_str::<serde_json::Value>(&r.text).unwrap()["clip"]["start"].as_f64().unwrap();
    assert_eq!((start(&a), start(&b)), (12.0, 12.5));
}
