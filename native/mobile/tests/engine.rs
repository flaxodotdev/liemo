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

#[test]
fn title_transitions_stay_on_the_title() {
    let dir = std::env::temp_dir().join(format!("liemo-mobile-tests-{}", std::process::id()));
    let e = Engine::new(dir.display().to_string(), Arc::new(NoMedia));
    let made = e.call("create_project".into(), r#"{"template":"sample"}"#.into());
    let id = serde_json::from_str::<serde_json::Value>(&made.text).unwrap()["id"].as_str().unwrap().to_string();
    e.open_project(id.clone());
    let at = 2.3;
    let without = e.render_frame(at, 320).unwrap();
    let r = e.call("add_text".into(), format!(r#"{{"project":"{id}","content":"HI","start":2.0,"duration":3.0,"animation":"none"}}"#));
    let tid = serde_json::from_str::<serde_json::Value>(&r.text).unwrap()["clip"]["id"].as_str().unwrap().to_string();
    assert!(e.call("set_transition".into(), format!(r#"{{"project":"{id}","clip":"{tid}","transition":"dip-black","duration":0.6}}"#)).ok);
    let with = e.render_frame(at, 320).unwrap();
    // A corner far from the words is untouched: the dip happens to the title, not the frame.
    let px = |f: &MediaFrame, x: u32, y: u32| f.rgba[((y * f.width + x) * 4) as usize..((y * f.width + x) * 4 + 3) as usize].to_vec();
    for (x, y) in [(4, 4), (with.width - 5, 4), (4, with.height - 5), (with.width - 5, with.height - 5)] {
        assert_eq!(px(&with, x, y), px(&without, x, y), "corner {x},{y} changed");
    }
}

#[test]
fn keyframes_ramps_and_captions() {
    use liemo_mobile::model::keys::{self, Ease, Key};
    use liemo_mobile::model::store::Store;
    use liemo_mobile::model::types::AssetType;
    let dir = std::env::temp_dir().join(format!("liemo-mobile-tests-{}", std::process::id()));
    let e = Engine::new(dir.display().to_string(), Arc::new(NoMedia));
    let made = e.call("create_project".into(), r#"{"template":"sample"}"#.into());
    let id = serde_json::from_str::<serde_json::Value>(&made.text).unwrap()["id"].as_str().unwrap().to_string();
    e.open_project(id.clone());
    let title = clip_of(&e, &id, "text");

    // Opacity keyed from 0 to 1 over the title: hidden at its start, shown by its end.
    let doc: serde_json::Value = serde_json::from_str(&e.call("get_project".into(), format!(r#"{{"project":"{id}"}}"#)).text).unwrap();
    let t0 = doc["clips"].as_array().unwrap().iter().find(|c| c["id"] == title.as_str()).unwrap()["start"].as_f64().unwrap();
    let plain = e.render_frame(t0 + 0.05, 320).unwrap();
    let r = e.call("set_keyframes".into(), format!(r#"{{"project":"{id}","clip":"{title}","property":"opacity","keys":[{{"time":{},"value":0}},{{"time":{},"value":1,"ease":"smooth"}}]}}"#, t0, t0 + 2.0));
    assert!(r.ok, "{}", r.text);
    let keyed = e.render_frame(t0 + 0.05, 320).unwrap();
    assert_ne!(plain.rgba, keyed.rgba, "opacity keyframes changed nothing");
    assert!(!e.call("set_keyframes".into(), format!(r#"{{"project":"{id}","clip":"{title}","property":"nope","keys":[]}}"#)).ok);

    // Captions from cues land on Titles; export writes them back out.
    let r = e.call("add_captions".into(), format!(r#"{{"project":"{id}","cues":[{{"start":1,"end":2,"text":"Hello"}},{{"start":2,"end":3.5,"text":"world"}}],"style":"pop"}}"#));
    assert!(r.ok, "{}", r.text);
    assert_eq!(serde_json::from_str::<serde_json::Value>(&r.text).unwrap()["added"], 2);
    let srt = std::env::temp_dir().join(format!("liemo-test-{}.srt", std::process::id()));
    let r = e.call("export_captions".into(), format!(r#"{{"project":"{id}","path":"{}"}}"#, srt.display()));
    assert!(r.ok, "{}", r.text);
    let text = std::fs::read_to_string(&srt).unwrap();
    assert!(text.contains("00:00:01,000 --> 00:00:02,000\nHello"), "{text}");
    let _ = std::fs::remove_file(&srt);
    e.close_project();

    // Store: splitting a ramped clip keeps the footage continuous; freeze frames ripple.
    let mut st = Store::new("t".into(), liemo_mobile::model::store::sample_clips(), vec![], vec![]);
    let v = st.clips.iter().find(|c| c.track == "V1").unwrap().id.clone();
    st.update_clip(&v, |c| {
        c.keys.insert("speed".into(), vec![Key { t: 0.0, v: 1.0, ease: Ease::Smooth }, Key { t: 4.0, v: 3.0, ease: Ease::Linear }]);
    });
    let whole = st.clip(&v).unwrap().clone();
    st.select(Some(v.clone()));
    st.time = 2.5;
    st.split();
    let b = st.selected_clip().unwrap().clone();
    assert!((b.in_ - (whole.in_ + whole.src_offset(2.5))).abs() < 1e-9);
    assert!((b.src_offset(1.0) - (whole.src_offset(3.5) - whole.src_offset(2.5))).abs() < 1e-9, "ramp jumps at the cut");
    assert!((keys::value_at(&b.keys["speed"], 0.0).unwrap() - keys::value_at(&whole.keys["speed"], 2.5).unwrap()).abs() < 1e-9);

    let mut st = Store::new("t".into(), liemo_mobile::model::store::sample_clips(), vec![], vec![]);
    let first = st.clips.iter().filter(|c| c.track == "V1").min_by(|a, b| a.start.total_cmp(&b.start)).unwrap().clone();
    let end_before = st.clips.iter().filter(|c| c.track == "V1").map(|c| c.start + c.dur).fold(0.0, f64::max);
    st.time = first.start + 2.0;
    st.freeze_frame(&first.id, 1.5);
    let held = st.selected_clip().unwrap().clone();
    assert!(held.frozen() && (held.start - (first.start + 2.0)).abs() < 1e-6 && (held.dur - 1.5).abs() < 1e-6);
    let end_after = st.clips.iter().filter(|c| c.track == "V1").map(|c| c.start + c.dur).fold(0.0, f64::max);
    assert!((end_after - end_before - 1.5).abs() < 1e-6, "{end_before} → {end_after}");

    // Multiple selection: copy two clips and paste them, keeping their spacing.
    let mut st = Store::new("t".into(), liemo_mobile::model::store::sample_clips(), vec![], vec![]);
    let titles: Vec<_> = st.clips.iter().filter(|c| c.kind == AssetType::Text).take(2).cloned().collect();
    st.select_many(titles.iter().map(|c| c.id.clone()).collect());
    assert!(st.many_selected());
    st.copy();
    st.paste(None, Some(30.0));
    let pasted = st.selected_clips();
    assert_eq!(pasted.len(), 2);
    assert!((pasted[1].start - pasted[0].start - (titles[1].start - titles[0].start)).abs() < 1e-6);
    st.remove();
    assert!(st.clips.iter().all(|c| c.start < 29.0));
}

#[test]
fn tracks_markers_masks_and_grading() {
    let dir = std::env::temp_dir().join(format!("liemo-mobile-tests-{}", std::process::id()));
    let e = Engine::new(dir.display().to_string(), Arc::new(NoMedia));
    let made = e.call("create_project".into(), r#"{"template":"sample"}"#.into());
    let id = serde_json::from_str::<serde_json::Value>(&made.text).unwrap()["id"].as_str().unwrap().to_string();
    e.open_project(id.clone());
    let call = |name: &str, args: String| {
        let r = e.call(name.into(), args);
        assert!(r.ok, "{name}: {}", r.text);
        serde_json::from_str::<serde_json::Value>(&r.text).unwrap()
    };
    let doc = || call("get_project", format!(r#"{{"project":"{id}"}}"#));

    // A new picture track sits in front of the others and draws on top.
    let before = e.render_frame(3.0, 320).unwrap();
    let t = call("add_track", format!(r#"{{"project":"{id}","kind":"video","name":"Cutaways"}}"#));
    let tid = t["track"]["id"].as_str().unwrap().to_string();
    assert_eq!(t["order"][1], tid.as_str(), "{t}");
    call("add_clip", format!(r#"{{"project":"{id}","asset":"sc-aurora","track":"{tid}","start":2,"duration":3}}"#));
    let after = e.render_frame(3.0, 320).unwrap();
    assert_ne!(before.rgba, after.rgba, "the new track didn't draw");
    assert!(!e.call("remove_track".into(), format!(r#"{{"project":"{id}","track":"V1"}}"#)).ok);
    call("remove_track", format!(r#"{{"project":"{id}","track":"{tid}"}}"#));
    assert_eq!(e.render_frame(3.0, 320).unwrap().rgba, before.rgba);
    assert!(e.call("undo".into(), format!(r#"{{"project":"{id}"}}"#)).ok);
    assert!(doc()["tracks"].as_array().unwrap().iter().any(|t| t["id"] == tid.as_str()), "undo brings the track back");

    // Markers and in/out are saved with the project.
    call("set_markers", format!(r#"{{"project":"{id}","markers":[{{"time":4,"name":"Drop"}},{{"time":1.5}}],"in":1,"out":9}}"#));
    let d = doc();
    assert_eq!(d["markers"][0]["t"], 1.5);
    assert_eq!(d["markers"][1]["name"], "Drop");
    assert_eq!((d["in"].as_f64(), d["out"].as_f64()), (Some(1.0), Some(9.0)));

    // A mask, a blend mode, colour wheels and a missing LUT all render without trouble.
    let v1 = clip_of(&e, &id, "scene");
    let plain = e.render_frame(1.0, 320).unwrap();
    call("update_clip", format!(r#"{{"project":"{id}","clip":"{v1}","mask":{{"shape":"ellipse","w":400,"h":300,"feather":30}}}}"#));
    let masked = e.render_frame(1.0, 320).unwrap();
    assert_ne!(plain.rgba, masked.rgba);
    let corner = |f: &MediaFrame| f.rgba[0..3].to_vec();
    assert!(corner(&masked).iter().all(|v| *v < 60), "outside the mask should be dark: {:?}", corner(&masked));
    call("update_clip", format!(r#"{{"project":"{id}","clip":"{v1}","mask":null,"blend":"screen"}}"#));
    let fx = call("apply_effect", format!(r#"{{"project":"{id}","clip":"{v1}","effect":"grade","params":{{"gain_hue":30,"gain_amt":0.8,"lift_lum":-0.3}}}}"#));
    assert!(fx.to_string().contains("grade"), "{fx}");
    let graded = e.render_frame(1.0, 320).unwrap();
    assert_ne!(graded.rgba, plain.rgba);
    call("apply_effect", format!(r#"{{"project":"{id}","clip":"{v1}","effect":"lut","params":{{"file":"/nonexistent.cube"}}}}"#));
    assert!(e.render_frame(1.0, 320).is_some());
    e.close_project();
}
