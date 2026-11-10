import SwiftUI

struct EffectPicker: View {
    let model: EditorModel
    let done: () -> Void
    @State private var query = ""

    @MainActor static let defs: [EffectDef] = (try? Core.shared.decode(EffectList.self, "list_effects"))?.effects ?? []

    static func title(_ group: String) -> String {
        switch group {
        case "Motion": return "Motion & beat"
        case "Light": return "Light & film"
        case "Distort": return "Distort & camera"
        case "Color": return "Colour & look"
        case "Scale": return "Colour scale"
        case "Filters": return "Instagram filters"
        case "Shader": return "Shader filters"
        default: return group
        }
    }

    var body: some View {
        let q = query.lowercased()
        let shown = Self.defs.filter { q.isEmpty || $0.name.lowercased().contains(q) || $0.group.lowercased().contains(q) }
        // The lively ones first; the rest in the engine's order.
        let first = ["Motion", "Light", "Particles", "Layout"]
        let found = shown.reduce(into: [String]()) { if !$0.contains($1.group) { $0.append($1.group) } }
        let groups = first.filter(found.contains) + found.filter { !first.contains($0) }
        VStack(spacing: 0) {
            header("Effects", done: done)
            TextField("Search effects", text: $query)
                .padding(10).background(Theme.surface2, in: RoundedRectangle(cornerRadius: 8)).padding(.horizontal, 16)
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 18) {
                    ForEach(groups, id: \.self) { g in
                        Text(Self.title(g).uppercased()).font(.system(size: 11, weight: .semibold)).foregroundStyle(Theme.muted)
                        LazyVGrid(columns: [GridItem(.adaptive(minimum: 100), spacing: 10)], spacing: 12) {
                            ForEach(shown.filter { $0.group == g }) { e in
                                Button {
                                    if let c = model.selected { model.edit("apply_effect", ["clip": c, "effect": e.id]) }
                                    done()
                                } label: {
                                    VStack(alignment: .leading, spacing: 5) {
                                        EngineImage(key: "fx:\(e.id)") { Core.shared.engine.effectThumb(id: e.id) }
                                            .aspectRatio(16 / 9, contentMode: .fit)
                                            .clipShape(RoundedRectangle(cornerRadius: 7, style: .continuous))
                                        Text(e.name).font(.system(size: 12)).lineLimit(1)
                                    }
                                }
                                .buttonStyle(.plain)
                            }
                        }
                    }
                }
                .padding(16)
            }
        }
    }
}

struct TransitionPicker: View {
    let model: EditorModel
    let done: () -> Void

    @MainActor static let defs: [TransitionDef] = (try? Core.shared.decode(TransitionList.self, "list_transitions"))?.transitions ?? []
    @MainActor static func name(_ id: String) -> String { defs.first { $0.id == id }?.name ?? id }

    var body: some View {
        let groups = Self.defs.reduce(into: [String]()) { if !$0.contains($1.group) { $0.append($1.group) } }
        VStack(spacing: 0) {
            header("Transition in", done: done)
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 18) {
                    Button { set("none") } label: {
                        Label("No transition", systemImage: "xmark").font(.system(size: 14, weight: .medium))
                    }
                    ForEach(groups, id: \.self) { g in
                        Text(g.uppercased()).font(.system(size: 11, weight: .semibold)).foregroundStyle(Theme.muted)
                        LazyVGrid(columns: [GridItem(.adaptive(minimum: 100), spacing: 10)], spacing: 12) {
                            ForEach(Self.defs.filter { $0.group == g }) { t in
                                Button { set(t.id) } label: {
                                    VStack(alignment: .leading, spacing: 5) {
                                        EngineImage(key: "tr:\(t.id)") { Core.shared.engine.transitionThumb(id: t.id) }
                                            .aspectRatio(16 / 9, contentMode: .fit)
                                            .clipShape(RoundedRectangle(cornerRadius: 7, style: .continuous))
                                        Text(t.name).font(.system(size: 12)).lineLimit(1)
                                    }
                                }
                                .buttonStyle(.plain)
                            }
                        }
                    }
                }
                .padding(16)
            }
        }
    }

    private func set(_ id: String) {
        if let c = model.selectedClip {
            var a: [String: Any] = ["clip": c.id, "transition": id]
            if id != "none" { a["duration"] = id == "text-zoom" ? 1.2 : (c.transition?.dur ?? 0.6) }
            model.edit("set_transition", a)
            // Text zoom opens through the title over the cut; without one, add a big word to edit.
            if id == "text-zoom", !(model.doc?.clips.contains { $0.type == "text" && $0.start <= c.start && $0.end > c.start - 0.5 } ?? false) {
                let start = max(0, c.start - 1.6)
                model.edit("add_text", ["content": "WOW", "start": start, "duration": c.start - start + 0.2,
                                        "font": "Bebas Neue", "size": 420, "animation": "pop"])
                model.selected = c.id
            }
        }
        done()
    }
}

func header(_ title: String, done: @escaping () -> Void) -> some View {
    HStack {
        Text(title).font(.system(size: 17, weight: .semibold))
        Spacer()
        Button("Done", action: done).font(.system(size: 15, weight: .medium))
    }
    .padding(16)
}
