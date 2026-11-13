import CryptoKit
import Foundation

/// This Mac's own agent: the agent this app carries, put under the home of the user who runs the app
/// and started on this Mac without SSH, so that MCP reaches the projects this Mac keeps
/// (`RemoteHosts.local`; `specboard://SpecBoard/Docs/Design:DesignAccess`「この Mac のエージェント」).
/// It is the same agent an app reaches over SSH — the same daemon, the same home — only put and
/// started by file and process here.
nonisolated enum LocalAgent {

    /// Whether a build number is a release's: the 12 digits a built DMG carries (a build from Xcode says 1).
    static func isRelease(build: String) -> Bool {
        build.count >= 12 && build.allSatisfy(\.isNumber)
    }

    /// Where the agent's own is kept on this Mac: `~/.specboard`, as an agent reached over SSH keeps it
    /// — or, for a build that is not a release's, a place of its own beside it, so that a development
    /// build never starts a daemon over the projects a release's agent keeps for the same user.
    static func home(build: String, under userHome: URL) -> URL {
        userHome.appendingPathComponent(isRelease(build: build) ? ".specboard" : ".specboard-dev", isDirectory: true)
    }

    /// How the home is said on the screen: `~/.specboard`, or the place of a build that is not a
    /// release's.
    static func shown(build: String) -> String { isRelease(build: build) ? "~/.specboard" : "~/.specboard-dev" }

    /// This app's, as it is said.
    static var shown: String { shown(build: Bundle.main.infoDictionary?["CFBundleVersion"] as? String ?? "") }

    /// This app's.
    static var home: URL {
        home(build: Bundle.main.infoDictionary?["CFBundleVersion"] as? String ?? "",
             under: FileManager.default.homeDirectoryForCurrentUser)
    }

    /// Where an agent of `version` is put under `home`, as on a host (`AgentDeploy.remotePath`).
    static func path(version: String, home: URL) -> URL {
        home.appendingPathComponent("agent", isDirectory: true)
            .appendingPathComponent(version, isDirectory: true)
            .appendingPathComponent("specboard-agent")
    }

    /// An agent put under a home: where, and whether one that differed was there before (a daemon
    /// started from that file runs the one before, until it is stopped).
    struct Placed: Equatable {
        var agent: URL
        var replaced: Bool
    }

    /// Puts `bundled` under `home` unless the same one is there already, and lets the agents of
    /// builds before this one go (as `AgentDeploy.place` does on a host).
    static func place(_ bundled: URL, version: String, home: URL) throws -> Placed {
        let files = FileManager.default
        let language = Language.current
        let agent = path(version: version, home: home)
        guard let data = try? Data(contentsOf: bundled) else {
            throw AgentDeploy.Failure(L10n.text(.Places.errorAgentMissing(AgentDeploy.Carried.macOS.label), in: language))
        }
        let wanted = AgentDeploy.digest(data)
        let there = (try? Data(contentsOf: agent)).map(AgentDeploy.digest)
        var replaced = false
        if there != wanted {
            do {
                let directory = agent.deletingLastPathComponent()
                try files.createDirectory(at: home, withIntermediateDirectories: true,
                                          attributes: [.posixPermissions: 0o700])
                try files.createDirectory(at: directory, withIntermediateDirectories: true)
                // Under a name of its own until it is whole, so a copy cut short is never run; then
                // named in one step, so there is no moment with no agent at its place.
                let incoming = directory.appendingPathComponent(".incoming-\(UUID().uuidString.prefix(8))")
                try data.write(to: incoming)
                try files.setAttributes([.posixPermissions: 0o700], ofItemAtPath: incoming.path)
                guard rename(incoming.path, agent.path) == 0 else {
                    let why = String(cString: strerror(errno))
                    try? files.removeItem(at: incoming)
                    throw AgentDeploy.Failure(why)
                }
                replaced = there != nil
            } catch {
                throw AgentDeploy.Failure(L10n.text(.Places.errorCantPlace(error.localizedDescription), in: language))
            }
        }
        prune(version: version, home: home)
        return Placed(agent: agent, replaced: replaced)
    }

    /// The agents of builds before this one, let go of: only folders that hold an agent, named
    /// `<version>-<build>` with a build number below this one's (`AgentDeploy.pruneCommand`).
    static func prune(version: String, home: URL) {
        guard let build = version.split(separator: "-").last.flatMap({ UInt64($0) }) else { return }
        let files = FileManager.default
        let root = home.appendingPathComponent("agent", isDirectory: true)
        for name in (try? files.contentsOfDirectory(atPath: root.path)) ?? [] {
            guard let theirs = name.split(separator: "-").last.flatMap({ UInt64($0) }), theirs < build else { continue }
            let folder = root.appendingPathComponent(name, isDirectory: true)
            guard files.fileExists(atPath: folder.appendingPathComponent("specboard-agent").path) else { continue }
            try? files.removeItem(at: folder)
        }
    }

    /// A home that is not a release's listens for MCP at any free port unless told one: the port
    /// agents listen at by default is the release's agent's on the same Mac.
    static func settle(home: URL, release: Bool) {
        guard !release else { return }
        let file = home.appendingPathComponent("agent.json")
        guard !FileManager.default.fileExists(atPath: file.path) else { return }
        try? FileManager.default.createDirectory(at: home, withIntermediateDirectories: true,
                                                 attributes: [.posixPermissions: 0o700])
        try? Data(#"{"mcpPort": 0}"#.utf8).write(to: file)
    }

    /// The way to the agent's own core (`AgentCore`): `attach` started on this Mac, which starts the
    /// daemon when none runs — and says why it cannot in the screen's language. The agent is started
    /// itself and told its home in the environment: handed to `env` as words, a path with an equals
    /// sign in it would be read as a variable to set, and the word after it started in its place.
    static func attach(_ agent: URL, home: URL, language: String = Language.current) -> AgentDeploy.Launch {
        AgentDeploy.Launch(executable: agent.path, arguments: ["attach", "--lang", language],
                           environment: ["SPECBOARD_AGENT_HOME": home.path])
    }

    /// The daemon of `home` told to stop, and waited for: true when none runs after.
    @discardableResult
    static func stop(_ agent: URL, home: URL) -> Bool {
        let process = Process()
        process.executableURL = agent
        process.arguments = ["stop"]
        var environment = ProcessInfo.processInfo.environment
        environment["SPECBOARD_AGENT_HOME"] = home.path
        process.environment = environment
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        do {
            try process.run()
        } catch {
            return false
        }
        process.waitUntilExit()
        return process.terminationStatus == 0
    }
}
