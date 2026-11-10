import SwiftUI

struct ExportPanel: View {
    let model: EditorModel
    @State private var settings = ExportSettings()
    @State private var progress: Double?
    @State private var result: URL?
    @State private var saved = false
    @State private var failure: String?
    @State private var exporter: Exporter?

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            Text("Export").font(.system(size: 17, weight: .semibold))
            if let p = progress {
                VStack(alignment: .leading, spacing: 10) {
                    ProgressView(value: p)
                    HStack {
                        Text("Rendering \(Int(p * 100))%").font(.system(size: 13)).foregroundStyle(Theme.muted)
                        Spacer()
                        Button("Cancel") { exporter?.cancel() }
                    }
                }
            } else if let url = result {
                VStack(alignment: .leading, spacing: 12) {
                    Label(saved ? "Saved to Photos" : "Exported", systemImage: "checkmark.circle.fill").foregroundStyle(.green)
                    HStack {
                        ShareLink(item: url) { Label("Share", systemImage: "square.and.arrow.up") }
                        Spacer()
                        Button("Export again") { result = nil; saved = false }
                    }
                }
            } else {
                Picker("Resolution", selection: $settings.resolution) {
                    Text("720p").tag(720); Text("1080p").tag(1080); Text("4K").tag(2160)
                }
                .pickerStyle(.segmented)
                Picker("Frame rate", selection: $settings.fps) {
                    Text("24 fps").tag(24); Text("30 fps").tag(30); Text("60 fps").tag(60)
                }
                .pickerStyle(.segmented)
                Text("\(model.doc?.aspect ?? "") · \(formatTime(model.duration)) · MP4, saved to Photos")
                    .font(.system(size: 13)).foregroundStyle(Theme.muted)
                if let f = failure { Text(f).font(.system(size: 13)).foregroundStyle(Theme.danger) }
                Button(action: start) {
                    Text("Export video").font(.system(size: 15, weight: .semibold))
                        .frame(maxWidth: .infinity).frame(height: 44)
                        .background(Color.white, in: RoundedRectangle(cornerRadius: 10)).foregroundStyle(.black)
                }
            }
            Spacer()
        }
        .padding(20)
        .onAppear { settings.fps = model.doc?.fps ?? 30; settings.resolution = model.doc?.exportResolution ?? 1080 }
    }

    private func start() {
        guard let doc = model.doc else { return }
        model.pause()
        failure = nil
        progress = 0
        let ex = Exporter()
        exporter = ex
        let s = settings
        Task {
            do {
                let url = try await Task.detached(priority: .userInitiated) {
                    try ex.run(doc: doc, settings: s) { p in Task { @MainActor in progress = p } }
                }.value
                progress = nil
                result = url
                do {
                    try await Exporter.saveToPhotos(url)
                    saved = true
                } catch {
                    failure = error.localizedDescription
                }
            } catch {
                progress = nil
                failure = error.localizedDescription
            }
        }
    }
}
