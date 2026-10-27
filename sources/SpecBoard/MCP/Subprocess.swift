import Foundation

/// A process run to its end, the way the app's shells, `ssh`, `git` and the update
/// check all need it.
///
/// Its output and its errors are read at the same time: read one to the end first,
/// and a process that fills the other pipe — 64 KB of errors — waits for a reader
/// that is waiting for it. Its input comes from a file rather than a pipe, for the
/// same reason (a project is megabytes). And it has a time limit: a host that stops
/// answering or a fetch that stalls would otherwise hold whatever waits on it for good.
nonisolated enum Subprocess {

    struct Outcome: Sendable {
        var status: Int32
        var out: Data
        var err: Data
        /// Stopped at the time limit.
        var timedOut: Bool
    }

    static func run(_ executable: String, _ arguments: [String], environment: [String: String]? = nil,
                    input: Data? = nil, timeout: TimeInterval) throws -> Outcome {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: executable)
        process.arguments = arguments
        if let environment { process.environment = environment }
        let out = Pipe(), err = Pipe()
        process.standardOutput = out
        process.standardError = err
        var fed: URL?
        defer { if let fed { try? FileManager.default.removeItem(at: fed) } }
        if let input {
            let url = FileManager.default.temporaryDirectory.appendingPathComponent("sb-in-\(UUID().uuidString)")
            try input.write(to: url)
            fed = url
            process.standardInput = try FileHandle(forReadingFrom: url)
        } else {
            process.standardInput = FileHandle.nullDevice
        }
        try process.run()

        final class Read: @unchecked Sendable { var data = Data() }
        let outRead = Read(), errRead = Read()
        let done = DispatchGroup()
        for (pipe, read) in [(out, outRead), (err, errRead)] {
            done.enter()
            DispatchQueue.global(qos: .utility).async {
                read.data = pipe.fileHandleForReading.readDataToEndOfFile()
                done.leave()
            }
        }
        var read = done.wait(timeout: .now() + timeout) == .success
        let timedOut = !read
        if !read {
            process.terminate()
            read = done.wait(timeout: .now() + 5) == .success
            if !read {
                kill(process.processIdentifier, SIGKILL)
                // Something it started may still hold the pipes: then its output is left unread.
                read = done.wait(timeout: .now() + 5) == .success
            }
        }
        process.waitUntilExit()
        return Outcome(status: process.terminationStatus, out: read ? outRead.data : Data(),
                       err: read ? errRead.data : Data(), timedOut: timedOut)
    }

    /// A process that ends by itself once it has put a child of its own in the background (`ssh -f`),
    /// waited for no longer than `limit`. Its output goes to the null device, its errors to a pipe.
    ///
    /// Past the limit it is stopped — SIGTERM, then SIGKILL after `grace` — and waited for again:
    /// the status of a process still running cannot be read (Foundation raises, and the app ends).
    /// One that outlives even SIGKILL is said to have failed, its status left unread.
    ///
    /// Its errors are read without waiting: the child it backgrounded, or a helper it started (a
    /// `ProxyCommand`), may hold the pipe open for as long as it lives, and reading to the end — or
    /// waiting for data on an empty pipe — would hold whatever waits on this as long.
    static func detached(_ executable: String, _ arguments: [String], limit: TimeInterval,
                         grace: TimeInterval = 3) throws -> Outcome {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: executable)
        process.arguments = arguments
        let err = Pipe()
        process.standardInput = FileHandle.nullDevice
        process.standardOutput = FileHandle.nullDevice
        process.standardError = err
        try process.run()
        try? err.fileHandleForWriting.close()

        var timedOut = false
        if !ended(process, within: limit) {
            timedOut = true
            process.terminate()
            if !ended(process, within: grace) {
                kill(process.processIdentifier, SIGKILL)
                _ = ended(process, within: grace)
            }
        }
        let said = buffered(err.fileHandleForReading)
        try? err.fileHandleForReading.close()
        guard !process.isRunning else {
            return Outcome(status: -1, out: Data(), err: said, timedOut: true)
        }
        return Outcome(status: process.terminationStatus, out: Data(), err: said, timedOut: timedOut)
    }

    /// Whether `process` has ended, waited for up to `seconds`.
    private static func ended(_ process: Process, within seconds: TimeInterval) -> Bool {
        let deadline = Date().addingTimeInterval(seconds)
        while process.isRunning && Date() < deadline { usleep(20_000) }
        return !process.isRunning
    }

    /// What the pipe holds now, up to 64 KB, read without waiting for more.
    private static func buffered(_ handle: FileHandle) -> Data {
        let fd = handle.fileDescriptor
        _ = fcntl(fd, F_SETFL, fcntl(fd, F_GETFL) | O_NONBLOCK)
        var data = Data()
        var chunk = [UInt8](repeating: 0, count: 4096)
        while data.count < 65_536 {
            let count = read(fd, &chunk, chunk.count)
            if count <= 0 { break }
            data.append(contentsOf: chunk[0..<count])
        }
        return data
    }
}
