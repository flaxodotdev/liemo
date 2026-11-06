// The preview renderer: a thread that composites the frame for the latest request and hands it
// back as a GPUI image. Requests pile up while it works; only the newest is drawn.

use super::images::to_image;
use crate::model::types::{Clip, Track};
use crate::render::canvas::Canvas;
use crate::render::compositor::{render_frame, Scene};
use crate::render::frames::DecoderPool;
use crossbeam_channel::{unbounded, Receiver, Sender};
use futures::channel::mpsc::UnboundedSender;
use gpui::RenderImage;
use parking_lot::Mutex;
use std::sync::Arc;

pub struct Job {
    pub time: f64,
    pub clips: Arc<Vec<Clip>>,
    pub tracks: Arc<Vec<Track>>,
    pub reset_media: bool,
    /// The project's canvas, frame rate and background.
    pub env: crate::model::time::Env,
}

pub struct Previewer {
    tx: Sender<Job>,
    pub latest: Arc<Mutex<Option<Arc<RenderImage>>>>,
}

impl Previewer {
    pub fn new(wake: UnboundedSender<()>) -> Self {
        let (tx, rx): (Sender<Job>, Receiver<Job>) = unbounded();
        let latest: Arc<Mutex<Option<Arc<RenderImage>>>> = Arc::new(Mutex::new(None));
        let out = latest.clone();
        std::thread::Builder::new()
            .name("rayna-preview".into())
            .spawn(move || {
                let mut pool = DecoderPool::new(1280, 1280);
                let mut failures = 0;
                while let Ok(mut job) = rx.recv() {
                    let mut reset = job.reset_media;
                    while let Ok(next) = rx.try_recv() {
                        reset |= next.reset_media;
                        job = next;
                    }
                    if reset {
                        pool.clear();
                    }
                    pool.retain(&job.clips);
                    crate::model::time::set_env(job.env);
                    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        let mut c = Canvas::new(job.env.w as u32, job.env.h as u32);
                        render_frame(&mut c, &Scene { clips: &job.clips, tracks: &job.tracks }, job.time, &mut pool);
                        to_image(&c.into_pixmap())
                    }));
                    match r {
                        Ok(img) => {
                            failures = 0;
                            *out.lock() = Some(img);
                            if wake.unbounded_send(()).is_err() {
                                break;
                            }
                        }
                        Err(_) => {
                            failures += 1;
                            eprintln!("Liemo: preview frame failed ({failures})");
                        }
                    }
                }
            })
            .expect("preview thread");
        Previewer { tx, latest }
    }

    pub fn request(&self, job: Job) {
        let _ = self.tx.send(job);
    }
}
