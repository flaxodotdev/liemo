import SwiftUI

enum Panel: String, Identifiable {
    case library, audio, inspector, effects, transitions, filters, voice, captions, agent, export
    case speed, animate, adjust, sound, stickers, sfx
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
                    PreviewStage(model: model)
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
    @Environment(\.displayScale) private var scale

    var body: some View {
        GeometryReader { geo in
            let a = model.aspect
            let fit = geo.size.width / geo.size.height > a
                ? CGSize(width: geo.size.height * a, height: geo.size.height)
                : CGSize(width: geo.size.width, height: geo.size.width / a)
            ZStack {
                Color.black
                if let f = model.frame { Image(decorative: f, scale: 1).resizable().interpolation(.high) }
            }
            .frame(width: fit.width, height: fit.height)
            .overlay(Rectangle().stroke(Theme.border, lineWidth: 1))
            .position(x: geo.size.width / 2, y: geo.size.height / 2)
            .onAppear { setMax(fit) }
            .onChange(of: fit) { _, f in setMax(f) }
            .onTapGesture { model.togglePlay() }
        }
        .padding(12)
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
