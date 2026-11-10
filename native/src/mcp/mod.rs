// `rayna mcp`: a Model Context Protocol server on stdio, so any AI harness (Claude Code, Claude
// Desktop, Cursor…) can edit Liemo projects.
//
// When the Liemo app is running it listens on a local socket and the server hands each tool
// call to it, so edits land in the open editor as they happen. Without the app, tools work on
// the project files directly.

pub mod tools;

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use local::{bind, connect};
use std::path::PathBuf;
use tools::{call, definitions, FileWorkspace, ToolOut};

const PROTOCOLS: [&str; 4] = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];
/// What every AI using Liemo is told up front: the whole feature map, so nothing gets missed.
pub const INSTRUCTIONS: &str = "Liemo is a video editor. Times are in seconds. Projects have five tracks: T1 titles, V2 overlays \
and picture-in-picture, V1 main video, A1 music, A2 voice and sound effects.

Workflow: list_projects or create_project (list_formats for sizes) -> get_project to read the timeline -> add material -> shape \
it -> render_frame to look at the result -> export. When Liemo is open, edits show up live in its editor and undo/redo work.

Material: list_library has sample footage (scenes), shader backgrounds, stills, overlays (light leaks, grain...), built-in music \
and text styles; add any of them with add_clip. import_media brings in the user's own video, audio and image files (absolute \
paths). add_text puts titles on T1; text can use bundled fonts or any font installed on the Mac (list_library kind font), and \
blend invert keeps text readable on any footage.

Look (visual clips): apply_effect stacks effects; list_effects shows every one with its parameters. Groups: Instagram filters; \
colour scale (scale-red, scale-blue, scale-violet... turns the picture into one colour from black through the colour to its \
highlights; its color param takes any #rrggbb, so any colour works); shader filters; stylize (ASCII, halftone, duotone, \
glow...); retro (VHS, CRT, glitch, dither); distort and camera; colour looks. amount mixes an effect with the clip underneath. \
update_clip effectsOn chooses whether effects stay on the clip's shape or fill the frame. set_transition adds transitions \
and fades.

Sound: update_clip volume, fadeIn and fadeOut work on any clip with sound. Audio clips (imported audio and recordings, type \
audio) can also be reworked with update_clip sound: presets slowed-reverb, sped-up, nightcore, daycore, hall, lofi, \
underwater, 8d, or fine control of speed (0.5-2; pitch follows speed like a record unless keepPitch), reverb, room, bass, \
muffle (low-pass) and pan8d (sound circling the listener). Changing speed changes the clip's length on the timeline, so \
read get_project afterwards. These sound effects don't apply to the sound of video clips or to built-in music. \
record_audio records from the microphone into A2 while Liemo has the project open.

Checking: render_frame shows the picture only; you can't hear audio, so describe sound changes in words. export makes \
video, gif, a still or an audio file (kind audio).";

pub fn socket_path() -> PathBuf {
    let p = crate::storage::root().join("mcp.sock");
    // Unix socket paths are limited to ~104 bytes on macOS; deep data folders use a short name
    // in /tmp derived from the folder, so the app and `rayna mcp` still agree on it.
    if p.as_os_str().len() < 100 {
        return p;
    }
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    crate::storage::root().hash(&mut h);
    PathBuf::from(format!("/tmp/rayna-{:016x}.sock", h.finish()))
}

/// A tool result in MCP's shape.
pub fn to_mcp(r: Result<ToolOut, String>) -> Value {
    match r {
        Ok(o) => {
            let mut content = vec![json!({ "type": "text", "text": o.text })];
            if let Some(png) = o.png {
                content.push(json!({ "type": "image", "data": base64(&png), "mimeType": "image/png" }));
            }
            json!({ "content": content, "isError": false })
        }
        Err(e) => json!({ "content": [{ "type": "text", "text": e }], "isError": true }),
    }
}

/// Ask a running Liemo to run the tool. None when the app isn't running.
fn via_app(name: &str, args: &Value) -> Option<Value> {
    let stream = connect(&socket_path()).ok()?;
    let mut w = stream.try_clone().ok()?;
    let line = serde_json::to_string(&json!({ "tool": name, "args": args })).ok()?;
    w.write_all(line.as_bytes()).ok()?;
    w.write_all(b"\n").ok()?;
    let mut reply = String::new();
    BufReader::new(stream).read_line(&mut reply).ok()?;
    serde_json::from_str(&reply).ok()
}

/// The stdio server loop.
pub fn serve_stdio() {
    crate::media::init();
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let msg: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => {
                let _ = writeln!(stdout, "{}", json!({ "jsonrpc": "2.0", "id": null, "error": { "code": -32700, "message": "Parse error" } }));
                let _ = stdout.flush();
                continue;
            }
        };
        let batch = msg.as_array().cloned().unwrap_or_else(|| vec![msg]);
        let mut replies = vec![];
        for m in batch {
            if let Some(r) = handle(&m) {
                replies.push(r);
            }
        }
        for r in replies {
            let _ = writeln!(stdout, "{r}");
        }
        let _ = stdout.flush();
    }
}

fn handle(m: &Value) -> Option<Value> {
    let id = m.get("id").cloned();
    let method = m.get("method").and_then(|v| v.as_str()).unwrap_or("");
    let params = m.get("params").cloned().unwrap_or(json!({}));
    // Notifications get no reply.
    let id = id?;
    let result = match method {
        "initialize" => {
            let asked = params.get("protocolVersion").and_then(|v| v.as_str()).unwrap_or(PROTOCOLS[1]);
            let version = if PROTOCOLS.contains(&asked) { asked } else { PROTOCOLS[1] };
            Ok(json!({
                "protocolVersion": version,
                "capabilities": { "tools": { "listChanged": false } },
                "serverInfo": { "name": "liemo", "title": "Liemo video editor", "version": env!("CARGO_PKG_VERSION") },
                "instructions": INSTRUCTIONS,
            }))
        }
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": definitions() })),
        "tools/call" => {
            let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            Ok(via_app(&name, &args).unwrap_or_else(|| to_mcp(call(&name, &args, &mut FileWorkspace))))
        }
        "resources/list" => Ok(json!({ "resources": [] })),
        "prompts/list" => Ok(json!({ "prompts": [] })),
        _ => Err(json!({ "code": -32601, "message": format!("Method not found: {method}") })),
    };
    Some(match result {
        Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
        Err(e) => json!({ "jsonrpc": "2.0", "id": id, "error": e }),
    })
}

/// In the app: accept tool calls from `rayna mcp` processes. Each request is handed to `run`
/// (which gets it onto the UI thread) and its reply written back.
pub fn listen(run: impl Fn(String, Value) -> Value + Send + Sync + 'static) {
    let path = socket_path();
    let _ = std::fs::create_dir_all(path.parent().unwrap());
    // A previous run may have left the socket behind; if nothing answers on it, take it over.
    if connect(&path).is_err() {
        let _ = std::fs::remove_file(&path);
    }
    let Ok(listener) = bind(&path) else {
        eprintln!("Liemo: another Liemo is already taking AI edits; this window won't.");
        return;
    };
    let run = std::sync::Arc::new(run);
    std::thread::spawn(move || {
        for conn in listener.incoming().flatten() {
            let run = run.clone();
            std::thread::spawn(move || {
                let mut w = match conn.try_clone() {
                    Ok(w) => w,
                    Err(_) => return,
                };
                let mut reader = BufReader::new(conn);
                let mut line = String::new();
                while reader.read_line(&mut line).map(|n| n > 0).unwrap_or(false) {
                    let reply = match serde_json::from_str::<Value>(&line) {
                        Ok(req) => {
                            let name = req.get("tool").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            run(name, req.get("args").cloned().unwrap_or(json!({})))
                        }
                        Err(e) => to_mcp(Err(e.to_string())),
                    };
                    let _ = writeln!(w, "{reply}");
                    line.clear();
                }
            });
        }
    });
}

/// How `liemo mcp` reaches the running app: a Unix socket at `socket_path()`, or on Windows a
/// localhost port whose number is written there.
#[cfg(unix)]
mod local {
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::Path;
    pub fn connect(p: &Path) -> std::io::Result<UnixStream> {
        UnixStream::connect(p)
    }
    pub fn bind(p: &Path) -> std::io::Result<UnixListener> {
        UnixListener::bind(p)
    }
}
#[cfg(windows)]
mod local {
    use std::net::{TcpListener, TcpStream};
    use std::path::Path;
    pub fn connect(p: &Path) -> std::io::Result<TcpStream> {
        let port: u16 = std::fs::read_to_string(p)?.trim().parse().map_err(|_| std::io::Error::other("bad port file"))?;
        TcpStream::connect(("127.0.0.1", port))
    }
    pub fn bind(p: &Path) -> std::io::Result<TcpListener> {
        let l = TcpListener::bind("127.0.0.1:0")?;
        std::fs::write(p, l.local_addr()?.port().to_string())?;
        Ok(l)
    }
}

pub fn unlisten() {
    let _ = std::fs::remove_file(socket_path());
}

fn base64(b: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(b.len().div_ceil(3) * 4);
    for c in b.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}
