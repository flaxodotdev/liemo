import Foundation

/// Tracks bundled with Liemo. All by Kevin MacLeod (incompetech.com), CC BY 4.0: free to use in
/// reels as long as the post credits him (the quick reel screen shows the line to paste).
struct Song: Identifiable, Hashable {
    let id: String
    let title: String
    let mood: String

    var url: URL? { Bundle.main.url(forResource: id, withExtension: "m4a") }
    var credit: String { "\u{201C}\(title)\u{201D} Kevin MacLeod (incompetech.com), licensed under CC BY 4.0" }

    static let library = [
        Song(id: "hitman", title: "Hitman", mood: "Dark · Hip-hop"),
        Song(id: "volatile-reaction", title: "Volatile Reaction", mood: "Heavy · Electronic"),
        Song(id: "exit-the-premises", title: "Exit the Premises", mood: "Driving · Electro"),
        Song(id: "raving-energy", title: "Raving Energy", mood: "EDM · Hype"),
        Song(id: "funkorama", title: "Funkorama", mood: "Funk · Groove"),
        Song(id: "pump", title: "Pump", mood: "Short · Punchy"),
        Song(id: "take-a-chance", title: "Take a Chance", mood: "Short · Upbeat"),
    ]
}

/// How the song is reworked.
enum Vibe: String, CaseIterable, Identifiable {
    case spedUp, slowed, original
    var id: String { rawValue }
    var name: String {
        switch self {
        case .spedUp: return "Sped up"
        case .slowed: return "Slowed + reverb"
        case .original: return "Original"
        }
    }
    /// Playback rate (pitch follows, like a record).
    var rate: Double {
        switch self {
        case .spedUp: return 1.25
        case .slowed: return 0.82
        case .original: return 1
        }
    }
    var reverb: Float {
        switch self {
        case .spedUp: return 14
        case .slowed: return 42
        case .original: return 0
        }
    }
}
