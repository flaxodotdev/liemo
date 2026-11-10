// Recording a voice-over or any sound straight into the project: a take records from the default
// input, then lands on the Voice & SFX track where the playhead was when it started. With "Play
// along", the timeline plays during the take so you can talk over the picture.

use super::Editor;
use crate::audio::record::{input_name, Recorder};
use crate::model::time::short_time;
use crate::ui::prefs;
use crate::ui::theme::*;
use gpui::prelude::*;
use gpui::{div, px, relative, AnyElement, Context, SharedString, Task};
use std::time::Duration;

pub struct Recording {
    rec: Recorder,
    /// Timeline time the take started at.
    at: f64,
    /// Whether the timeline plays during the take.
    along: bool,
    _tick: Task<()>,
}

fn play_along() -> bool {
    prefs::get("record_play_along").and_then(|v| v.as_bool()).unwrap_or(false)
}

impl Editor {
    /// Start a take at `at` (default the playhead). `along` overrides the Play along setting.
    pub fn start_recording(&mut self, at: Option<f64>, along: Option<bool>, cx: &mut Context<Self>) -> Result<String, String> {
        if let Some(r) = &self.recording {
            return Err(format!("Already recording ({} so far)", short_time(r.rec.elapsed())));
        }
        let rec = Recorder::start().map_err(|e| e.to_string())?;
        if let Some(t) = at {
            self.seek(t.max(0.0));
        }
        let at = self.store.time;
        let along = along.unwrap_or_else(play_along);
        if along && !self.store.playing {
            self.play();
        }
        // Redraw for the timer and level meter.
        let tick = cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(Duration::from_millis(60)).await;
            if this.update(cx, |_, cx| cx.notify()).is_err() {
                break;
            }
        });
        let device = rec.device.clone();
        self.recording = Some(Recording { rec, at, along, _tick: tick });
        cx.notify();
        Ok(device)
    }

    /// Stop the take and put it on the Voice & SFX track where it started. Returns the WAV's
    /// path and that time; the clip lands once the file has been read (a moment later).
    pub fn stop_recording(&mut self, cx: &mut Context<Self>) -> Result<(std::path::PathBuf, f64, f64), String> {
        let Some(Recording { rec, at, along, .. }) = self.recording.take() else { return Err("Not recording".into()) };
        if self.store.playing && along {
            self.pause();
        }
        let secs = rec.elapsed();
        let r = rec.finish().map_err(|e| e.to_string());
        if let Ok(path) = &r {
            self.import(vec![path.clone()], Some((at, Some("A2".into()))), cx);
        }
        cx.notify();
        r.map(|p| (p, at, secs))
    }

    /// Where a take stands: (seconds so far, input device, timeline start).
    pub fn recording_status(&self) -> Option<(f64, String, f64)> {
        self.recording.as_ref().map(|r| (r.rec.elapsed(), r.rec.device.clone(), r.at))
    }

    pub fn toggle_recording(&mut self, cx: &mut Context<Self>) {
        let r = if self.recording.is_some() { self.stop_recording(cx).map(|_| ()) } else { self.start_recording(None, None, cx).map(|_| ()) };
        if let Err(e) = r {
            self.store.toast(e);
        }
        cx.notify();
    }

    /// The Record section at the top of the Audio tab.
    pub(super) fn record_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let live = self.recording.as_ref();
        let along = play_along();
        let button_row = div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                button("rec-toggle", if live.is_some() { Variant::Destructive } else { Variant::Primary }, 32.)
                    .child(icon(if live.is_some() { "square" } else { "mic" }, 14., if live.is_some() { coral() } else { black() }))
                    .child(if live.is_some() { "Stop" } else { "Record" })
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_recording(cx))),
            )
            .child(match live {
                Some(r) => {
                    // Level meter: a bar that fills with the input's peak.
                    let lv = r.rec.level();
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .flex_1()
                        .min_w_0()
                        .child(div().size(px(8.)).rounded_full().bg(coral()))
                        .child(type_timecode(div(), 12.).child(short_time(r.rec.elapsed())))
                        .child(div().relative().flex_1().h(px(6.)).rounded_full().bg(surface3()).child(div().absolute().left_0().top_0().bottom_0().rounded_full().bg(if lv > 0.95 { coral() } else { ink() }).w(relative(lv))))
                        .into_any_element()
                }
                None => div()
                    .flex_1()
                    .min_w_0()
                    .child(
                        segmented("rec-along", &[(false, "Silent"), (true, "Play along")], &along, |_| false, |v, _, _| prefs::set("record_play_along", serde_json::Value::Bool(v))),
                    )
                    .into_any_element(),
            });
        let note: SharedString = match live {
            Some(r) => format!("Recording from {}. The take goes on Voice & SFX at {}.", r.rec.device, short_time(r.at)).into(),
            None => match input_name() {
                Some(n) => format!("Records from {n} at the playhead. Play along plays the timeline while you record, so use headphones.").into(),
                None => "No microphone found. Connect one, or pick an input in System Settings › Sound.".into(),
            },
        };
        div().flex().flex_col().gap_2().child(button_row).child(micro(note)).into_any_element()
    }
}
