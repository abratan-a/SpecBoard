import Foundation
import os

/// The Rust core (`sources/core`, `specboard://SpecBoard/Docs/Design:CoreApi`), opened on the projects' store: a request
/// in JSON, its answer in JSON, and notices of what changed.
///
/// The store is the core's own form (`specboard-core.db`). The store of the versions before the
/// core (`specboard.db`) is neither read nor moved.
nonisolated final class CoreClient: @unchecked Sendable {
    /// A request the core refused, said as the core says it.
    struct Refusal: LocalizedError {
        let errorDescription: String?
        init(_ message: String) { errorDescription = message }

        /// For an answer that is not the core's JSON: the start of what it wrote is in the message.
        static func unreadable(_ reply: String) -> Refusal {
            Refusal(L10n.text(.Common.problemCoreUnreadableSaid(String(reply.prefix(200))), in: Language.current))
        }
    }

    /// The core this asks: the one this app holds over the store on this Mac, or an agent's on a
    /// host (`AgentCore`), asked the same.
    private enum Way {
        case here(Core)
        case agent(AgentCore)
    }

    private let way: Way
    /// Where its projects are kept: nil for this Mac, else the host's (`RemoteHosts.Host.id`).
    let place: UUID?
    /// This Mac as the core knows the window it writes from (a hold's `client`): empty for the
    /// store this app holds, the Mac's name for an agent's, which the agent says with each request.
    let client: String

    /// The store at `url` opened; a new, empty one made there when there is none.
    init(at url: URL) throws {
        way = .here(FileManager.default.fileExists(atPath: url.path)
            ? try Core.open(place: url.path) : try Core.create(place: url.path))
        place = nil
        client = ""
    }

    /// The core of the agent on the host `place`, reached through `agent` (started, as `client`).
    init(agent: AgentCore, place: UUID, client: String) {
        way = .agent(agent)
        self.place = place
        self.client = client
    }

    var isAgent: Bool { place != nil }

    /// Each request said in the app's log as it is answered: its name (`asked`), how long the answer took and
    /// how big it was — read with `log show --predicate 'subsystem == "com.m-osone.SpecBoard"'` (or `log stream`).
    static let log = Logger(subsystem: "com.m-osone.SpecBoard", category: "core")

    /// The requests asked since `keepAsked`, by name (`asked`): nil while none are kept.
    private let kept = OSAllocatedUnfairLock<[String]?>(initialState: nil)
    /// The answers given since `keepAsked`, by the request's name, with their size in bytes: nil while none
    /// are kept.
    private let keptAnswers = OSAllocatedUnfairLock<[Answered]?>(initialState: nil)

    /// An answer as `takeAnswers` gives it: the request's name and how big the answer was.
    struct Answered: Equatable, Sendable {
        var name: String
        var bytes: Int
    }

    /// The last reading of each tree read (`readDetail`), with its hashes: what the next is asked from.
    let held = HeldDetails()
    /// The last reading of each project's working directories (`workdirDetail`): what the next is asked from.
    let workdirs = HeldWorkdirs()

    /// From now on, each request's name is kept, to be taken with `takeAsked`, and each answer's size, to be
    /// taken with `takeAnswers`: what a test counts.
    func keepAsked() {
        kept.withLock { $0 = [] }
        keptAnswers.withLock { $0 = [] }
    }

    /// The names of the requests asked since `keepAsked` or the last take, in the order asked.
    func takeAsked() -> [String] {
        kept.withLock { list in
            defer { if list != nil { list = [] } }
            return list ?? []
        }
    }

    /// The answers given since `keepAsked` or the last take, in the order given.
    func takeAnswers() -> [Answered] {
        keptAnswers.withLock { list in
            defer { if list != nil { list = [] } }
            return list ?? []
        }
    }

    /// A request as the log and `takeAsked` name it: its op, and for `tool` the tool's name after it
    /// (`tool card_read`). Nothing else of what it asks is said: a project's or a card's name stays out of the log.
    static func asked(_ op: String, tool: String?) -> String {
        guard op == "tool", let tool else { return op }
        return "tool \(tool)"
    }

    /// A request's text, and the core's answer as it wrote it. `name`: what the log says it was (`asked`).
    private func call(_ name: String, _ request: String) throws -> String {
        kept.withLock { $0?.append(name) }
        let started = ContinuousClock.now
        let answer: String
        switch way {
        case .here(let core): answer = core.call(request: request)
        case .agent(let agent): answer = try agent.call(request)
        }
        let took = ContinuousClock.now - started
        let seconds = Double(took.components.seconds) + Double(took.components.attoseconds) / 1e18
        let place = isAgent ? "agent" : "here"
        keptAnswers.withLock { $0?.append(Answered(name: name, bytes: answer.utf8.count)) }
        Self.log.notice("\(name, privacy: .public) \(seconds, format: .fixed(precision: 3), privacy: .public)s \(answer.utf8.count, privacy: .public)B \(place, privacy: .public)")
        return answer
    }

    /// Where the Swift app kept its store: Application Support in the user's own Library
    /// (the app is not sandboxed, so the ordinary folder). The core's sits beside it.
    static func swiftStoreURL() throws -> URL {
        let base = try FileManager.default.url(for: .applicationSupportDirectory, in: .userDomainMask,
                                               appropriateFor: nil, create: true)
        let dir = base.appendingPathComponent("SpecBoard", isDirectory: true)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir.appendingPathComponent("specboard.db")
    }

    /// The core's store beside the Swift app's: the same folder, its own file, named after
    /// it (`specboard.db` → `specboard-core.db`).
    static func storeURL(beside swift: URL) -> URL {
        swift.deletingLastPathComponent()
            .appendingPathComponent(swift.deletingPathExtension().lastPathComponent + "-core.db")
    }

    /// A request: `op` and its fields. Returns what the core answered (`result`), or throws
    /// its refusal (`error`). Every request carries the screen's language (`lang`): what the core
    /// says is said in it.
    func request(_ op: String, _ fields: [String: Any] = [:]) throws -> Any {
        var body = fields
        body["op"] = op
        body["lang"] = Language.current
        let sent = try JSONSerialization.data(withJSONObject: body)
        let reply = try call(Self.asked(op, tool: fields["name"] as? String), String(decoding: sent, as: UTF8.self))
        guard let answer = try JSONSerialization.jsonObject(with: Data(reply.utf8)) as? [String: Any] else {
            throw Refusal.unreadable(reply)
        }
        if let error = answer["error"] as? String { throw Refusal(error) }
        return answer["result"] ?? NSNull()
    }

    /// A request whose answer is kept in the order the core wrote it: for what is shown as
    /// written (a document's value, a tool's answer).
    @MainActor
    func ordered(_ op: String, _ fields: [String: Any] = [:]) throws -> OJSON {
        try ordered(reading: try reply(op, fields))
    }

    /// A request's answer as the core wrote it, not yet read: asked off the main thread for what
    /// takes a while — a big project of an agent's, read as it is opened — and read
    /// on it (`ordered(reading:)`).
    nonisolated func reply(_ op: String, _ fields: [String: Any] = [:]) throws -> String {
        var body = fields
        body["op"] = op
        body["lang"] = Language.current
        let sent = try JSONSerialization.data(withJSONObject: body)
        return try call(Self.asked(op, tool: fields["name"] as? String), String(decoding: sent, as: UTF8.self))
    }

    /// An answer as the core wrote it (`reply`), read in the order written: its result, or its refusal
    /// thrown.
    @MainActor
    func ordered(reading reply: String) throws -> OJSON {
        guard case .object(let answer)? = OJSON.parse(reply) else {
            throw Refusal.unreadable(reply)
        }
        if case .string(let error)? = answer["error"] { throw Refusal(error) }
        return answer["result"] ?? .null
    }

    /// A request whose fields go as they were written, in their order, and whose answer
    /// comes back the same: for what is handed on as it came (an MCP message — its
    /// arguments' order is the caller's).
    func ordered(_ op: String, _ fields: OrderedObject) throws -> OJSON {
        var body = OrderedObject([("op", .string(op))])
        for (key, value) in fields.entries { body[key] = value }
        body["lang"] = .string(Language.current)
        var tool: String?
        if case .string(let name)? = fields["name"] { tool = name }
        let reply = try call(Self.asked(op, tool: tool), OJSON.object(body).compact())
        guard case .object(let answer)? = OJSON.parse(reply) else {
            throw Refusal.unreadable(reply)
        }
        if case .string(let error)? = answer["error"] { throw Refusal(error) }
        return answer["result"] ?? .null
    }

    /// What answers `app_control` — the window read and moved, for MCP —, draws a map for
    /// `as: picture`, and asks a person whether MCP may do what it asked to, handed to the core: given
    /// the call's arguments and who made it (both JSON), it answers in JSON (`specboard_core::App`).
    /// Called on the thread the MCP request is answered on.
    func answerAppControl(_ answer: CoreApp) {
        switch way {
        case .here(let core):
            core.setApp(app: answer)
        case .agent(let agent):
            // Handed back by the agent for a window of this app's: answered off its reading
            // thread (the answer waits on main), and said back.
            agent.onApp = { [weak agent] id, arguments, caller in
                DispatchQueue.global(qos: .userInitiated).async {
                    agent?.answerApp(id, answer.appControl(arguments: arguments, caller: caller))
                }
            }
            // A map drawn for `as: picture`, off the reading thread as well (it is drawn on main).
            agent.onPicture = { [weak agent] id, request in
                DispatchQueue.global(qos: .userInitiated).async {
                    agent?.answerPicture(id, answer.picture(request: request))
                }
            }
            // Asked on every Mac connected to the agent: taken away here when answered on another.
            let host = place?.uuidString ?? ""
            agent.onAsk = { [weak agent] id, request in
                DispatchQueue.global(qos: .userInitiated).async {
                    agent?.answerAsk(id, AskCenter.ask(request, tag: "\(host):\(id)"))
                }
            }
            agent.onAsked = { id in
                Task { @MainActor in AskCenter.shared.withdraw(tag: "\(host):\(id)") }
            }
        }
    }

    /// What this app's window shows of the agent's projects, told to it (`AgentCore.tellWindows`);
    /// nothing for this Mac's core.
    func tellWindows(_ windows: [[String: Any]]) {
        if case .agent(let agent) = way { agent.tellWindows(windows) }
    }

    /// A request whose answer is read as `T`.
    func request<T: Decodable>(_ type: T.Type, _ op: String, _ fields: [String: Any] = [:]) throws -> T {
        let result = try request(op, fields)
        let data = try JSONSerialization.data(withJSONObject: result, options: [.fragmentsAllowed])
        return try JSONDecoder().decode(T.self, from: data)
    }

    /// Told, as each write commits, which tree of which project changed
    /// (`{"op": "changed", "project", "branch", "rev"}`): this Mac's, on the thread the write ran on;
    /// an agent's, on the thread its lines are read on, all but what this app's own requests
    /// changed (it reads those again itself, as it does what it writes on the main thread here).
    func listen(_ heard: @escaping @Sendable (String) -> Void) {
        switch way {
        case .here(let core):
            core.listen(listener: Listener(heard))
        case .agent(let agent):
            agent.onChanged = { notice, own in if !own { heard(notice) } }
        }
    }

    func close() {
        switch way {
        case .here(let core): core.close()
        case .agent(let agent): agent.stop()
        }
    }

    private final class Listener: CoreListener, @unchecked Sendable {
        let heard: @Sendable (String) -> Void
        init(_ heard: @escaping @Sendable (String) -> Void) { self.heard = heard }
        func changed(notice: String) { heard(notice) }
    }
}
