// The in-app agent: a chat that edits the open project through the AI command-line tools the
// user already has (and pays for) — Claude Code, Codex, Gemini CLI, OpenCode, Grok, Cursor,
// GitHub Copilot, Qwen Code and Amp. Liemo runs
// the tool headless in a private folder, gives it Liemo's MCP server as its only MCP server, turns
// off its shell, file and web tools, and turns its event stream into `Event`s for the chat. The
// MCP server proxies to this window, so edits land live and can be undone.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, OnceLock};

use parking_lot::Mutex;
use serde_json::{json, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Provider {
    Claude,
    Codex,
    Gemini,
    OpenCode,
    Grok,
    Cursor,
    Copilot,
    Qwen,
    Amp,
}

pub const PROVIDERS: [Provider; 9] =
    [Provider::Claude, Provider::Codex, Provider::Gemini, Provider::OpenCode, Provider::Grok, Provider::Cursor, Provider::Copilot, Provider::Qwen, Provider::Amp];

impl Provider {
    pub fn id(self) -> &'static str {
        match self {
            Provider::Claude => "claude",
            Provider::Codex => "codex",
            Provider::Gemini => "gemini",
            Provider::OpenCode => "opencode",
            Provider::Grok => "grok",
            Provider::Cursor => "cursor",
            Provider::Copilot => "copilot",
            Provider::Qwen => "qwen",
            Provider::Amp => "amp",
        }
    }
    /// The executable names to look for.
    fn bins(self) -> &'static [&'static str] {
        match self {
            Provider::Claude => &["claude"],
            Provider::Codex => &["codex"],
            Provider::Gemini => &["gemini"],
            Provider::OpenCode => &["opencode"],
            Provider::Grok => &["grok"],
            Provider::Cursor => &["cursor-agent"],
            Provider::Copilot => &["copilot"],
            Provider::Qwen => &["qwen"],
            Provider::Amp => &["amp"],
        }
    }
    /// The logo icon (rayna/icons/<name>.svg), or None for a letter badge.
    pub fn logo(self) -> Option<&'static str> {
        match self {
            Provider::Claude => Some("brand-claude"),
            Provider::Codex => Some("brand-openai"),
            Provider::Gemini => Some("brand-googlegemini"),
            Provider::OpenCode => Some("brand-opencode"),
            Provider::Cursor => Some("brand-cursor"),
            Provider::Copilot => Some("brand-githubcopilot"),
            Provider::Qwen => Some("brand-qwen"),
            Provider::Grok | Provider::Amp => None,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Provider::Claude => "Claude Code",
            Provider::Codex => "Codex",
            Provider::Gemini => "Gemini CLI",
            Provider::OpenCode => "OpenCode",
            Provider::Grok => "Grok",
            Provider::Cursor => "Cursor",
            Provider::Copilot => "GitHub Copilot",
            Provider::Qwen => "Qwen Code",
            Provider::Amp => "Amp",
        }
    }
    pub fn from_id(id: &str) -> Option<Provider> {
        PROVIDERS.into_iter().find(|p| p.id() == id)
    }
    /// The models to offer, as (id, label); the tool's own default is always first (""). Read
    /// from the tool where it can tell us (cached after the first ask).
    pub fn models(self) -> Vec<(String, String)> {
        static CACHE: OnceLock<Mutex<HashMap<Provider, Vec<(String, String)>>>> = OnceLock::new();
        let cache = CACHE.get_or_init(Default::default);
        if let Some(v) = cache.lock().get(&self) {
            return v.clone();
        }
        let mut v = vec![(String::new(), "Default".to_string())];
        v.extend(discover_models(self));
        cache.lock().insert(self, v.clone());
        v
    }
}

// ---------- Finding the tools ----------
/// The user's login-shell PATH: apps opened from Finder get a bare one, and these tools live in
/// ~/.local/bin, Homebrew and friends.
pub fn shell_path() -> &'static String {
    static P: OnceLock<String> = OnceLock::new();
    P.get_or_init(|| {
        if cfg!(windows) {
            // Windows apps get the full PATH; add where npm, bun and friends put their tools.
            let mut parts: Vec<PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
            let home = dirs::home_dir().unwrap_or_default();
            for extra in [dirs::data_dir().map(|d| d.join("npm")), Some(home.join(".local").join("bin")), Some(home.join(".bun").join("bin"))].into_iter().flatten() {
                if !parts.contains(&extra) {
                    parts.push(extra);
                }
            }
            return std::env::join_paths(parts).map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
        }
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
        let from_shell = Command::new(&shell)
            .args(["-lc", "printf %s \"$PATH\""])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();
        let home = dirs::home_dir().map(|h| h.display().to_string()).unwrap_or_default();
        let mut parts: Vec<String> = from_shell.split(':').filter(|s| !s.is_empty()).map(String::from).collect();
        for extra in [
            format!("{home}/.local/bin"),
            format!("{home}/.opencode/bin"),
            format!("{home}/.bun/bin"),
            format!("{home}/.npm-global/bin"),
            "/opt/homebrew/bin".into(),
            "/usr/local/bin".into(),
            "/usr/bin".into(),
            "/bin".into(),
        ] {
            if !parts.contains(&extra) {
                parts.push(extra);
            }
        }
        parts.join(":")
    })
}

fn find(bin: &str) -> Option<PathBuf> {
    // On Windows, npm installs tools as .cmd shims.
    let names: Vec<String> = if cfg!(windows) { ["exe", "cmd", "bat"].iter().map(|e| format!("{bin}.{e}")).collect() } else { vec![bin.to_string()] };
    std::env::split_paths(shell_path()).flat_map(|d| names.iter().map(move |n| d.join(n))).find(|p| p.is_file())
}

/// The tools installed on this Mac, found once (in the background at launch, which also asks
/// each for its models).
pub fn installed() -> &'static Vec<(Provider, PathBuf)> {
    static I: OnceLock<Vec<(Provider, PathBuf)>> = OnceLock::new();
    I.get_or_init(|| PROVIDERS.into_iter().filter_map(|p| p.bins().iter().find_map(|b| find(b)).map(|path| (p, path))).collect())
}

fn run_capture(bin: &PathBuf, args: &[&str]) -> Option<String> {
    let out = Command::new(bin).args(args).env("PATH", shell_path()).stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).to_string())
}

fn discover_models(p: Provider) -> Vec<(String, String)> {
    let bin = installed().iter().find(|(q, _)| *q == p).map(|(_, b)| b.clone());
    match p {
        Provider::Claude => [("opus", "Opus"), ("sonnet", "Sonnet"), ("haiku", "Haiku")].iter().map(|(a, b)| (a.to_string(), b.to_string())).collect(),
        // Codex keeps the list it shows in its picker.
        Provider::Codex => dirs::home_dir()
            .and_then(|h| std::fs::read(h.join(".codex/models_cache.json")).ok())
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            .and_then(|v| v["models"].as_array().cloned())
            .unwrap_or_default()
            .into_iter()
            .filter(|m| m["visibility"] == "list")
            .filter_map(|m| Some((m["slug"].as_str()?.to_string(), m["display_name"].as_str().or(m["slug"].as_str())?.to_string())))
            .collect(),
        Provider::OpenCode => bin
            .and_then(|b| run_capture(&b, &["models"]))
            .map(|t| t.lines().map(str::trim).filter(|l| l.contains('/') && !l.contains(' ')).map(|l| (l.to_string(), l.to_string())).collect())
            .unwrap_or_default(),
        // "  * grok-4.7 (default)"
        Provider::Grok => bin
            .and_then(|b| run_capture(&b, &["models"]))
            .map(|t| {
                t.lines()
                    .filter_map(|l| l.trim().strip_prefix("* ").or_else(|| l.trim().strip_prefix("- ")))
                    .filter_map(|l| l.split_whitespace().next())
                    .map(|m| (m.to_string(), m.to_string()))
                    .collect()
            })
            .unwrap_or_default(),
        Provider::Cursor => bin
            .and_then(|b| run_capture(&b, &["models"]))
            .map(|t| t.lines().filter_map(|l| l.trim().split_whitespace().next()).filter(|m| m.chars().any(|c| c.is_ascii_digit()) || m.contains('-')).map(|m| (m.to_string(), m.to_string())).collect())
            .unwrap_or_default(),
        _ => vec![],
    }
}

// ---------- Events ----------
#[derive(Clone, Debug)]
pub enum Event {
    /// A whole block of reply text.
    Text(String),
    /// More of the reply, as it streams.
    Delta(String),
    /// A Liemo tool started: call id, tool name, a short summary of its input.
    ToolStart(String, String, String),
    /// A tool finished: call id, whether it worked.
    ToolDone(String, bool),
    /// The tool's conversation id, to continue it next turn.
    Session(String),
    /// The turn ended, with an error message if it failed.
    Done(Option<String>),
}

/// A running turn; dropping it doesn't stop the tool, `stop` does.
#[derive(Clone)]
pub struct Run(Arc<Mutex<Option<Child>>>);
impl Run {
    pub fn stop(&self) {
        if let Some(mut c) = self.0.lock().take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

/// How far the chat may go on its own. Plan: it looks and writes documents but can't change the
/// edit. Ask: each change waits for the person's yes. Auto: it edits straight away (everything
/// can still be undone). There's no mode without these checks for paid requests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Auto,
    Ask,
    Plan,
}
pub const MODES: [(Mode, &str, &str); 3] = [
    (Mode::Auto, "Auto", "Edits straight away; undo anything"),
    (Mode::Ask, "Ask first", "Each change waits for your yes"),
    (Mode::Plan, "Plan", "Looks and plans, changes nothing"),
];
static MODE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
impl Mode {
    pub fn id(self) -> &'static str {
        match self {
            Mode::Auto => "auto",
            Mode::Ask => "ask",
            Mode::Plan => "plan",
        }
    }
    pub fn from_id(s: &str) -> Mode {
        match s {
            "ask" => Mode::Ask,
            "plan" => Mode::Plan,
            _ => Mode::Auto,
        }
    }
}
pub fn mode() -> Mode {
    match MODE.load(std::sync::atomic::Ordering::Relaxed) {
        1 => Mode::Ask,
        2 => Mode::Plan,
        _ => Mode::Auto,
    }
}
pub fn set_mode(m: Mode) {
    MODE.store(m as u8, std::sync::atomic::Ordering::Relaxed);
}

/// Tools that only look (or write the plan's documents), allowed in every mode.
pub fn read_only(tool: &str) -> bool {
    matches!(
        tool,
        "render_frame" | "get_project" | "list_projects" | "list_library" | "list_effects" | "list_transitions" | "list_formats"
            | "read_docs" | "write_doc" | "design_check" | "search_stock" | "transcribe" | "export_captions"
    )
}

/// One line saying what a tool call would do, for an approval prompt.
pub fn describe_call(tool: &str, args: &Value) -> String {
    let s = summary(args);
    let what = tool.replace('_', " ");
    if s.is_empty() { what } else { format!("{what}: {s}") }
}

/// The note that goes with each message in a mode other than Auto.
pub fn mode_note(m: Mode) -> Option<&'static str> {
    match m {
        Mode::Auto => None,
        Mode::Ask => Some("[Ask mode: each change you make waits for the user's approval; if one is declined, don't retry it, ask what they'd like instead.]"),
        Mode::Plan => Some("[Plan mode: look (get_project, render_frame, read_docs, design_check, search_stock) and plan, but don't change the edit; tools that edit are refused. Write the plan into the pipeline documents with write_doc, or reply with it, then stop. The user switches to Auto to carry it out.]"),
    }
}

pub struct Turn {
    pub provider: Provider,
    pub model: String,
    pub prompt: String,
    /// The conversation to continue, if any.
    pub session: Option<String>,
    pub project_id: String,
    pub project_name: String,
}

fn context(t: &Turn) -> String {
    let brand = crate::model::brand::load().describe().map(|b| format!("\n\n{b}")).unwrap_or_default();
    format!(
        "You are the editing assistant inside Liemo, a video editor. The user has the project \"{}\" (id {}) open, \
         and your Liemo tool calls change it live in their editor; they can undo anything. Work on that project unless \
         they ask otherwise. Use only the liemo tools. Check results with render_frame when the look matters. Keep \
         replies short: say what you changed.\n\n{}\n\nThe user may point at things: [video \"…\" · id …] names a clip by id; \
         [Frame · … · time t] and [Snapshot · … · region x y w h] are what they see at time t, so look with render_frame \
         (time t, and that region) before answering.{}\n\nWhat Liemo can do:\n{}",
        t.project_name, t.project_id, crate::pipeline::describe(), brand, crate::mcp::INSTRUCTIONS
    )
}

/// Liemo's MCP server, as this executable runs it.
fn liemo_exe() -> String {
    std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_else(|_| "liemo".into())
}

/// The private folder a tool runs in (per tool and project, so "continue the latest" means this
/// project's chat).
fn workdir(t: &Turn) -> std::io::Result<PathBuf> {
    let dir = crate::storage::root().join("agent").join(t.provider.id()).join(&t.project_id);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn toml_str(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Built-in tools to turn off, so the agent can only edit through Liemo.
const GEMINI_BUILTINS: &[&str] = &[
    "run_shell_command", "write_file", "replace", "read_file", "read_many_files", "glob", "search_file_content",
    "list_directory", "web_fetch", "google_web_search", "save_memory",
];
const GROK_BUILTINS: &str = "run_terminal_command,read_file,search_replace,list_dir,grep,kill_command_or_subagent,get_command_or_subagent_output,spawn_subagent,scheduler_create";

fn command(t: &Turn, bin: &PathBuf, dir: &PathBuf) -> std::io::Result<Command> {
    let exe = liemo_exe();
    let first = t.session.is_none();
    // Tools without a system-prompt flag get the context in their first message.
    let prompt_with_context = if first { format!("{}\n\n{}", context(t), t.prompt) } else { t.prompt.clone() };
    let mut c = Command::new(bin);
    c.current_dir(dir).env("PATH", shell_path());
    match t.provider {
        Provider::Claude => {
            let mcp = json!({ "mcpServers": { "liemo": { "command": exe, "args": ["mcp", "--agent"] } } }).to_string();
            c.args(["-p", &t.prompt, "--output-format", "stream-json", "--verbose", "--mcp-config", &mcp, "--strict-mcp-config", "--tools", "", "--allowedTools", "mcp__liemo"]);
            c.args(["--append-system-prompt", &context(t)]);
            if let Some(s) = &t.session {
                c.args(["--resume", s]);
            }
            if !t.model.is_empty() {
                c.args(["--model", &t.model]);
            }
        }
        Provider::Codex => {
            // An inline table replaces the user's own MCP servers for this run.
            let servers = format!("mcp_servers={{liemo={{command={},args=[\"mcp\",\"--agent\"]}}}}", toml_str(&exe));
            c.arg("exec");
            if let Some(s) = &t.session {
                c.args(["resume", s]);
            }
            c.args(["--json", "--skip-git-repo-check", "--sandbox", "read-only", "-c", &servers, "-c", "approval_policy=\"never\""]);
            if !t.model.is_empty() {
                c.args(["-m", &t.model]);
            }
            c.arg(&prompt_with_context);
        }
        Provider::Gemini => {
            let settings = json!({
                "mcpServers": { "liemo": { "command": exe, "args": ["mcp", "--agent"], "trust": true } },
                "tools": { "exclude": GEMINI_BUILTINS },
            });
            std::fs::create_dir_all(dir.join(".gemini"))?;
            std::fs::write(dir.join(".gemini/settings.json"), serde_json::to_vec_pretty(&settings)?)?;
            c.args(["-p", &prompt_with_context, "-o", "stream-json", "--skip-trust", "--allowed-mcp-server-names", "liemo"]);
            if t.session.is_some() {
                c.args(["--resume", "latest"]);
            }
            if !t.model.is_empty() {
                c.args(["-m", &t.model]);
            }
        }
        Provider::OpenCode => {
            let cfg = json!({
                "$schema": "https://opencode.ai/config.json",
                "mcp": { "liemo": { "type": "local", "command": [exe, "mcp", "--agent"], "enabled": true } },
                "permission": { "edit": "deny", "bash": "deny", "webfetch": "deny" },
            });
            std::fs::write(dir.join("opencode.json"), serde_json::to_vec_pretty(&cfg)?)?;
            c.args(["run", "--format", "json"]);
            if let Some(s) = &t.session {
                c.args(["-s", s]);
            }
            if !t.model.is_empty() {
                c.args(["-m", &t.model]);
            }
            c.arg(&prompt_with_context);
        }
        Provider::Cursor => {
            // Cursor reads MCP servers from the workspace; --approve-mcps keeps headless runs from
            // stopping to ask about them.
            std::fs::create_dir_all(dir.join(".cursor"))?;
            std::fs::write(dir.join(".cursor/mcp.json"), serde_json::to_vec_pretty(&json!({ "mcpServers": { "liemo": { "command": exe, "args": ["mcp", "--agent"] } } }))?)?;
            c.args(["-p", "--output-format", "stream-json", "--trust", "--approve-mcps"]);
            if let Some(s) = &t.session {
                c.args(["--resume", s]);
            }
            if !t.model.is_empty() {
                c.args(["-m", &t.model]);
            }
            c.arg(&prompt_with_context);
        }
        Provider::Copilot => {
            let mcp = json!({ "mcpServers": { "liemo": { "type": "local", "command": exe, "args": ["mcp", "--agent"], "tools": ["*"] } } }).to_string();
            c.args(["-p", &prompt_with_context, "--output-format", "json", "--additional-mcp-config", &mcp, "--allow-tool", "liemo", "--deny-tool", "shell", "--deny-tool", "write"]);
            if let Some(s) = &t.session {
                c.args(["--resume", s]);
            }
            if !t.model.is_empty() {
                c.args(["--model", &t.model]);
            }
        }
        Provider::Qwen => {
            let settings = json!({
                "mcpServers": { "liemo": { "command": exe, "args": ["mcp", "--agent"], "trust": true } },
                "tools": { "exclude": GEMINI_BUILTINS },
            });
            std::fs::create_dir_all(dir.join(".qwen"))?;
            std::fs::write(dir.join(".qwen/settings.json"), serde_json::to_vec_pretty(&settings)?)?;
            c.args(["-p", &prompt_with_context, "--output-format", "stream-json"]);
            if t.session.is_some() {
                c.arg("--continue");
            }
            if !t.model.is_empty() {
                c.args(["-m", &t.model]);
            }
        }
        Provider::Amp => {
            let mcp = json!({ "liemo": { "command": exe, "args": ["mcp", "--agent"] } }).to_string();
            c.arg("-x");
            if let Some(s) = &t.session {
                c.args(["threads", "continue", s]);
            }
            c.args(["--stream-json", "--mcp-config", &mcp, &prompt_with_context]);
        }
        Provider::Grok => {
            std::fs::create_dir_all(dir.join(".grok"))?;
            std::fs::write(dir.join(".grok/config.toml"), format!("[mcp_servers.liemo]\ncommand = {}\nargs = [\"mcp\", \"--agent\"]\nenabled = true\n", toml_str(&exe)))?;
            c.args(["-p", &prompt_with_context, "--output-format", "streaming-json", "--trust", "--allow", "mcp__liemo", "--disallowed-tools", GROK_BUILTINS, "--disable-web-search"]);
            if let Some(s) = &t.session {
                c.args(["--resume", s]);
            }
            if !t.model.is_empty() {
                c.args(["-m", &t.model]);
            }
        }
    }
    Ok(c)
}

/// Start a turn. Events arrive on `send` from a background thread; the last one is `Done`.
pub fn start(t: Turn, send: impl Fn(Event) + Send + 'static) -> Option<Run> {
    let Some((_, bin)) = installed().iter().find(|(p, _)| *p == t.provider).cloned() else {
        send(Event::Done(Some(format!("{} isn't installed on this Mac.", t.provider.name()))));
        return None;
    };
    let spawn = (|| -> std::io::Result<Child> {
        let dir = workdir(&t)?;
        command(&t, &bin, &dir)?.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()
    })();
    let mut child = match spawn {
        Ok(c) => c,
        Err(e) => {
            send(Event::Done(Some(format!("Couldn't start {}: {e}", t.provider.name()))));
            return None;
        }
    };
    let stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    let run = Run(Arc::new(Mutex::new(Some(child))));
    let handle = run.clone();
    let provider = t.provider;
    std::thread::spawn(move || {
        let mut p = Parser { provider, failed: None, finished: false, names: HashMap::new() };
        if let Some(out) = stdout {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                if let Ok(v) = serde_json::from_str::<Value>(line.trim()) {
                    for e in p.parse(&v) {
                        send(e);
                    }
                }
            }
        }
        let mut err = String::new();
        if let Some(e) = stderr.as_mut() {
            let _ = e.read_to_string(&mut err);
        }
        let status = handle.0.lock().take().map(|mut c| c.wait());
        let stopped = status.is_none();
        let ok = status.and_then(|s| s.ok()).map(|s| s.success()).unwrap_or(false);
        let error = if stopped {
            Some("Stopped.".to_string())
        } else if p.failed.is_some() {
            p.failed.take()
        } else if !ok && !p.finished {
            Some(last_lines(&err).unwrap_or_else(|| format!("{} stopped unexpectedly.", provider.name())))
        } else {
            None
        };
        send(Event::Done(error));
    });
    Some(run)
}

fn last_lines(s: &str) -> Option<String> {
    let lines: Vec<&str> = s.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    (!lines.is_empty()).then(|| lines[lines.len().saturating_sub(3)..].join("\n"))
}

/// Liemo's tools as the tools name them ("mcp__liemo__add_clip", "liemo__add_clip"…).
fn liemo_tool(name: &str) -> Option<String> {
    let n = name.trim_start_matches("mcp__").trim_start_matches("mcp_");
    n.strip_prefix("liemo__").or_else(|| n.strip_prefix("liemo_")).or_else(|| n.strip_prefix("liemo.")).map(String::from)
}

fn summary(input: &Value) -> String {
    let mut parts = vec![];
    for k in ["content", "asset", "effect", "transition", "name", "time", "kind"] {
        if let Some(v) = input.get(k) {
            let s = v.as_str().map(String::from).unwrap_or_else(|| v.to_string());
            parts.push(if s.chars().count() > 32 { format!("{}…", s.chars().take(32).collect::<String>()) } else { s });
        }
    }
    parts.join(" · ")
}

struct Parser {
    provider: Provider,
    failed: Option<String>,
    finished: bool,
    /// Tool call id → whether it's one of Liemo's (others are hidden).
    names: HashMap<String, bool>,
}

/// The first tool name anywhere inside `v` ("name", "toolName", "tool", or a key naming it).
fn find_name(v: &Value) -> Option<String> {
    match v {
        Value::Object(m) => {
            for k in ["name", "toolName", "tool_name", "tool"] {
                if let Some(s) = m.get(k).and_then(Value::as_str) {
                    return Some(s.to_string());
                }
            }
            m.values().find_map(find_name)
        }
        _ => None,
    }
}
fn find_args(v: &Value) -> Value {
    match v {
        Value::Object(m) => ["args", "arguments", "input", "parameters"].iter().find_map(|k| m.get(*k).cloned()).or_else(|| m.values().map(find_args).find(|a| !a.is_null())).unwrap_or(Value::Null),
        _ => Value::Null,
    }
}

impl Parser {
    /// For tools whose JSON lines I haven't seen first hand: take text, Liemo tool calls and a
    /// session id from wherever they appear, and errors from "error" fields.
    fn loose(&mut self, v: &Value) -> Vec<Event> {
        let mut out = vec![];
        let t = v["type"].as_str().unwrap_or("").to_lowercase();
        if let Some(sid) = ["session_id", "sessionId", "chatId", "thread_id"].iter().find_map(|k| v[*k].as_str()) {
            out.push(Event::Session(sid.to_string()));
        }
        if t.contains("error") {
            self.failed = Some(v["message"].as_str().or(v["error"]["message"].as_str()).or(v["error"].as_str()).unwrap_or("The tool reported an error.").to_string());
            return out;
        }
        if t.contains("tool") {
            let id = ["id", "toolCallId", "tool_id", "call_id", "callId"].iter().find_map(|k| v[*k].as_str()).unwrap_or("").to_string();
            let status = v["status"].as_str().unwrap_or("");
            if let Some(name) = find_name(v) {
                if !self.names.contains_key(&id) {
                    out.extend(self.tool_start(&id, &name, &find_args(v)));
                }
            }
            if t.contains("result") || t.contains("end") || matches!(status, "completed" | "success" | "error" | "failed") {
                let ok = !(t.contains("error") || matches!(status, "error" | "failed"));
                out.extend(self.tool_done(&id, ok));
            }
            return out;
        }
        if t.contains("result") || t == "done" || t == "end" {
            self.finished = true;
        }
        let text = v["delta"].as_str().or(v["text"].as_str()).or(v["content"].as_str()).or(v["message"]["content"].as_str());
        if let Some(text) = text {
            if t.contains("delta") || v["delta"].is_string() {
                out.push(Event::Delta(text.to_string()));
            } else if t.contains("message") || t.contains("assistant") || t.contains("text") {
                out.push(Event::Text(text.to_string()));
            }
        }
        out
    }

    fn tool_start(&mut self, id: &str, name: &str, input: &Value) -> Vec<Event> {
        let mine = liemo_tool(name);
        self.names.insert(id.to_string(), mine.is_some());
        match mine {
            Some(n) => vec![Event::ToolStart(id.to_string(), n, summary(input))],
            None => vec![],
        }
    }
    fn tool_done(&mut self, id: &str, ok: bool) -> Vec<Event> {
        if self.names.get(id).copied().unwrap_or(false) {
            vec![Event::ToolDone(id.to_string(), ok)]
        } else {
            vec![]
        }
    }

    fn parse(&mut self, v: &Value) -> Vec<Event> {
        let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("");
        match self.provider {
            Provider::Copilot | Provider::Qwen => self.loose(v),
            Provider::Cursor if s("type") == "tool_call" => {
                // {"type":"tool_call","subtype":"started"|"completed","call_id":…,"tool_call":{…}}
                let id = s("call_id").to_string();
                let name = find_name(&v["tool_call"]).unwrap_or_default();
                if s("subtype") == "started" {
                    self.tool_start(&id, &name, &find_args(&v["tool_call"]))
                } else {
                    let ok = !v.to_string().contains("\"error\"");
                    self.tool_done(&id, ok)
                }
            }
            Provider::Claude | Provider::Cursor | Provider::Amp => match s("type") {
                "system" if s("subtype") == "init" => vec![Event::Session(s("session_id").into())],
                "assistant" => {
                    let mut out = vec![];
                    for c in v["message"]["content"].as_array().into_iter().flatten() {
                        match c["type"].as_str() {
                            Some("text") => out.push(Event::Text(c["text"].as_str().unwrap_or("").into())),
                            Some("tool_use") => out.extend(self.tool_start(c["id"].as_str().unwrap_or(""), c["name"].as_str().unwrap_or(""), &c["input"])),
                            _ => {}
                        }
                    }
                    out
                }
                "user" => {
                    let mut out = vec![];
                    for c in v["message"]["content"].as_array().into_iter().flatten() {
                        if c["type"] == "tool_result" {
                            out.extend(self.tool_done(c["tool_use_id"].as_str().unwrap_or(""), c["is_error"] != true));
                        }
                    }
                    out
                }
                "result" => {
                    self.finished = true;
                    if s("subtype") != "success" || v["is_error"] == true {
                        self.failed = Some(v["result"].as_str().map(String::from).unwrap_or_else(|| format!("{}: {}", self.provider.name(), s("subtype"))));
                    }
                    let sid = v["session_id"].as_str().or(v["chatId"].as_str()).or(v["thread_id"].as_str()).unwrap_or("");
                    vec![Event::Session(sid.into())]
                }
                _ => vec![],
            },
            Provider::Codex => {
                let item = &v["item"];
                let it = |k: &str| item.get(k).and_then(Value::as_str).unwrap_or("");
                match (s("type"), it("type")) {
                    ("thread.started", _) => vec![Event::Session(s("thread_id").into())],
                    ("item.completed", "agent_message") => vec![Event::Text(it("text").into())],
                    ("item.started", "mcp_tool_call") => {
                        let name = format!("{}__{}", it("server"), it("tool"));
                        self.tool_start(it("id"), &name, item.get("arguments").unwrap_or(&Value::Null))
                    }
                    ("item.completed", "mcp_tool_call") => self.tool_done(it("id"), it("status") != "failed" && item.get("error").map(Value::is_null).unwrap_or(true)),
                    ("turn.completed", _) => {
                        self.finished = true;
                        vec![]
                    }
                    ("turn.failed", _) | ("error", _) => {
                        let msg = v["error"]["message"].as_str().or(v["message"].as_str()).unwrap_or("Codex failed.");
                        self.failed = Some(msg.to_string());
                        vec![]
                    }
                    _ => vec![],
                }
            }
            Provider::Gemini => match s("type") {
                "init" => vec![Event::Session(s("session_id").into())],
                "message" if s("role") == "assistant" => {
                    let text = s("content").to_string();
                    if v["delta"] == true { vec![Event::Delta(text)] } else { vec![Event::Text(text)] }
                }
                "tool_use" => self.tool_start(s("tool_id"), s("tool_name"), v.get("parameters").unwrap_or(&Value::Null)),
                "tool_result" => self.tool_done(s("tool_id"), s("status") != "error"),
                "error" => {
                    self.failed = Some(v["message"].as_str().or(v["error"]["message"].as_str()).unwrap_or("Gemini CLI failed.").to_string());
                    vec![]
                }
                "result" => {
                    self.finished = true;
                    if s("status") == "error" {
                        self.failed = Some(v["error"]["message"].as_str().unwrap_or("Gemini CLI failed.").to_string());
                    }
                    vec![]
                }
                _ => vec![],
            },
            Provider::OpenCode => {
                let part = &v["part"];
                let mut out = vec![];
                if let Some(id) = v.get("sessionID").and_then(Value::as_str) {
                    out.push(Event::Session(id.into()));
                }
                match s("type") {
                    "text" => out.push(Event::Text(part["text"].as_str().unwrap_or("").into())),
                    "tool_use" | "tool" => {
                        let id = part["callID"].as_str().or(part["id"].as_str()).unwrap_or("");
                        let tool = part["tool"].as_str().unwrap_or("");
                        let status = part["state"]["status"].as_str().unwrap_or("");
                        if !self.names.contains_key(id) {
                            out.extend(self.tool_start(id, tool, &part["state"]["input"]));
                        }
                        if status == "completed" || status == "error" {
                            out.extend(self.tool_done(id, status == "completed"));
                        }
                    }
                    "error" => {
                        let e = &v["error"];
                        self.failed = Some(e["message"].as_str().or(e["data"]["message"].as_str()).unwrap_or("OpenCode failed.").to_string());
                    }
                    "step_finish" => self.finished = true,
                    _ => {}
                }
                out
            }
            Provider::Grok => match s("type") {
                "text" => vec![Event::Delta(s("data").into())],
                "tool_call" => {
                    let raw = &v["rawInput"];
                    // Grok calls MCP tools through its own use_tool.
                    let (name, input) = if s("toolName") == "use_tool" { (raw["tool_name"].as_str().unwrap_or("").to_string(), raw["tool_input"].clone()) } else { (s("toolName").to_string(), raw.clone()) };
                    self.tool_start(s("toolCallId"), &name, &input)
                }
                "tool_call_update" => match v["status"].as_str() {
                    Some("completed") => self.tool_done(s("toolCallId"), true),
                    Some("failed") | Some("error") => self.tool_done(s("toolCallId"), false),
                    _ => vec![],
                },
                "end" => {
                    self.finished = true;
                    vec![Event::Session(s("sessionId").into())]
                }
                "error" => {
                    self.failed = Some(v["message"].as_str().or(v["error"]["message"].as_str()).unwrap_or("Grok failed.").to_string());
                    vec![]
                }
                _ => vec![],
            },
        }
    }
}
