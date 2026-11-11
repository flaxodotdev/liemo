// Auto captions on the Mac: Apple's Speech framework listens to a sound file and gives back each
// word with when it was said. Recognition runs on the device where the language allows it.

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::AnyThread;
use objc2_foundation::{ns_string, NSBundle, NSError, NSOperationQueue, NSString, NSURL};
use objc2_speech::{SFSpeechRecognitionResult, SFSpeechRecognizer, SFSpeechRecognizerAuthorizationStatus, SFSpeechURLRecognitionRequest};
use parking_lot::Mutex;
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

/// Words said in a mix (48 kHz interleaved stereo, the voice without music), with their times in
/// seconds. The sound goes to the recogniser in pieces of up to 50 s (16 kHz mono), on the device
/// first and Apple's servers when the device hears nothing; silent pieces are skipped. Blocks, so
/// call it off the UI thread.
pub fn transcribe_mix(mix: &[f32]) -> Result<Vec<(String, f64, f64)>, String> {
    let mono: Vec<f32> = mix.chunks_exact(6).map(|f| (f[0] + f[1] + f[2] + f[3] + f[4] + f[5]) / 6.0).collect();
    let rate = 16000usize;
    let mut words = vec![];
    for (i, part) in mono.chunks(50 * rate).enumerate() {
        if !part.iter().any(|v| v.abs() > 0.01) {
            continue;
        }
        let path = std::env::temp_dir().join(format!("liemo-captions-{}-{i}.wav", std::process::id()));
        write_wav16(&path, part, rate as u32).map_err(|e| e.to_string())?;
        let offset = (i * 50) as f64;
        let mut got = transcribe(&path, true).unwrap_or_default();
        if got.is_empty() {
            got = match transcribe(&path, false) {
                Ok(w) => w,
                // Nothing said in this piece.
                Err(e) if e.to_lowercase().contains("speech") || e.to_lowercase().contains("empty") => vec![],
                Err(e) => {
                    let _ = std::fs::remove_file(&path);
                    return Err(e);
                }
            };
        }
        let _ = std::fs::remove_file(&path);
        words.extend(got.into_iter().map(|(w, a, b)| (w, a + offset, b + offset)));
    }
    Ok(words)
}

fn write_wav16(path: &Path, s: &[f32], rate: u32) -> std::io::Result<()> {
    let mut out = Vec::with_capacity(44 + s.len() * 2);
    let len = (s.len() * 2) as u32;
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    for v in [16u32.to_le_bytes(), [1, 0, 1, 0], rate.to_le_bytes(), (rate * 2).to_le_bytes(), [2, 0, 16, 0]] {
        out.extend_from_slice(&v);
    }
    out.extend_from_slice(b"data");
    out.extend_from_slice(&len.to_le_bytes());
    for v in s {
        out.extend_from_slice(&((v.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    std::fs::write(path, out)
}

/// Each recognised word with its start and end, in seconds into the file.
fn transcribe(path: &Path, on_device: bool) -> Result<Vec<(String, f64, f64)>, String> {
    objc2::rc::autoreleasepool(|_| unsafe {
        // Asking for speech recognition without a reason in the app's Info.plist ends the process,
        // so a build run outside the app bundle stops here instead.
        if NSBundle::mainBundle().objectForInfoDictionaryKey(ns_string!("NSSpeechRecognitionUsageDescription")).is_none() {
            return Err("Auto captions need the Liemo app. They aren't available in a development build.".into());
        }
        let mut status = SFSpeechRecognizer::authorizationStatus();
        if status == SFSpeechRecognizerAuthorizationStatus::NotDetermined {
            let (tx, rx) = mpsc::channel();
            let tx = Mutex::new(Some(tx));
            let handler = RcBlock::new(move |s: SFSpeechRecognizerAuthorizationStatus| {
                if let Some(tx) = tx.lock().take() {
                    let _ = tx.send(s);
                }
            });
            SFSpeechRecognizer::requestAuthorization(&handler);
            status = rx.recv_timeout(Duration::from_secs(300)).unwrap_or(SFSpeechRecognizerAuthorizationStatus::Denied);
        }
        if status != SFSpeechRecognizerAuthorizationStatus::Authorized {
            return Err("Allow speech recognition for Liemo in System Settings › Privacy & Security › Speech Recognition, then try again.".into());
        }
        let rec = SFSpeechRecognizer::init(SFSpeechRecognizer::alloc()).ok_or("Speech recognition isn't available for your language.")?;
        if !rec.isAvailable() {
            return Err("Speech recognition isn't available right now. Try again in a moment.".into());
        }
        // Results arrive on a queue of their own: the main thread may be the one waiting.
        let queue = NSOperationQueue::new();
        rec.setQueue(&queue);
        let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
        let req = SFSpeechURLRecognitionRequest::initWithURL(SFSpeechURLRecognitionRequest::alloc(), &url);
        req.setShouldReportPartialResults(false);
        req.setAddsPunctuation(true);
        if on_device && !rec.supportsOnDeviceRecognition() {
            return Ok(vec![]);
        }
        req.setRequiresOnDeviceRecognition(on_device);
        let (tx, rx) = mpsc::channel::<Result<Vec<(String, f64, f64)>, String>>();
        let tx = Mutex::new(Some(tx));
        let handler = RcBlock::new(move |result: *mut SFSpeechRecognitionResult, error: *mut NSError| {
            let send = |r| {
                if let Some(tx) = tx.lock().take() {
                    let _ = tx.send(r);
                }
            };
            if let Some(e) = error.as_ref() {
                // Silence ends with an error rather than an empty result.
                let msg = e.localizedDescription().to_string();
                send(if msg.to_lowercase().contains("no speech") { Ok(vec![]) } else { Err(msg) });
                return;
            }
            let Some(r) = result.as_ref() else { return };
            if !r.isFinal() {
                return;
            }
            let segs = r.bestTranscription().segments();
            let words = (0..segs.count())
                .map(|i| {
                    let s: Retained<_> = segs.objectAtIndex(i);
                    (s.substring().to_string(), s.timestamp(), s.timestamp() + s.duration())
                })
                .filter(|w| !w.0.trim().is_empty())
                .collect();
            send(Ok(words));
        });
        let task = rec.recognitionTaskWithRequest_resultHandler(&req, &handler);
        let out = rx.recv_timeout(Duration::from_secs(1800)).unwrap_or_else(|_| Err("Speech recognition took too long.".into()));
        drop(task);
        out
    })
}
