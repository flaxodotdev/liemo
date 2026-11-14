import Foundation
import Speech

/// Word-level timing for lyrics that don't have it. The song is transcribed on the
/// device, and the recognized words anchor the real lyrics to real moments — even
/// songs with no synced lyrics anywhere on the web come out word-by-word. What comes
/// out is enhanced LRC (`[00:33.06] <00:33.06>Suena <00:33.61>tu …`), the same format
/// word-synced songs use, so everything downstream just works.
enum LyricAligner {

    struct Seg {
        let word: String
        let time: Double
        let dur: Double
    }

    struct Result {
        let lrc: String
        /// Matched lyric words / all lyric words — how much of the timing was heard.
        let coverage: Double
    }

    struct AlignError: LocalizedError {
        let message: String
        var errorDescription: String? { message }
    }

    /// Transcribe the song, then align. One locale is chosen up front: Spanish lyrics
    /// need a Spanish ear, otherwise the device's own language.
    static func align(url: URL, lyrics: String) async throws -> Result {
        let segs = try await transcript(url: url, wantSpanish: isSpanish(lyrics))
        guard let r = build(transcript: segs, lyrics: lyrics) else {
            throw AlignError(message: "Couldn't hear the words clearly enough. Try again or keep the line timing.")
        }
        if r.coverage < 0.3 {
            throw AlignError(message: "Only heard \(Int(r.coverage * 100))% of the words — keeping the old timing. Try the calibration tap instead.")
        }
        return r
    }

    /// Pure function: enhanced LRC from a transcript and any lyric text (plain or
    /// already stamped — existing tags are stripped first). Returns nil when there's
    /// nothing to anchor on.
    static func build(transcript: [Seg], lyrics: String) -> Result? {
        // Lyric words, line by line, without any existing stamps.
        let clean = lyrics
            .replacingOccurrences(of: "\\[[^\\]]*\\]", with: " ", options: .regularExpression)
            .replacingOccurrences(of: "<[^>]*>", with: " ", options: .regularExpression)
        let lines = clean.split(whereSeparator: \.isNewline).map { $0.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty }
        var lw: [(norm: String, raw: String, line: Int)] = []
        for (li, l) in lines.enumerated() {
            for w in l.split(separator: " ") {
                let n = norm(String(w))
                if !n.isEmpty { lw.append((n, String(w), li)) }
            }
        }
        guard !lw.isEmpty else { return nil }
        let tw = transcript.compactMap { s -> (String, Seg)? in
            let n = norm(s.word)
            return n.isEmpty ? nil : (n, s)
        }
        guard !tw.isEmpty else { return nil }

        // Longest common subsequence: lyric word i matched to heard word j.
        let n = lw.count, m = tw.count
        var dp = [[Int16]](repeating: [Int16](repeating: 0, count: m + 1), count: n + 1)
        for i in stride(from: n - 1, through: 0, by: -1) {
            for j in stride(from: m - 1, through: 0, by: -1) {
                dp[i][j] = lw[i].norm == tw[j].0 ? dp[i + 1][j + 1] + 1 : max(dp[i + 1][j], dp[i][j + 1])
            }
        }
        var times = [Double?](repeating: nil, count: n)
        var i = 0, j = 0
        while i < n, j < m {
            if lw[i].norm == tw[j].0, dp[i][j] == dp[i + 1][j + 1] + 1 {
                times[i] = tw[j].1.time
                i += 1; j += 1
            } else if dp[i + 1][j] >= dp[i][j + 1] {
                i += 1
            } else {
                j += 1
            }
        }
        let matched = times.compactMap { $0 }.count
        guard matched > 0 else { return nil }

        // Unheard words slide between their heard neighbours — a steady pace in between.
        var lastAnchor: (idx: Int, time: Double)?
        for k in 0..<n {
            if let t = times[k] { lastAnchor = (k, t); continue }
            let nxt = (k + 1..<n).first { times[$0] != nil }
            if let last = lastAnchor, let nx = nxt {
                let dt = (times[nx]! - last.time) / Double(nx - last.idx)
                times[k] = last.time + dt * Double(k - last.idx)
            } else if let nx = nxt {
                times[k] = max(0, times[nx]! - 0.25 * Double(nx - k))
            } else if let last = lastAnchor {
                times[k] = last.time + 0.25 * Double(k - last.idx)
            }
        }

        // Enhanced LRC, one line of lyrics per output line.
        var out: [String] = []
        var k = 0
        for (li, line) in lines.enumerated() {
            var words = [(t: Double, raw: String)]()
            while k < n, lw[k].line == li { words.append((times[k] ?? 0, lw[k].raw)); k += 1 }
            guard !words.isEmpty else { continue }
            let base = words[0].t
            var s = "[\(stampStr(base))] "
            for (w, word) in words.enumerated() where !word.raw.isEmpty {
                s += "<\(stampStr(max(word.t, base)))>\(word.raw) "
            }
            out.append(s.trimmingCharacters(in: .whitespaces))
        }
        guard !out.isEmpty else { return nil }
        return Result(lrc: out.joined(separator: "\n"), coverage: Double(matched) / Double(n))
    }

    // MARK: Transcription

    private static func isSpanish(_ s: String) -> Bool {
        s.range(of: "[¿¡ñáéíóúü]", options: [.regularExpression, .caseInsensitive]) != nil
    }

    private static func transcript(url: URL, wantSpanish: Bool) async throws -> [Seg] {
        let status = await withCheckedContinuation { c in
            SFSpeechRecognizer.requestAuthorization { s in c.resume(returning: s) }
        }
        guard status == .authorized else {
            throw AlignError(message: "Speech recognition isn't allowed. Enable it in Settings → Liemo.")
        }
        let locale = Locale(identifier: wantSpanish ? "es-419" : Locale.current.identifier)
        guard let rec = SFSpeechRecognizer(locale: locale) ?? SFSpeechRecognizer(locale: Locale(identifier: "en-US")) else {
            throw AlignError(message: "No speech recognizer for that language on this device.")
        }
        let req = SFSpeechURLRecognitionRequest(url: url)
        req.shouldReportPartialResults = false
        req.addsPunctuation = false
        req.taskHint = .search

        return try await withCheckedThrowingContinuation { cont in
            var latest: SFSpeechRecognitionResult?
            rec.recognitionTask(with: req) { result, error in
                if let error {
                    cont.resume(throwing: error)
                    return
                }
                if let result {
                    latest = result
                    if result.isFinal {
                        let segs = result.bestTranscription.segments.map {
                            Seg(word: $0.substring, time: $0.timestamp, dur: $0.duration)
                        }
                        cont.resume(returning: segs)
                    }
                }
            }
            _ = latest // results accumulate; the final one has every segment
        }
    }

    private static func norm(_ w: String) -> String {
        String(w.folding(options: [.diacriticInsensitive, .caseInsensitive], locale: nil)
            .filter { $0.isLetter || $0.isNumber })
    }

    private static func stampStr(_ t: Double) -> String {
        let s = max(0, t)
        let m = Int(s / 60)
        return String(format: "%02d:%05.2f", m, s - Double(m) * 60)
    }
}
