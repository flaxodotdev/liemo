// Changing a clip's speed keeps the main track back to back: slower pushes what follows along,
// faster pulls it back.
use liemo_mobile::api::{Engine, MediaFrame, MediaHost, MediaProbe};
use std::sync::Arc;
struct NoMedia;
impl MediaHost for NoMedia {
    fn probe(&self, _: String) -> Option<MediaProbe> { None }
    fn video_frame(&self, _: String, _: f64, _: u32, _: u32) -> Option<MediaFrame> { None }
    fn decode_image(&self, _: String, _: u32) -> Option<MediaFrame> { None }
    fn decode_audio(&self, _: String) -> Option<Vec<f32>> { None }
    fn segment(&self, _: MediaFrame) -> Option<MediaFrame> { None }
}
#[test]
fn speed_changes_keep_the_track_tight() {
    let dir = std::env::temp_dir().join(format!("liemo-speed-{}", std::process::id()));
    let e = Engine::new(dir.display().to_string(), Arc::new(NoMedia));
    let id = serde_json::from_str::<serde_json::Value>(&e.call("create_project".into(), r#"{"template":"sample"}"#.into()).text).unwrap()["id"].as_str().unwrap().to_string();
    e.open_project(id.clone());
    let v1 = |e: &Engine| -> Vec<(String, f64, f64)> {
        let d: serde_json::Value = serde_json::from_str(&e.call("get_project".into(), format!(r#"{{"project":"{id}"}}"#)).text).unwrap();
        let mut v: Vec<_> = d["clips"].as_array().unwrap().iter().filter(|c| c["track"] == "V1")
            .map(|c| (c["id"].as_str().unwrap().to_string(), c["start"].as_f64().unwrap(), c["end"].as_f64().unwrap())).collect();
        v.sort_by(|a, b| a.1.total_cmp(&b.1));
        v
    };
    for (clip, speed) in [("c2", 0.5), ("c3", 2.0), ("c3", 0.3)] {
        let r = e.call("update_clip".into(), format!(r#"{{"project":"{id}","clip":"{clip}","speed":{speed}}}"#));
        assert!(r.ok, "{}", r.text);
        let v = v1(&e);
        for w in v.windows(2) {
            assert!((w[0].2 - w[1].1).abs() < 0.02, "{clip} x{speed}: gap or overlap {v:?}");
        }
    }
}
