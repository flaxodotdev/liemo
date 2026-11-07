use super::{Editor, SaveState};
use crate::model::time::tc;
use crate::storage::relative_time;
use crate::ui::theme::*;
use gpui::prelude::*;
use gpui::{div, px, Context, FontWeight, Window};
use gpui_component::input::Input;

impl Editor {
    pub(super) fn render_top_bar(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let end = self.store.end();
        let badge = |content: gpui::Div| type_caption(div()).flex().items_center().gap_1().h(px(28.)).rounded_full().bg(surface1()).px_3().text_color(ink_muted()).child(content);
        div()
            .relative()
            .h(px(56.))
            .flex_none()
            .flex()
            .items_center()
            .gap_4()
            .min_w_0()
            .pl(px(TRAFFIC_LIGHTS_W - 16.))
            .child(titlebar_drag("editor-titlebar"))
            .child(
                div()
                    .id("home")
                    .flex()
                    .items_center()
                    .gap_2()
                    .cursor_pointer()
                    .rounded(px(10.))
                    .child(icon("chevron-left", 16., ink_muted()))
                    .child(icon("liemo-mark", 22., ink()))
                    .child(div().font_family(DISPLAY).text_size(px(22.)).font_weight(FontWeight::SEMIBOLD).child("Liemo"))
                    .tooltip(|w, cx| gpui_component::tooltip::Tooltip::new("All projects").build(w, cx))
                    .on_click(cx.listener(|this, _, window, cx| this.go_home(window, cx))),
            )
            .child(div().relative().w(px(224.)).child(self.track_bounds("input:name")).child(Input::new(&self.name_input).appearance(false).aria_label("Project name")))
            .child(badge(div().flex().gap_1().child(format!("{} · {} ·", self.project.format_label(), self.project.aspect_key())).child(timecode(format!("{} fps", self.project.fps), 12.).text_color(ink()))))
            .child(badge(div().flex().gap_1().child("Length").child(timecode(tc(end), 12.).text_color(ink()))))
            .child(self.render_save_indicator(cx))
            .child(spacer())
            .child(icon_button("undo", "undo-2", "Undo", Some("⌘Z"), 32., Variant::Secondary, cx.listener(|this, _, _, cx| {
                this.store.undo();
                cx.notify();
            })).bg(surface1()).when(!self.store.can_undo(), |d| d.opacity(0.4)))
            .child(icon_button("redo", "redo-2", "Redo", Some("⇧⌘Z"), 32., Variant::Secondary, cx.listener(|this, _, _, cx| {
                this.store.redo();
                cx.notify();
            })).bg(surface1()).when(!self.store.can_redo(), |d| d.opacity(0.4)))
            .child({
                // Panel toggles, as in VS Code's title bar.
                use super::layout::Edge;
                let t = |id: &'static str, ic: &'static str, tip: &'static str, key: &'static str, on: bool, e: Edge, cx: &mut Context<Self>| {
                    icon_button(id, ic, tip, Some(key), 32., Variant::Ghost, cx.listener(move |this, _, _, cx| this.toggle_panel(e, cx)))
                        .text_color(if on { ink() } else { ink_muted() })
                        .when(on, |d| d.bg(surface1()))
                };
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(t("tog-lib", "panel-left", "Library", "⌘B", self.layout.lib, Edge::Library, cx))
                    .child(t("tog-tl", "panel-bottom", "Timeline", "⌘J", self.layout.tl, Edge::Timeline, cx))
                    .child(t("tog-insp", "panel-right", "Inspector", "⌥⌘B", self.layout.insp, Edge::Inspector, cx))
            })
            .child(
                button("ai", Variant::Secondary, 40.)
                    .pl(px(6.))
                    .bg(if self.agent.open { surface3() } else { surface1() })
                    .child(agent_stack(if self.agent.open { surface3() } else { surface1() }))
                    .child("AI")
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_agent(cx))),
            )
            .child(
                button("import", Variant::Secondary, 40.)
                    .bg(surface1())
                    .child(icon("upload", 16., ink()))
                    .child("Import")
                    .on_click(cx.listener(|this, _, window, cx| this.pick_files(None, window, cx))),
            )
            .child(
                button("export", Variant::Primary, 40.)
                    .child(icon("download", 16., black()))
                    .child("Export")
                    .when(self.store.clips.is_empty(), |d| d.opacity(0.4))
                    .on_click(cx.listener(|this, _, window, cx| {
                        if !this.store.clips.is_empty() {
                            this.open_export(window, cx);
                        }
                    })),
            )
    }

    fn render_save_indicator(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let row = type_caption(div()).flex().items_center().gap(px(6.)).text_color(ink_muted());
        match self.save_state {
            SaveState::Error => row
                .id("retry-save")
                .cursor_pointer()
                .rounded_full()
                .px_2()
                .py_1()
                .text_color(coral())
                .hover(|s| s.bg(surface1()))
                .child(icon("cloud-alert", 14., coral()))
                .child("Couldn't save. Retry")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.save_state = SaveState::Saving;
                    this.save_now(cx);
                }))
                .into_any_element(),
            SaveState::Saving => row.child(icon("loader-2", 14., ink_muted())).child("Saving").into_any_element(),
            SaveState::Saved => row.child(icon("check", 14., success())).child(format!("Saved {}", relative_time(self.saved_at))).into_any_element(),
        }
    }
}
