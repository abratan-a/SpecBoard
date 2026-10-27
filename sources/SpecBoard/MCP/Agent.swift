import CryptoKit
import Foundation

/// Putting the agent this app carries on a host and starting it there, over the SSH
/// connection to it: as Claude Code's SSH sessions do, nothing is set up on
/// the host by hand and nothing is opened on its network.
nonisolated enum AgentDeploy {

    /// How the agent is started: over SSH (`attach`), or, in tests, as a process on this Mac.
    struct Launch: Sendable {
        var executable: String
        var arguments: [String]
    }

    /// What kept the agent from being put on a host, started or reached.
    struct Failure: LocalizedError {
        var errorDescription: String?
        init(_ message: String) { errorDescription = message }
    }

    /// The agent this app carries for macOS, beside its executable (`tools/build-agent.sh`); nil in a
    /// build without it.
    static func bundled(_ bundle: Bundle = .main) -> URL? {
        bundle.url(forAuxiliaryExecutable: "specboard-agent")
    }

    /// The hosts this app carries an agent for:
    /// macOS on Apple silicon (the Mac agent is arm64 only); Linux on x86_64, statically linked.
    enum Carried: Equatable, Sendable {
        case macOS
        case linuxX86_64

        /// The one for a host, by what `uname -s -m` says there; none for a host it carries none for.
        static func forHost(_ system: String) -> Carried? {
            let parts = system.split(whereSeparator: \.isWhitespace)
            switch (parts.first, parts.dropFirst().first) {
            case ("Darwin"?, "arm64"?): return .macOS
            case ("Linux"?, "x86_64"?): return .linuxX86_64
            default: return nil
            }
        }

        var label: String {
            switch self {
            case .macOS: "macOS"
            case .linuxX86_64: L10n.text(.Places.agentLinuxLabel, in: Language.current)
            }
        }
    }

    /// The agent this app carries for Linux on x86_64, among its resources: an ELF is not code that can
    /// be signed, so it is not beside the executable; the app's signature seals it all the same.
    static let linuxName = "specboard-agent-linux-x86_64"

    /// The agent this app carries for a kind of host; nil in a build without it.
    static func bundled(_ carried: Carried, _ bundle: Bundle = .main) -> URL? {
        switch carried {
        case .macOS: bundled(bundle)
        case .linuxX86_64: bundle.url(forResource: linuxName, withExtension: nil)
        }
    }

    /// The agent this app carries for the host at the other end: asked what it is (`uname -s -m`),
    /// read only. Throws when the host cannot be asked, or is one it carries none for.
    static func agent(for ssh: SSH, bundle: Bundle = .main) throws -> Data {
        let language = Language.current
        let system = ssh.exec("uname -s -m")
        guard system.ok else {
            let why = system.error.isEmpty ? L10n.text(.Places.errorUnameFailed, in: language) : system.error
            throw Failure(L10n.text(.Places.errorHostUnreadable(why), in: language))
        }
        guard let carried = Carried.forHost(system.out) else {
            throw Failure(L10n.text(.Places.errorNoAgentForHost(system.out), in: language))
        }
        guard let url = bundled(carried, bundle), let agent = try? Data(contentsOf: url) else {
            throw Failure(L10n.text(.Places.errorAgentMissing(carried.label), in: language))
        }
        return agent
    }

    /// A file's sha256 on the host, as `sha256sum` prints it there, else `shasum` (a Linux may have
    /// either; a Mac has the second).
    static func digestCommand(_ path: String) -> String {
        "{ sha256sum \"\(path)\" 2>/dev/null || shasum -a 256 \"\(path)\"; }"
    }

    /// The app's version and build, as a directory name: an agent placed by another build is
    /// replaced, whatever its version says (its digest is checked too).
    static func version(_ bundle: Bundle = .main) -> String {
        let info = bundle.infoDictionary ?? [:]
        let short = info["CFBundleShortVersionString"] as? String ?? "0"
        let build = info["CFBundleVersion"] as? String ?? "0"
        let raw = "\(short)-\(build)"
        return String(raw.unicodeScalars.map {
            CharacterSet.alphanumerics.contains($0) || "._-".unicodeScalars.contains($0) ? Character($0) : "_"
        })
    }

    /// Where agents are put on a host: under its home, one directory a version. What the agent's
    /// own keeps (its database, its socket) is beside it, in `~/.specboard` (`daemon.rs`).
    static let home = "$HOME/.specboard/agent"

    /// `~/.specboard/agent/<version>/specboard-agent`.
    static func remotePath(_ version: String, under root: String = home) -> String {
        "\(root)/\(version)/specboard-agent"
    }

    static func digest(_ data: Data) -> String {
        SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
    }

    /// Puts the agent (the one for the host: `agent(for:)`) on the host unless the same one is already
    /// there, and lets older builds' go (`pruneCommand`). Throws what kept it from being put there.
    static func place(_ ssh: SSH, agent: Data, version: String, under root: String = home) throws {
        let path = remotePath(version, under: root)
        let wanted = digest(agent)
        let there = ssh.exec("\(digestCommand(path)) 2>/dev/null")
        // Put there unless the one there is the one this app carries.
        if !(there.ok && there.out.split(separator: " ").first.map(String.init) == wanted) {
            let directory = "\(root)/\(version)"
            // Under a name of its own until it is known whole, so a copy cut short is never run.
            let sent = ssh.exec("umask 077; mkdir -p \"\(directory)\" && cat > \"\(directory)/.incoming\""
                                + " && \(digestCommand("\(directory)/.incoming"))", data: agent)
            guard sent.ok, sent.out.split(separator: " ").first.map(String.init) == wanted else {
                let why = sent.error.isEmpty ? L10n.text(.Places.errorArrivedDiffers, in: Language.current) : sent.error
                throw Failure(L10n.text(.Places.errorCantPlace(why), in: Language.current))
            }
            let named = ssh.exec("chmod 700 \"\(directory)/.incoming\" && mv -f \"\(directory)/.incoming\" \"\(path)\"")
            guard named.ok else { throw Failure(L10n.text(.Places.errorCantPlace(named.error), in: Language.current)) }
        }
        _ = ssh.exec(pruneCommand(version, under: root))
    }

    /// What lets the agents of builds before this one go: only folders that hold an agent, named
    /// `<version>-<build>` with a build number below this one's. A newer build's is left: the app
    /// that put it may be running, and its daemon on it (an older app is turned away from it, not
    /// let take its place). Nothing goes when this build has no number.
    static func pruneCommand(_ version: String, under root: String = home) -> String {
        guard let build = version.split(separator: "-").last.flatMap({ UInt64($0) }) else { return "true" }
        return "cd \"\(root)\" && for x in */; do x=${x%/}; b=${x##*-};"
            + " case \"$b\" in ''|*[!0-9]*) continue;; esac;"
            + " [ -f \"$x/specboard-agent\" ] && [ \"$b\" -lt \(build) ] && rm -rf -- \"$x\"; done; true"
    }

    /// The way to the agent's own core over the connection (`AgentCore`): `attach`, which starts
    /// the daemon when none runs — and says why it cannot in the screen's language, whatever the host's.
    static func attach(_ ssh: SSH, version: String, under root: String = home, language: String = Language.current) -> Launch {
        Launch(executable: "/usr/bin/ssh",
               arguments: ssh.arguments(running: "exec \"\(remotePath(version, under: root))\" attach --lang \(language)"))
    }
}
