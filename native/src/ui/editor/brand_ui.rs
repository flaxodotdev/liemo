// The brand kit dialog and what uses it (applying the brand to text, adding the logo), saving and
// placing components, and connecting the user's own Pexels and ElevenLabs accounts.

use super::Editor;
use crate::model::brand::{self, norm_hex, Brand};
use crate::model::types::AssetType;
use crate::integrations::{self, Service};
use crate::ui::components;
use crate::ui::theme::*;
use gpui::prelude::*;
use gpui::{div, px, relative, AppContext, Context, Entity, MouseButton, Window};
use gpui::Focusable;
use gpui_component::input::{Input, InputState};
use gpui_component::Sizable;
use std::path::PathBuf;

pub struct BrandEdit {
    name: Entity<InputState>,
    colors: Entity<InputState>,
    font: Entity<InputState>,
    logo: Option<PathBuf>,
}

impl BrandEdit {
    pub fn focused(&self, window: &Window, cx: &gpui::App) -> bool {
        [&self.name, &self.colors, &self.font].iter().any(|s| s.read(cx).focus_handle(cx).contains_focused(window, cx))
    }
}

impl Editor {
    pub fn open_brand(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let b = brand::load();
        self.brand_edit = Some(self.brand_fields(&b, window, cx));
        cx.notify();
    }

    fn brand_fields(&self, b: &Brand, window: &mut Window, cx: &mut Context<Self>) -> BrandEdit {
        let field = |v: String, ph: &'static str, window: &mut Window, cx: &mut Context<Self>| cx.new(|cx| InputState::new(window, cx).placeholder(ph).default_value(v));
        BrandEdit {
            name: field(b.name.clone(), "Brand name", window, cx),
            colors: field(b.colors.join(", "), "#ff5500, #111111, #f5f5f4", window, cx),
            font: field(b.font.clone(), "Inter, Mona Sans or any installed font", window, cx),
            logo: b.logo.clone(),
        }
    }

    fn brand_from_fields(&self, cx: &Context<Self>) -> Option<Brand> {
        let e = self.brand_edit.as_ref()?;
        let colors = e.colors.read(cx).value().split([',', ' ', ';']).filter_map(norm_hex).collect();
        Some(Brand { name: e.name.read(cx).value().trim().to_string(), colors, font: e.font.read(cx).value().trim().to_string(), logo: e.logo.clone() })
    }

    fn save_brand(&mut self, cx: &mut Context<Self>) {
        let Some(b) = self.brand_from_fields(cx) else { return };
        match brand::save(&b) {
            Ok(()) => self.store.toast(if b.is_empty() { "Brand kit cleared".to_string() } else { "Brand kit saved. Its colours lead the swatches, and the AI chat knows it.".to_string() }),
            Err(e) => self.store.toast(format!("Couldn't save the brand kit: {e}")),
        }
        self.brand_edit = None;
        cx.notify();
    }

    /// Fill the dialog from a DESIGN.md (or any text file with colours and a font in it).
    fn pick_design_md(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(gpui::PathPromptOptions { files: true, directories: false, multiple: false, prompt: Some("Read brand".into()) });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await {
                let Some(p) = paths.into_iter().next() else { return };
                let text = std::fs::read(&p).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
                let _ = this.update_in(cx, |this, window, cx| {
                    let mut b = brand::from_text(&text);
                    let logo = this.brand_edit.as_ref().and_then(|e| e.logo.clone());
                    b.logo = logo;
                    if b.colors.is_empty() && b.font.is_empty() {
                        this.store.toast("No colours or font found in that file");
                    }
                    this.brand_edit = Some(this.brand_fields(&b, window, cx));
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn pick_logo(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(gpui::PathPromptOptions { files: true, directories: false, multiple: false, prompt: Some("Choose logo".into()) });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await {
                let _ = this.update(cx, |this, cx| {
                    let p = paths.into_iter().next();
                    if p.as_ref().is_some_and(|p| crate::media::import::kind_of(p) != Some(AssetType::Image)) {
                        this.store.toast("The logo needs to be an image file: PNG, JPEG or WebP");
                        return;
                    }
                    if let Some(e) = this.brand_edit.as_mut() {
                        e.logo = p;
                    }
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// Dress the selected text clips in the brand's typeface and main colour.
    pub fn apply_brand(&mut self) {
        let b = brand::load();
        if b.colors.is_empty() && b.font.is_empty() {
            self.store.toast("Set up a brand kit first (⌘K, Brand kit)");
            return;
        }
        let ids: Vec<String> = self.store.selection().into_iter().filter(|id| self.store.clip(id).is_some_and(|c| c.kind == AssetType::Text)).collect();
        if ids.is_empty() {
            self.store.toast("Select some text first");
            return;
        }
        self.store.commit();
        for id in ids {
            let b = b.clone();
            self.store.update_text(&id, move |t| {
                if !b.font.is_empty() {
                    t.font = b.font.clone();
                }
                if let Some(c) = b.colors.first() {
                    t.color = c.clone();
                }
            });
        }
    }

    /// The brand's logo on Overlays at the playhead.
    pub fn add_logo(&mut self, cx: &mut Context<Self>) {
        match brand::load().logo.filter(|p| p.exists()) {
            Some(p) => {
                let t = self.store.time;
                self.import(vec![p], Some((t, Some("V2".into()))), cx);
            }
            None => self.store.toast("Choose a logo in the brand kit first (⌘K, Brand kit)"),
        }
    }

    /// Save the selected clips as a component, named after the first.
    pub fn save_component(&mut self, cx: &mut Context<Self>) {
        let clips: Vec<_> = self.store.selection().iter().filter_map(|id| self.store.clip(id).cloned()).collect();
        if clips.is_empty() {
            self.store.toast("Select the clips to save as a component");
            return;
        }
        let name = if clips.len() == 1 { clips[0].name.clone() } else { format!("{} + {}", clips[0].name, clips.len() - 1) };
        match components::save(&name, clips) {
            Ok(c) => self.store.toast(format!("Saved \"{}\" as a component. Add it in any project from ⌘K.", c.name)),
            Err(e) => self.store.toast(format!("Couldn't save the component: {e}")),
        }
        cx.notify();
    }

    pub fn insert_component(&mut self, id: &str) {
        match components::get(id) {
            Some(c) => self.store.paste(Some(c.clips), None),
            None => self.store.toast("That component is gone"),
        }
    }

    pub(super) fn render_brand(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let e = self.brand_edit.as_ref().unwrap();
        let preview = self.brand_from_fields(cx).unwrap_or_default();
        let logo = e.logo.as_ref().map(|p| crate::media::import::file_name(p)).unwrap_or_else(|| "None".into());
        let has_logo = e.logo.is_some();
        let input = |s: &Entity<InputState>, key: &str| div().relative().flex_1().child(self.track_bounds(format!("input:{key}"))).child(Input::new(s).small());
        let mut swatches = div().flex().gap_1().h(px(18.));
        for c in &preview.colors {
            swatches = swatches.child(div().size(px(18.)).rounded_full().border_1().border_color(hairline()).bg(hex(c)));
        }
        let body = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(field_row("Name", input(&e.name, "brand-name"), None))
            .child(field_row("Colours", input(&e.colors, "brand-colors"), None))
            .child(div().pl(px(84.)).child(swatches))
            .child(field_row("Typeface", input(&e.font, "brand-font"), None))
            .child(field_row(
                "Logo",
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().flex_1().min_w_0().truncate().text_size(px(13.)).text_color(ink_muted()).child(logo))
                    .child(button("brand-logo", Variant::Secondary, 30.).child("Choose…").on_click(cx.listener(|this, _, window, cx| this.pick_logo(window, cx))))
                    .when(has_logo, |d| {
                        d.child(button("brand-logo-x", Variant::Ghost, 30.).child("Remove").on_click(cx.listener(|this, _, _, cx| {
                            if let Some(e) = this.brand_edit.as_mut() {
                                e.logo = None;
                            }
                            cx.notify();
                        })))
                    }),
                None,
            ));
        let footer = div()
            .flex()
            .items_center()
            .gap_2()
            .child(button("brand-md", Variant::Ghost, 36.).child("Read a DESIGN.md…").on_click(cx.listener(|this, _, window, cx| this.pick_design_md(window, cx))))
            .child(div().flex_1())
            .child(button("brand-cancel", Variant::Secondary, 36.).child("Cancel").on_click(cx.listener(|this, _, _, cx| {
                this.brand_edit = None;
                cx.notify();
            })))
            .child(button("brand-save", Variant::Primary, 36.).child("Save").on_click(cx.listener(|this, _, _, cx| this.save_brand(cx))));
        div()
            .id("brand-backdrop")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .bg(black_overlay(0.6))
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                this.brand_edit = None;
                cx.notify();
            }))
            .child(
                div()
                    .id("brand-dialog")
                    .w(px(480.))
                    .max_h(relative(0.92))
                    .overflow_y_scroll()
                    .rounded(px(20.))
                    .bg(surface1())
                    .border_1()
                    .border_color(hairline())
                    .shadow_lg()
                    .p_6()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(type_headline(div()).child("Brand kit"))
                            .child(div().text_size(px(13.)).text_color(ink_muted()).child("Set it once for every project: its colours lead the swatches, Apply brand dresses text in it, and the AI chat keeps to it.")),
                    )
                    .child(body)
                    .child(footer),
            )
    }
}

impl Editor {
    pub fn open_connect(&mut self, s: Service, window: &mut Window, cx: &mut Context<Self>) {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Paste your API key").masked(true));
        input.update(cx, |i, cx| i.focus(window, cx));
        self.connect_edit = Some((s, input));
        cx.notify();
    }

    pub fn disconnect(&mut self, s: Service) {
        match integrations::set_key(s, None) {
            Ok(()) => self.store.toast(format!("Disconnected {}", s.name())),
            Err(e) => self.store.toast(format!("Couldn't remove the {} key: {e}", s.name())),
        }
    }

    fn save_connect(&mut self, cx: &mut Context<Self>) {
        let Some((s, input)) = self.connect_edit.take() else { return };
        let k = input.read(cx).value().trim().to_string();
        if k.is_empty() {
            self.store.toast("Paste a key first");
            self.connect_edit = Some((s, input));
            return;
        }
        let what = match s {
            Service::Pexels => "The AI chat can now search Pexels for stock photos and video.",
            Service::ElevenLabs => "The AI chat can now make sound effects and music; it asks you before each request.",
        };
        match integrations::set_key(s, Some(&k)) {
            Ok(()) => self.store.toast(format!("Connected {}. {what}", s.name())),
            Err(e) => self.store.toast(format!("Couldn't save the {} key: {e}", s.name())),
        }
        cx.notify();
    }

    pub(super) fn render_connect(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (s, input) = self.connect_edit.as_ref().unwrap();
        let s = *s;
        let blurb = match s {
            Service::Pexels => "Free stock photos and video for the AI chat to search, download and build moodboards from.",
            Service::ElevenLabs => "Sound effects and music made from a description. Each request uses your ElevenLabs credits, and Liemo asks you first.",
        };
        let url = s.key_url();
        div()
            .id("connect-backdrop")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .bg(black_overlay(0.6))
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                this.connect_edit = None;
                cx.notify();
            }))
            .child(
                div()
                    .id("connect-dialog")
                    .w(px(440.))
                    .rounded(px(20.))
                    .bg(surface1())
                    .border_1()
                    .border_color(hairline())
                    .shadow_lg()
                    .p_6()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(div().flex().flex_col().gap_1().child(type_headline(div()).child(format!("Connect {}", s.name()))).child(div().text_size(px(13.)).text_color(ink_muted()).child(blurb)))
                    .child(div().relative().child(self.track_bounds("input:connect")).child(Input::new(input)))
                    .child(div().text_size(px(12.)).text_color(ink_muted()).child(if cfg!(target_os = "macos") { "Kept in your Keychain. Nothing else leaves this Mac." } else { "Kept in Liemo's data folder, readable only by you." }))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(button("connect-get", Variant::Ghost, 36.).child("Get a key").on_click(move |_, _, cx| cx.open_url(url)))
                            .child(div().flex_1())
                            .child(button("connect-cancel", Variant::Secondary, 36.).child("Cancel").on_click(cx.listener(|this, _, _, cx| {
                                this.connect_edit = None;
                                cx.notify();
                            })))
                            .child(button("connect-save", Variant::Primary, 36.).child("Connect").on_click(cx.listener(|this, _, _, cx| this.save_connect(cx)))),
                    ),
            )
    }
}
