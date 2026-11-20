// The music side of Liemo: songs and music videos the user brings in, kept in Documents/Music
// (visible in the Files app as On My iPhone › Liemo › Music, so files can be dropped there too).
// Nothing is bundled: the library starts empty.
import AVFoundation
import Observation
import UIKit

struct MusicTrack: Identifiable, Codable, Hashable {
    let id: String
    /// Path inside the music folder. Never absolute: the app's folder moves between updates.
    var file: String
    var title: String
    var artist = ""
    var album = ""
    var albumArtist = ""
    var genre = ""
    var trackNumber = 0
    var year = 0
    var duration = 0.0
    var isVideo = false
    var added = 0.0
    var size: Int64 = 0
    var modified = 0.0
    var hasArt = false
    /// The part of the song that hits, looped on request. Found on first use, then the user's.
    var hookStart: Double?
    var hookEnd: Double?
    var plays = 0
    var lastPlayed: Double?
    var favorite = false
    /// This song's own sound (speed, pitch, reverb…), kept when "Remember each song's sound" is on.
    var sound: SoundSettings?

    var url: URL { MusicLibrary.root.appendingPathComponent(file) }
    var artistName: String { artist.isEmpty ? "Unknown artist" : artist }
    var albumName: String { album.isEmpty ? "Unknown album" : album }
    var genreName: String { genre.isEmpty ? "Unknown genre" : genre }
    /// Albums group by their album artist when the file names one, so compilations stay whole.
    var albumKey: String { "\(albumName)\u{1}\(albumArtist.isEmpty ? artistName : albumArtist)" }
    var hook: ClosedRange<Double>? {
        guard let a = hookStart, let b = hookEnd, b - a > 0.5 else { return nil }
        return a...b
    }
}

struct Playlist: Identifiable, Codable, Hashable {
    let id: String
    var name: String
    var tracks: [String] = []
    var created = Date().timeIntervalSince1970
}

private struct LibraryFile: Codable {
    var tracks: [MusicTrack] = []
    var playlists: [Playlist] = []
}

@MainActor @Observable
final class MusicLibrary {
    static let shared = MusicLibrary()

    nonisolated static let audioTypes: Set<String> = ["mp3", "m4a", "m4b", "aac", "wav", "aif", "aiff", "aifc", "caf", "flac", "mp2"]
    nonisolated static let videoTypes: Set<String> = ["mp4", "m4v", "mov", "3gp"]
    nonisolated static func isMedia(_ url: URL) -> Bool {
        let e = url.pathExtension.lowercased()
        return audioTypes.contains(e) || videoTypes.contains(e)
    }

    /// Documents/Music: what the Files app shows for Liemo.
    nonisolated static var root: URL {
        let docs = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
        let d = docs.appendingPathComponent("Music", isDirectory: true)
        try? FileManager.default.createDirectory(at: d, withIntermediateDirectories: true)
        return d
    }
    nonisolated static var artDir: URL {
        let d = Core.shared.dataDir.appendingPathComponent("music-art", isDirectory: true)
        try? FileManager.default.createDirectory(at: d, withIntermediateDirectories: true)
        return d
    }
    nonisolated static func artURL(_ id: String) -> URL { artDir.appendingPathComponent("\(id).jpg") }
    private static var indexURL: URL { Core.shared.dataDir.appendingPathComponent("music.json") }

    var tracks: [MusicTrack] = []
    var playlists: [Playlist] = []
    /// What a scan or import is doing, for the screen; nil when idle.
    var status: String?
    /// Bumped when covers change, so their images reload.
    var artStamp = 0

    private init() { load() }

    // MARK: Lookups

    func track(_ id: String) -> MusicTrack? { tracks.first { $0.id == id } }
    var songs: [MusicTrack] { tracks.filter { !$0.isVideo } }
    var videos: [MusicTrack] { tracks.filter(\.isVideo) }

    var artists: [(name: String, tracks: [MusicTrack])] {
        Dictionary(grouping: songs, by: \.artistName).map { ($0.key, $0.value) }
            .sorted { $0.name.localizedStandardCompare($1.name) == .orderedAscending }
    }
    var albums: [(key: String, name: String, artist: String, tracks: [MusicTrack])] {
        Dictionary(grouping: songs, by: \.albumKey).map { k, v in
            let parts = k.components(separatedBy: "\u{1}")
            return (k, parts[0], parts.count > 1 ? parts[1] : "", Self.albumOrder(v))
        }
        .sorted { $0.name.localizedStandardCompare($1.name) == .orderedAscending }
    }
    var genres: [(name: String, tracks: [MusicTrack])] {
        Dictionary(grouping: songs, by: \.genreName).map { ($0.key, $0.value) }
            .sorted { $0.name.localizedStandardCompare($1.name) == .orderedAscending }
    }
    static func albumOrder(_ t: [MusicTrack]) -> [MusicTrack] {
        t.sorted { ($0.trackNumber == 0 ? Int.max : $0.trackNumber, $0.title) < ($1.trackNumber == 0 ? Int.max : $1.trackNumber, $1.title) }
    }

    // MARK: Saving

    private func load() {
        guard let data = try? Data(contentsOf: Self.indexURL),
              let f = try? JSONDecoder().decode(LibraryFile.self, from: data) else { return }
        tracks = f.tracks
        playlists = f.playlists
    }
    func save() {
        let f = LibraryFile(tracks: tracks, playlists: playlists)
        if let data = try? JSONEncoder().encode(f) { try? data.write(to: Self.indexURL, options: .atomic) }
    }

    func update(_ t: MusicTrack) {
        guard let i = tracks.firstIndex(where: { $0.id == t.id }) else { return }
        tracks[i] = t
        save()
    }

    // MARK: Scanning

    private var scanning = false

    /// Bring the library in line with the music folder: new files read in, changed ones re-read,
    /// gone ones dropped (and from playlists).
    func scan() async {
        guard !scanning else { return }
        scanning = true
        defer { scanning = false; status = nil }
        let root = Self.root
        let found: [(rel: String, size: Int64, modified: Double)] = await Task.detached {
            var out: [(String, Int64, Double)] = []
            let keys: [URLResourceKey] = [.isRegularFileKey, .fileSizeKey, .contentModificationDateKey]
            guard let e = FileManager.default.enumerator(at: root, includingPropertiesForKeys: keys, options: [.skipsHiddenFiles]) else { return out }
            let base = root.standardizedFileURL.path
            for case let url as URL in e where Self.isMedia(url) {
                let v = try? url.resourceValues(forKeys: Set(keys))
                guard v?.isRegularFile == true else { continue }
                var rel = url.standardizedFileURL.path
                if rel.hasPrefix(base) { rel.removeFirst(base.count) }
                rel = rel.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
                out.append((rel, Int64(v?.fileSize ?? 0), v?.contentModificationDate?.timeIntervalSince1970 ?? 0))
            }
            return out
        }.value

        let byFile = Dictionary(tracks.map { ($0.file, $0) }, uniquingKeysWith: { a, _ in a })
        var next: [MusicTrack] = []
        let fresh = found.filter { f in byFile[f.rel].map { $0.size != f.size || $0.modified != f.modified } ?? true }
        var done = 0
        for f in found {
            if var t = byFile[f.rel] {
                if t.size == f.size && t.modified == f.modified { next.append(t); continue }
                done += 1
                status = "Reading \(done) of \(fresh.count)…"
                let m = await Self.read(root.appendingPathComponent(f.rel), id: t.id)
                t.apply(m)
                t.size = f.size
                t.modified = f.modified
                next.append(t)
            } else {
                done += 1
                status = "Reading \(done) of \(fresh.count)…"
                let id = UUID().uuidString.lowercased()
                var t = MusicTrack(id: id, file: f.rel, title: "")
                t.apply(await Self.read(root.appendingPathComponent(f.rel), id: id))
                t.size = f.size
                t.modified = f.modified
                t.added = Date().timeIntervalSince1970
                next.append(t)
            }
        }
        let kept = Set(next.map(\.id))
        for t in tracks where !kept.contains(t.id) { try? FileManager.default.removeItem(at: Self.artURL(t.id)) }
        tracks = next
        for i in playlists.indices { playlists[i].tracks.removeAll { !kept.contains($0) } }
        if !fresh.isEmpty { artStamp += 1 }
        save()
    }

    // MARK: Importing

    /// Files and folders from the Files app (or anywhere the picker reaches) copied into the
    /// music folder; a folder keeps its name and its inside. Returns how many songs came in.
    @discardableResult
    func importItems(_ urls: [URL]) async -> Int {
        status = "Importing…"
        let root = Self.root
        let count: Int = await Task.detached {
            var n = 0
            let fm = FileManager.default
            for url in urls {
                let scoped = url.startAccessingSecurityScopedResource()
                defer { if scoped { url.stopAccessingSecurityScopedResource() } }
                var isDir: ObjCBool = false
                guard fm.fileExists(atPath: url.path, isDirectory: &isDir) else { continue }
                if isDir.boolValue {
                    let dest = Self.unique(root.appendingPathComponent(url.lastPathComponent, isDirectory: true))
                    guard let e = fm.enumerator(at: url, includingPropertiesForKeys: [.isRegularFileKey], options: [.skipsHiddenFiles]) else { continue }
                    let base = url.standardizedFileURL.path
                    for case let f as URL in e where Self.isMedia(f) {
                        var rel = f.standardizedFileURL.path
                        if rel.hasPrefix(base) { rel.removeFirst(base.count) }
                        let to = dest.appendingPathComponent(rel.trimmingCharacters(in: CharacterSet(charactersIn: "/")))
                        try? fm.createDirectory(at: to.deletingLastPathComponent(), withIntermediateDirectories: true)
                        if (try? fm.copyItem(at: f, to: to)) != nil { n += 1 }
                    }
                } else if Self.isMedia(url) {
                    let dir = root.appendingPathComponent("Imported", isDirectory: true)
                    try? fm.createDirectory(at: dir, withIntermediateDirectories: true)
                    if (try? fm.copyItem(at: url, to: Self.unique(dir.appendingPathComponent(url.lastPathComponent)))) != nil { n += 1 }
                }
            }
            return n
        }.value
        await scan()
        return count
    }

    nonisolated private static func unique(_ url: URL) -> URL {
        let fm = FileManager.default
        guard fm.fileExists(atPath: url.path) else { return url }
        let ext = url.pathExtension
        let base = url.deletingPathExtension().lastPathComponent
        let dir = url.deletingLastPathComponent()
        for i in 2... {
            let name = ext.isEmpty ? "\(base) \(i)" : "\(base) \(i).\(ext)"
            let u = dir.appendingPathComponent(name)
            if !fm.fileExists(atPath: u.path) { return u }
        }
        return url
    }

    /// Delete the file itself (and its cover); it leaves every playlist.
    func delete(_ t: MusicTrack) {
        try? FileManager.default.removeItem(at: t.url)
        try? FileManager.default.removeItem(at: Self.artURL(t.id))
        tracks.removeAll { $0.id == t.id }
        for i in playlists.indices { playlists[i].tracks.removeAll { $0 == t.id } }
        save()
    }

    func setCover(_ t: MusicTrack, image: UIImage) {
        guard let data = Self.jpeg(image) else { return }
        try? data.write(to: Self.artURL(t.id), options: .atomic)
        var t = t
        t.hasArt = true
        update(t)
        ArtCache.shared.forget(t.id)
        artStamp += 1
    }

    // MARK: Playlists

    @discardableResult
    func newPlaylist(_ name: String, tracks: [String] = []) -> Playlist {
        let p = Playlist(id: UUID().uuidString.lowercased(), name: name.isEmpty ? "New playlist" : name, tracks: tracks)
        playlists.append(p)
        save()
        return p
    }
    func add(_ ids: [String], to p: Playlist) {
        guard let i = playlists.firstIndex(where: { $0.id == p.id }) else { return }
        playlists[i].tracks.append(contentsOf: ids)
        save()
    }
    func setPlaylist(_ p: Playlist) {
        guard let i = playlists.firstIndex(where: { $0.id == p.id }) else { return }
        playlists[i] = p
        save()
    }
    func deletePlaylist(_ p: Playlist) {
        playlists.removeAll { $0.id == p.id }
        save()
    }

    // MARK: Reading a file

    struct Meta: Sendable {
        var title = "", artist = "", album = "", albumArtist = "", genre = ""
        var trackNumber = 0, year = 0
        var duration = 0.0
        var isVideo = false
        var hasArt = false
    }

    /// Tags (ID3, iTunes, QuickTime) and the cover, which is saved small for the lists. Files
    /// without tags fall back on their name, "Artist - Title" when it's written that way.
    nonisolated static func read(_ url: URL, id: String) async -> Meta {
        var m = Meta()
        let asset = AVURLAsset(url: url)
        m.duration = (try? await asset.load(.duration)).map(CMTimeGetSeconds) ?? 0
        if !m.duration.isFinite { m.duration = 0 }
        if videoTypes.contains(url.pathExtension.lowercased()) {
            m.isVideo = !((try? await asset.loadTracks(withMediaType: .video)) ?? []).isEmpty
        }
        var items = (try? await asset.load(.commonMetadata)) ?? []
        for f in (try? await asset.load(.availableMetadataFormats)) ?? [] {
            items += (try? await asset.loadMetadata(for: f)) ?? []
        }
        func first(_ ids: [AVMetadataIdentifier]) async -> AVMetadataItem? {
            for id in ids { if let i = items.first(where: { $0.identifier == id }) { return i } }
            return nil
        }
        func text(_ ids: [AVMetadataIdentifier]) async -> String {
            guard let i = await first(ids) else { return "" }
            if let s = try? await i.load(.stringValue) { return s.trimmingCharacters(in: .whitespacesAndNewlines) }
            if let n = try? await i.load(.numberValue) { return n.stringValue }
            return ""
        }
        m.title = await text([.commonIdentifierTitle, .id3MetadataTitleDescription, .iTunesMetadataSongName, .quickTimeMetadataTitle])
        m.artist = await text([.commonIdentifierArtist, .id3MetadataLeadPerformer, .iTunesMetadataArtist, .quickTimeMetadataArtist])
        m.album = await text([.commonIdentifierAlbumName, .id3MetadataAlbumTitle, .iTunesMetadataAlbum, .quickTimeMetadataAlbum])
        m.albumArtist = await text([.iTunesMetadataAlbumArtist, .id3MetadataBand])
        m.genre = cleanGenre(await text([.iTunesMetadataUserGenre, .id3MetadataContentType, .quickTimeMetadataGenre, .iTunesMetadataPredefinedGenre]))
        // Track number: "3/12" in ID3, two big-endian bytes after two zeros in iTunes atoms.
        if let i = await first([.iTunesMetadataTrackNumber]), let d = try? await i.load(.dataValue), d.count >= 4 {
            m.trackNumber = Int(d[2]) << 8 | Int(d[3])
        } else {
            m.trackNumber = Int(await text([.id3MetadataTrackNumber]).split(separator: "/").first ?? "") ?? 0
        }
        let yearText = await text([.id3MetadataYear, .id3MetadataRecordingTime, .iTunesMetadataReleaseDate, .commonIdentifierCreationDate, .quickTimeMetadataYear])
        m.year = Int(yearText.prefix(4)) ?? 0
        if m.title.isEmpty {
            let name = url.deletingPathExtension().lastPathComponent.replacingOccurrences(of: "_", with: " ")
            let parts = name.components(separatedBy: " - ")
            if parts.count >= 2, m.artist.isEmpty {
                m.artist = parts[0].trimmingCharacters(in: .whitespaces)
                m.title = parts.dropFirst().joined(separator: " - ").trimmingCharacters(in: .whitespaces)
            } else {
                m.title = name
            }
        }
        // The cover, else a frame of a music video.
        var cover: UIImage?
        if let i = await first([.commonIdentifierArtwork, .id3MetadataAttachedPicture, .iTunesMetadataCoverArt]),
           let d = try? await i.load(.dataValue) {
            cover = UIImage(data: d)
        }
        if cover == nil, m.isVideo {
            let g = AVAssetImageGenerator(asset: asset)
            g.appliesPreferredTrackTransform = true
            g.maximumSize = CGSize(width: 600, height: 600)
            let t = CMTime(seconds: min(5, m.duration / 3), preferredTimescale: 600)
            if let img = try? await g.image(at: t).image { cover = UIImage(cgImage: img) }
        }
        if let cover, let data = jpeg(cover) {
            try? data.write(to: artURL(id), options: .atomic)
            m.hasArt = true
        }
        return m
    }

    /// ID3 genres sometimes come as "(17)" or "(17)Rock": keep the words, or name the number.
    nonisolated private static func cleanGenre(_ g: String) -> String {
        var s = g
        if s.hasPrefix("("), let close = s.firstIndex(of: ")") {
            let num = Int(s[s.index(after: s.startIndex)..<close])
            s = String(s[s.index(after: close)...])
            if s.isEmpty, let num, num < id3Genres.count { s = id3Genres[num] }
        } else if let num = Int(s), num < id3Genres.count {
            s = id3Genres[num]
        }
        return s.trimmingCharacters(in: .whitespaces)
    }
    nonisolated private static let id3Genres = ["Blues", "Classic Rock", "Country", "Dance", "Disco", "Funk", "Grunge", "Hip-Hop", "Jazz", "Metal", "New Age", "Oldies", "Other", "Pop", "R&B", "Rap", "Reggae", "Rock", "Techno", "Industrial", "Alternative", "Ska", "Death Metal", "Pranks", "Soundtrack", "Euro-Techno", "Ambient", "Trip-Hop", "Vocal", "Jazz+Funk", "Fusion", "Trance", "Classical", "Instrumental", "Acid", "House", "Game", "Sound Clip", "Gospel", "Noise", "Alternative Rock", "Bass", "Soul", "Punk", "Space", "Meditative", "Instrumental Pop", "Instrumental Rock", "Ethnic", "Gothic", "Darkwave", "Techno-Industrial", "Electronic", "Pop-Folk", "Eurodance", "Dream", "Southern Rock", "Comedy", "Cult", "Gangsta", "Top 40", "Christian Rap", "Pop/Funk", "Jungle", "Native American", "Cabaret", "New Wave", "Psychedelic", "Rave", "Showtunes", "Trailer", "Lo-Fi", "Tribal", "Acid Punk", "Acid Jazz", "Polka", "Retro", "Musical", "Rock & Roll", "Hard Rock"]

    nonisolated static func jpeg(_ image: UIImage) -> Data? {
        let side: CGFloat = 600
        let s = min(1, side / max(image.size.width, image.size.height))
        let size = CGSize(width: image.size.width * s, height: image.size.height * s)
        let r = UIGraphicsImageRenderer(size: size, format: { let f = UIGraphicsImageRendererFormat(); f.scale = 1; return f }())
        return r.image { _ in image.draw(in: CGRect(origin: .zero, size: size)) }.jpegData(compressionQuality: 0.85)
    }
}

private extension MusicTrack {
    mutating func apply(_ m: MusicLibrary.Meta) {
        title = m.title
        artist = m.artist
        album = m.album
        albumArtist = m.albumArtist
        genre = m.genre
        trackNumber = m.trackNumber
        year = m.year
        duration = m.duration
        isVideo = m.isVideo
        hasArt = m.hasArt
    }
}

/// Covers, decoded once.
final class ArtCache: @unchecked Sendable {
    static let shared = ArtCache()
    private let cache = NSCache<NSString, UIImage>()
    func image(_ id: String) -> UIImage? {
        if let i = cache.object(forKey: id as NSString) { return i }
        guard let i = UIImage(contentsOfFile: MusicLibrary.artURL(id).path) else { return nil }
        cache.setObject(i, forKey: id as NSString)
        return i
    }
    func forget(_ id: String) { cache.removeObject(forKey: id as NSString) }
}

// MARK: - The hook

/// The part of a song that hits: the loudest stretch, favouring a jump up from what came
/// before (the drop), about four bars long and starting on a beat. Also a coarse waveform
/// for picking it by hand. Works on mono 12 kHz, which is plenty and keeps memory small.
enum HookFinder {
    struct Analysis: Sendable {
        var peaks: [Float]          // 0…1, `bins` of them across the song
        var hook: ClosedRange<Double>
        var bpm: Double
        var firstBeat: Double
    }

    static let rate = 12000.0

    static func analyze(_ url: URL, bins: Int = 240) async -> Analysis? {
        guard let s = await mono(url), s.count > Int(rate * 2) else { return nil }
        let duration = Double(s.count) / rate
        // Peaks for drawing.
        var peaks = [Float](repeating: 0, count: bins)
        let per = max(1, s.count / bins)
        for b in 0..<bins {
            let lo = b * per, hi = min(s.count, lo + per)
            var m: Float = 0
            if lo < hi { for i in lo..<hi { m = max(m, abs(s[i])) } }
            peaks[b] = m
        }
        let top = max(peaks.max() ?? 1, 1e-4)
        peaks = peaks.map { $0 / top }

        let (bpm, first) = tempo(s)
        let beat = 60 / bpm
        // Four bars, kept between 8 and 24 seconds.
        var length = beat * 16
        while length > 24 { length /= 2 }
        while length < 8 { length *= 2 }
        length = min(length, duration)
        var start = drop(s, window: length)
        // Onto the beat grid.
        start = max(0, first + ((start - first) / beat).rounded() * beat)
        if start + length > duration { start = max(0, duration - length) }
        return Analysis(peaks: peaks, hook: start...(start + length), bpm: bpm, firstBeat: first)
    }

    private static func mono(_ url: URL) async -> [Float]? {
        let asset = AVURLAsset(url: url)
        guard let track = try? await asset.loadTracks(withMediaType: .audio).first,
              let reader = try? AVAssetReader(asset: asset) else { return nil }
        let out = AVAssetReaderTrackOutput(track: track, outputSettings: [
            AVFormatIDKey: kAudioFormatLinearPCM, AVSampleRateKey: rate, AVNumberOfChannelsKey: 1,
            AVLinearPCMBitDepthKey: 32, AVLinearPCMIsFloatKey: true, AVLinearPCMIsNonInterleaved: false,
            AVLinearPCMIsBigEndianKey: false,
        ])
        guard reader.canAdd(out) else { return nil }
        reader.add(out)
        guard reader.startReading() else { return nil }
        var s: [Float] = []
        while let buf = out.copyNextSampleBuffer() {
            guard let block = CMSampleBufferGetDataBuffer(buf) else { continue }
            let n = CMBlockBufferGetDataLength(block) / 4
            let at = s.count
            s.append(contentsOf: repeatElement(0, count: n))
            s.withUnsafeMutableBytes { raw in
                _ = CMBlockBufferCopyDataBytes(block, atOffset: 0, dataLength: n * 4, destination: raw.baseAddress! + at * 4)
            }
        }
        return reader.status == .completed ? s : nil
    }

    private static func rms(_ s: [Float], _ from: Int, _ len: Int) -> Float {
        var sum: Float = 0
        let hi = min(s.count, from + len)
        if from < hi { for i in from..<hi { sum += s[i] * s[i] } }
        return (sum / Float(max(1, len))).squareRoot()
    }

    /// Start of the `window`-second stretch with the most energy plus a bonus for a jump up.
    private static func drop(_ s: [Float], window: Double) -> Double {
        let block = Int(rate / 4)
        let n = s.count / block
        let w = max(1, Int(window * 4))
        guard n > w else { return 0 }
        let e = (0..<n).map { rms(s, $0 * block, block) }
        var prefix = [Float](repeating: 0, count: n + 1)
        for i in 0..<n { prefix[i + 1] = prefix[i] + e[i] }
        func mean(_ a: Int, _ b: Int) -> Float {
            let lo = max(0, a), hi = min(n, b)
            return hi > lo ? (prefix[hi] - prefix[lo]) / Float(hi - lo) : 0
        }
        let look = 16
        var best = 0, score = -Float.infinity
        for st in 0...(n - w) {
            let jump = st >= look ? mean(st, st + look) - mean(st - look, st) : 0
            let v = mean(st, st + w) + 0.8 * jump
            if v > score { score = v; best = st }
        }
        return Double(best) / 4
    }

    /// Tempo from onset autocorrelation (70–180 BPM, leaning to 125) and the grid's phase.
    private static func tempo(_ s: [Float]) -> (Double, Double) {
        let hop = 128
        let n = s.count / hop
        guard n > 200 else { return (120, 0) }
        let energy = (0..<n).map { i -> Float in let r = rms(s, i * hop, hop); return log(r * r + 1e-6) }
        var onset = [Float](repeating: 0, count: n)
        for i in 1..<n { onset[i] = max(0, energy[i] - energy[i - 1]) }
        let avg = onset.reduce(0, +) / Float(n)
        for i in 0..<n { onset[i] -= avg }
        // At most two minutes of it: plenty for tempo, and quick.
        let m = min(n, Int(120 * rate) / hop)
        func ac(_ lag: Double) -> Double {
            let l = Int(lag), f = Float(lag - Double(l))
            let count = m - l - 1
            guard count > 0 else { return 0 }
            var dot: Float = 0
            for t in 0..<count { dot += onset[t] * (onset[t + l] * (1 - f) + onset[t + l + 1] * f) }
            return Double(dot) / Double(count)
        }
        let fr = rate / Double(hop)
        var bestLag = 60 / 120 * fr, bestScore = -Double.infinity
        var bpm = 70.0
        while bpm <= 180 {
            let lag = 60 / bpm * fr
            var sc = ac(lag) + 0.5 * ac(2 * lag) + 0.25 * ac(4 * lag)
            sc *= 0.85 + 0.15 * exp(-pow(log2(bpm / 125), 2) / 0.5)
            if sc > bestScore { bestScore = sc; bestLag = lag }
            bpm += 0.5
        }
        var bestOff = 0, bestSum = -Float.infinity
        for off in 0..<max(1, Int(bestLag)) {
            var sum: Float = 0
            var t = Double(off)
            while Int(t) < m { sum += onset[Int(t)]; t += bestLag }
            if sum > bestSum { bestSum = sum; bestOff = off }
        }
        return (60 * fr / bestLag, Double(bestOff) / fr)
    }
}
