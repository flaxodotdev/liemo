import AVFoundation
import CoreGraphics
import Foundation
import Observation
import QuartzCore
import UIKit

/// One open project: its state as the engine reports it, the preview frame, playback.
@MainActor
@Observable
final class EditorModel {
    let id: String
    private(set) var doc: ProjectDoc?
    var time: Double = 0 { didSet { if !playing { requestFrame() } } }
    private(set) var playing = false
    var selected: String?
    private(set) var frame: CGImage?
    private(set) var canUndo = false
    private(set) var canRedo = false
    var error: String?
    /// Filmstrip frames by index (frame i shows time (i + ½) × tileSpan), and which version of
    /// the edit each was drawn from: old frames stay up until fresh ones replace them.
    private(set) var tiles: [Int: CGImage] = [:]
    @ObservationIgnored private var tileRev: [Int: Int] = [:]
    @ObservationIgnored private var tilesBusy: Set<Int> = []
    @ObservationIgnored private(set) var tileSpan: Double = 1
    private(set) var rev = 0
    /// Waveform levels per sound clip.
    private(set) var peaks: [String: [Float]] = [:]

    /// Long side of the preview render, in pixels (set from the stage's size).
    var previewMax: UInt32 = 960

    private let core = Core.shared
    @ObservationIgnored private var rendering = false
    @ObservationIgnored private var pending = false
    @ObservationIgnored private var link: CADisplayLink?
    @ObservationIgnored private var playStart: (host: CFTimeInterval, time: Double) = (0, 0)
    @ObservationIgnored private let audio = AudioOut()
    /// While recording a voiceover the playhead may run past the end of the edit.
    @ObservationIgnored var recordingPastEnd = false
    @ObservationIgnored private var gestureSent: CFTimeInterval = 0

    init(id: String) {
        self.id = id
    }

    var duration: Double { doc?.duration ?? 0 }
    var selectedClip: Clip? { doc?.clips.first { $0.id == selected } }
    var aspect: Double { guard let d = doc else { return 16 / 9 }; return d.canvas.width / d.canvas.height }

    func open() {
        let r = core.engine.openProject(id: id)
        if !r.ok { error = r.text }
        reload()
    }

    func close() {
        pause()
        core.engine.closeProject()
    }

    /// Run an edit tool on this project; the view updates from what the engine then reports.
    @discardableResult
    func edit(_ tool: String, _ args: [String: Any] = [:]) -> Bool {
        var a = args
        a["project"] = id
        let r = core.call(tool, a)
        if !r.ok { error = r.text }
        reload()
        return r.ok
    }

    /// Add a clip (add_clip, add_text) and select it.
    @discardableResult
    func add(_ tool: String, _ args: [String: Any]) -> Bool {
        var a = args
        a["project"] = id
        let r = core.call(tool, a)
        if r.ok, let j = try? JSONSerialization.jsonObject(with: Data(r.text.utf8)) as? [String: Any],
           let c = j["clip"] as? [String: Any], let cid = c["id"] as? String {
            selected = cid
        } else if !r.ok {
            error = r.text
        }
        reload()
        return r.ok
    }

    /// Bring a file into the project and lay it on `track` at the playhead (pushing later clips
    /// along), with any clip fields in `extra`; the new clip is selected.
    @discardableResult
    func addMedia(_ url: URL, track: String, duration: Double, extra: [String: Any]) -> Bool {
        let r = core.call("import_media", ["project": id, "paths": [url.path], "addToTimeline": false])
        guard r.ok, let j = try? JSONSerialization.jsonObject(with: Data(r.text.utf8)) as? [String: Any],
              let media = (j["media"] as? [[String: Any]])?.first?["id"] as? String else {
            error = r.ok ? "That file couldn't be added." : r.text
            return false
        }
        guard add("add_clip", ["asset": media, "track": track, "start": time, "duration": duration]) else { return false }
        if !extra.isEmpty, let s = selected { update(s, extra) }
        return true
    }

    /// Hold the selected clip's picture at the playhead for a moment: the clip splits there and a
    /// still of that frame goes between the halves.
    func freeze(_ c: Clip, hold: Double = 1.5) {
        guard c.isVisual, c.type != "text", time > c.start + 0.04, time < c.end - 0.04 else {
            error = "Put the playhead over the clip, then freeze."
            return
        }
        pause()
        let t = time
        guard let img = core.engine.renderClip(clipId: c.id, time: t, max: 1920)?.cgImage,
              let data = UIImage(cgImage: img).pngData() else { return }
        let url = core.mediaDir.appendingPathComponent("Freeze \(UUID().uuidString.prefix(4)).png")
        guard (try? data.write(to: url)) != nil else { return }
        edit("split_clip", ["clip": c.id, "time": t])
        addMedia(url, track: c.track, duration: hold, extra: [:])
    }

    // MARK: Gestures on the preview

    /// The edits until `endGesture` are one undo step, and go to the engine at most ~20 times a
    /// second so the preview keeps up with the finger.
    func beginGesture() {
        core.engine.beginGesture()
        gestureSent = 0
    }
    func gestureUpdate(_ clip: String, _ fields: [String: Any], final: Bool = false) {
        let now = CACurrentMediaTime()
        guard final || now - gestureSent > 0.05 else { return }
        gestureSent = now
        update(clip, fields)
    }
    func endGesture() {
        core.engine.endGesture()
        canUndo = core.engine.canUndo()
    }

    func update(_ clip: String, _ fields: [String: Any]) {
        var f = fields
        f["clip"] = clip
        edit("update_clip", f)
    }

    /// The project changed outside this model (Liemo AI): show what the engine has now.
    func reload() {
        if let d = try? core.decode(ProjectDoc.self, "get_project", ["project": id]) {
            doc = d
            if let s = selected, !d.clips.contains(where: { $0.id == s }) { selected = nil }
        }
        rev += 1
        canUndo = core.engine.canUndo()
        canRedo = core.engine.canRedo()
        requestFrame()
    }

    func setTileSpan(_ s: Double) {
        guard abs(s - tileSpan) > 1e-6 else { return }
        tileSpan = s
        tiles = [:]
        tileRev = [:]
    }

    /// Draw filmstrip frame `i` if it's missing or older than the edit.
    func loadTile(_ i: Int) {
        guard tileRev[i] != rev, !tilesBusy.contains(i) else { return }
        tilesBusy.insert(i)
        let (t, r, span, engine) = ((Double(i) + 0.5) * tileSpan, rev, tileSpan, core.engine)
        Task.detached(priority: .utility) {
            let img = engine.renderFrame(time: t, max: 140)?.cgImage
            await MainActor.run {
                self.tilesBusy.remove(i)
                guard span == self.tileSpan else { return }
                if let img { self.tiles[i] = img }
                self.tileRev[i] = r
                if r != self.rev { self.loadTile(i) }
            }
        }
    }

    func loadPeaks(_ c: Clip) {
        let (id, buckets, engine) = (c.id, UInt32(min(2000, max(16, c.dur * 24))), core.engine)
        Task.detached(priority: .utility) {
            let p = engine.audioPeaks(clipId: id, buckets: buckets)
            await MainActor.run { self.peaks[id] = p }
        }
    }

    func undo() { edit("undo") }
    func redo() { edit("redo") }

    // MARK: Preview

    /// Render the frame at `time` off the main thread; while one renders, only the latest
    /// request waits behind it.
    func requestFrame() {
        if rendering { pending = true; return }
        rendering = true
        let (t, max, engine) = (time, previewMax, core.engine)
        Task.detached(priority: .userInitiated) {
            let img = engine.renderFrame(time: t, max: max)?.cgImage
            await MainActor.run {
                if let img { self.frame = img }
                self.rendering = false
                if self.pending {
                    self.pending = false
                    self.requestFrame()
                }
            }
        }
    }

    // MARK: Playback

    func togglePlay() { playing ? pause() : play() }

    /// `silent`: picture only (while recording a voiceover, so the mic doesn't hear the mix).
    func play(silent: Bool = false) {
        guard duration > 0 || silent else { return }
        if time >= duration - 0.05 && !silent { time = 0 }
        playing = true
        playStart = (CACurrentMediaTime(), time)
        if !silent { audio.start(engine: core.engine, from: time) }
        let l = CADisplayLink(target: DisplayTarget { [weak self] in self?.tick() }, selector: #selector(DisplayTarget.fire))
        l.preferredFrameRateRange = CAFrameRateRange(minimum: 24, maximum: 30, preferred: 30)
        l.add(to: .main, forMode: .common)
        link = l
    }

    func pause() {
        guard playing else { return }
        playing = false
        link?.invalidate()
        link = nil
        audio.stop()
        requestFrame()
    }

    private func tick() {
        let t = playStart.time + (CACurrentMediaTime() - playStart.host)
        if t >= duration && !recordingPastEnd {
            time = duration
            pause()
            return
        }
        time = t
        requestFrame()
    }
}

private final class DisplayTarget: NSObject {
    let f: () -> Void
    init(_ f: @escaping () -> Void) { self.f = f }
    @objc func fire() { f() }
}

/// Plays the engine's mix: renders half-second blocks ahead and queues them on a player node.
final class AudioOut: @unchecked Sendable {
    private let avEngine = AVAudioEngine()
    private let node = AVAudioPlayerNode()
    private let format = AVAudioFormat(standardFormatWithSampleRate: 48000, channels: 2)!
    private let queue = DispatchQueue(label: "liemo.audio", qos: .userInitiated)
    private var generation = 0
    private var next: Double = 0
    private static let block: AVAudioFrameCount = 24000

    init() {
        avEngine.attach(node)
        avEngine.connect(node, to: avEngine.mainMixerNode, format: format)
    }

    func start(engine: Engine, from t: Double) {
        try? AVAudioSession.sharedInstance().setCategory(.playback, mode: .default)
        try? AVAudioSession.sharedInstance().setActive(true)
        queue.sync {
            generation += 1
            next = t
        }
        if !avEngine.isRunning { try? avEngine.start() }
        let gen = queue.sync { generation }
        for _ in 0..<3 { schedule(engine, gen) }
        node.play()
    }

    func stop() {
        queue.sync { generation += 1 }
        node.stop()
    }

    private func schedule(_ engine: Engine, _ gen: Int) {
        queue.async { [self] in
            guard gen == generation else { return }
            let t = next
            next += Double(Self.block) / 48000
            let samples = engine.renderAudio(start: t, frames: Self.block)
            guard let buf = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: Self.block) else { return }
            buf.frameLength = Self.block
            let l = buf.floatChannelData![0], r = buf.floatChannelData![1]
            for i in 0..<Int(Self.block) {
                l[i] = samples[i * 2]
                r[i] = samples[i * 2 + 1]
            }
            node.scheduleBuffer(buf, completionCallbackType: .dataConsumed) { [weak self] _ in
                self?.schedule(engine, gen)
            }
        }
    }
}
