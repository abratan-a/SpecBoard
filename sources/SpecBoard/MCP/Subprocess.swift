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
}
