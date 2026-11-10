import Accelerate
import AVFoundation

enum SongFX {
    /// Render `length` seconds of output from `source`, starting `start` seconds into it, at the
    /// vibe's speed with its reverb, to an AAC file at `out`.
    static func render(source: URL, start: Double, length: Double, vibe: Vibe, to out: URL) throws {
        let file = try AVAudioFile(forReading: source)
        let fmt = file.processingFormat
        let engine = AVAudioEngine()
        let player = AVAudioPlayerNode()
        let speed = AVAudioUnitVarispeed()
        let reverb = AVAudioUnitReverb()
        speed.rate = Float(vibe.rate)
        reverb.loadFactoryPreset(.largeHall2)
        reverb.wetDryMix = vibe.reverb
        for n in [player, speed, reverb] as [AVAudioNode] { engine.attach(n) }
        engine.connect(player, to: speed, format: fmt)
        engine.connect(speed, to: reverb, format: fmt)
        engine.connect(reverb, to: engine.mainMixerNode, format: fmt)

        let outFmt = AVAudioFormat(standardFormatWithSampleRate: 48000, channels: 2)!
        try engine.enableManualRenderingMode(.offline, format: outFmt, maximumFrameCount: 4096)
        try engine.start()

        let first = AVAudioFramePosition(max(0, start) * fmt.sampleRate)
        guard first < file.length else { throw CoreError(message: "The song is shorter than that.") }
        let need = AVAudioFramePosition((length * vibe.rate + 1) * fmt.sampleRate)
        player.scheduleSegment(file, startingFrame: first, frameCount: AVAudioFrameCount(min(need, file.length - first)), at: nil)
        player.play()

        try? FileManager.default.removeItem(at: out)
        let writer = try AVAudioFile(forWriting: out, settings: [
            AVFormatIDKey: kAudioFormatMPEG4AAC, AVSampleRateKey: 48000, AVNumberOfChannelsKey: 2, AVEncoderBitRateKey: 192_000,
        ], commonFormat: .pcmFormatFloat32, interleaved: false)
        let total = AVAudioFramePosition(length * 48000)
        let buf = AVAudioPCMBuffer(pcmFormat: engine.manualRenderingFormat, frameCapacity: engine.manualRenderingMaximumFrameCount)!
        while engine.manualRenderingSampleTime < total {
            let n = min(buf.frameCapacity, AVAudioFrameCount(total - engine.manualRenderingSampleTime))
            switch try engine.renderOffline(n, to: buf) {
            case .success: try writer.write(from: buf)
            case .error: throw CoreError(message: "The song couldn't be processed.")
            default: break
            }
        }
        player.stop()
        engine.stop()
    }

    /// Where the song hits hardest: the start of the `window`-second stretch with the most
    /// energy, favouring a jump up from what came before (the drop).
    static func dropStart(_ samples: [Float], window: Double) -> Double {
        let block = 12000          // 0.25 s of stereo frames at 48 kHz
        let n = samples.count / 2 / block
        guard n > 8 else { return 0 }
        var e = [Float](repeating: 0, count: n)
        samples.withUnsafeBufferPointer { p in
            for i in 0..<n {
                var ms: Float = 0
                vDSP_measqv(p.baseAddress! + i * block * 2, 1, &ms, vDSP_Length(block * 2))
                e[i] = sqrt(ms)
            }
        }
        let w = max(1, Int(window / 0.25)), look = 16   // 4 s either side of the cut
        guard n > w else { return 0 }
        func mean(_ a: Int, _ b: Int) -> Float {
            let lo = max(0, a), hi = min(n, b)
            return hi > lo ? e[lo..<hi].reduce(0, +) / Float(hi - lo) : 0
        }
        var best = 0, score = -Float.infinity
        for s in 0...(n - w) {
            // A jump only counts with real music before it to jump from.
            let jump = s >= look ? mean(s, s + look) - mean(s - look, s) : 0
            let v = mean(s, s + w) + 0.8 * jump
            if v > score { score = v; best = s }
        }
        return Double(best) * 0.25
    }

    /// Tempo and the first beat of 48 kHz interleaved stereo. With `near`, only tempos within a
    /// few percent of it are considered (the phase is what's wanted then).
    static func beats(_ samples: [Float], near: Double? = nil) -> (bpm: Double, first: Double) {
        let hop = 512
        let n = samples.count / 2 / hop
        guard n > 200 else { return (near ?? 120, 0) }
        var energy = [Float](repeating: 0, count: n)
        samples.withUnsafeBufferPointer { p in
            for i in 0..<n {
                var ms: Float = 0
                vDSP_measqv(p.baseAddress! + i * hop * 2, 1, &ms, vDSP_Length(hop * 2))
                energy[i] = log(ms + 1e-6)
            }
        }
        // Onset strength (how much louder each hop is than the last), centred: a curve that's
        // always positive correlates with itself at every lag.
        var onset = [Float](repeating: 0, count: n)
        for i in 1..<n { onset[i] = max(0, energy[i] - energy[i - 1]) }
        let avg = onset.reduce(0, +) / Float(n)
        onset = onset.map { $0 - avg }

        func ac(_ lag: Double) -> Double {
            let L = Int(lag), f = Float(lag - Double(L))
            guard n - L - 1 > 0 else { return 0 }
            var dot: Float = 0
            for t in 0..<(n - L - 1) { dot += onset[t] * (onset[t + L] * (1 - f) + onset[t + L + 1] * f) }
            return Double(dot) / Double(n - L - 1)
        }
        let rate = 48000.0 / Double(hop)
        let range = near.map { ($0 * 0.96, $0 * 1.04) } ?? (70, 180)
        var bestLag = 60 / (near ?? 120) * rate, bestScore = -Double.infinity
        for bpm in stride(from: range.0, through: range.1, by: 0.25) {
            let lag = 60 / bpm * rate
            // The real beat also lines up at two and four beats; a 3:2 ghost of it doesn't.
            var s = ac(lag) + 0.5 * ac(2 * lag) + 0.25 * ac(4 * lag)
            if near == nil {
                // A light nudge toward the tempos clips are cut to, to settle half/double ties.
                s *= 0.85 + 0.15 * exp(-pow(log2(bpm / 125), 2) / 0.5)
            }
            if s > bestScore { bestScore = s; bestLag = lag }
        }
        // Phase: where a grid of that spacing lines up with the most onsets.
        var bestOff = 0, bestSum: Float = -.infinity
        for off in 0..<max(1, Int(bestLag)) {
            var sum: Float = 0
            var t = Double(off)
            while Int(t) < n { sum += onset[Int(t)]; t += bestLag }
            if sum > bestSum { bestSum = sum; bestOff = off }
        }
        return (60 * rate / bestLag, Double(bestOff) / rate)
    }

    /// The sound of a video, as an M4A next to it.
    static func extractAudio(from video: URL) async throws -> URL {
        let out = video.deletingPathExtension().appendingPathExtension("m4a")
        try? FileManager.default.removeItem(at: out)
        guard let ex = AVAssetExportSession(asset: AVURLAsset(url: video), presetName: AVAssetExportPresetAppleM4A) else {
            throw CoreError(message: "That video's sound couldn't be read.")
        }
        try await ex.export(to: out, as: .m4a)
        return out
    }
}
