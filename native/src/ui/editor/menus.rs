// Right-click menus: clips (on the timeline and the stage), empty track space, track headers,
// keyframes and library items. Each is built when it opens, from the edit as it is then.

use super::Editor;
use crate::model::assets::asset;
use crate::model::keys::{self, Ease};
use crate::model::types::*;
use gpui::{App, ClickEvent, Context, Entity, SharedString, Window};
use gpui_component::menu::{PopupMenu, PopupMenuItem};

/// A menu item that runs `f` on the editor.
fn act(ed: &Entity<Editor>, label: impl Into<SharedString>, f: impl Fn(&mut Editor, &mut Window, &mut Context<Editor>) + 'static) -> PopupMenuItem {
    let ed = ed.clone();
    PopupMenuItem::new(label).on_click(move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
        ed.update(cx, |this, cx| {
            f(this, window, cx);
            this.restart_audio();
            cx.notify();
        })
    })
}

/// Picture clips whose speed can change: they run through time.
fn timed(c: &Clip) -> bool {
    matches!(c.kind, AssetType::Video | AssetType::Scene | AssetType::Overlay | AssetType::Audio)
}

pub fn clip_menu(mut menu: PopupMenu, ed: &Entity<Editor>, window: &mut Window, cx: &mut Context<PopupMenu>) -> PopupMenu {
    let e = ed.read(cx);
    let Some(c) = e.store.selected_clip().cloned() else { return menu };
    let n = e.store.selection().len();
    let time = e.store.time;
    let under = e.store.selected_clips().iter().any(|k| time > k.start && time < k.start + k.dur);
    let has_clipboard = !e.store.clipboard.is_empty();
    let track_name = e.store.track(&c.track).name.clone();
    let media_path = asset(&c.asset).and_then(|a| a.media.as_ref().map(|m| m.path.clone()));
    let id = c.id.clone();

    if n > 1 {
        menu = menu.label(format!("{n} clips selected"));
    }
    menu = menu
        .item(act(ed, "Cut", |e, _, cx| e.cut_to_clipboard(cx)))
        .item(act(ed, "Copy", |e, _, cx| e.copy_to_clipboard(cx)))
        .item(act(ed, "Paste at playhead", |e, _, cx| e.paste_from_clipboard(cx)).disabled(!has_clipboard))
        .item(act(ed, "Duplicate", |e, _, _| e.store.duplicate()))
        .item(act(ed, if n > 1 { "Delete clips" } else { "Delete" }, |e, _, _| e.store.remove()))
        .separator()
        .item(act(ed, "Split at playhead", |e, _, _| e.store.split()).disabled(!under))
        .item(act(ed, "Add to AI chat", |e, window, cx| e.mention_selection(window, cx)))
        .item(act(ed, "Save as component", |e, _, cx| e.save_component(cx)));
    if c.kind == AssetType::Text && !crate::model::brand::load().is_empty() {
        menu = menu.item(act(ed, "Apply brand", |e, _, _| e.apply_brand()));
    }

    if n == 1 && timed(&c) {
        let cur = c.speed();
        let audio = c.kind == AssetType::Audio;
        let (rev, frozen, ramped) = (c.motion.as_ref().map(|m| m.reverse).unwrap_or(false), c.frozen(), c.ramped());
        let (ed2, id2) = (ed.clone(), id.clone());
        menu = menu.submenu("Speed", window, cx, move |mut m, _, _| {
            let speeds: &[(f64, &str)] = if audio { &[(0.5, "0.5× slow"), (0.75, "0.75×"), (1.0, "Normal"), (1.25, "1.25×"), (1.5, "1.5×"), (2.0, "2× fast")] } else { &[(0.25, "0.25× slow-mo"), (0.5, "0.5×"), (1.0, "Normal"), (2.0, "2×"), (4.0, "4× fast")] };
            for &(v, l) in speeds {
                let id = id2.clone();
                m = m.item(act(&ed2, l, move |e, _, _| {
                    e.store.commit();
                    e.store.set_speed(&id, v);
                }).checked(!ramped && !frozen && (cur - v).abs() < 1e-3));
            }
            if !audio {
                let (a, b, c2, d) = (id2.clone(), id2.clone(), id2.clone(), id2.clone());
                m = m
                    .separator()
                    .item(act(&ed2, "Reverse", move |e, _, _| e.store.set_reverse(&a, !rev)).checked(rev))
                    .item(act(&ed2, "Freeze frame at playhead", move |e, _, _| e.store.freeze_frame(&b, 2.0)))
                    .separator()
                    .label("Speed ramp")
                    .item(act(&ed2, "Slow-mo in the middle", move |e, _, _| e.store.ramp_preset(&c2, "slowmo")))
                    .item(act(&ed2, "Speed up", {
                        let id = d.clone();
                        move |e, _, _| e.store.ramp_preset(&id, "up")
                    }))
                    .item(act(&ed2, "Slow down", {
                        let id = d.clone();
                        move |e, _, _| e.store.ramp_preset(&id, "down")
                    }))
                    .item(act(&ed2, "Fast, slow, fast", {
                        let id = d.clone();
                        move |e, _, _| e.store.ramp_preset(&id, "pulse")
                    }))
                    .item(act(&ed2, "Remove ramp", move |e, _, _| e.store.ramp_preset(&d, "none")).disabled(!ramped));
            }
            m
        });
    }
    {
        let (ed2, id2) = (ed.clone(), id.clone());
        menu = menu.submenu("Fade", window, cx, move |mut m, _, _| {
            for (key, l) in [("fadein", "Fade in"), ("fadeout", "Fade out"), ("both", "Fade in and out"), ("nofade", "No fades")] {
                let id = id2.clone();
                m = m.item(act(&ed2, l, move |e, _, _| e.store.apply_transition(key, Some(&id))));
            }
            m
        });
    }
    if n == 1 {
        let props: Vec<&'static str> = if c.visual() { vec!["x", "y", "scale", "rot", "opacity"] } else { vec!["volume"] };
        let lt = time - c.start;
        let keyed = !c.keys.is_empty();
        let (ed2, id2, c2) = (ed.clone(), id.clone(), c.clone());
        menu = menu.submenu("Keyframes", window, cx, move |mut m, _, _| {
            m = m.label("At the playhead");
            for p in &props {
                let p = *p;
                let id = id2.clone();
                let on = c2.keys.get(p).map(|k| keys::has_key_at(k, crate::model::time::snap_frame(lt.clamp(0.0, c2.dur)))).unwrap_or(false);
                m = m.item(act(&ed2, keys::prop_label(p), move |e, _, _| e.toggle_key_now(&id, p)).checked(on));
            }
            let id = id2.clone();
            m.separator().item(act(&ed2, "Remove all keyframes", move |e, _, _| {
                let lt = e.store.time - e.store.clip(&id).map(|c| c.start).unwrap_or(0.0);
                e.store.clear_keys(&id, None, lt);
            }).disabled(!keyed))
        });
    }
    if n == 1 {
        if c.kind == AssetType::Video {
            let id = id.clone();
            menu = menu.item(act(ed, "Detach audio", move |e, _, _| {
                e.store.detach_audio(&id);
            }));
        }
        if matches!(c.kind, AssetType::Music | AssetType::Audio | AssetType::Video) {
            let (id, on) = (id.clone(), c.duck > 0.0);
            menu = menu.item(act(ed, "Duck under voice", move |e, _, _| {
                e.store.commit();
                e.store.update_clip(&id, |c| c.duck = if on { 0.0 } else { 0.7 });
            }).checked(on));
        }
        if c.visual() && c.kind != AssetType::Text {
            let id = id.clone();
            menu = menu.item(act(ed, "Apply LUT (.cube)…", move |e, window, cx| e.pick_lut(id.clone(), None, window, cx)));
        }
    }
    let tid = c.track.clone();
    menu = menu.separator().item(act(ed, format!("Select all on {track_name}"), move |e, _, _| e.store.select_all(Some(&tid))));
    if let Some(p) = media_path {
        menu = menu.item(act(ed, if cfg!(target_os = "macos") { "Show in Finder" } else { "Show file" }, move |_, _, cx| cx.reveal_path(&p)));
    }
    menu
}

/// Empty space on a track: paste there, select the track, bring in files.
pub fn lane_menu(menu: PopupMenu, ed: &Entity<Editor>, track: String, time: f64) -> PopupMenu {
    let (t1, t2) = (track.clone(), track.clone());
    let menu = menu
        .item(act(ed, "Add marker here", move |e, _, _| {
            e.seek(time);
            e.store.toggle_marker();
        }))
        .item(act(ed, "Set in point here", move |e, _, _| e.store.set_in(Some(time))))
        .item(act(ed, "Set out point here", move |e, _, _| e.store.set_out(Some(time))))
        .separator();
    // Pasting also takes the system clipboard (files, pictures, text), so it's always offered.
    menu.item(act(ed, "Paste here", move |e, _, cx| {
        e.seek(time);
        e.paste_from_clipboard(cx);
    }))
        .item(act(ed, "Select all on this track", move |e, _, _| e.store.select_all(Some(&t1))))
        .separator()
        .item(act(ed, "Import media here…", move |e, window, cx| e.pick_files(Some((time, Some(t2.clone()))), window, cx)))
}

pub fn track_menu(menu: PopupMenu, ed: &Entity<Editor>, t: &Track, window: &mut Window, cx: &mut Context<PopupMenu>) -> PopupMenu {
    let (a, b, c, r, u, d, x) = (t.id.clone(), t.id.clone(), t.id.clone(), t.id.clone(), t.id.clone(), t.id.clone(), t.id.clone());
    let audio = t.kind == TrackKind::Audio;
    let ed2 = ed.clone();
    menu.item(act(ed, "Rename…", move |e, window, cx| e.start_rename(super::Rename::Track(r.clone()), window, cx)))
        .item(act(ed, if audio { "Mute" } else { "Hide" }, move |e, _, _| e.store.toggle_track(&a, if audio { "muted" } else { "hidden" })).checked(if audio { t.muted } else { t.hidden }))
        .item(act(ed, "Lock", move |e, _, _| e.store.toggle_track(&b, "locked")).checked(t.locked))
        .separator()
        .item(act(ed, if audio { "Move up" } else { "Move up (in front)" }, move |e, _, _| e.store.move_track(&u, -1)))
        .item(act(ed, if audio { "Move down" } else { "Move down (behind)" }, move |e, _, _| e.store.move_track(&d, 1)))
        .submenu("Add track", window, cx, move |m, _, _| add_track_menu(m, &ed2))
        .item(act(ed, "Remove track", move |e, _, _| {
            e.store.remove_track(&x);
        }).disabled(t.id == "V1"))
        .separator()
        .item(act(ed, "Select all on this track", move |e, _, _| e.store.select_all(Some(&c))))
}

/// New tracks: picture, titles or sound.
pub fn add_track_menu(menu: PopupMenu, ed: &Entity<Editor>) -> PopupMenu {
    menu.item(act(ed, "Video track (in front)", |e, _, _| {
        e.store.add_track(TrackKind::Overlay, None);
    }))
    .item(act(ed, "Titles track", |e, _, _| {
        e.store.add_track(TrackKind::Text, None);
    }))
    .item(act(ed, "Audio track", |e, _, _| {
        e.store.add_track(TrackKind::Audio, None);
    }))
}

/// A marker on the ruler.
pub fn marker_menu(mut menu: PopupMenu, ed: &Entity<Editor>, i: usize) -> PopupMenu {
    menu = menu.item(act(ed, "Rename…", move |e, window, cx| e.start_rename(super::Rename::Marker(i), window, cx)));
    menu = menu.item(act(ed, "Set in point here", move |e, _, _| {
        let t = e.store.markers.get(i).map(|m| m.t);
        e.store.set_in(t);
    }));
    menu = menu.item(act(ed, "Set out point here", move |e, _, _| {
        let t = e.store.markers.get(i).map(|m| m.t);
        e.store.set_out(t);
    }));
    menu = menu.separator().label("Colour");
    for (col, l) in MARKER_COLORS {
        menu = menu.item(act(ed, *l, move |e, _, _| e.store.update_marker(i, |m| m.color = col.to_string())));
    }
    menu.separator()
        .item(act(ed, "Delete marker", move |e, _, _| e.store.remove_marker(i)))
        .item(act(ed, "Delete all markers", |e, _, _| {
            e.store.commit();
            e.store.markers.clear();
            e.store.changed();
        }))
}

/// A keyframe diamond on the timeline: how it eases, or remove it.
pub fn key_menu(menu: PopupMenu, ed: &Entity<Editor>, clip: String, lt: f64, now: Option<Ease>) -> PopupMenu {
    let mut menu = menu.label("Into the next keyframe");
    for (e, l) in [(Ease::Linear, "Linear"), (Ease::Smooth, "Ease in and out"), (Ease::Hold, "Hold")] {
        let id = clip.clone();
        menu = menu.item(act(ed, l, move |ed, _, _| ed.store.edit_keys_at(&id, lt, Some(e))).checked(now == Some(e)));
    }
    let (a, b) = (clip.clone(), clip);
    menu.separator()
        .item(act(ed, "Go to keyframe", move |e, _, _| {
            let s = e.store.clip(&a).map(|c| c.start).unwrap_or(0.0);
            e.seek(s + lt);
        }))
        .item(act(ed, "Delete keyframe", move |e, _, _| e.store.edit_keys_at(&b, lt, None)))
}

/// A library item: add it, or find its file.
pub fn asset_menu(menu: PopupMenu, ed: &Entity<Editor>, asset_id: String) -> PopupMenu {
    let path = asset(&asset_id).and_then(|a| a.media.as_ref().map(|m| m.path.clone()));
    let video = asset(&asset_id).map(|a| a.kind == AssetType::Video && a.media.is_some()).unwrap_or(false);
    let (pid, has_proxy) = (asset_id.clone(), crate::media::proxy::exists(&asset_id));
    let mut menu = menu.item(act(ed, "Add at playhead", move |e, _, _| e.add_at_playhead(&asset_id)));
    if video {
        menu = menu.item(act(ed, if has_proxy { "Remove editing proxy" } else { "Make editing proxy" }, move |e, _, cx| {
            if has_proxy {
                e.remove_proxy(&pid);
            } else {
                e.make_proxy(&pid, cx);
            }
        }));
    }
    if let Some(p) = path {
        menu = menu.item(act(ed, if cfg!(target_os = "macos") { "Show in Finder" } else { "Show file" }, move |_, _, cx| cx.reveal_path(&p)));
    }
    menu
}

/// The timeline's Captions button.
pub fn captions_menu(mut menu: PopupMenu, ed: &Entity<Editor>, window: &mut Window, cx: &mut Context<PopupMenu>) -> PopupMenu {
    let e = ed.read(cx);
    let busy = e.captioning;
    let any = e.store.clips.iter().any(|c| c.kind == AssetType::Text);
    let mac = cfg!(target_os = "macos");
    menu = menu.label(if busy { "Listening to the edit…" } else if mac { "Auto captions from the sound" } else { "Auto captions (Mac only)" });
    // The everyday styles up front; the whole caption library one level down.
    for key in ["pop", "clean", "boxed"] {
        let l = crate::model::captions::caption_style(key).map(|s| s.name).unwrap_or(key);
        menu = menu.item(act(ed, l, move |e, _, cx| e.auto_captions(key, cx)).disabled(busy || !mac));
    }
    if mac && !busy {
        let ed2 = ed.clone();
        menu = menu.submenu("More styles", window, cx, move |mut m, _, _| {
            for s in crate::model::captions::CAPTION_STYLES.iter().filter(|s| !matches!(s.id, "pop" | "clean" | "boxed")) {
                let key = s.id;
                m = m.item(act(&ed2, s.name, move |e, _, cx| e.auto_captions(key, cx)));
            }
            m
        });
    }
    if any {
        let ed2 = ed.clone();
        menu = menu.separator().submenu("Restyle captions", window, cx, move |mut m, _, _| {
            m = m.label("The selected text, or every caption");
            for s in crate::model::captions::CAPTION_STYLES.iter() {
                let key = s.id;
                m = m.item(act(&ed2, s.name, move |e, _, _| {
                    let n = e.store.restyle_captions(key);
                    if n == 0 {
                        e.store.toast("No captions to restyle");
                    }
                }));
            }
            m
        });
    }
    menu.separator()
        .item(act(ed, "Import SRT or VTT…", |e, window, cx| e.pick_subtitles(window, cx)))
        .item(act(ed, "Export SRT…", |e, window, cx| e.export_captions("srt", window, cx)).disabled(!any))
        .item(act(ed, "Export WebVTT…", |e, window, cx| e.export_captions("vtt", window, cx)).disabled(!any))
}
