import AVKit
import PhotosUI
import SwiftUI
import UniformTypeIdentifiers

/// Which half of Liemo is on screen: the video editor or the music player.
enum AppMode: String { case editor, music }

/// Liemo's music player: your songs and music videos by song, artist, album, genre and
/// playlist, with a player that can speed songs up, slow them down, shift their pitch, add
/// reverb and loop the part that hits.
struct MusicHome: View {
    @AppStorage("liemo-mode") private var mode = AppMode.editor.rawValue
    @State private var tab: MusicTab = .songs
    @State private var search = ""
    @State private var importing = false
    @State private var importingFolder = false
    @State private var settings = false
    @State private var nowPlaying = false
    @State private var reelSong: MusicTrack?
    @State private var path = NavigationPath()
    private var library: MusicLibrary { .shared }
    private var player: MusicPlayer { .shared }

    var body: some View {
        NavigationStack(path: $path) {
            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    header
                    if library.tracks.isEmpty && library.status == nil {
                        empty
                    } else {
                        searchField
                        if search.isEmpty { tabs }
                        if let s = library.status {
                            HStack(spacing: 8) { ProgressView().controlSize(.small); Text(s).font(.system(size: 13)).foregroundStyle(Theme.muted) }
                        }
                        content
                    }
                }
                .padding(.horizontal, 16)
                .padding(.bottom, player.current == nil ? 24 : 96)
            }
            .scrollDismissesKeyboard(.immediately)
            .refreshable { await library.scan() }
            .background(Theme.bg.ignoresSafeArea())
            .navigationDestination(for: MusicRoute.self) { r in
                MusicListPage(route: r, reelSong: $reelSong).toolbarBackground(Theme.bg, for: .navigationBar)
            }
            .overlay(alignment: .bottom) {
                if player.current != nil { MiniPlayer { nowPlaying = true }.padding(.horizontal, 10).padding(.bottom, 6) }
            }
        }
        .task {
            await library.scan()
            // For checks: `-music-test <steps>` (comma-separated: play, video, open, loop, sound,
            // hook, queue) drives the player silently.
            if let steps = UserDefaults.standard.string(forKey: "music-test")?.split(separator: ","), !library.tracks.isEmpty {
                player.silent = true
                if steps.contains("video") { player.play(library.videos.map(\.id)) }
                if steps.contains("play") { player.play(SongSort.title.apply(library.songs).map(\.id), at: 1) }
                if steps.contains("loop") { await player.setLooping(true) }
                if steps.contains("sound") { player.sound = SoundSettings.presets[3].sound }
                if steps.contains("open") { nowPlaying = true }
            }
        }
        .onReceive(NotificationCenter.default.publisher(for: UIApplication.willEnterForegroundNotification)) { _ in
            // Files dropped into the Music folder from the Files app or a computer show up.
            Task { await library.scan() }
        }
        .fileImporter(isPresented: $importing, allowedContentTypes: [.audio, .movie, .mpeg4Movie, .quickTimeMovie], allowsMultipleSelection: true) { r in
            if case .success(let urls) = r { Task { await library.importItems(urls) } }
        }
        .fileImporter(isPresented: $importingFolder, allowedContentTypes: [.folder], allowsMultipleSelection: true) { r in
            if case .success(let urls) = r { Task { await library.importItems(urls) } }
        }
        .sheet(isPresented: $settings) { MusicSettingsView().preferredColorScheme(.dark) }
        .fullScreenCover(isPresented: $nowPlaying) { NowPlayingView(reelSong: $reelSong).preferredColorScheme(.dark) }
        .fullScreenCover(item: $reelSong) { t in
            QuickReelView(done: { id in
                reelSong = nil
                if let id {
                    // Over to the editor, straight into the new reel.
                    player.pause()
                    UserDefaults.standard.set(id, forKey: "liemo-open-project")
                    mode = AppMode.editor.rawValue
                }
            }, song: t.url)
            .preferredColorScheme(.dark)
        }
        .alert("Music", isPresented: Binding(get: { player.error != nil }, set: { if !$0 { player.error = nil } })) {
            Button("OK") { player.error = nil }
        } message: { Text(player.error ?? "") }
    }

    private var header: some View {
        HStack(spacing: 10) {
            Image(systemName: "music.note").font(.system(size: 20, weight: .semibold))
            Text("Music").font(.system(size: 22, weight: .semibold))
            Spacer()
            Button { mode = AppMode.editor.rawValue } label: {
                Label("Editor", systemImage: "film").font(.system(size: 15, weight: .semibold))
                    .padding(.horizontal, 14).frame(height: 36)
                    .background(Theme.surface2, in: Capsule()).foregroundStyle(Theme.ink)
            }
            Menu {
                Button { importing = true } label: { Label("Songs and videos", systemImage: "doc.badge.plus") }
                Button { importingFolder = true } label: { Label("A folder", systemImage: "folder.badge.plus") }
            } label: {
                Image(systemName: "plus").font(.system(size: 16, weight: .semibold))
                    .frame(width: 36, height: 36).background(Color.white, in: Circle()).foregroundStyle(.black)
            }
            Button { settings = true } label: {
                Image(systemName: "gearshape").font(.system(size: 16))
                    .frame(width: 36, height: 36).background(Theme.surface2, in: Circle()).foregroundStyle(Theme.ink)
            }
        }
        .padding(.top, 8)
    }

    private var empty: some View {
        VStack(spacing: 14) {
            Image(systemName: "music.note.list").font(.system(size: 40)).foregroundStyle(Theme.muted)
            Text("No music yet").font(.system(size: 20, weight: .semibold))
            Text("Bring in songs and music videos from your files, or a whole folder at once. You can also drop them into the Music folder: Files › On My iPhone › Liemo › Music.")
                .font(.system(size: 15)).foregroundStyle(Theme.muted).multilineTextAlignment(.center)
            HStack(spacing: 10) {
                Button { importing = true } label: { Label("Add songs", systemImage: "doc.badge.plus") }
                    .buttonStyle(.borderedProminent).tint(.white).foregroundStyle(.black)
                Button { importingFolder = true } label: { Label("Add a folder", systemImage: "folder.badge.plus") }
                    .buttonStyle(.bordered)
            }
        }
        .frame(maxWidth: .infinity).padding(.vertical, 70).padding(.horizontal, 12)
    }

    private var searchField: some View {
        HStack(spacing: 8) {
            Image(systemName: "magnifyingglass").foregroundStyle(Theme.muted)
            TextField("Songs, artists, albums", text: $search).textInputAutocapitalization(.never).autocorrectionDisabled()
            if !search.isEmpty { Button { search = "" } label: { Image(systemName: "xmark.circle.fill").foregroundStyle(Theme.muted) } }
        }
        .padding(10).background(Theme.surface2, in: RoundedRectangle(cornerRadius: 10))
    }

    private var tabs: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                ForEach(MusicTab.allCases) { t in
                    Button { tab = t } label: {
                        Text(t.title).font(.system(size: 14, weight: .medium))
                            .padding(.horizontal, 14).frame(height: 32)
                            .background(tab == t ? Color.white : Theme.surface2, in: Capsule())
                            .foregroundStyle(tab == t ? Color.black : Theme.ink)
                    }
                }
            }
        }
    }

    @ViewBuilder private var content: some View {
        if !search.isEmpty {
            SearchResults(query: search, reelSong: $reelSong)
        } else {
            switch tab {
            case .songs: SongsTab(reelSong: $reelSong)
            case .artists:
                LazyVStack(spacing: 0) {
                    ForEach(library.artists, id: \.name) { a in
                        NavigationLink(value: MusicRoute.artist(a.name)) {
                            GroupRow(art: a.tracks.first(where: \.hasArt)?.id, round: true, title: a.name,
                                     sub: "\(a.tracks.count) song\(a.tracks.count == 1 ? "" : "s")")
                        }.buttonStyle(.plain)
                    }
                }
            case .albums: AlbumGrid(albums: library.albums)
            case .genres:
                LazyVStack(spacing: 0) {
                    ForEach(library.genres, id: \.name) { g in
                        NavigationLink(value: MusicRoute.genre(g.name)) {
                            GroupRow(art: g.tracks.first(where: \.hasArt)?.id, round: false, title: g.name,
                                     sub: "\(g.tracks.count) song\(g.tracks.count == 1 ? "" : "s")")
                        }.buttonStyle(.plain)
                    }
                }
            case .playlists: PlaylistsTab()
            case .videos: VideoGrid(reelSong: $reelSong)
            }
        }
    }
}

enum MusicTab: String, CaseIterable, Identifiable {
    case songs, artists, albums, genres, playlists, videos
    var id: String { rawValue }
    var title: String { rawValue.capitalized }
}

enum MusicRoute: Hashable {
    case artist(String), album(String), genre(String), playlist(String)
    case recent, mostPlayed, favorites
}

// MARK: - Songs

enum SongSort: String, CaseIterable, Identifiable {
    case title, artist, album, added, plays
    var id: String { rawValue }
    var label: String {
        switch self {
        case .title: "Title"
        case .artist: "Artist"
        case .album: "Album"
        case .added: "Recently added"
        case .plays: "Most played"
        }
    }
    func apply(_ t: [MusicTrack]) -> [MusicTrack] {
        switch self {
        case .title: t.sorted { $0.title.localizedStandardCompare($1.title) == .orderedAscending }
        case .artist: t.sorted { ($0.artistName, $0.albumName, $0.trackNumber) < ($1.artistName, $1.albumName, $1.trackNumber) }
        case .album: t.sorted { ($0.albumName, $0.trackNumber, $0.title) < ($1.albumName, $1.trackNumber, $1.title) }
        case .added: t.sorted { $0.added > $1.added }
        case .plays: t.sorted { $0.plays > $1.plays }
        }
    }
}

private struct SongsTab: View {
    @Binding var reelSong: MusicTrack?
    @AppStorage("music-sort") private var sort = SongSort.title.rawValue

    var body: some View {
        let list = (SongSort(rawValue: sort) ?? .title).apply(MusicLibrary.shared.songs)
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 10) {
                PlayButtons(ids: list.map(\.id))
                Spacer()
                Menu {
                    Picker("Sort", selection: $sort) {
                        ForEach(SongSort.allCases) { Text($0.label).tag($0.rawValue) }
                    }
                } label: {
                    Label((SongSort(rawValue: sort) ?? .title).label, systemImage: "arrow.up.arrow.down")
                        .font(.system(size: 13)).foregroundStyle(Theme.muted)
                }
            }
            if list.isEmpty {
                Text("No songs yet: music videos are under Videos.").font(.system(size: 14)).foregroundStyle(Theme.muted).padding(.vertical, 20)
            }
            TrackList(tracks: list, reelSong: $reelSong)
        }
    }
}

/// Play and Shuffle for a list.
struct PlayButtons: View {
    let ids: [String]
    var body: some View {
        HStack(spacing: 8) {
            Button { MusicPlayer.shared.shuffle = false; MusicPlayer.shared.play(ids) } label: {
                Label("Play", systemImage: "play.fill").font(.system(size: 14, weight: .semibold))
                    .padding(.horizontal, 14).frame(height: 34).background(Color.white, in: Capsule()).foregroundStyle(.black)
            }
            Button { MusicPlayer.shared.play(ids, at: Int.random(in: 0..<max(1, ids.count)), shuffled: true) } label: {
                Label("Shuffle", systemImage: "shuffle").font(.system(size: 14, weight: .semibold))
                    .padding(.horizontal, 14).frame(height: 34).background(Theme.surface2, in: Capsule()).foregroundStyle(Theme.ink)
            }
        }
        .disabled(ids.isEmpty)
    }
}

struct TrackList: View {
    let tracks: [MusicTrack]
    @Binding var reelSong: MusicTrack?
    var numbered = false
    var playlist: Playlist? = nil

    var body: some View {
        LazyVStack(spacing: 0) {
            ForEach(Array(tracks.enumerated()), id: \.offset) { i, t in
                TrackRow(track: t, number: numbered ? (t.trackNumber > 0 ? t.trackNumber : i + 1) : nil, reelSong: $reelSong, playlist: playlist, position: i) {
                    MusicPlayer.shared.play(tracks.map(\.id), at: i)
                }
            }
        }
    }
}

struct TrackRow: View {
    let track: MusicTrack
    var number: Int? = nil
    @Binding var reelSong: MusicTrack?
    var playlist: Playlist? = nil
    var position = 0
    let play: () -> Void
    @State private var editing = false
    @State private var hookEditing = false
    @State private var deleting = false
    @State private var naming = false
    @State private var newName = ""

    var body: some View {
        let isCurrent = MusicPlayer.shared.current?.id == track.id
        HStack(spacing: 12) {
            Button(action: play) {
                HStack(spacing: 12) {
                    if let number {
                        Text("\(number)").font(.system(size: 14).monospacedDigit()).foregroundStyle(Theme.muted).frame(width: 26)
                    } else {
                        Artwork(id: track.id, has: track.hasArt, size: 46, video: track.isVideo)
                    }
                    VStack(alignment: .leading, spacing: 3) {
                        HStack(spacing: 5) {
                            if isCurrent { Image(systemName: "waveform").font(.system(size: 12)).foregroundStyle(Theme.accent) }
                            Text(track.title).font(.system(size: 15, weight: .medium)).lineLimit(1)
                                .foregroundStyle(isCurrent ? Theme.accent : Theme.ink)
                            if track.favorite { Image(systemName: "heart.fill").font(.system(size: 10)).foregroundStyle(Theme.danger) }
                        }
                        Text("\(track.artistName) · \(track.albumName)").font(.system(size: 12)).foregroundStyle(Theme.muted).lineLimit(1)
                    }
                    Spacer(minLength: 0)
                    Text(clock(track.duration)).font(.system(size: 12).monospacedDigit()).foregroundStyle(Theme.muted)
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            Menu { actions } label: {
                Image(systemName: "ellipsis").font(.system(size: 15)).foregroundStyle(Theme.muted).frame(width: 30, height: 40)
            }
        }
        .padding(.vertical, 6)
        .contextMenu { actions }
        .sheet(isPresented: $editing) { TrackInfoEditor(track: track).preferredColorScheme(.dark) }
        .sheet(isPresented: $hookEditing) { HookEditor(track: track).preferredColorScheme(.dark) }
        .confirmationDialog("Delete “\(track.title)”?", isPresented: $deleting, titleVisibility: .visible) {
            Button("Delete the file", role: .destructive) { MusicLibrary.shared.delete(track) }
        } message: { Text("The file goes from Liemo's Music folder and every playlist.") }
        .alert("New playlist", isPresented: $naming) {
            TextField("Name", text: $newName)
            Button("Cancel", role: .cancel) {}
            Button("Create") { MusicLibrary.shared.newPlaylist(newName, tracks: [track.id]) }
        }
    }

    @ViewBuilder private var actions: some View {
        Button { MusicPlayer.shared.playNext([track.id]) } label: { Label("Play next", systemImage: "text.insert") }
        Button { MusicPlayer.shared.addToQueue([track.id]) } label: { Label("Add to queue", systemImage: "text.append") }
        Menu {
            Button { newName = ""; naming = true } label: { Label("New playlist…", systemImage: "plus") }
            ForEach(MusicLibrary.shared.playlists) { p in
                Button(p.name) { MusicLibrary.shared.add([track.id], to: p) }
            }
        } label: { Label("Add to playlist", systemImage: "music.note.list") }
        Button {
            var t = track
            t.favorite.toggle()
            MusicLibrary.shared.update(t)
        } label: { Label(track.favorite ? "Unfavorite" : "Favorite", systemImage: track.favorite ? "heart.slash" : "heart") }
        Divider()
        Button { hookEditing = true } label: { Label("The hook…", systemImage: "repeat") }
        if !track.isVideo {
            Button { reelSong = track } label: { Label("Make a reel with it", systemImage: "wand.and.stars") }
        }
        Button { editing = true } label: { Label("Edit info", systemImage: "pencil") }
        ShareLink(item: track.url) { Label("Share", systemImage: "square.and.arrow.up") }
        if let playlist {
            Button(role: .destructive) {
                var p = playlist
                if p.tracks.indices.contains(position) { p.tracks.remove(at: position) }
                MusicLibrary.shared.setPlaylist(p)
            } label: { Label("Remove from playlist", systemImage: "minus.circle") }
        }
        Button(role: .destructive) { deleting = true } label: { Label("Delete", systemImage: "trash") }
    }
}

struct GroupRow: View {
    let art: String?
    let round: Bool
    let title: String
    let sub: String
    var body: some View {
        HStack(spacing: 12) {
            Artwork(id: art ?? "", has: art != nil, size: 50, round: round)
            VStack(alignment: .leading, spacing: 3) {
                Text(title).font(.system(size: 15, weight: .medium)).lineLimit(1)
                Text(sub).font(.system(size: 12)).foregroundStyle(Theme.muted)
            }
            Spacer()
            Image(systemName: "chevron.right").font(.system(size: 13)).foregroundStyle(Theme.muted)
        }
        .padding(.vertical, 6)
        .contentShape(Rectangle())
    }
}

/// A song's cover, or a quiet placeholder.
struct Artwork: View {
    let id: String
    let has: Bool
    let size: CGFloat
    var round = false
    var video = false
    @State private var image: UIImage?

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: round ? size / 2 : max(6, size * 0.08))
        ZStack {
            shape.fill(Theme.surface3)
            if let image {
                Image(uiImage: image).resizable().scaledToFill()
            } else {
                Image(systemName: video ? "film" : "music.note").font(.system(size: size * 0.36)).foregroundStyle(Theme.muted)
            }
        }
        .frame(width: size, height: size)
        .clipShape(shape)
        .task(id: "\(id)-\(MusicLibrary.shared.artStamp)") {
            guard has, !id.isEmpty else { image = nil; return }
            let i = id
            image = await Task.detached { ArtCache.shared.image(i) }.value
        }
    }
}

// MARK: - Albums, videos, playlists

private struct AlbumGrid: View {
    let albums: [(key: String, name: String, artist: String, tracks: [MusicTrack])]
    var body: some View {
        LazyVGrid(columns: [GridItem(.adaptive(minimum: 150), spacing: 14)], spacing: 18) {
            ForEach(albums, id: \.key) { a in
                NavigationLink(value: MusicRoute.album(a.key)) {
                    VStack(alignment: .leading, spacing: 6) {
                        GeometryReader { g in
                            Artwork(id: a.tracks.first(where: \.hasArt)?.id ?? "", has: a.tracks.contains(where: \.hasArt), size: g.size.width)
                        }
                        .aspectRatio(1, contentMode: .fit)
                        Text(a.name).font(.system(size: 14, weight: .medium)).lineLimit(1)
                        Text(a.artist).font(.system(size: 12)).foregroundStyle(Theme.muted).lineLimit(1)
                    }
                }
                .buttonStyle(.plain)
            }
        }
    }
}

private struct VideoGrid: View {
    @Binding var reelSong: MusicTrack?
    var body: some View {
        let list = MusicLibrary.shared.videos.sorted { $0.title.localizedStandardCompare($1.title) == .orderedAscending }
        if list.isEmpty {
            Text("No music videos yet. Add .mp4 or .mov files and they show up here.")
                .font(.system(size: 14)).foregroundStyle(Theme.muted).padding(.vertical, 20)
        }
        LazyVGrid(columns: [GridItem(.adaptive(minimum: 160), spacing: 14)], spacing: 16) {
            ForEach(Array(list.enumerated()), id: \.element.id) { i, t in
                Button { MusicPlayer.shared.play(list.map(\.id), at: i) } label: {
                    VStack(alignment: .leading, spacing: 6) {
                        ZStack {
                            RoundedRectangle(cornerRadius: 10).fill(Theme.surface3)
                            if let img = ArtCache.shared.image(t.id) {
                                Image(uiImage: img).resizable().scaledToFill()
                            }
                            Image(systemName: "play.circle.fill").font(.system(size: 30)).foregroundStyle(.white.opacity(0.9))
                        }
                        .aspectRatio(16 / 9, contentMode: .fit)
                        .clipShape(RoundedRectangle(cornerRadius: 10))
                        Text(t.title).font(.system(size: 14, weight: .medium)).lineLimit(1)
                        Text(t.artistName).font(.system(size: 12)).foregroundStyle(Theme.muted).lineLimit(1)
                    }
                }
                .buttonStyle(.plain)
            }
        }
    }
}

private struct PlaylistsTab: View {
    @State private var naming = false
    @State private var name = ""
    var body: some View {
        let lib = MusicLibrary.shared
        LazyVStack(spacing: 0) {
            Button { name = ""; naming = true } label: {
                HStack(spacing: 12) {
                    Image(systemName: "plus").font(.system(size: 18)).frame(width: 50, height: 50)
                        .background(Theme.surface2, in: RoundedRectangle(cornerRadius: 6))
                    Text("New playlist").font(.system(size: 15, weight: .medium))
                    Spacer()
                }
                .padding(.vertical, 6).contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            smart(.favorites, "Favorites", "heart.fill", lib.tracks.filter(\.favorite).count)
            smart(.recent, "Recently added", "clock", min(50, lib.tracks.count))
            smart(.mostPlayed, "Most played", "flame", lib.tracks.filter { $0.plays > 0 }.prefix(50).count)
            ForEach(lib.playlists) { p in
                NavigationLink(value: MusicRoute.playlist(p.id)) {
                    GroupRow(art: p.tracks.lazy.compactMap { lib.track($0) }.first(where: \.hasArt)?.id, round: false,
                             title: p.name, sub: "\(p.tracks.count) song\(p.tracks.count == 1 ? "" : "s")")
                }
                .buttonStyle(.plain)
                .contextMenu {
                    Button(role: .destructive) { lib.deletePlaylist(p) } label: { Label("Delete playlist", systemImage: "trash") }
                }
            }
        }
        .alert("New playlist", isPresented: $naming) {
            TextField("Name", text: $name)
            Button("Cancel", role: .cancel) {}
            Button("Create") { lib.newPlaylist(name) }
        }
    }

    private func smart(_ r: MusicRoute, _ title: String, _ icon: String, _ count: Int) -> some View {
        NavigationLink(value: r) {
            HStack(spacing: 12) {
                Image(systemName: icon).font(.system(size: 18)).frame(width: 50, height: 50)
                    .background(Theme.surface2, in: RoundedRectangle(cornerRadius: 6))
                VStack(alignment: .leading, spacing: 3) {
                    Text(title).font(.system(size: 15, weight: .medium))
                    Text("\(count) song\(count == 1 ? "" : "s")").font(.system(size: 12)).foregroundStyle(Theme.muted)
                }
                Spacer()
                Image(systemName: "chevron.right").font(.system(size: 13)).foregroundStyle(Theme.muted)
            }
            .padding(.vertical, 6).contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

// MARK: - Pages

/// An artist, album, genre or playlist: its songs with Play and Shuffle.
struct MusicListPage: View {
    let route: MusicRoute
    @Binding var reelSong: MusicTrack?
    @State private var renaming = false
    @State private var name = ""
    @State private var editing = false

    var body: some View {
        let lib = MusicLibrary.shared
        let (title, sub, tracks, numbered, playlist) = resolve(lib)
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                HStack(alignment: .bottom, spacing: 14) {
                    Artwork(id: tracks.first(where: \.hasArt)?.id ?? "", has: tracks.contains(where: \.hasArt), size: 120,
                            round: { if case .artist = route { return true } else { return false } }())
                    VStack(alignment: .leading, spacing: 4) {
                        Text(title).font(.system(size: 22, weight: .bold)).lineLimit(2)
                        Text(sub).font(.system(size: 14)).foregroundStyle(Theme.muted)
                    }
                }
                PlayButtons(ids: tracks.map(\.id))
                if let playlist, editing {
                    List {
                        ForEach(Array(playlist.tracks.enumerated()), id: \.offset) { _, id in
                            Text(lib.track(id)?.title ?? "Missing song")
                        }
                        .onMove { from, to in
                            var p = playlist
                            p.tracks.move(fromOffsets: from, toOffset: to)
                            lib.setPlaylist(p)
                        }
                        .onDelete { off in
                            var p = playlist
                            p.tracks.remove(atOffsets: off)
                            lib.setPlaylist(p)
                        }
                        .listRowBackground(Theme.surface1)
                    }
                    .environment(\.editMode, .constant(.active))
                    .scrollContentBackground(.hidden)
                    .frame(minHeight: CGFloat(playlist.tracks.count) * 46 + 20)
                } else {
                    TrackList(tracks: tracks, reelSong: $reelSong, numbered: numbered, playlist: playlist)
                }
            }
            .padding(.horizontal, 16)
            .padding(.bottom, 100)
        }
        .background(Theme.bg.ignoresSafeArea())
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            if let playlist {
                ToolbarItem(placement: .primaryAction) {
                    Menu {
                        Button { editing.toggle() } label: { Label(editing ? "Done" : "Reorder and remove", systemImage: "arrow.up.arrow.down") }
                        Button { name = playlist.name; renaming = true } label: { Label("Rename", systemImage: "pencil") }
                        Button { MusicPlayer.shared.addToQueue(playlist.tracks) } label: { Label("Add to queue", systemImage: "text.append") }
                        Button(role: .destructive) { lib.deletePlaylist(playlist) } label: { Label("Delete playlist", systemImage: "trash") }
                    } label: { Image(systemName: "ellipsis.circle") }
                }
            }
        }
        .alert("Rename playlist", isPresented: $renaming) {
            TextField("Name", text: $name)
            Button("Cancel", role: .cancel) {}
            Button("Rename") { if var p = playlist { p.name = name; lib.setPlaylist(p) } }
        }
    }

    private func resolve(_ lib: MusicLibrary) -> (String, String, [MusicTrack], Bool, Playlist?) {
        func count(_ t: [MusicTrack]) -> String {
            let mins = Int(t.reduce(0) { $0 + $1.duration } / 60)
            return "\(t.count) song\(t.count == 1 ? "" : "s") · \(mins) min"
        }
        switch route {
        case .artist(let a):
            let t = lib.songs.filter { $0.artistName == a }.sorted { ($0.albumName, $0.trackNumber, $0.title) < ($1.albumName, $1.trackNumber, $1.title) }
            return (a, count(t), t, false, nil)
        case .album(let key):
            let t = MusicLibrary.albumOrder(lib.songs.filter { $0.albumKey == key })
            let parts = key.components(separatedBy: "\u{1}")
            let year = t.map(\.year).max() ?? 0
            return (parts[0], [parts.count > 1 ? parts[1] : "", year > 0 ? String(year) : "", count(t)].filter { !$0.isEmpty }.joined(separator: " · "), t, true, nil)
        case .genre(let g):
            let t = SongSort.artist.apply(lib.songs.filter { $0.genreName == g })
            return (g, count(t), t, false, nil)
        case .playlist(let id):
            let p = lib.playlists.first { $0.id == id }
            let t = (p?.tracks ?? []).compactMap { lib.track($0) }
            return (p?.name ?? "Playlist", count(t), t, false, p)
        case .recent:
            let t = Array(lib.tracks.sorted { $0.added > $1.added }.prefix(50))
            return ("Recently added", count(t), t, false, nil)
        case .mostPlayed:
            let t = Array(lib.tracks.filter { $0.plays > 0 }.sorted { $0.plays > $1.plays }.prefix(50))
            return ("Most played", count(t), t, false, nil)
        case .favorites:
            let t = lib.tracks.filter(\.favorite)
            return ("Favorites", count(t), t, false, nil)
        }
    }
}

private struct SearchResults: View {
    let query: String
    @Binding var reelSong: MusicTrack?
    var body: some View {
        let lib = MusicLibrary.shared
        let q = query.lowercased()
        let tracks = lib.tracks.filter { "\($0.title) \($0.artist) \($0.album) \($0.genre)".lowercased().contains(q) }
        let artists = lib.artists.filter { $0.name.lowercased().contains(q) }
        let albums = lib.albums.filter { $0.name.lowercased().contains(q) }
        VStack(alignment: .leading, spacing: 10) {
            if !artists.isEmpty {
                Text("Artists").font(.system(size: 13, weight: .semibold)).foregroundStyle(Theme.muted)
                ForEach(artists.prefix(5), id: \.name) { a in
                    NavigationLink(value: MusicRoute.artist(a.name)) {
                        GroupRow(art: a.tracks.first(where: \.hasArt)?.id, round: true, title: a.name, sub: "\(a.tracks.count) songs")
                    }.buttonStyle(.plain)
                }
            }
            if !albums.isEmpty {
                Text("Albums").font(.system(size: 13, weight: .semibold)).foregroundStyle(Theme.muted)
                ForEach(albums.prefix(5), id: \.key) { a in
                    NavigationLink(value: MusicRoute.album(a.key)) {
                        GroupRow(art: a.tracks.first(where: \.hasArt)?.id, round: false, title: a.name, sub: a.artist)
                    }.buttonStyle(.plain)
                }
            }
            if !tracks.isEmpty {
                Text("Songs").font(.system(size: 13, weight: .semibold)).foregroundStyle(Theme.muted)
                TrackList(tracks: tracks, reelSong: $reelSong)
            }
            if tracks.isEmpty && artists.isEmpty && albums.isEmpty {
                Text("Nothing called “\(query)”.").font(.system(size: 14)).foregroundStyle(Theme.muted).padding(.vertical, 20)
            }
        }
    }
}

// MARK: - Mini player

struct MiniPlayer: View {
    let open: () -> Void
    var body: some View {
        let p = MusicPlayer.shared
        if let t = p.current {
            HStack(spacing: 12) {
                Artwork(id: t.id, has: t.hasArt, size: 44, video: t.isVideo)
                VStack(alignment: .leading, spacing: 2) {
                    Text(t.title).font(.system(size: 14, weight: .semibold)).lineLimit(1)
                    HStack(spacing: 6) {
                        Text(t.artistName).font(.system(size: 12)).foregroundStyle(Theme.muted).lineLimit(1)
                        if !p.sound.isNormal { Text(String(format: "%.2g×", p.sound.speed)).font(.system(size: 10, weight: .semibold)).padding(.horizontal, 5).background(Theme.surface3, in: Capsule()) }
                        if p.looping { Image(systemName: "repeat").font(.system(size: 10, weight: .bold)).foregroundStyle(Theme.accent) }
                    }
                }
                Spacer(minLength: 0)
                Button { p.toggle() } label: {
                    Image(systemName: p.playing ? "pause.fill" : "play.fill").font(.system(size: 20)).frame(width: 40, height: 40)
                }
                Button { p.next() } label: { Image(systemName: "forward.fill").font(.system(size: 17)).frame(width: 36, height: 40) }
            }
            .foregroundStyle(Theme.ink)
            .padding(.horizontal, 10).padding(.vertical, 8)
            .background(Theme.surface2, in: RoundedRectangle(cornerRadius: 14))
            .overlay(alignment: .bottom) {
                GeometryReader { g in
                    Capsule().fill(Theme.ink.opacity(0.8))
                        .frame(width: g.size.width * (p.duration > 0 ? min(1, p.time / p.duration) : 0), height: 2)
                }
                .frame(height: 2).padding(.horizontal, 14)
            }
            .contentShape(Rectangle())
            .onTapGesture(perform: open)
        }
    }
}

func clock(_ t: Double) -> String {
    guard t.isFinite, t >= 0 else { return "0:00" }
    let s = Int(t.rounded(.down))
    return s >= 3600 ? String(format: "%d:%02d:%02d", s / 3600, s / 60 % 60, s % 60) : String(format: "%d:%02d", s / 60, s % 60)
}
