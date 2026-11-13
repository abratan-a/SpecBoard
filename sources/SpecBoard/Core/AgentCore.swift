import Foundation

/// The core an agent keeps on a host, reached over SSH: `specboard-agent attach` started there,
/// the daemon's lines (`sources/core/agent/src/daemon.rs`) on its standard input and output. A
/// request goes as the core's own (`CoreClient`) and is waited for; what changed is told as it
/// comes, on the thread that reads the lines. Started over SSH (`AgentDeploy.attach`) or, in
/// tests, as a process on this Mac.
nonisolated final class AgentCore: @unchecked Sendable {

    private let process = Process()
    private let input = Pipe(), output = Pipe(), errors = Pipe()
    /// Lines to the daemon go out one at a time.
    private let writing = NSLock()
    /// Guards what follows: the requests waited for and their answers, and whether it ended.
    private let guarding = NSLock()
    private var next = 1
    private var waiting: [Int: DispatchSemaphore] = [:]
    private var answers: [Int: String] = [:]
    /// What the daemon said to the lines about the hub asked by number (`askHub`).
    private var hubAnswers: [Int: [String: Any]] = [:]
    private var ended = false
    /// What `attach` said on its standard error, read as it comes — a pipe not read fills (64 KB)
    /// and stops the writer, and with it the way — and only its end kept (`saidLimit`).
    private var said = Data()
    private let saidDone = DispatchSemaphore(value: 0)
    static let saidLimit = 16 << 10

    /// `said` with `chunk` after it, cut to its last `saidLimit` bytes.
    static func kept(_ said: Data, adding chunk: Data) -> Data {
        var said = said
        said.append(chunk)
        // Copied: cutting the front off `Data` keeps all it held behind it.
        return said.count > saidLimit ? Data(said.suffix(saidLimit)) : said
    }
    /// What the daemon says when it takes this app: itself, or why it cannot.
    private var greeting: Result<[String: Any], AgentDeploy.Failure>?
    private let greeted = DispatchSemaphore(value: 0)

    /// The daemon that took this app: its number, the file it runs from, the app's build.
    private(set) var daemon: [String: Any] = [:]

    /// What changed in the daemon's core (the notice's text), and whether this app's own
    /// request changed it. Set before `start`.
    var onChanged: (@Sendable (String, Bool) -> Void)?
    /// Where the daemon listens for MCP (`{port, listening, problem}`), as it says when it changes,
    /// and why a port asked for was refused (`movePort`).
    var onMCP: (@Sendable ([String: Any], String?) -> Void)?
    /// Where the daemon's link to the hub it joined stands (`{joined, id?, to?, link, members}`), as it
    /// says when that changes or when asked (`tellHub`).
    var onHub: (@Sendable ([String: Any]) -> Void)?
    /// `app_control` the daemon hands to this app for one of its windows: the call's number, its
    /// arguments and caller (both JSON), answered with `answerApp`.
    var onApp: (@Sendable (Int, String, String) -> Void)?
    /// A map the daemon asks this app to draw for `as: picture`: the call's number and the request
    /// (JSON), answered with `answerPicture`. Asked of this app whatever window it shows.
    var onPicture: (@Sendable (Int, String) -> Void)?
    /// A person asked whether MCP may do what it asked to (`AskCenter`): the question's number and the
    /// core's request, answered with `answerAsk`; and the question taken away (answered on another Mac,
    /// or no answer in time).
    var onAsk: (@Sendable (Int, String) -> Void)?
    var onAsked: (@Sendable (Int) -> Void)?
    /// Called once, when the way ends — the SSH session closed, the host went, the daemon
    /// stopped — with what was said on its standard error.
    var onEnd: (@Sendable (String) -> Void)?

    var isRunning: Bool { process.isRunning && !hasEnded }

    private var hasEnded: Bool {
        guarding.lock()
        defer { guarding.unlock() }
        return ended
    }

    /// Starts `attach` and waits for the daemon to take this app, named `client` (the Mac's name),
    /// or to say why it cannot.
    func start(_ launch: AgentDeploy.Launch, client: String, timeout: TimeInterval = 30) throws {
        process.executableURL = URL(fileURLWithPath: launch.executable)
        process.arguments = launch.arguments
        if !launch.environment.isEmpty {
            process.environment = ProcessInfo.processInfo.environment.merging(launch.environment) { _, added in added }
        }
        process.standardInput = input
        process.standardOutput = output
        process.standardError = errors
        process.terminationHandler = { [weak self] _ in self?.finish() }
        // A line written after the way went must fail the write, not take the app down.
        _ = fcntl(input.fileHandleForWriting.fileDescriptor, F_SETNOSIGPIPE, 1)
        try process.run()
        Thread.detachNewThread { [weak self] in self?.readLines() }
        Thread.detachNewThread { [weak self] in self?.readErrors() }
        send(["t": "hello", "client": client])
        if greeted.wait(timeout: .now() + timeout) == .timedOut {
            stop()
            throw AgentDeploy.Failure(L10n.text(.Model.agentNoGreeting(Int(timeout)), in: Language.current))
        }
        switch greeting {
        case .success(let said)?:
            daemon = said
        case .failure(let failure)?:
            stop()
            throw failure
        case nil:
            stop()
            let said = errorsSaid()
            throw AgentDeploy.Failure(said.isEmpty ? L10n.text(.Model.agentDroppedEarly, in: Language.current) : said)
        }
    }

    /// A request of the core's (its JSON's text), and the core's answer as it wrote it. Throws when
    /// the way is gone, or no answer comes in `timeout`.
    func call(_ request: String, timeout: TimeInterval = 120) throws -> String {
        let done = DispatchSemaphore(value: 0)
        guarding.lock()
        if ended {
            guarding.unlock()
            throw CoreClient.Refusal(L10n.text(.Model.agentLostNow, in: Language.current))
        }
        let id = next
        next += 1
        waiting[id] = done
        guarding.unlock()
        send(["t": "core", "id": id, "request": request])
        let timedOut = done.wait(timeout: .now() + timeout) == .timedOut
        guarding.lock()
        waiting[id] = nil
        let answer = answers.removeValue(forKey: id)
        guarding.unlock()
        if let answer { return answer }
        throw CoreClient.Refusal(timedOut ? L10n.text(.Model.agentNoAnswer(Int(timeout)), in: Language.current)
                                          : L10n.text(.Model.agentLostWhileWaiting, in: Language.current))
    }

    /// The answer to `app_control` call `id` (`onApp`).
    func answerApp(_ id: Int, _ answer: String) {
        send(["t": "app", "id": id, "answer": answer])
    }

    /// The answer to picture `id` (`onPicture`).
    func answerPicture(_ id: Int, _ answer: String) {
        send(["t": "picture", "id": id, "answer": answer])
    }

    /// The answer to question `id` (`onAsk`).
    func answerAsk(_ id: Int, _ answer: String) {
        send(["t": "ask", "id": id, "answer": answer])
    }

    /// This app's windows that show the daemon's projects (`{window, project, branch, section, card,
    /// front}`, the project by its id): what `app_control` can be aimed at.
    func tellWindows(_ windows: [[String: Any]]) {
        send(["t": "windows", "windows": windows])
    }

    /// The daemon asked to listen for MCP at `port` from now on (its user's setting); what it says
    /// comes to `onMCP`.
    func movePort(_ port: Int) {
        send(["t": "mcp_port", "port": port])
    }

    /// The daemon asked where its link to the hub stands; it comes to `onHub`.
    func tellHub() {
        send(["t": "hub"])
    }

    /// Where the daemon's link to the hub stands, asked now and waited for.
    func hubSaid(timeout: TimeInterval = 10) throws -> [String: Any] {
        try askHub(["t": "hub"], timeout: timeout)
    }

    /// The agent's id and public key in the hub, made when it has none yet: the private key stays on
    /// its host.
    func hubKey(timeout: TimeInterval = 30) throws -> (id: String, key: String) {
        let said = try askHub(["t": "hub_key"], timeout: timeout)
        guard let id = said["id"] as? String, let key = said["key"] as? String else {
            throw CoreClient.Refusal.unreadable(String(describing: said))
        }
        return (id, key)
    }

    /// The agent joined to the hub: where the hub is, as the agent's own host reaches it, and the
    /// hub's host's public host keys — the one host its link will take for the hub.
    func hubJoin(to: String, hostKeys: [String], timeout: TimeInterval = 30) throws {
        _ = try askHub(["t": "hub_join", "to": to, "hostKeys": hostKeys], timeout: timeout)
    }

    /// The agent out of the hub: its link let go, and nothing of the hub kept.
    func hubLeave(timeout: TimeInterval = 30) throws {
        _ = try askHub(["t": "hub_leave"], timeout: timeout)
    }

    /// A line about the hub asked by number, and the daemon's answer to it (told apart, by the
    /// number, from what it says of the link on its own); its refusal is thrown as it said it.
    private func askHub(_ message: [String: Any], timeout: TimeInterval) throws -> [String: Any] {
        let done = DispatchSemaphore(value: 0)
        guarding.lock()
        if ended {
            guarding.unlock()
            throw CoreClient.Refusal(L10n.text(.Model.agentLostNow, in: Language.current))
        }
        let id = next
        next += 1
        waiting[id] = done
        guarding.unlock()
        var message = message
        message["id"] = id
        send(message)
        let timedOut = done.wait(timeout: .now() + timeout) == .timedOut
        guarding.lock()
        waiting[id] = nil
        let answer = hubAnswers.removeValue(forKey: id)
        guarding.unlock()
        guard let answer else {
            throw CoreClient.Refusal(timedOut ? L10n.text(.Model.agentNoAnswer(Int(timeout)), in: Language.current)
                                              : L10n.text(.Model.agentLostWhileWaiting, in: Language.current))
        }
        if let refused = answer["refused"] as? String { throw CoreClient.Refusal(refused) }
        return answer
    }

    /// Lets the way go: its input closes, and `attach` on the host ends (the daemon stays).
    func stop() {
        try? input.fileHandleForWriting.close()
        if process.isRunning { process.terminate() }
    }

    // MARK: - Lines

    /// A line as it goes to the daemon: with the screen's language (`lang`), so what the daemon says of its
    /// own (where MCP listens, why it cannot) and the core's answers are in it.
    nonisolated static func framed(_ message: [String: Any], language: String = Language.current) -> [String: Any] {
        var message = message
        message["lang"] = language
        return message
    }

    private func send(_ message: [String: Any]) {
        guard let data = try? JSONSerialization.data(withJSONObject: Self.framed(message), options: [.withoutEscapingSlashes])
        else { return }
        writing.lock()
        defer { writing.unlock() }
        try? input.fileHandleForWriting.write(contentsOf: data + Data([0x0A]))
    }

    /// The daemon's lines read as they come, for as long as the way lasts — each read and what is read
    /// from it let go of as it is done. This thread never ends while connected, and what Foundation
    /// hands back for later (the data read, the JSON made of it) went into its one pool, let go of only
    /// at its end: each project read again after an AI's write — megabytes for a big one — stayed, and
    /// the app grew by that much a write.
    private func readLines() {
        let handle = output.fileHandleForReading
        var buffer = Data()
        while Self.readTurn(handle, &buffer, receive) {}
        finish()
    }

    /// One read: what came, split into lines, each handed to `receive`; false once the way has ended.
    /// What it made for later is let go of before it returns. Only what came is looked through for a
    /// line's end: a line of megabytes comes in many reads.
    static func readTurn(_ handle: FileHandle, _ buffer: inout Data, _ receive: (Data) -> Void) -> Bool {
        autoreleasepool {
            let chunk = handle.availableData
            if chunk.isEmpty { return false }
            var from = buffer.endIndex
            buffer.append(chunk)
            while let newline = buffer[from...].firstIndex(of: 0x0A) {
                let line = Data(buffer[buffer.startIndex..<newline])
                buffer.removeSubrange(buffer.startIndex...newline)
                from = buffer.startIndex
                receive(line)
            }
            return true
        }
    }

    private func receive(_ line: Data) {
        guard let message = (try? JSONSerialization.jsonObject(with: line)) as? [String: Any] else { return }
        switch message["t"] as? String {
        case "ready":
            greeting = .success(message)
            greeted.signal()
        case "failed":
            greeting = .failure(AgentDeploy.Failure(message["message"] as? String ?? L10n.text(.Model.agentCantConnect, in: Language.current)))
            greeted.signal()
        case "core":
            guard let id = message["id"] as? Int else { return }
            guarding.lock()
            let done = waiting[id]
            if done != nil { answers[id] = message["answer"] as? String ?? "" }
            guarding.unlock()
            done?.signal()
        case "changed":
            guard let notice = message["notice"] as? String else { return }
            onChanged?(notice, message["own"] as? Bool ?? false)
        case "mcp":
            onMCP?(message["mcp"] as? [String: Any] ?? [:], message["refused"] as? String)
        case "hub", "hub_key":
            // The answer to a line asked by number is the asker's; what the daemon says of the link
            // on its own — and the state an answer carries — is everyone's.
            if let id = message["re"] as? Int {
                guarding.lock()
                let done = waiting[id]
                if done != nil { hubAnswers[id] = message }
                guarding.unlock()
                done?.signal()
            }
            if message["t"] as? String == "hub" { onHub?(message) }
        case "app":
            guard let id = message["id"] as? Int else { return }
            onApp?(id, message["arguments"] as? String ?? "{}", message["caller"] as? String ?? "null")
        case "picture":
            guard let id = message["id"] as? Int else { return }
            onPicture?(id, message["request"] as? String ?? "{}")
        case "ask":
            guard let id = message["id"] as? Int else { return }
            onAsk?(id, message["request"] as? String ?? "{}")
        case "asked":
            guard let id = message["id"] as? Int else { return }
            onAsked?(id)
        default:
            break
        }
    }

    private func finish() {
        guarding.lock()
        let first = !ended
        ended = true
        let left = Array(waiting.values)
        guarding.unlock()
        guard first else { return }
        // Whatever was waited for has no answer coming.
        for done in left { done.signal() }
        if greeting == nil { greeted.signal() }
        onEnd?(errorsSaid())
    }

    /// Standard error read until `attach` closes it, its end kept.
    private func readErrors() {
        let handle = errors.fileHandleForReading
        while true {
            let chunk = autoreleasepool { handle.availableData }
            if chunk.isEmpty { break }
            guarding.lock()
            said = Self.kept(said, adding: chunk)
            guarding.unlock()
        }
        saidDone.signal()
    }

    /// What `attach` said on its standard error: all of it once it has closed, waited for a moment
    /// (the process has ended by now, or been told to).
    private func errorsSaid() -> String {
        if saidDone.wait(timeout: .now() + 1) == .success { saidDone.signal() }
        guarding.lock()
        defer { guarding.unlock() }
        return String(decoding: said, as: UTF8.self).trimmingCharacters(in: .whitespacesAndNewlines)
    }
}
