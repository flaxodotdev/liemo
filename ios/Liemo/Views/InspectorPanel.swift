import SwiftUI

struct InspectorPanel: View {
    let model: EditorModel
    let open: (Panel) -> Void

    var body: some View {
        ScrollView {
            if let c = model.selectedClip {
                VStack(alignment: .leading, spacing: 22) {
                    Text(c.name).font(.system(size: 17, weight: .semibold))
                    if let t = c.text { TextSection(model: model, clip: c, text: t) }
                    if c.isVisual { transform(c) }
                    if c.hasSound { sound(c) }
                    if c.isVisual { effects(c); transition(c) }
                    timing(c)
                }
                .padding(16)
                .id(c.id)   // fresh drafts for another clip; edits to this one keep the views (and open pickers) alive
            } else {
                VStack(spacing: 8) {
                    Text("Nothing selected").font(.system(size: 16, weight: .semibold))
                    Text("Tap a clip on the timeline to edit it.").font(.system(size: 14)).foregroundStyle(Theme.muted)
                }
                .frame(maxWidth: .infinity).padding(.top, 60)
            }
        }
    }

    private func transform(_ c: Clip) -> some View {
        Section("Transform") {
            SliderRow(label: "Scale", value: c.scale, range: 0.1...4, fmt: { "\(Int($0 * 100))%" }) { model.update(c.id, ["scale": $0]) }
            SliderRow(label: "X", value: c.x, range: -1500...1500, fmt: { "\(Int($0))" }) { model.update(c.id, ["x": $0]) }
            SliderRow(label: "Y", value: c.y, range: -1500...1500, fmt: { "\(Int($0))" }) { model.update(c.id, ["y": $0]) }
            SliderRow(label: "Rotation", value: c.rot, range: -180...180, fmt: { "\(Int($0))°" }) { model.update(c.id, ["rotation": $0]) }
            SliderRow(label: "Opacity", value: c.opacity, range: 0...1, fmt: { "\(Int($0 * 100))%" }) { model.update(c.id, ["opacity": $0]) }
            if c.type != "text" {
                Toggle("Mirror", isOn: Binding(get: { c.motion?.flipH ?? false }, set: { model.update(c.id, ["flipH": $0]) })).font(.system(size: 14))
                Toggle("Flip upside down", isOn: Binding(get: { c.motion?.flipV ?? false }, set: { model.update(c.id, ["flipV": $0]) })).font(.system(size: 14))
            }
        }
    }

    private func sound(_ c: Clip) -> some View {
        Section("Sound") {
            SliderRow(label: "Volume", value: c.volume, range: 0...1, fmt: { "\(Int($0 * 100))%" }) { model.update(c.id, ["volume": $0]) }
        }
    }

    private func timing(_ c: Clip) -> some View {
        Section("Timing") {
            SliderRow(label: "Duration", value: c.dur, range: 0.2...max(30, c.dur * 2), fmt: { String(format: "%.1fs", $0) }) { model.update(c.id, ["duration": $0]) }
            SliderRow(label: "Fade in", value: c.fadeIn, range: 0...max(0.1, min(5, c.dur / 2)), fmt: { String(format: "%.1fs", $0) }) { model.update(c.id, ["fadeIn": $0]) }
            SliderRow(label: "Fade out", value: c.fadeOut, range: 0...max(0.1, min(5, c.dur / 2)), fmt: { String(format: "%.1fs", $0) }) { model.update(c.id, ["fadeOut": $0]) }
        }
    }

    private func effects(_ c: Clip) -> some View {
        Section("Effects") {
            ForEach(c.effects) { e in EffectRow(model: model, clip: c, effect: e) }
            Button { open(.effects) } label: { Label("Add effect", systemImage: "plus") }
                .font(.system(size: 14, weight: .medium))
        }
    }

    private func transition(_ c: Clip) -> some View {
        Section("Transition in") {
            Button { open(.transitions) } label: {
                HStack {
                    Text(c.transition.map { t in TransitionPicker.name(t.id) } ?? "None")
                    Spacer()
                    Image(systemName: "chevron.right").foregroundStyle(Theme.muted)
                }
                .font(.system(size: 14)).foregroundStyle(Theme.ink)
            }
            if let t = c.transition {
                SliderRow(label: "Length", value: t.dur, range: 0.1...3, fmt: { String(format: "%.1fs", $0) }) {
                    model.edit("set_transition", ["clip": c.id, "transition": t.id, "duration": $0])
                }
            }
        }
    }
}

private struct Section<Content: View>: View {
    let title: String
    @ViewBuilder let content: Content
    init(_ title: String, @ViewBuilder content: () -> Content) {
        self.title = title
        self.content = content()
    }
    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(title.uppercased()).font(.system(size: 11, weight: .semibold)).foregroundStyle(Theme.muted)
            content
        }
    }
}

/// A slider that edits a draft while dragging and commits once, on release (one undo step).
struct SliderRow: View {
    let label: String
    let value: Double
    let range: ClosedRange<Double>
    let fmt: (Double) -> String
    let commit: (Double) -> Void
    @State private var v: Double
    @State private var dragging = false

    init(label: String, value: Double, range: ClosedRange<Double>, fmt: @escaping (Double) -> String, commit: @escaping (Double) -> Void) {
        self.label = label
        self.value = value
        self.range = range
        self.fmt = fmt
        self.commit = commit
        _v = State(initialValue: value)
    }

    var body: some View {
        VStack(spacing: 2) {
            HStack {
                Text(label).font(.system(size: 14))
                Spacer()
                Text(fmt(v)).font(.system(size: 13, design: .monospaced)).foregroundStyle(Theme.muted)
            }
            Slider(value: $v, in: range) { editing in
                dragging = editing
                if !editing && v != value { commit(v) }
            }
        }
        .onChange(of: value) { _, new in if !dragging { v = new } }
    }
}

private struct TextSection: View {
    let model: EditorModel
    let clip: Clip
    let text: TextSpec
    @State private var content: String
    @State private var color: Color
    @State private var fonts = false

    init(model: EditorModel, clip: Clip, text: TextSpec) {
        self.model = model
        self.clip = clip
        self.text = text
        _content = State(initialValue: text.content)
        _color = State(initialValue: Color(css: text.color) ?? .white)
    }

    private func set(_ f: [String: Any]) { model.update(clip.id, ["text": f]) }

    var body: some View {
        Section("Style") {
            TextStyleStrip(current: text) { set($0.update) }
            StyleColorRow(label: "Outline", value: text.outline, fallback: "#000000") { set(["outline": $0 ?? "none"]) }
            if text.outline != nil {
                SliderRow(label: "Outline width", value: text.outlineWidth ?? 6, range: 1...24, fmt: { "\(Int($0))" }) { set(["outlineWidth": $0]) }
            }
            StyleColorRow(label: "Shadow", value: text.shadow, fallback: "#000000") { set(["shadow": $0 ?? "none"]) }
            StyleColorRow(label: "Glow", value: text.glow, fallback: "#ff2bd6") { set(["glow": $0 ?? "none"]) }
            StyleColorRow(label: "Highlight", value: text.highlight, fallback: "#ffd400") { set(["highlight": $0 ?? "none"]) }
            StyleColorRow(label: "Liquid glass", value: text.glass, fallback: "#ffffff") { set(["glass": $0 ?? "none"]) }
            SliderRow(label: "3D tilt ↕", value: text.tiltX ?? 0, range: -70...70, fmt: { "\(Int($0))°" }) { set(["tiltX": $0]) }
            SliderRow(label: "3D tilt ↔", value: text.tiltY ?? 0, range: -70...70, fmt: { "\(Int($0))°" }) { set(["tiltY": $0]) }
            VStack(alignment: .leading, spacing: 6) {
                Text("Blend").font(.system(size: 14))
                Picker("Blend", selection: Binding(get: { text.blend ?? "normal" }, set: { set(["blend": $0]) })) {
                    Text("None").tag("normal")
                    Text("Invert").tag("invert")
                    Text("Soft invert").tag("exclusion")
                }
                .pickerStyle(.segmented)
                Text("Invert flips the text to the opposite of the picture behind it, so it reads on anything.")
                    .font(.system(size: 12)).foregroundStyle(Theme.muted)
            }
        }
        Section("Text") {
            TextField("Text", text: $content, axis: .vertical)
                .lineLimit(1...5)
                .padding(10).background(Theme.surface2, in: RoundedRectangle(cornerRadius: 8))
                .onSubmit { set(["content": content]) }
                .onChange(of: content) { _, _ in debounce() }
            Button { fonts = true } label: {
                HStack {
                    Text("Font").foregroundStyle(Theme.ink)
                    Spacer()
                    Text(text.font).font(.custom(text.font, size: 15)).foregroundStyle(Theme.muted)
                    Image(systemName: "chevron.right").foregroundStyle(Theme.muted)
                }
                .font(.system(size: 14))
            }
            // On the button itself: a modifier on the Section would be copied onto every row,
            // giving several font sheets on one switch.
            .sheet(isPresented: $fonts) {
                FontPicker(current: text.font) { f in
                    fonts = false
                    if let f { set(["font": f]) }
                }
            }
            SliderRow(label: "Size", value: text.size, range: 12...300, fmt: { "\(Int($0))" }) { set(["size": $0]) }
            Picker("Weight", selection: Binding(get: { Int(text.weight) }, set: { set(["weight": $0]) })) {
                ForEach([400, 500, 600, 700, 800], id: \.self) { Text("\($0)").tag($0) }
            }
            .pickerStyle(.segmented)
            Picker("Align", selection: Binding(get: { text.align }, set: { set(["align": $0]) })) {
                Image(systemName: "text.alignleft").tag("left")
                Image(systemName: "text.aligncenter").tag("center")
                Image(systemName: "text.alignright").tag("right")
            }
            .pickerStyle(.segmented)
            ColorPicker("Colour", selection: $color, supportsOpacity: false)
                .font(.system(size: 14))
                .onChange(of: color) { _, c in colorChanged(c) }
            HStack {
                Text("Animation").font(.system(size: 14))
                Spacer()
                Picker("Animation", selection: Binding(get: { text.anim }, set: { set(["animation": $0]) })) {
                    ForEach(AnimatePanel.textAnims, id: \.0) { Text($0.1).tag($0.0) }
                }
            }
            Toggle("Backing box", isOn: Binding(get: { text.box }, set: { set(["box": $0]) })).font(.system(size: 14))
        }
        .onChange(of: text.color) { _, new in
            if new != color.css, let c = Color(css: new) { color = c }
        }
        .onChange(of: text.content) { _, new in
            if new != content && pending == nil { content = new }
        }
    }

    @State private var pending: Task<Void, Never>?
    @State private var colorPending: Task<Void, Never>?

    /// The colour picker reports every step of a drag; send the colour it settles on.
    private func colorChanged(_ c: Color) {
        colorPending?.cancel()
        let css = c.css
        guard css != text.color else { return }
        colorPending = Task {
            try? await Task.sleep(for: .milliseconds(250))
            if !Task.isCancelled { set(["color": css]) }
        }
    }

    private func debounce() {
        pending?.cancel()
        let c = content
        pending = Task {
            try? await Task.sleep(for: .milliseconds(600))
            if !Task.isCancelled && c != text.content { set(["content": c]) }
            if !Task.isCancelled { pending = nil }
        }
    }
}

private struct FontPicker: View {
    let current: String
    let done: (String?) -> Void
    @State private var query = ""
    @State private var lib: Library?

    var body: some View {
        NavigationStack {
            List {
                let q = query.lowercased()
                let bundled = (lib?.fonts.map(\.family) ?? []).filter { q.isEmpty || $0.lowercased().contains(q) }
                let installed = (lib?.installedFonts ?? []).filter { q.isEmpty || $0.lowercased().contains(q) }
                if !bundled.isEmpty { Section("Liemo") { ForEach(bundled, id: \.self, content: row) } }
                if !installed.isEmpty { Section("On this device") { ForEach(installed, id: \.self, content: row) } }
            }
            .searchable(text: $query)
            .navigationTitle("Font").navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { done(nil) } } }
            .onAppear { lib = try? Core.shared.decode(Library.self, "list_library") }
        }
    }

    private func row(_ f: String) -> some View {
        // One plain button per row, its tap area exactly its own row. Some fonts (Zapfino,
        // script faces) draw far outside their line, so the name is clipped to the row too.
        Button { done(f) } label: {
            HStack {
                Text(f).font(.custom(f, size: 17)).foregroundStyle(Theme.ink)
                    .lineLimit(1).minimumScaleFactor(0.6)
                    .frame(maxWidth: .infinity, alignment: .leading)
                if f == current { Image(systemName: "checkmark").foregroundStyle(Theme.accent) }
            }
            .frame(height: 34)
            .clipped()
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .id(f)
    }
}

struct EffectRow: View {
    let model: EditorModel
    let clip: Clip
    let effect: AppliedEffect
    @State private var expanded: Bool

    init(model: EditorModel, clip: Clip, effect: AppliedEffect, expandedAtStart: Bool = false) {
        self.model = model
        self.clip = clip
        self.effect = effect
        _expanded = State(initialValue: expandedAtStart)
    }

    private var def: EffectDef? { EffectPicker.defs.first { $0.id == effect.id } }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Toggle(def?.name ?? effect.id, isOn: Binding(get: { effect.enabled }, set: { up(["enabled": $0]) }))
                    .font(.system(size: 14, weight: .medium))
                Button { model.edit("remove_effect", ["clip": clip.id, "uid": effect.uid]) } label: {
                    Image(systemName: "xmark").font(.system(size: 12)).frame(width: 28, height: 28)
                }
                .foregroundStyle(Theme.muted)
            }
            SliderRow(label: "Amount", value: effect.amount, range: 0...1, fmt: { "\(Int($0 * 100))%" }) { up(["amount": $0]) }
            if let d = def, let ps = d.params, !ps.isEmpty {
                DisclosureGroup("Settings", isExpanded: $expanded) {
                    VStack(spacing: 12) {
                        ForEach(ps, id: \.key) { p in param(p) }
                    }
                    .padding(.top, 8)
                }
                .font(.system(size: 13))
            }
        }
        .padding(12)
        .background(Theme.surface2, in: RoundedRectangle(cornerRadius: 10))
    }

    private func up(_ f: [String: Any]) {
        var a = f
        a["clip"] = clip.id
        a["uid"] = effect.uid
        model.edit("update_effect", a)
    }

    private func value(_ p: EffectDef.Param) -> JSONValue? { effect.params[p.key] ?? p.default }

    @ViewBuilder private func param(_ p: EffectDef.Param) -> some View {
        switch p.type {
        case "number":
            SliderRow(label: p.label, value: value(p)?.number ?? p.min ?? 0, range: (p.min ?? 0)...max((p.max ?? 1), (p.min ?? 0) + 0.001),
                      fmt: { v in (p.step ?? 0.01) >= 1 ? "\(Int(v))\(p.unit ?? "")" : String(format: "%.2f", v) }) { up(["params": [p.key: $0]]) }
        case "select":
            HStack {
                Text(p.label).font(.system(size: 14))
                Spacer()
                Picker(p.label, selection: Binding(get: { value(p)?.string ?? "" }, set: { up(["params": [p.key: $0]]) })) {
                    ForEach(p.options ?? [], id: \.self) { Text($0.capitalized).tag($0) }
                }
            }
        case "color":
            ColorPicker(p.label, selection: Binding(get: { Color(css: value(p)?.string ?? "") ?? .white }, set: { up(["params": [p.key: $0.css]]) }), supportsOpacity: false)
                .font(.system(size: 14))
        default:
            Toggle(p.label, isOn: Binding(get: { value(p)?.bool ?? false }, set: { up(["params": [p.key: $0]]) })).font(.system(size: 14))
        }
    }
}
