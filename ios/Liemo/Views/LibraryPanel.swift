import PhotosUI
import SwiftUI
import UniformTypeIdentifiers

struct LibraryPanel: View {
    let model: EditorModel
    let done: () -> Void
    @State private var library: Library?
    @State private var tab: String

    init(model: EditorModel, startTab: String = "footage", done: @escaping () -> Void) {
        self.model = model
        self.done = done
        _tab = State(initialValue: startTab)
    }
    @State private var picks: [PhotosPickerItem] = []
    @State private var importing = false
    @State private var filesOpen = false

    private let tabs = [("footage", "Footage"), ("backgrounds", "Backgrounds"), ("stills", "Stills"), ("overlays", "Overlays"), ("music", "Music"), ("text", "Text")]

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text("Add").font(.system(size: 17, weight: .semibold))
                Spacer()
                PhotosPicker(selection: $picks, maxSelectionCount: 20, matching: .any(of: [.images, .videos])) {
                    Label("Photos", systemImage: "photo.on.rectangle").font(.system(size: 14, weight: .medium))
                }
                Button { filesOpen = true } label: { Label("Files", systemImage: "folder").font(.system(size: 14, weight: .medium)) }
                    .padding(.leading, 10)
            }
            .padding(16)
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 6) {
                    ForEach(tabs, id: \.0) { t in
                        Button { tab = t.0 } label: {
                            Text(t.1).font(.system(size: 13, weight: .medium))
                                .padding(.horizontal, 12).frame(height: 30)
                                .background(tab == t.0 ? Color.white : Theme.surface2, in: Capsule())
                                .foregroundStyle(tab == t.0 ? Color.black : Theme.ink)
                        }
                    }
                }
                .padding(.horizontal, 16)
            }
            ScrollView {
                if importing { ProgressView("Importing…").padding(.top, 30) }
                LazyVGrid(columns: [GridItem(.adaptive(minimum: 140), spacing: 10)], spacing: 14) {
                    ForEach(items) { item in
                        Button { add(item) } label: { card(item) }.buttonStyle(.plain)
                    }
                }
                .padding(16)
            }
        }
        .onAppear { library = try? Core.shared.decode(Library.self, "list_library") }
        .onChange(of: picks) { _, p in if !p.isEmpty { importPhotos(p) } }
        .fileImporter(isPresented: $filesOpen, allowedContentTypes: [.movie, .audio, .image], allowsMultipleSelection: true) { r in
            if case .success(let urls) = r { importFiles(urls) }
        }
    }

    private var items: [Library.Item] {
        guard let l = library else { return [] }
        switch tab {
        case "backgrounds": return l.shader_backgrounds
        case "stills": return l.stills
        case "overlays": return l.overlays
        case "music": return l.music
        case "text": return l.textStyles
        default: return l.footage
        }
    }

    private func card(_ item: Library.Item) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Group {
                if tab == "music" {
                    ZStack { Theme.surface2; Image(systemName: "music.note").font(.system(size: 22)).foregroundStyle(Theme.muted) }
                } else {
                    EngineImage(key: "asset:\(item.id)") { Core.shared.engine.assetThumb(id: item.id) }
                }
            }
            .aspectRatio(16 / 9, contentMode: .fit)
            .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
            Text(item.name).font(.system(size: 13, weight: .medium)).lineLimit(1)
            if let a = item.about { Text(a).font(.system(size: 11)).foregroundStyle(Theme.muted).lineLimit(1) }
        }
    }

    private func add(_ item: Library.Item) {
        let ok = tab == "text"
            ? model.add("add_text", ["style": item.id, "content": item.name, "start": model.time])
            : model.add("add_clip", ["asset": item.id, "start": model.time])
        if ok { done() }
    }

    private func importPhotos(_ p: [PhotosPickerItem]) {
        importing = true
        Task {
            var paths: [String] = []
            for item in p {
                if let f = try? await item.loadTransferable(type: ImportedFile.self) { paths.append(f.url.path) }
            }
            picks = []
            finish(paths)
        }
    }

    private func importFiles(_ urls: [URL]) {
        var paths: [String] = []
        for u in urls {
            let scoped = u.startAccessingSecurityScopedResource()
            if let dst = try? ImportedFile.copy(u) { paths.append(dst.path) }
            if scoped { u.stopAccessingSecurityScopedResource() }
        }
        finish(paths)
    }

    private func finish(_ paths: [String]) {
        importing = false
        guard !paths.isEmpty else { return }
        if model.edit("import_media", ["paths": paths, "start": model.time]) { done() }
    }
}

/// A photo or video from the picker, copied into Liemo's imports folder.
struct ImportedFile: Transferable {
    let url: URL

    static var transferRepresentation: some TransferRepresentation {
        FileRepresentation(importedContentType: .movie) { ImportedFile(url: try copy($0.file)) }
        FileRepresentation(importedContentType: .image) { ImportedFile(url: try copy($0.file)) }
    }

    static func copy(_ src: URL) throws -> URL {
        let ext = src.pathExtension.isEmpty ? "mov" : src.pathExtension.lowercased()
        let dst = Core.shared.mediaDir.appendingPathComponent("\(UUID().uuidString.prefix(8))-\(src.deletingPathExtension().lastPathComponent).\(ext)")
        try FileManager.default.copyItem(at: src, to: dst)
        return dst
    }
}
