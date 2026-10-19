import Foundation
import SwiftUI

/// What the 「MCP」 dot in the status bar (`StatusBar`) says: whether the agents this app is connected to
/// answer MCP themselves — red while one cannot (its port taken), green while one does, grey while
/// none does.
///
/// MCP is answered by each agent's own, for its projects (`agent/src/mcp.rs`): the app listens
/// nowhere and relays nothing, so this Mac's projects are not reached at all. What an agent says of
/// its MCP comes over the way to it (`RemoteHosts.CoreState.mcp`).
struct MCPLight: Equatable {
    enum Tone: Equatable {
        /// An agent answers MCP, and none fails to.
        case connected
        /// None does: no agent, none connected, or none said yet.
        case idle
        /// An agent cannot: the port of its user's setting is taken.
        case failed
    }

    var tone: Tone
    /// One line an agent: its name, then where it answers MCP or why it does not.
    var lines: [String]
    /// How many of the agents switched on the app is connected to now.
    var connected = 0
    /// The count as the status bar says it.
    var agents: String {
        switch connected {
        case 0: "No Agents Connected"
        case 1: "1 Agent Connected"
        default: "\(connected) Agents Connected"
        }
    }

    /// The tooltip.
    var help: String { lines.joined(separator: "\n") }

    var color: Color {
        switch tone {
        case .connected: .green
        case .idle: Color.secondary.opacity(0.5)
        case .failed: .red
        }
    }

    /// The dot for what is known of each agent switched on, as the list has them.
    static func of(hosts: [RemoteHosts.Host], cores: [UUID: RemoteHosts.CoreState]) -> MCPLight {
        var lines: [String] = []
        var answering = 0, failing = 0, connected = 0
        for host in hosts where host.enabled {
            let core = cores[host.id] ?? RemoteHosts.CoreState()
            if core.connected { connected += 1 }
            let said: String
            if !core.connected {
                said = L10n[.MCP.lightNotConnected]
            } else if let mcp = core.mcp {
                if mcp.listening {
                    answering += 1
                    said = L10n[.MCP.lightListening(String(mcp.port))]
                } else {
                    failing += 1
                    said = mcp.problem.isEmpty ? L10n[.MCP.lightNotListening] : L10n[.MCP.lightNotListeningWhy(mcp.problem)]
                }
            } else {
                said = L10n[.MCP.lightUnknown]
            }
            lines.append("\(host.displayName): \(said)")
        }
        if lines.isEmpty {
            lines = [L10n[.MCP.lightNoAgents]]
        }
        let tone: Tone = failing > 0 ? .failed : answering > 0 ? .connected : .idle
        return MCPLight(tone: tone, lines: lines, connected: connected)
    }
}
