// The AI sidebar: a full-height chat on the right of the editor. It runs an AI command-line tool
// the user already has, which edits this project through Liemo's MCP server (see crate::agent);
// edits arrive live and undo like any other.

use super::inspector::{item, MenuEntry};
use super::Editor;
use crate::agent::{self, Event, Provider};
use crate::ui::prefs;
use crate::ui::theme::*;
use futures::StreamExt;
use gpui::prelude::*;
use gpui::{div, px, Animation, AnimationExt, AnyElement, Context, Entity, FontStyle, FontWeight, HighlightStyle, ScrollHandle, StyledText, Window};
use gpui_component::input::{Textarea, TextareaState};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

pub const SIDEBAR_W: f32 = 380.0;

pub enum Msg {
    User(String),
    Reply(String),
    /// A Liemo action: call id, tool, summary, and how it went (None while running).
    Tool(String, String, String, Option<bool>),
    Error(String),
}

pub struct AgentChat {
    pub open: bool,
    pub provider: Option<Provider>,
    pub model: String,
    pub input: Entity<TextareaState>,
    pub msgs: Vec<Msg>,
    pub run: Option<agent::Run>,
    /// Each tool's conversation, so follow-ups continue it.
    pub sessions: HashMap<Provider, String>,
    pub scroll: ScrollHandle,
    /// Action groups the user opened (by their first call id).
    pub expanded: HashSet<String>,
    /// The last reply is still streaming in (deltas append to it).
    streaming: bool,
}

impl AgentChat {
    pub fn new(window: &mut Window, cx: &mut Context<Editor>) -> Self {
        let input = cx.new(|cx| TextareaState::new(window, cx).auto_grow(1, 8).placeholder("Ask for an edit…"));
        AgentChat {
            open: prefs::get_str("agent_open").as_deref() == Some("1") || std::env::var("RAYNA_TAB").as_deref() == Ok("agent"),
            provider: prefs::get_str("agent_provider").and_then(|p| Provider::from_id(&p)),
            model: prefs::get_str("agent_model").unwrap_or_default(),
            input,
            msgs: vec![],
            run: None,
            sessions: HashMap::new(),
            scroll: ScrollHandle::new(),
            expanded: HashSet::new(),
            streaming: false,
        }
    }
    /// The chosen tool if it's installed, else the first installed one.
    pub fn current(&self) -> Option<Provider> {
        let installed = agent::installed();
        self.provider.filter(|p| installed.iter().any(|(q, _)| q == p)).or_else(|| installed.first().map(|(p, _)| *p))
    }
}

const SUGGESTIONS: [&str; 4] = [
    "Add a title that says \"Summer 2026\" in the first three seconds",
    "Give the footage a warm film look",
    "Add a soft dissolve between every clip",
    "Add music and fade it out at the end",
];

/// Tools that only look, not change.
const LOOKS: [&str; 7] = ["render_frame", "get_project", "list_projects", "list_library", "list_effects", "list_transitions", "list_formats"];

// ---------- Markdown (the subset these tools write) ----------
/// Inline **bold**, *italic* and `code`, as styled text.
fn inline(src: &str) -> StyledText {
    let mut text = String::new();
    let mut runs: Vec<(std::ops::Range<usize>, HighlightStyle)> = vec![];
    let mut rest = src;
    loop {
        // The earliest marker that has a closing partner later on the line.
        let next = [("**", 0u8), ("`", 1), ("*", 2), ("_", 2)]
            .iter()
            .filter_map(|(m, k)| rest.find(m).map(|i| (i, *m, *k)))
            .filter(|(i, m, _)| rest[i + m.len()..].contains(m))
            .min_by_key(|(i, m, _)| (*i, std::cmp::Reverse(m.len())));
        let Some((i, m, kind)) = next else {
            text.push_str(rest);
            break;
        };
        text.push_str(&rest[..i]);
        let after = &rest[i + m.len()..];
        let end = after.find(m).unwrap_or(after.len());
        let inner = &after[..end];
        let start = text.len();
        text.push_str(inner);
        if !inner.is_empty() {
            let style = match kind {
                0 => HighlightStyle { font_weight: Some(FontWeight::SEMIBOLD), ..Default::default() },
                1 => HighlightStyle { background_color: Some(alpha(ink(), 0.1)), ..Default::default() },
                _ => HighlightStyle { font_style: Some(FontStyle::Italic), ..Default::default() },
            };
            runs.push((start..text.len(), style));
        }
        rest = &after[(end + m.len()).min(after.len())..];
    }
    StyledText::new(text).with_highlights(runs)
}

fn code_block(lines: Vec<&str>) -> gpui::Div {
    div()
        .rounded(px(8.))
        .bg(canvas_bg())
        .border_1()
        .border_color(hairline_soft())
        .p_2()
        .font_family(MONO)
        .text_size(px(11.5))
        .line_height(px(17.))
        .children(lines.into_iter().map(|l| div().child(if l.is_empty() { " ".to_string() } else { l.to_string() })))
}

/// Paragraphs, headings, lists and fenced code blocks.
fn markdown(src: &str) -> AnyElement {
    let mut col = div().flex().flex_col().gap_2().text_size(px(13.)).line_height(px(20.));
    let mut para: Vec<String> = vec![];
    let mut code: Option<Vec<&str>> = None;
    fn flush(col: gpui::Div, para: &mut Vec<String>) -> gpui::Div {
        if para.is_empty() {
            return col;
        }
        let t = para.join(" ");
        para.clear();
        col.child(div().child(inline(&t)))
    }
    for line in src.lines() {
        if line.trim_start().starts_with("```") {
            match code.take() {
                Some(lines) => col = col.child(code_block(lines)),
                None => {
                    col = flush(col, &mut para);
                    code = Some(vec![]);
                }
            }
            continue;
        }
        if let Some(lines) = code.as_mut() {
            lines.push(line);
            continue;
        }
        let s = line.trim();
        let bullet = s.strip_prefix("- ").or_else(|| s.strip_prefix("* ")).or_else(|| s.strip_prefix("• "));
        let numbered = s.split_once(". ").filter(|(n, _)| !n.is_empty() && n.len() <= 3 && n.chars().all(|c| c.is_ascii_digit()));
        if s.is_empty() {
            col = flush(col, &mut para);
        } else if let Some(h) = s.strip_prefix("### ").or_else(|| s.strip_prefix("## ")).or_else(|| s.strip_prefix("# ")) {
            col = flush(col, &mut para);
            col = col.child(div().font_weight(FontWeight::SEMIBOLD).text_size(px(13.5)).child(inline(h)));
        } else if let Some(item) = bullet {
            col = flush(col, &mut para);
            col = col.child(div().flex().gap_2().child(div().flex_none().text_color(ink_muted()).child("•")).child(div().flex_1().min_w_0().child(inline(item))));
        } else if let Some((n, item)) = numbered {
            col = flush(col, &mut para);
            col = col.child(div().flex().gap_2().child(div().flex_none().min_w(px(16.)).text_color(ink_muted()).child(format!("{n}."))).child(div().flex_1().min_w_0().child(inline(item))));
        } else {
            para.push(s.to_string());
        }
    }
    if let Some(lines) = code {
        col = col.child(code_block(lines));
    }
    flush(col, &mut para).into_any_element()
}

impl Editor {
    pub(super) fn toggle_agent(&mut self, cx: &mut Context<Self>) {
        self.agent.open = !self.agent.open;
        prefs::set("agent_open", serde_json::Value::String(if self.agent.open { "1" } else { "0" }.into()));
        cx.notify();
    }

    pub(super) fn agent_send(&mut self, text: String, window: &mut Window, cx: &mut Context<Self>) {
        let text = text.trim().to_string();
        if text.is_empty() || self.agent.run.is_some() {
            return;
        }
        let Some(provider) = self.agent.current() else { return };
        self.agent.open = true;
        self.agent.input.update(cx, |s, cx| s.set_value("", window, cx));
        self.agent.msgs.push(Msg::User(text.clone()));
        self.agent.streaming = false;
        let (tx, mut rx) = futures::channel::mpsc::unbounded();
        // Only Claude Code offers model choices here; the others use their own default.
        let model = if provider.models().iter().any(|(m, _)| *m == self.agent.model) { self.agent.model.clone() } else { String::new() };
        let turn = agent::Turn {
            provider,
            model,
            prompt: text,
            session: self.agent.sessions.get(&provider).cloned(),
            project_id: self.project.id.clone(),
            project_name: self.store.name.clone(),
        };
        self.agent.run = agent::start(turn, move |e| {
            let _ = tx.unbounded_send(e);
        });
        cx.spawn(async move |this, cx| {
            while let Some(e) = rx.next().await {
                let done = matches!(e, Event::Done(_));
                let _ = this.update(cx, |ed, cx| ed.agent_event(provider, e, cx));
                if done {
                    break;
                }
            }
        })
        .detach();
        self.agent.scroll.scroll_to_bottom();
        cx.notify();
    }

    fn agent_event(&mut self, provider: Provider, e: Event, cx: &mut Context<Self>) {
        let a = &mut self.agent;
        match e {
            Event::Text(t) => {
                if !t.trim().is_empty() {
                    a.msgs.push(Msg::Reply(t));
                }
                a.streaming = false;
            }
            Event::Delta(t) => match (a.streaming, a.msgs.last_mut()) {
                (true, Some(Msg::Reply(r))) => r.push_str(&t),
                _ => {
                    if !t.is_empty() {
                        a.msgs.push(Msg::Reply(t));
                        a.streaming = true;
                    }
                }
            },
            Event::ToolStart(id, name, summary) => {
                a.streaming = false;
                a.msgs.push(Msg::Tool(id, name, summary, None));
            }
            Event::ToolDone(id, ok) => {
                for m in a.msgs.iter_mut().rev() {
                    if let Msg::Tool(i, _, _, state) = m {
                        if *i == id {
                            *state = Some(ok);
                            break;
                        }
                    }
                }
            }
            Event::Session(s) => {
                if !s.is_empty() {
                    a.sessions.insert(provider, s);
                }
            }
            Event::Done(err) => {
                a.run = None;
                a.streaming = false;
                // A tool still "running" when the turn ends didn't finish.
                for m in a.msgs.iter_mut() {
                    if let Msg::Tool(_, _, _, state @ None) = m {
                        *state = Some(false);
                    }
                }
                if let Some(e) = err {
                    a.msgs.push(Msg::Error(e));
                }
            }
        }
        self.agent.scroll.scroll_to_bottom();
        cx.notify();
    }

    /// One reply's actions, folded into a line ("2 changes · 1 check") that opens to the list.
    fn action_group(&self, tools: &[(String, String, String, Option<bool>)], cx: &mut Context<Self>) -> AnyElement {
        let key = tools[0].0.clone();
        let open = self.agent.expanded.contains(&key);
        let running = tools.iter().any(|t| t.3.is_none());
        let failed = tools.iter().filter(|t| t.3 == Some(false)).count();
        let looks = tools.iter().filter(|t| LOOKS.contains(&t.1.as_str())).count();
        let changes = tools.len() - looks;
        let title = if running {
            let last = tools.iter().rev().find(|t| t.3.is_none()).map(|t| t.1.replace('_', " ")).unwrap_or_default();
            format!("Working · {last}")
        } else {
            let mut parts = vec![];
            if changes > 0 {
                parts.push(format!("{changes} change{}", if changes == 1 { "" } else { "s" }));
            }
            if looks > 0 {
                parts.push(format!("{looks} check{}", if looks == 1 { "" } else { "s" }));
            }
            if failed > 0 {
                parts.push(format!("{failed} failed"));
            }
            parts.join(" · ")
        };
        let (ic, color) = if running {
            ("loader-2", ink_muted())
        } else if failed > 0 {
            ("alert-triangle", coral())
        } else {
            ("check", success())
        };
        let k2 = key.clone();
        div()
            .flex()
            .flex_col()
            .rounded(px(10.))
            .border_1()
            .border_color(hairline_soft())
            .child(
                div()
                    .id(gpui::SharedString::from(format!("grp-{key}")))
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py(px(7.))
                    .cursor_pointer()
                    .hover(|d| d.bg(surface2()))
                    .rounded(px(10.))
                    .text_size(px(12.))
                    .text_color(ink_muted())
                    .child(icon(ic, 13., color))
                    .child(div().flex_1().min_w_0().truncate().child(title))
                    .child(icon(if open { "chevron-up" } else { "chevron-down" }, 13., ink_muted()))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !this.agent.expanded.remove(&k2) {
                            this.agent.expanded.insert(k2.clone());
                        }
                        cx.notify();
                    })),
            )
            .when(open, |d| {
                d.child(div().flex().flex_col().gap(px(5.)).px_3().pb_2().pt_2().border_t_1().border_color(hairline_soft()).children(tools.iter().map(|(_, name, summary, state)| {
                    let (ic, color) = match state {
                        None => ("loader-2", ink_muted()),
                        Some(true) => ("check", success()),
                        Some(false) => ("x", coral()),
                    };
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .text_size(px(12.))
                        .text_color(ink_muted())
                        .child(icon(ic, 12., color))
                        .child(type_timecode(div(), 11.).flex_none().text_color(ink()).child(name.clone()))
                        .when(!summary.is_empty(), |d| d.child(div().min_w_0().truncate().child(summary.clone())))
                })))
            })
            .into_any_element()
    }

    /// The full-height AI sidebar.
    pub(super) fn render_agent(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let installed = agent::installed();
        let current = self.agent.current();
        let running = self.agent.run.is_some();
        let who = current.map(|p| p.name()).unwrap_or("AI");

        // ---- Header ----
        let header = panel_header()
            .child(agent_stack(surface1()))
            .child(type_caption(div()).flex_1().child("Liemo AI"))
            .child(icon_button("agent-new", "plus", "New chat", None, 28., Variant::Ghost, cx.listener(|this, _, _, cx| {
                if let Some(r) = this.agent.run.take() {
                    r.stop();
                }
                this.agent.msgs.clear();
                this.agent.sessions.clear();
                this.agent.expanded.clear();
                cx.notify();
            })))
            .child(icon_button("agent-close", "x", "Close", None, 28., Variant::Ghost, cx.listener(|this, _, _, cx| this.toggle_agent(cx))));

        // ---- Conversation ----
        let mut body = div().id("agent-scroll").flex_1().min_h_0().overflow_y_scroll().track_scroll(&self.agent.scroll).flex().flex_col().gap_4().px_4().py_4();
        if installed.is_empty() {
            body = body.child(
                div().flex().flex_col().gap_2().pt_6().child(type_headline(div()).child("No AI tool found")).child(micro(
                    "Liemo AI uses an AI command-line tool you already have, with its own login: Claude Code, Codex, Gemini CLI, OpenCode or Grok. Install one, sign in, then reopen Liemo.",
                )),
            );
        } else if self.agent.msgs.is_empty() {
            body = body
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .pt_4()
                        .pb_2()
                        .child(type_headline(div()).child("What should we change?"))
                        .child(micro(format!("Ask in plain words. {who} edits this project as you watch, with your own {who} login. Undo works as usual."))),
                )
                .children(SUGGESTIONS.iter().enumerate().map(|(i, s)| {
                    let s = s.to_string();
                    div()
                        .id(("agent-suggest", i))
                        .flex()
                        .items_center()
                        .gap_2()
                        .px_3()
                        .py(px(9.))
                        .rounded(px(10.))
                        .border_1()
                        .border_color(hairline_soft())
                        .hover(|d| d.bg(surface2()))
                        .cursor_pointer()
                        .text_size(px(12.5))
                        .text_color(ink_muted())
                        .child(div().flex_1().min_w_0().child(s.clone()))
                        .child(icon("arrow-up", 13., ink_muted()))
                        .on_click(cx.listener(move |this, _, window, cx| this.agent_send(s.clone(), window, cx)))
                }));
        }
        // Messages; consecutive actions fold into one group.
        let mut items: Vec<AnyElement> = vec![];
        let mut group: Vec<(String, String, String, Option<bool>)> = vec![];
        let n = self.agent.msgs.len();
        for i in 0..=n {
            if let Some(Msg::Tool(id, name, summary, state)) = self.agent.msgs.get(i) {
                group.push((id.clone(), name.clone(), summary.clone(), *state));
                continue;
            }
            if !group.is_empty() {
                items.push(self.action_group(&group, cx));
                group.clear();
            }
            match self.agent.msgs.get(i) {
                Some(Msg::User(t)) => items.push(
                    div()
                        .flex()
                        .justify_end()
                        .child(div().max_w(gpui::relative(0.85)).px_3().py_2().rounded(px(14.)).bg(surface3()).text_size(px(13.)).line_height(px(19.)).child(t.clone()))
                        .into_any_element(),
                ),
                Some(Msg::Reply(t)) => items.push(markdown(t)),
                Some(Msg::Error(t)) => items.push(
                    div()
                        .flex()
                        .gap_2()
                        .px_3()
                        .py_2()
                        .rounded(px(10.))
                        .bg(alpha(coral(), 0.1))
                        .text_size(px(12.5))
                        .text_color(coral())
                        .child(icon("alert-triangle", 14., coral()))
                        .child(div().flex_1().min_w_0().child(t.clone()))
                        .into_any_element(),
                ),
                _ => {}
            }
        }
        body = body.children(items);
        if running && !self.agent.streaming {
            let dot = |n: usize| {
                div().size(px(6.)).rounded_full().bg(ink_muted()).with_animation(("agent-dot", n), Animation::new(Duration::from_millis(1100)).repeat(), move |d, t| {
                    let phase = ((t * std::f32::consts::TAU) - n as f32 * 0.9).sin() * 0.5 + 0.5;
                    d.opacity(0.25 + 0.75 * phase)
                })
            };
            body = body.child(div().flex().items_center().gap(px(5.)).py_1().child(dot(0)).child(dot(1)).child(dot(2)));
        }

        // ---- Composer: the message, then the tool picker and send ----
        let mut entries: Vec<MenuEntry> = vec![];
        for (p, _) in installed.iter() {
            for (m, l) in p.models() {
                let (p, m) = (*p, m.to_string());
                let label = if m.is_empty() { p.name().to_string() } else { format!("{} · {l}", p.name()) };
                let checked = Some(p) == current && self.agent.model == m;
                entries.push(item(label, checked, move |e: &mut Editor| {
                    e.agent.provider = Some(p);
                    e.agent.model = m.clone();
                    prefs::set("agent_provider", serde_json::Value::String(p.id().into()));
                    prefs::set("agent_model", serde_json::Value::String(m.clone()));
                }));
            }
        }
        let current_label = match current {
            Some(p) => match p.models().iter().find(|(m, _)| !m.is_empty() && *m == self.agent.model) {
                Some((_, l)) => format!("{} · {l}", p.name()),
                None => p.name().to_string(),
            },
            None => "No AI tool".to_string(),
        };
        let action = if running {
            button("agent-stop", Variant::Secondary, 30.).w(px(30.)).px_0().child(div().size(px(9.)).rounded(px(2.)).bg(ink())).on_click(cx.listener(|this, _, _, cx| {
                if let Some(r) = this.agent.run.clone() {
                    r.stop();
                }
                cx.notify();
            }))
        } else {
            button("agent-send", Variant::Primary, 30.).w(px(30.)).px_0().child(icon("arrow-up", 15., black())).on_click(cx.listener(|this, _, window, cx| {
                let text = this.agent.input.read(cx).value().to_string();
                this.agent_send(text, window, cx);
            }))
        };
        let composer = div().px_3().pt_1().child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .rounded(px(14.))
                .bg(surface2())
                .border_1()
                .border_color(hairline())
                .p_2()
                // Enter sends; Shift+Enter starts a new line.
                .capture_key_down(cx.listener(|this, ev: &gpui::KeyDownEvent, window, cx| {
                    let k = &ev.keystroke;
                    if k.key == "enter" && !k.modifiers.shift && !k.modifiers.alt {
                        cx.stop_propagation();
                        let text = this.agent.input.read(cx).value().to_string();
                        this.agent_send(text, window, cx);
                    }
                }))
                .child(div().relative().px_1().child(self.track_bounds("input:agent")).child(Textarea::new(&self.agent.input).appearance(false).text_size(px(13.)).aria_label("Message Liemo AI")))
                .child(div().flex().items_center().gap_2().child(div().w(px(176.)).child(self.select("agent-provider", current_label, entries, cx))).child(div().flex_1()).child(action)),
        );

        panel()
            .w(px(SIDEBAR_W))
            .flex_none()
            .h_full()
            .child(header)
            .child(body)
            .when(!installed.is_empty(), |d| {
                d.child(composer).child(micro(format!("Runs {who} with your login. It can only use Liemo's editing tools.")).px_4().pt_2().pb_3())
            })
            .into_any_element()
    }
}
