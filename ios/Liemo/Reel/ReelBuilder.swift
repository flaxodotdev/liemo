import Foundation

/// Everything the quick reel screen collects.
struct ReelPlan {
    var media: [URL] = []
    var song: URL?
    var songName = ""
    var vibe: Vibe = .spedUp
    var length: Double = 15
    var words = ""
}

/// Builds a 9:16 reel from a plan with the engine's tools: the song reworked and cut to its drop,
/// the footage cut on the beat with hard transitions and hits, and the words popping in on the
/// beat.
final class ReelBuilder: @unchecked Sendable {
    private let core = Core.shared

    func build(_ plan: ReelPlan, step: @escaping @Sendable (String) -> Void) throws -> String {
        guard let song = plan.song else { throw CoreError(message: "Pick a song.") }
        guard !plan.media.isEmpty else { throw CoreError(message: "Pick some photos or videos.") }

        // 1. The song: find the drop, rework from there.
        step(plan.vibe == .original ? "Finding the drop…" : "Making it \(plan.vibe.name.lowercased())…")
        let raw = core.host.decodeAudio(path: song.path) ?? []
        let start = SongFX.dropStart(raw, window: plan.length * plan.vibe.rate)
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
                let used = cursor[m.id] ?? 0
                let room = max(0, m.duration - (b - a))
                let inPoint = room > 0 ? (used + Double.random(in: 0...1.5)).truncatingRemainder(dividingBy: room) : 0
                args["in"] = inPoint
                cursor[m.id] = inPoint + (b - a)
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

        // 5. Words popping in on the beat.
        let words = chunks(plan.words)
        if !words.isEmpty {
            step("Adding the words…")
            var at = first > 0.08 ? first : first + beat
            // Spread the words over the reel, each held a whole number of beats (one to four).
            let room = (plan.length - at) / beat
            let perWord = Double(min(4, max(bpm > 140 ? 2 : 1, Int(room / Double(words.count)))))
            let each = beat * perWord
            for (n, w) in words.enumerated() {
                guard at < plan.length - 0.1 else { break }
                let size = min(118, 1150 / Double(max(4, w.count)))
                core.call("add_text", [
                    "project": id, "content": w, "start": at, "duration": min(each, plan.length - at),
                    "font": "Mona Sans", "weight": 800, "size": size, "y": 210,
                    "color": n % 3 == 2 ? "#ffd400" : "#ffffff", "align": "center", "animation": "pop", "box": false,
                ])
                at += each
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
                         transitions: ["whip", "remocn-glitch-cut", "zoom", "spin", "remocn-zoom-blur", "push-left"],
                         transitionLength: 0.25)
        case .slowed:
            return Looks(grade: ("ig-moody", 0.75),
                         hits: [("glow", 0.6), ("vhs", 0.6), ("rgbsplit", 0.5), ("dream", 0.6)],
                         transitions: ["blur", "remocn-smear-pan", "dissolve", "remocn-focus-pull", "remocn-warp-dissolve"],
                         transitionLength: 0.5)
        case .original:
            return Looks(grade: ("ig-teal-orange", 0.6),
                         hits: [("rgbsplit", 0.7), ("shake", 0.6), ("zoom-blur", 0.5)],
                         transitions: ["whip", "zoom", "glitch", "push-up"],
                         transitionLength: 0.3)
        }
    }
}
