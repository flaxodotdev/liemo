import AVFoundation
import PhotosUI
import SwiftUI

/// Pick footage, a song and a vibe, add some words: Liemo cuts the reel. Songs you bring in
/// are recognised and get their real lyrics from the web, so the words land on their moments.
struct QuickReelView: View {
    let done: (String?) -> Void
    /// A song to start with (from the music player): recognised and given its lyrics like any other.
    var song: URL? = nil
    @State private var songTaken = false

    enum Pick {
        case bundled(Song)
        case saved(SavedSong)

        var id: String {
            switch self {
            case .bundled(let s): return s.id
            case .saved(let s): return s.id
            }
        }
        var name: String {
            switch self {
            case .bundled(let s): return s.title
            case .saved(let s): return s.title
            }
        }
        var url: URL? {
            switch self {
            case .bundled(let s): return s.url
            case .saved(let s): return s.url
            }
        }
    }

    @State private var plan = ReelPlan()
    @State private var picks: [PhotosPickerItem] = []
    @State private var thumbs: [UIImage] = []
    @State private var loadingMedia = false
    @State private var pick: Pick? = .bundled(Song.library.first!)
    @State private var songFromVideo: PhotosPickerItem?
    @State private var filesOpen = false
    @State private var preview: AVAudioPlayer?
    @State private var playing: String?
    @State private var progress: String?
    @State private var songWork: String?
    @State private var failure: String?
    @State private var editing: SavedSong?
    @State private var libraryOpen = false
    // Customizations: where the song starts, and where each clip starts.
    @State private var songStartChoice = 0        // 0 the drop, 1 the start, 2 a custom moment
    @State private var customStart = 0.0
    @State private var songDuration = 0.0
    @State private var trimming: Int?             // index into the footage, when choosing its start

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 26) {
                    mediaSection
                    songSection
                    vibeSection
                    wordsSection
                    captionsSection
                    if let f = failure { Text(f).font(.system(size: 14)).foregroundStyle(Theme.danger) }
                }
                .padding(16)
                .padding(.bottom, 90)
            }
            .background(Theme.bg)
            .safeAreaInset(edge: .bottom) { makeButton }
            .navigationTitle("Quick reel")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { stopPreview(); done(nil) } }
                ToolbarItem(placement: .primaryAction) {
                    Button { libraryOpen = true } label: { Image(systemName: "music.note.list") }
                }
            }
            .overlay { if let p = progress ?? songWork { working(p) } }
            .interactiveDismissDisabled(progress != nil || songWork != nil)
            .navigationDestination(isPresented: $libraryOpen) { SongLibraryView() }
        }
        .onAppear {
            if let song, !songTaken { songTaken = true; own(song) }
        }
        .sheet(item: $editing) { song in
            LyricsEditor(song: song) { updated in
                SongLibrary.shared.update(updated)
                if case .saved(let current) = pick, current.id == updated.id { pick = .saved(updated) }
            }
            .preferredColorScheme(.dark)
        }
        .sheet(isPresented: Binding(get: { trimming != nil }, set: { if !$0 { trimming = nil } })) {
            if let i = trimming, i < plan.media.count {
                ClipStartSheet(url: plan.media[i], initial: plan.videoStarts.count > i ? plan.videoStarts[i] ?? 0 : 0) { v in
                    if plan.videoStarts.count > i { plan.videoStarts[i] = v }
                }
                .preferredColorScheme(.dark)
            }
        }
        .onChange(of: picks) { _, p in loadMedia(p) }
        .onChange(of: songFromVideo) { _, v in if let v { loadSongVideo(v) } }
        .onChange(of: pick?.id) { _, _ in measureSong() }
        .fileImporter(isPresented: $filesOpen, allowedContentTypes: [.audio]) { r in
            if case .success(let u) = r {
                let scoped = u.startAccessingSecurityScopedResource()
                let copied = try? ImportedFile.copy(u)
                if scoped { u.stopAccessingSecurityScopedResource() }
                if let copied { own(copied) }
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
            title("Footage", "In the order they'll play · tap one to choose where it starts")
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
                    ForEach(Array(thumbs.enumerated()), id: \.offset) { i, img in
                        Button { trimming = i } label: {
                            Image(uiImage: img).resizable().scaledToFill()
                                .frame(width: 62, height: 110).clipShape(RoundedRectangle(cornerRadius: 10))
                                .overlay(alignment: .bottomTrailing) {
                                    if plan.videoStarts.count > i, plan.videoStarts[i] != nil {
                                        Image(systemName: "clock.arrow.circlepath").font(.system(size: 9, weight: .bold))
                                            .padding(4).background(Color.black.opacity(0.65), in: Circle())
                                            .foregroundStyle(.white)
                                    }
                                }
                        }
                        .buttonStyle(.plain)
                    }
                }
            }
        }
    }

    private var songSection: some View {
        VStack(alignment: .leading, spacing: 10) {
            title("Song", "Yours get recognised and pick up their lyrics from the web")
            HStack(spacing: 8) {
                Button { filesOpen = true } label: { pill("Your song", "music.note", pick == nil) }
                PhotosPicker(selection: $songFromVideo, matching: .videos) { pill("From a video", "film", false) }
            }
            if !SongLibrary.shared.songs.isEmpty { savedList }
            VStack(spacing: 0) {
                ForEach(Song.library) { s in
                    row(id: s.id, main: s.title, sub: s.mood) {
                        pick = .bundled(s)
                        fillWords()
                    } onPlay: { togglePreview(.bundled(s)) }
                }
            }
            .background(Theme.surface1, in: RoundedRectangle(cornerRadius: 12))
            lyricNotes
            if songDuration > plan.length { songStartSection }
        }
    }

    private var savedList: some View {
        VStack(spacing: 0) {
            ForEach(SongLibrary.shared.songs) { s in
                row(id: s.id, main: s.title, sub: s.artist.isEmpty ? "Your song" : s.artist) {
                    pick = .saved(s)
                    fillWords()
                } onPlay: { togglePreview(.saved(s)) }
            }
        }
        .background(Theme.surface1, in: RoundedRectangle(cornerRadius: 12))
    }

    /// What the picked song brings: its credit, its lyrics, and the buttons to fix either.
    @ViewBuilder private var lyricNotes: some View {
        switch pick {
        case .bundled(let s):
            Text("Credit when you post: \(s.credit)").font(.system(size: 12)).foregroundStyle(Theme.muted)
                .textSelection(.enabled)
        case .saved(let s):
            HStack(spacing: 8) {
                Image(systemName: s.hasSynced ? "waveform" : "text.quote").font(.system(size: 12))
                    .foregroundStyle(s.hasSynced ? Theme.accent : Theme.muted)
                Text(s.hasSynced
                     ? (s.wordSynced ? "Word-by-word synced"
                        : s.offset != 0 ? "Synced · timing fixed (\(s.offset >= 0 ? "+" : "")\(String(format: "%.1f", s.offset))s)"
                        : "Lyrics synced to the music")
                     : (s.plainText.isEmpty ? "No lyrics yet" : "Lyrics on the beat"))
                    .font(.system(size: 12)).foregroundStyle(Theme.muted)
                Spacer()
                if !s.hasSynced, !s.artist.isEmpty {
                    Button("Find lyrics") { findLyrics(s) }
                        .font(.system(size: 12, weight: .medium)).foregroundStyle(Theme.accent)
                }
                Button("Edit") { editing = s }
                    .font(.system(size: 12, weight: .medium)).foregroundStyle(Theme.accent)
            }
        case nil:
            EmptyView()
        }
    }

    private func row(id: String, main: String, sub: String, onSelect: @escaping () -> Void, onPlay: @escaping () -> Void) -> some View {
        HStack(spacing: 12) {
            Button(action: onPlay) {
                Image(systemName: playing == id ? "stop.fill" : "play.fill").font(.system(size: 13))
                    .frame(width: 34, height: 34).background(Theme.surface3, in: Circle())
            }
            VStack(alignment: .leading, spacing: 2) {
                Text(main).font(.system(size: 15, weight: .medium)).lineLimit(1)
                Text(sub).font(.system(size: 12)).foregroundStyle(Theme.muted).lineLimit(1)
            }
            Spacer()
            if pick?.id == id { Image(systemName: "checkmark").foregroundStyle(Theme.accent) }
        }
        .foregroundStyle(Theme.ink)
        .padding(.vertical, 8).padding(.horizontal, 12)
        .contentShape(Rectangle())
        .onTapGesture(perform: onSelect)
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

    /// Which part of the song plays: the drop (picked for you), the very start, or a moment
    /// the user scrubs to and hears.
    private var songStartSection: some View {
        VStack(alignment: .leading, spacing: 10) {
            title("Start at")
            Picker("Start at", selection: $songStartChoice) {
                Text("The drop").tag(0)
                Text("The start").tag(1)
                Text("Pick").tag(2)
            }
            .pickerStyle(.segmented)
            if songStartChoice == 2 {
                let maxStart = max(0.1, songDuration - plan.length)
                HStack(spacing: 12) {
                    Button { playFrom(customStart) } label: {
                        Image(systemName: playing == "scrub" ? "stop.fill" : "play.fill").font(.system(size: 12))
                            .frame(width: 30, height: 30).background(Theme.surface3, in: Circle())
                    }
                    .foregroundStyle(Theme.ink)
                    VStack(alignment: .leading, spacing: 4) {
                        Slider(value: $customStart, in: 0...maxStart)
                        Text("\(tc(customStart)) of \(tc(songDuration))").font(.system(size: 11)).foregroundStyle(Theme.muted).monospacedDigit()
                    }
                }
            }
        }
    }

    private func tc(_ t: Double) -> String {
        String(format: "%d:%02d", Int(t) / 60, Int(t) % 60)
    }

    /// Hear the song from the scrubbed moment.
    private func playFrom(_ at: Double) {
        if playing == "scrub" { stopPreview(); return }
        stopPreview()
        guard let u = pick?.url, let player = try? AVAudioPlayer(contentsOf: u) else { return }
        try? AVAudioSession.sharedInstance().setCategory(.playback)
        player.currentTime = at
        player.play()
        preview = player
        playing = "scrub"
    }

    private var captionsSection: some View {
        VStack(alignment: .leading, spacing: 12) {
            title("Captions", "How the words on the reel look")
            if case .saved(let s) = pick, s.hasSynced {
                Toggle(isOn: $plan.captions) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text("Use the song's lyrics").font(.system(size: 15))
                        Text("Off keeps just the words you typed below").font(.system(size: 12)).foregroundStyle(Theme.muted)
                    }
                }
                .font(.system(size: 15))
                if plan.captions {
                    Toggle("Word by word", isOn: $plan.wordByWord).font(.system(size: 15))
                }
            }
            VStack(alignment: .leading, spacing: 8) {
                Text("Font").font(.system(size: 12)).foregroundStyle(Theme.muted)
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 8) {
                        ForEach(Self.captionFonts, id: \.self) { f in
                            Button { plan.font = f } label: { pill(f, "textformat", plan.font == f) }
                        }
                    }
                }
            }
            VStack(alignment: .leading, spacing: 8) {
                Text("Style").font(.system(size: 12)).foregroundStyle(Theme.muted)
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 8) {
                        ForEach(CaptionStyle.allCases) { s in
                            Button { plan.style = s } label: { pill(s.name, "paintbrush", plan.style == s) }
                        }
                    }
                }
            }
            VStack(alignment: .leading, spacing: 8) {
                Text("Placement").font(.system(size: 12)).foregroundStyle(Theme.muted)
                Picker("Placement", selection: $plan.placement) {
                    ForEach(CaptionPlacement.allCases) { p in
                        Text(p.name).tag(p)
                    }
                }
                .pickerStyle(.segmented)
            }
            VStack(alignment: .leading, spacing: 8) {
                Text("Size").font(.system(size: 12)).foregroundStyle(Theme.muted)
                Picker("Size", selection: $plan.size) {
                    ForEach(CaptionSize.allCases) { s in
                        Text(s.name).tag(s)
                    }
                }
                .pickerStyle(.segmented)
            }
            HStack(spacing: 8) {
                Text("Align").font(.system(size: 12)).foregroundStyle(Theme.muted)
                Spacer()
                ForEach(CaptionAlign.allCases) { al in
                    Button {
                        plan.align = al
                    } label: {
                        Image(systemName: al == .left ? "text.alignleft" : al == .right ? "text.alignright" : "text.aligncenter")
                            .font(.system(size: 13, weight: .medium))
                            .frame(width: 44, height: 30)
                            .background(plan.align == al ? Theme.ink : Theme.surface2, in: RoundedRectangle(cornerRadius: 8))
                            .foregroundStyle(plan.align == al ? Theme.bg : Theme.ink)
                    }
                    .buttonStyle(.plain)
                }
            }
        }
    }

    static let captionFonts = ["Bebas Neue", "Mona Sans", "Inter", "Instrument Serif", "JetBrains Mono", "Caveat"]

    private var wordsSection: some View {
        VStack(alignment: .leading, spacing: 10) {
            title("Words", pick == nil ? nil : "From the song's lyrics, or your own")
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

    private var ready: Bool { pick?.url != nil && !plan.media.isEmpty && !loadingMedia }

    /// How long the picked song runs, so "start at" has a range to scrub over.
    private func measureSong() {
        stopPreview()
        songStartChoice = 0
        customStart = 0
        songDuration = 0
        guard let u = pick?.url, let f = try? AVAudioFile(forReading: u) else { return }
        songDuration = Double(f.length) / f.processingFormat.sampleRate
    }

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
            plan.videoStarts = Array(repeating: nil, count: urls.count)
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

    /// A song you brought in: recognised, given its lyrics, kept for next time.
    private func own(_ url: URL) {
        songWork = "Recognizing the song…"
        let name = url.deletingPathExtension().lastPathComponent.split(separator: "-", maxSplits: 1).last.map(String.init) ?? "Your song"
        Task {
            let (match, lyrics) = await SongMatcher.identify(url: url, fallbackName: String(name))
            let saved = SongLibrary.shared.add(url: url, title: match.title, artist: match.artist, synced: lyrics.synced, plain: lyrics.plain)
            pick = .saved(saved)
            fillWords()
            songWork = nil
        }
    }

    private func loadSongVideo(_ item: PhotosPickerItem) {        Task {
            if let f = try? await item.loadTransferable(type: ImportedFile.self), let a = try? await SongFX.extractAudio(from: f.url) {
                own(a)
            } else {
                failure = "That video's sound couldn't be read."
            }
            songFromVideo = nil
        }
    }

    /// The picked song's words go into the field, ready to trim.
    private func fillWords() {
        if case .saved(let s) = pick { plan.words = s.plainText }
    }

    private func findLyrics(_ song: SavedSong) {
        songWork = "Finding the lyrics…"
        Task {
            var s = song
            let duration = s.url.map { (try? AVAudioFile(forReading: $0)).map { Double($0.length) / $0.processingFormat.sampleRate } } ?? nil
            if let got = try? await SongMatcher.fetchLyrics(artist: s.artist, track: s.title, duration: duration) {
                s.synced = got.synced ?? s.synced
                s.plain = got.plain ?? s.plain
                SongLibrary.shared.update(s)
                if case .saved(let current) = pick, current.id == s.id {
                    pick = .saved(s)
                    fillWords()
                }
            }
            songWork = nil
        }
    }

    private func togglePreview(_ p: Pick) {
        if playing == p.id { stopPreview(); return }
        stopPreview()
        guard let u = p.url, let player = try? AVAudioPlayer(contentsOf: u) else { return }
        try? AVAudioSession.sharedInstance().setCategory(.playback)
        player.currentTime = min(30, player.duration / 3)
        player.play()
        preview = player
        playing = p.id
        pick = p
        if case .saved = p { fillWords() }
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
        p.song = pick?.url
        p.songName = pick?.name ?? ""
        p.songStart = switch songStartChoice {
        case 1: 0
        case 2: customStart
        default: nil
        }
        if case .saved(let s) = pick {
            // The song's own calibration shift rides on every stamp: whatever the calibrator
            // saved is the truth for this file, not whatever LRCLIB guessed.
            p.lyrics = s.lines.map { LyricLine(time: $0.time + s.offset, text: $0.text) }
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

/// Choose where a piece of footage starts in its own file: scrub the slider, see the frame
/// under the playhead, and either use that moment or let the reel pick moments for you.
struct ClipStartSheet: View {
    let url: URL
    let initial: Double
    let done: (Double?) -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var start: Double = 0
    @State private var duration = 0.0
    @State private var frame: UIImage?
    @State private var gen = AVAssetImageGenerator(asset: AVURLAsset(url: URL(fileURLWithPath: "/dev/null")))
    @State private var lastRequested = -1.0

    var body: some View {
        NavigationStack {
            VStack(spacing: 18) {
                if duration < 0.2 {
                    // A photo: it plays whole, nothing to choose.
                    Text("This one's a photo — it plays the whole time.")
                        .font(.system(size: 14)).foregroundStyle(Theme.muted)
                        .padding(.top, 40)
                } else {
                    ZStack {
                        Color(Theme.bg)
                        if let frame {
                            Image(uiImage: frame).resizable().scaledToFit()
                                .frame(maxHeight: 360).clipShape(RoundedRectangle(cornerRadius: 12))
                        } else {
                            ProgressView().controlSize(.large)
                        }
                    }
                    .frame(maxHeight: 380)
                    Slider(value: $start, in: 0...max(0.1, duration - 0.5))
                        .onChange(of: start) { _, t in scrub(t) }
                    HStack {
                        Text(tc(start)).font(.system(size: 12)).foregroundStyle(Theme.muted).monospacedDigit()
                        Spacer()
                        Text(tc(duration)).font(.system(size: 12)).foregroundStyle(Theme.muted).monospacedDigit()
                    }
                }
                Spacer()
            }
            .padding(16)
            .background(Theme.bg)
            .navigationTitle("Where it starts")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Liemo picks") { hand(nil) } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Use this") { hand(start) }
                        .disabled(duration < 0.2)
                        .font(.system(size: 15, weight: .semibold))
                }
            }
            .onAppear {
                let asset = AVURLAsset(url: url)
                duration = CMTimeGetSeconds(asset.duration)
                if duration.isNaN { duration = 0 }
                start = min(max(initial, 0), max(0, duration - 0.5))
                gen = AVAssetImageGenerator(asset: asset)
                gen.appliesPreferredTrackTransform = true
                gen.maximumSize = CGSize(width: 640, height: 1138)
                scrub(start)
            }
        }
    }

    private func hand(_ v: Double?) {
        done(v)
        dismiss()
    }

    private func scrub(_ t: Double) {
        // Skip near-duplicate requests while the slider whips around.
        guard abs(t - lastRequested) > 0.2 || (t == 0 && lastRequested != 0) else { return }
        lastRequested = t
        let time = CMTime(seconds: t, preferredTimescale: 600)
        Task {
            if let img = try? await gen.image(at: time).image {
                frame = UIImage(cgImage: img)
            }
        }
    }

    private func tc(_ t: Double) -> String {
        String(format: "%d:%02d", Int(t) / 60, Int(t) % 60)
    }
}
