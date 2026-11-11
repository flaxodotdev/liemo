import SwiftUI

enum Panel: String, Identifiable {
    case library, audio, inspector, effects, transitions, filters, voice, captions, agent, export
    case speed, animate, adjust, sound, stickers, sfx, cutout
    var id: String { rawValue }
}

struct EditorView: View {
    @State private var model: EditorModel
    @State private var agent = Agent()
    @State private var sheet: Panel?
    @State private var side: Panel? = .library
    @State private var renaming = false
    /// The preview's height on iPhone; drag the grip under the transport to change it.
    @AppStorage("liemo.previewHeight") private var previewHeight: Double = 360
    @State private var resizeStart: Double?
    @State private var newName = ""
    @Environment(\.horizontalSizeClass) private var size
    @Environment(\.dismiss) private var dismiss

    init(id: String) {
        _model = State(initialValue: EditorModel(id: id))
    }

    private var wide: Bool { size == .regular }

    var body: some View {
        HStack(spacing: 0) {
            GeometryReader { g in
                // Bars, transport, grip and toolbar always show; the preview takes what's asked
                // of it up to what leaves the timeline room, and the timeline gets the rest.
                let room = max(160, g.size.height - (48 + 52 + 18 + 64 + 200))
                VStack(spacing: 0) {
                    topBar
                    PreviewStage(model: model, editText: { show(.inspector) })
                        .frame(height: wide ? nil : min(previewHeight, room))
                        .frame(maxHeight: wide ? .infinity : nil)
                    transport
                    if !wide { grip(room) }
                    TimelineView(model: model, addMedia: { show(.library) }, addAudio: { show(.audio) })
                        .frame(minHeight: 200, maxHeight: wide ? 320 : .infinity)
                    toolbar
                }
            }
            if wide, let side {
                Divider().overlay(Theme.border)
                panel(side, inSheet: false)
                    .frame(width: 360)
                    .background(Theme.surface1)
            }
        }
        .background(Theme.bg.ignoresSafeArea())
        .onAppear {
            model.open()
            // For screenshots and UI checks: `-liemo-time 3 -liemo-select c2 -liemo-panel inspector`.
            let d = UserDefaults.standard
            if d.object(forKey: "liemo-time") != nil { model.time = d.double(forKey: "liemo-time") }
            if let c = d.string(forKey: "liemo-select") { model.selected = c }
            if let f = d.string(forKey: "liemo-font"), let c = model.selected { model.update(c, ["text": ["font": f]]) }
            if let p = d.string(forKey: "liemo-panel").flatMap(Panel.init(rawValue:)) { show(p) }
        }
        .onDisappear { model.close() }
        .sheet(item: $sheet) { p in
            panel(p, inSheet: true)
                .presentationDetents([.export, .voice, .captions, .speed].contains(p) ? [.medium] : [.medium, .large])
                .presentationBackground(Theme.surface1)
                .presentationDragIndicator(.visible)
                // Keep the preview bright and live behind a half-height sheet.
                .presentationBackgroundInteraction(.enabled(upThrough: .medium))
        }
        .alert("Liemo", isPresented: Binding(get: { model.error != nil }, set: { if !$0 { model.error = nil } })) {
            Button("OK", role: .cancel) {}
        } message: { Text(model.error ?? "") }
        .alert("Rename project", isPresented: $renaming) {
            TextField("Name", text: $newName)
            Button("Cancel", role: .cancel) {}
            Button("Rename") { model.edit("set_project", ["name": newName]) }
        }
    }

    /// Opens a panel: beside the editor on iPad, as a sheet on iPhone.
    private func show(_ p: Panel) {
        if wide && p != .export {
            side = side == p ? nil : p
        } else {
            sheet = p
        }
    }

    @ViewBuilder private func panel(_ p: Panel, inSheet: Bool) -> some View {
        let close = { if inSheet { sheet = nil } }
        switch p {
        case .library: LibraryPanel(model: model, done: close)
        case .audio: LibraryPanel(model: model, startTab: "music", done: close)
        case .inspector: InspectorPanel(model: model, open: show)
        case .effects: EffectPicker(model: model) { if inSheet { sheet = nil } else { side = .inspector } }
        case .transitions: TransitionPicker(model: model) { if inSheet { sheet = nil } else { side = .inspector } }
        case .filters: FilterPanel(model: model, done: close)
        case .voice: VoicePanel(model: model, done: close)
        case .speed: SpeedPanel(model: model, done: close)
        case .animate: AnimatePanel(model: model, done: close)
        case .adjust: AdjustPanel(model: model, done: close)
        case .sound: SoundPanel(model: model, done: close)
        case .stickers: StickerPanel(model: model, done: close)
        case .sfx: SfxPanel(model: model, done: close)
        case .cutout: CutoutPanel(model: model, done: close)
        case .captions: CaptionsPanel(model: model, done: close)
        case .agent: AgentPanel(agent: agent, editor: model)
        case .export: ExportPanel(model: model)
        }
    }

    // MARK: Bars

    private var topBar: some View {
        HStack(spacing: 4) {
            Button { dismiss() } label: { Image(systemName: "chevron.left").frame(width: 36, height: 36) }
            Menu {
                Button { newName = model.doc?.name ?? ""; renaming = true } label: { Label("Rename", systemImage: "pencil") }
                if let d = model.doc { Text("\(d.format) · \(d.aspect) · \(d.fps) fps") }
            } label: {
                HStack(spacing: 5) {
                    Text(model.doc?.name ?? "").font(.system(size: 16, weight: .semibold)).lineLimit(1)
                    Image(systemName: "chevron.down").font(.system(size: 11, weight: .semibold)).foregroundStyle(Theme.muted)
                }
            }
            Spacer(minLength: 8)
            Button { show(.agent) } label: {
                Image(systemName: "sparkles").frame(width: 36, height: 36)
                    .foregroundStyle(side == .agent && wide ? Theme.accent : Theme.ink)
            }
            Button { show(.export) } label: {
                Text(resolutionLabel).font(.system(size: 13, weight: .semibold)).foregroundStyle(Theme.muted).frame(height: 36).padding(.horizontal, 4)
            }
            Button { show(.export) } label: {
                Text("Export").font(.system(size: 14, weight: .semibold))
                    .padding(.horizontal, 14).frame(height: 32)
                    .background(Color.white, in: Capsule()).foregroundStyle(.black)
            }
        }
        .foregroundStyle(Theme.ink)
        .padding(.horizontal, 10).frame(height: 48)
    }

    /// Drag to make the preview bigger (a tall reel) or smaller (more timeline); double-tap to
    /// switch between big and normal.
    private func grip(_ room: CGFloat) -> some View {
        let most = Double(room)
        return Capsule().fill(Theme.surface3).frame(width: 40, height: 5)
            .frame(maxWidth: .infinity).frame(height: 18)
            .contentShape(Rectangle())
            .gesture(
                DragGesture(minimumDistance: 1)
                    .onChanged { v in
                        let s = resizeStart ?? previewHeight
                        resizeStart = s
                        previewHeight = (min(s, most) + Double(v.translation.height)).clamped(160, most)
                    }
                    .onEnded { _ in resizeStart = nil }
            )
            .onTapGesture(count: 2) {
                withAnimation(.easeOut(duration: 0.2)) { previewHeight = previewHeight < most - 40 ? most : 360 }
            }
            .accessibilityLabel("Resize preview")
    }

    private var resolutionLabel: String {
        switch model.doc?.exportResolution ?? 1080 {
        case ..<1000: return "SD"
        case 2000...: return "4K"
        default: return "HD"
        }
    }

    private var transport: some View {
        HStack {
            Button { model.togglePlay() } label: {
                Image(systemName: model.playing ? "pause.fill" : "play.fill").font(.system(size: 20))
                    .frame(width: 44, height: 44)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            VStack(spacing: 2) {
                Text(formatTime(model.time)).font(.system(size: 13, weight: .medium, design: .monospaced))
                Text(formatTime(model.duration)).font(.system(size: 12, design: .monospaced)).foregroundStyle(Theme.muted)
            }
            HStack(spacing: 4) {
                Button { model.undo() } label: { Image(systemName: "arrow.uturn.backward").frame(width: 36, height: 36) }
                    .disabled(!model.canUndo)
                Button { model.redo() } label: { Image(systemName: "arrow.uturn.forward").frame(width: 36, height: 36) }
                    .disabled(!model.canRedo)
            }
            .frame(maxWidth: .infinity, alignment: .trailing)
        }
        .foregroundStyle(Theme.ink)
        .padding(.horizontal, 14).frame(height: 52)
    }

    private var toolbar: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 0) {
                if let c = model.selectedClip {
                    tool(c.type == "text" ? "Edit text" : "Edit", "slider.horizontal.3") { show(.inspector) }
                    tool("Split", "scissors") { model.edit("split_clip", ["clip": c.id, "time": model.time]) }
                    if c.type == "text" {
                        tool("Animate", "wand.and.rays") { show(.animate) }
                        tool("Effects", "sparkles.rectangle.stack") { show(.effects) }
                    } else if c.isVisual {
                        tool("Speed", "gauge.with.dots.needle.67percent") { show(.speed) }
                        tool("Animate", "wand.and.rays") { show(.animate) }
                        tool("Cutout", "person.crop.rectangle") { show(.cutout) }
                        tool("Adjust", "dial.medium") { show(.adjust) }
                        tool("Filters", "camera.filters") { show(.filters) }
                        tool("Effects", "sparkles.rectangle.stack") { show(.effects) }
                        tool("Transition", "square.on.square.intersection.dashed") { show(.transitions) }
                        tool("Mirror", "arrow.left.and.right.righttriangle.left.righttriangle.right") { model.update(c.id, ["flipH": !(c.motion?.flipH ?? false)]) }
                        tool("Freeze", "snowflake") { model.freeze(c) }
                    }
                    if c.type == "audio" { tool("Sound", "waveform.badge.magnifyingglass") { show(.sound) } }
                    if c.hasSound { tool("Volume", "speaker.wave.2") { show(.inspector) } }
                    tool("Duplicate", "plus.square.on.square") { model.edit("duplicate_clip", ["clip": c.id]) }
                    tool("Delete", "trash") { model.edit("remove_clip", ["clip": c.id]) }
                    tool("Done", "checkmark") { model.selected = nil }
                } else {
                    tool("Media", "photo.on.rectangle") { show(.library) }
                    tool("Audio", "music.note") { show(.audio) }
                    tool("Text", "textformat") {
                        if model.add("add_text", ["content": "Your title", "start": model.time]) { show(.inspector) }
                    }
                    tool("Stickers", "face.smiling") { show(.stickers) }
                    tool("Sound FX", "speaker.wave.3") { show(.sfx) }
                    tool("Voice", "mic") { show(.voice) }
                    tool("Captions", "captions.bubble") { show(.captions) }
                    tool("Filters", "camera.filters") { show(.filters) }
                    tool("Split", "scissors") { model.edit("split_clip", ["time": model.time]) }
                    tool("Liemo AI", "sparkles") { show(.agent) }
                }
            }
            .padding(.horizontal, 8)
        }
        .frame(height: 64)
        .background(Theme.surface1)
    }

    private func tool(_ title: String, _ icon: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            VStack(spacing: 4) {
                Image(systemName: icon).font(.system(size: 18))
                Text(title).font(.system(size: 11))
            }
            .frame(minWidth: 64).frame(height: 56)
            .foregroundStyle(Theme.ink)
        }
    }
}

struct PreviewStage: View {
    let model: EditorModel
    /// Tapping a title that's already selected opens its text settings.
    var editText: () -> Void = {}
    @Environment(\.displayScale) private var scale

    var body: some View {
        GeometryReader { geo in
            let a = model.aspect
            let fit = geo.size.width / geo.size.height > a
                ? CGSize(width: geo.size.height * a, height: geo.size.height)
                : CGSize(width: geo.size.width, height: geo.size.width / a)
            let k = fit.width / (model.doc?.canvas.width ?? 1280)
            ZStack {
                Color.black
                if let l = live, let bg = l.bg {
                    Image(decorative: bg, scale: 1).resizable().interpolation(.high)
                } else if let f = model.frame {
                    Image(decorative: f, scale: 1).resizable().interpolation(.high)
                }
                // While a finger moves a clip, the clip alone moves over a still background.
                if let l = live, let layer = l.layer {
                    Image(decorative: layer, scale: 1).resizable().interpolation(.high)
                        .scaleEffect(l.scale, anchor: anchor(l, fit))
                        .rotationEffect(l.rot, anchor: anchor(l, fit))
                        .offset(l.offset)
                        .allowsHitTesting(false)
                }
                selectionBox(fit: fit, k: k)
            }
            .frame(width: fit.width, height: fit.height)
            .coordinateSpace(.named("stage"))
            .clipped()
            .overlay(Rectangle().stroke(Theme.border, lineWidth: 1))
            .overlay(alignment: .topTrailing) {
                if model.selectedClip?.type == "text" {
                    Button { tilt.toggle() } label: {
                        Text("3D").font(.system(size: 12, weight: .bold))
                            .padding(.horizontal, 10).frame(height: 26)
                            .background(tilt ? Color.white : Color.black.opacity(0.55), in: Capsule())
                            .foregroundStyle(tilt ? Color.black : Color.white)
                    }
                    .padding(8)
                    .accessibilityLabel("Tilt in 3D by dragging")
                }
            }
            .contentShape(Rectangle())
            .onTapGesture(coordinateSpace: .local) { p in tapped(p, k: k) }
            .gesture(moveOrTilt(fit: fit, k: k))
            .simultaneousGesture(pinch(fit: fit))
            .simultaneousGesture(twist(fit: fit))
            .position(x: geo.size.width / 2, y: geo.size.height / 2)
            .onAppear { setMax(fit) }
            .onChange(of: fit) { _, f in setMax(f) }
        }
        .padding(12)
        .task(id: boxKey) { refreshBox() }
        .onChange(of: model.frameRev) { _, r in
            // Hand back to the engine's frame once it shows the committed edit (no flash between).
            if let c = live?.committed, r >= c { live = nil }
        }
    }

    // MARK: Selecting

    private func tapped(_ p: CGPoint, k: CGFloat) {
        let hit = Core.shared.engine.hitTest(time: model.time, x: Double(p.x / k), y: Double(p.y / k))
        if let hit {
            if hit == model.selected, model.selectedClip?.type == "text" { editText() }
            model.selected = hit
        } else if model.selected != nil {
            model.selected = nil
        } else {
            model.togglePlay()
        }
    }

    private var boxKey: String { "\(model.selected ?? "")|\(model.rev)|\(model.time)" }
    @State private var box: [Double] = []
    private func refreshBox() {
        guard let s = model.selected else { box = []; return }
        box = Core.shared.engine.clipBox(clipId: s, time: model.time)
    }

    /// The selected clip's outline and a corner handle to resize it with one finger. It follows the
    /// moving layer during a gesture.
    @ViewBuilder
    private func selectionBox(fit: CGSize, k: CGFloat) -> some View {
        if box.count == 7, let c = model.selectedClip, model.time >= c.start, model.time < c.end {
            let (cx, cy, w, h) = (box[0] * k, box[1] * k, box[2] * k, box[3] * k)
            let rot = Angle.degrees(box[4])
            // Follows the moving layer only while fingers are down: once the edit lands, the box
            // is already where the clip is.
            let l = live?.committed == nil ? live : nil
            ZStack {
                Rectangle().stroke(Color.white.opacity(0.9), lineWidth: 1.5)
                    .frame(width: w, height: h)
                    .position(x: cx, y: cy)
                    .rotationEffect(rot, anchor: UnitPoint(x: cx / fit.width, y: cy / fit.height))
                // Bottom-right corner, turned with the clip.
                let corner = CGPoint(x: cx + (w / 2) * cos(rot.radians) - (h / 2) * sin(rot.radians),
                                     y: cy + (w / 2) * sin(rot.radians) + (h / 2) * cos(rot.radians))
                Circle().fill(Color.white).frame(width: 18, height: 18)
                    .overlay(Circle().stroke(Color.black.opacity(0.4), lineWidth: 1))
                    .frame(width: 44, height: 44).contentShape(Rectangle())
                    .position(corner)
                    .highPriorityGesture(resize(fit: fit, k: k))
                    .accessibilityLabel("Resize")
            }
            .scaleEffect(l?.scale ?? 1, anchor: l.map { anchor($0, fit) } ?? .center)
            .rotationEffect(l?.rot ?? .zero, anchor: l.map { anchor($0, fit) } ?? .center)
            .offset(l?.offset ?? .zero)
        }
    }

    // MARK: Moving the selected clip with fingers

    /// What a gesture shows while it runs: the frame without the clip, the clip alone, and how far
    /// it has moved, grown and turned (or, for 3D text, the text re-drawn as it tilts).
    struct Live {
        var clip: String
        var anchor: CGPoint
        var bg: CGImage?
        var layer: CGImage?
        var offset: CGSize = .zero
        var scale: CGFloat = 1
        var rot: Angle = .zero
        var tilt: (x: Double, y: Double)?
        /// The edit version that holds the result, once committed.
        var committed: Int?
    }
    @State private var live: Live?
    @State private var tilt = false
    @State private var start: (x: Double, y: Double, scale: Double, rot: Double, tx: Double, ty: Double)?
    /// Which gestures are under way (drag, pinch, twist, handle); the edit lands when all end.
    @State private var fingers: Set<String> = []
    @State private var layerBusy = false
    @State private var pointsPerUnit: CGFloat = 1
    @State private var handleFrom: CGFloat?

    private func anchor(_ l: Live, _ fit: CGSize) -> UnitPoint {
        UnitPoint(x: l.anchor.x / fit.width, y: l.anchor.y / fit.height)
    }

    /// The clip a new gesture works on: the one under the finger, else the selected one.
    private func target(at p: CGPoint?, k: CGFloat) -> Clip? {
        if let p, let hit = Core.shared.engine.hitTest(time: model.time, x: Double(p.x / k), y: Double(p.y / k)), hit != model.selected {
            model.selected = hit
        }
        return model.selectedClip
    }

    /// A gesture joins: the first one sets up the still background and the clip's own layer.
    private func join(_ g: String, at p: CGPoint?, fit: CGSize) -> Bool {
        if fingers.contains(g) { return live?.committed == nil }
        let k = fit.width / (model.doc?.canvas.width ?? 1280)
        if fingers.isEmpty {
            guard live == nil || live?.committed != nil, let c = target(at: p, k: k), c.isVisual,
                  model.time >= c.start, model.time < c.end else { return false }
            model.pause()
            model.beginGesture()
            pointsPerUnit = k
            start = (c.x, c.y, c.scale, c.rot, c.text?.tiltX ?? 0, c.text?.tiltY ?? 0)
            let fresh = Core.shared.engine.clipBox(clipId: c.id, time: model.time)
            let (w, h) = (model.doc?.canvas.width ?? 1280, model.doc?.canvas.height ?? 720)
            let au = fresh.count == 7 ? CGPoint(x: fresh[5], y: fresh[6]) : CGPoint(x: w / 2 + c.x, y: h / 2 + c.y)
            live = Live(clip: c.id, anchor: CGPoint(x: au.x * k, y: au.y * k))
            let (id, t, max, engine) = (c.id, model.time, model.previewMax, Core.shared.engine)
            Task.detached(priority: .userInitiated) {
                let bg = engine.renderFrameWithout(clipId: id, time: t, max: max)?.cgImage
                let layer = engine.renderClip(clipId: id, time: t, max: max)?.cgImage
                await MainActor.run {
                    guard self.live?.clip == id, self.live?.committed == nil else { return }
                    self.live?.bg = bg
                    if self.live?.layer == nil { self.live?.layer = layer }
                }
            }
        }
        fingers.insert(g)
        return true
    }

    /// A gesture ends; when it's the last, one edit with where the clip ended up.
    private func leave(_ g: String) {
        guard fingers.remove(g) != nil, fingers.isEmpty else { return }
        defer { start = nil }
        guard let l = live, let s = start, let c = model.selectedClip, c.id == l.clip else {
            model.endGesture()
            live = nil
            return
        }
        var f: [String: Any] = [:]
        if let t = l.tilt { f["text"] = ["tiltX": t.x, "tiltY": t.y] }
        if l.offset != .zero {
            f["x"] = s.x + Double(l.offset.width / pointsPerUnit)
            f["y"] = s.y + Double(l.offset.height / pointsPerUnit)
        }
        if abs(l.scale - 1) > 0.001 { f["scale"] = (s.scale * Double(l.scale)).clamped(0.05, 8) }
        if abs(l.rot.degrees) > 0.01 { f["rotation"] = s.rot + l.rot.degrees }
        if !f.isEmpty { model.update(c.id, f) }
        model.endGesture()
        if f.isEmpty { live = nil } else { live?.committed = model.rev }
    }

    private func moveOrTilt(fit: CGSize, k: CGFloat) -> some Gesture {
        DragGesture(minimumDistance: 4)
            .onChanged { v in
                guard join("drag", at: v.startLocation, fit: fit), let s = start, var l = live else { return }
                if tilt && model.selectedClip?.type == "text" {
                    let t = ((s.tx - v.translation.height * 0.35).clamped(-70, 70), (s.ty + v.translation.width * 0.35).clamped(-70, 70))
                    l.tilt = t
                    live = l
                    redrawLayer(l.clip, tilt: t)
                } else {
                    l.offset = v.translation
                    live = l
                }
            }
            .onEnded { _ in leave("drag") }
    }

    /// 3D tilt changes how the text itself is drawn: the engine re-draws just that clip.
    private func redrawLayer(_ id: String, tilt t: (x: Double, y: Double)) {
        guard !layerBusy else { return }
        layerBusy = true
        model.liveUpdate(id, ["text": ["tiltX": t.x, "tiltY": t.y]])
        let (time, max, engine) = (model.time, model.previewMax, Core.shared.engine)
        Task.detached(priority: .userInitiated) {
            let layer = engine.renderClip(clipId: id, time: time, max: max)?.cgImage
            await MainActor.run {
                self.layerBusy = false
                if self.live?.clip == id, self.live?.committed == nil, let layer { self.live?.layer = layer }
                // Catch up with where the finger went meanwhile.
                if let l = self.live, let lt = l.tilt, l.committed == nil, lt.x != t.x || lt.y != t.y { self.redrawLayer(id, tilt: lt) }
            }
        }
    }

    private func pinch(fit: CGSize) -> some Gesture {
        MagnifyGesture()
            .onChanged { v in
                guard join("pinch", at: nil, fit: fit) else { return }
                live?.scale = v.magnification
            }
            .onEnded { _ in leave("pinch") }
    }

    private func twist(fit: CGSize) -> some Gesture {
        RotateGesture()
            .onChanged { v in
                guard join("twist", at: nil, fit: fit) else { return }
                live?.rot = v.rotation
            }
            .onEnded { _ in leave("twist") }
    }

    /// The corner handle: the clip grows or shrinks about its anchor as the finger moves away
    /// from it or towards it.
    private func resize(fit: CGSize, k: CGFloat) -> some Gesture {
        DragGesture(minimumDistance: 1, coordinateSpace: .named("stage"))
            .onChanged { v in
                guard join("handle", at: nil, fit: fit), let l = live else { return }
                let d = { (p: CGPoint) in hypot(p.x - l.anchor.x, p.y - l.anchor.y) }
                if handleFrom == nil { handleFrom = max(8, d(v.startLocation)) }
                live?.scale = max(0.05, d(v.location) / (handleFrom ?? 1))
            }
            .onEnded { _ in
                handleFrom = nil
                leave("handle")
            }
    }

    private func setMax(_ s: CGSize) {
        // Render at the screen's pixels, capped so playback keeps up.
        let px = UInt32((max(s.width, s.height) * scale).clamped(320, 1440))
        if px != model.previewMax {
            model.previewMax = px
            model.requestFrame()
        }
    }
}
