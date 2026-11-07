// The editor's panel layout, VS Code style: the library, inspector and AI sidebar resize in
// width and the timeline in height by dragging the gaps between them, and each can be closed.
// Sizes and visibility are remembered.

use super::{Drag, Editor};
use crate::ui::prefs;
use crate::ui::theme::*;
use gpui::prelude::*;
use gpui::{div, px, Context, CursorStyle, Div, MouseButton, MouseDownEvent, Point, Pixels, SharedString};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Library,
    Inspector,
    Agent,
    Timeline,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Layout {
    pub lib_w: f32,
    pub insp_w: f32,
    pub agent_w: f32,
    pub tl_h: f32,
    pub lib: bool,
    pub insp: bool,
    pub tl: bool,
}

impl Default for Layout {
    fn default() -> Self {
        Layout { lib_w: 330., insp_w: 300., agent_w: 380., tl_h: 320., lib: true, insp: true, tl: true }
    }
}

impl Layout {
    pub fn load() -> Self {
        prefs::get("layout").and_then(|v| serde_json::from_value::<Layout>(v).ok()).unwrap_or_default().clamped()
    }
    pub fn save(&self) {
        if let Ok(v) = serde_json::to_value(self) {
            prefs::set("layout", v);
        }
    }
    fn clamped(mut self) -> Self {
        self.lib_w = self.lib_w.clamp(240., 560.);
        self.insp_w = self.insp_w.clamp(240., 520.);
        self.agent_w = self.agent_w.clamp(300., 720.);
        self.tl_h = self.tl_h.clamp(140., 720.);
        self
    }
    pub fn size(&self, e: Edge) -> f32 {
        match e {
            Edge::Library => self.lib_w,
            Edge::Inspector => self.insp_w,
            Edge::Agent => self.agent_w,
            Edge::Timeline => self.tl_h,
        }
    }
    /// The new size for a drag of `delta` px (along the edge's axis) from `size0`.
    pub fn resize(&mut self, e: Edge, size0: f32, delta: f32) {
        match e {
            // The library's handle is on its right; the others' on their left / top.
            Edge::Library => self.lib_w = size0 + delta,
            Edge::Inspector => self.insp_w = size0 - delta,
            Edge::Agent => self.agent_w = size0 - delta,
            Edge::Timeline => self.tl_h = size0 - delta,
        }
        *self = self.clamped();
    }
}

impl Editor {
    /// The gap between two panels, which drags to resize `edge`. Horizontal for the timeline.
    pub(super) fn splitter(&self, edge: Edge, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let rows = edge == Edge::Timeline;
        let id = SharedString::from(format!("split-{edge:?}"));
        let line = if rows { div().h(px(2.)).w_full() } else { div().w(px(2.)).h_full() };
        div()
            .id(id.clone())
            .group(id.clone())
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .when(rows, |d| d.h(px(8.)).w_full().cursor(CursorStyle::ResizeUpDown))
            .when(!rows, |d| d.w(px(8.)).h_full().cursor(CursorStyle::ResizeLeftRight))
            .child(line.rounded_full().bg(alpha(signal(), 0.0)).group_hover(id.clone(), |s| s.bg(alpha(signal(), 0.7))))
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                let at = if rows { f32::from(ev.position.y) } else { f32::from(ev.position.x) };
                this.begin_drag(Drag::Resize { edge, start: at, size0: this.layout.size(edge) }, cx);
            }))
    }

    pub(super) fn resize_drag(&mut self, edge: Edge, start: f32, size0: f32, pos: Point<Pixels>) {
        let at = if edge == Edge::Timeline { f32::from(pos.y) } else { f32::from(pos.x) };
        self.layout.resize(edge, size0, at - start);
    }

    pub(super) fn toggle_panel(&mut self, e: Edge, cx: &mut Context<Self>) {
        match e {
            Edge::Library => self.layout.lib = !self.layout.lib,
            Edge::Inspector => self.layout.insp = !self.layout.insp,
            Edge::Timeline => self.layout.tl = !self.layout.tl,
            Edge::Agent => return self.toggle_agent(cx),
        }
        self.layout.save();
        cx.notify();
    }
}
