import Foundation

/// Everything the quick reel screen collects.
struct ReelPlan {
    var media: [URL] = []
    var song: URL?
    var songName = ""
    var vibe: Vibe = .spedUp
    var length: Double = 15
    var words = ""
    /// Synced lyrics, in the song's own time. Empty means fall back to `words`, spread on the beat.
    var lyrics: [LyricLine] = []
    /// Captions on the reel at all: the song's lyrics when it has them, else `words`.
    var captions = true
    /// Words pop in one at a time (the song's stamps, or spread across the line) rather than
    /// whole lines at once.
    var wordByWord = true
    /// The caption face and look.
    var font = "Bebas Neue"
    var style: CaptionStyle = .poster
    /// Where the song starts in its own file: nil lets the drop detector pick.
    var songStart: Double?
    /// Where each piece of footage starts in its own file; a nil entry picks moments for you.
    var videoStarts: [Double?] = []
    /// How high on the frame the captions sit.
    var placement: CaptionPlacement = .low
    /// How captions line up horizontally.
    var align: CaptionAlign = .center
    /// How big the captions are.
    var size: CaptionSize = .medium
}

/// Vertical placement of the captions.
enum CaptionPlacement: String, CaseIterable, Identifiable {
    case top, middle, low
    var id: String { rawValue }
    var name: String {
        switch self {
        case .top: "Top"
        case .middle: "Middle"
        case .low: "Low"
        }
    }
    var y: Double {
        switch self {
        case .top: -250
        case .middle: 0
        case .low: 210
        }
    }
}

/// Horizontal alignment of the captions.
enum CaptionAlign: String, CaseIterable, Identifiable {
    case left, center, right
    var id: String { rawValue }
    var name: String {
        switch self {
        case .left: "Left"
        case .center: "Center"
        case .right: "Right"
        }
    }
}

/// Caption size: a factor on the size the layout picks for each line.
enum CaptionSize: String, CaseIterable, Identifiable {
    case small, medium, big
    var id: String { rawValue }
    var name: String {
        switch self {
        case .small: "Small"
        case .medium: "Medium"
        case .big: "Big"
        }
    }
    var factor: Double {
        switch self {
        case .small: 0.72
        case .medium: 0.88
        case .big: 1.12
        }
    }
}

/// How the lyric captions look: a preset of face, colours and entrance.
enum CaptionStyle: String, CaseIterable, Identifiable {
    case poster, clean, neon, glass, invert, marker
    var id: String { rawValue }
    var name: String {
        switch self {
        case .poster: "Poster"
        case .clean: "Clean"
        case .neon: "Neon"
        case .glass: "Liquid glass"
        case .invert: "Invert"
        case .marker: "Marker"
        }
    }
}

/// Builds a 9:16 reel from a plan with the engine's tools: the song reworked and cut to its drop,
/// the footage cut on the beat with hard transitions and hits, and the words popping in on the
/// beat.
final class ReelBuilder: @unchecked Sendable {
    private let core = Core.shared

    func build(_ plan: ReelPlan, step: @escaping @Sendable (String) -> Void) throws -> String {
        guard let song = plan.song else { throw CoreError(message: "Pick a song.") }
        guard !plan.media.isEmpty else { throw CoreError(message: "Pick some photos or videos.") }

        // 1. The song: find the drop, rework from there — unless the user picked the moment.
        step(plan.vibe == .original ? "Finding the drop…" : "Making it \(plan.vibe.name.lowercased())…")
        let raw = core.host.decodeAudio(path: song.path) ?? []
        let rawDuration = Double(raw.count) / 2 / 48000
        let window = plan.length * plan.vibe.rate
        let start = plan.songStart.map { min($0, max(0, rawDuration - window)) } ?? SongFX.dropStart(raw, window: window)
        // Named for the timeline ("Hitman (sped up)"), in a folder of its own so names never clash.
        let folder = core.mediaDir.appendingPathComponent("reel-\(UUID().uuidString.prefix(8))", isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let label = plan.vibe == .original ? plan.songName : "\(plan.songName) (\(plan.vibe.name.lowercased()))"
        let audio = folder.appendingPathComponent(label.replacingOccurrences(of: "/", with: "-") + ".m4a")
        try SongFX.render(source: song, start: start, length: plan.length, vibe: plan.vibe, to: audio)

        step("Finding the beat…")
        // Tempo from a minute of the original around the drop (short clips fool it), scaled by
        // the vibe's speed and kept in cutting range; the beat grid's phase from the result.
        let from = Int(start * 48000) * 2
        let minute = raw.count > from ? Array(raw[from..<min(raw.count, from + 60 * 48000 * 2)]) : raw
        var tempo = SongFX.beats(minute).bpm * plan.vibe.rate
        while tempo > 175 { tempo /= 2 }
        while tempo < 80 { tempo *= 2 }
        let mix = core.host.decodeAudio(path: audio.path) ?? []
        let (bpm, first) = SongFX.beats(mix, near: tempo)
        let beat = 60 / bpm
        step(String(format: "Cutting at %.0f BPM…", bpm))

        // 2. A project with the media in its pool.
        let name = plan.words.split(separator: "\n").first.map(String.init)?.trimmingCharacters(in: .whitespaces) ?? ""
        let made = try core.decode(Created.self, "create_project", [
            "format": "reel", "name": name.isEmpty ? "Reel · \(plan.songName)" : String(name.prefix(40)),
        ])
        let id = made.id
        let opened = core.engine.openProject(id: id)
        guard opened.ok else { throw CoreError(message: opened.text) }
        defer { core.engine.closeProject() }

        let imported = try core.decode(Imported.self, "import_media", [
            "project": id, "paths": plan.media.map(\.path) + [audio.path], "addToTimeline": false,
        ])
        let visuals = imported.media.filter { $0.type != "audio" }
        guard let sound = imported.media.first(where: { $0.type == "audio" }), !visuals.isEmpty else {
            throw CoreError(message: imported.failed?.first?.error ?? "Those files couldn't be read.")
        }

        // 3. Cuts on the beat: mostly every two beats, with one-beat flurries before the hits.
        var cuts = [0.0]
        var t = first > 0.08 ? first : first + beat
        var i = 0
        let fast = bpm > 140
        while t < plan.length - 0.05 {
            cuts.append(t)
            let flurry = i % 8 >= 6
            t += beat * (fast ? (flurry ? 1 : 2) : (flurry ? 0.5 : 1)) * (bpm < 95 ? 2 : 1)
            i += 1
        }
        cuts.append(plan.length)

        let look = Looks.of(plan.vibe)
        var clips: [String] = []
        var cursor: [String: Double] = [:]   // how far into each video we've used
        for k in 0..<(cuts.count - 1) {
            let (a, b) = (cuts[k], cuts[k + 1])
            guard b - a > 0.04 else { continue }
            let m = visuals[k % visuals.count]
            var args: [String: Any] = ["project": id, "asset": m.id, "track": "V1", "start": a, "duration": b - a]
            if m.type == "video" {
                // A start the user picked for this clip, else moments chosen for them.
                let wanted = k < plan.videoStarts.count ? plan.videoStarts[k] ?? -1 : -1
                if wanted >= 0 {
                    let inPoint = min(wanted, max(0, m.duration - (b - a)))
                    args["in"] = inPoint
                    cursor[m.id] = inPoint + (b - a)
                } else {
                    let used = cursor[m.id] ?? 0
                    let room = max(0, m.duration - (b - a))
                    let inPoint = room > 0 ? (used + Double.random(in: 0...1.5)).truncatingRemainder(dividingBy: room) : 0
                    args["in"] = inPoint
                    cursor[m.id] = inPoint + (b - a)
                }
            }
            let r = core.call("add_clip", args)
            guard r.ok, let cid = clipId(r.text) else { continue }
            clips.append(cid)
            core.call("apply_effect", ["project": id, "clip": cid, "effect": look.grade.0, "amount": look.grade.1])
            if k % 4 == 3 {
                let hit = look.hits[(k / 4) % look.hits.count]
                core.call("apply_effect", ["project": id, "clip": cid, "effect": hit.0, "amount": hit.1])
            }
            if k > 0 && k % 2 == 0 {
                let tr = look.transitions[(k / 2) % look.transitions.count]
                core.call("set_transition", ["project": id, "clip": cid, "transition": tr, "duration": min(look.transitionLength, (b - a) * 0.6)])
            }
        }

        // 4. The song.
        let s = core.call("add_clip", ["project": id, "asset": sound.id, "track": "A1", "start": 0, "duration": plan.length])
        if let sid = clipId(s.text) {
            core.call("update_clip", ["project": id, "clip": sid, "fadeIn": 0.05, "fadeOut": 0.6])
        }

        // 5. Words popping in on the beat. Synced lyrics go on their real moments (the song
        // starts at `start` and plays at the vibe's rate, so a line at time t lands at
        // (t - start) / rate); otherwise the words spread out, each held a whole number of beats.
        if !plan.lyrics.isEmpty && plan.captions {
            step("Adding the lyrics…")
            let sizeF = sizeFactor(plan.font)
            let lines = plan.lyrics.map { l in
                LyricLine(time: (l.time - start) / plan.vibe.rate, text: l.text,
                          words: l.words.map { ws in ws.map { LyricWord(time: ($0.time - start) / plan.vibe.rate, text: $0.text) } })
            }
            // Lines sung before the drop never show; one that runs over it starts at the top.
            var shown = lines.indices.filter { i in
                (i + 1 < lines.count ? lines[i + 1].time : .infinity) > 0.15
            }.map { lines[$0] }
            if !shown.isEmpty { shown[0].time = max(shown[0].time, 0.05) }
            var last = 0.0
            for (n, l) in shown.enumerated() {
                let at = max(l.time, last + 0.05)
                let next = n + 1 < shown.count ? max(shown[n + 1].time, at + 0.05) : plan.length
                let until = min(next, plan.length) - 0.05
                guard until - at > 0.3 else { continue }
                // One size for the whole line so word-by-word captions don't bounce around.
                let size = min(118, 1150 / Double(max(4, l.text.count))) * sizeF
                // Word-by-word pops: the song's own word stamps when it has them, otherwise
                // the line's words spread across the line's span — longer words held longer.
                // Every word pops in, holds to the next word's moment, and is replaced.
                var pops: [LyricWord] = []
                if plan.wordByWord {
                    if let stamped = l.words, stamped.count > 1 {
                        pops = stamped
                    } else {
                        let toks = l.text.split(separator: " ").filter { !$0.isEmpty }
                        if toks.count > 1, until - at > 0.6 {
                            let total = Double(toks.reduce(0) { $0 + max(1, $1.count) })
                            var acc = at
                            for tok in toks {
                                pops.append(LyricWord(time: acc, text: String(tok)))
                                acc += (until - at) * Double(max(1, tok.count)) / total
                            }
                        }
                    }
                }
                if pops.count > 1 {
                    // Word pops are sized by the longest word in the line, not the whole
                    // line — that's why short words looked tiny before.
                    let longest = pops.map { $0.text.count }.max() ?? 4
                    let wSize = min(150, 1600 / Double(max(3, longest))) * sizeF
                    for (w, word) in pops.enumerated() {
                        let wAt = max(word.time, at)
                        guard wAt < until else { break }
                        let wNext = w + 1 < pops.count ? max(pops[w + 1].time, wAt + 0.08) : until
                        core.call("add_text", lyric(word.text, project: id, size: wSize, at: wAt, dur: min(wNext, until) - wAt, accent: n % 3 == 2, font: plan.font, style: plan.style, place: plan.placement, align: plan.align, sizeK: plan.size.factor))
                    }
                } else {
                    core.call("add_text", lyric(l.text, project: id, size: size, at: at, dur: until - at, accent: n % 3 == 2, font: plan.font, style: plan.style, place: plan.placement, align: plan.align, sizeK: plan.size.factor))
                }
                last = at
            }
        } else {
            let text = plan.words.trimmingCharacters(in: .whitespacesAndNewlines)
            if !text.isEmpty {
                let lines = text.split(separator: "\n").map { $0.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty }
                if lines.count > 1 {
                    // Pasted plain lyrics: the reel starts partway into the song, so start the
                    // words partway into the lyrics too. Songs don't sing at the very start or
                    // end, so the drop's fraction maps into the ~8–90% stretch where words live.
                    step("Adding the lyrics…")
                    let duration = Double(raw.count) / 2 / 48000
                    let f = ((start / max(duration, 1)) - 0.08) / 0.82
                    let skip = min(Double(lines.count - 1), max(0, f) * Double(lines.count))
                    let shown = Array(lines.dropFirst(Int(skip)))
                    var at = max(first > 0.08 ? first : first + beat, 0.05)
                    let each = min(beat * 4, max(beat, (plan.length - at) / Double(shown.count)))
                    let sizeF = sizeFactor(plan.font)
                    for (n, w) in shown.enumerated() {
                        guard at < plan.length - 0.1 else { break }
                        let size = min(140, 1400 / Double(max(4, w.count))) * sizeF
                        core.call("add_text", lyric(w, project: id, size: size, at: at, dur: min(each, plan.length - at), accent: n % 3 == 2, font: plan.font, style: plan.style, place: plan.placement, align: plan.align, sizeK: plan.size.factor))
                        at += each
                    }
                } else {
                    // One caption: its words spread over the reel, each held a whole number
                    // of beats (one to four).
                    let words = chunks(text)
                    var at = first > 0.08 ? first : first + beat
                    let room = (plan.length - at) / beat
                    let perWord = Double(min(4, max(bpm > 140 ? 2 : 1, Int(room / Double(words.count)))))
                    let each = beat * perWord
                    let sizeF = sizeFactor(plan.font)
                    for (n, w) in words.enumerated() {
                        guard at < plan.length - 0.1 else { break }
                        let size = min(140, 1400 / Double(max(4, w.count))) * sizeF
                        core.call("add_text", lyric(w, project: id, size: size, at: at, dur: min(each, plan.length - at), accent: n % 3 == 2, font: plan.font, style: plan.style, place: plan.placement, align: plan.align, sizeK: plan.size.factor))
                        at += each
                    }
                }
            }
        }
        step("Done")
        return id
    }

    /// One or two words per caption: short words ride along with the next one.
    private func chunks(_ text: String) -> [String] {
        let words = text.split(whereSeparator: \.isWhitespace).map(String.init)
        var out: [String] = []
        var i = 0
        while i < words.count {
            if words[i].count <= 3, i + 1 < words.count, words[i + 1].count <= 7 {
                out.append(words[i] + " " + words[i + 1])
                i += 2
            } else {
                out.append(words[i])
                i += 1
            }
        }
        return out
    }

    /// Lyric captions: the face and look the user picked. Poster is the classic reel look —
    /// Bebas Neue in caps with a dark outline — and the others trade that for a cleaner,
    /// glowing, glassy, inverted or highlighted style. Yellow/orange accents every third caption.
    private func lyric(_ content: String, project: String, size: Double, at: Double, dur: Double, accent: Bool, font: String, style: CaptionStyle, place: CaptionPlacement, align: CaptionAlign, sizeK: Double) -> [String: Any] {
        var a: [String: Any] = [
            "project": project, "content": content.uppercased(), "start": at, "duration": dur,
            "font": font, "weight": weight(of: font), "size": size * sizeK, "y": place.y,
            "align": align.rawValue, "animation": "pop",
        ]
        switch style {
        case .poster:
            a["color"] = accent ? "#ffd400" : "#ffffff"
            a["outline"] = "#000000"
            a["outlineWidth"] = 5
        case .clean:
            a["color"] = "#ffffff"
            a["shadow"] = "#000000"
        case .neon:
            a["color"] = "#ffffff"
            a["glow"] = accent ? "#00e5ff" : "#ff2bd6"
        case .glass:
            a["color"] = "#ffffff"
            a["glassText"] = "#ffffff"
        case .invert:
            a["color"] = "#ffffff"
            a["blend"] = "invert"
        case .marker:
            a["color"] = "#111111"
            a["highlight"] = accent ? "#ff7a3d" : "#ffd400"
        }
        return a
    }

    /// Captions are sized for Bebas's condensed faces; wider faces get scaled down to fit.
    private func sizeFactor(_ font: String) -> Double {
        switch font {
        case "Bebas Neue": 1.0
        case "Instrument Serif": 0.88
        case "Caveat": 0.92
        case "JetBrains Mono": 0.72
        default: 0.8
        }
    }

    private func weight(of font: String) -> Double {
        switch font {
        case "Bebas Neue", "Instrument Serif": 400
        case "JetBrains Mono": 500
        case "Caveat": 700
        default: 800
        }
    }

    private func clipId(_ json: String) -> String? {
        let j = try? JSONSerialization.jsonObject(with: Data(json.utf8)) as? [String: Any]
        return (j?["clip"] as? [String: Any])?["id"] as? String
    }
}

private struct Imported: Decodable {
    struct Media: Decodable { let id: String; let type: String; let duration: Double }
    struct Failed: Decodable { let error: String }
    let media: [Media]
    let failed: [Failed]?
}

/// The grade, the hits and the transitions for each vibe.
private struct Looks {
    let grade: (String, Double)
    let hits: [(String, Double)]
    let transitions: [String]
    let transitionLength: Double

    static func of(_ v: Vibe) -> Looks {
        switch v {
        case .spedUp:
            return Looks(grade: ("ig-punchy", 0.7),
                         hits: [("rgbsplit", 0.8), ("shake", 0.7), ("zoom-blur", 0.6), ("glitch", 0.7)],
                         transitions: ["whip", "remocn-glitch-cut", "grid-flash-4x4-random", "zoom", "spin", "remocn-zoom-blur", "push-left"],
                         transitionLength: 0.25)
        case .slowed:
            return Looks(grade: ("ig-moody", 0.75),
                         hits: [("glow", 0.6), ("vhs", 0.6), ("rgbsplit", 0.5), ("dream", 0.6)],
                         transitions: ["blur", "remocn-smear-pan", "dissolve", "remocn-focus-pull", "remocn-warp-dissolve"],
                         transitionLength: 0.5)
        case .original:
            return Looks(grade: ("ig-teal-orange", 0.6),
                         hits: [("rgbsplit", 0.7), ("shake", 0.6), ("zoom-blur", 0.5)],
                         transitions: ["whip", "zoom", "grid-flash", "push-up"],
                         transitionLength: 0.3)
        }
    }
}
