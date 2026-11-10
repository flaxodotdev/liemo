import SwiftUI

/// The desktop app's palette.
enum Theme {
    static let bg = Color(hex: 0x090909)
    static let surface1 = Color(hex: 0x141414)
    static let surface2 = Color(hex: 0x1c1c1c)
    static let surface3 = Color(hex: 0x242424)
    static let border = Color(hex: 0x262626)
    static let ink = Color.white
    static let muted = Color(hex: 0x999999)
    static let accent = Color(hex: 0x0099ff)
    static let danger = Color(hex: 0xff5577)

    static func clipColor(_ type: String) -> Color {
        switch type {
        case "text": return Color(hex: 0x3b2f55)
        case "music", "audio": return Color(hex: 0x1d3d31)
        case "overlay": return Color(hex: 0x333333)
        default: return Color(hex: 0x223552)
        }
    }
}

extension Color {
    init(hex: UInt32) {
        self.init(red: Double((hex >> 16) & 0xff) / 255, green: Double((hex >> 8) & 0xff) / 255, blue: Double(hex & 0xff) / 255)
    }
    init?(css: String) {
        let s = css.trimmingCharacters(in: CharacterSet(charactersIn: "#"))
        guard s.count == 6, let v = UInt32(s, radix: 16) else { return nil }
        self.init(hex: v)
    }
    var css: String {
        let c = UIColor(self).cgColor.converted(to: CGColorSpace(name: CGColorSpace.sRGB)!, intent: .defaultIntent, options: nil)?.components ?? [1, 1, 1]
        let v = c.prefix(3).map { Int(($0 * 255).rounded()).clamped(0, 255) }
        return String(format: "#%02x%02x%02x", v[0], v[1], v.count > 2 ? v[2] : v[1])
    }
}

extension Comparable {
    func clamped(_ lo: Self, _ hi: Self) -> Self { min(max(self, lo), hi) }
}

/// The L mark.
struct LiemoMark: View {
    var size: CGFloat = 22
    var body: some View {
        Image("LiemoMark").resizable().interpolation(.high).frame(width: size, height: size)
            .clipShape(RoundedRectangle(cornerRadius: size * 0.24, style: .continuous))
    }
}

/// An engine-drawn picture (library cards, effect and transition previews).
struct EngineImage: View {
    let key: String
    let make: @Sendable () -> MediaFrame?
    @State private var image: CGImage?
    var body: some View {
        // The box takes the size it's given; the picture fills it and is cut to it, never
        // pushing the box wider (scaledToFill inside a ZStack would).
        // Hit-testing isn't clipped, so the filled picture must not take taps either: otherwise
        // its spill-over catches taps meant for the card next to it.
        Theme.surface3
            .overlay {
                if let image { Image(decorative: image, scale: 1).resizable().scaledToFill().allowsHitTesting(false) }
            }
            .clipped()
            .contentShape(Rectangle())
        .task(id: key) {
            if image != nil { return }
            let (k, m) = (key, make)
            image = await Task.detached(priority: .utility) { Core.shared.thumb(k, m) }.value
        }
    }
}
