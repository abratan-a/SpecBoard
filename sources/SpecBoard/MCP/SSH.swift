import CryptoKit
import Foundation

/// Talking to a host over one long-lived SSH connection.
///
/// Everything rides a single multiplexed master: the agent that lets the sessions on the
/// host reach this app, and the polling that reports whether any of it is working.
/// Measured against a real host, that makes a liveness check an 11ms question to
/// a local socket and a remote command a 31ms one with no handshake — which is what
/// makes polling every couple of seconds reasonable rather than rude.
///
/// One connection also means one thing to supervise. When a host restarts and
/// comes back, the agent is started again over the master put back.
nonisolated struct SSH: Sendable {

    struct Result {
        var status: Int32
        var out: String
        var error: String
        var ok: Bool { status == 0 }
    }

    /// `user@build-host`, or any alias `~/.ssh/config` knows.
    var destination: String

    /// The control socket. Per destination, in the app's own temp directory, named by a
    /// digest of the destination: a socket's path has room for 104 bytes, and a
    /// destination can be long.
    var controlPath: String {
        let digest = SHA256.hash(data: Data(destination.utf8)).prefix(8).map { String(format: "%02x", $0) }.joined()
        return FileManager.default.temporaryDirectory.appendingPathComponent("sb-\(digest).sock").path
    }

    /// How long a command on the host may take before it is stopped.
    static let timeLimit: TimeInterval = 120

    private var base: [String] {
        ["-o", "ControlPath=\(controlPath)",
         "-o", "BatchMode=yes",
         "-o", "ConnectTimeout=8"]
    }

    // MARK: - The master

    /// Open the shared connection. Idempotent — an existing live master is left alone.
    @discardableResult
    func connect() -> Result {
        if check().ok { return Result(status: 0, out: "already", error: "") }
        let launched = run([
            "-M", "-N", "-f",
            // Detect a host that went away rather than hanging on a dead socket.
            "-o", "ServerAliveInterval=15", "-o", "ServerAliveCountMax=3",
            "-o", "ControlMaster=yes", "-o", "ControlPersist=120",
        ] + base + [destination], detached: true)

        // `-f` backgrounds after authenticating, so the parent returns before the
        // child has created the control socket. Anything that needs the socket —
        // starting the agent, most of all — has to wait for it to actually be there,
        // or it fails with "No such file or directory" a few milliseconds too early.
        let deadline = Date().addingTimeInterval(10)
        while Date() < deadline {
            if check().ok { return launched }
            usleep(50_000)
        }
        return launched.ok
            ? Result(status: 1, out: "", error: L10n.text(.Places.errorNoSocket, in: Language.current))
            : launched
    }

    /// Asks the local socket, not the network.
    func check() -> Result { run(["-O", "check"] + base + [destination]) }

    func disconnect() { _ = run(["-O", "exit"] + base + [destination]) }

    // MARK: - Commands

    /// Run a command on the host, riding the master.
    func exec(_ command: String) -> Result {
        run(base + [destination, command])
    }

    /// Run a command on the host with `input` on its standard input.
    func exec(_ command: String, input: String) -> Result {
        run(base + [destination, command], input: Data(input.utf8))
    }

    /// Run a command on the host with `data` on its standard input — a file sent whole.
    func exec(_ command: String, data: Data) -> Result {
        run(base + [destination, command], input: data)
    }

    /// The arguments for `ssh` that run `command` on the host riding the master, for as long as
    /// it runs, its standard input and output this side's: the agent's way.
    func arguments(running command: String) -> [String] {
        base + ["-T", destination, command]
    }

    /// Feed a script to `python3` on the host. Every JSON edit goes through this —
    /// hosts commonly have python3 and not jq, and quoting a program through a shell
    /// argument is a way to get bitten.
    func python(_ script: String, stdin: String = "") -> Result {
        run(base + [destination, "python3 -"], input: Data((script + "\n" + stdin).utf8))
    }

    // MARK: - Process

    /// `detached` is for the master, which uses `-f` to fork into the background
    /// after authenticating. The forked child inherits whatever descriptors it was
    /// given, so handing it a pipe means reading to EOF never returns — the child
    /// is meant to outlive this call. It gets the null device instead, and its
    /// failures are read back from `-O check` rather than from stderr.
    private func run(_ arguments: [String], input: Data? = nil,
                     detached: Bool = false) -> Result {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/ssh")
        process.arguments = arguments

        if detached {
            let log = Pipe()
            process.standardOutput = FileHandle.nullDevice
            process.standardError = log
            do { try process.run() } catch {
                return Result(status: -1, out: "", error: error.localizedDescription)
            }
            // Bounded: the parent exits once the child is backgrounded, and the
            // child holds the write end open for as long as it lives.
            let deadline = Date().addingTimeInterval(20)
            while process.isRunning && Date() < deadline { usleep(20_000) }
            if process.isRunning { process.terminate() }
            try? log.fileHandleForWriting.close()
            let text = String(decoding: log.fileHandleForReading.availableData, as: UTF8.self)
            return Result(status: process.terminationStatus, out: "",
                          error: text.trimmingCharacters(in: .whitespacesAndNewlines))
        }

        do {
            let ran = try Subprocess.run("/usr/bin/ssh", arguments, input: input, timeout: Self.timeLimit)
            return Result(ran, limit: Self.timeLimit, trimmingOut: true)
        } catch {
            return Result(status: -1, out: "", error: error.localizedDescription)
        }
    }
}

nonisolated extension SSH.Result {
    /// A finished process, its errors said; one stopped at its time limit says so.
    init(_ ran: Subprocess.Outcome, limit: TimeInterval, trimmingOut: Bool) {
        let said = String(decoding: ran.err, as: UTF8.self).trimmingCharacters(in: .whitespacesAndNewlines)
        let out = String(decoding: ran.out, as: UTF8.self)
        let told: String
        if !ran.timedOut {
            told = said
        } else if said.isEmpty {
            told = L10n.text(.Places.errorTimedOut(Int(limit)), in: Language.current)
        } else {
            told = L10n.text(.Places.errorTimedOutWith(Int(limit), said), in: Language.current)
        }
        self.init(status: ran.timedOut && ran.status == 0 ? -1 : ran.status,
                  out: trimmingOut ? out.trimmingCharacters(in: .whitespacesAndNewlines) : out,
                  error: told)
    }
}
