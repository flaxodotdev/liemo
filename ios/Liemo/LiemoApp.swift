import SwiftUI

@main
struct LiemoApp: App {
    /// Liemo is a video editor and a music player; this is which one is showing (and opens next time).
    @AppStorage("liemo-mode") private var mode = AppMode.editor.rawValue

    var body: some Scene {
        WindowGroup {
            Group {
                if mode == AppMode.music.rawValue {
                    MusicHome().transition(.opacity)
                } else {
                    ProjectsView().transition(.opacity)
                }
            }
            .animation(.easeInOut(duration: 0.2), value: mode)
            .preferredColorScheme(.dark)
            .tint(Theme.accent)
        }
    }
}
