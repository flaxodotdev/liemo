import AVKit
import PhotosUI
import SwiftUI

/// The full player: cover (or the music video), the scrubber with the hook marked on it, the
/// controls, and the ways into sound, the hook and the queue.
struct NowPlayingView: View {
    @Binding var reelSong: MusicTrack?
    @Environment(\.dismiss) private var dismiss
    @State private var scrubbing: Double?
    @State private var soundOpen = false
    @State private var hookOpen = false
    @State private var queueOpen = false
    @State private var infoOpen = false
    @State private var findingHook = false
    private let testSteps = UserDefaults.standard.string(forKey: "music-test")?.split(separator: ",").map(String.init) ?? []

    var body: some View {
        let p = MusicPlayer.shared
        ZStack {
            background
            if let t = p.current {
                VStack(spacing: 18) {
                    HStack {
                        Button { dismiss() } label: { Image(systemName: "chevron.down").font(.system(size: 18, weight: .semibold)).frame(width: 40, height: 40) }
                        Spacer()
                        VStack(spacing: 2) {
                            Text("Playing from your music").font(.system(size: 11)).foregroundStyle(Theme.muted)
                            if !p.sound.isNormal { Text(soundLabel(p.sound)).font(.system(size: 12, weight: .semibold)).foregroundStyle(Theme.accent) }
                        }
                        Spacer()
                        Menu { more(t) } label: { Image(systemName: "ellipsis").font(.system(size: 18, weight: .semibold)).frame(width: 40, height: 40) }
                    }
                    Spacer(minLength: 0)
                    stage(t)
                    Spacer(minLength: 0)
                    HStack(alignment: .center) {
                        VStack(alignment: .leading, spacing: 4) {
                            Text(t.title).font(.system(size: 22, weight: .bold)).lineLimit(2)
                            Text(t.artistName).font(.system(size: 16)).foregroundStyle(Theme.muted).lineLimit(1)
                        }
                        Spacer()
                        Button {
                            var x = t
                            x.favorite.toggle()
                            MusicLibrary.shared.update(x)
                        } label: {
                            Image(systemName: t.favorite ? "heart.fill" : "heart").font(.system(size: 22))
                                .foregroundStyle(t.favorite ? Theme.danger : Theme.ink)
                        }
                    }
                    scrubber(t)
                    controls
                    tools(t)
                }
                .padding(.horizontal, 22)
                .padding(.bottom, 12)
            } else {
                VStack(spacing: 12) {
                    Text("Nothing playing").font(.system(size: 18, weight: .semibold))
                    Button("Close") { dismiss() }
                }
            }
        }
        .foregroundStyle(Theme.ink)
        .onAppear {
            if testSteps.contains("sound") { soundOpen = true }
            if testSteps.contains("hook") { hookOpen = true }
            if testSteps.contains("queue") { queueOpen = true }
        }
        .sheet(isPresented: $soundOpen) { SoundSheet().presentationDetents([.medium, .large]).preferredColorScheme(.dark) }
        .sheet(isPresented: $hookOpen) { if let t = p.current { HookEditor(track: t).preferredColorScheme(.dark) } }
        .sheet(isPresented: $queueOpen) { QueueSheet().preferredColorScheme(.dark) }
        .sheet(isPresented: $infoOpen) { if let t = p.current { TrackInfoEditor(track: t).preferredColorScheme(.dark) } }
    }

    @ViewBuilder private var background: some View {
        // The blurred cover fills the screen without widening it: a filled image sizes its
        // container to itself, so it's an overlay on a layer the screen sizes.
        Theme.bg
            .overlay {
                if let t = MusicPlayer.shared.current, let img = ArtCache.shared.image(t.id) {
                    Image(uiImage: img).resizable().scaledToFill().blur(radius: 60).opacity(0.45)
                }
            }
            .overlay { LinearGradient(colors: [.clear, Theme.bg.opacity(0.9)], startPoint: .top, endPoint: .bottom) }
            .clipped()
            .ignoresSafeArea()
    }

    @ViewBuilder private func stage(_ t: MusicTrack) -> some View {
        if let v = MusicPlayer.shared.video {
            VideoPlayer(player: v)
                .aspectRatio(16 / 9, contentMode: .fit)
                .clipShape(RoundedRectangle(cornerRadius: 12))
        } else {
            GeometryReader { g in
                let side = min(g.size.width, g.size.height)
                Artwork(id: t.id, has: t.hasArt, size: side)
                    .shadow(color: .black.opacity(0.5), radius: 24, y: 10)
                    .scaleEffect(MusicPlayer.shared.playing ? 1 : 0.88)
                    .animation(.spring(duration: 0.35), value: MusicPlayer.shared.playing)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
            .aspectRatio(1, contentMode: .fit)
        }
    }

    /// The song's timeline: played part, the hook as a band, and a handle to drag.
    private func scrubber(_ t: MusicTrack) -> some View {
        let p = MusicPlayer.shared
        let d = max(t.duration, 0.01)
        let now = scrubbing ?? p.time
        return VStack(spacing: 6) {
            GeometryReader { g in
                let w = g.size.width
                ZStack(alignment: .leading) {
                    Capsule().fill(Theme.ink.opacity(0.18)).frame(height: 5)
                    if let h = t.hook {
                        Capsule().fill(Theme.accent.opacity(p.looping ? 0.75 : 0.35))
                            .frame(width: max(4, w * (h.upperBound - h.lowerBound) / d), height: 5)
                            .offset(x: w * h.lowerBound / d)
                    }
                    Capsule().fill(Theme.ink).frame(width: max(0, w * min(1, now / d)), height: 5)
                    Circle().fill(Theme.ink).frame(width: scrubbing == nil ? 13 : 20)
                        .offset(x: w * min(1, now / d) - (scrubbing == nil ? 6.5 : 10))
                }
                .frame(maxHeight: .infinity)
                .contentShape(Rectangle())
                .gesture(DragGesture(minimumDistance: 0)
                    .onChanged { v in scrubbing = min(max(0, v.location.x / w), 1) * d }
                    .onEnded { v in
                        p.seek(to: min(max(0, v.location.x / w), 1) * d)
                        scrubbing = nil
                    })
            }
            .frame(height: 24)
            HStack {
                Text(clock(now)).font(.system(size: 12).monospacedDigit())
                Spacer()
                if p.looping, let h = t.hook { Text("Looping \(clock(h.lowerBound))–\(clock(h.upperBound))").font(.system(size: 11)).foregroundStyle(Theme.accent) }
                Spacer()
                Text("-" + clock(max(0, d - now))).font(.system(size: 12).monospacedDigit())
            }
            .foregroundStyle(Theme.muted)
        }
    }

    private var controls: some View {
        let p = MusicPlayer.shared
        return HStack {
            Button { p.shuffle.toggle() } label: {
                Image(systemName: "shuffle").font(.system(size: 18)).foregroundStyle(p.shuffle ? Theme.accent : Theme.ink)
            }
            Spacer()
            Button { p.previous() } label: { Image(systemName: "backward.fill").font(.system(size: 28)) }
            Spacer()
            Button { p.toggle() } label: {
                Image(systemName: p.playing ? "pause.circle.fill" : "play.circle.fill").font(.system(size: 66))
            }
            Spacer()
            Button { p.next() } label: { Image(systemName: "forward.fill").font(.system(size: 28)) }
            Spacer()
            Button {
                p.repeatMode = p.repeatMode == .off ? .all : p.repeatMode == .all ? .one : .off
            } label: {
                Image(systemName: p.repeatMode == .one ? "repeat.1" : "repeat").font(.system(size: 18))
                    .foregroundStyle(p.repeatMode == .off ? Theme.ink : Theme.accent)
            }
        }
    }

    private func tools(_ t: MusicTrack) -> some View {
        let p = MusicPlayer.shared
        return HStack(spacing: 10) {
            tool("Sound", "slider.horizontal.3", on: !p.sound.isNormal) { soundOpen = true }
            tool(findingHook ? "Finding…" : "Loop hook", "repeat.circle", on: p.looping) {
                findingHook = true
                Task { await p.setLooping(!p.looping); findingHook = false }
            }
            tool("Hook", "scissors", on: false) { hookOpen = true }
            tool("Queue", "list.bullet", on: false) { queueOpen = true }
        }
    }

    private func tool(_ name: String, _ icon: String, on: Bool, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            VStack(spacing: 5) {
                Image(systemName: icon).font(.system(size: 19))
                Text(name).font(.system(size: 11, weight: .medium))
            }
            .frame(maxWidth: .infinity).frame(height: 56)
            .background(on ? Theme.accent.opacity(0.22) : Theme.surface2.opacity(0.7), in: RoundedRectangle(cornerRadius: 12))
            .foregroundStyle(on ? Theme.accent : Theme.ink)
        }
    }

    @ViewBuilder private func more(_ t: MusicTrack) -> some View {
        let p = MusicPlayer.shared
        Menu {
            Button("15 minutes") { p.sleep(after: 15) }
            Button("30 minutes") { p.sleep(after: 30) }
            Button("45 minutes") { p.sleep(after: 45) }
            Button("1 hour") { p.sleep(after: 60) }
            Button("When this song ends") { p.sleepAfterThisSong() }
            if p.sleepAt != nil || p.stopAfterTrack { Button("Turn off", role: .destructive) { p.sleep(after: nil) } }
        } label: {
            Label(p.sleepAt.map { "Sleep at \($0.formatted(date: .omitted, time: .shortened))" } ?? (p.stopAfterTrack ? "Sleep after this song" : "Sleep timer"), systemImage: "moon.zzz")
        }
        if !t.isVideo {
            Button { dismiss(); DispatchQueue.main.asyncAfter(deadline: .now() + 0.4) { reelSong = t } } label: {
                Label("Make a reel with it", systemImage: "wand.and.stars")
            }
        }
        Button { infoOpen = true } label: { Label("Edit info", systemImage: "pencil") }
        ShareLink(item: t.url) { Label("Share", systemImage: "square.and.arrow.up") }
    }

    private func soundLabel(_ s: SoundSettings) -> String {
        var parts: [String] = []
        if s.speed != 1 { parts.append(String(format: "%.2f×", s.speed)) }
        if s.pitch != 0 { parts.append(String(format: "%+.0f st", s.pitch)) }
        if s.reverb > 0 { parts.append("reverb") }
        if s.bass != 0 || s.treble != 0 { parts.append("EQ") }
        return parts.joined(separator: " · ")
    }
}

// MARK: - Sound

struct SoundSheet: View {
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        let p = MusicPlayer.shared
        let s = Binding(get: { p.sound }, set: { p.sound = $0 })
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    ScrollView(.horizontal, showsIndicators: false) {
                        HStack(spacing: 8) {
                            ForEach(SoundSettings.presets) { pr in
                                Button { p.sound = pr.sound } label: {
                                    Text(pr.name).font(.system(size: 13, weight: .medium))
                                        .padding(.horizontal, 12).frame(height: 32)
                                        .background(p.sound == pr.sound ? Color.white : Theme.surface2, in: Capsule())
                                        .foregroundStyle(p.sound == pr.sound ? Color.black : Theme.ink)
                                }
                            }
                        }
                    }
                    slider("Speed", value: s.speed, range: 0.5...2, step: 0.01, text: String(format: "%.2f×", p.sound.speed))
                    Toggle("Pitch follows speed (like a record)", isOn: s.vinyl).font(.system(size: 14))
                    slider("Pitch", value: s.pitch, range: -12...12, step: 1, text: p.sound.pitch == 0 ? "Original" : String(format: "%+.0f semitones", p.sound.pitch))
                    if p.video != nil {
                        Text("Music videos change speed only; pitch, reverb and EQ work on songs.")
                            .font(.system(size: 12)).foregroundStyle(Theme.muted)
                    }
                    slider("Reverb", value: s.reverb, range: 0...1, step: 0.01, text: p.sound.reverb == 0 ? "Off" : "\(Int(p.sound.reverb * 100))%")
                    slider("Room", value: s.room, range: 0...1, step: 0.01, text: p.sound.room < 0.25 ? "Room" : p.sound.room < 0.5 ? "Big room" : p.sound.room < 0.75 ? "Hall" : "Cathedral")
                    slider("Bass", value: s.bass, range: -12...12, step: 0.5, text: String(format: "%+.1f dB", p.sound.bass))
                    slider("Treble", value: s.treble, range: -12...12, step: 0.5, text: String(format: "%+.1f dB", p.sound.treble))
                }
                .padding(16)
            }
            .background(Theme.bg)
            .navigationTitle("Sound")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Reset") { p.sound = SoundSettings() } }
                ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } }
            }
        }
    }

    private func slider(_ name: String, value: Binding<Double>, range: ClosedRange<Double>, step: Double, text: String) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack {
                Text(name).font(.system(size: 14, weight: .medium))
                Spacer()
                Text(text).font(.system(size: 13).monospacedDigit()).foregroundStyle(Theme.muted)
            }
            Slider(value: value, in: range, step: step).tint(.white)
        }
    }
}

// MARK: - The hook

/// Pick the part of the song that hits: drag its edges on the waveform, hear it looped, or let
/// Liemo find it again.
struct HookEditor: View {
    let track: MusicTrack
    @Environment(\.dismiss) private var dismiss
    @State private var peaks: [Float] = []
    @State private var start = 0.0
    @State private var end = 0.0
    @State private var bpm = 0.0
    @State private var found: ClosedRange<Double>?
    @State private var loading = true
    @State private var dragStart: (Double, Double)?

    var body: some View {
        let d = max(track.duration, 0.01)
        NavigationStack {
            VStack(alignment: .leading, spacing: 18) {
                Text(track.title).font(.system(size: 17, weight: .semibold)).lineLimit(1)
                Text("The hook is the part that hits. Drag its edges, or drag the middle to move it.")
                    .font(.system(size: 13)).foregroundStyle(Theme.muted)
                GeometryReader { g in
                    let w = g.size.width
                    let x0 = w * start / d, x1 = w * end / d
                    ZStack(alignment: .leading) {
                        HStack(alignment: .center, spacing: 1) {
                            ForEach(Array(peaks.enumerated()), id: \.offset) { i, v in
                                let t = Double(i) / Double(max(1, peaks.count)) * d
                                Capsule().fill(t >= start && t <= end ? Theme.accent : Theme.ink.opacity(0.3))
                                    .frame(maxWidth: .infinity).frame(height: max(2, CGFloat(v) * g.size.height))
                            }
                        }
                        .frame(maxHeight: .infinity)
                        if loading { ProgressView().frame(maxWidth: .infinity) }
                        // The band and its handles.
                        Rectangle().fill(Theme.accent.opacity(0.12)).frame(width: max(2, x1 - x0)).offset(x: x0)
                            .gesture(DragGesture().onChanged { v in
                                let base = dragStart ?? (start, end)
                                dragStart = base
                                let len = base.1 - base.0
                                let s = min(max(0, base.0 + v.translation.width / w * d), d - len)
                                start = s; end = s + len
                            }.onEnded { _ in dragStart = nil; preview() })
                        handle(at: x0) { x in start = min(max(0, x / w * d), end - 1) }
                        handle(at: x1) { x in end = max(min(d, x / w * d), start + 1) }
                    }
                    .coordinateSpace(name: "hook")
                }
                .frame(height: 120)
                HStack {
                    Text("\(clock(start)) – \(clock(end))").font(.system(size: 15, weight: .semibold).monospacedDigit())
                    Text(String(format: "%.1f s", end - start)).font(.system(size: 13)).foregroundStyle(Theme.muted)
                    Spacer()
                    if bpm > 0 { Text("\(Int(bpm.rounded())) BPM").font(.system(size: 13)).foregroundStyle(Theme.muted) }
                }
                HStack(spacing: 10) {
                    Button { preview() } label: { Label("Hear it looped", systemImage: "repeat").frame(maxWidth: .infinity) }
                        .buttonStyle(.bordered)
                    Button {
                        if let f = found { start = f.lowerBound; end = f.upperBound; preview() }
                    } label: { Label("Find it for me", systemImage: "sparkles").frame(maxWidth: .infinity) }
                        .buttonStyle(.bordered).disabled(found == nil)
                }
                if bpm > 0 {
                    HStack(spacing: 8) {
                        Text("Length").font(.system(size: 13)).foregroundStyle(Theme.muted)
                        ForEach([4, 8, 16, 32], id: \.self) { beats in
                            Button("\(beats / 4) bar\(beats == 4 ? "" : "s")") {
                                end = min(d, start + 60 / bpm * Double(beats))
                                preview()
                            }
                            .font(.system(size: 13)).buttonStyle(.bordered)
                        }
                    }
                }
                Spacer()
            }
            .padding(16)
            .background(Theme.bg)
            .navigationTitle("The hook")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") { save(); dismiss() }.disabled(loading || end - start < 1)
                }
            }
        }
        .task {
            let a = await HookFinder.analyze(track.url)
            loading = false
            guard let a else { return }
            peaks = a.peaks
            bpm = a.bpm
            found = a.hook
            start = track.hookStart ?? a.hook.lowerBound
            end = track.hookEnd ?? a.hook.upperBound
        }
    }

    private func handle(at x: CGFloat, move: @escaping (CGFloat) -> Void) -> some View {
        Capsule().fill(Color.white).frame(width: 6, height: 120)
            .overlay(Capsule().stroke(Theme.accent, lineWidth: 2))
            .frame(width: 30)
            .contentShape(Rectangle())
            .offset(x: x - 15)
            .gesture(DragGesture(coordinateSpace: .named("hook")).onChanged { v in move(v.location.x) }.onEnded { _ in preview() })
    }

    private func save() {
        guard var t = MusicLibrary.shared.track(track.id) else { return }
        t.hookStart = start
        t.hookEnd = end
        MusicLibrary.shared.update(t)
        if MusicPlayer.shared.current?.id == t.id { MusicPlayer.shared.hookChanged() }
    }

    /// Save it and loop it, so what you hear is what you picked.
    private func preview() {
        save()
        let p = MusicPlayer.shared
        if p.current?.id != track.id { p.play([track.id]) }
        Task { if !p.looping { await p.setLooping(true) } else { p.hookChanged() } }
    }
}

// MARK: - Queue

struct QueueSheet: View {
    @Environment(\.dismiss) private var dismiss
    var body: some View {
        let p = MusicPlayer.shared
        let lib = MusicLibrary.shared
        NavigationStack {
            List {
                Section("Now playing") {
                    if let t = p.current { row(t, current: true) }
                }
                Section("Up next") {
                    ForEach(Array(p.queue.enumerated()), id: \.offset) { i, id in
                        if i > p.index, let t = lib.track(id) {
                            Button { p.jump(to: i) } label: { row(t, current: false) }.buttonStyle(.plain)
                        }
                    }
                    .onMove { p.moveInQueue(from: $0, to: $1) }
                    .onDelete { p.removeFromQueue(at: $0) }
                }
            }
            .scrollContentBackground(.hidden)
            .background(Theme.bg)
            .navigationTitle("Queue")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Clear") { p.clearUpNext() } }
                ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } }
                ToolbarItem(placement: .primaryAction) { EditButton() }
            }
        }
    }

    private func row(_ t: MusicTrack, current: Bool) -> some View {
        HStack(spacing: 12) {
            Artwork(id: t.id, has: t.hasArt, size: 40, video: t.isVideo)
            VStack(alignment: .leading, spacing: 2) {
                Text(t.title).font(.system(size: 14, weight: .medium)).foregroundStyle(current ? Theme.accent : Theme.ink).lineLimit(1)
                Text(t.artistName).font(.system(size: 12)).foregroundStyle(Theme.muted).lineLimit(1)
            }
            Spacer()
        }
        .listRowBackground(Theme.surface1)
    }
}

// MARK: - Info

struct TrackInfoEditor: View {
    let track: MusicTrack
    @Environment(\.dismiss) private var dismiss
    @State private var t: MusicTrack
    @State private var cover: PhotosPickerItem?

    init(track: MusicTrack) {
        self.track = track
        _t = State(initialValue: track)
    }

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    HStack {
                        Spacer()
                        PhotosPicker(selection: $cover, matching: .images) {
                            VStack(spacing: 6) {
                                Artwork(id: t.id, has: t.hasArt, size: 120, video: t.isVideo)
                                Text("Change cover").font(.system(size: 13))
                            }
                        }
                        Spacer()
                    }
                    .listRowBackground(Color.clear)
                }
                Section {
                    TextField("Title", text: $t.title)
                    TextField("Artist", text: $t.artist)
                    TextField("Album", text: $t.album)
                    TextField("Album artist", text: $t.albumArtist)
                    TextField("Genre", text: $t.genre)
                    TextField("Year", value: $t.year, format: .number.grouping(.never)).keyboardType(.numberPad)
                    TextField("Track number", value: $t.trackNumber, format: .number).keyboardType(.numberPad)
                }
                Section {
                    LabeledContent("File", value: t.file)
                    LabeledContent("Length", value: clock(t.duration))
                    LabeledContent("Played", value: "\(t.plays) time\(t.plays == 1 ? "" : "s")")
                }
            }
            .scrollContentBackground(.hidden)
            .background(Theme.bg)
            .navigationTitle("Info")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") {
                        // Keep what changed elsewhere meanwhile (plays, hook); take the edited fields.
                        guard var cur = MusicLibrary.shared.track(t.id) else { dismiss(); return }
                        cur.title = t.title.isEmpty ? cur.title : t.title
                        cur.artist = t.artist
                        cur.album = t.album
                        cur.albumArtist = t.albumArtist
                        cur.genre = t.genre
                        cur.year = t.year
                        cur.trackNumber = t.trackNumber
                        MusicLibrary.shared.update(cur)
                        dismiss()
                    }
                }
            }
        }
        .onChange(of: cover) { _, item in
            guard let item else { return }
            Task {
                if let data = try? await item.loadTransferable(type: Data.self), let img = UIImage(data: data) {
                    MusicLibrary.shared.setCover(track, image: img)
                    t.hasArt = true
                }
            }
        }
    }
}

// MARK: - Settings

struct MusicSettingsView: View {
    @Environment(\.dismiss) private var dismiss
    @AppStorage("music-per-song-sound") private var perSong = false
    @AppStorage("music-start-at-hook") private var startAtHook = false
    @State private var rescanning = false

    var body: some View {
        let lib = MusicLibrary.shared
        let p = MusicPlayer.shared
        NavigationStack {
            Form {
                Section {
                    Toggle("Remember each song's sound", isOn: $perSong)
                    Toggle("Start songs at their hook", isOn: $startAtHook)
                    Picker("Repeat", selection: Binding(get: { p.repeatMode }, set: { p.repeatMode = $0 })) {
                        Text("Off").tag(RepeatMode.off)
                        Text("All").tag(RepeatMode.all)
                        Text("This song").tag(RepeatMode.one)
                    }
                    Toggle("Shuffle", isOn: Binding(get: { p.shuffle }, set: { p.shuffle = $0 }))
                } header: { Text("Playing") } footer: {
                    Text("With “Remember each song's sound” on, speed, pitch, reverb and EQ stick to the song you set them on. Off, they apply to everything.")
                }
                Section {
                    LabeledContent("Songs", value: "\(lib.songs.count)")
                    LabeledContent("Music videos", value: "\(lib.videos.count)")
                    LabeledContent("Playlists", value: "\(lib.playlists.count)")
                    Button {
                        rescanning = true
                        Task { await lib.scan(); rescanning = false }
                    } label: { HStack { Text("Look for new files"); if rescanning { Spacer(); ProgressView() } } }
                } header: { Text("Library") } footer: {
                    Text("Liemo's music lives in Files › On My iPhone › Liemo › Music. Drop songs or folders there (from Files, AirDrop or a computer) and they show up here.")
                }
                Section {
                    Button("Reset sound to normal") { p.sound = SoundSettings() }
                    Button("Forget play counts") {
                        for var t in lib.tracks where t.plays > 0 { t.plays = 0; t.lastPlayed = nil; lib.update(t) }
                    }
                    Button("Forget every hook") {
                        for var t in lib.tracks where t.hookStart != nil { t.hookStart = nil; t.hookEnd = nil; lib.update(t) }
                    }
                }
            }
            .scrollContentBackground(.hidden)
            .background(Theme.bg)
            .navigationTitle("Music settings")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } } }
        }
    }
}
