import AVFoundation
import PhotosUI
import SwiftUI

/// Pick footage, a song and a vibe, add some words: Liemo cuts the reel.
struct QuickReelView: View {
    let done: (String?) -> Void

    @State private var plan = ReelPlan()
    @State private var picks: [PhotosPickerItem] = []
    @State private var thumbs: [UIImage] = []
    @State private var loadingMedia = false
    @State private var song: Song? = Song.library.first
    @State private var ownSong: URL?
    @State private var songFromVideo: PhotosPickerItem?
    @State private var filesOpen = false
    @State private var preview: AVAudioPlayer?
    @State private var playing: String?
    @State private var progress: String?
    @State private var failure: String?

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 26) {
                    mediaSection
                    songSection
                    vibeSection
                    wordsSection
                    if let f = failure { Text(f).font(.system(size: 14)).foregroundStyle(Theme.danger) }
                }
                .padding(16)
                .padding(.bottom, 90)
            }
            .background(Theme.bg)
            .safeAreaInset(edge: .bottom) { makeButton }
            .navigationTitle("Quick reel")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { stopPreview(); done(nil) } } }
            .overlay { if let p = progress { working(p) } }
            .interactiveDismissDisabled(progress != nil)
        }
        .onChange(of: picks) { _, p in loadMedia(p) }
        .onChange(of: songFromVideo) { _, v in if let v { loadSongVideo(v) } }
        .fileImporter(isPresented: $filesOpen, allowedContentTypes: [.audio]) { r in
            if case .success(let u) = r {
                let scoped = u.startAccessingSecurityScopedResource()
                ownSong = try? ImportedFile.copy(u)
                if scoped { u.stopAccessingSecurityScopedResource() }
                if ownSong != nil { song = nil }
            }
        }
    }

    // MARK: Sections

    private func title(_ t: String, _ sub: String? = nil) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(t).font(.system(size: 17, weight: .semibold))
            if let sub { Text(sub).font(.system(size: 13)).foregroundStyle(Theme.muted) }
        }
    }

    private var mediaSection: some View {
        VStack(alignment: .leading, spacing: 12) {
            title("Footage", "Photos and videos, in the order they'll play")
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 8) {
                    PhotosPicker(selection: $picks, maxSelectionCount: 30, selectionBehavior: .ordered, matching: .any(of: [.images, .videos])) {
                        VStack(spacing: 6) {
                            Image(systemName: loadingMedia ? "hourglass" : "plus").font(.system(size: 20))
                            Text(plan.media.isEmpty ? "Add" : "Change").font(.system(size: 12))
                        }
                        .frame(width: 72, height: 110)
                        .background(Theme.surface2, in: RoundedRectangle(cornerRadius: 10))
                        .foregroundStyle(Theme.ink)
                    }
                    ForEach(Array(thumbs.enumerated()), id: \.offset) { _, img in
                        Image(uiImage: img).resizable().scaledToFill()
                            .frame(width: 62, height: 110).clipShape(RoundedRectangle(cornerRadius: 10))
                    }
                }
            }
        }
    }

    private var songSection: some View {
        VStack(alignment: .leading, spacing: 10) {
            title("Song")
            HStack(spacing: 8) {
                Button { filesOpen = true } label: { pill("Your song", "music.note", ownSong != nil && song == nil) }
                PhotosPicker(selection: $songFromVideo, matching: .videos) { pill("From a video", "film", false) }
            }
            if let u = ownSong, song == nil {
                HStack {
                    Image(systemName: "checkmark.circle.fill").foregroundStyle(.green)
                    Text(u.deletingPathExtension().lastPathComponent.split(separator: "-", maxSplits: 1).last.map(String.init) ?? "Your song")
                        .font(.system(size: 14)).lineLimit(1)
                }
            }
            VStack(spacing: 0) {
                ForEach(Song.library) { s in
                    HStack(spacing: 12) {
                        Button { togglePreview(s) } label: {
                            Image(systemName: playing == s.id ? "stop.fill" : "play.fill").font(.system(size: 13))
                                .frame(width: 34, height: 34).background(Theme.surface3, in: Circle())
                        }
                        VStack(alignment: .leading, spacing: 2) {
                            Text(s.title).font(.system(size: 15, weight: .medium))
                            Text(s.mood).font(.system(size: 12)).foregroundStyle(Theme.muted)
                        }
                        Spacer()
                        if song == s { Image(systemName: "checkmark").foregroundStyle(Theme.accent) }
                    }
                    .foregroundStyle(Theme.ink)
                    .padding(.vertical, 8).padding(.horizontal, 12)
                    .contentShape(Rectangle())
                    .onTapGesture { song = s }
                }
            }
            .background(Theme.surface1, in: RoundedRectangle(cornerRadius: 12))
            if let s = song {
                Text("Credit when you post: \(s.credit)").font(.system(size: 12)).foregroundStyle(Theme.muted)
                    .textSelection(.enabled)
            }
        }
    }

    private var vibeSection: some View {
        VStack(alignment: .leading, spacing: 12) {
            title("Vibe")
            Picker("Vibe", selection: $plan.vibe) {
                ForEach(Vibe.allCases) { Text($0.name).tag($0) }
            }
            .pickerStyle(.segmented)
            Picker("Length", selection: $plan.length) {
                ForEach([10.0, 15, 20, 30], id: \.self) { Text("\(Int($0))s").tag($0) }
            }
            .pickerStyle(.segmented)
        }
    }

    private var wordsSection: some View {
        VStack(alignment: .leading, spacing: 10) {
            title("Words", "They pop in on the beat, a word or two at a time")
            TextField("pov: you finally stopped overthinking", text: $plan.words, axis: .vertical)
                .lineLimit(2...6)
                .padding(12).background(Theme.surface2, in: RoundedRectangle(cornerRadius: 10))
        }
    }

    private var makeButton: some View {
        Button(action: make) {
            Label("Make reel", systemImage: "wand.and.stars").font(.system(size: 16, weight: .semibold))
                .frame(maxWidth: .infinity).frame(height: 50)
                .background(ready ? Color.white : Theme.surface3, in: RoundedRectangle(cornerRadius: 14))
                .foregroundStyle(ready ? Color.black : Theme.muted)
        }
        .disabled(!ready)
        .padding(.horizontal, 16).padding(.vertical, 10)
        .background(Theme.bg)
    }

    private func pill(_ t: String, _ icon: String, _ on: Bool) -> some View {
        Label(t, systemImage: icon).font(.system(size: 13, weight: .medium))
            .padding(.horizontal, 12).frame(height: 32)
            .background(on ? Color.white : Theme.surface2, in: Capsule())
            .foregroundStyle(on ? Color.black : Theme.ink)
    }

    private func working(_ p: String) -> some View {
        ZStack {
            Color.black.opacity(0.75).ignoresSafeArea()
            VStack(spacing: 14) {
                ProgressView().controlSize(.large)
                Text(p).font(.system(size: 15, weight: .medium))
            }
        }
    }

    // MARK: Actions

    private var ready: Bool { !plan.media.isEmpty && (song != nil || ownSong != nil) && !loadingMedia }

    private func loadMedia(_ items: [PhotosPickerItem]) {
        loadingMedia = true
        Task {
            var urls: [URL] = [], imgs: [UIImage] = []
            for item in items {
                guard let f = try? await item.loadTransferable(type: ImportedFile.self) else { continue }
                urls.append(f.url)
                if let img = await thumbnail(f.url) { imgs.append(img) }
            }
            plan.media = urls
            thumbs = imgs
            loadingMedia = false
        }
    }

    private func thumbnail(_ url: URL) async -> UIImage? {
        if let img = UIImage(contentsOfFile: url.path) { return img.preparingThumbnail(of: CGSize(width: 180, height: 320)) }
        let gen = AVAssetImageGenerator(asset: AVURLAsset(url: url))
        gen.appliesPreferredTrackTransform = true
        gen.maximumSize = CGSize(width: 180, height: 320)
        return (try? await gen.image(at: .zero).image).map(UIImage.init(cgImage:))
    }

    private func loadSongVideo(_ item: PhotosPickerItem) {
        Task {
            if let f = try? await item.loadTransferable(type: ImportedFile.self), let a = try? await SongFX.extractAudio(from: f.url) {
                ownSong = a
                song = nil
            } else {
                failure = "That video's sound couldn't be read."
            }
            songFromVideo = nil
        }
    }

    private func togglePreview(_ s: Song) {
        if playing == s.id { stopPreview(); return }
        stopPreview()
        guard let u = s.url, let p = try? AVAudioPlayer(contentsOf: u) else { return }
        try? AVAudioSession.sharedInstance().setCategory(.playback)
        p.currentTime = min(30, p.duration / 3)
        p.play()
        preview = p
        playing = s.id
        song = s
    }

    private func stopPreview() {
        preview?.stop()
        preview = nil
        playing = nil
    }

    private func make() {
        stopPreview()
        failure = nil
        var p = plan
        if let s = song {
            p.song = s.url
            p.songName = s.title
        } else {
            p.song = ownSong
            p.songName = "Your song"
        }
        progress = "Starting…"
        let plan = p
        Task {
            do {
                let id = try await Task.detached(priority: .userInitiated) {
                    try ReelBuilder().build(plan) { s in Task { @MainActor in progress = s } }
                }.value
                progress = nil
                done(id)
            } catch {
                progress = nil
                failure = error.localizedDescription
            }
        }
    }
}
