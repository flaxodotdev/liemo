// Design tokens (docs/DESIGN.md, as in globals.css) and the building blocks every panel uses,
// so spacing, type and states stay identical. Dark only; hierarchy comes from surface lift.

use gpui::prelude::*;
use gpui::{div, px, rgb, rgba, svg, AnyElement, App, ClickEvent, Context, Div, ElementId, Entity, FontWeight, Hsla, SharedString, Stateful, Window};
use gpui_component::color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState};
use gpui_component::tooltip::Tooltip;
use gpui_component::{Colorize, Sizable};
use std::collections::HashMap;
use std::rc::Rc;

pub fn canvas_bg() -> Hsla {
    rgb(0x090909).into()
}
pub fn surface1() -> Hsla {
    rgb(0x141414).into()
}
pub fn surface2() -> Hsla {
    rgb(0x1c1c1c).into()
}
pub fn surface3() -> Hsla {
    rgb(0x242424).into()
}
pub fn hairline() -> Hsla {
    rgb(0x262626).into()
}
pub fn hairline_soft() -> Hsla {
    rgb(0x1a1a1a).into()
}
pub fn ink() -> Hsla {
    rgb(0xffffff).into()
}
pub fn ink_muted() -> Hsla {
    rgb(0x999999).into()
}
pub fn signal() -> Hsla {
    rgb(0x0099ff).into()
}
pub fn coral() -> Hsla {
    rgb(0xff5577).into()
}
pub fn success() -> Hsla {
    rgb(0x22c55e).into()
}
pub fn black() -> Hsla {
    rgb(0x000000).into()
}
pub fn alpha(c: Hsla, a: f32) -> Hsla {
    Hsla { a: c.a * a, ..c }
}
pub fn white_a(a: f32) -> Hsla {
    alpha(ink(), a)
}
pub fn hex(s: &str) -> Hsla {
    let [r, g, b, a] = crate::render::canvas::hex_rgba(s);
    rgba(((r as u32) << 24) | ((g as u32) << 16) | ((b as u32) << 8) | a as u32).into()
}

pub const SANS: &str = "Inter";
pub const DISPLAY: &str = "Mona Sans";
pub const MONO: &str = "JetBrains Mono";

// ---------- Type scale ----------
pub fn type_headline(d: Div) -> Div {
    d.font_family(DISPLAY).text_size(px(22.)).font_weight(FontWeight::SEMIBOLD).line_height(px(26.))
}
pub fn type_body(d: Div) -> Div {
    d.text_size(px(15.)).line_height(px(19.5))
}
pub fn type_body_sm(d: Div) -> Div {
    d.text_size(px(14.)).font_weight(FontWeight::MEDIUM).line_height(px(19.6))
}
pub fn type_caption(d: Div) -> Div {
    d.text_size(px(13.)).font_weight(FontWeight::MEDIUM).line_height(px(15.6))
}
pub fn type_micro(d: Div) -> Div {
    d.text_size(px(12.)).font_weight(FontWeight::NORMAL).line_height(px(15.))
}
pub fn type_label(d: Div) -> Div {
    d.text_size(px(11.)).font_weight(FontWeight::MEDIUM).line_height(px(13.2))
}
pub fn type_timecode(d: Div, size: f32) -> Div {
    d.font_family(MONO).text_size(px(size)).font_weight(FontWeight::MEDIUM)
}

/// An uppercase section label.
pub fn label(text: impl Into<SharedString>) -> Div {
    let t: SharedString = text.into();
    type_label(div()).text_color(ink_muted()).child(SharedString::from(t.to_uppercase()))
}
pub fn micro(text: impl Into<SharedString>) -> Div {
    type_micro(div()).text_color(ink_muted()).child(text.into())
}
pub fn timecode(text: impl Into<SharedString>, size: f32) -> Div {
    type_timecode(div(), size).child(text.into())
}

// ---------- Icons ----------
pub fn icon(name: &str, size: f32, color: Hsla) -> gpui::Svg {
    svg().path(SharedString::from(format!("rayna/icons/{name}.svg"))).size(px(size)).flex_none().text_color(color)
}

// ---------- Layout pieces ----------
pub fn panel() -> Div {
    div().flex().flex_col().min_h_0().min_w_0().overflow_hidden().rounded(px(15.)).bg(surface1())
}
pub fn panel_header() -> Div {
    div().flex().items_center().gap_2().min_h(px(48.)).border_b_1().border_color(hairline_soft()).px_4().py_2()
}
pub fn section_label(title: &str, action: Option<&str>) -> Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap_2()
        .child(label(title.to_string()))
        .when_some(action, |d, a| d.child(type_micro(div()).text_color(ink_muted()).child(a.to_string())))
}
/// A titled block in the inspector, separated by a hairline.
pub fn group(title: Option<&str>, action: Option<String>, last: bool) -> Div {
    div()
        .flex()
        .flex_col()
        .gap_3()
        .py_4()
        .when(!last, |d| d.border_b_1().border_color(hairline_soft()))
        .when_some(title, |d, t| d.child(section_label(t, action.as_deref())))
}
/// Label · control · value row.
pub fn field_row(label_text: &str, control: impl IntoElement, value: Option<String>) -> Div {
    div()
        .flex()
        .items_center()
        .gap_3()
        .child(type_micro(div()).w(px(72.)).flex_none().text_color(ink_muted()).child(label_text.to_string()))
        .child(div().flex_1().min_w_0().child(control))
        .when_some(value, |d, v| d.child(type_timecode(div(), 11.).w(px(48.)).flex_none().text_right().text_color(ink()).child(v)))
}
pub fn kbd(text: &str) -> Div {
    type_timecode(div(), 11.).flex_none().rounded(px(4.)).bg(surface3()).px(px(6.)).py(px(2.)).text_color(ink()).child(text.to_string())
}

// ---------- Buttons ----------
#[derive(Clone, Copy, PartialEq)]
pub enum Variant {
    Primary,
    Secondary,
    Ghost,
    Destructive,
    Outline,
}

/// A pill button. `size`: height in px.
pub fn button(id: impl Into<ElementId>, variant: Variant, height: f32) -> Stateful<Div> {
    let (bg, fg, hover) = match variant {
        Variant::Primary => (ink(), black(), rgb(0xe6e6e6).into()),
        Variant::Secondary => (surface2(), ink(), surface3()),
        Variant::Ghost => (alpha(ink(), 0.0), ink_muted(), surface2()),
        Variant::Destructive => (alpha(coral(), 0.15), coral(), alpha(coral(), 0.25)),
        Variant::Outline => (alpha(ink(), 0.0), ink_muted(), surface2()),
    };
    div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .gap(px(6.))
        .h(px(height))
        .px(px(if height >= 36.0 { 16. } else { 12. }))
        .rounded_full()
        .bg(bg)
        .text_color(fg)
        .text_size(px(if height >= 36.0 { 14. } else { 13. }))
        .font_weight(FontWeight::MEDIUM)
        .cursor_pointer()
        .when(variant == Variant::Outline, |d| d.border_1().border_color(hairline()))
        .hover(move |s| s.bg(hover).text_color(if variant == Variant::Ghost || variant == Variant::Outline { ink() } else { fg }))
}

/// A round icon button with a tooltip (and an optional shortcut shown in it).
/// Three agent marks overlapping like an avatar stack (Claude, Codex, Gemini). `ring` is the
/// colour behind them, so each disc reads as cut out of the one before.
pub fn agent_stack(ring: Hsla) -> Div {
    let disc = |name: &str, bg: Hsla, fg: Hsla, first: bool| {
        div()
            .size(px(22.))
            .flex_none()
            .rounded_full()
            .border_2()
            .border_color(ring)
            .bg(bg)
            .flex()
            .items_center()
            .justify_center()
            .when(!first, |d| d.ml(px(-8.)))
            .child(icon(name, 11., fg))
    };
    div()
        .flex()
        .items_center()
        .child(disc("brand-claude", rgb(0xd97757).into(), rgb(0xffffff).into(), true))
        .child(disc("brand-openai", rgb(0xffffff).into(), rgb(0x0d0d0d).into(), false))
        .child(disc("brand-googlegemini", rgb(0x3d7bf7).into(), rgb(0xffffff).into(), false))
}

pub fn icon_button(
    id: impl Into<ElementId>,
    icon_name: &str,
    tip: &str,
    shortcut: Option<&str>,
    size: f32,
    variant: Variant,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let tip = match shortcut {
        Some(s) => format!("{tip}  {s}"),
        None => tip.to_string(),
    };
    let fg = if variant == Variant::Ghost { ink_muted() } else { ink() };
    button(id, variant, size)
        .w(px(size))
        .px_0()
        .child(icon(icon_name, (size * 0.45).max(14.0).min(18.0), fg))
        .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
        .on_click(on_click)
}

/// A segmented control: one option active.
pub fn segmented<T: Clone + PartialEq + 'static>(
    id: &str,
    options: &[(T, &str)],
    value: &T,
    disabled: impl Fn(&T) -> bool,
    on_change: impl Fn(T, &mut Window, &mut App) + 'static,
) -> Div {
    let on_change = std::rc::Rc::new(on_change);
    // Each option is its own rounded pill inside the frame: GPUI clips overflow to a rectangle,
    // so a fill on a square segment would poke out of the frame's rounded corners.
    div().flex().w_full().h(px(30.)).p(px(2.)).gap(px(2.)).rounded(px(9.)).border_1().border_color(hairline()).children(options.iter().enumerate().map(|(i, (v, l))| {
        let active = v == value;
        let off = disabled(v);
        let cb = on_change.clone();
        let v2 = v.clone();
        div()
            .id(SharedString::from(format!("{id}-{i}")))
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .px_1()
            .rounded(px(6.))
            .bg(if active { surface3() } else { alpha(ink(), 0.0) })
            .text_color(if active { ink() } else { ink_muted() })
            .text_size(px(12.))
            .when(off, |d| d.opacity(0.4))
            .when(!off, |d| d.cursor_pointer().hover(|s| s.bg(surface2()).text_color(ink())).on_click(move |_, w, cx| cb(v2.clone(), w, cx)))
            .child(div().truncate().child(l.to_string()))
    }))
}

/// `true` when `s` looks like a `#rrggbb` hex colour.
fn looks_hex(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s[1..].chars().all(|c| c.is_ascii_hexdigit())
}

/// Settings key: colours chosen with the custom picker, offered in every colour row.
const RECENT_KEY: &str = "custom_colors";

/// The colours the user picked with the custom picker (newest first), like the browser
/// version's recently used colours. Losing them is harmless, as with all prefs.
pub fn recent_colors() -> Vec<String> {
    crate::ui::prefs::get(RECENT_KEY)
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect()
}

/// Remember a custom colour so it shows up as a swatch in every colour row.
pub fn push_recent_color(c: &str) {
    let c = c.to_ascii_lowercase();
    if !looks_hex(&c) {
        return;
    }
    let mut all: Vec<String> = recent_colors().into_iter().filter(|x| *x != c).collect();
    all.insert(0, c);
    all.truncate(6);
    crate::ui::prefs::set(RECENT_KEY, serde_json::Value::Array(all.into_iter().map(serde_json::Value::String).collect()));
}

/// Round colour swatches with a selected ring: the presets, then recently picked custom colours,
/// then `custom` — a pick-anything element, when the caller has one.
pub fn swatches(id: &str, colors: &[&str], value: &str, size: f32, custom: Option<AnyElement>, on_pick: impl Fn(String, &mut Window, &mut App) + 'static) -> Div {
    let on_pick = Rc::new(on_pick);
    // The colour in use is always drawn (so its ring shows), followed by remembered custom colours.
    let matched = colors.iter().any(|c| c.eq_ignore_ascii_case(value));
    let mut extra: Vec<String> = if !matched && looks_hex(value) { vec![value.to_string()] } else { vec![] };
    for c in recent_colors() {
        if extra.len() >= 6 {
            break;
        }
        if extra.iter().any(|x| x.eq_ignore_ascii_case(&c)) || colors.iter().any(|p| p.eq_ignore_ascii_case(&c)) {
            continue;
        }
        extra.push(c);
    }
    let all = colors.iter().map(|c| c.to_string()).chain(extra).collect::<Vec<_>>();
    div().flex().flex_wrap().items_center().gap(px(6.))
        .children(all.iter().enumerate().map(|(i, c)| {
            let sel = c.eq_ignore_ascii_case(value);
            let cb = on_pick.clone();
            let col = c.to_string();
            div()
                .id(SharedString::from(format!("{id}-{i}")))
                .size(px(size + 6.))
                .rounded_full()
                .flex()
                .items_center()
                .justify_center()
                .border_2()
                .border_color(if sel { signal() } else { alpha(ink(), 0.0) })
                .cursor_pointer()
                .child(div().size(px(size)).rounded_full().bg(hex(c)).border_1().border_color(white_a(0.15)))
                .on_click(move |_, w, cx| cb(col.clone(), w, cx))
        }))
        .when_some(custom, |d, c| d.child(c))
}

/// A custom-colour button (palette, HSL sliders and a hex field) that hands its colour to
/// `on_set`, remembers it as a recent, and stays in sync when the row's colour changes from
/// somewhere else (undo, a swatch, the AI agent). `pickers` caches a picker per row key.
pub fn color_picker<V: 'static>(
    pickers: &mut HashMap<String, (Entity<ColorPickerState>, gpui::Subscription)>,
    key: &str,
    cur: &str,
    on_set: Rc<dyn Fn(&mut V, String)>,
    window: &mut Window,
    cx: &mut Context<V>,
) -> AnyElement {
    if !pickers.contains_key(key) {
        let st = cx.new(|cx| ColorPickerState::new(window, cx).default_value(hex(cur)));
        let sub = cx.subscribe_in(&st, window, move |this: &mut V, _, ev: &ColorPickerEvent, _, _cx| {
            if let ColorPickerEvent::Change(Some(c)) = ev {
                let col = c.to_hex().to_ascii_lowercase();
                push_recent_color(&col);
                (on_set)(this, col);
            }
        });
        pickers.insert(key.to_string(), (st, sub));
    }
    let (st, _) = pickers.get(key).unwrap();
    // Follow external changes to the row's colour (but never while it shows the same colour,
    // so an open picker isn't reset mid-edit).
    let same = st.read(cx).value().map(|v| v.to_hex().to_ascii_lowercase()) == Some(cur.to_ascii_lowercase());
    if !same {
        let target = hex(cur);
        let st = st.clone();
        st.update(cx, |st, cx| st.set_value(target, window, cx));
    }
    ColorPicker::new(&pickers[key].0).small().into_any_element()
}

/// Key–value table (e.g. Start / End / Duration).
pub fn kv(rows: Vec<(String, String)>) -> Div {
    div().flex().flex_col().gap(px(6.)).children(rows.into_iter().map(|(k, v)| {
        div()
            .flex()
            .justify_between()
            .gap_3()
            .child(type_caption(div()).font_weight(FontWeight::NORMAL).text_color(ink_muted()).child(k))
            .child(type_timecode(div(), 12.).child(v))
    }))
}

pub fn spacer() -> Div {
    div().flex_1()
}


pub fn black_overlay(a: f32) -> Hsla {
    Hsla { h: 0.0, s: 0.0, l: 0.0, a }
}

/// Map the design tokens onto gpui-component's theme (inputs, menus, tooltips, toasts).
pub fn install(cx: &mut App) {
    use gpui_component::theme::{Theme, ThemeMode};
    Theme::change(ThemeMode::Dark, None, cx);
    let t = Theme::global_mut(cx);
    t.font_family = SANS.into();
    t.font_size = px(14.);
    t.mono_font_family = MONO.into();
    t.radius = px(10.);
    t.radius_lg = px(15.);
    t.notification.placement = gpui::Anchor::BottomCenter;
    let c = &mut t.colors;
    c.background = canvas_bg();
    c.foreground = ink();
    c.border = hairline();
    c.input = hairline();
    c.ring = signal();
    c.caret = signal();
    c.selection = alpha(signal(), 0.45);
    c.primary = ink();
    c.primary_hover = rgb(0xe6e6e6).into();
    c.primary_active = rgb(0xd4d4d4).into();
    c.primary_foreground = black();
    c.secondary = surface2();
    c.secondary_hover = surface3();
    c.secondary_active = surface3();
    c.secondary_foreground = ink();
    c.muted = surface3();
    c.muted_foreground = ink_muted();
    c.accent = surface3();
    c.accent_foreground = ink();
    c.popover = surface2();
    c.popover_foreground = ink();
    c.danger = coral();
    c.success = success();
    c.list_active = surface3();
    c.list_hover = surface3();
    c.overlay = black_overlay(0.6);
    c.scrollbar = alpha(ink(), 0.0);
    c.scrollbar_thumb = hairline();
    c.scrollbar_thumb_hover = surface3();
}

/// Room the macOS traffic lights take at the left of a header.
pub const TRAFFIC_LIGHTS_W: f32 = 88.0;

/// An empty layer behind a header that moves the window like a title bar (drag to move,
/// double-click to zoom). Headers put their controls on top of it.
pub fn titlebar_drag(id: &'static str) -> Stateful<Div> {
    div().id(id).absolute().top_0().left_0().size_full().on_mouse_down(gpui::MouseButton::Left, |ev: &gpui::MouseDownEvent, window, _| {
        if ev.click_count >= 2 {
            window.titlebar_double_click();
        } else {
            window.start_window_move();
        }
    })
}
