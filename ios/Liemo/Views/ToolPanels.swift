import AVFoundation
import Speech
import SwiftUI

// MARK: - Filters

/// Looks for the selected clip, or for every clip on the main track when nothing's selected.
/// A clip wears one look at a time: picking another replaces it.
struct FilterPanel: View {
    let model: EditorModel
    let done: () -> Void
    @State private var strength = 0.8

    private var looks: [EffectDef] { EffectPicker.defs.filter { $0.id.hasPrefix("ig-") } }
    private var targets: [Clip] {
        if let c = model.selectedClip { return c.isVisual ? [c] : [] }
        return model.doc?.clips.filter { $0.track == "V1" } ?? []
    }
    private var current: String? {
        let ids = Set(targets.compactMap { $0.effects.first { $0.id.hasPrefix("ig-") }?.id })
        return ids.count == 1 ? ids.first : nil
    }

    var body: some View {
        VStack(spacing: 0) {
            header(model.selectedClip == nil ? "Filters · all clips" : "Filters", done: done)
            HStack {
                Text("Strength").font(.system(size: 14))
                Slider(value: $strength, in: 0.1...1)
                Text("\(Int(strength * 100))%").font(.system(size: 13, design: .monospaced)).foregroundStyle(Theme.muted).frame(width: 44)
            }
            .padding(.horizontal, 16)
            ScrollView {
                LazyVGrid(columns: [GridItem(.adaptive(minimum: 86), spacing: 10)], spacing: 12) {
                    cell(id: nil, name: "None") { ZStack { Theme.surface3; Image(systemName: "nosign").foregroundStyle(Theme.muted) } }
                    ForEach(looks) { e in
                        cell(id: e.id, name: e.name.replacingOccurrences(of: "IG ", with: "")) {
                            EngineImage(key: "fx:\(e.id)") { Core.shared.engine.effectThumb(id: e.id) }
                        }
                    }
                }
                .padding(16)
            }
        }
    }

    private func cell<P: View>(id: String?, name: String, @ViewBuilder picture: () -> P) -> some View {
        Button { apply(id) } label: {
            VStack(spacing: 5) {
                picture()
                    .aspectRatio(3 / 4, contentMode: .fit)
                    .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
                    .overlay(RoundedRectangle(cornerRadius: 8, style: .continuous).stroke(current == id ? Color.white : .clear, lineWidth: 2))
                Text(name).font(.system(size: 11)).lineLimit(1)
                    .foregroundStyle(current == id ? Theme.ink : Theme.muted)
            }
        }
        .buttonStyle(.plain)
    }

    private func apply(_ look: String?) {
        for c in targets {
            for e in c.effects where e.id.hasPrefix("ig-") {
                model.edit("remove_effect", ["clip": c.id, "uid": e.uid])
            }
            if let look { model.edit("apply_effect", ["clip": c.id, "effect": look, "amount": strength]) }
        }
    }
}

// MARK: - Voice

/// Record a voiceover from the playhead while the picture plays; it lands on the voice track.
struct VoicePanel: View {
    let model: EditorModel
    let done: () -> Void
    @State private var recorder: AVAudioRecorder?
    @State private var started: Double = 0
    @State private var level: Float = 0
    @State private var denied = false
    @State private var meter: Timer?

    var body: some View {
        VStack(spacing: 18) {
            header("Voiceover", done: { stop(save: false); done() })
            Text(recorder == nil ? "Recording starts at \(formatTime(model.time)). The picture plays so you can talk over it." : "Recording · \(formatTime(model.time - started))")
                .font(.system(size: 14)).foregroundStyle(Theme.muted).multilineTextAlignment(.center)
                .padding(.horizontal, 24)
            Button { recorder == nil ? start() : stop(save: true) } label: {
                ZStack {
                    Circle().stroke(Color.white, lineWidth: 4).frame(width: 78, height: 78)
                    if recorder == nil {
                        Circle().fill(Theme.danger).frame(width: 62, height: 62)
                    } else {
                        RoundedRectangle(cornerRadius: 6).fill(Theme.danger).frame(width: 30, height: 30)
                    }
                }
                .scaleEffect(1 + CGFloat(level) * 0.12)
                .animation(.easeOut(duration: 0.1), value: level)
            }
            if denied {
                Text("Liemo needs the microphone. Allow it in Settings › Privacy › Microphone.")
                    .font(.system(size: 13)).foregroundStyle(Theme.danger).multilineTextAlignment(.center)
            }
            Spacer()
        }
        .onDisappear { stop(save: false) }
    }

    private func start() {
        AVAudioApplication.requestRecordPermission { ok in
            Task { @MainActor in
                guard ok else { denied = true; return }
                let session = AVAudioSession.sharedInstance()
                try? session.setCategory(.playAndRecord, mode: .spokenAudio, options: [.defaultToSpeaker, .allowBluetoothHFP])
                try? session.setActive(true)
                let url = Core.shared.mediaDir.appendingPathComponent("Voiceover \(UUID().uuidString.prefix(4)).m4a")
                guard let r = try? AVAudioRecorder(url: url, settings: [
                    AVFormatIDKey: kAudioFormatMPEG4AAC, AVSampleRateKey: 48000, AVNumberOfChannelsKey: 1, AVEncoderBitRateKey: 128_000,
                ]) else { return }
                r.isMeteringEnabled = true
                started = model.time
                model.recordingPastEnd = true
                r.record()
                recorder = r
                model.play(silent: true)
                meter = Timer.scheduledTimer(withTimeInterval: 0.08, repeats: true) { _ in
                    Task { @MainActor in
                        r.updateMeters()
                        level = max(0, (r.averagePower(forChannel: 0) + 50) / 50)
                    }
                }
            }
        }
    }

    private func stop(save: Bool) {
        meter?.invalidate()
        meter = nil
        guard let r = recorder else { return }
        recorder = nil
        let length = model.time - started
        r.stop()
        model.pause()
        model.recordingPastEnd = false
        try? AVAudioSession.sharedInstance().setCategory(.playback)
        if save && length > 0.3 {
            model.edit("import_media", ["paths": [r.url.path], "start": started, "track": "A2"])
        } else {
            try? FileManager.default.removeItem(at: r.url)
        }
        level = 0
    }
}

// MARK: - Captions

/// What's said in the edit, transcribed on this device and laid on the Titles track as
/// captions a word or a few at a time.
struct CaptionsPanel: View {
    let model: EditorModel
    let done: () -> Void
    @State private var style = "pop"
    @State private var status: String?
    @State private var failure: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            header("Auto captions", done: done)
            Group {
                Text("Liemo listens to the edit's sound (voice, video audio) and writes captions that follow it. It runs on this device.")
                    .font(.system(size: 14)).foregroundStyle(Theme.muted)
                Picker("Style", selection: $style) {
                    Text("Pop").tag("pop"); Text("Clean").tag("clean"); Text("Boxed").tag("boxed")
                }
                .pickerStyle(.segmented)
                if let s = status {
                    HStack(spacing: 10) { ProgressView(); Text(s).font(.system(size: 14)) }
                } else {
                    Button(action: run) {
                        Text("Generate captions").font(.system(size: 15, weight: .semibold))
                            .frame(maxWidth: .infinity).frame(height: 46)
                            .background(Color.white, in: RoundedRectangle(cornerRadius: 12)).foregroundStyle(.black)
                    }
                }
                if let f = failure { Text(f).font(.system(size: 13)).foregroundStyle(Theme.danger) }
            }
            .padding(.horizontal, 16)
            Spacer()
        }
    }

    private func run() {
        failure = nil
        status = "Listening…"
        let (duration, engine) = (model.duration, Core.shared.engine)
        Task {
            do {
                guard await withCheckedContinuation({ c in SFSpeechRecognizer.requestAuthorization { c.resume(returning: $0 == .authorized) } }) else {
                    throw CoreError(message: "Allow speech recognition for Liemo in Settings to make captions.")
                }
                // The voice without the music, in pieces short enough for the recogniser.
                let chunks = try await Task.detached { try Self.writeChunks(engine.renderVoiceRange(start: 0, stop: duration)) }.value
                var segments: [(String, Double, Double)] = []
                for (i, c) in chunks.enumerated() {
                    status = chunks.count > 1 ? "Writing captions… \(i + 1) of \(chunks.count)" : "Writing captions…"
                    let words = try await Self.transcribe(c.url)
                    segments += words.map { ($0.0, $0.1 + c.offset, $0.2 + c.offset) }
                    try? FileManager.default.removeItem(at: c.url)
                }
                guard !segments.isEmpty else { throw CoreError(message: "No speech found. Captions listen to voice clips and video sound; music is left out.") }
                for chunk in Self.group(segments) { add(chunk) }
                status = nil
                done()
            } catch {
                status = nil
                failure = error.localizedDescription
            }
        }
    }

    private func add(_ c: (text: String, start: Double, end: Double)) {
        var a: [String: Any] = ["content": c.text, "start": c.start, "duration": max(0.25, c.end - c.start), "y": 260, "align": "center"]
        switch style {
        case "clean": a.merge(["font": "Inter", "weight": 600, "size": 52, "color": "#ffffff", "animation": "fade", "box": false]) { $1 }
        case "boxed": a.merge(["font": "Inter", "weight": 700, "size": 48, "color": "#ffffff", "animation": "none", "box": true]) { $1 }
        default: a.merge(["font": "Mona Sans", "weight": 800, "size": 76, "color": "#ffffff", "animation": "pop", "box": false]) { $1 }
        }
        model.edit("add_text", a)
    }

    /// Words → captions of up to three words and ~16 letters, split at pauses.
    static func group(_ words: [(String, Double, Double)]) -> [(text: String, start: Double, end: Double)] {
        var out: [(text: String, start: Double, end: Double)] = []
        var cur: [(String, Double, Double)] = []
        func flush() {
            guard let f = cur.first, let l = cur.last else { return }
            out.append((cur.map(\.0).joined(separator: " "), f.1, l.2))
            cur = []
        }
        for w in words {
            let len = cur.map(\.0.count).reduce(0, +) + w.0.count
            if let l = cur.last, cur.count >= 3 || len > 16 || w.1 - l.2 > 0.35 { flush() }
            cur.append(w)
        }
        flush()
        // Each caption holds until the next one starts (unless there's a long pause).
        for i in out.indices.dropLast() where out[i + 1].start - out[i].end < 0.6 {
            out[i].end = out[i + 1].start
        }
        return out
    }

    /// 48 kHz stereo → 16 kHz mono 16-bit WAV files of at most 50 s (the recogniser's sweet
    /// spot), each with where it starts in the edit. Silent pieces are left out.
    nonisolated private static func writeChunks(_ samples: [Float]) throws -> [(url: URL, offset: Double)] {
        let n = samples.count / 2
        guard n > 0 else { throw CoreError(message: "This edit has no voice or video sound to caption.") }
        // Average each 3 stereo frames into one mono sample.
        var mono = [Int16](repeating: 0, count: n / 3)
        for i in 0..<mono.count {
            var acc: Float = 0
            for j in 0..<3 { acc += samples[(i * 3 + j) * 2] + samples[(i * 3 + j) * 2 + 1] }
            mono[i] = Int16((acc / 6).clamped(-1, 1) * 32767)
        }
        let rate = 16000, piece = 50 * rate
        var out: [(URL, Double)] = []
        var start = 0
        while start < mono.count {
            let end = min(mono.count, start + piece)
            let part = mono[start..<end]
            if part.contains(where: { abs(Int($0)) > 300 }) {
                let url = FileManager.default.temporaryDirectory.appendingPathComponent("captions-\(UUID().uuidString).wav")
                try wav(Array(part), rate: rate).write(to: url)
                out.append((url, Double(start) / Double(rate)))
            }
            start = end
        }
        return out
    }

    nonisolated private static func wav(_ s: [Int16], rate: Int) -> Data {
        var d = Data()
        func u32(_ v: Int) { var x = UInt32(v).littleEndian; d.append(Data(bytes: &x, count: 4)) }
        func u16(_ v: Int) { var x = UInt16(v).littleEndian; d.append(Data(bytes: &x, count: 2)) }
        d.append("RIFF".data(using: .ascii)!); u32(36 + s.count * 2)
        d.append("WAVEfmt ".data(using: .ascii)!); u32(16); u16(1); u16(1); u32(rate); u32(rate * 2); u16(2); u16(16)
        d.append("data".data(using: .ascii)!); u32(s.count * 2)
        s.withUnsafeBufferPointer { d.append(Data(buffer: $0)) }
        return d
    }

    /// Words with their times in one piece of sound. On the device first; if that can't hear
    /// anything, Apple's servers (the language may lack an on-device model). Hearing nothing is an
    /// empty result, not a failure.
    private static func transcribe(_ url: URL) async throws -> [(String, Double, Double)] {
        guard let rec = SFSpeechRecognizer(), rec.isAvailable else { throw CoreError(message: "Speech recognition isn't available for this language.") }
        if rec.supportsOnDeviceRecognition, let words = try? await recognise(rec, url, onDevice: true), !words.isEmpty {
            return words
        }
        do {
            return try await recognise(rec, url, onDevice: false)
        } catch {
            let ns = error as NSError
            // "No speech detected" / "Empty recognition": nothing said in this piece.
            if ns.domain == "kAFAssistantErrorDomain" || ns.localizedDescription.lowercased().contains("speech") || ns.localizedDescription.lowercased().contains("empty") {
                return []
            }
            throw error
        }
    }

    private static func recognise(_ rec: SFSpeechRecognizer, _ url: URL, onDevice: Bool) async throws -> [(String, Double, Double)] {
        let req = SFSpeechURLRecognitionRequest(url: url)
        req.shouldReportPartialResults = false
        req.addsPunctuation = true
        req.requiresOnDeviceRecognition = onDevice
        return try await withCheckedThrowingContinuation { c in
            var finished = false
            rec.recognitionTask(with: req) { result, error in
                guard !finished else { return }
                if let error { finished = true; c.resume(throwing: error); return }
                guard let r = result, r.isFinal else { return }
                finished = true
                c.resume(returning: r.bestTranscription.segments.map { ($0.substring, $0.timestamp, $0.timestamp + $0.duration) })
            }
        }
    }
}
