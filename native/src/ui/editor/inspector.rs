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
use gpui::{div, px, relative, AnyElement, Context, Div, Entity, SharedString, Window};
use gpui_component::button::Button;
use gpui_component::color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState};
use gpui_component::input::Textarea;
use gpui_component::menu::{DropdownMenu, PopupMenuItem};
use gpui_component::{Colorize, Sizable};
use std::rc::Rc;

pub enum MenuEntry {
    Label(String),
    Separator,
    Item { label: String, checked: bool, act: Rc<dyn Fn(&mut Editor)> },
}
pub fn item(label: impl Into<String>, checked: bool, act: impl Fn(&mut Editor) + 'static) -> MenuEntry {
    MenuEntry::Item { label: label.into(), checked, act: Rc::new(act) }
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

    /// Label · slider · value, with one undo step per drag.
    #[allow(clippy::too_many_arguments)]
    pub fn slider_field(&self, label_text: &str, key: impl Into<String>, value: f64, min: f64, max: f64, step: f64, fmt: impl Fn(f64) -> String, apply: Apply, cx: &mut Context<Self>) -> Div {
        field_row(label_text, self.slider(key, value, min, max, step, true, apply, cx), Some(fmt(value)))
    }

    /// A select: a pill showing the current choice that opens a menu.
    pub fn select(&self, id: impl Into<SharedString>, current: impl Into<SharedString>, entries: Vec<MenuEntry>, cx: &mut Context<Self>) -> impl IntoElement {
        let ed: Entity<Editor> = cx.entity();
        let entries = Rc::new(entries);
        let long = entries.len() > 12;
        Button::new(id.into())
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
                if long {
                    menu = menu.scrollable(true).max_h(px(360.));
                }
                for e in entries.iter() {
                    menu = match e {
                        MenuEntry::Label(l) => menu.label(l.clone()),
                        MenuEntry::Separator => menu.separator(),
                        MenuEntry::Item { label, checked, act } => {
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
    }

    pub(super) fn render_inspector(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body: Vec<AnyElement> = match self.store.selected_clip().cloned() {
            Some(c) => self.clip_inspector(&c, window, cx),
            None => self.project_inspector(),
        };
        panel()
            .w(px(300.))
            .min_w(px(260.))
            .flex_none()
            .child(panel_header().child(label("Inspector")))
            .child(div().id("inspector-scroll").flex_1().min_h_0().overflow_y_scroll().child(div().flex().flex_col().px_4().pb_4().children(body)))
    }

    fn project_inspector(&self) -> Vec<AnyElement> {
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
            ("+ −", "Zoom timeline"),
            ("N", "Toggle snapping"),
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
            out.extend(self.text_groups(c, t, cx));
        }
        let id = c.id.clone();
        let num = |this: &Editor, label: &str, field: fn(&mut Clip) -> &mut f64, value: f64, min: f64, max: f64, step: f64, fmt: fn(f64) -> String, cx: &mut Context<Editor>| {
            let id = id.clone();
            this.slider_field(label, format!("f-{label}"), value, min, max, step, fmt, Rc::new(move |e: &mut Editor, v| e.store.update_clip(&id, |c| *field(c) = v)), cx)
        };
        if c.visual() {
            out.push(
                group(Some("Transform"), None, false)
                    .child(num(self, "Position X", |c| &mut c.x, c.x, -(crate::model::time::W() as f64) / 2.0, crate::model::time::W() as f64 / 2.0, 1.0, |v| format!("{}", v.round()), cx))
                    .child(num(self, "Position Y", |c| &mut c.y, c.y, -(crate::model::time::H() as f64) / 2.0, crate::model::time::H() as f64 / 2.0, 1.0, |v| format!("{}", v.round()), cx))
                    .child(num(self, "Scale", |c| &mut c.scale, c.scale, 0.1, 3.0, 0.01, pct, cx))
                    .child(num(self, "Rotation", |c| &mut c.rot, c.rot, -180.0, 180.0, 1.0, |v| format!("{v}°"), cx))
                    .child(num(self, "Opacity", |c| &mut c.opacity, c.opacity, 0.0, 1.0, 0.01, pct, cx))
                    .into_any_element(),
            );
            out.push(self.transition_control(c, cx).into_any_element());
            out.push(self.effect_stack(c, window, cx).into_any_element());
        } else {
            let mut g = group(Some("Audio"), None, false).child(num(self, "Volume", |c| &mut c.volume, c.volume, 0.0, 1.0, 0.01, pct, cx));
            if c.kind == AssetType::Music {
                if let Some(a) = asset(&c.asset) {
                    g = g.child(micro(format!("{} · {} BPM. The track keeps playing for as long as you stretch the clip.", a.sub, a.bpm)));
                }
            }
            out.push(g.into_any_element());
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

    fn text_groups(&self, c: &Clip, t: &TextProps, cx: &mut Context<Self>) -> Vec<AnyElement> {
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
        let font_entries: Vec<MenuEntry> = FONTS
            .iter()
            .map(|f| {
                let fam = f.family;
                item(fam, fam == t.font, set(Box::new(move |t: &mut TextProps| {
                    t.font = fam.into();
                    t.weight = font_def(fam).weight;
                })))
            })
            .collect();
        let weights = [(400.0, "Regular"), (500.0, "Medium"), (600.0, "Semibold"), (700.0, "Bold"), (800.0, "Heavy")];
        let weight_label = weights.iter().find(|(w, _)| *w == t.weight).map(|(_, l)| *l).unwrap_or("Regular");
        let weight_entries: Vec<MenuEntry> = weights.iter().map(|(w, l)| {
            let w = *w;
            item(*l, w == t.weight, set(Box::new(move |t: &mut TextProps| t.weight = w)))
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
        let s_anim = seg_set.clone();
        let s_col = seg_set.clone();
        vec![
            group(Some("Text"), None, false)
                .child(div().relative().child(self.track_bounds("input:text")).child(Textarea::new(&self.text_input).h(px(64.)).aria_label("Text")))
                .child(field_row("Font", self.select("f-font", t.font.clone(), font_entries, cx), None))
                .child(field_row("Weight", self.select("f-weight", weight_label, weight_entries, cx), None))
                .child(self.slider_field("Size", "f-tsize", t.size, 16.0, 240.0, 1.0, |v| format!("{v}px"), Rc::new(move |e: &mut Editor, v| e.store.update_text(&tid, |t| t.size = v)), cx))
                .child(self.slider_field("Height", "f-ty", t.y, -320.0, 320.0, 1.0, |v| format!("{}", v.round()), Rc::new(move |e: &mut Editor, v| e.store.update_text(&tid2, |t| t.y = v)), cx))
                .child(swatches("tcol", &TEXT_COLORS, &t.color, 18., move |col, w, cx| (s_col(Box::new(move |t: &mut TextProps| t.color = col.clone())))(w, cx)))
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
                .child(field_row("Animate", segmented("anim", &TEXT_ANIMS, &t.anim, |_| false, move |v, w, cx| (s_anim(Box::new(move |t: &mut TextProps| t.anim = v)))(w, cx)), None))
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

        let mix = {
            let (cid, uid) = (cid.clone(), uid.clone());
            self.slider_field(
                if matches!(def.engine, crate::fx::registry::Engine::Css | crate::fx::registry::Engine::Look) { "Strength" } else { "Mix" },
                format!("fx-{uid}-amount"),
                fx.amount,
                0.0,
                1.0,
                0.01,
                pct,
                Rc::new(move |e: &mut Editor, v| e.store.update_effect(&cid, &uid, |x| x.amount = v)),
                cx,
            )
        };
        let mut card = div().flex().flex_col().gap_3().rounded(px(10.)).bg(surface2()).p_3().when(!fx.enabled, |d| d.opacity(0.6)).child(header).child(mix);
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
                ParamKind::Range { min, max, step, unit } => {
                    let v = match fx.params.get(key) {
                        Some(ParamValue::Num(n)) => *n,
                        _ => *min,
                    };
                    let unit = *unit;
                    let s = setter.clone();
                    self.slider_field(p.label, format!("fx-{}-{key}", fx.uid), v, *min, *max, *step, move |v| format!("{}{unit}", (v * 100.0).round() / 100.0), Rc::new(move |e: &mut Editor, v| s(ParamValue::Num(v))(e)), cx)
                        .into_any_element()
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
                    let picker = self.color_picker(&c.id, &fx.uid, key, &cur, window, cx);
                    field_row(
                        p.label,
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap(px(4.))
                            .child(swatches(&format!("fxs-{}-{key}", fx.uid), &SWATCHES, &cur, 16., move |col, _, cx| {
                                let s = s.clone();
                                ed.update(cx, |e, cx| {
                                    e.store.commit();
                                    s(ParamValue::Str(col))(e);
                                    cx.notify();
                                })
                            }))
                            .child(picker),
                        None,
                    )
                    .into_any_element()
                }
            };
            card = card.child(el);
        }
        card.into_any_element()
    }

    /// A custom-colour button for an effect colour (state kept per effect parameter).
    fn color_picker(&mut self, cid: &str, uid: &str, pkey: &'static str, cur: &str, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let k = format!("{uid}-{pkey}");
        if !self.pickers.contains_key(&k) {
            let st = cx.new(|cx| ColorPickerState::new(window, cx).default_value(hex(cur)));
            let (cid, uid) = (cid.to_string(), uid.to_string());
            let sub = cx.subscribe_in(&st, window, move |this: &mut Editor, _, ev: &ColorPickerEvent, _, cx| {
                if let ColorPickerEvent::Change(Some(c)) = ev {
                    let col = c.to_hex();
                    this.store.commit();
                    this.store.update_effect(&cid, &uid, |x| {
                        x.params.insert(pkey.to_string(), ParamValue::Str(col.clone()));
                    });
                    cx.notify();
                }
            });
            self.pickers.insert(k.clone(), (st, sub));
        }
        ColorPicker::new(&self.pickers[&k].0).small().into_any_element()
    }
}

