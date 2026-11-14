import AVFoundation
import SwiftUI

/// Every song you brought in, with its lyrics. Add more, name them, fix their lyrics, drop
/// the ones you're done with. The quick reel screen reads straight from here.
struct SongLibraryView: View {
    @State private var filesOpen = false
    @State private var importing: String?
    @State private var editing: SavedSong?
    @State private var failure: String?

    var body: some View {
        Group {
            if SongLibrary.shared.songs.isEmpty {
                empty
            } else {
                list
            }
        }
        .background(Theme.bg)
        .navigationTitle("Song library")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar { ToolbarItem(placement: .primaryAction) {
            Button { filesOpen = true } label: { Image(systemName: "plus") }
        } }
        .fileImporter(isPresented: $filesOpen, allowedContentTypes: [.audio]) { r in
            if case .success(let u) = r {
                let scoped = u.startAccessingSecurityScopedResource()
                let copied = try? ImportedFile.copy(u)
                if scoped { u.stopAccessingSecurityScopedResource() }
                if let copied { importSong(copied) }
            }
        }
        .sheet(item: $editing) { song in
            LyricsEditor(song: song) { SongLibrary.shared.update($0) }
                .preferredColorScheme(.dark)
        }
    }

    private var list: some View {
        List {
            ForEach(SongLibrary.shared.songs) { s in
                Button { editing = s } label: {
                    VStack(alignment: .leading, spacing: 3) {
                        Text(s.title).font(.system(size: 15, weight: .medium)).foregroundStyle(Theme.ink).lineLimit(1)
                        Text(s.artist.isEmpty ? "No artist" : s.artist).font(.system(size: 12)).foregroundStyle(Theme.muted).lineLimit(1)
                    }
                    .padding(.vertical, 3)
                }
                .swipeActions {
                    Button(role: .destructive) { SongLibrary.shared.remove(s) } label: { Label("Delete", systemImage: "trash") }
                }
            }
        }
        .listStyle(.insetGrouped)
        .scrollContentBackground(.hidden)
        .overlay { if let t = importing { working(t) } }
    }

    private var empty: some View {
        VStack(spacing: 14) {
            Image(systemName: "music.note.list").font(.system(size: 34)).foregroundStyle(Theme.muted)
            Text("No songs yet").font(.system(size: 18, weight: .semibold))
            Text("Add an audio file and Liemo names it and finds its lyrics for you.")
                .font(.system(size: 14)).foregroundStyle(Theme.muted).multilineTextAlignment(.center)
            Button("Add a song") { filesOpen = true }
                .buttonStyle(.borderedProminent)
        }
        .padding(.vertical, 80).padding(.horizontal, 24)
    }

    private func working(_ t: String) -> some View {
        VStack(spacing: 12) {
            ProgressView().controlSize(.large)
            Text(t).font(.system(size: 14)).foregroundStyle(Theme.muted)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Theme.bg.opacity(0.85))
    }

    /// Same treatment the quick reel gives a new song: recognise it, fetch its lyrics, keep it.
    private func importSong(_ url: URL) {
        importing = "Recognizing the song…"
        let name = url.deletingPathExtension().lastPathComponent.split(separator: "-", maxSplits: 1).last.map(String.init) ?? "Your song"
        Task {
            let (match, lyrics) = await SongMatcher.identify(url: url, fallbackName: String(name))
            SongLibrary.shared.add(url: url, title: match.title, artist: match.artist, synced: lyrics.synced, plain: lyrics.plain)
            importing = nil
        }
    }
}

/// Name a song and set its lyrics by hand. Pasted lines with `[1:23.45]` stamps in front
/// stay glued to their moments in the reel; lines without stamps ride on the beat.
struct LyricsEditor: View {
    let song: SavedSong
    let onSave: (SavedSong) -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var name: String
    @State private var artist: String
    @State private var lyrics: String
    @State private var fetching = false
    @State private var failure: String?
    @State private var calibrating = false
    @State private var aligning = false
    @State private var offset: Double

    init(song: SavedSong, onSave: @escaping (SavedSong) -> Void) {
        self.song = song
        self.onSave = onSave
        _name = State(initialValue: song.title)
        _artist = State(initialValue: song.artist)
        // The raw text: synced lyrics keep their stamps so they stay glued to the music.
        _lyrics = State(initialValue: song.synced ?? song.plain ?? "")
        _offset = State(initialValue: song.offset)
    }

    var body: some View {
        NavigationStack {
            Form {
                Section("Song") {
                    TextField("Name", text: $name)
                    TextField("Artist", text: $artist)
                }
                Section {
                    TextEditor(text: $lyrics).frame(minHeight: 220).font(.system(size: 14))
                    if let f = failure { Text(f).font(.system(size: 13)).foregroundStyle(Theme.danger) }
                } header: {
                    Text("Lyrics")
                } footer: {
                    Text("[1:23.45] before a line pins it to that moment. Lines without stamps ride on the beat.")
                }
                Section {
                    Button {
                        fetch()
                    } label: {
                        if fetching { ProgressView() } else { Label("Find lyrics on the web", systemImage: "globe") }
                    }
                    if song.hasSynced {
                        Button { calibrating = true } label: {
                            Label(offset == 0 ? "Fix the lyric timing" : "Lyric timing: \(offset >= 0 ? "+" : "")\(String(format: "%.1f", offset))s — adjust", systemImage: "timer")
                        }
                    }
                    if song.url != nil, !lyrics.isEmpty, !lyrics.contains("<") {
                        Button {
                            listen()
                        } label: {
                            if aligning { HStack(spacing: 8) { ProgressView(); Text("Listening to the song…") } }
                            else { Label("Word-sync by listening", systemImage: "waveform.badge.magnifyingglass") }
                        }
                        .disabled(aligning)
                    }
                } footer: {
                    if song.hasSynced {
                        Text("Lyrics land early or late? Play the song, tap when a line matches what you hear, and every line moves with it.")
                    } else if !lyrics.isEmpty, song.url != nil {
                        Text("\"Word-sync by listening\" plays the song through the device's ear and stamps every word at the moment it's sung — even without timestamps from the web.")
                    }
                }
            }
            .navigationTitle("Edit song")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") {
                        var s = song
                        s.title = name.trimmingCharacters(in: .whitespaces)
                        s.artist = artist.trimmingCharacters(in: .whitespaces)
                        s.offset = offset
                        if !SongLibrary.parseLRC(lyrics).isEmpty {
                            s.synced = lyrics                      // stamped: it stays synced
                        } else {
                            s.plain = lyrics
                            s.synced = nil                         // no stamps: plain words now
                        }
                        onSave(s)
                        dismiss()
                    }
                }
            }
            .sheet(isPresented: $calibrating) {
                LyricCalibrator(song: song) { updated in
                    offset = updated.offset   // shows on the label; calibrator also saved it
                    onSave(updated)           // refresh the caller (Quick reel's picked song)
                }
                .preferredColorScheme(.dark)
            }
        }
    }

    private func fetch() {
        fetching = true
        failure = nil
        Task {
            defer { fetching = false }
            let duration = song.url.map { (try? AVAudioFile(forReading: $0)).map { Double($0.length) / $0.processingFormat.sampleRate } } ?? nil
            guard let got = try? await SongMatcher.fetchLyrics(artist: artist, track: name, duration: duration) else {
                failure = "Nothing found. Check the name and artist."
                return
            }
            if let text = got.anyText {
                lyrics = text
                if got.synced == nil { failure = "Only plain lyrics found for this one." }
            } else {
                failure = "Nothing found. Check the name and artist."
            }
        }
    }

    /// On-device word-sync: transcribe the song, anchor the pasted lyrics to what's heard,
    /// and rewrite the field as enhanced LRC. Saving keeps it as synced lyrics.
    private func listen() {
        guard let url = song.url else { return }
        aligning = true
        failure = nil
        Task {
            defer { aligning = false }
            do {
                let got = try await LyricAligner.align(url: url, lyrics: lyrics)
                lyrics = got.lrc
            } catch {
                failure = error.localizedDescription
            }
        }
    }
}

/// One-tap timing calibration. The song plays; watch the highlighted line and when it
/// matches what you hear, tap. That single measurement shifts every stamp of the song,
/// so the reel captions land on the audio of *this* file, not of some other version.
struct LyricCalibrator: View {
    let song: SavedSong
    let onDone: (SavedSong) -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var player: AVAudioPlayer?
    @State private var at = 0.0
    @State private var timer: Timer?
    @State private var draft: Double

    init(song: SavedSong, onDone: @escaping (SavedSong) -> Void) {
        self.song = song
        self.onDone = onDone
        _draft = State(initialValue: song.offset)
    }

    private var lines: [LyricLine] { song.lines }

    /// Which line is sounding at the playhead, given the current draft shift.
    private var currentIndex: Int? {
        let t = at - draft
        return lines.lastIndex { $0.time <= t + 0.02 }
    }
    private var current: LyricLine? { currentIndex.map { lines[$0] } }

    var body: some View {
        NavigationStack {
            VStack(spacing: 26) {
                Spacer()
                VStack(spacing: 10) {
                    Text("Now playing").font(.system(size: 13)).foregroundStyle(Theme.muted)
                    Text("\(Int(at / 60)):\(String(format: "%02d", Int(at) % 60))")
                        .font(.system(size: 40, weight: .bold, design: .rounded)).foregroundStyle(Theme.ink)
                }
                VStack(spacing: 8) {
                    // The line the stamps say is sounding now — the one to listen for.
                    Text(current?.text ?? "Wait for a line you recognise…")
                        .font(.system(size: 21, weight: .semibold)).foregroundStyle(Theme.ink)
                        .multilineTextAlignment(.center).padding(.horizontal, 26)
                    if let i = currentIndex, i + 1 < lines.count {
                        Text(lines[i + 1].text).font(.system(size: 14)).foregroundStyle(Theme.muted)
                            .multilineTextAlignment(.center).lineLimit(2).padding(.horizontal, 26)
                    }
                }
                .frame(maxHeight: 150)
                Spacer()
                Button {
                    // Heard it: this moment minus the stamp is the shift every line needs.
                    if let c = current {
                        draft = min(15, max(-15, at - c.time))
                    }
                } label: {
                    Label("That's this line", systemImage: "hand.tap")
                        .font(.system(size: 17, weight: .semibold))
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 13)
                }
                .buttonStyle(.borderedProminent)
                .disabled(current == nil)
                .padding(.horizontal, 26)
                HStack(spacing: 18) {
                    Button { draft -= 0.1 } label: { Image(systemName: "minus").padding(10) }
                        .buttonStyle(.bordered)
                    VStack(spacing: 2) {
                        Text("shift \(draft >= 0 ? "+" : "")\(String(format: "%.1f", draft))s")
                            .font(.system(size: 14, weight: .semibold)).foregroundStyle(Theme.ink)
                        Text("nudge earlier / later").font(.system(size: 11)).foregroundStyle(Theme.muted)
                    }
                    Button { draft += 0.1 } label: { Image(systemName: "plus").padding(10) }
                        .buttonStyle(.bordered)
                }
                Button("Done") {
                    var s = song
                    s.offset = draft
                    SongLibrary.shared.update(s)   // saved right here; survives dismissal
                    onDone(s)
                    dismiss()
                }
                .font(.system(size: 16, weight: .semibold))
                .padding(.bottom, 18)
            }
            .background(Theme.bg)
            .navigationTitle("Lyric timing")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } } }
            .onAppear(perform: play)
            .onDisappear {
                timer?.invalidate()
                player?.stop()
            }
        }
    }

    private func play() {
        guard let u = song.url, let p = try? AVAudioPlayer(contentsOf: u) else { return }
        try? AVAudioSession.sharedInstance().setCategory(.playback)
        p.prepareToPlay()
        p.play()
        player = p
        timer = Timer.scheduledTimer(withTimeInterval: 0.1, repeats: true) { _ in
            at = p.currentTime
        }
    }
}
