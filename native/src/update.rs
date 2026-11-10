// Updates from GitHub Releases.
//
// A release is tagged vX.Y.Z and carries, per architecture, `Liemo-X.Y.Z-macos-<arch>.zip` (the
// notarised Liemo.app, zipped with ditto) and `….zip.sig`: an ed25519 signature of the zip, made
// with the release key (`liemo sign-update`). The app checks the latest release, and on request
// downloads the zip, checks that signature against the public key below, checks the new app's
// version, bundle id and code signature (same Apple team as this copy), swaps the bundles and
// relaunches.
//
// LIEMO_UPDATE_FEED points the check at another release JSON (for testing).

use anyhow::{anyhow, bail, Context, Result};
use base64::Engine;
use parking_lot::Mutex;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

pub const REPO: &str = "flaxodotdev/liemo";
pub const BUNDLE_ID: &str = "dev.flaxo.liemo";
/// Verifies update archives. The private half lives outside the repo (a GitHub Actions secret).
const PUBLIC_KEY: &str = "0zu1tut/3NHkJWYvCMLPSpazsK6SyKnrH0HuzofuB5c=";

pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
pub fn os() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    }
}
pub fn arch() -> &'static str {
    if cfg!(target_arch = "aarch64") { "arm64" } else { "x86_64" }
}

#[derive(Clone, Debug)]
pub struct Release {
    pub version: String,
    pub notes: String,
    pub page: String,
    zip: String,
    sig: String,
}

#[derive(Clone, Debug)]
pub enum State {
    Idle,
    Checking,
    UpToDate,
    Available(Release),
    /// Bytes so far, total (0 if unknown).
    Downloading(Release, u64, u64),
    Installing(Release),
    /// Swapped in; relaunching.
    Done,
    Failed(String, Option<Release>),
}

static STATE: Mutex<State> = Mutex::new(State::Idle);
static GEN: AtomicU64 = AtomicU64::new(0);
/// Set when the user asked (menu); the UI then shows the result even if there's nothing new.
static ASKED: AtomicU64 = AtomicU64::new(0);

pub fn state() -> State {
    STATE.lock().clone()
}
/// Bumps whenever the state changes, so views know to redraw.
pub fn generation() -> u64 {
    GEN.load(Ordering::Relaxed)
}
pub fn asked() -> u64 {
    ASKED.load(Ordering::Relaxed)
}
fn set(s: State) {
    *STATE.lock() = s;
    GEN.fetch_add(1, Ordering::Relaxed);
}

/// The .app this copy runs from, if it runs from one.
pub fn bundle_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
    exe.ancestors().find(|p| p.extension().is_some_and(|e| e == "app")).map(Path::to_path_buf)
}

/// Whether to look for updates: an installed Mac app (or a test feed), and always on Windows and
/// Linux, where a new version is offered as a download.
pub fn supported() -> bool {
    !cfg!(target_os = "macos") || bundle_path().is_some() || std::env::var("LIEMO_UPDATE_FEED").is_ok()
}

/// Whether this copy can install an update itself (the Mac app); elsewhere it links to it.
pub fn can_install() -> bool {
    cfg!(target_os = "macos") && (bundle_path().is_some() || std::env::var("LIEMO_UPDATE_FEED").is_ok())
}

// ---------- versions ----------
fn parse(v: &str) -> Option<Vec<u64>> {
    let v = v.trim().trim_start_matches('v');
    if v.contains('-') {
        return None; // pre-releases aren't offered
    }
    v.split('.').map(|p| p.parse().ok()).collect()
}
pub fn newer(candidate: &str, current: &str) -> bool {
    match (parse(candidate), parse(current)) {
        (Some(a), Some(b)) => {
            let n = a.len().max(b.len());
            let pad = |v: &Vec<u64>| (0..n).map(|i| v.get(i).copied().unwrap_or(0)).collect::<Vec<_>>();
            pad(&a) > pad(&b)
        }
        _ => false,
    }
}

// ---------- network ----------
fn agent() -> ureq::Agent {
    ureq::Agent::config_builder().timeout_global(Some(std::time::Duration::from_secs(60))).build().into()
}
fn get(url: &str) -> Result<ureq::http::Response<ureq::Body>> {
    agent()
        .get(url)
        .header("User-Agent", &format!("Liemo/{}", current_version()))
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| anyhow!("{e}"))
}

/// The newest release, if it's newer than this copy.
pub fn fetch_latest() -> Result<Option<Release>> {
    let feed = std::env::var("LIEMO_UPDATE_FEED").unwrap_or_else(|_| format!("https://api.github.com/repos/{REPO}/releases/latest"));
    let text = if let Some(path) = feed.strip_prefix("file://") {
        std::fs::read_to_string(path)?
    } else {
        let mut r = get(&feed).context("Couldn't reach GitHub")?;
        r.body_mut().read_to_string().map_err(|e| anyhow!("{e}"))?
    };
    let v: serde_json::Value = serde_json::from_str(&text).context("GitHub sent something unexpected")?;
    let tag = v["tag_name"].as_str().unwrap_or("");
    let version = tag.trim_start_matches('v').to_string();
    if v["draft"].as_bool() == Some(true) || v["prerelease"].as_bool() == Some(true) || !newer(&version, current_version()) {
        return Ok(None);
    }
    let want = format!("Liemo-{version}-{}-{}.zip", os(), arch());
    let assets = v["assets"].as_array().cloned().unwrap_or_default();
    let url_of = |name: &str| assets.iter().find(|a| a["name"].as_str() == Some(name)).and_then(|a| a["browser_download_url"].as_str()).map(String::from);
    let (zip, sig) = match (url_of(&want), url_of(&format!("{want}.sig"))) {
        (Some(z), Some(s)) => (z, s),
        // Windows and Linux only need to know there's something new (they link to the page).
        _ if !can_install() && assets.iter().any(|a| a["name"].as_str().is_some_and(|n| n.contains(os()))) => (String::new(), String::new()),
        _ => return Ok(None), // nothing for this computer yet
    };
    Ok(Some(Release {
        version,
        notes: v["body"].as_str().unwrap_or("").trim().to_string(),
        page: v["html_url"].as_str().map(String::from).unwrap_or_else(|| format!("https://github.com/{REPO}/releases/latest")),
        zip,
        sig,
    }))
}

/// Check in the background. `asked` marks a check the user started from the menu.
pub fn check(asked: bool) {
    if asked {
        ASKED.fetch_add(1, Ordering::Relaxed);
    }
    if matches!(state(), State::Checking | State::Downloading(..) | State::Installing(_) | State::Done) {
        return;
    }
    set(State::Checking);
    std::thread::spawn(|| {
        let result = fetch_latest();
        if result.is_ok() {
            crate::ui::prefs::set("update_checked", serde_json::Value::String(now().to_string()));
        }
        set(match result {
            Ok(Some(r)) => {
                // Test hook: install straight away (scripts/update-test.sh).
                if std::env::var("LIEMO_UPDATE_AUTOINSTALL").is_ok() {
                    install(r.clone());
                }
                State::Available(r)
            }
            Ok(None) => State::UpToDate,
            Err(e) => State::Failed(format!("{e:#}"), None),
        });
    });
}

/// Check at launch and every few hours after, in installed copies. Checks are at most an hour
/// apart (across all running copies), and only a check that reached GitHub counts toward that,
/// so a failed one is retried next launch.
pub fn check_on_launch() {
    if !supported() || std::env::var("LIEMO_NO_UPDATE_CHECK").is_ok() {
        return;
    }
    // Every launch asks GitHub (a release may have come out minutes ago); only the background
    // re-checks are spaced out.
    check(false);
    std::thread::spawn(|| loop {
        std::thread::sleep(std::time::Duration::from_secs(4 * 3600));
        check_if_due();
    });
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn check_if_due() {
    let last = crate::ui::prefs::get_str("update_checked").and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
    if now().saturating_sub(last) < 3600 && std::env::var("LIEMO_UPDATE_FEED").is_err() {
        return;
    }
    check(false);
}

// ---------- install ----------
fn download(url: &str, to: &Path, rel: &Release) -> Result<()> {
    if let Some(path) = url.strip_prefix("file://") {
        std::fs::copy(path, to)?;
        return Ok(());
    }
    let mut r = get(url)?;
    let total = r.headers().get("content-length").and_then(|v| v.to_str().ok()).and_then(|v| v.parse().ok()).unwrap_or(0);
    let mut body = r.body_mut().with_config().limit(2 << 30).reader();
    let mut out = std::fs::File::create(to)?;
    let mut buf = vec![0u8; 1 << 16];
    let (mut got, mut shown) = (0u64, 0u64);
    loop {
        let n = body.read(&mut buf)?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n])?;
        got += n as u64;
        if got - shown > 1 << 20 {
            shown = got;
            set(State::Downloading(rel.clone(), got, total));
        }
    }
    Ok(())
}

fn verify_signature(zip: &Path, sig_b64: &str) -> Result<()> {
    let b64 = base64::engine::general_purpose::STANDARD;
    let key: [u8; 32] = b64.decode(PUBLIC_KEY)?.try_into().map_err(|_| anyhow!("bad public key"))?;
    let sig: [u8; 64] = b64.decode(sig_b64.trim())?.try_into().map_err(|_| anyhow!("The update's signature is malformed"))?;
    let key = ed25519_dalek::VerifyingKey::from_bytes(&key)?;
    let data = std::fs::read(zip)?;
    key.verify_strict(&data, &ed25519_dalek::Signature::from_bytes(&sig)).map_err(|_| anyhow!("The update's signature doesn't match. It wasn't installed."))
}

fn plist(app: &Path, key: &str) -> Option<String> {
    let out = Command::new("/usr/libexec/PlistBuddy").arg("-c").arg(format!("Print :{key}")).arg(app.join("Contents/Info.plist")).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}
fn team_id(app: &Path) -> Option<String> {
    let out = Command::new("/usr/bin/codesign").args(["-dv", "--verbose=2"]).arg(app).output().ok()?;
    let text = String::from_utf8_lossy(&out.stderr).to_string();
    text.lines().find_map(|l| l.strip_prefix("TeamIdentifier=")).map(|s| s.trim().to_string()).filter(|s| s != "not set")
}

/// Download, verify and swap in `rel`; then relaunch. Runs on a background thread.
pub fn install(rel: Release) {
    std::thread::spawn(move || {
        if let Err(e) = install_blocking(&rel) {
            set(State::Failed(format!("{e:#}"), Some(rel)));
        }
    });
}

fn install_blocking(rel: &Release) -> Result<()> {
    let app = bundle_path().ok_or_else(|| anyhow!("Updates install into the Liemo app. This copy isn't running from one."))?;
    let parent = app.parent().ok_or_else(|| anyhow!("Can't find the folder Liemo is in"))?;
    // Stage next to the app so the final swap is a rename on the same disk.
    let stage = parent.join(format!(".liemo-update-{}", rel.version));
    let _ = std::fs::remove_dir_all(&stage);
    std::fs::create_dir_all(&stage).map_err(|_| anyhow!("Liemo can't write to {}. Move Liemo to Applications (or a folder you own) and try again.", parent.display()))?;
    set(State::Downloading(rel.clone(), 0, 0));
    let zip = stage.join("Liemo.zip");
    download(&rel.zip, &zip, rel).context("Download failed")?;
    let sig_path = stage.join("Liemo.zip.sig");
    download(&rel.sig, &sig_path, rel).context("Download failed")?;
    set(State::Installing(rel.clone()));
    verify_signature(&zip, &std::fs::read_to_string(&sig_path)?)?;

    let unpacked = stage.join("app");
    let ok = Command::new("/usr/bin/ditto").args(["-x", "-k"]).arg(&zip).arg(&unpacked).status()?.success();
    if !ok {
        bail!("Couldn't unpack the update");
    }
    let new_app = std::fs::read_dir(&unpacked)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .find(|p| p.extension().is_some_and(|e| e == "app"))
        .ok_or_else(|| anyhow!("The update doesn't contain Liemo.app"))?;
    if plist(&new_app, "CFBundleIdentifier").as_deref() != Some(BUNDLE_ID) {
        bail!("The update isn't Liemo (wrong bundle id)");
    }
    if plist(&new_app, "CFBundleShortVersionString").as_deref() != Some(rel.version.as_str()) {
        bail!("The update's version doesn't match the release");
    }
    let signed = Command::new("/usr/bin/codesign").args(["--verify", "--deep", "--strict"]).arg(&new_app).status()?.success();
    if !signed {
        bail!("The update's code signature is broken");
    }
    if let Some(mine) = team_id(&app) {
        if team_id(&new_app).as_deref() != Some(mine.as_str()) {
            bail!("The update is signed by a different developer");
        }
    }

    // Swap: this app aside, the new one in its place (rolled back if the second step fails).
    let old = stage.join("previous.app");
    std::fs::rename(&app, &old).map_err(|e| anyhow!("Couldn't replace Liemo ({e}). Is it in a folder you can write to?"))?;
    if let Err(e) = std::fs::rename(&new_app, &app) {
        let _ = std::fs::rename(&old, &app);
        bail!("Couldn't put the update in place ({e})");
    }
    set(State::Done);
    relaunch(&app, &stage);
    Ok(())
}

/// Start the new app once this process has exited, and tidy the staging folder.
fn relaunch(app: &Path, stage: &Path) {
    // Test runs carry their sandbox settings (data folder, muted, in the background) across.
    let q = |v: &str| v.replace('\'', "'\\''");
    let env: String = std::env::vars()
        .filter(|(k, _)| (k.starts_with("RAYNA_") || k.starts_with("LIEMO_")) && k != "LIEMO_UPDATE_AUTOINSTALL")
        .map(|(k, v)| format!(" --env '{}={}'", q(&k), q(&v)))
        .collect();
    let script = format!(
        "while kill -0 {pid} 2>/dev/null; do sleep 0.2; done; rm -rf '{stage}'; /usr/bin/open{bg}{env} '{app}'",
        bg = if std::env::var("RAYNA_BACKGROUND").is_ok() { " -g" } else { "" },
        pid = std::process::id(),
        stage = q(&stage.display().to_string()),
        app = q(&app.display().to_string()),
    );
    let _ = Command::new("/bin/sh").arg("-c").arg(script).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn();
}

// ---------- release tooling (used by CI) ----------
/// `liemo update-keygen <secret-key-file>`: make a signing key; prints the public key.
pub fn keygen(out: &Path) -> Result<()> {
    let mut seed = [0u8; 32];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut seed)?;
    let key = ed25519_dalek::SigningKey::from_bytes(&seed);
    let b64 = base64::engine::general_purpose::STANDARD;
    let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(out).with_context(|| format!("{} already exists", out.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        f.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    writeln!(f, "{}", b64.encode(seed))?;
    println!("{}", b64.encode(key.verifying_key().to_bytes()));
    Ok(())
}

/// `liemo sign-update <secret-key-file|-> <zip>`: writes <zip>.sig. `-` reads the key from
/// LIEMO_UPDATE_KEY.
pub fn sign(key_src: &str, zip: &Path) -> Result<()> {
    use ed25519_dalek::Signer;
    let b64 = base64::engine::general_purpose::STANDARD;
    let text = if key_src == "-" { std::env::var("LIEMO_UPDATE_KEY").context("LIEMO_UPDATE_KEY isn't set")? } else { std::fs::read_to_string(key_src)? };
    let seed: [u8; 32] = b64.decode(text.trim())?.try_into().map_err(|_| anyhow!("That isn't a Liemo update key"))?;
    let key = ed25519_dalek::SigningKey::from_bytes(&seed);
    let sig = key.sign(&std::fs::read(zip)?);
    let mut path = zip.as_os_str().to_owned();
    path.push(".sig");
    std::fs::write(&path, b64.encode(sig.to_bytes()))?;
    // Refuse to ship a key that doesn't match the one built into the app.
    if b64.encode(key.verifying_key().to_bytes()) != PUBLIC_KEY {
        bail!("This key doesn't match the public key built into Liemo");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::newer;
    #[test]
    fn versions() {
        assert!(newer("0.2.0", "0.1.9"));
        assert!(newer("v1.0.0", "0.9.12"));
        assert!(newer("0.1.10", "0.1.9"));
        assert!(!newer("0.1.0", "0.1.0"));
        assert!(!newer("0.1", "0.1.0"));
        assert!(!newer("0.3.0-beta.1", "0.2.0"));
    }
}
