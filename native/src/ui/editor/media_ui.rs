// Bringing in folders (and keeping up with them), and editing proxies for big videos.

use super::Editor;
use crate::media::{import, proxy};
use crate::model::types::AssetType;
use crate::storage;
use gpui::{AppContext, Context};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Proxies being made: media id → how far along (0–1).
pub type Proxying = Arc<Mutex<HashMap<String, f64>>>;

/// Every media or subtitle file in a folder and its subfolders (four levels), by name; hidden
/// files and folders skipped.
pub fn files_in(dir: &Path) -> Vec<PathBuf> {
    fn walk(d: &Path, depth: u32, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(d) else { return };
        let mut entries: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
        entries.sort();
        for p in entries {
            if p.file_name().and_then(|n| n.to_str()).map(|n| n.starts_with('.')).unwrap_or(true) {
                continue;
            }
            if p.is_dir() {
                if depth < 4 {
                    walk(&p, depth + 1, out);
                }
            } else if import::kind_of(&p).is_some() {
                out.push(p);
            }
        }
    }
    let mut out = vec![];
    walk(dir, 0, &mut out);
    out
}

impl Editor {
    /// Folders become their files; each folder is remembered so files added to it later join the
    /// media pool next time the project opens.
    pub(super) fn expand_folders(&mut self, paths: Vec<PathBuf>) -> Vec<PathBuf> {
        let mut out = vec![];
        for p in paths {
            if p.is_dir() {
                let found = files_in(&p);
                let name = import::file_name(&p);
                if found.is_empty() {
                    self.store.toast(format!("No video, audio or images in {name}"));
                } else {
                    self.store.toast(format!("Watching {name}: {} files now, and new ones each time this project opens", found.len()));
                }
                if !self.project.watch.contains(&p) {
                    self.project.watch.push(p);
                    self.store.changed();
                }
                out.extend(found);
            } else {
                out.push(p);
            }
        }
        out
    }

    /// Files that appeared in watched folders since the project last had them.
    pub(super) fn rescan_watched(&mut self, cx: &mut Context<Self>) {
        if self.project.watch.is_empty() {
            return;
        }
        let known: Vec<PathBuf> = self.store.pool.iter().filter_map(|id| storage::get_media(id)).map(|m| m.path).collect();
        let mut new = vec![];
        for dir in self.project.watch.clone() {
            for f in files_in(&dir) {
                if !known.contains(&f) && !crate::model::captions::is_subtitle_file(&f) {
                    new.push(f);
                }
            }
        }
        if !new.is_empty() {
            self.store.toast(format!("{} new file{} in watched folders", new.len(), if new.len() == 1 { "" } else { "s" }));
            self.import(new, None, cx);
        }
    }

    /// Make an editing proxy for an imported video in the background.
    pub fn make_proxy(&mut self, media_id: &str, cx: &mut Context<Self>) {
        let Some(rec) = storage::get_media(media_id).filter(|m| m.kind == AssetType::Video) else { return };
        if self.proxying.lock().contains_key(media_id) || proxy::exists(media_id) {
            return;
        }
        self.proxying.lock().insert(media_id.to_string(), 0.0);
        let (prog, id) = (self.proxying.clone(), media_id.to_string());
        let cancel = Arc::new(AtomicBool::new(false));
        let task = cx.background_spawn({
            let (prog, id, cancel) = (prog.clone(), id.clone(), cancel.clone());
            async move {
                let dst = proxy::path(&id);
                proxy::make(&rec.path, &dst, &cancel, &|f| {
                    if let Some(v) = prog.lock().get_mut(&id) {
                        *v = f;
                    }
                })
                .map(|_| rec.name)
                .map_err(|e| e.to_string())
            }
        });
        // Redraw now and then while it runs, so the library shows how far along it is.
        cx.spawn(async move |this, cx| {
            let mut task = task;
            loop {
                let tick = cx.background_executor().timer(Duration::from_millis(700));
                match futures::future::select(task, Box::pin(tick)).await {
                    futures::future::Either::Left((res, _)) => {
                        let _ = this.update(cx, |this, cx| {
                            this.proxying.lock().remove(&id);
                            match res {
                                Ok(name) => {
                                    this.store.toast(format!("Editing proxy ready for {name}"));
                                    this.reset_media = true;
                                }
                                Err(e) => this.store.toast(format!("Couldn't make a proxy: {e}")),
                            }
                            cx.notify();
                        });
                        break;
                    }
                    futures::future::Either::Right((_, t)) => {
                        task = t;
                        if this.update(cx, |_, cx| cx.notify()).is_err() {
                            cancel.store(true, Ordering::Relaxed);
                            break;
                        }
                    }
                }
            }
        })
        .detach();
    }

    pub fn remove_proxy(&mut self, media_id: &str) {
        proxy::remove(media_id);
        self.reset_media = true;
        self.store.toast("Proxy removed. The preview plays the original.");
    }

    /// Proxies for every big video in the media pool.
    pub fn make_all_proxies(&mut self, cx: &mut Context<Self>) {
        let ids: Vec<String> = self
            .store
            .pool
            .iter()
            .filter_map(|id| storage::get_media(id))
            .filter(|m| m.kind == AssetType::Video && proxy::worth_it(m.width, m.height) && !proxy::exists(&m.id))
            .map(|m| m.id)
            .collect();
        if ids.is_empty() {
            self.store.toast("No videos above 1080p need a proxy");
            return;
        }
        self.store.toast(format!("Making {} editing prox{}", ids.len(), if ids.len() == 1 { "y" } else { "ies" }));
        for id in ids {
            self.make_proxy(&id, cx);
        }
    }

    pub fn set_proxies(&mut self, on: bool) {
        proxy::ENABLED.store(on, Ordering::Relaxed);
        crate::ui::prefs::set("proxies", serde_json::json!(on));
        self.reset_media = true;
    }

    /// What the library shows for an imported video's proxy.
    pub fn proxy_status(&self, media_id: &str) -> Option<String> {
        if let Some(f) = self.proxying.lock().get(media_id) {
            return Some(format!("Proxy {}%", (f * 100.0).round()));
        }
        proxy::exists(media_id).then(|| "Proxy".to_string())
    }
}
