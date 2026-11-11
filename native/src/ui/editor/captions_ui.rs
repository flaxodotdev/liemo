// Captions: auto captions from the edit's sound (Apple's speech recognition on the Mac), subtitle
// files in (SRT, VTT) and out. Captions are text clips on the Titles track.

use super::Editor;
use crate::model::captions::{self, Cue};
use gpui::{AppContext, Context, Window};
use std::path::{Path, PathBuf};

pub const STYLES: [(&str, &str); 3] = [("pop", "Pop"), ("clean", "Clean"), ("boxed", "Boxed")];

impl Editor {
    /// Listen to the whole edit's sound and caption it in `style`.
    pub fn auto_captions(&mut self, style: &'static str, cx: &mut Context<Self>) {
        if !cfg!(target_os = "macos") {
            self.store.toast("Auto captions use Apple's speech recognition, so they're Mac only. Import an SRT or VTT file instead.");
            return;
        }
        if self.captioning {
            return;
        }
        let end = self.store.end();
        if end <= 0.0 {
            self.store.toast("Add some sound to caption first");
            return;
        }
        self.refresh_arcs();
        let (clips, tracks) = (self.clips_arc.clone(), self.tracks_arc.clone());
        let env = self.env();
        self.captioning = true;
        self.store.toast("Listening to the edit… captions appear when it's done");
        let task = cx.background_spawn(async move {
            crate::model::time::set_env(env);
            let Some(mix) = crate::audio::mix::voice_range(&clips, &tracks, 0.0, end) else {
                return Err("There's no voice or video sound to caption (music is left out). Unmute a track or add a voice clip.".to_string());
            };
            transcribe(&mix)
        });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            let _ = this.update(cx, |this, cx| {
                this.captioning = false;
                match res {
                    Ok(words) if words.is_empty() => this.store.toast("No speech found in this edit"),
                    Ok(words) => {
                        let cues = captions::group_words(&words);
                        let n = this.store.add_captions(&cues, style);
                        this.store.toast(format!("Added {n} captions on Titles. Select them all to restyle them together."));
                    }
                    Err(e) => this.store.toast(e),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn pick_subtitles(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(gpui::PathPromptOptions { files: true, directories: false, multiple: false, prompt: Some("Import captions".into()) });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await {
                let _ = this.update(cx, |this, cx| {
                    for p in paths {
                        this.import_subtitles(&p);
                    }
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// An SRT or VTT file → captions on Titles, in the Caption text style.
    pub fn import_subtitles(&mut self, path: &Path) {
        let name = crate::media::import::file_name(path);
        let text = match std::fs::read(path) {
            Ok(b) => String::from_utf8_lossy(&b).into_owned(),
            Err(e) => {
                self.store.toast(format!("{name} couldn't be read: {e}"));
                return;
            }
        };
        let cues = captions::parse(&text);
        if cues.is_empty() {
            self.store.toast(format!("{name} has no captions Liemo can read. It takes SRT and WebVTT files."));
            return;
        }
        let n = self.store.add_captions(&cues, "subtitle");
        self.store.toast(format!("Added {n} captions from {name}"));
    }

    /// Save the edit's titles as a subtitle file ("srt" or "vtt").
    pub fn export_captions(&mut self, ext: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        let cues: Vec<Cue> = captions::cues_from_clips(&self.store.clips);
        if cues.is_empty() {
            self.store.toast("There are no titles or captions to export");
            return;
        }
        let dir = dirs::download_dir().unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")));
        let base = if self.store.name.trim().is_empty() { "Captions".to_string() } else { self.store.name.trim().replace(['/', ':'], "-") };
        let rx = cx.prompt_for_new_path(&dir, Some(&format!("{base}.{ext}")));
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(path))) = rx.await else { return };
            let body = if ext == "vtt" { captions::to_vtt(&cues) } else { captions::to_srt(&cues) };
            let res = std::fs::write(&path, body);
            let _ = this.update(cx, |this, cx| {
                match res {
                    Ok(()) => this.store.toast(format!("Saved {} captions to {}", cues.len(), crate::media::import::file_name(&path))),
                    Err(e) => this.store.toast(format!("Couldn't save the captions: {e}")),
                }
                cx.notify();
            });
        })
        .detach();
    }
}

#[cfg(target_os = "macos")]
fn transcribe(mix: &[f32]) -> Result<Vec<(String, f64, f64)>, String> {
    crate::speech_mac::transcribe_mix(mix)
}
#[cfg(not(target_os = "macos"))]
fn transcribe(_: &[f32]) -> Result<Vec<(String, f64, f64)>, String> {
    Err("Auto captions are Mac only".into())
}
