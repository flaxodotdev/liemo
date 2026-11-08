import AVFoundation
import CoreGraphics
import Foundation
import Observation
import QuartzCore

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
    /// Long side of the preview render, in pixels (set from the stage's size).
    var previewMax: UInt32 = 960

    private let core = Core.shared
    @ObservationIgnored private var rendering = false
    @ObservationIgnored private var pending = false
    @ObservationIgnored private var link: CADisplayLink?
    @ObservationIgnored private var playStart: (host: CFTimeInterval, time: Double) = (0, 0)
    @ObservationIgnored private let audio = AudioOut()

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
        canUndo = core.engine.canUndo()
        canRedo = core.engine.canRedo()
        requestFrame()
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

    func play() {
        guard duration > 0 else { return }
        if time >= duration - 0.05 { time = 0 }
        playing = true
        playStart = (CACurrentMediaTime(), time)
        audio.start(engine: core.engine, from: time)
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
        if t >= duration {
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
