import SwiftUI

@main
struct LiemoApp: App {
    var body: some Scene {
        WindowGroup {
            ProjectsView()
                .preferredColorScheme(.dark)
                .tint(Theme.accent)
        }
    }
}
