import AVFoundation
import SwiftUI
import UIKit

// MARK: - Text styles

/// Ready-made looks for text: font, colour and the extras (outline, shadow, glow, highlight,
/// box). Picking one keeps the words and size and replaces the rest of the look.
struct TextStyle: Identifiable {
    let id: String
    let name: String
    let fields: [String: Any]

    static let all: [TextStyle] = [
        TextStyle(id: "classic", name: "Classic", fields: ["font": "Mona Sans", "weight": 800, "color": "#ffffff"]),
        TextStyle(id: "outline", name: "Outline", fields: ["font": "Mona Sans", "weight": 800, "color": "#ffffff", "outline": "#000000", "outlineWidth": 8]),
        TextStyle(id: "comic", name: "Comic", fields: ["font": "Bebas Neue", "weight": 400, "color": "#ffe14d", "outline": "#111111", "outlineWidth": 10]),
        TextStyle(id: "shadow", name: "Shadow", fields: ["font": "Inter", "weight": 700, "color": "#ffffff", "shadow": "#000000"]),
        TextStyle(id: "neon-pink", name: "Neon", fields: ["font": "Mona Sans", "weight": 700, "color": "#fff0fb", "glow": "#ff2bd6"]),
        TextStyle(id: "neon-blue", name: "Neon blue", fields: ["font": "Mona Sans", "weight": 700, "color": "#eafcff", "glow": "#00c8ff"]),
        TextStyle(id: "highlight", name: "Highlight", fields: ["font": "Inter", "weight": 800, "color": "#111111", "highlight": "#ffd400"]),
        TextStyle(id: "tag", name: "Tag", fields: ["font": "Inter", "weight": 800, "color": "#ffffff", "highlight": "#ff3b5c"]),
        TextStyle(id: "label", name: "Label", fields: ["font": "Inter", "weight": 700, "color": "#111111", "highlight": "#ffffff"]),
        TextStyle(id: "serif", name: "Serif", fields: ["font": "Instrument Serif", "weight": 400, "color": "#ffffff", "shadow": "#000000"]),
        TextStyle(id: "poster", name: "Poster", fields: ["font": "Bebas Neue", "weight": 400, "color": "#ff7a3d"]),
        TextStyle(id: "caption", name: "Caption", fields: ["font": "Inter", "weight": 600, "color": "#ffffff", "box": true]),
    ]

    /// Everything a style sets, with the extras it doesn't use turned off.
    var update: [String: Any] {
        var f: [String: Any] = ["outline": "none", "shadow": "none", "glow": "none", "highlight": "none", "box": false]
        f.merge(fields) { $1 }
        return f
    }

    /// The JSON the engine draws a preview card from.
    func previewJSON(_ word: String) -> String {
        var p: [String: Any] = ["content": word, "size": 260, "align": "center", "anim": "none", "box": false, "y": 0]
        for (k, v) in fields { p[k] = v }
        let data = (try? JSONSerialization.data(withJSONObject: p)) ?? Data()
        return String(decoding: data, as: UTF8.self)
    }
}

/// The style gallery: engine-drawn cards, as the text will really look.
struct TextStyleStrip: View {
    let current: TextSpec
    let pick: (TextStyle) -> Void

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                ForEach(TextStyle.all) { s in
                    Button { pick(s) } label: {
                        VStack(spacing: 4) {
                            EngineImage(key: "ts:\(s.id)") { Core.shared.engine.textPreview(propsJson: s.previewJSON("Aa"), width: 192, height: 132) }
                                .frame(width: 84, height: 58)
                                .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
                                .overlay(RoundedRectangle(cornerRadius: 8, style: .continuous).stroke(matches(s) ? Color.white : Theme.border, lineWidth: matches(s) ? 2 : 1))
                            Text(s.name).font(.system(size: 11)).foregroundStyle(matches(s) ? Theme.ink : Theme.muted)
                        }
                    }
                    .buttonStyle(.plain)
                }
            }
        }
    }

    private func matches(_ s: TextStyle) -> Bool {
        let f = s.fields
        return (f["font"] as? String) == current.font && (f["color"] as? String) == current.color
            && (f["outline"] as? String) == current.outline && (f["glow"] as? String) == current.glow
            && (f["highlight"] as? String) == current.highlight && (f["shadow"] as? String) == current.shadow
            && ((f["box"] as? Bool) ?? false) == current.box
    }
}

/// An optional colour (outline, shadow, glow, highlight): a switch and the colour.
struct StyleColorRow: View {
    let label: String
    let value: String?
    let fallback: String
    let set: (String?) -> Void
    @State private var color: Color

    init(label: String, value: String?, fallback: String, set: @escaping (String?) -> Void) {
        self.label = label
        self.value = value
        self.fallback = fallback
        self.set = set
        _color = State(initialValue: Color(css: value ?? fallback) ?? .white)
    }

    var body: some View {
        HStack {
            Toggle(label, isOn: Binding(get: { value != nil }, set: { set($0 ? color.css : nil) }))
                .font(.system(size: 14))
            if value != nil {
                ColorPicker("", selection: $color, supportsOpacity: false).labelsHidden()
                    .onChange(of: color) { _, c in if c.css != value { set(c.css) } }
            }
        }
        .onChange(of: value) { _, v in if let v, v != color.css, let c = Color(css: v) { color = c } }
    }
}

// MARK: - Speed

struct SpeedPanel: View {
    let model: EditorModel
    let done: () -> Void
    @State private var speed: Double = 1

    private var clip: Clip? { model.selectedClip }

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            header("Speed", done: done)
            if let c = clip {
                VStack(alignment: .leading, spacing: 14) {
                    HStack {
                        Text(String(format: "%.2g×", speed)).font(.system(size: 28, weight: .semibold, design: .monospaced))
                        Spacer()
                        Text("\(formatTime(c.dur * (c.motion?.speed ?? 1) / speed)) long").font(.system(size: 13)).foregroundStyle(Theme.muted)
                    }
                    // Log scale, so 0.25× and 4× sit at the ends and 1× in the middle.
                    Slider(value: Binding(get: { log2(speed) }, set: { speed = pow(2, $0) }), in: -2...2) { editing in
                        if !editing { commit(c) }
                    }
                    HStack(spacing: 6) {
                        ForEach([0.25, 0.5, 1, 1.5, 2, 3, 4], id: \.self) { v in
                            Button { speed = v; commit(c) } label: {
                                Text(String(format: "%g×", v)).font(.system(size: 13, weight: .medium))
                                    .frame(maxWidth: .infinity).frame(height: 32)
                                    .background(abs(speed - v) < 0.01 ? Color.white : Theme.surface2, in: Capsule())
                                    .foregroundStyle(abs(speed - v) < 0.01 ? Color.black : Theme.ink)
                            }
                        }
                    }
                    Toggle("Play backwards", isOn: Binding(get: { c.motion?.reverse ?? false }, set: { model.update(c.id, ["reverse": $0]) }))
                        .font(.system(size: 14))
                    Text("The clip keeps the same footage: faster makes it shorter, slower longer. Its sound follows.")
                        .font(.system(size: 12)).foregroundStyle(Theme.muted)
                }
                .padding(.horizontal, 16)
            }
            Spacer()
        }
        .onAppear { speed = clip?.motion?.speed ?? 1 }
    }

    private func commit(_ c: Clip) {
        speed = (speed * 100).rounded() / 100
        if abs(speed - (c.motion?.speed ?? 1)) > 0.001 { model.update(c.id, ["speed": speed]) }
    }
}

// MARK: - Animations

struct AnimatePanel: View {
    let model: EditorModel
    let done: () -> Void
    @State private var phase = "in"

    private static let clipAnims: [(String, String, String)] = [
        ("none", "None", "nosign"), ("fade", "Fade", "circle.lefthalf.filled"), ("zoom-in", "Zoom in", "plus.magnifyingglass"),
        ("zoom-out", "Zoom out", "minus.magnifyingglass"), ("slide-left", "Slide left", "arrow.left"), ("slide-right", "Slide right", "arrow.right"),
        ("slide-up", "Slide up", "arrow.up"), ("slide-down", "Slide down", "arrow.down"), ("whip-left", "Whip left", "arrow.left.to.line"),
        ("whip-right", "Whip right", "arrow.right.to.line"), ("spin", "Spin", "arrow.trianglehead.clockwise"), ("pop", "Pop", "burst"),
        ("drop", "Drop", "arrow.down.to.line"), ("rise", "Rise", "arrow.up.to.line"), ("swing", "Swing", "metronome"),
        ("flip", "Flip", "rectangle.portrait.rotate"), ("stretch", "Stretch", "arrow.left.and.right"), ("shake", "Shake", "waveform.path"),
    ]
    private static let loops: [(String, String, String)] = [
        ("none", "None", "nosign"), ("breathe", "Breathe", "lungs"), ("float", "Float", "cloud"), ("wiggle", "Wiggle", "scribble"),
        ("pulse", "Pulse", "heart"), ("spin", "Spin", "arrow.trianglehead.clockwise"), ("shake", "Shake", "waveform.path"), ("sway", "Sway", "wind"),
    ]
    static let textAnims: [(String, String, String)] = [
        ("none", "None", "nosign"), ("fade", "Fade", "circle.lefthalf.filled"), ("rise", "Rise", "arrow.up"), ("slide", "Slide", "arrow.right"),
        ("descend", "Descend", "arrow.down"), ("type", "Type", "character.cursor.ibeam"), ("caret", "Typewriter", "keyboard"),
        ("pop", "Pop", "burst"), ("bounce", "Bounce", "arrow.down.to.line"), ("zoom", "Zoom", "plus.magnifyingglass"),
        ("settle", "Settle", "minus.magnifyingglass"), ("squeeze", "Squeeze", "arrow.right.and.line.vertical.and.arrow.left"),
        ("stretch", "Stretch", "arrow.left.and.right"), ("blur", "Blur", "aqi.medium"), ("fog", "Fog", "cloud.fog"),
        ("reveal", "Mask reveal", "rectangle.bottomhalf.filled"), ("wipe", "Wipe", "rectangle.lefthalf.filled"), ("echo", "Echo", "square.stack"),
        ("wave", "Wave", "water.waves"), ("drop", "Letter drop", "arrow.down.to.line.compact"), ("spring", "Letter spring", "arrow.up.and.down"),
        ("flip", "Rolodex", "rectangle.split.1x2"), ("tracking", "Tracking in", "arrow.right.and.line.vertical.and.arrow.left"),
        ("scramble", "Decode", "number"), ("words", "Word by word", "text.word.spacing"), ("wordrise", "Word rise", "text.alignleft"),
        ("wordpop", "Word pop", "text.bubble"), ("marker", "Marker", "highlighter"), ("underline", "Underline", "underline"),
        ("shimmer", "Shimmer", "sparkles"), ("count", "Count up", "textformat.123"), ("glitch", "Glitch", "tv"), ("flicker", "Flicker", "lightbulb"),
    ]

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            header("Animate", done: done)
            if let c = model.selectedClip {
                ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    if c.type == "text" {
                        grid(Self.textAnims, current: c.text?.anim ?? "none") { model.update(c.id, ["text": ["animation": $0]]) }
                    } else {
                        Picker("", selection: $phase) { Text("In").tag("in"); Text("Out").tag("out"); Text("Loop").tag("loop") }.pickerStyle(.segmented)
                        if phase == "loop" {
                            let cur = c.motion?.animLoop ?? ""
                            grid(Self.loops, current: cur.isEmpty ? "none" : cur) { model.update(c.id, ["animLoop": $0]) }
                            SliderRow(label: "Speed", value: c.motion?.animLoopSpeed ?? 1, range: 0.25...4, fmt: { String(format: "%.2g×", $0) }) {
                                model.update(c.id, ["animLoopSpeed": $0])
                            }
                            .id("loop")
                        } else {
                            let cur = phase == "in" ? c.motion?.animIn : c.motion?.animOut
                            grid(Self.clipAnims, current: (cur ?? "").isEmpty ? "none" : cur!) {
                                model.update(c.id, [phase == "in" ? "animIn" : "animOut": $0])
                            }
                            SliderRow(label: "Length", value: (phase == "in" ? c.motion?.animInDur : c.motion?.animOutDur) ?? 0.5,
                                      range: 0.1...min(3, max(0.2, c.dur / 2)), fmt: { String(format: "%.1fs", $0) }) {
                                model.update(c.id, [phase == "in" ? "animInDur" : "animOutDur": $0])
                            }
                            .id(phase)
                        }
                    }
                }
                .padding(.horizontal, 16).padding(.bottom, 20)
                }
            }
            Spacer(minLength: 0)
        }
    }

    private func grid(_ items: [(String, String, String)], current: String, pick: @escaping (String) -> Void) -> some View {
        LazyVGrid(columns: [GridItem(.adaptive(minimum: 72), spacing: 8)], spacing: 8) {
            ForEach(items, id: \.0) { id, name, icon in
                Button { pick(id) } label: {
                    VStack(spacing: 6) {
                        Image(systemName: icon).font(.system(size: 18))
                        Text(name).font(.system(size: 11))
                    }
                    .frame(maxWidth: .infinity).frame(height: 62)
                    .background(current == id ? Color.white : Theme.surface2, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
                    .foregroundStyle(current == id ? Color.black : Theme.ink)
                }
            }
        }
    }
}

// MARK: - Adjust

struct AdjustPanel: View {
    let model: EditorModel
    let done: () -> Void

    private static let sliders: [(String, String, ClosedRange<Double>)] = [
        ("brightness", "Brightness", -100...100), ("contrast", "Contrast", -100...100), ("saturation", "Saturation", -100...100),
        ("warmth", "Warmth", -100...100), ("tint", "Tint", -100...100), ("highlights", "Highlights", -100...100),
        ("shadows", "Shadows", -100...100), ("vignette", "Vignette", 0...100), ("grain", "Grain", 0...100), ("sharpen", "Sharpen", 0...100),
    ]

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text("Adjust").font(.system(size: 17, weight: .semibold))
                Spacer()
                if let c = model.selectedClip, let fx = adjust(c) {
                    Button("Reset") { model.edit("remove_effect", ["clip": c.id, "uid": fx.uid]) }.font(.system(size: 15))
                        .padding(.trailing, 12)
                }
                Button("Done", action: done).font(.system(size: 15, weight: .medium))
            }
            .padding(16)
            if let c = model.selectedClip {
                ScrollView {
                    VStack(spacing: 14) {
                        ForEach(Self.sliders, id: \.0) { key, label, range in
                            SliderRow(label: label, value: adjust(c)?.params[key]?.number ?? 0, range: range, fmt: { "\(Int($0))" }) { v in
                                set(c, key, v.rounded())
                            }
                        }
                    }
                    .padding(.horizontal, 16).padding(.bottom, 20)
                    .id(adjust(c)?.uid ?? "none")
                }
            }
        }
    }

    private func adjust(_ c: Clip) -> AppliedEffect? { c.effects.first { $0.id == "adjust" } }

    private func set(_ c: Clip, _ key: String, _ v: Double) {
        if let fx = adjust(c) {
            model.edit("update_effect", ["clip": c.id, "uid": fx.uid, "params": [key: v]])
        } else {
            model.edit("apply_effect", ["clip": c.id, "effect": "adjust", "params": [key: v]])
        }
    }
}

// MARK: - Sound (audio clips)

struct SoundPanel: View {
    let model: EditorModel
    let done: () -> Void

    private static let presets: [(String, String, String)] = [
        ("original", "Original", "waveform"), ("slowed-reverb", "Slowed + reverb", "tortoise"), ("sped-up", "Sped up", "hare"),
        ("nightcore", "Nightcore", "bolt"), ("daycore", "Daycore", "cloud"), ("hall", "Concert hall", "building.columns"),
        ("lofi", "Lo-fi", "radio"), ("underwater", "Underwater", "drop"), ("8d", "8D audio", "headphones"),
    ]

    var body: some View {
        VStack(spacing: 0) {
            header("Sound", done: done)
            if let c = model.selectedClip {
                ScrollView {
                    VStack(alignment: .leading, spacing: 16) {
                        LazyVGrid(columns: [GridItem(.adaptive(minimum: 96), spacing: 8)], spacing: 8) {
                            ForEach(Self.presets, id: \.0) { id, name, icon in
                                Button { model.update(c.id, ["sound": ["preset": id]]) } label: {
                                    VStack(spacing: 6) {
                                        Image(systemName: icon).font(.system(size: 17))
                                        Text(name).font(.system(size: 11)).lineLimit(1)
                                    }
                                    .frame(maxWidth: .infinity).frame(height: 60)
                                    .background(Theme.surface2, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
                                    .foregroundStyle(Theme.ink)
                                }
                            }
                        }
                        let s = c.sound
                        SliderRow(label: "Speed", value: s?.speed ?? 1, range: 0.5...1.6, fmt: { String(format: "%.2f×", $0) }) { model.update(c.id, ["sound": ["speed": $0]]) }
                        Toggle("Keep pitch", isOn: Binding(get: { s?.keepPitch ?? false }, set: { model.update(c.id, ["sound": ["keepPitch": $0]]) })).font(.system(size: 14))
                        SliderRow(label: "Reverb", value: s?.reverb ?? 0, range: 0...1, fmt: { "\(Int($0 * 100))%" }) { model.update(c.id, ["sound": ["reverb": $0]]) }
                        SliderRow(label: "Bass", value: s?.bass ?? 0, range: 0...1, fmt: { "\(Int($0 * 100))%" }) { model.update(c.id, ["sound": ["bass": $0]]) }
                        SliderRow(label: "Muffle", value: s?.muffle ?? 0, range: 0...1, fmt: { "\(Int($0 * 100))%" }) { model.update(c.id, ["sound": ["muffle": $0]]) }
                        SliderRow(label: "8D", value: s?.pan8d ?? 0, range: 0...1, fmt: { "\(Int($0 * 100))%" }) { model.update(c.id, ["sound": ["pan8d": $0]]) }
                    }
                    .padding(.horizontal, 16).padding(.bottom, 20)
                }
            }
        }
    }
}

// MARK: - Stickers

struct StickerPanel: View {
    let model: EditorModel
    let done: () -> Void

    private static let groups: [(String, [String])] = [
        ("Faces", ["😂", "🤣", "😭", "😍", "🥹", "😎", "🤯", "😳", "🥶", "🤡", "💀", "👀", "😤", "🫠", "🙄", "😈"]),
        ("Hands", ["🔥", "💯", "👍", "👎", "👏", "🙌", "🤝", "✌️", "🫶", "💪", "👉", "👈", "☝️", "🤞", "🫡", "🙏"]),
        ("Hearts", ["❤️", "🧡", "💛", "💚", "💙", "💜", "🖤", "🤍", "💔", "❤️‍🔥", "💖", "💥", "✨", "⭐️", "🌟", "⚡️"]),
        ("Things", ["🎵", "🎧", "📸", "🎬", "🎉", "🏆", "💸", "💎", "🚀", "🌈", "☀️", "🌙", "🌊", "🍿", "📍", "⏰"]),
        ("Signs", ["✅", "❌", "⚠️", "❓", "❗️", "💬", "🆕", "🔝", "🔴", "🟢", "➡️", "⬇️", "🔊", "📢", "🎯", "♻️"]),
    ]

    var body: some View {
        VStack(spacing: 0) {
            header("Stickers", done: done)
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    ForEach(Self.groups, id: \.0) { name, list in
                        Text(name.uppercased()).font(.system(size: 11, weight: .semibold)).foregroundStyle(Theme.muted)
                        LazyVGrid(columns: [GridItem(.adaptive(minimum: 52), spacing: 6)], spacing: 6) {
                            ForEach(list, id: \.self) { e in
                                Button { add(e) } label: {
                                    Text(e).font(.system(size: 34)).frame(maxWidth: .infinity).frame(height: 52)
                                        .background(Theme.surface2, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
                                }
                            }
                        }
                    }
                }
                .padding(.horizontal, 16).padding(.bottom, 20)
            }
        }
    }

    private func add(_ emoji: String) {
        guard let url = Self.file(for: emoji) else { return }
        if model.addMedia(url, track: "V2", duration: 3, extra: ["scale": 0.35]) { done() }
    }

    /// The emoji drawn big on a transparent square, kept for reuse.
    static func file(for emoji: String) -> URL? {
        let dir = Core.shared.mediaDir.appendingPathComponent("stickers", isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let name = emoji.unicodeScalars.map { String($0.value, radix: 16) }.joined(separator: "-")
        let url = dir.appendingPathComponent("Sticker \(name).png")
        if FileManager.default.fileExists(atPath: url.path) { return url }
        let size = CGSize(width: 640, height: 640)
        let fmt = UIGraphicsImageRendererFormat()
        fmt.scale = 1
        fmt.opaque = false
        let img = UIGraphicsImageRenderer(size: size, format: fmt).image { _ in
            let s = NSAttributedString(string: emoji, attributes: [.font: UIFont.systemFont(ofSize: 500)])
            let b = s.boundingRect(with: size, options: .usesLineFragmentOrigin, context: nil)
            s.draw(at: CGPoint(x: (size.width - b.width) / 2, y: (size.height - b.height) / 2))
        }
        guard let data = img.pngData(), (try? data.write(to: url)) != nil else { return nil }
        return url
    }
}

// MARK: - Sound effects

/// Sound effects made on the device (no licences to worry about), dropped on the voice & SFX
/// track at the playhead.
struct SfxPanel: View {
    let model: EditorModel
    let done: () -> Void
    @State private var player: AVAudioPlayer?

    var body: some View {
        VStack(spacing: 0) {
            header("Sound effects", done: done)
            ScrollView {
                VStack(spacing: 0) {
                    ForEach(Sfx.allCases) { s in
                        HStack(spacing: 12) {
                            Button { preview(s) } label: {
                                Image(systemName: "play.fill").font(.system(size: 12)).frame(width: 32, height: 32)
                                    .background(Theme.surface3, in: Circle())
                            }
                            VStack(alignment: .leading, spacing: 2) {
                                Text(s.name).font(.system(size: 15, weight: .medium))
                                Text(String(format: "%.1fs", s.length)).font(.system(size: 12)).foregroundStyle(Theme.muted)
                            }
                            Spacer()
                            Button("Add") { add(s) }.font(.system(size: 14, weight: .semibold))
                        }
                        .foregroundStyle(Theme.ink)
                        .padding(.vertical, 8).padding(.horizontal, 16)
                    }
                }
            }
        }
        .onDisappear { player?.stop() }
    }

    private func preview(_ s: Sfx) {
        guard let url = s.file() else { return }
        try? AVAudioSession.sharedInstance().setCategory(.playback)
        player = try? AVAudioPlayer(contentsOf: url)
        player?.play()
    }

    private func add(_ s: Sfx) {
        guard let url = s.file() else { return }
        if model.addMedia(url, track: "A2", duration: s.length, extra: [:]) { done() }
    }
}

enum Sfx: String, CaseIterable, Identifiable {
    case whoosh, swoosh, pop, click, hit, boom, bassDrop, riser, ding, glitch
    var id: String { rawValue }
    var name: String {
        switch self {
        case .bassDrop: return "Bass drop"
        default: return rawValue.prefix(1).uppercased() + rawValue.dropFirst()
        }
    }
    var length: Double {
        switch self {
        case .whoosh: return 0.7
        case .swoosh: return 0.35
        case .pop: return 0.15
        case .click: return 0.05
        case .hit: return 0.5
        case .boom: return 1.4
        case .bassDrop: return 1.6
        case .riser: return 2.0
        case .ding: return 1.3
        case .glitch: return 0.5
        }
    }

    /// The effect as a 48 kHz stereo file, synthesised once.
    func file() -> URL? {
        let dir = Core.shared.mediaDir.appendingPathComponent("sfx", isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let url = dir.appendingPathComponent("\(name).caf")
        if FileManager.default.fileExists(atPath: url.path) { return url }
        let sr = 48000.0
        let n = Int(length * sr)
        guard let fmt = AVAudioFormat(standardFormatWithSampleRate: sr, channels: 2),
              let buf = AVAudioPCMBuffer(pcmFormat: fmt, frameCapacity: AVAudioFrameCount(n)) else { return nil }
        buf.frameLength = AVAudioFrameCount(n)
        let l = buf.floatChannelData![0], r = buf.floatChannelData![1]
        var seed: UInt32 = 0x9e3779b9
        func noise() -> Float {
            seed ^= seed << 13; seed ^= seed >> 17; seed ^= seed << 5
            return Float(seed) / Float(UInt32.max) * 2 - 1
        }
        var lp: Float = 0, phase = 0.0
        for i in 0..<n {
            let t = Double(i) / sr, p = t / length
            var v: Float = 0, pan: Float = 0.5
            switch self {
            case .whoosh, .swoosh:
                // Filtered noise whose brightness and loudness swell and fall, sweeping across.
                let env = Float(sin(.pi * p)) * Float(sin(.pi * p))
                let cut = Float(0.02 + 0.25 * sin(.pi * p))
                lp += cut * (noise() - lp)
                v = lp * env * 2.2
                pan = Float(p)
            case .pop:
                phase += 2 * .pi * (180 + 520 * exp(-t * 40)) / sr
                v = Float(sin(phase)) * Float(exp(-t * 30))
            case .click:
                v = noise() * Float(exp(-t * 300))
            case .hit:
                phase += 2 * .pi * (55 + 90 * exp(-t * 20)) / sr
                v = Float(sin(phase)) * Float(exp(-t * 7)) + noise() * Float(exp(-t * 35)) * 0.5
            case .boom:
                phase += 2 * .pi * (38 + 70 * exp(-t * 6)) / sr
                lp += 0.05 * (noise() - lp)
                v = Float(tanh(1.6 * sin(phase))) * Float(exp(-t * 2.4)) + lp * Float(exp(-t * 8)) * 1.5
            case .bassDrop:
                phase += 2 * .pi * (30 + 120 * exp(-t * 2.2)) / sr
                v = Float(tanh(2.2 * sin(phase))) * Float(min(1, t * 30) * exp(-t * 1.1))
            case .riser:
                phase += 2 * .pi * (180 * pow(6, p)) / sr
                let saw = Float(phase.truncatingRemainder(dividingBy: 2 * .pi) / .pi - 1)
                lp += Float(0.02 + 0.3 * p) * (noise() - lp)
                v = (saw * 0.35 + lp * 1.2) * Float(pow(p, 1.6))
            case .ding:
                phase += 2 * .pi * 1318.5 / sr
                v = Float(sin(phase) + 0.5 * sin(phase * 2.01) + 0.25 * sin(phase * 3.02)) * Float(exp(-t * 3)) * 0.5
            case .glitch:
                let slot = Int(t * 40)
                let on = (slot * 7919 % 5) < 3
                phase += 2 * .pi * Double(220 * (1 + slot % 4)) / sr
                v = on ? (sin(phase) > 0 ? 0.45 : -0.45) + noise() * 0.2 : 0
            }
            v = max(-1, min(1, v * 0.8))
            l[i] = v * min(1, 2 * (1 - pan))
            r[i] = v * min(1, 2 * pan)
        }
        guard let file = try? AVAudioFile(forWriting: url, settings: fmt.settings), (try? file.write(from: buf)) != nil else { return nil }
        return url
    }
}

// MARK: - Cutout

/// Remove background (the device's AI finds the subject) or chroma key (a green or blue screen),
/// with edges, outline, shadow and what goes behind.
struct CutoutPanel: View {
    let model: EditorModel
    let done: () -> Void

    private var clip: Clip? { model.selectedClip }
    private var active: AppliedEffect? { clip?.effects.first { $0.id == "remove-bg" || $0.id == "chroma-key" } }

    var body: some View {
        VStack(spacing: 0) {
            header("Cutout", done: done)
            if let c = clip {
                ScrollView {
                    VStack(alignment: .leading, spacing: 14) {
                        HStack(spacing: 8) {
                            mode("Remove background", "person.crop.rectangle", id: "remove-bg", c)
                            mode("Chroma key", "eyedropper.halffull", id: "chroma-key", c)
                        }
                        if let fx = active {
                            Text(fx.id == "remove-bg"
                                 ? "Liemo's AI finds the people, pets and things in front. Put the clip on the overlay track to see what's underneath."
                                 : "Pick the screen's colour (the picker's eyedropper can sample the preview), then widen Similarity until it's gone.")
                                .font(.system(size: 12)).foregroundStyle(Theme.muted)
                            EffectRow(model: model, clip: c, effect: fx, expandedAtStart: true)
                            Button(role: .destructive) { model.edit("remove_effect", ["clip": c.id, "uid": fx.uid]) } label: {
                                Label("Turn off", systemImage: "xmark.circle").font(.system(size: 14))
                            }
                        }
                    }
                    .padding(.horizontal, 16).padding(.bottom, 20)
                }
            }
        }
    }

    private func mode(_ title: String, _ icon: String, id: String, _ c: Clip) -> some View {
        let on = active?.id == id
        return Button {
            if let fx = active, fx.id != id { model.edit("remove_effect", ["clip": c.id, "uid": fx.uid]) }
            if !on { model.edit("apply_effect", ["clip": c.id, "effect": id]) }
        } label: {
            VStack(spacing: 6) {
                Image(systemName: icon).font(.system(size: 20))
                Text(title).font(.system(size: 12, weight: .medium))
            }
            .frame(maxWidth: .infinity).frame(height: 70)
            .background(on ? Color.white : Theme.surface2, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
            .foregroundStyle(on ? Color.black : Theme.ink)
        }
    }
}
