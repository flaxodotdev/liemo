import SwiftUI

struct ProjectsView: View {
    @State private var projects: [ProjectSummary] = []
    @State private var path: [String] = []
    @State private var creating = false
    @State private var reeling = false
    @State private var renaming: ProjectSummary?
    @State private var newName = ""
    @State private var stamp = UUID()

    private let core = Core.shared

    var body: some View {
        NavigationStack(path: $path) {
            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    header
                    if projects.isEmpty {
                        empty
                    } else {
                        LazyVGrid(columns: [GridItem(.adaptive(minimum: 160, maximum: 300), spacing: 14)], spacing: 18) {
                            ForEach(projects) { p in
                                Button { path.append(p.id) } label: { ProjectCard(project: p, stamp: stamp) }
                                    .buttonStyle(.plain)
                                    .contextMenu { menu(p) }
                            }
                        }
                    }
                }
                .padding(.horizontal, 16)
                .padding(.bottom, 32)
            }
            .background(Theme.bg.ignoresSafeArea())
            .navigationDestination(for: String.self) { id in
                EditorView(id: id).toolbar(.hidden, for: .navigationBar)
            }
            .onAppear {
                refresh()
                // The launch-argument checks below run once per launch, not every time the list returns.
                guard !LaunchChecks.ran else { return }
                LaunchChecks.ran = true
                // For checks: `-liemo-reel-test <vibe>` makes a quick reel from the pictures in imports.
                if let v = UserDefaults.standard.string(forKey: "liemo-reel-test"), path.isEmpty {
                    let files = (try? FileManager.default.contentsOfDirectory(at: core.mediaDir, includingPropertiesForKeys: nil)) ?? []
                    var plan = ReelPlan()
                    plan.media = files.filter { ["jpg", "jpeg", "png", "mov", "mp4"].contains($0.pathExtension.lowercased()) }.sorted { $0.path < $1.path }
                    plan.song = Song.library[0].url
                    plan.songName = Song.library[0].title
                    plan.vibe = Vibe(rawValue: v) ?? .spedUp
                    plan.words = "pov: you finally stopped overthinking and started living"
                    let started = Date()
                    Task {
                        let id = try? await Task.detached { try ReelBuilder().build(plan) { print("reel:", $0) } }.value
                        print("reel: built in \(Date().timeIntervalSince(started))s ->", id ?? "failed")
                        refresh()
                        if let id { path.append(id) }
                    }
                }
                // For checks: `-liemo-project <id>` opens that project.
                if let id = UserDefaults.standard.string(forKey: "liemo-project"), path.isEmpty { path.append(id) }
                // For screenshots and UI checks: `-liemo-open-first` opens a project straight away.
                if ProcessInfo.processInfo.arguments.contains("-liemo-open-first"), path.isEmpty {
                    if projects.isEmpty, let r = try? core.decode(Created.self, "create_project", ["template": "sample"]) { refresh(); path.append(r.id) }
                    else if let p = projects.first { path.append(p.id) }
                }
            }
            .onChange(of: path) { _, p in if p.isEmpty { refresh() } }
            .sheet(isPresented: $creating) {
                NewProjectSheet { id in
                    creating = false
                    refresh()
                    if let id { path.append(id) }
                }
            }
            .fullScreenCover(isPresented: $reeling) {
                QuickReelView { id in
                    reeling = false
                    refresh()
                    if let id { path.append(id) }
                }
                .preferredColorScheme(.dark)
            }
            .alert("Rename project", isPresented: Binding(get: { renaming != nil }, set: { if !$0 { renaming = nil } })) {
                TextField("Name", text: $newName)
                Button("Cancel", role: .cancel) {}
                Button("Rename") {
                    if let p = renaming { core.call("set_project", ["project": p.id, "name": newName]) }
                    refresh()
                }
            }
        }
    }

    private var header: some View {
        HStack(spacing: 10) {
            LiemoMark(size: 26)
            Text("Liemo").font(.system(size: 22, weight: .semibold))
            Spacer()
            Button { reeling = true } label: {
                Label("Quick reel", systemImage: "wand.and.stars").font(.system(size: 15, weight: .semibold))
                    .padding(.horizontal, 14).frame(height: 36)
                    .background(Theme.surface2, in: Capsule()).foregroundStyle(Theme.ink)
            }
            Button { creating = true } label: {
                Label("New", systemImage: "plus").font(.system(size: 15, weight: .semibold))
                    .padding(.horizontal, 14).frame(height: 36)
                    .background(Color.white, in: Capsule()).foregroundStyle(.black)
            }
        }
        .padding(.top, 8)
    }

    private var empty: some View {
        VStack(spacing: 14) {
            Text("No projects yet").font(.system(size: 20, weight: .semibold))
            Text("Start from a format, or open the sample reel to see what Liemo does.")
                .font(.system(size: 15)).foregroundStyle(Theme.muted).multilineTextAlignment(.center)
            Button("Open the sample") {
                if let r = try? core.decode(Created.self, "create_project", ["template": "sample"]) {
                    refresh()
                    path.append(r.id)
                }
            }
            .buttonStyle(.bordered)
        }
        .frame(maxWidth: .infinity).padding(.vertical, 80)
    }

    @ViewBuilder private func menu(_ p: ProjectSummary) -> some View {
        Button { newName = p.name; renaming = p } label: { Label("Rename", systemImage: "pencil") }
        Button { _ = core.engine.duplicateProject(id: p.id); refresh() } label: { Label("Duplicate", systemImage: "plus.square.on.square") }
        Button(role: .destructive) { _ = core.engine.deleteProject(id: p.id); refresh() } label: { Label("Delete", systemImage: "trash") }
    }

    private func refresh() {
        projects = (try? core.decode(ProjectList.self, "list_projects"))?.projects ?? []
        stamp = UUID()
    }
}

struct Created: Decodable { let id: String }

@MainActor private enum LaunchChecks { static var ran = false }

private struct ProjectCard: View {
    let project: ProjectSummary
    let stamp: UUID

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            // Every card is the same box; the picture inside keeps the project's real shape
            // (a 9:16 reel stands tall, a 16:9 video lies wide) and never spills out of the card.
            Theme.surface2
                .aspectRatio(4 / 3, contentMode: .fit)
                .overlay {
                    if let img = UIImage(contentsOfFile: Core.shared.engine.projectThumbPath(id: project.id)) {
                        Image(uiImage: img).resizable().scaledToFit()
                            .clipShape(RoundedRectangle(cornerRadius: 4, style: .continuous))
                            .padding(8)
                    } else {
                        LiemoMark(size: 28).opacity(0.5)
                    }
                }
                .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
                .overlay(RoundedRectangle(cornerRadius: 10, style: .continuous).stroke(Theme.border))
                .id(stamp)
            Text(project.name).font(.system(size: 15, weight: .medium)).lineLimit(1)
            Text("\(project.aspect) · \(formatTime(project.duration)) · \(project.edited)")
                .font(.system(size: 12)).foregroundStyle(Theme.muted).lineLimit(1)
        }
    }
}

struct NewProjectSheet: View {
    let done: (String?) -> Void
    @State private var formats: [FormatDef] = []

    var body: some View {
        NavigationStack {
            List {
                Section {
                    Button { create(["template": "sample"]) } label: {
                        row(title: "Nature reel", sub: "The sample: footage, titles, music, effects", w: 16, h: 9)
                    }
                }
                Section("Formats") {
                    ForEach(formats) { f in
                        Button { create(["format": f.id]) } label: {
                            row(title: f.name, sub: "\(f.aspect) · \(f.bestFor)", w: f.canvas.first ?? 16, h: f.canvas.last ?? 9)
                        }
                    }
                }
            }
            .navigationTitle("New project")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { done(nil) } } }
            .onAppear { formats = (try? Core.shared.decode(FormatList.self, "list_formats"))?.formats ?? [] }
        }
    }

    private func row(title: String, sub: String, w: Double, h: Double) -> some View {
        HStack(spacing: 14) {
            ZStack {
                RoundedRectangle(cornerRadius: 3).stroke(Theme.muted, lineWidth: 1.5)
                    .aspectRatio(w / h, contentMode: .fit)
            }
            .frame(width: 30, height: 30)
            VStack(alignment: .leading, spacing: 2) {
                Text(title).foregroundStyle(Theme.ink)
                Text(sub).font(.system(size: 13)).foregroundStyle(Theme.muted)
            }
        }
    }

    private func create(_ args: [String: Any]) {
        done((try? Core.shared.decode(Created.self, "create_project", args))?.id)
    }
}
