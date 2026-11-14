import Foundation
import Observation

/// One word of a line, with its own moment — from word-synced (enhanced) LRC.
struct LyricWord: Codable, Hashable {
    var time: Double
    let text: String
}

/// One line of a song at a moment in it, straight from synced lyrics (LRC).
struct LyricLine: Codable, Hashable {
    var time: Double
    let text: String
    /// Per-word stamps when the song is word-synced; nil for line-synced lyrics.
    var words: [LyricWord]? = nil
}

/// A song you brought in yourself, kept in the directory so it's one tap next time. Synced
/// lyrics ride along, so picking the song fills the words and they land on their real moments.
struct SavedSong: Identifiable, Codable, Hashable {
    let id: String
    var title: String
    var artist: String
    var file: String            // file name inside the songs folder
    var synced: String?         // the LRC text as fetched
    var plain: String?
    var added: Double
    /// Seconds to shift every lyric stamp by, set from the timing calibrator. A positive
    /// value moves the words later in the song. Saved per song, so it survives restarts.
    var offset: Double

    init(id: String, title: String, artist: String, file: String,
         synced: String?, plain: String?, added: Double, offset: Double = 0) {
        self.id = id; self.title = title; self.artist = artist; self.file = file
        self.synced = synced; self.plain = plain; self.added = added; self.offset = offset
    }

    // Older songs.json files have no offset; decoding tolerates that instead of losing them.
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        title = try c.decode(String.self, forKey: .title)
        artist = try c.decode(String.self, forKey: .artist)
        file = try c.decode(String.self, forKey: .file)
        synced = try c.decodeIfPresent(String.self, forKey: .synced)
        plain = try c.decodeIfPresent(String.self, forKey: .plain)
        added = try c.decode(Double.self, forKey: .added)
        offset = try c.decodeIfPresent(Double.self, forKey: .offset) ?? 0
    }

    var url: URL? {
        let f = SongLibrary.songsDir.appendingPathComponent(file)
        return (try? f.checkResourceIsReachable()) == true ? f : nil
    }
    var hasSynced: Bool { !(synced ?? "").isEmpty }
    var lines: [LyricLine] { SongLibrary.parseLRC(synced ?? "") }
    /// True when the synced lyrics carry per-word stamps (enhanced LRC).
    var wordSynced: Bool { lines.first?.words?.isEmpty == false }
    var plainText: String {
        if let plain, !plain.isEmpty { return plain }
        return lines.map(\.text).joined(separator: "\n")
    }
}

/// The song directory: everything you imported with lyrics, in a JSON file next to the
/// audio files. Bundled songs stay in Songs.swift; this is only yours.
@MainActor @Observable
final class SongLibrary {
    static let shared = SongLibrary()
    static nonisolated var songsDir: URL {
        let d = Core.shared.dataDir.appendingPathComponent("songs", isDirectory: true)
        try? FileManager.default.createDirectory(at: d, withIntermediateDirectories: true)
        return d
    }

    private(set) var songs: [SavedSong] = []
    private let file = Core.shared.dataDir.appendingPathComponent("songs.json")

    private init() { load() }

    private func load() {
        guard let data = try? Data(contentsOf: file),
              let saved = try? JSONDecoder().decode([SavedSong].self, from: data) else { return }
        songs = saved
    }

    private func save() {
        let data = (try? JSONEncoder().encode(songs)) ?? Data()
        try? data.write(to: file, options: .atomic)
    }

    /// Add a song, copying its audio into the songs folder. Replaces nothing: the same song
    /// recognised twice is two entries; the library screen is where duplicates get cleaned.
    @discardableResult
    func add(url: URL, title: String, artist: String, synced: String?, plain: String?) -> SavedSong {
        let ext = url.pathExtension.isEmpty ? "m4a" : url.pathExtension.lowercased()
        let name = "\(UUID().uuidString.prefix(8)).\(ext)"
        var copy = url
        if url.deletingLastPathComponent() != Self.songsDir {
            copy = Self.songsDir.appendingPathComponent(name)
            try? FileManager.default.removeItem(at: copy)
            try? FileManager.default.copyItem(at: url, to: copy)
        }
        let song = SavedSong(id: name, title: title, artist: artist,
                             file: copy.lastPathComponent, synced: synced, plain: plain,
                             added: Date().timeIntervalSince1970)
        songs.insert(song, at: 0)
        save()
        return song
    }

    func update(_ song: SavedSong) {
        if let i = songs.firstIndex(where: { $0.id == song.id }) { songs[i] = song }
        save()
    }

    func remove(_ song: SavedSong) {
        songs.removeAll { $0.id == song.id }
        save()
    }

    // MARK: LRC

    /// `[01:23.45] words`, also `[01:23]` and several stamps in front of one line. An
    /// `[offset: ±ms]` tag shifts every stamp that follows it. Word-synced lines carry
    /// `<00:13.50>` stamps before each word — parsed into per-word moments so captions
    /// can pop word by word. Notes and symbols the caption font can't draw (`♪`, `★`, …)
    /// are stripped so they never show up as odd boxes on screen.
    static nonisolated func parseLRC(_ text: String) -> [LyricLine] {
        var out: [LyricLine] = []
        var offset = 0.0
        for raw in text.split(whereSeparator: \.isNewline) {
            var stamps: [Double] = []
            var rest = Substring(raw)
            while rest.first == "[" {
                guard let close = rest.firstIndex(of: "]") else { break }
                let tag = rest[rest.index(after: rest.startIndex)..<close]
                if tag.hasPrefix("offset"), let ms = Double(tag.dropFirst("offset".count).trimmingCharacters(in: CharacterSet(charactersIn: ": ="))) {
                    offset = ms / 1000
                    rest = rest[rest.index(after: close)...]
                } else if let t = stamp(tag) {
                    stamps.append(t)
                    rest = rest[rest.index(after: close)...]
                } else { break }
            }
            let line = rest.trimmingCharacters(in: .whitespaces)
            guard !stamps.isEmpty, !line.isEmpty, line != "♪" else { continue }

            // Word-level stamps: `<00:13.50>¿Terapia <00:13.80>pa <00:14.01>qué?`
            var words: [LyricWord]? = nil
            var display = line
            if line.contains("<") {
                var at = stamps[0]
                var parts: [(Double, String)] = []
                var scan = Substring(line)
                while let open = scan.firstIndex(of: "<") {
                    let head = String(scan[..<open]).trimmingCharacters(in: .whitespaces)
                    if !head.isEmpty { parts.append((at, head)) }
                    guard let close = scan.firstIndex(of: ">") else { break }
                    if let t = stamp(scan[scan.index(after: open)..<close]) { at = t }
                    scan = scan[scan.index(after: close)...]
                }
                let tail = String(scan).trimmingCharacters(in: .whitespaces)
                if !tail.isEmpty { parts.append((at, tail)) }
                if parts.count > 1 {
                    // A "word" that was only symbols sanitizes away to nothing — skip it.
                    words = parts.compactMap { p in
                        let t = sanitize(p.1)
                        return t.isEmpty ? nil : LyricWord(time: p.0, text: t)
                    }
                    display = parts.map { $0.1 }.joined(separator: " ")
                }
            }
            display = sanitize(display)
            let shifted = words.map { ws in ws.map { LyricWord(time: max(0, $0.time - offset), text: $0.text) } }
            for t in stamps { out.append(LyricLine(time: max(0, t - offset), text: display, words: shifted)) }
        }
        return out.sorted { $0.time < $1.time }
    }

    /// Only what the caption font can actually draw: letters, numbers, spaces,
    /// punctuation. Symbol glyphs (`♪`, `★`, emoji) turn into boxes on the render, so
    /// they go.
    static nonisolated func sanitize(_ s: String) -> String {
        String(s.filter { $0.isLetter || $0.isNumber || $0.isWhitespace || $0.isPunctuation })
            .trimmingCharacters(in: .whitespaces)
    }

    /// A time stamp: `01:23.45`, `1:23`, `23.45`.
    private static nonisolated func stamp(_ tag: some StringProtocol) -> Double? {
        let parts = tag.split(separator: ":")
        guard parts.count <= 2, let secs = Double(parts.last ?? "") else { return nil }
        let mins = parts.count == 2 ? Double(parts[0]) ?? 0 : 0
        return mins * 60 + secs
    }
}
