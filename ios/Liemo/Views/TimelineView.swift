import SwiftUI

/// The timeline, phone-editor style: everything scrolls under a fixed playhead in the middle.
/// The main video sits at the bottom as a filmstrip of real frames, sound above it with its
/// waveform (or "Add audio" when there's none), and titles/overlays above that as slim layers,
/// shown only when there are some. Drag to scrub, pinch to zoom, tap a clip to select it, then
/// drag it to move or drag its ends to trim; long-press a lane to hide or mute its track.
///
/// For smooth scrubbing the clips live in `TrackContent`, laid out from 0:00 and slid as one
/// piece: it doesn't read the playhead, so moving the playhead never rebuilds it. Each clip keeps
/// its own drag, so moving one redraws only that clip.
struct TimelineView: View {
    let model: EditorModel
    let addMedia: () -> Void
    let addAudio: () -> Void
    @State private var pps: CGFloat = 60
    @State private var zoomStart: CGFloat?
    @State private var scrubStart: Double?
    /// How far the lanes are scrolled up (to reach layers above the visible ones), and whether the
    /// current drag scrolls them rather than scrubbing.
    @State private var vScroll: CGFloat = 0
    @State private var vStart: CGFloat?
    @State private var dragAxis: Axis?

    private let ruler: CGFloat = 30
    private let gap: CGFloat = 6

    /// One lane on screen, bottom-up: main video, then sound, then layers.
    struct Lane: Identifiable, Equatable {
        let track: String
        let height: CGFloat
        /// Titles at the same moment stack in rows.
        var rows: Int = 1
        var y: CGFloat = 0
        var id: String { track }
    }

    /// Bottom-up: the main video, every sound track, then picture layers and titles, each only
    /// when it has clips (tracks added on the Mac included). `scroll` lifts them all.
    private func lanes(_ doc: ProjectDoc, height: CGFloat, scroll: CGFloat = 0) -> [Lane] {
        let used = Set(doc.clips.map(\.track))
        var stack = [Lane(track: "V1", height: 58)]
        let audio = doc.tracks.filter { $0.kind == "audio" }.map(\.id)
        let sounding = audio.filter { used.contains($0) }
        if sounding.isEmpty {
            stack.append(Lane(track: "A1", height: 34))
        } else {
            stack += sounding.map { Lane(track: $0, height: 34) }
        }
        // Picture layers from the back up (the track list runs front to back), then titles.
        for t in doc.tracks.reversed() where t.id != "V1" && t.kind != "audio" && used.contains(t.id) {
            if t.kind == "text" {
                let n = max(1, stackRows(doc.clips.filter { $0.track == t.id }).count)
                stack.append(Lane(track: t.id, height: CGFloat(n) * 30 + CGFloat(n - 1) * 4, rows: n))
            } else {
                stack.append(Lane(track: t.id, height: 30))
            }
        }
        var y = height - 18 + scroll
        for i in stack.indices {
            y -= stack[i].height
            stack[i].y = y
            y -= gap
        }
        return stack
    }
    /// How far the lanes can scroll: the stack's height beyond the room under the ruler.
    private func maxScroll(_ doc: ProjectDoc?, height: CGFloat) -> CGFloat {
        guard let doc, let top = lanes(doc, height: height).last else { return 0 }
        return max(0, ruler + 6 - top.y)
    }

    var body: some View {
        GeometryReader { geo in
            let mid = geo.size.width / 2
            let origin = mid - CGFloat(model.time) * pps
            let doc = model.doc
            let scrollMax = maxScroll(doc, height: geo.size.height)
            let lift = min(vScroll, scrollMax)
            let laneList = doc.map { lanes($0, height: geo.size.height, scroll: lift) } ?? []
            let noSound = doc.map { d in !d.clips.contains { $0.track == "A1" || $0.track == "A2" } } ?? false
            ZStack(alignment: .topLeading) {
                Theme.bg
                rulerView(origin: origin, width: geo.size.width)
                if noSound, let lane = laneList.first(where: { $0.track == "A1" }) {
                    Button(action: addAudio) {
                        Label("Add audio", systemImage: "plus")
                            .font(.system(size: 12, weight: .medium))
                            .padding(.leading, 12)
                            .frame(width: geo.size.width - 16, height: lane.height, alignment: .leading)
                            .background(Theme.surface1, in: RoundedRectangle(cornerRadius: 6, style: .continuous))
                            .foregroundStyle(Theme.muted)
                    }
                    .offset(x: 8, y: lane.y)
                }
                TrackContent(model: model, pps: pps, lanes: laneList)
                    .equatable()
                    .offset(x: origin)
                    // Lanes scrolled up pass under the ruler, not over it.
                    .mask(VStack(spacing: 0) { Color.clear.frame(height: ruler); Color.black })
                if scrollMax > 0 {
                    // A thin bar on the right says there's more above or below.
                    let room = geo.size.height - ruler
                    let barH = max(24, room * room / (room + scrollMax))
                    Capsule().fill(Color.white.opacity(0.35))
                        .frame(width: 3, height: barH)
                        .offset(x: geo.size.width - 5, y: ruler + (room - barH) * (1 - lift / scrollMax))
                        .allowsHitTesting(false)
                }
                // The playhead.
                Rectangle().fill(Color.white).frame(width: 2).offset(x: mid - 1)
                    .allowsHitTesting(false)
                if let main = laneList.first {
                    Button(action: addMedia) {
                        Image(systemName: "plus").font(.system(size: 16, weight: .semibold))
                            .frame(width: 36, height: 36)
                            .background(Color.white, in: Circle())
                            .foregroundStyle(.black)
                    }
                    .offset(x: geo.size.width - 48, y: main.y + (main.height - 36) / 2)
                    .accessibilityLabel("Add media")
                }
            }
            .clipped()
            .contentShape(Rectangle())
            .gesture(
                DragGesture(minimumDistance: 2)
                    .onChanged { v in
                        // Mostly up or down (with lanes to scroll): scroll them. Otherwise scrub.
                        if dragAxis == nil {
                            dragAxis = abs(v.translation.height) > abs(v.translation.width) && scrollMax > 0 ? .vertical : .horizontal
                        }
                        if dragAxis == .vertical {
                            let s = vStart ?? lift
                            vStart = s
                            vScroll = (s + v.translation.height).clamped(0, scrollMax)
                            return
                        }
                        if model.playing { model.pause() }
                        let s = scrubStart ?? model.time
                        scrubStart = s
                        model.time = (s - Double(v.translation.width / pps)).clamped(0, max(model.duration, 0))
                    }
                    .onEnded { _ in
                        scrubStart = nil
                        vStart = nil
                        dragAxis = nil
                    }
            )
            .simultaneousGesture(
                MagnifyGesture()
                    .onChanged { v in
                        let s = zoomStart ?? pps
                        zoomStart = s
                        pps = (s * v.magnification).clamped(8, 400)
                    }
                    .onEnded { _ in zoomStart = nil }
            )
            .onTapGesture { model.selected = nil }
            .onChange(of: tileSpan) { _, s in model.setTileSpan(s) }
            .onAppear { model.setTileSpan(tileSpan) }
        }
    }

    /// Seconds each filmstrip frame covers at this zoom.
    private var tileSpan: Double {
        let w = Double(58 * min(max(model.aspect, 0.56), 1.0))
        return w / Double(pps)
    }

    private func rulerView(origin: CGFloat, width: CGFloat) -> some View {
        let step: Double = pps > 120 ? 0.5 : pps > 40 ? 1 : pps > 15 ? 5 : 10
        let first = max(0, floor(Double(-origin / pps) / step) * step)
        let last = Double((width - origin) / pps)
        let ticks = Array(stride(from: first, through: max(first, last), by: step))
        return ZStack(alignment: .topLeading) {
            ForEach(ticks, id: \.self) { t in
                let major = t.truncatingRemainder(dividingBy: step * 2) == 0
                VStack(alignment: .leading, spacing: 2) {
                    Rectangle().fill(Theme.muted.opacity(0.6)).frame(width: 1, height: major ? 8 : 4)
                    if major { Text(formatTime(t).replacingOccurrences(of: ".0", with: "")).font(.system(size: 9, design: .monospaced)).foregroundStyle(Theme.muted) }
                }
                .offset(x: origin + CGFloat(t) * pps)
            }
        }
        .frame(height: ruler, alignment: .topLeading)
        .allowsHitTesting(false)
    }
}

/// Every lane's clips, positioned from 0:00. Equal inputs mean SwiftUI skips it entirely while
/// the playhead moves.
private struct TrackContent: View, Equatable {
    let model: EditorModel
    let pps: CGFloat
    let lanes: [TimelineView.Lane]

    static func == (a: Self, b: Self) -> Bool { a.model === b.model && a.pps == b.pps && a.lanes == b.lanes }

    var body: some View {
        ZStack(alignment: .topLeading) {
            if let doc = model.doc {
                ForEach(lanes) { lane in
                    let track = doc.tracks.first { $0.id == lane.track }
                    let off = track.map { $0.hidden || $0.muted } ?? false
                    ZStack(alignment: .topLeading) {
                        let rows = stackRows(doc.clips.filter { $0.track == lane.track })
                        ForEach(doc.clips.filter { $0.track == lane.track }) { c in
                            let r = rows.row[c.id] ?? 0
                            let rowH = lane.rows > 1 ? 30 : lane.height
                            ClipView(model: model, clip: c, lane: TimelineView.Lane(track: lane.track, height: rowH), pps: pps)
                                .offset(y: CGFloat(r) * (rowH + 4))
                        }
                    }
                    .opacity(off ? 0.4 : 1)
                    .offset(y: lane.y)
                    .contextMenu { if let t = track { trackMenu(t) } }
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }

    @ViewBuilder private func trackMenu(_ t: Track) -> some View {
        let audio = t.kind == "audio"
        Section(t.name) {
            if audio {
                Button { model.edit("set_track", ["track": t.id, "muted": !t.muted]) } label: {
                    Label(t.muted ? "Unmute track" : "Mute track", systemImage: t.muted ? "speaker.wave.2" : "speaker.slash")
                }
            } else {
                Button { model.edit("set_track", ["track": t.id, "hidden": !t.hidden]) } label: {
                    Label(t.hidden ? "Show track" : "Hide track", systemImage: t.hidden ? "eye" : "eye.slash")
                }
            }
            Button { model.edit("set_track", ["track": t.id, "locked": !t.locked]) } label: {
                Label(t.locked ? "Unlock track" : "Lock track", systemImage: t.locked ? "lock.open" : "lock")
            }
        }
    }
}

/// One clip on its lane. While it's dragged (moved or trimmed) only this view changes; the edit
/// goes to the engine once, when the finger lifts.
private struct ClipView: View {
    let model: EditorModel
    let clip: Clip
    let lane: TimelineView.Lane
    let pps: CGFloat
    @State private var dx: CGFloat = 0
    @State private var kind: Kind?

    enum Kind { case move, left, right }

    var body: some View {
        let c = clip
        let selected = model.selected == c.id
        let x = CGFloat(c.start) * pps + (kind == .right || kind == nil ? 0 : dx)
        let w = max(6, CGFloat(c.dur) * pps + (kind == .left ? -dx : kind == .right ? dx : 0))
        ZStack(alignment: .leading) {
            switch lane.track {
            case "V1": Filmstrip(model: model, clip: c, width: w, height: lane.height, pps: pps)
            case _ where clip.type == "music" || clip.type == "audio": waveform(width: w)
            default: layer(width: w)
            }
            if selected {
                RoundedRectangle(cornerRadius: 6, style: .continuous).stroke(Color.white, lineWidth: 2)
                handle(.left).frame(maxWidth: .infinity, alignment: .leading)
                handle(.right).frame(maxWidth: .infinity, alignment: .trailing)
            }
        }
        .frame(width: w, height: lane.height)
        .clipShape(RoundedRectangle(cornerRadius: 6, style: .continuous))
        .padding(.horizontal, 0.75)
        .shadow(color: kind == .move ? .black.opacity(0.5) : .clear, radius: 6, y: 3)
        .offset(x: x)
        .onTapGesture { model.selected = selected ? nil : c.id }
        .gesture(selected ? drag(.move, minimum: 3) : nil)
    }

    /// Sound: its waveform, on the sound colour.
    private func waveform(width: CGFloat) -> some View {
        let peaks = model.peaks[clip.id] ?? []
        return ZStack(alignment: .leading) {
            Theme.clipColor(clip.type)
            if !peaks.isEmpty {
                Canvas { ctx, size in
                    let n = peaks.count
                    let bar = max(1, size.width / CGFloat(n))
                    var path = Path()
                    for (i, p) in peaks.enumerated() {
                        let h = max(1.5, CGFloat(p) * (size.height - 10))
                        path.addRect(CGRect(x: CGFloat(i) * bar, y: (size.height - h) / 2, width: max(1, bar - 1), height: h))
                    }
                    ctx.fill(path, with: .color(Color(hex: 0x5fd39a).opacity(0.75)))
                }
            }
            if width >= 60 {
                Text(clip.name).font(.system(size: 11, weight: .medium)).lineLimit(1)
                    .foregroundStyle(Theme.ink)
                    .padding(.leading, 7)
            }
        }
        .task(id: "\(clip.id)-\(clip.dur)-\(clip.in)") { model.loadPeaks(clip) }
    }

    /// Titles and overlays: a slim bar with what it says.
    private func layer(width: CGFloat) -> some View {
        ZStack(alignment: .leading) {
            Theme.clipColor(clip.type)
            if width >= 30 {
                HStack(spacing: 4) {
                    Image(systemName: clip.type == "text" ? "textformat" : "square.on.square").font(.system(size: 9, weight: .semibold))
                    Text(clip.type == "text" ? (clip.text?.content.replacingOccurrences(of: "\n", with: " ") ?? clip.name) : clip.name)
                        .font(.system(size: 11, weight: .medium)).lineLimit(1)
                }
                .foregroundStyle(Theme.ink)
                .padding(.horizontal, 7)
            }
        }
    }

    private func handle(_ k: Kind) -> some View {
        RoundedRectangle(cornerRadius: 3).fill(Color.white)
            .frame(width: 10, height: max(10, lane.height - 12))
            .padding(.horizontal, 2)
            .frame(width: 22, height: lane.height)
            .contentShape(Rectangle())
            .highPriorityGesture(drag(k, minimum: 1))
    }

    private func drag(_ k: Kind, minimum: CGFloat) -> some Gesture {
        DragGesture(minimumDistance: minimum)
            .onChanged { v in
                kind = k
                dx = v.translation.width
            }
            .onEnded { v in commit(k, v.translation.width) }
    }

    private func commit(_ k: Kind, _ moved: CGFloat) {
        let c = clip
        let d = Double(moved / pps)
        // Keep the dragged look until the engine's answer replaces this clip.
        defer {
            kind = nil
            dx = 0
        }
        switch k {
        case .move:
            model.update(c.id, ["start": max(0, c.start + d)])
        case .right:
            model.update(c.id, ["duration": max(0.2, c.dur + d)])
        case .left:
            let media = ["video", "audio", "music"].contains(c.type)
            var d = min(d, c.dur - 0.2)
            d = max(d, -c.start)
            if media { d = max(d, -c.in) }
            var f: [String: Any] = ["start": c.start + d, "duration": c.dur - d]
            if media { f["in"] = c.in + d }
            model.update(c.id, f)
        }
    }
}

/// The main video: frames of the edit (as it will look) laid edge to edge.
private struct Filmstrip: View {
    let model: EditorModel
    let clip: Clip
    let width: CGFloat
    let height: CGFloat
    let pps: CGFloat

    var body: some View {
        let span = model.tileSpan
        let tileW = CGFloat(span) * pps
        let first = Int(floor(clip.start / span)), last = Int(ceil(clip.end / span))
        let left = CGFloat(Double(first) * span - clip.start) * pps
        ZStack(alignment: .topLeading) {
            Theme.surface2
            ForEach(first..<max(first + 1, last), id: \.self) { i in
                FilmTile(model: model, index: i)
                    .frame(width: tileW, height: height)
                    .clipped()
                    .offset(x: left + CGFloat(i - first) * tileW)
            }
            if clip.transition != nil {
                Image(systemName: "square.on.square.intersection.dashed").font(.system(size: 10, weight: .bold))
                    .padding(4).background(.black.opacity(0.55), in: Circle()).foregroundStyle(.white)
                    .padding(4)
            }
        }
        .frame(width: width, height: height, alignment: .topLeading)
    }
}

/// One filmstrip frame; asks the model for its picture when it comes on screen.
private struct FilmTile: View {
    let model: EditorModel
    let index: Int
    var body: some View {
        ZStack {
            Theme.surface2
            if let img = model.tiles[index] {
                Image(decorative: img, scale: 1).resizable().scaledToFill()
            }
        }
        .overlay(alignment: .trailing) { Rectangle().fill(Color.black.opacity(0.25)).frame(width: 0.5) }
        .task(id: model.rev) { model.loadTile(index) }
    }
}

/// Rows for clips that share moments (titles stack): first fit by start time.
func stackRows(_ clips: [Clip]) -> (row: [String: Int], count: Int) {
    var ends: [Double] = []
    var row: [String: Int] = [:]
    for c in clips.sorted(by: { $0.start < $1.start }) {
        if let r = ends.firstIndex(where: { $0 <= c.start + 1e-6 }) {
            ends[r] = c.end
            row[c.id] = r
        } else {
            ends.append(c.end)
            row[c.id] = ends.count - 1
        }
    }
    return (row, max(1, ends.count))
}
