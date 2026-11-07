// The Agent tab: chat with an AI command-line tool the user already has, which edits this project
// through Liemo's MCP server (see crate::agent). Edits arrive live and undo like any other.

use super::inspector::{item, MenuEntry};
use super::Editor;
use crate::agent::{self, Event, Provider};
use crate::ui::prefs;
use crate::ui::theme::*;
use futures::StreamExt;
use gpui::prelude::*;
use gpui::{div, px, AnyElement, Context, Entity, ScrollHandle, Window};
use gpui_component::input::{Input, InputState};
use std::collections::HashMap;

pub enum Msg {
    User(String),
    Reply(String),
    /// A Liemo action: call id, tool, summary, and how it went (None while running).
    Tool(String, String, String, Option<bool>),
    Error(String),
}

pub struct AgentChat {
    pub provider: Option<Provider>,
    pub model: String,
    pub input: Entity<InputState>,
    pub msgs: Vec<Msg>,
    pub run: Option<agent::Run>,
    /// Each tool's conversation, so follow-ups continue it.
    pub sessions: HashMap<Provider, String>,
    pub scroll: ScrollHandle,
    /// The last reply is still streaming in (deltas append to it).
    streaming: bool,
}

impl AgentChat {
    pub fn new(window: &mut Window, cx: &mut Context<Editor>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Ask for an edit…"));
        AgentChat {
            provider: prefs::get_str("agent_provider").and_then(|p| Provider::from_id(&p)),
            model: prefs::get_str("agent_model").unwrap_or_default(),
            input,
            msgs: vec![],
            run: None,
            sessions: HashMap::new(),
            scroll: ScrollHandle::new(),
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

impl Editor {
    pub(super) fn agent_send(&mut self, text: String, window: &mut Window, cx: &mut Context<Self>) {
        let text = text.trim().to_string();
        if text.is_empty() || self.agent.run.is_some() {
            return;
        }
        let Some(provider) = self.agent.current() else { return };
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

    pub(super) fn render_agent(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let installed = agent::installed();
        let current = self.agent.current();
        let running = self.agent.run.is_some();

        // ---- Header: one list of tool + model, and a new chat ----
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
        let header = div()
            .flex()
            .items_center()
            .gap_2()
            .p_3()
            .border_b_1()
            .border_color(hairline_soft())
            .child(div().flex_1().min_w_0().child(self.select("agent-provider", current_label, entries, cx)))
            .child(icon_button("agent-new", "plus", "New chat", None, 32., Variant::Ghost, cx.listener(|this, _, _, cx| {
                if let Some(r) = this.agent.run.take() {
                    r.stop();
                }
                this.agent.msgs.clear();
                this.agent.sessions.clear();
                cx.notify();
            })));

        // ---- Conversation ----
        let mut body = div().id("agent-scroll").flex_1().min_h_0().overflow_y_scroll().track_scroll(&self.agent.scroll).flex().flex_col().gap_3().p_3();
        if installed.is_empty() {
            body = body.child(
                div().flex().flex_col().gap_2().pt_6().child(type_caption(div()).child("No AI tool found")).child(micro(
                    "The agent uses an AI command-line tool you already have, with its own login: Claude Code, Codex, Gemini CLI, OpenCode or Grok. Install one, sign in, then reopen this project.",
                )),
            );
        } else if self.agent.msgs.is_empty() {
            let who = current.map(|p| p.name()).unwrap_or("");
            body = body
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .pt_2()
                        .child(type_caption(div()).child("Edit with AI"))
                        .child(micro(format!("Ask for changes in plain words. {who} makes them in this project as you watch, using your own {who} login. Undo works as usual."))),
                )
                .children(SUGGESTIONS.iter().enumerate().map(|(i, s)| {
                    let s = s.to_string();
                    div()
                        .id(("agent-suggest", i))
                        .px_3()
                        .py_2()
                        .rounded(px(10.))
                        .bg(surface2())
                        .hover(|d| d.bg(surface3()))
                        .cursor_pointer()
                        .text_size(px(12.5))
                        .text_color(ink_muted())
                        .child(s.clone())
                        .on_click(cx.listener(move |this, _, window, cx| this.agent_send(s.clone(), window, cx)))
                }));
        }
        for (i, m) in self.agent.msgs.iter().enumerate() {
            body = body.child(match m {
                Msg::User(t) => div()
                    .flex()
                    .justify_end()
                    .child(div().max_w(gpui::relative(0.88)).px_3().py_2().rounded(px(14.)).bg(surface3()).text_size(px(13.)).child(t.clone()))
                    .into_any_element(),
                Msg::Reply(t) => div().text_size(px(13.)).line_height(px(19.)).child(t.replace("**", "").replace('`', "")).into_any_element(),
                Msg::Tool(_, name, summary, state) => {
                    let (ic, color) = match state {
                        None => ("loader-2", ink_muted()),
                        Some(true) => ("check", success()),
                        Some(false) => ("x", coral()),
                    };
                    div()
                        .id(("agent-tool", i))
                        .flex()
                        .items_center()
                        .gap_2()
                        .text_size(px(12.))
                        .text_color(ink_muted())
                        .child(icon(ic, 13., color))
                        .child(type_timecode(div(), 11.5).text_color(ink()).child(name.clone()))
                        .when(!summary.is_empty(), |d| d.child(div().min_w_0().truncate().child(summary.clone())))
                        .into_any_element()
                }
                Msg::Error(t) => div()
                    .px_3()
                    .py_2()
                    .rounded(px(10.))
                    .bg(alpha(coral(), 0.1))
                    .text_size(px(12.5))
                    .text_color(coral())
                    .child(t.clone())
                    .into_any_element(),
            });
        }
        if running && !self.agent.streaming {
            let who = current.map(|p| p.name()).unwrap_or("The agent");
            body = body.child(div().flex().items_center().gap_2().text_size(px(12.)).text_color(ink_muted()).child(icon("loader-2", 13., ink_muted())).child(format!("{who} is working…")));
        }

        // ---- Composer ----
        let send = if running {
            button("agent-stop", Variant::Secondary, 32.)
                .w(px(32.))
                .px_0()
                .child(div().size(px(10.)).rounded(px(2.)).bg(ink()))
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Some(r) = this.agent.run.clone() {
                        r.stop();
                    }
                    cx.notify();
                }))
        } else {
            button("agent-send", Variant::Primary, 32.)
                .w(px(32.))
                .px_0()
                .child(icon("arrow-up", 16., black()))
                .on_click(cx.listener(|this, _, window, cx| {
                    let text = this.agent.input.read(cx).value().to_string();
                    this.agent_send(text, window, cx);
                }))
        };
        let composer = div()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .border_t_1()
            .border_color(hairline_soft())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().relative().flex_1().min_w_0().child(self.track_bounds("input:agent")).child(Input::new(&self.agent.input).aria_label("Message the agent")))
                    .child(send),
            )
            .when_some(current, |d, p| d.child(micro(format!("Runs {} with your login. It can only use Liemo's editing tools.", p.name()))));

        let _ = window;
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(header)
            .child(body)
            .when(!installed.is_empty(), |d| d.child(composer))
            .into_any_element()
    }
}
