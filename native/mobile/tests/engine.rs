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
}

#[test]
fn edit_undo_render() {
    let dir = std::env::temp_dir().join(format!("liemo-mobile-test-{}", std::process::id()));
    let e = Engine::new(dir.display().to_string(), Arc::new(NoMedia));
    let made = e.call("create_project".into(), r#"{"template":"sample"}"#.into());
    assert!(made.ok, "{}", made.text);
    let id = serde_json::from_str::<serde_json::Value>(&made.text).unwrap()["id"].as_str().unwrap().to_string();

    assert!(e.open_project(id.clone()).ok);
    assert!(!e.can_undo());
    let r = e.call("update_clip".into(), format!(r#"{{"project":"{id}","clip":"c7","text":{{"content":"Hello iPhone"}}}}"#));
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
    let _ = std::fs::remove_dir_all(dir);
}
