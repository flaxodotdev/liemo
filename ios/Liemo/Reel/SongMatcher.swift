import AVFoundation
import Foundation
import ShazamKit

/// Names a song from its sound (Shazam) and finds its lyrics on the web (LRCLIB, free and
/// no key). The lyrics come back synced when there's a timing for them, so the reel can put
/// every line on its real moment.
enum SongMatcher {
    // MARK: Recognition

    struct Match: Sendable {
        let title: String
        let artist: String
    }

    /// Reads a stretch out of the middle of the file and asks Shazam about it. The middle has
    /// the vocals; the start is often an intro that matches nothing. The audio is fingerprinted
    /// once and matched, so the answer comes in a few seconds, not after a hang.
    static func recognize(url: URL) async throws -> Match {
        let file = try AVAudioFile(forReading: url)
        let fmt = file.processingFormat
        let rate = fmt.sampleRate
        let whole = AVAudioFramePosition(file.length)
        let sample = AVAudioFrameCount(min(whole, AVAudioFramePosition(15 * rate)))
        let start = max(0, min(whole - AVAudioFramePosition(sample), AVAudioFramePosition(whole * 3 / 10)))
        guard let buf = AVAudioPCMBuffer(pcmFormat: fmt, frameCapacity: sample) else {
            throw CoreError(message: "That song couldn't be read.")
        }
        buf.frameLength = sample
        file.framePosition = start
        try file.read(into: buf)

        var signature: SHSignature
        do {
            let gen = SHSignatureGenerator()
            try gen.append(buf, at: nil)
            signature = try gen.signature()
        } catch {
            // Odd sample rates and channel counts are the usual cause; Shazam can't hear them.
            throw CoreError(message: "That song's format can't be fingerprinted. Try an M4A or MP3 file.")
        }

        return try await withCheckedThrowingContinuation { cont in
            // The session must outlive the call that starts the match, or it dies mid-request
            // and nothing ever comes back. The bridge holds it until the answer arrives.
            let session = SHSession()
            let bridge = MatchBridge(session: session) { result in cont.resume(with: result) }
            MatchBridge.keep(bridge)
            session.delegate = bridge
            session.match(signature)
            // Even with no network, say so in seconds, not half a minute.
            let watchdog = Task {
                try? await Task.sleep(for: .seconds(12))
                bridge.complete(.failure(CoreError(message: "Recognition timed out. Check your connection.")))
            }
        }
    }

    /// SHSession doesn't promise to hold its delegate, so the bridge holds both until the
    /// match comes back and then lets go. Answers exactly once, whichever arrives first.
    private final class MatchBridge: NSObject, SHSessionDelegate, @unchecked Sendable {
        private static var all: [MatchBridge] = []
        private static let lock = NSLock()
        private let finish: @Sendable (Result<Match, Error>) -> Void
        private var done = false
        private let onceLock = NSLock()
        private let session: SHSession

        init(session: SHSession, finish: @escaping @Sendable (Result<Match, Error>) -> Void) {
            self.session = session
            self.finish = finish
        }

        static func keep(_ b: MatchBridge) { lock.lock(); all.append(b); lock.unlock() }
        static func forget(_ b: MatchBridge) { lock.lock(); all.removeAll { $0 === b }; lock.unlock() }

        private func once() -> Bool {
            onceLock.lock(); defer { onceLock.unlock() }
            guard !done else { return false }
            done = true
            return true
        }

        func complete(_ result: Result<Match, Error>) {
            guard once() else { return }
            Self.forget(self)
            finish(result)
        }

        func session(_ session: SHSession, didFind match: SHMatch) {
            guard let m = match.mediaItems.first else { return complete(.failure(CoreError(message: "No match."))) }
            complete(.success(Match(title: m.title ?? "", artist: m.artist ?? "")))
        }

        func session(_ session: SHSession, didNotFindMatchFor signature: SHSignature, error: (any Error)?) {
            // Surface Apple's own complaint (permissions, network) instead of shrugging at it.
            if let e = error {
                complete(.failure(CoreError(message: "Shazam couldn't run: \(e.localizedDescription)")))
            } else {
                complete(.failure(CoreError(message: "That song didn't match. Name it and add lyrics by hand.")))
            }
        }
    }

    // MARK: Lyrics

    struct Fetched: Sendable {
        let synced: String?
        let plain: String?
        var syncedLines: [LyricLine] { SongLibrary.parseLRC(synced ?? "") }
        var anyText: String? {
            if let p = plain, !p.isEmpty { return p }
            if let s = synced, !s.isEmpty { return s }
            return nil
        }
    }

    /// LRCLIB: `get` for an exact artist/track pair, `search` when that misses. With the
    /// song's real duration, the best match wins: a synced version close in length beats a
    /// plain one, and a sped-up or live cut of another length is left alone.
    static func fetchLyrics(artist: String, track: String, duration: Double? = nil) async throws -> Fetched {
        var got: [LRCResult] = []
        var parts: [URLQueryItem] = [.init(name: "artist_name", value: artist), .init(name: "track_name", value: track)]
        got += try await lrclib("get", parts) ?? []
        parts = [.init(name: "track_name", value: track), .init(name: "artist_name", value: artist)]
        got += try await lrclib("search", parts) ?? []
        got += try await lrclib("search", [.init(name: "q", value: "\(artist) \(track)")]) ?? []

        // Score: word-synced beats synced beats plain, and a duration close to the
        // real file beats a different cut of the song.
        func score(_ r: LRCResult) -> Double {
            var s = 0.0
            if !(r.syncedLyrics ?? "").isEmpty { s += 10 }
            if r.hasWordSync == true { s += 3 }
            if !(r.plainLyrics ?? "").isEmpty { s += 1 }
            if let duration, let d = r.duration, d > 0 {
                let near = max(0.0, 1.0 - abs(d - duration) / 15)
                s += 4 * near
            }
            return s
        }
        if let best = got.max(by: { score($0) < score($1) }), !(best.syncedLyrics ?? "").isEmpty || !(best.plainLyrics ?? "").isEmpty {
            return Fetched(synced: best.syncedLyrics, plain: best.plainLyrics)
        }
        return Fetched(synced: nil, plain: nil)
    }

    private struct LRCResult: Decodable {
        var trackName: String?
        var artistName: String?
        var duration: Double?
        var plainLyrics: String?
        var syncedLyrics: String?
        var hasWordSync: Bool?
    }

    private static func lrclib(_ endpoint: String, _ query: [URLQueryItem]) async throws -> [LRCResult]? {
        var parts = URLComponents(string: "https://lrclib.net/api/\(endpoint)")!
        parts.queryItems = query
        var req = URLRequest(url: parts.url!)
        req.setValue("Liemo/0.6 (https://github.com/flaxo)", forHTTPHeaderField: "User-Agent")
        let (data, resp) = try await URLSession.shared.data(for: req)
        guard (resp as? HTTPURLResponse)?.statusCode == 200 else { return nil }
        // `get` answers with one result, `search` with a list.
        if let one = try? JSONDecoder().decode(LRCResult.self, from: data) { return [one] }
        return try JSONDecoder().decode([LRCResult].self, from: data)
    }

    /// Recognition and lyrics together, with a name fallback for the unrecognisable.
    static func identify(url: URL, fallbackName: String) async -> (Match, Fetched) {
        var match = Match(title: fallbackName, artist: "")
        do { match = try await recognize(url: url) } catch {}
        var lyrics = Fetched(synced: nil, plain: nil)
        if !match.title.isEmpty {
            let duration = (try? AVAudioFile(forReading: url)).map { Double($0.length) / $0.processingFormat.sampleRate }
            lyrics = (try? await fetchLyrics(artist: match.artist, track: match.title, duration: duration)) ?? lyrics
        }
        return (match, lyrics)
    }
}
