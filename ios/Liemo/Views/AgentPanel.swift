import SwiftUI

struct AgentPanel: View {
    let agent: Agent
    let editor: EditorModel
    @State private var prompt = ""
    @State private var settings = false
    @State private var key = ""

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                Image(systemName: "sparkles")
                Text("Liemo AI").font(.system(size: 17, weight: .semibold))
                Spacer()
                if !agent.items.isEmpty {
                    Button { agent.clear() } label: { Image(systemName: "square.and.pencil").frame(width: 32, height: 32) }
                }
                Button { key = Keychain.apiKey ?? ""; settings = true } label: { Image(systemName: "key").frame(width: 32, height: 32) }
            }
            .foregroundStyle(Theme.ink)
            .padding(.horizontal, 16).frame(height: 52)
            Divider().overlay(Theme.border)

            if !agent.hasKey {
                setup
            } else {
                ScrollViewReader { proxy in
                    ScrollView {
                        LazyVStack(alignment: .leading, spacing: 12) {
                            if agent.items.isEmpty {
                                Text("Ask for an edit: \u{201C}make a 15 second reel from the footage with a title and music\u{201D}, \u{201C}add a film look to every clip\u{201D}, \u{201C}fade the music out at the end\u{201D}.")
                                    .font(.system(size: 14)).foregroundStyle(Theme.muted).padding(.top, 8)
                            }
                            ForEach(agent.items) { item in bubble(item).id(item.id) }
                            if agent.busy { ProgressView().padding(.vertical, 4).id("busy") }
                        }
                        .padding(16)
                    }
                    .onChange(of: agent.items.count) { _, _ in withAnimation { proxy.scrollTo(agent.busy ? "busy" : agent.items.last?.id as AnyHashable?, anchor: .bottom) } }
                }
                composer
            }
        }
        .sheet(isPresented: $settings) { keySheet }
    }

    @ViewBuilder private func bubble(_ item: Agent.Item) -> some View {
        switch item {
        case .user(_, let t):
            Text(t).font(.system(size: 15))
                .padding(.horizontal, 12).padding(.vertical, 8)
                .background(Theme.surface3, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
                .frame(maxWidth: .infinity, alignment: .trailing)
        case .assistant(_, let t):
            Text((try? AttributedString(markdown: t, options: .init(interpretedSyntax: .inlineOnlyPreservingWhitespace))) ?? AttributedString(t))
                .font(.system(size: 15)).frame(maxWidth: .infinity, alignment: .leading)
        case .tool(_, let title, let ok):
            Label(title, systemImage: ok ? "checkmark.circle" : "exclamationmark.circle")
                .font(.system(size: 12)).foregroundStyle(ok ? Theme.muted : Theme.danger)
        case .error(_, let t):
            Text(t).font(.system(size: 14)).foregroundStyle(Theme.danger)
        }
    }

    private var composer: some View {
        VStack(alignment: .leading, spacing: 8) {
            Menu {
                Picker("Model", selection: Binding(get: { agent.model }, set: { agent.model = $0 })) {
                    ForEach(Agent.models) { Text($0.name).tag($0.id) }
                }
            } label: {
                HStack(spacing: 6) {
                    Circle().fill(Color(hex: 0xd97757)).frame(width: 14, height: 14)
                    Text(Agent.models.first { $0.id == agent.model }?.name ?? agent.model).font(.system(size: 13, weight: .medium))
                    Image(systemName: "chevron.down").font(.system(size: 10))
                }
                .foregroundStyle(Theme.muted)
            }
            HStack(alignment: .bottom, spacing: 8) {
                TextField("Ask Liemo AI to edit…", text: $prompt, axis: .vertical)
                    .lineLimit(1...6).font(.system(size: 15))
                    .onSubmit(send)
                Button { agent.busy ? agent.stop() : send() } label: {
                    Image(systemName: agent.busy ? "stop.fill" : "arrow.up")
                        .font(.system(size: 14, weight: .bold))
                        .frame(width: 32, height: 32)
                        .background(Color.white, in: Circle()).foregroundStyle(.black)
                }
                .disabled(!agent.busy && prompt.trimmingCharacters(in: .whitespaces).isEmpty)
            }
        }
        .padding(12)
        .background(Theme.surface2, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 16, style: .continuous).stroke(Theme.border))
        .padding(12)
    }

    private func send() {
        let p = prompt
        prompt = ""
        agent.send(p, editor: editor)
    }

    private var setup: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Connect Claude").font(.system(size: 16, weight: .semibold))
            Text("Liemo AI edits your project with Claude. Paste an Anthropic API key; it stays in this device's Keychain.")
                .font(.system(size: 14)).foregroundStyle(Theme.muted)
            SecureField("sk-ant-…", text: $key)
                .textInputAutocapitalization(.never).autocorrectionDisabled()
                .padding(10).background(Theme.surface2, in: RoundedRectangle(cornerRadius: 8))
            HStack {
                Link("Get a key", destination: URL(string: "https://console.anthropic.com/settings/keys")!).font(.system(size: 14))
                Spacer()
                Button("Save") { Keychain.apiKey = key.trimmingCharacters(in: .whitespacesAndNewlines); key = "" }
                    .buttonStyle(.borderedProminent).disabled(key.isEmpty)
            }
            Spacer()
        }
        .padding(16)
    }

    private var keySheet: some View {
        NavigationStack {
            Form {
                Section("Anthropic API key") {
                    SecureField("sk-ant-…", text: $key).textInputAutocapitalization(.never).autocorrectionDisabled()
                }
                Section { Button("Remove key", role: .destructive) { Keychain.apiKey = nil; key = ""; settings = false } }
            }
            .navigationTitle("Liemo AI").navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { settings = false } }
                ToolbarItem(placement: .confirmationAction) { Button("Save") { Keychain.apiKey = key; settings = false } }
            }
        }
        .presentationDetents([.medium])
    }
}
