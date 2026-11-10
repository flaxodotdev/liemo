import Foundation
import Observation

/// Liemo AI on iPhone and iPad: Claude through the Anthropic API, editing with the same tools
/// the desktop's agents use over MCP.
@MainActor
@Observable
final class Agent {
    struct Model: Identifiable, Hashable { let id: String; let name: String }
    static let models = [
        Model(id: "claude-sonnet-5-5", name: "Claude Sonnet 5.5"),
        Model(id: "claude-opus-5-5", name: "Claude Opus 5.5"),
        Model(id: "claude-haiku-5-5", name: "Claude Haiku 5.5"),
    ]

    enum Item: Identifiable {
        case user(UUID, String)
        case assistant(UUID, String)
        case tool(UUID, title: String, ok: Bool)
        case error(UUID, String)
        var id: UUID {
            switch self {
            case .user(let i, _), .assistant(let i, _), .tool(let i, _, _), .error(let i, _): return i
            }
        }
    }

    private(set) var items: [Item] = []
    private(set) var busy = false
    var model: String = UserDefaults.standard.string(forKey: "agent.model") ?? Agent.models[0].id {
        didSet { UserDefaults.standard.set(model, forKey: "agent.model") }
    }
    var hasKey: Bool { !(Keychain.apiKey ?? "").isEmpty }

    @ObservationIgnored private var messages: [[String: Any]] = []
    @ObservationIgnored private var task: Task<Void, Never>?
    @ObservationIgnored private lazy var tools: [[String: Any]] = {
        let defs = (try? JSONSerialization.jsonObject(with: Data(Core.shared.engine.toolDefinitions().utf8))) as? [[String: Any]] ?? []
        return defs.map { ["name": $0["name"] ?? "", "description": $0["description"] ?? "", "input_schema": $0["inputSchema"] ?? ["type": "object"]] }
    }()
    @ObservationIgnored private lazy var titles: [String: String] = {
        let defs = (try? JSONSerialization.jsonObject(with: Data(Core.shared.engine.toolDefinitions().utf8))) as? [[String: Any]] ?? []
        return Dictionary(defs.compactMap { d in (d["name"] as? String).map { ($0, (d["title"] as? String) ?? $0) } }, uniquingKeysWith: { a, _ in a })
    }()

    func clear() {
        stop()
        items = []
        messages = []
    }

    func stop() {
        task?.cancel()
        task = nil
        busy = false
    }

    /// Send a message; `editor` is told after every edit so the preview keeps up.
    func send(_ text: String, editor: EditorModel) {
        let text = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty, !busy else { return }
        items.append(.user(UUID(), text))
        messages.append(["role": "user", "content": text])
        busy = true
        task = Task { [weak self] in
            await self?.run(editor: editor)
            self?.busy = false
        }
    }

    private func run(editor: EditorModel) async {
        let system = Core.shared.engine.instructions() + """

        You're inside Liemo on the person's \(UIDeviceName.current). The project open in the editor has id \(editor.id) \
        ("\(editor.doc?.name ?? "")", \(editor.doc?.aspect ?? "")); work on it unless they say otherwise. The playhead is at \
        \(String(format: "%.1f", editor.time)) s. Keep replies short; describe what you changed in plain words.
        """
        for _ in 0..<40 {
            if Task.isCancelled { return }
            let reply: [String: Any]
            do {
                reply = try await request(system: system)
            } catch {
                if !Task.isCancelled { items.append(.error(UUID(), error.localizedDescription)) }
                return
            }
            let content = reply["content"] as? [[String: Any]] ?? []
            messages.append(["role": "assistant", "content": content])
            var results: [[String: Any]] = []
            for block in content {
                switch block["type"] as? String {
                case "text":
                    if let t = block["text"] as? String, !t.isEmpty { items.append(.assistant(UUID(), t)) }
                case "tool_use":
                    let name = block["name"] as? String ?? ""
                    var input = block["input"] as? [String: Any] ?? [:]
                    if input["project"] == nil { input["project"] = editor.id }
                    let r = await Task.detached { Core.shared.call(name, input) }.value
                    items.append(.tool(UUID(), title: titles[name] ?? name, ok: r.ok))
                    var out: [[String: Any]] = [["type": "text", "text": r.text]]
                    if let png = r.png {
                        out.append(["type": "image", "source": ["type": "base64", "media_type": "image/png", "data": png.base64EncodedString()]])
                    }
                    results.append(["type": "tool_result", "tool_use_id": block["id"] ?? "", "content": out, "is_error": !r.ok])
                    editor.reload()
                default: break
                }
            }
            if results.isEmpty || reply["stop_reason"] as? String != "tool_use" { return }
            messages.append(["role": "user", "content": results])
        }
    }

    private func request(system: String) async throws -> [String: Any] {
        guard let key = Keychain.apiKey, !key.isEmpty else { throw CoreError(message: "Add your Anthropic API key in Liemo AI settings.") }
        var req = URLRequest(url: URL(string: "https://api.anthropic.com/v1/messages")!)
        req.httpMethod = "POST"
        req.setValue(key, forHTTPHeaderField: "x-api-key")
        req.setValue("2023-06-01", forHTTPHeaderField: "anthropic-version")
        req.setValue("application/json", forHTTPHeaderField: "content-type")
        req.timeoutInterval = 300
        req.httpBody = try JSONSerialization.data(withJSONObject: [
            "model": model, "max_tokens": 4096, "system": system, "tools": tools, "messages": messages,
        ])
        let (data, resp) = try await URLSession.shared.data(for: req)
        let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] ?? [:]
        guard (resp as? HTTPURLResponse)?.statusCode == 200 else {
            let msg = (json["error"] as? [String: Any])?["message"] as? String ?? "The request failed (\((resp as? HTTPURLResponse)?.statusCode ?? 0))."
            throw CoreError(message: msg)
        }
        return json
    }
}

import UIKit
enum UIDeviceName {
    @MainActor static var current: String { UIDevice.current.userInterfaceIdiom == .pad ? "iPad" : "iPhone" }
}
