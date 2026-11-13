import Foundation

/// The way to a hub, reached over SSH: `specboard-agent hub attach` started there as the hub's user,
/// the hub's lines (`sources/core/agent/src/hub/serve.rs`) on its standard input and output. It says
/// who the hub's members are as that changes, and adds, renames and removes them — what only the
/// hub's user may do. Started over SSH (`AgentDeploy.hubAttach`) or, in tests, as a process on this
/// Mac.
nonisolated final class HubCore: @unchecked Sendable {

    private let process = Process()
    private let input = Pipe(), output = Pipe(), errors = Pipe()
    /// Lines to the hub go out one at a time.
    private let writing = NSLock()
    /// One line is asked and answered at a time.
    private let asking = NSLock()
    /// Guards what follows.
    private let guarding = NSLock()
    private var ended = false
    private var kept: [RemoteHosts.HubMember] = []
    /// What the one asking now waits for.
    private var asked: Asked?
    private var said = Data()
    private let saidDone = DispatchSemaphore(value: 0)
    /// What the hub says when it takes this app: itself, or why it cannot.
    private var greeting: Result<[String: Any], AgentDeploy.Failure>?
    private let greeted = DispatchSemaphore(value: 0)

    /// The hub that took this app: the form of its lines, the app's build it runs.
    private(set) var hub: [String: Any] = [:]

    /// The hub's members, as it said them last.
    var members: [RemoteHosts.HubMember] {
        guarding.lock()
        defer { guarding.unlock() }
        return kept
    }

    /// The hub's members, each time it says them. Set before `start`.
    var onMembers: (@Sendable ([RemoteHosts.HubMember]) -> Void)?
    /// Called once, when the way ends, with what was said on its standard error.
    var onEnd: (@Sendable (String) -> Void)?

    /// Starts `hub attach` and waits for the hub to take this app, or to say why it cannot.
    func start(_ launch: AgentDeploy.Launch, timeout: TimeInterval = 30) throws {
        process.executableURL = URL(fileURLWithPath: launch.executable)
        process.arguments = launch.arguments
        process.standardInput = input
        process.standardOutput = output
        process.standardError = errors
        process.terminationHandler = { [weak self] _ in self?.finish() }
        // A line written after the way went must fail the write, not take the app down.
        _ = fcntl(input.fileHandleForWriting.fileDescriptor, F_SETNOSIGPIPE, 1)
        try process.run()
        Thread.detachNewThread { [weak self] in self?.readLines() }
        Thread.detachNewThread { [weak self] in self?.readErrors() }
        send(["t": "hello"])
        if greeted.wait(timeout: .now() + timeout) == .timedOut {
            stop()
            throw AgentDeploy.Failure(L10n.text(.Model.agentNoGreeting(Int(timeout)), in: Language.current))
        }
        switch greeting {
        case .success(let said)?:
            hub = said
        case .failure(let failure)?:
            stop()
            throw failure
        case nil:
            stop()
            let said = errorsSaid()
            throw AgentDeploy.Failure(said.isEmpty ? L10n.text(.Model.agentDroppedEarly, in: Language.current) : said)
        }
    }

    /// A member added — the id and the public key its agent made, the name it goes by — and the
    /// hub's host's public host keys, for that agent to know the hub's host by.
    func enroll(id: String, name: String, key: String) throws -> [String] {
        try ask(["t": "enroll", "id": id, "name": name, "key": key], for: "enrolled")["hostKeys"] as? [String] ?? []
    }

    /// A member gone from the hub: its key's line too, and its connection closed.
    func remove(id: String) throws {
        _ = try ask(["t": "remove", "id": id], for: "removed")
    }

    func rename(id: String, to name: String) throws {
        _ = try ask(["t": "rename", "id": id, "name": name], for: "renamed")
    }

    /// Lets the way go: its input closes, and `hub attach` on the host ends (the hub stays).
    func stop() {
        try? input.fileHandleForWriting.close()
        if process.isRunning { process.terminate() }
    }

    // MARK: - Lines

    /// One line asked: the kinds of line that answer it, the line that came, and what wakes the one
    /// waiting. Its own, so that an answer that comes as the wait gives up wakes nobody asked later.
    nonisolated private final class Asked {
        let kinds: Set<String>
        var answer: [String: Any]?
        let done = DispatchSemaphore(value: 0)
        init(_ kinds: Set<String>) { self.kinds = kinds }
    }

    /// One line asked, and its answer of the kind `kind` waited for; the hub's refusal is thrown as
    /// it said it.
    private func ask(_ message: [String: Any], for kind: String, timeout: TimeInterval = 30) throws -> [String: Any] {
        asking.lock()
        defer { asking.unlock() }
        guarding.lock()
        if ended {
            guarding.unlock()
            throw CoreClient.Refusal(L10n.text(.Model.agentLostNow, in: Language.current))
        }
        let asked = Asked([kind, "refused"])
        self.asked = asked
        guarding.unlock()
        send(message)
        let timedOut = asked.done.wait(timeout: .now() + timeout) == .timedOut
        guarding.lock()
        if self.asked === asked { self.asked = nil }
        let line = asked.answer
        guarding.unlock()
        guard let line else {
            throw CoreClient.Refusal(timedOut ? L10n.text(.Model.agentNoAnswer(Int(timeout)), in: Language.current)
                                              : L10n.text(.Model.agentLostWhileWaiting, in: Language.current))
        }
        if line["t"] as? String == "refused" {
            throw CoreClient.Refusal(line["message"] as? String ?? "")
        }
        return line
    }

    private func send(_ message: [String: Any]) {
        guard let data = try? JSONSerialization.data(withJSONObject: AgentCore.framed(message), options: [.withoutEscapingSlashes])
        else { return }
        writing.lock()
        defer { writing.unlock() }
        try? input.fileHandleForWriting.write(contentsOf: data + Data([0x0A]))
    }

    private func readLines() {
        let handle = output.fileHandleForReading
        var buffer = Data()
        while AgentCore.readTurn(handle, &buffer, receive) {}
        finish()
    }

    private func receive(_ line: Data) {
        guard let message = (try? JSONSerialization.jsonObject(with: line)) as? [String: Any],
              let kind = message["t"] as? String else { return }
        // Who is there, said with the hub's greeting, with what it answers and on its own.
        if let told = message["members"] as? [[String: Any]] {
            let members = RemoteHosts.HubMember.list(told)
            guarding.lock()
            kept = members
            guarding.unlock()
            onMembers?(members)
        }
        switch kind {
        case "ready":
            greeting = .success(message)
            greeted.signal()
        case "failed":
            greeting = .failure(AgentDeploy.Failure(message["message"] as? String ?? L10n.text(.Model.agentCantConnect, in: Language.current)))
            greeted.signal()
        default:
            guarding.lock()
            let asked = self.asked.flatMap { $0.kinds.contains(kind) ? $0 : nil }
            if let asked {
                asked.answer = message
                self.asked = nil
            }
            guarding.unlock()
            asked?.done.signal()
        }
    }

    private func finish() {
        guarding.lock()
        let first = !ended
        ended = true
        let waiting = asked
        asked = nil
        guarding.unlock()
        guard first else { return }
        // Whatever was waited for has no answer coming.
        waiting?.done.signal()
        if greeting == nil { greeted.signal() }
        onEnd?(errorsSaid())
    }

    private func readErrors() {
        let handle = errors.fileHandleForReading
        while true {
            let chunk = autoreleasepool { handle.availableData }
            if chunk.isEmpty { break }
            guarding.lock()
            said = AgentCore.kept(said, adding: chunk)
            guarding.unlock()
        }
        saidDone.signal()
    }

    private func errorsSaid() -> String {
        if saidDone.wait(timeout: .now() + 1) == .success { saidDone.signal() }
        guarding.lock()
        defer { guarding.unlock() }
        return String(decoding: said, as: UTF8.self).trimmingCharacters(in: .whitespacesAndNewlines)
    }
}
