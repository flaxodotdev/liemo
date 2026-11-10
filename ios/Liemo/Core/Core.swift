import CoreGraphics
import Foundation
import UIKit

/// The Rust engine, shared by every screen. Its calls are synchronous and thread-safe (the
/// engine serialises them), so slow ones (rendering, export) run off the main thread.
final class Core: @unchecked Sendable {
    static let shared = Core()

    let engine: Engine
    let host = AVMediaHost()
    let dataDir: URL

    private init() {
        let support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        dataDir = support.appendingPathComponent("Liemo", isDirectory: true)
        try? FileManager.default.createDirectory(at: dataDir, withIntermediateDirectories: true)
        engine = Engine(dataDir: dataDir.path, host: host)
    }

    /// Where imported photos and videos are copied.
    var mediaDir: URL {
        let d = dataDir.appendingPathComponent("imports", isDirectory: true)
        try? FileManager.default.createDirectory(at: d, withIntermediateDirectories: true)
        return d
    }

    @discardableResult
    func call(_ tool: String, _ args: [String: Any] = [:]) -> ToolResult {
        let data = (try? JSONSerialization.data(withJSONObject: args)) ?? Data("{}".utf8)
        return engine.call(tool: tool, argsJson: String(decoding: data, as: UTF8.self))
    }

    func decode<T: Decodable>(_ type: T.Type, _ tool: String, _ args: [String: Any] = [:]) throws -> T {
        let r = call(tool, args)
        guard r.ok else { throw CoreError(message: r.text) }
        return try JSONDecoder().decode(T.self, from: Data(r.text.utf8))
    }

    // Library pictures are drawn by the engine once and kept.
    private let thumbs = NSCache<NSString, CGImage>()
    func thumb(_ key: String, _ make: () -> MediaFrame?) -> CGImage? {
        if let c = thumbs.object(forKey: key as NSString) { return c }
        guard let img = make()?.cgImage else { return nil }
        thumbs.setObject(img, forKey: key as NSString)
        return img
    }
}

struct CoreError: LocalizedError {
    let message: String
    var errorDescription: String? { message }
}

extension MediaFrame {
    /// The engine's straight-alpha RGBA as a CGImage.
    var cgImage: CGImage? {
        guard width > 0, height > 0, let provider = CGDataProvider(data: rgba as CFData) else { return nil }
        return CGImage(
            width: Int(width), height: Int(height), bitsPerComponent: 8, bitsPerPixel: 32, bytesPerRow: Int(width) * 4,
            space: CGColorSpace(name: CGColorSpace.sRGB)!,
            bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.last.rawValue),
            provider: provider, decode: nil, shouldInterpolate: true, intent: .defaultIntent)
    }
}

// MARK: - What the tools return

struct ProjectList: Decodable { let projects: [ProjectSummary] }

struct ProjectSummary: Decodable, Identifiable, Hashable {
    let id: String
    let name: String
    let format: String
    let aspect: String
    let duration: Double
    let clips: Int
    let edited: String
}

struct ProjectDoc: Decodable {
    struct Canvas: Decodable { let width: Double; let height: Double }
    let id: String
    let name: String
    let format: String
    let aspect: String
    let fps: Int
    let exportResolution: Int
    let background: String
    let canvas: Canvas
    let duration: Double
    let tracks: [Track]
    let clips: [Clip]
}

struct Track: Decodable, Identifiable, Hashable {
    let id: String
    let name: String
    let kind: String
    let hidden: Bool
    let muted: Bool
    let locked: Bool
}

struct Clip: Decodable, Identifiable, Hashable {
    let id: String
    let asset: String
    let type: String
    let name: String
    let track: String
    let start: Double
    let dur: Double
    let `in`: Double
    let x: Double
    let y: Double
    let scale: Double
    let rot: Double
    let opacity: Double
    let fadeIn: Double
    let fadeOut: Double
    let volume: Double
    let end: Double
    let transition: ClipTransition?
    let effects: [AppliedEffect]
    let text: TextSpec?
    let motion: Motion?
    let sound: SoundFx?

    var isVisual: Bool { !["music", "audio"].contains(type) }
    var hasSound: Bool { ["music", "audio", "video"].contains(type) }
}

struct ClipTransition: Decodable, Hashable { let id: String; let dur: Double }

/// Speed, direction, flips, entrance and exit of a picture clip.
struct Motion: Decodable, Hashable {
    let speed: Double
    let reverse: Bool
    let flipH: Bool
    let flipV: Bool
    let animIn: String
    let animInDur: Double
    let animOut: String
    let animOutDur: Double
    let animLoop: String?
    let animLoopSpeed: Double?
}

/// An audio clip's reworking (slowed, reverb, 8D…).
struct SoundFx: Decodable, Hashable {
    let speed: Double
    let keepPitch: Bool
    let reverb: Double
    let room: Double
    let bass: Double
    let muffle: Double
    let pan8d: Double
}

struct AppliedEffect: Decodable, Hashable, Identifiable {
    let uid: String
    let id: String
    let enabled: Bool
    let amount: Double
    let params: [String: JSONValue]
}

struct TextSpec: Decodable, Hashable {
    let content: String
    let font: String
    let size: Double
    let weight: Double
    let color: String
    let align: String
    let anim: String
    let box: Bool
    let y: Double
    let outline: String?
    let outlineWidth: Double?
    let shadow: String?
    let glow: String?
    let highlight: String?
    /// normal, invert (reads on any footage) or exclusion (a softer invert).
    let blend: String?
}

struct Library: Decodable {
    struct Item: Decodable, Identifiable, Hashable {
        let id: String
        let name: String
        let about: String?
        let duration: Double?
    }
    struct Font: Decodable, Hashable { let family: String; let role: String }
    let footage: [Item]
    let shader_backgrounds: [Item]
    let stills: [Item]
    let overlays: [Item]
    let music: [Item]
    let textStyles: [Item]
    let fonts: [Font]
    let installedFonts: [String]
}

struct EffectList: Decodable { let effects: [EffectDef] }
struct EffectDef: Decodable, Identifiable, Hashable {
    struct Param: Decodable, Hashable {
        let key: String
        let label: String
        let type: String
        let options: [String]?
        let min: Double?
        let max: Double?
        let step: Double?
        let unit: String?
        let `default`: JSONValue?
    }
    let id: String
    let name: String
    let group: String
    /// Effects with nothing to set have no params.
    let params: [Param]?
}

struct TransitionList: Decodable { let transitions: [TransitionDef] }
struct TransitionDef: Decodable, Identifiable, Hashable { let id: String; let name: String; let group: String }

struct FormatList: Decodable { let formats: [FormatDef] }
struct FormatDef: Decodable, Identifiable, Hashable {
    let id: String
    let name: String
    let aspect: String
    let canvas: [Double]
    let bestFor: String
}

/// Any JSON value (effect parameters).
enum JSONValue: Codable, Hashable {
    case number(Double), string(String), bool(Bool), null

    init(from decoder: Decoder) throws {
        let c = try decoder.singleValueContainer()
        if let b = try? c.decode(Bool.self) { self = .bool(b) }
        else if let n = try? c.decode(Double.self) { self = .number(n) }
        else if let s = try? c.decode(String.self) { self = .string(s) }
        else { self = .null }
    }
    func encode(to encoder: Encoder) throws {
        var c = encoder.singleValueContainer()
        switch self {
        case .number(let n): try c.encode(n)
        case .string(let s): try c.encode(s)
        case .bool(let b): try c.encode(b)
        case .null: try c.encodeNil()
        }
    }
    var number: Double? { if case .number(let n) = self { return n }; return nil }
    var string: String? { if case .string(let s) = self { return s }; return nil }
    var bool: Bool? { if case .bool(let b) = self { return b }; return nil }
    var any: Any {
        switch self {
        case .number(let n): return n
        case .string(let s): return s
        case .bool(let b): return b
        case .null: return NSNull()
        }
    }
}

func formatTime(_ t: Double) -> String {
    let t = max(0, t)
    let m = Int(t) / 60, s = Int(t) % 60, f = Int((t - floor(t)) * 10)
    return String(format: "%d:%02d.%d", m, s, f)
}
