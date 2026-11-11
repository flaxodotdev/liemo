// Captions: subtitle files in and out (SubRip .srt and WebVTT .vtt), and recognised words grouped
// into short on-screen captions. Captions live on the timeline as ordinary text clips on Titles.

use super::types::{AssetType, Clip};

#[derive(Clone, Debug, PartialEq)]
pub struct Cue {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

pub fn is_subtitle_file(path: &std::path::Path) -> bool {
    matches!(path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref(), Some("srt" | "vtt"))
}

/// "00:01:02,345", "01:02.345" or "62.3" → seconds.
fn parse_time(s: &str) -> Option<f64> {
    let s = s.trim().replace(',', ".");
    let parts: Vec<&str> = s.split(':').collect();
    let mut secs = 0.0;
    for p in &parts {
        secs = secs * 60.0 + p.trim().parse::<f64>().ok()?;
    }
    Some(secs)
}

/// Read SubRip or WebVTT. Cue settings, numbering, notes, styles and inline tags are dropped.
pub fn parse(src: &str) -> Vec<Cue> {
    let src = src.trim_start_matches('\u{feff}').replace("\r\n", "\n").replace('\r', "\n");
    let mut out = vec![];
    for block in src.split("\n\n") {
        let lines: Vec<&str> = block.lines().map(str::trim_end).filter(|l| !l.trim().is_empty()).collect();
        let Some(ti) = lines.iter().position(|l| l.contains("-->")) else { continue };
        let (a, b) = lines[ti].split_once("-->").unwrap();
        // WebVTT puts cue settings after the end time ("00:01.000 align:start").
        let b = b.split_whitespace().next().unwrap_or("");
        let (Some(start), Some(end)) = (parse_time(a), parse_time(b)) else { continue };
        let text = lines[ti + 1..].iter().map(|l| strip_tags(l)).collect::<Vec<_>>().join("\n");
        let text = text.trim().to_string();
        if !text.is_empty() && end > start {
            out.push(Cue { start, end, text });
        }
    }
    out.sort_by(|a, b| a.start.total_cmp(&b.start));
    out
}

fn strip_tags(s: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for ch in s.chars() {
        match ch {
            '<' => depth += 1,
            '>' if depth > 0 => depth -= 1,
            '{' if s.contains("{\\") => depth += 1,
            '}' if depth > 0 => depth -= 1,
            _ if depth == 0 => out.push(ch),
            _ => {}
        }
    }
    out.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&nbsp;", " ")
}

fn stamp(t: f64, sep: char) -> String {
    let ms = (t.max(0.0) * 1000.0).round() as u64;
    format!("{:02}:{:02}:{:02}{sep}{:03}", ms / 3_600_000, (ms / 60_000) % 60, (ms / 1000) % 60, ms % 1000)
}

pub fn to_srt(cues: &[Cue]) -> String {
    cues.iter().enumerate().map(|(i, c)| format!("{}\n{} --> {}\n{}\n", i + 1, stamp(c.start, ','), stamp(c.end, ','), c.text)).collect::<Vec<_>>().join("\n")
}

pub fn to_vtt(cues: &[Cue]) -> String {
    let body: Vec<String> = cues.iter().map(|c| format!("{} --> {}\n{}\n", stamp(c.start, '.'), stamp(c.end, '.'), c.text)).collect();
    format!("WEBVTT\n\n{}", body.join("\n"))
}

/// The edit's titles as cues, in time order.
pub fn cues_from_clips(clips: &[Clip]) -> Vec<Cue> {
    let mut v: Vec<Cue> = clips
        .iter()
        .filter(|c| c.kind == AssetType::Text)
        .filter_map(|c| c.text.as_ref().map(|t| Cue { start: c.start, end: c.start + c.dur, text: t.content.trim().to_string() }))
        .filter(|c| !c.text.is_empty())
        .collect();
    v.sort_by(|a, b| a.start.total_cmp(&b.start));
    v
}

/// Words (text, start, end) → captions of up to three words and about 16 letters, split at
/// pauses; each holds until the next starts unless there's a long pause. The iPhone app's rule.
pub fn group_words(words: &[(String, f64, f64)]) -> Vec<Cue> {
    let mut out: Vec<Cue> = vec![];
    let mut cur: Vec<&(String, f64, f64)> = vec![];
    let flush = |cur: &mut Vec<&(String, f64, f64)>, out: &mut Vec<Cue>| {
        if let (Some(f), Some(l)) = (cur.first(), cur.last()) {
            out.push(Cue { start: f.1, end: l.2, text: cur.iter().map(|w| w.0.as_str()).collect::<Vec<_>>().join(" ") });
        }
        cur.clear();
    };
    for w in words {
        let len: usize = cur.iter().map(|w| w.0.chars().count()).sum::<usize>() + w.0.chars().count();
        if let Some(l) = cur.last() {
            if cur.len() >= 3 || len > 16 || w.1 - l.2 > 0.35 {
                flush(&mut cur, &mut out);
            }
        }
        cur.push(w);
    }
    flush(&mut cur, &mut out);
    for i in 1..out.len() {
        if out[i].start - out[i - 1].end < 0.6 {
            out[i - 1].end = out[i].start;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reads_srt_and_vtt() {
        let srt = "1\r\n00:00:01,000 --> 00:00:02,500\r\n<i>Hello</i> there\r\n\r\n2\r\n00:00:03,000 --> 00:00:04,000\r\nSecond\r\nline\r\n";
        let c = parse(srt);
        assert_eq!(c.len(), 2);
        assert_eq!(c[0], Cue { start: 1.0, end: 2.5, text: "Hello there".into() });
        assert_eq!(c[1].text, "Second\nline");
        let vtt = "WEBVTT\n\nNOTE hi\n\nintro\n00:01.000 --> 00:02.000 align:start\nHey\n";
        assert_eq!(parse(vtt), vec![Cue { start: 1.0, end: 2.0, text: "Hey".into() }]);
        assert_eq!(parse(&to_srt(&c)), c);
        assert_eq!(parse(&to_vtt(&c)), c);
    }
    #[test]
    fn groups_words() {
        let w = |s: &str, a: f64, b: f64| (s.to_string(), a, b);
        let g = group_words(&[w("one", 0.0, 0.2), w("two", 0.25, 0.4), w("three", 0.45, 0.6), w("four", 0.65, 0.8), w("later", 3.0, 3.4)]);
        assert_eq!(g.iter().map(|c| c.text.as_str()).collect::<Vec<_>>(), ["one two three", "four", "later"]);
        assert_eq!(g[0].end, 0.65);
        assert_eq!(g[1].end, 0.8);
    }
}
