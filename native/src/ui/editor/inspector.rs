// The inspector: the project at a glance, or the selected clip's timing, text, transform,
// transition, effect stack, sound and fades. Also the shared slider and select controls.

use super::{Apply, Editor};
use crate::fx::registry::{effect_def, effects, ParamKind, EFFECT_GROUPS};
use crate::fx::transitions::{transition_def, TRANSITIONS, TRANSITION_GROUPS};
use crate::model::assets::{asset, font_def, FONTS, TEXT_ANIMS, TEXT_COLORS};
use crate::model::time::tc;
use crate::model::types::*;
use crate::ui::theme::*;
use gpui::prelude::*;
use gpui::{anchored, deferred, div, px, relative, AnyElement, Context, Div, Entity, SharedString, Window};
use gpui_component::button::Button;
use gpui_component::input::{Input, Textarea};
use gpui_component::menu::{DropdownMenu, PopupMenuItem};
use std::rc::Rc;

pub enum MenuEntry {
    Label(String),
    Separator,
    /// `font`: draw the label in this font family (font pickers).
    Item { label: String, checked: bool, act: Rc<dyn Fn(&mut Editor)>, font: Option<String> },
}
pub fn item(label: impl Into<String>, checked: bool, act: impl Fn(&mut Editor) + 'static) -> MenuEntry {
    MenuEntry::Item { label: label.into(), checked, act: Rc::new(act), font: None }
}
/// A font's name, shown in that font.
pub fn font_item(family: impl Into<String>, checked: bool, act: impl Fn(&mut Editor) + 'static) -> MenuEntry {
    let family = family.into();
    MenuEntry::Item { label: family.clone(), checked, act: Rc::new(act), font: Some(family) }
}

const SWATCHES: [&str; 9] = ["#ffffff", "#22c55e", "#0099ff", "#6a4cf5", "#d44df0", "#ff5577", "#ff7a3d", "#ffd9a0", "#1b0f2e"];
fn pct(v: f64) -> String {
    format!("{}%", (v * 100.0).round())
}

impl Editor {
    /// A slider bound to a value; `apply` writes it. With `history`, one drag is one undo step.
    pub fn slider(&self, key: impl Into<String>, value: f64, min: f64, max: f64, step: f64, history: bool, apply: Apply, cx: &mut Context<Self>) -> Div {
        let key: String = key.into();
        let f = if max > min { ((value - min) / (max - min)).clamp(0.0, 1.0) as f32 } else { 0.0 };
        let k2 = key.clone();
        div()
            .relative()
            .h(px(18.))
            .w_full()
            .cursor_pointer()
            .child(self.track_bounds(key.clone()))
            .child(div().absolute().left_0().right_0().top(px(7.)).h(px(4.)).rounded_full().bg(surface3()))
            .child(div().absolute().left_0().top(px(7.)).h(px(4.)).rounded_full().bg(ink()).w(relative(f)))
            .child(div().absolute().top(px(2.)).left(relative(f)).ml(px(-7.)).size(px(14.)).rounded_full().bg(ink()).border_2().border_color(surface1()))
            .on_mouse_down(gpui::MouseButton::Left, cx.listener(move |this, ev: &gpui::MouseDownEvent, _, cx| {
                cx.stop_propagation();
                this.begin_slider(k2.clone(), min, max, step, history, apply.clone(), ev.position, cx);
            }))
    }

    /// A slider for a property that can be keyframed: it shows and sets the value at the playhead,
    /// and the diamond beside it adds or removes a keyframe there.
    #[allow(clippy::too_many_arguments)]
    pub fn keyed_slider(&self, c: &Clip, prop: &str, label_text: &str, min: f64, max: f64, step: f64, fmt: impl Fn(f64) -> String, apply: Option<Apply>, cx: &mut Context<Self>) -> Div {
        let lt = (self.store.time - c.start).clamp(0.0, c.dur);
        let value = c.value(prop, lt).unwrap_or(min);
        let keyed = c.keys.get(prop).map(|k| !k.is_empty()).unwrap_or(false);
        let here = c.keys.get(prop).map(|k| crate::model::keys::has_key_at(k, crate::model::time::snap_frame(lt))).unwrap_or(false);
        let (id, p) = (c.id.clone(), prop.to_string());
        let apply = apply.unwrap_or_else(|| {
            let (id, p) = (id.clone(), p.clone());
            Rc::new(move |e: &mut Editor, v| e.set_props_now(&id, &[(p.as_str(), v)]))
        });
        let tip = if here { "Remove this keyframe" } else if keyed { "Add a keyframe at the playhead" } else { "Animate: add a keyframe at the playhead" };
        let key_btn = div()
            .id(SharedString::from(format!("kf-{}-{prop}", c.id)))
            .size(px(20.))
            .flex_none()
            .rounded(px(5.))
            .cursor_pointer()
            .hover(|s| s.bg(surface2()))
            .child(key_glyph(if keyed { signal() } else { ink_muted() }, here))
            .tooltip(move |w, cx| gpui_component::tooltip::Tooltip::new(tip).build(w, cx))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.toggle_key_now(&id, &p);
                this.restart_audio();
                cx.notify();
            }));
        field_row(label_text, self.slider(format!("k-{}-{prop}", c.id), value, min, max, step, true, apply, cx), Some(fmt(value))).child(key_btn)
    }

    /// Label · slider · value, with one undo step per drag.
    #[allow(clippy::too_many_arguments)]
    pub fn slider_field(&self, label_text: &str, key: impl Into<String>, value: f64, min: f64, max: f64, step: f64, fmt: impl Fn(f64) -> String, apply: Apply, cx: &mut Context<Self>) -> Div {
        field_row(label_text, self.slider(key, value, min, max, step, true, apply, cx), Some(fmt(value)))
    }

    /// A select: a pill showing the current choice. Short lists open a menu; long ones (fonts,
    /// effects, transitions…) open a searchable list.
    pub fn select(&self, id: impl Into<SharedString>, current: impl Into<SharedString>, entries: Vec<MenuEntry>, cx: &mut Context<Self>) -> AnyElement {
        let id: SharedString = id.into();
        let entries = Rc::new(entries);
        if entries.iter().filter(|e| matches!(e, MenuEntry::Item { .. })).count() > 12 {
            return self.combobox(id, current.into(), entries, cx);
        }
        let ed: Entity<Editor> = cx.entity();
        Button::new(id)
            .label(current.into())
            .outline()
            .dropdown_caret(true)
            .w_full()
            .h(px(32.))
            .px_3()
            .rounded(px(10.))
            .bg(alpha(hairline(), 0.3))
            .border_1()
            .border_color(hairline())
            .text_size(px(13.))
            .dropdown_menu(move |menu, _, _| {
                let mut menu = menu.min_w(px(200.));
                for e in entries.iter() {
                    menu = match e {
                        MenuEntry::Label(l) => menu.label(l.clone()),
                        MenuEntry::Separator => menu.separator(),
                        MenuEntry::Item { label, checked, act, .. } => {
                            let act = act.clone();
                            let ed = ed.clone();
                            menu.item(PopupMenuItem::new(label.clone()).checked(*checked).on_click(move |_, _, cx| {
                                ed.update(cx, |this, cx| {
                                    act(this);
                                    cx.notify();
                                });
                            }))
                        }
                    };
                }
                menu
            })
            .into_any_element()
    }

    /// A select with a search field: type to filter, arrows to move, Enter to pick, Escape or a
    /// click outside to close. Group labels stay above their matching items.
    fn combobox(&self, id: SharedString, current: SharedString, entries: Rc<Vec<MenuEntry>>, cx: &mut Context<Self>) -> AnyElement {
        let open = self.combo.as_ref() == Some(&id);
        let tid = id.clone();
        // The same pill as a normal select; only what opens differs.
        // A font picker shows the current font in itself.
        let trigger_font = entries.iter().find_map(|e| match e {
            MenuEntry::Item { checked: true, font: Some(f), .. } => Some(f.clone()),
            _ => None,
        });
        let trigger = Button::new(id.clone())
            .label(current)
            .when_some(trigger_font, |b, f| b.font_family(f))
            .outline()
            .dropdown_caret(true)
            .w_full()
            .h(px(32.))
            .px_3()
            .rounded(px(10.))
            .bg(alpha(hairline(), 0.3))
            .border_1()
            .border_color(hairline())
            .text_size(px(13.))
            .on_click(cx.listener(move |this, _, window, cx| {
                if this.combo.as_ref() == Some(&tid) {
                    this.combo = None;
                } else {
                    this.combo = Some(tid.clone());
                    // usize::MAX: start on the current choice (resolved when the list is built).
                    this.combo_hi = usize::MAX;
                    this.combo_jump.set(true);
                    this.combo_input.update(cx, |s, cx| {
                        s.set_value("", window, cx);
                        s.focus(window, cx);
                    });
                }
                cx.notify();
            }));
        let geo_key = format!("combo:{id}");
        let mut wrap = div().relative().w_full().child(self.track_bounds(geo_key.clone())).child(trigger);
        if !open {
            return wrap.into_any_element();
        }

        // Filter: items whose label contains the query; a label shows while an item under it does.
        let q = self.combo_input.read(cx).value().trim().to_lowercase();
        enum Row {
            Label(String),
            Sep,
            Item(String, bool, usize, Option<String>),
        }
        let mut rows: Vec<Row> = vec![];
        let mut acts: Vec<Rc<dyn Fn(&mut Editor)>> = vec![];
        let mut pending: Option<String> = None;
        let mut checked_at = None;
        for e in entries.iter() {
            match e {
                MenuEntry::Label(l) => pending = Some(l.clone()),
                MenuEntry::Separator => {
                    if q.is_empty() && !rows.is_empty() {
                        rows.push(Row::Sep);
                    }
                }
                MenuEntry::Item { label, checked, act, font } => {
                    if !q.is_empty() && !label.to_lowercase().contains(&q) {
                        continue;
                    }
                    if let Some(l) = pending.take() {
                        rows.push(Row::Label(l));
                    }
                    if *checked && checked_at.is_none() {
                        checked_at = Some(acts.len());
                    }
                    rows.push(Row::Item(label.clone(), *checked, acts.len(), font.clone()));
                    acts.push(act.clone());
                }
            }
        }
        let n = acts.len();
        // Which list row each item is, for keeping the highlight in view.
        let row_of: Rc<Vec<usize>> = Rc::new(rows.iter().enumerate().filter_map(|(r, row)| if let Row::Item(_, _, i, _) = row { Some((*i, r)) } else { None }).map(|(_, r)| r).collect());
        let start = checked_at.unwrap_or(0);
        let hi = if n == 0 {
            0
        } else if self.combo_hi == usize::MAX {
            // Just opened: show the current choice a few rows down from the top, once, so the
            // list scrolls freely afterwards.
            // Keep asking until the list has been laid out (the first frame has no rows yet).
            if self.combo_jump.get() {
                let row = row_of[start].saturating_sub(3);
                self.combo_scroll.scroll_to_top_of_item(row);
                if self.combo_scroll.bounds_for_item(row).is_some() {
                    self.combo_jump.set(false);
                }
            }
            start
        } else {
            self.combo_hi.min(n - 1)
        };
        let acts = Rc::new(acts);
        let keys = {
            let (acts, row_of) = (acts.clone(), row_of.clone());
            cx.listener(move |this: &mut Editor, ev: &gpui::KeyDownEvent, _, cx| {
                let n = acts.len();
                if this.combo_hi == usize::MAX {
                    this.combo_hi = start;
                }
                match ev.keystroke.key.as_str() {
                    "down" if n > 0 => {
                        this.combo_hi = (this.combo_hi.min(n - 1) + 1) % n;
                        this.combo_scroll.scroll_to_item(row_of[this.combo_hi]);
                    }
                    "up" if n > 0 => {
                        this.combo_hi = (this.combo_hi.min(n - 1) + n - 1) % n;
                        this.combo_scroll.scroll_to_item(row_of[this.combo_hi]);
                    }
                    "enter" if n > 0 => {
                        let act = acts[this.combo_hi.min(n - 1)].clone();
                        this.combo = None;
                        act(this);
                    }
                    "escape" => this.combo = None,
                    _ => return,
                }
                cx.stop_propagation();
                cx.notify();
            })
        };
        let list = div()
            .id("combo-list")
            .flex()
            .flex_col()
            .p_1()
            .max_h(px(340.))
            .overflow_y_scroll()
            .track_scroll(&self.combo_scroll)
            .children(rows.into_iter().map(|row| match row {
                Row::Label(l) => div().px_2().pt_2().pb_1().child(label(l)).into_any_element(),
                Row::Sep => div().my_1().h(px(1.)).bg(hairline_soft()).into_any_element(),
                Row::Item(text, checked, i, font) => {
                    let act = acts[i].clone();
                    div()
                        .id(("combo-item", i))
                        .flex()
                        .items_center()
                        .gap_2()
                        .px_2()
                        .py(px(5.))
                        .rounded(px(7.))
                        .text_size(px(13.))
                        .cursor_pointer()
                        .when(i == hi, |d| d.bg(surface3()))
                        .hover(|s| s.bg(surface2()))
                        .child(div().flex_1().min_w_0().truncate().when_some(font, |d, f| d.font_family(f).text_size(px(15.))).child(text))
                        .when(checked, |d| d.child(icon("check", 13., ink())))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.combo = None;
                            act(this);
                            cx.notify();
                        }))
                        .into_any_element()
                }
            }))
            .when(n == 0, |d| d.child(div().px_2().py_3().child(micro("No matches"))));
        // Exactly as wide as the select it opened from (measured last frame).
        let width = self.geo.borrow().get(&geo_key).map(|b| f32::from(b.size.width)).unwrap_or(240.);
        let panel = div()
            .id("combo-panel")
            .occlude()
            .w(px(width))
            .flex()
            .flex_col()
            .overflow_hidden()
            .rounded(px(12.))
            .bg(surface1())
            .border_1()
            .border_color(hairline())
            .shadow_lg()
            .capture_key_down(keys)
            .on_mouse_down_out({
                // A press on the select's own button is left to its click, which toggles closed.
                let key = geo_key.clone();
                cx.listener(move |this, ev: &gpui::MouseDownEvent, _, cx| {
                    if this.geo.borrow().get(&key).is_some_and(|b| b.contains(&ev.position)) {
                        return;
                    }
                    this.combo = None;
                    cx.notify();
                })
            })
            .child(
                div()
                    .relative()
                    .w_full()
                    .h(px(40.))
                    .px_3()
                    .flex()
                    .items_center()
                    .child(self.track_bounds("input:combo"))
                    // Same type as the select buttons: 13 px, medium.
                    .child(Input::new(&self.combo_input).appearance(false).text_size(px(13.)).font_weight(gpui::FontWeight::MEDIUM).px_0().w_full().aria_label("Search")),
            )
            .child(div().w_full().h(px(1.)).flex_none().bg(hairline()))
            .child(list);
        wrap = wrap.child(div().absolute().top(px(36.)).left_0().child(deferred(anchored().snap_to_window_with_margin(px(8.)).child(panel)).with_priority(2)));
        wrap.into_any_element()
    }

    pub(super) fn render_inspector(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body: Vec<AnyElement> = match self.store.selected_clip().cloned() {
            Some(_) if self.store.many_selected() => self.multi_inspector(cx),
            Some(c) => self.clip_inspector(&c, window, cx),
            None => self.project_inspector(cx),
        };
        panel()
            .w(px(self.layout.insp_w))
            .flex_none()
            .child(
                panel_header()
                    .child(label("Inspector").flex_1())
                    .child(icon_button("insp-close", "x", "Close inspector", Some("⌥⌘B"), 26., Variant::Ghost, cx.listener(|this, _, _, cx| this.toggle_panel(super::layout::Edge::Inspector, cx)))),
            )
            .child(div().id("inspector-scroll").flex_1().min_h_0().overflow_y_scroll().child(div().flex().flex_col().px_4().pb_4().children(body)))
    }

    fn project_inspector(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let s = &self.store;
        let shortcuts = [
            ("Space", "Play / pause"),
            ("S", "Split at playhead"),
            ("⌫", "Delete clip"),
            ("← →", "Step one frame (⇧ for 10)"),
            ("⌘Z", "Undo, ⇧⌘Z to redo"),
            ("⌘C ⌘V", "Copy and paste at playhead"),
            ("⌘X", "Cut clip"),
            ("⌥ arrows", "Nudge on screen (⇧ for 10px)"),
            ("⌘D", "Duplicate clip"),
            ("⇧ click", "Add a clip to the selection"),
            ("⌘A", "Select every clip"),
            ("⌥ drag edge", "Change a clip's speed"),
            ("◆", "Keyframe the value at the playhead"),
            ("M", "Marker at playhead ([ ] to jump)"),
            ("I O", "In and out points (⌥X clears)"),
            ("F", "Full-screen preview"),
            ("⌘= ⌘0", "Zoom the preview, back to fit"),
            ("+ −", "Zoom timeline"),
            ("N", "Toggle snapping"),
            ("⌘B ⌘J", "Show or hide library, timeline"),
            ("⌥⌘B", "Show or hide inspector"),
            ("⌘I", "Show or hide Liemo AI"),
        ];
        vec![
            group(None, None, false)
                .child(type_headline(div()).child(if s.name.is_empty() { "Untitled edit".to_string() } else { s.name.clone() }))
                .child(kv(vec![
                    ("Clips".into(), s.clips.len().to_string()),
                    ("Length".into(), tc(s.end())),
                    ("Frame rate".into(), format!("{} fps", self.project.fps)),
                    ("Format".into(), format!("{} · {}", self.project.format_label(), self.project.aspect_key())),
                ]))
                .into_any_element(),
            {
                let on = crate::media::proxy::ENABLED.load(std::sync::atomic::Ordering::Relaxed);
                let ed = cx.entity();
                group(Some("Proxies"), None, false)
                    .child(field_row("Preview", segmented("proxies", &[(false, "Originals"), (true, "Proxies")], &on, |_| false, move |v, _, cx| ed.update(cx, |e, cx| {
                        e.set_proxies(v);
                        cx.notify();
                    })), None))
                    .child(button("all-proxies", Variant::Secondary, 30.).text_size(px(12.)).child("Make proxies for big videos").on_click(cx.listener(|this, _, _, cx| {
                        this.make_all_proxies(cx);
                        cx.notify();
                    })))
                    .child(micro("Videos above 1080p get a small editing copy when imported, so the preview stays smooth. Export always uses the originals."))
                    .into_any_element()
            },
            group(Some("Get started"), None, false)
                .child(micro("Drag footage, music, overlays and text from the library onto a track. Click a clip to edit its timing, look and type here."))
                .into_any_element(),
            group(Some("Shortcuts"), None, true)
                .child(div().flex().flex_col().gap(px(6.)).children(shortcuts.iter().map(|(k, d)| {
                    div().flex().items_center().gap_3().child(div().w(px(76.)).flex_none().flex().child(kbd(k))).child(micro(*d))
                })))
                .into_any_element(),
        ]
    }

    fn clip_inspector(&mut self, c: &Clip, window: &mut Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let kind = match c.kind {
            AssetType::Scene | AssetType::Video => "Video",
            AssetType::Image | AssetType::Photo => "Image",
            AssetType::Overlay => "Overlay",
            AssetType::Text => "Text",
            AssetType::Music => "Music",
            AssetType::Audio => "Audio",
        };
        let mut rows = vec![("Start".to_string(), tc(c.start)), ("End".into(), tc(c.start + c.dur)), ("Duration".into(), tc(c.dur))];
        if c.in_ > 0.0 {
            rows.push(("Source in".into(), tc(c.in_)));
        }
        let mut out = vec![group(None, None, false)
            .child(type_label(div()).self_start().rounded(px(6.)).bg(surface3()).px_2().py_1().text_color(ink_muted()).child(format!("{} · {}", kind.to_uppercase(), c.track)))
            .child(type_headline(div()).child(c.name.clone()))
            .child(kv(rows))
            .into_any_element()];

        if let Some(t) = &c.text {
            out.extend(self.text_groups(c, t, window, cx));
        }
        let id = c.id.clone();
        let num = |this: &Editor, label: &str, field: fn(&mut Clip) -> &mut f64, value: f64, min: f64, max: f64, step: f64, fmt: fn(f64) -> String, cx: &mut Context<Editor>| {
            let id = id.clone();
            this.slider_field(label, format!("f-{label}"), value, min, max, step, fmt, Rc::new(move |e: &mut Editor, v| e.store.update_clip(&id, |c| *field(c) = v)), cx)
        };
        if c.visual() {
            let (w, h) = (crate::model::time::W() as f64, crate::model::time::H() as f64);
            let animated = ["x", "y", "scale", "rot", "opacity"].iter().filter(|p| c.keys.contains_key(**p)).count();
            out.push(
                group(Some("Transform"), (animated > 0).then(|| "Animated".to_string()), false)
                    .child(self.keyed_slider(c, "x", "Position X", -w / 2.0, w / 2.0, 1.0, |v| format!("{}", v.round()), None, cx))
                    .child(self.keyed_slider(c, "y", "Position Y", -h / 2.0, h / 2.0, 1.0, |v| format!("{}", v.round()), None, cx))
                    .child(self.keyed_slider(c, "scale", "Scale", 0.1, 3.0, 0.01, pct, None, cx))
                    .child(self.keyed_slider(c, "rot", "Rotation", -180.0, 180.0, 1.0, |v| format!("{}°", v.round()), None, cx))
                    .child(self.keyed_slider(c, "opacity", "Opacity", 0.0, 1.0, 0.01, pct, None, cx))
                    .when(animated > 0, |d| d.child(micro("With keyframes, moving the slider or the picture sets a keyframe at the playhead.")))
                    .into_any_element(),
            );
            out.push(self.blend_mask_group(c, cx).into_any_element());
            if matches!(c.kind, AssetType::Video | AssetType::Scene | AssetType::Overlay) {
                out.push(self.speed_group(c, cx).into_any_element());
            }
            if c.kind == AssetType::Video && asset(&c.asset).and_then(|a| a.media.as_ref().map(|m| m.has_audio)).unwrap_or(false) {
                out.push(group(Some("Sound"), None, false).child(self.keyed_slider(c, "volume", "Volume", 0.0, MAX_VOLUME, 0.01, pct, None, cx)).children(self.sound_tools(c, cx)).into_any_element());
            }
            out.push(self.transition_control(c, cx).into_any_element());
            out.push(self.effect_stack(c, window, cx).into_any_element());
        } else {
            let mut g = group(Some("Audio"), None, false).child(self.keyed_slider(c, "volume", "Volume", 0.0, MAX_VOLUME, 0.01, pct, None, cx)).children(self.sound_tools(c, cx));
            if c.kind == AssetType::Music {
                if let Some(a) = asset(&c.asset) {
                    g = g.child(micro(format!("{} · {} BPM. The track keeps playing for as long as you stretch the clip.", a.sub, a.bpm)));
                }
            }
            out.push(g.into_any_element());
            if c.kind == AssetType::Audio {
                out.push(self.sound_group(c, cx).into_any_element());
            }
        }
        let max_fade = (c.dur / 2.0).min(3.0);
        let max_fade = (max_fade * 100.0).round() / 100.0;
        out.push(
            group(Some("Fades"), None, false)
                .child(num(self, "Fade in", |c| &mut c.fade_in, c.fade_in, 0.0, max_fade, 0.05, |v| format!("{v:.2}s"), cx))
                .child(num(self, "Fade out", |c| &mut c.fade_out, c.fade_out, 0.0, max_fade, 0.05, |v| format!("{v:.2}s"), cx))
                .into_any_element(),
        );
        out.push(
            group(None, None, true)
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap(px(6.))
                        .child(button("i-split", Variant::Secondary, 32.).child(icon("scissors", 14., ink())).child("Split").on_click(cx.listener(|this, _, _, cx| {
                            this.store.split();
                            cx.notify();
                        })))
                        .child(button("i-dup", Variant::Secondary, 32.).child(icon("copy", 14., ink())).child("Duplicate").on_click(cx.listener(|this, _, _, cx| {
                            this.store.duplicate();
                            cx.notify();
                        })))
                        .child(button("i-del", Variant::Destructive, 32.).child(icon("trash-2", 14., coral())).child("Delete").on_click(cx.listener(|this, _, _, cx| {
                            this.store.remove();
                            cx.notify();
                        }))),
                )
                .into_any_element(),
        );
        out
    }

    /// Speed for picture clips that run through time: constant speed, direction, freeze frames
    /// and speed ramps (Speed keyframes).
    fn speed_group(&self, c: &Clip, cx: &mut Context<Self>) -> Div {
        let id = c.id.clone();
        let (ramped, frozen) = (c.ramped(), c.frozen());
        let rev = c.motion.as_ref().map(|m| m.reverse).unwrap_or(false);
        let cur = c.speed();
        let chips = div().flex().flex_wrap().gap(px(6.)).children([(0.25, "¼×"), (0.5, "½×"), (1.0, "1×"), (2.0, "2×"), (4.0, "4×")].into_iter().map(|(v, l)| {
            let id = id.clone();
            let on = !ramped && !frozen && (cur - v).abs() < 1e-3;
            button(SharedString::from(format!("spd-{l}")), if on { Variant::Primary } else { Variant::Secondary }, 28.)
                .text_size(px(12.))
                .child(l)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.store.commit();
                    this.store.set_speed(&id, v);
                    this.restart_audio();
                    cx.notify();
                }))
                .into_any_element()
        }));
        let sid = id.clone();
        // Constant speed changes the clip's length to keep its footage; with a ramp, a keyframe.
        let apply: Apply = Rc::new(move |e: &mut Editor, v| {
            let ramp = e.store.clip(&sid).map(|c| c.ramped()).unwrap_or(false);
            if ramp {
                e.set_props_now(&sid, &[("speed", v)]);
            } else {
                e.store.set_speed(&sid, v);
            }
        });
        let ed = cx.entity();
        let rid = id.clone();
        let direction = segmented(&format!("dir-{id}"), &[(false, "Forward"), (true, "Reverse")], &rev, |_| frozen, move |v, _, cx| {
            let rid = rid.clone();
            ed.update(cx, |e, cx| {
                e.store.set_reverse(&rid, v);
                e.restart_audio();
                cx.notify();
            })
        });
        let ramps = div().flex().flex_wrap().gap(px(6.)).children([("slowmo", "Slow-mo middle"), ("up", "Speed up"), ("down", "Slow down"), ("pulse", "Fast-slow-fast")].into_iter().map(|(k, l)| {
            let id = id.clone();
            button(SharedString::from(format!("ramp-{k}")), Variant::Secondary, 28.)
                .text_size(px(12.))
                .child(l)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.store.ramp_preset(&id, k);
                    this.restart_audio();
                    cx.notify();
                }))
                .into_any_element()
        }));
        let (fid, nid) = (id.clone(), id.clone());
        let status = if frozen { Some("Freeze frame".to_string()) } else if ramped { Some("Ramp".to_string()) } else if (cur - 1.0).abs() > 1e-3 || rev { Some(format!("{}{:.2}×", if rev { "◀ " } else { "" }, cur)) } else { None };
        let mut g = group(Some("Speed"), status, false);
        if frozen {
            return g.child(micro("This clip holds one frame. Trim it to hold for longer or shorter.")).child(
                button("unfreeze", Variant::Secondary, 30.).text_size(px(12.)).child("Play normally").on_click(cx.listener(move |this, _, _, cx| {
                    this.store.commit();
                    this.store.update_clip(&nid, |c| {
                        if let Some(m) = c.motion.as_mut() {
                            m.freeze = false;
                        }
                    });
                    cx.notify();
                })),
            );
        }
        g = g
            .child(chips)
            .child(self.keyed_slider(c, "speed", "Speed", 0.25, 4.0, 0.01, |v| format!("{v:.2}×"), Some(apply), cx))
            .child(field_row("Direction", direction, None))
            .child(div().flex().flex_col().gap_2().child(label("Speed ramp")).child(ramps))
            .when(ramped, |d| {
                let id = id.clone();
                d.child(button("no-ramp", Variant::Ghost, 28.).text_size(px(12.)).child("Remove ramp").on_click(cx.listener(move |this, _, _, cx| {
                    this.store.ramp_preset(&id, "none");
                    cx.notify();
                })))
            })
            .child(
                button("freeze", Variant::Secondary, 30.).text_size(px(12.)).child(icon("pause", 13., ink())).child("Freeze frame at playhead").on_click(cx.listener(move |this, _, _, cx| {
                    this.store.freeze_frame(&fid, 2.0);
                    this.restart_audio();
                    cx.notify();
                })),
            )
            .child(micro(if ramped { "The ramp follows the Speed keyframes: move the playhead, set a speed, and the clip eases between them." } else { "Hold ⌥ and drag the clip's right edge on the timeline to stretch it to any speed." }));
        g
    }

    /// Several clips selected: what they share, changed all at once.
    fn multi_inspector(&mut self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let sel = self.store.selected_clips();
        let Some(first) = sel.first().cloned() else { return vec![] };
        let n = sel.len();
        let start = sel.iter().map(|c| c.start).fold(f64::INFINITY, f64::min);
        let end = sel.iter().map(|c| c.start + c.dur).fold(0.0, f64::max);
        let visual = sel.iter().filter(|c| c.visual()).count();
        let sound = sel.iter().filter(|c| matches!(c.kind, AssetType::Audio | AssetType::Music | AssetType::Video)).count();
        let tracks: std::collections::BTreeSet<&str> = sel.iter().map(|c| c.track.as_str()).collect();
        let all = |f: fn(&mut Clip, f64)| -> Apply { Rc::new(move |e: &mut Editor, v| e.store.update_selection(|c| f(c, v))) };
        let max_fade = sel.iter().map(|c| c.dur / 2.0).fold(f64::INFINITY, f64::min).min(3.0).max(0.05);
        let mut out = vec![group(None, None, false)
            .child(type_label(div()).self_start().rounded(px(6.)).bg(surface3()).px_2().py_1().text_color(ink_muted()).child(format!("{n} CLIPS · {}", tracks.into_iter().collect::<Vec<_>>().join(" "))))
            .child(type_headline(div()).child(format!("{n} clips selected")))
            .child(kv(vec![("From".into(), tc(start)), ("To".into(), tc(end)), ("Spans".into(), tc(end - start))]))
            .child(micro("Changes here apply to every selected clip. Drag any of them on the timeline to move them together."))
            .into_any_element()];
        let mut g = group(Some("Together"), None, false);
        if visual > 0 {
            let first_vis = sel.iter().find(|c| c.visual()).map(|c| c.opacity).unwrap_or(1.0);
            g = g.child(self.slider_field("Opacity", "m-opacity", first_vis, 0.0, 1.0, 0.01, pct, all(|c, v| if c.visual() { c.opacity = v }), cx));
        }
        if sound > 0 {
            let first_snd = sel.iter().find(|c| !c.visual() || c.kind == AssetType::Video).map(|c| c.volume).unwrap_or(1.0);
            g = g.child(self.slider_field("Volume", "m-volume", first_snd, 0.0, 1.0, 0.01, pct, all(|c, v| if c.kind.is_audio() || c.kind == AssetType::Video { c.volume = v }), cx));
        }
        g = g
            .child(self.slider_field("Fade in", "m-fadein", first.fade_in.min(max_fade), 0.0, max_fade, 0.05, |v| format!("{v:.2}s"), all(|c, v| c.fade_in = v), cx))
            .child(self.slider_field("Fade out", "m-fadeout", first.fade_out.min(max_fade), 0.0, max_fade, 0.05, |v| format!("{v:.2}s"), all(|c, v| c.fade_out = v), cx));
        let timed: Vec<String> = sel.iter().filter(|c| matches!(c.kind, AssetType::Video | AssetType::Scene | AssetType::Overlay | AssetType::Audio)).map(|c| c.id.clone()).collect();
        if !timed.is_empty() {
            let chips = div().flex().flex_wrap().gap(px(6.)).children([(0.5, "½×"), (1.0, "1×"), (2.0, "2×")].into_iter().map(|(v, l)| {
                let ids = timed.clone();
                button(SharedString::from(format!("mspd-{l}")), Variant::Secondary, 28.)
                    .text_size(px(12.))
                    .child(l)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.store.commit();
                        for id in &ids {
                            this.store.set_speed(id, v);
                        }
                        this.restart_audio();
                        cx.notify();
                    }))
                    .into_any_element()
            }));
            g = g.child(field_row("Speed", chips, None));
        }
        out.push(g.into_any_element());
        out.push(
            group(None, None, true)
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap(px(6.))
                        .child(button("m-split", Variant::Secondary, 32.).child(icon("scissors", 14., ink())).child("Split").on_click(cx.listener(|this, _, _, cx| {
                            this.store.split();
                            cx.notify();
                        })))
                        .child(button("m-dup", Variant::Secondary, 32.).child(icon("copy", 14., ink())).child("Duplicate").on_click(cx.listener(|this, _, _, cx| {
                            this.store.duplicate();
                            cx.notify();
                        })))
                        .child(button("m-del", Variant::Destructive, 32.).child(icon("trash-2", 14., coral())).child(format!("Delete {n}")).on_click(cx.listener(|this, _, _, cx| {
                            this.store.remove();
                            cx.notify();
                        }))),
                )
                .into_any_element(),
        );
        out
    }

    /// Speed, reverb and the rest for an audio clip, with one-click presets on top.
    fn sound_group(&self, c: &Clip, cx: &mut Context<Self>) -> Div {
        use crate::audio::fx::PRESETS;
        let fx = c.sound.clone().unwrap_or_default();
        let id = c.id.clone();
        let chips = div().flex().flex_wrap().gap(px(6.)).children(PRESETS.iter().map(|(key, label, p)| {
            let active = fx == *p;
            let (id, p) = (id.clone(), p.clone());
            button(SharedString::from(format!("snd-{key}")), if active { Variant::Primary } else { Variant::Secondary }, 28.)
                .text_size(px(12.))
                .child(*label)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.store.commit();
                    let p = p.clone();
                    this.store.set_sound(&id, move |f| *f = p);
                    cx.notify();
                }))
                .into_any_element()
        }));
        let sl = |this: &Editor, label: &str, value: f64, min: f64, max: f64, step: f64, fmt: fn(f64) -> String, set: fn(&mut AudioFx, f64), cx: &mut Context<Editor>| {
            let id = id.clone();
            this.slider_field(label, format!("snd-{label}"), value, min, max, step, fmt, Rc::new(move |e: &mut Editor, v| e.store.set_sound(&id, |f| set(f, v))), cx)
        };
        let pct = |v: f64| format!("{}%", (v * 100.0).round());
        let ed = cx.entity();
        let pid = id.clone();
        let pitch = segmented(&format!("snd-pitch-{id}"), &[(false, "Follows speed"), (true, "Keep pitch")], &fx.keep_pitch, |_| false, move |v, _, cx| {
            let pid = pid.clone();
            ed.update(cx, |e, cx| {
                e.store.commit();
                e.store.set_sound(&pid, |f| f.keep_pitch = v);
                cx.notify();
            })
        });
        group(Some("Sound"), (!fx.is_identity()).then(|| "Reworked".to_string()), false)
            .child(chips)
            .child(sl(self, "Speed", fx.speed, 0.5, 2.0, 0.01, |v| format!("{v:.2}×"), |f, v| f.speed = v, cx))
            .child(field_row("Pitch", pitch, None))
            .child(sl(self, "Reverb", fx.reverb, 0.0, 1.0, 0.01, pct, |f, v| f.reverb = v, cx))
            .child(sl(self, "Room size", fx.room, 0.0, 1.0, 0.01, pct, |f, v| f.room = v, cx))
            .child(sl(self, "Bass boost", fx.bass, 0.0, 1.0, 0.01, pct, |f, v| f.bass = v, cx))
            .child(sl(self, "Muffle", fx.muffle, 0.0, 1.0, 0.01, pct, |f, v| f.muffle = v, cx))
            .child(sl(self, "8D", fx.pan8d, 0.0, 1.0, 0.01, pct, |f, v| f.pan8d = v, cx))
            .child(label("EQ"))
            .child(sl(self, "Low", fx.eq_low, -12.0, 12.0, 0.5, |v| format!("{v:+.1} dB"), |f, v| f.eq_low = v, cx))
            .child(sl(self, "Mid", fx.eq_mid, -12.0, 12.0, 0.5, |v| format!("{v:+.1} dB"), |f, v| f.eq_mid = v, cx))
            .child(sl(self, "High", fx.eq_high, -12.0, 12.0, 0.5, |v| format!("{v:+.1} dB"), |f, v| f.eq_high = v, cx))
            .child(micro("Slowing down makes the clip longer and speeding up makes it shorter. Reverb tails ring out to the clip's end."))
    }

    fn text_groups(&mut self, c: &Clip, t: &TextProps, window: &mut Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let id = c.id.clone();
        let set = |f: Box<dyn Fn(&mut TextProps)>| {
            let id = id.clone();
            let f = Rc::new(f);
            move |e: &mut Editor| {
                e.store.commit();
                let f = f.clone();
                e.store.update_text(&id, move |t| f(t));
            }
        };
        let font_item = |fam: &'static str| {
            font_item(fam, fam == t.font, set(Box::new(move |t: &mut TextProps| {
                t.font = fam.into();
                t.weight = font_def(fam).weight;
            })))
        };
        // The bundled faces, then every font installed on this Mac.
        let mut font_entries: Vec<MenuEntry> = FONTS.iter().map(|f| font_item(f.family)).collect();
        let installed = crate::render::text::system_families();
        if !installed.is_empty() {
            font_entries.push(MenuEntry::Separator);
            font_entries.push(MenuEntry::Label(format!("Installed on this Mac ({})", installed.len())));
            font_entries.extend(installed.iter().map(|f| font_item(f.as_str())));
        }
        let weights = [(400.0, "Regular"), (500.0, "Medium"), (600.0, "Semibold"), (700.0, "Bold"), (800.0, "Heavy")];
        let weight_label = weights.iter().find(|(w, _)| *w == t.weight).map(|(_, l)| *l).unwrap_or("Regular");
        let weight_entries: Vec<MenuEntry> = weights.iter().map(|(w, l)| {
            let w = *w;
            item(*l, w == t.weight, set(Box::new(move |t: &mut TextProps| t.weight = w)))
        }).collect();
        // Twelve animations don't fit a segmented control: a menu, like Font and Weight.
        let anim_label = TEXT_ANIMS.iter().find(|(a, _)| *a == t.anim).map(|(_, l)| l.to_string()).unwrap_or_else(|| "None".into());
        let anim_entries: Vec<MenuEntry> = TEXT_ANIMS.iter().map(|(a, l)| {
            let a = *a;
            item(*l, a == t.anim, set(Box::new(move |t: &mut TextProps| t.anim = a)))
        }).collect();
        let tid = id.clone();
        let tid2 = id.clone();
        let ed = cx.entity();
        let seg_set = move |f: Box<dyn Fn(&mut TextProps)>| {
            let ed = ed.clone();
            let id = id.clone();
            let f = Rc::new(f);
            move |_w: &mut Window, cx: &mut gpui::App| {
                let f = f.clone();
                let id = id.clone();
                ed.update(cx, move |e, cx| {
                    e.store.commit();
                    e.store.update_text(&id, move |t| f(t));
                    cx.notify();
                });
            }
        };
        let s_align = seg_set.clone();
        let s_box = seg_set.clone();
        let s_col = seg_set.clone();
        let s_blend = seg_set.clone();
        // A custom-colour button next to the palette; picks are remembered for every row.
        let tcol_apply: Rc<dyn Fn(&mut Editor, String)> = Rc::new({
            let tid3 = c.id.clone();
            move |e: &mut Editor, col: String| {
                e.store.commit();
                e.store.update_text(&tid3, move |t| t.color = col);
            }
        });
        // The brand's colours lead the swatches.
        let mut brand_cols: Vec<String> = crate::model::brand::load().colors;
        let rest: Vec<String> = TEXT_COLORS.iter().map(|c| c.to_string()).filter(|c| !brand_cols.iter().any(|b| b.eq_ignore_ascii_case(c))).collect();
        brand_cols.extend(rest);
        let tcol_picker = color_picker(&mut self.pickers, &format!("tcol-{}", c.id), &t.color, tcol_apply, window, cx);
        vec![
            group(Some("Text"), None, false)
                .child(div().relative().child(self.track_bounds("input:text")).child(Textarea::new(&self.text_input).h(px(64.)).aria_label("Text")))
                .child(field_row("Font", self.select("f-font", t.font.clone(), font_entries, cx), None))
                .child(field_row("Weight", self.select("f-weight", weight_label, weight_entries, cx), None))
                .child(self.slider_field("Size", "f-tsize", t.size, 16.0, 240.0, 1.0, |v| format!("{v}px"), Rc::new(move |e: &mut Editor, v| e.store.update_text(&tid, |t| t.size = v)), cx))
                .child(self.slider_field("Height", "f-ty", t.y, -320.0, 320.0, 1.0, |v| format!("{}", v.round()), Rc::new(move |e: &mut Editor, v| e.store.update_text(&tid2, |t| t.y = v)), cx))
                .child(swatches("tcol", &brand_cols.iter().map(String::as_str).collect::<Vec<_>>(), &t.color, 18., Some(tcol_picker), move |col, w, cx| (s_col(Box::new(move |t: &mut TextProps| t.color = col.clone())))(w, cx)))
                .child(field_row(
                    "Style",
                    segmented(
                        "tblend",
                        &[(TextBlend::Normal, "Normal"), (TextBlend::Invert, "Invert"), (TextBlend::Exclusion, "Soft invert")],
                        &t.blend,
                        |_| false,
                        move |v, w, cx| (s_blend(Box::new(move |t: &mut TextProps| t.blend = v)))(w, cx),
                    ),
                    None,
                ))
                .into_any_element(),
            group(Some("Layout & motion"), None, false)
                .child(field_row(
                    "Align",
                    segmented("align", &[(TextAlign::Left, "Left"), (TextAlign::Center, "Center"), (TextAlign::Right, "Right")], &t.align, |_| false, move |v, w, cx| (s_align(Box::new(move |t: &mut TextProps| t.align = v)))(w, cx)),
                    None,
                ))
                .child(field_row(
                    "Backing",
                    segmented("box", &[(false, "None"), (true, "Box")], &t.boxed, |_| false, move |v, w, cx| (s_box(Box::new(move |t: &mut TextProps| t.boxed = v)))(w, cx)),
                    None,
                ))
                .child(field_row("Animate", self.select("f-anim", anim_label, anim_entries, cx), None))
                // Turned in 3D, in perspective.
                .child(self.slider_field("3D tilt ↕", "f-tiltx", t.tilt_x, -70.0, 70.0, 1.0, |v| format!("{}°", v.round()), Rc::new({
                    let id = c.id.clone();
                    move |e: &mut Editor, v| e.store.update_text(&id, |t| t.tilt_x = v)
                }), cx))
                .child(self.slider_field("3D tilt ↔", "f-tilty", t.tilt_y, -70.0, 70.0, 1.0, |v| format!("{}°", v.round()), Rc::new({
                    let id = c.id.clone();
                    move |e: &mut Editor, v| e.store.update_text(&id, |t| t.tilt_y = v)
                }), cx))
                .into_any_element(),
        ]
    }

    /// The transition into this clip: type, length, and whether it blends from a previous clip.
    fn transition_control(&self, c: &Clip, cx: &mut Context<Self>) -> Div {
        let prev_touches = self.store.clips.iter().any(|k| k.track == c.track && k.id != c.id && (k.start + k.dur - c.start).abs() < 1.5 / 30.0);
        let tr = c.transition.clone();
        let def = tr.as_ref().and_then(|t| transition_def(&t.id));
        let id = c.id.clone();
        let mut entries = vec![{
            let id = id.clone();
            item("None (hard cut)", tr.is_none(), move |e: &mut Editor| {
                e.store.commit();
                e.store.update_clip(&id, |c| c.transition = None);
            })
        }];
        for (g, l) in TRANSITION_GROUPS {
            entries.push(MenuEntry::Separator);
            entries.push(MenuEntry::Label(l.to_string()));
            for t in TRANSITIONS.iter().filter(|t| t.group == g) {
                let id = id.clone();
                let tid = t.id;
                entries.push(item(t.name, tr.as_ref().map(|x| x.id == tid).unwrap_or(false), move |e: &mut Editor| e.store.apply_transition(tid, Some(&id))));
            }
        }
        let action = def.map(|_| if prev_touches { "Blends from the clip before".to_string() } else { "Plays over the start".to_string() });
        let mut g = group(Some("Transition in"), action, false).child(field_row("Type", self.select("f-transition", def.map(|d| d.name).unwrap_or("None (hard cut)"), entries, cx), None));
        if let Some(tr) = tr {
            let id = id.clone();
            g = g.child(self.slider_field(
                "Duration",
                "f-trdur",
                tr.dur,
                0.1,
                (c.dur.min(3.0)).max(0.2),
                0.05,
                |v| format!("{v:.2}s"),
                Rc::new(move |e: &mut Editor, v| e.store.update_clip(&id, |c| {
                    if let Some(t) = c.transition.as_mut() {
                        t.dur = v;
                    }
                })),
                cx,
            ));
        }
        g
    }

    /// The clip's effects, top to bottom in the order they are applied.
    fn effect_stack(&mut self, c: &Clip, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let on_object = effects_on_object(c);
        let id = c.id.clone();
        let ed = cx.entity();
        let scope = segmented(
            "fxscope",
            &[(FxScope::Clip, if c.text.is_some() { "Just the text" } else { "Just this clip" }), (FxScope::Frame, "Whole frame")],
            &if on_object { FxScope::Clip } else { FxScope::Frame },
            |_| false,
            move |v, _, cx| {
                let id = id.clone();
                ed.update(cx, |e, cx| {
                    e.store.commit();
                    e.store.update_clip(&id, |c| c.fx_scope = Some(v));
                    cx.notify();
                })
            },
        );
        let n = c.effects.len();
        let mut g = group(Some("Effects"), (n > 0).then(|| format!("{n} applied, top first")), false).child(field_row("Applies to", scope, None));
        if n == 0 {
            g = g.child(micro("No effects yet. Add one below, or drag one from the Effects tab onto the clip."));
        }
        for (i, fx) in c.effects.iter().enumerate() {
            g = g.child(self.effect_card(c, fx, i == 0, i + 1 == n, window, cx));
        }
        let mut entries = vec![];
        for (gi, (grp, l)) in EFFECT_GROUPS.iter().enumerate() {
            if gi > 0 {
                entries.push(MenuEntry::Separator);
            }
            entries.push(MenuEntry::Label(l.to_string()));
            for e in effects().iter().filter(|e| e.group == *grp) {
                let id = c.id.clone();
                let eid = e.id;
                entries.push(item(e.name, false, move |ed: &mut Editor| ed.store.apply_effect(eid, Some(&id))));
            }
        }
        g.child(div().child(self.select("add-effect", "+  Add effect", entries, cx)))
    }

    fn effect_card(&mut self, c: &Clip, fx: &ClipEffect, first: bool, last: bool, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(def) = effect_def(&fx.id) else { return div().into_any_element() };
        let (cid, uid) = (c.id.clone(), fx.uid.clone());
        let act = |f: fn(&mut Editor, &str, &str)| {
            let (cid, uid) = (cid.clone(), uid.clone());
            cx.listener(move |this: &mut Editor, _: &gpui::ClickEvent, _, cx| {
                f(this, &cid, &uid);
                cx.notify();
            })
        };
        let header = div()
            .flex()
            .items_center()
            .gap_1()
            .child(type_caption(div()).flex_1().truncate().child(def.name))
            .child(icon_button(SharedString::from(format!("fx-en-{uid}")), if fx.enabled { "eye" } else { "eye-off" }, if fx.enabled { "Turn effect off" } else { "Turn effect on" }, None, 24., Variant::Ghost, act(|e, c, u| {
                e.store.commit();
                e.store.update_effect(c, u, |x| x.enabled = !x.enabled);
            })))
            .child(icon_button(SharedString::from(format!("fx-up-{uid}")), "chevron-up", "Move up", None, 24., Variant::Ghost, act(|e, c, u| e.store.move_effect(c, u, -1))).when(first, |d| d.opacity(0.3)))
            .child(icon_button(SharedString::from(format!("fx-dn-{uid}")), "chevron-down", "Move down", None, 24., Variant::Ghost, act(|e, c, u| e.store.move_effect(c, u, 1))).when(last, |d| d.opacity(0.3)))
            .child(icon_button(SharedString::from(format!("fx-rm-{uid}")), "x", "Remove effect", None, 24., Variant::Ghost, act(|e, c, u| e.store.remove_effect(c, u))));

        let mix = self.keyed_slider(
            c,
            &format!("fx.{uid}.amount"),
            if matches!(def.engine, crate::fx::registry::Engine::Css | crate::fx::registry::Engine::Look) { "Strength" } else { "Mix" },
            0.0,
            1.0,
            0.01,
            pct,
            None,
            cx,
        );
        let mut card = div().flex().flex_col().gap_3().rounded(px(10.)).bg(surface2()).p_3().when(!fx.enabled, |d| d.opacity(0.6)).child(header).child(mix);
        if super::grade_ui::is_grade(fx) {
            return card.children(self.grade_controls(c, fx, cx)).into_any_element();
        }
        for p in &def.params {
            if p.key == "tint" && fx.params.get("color") != Some(&ParamValue::Str("tint".into())) {
                continue;
            }
            let key = p.key;
            let (cid, uid) = (cid.clone(), uid.clone());
            let setter = move |v: ParamValue| {
                let (cid, uid) = (cid.clone(), uid.clone());
                move |e: &mut Editor| e.store.update_effect(&cid, &uid, |x| {
                    x.params.insert(key.to_string(), v.clone());
                })
            };
            let el: AnyElement = match &p.kind {
                ParamKind::File(ext) => self.file_param(c, fx, key, p.label, ext, cx),
                ParamKind::Range { min, max, step, unit } => {
                    let unit = *unit;
                    // A parameter never set yet shows its lowest value.
                    self.keyed_slider(c, &format!("fx.{}.{key}", fx.uid), p.label, *min, *max, *step, move |v| format!("{}{unit}", (v * 100.0).round() / 100.0), None, cx).into_any_element()
                }
                ParamKind::Select(options) => {
                    let cur = match fx.params.get(key) {
                        Some(ParamValue::Str(s)) => s.clone(),
                        _ => String::new(),
                    };
                    let label_now = options.iter().find(|(v, _)| *v == cur).map(|(_, l)| *l).unwrap_or("");
                    let entries = options
                        .iter()
                        .map(|(v, l)| {
                            let s = setter.clone();
                            let v = v.to_string();
                            item(*l, cur == v, move |e: &mut Editor| {
                                e.store.commit();
                                s(ParamValue::Str(v.clone()))(e)
                            })
                        })
                        .collect();
                    field_row(p.label, self.select(SharedString::from(format!("fx-{}-{key}", fx.uid)), label_now, entries, cx), None).into_any_element()
                }
                ParamKind::Toggle => {
                    let on = matches!(fx.params.get(key), Some(ParamValue::Bool(true)));
                    let ed = cx.entity();
                    let s = setter.clone();
                    field_row(
                        p.label,
                        segmented(&format!("fx-{}-{key}", fx.uid), &[(false, "Off"), (true, "On")], &on, |_| false, move |v, _, cx| {
                            let s = s.clone();
                            ed.update(cx, |e, cx| {
                                e.store.commit();
                                s(ParamValue::Bool(v))(e);
                                cx.notify();
                            })
                        }),
                        None,
                    )
                    .into_any_element()
                }
                ParamKind::Color => {
                    let cur = match fx.params.get(key) {
                        Some(ParamValue::Str(s)) => s.clone(),
                        _ => "#ffffff".into(),
                    };
                    let ed = cx.entity();
                    let s = setter.clone();
                    let (cid2, uid2, pkey2) = (c.id.clone(), fx.uid.to_string(), key.to_string());
                    let apply: Rc<dyn Fn(&mut Editor, String)> = Rc::new(move |e: &mut Editor, col: String| {
                        e.store.commit();
                        e.store.update_effect(&cid2, &uid2, |x| {
                            x.params.insert(pkey2.clone(), ParamValue::Str(col.clone()));
                        });
                    });
                    let picker = color_picker(&mut self.pickers, &format!("fx-{}-{key}", fx.uid), &cur, apply, window, cx);
                    field_row(
                        p.label,
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap(px(4.))
                            .child(swatches(&format!("fxs-{}-{key}", fx.uid), &SWATCHES, &cur, 16., Some(picker), move |col, _, cx| {
                                let s = s.clone();
                                ed.update(cx, |e, cx| {
                                    e.store.commit();
                                    s(ParamValue::Str(col))(e);
                                    cx.notify();
                                })
                            })),
                        None,
                    )
                    .into_any_element()
                }
            };
            card = card.child(el);
        }
        card.into_any_element()
    }

}


/// The keyframe button's diamond: filled when there's a key at the playhead.
fn key_glyph(color: gpui::Hsla, filled: bool) -> impl IntoElement {
    gpui::canvas(|_, _, _| {}, move |b: gpui::Bounds<gpui::Pixels>, _, window, _| {
        let (cx, cy) = (b.origin.x + b.size.width / 2.0, b.origin.y + b.size.height / 2.0);
        let r = px(5.);
        let mut pb = if filled { gpui::PathBuilder::fill() } else { gpui::PathBuilder::stroke(px(1.3)) };
        pb.move_to(gpui::point(cx, cy - r));
        pb.line_to(gpui::point(cx + r, cy));
        pb.line_to(gpui::point(cx, cy + r));
        pb.line_to(gpui::point(cx - r, cy));
        pb.close();
        if let Ok(p) = pb.build() {
            window.paint_path(p, color);
        }
    })
    .size_full()
}
