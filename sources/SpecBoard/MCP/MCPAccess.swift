import Foundation

/// A call through MCP, as the Swift app's store reads it while it is running: whether one
/// is, the branch it names, and who it is from (`specboard://SpecBoard/Docs/Design:DesignAccess`).
///
/// The key a request carries, and what each tool may touch with it, are the core's to check
/// now (`sources/core/src/access.rs`); `app_control`, the one tool the window answers
/// (`AppControl`), binds `inCall` here as it reads the window's projects.
nonisolated enum MCPAccess {

    enum Need: Equatable, Sendable {
        /// Project-free (`guide`), or answering only with what may be seen (`project_list`),
        /// or making what the key then owns (`project_create`).
        case none
        case read
        case write
        /// The window, where any project can show (`app_control`): what the key may do with it
        /// goes by the call's op (the core checks it).
        case drive
    }

    struct Context: Sendable {
        var caller: Caller
        var need: Need
    }

    @TaskLocal static var context: Context?

    /// Whether a tool call is running — with a key or without. A call reads and writes
    /// the branch it names (`Named`), main when it names none — whatever branch the
    /// window has open (`specboard://SpecBoard/Docs/Design:DesignBranches` §4).
    @TaskLocal static var inCall = false

    /// The tree a call names, of the project it is about: the `branch` argument; main when it
    /// names none.
    struct Named: Sendable, Equatable {
        var projectId: String
        var branchId: String?
        var name: String
    }

    /// The line that registers an agent's MCP, listened at `port` on its host, with Claude Code there,
    /// carrying an access key of that agent's.
    static func registerCommand(key value: String, port: UInt16) -> String {
        "claude mcp add --transport http --scope user specboard http://127.0.0.1:\(port)/mcp"
            + " --header \"Authorization: Bearer \(value)\""
    }

    /// What the app kept of the relays that are gone: whether MCP was on and its port, and the tunnel's
    /// code from before access keys. Forgotten at launch: nothing reads them.
    static func forgetRelaySettings(_ defaults: UserDefaults = .standard) {
        for key in ["mcp.enabled", "mcp.port", "mcp.tunnelToken"] { defaults.removeObject(forKey: key) }
    }
}
