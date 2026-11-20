// Plays the music library. Songs run through an audio engine (speed, pitch, bass, treble,
// reverb) so they can be reworked live; music videos play through AVPlayer, which can change
// speed (with or without the pitch following) but nothing else.
import AVFoundation
import MediaPlayer
import Observation
import UIKit

/// How the music sounds. Speed with `vinyl` on changes pitch too, like a record.
struct SoundSettings: Codable, Equatable, Hashable {
    var speed = 1.0
    var vinyl = false
    var pitch = 0.0       // semitones, on top of the speed's own when `vinyl`
    var reverb = 0.0      // 0…1
    var room = 0.5        // 0 small room … 1 cathedral
    var bass = 0.0        // dB
    var treble = 0.0      // dB

    var isNormal: Bool { self == SoundSettings() }

    struct Preset: Identifiable {
        let name: String
        let sound: SoundSettings
        var id: String { name }
    }
    static let presets: [Preset] = [
        Preset(name: "Normal", sound: SoundSettings()),
        Preset(name: "Sped up", sound: SoundSettings(speed: 1.25, vinyl: true, reverb: 0.08)),
        Preset(name: "Nightcore", sound: SoundSettings(speed: 1.3, vinyl: true, treble: 2)),
        Preset(name: "Slowed + reverb", sound: SoundSettings(speed: 0.82, vinyl: true, reverb: 0.45, room: 0.85)),
        Preset(name: "Daycore", sound: SoundSettings(speed: 0.85, vinyl: true, bass: 2)),
        Preset(name: "Bass boost", sound: SoundSettings(bass: 9)),
        Preset(name: "Concert hall", sound: SoundSettings(reverb: 0.35, room: 0.7)),
        Preset(name: "Faster, same pitch", sound: SoundSettings(speed: 1.15)),
        Preset(name: "Slower, same pitch", sound: SoundSettings(speed: 0.85)),
        Preset(name: "Lo-fi", sound: SoundSettings(speed: 0.94, vinyl: true, reverb: 0.15, room: 0.3, bass: 3, treble: -9)),
    ]
}

enum RepeatMode: String, Codable { case off, all, one }

private struct SavedState: Codable {
    var queue: [String]
    var index: Int
    var time: Double
    var shuffle: Bool
    var repeatMode: RepeatMode
    var sound: SoundSettings
    var original: [String]?
}

@MainActor @Observable
final class MusicPlayer {
    static let shared = MusicPlayer()

    // What plays.
    private(set) var queue: [String] = []
    private(set) var index = 0
    private(set) var playing = false
    private(set) var time = 0.0
    var current: MusicTrack? { queue.indices.contains(index) ? MusicLibrary.shared.track(queue[index]) : nil }
    var duration: Double { current?.duration ?? 0 }

    var shuffle = false {
        didSet { guard shuffle != oldValue, !restoring else { return }; reshuffle() }
    }
    var repeatMode: RepeatMode = .off { didSet { persist() } }
    var sound = SoundSettings() {
        didSet {
            guard sound != oldValue else { return }
            applySound()
            if Self.perSong, var t = current { t.sound = sound.isNormal ? nil : sound; MusicLibrary.shared.update(t) }
            persist()
        }
    }
    /// Play the hook over and over.
    private(set) var looping = false
    /// The music video's player, when one plays.
    private(set) var video: AVPlayer?
    /// When the music stops by itself; `stopAfterTrack` waits for the song to end instead.
    private(set) var sleepAt: Date?
    private(set) var stopAfterTrack = false
    var error: String?
    /// Checks run without sound.
    var silent = false { didSet { engine.mainMixerNode.outputVolume = silent ? 0 : 1; video?.volume = silent ? 0 : 1 } }

    static var perSong: Bool { UserDefaults.standard.bool(forKey: "music-per-song-sound") }

    // The audio engine.
    private let engine = AVAudioEngine()
    private let node = AVAudioPlayerNode()
    private let timePitch = AVAudioUnitTimePitch()
    private let eq = AVAudioUnitEQ(numberOfBands: 2)
    private let reverb = AVAudioUnitReverb()
    private var file: AVAudioFile?
    private var format: AVAudioFormat?
    private var reverbPreset: AVAudioUnitReverbPreset?
    /// Bumped whenever scheduling starts over, so stale completions do nothing.
    private var generation = 0
    /// Where the node's sample clock started: song time = `base` + played (or the loop maths).
    private var base = 0.0
    private var loopFirst = 0.0       // seconds of the first, partial pass before the loop repeats
    private var loopRange: ClosedRange<Double>?
    private var pausedAt = 0.0
    private var original: [String]?   // the order before shuffling
    private var ticker: Timer?
    private var videoEnd: NSObjectProtocol?
    private var lastInfo = Date.distantPast
    private var restoring = false

    private init() {
        engine.attach(node)
        engine.attach(timePitch)
        engine.attach(eq)
        engine.attach(reverb)
        engine.connect(timePitch, to: eq, format: nil)
        engine.connect(eq, to: reverb, format: nil)
        engine.connect(reverb, to: engine.mainMixerNode, format: nil)
        eq.bands[0].filterType = .lowShelf
        eq.bands[0].frequency = 110
        eq.bands[1].filterType = .highShelf
        eq.bands[1].frequency = 7000
        for b in eq.bands { b.bypass = false; b.gain = 0 }
        reverb.wetDryMix = 0
        restore()
        applySound()
        remoteControls()
        NotificationCenter.default.addObserver(forName: AVAudioSession.interruptionNotification, object: nil, queue: .main) { [weak self] n in
            guard let raw = n.userInfo?[AVAudioSessionInterruptionTypeKey] as? UInt,
                  AVAudioSession.InterruptionType(rawValue: raw) == .began else { return }
            MainActor.assumeIsolated { self?.pause() }
        }
        NotificationCenter.default.addObserver(forName: AVAudioSession.routeChangeNotification, object: nil, queue: .main) { [weak self] n in
            // Headphones pulled out: stop rather than play out loud.
            guard let raw = n.userInfo?[AVAudioSessionRouteChangeReasonKey] as? UInt,
                  AVAudioSession.RouteChangeReason(rawValue: raw) == .oldDeviceUnavailable else { return }
            MainActor.assumeIsolated { self?.pause() }
        }
    }

    // MARK: Playing

    /// Play `ids` starting at `start`; with `shuffled`, in a random order starting there.
    func play(_ ids: [String], at start: Int = 0, shuffled: Bool = false) {
        guard !ids.isEmpty else { return }
        original = nil
        queue = ids
        index = min(max(0, start), ids.count - 1)
        if shuffled || shuffle {
            shuffle = true
            original = ids
            let first = ids[index]
            var rest = ids
            rest.remove(at: index)
            queue = [first] + rest.shuffled()
            index = 0
        }
        load(from: startPoint(), autoplay: true)
    }

    func playNext(_ ids: [String]) {
        if queue.isEmpty { play(ids); return }
        queue.insert(contentsOf: ids, at: index + 1)
        original?.append(contentsOf: ids)
        persist()
    }
    func addToQueue(_ ids: [String]) {
        if queue.isEmpty { play(ids); return }
        queue.append(contentsOf: ids)
        original?.append(contentsOf: ids)
        persist()
    }
    func jump(to i: Int) {
        guard queue.indices.contains(i) else { return }
        index = i
        load(from: startPoint(), autoplay: true)
    }
    func removeFromQueue(at offsets: IndexSet) {
        // The playing song stays; everything else can go.
        let gone = offsets.filter { $0 != index }
        let before = gone.filter { $0 < index }.count
        for i in gone.sorted(by: >) { queue.remove(at: i) }
        index -= before
        persist()
    }
    func moveInQueue(from: IndexSet, to: Int) {
        let id = queue.indices.contains(index) ? queue[index] : nil
        queue.move(fromOffsets: from, toOffset: to)
        if let id, let i = queue.firstIndex(of: id) { index = i }
        persist()
    }
    func clearUpNext() {
        guard queue.indices.contains(index) else { return }
        queue = Array(queue[...index])
        persist()
    }

    func toggle() { playing ? pause() : resume() }

    func resume() {
        guard let t = current else { return }
        if video == nil && file == nil { load(from: pausedAt, autoplay: true); return }
        activateSession()
        if let video {
            video.rate = Float(sound.speed)
        } else {
            startEngine()
            node.play()
        }
        playing = true
        _ = t
        startTicker()
        nowPlaying()
    }

    func pause() {
        guard playing else { return }
        pausedAt = currentTime()
        time = pausedAt
        if let video { video.pause() } else {
            node.pause()
            engine.pause()
        }
        playing = false
        nowPlaying()
        persist()
    }

    func next() {
        guard !queue.isEmpty else { return }
        if index + 1 < queue.count { index += 1 }
        else if repeatMode == .all { index = 0 }
        else { stop(); return }
        load(from: startPoint(), autoplay: true)
    }

    /// Back to the start of the song, or the one before when it's only just begun.
    func previous() {
        guard !queue.isEmpty else { return }
        if currentTime() > 3 || index == 0 {
            seek(to: looping ? (current?.hookStart ?? 0) : 0)
            return
        }
        index -= 1
        load(from: startPoint(), autoplay: true)
    }

    func stop() {
        generation += 1
        node.stop()
        engine.pause()
        video?.pause()
        playing = false
        pausedAt = 0
        time = 0
        ticker?.invalidate()
        nowPlaying()
        persist()
    }

    func seek(to t: Double) {
        let t = max(0, min(t, max(0, duration - 0.05)))
        if let video {
            video.seek(to: CMTime(seconds: t, preferredTimescale: 600), toleranceBefore: .zero, toleranceAfter: .zero)
            time = t
        } else {
            schedule(from: t)
            if playing { node.play() }
            time = t
            pausedAt = t
        }
        nowPlaying()
    }

    // MARK: The hook loop

    /// Loop the hook (finding it first when the song has none yet).
    func setLooping(_ on: Bool) async {
        guard var t = current else { return }
        if on && t.hook == nil {
            guard let a = await HookFinder.analyze(t.url) else { error = "Couldn't read this song's sound."; return }
            t.hookStart = a.hook.lowerBound
            t.hookEnd = a.hook.upperBound
            MusicLibrary.shared.update(t)
        }
        looping = on
        let now = currentTime()
        if on, let h = t.hook {
            // Jump into the hook unless we're already in it.
            seek(to: h.contains(now) ? now : h.lowerBound)
        } else {
            seek(to: now)
        }
    }

    /// The hook moved (the user picked it again): keep looping the new one.
    func hookChanged() {
        guard looping, let h = current?.hook else { return }
        let now = currentTime()
        seek(to: h.contains(now) ? now : h.lowerBound)
    }

    // MARK: Sleep timer

    func sleep(after minutes: Double?) {
        stopAfterTrack = false
        sleepAt = minutes.map { Date().addingTimeInterval($0 * 60) }
    }
    func sleepAfterThisSong() {
        sleepAt = nil
        stopAfterTrack = true
    }

    // MARK: Internals

    private func startPoint() -> Double {
        if looping, let h = current?.hook { return h.lowerBound }
        if UserDefaults.standard.bool(forKey: "music-start-at-hook"), let h = current?.hook { return h.lowerBound }
        return 0
    }

    private func load(from start: Double, autoplay: Bool) {
        generation += 1
        node.stop()
        teardownVideo()
        file = nil
        guard var t = current else { stop(); return }
        if Self.perSong { let s = t.sound ?? SoundSettings(); if s != sound { sound = s } }
        if looping && t.hook == nil { looping = false }
        if t.isVideo {
            let item = AVPlayerItem(url: t.url)
            item.audioTimePitchAlgorithm = sound.vinyl ? .varispeed : .timeDomain
            let p = AVPlayer(playerItem: item)
            p.automaticallyWaitsToMinimizeStalling = false
            if silent { p.volume = 0 }
            video = p
            videoEnd = NotificationCenter.default.addObserver(forName: .AVPlayerItemDidPlayToEndTime, object: item, queue: .main) { [weak self] _ in
                MainActor.assumeIsolated { self?.trackEnded() }
            }
            if start > 0 { p.seek(to: CMTime(seconds: start, preferredTimescale: 600)) }
        } else {
            do {
                let f = try AVAudioFile(forReading: t.url)
                file = f
                if format != f.processingFormat {
                    format = f.processingFormat
                    engine.disconnectNodeOutput(node)
                    engine.connect(node, to: timePitch, format: f.processingFormat)
                }
                schedule(from: start)
            } catch {
                self.error = "“\(t.title)” couldn't be played."
                // Skip the broken one rather than stopping the music.
                if index + 1 < queue.count { index += 1; load(from: 0, autoplay: autoplay) } else { stop() }
                return
            }
        }
        time = start
        pausedAt = start
        t.plays += 1
        t.lastPlayed = Date().timeIntervalSince1970
        MusicLibrary.shared.update(t)
        if autoplay {
            playing = false
            resume()
        } else {
            nowPlaying()
        }
        persist()
    }

    /// Queue the file from `t`: to the end, or (looping) to the hook's end and then the hook
    /// over and over, two passes ahead so the seams are seamless.
    private func schedule(from t: Double) {
        guard let f = file else { return }
        generation += 1
        let g = generation
        node.stop()
        let sr = f.processingFormat.sampleRate
        let total = f.length
        let startFrame = AVAudioFramePosition(t * sr)
        base = t
        if looping, let h = current?.hook, t < h.upperBound {
            let a = AVAudioFramePosition(h.lowerBound * sr), b = min(total, AVAudioFramePosition(h.upperBound * sr))
            let from = min(max(startFrame, 0), b - 1)
            loopRange = h
            loopFirst = Double(b - from) / sr
            node.scheduleSegment(f, startingFrame: from, frameCount: AVAudioFrameCount(b - from), at: nil)
            func again() {
                node.scheduleSegment(f, startingFrame: a, frameCount: AVAudioFrameCount(b - a), at: nil, completionCallbackType: .dataConsumed) { [weak self] _ in
                    DispatchQueue.main.async { MainActor.assumeIsolated {
                        guard let self, self.generation == g else { return }
                        again()
                    } }
                }
            }
            again()
            again()
        } else {
            loopRange = nil
            guard startFrame < total else { return }
            node.scheduleSegment(f, startingFrame: startFrame, frameCount: AVAudioFrameCount(total - startFrame), at: nil, completionCallbackType: .dataPlayedBack) { [weak self] _ in
                DispatchQueue.main.async { MainActor.assumeIsolated {
                    guard let self, self.generation == g else { return }
                    self.trackEnded()
                } }
            }
        }
    }

    private func trackEnded() {
        if stopAfterTrack { stopAfterTrack = false; pause(); seek(to: 0); return }
        if repeatMode == .one {
            load(from: startPoint(), autoplay: true)
        } else {
            next()
        }
    }

    func currentTime() -> Double {
        if let video { return CMTimeGetSeconds(video.currentTime()).isFinite ? CMTimeGetSeconds(video.currentTime()) : 0 }
        guard playing, let nt = node.lastRenderTime, nt.isSampleTimeValid,
              let pt = node.playerTime(forNodeTime: nt) else { return pausedAt }
        let played = max(0, Double(pt.sampleTime) / pt.sampleRate)
        if let h = loopRange {
            if played < loopFirst { return base + played }
            let len = h.upperBound - h.lowerBound
            return h.lowerBound + (played - loopFirst).truncatingRemainder(dividingBy: max(len, 0.01))
        }
        return min(base + played, duration)
    }

    private func applySound() {
        let s = sound
        timePitch.rate = Float(s.speed)
        timePitch.pitch = Float((s.vinyl ? 1200 * log2(s.speed) : 0) + s.pitch * 100)
        eq.bands[0].gain = Float(s.bass)
        eq.bands[1].gain = Float(s.treble)
        let preset: AVAudioUnitReverbPreset = s.room < 0.25 ? .mediumRoom : s.room < 0.5 ? .largeRoom : s.room < 0.75 ? .largeHall : .cathedral
        if preset != reverbPreset { reverb.loadFactoryPreset(preset); reverbPreset = preset }
        reverb.wetDryMix = Float(s.reverb * 70)
        if let video, let item = video.currentItem {
            item.audioTimePitchAlgorithm = s.vinyl ? .varispeed : .timeDomain
            if playing { video.rate = Float(s.speed) }
        }
        nowPlaying()
    }

    private func startEngine() {
        guard !engine.isRunning else { return }
        engine.prepare()
        try? engine.start()
    }

    private func activateSession() {
        let s = AVAudioSession.sharedInstance()
        try? s.setCategory(.playback, mode: .default)
        try? s.setActive(true)
    }

    private func teardownVideo() {
        if let videoEnd { NotificationCenter.default.removeObserver(videoEnd) }
        videoEnd = nil
        video?.pause()
        video = nil
    }

    private func reshuffle() {
        guard !queue.isEmpty else { persist(); return }
        let id = queue[index]
        if shuffle {
            original = queue
            var rest = queue
            rest.remove(at: index)
            queue = [id] + rest.shuffled()
            index = 0
        } else if let o = original {
            queue = o
            index = o.firstIndex(of: id) ?? 0
            original = nil
        }
        persist()
    }

    private func startTicker() {
        ticker?.invalidate()
        ticker = Timer.scheduledTimer(withTimeInterval: 0.1, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated { self?.tick() }
        }
    }

    private func tick() {
        guard playing else { ticker?.invalidate(); return }
        time = currentTime()
        // Music videos loop their hook by seeking back.
        if video != nil, looping, let h = current?.hook, time >= h.upperBound || time < h.lowerBound - 0.5 {
            seek(to: h.lowerBound)
        }
        if let at = sleepAt, Date() >= at { sleepAt = nil; pause() }
        if Date().timeIntervalSince(lastInfo) > 5 { nowPlaying() }
    }

    // MARK: Lock screen and Control Center

    private func remoteControls() {
        let c = MPRemoteCommandCenter.shared()
        c.playCommand.addTarget { [weak self] _ in MainActor.assumeIsolated { self?.resume() }; return .success }
        c.pauseCommand.addTarget { [weak self] _ in MainActor.assumeIsolated { self?.pause() }; return .success }
        c.togglePlayPauseCommand.addTarget { [weak self] _ in MainActor.assumeIsolated { self?.toggle() }; return .success }
        c.nextTrackCommand.addTarget { [weak self] _ in MainActor.assumeIsolated { self?.next() }; return .success }
        c.previousTrackCommand.addTarget { [weak self] _ in MainActor.assumeIsolated { self?.previous() }; return .success }
        c.changePlaybackPositionCommand.addTarget { [weak self] e in
            guard let e = e as? MPChangePlaybackPositionCommandEvent else { return .commandFailed }
            MainActor.assumeIsolated { self?.seek(to: e.positionTime) }
            return .success
        }
    }

    private func nowPlaying() {
        lastInfo = Date()
        guard let t = current else { MPNowPlayingInfoCenter.default().nowPlayingInfo = nil; return }
        var info: [String: Any] = [
            MPMediaItemPropertyTitle: t.title,
            MPMediaItemPropertyArtist: t.artistName,
            MPMediaItemPropertyAlbumTitle: t.albumName,
            MPMediaItemPropertyPlaybackDuration: t.duration,
            MPNowPlayingInfoPropertyElapsedPlaybackTime: playing ? currentTime() : pausedAt,
            MPNowPlayingInfoPropertyPlaybackRate: playing ? sound.speed : 0,
        ]
        if let img = ArtCache.shared.image(t.id) {
            info[MPMediaItemPropertyArtwork] = MPMediaItemArtwork(boundsSize: img.size) { _ in img }
        }
        MPNowPlayingInfoCenter.default().nowPlayingInfo = info
    }

    // MARK: Remembering

    private func persist() {
        let s = SavedState(queue: queue, index: index, time: playing ? currentTime() : pausedAt, shuffle: shuffle,
                           repeatMode: repeatMode, sound: sound, original: original)
        if let d = try? JSONEncoder().encode(s) { UserDefaults.standard.set(d, forKey: "music-player") }
    }

    private func restore() {
        guard let d = UserDefaults.standard.data(forKey: "music-player"),
              let s = try? JSONDecoder().decode(SavedState.self, from: d) else { return }
        restoring = true
        defer { restoring = false }
        queue = s.queue
        index = min(s.index, max(0, s.queue.count - 1))
        pausedAt = s.time
        time = s.time
        shuffle = s.shuffle
        original = s.original
        repeatMode = s.repeatMode
        sound = s.sound
    }
}
