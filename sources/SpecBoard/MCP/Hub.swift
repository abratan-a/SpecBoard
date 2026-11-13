import Foundation

// The hub among the places (`specboard://SpecBoard/Docs/Design:DesignHub` §7): the hubs this app
// reaches, where each agent stands with the hub it joined, and joining and leaving. The hub keeps
// no project: its place has only settings — who its members are.

extension RemoteHosts {

    /// A member of a hub, as the hub — or an agent linked to it — says it.
    nonisolated struct HubMember: Equatable, Sendable, Identifiable {
        /// What its agent made when it joined; what the hub names it by.
        var id: String
        /// What it is called in the hub: the name of its place in the app that added it.
        var name: String
        var online = false
        /// The app's build its agent was made for, while it is linked.
        var build: String?
        /// Since when it is linked, or when it was last.
        var since: Date?
        var seen: Date?

        /// The members as a line says them.
        static func list(_ said: [[String: Any]]) -> [HubMember] {
            said.compactMap { one in
                guard let id = one["id"] as? String else { return nil }
                let time = { (key: String) in
                    (one[key] as? Int).map { Date(timeIntervalSince1970: TimeInterval($0)) }
                }
                return HubMember(id: id, name: one["name"] as? String ?? id, online: one["online"] as? Bool ?? false,
                                 build: one["build"] as? String, since: time("since"), seen: time("seen"))
            }
        }

        /// Its row in the hub's settings: since when it is linked, or when it was last.
        func said() -> String {
            if online {
                return since.map { L10n[.Places.hubSettingsSince(HubLink.clock($0))] } ?? L10n[.Places.statusConnected]
            }
            guard let seen else { return L10n[.Places.statusNotYet] }
            let day = seen.formatted(Date.FormatStyle(date: .abbreviated, time: .shortened, locale: Language.shared.locale))
            return L10n[.Places.hubSettingsSeen(day)]
        }
    }

    /// Where an agent stands with the hub it joined, as it says it (its `hub` line).
    nonisolated struct HubLink: Equatable, Sendable {
        enum Stands: Equatable, Sendable {
            /// Not joined.
            case off
            case starting
            case up(since: Date)
            case down(at: Date, why: String, retryAt: Date)
        }

        var joined = false
        /// Who the agent is in the hub, once it has a key.
        var id: String?
        /// Where the hub is, as the agent's host reaches it.
        var to: String?
        var stands: Stands = .off
        /// The hub's members as the agent was last told them (none linked while it is not).
        var members: [HubMember] = []

        init(_ said: [String: Any]) {
            joined = said["joined"] as? Bool ?? false
            id = said["id"] as? String
            to = said["to"] as? String
            let link = said["link"] as? [String: Any] ?? [:]
            let time = { (key: String) in Date(timeIntervalSince1970: TimeInterval(link[key] as? Int ?? 0)) }
            stands = switch link["is"] as? String {
            case "up": .up(since: time("since"))
            case "down": .down(at: time("at"), why: link["why"] as? String ?? "", retryAt: time("retryAt"))
            case "starting": .starting
            default: .off
            }
            members = HubMember.list(said["members"] as? [[String: Any]] ?? [])
        }

        /// A time of today, as the screen's language writes it.
        static func clock(_ date: Date) -> String {
            date.formatted(Date.FormatStyle(date: .omitted, time: .shortened, locale: Language.shared.locale))
        }

        /// This agent among the members, and the others: who a project can be shared with.
        var me: HubMember? { id.flatMap { id in members.first { $0.id == id } } }
        var others: [HubMember] { members.filter { $0.id != id } }

        var isUp: Bool {
            if case .up = stands { return true }
            return false
        }

        var isDown: Bool {
            if case .down = stands { return true }
            return false
        }

        /// Where the link stands, as the tab says it at `now`.
        func said(now: Date) -> String {
            switch stands {
            case .off: return L10n[.Places.hubNotIn]
            case .starting: return L10n[.Places.statusConnecting]
            case .up(let since): return L10n[.Places.hubUp(Self.clock(since))]
            case .down(let at, let why, let retryAt):
                let seconds = max(0, Int(retryAt.timeIntervalSince(now).rounded(.up)))
                return L10n[.Places.hubDown(Self.clock(at), why, String(seconds))]
            }
        }

        /// Why a project the member `owner` shares cannot be used now, in a word; nil when it can:
        /// this agent is not linked, the owner is not, or the owner runs another build — the tools
        /// are described by this side and answered by the owner's.
        func away(_ owner: String) -> String? {
            guard isUp else { return L10n[.Places.hubLinkDown] }
            guard let member = members.first(where: { $0.id == owner }), member.online else {
                return L10n[.Places.hubOwnerAway]
            }
            return member.build == me?.build ? nil : L10n[.Places.hubOtherVersion]
        }
    }

    /// A hub this app reaches: a place of its own in the list, after the agents.
    struct Hub: Identifiable, Codable, Hashable {
        var id = UUID()
        /// The hub's user at its host (`sbhub@host`), or any alias `~/.ssh/config` knows.
        var destination: String
        /// Keep the way to the hub up while the app runs.
        var enabled: Bool = true
        /// What the hub is called here; empty: by its destination.
        var name: String = ""

        var ssh: SSH { SSH(destination: destination) }

        var displayName: String {
            let given = name.trimmingCharacters(in: .whitespacesAndNewlines)
            return given.isEmpty ? destination : given
        }
    }

    /// The way to a hub (`HubCore`): up while the hub is enabled.
    struct HubState {
        var connected = false
        var busy = false
        /// Why it is not connected, as said last.
        var message = ""
        var misses = 0
        var lastTry: Date?
        /// The build the hub runs, once it said.
        var build: String?
        var members: [HubMember] = []
    }

    // MARK: - Joining and leaving

    /// The agent made a member of the hub: the agent makes a key and an id (the private half stays
    /// on its host), the hub adds them under `name` and hands back its host's public host keys, and
    /// the agent is told where the hub is (`to`, as its own host reaches it) and those keys — the one
    /// host it will take for the hub. The member's id. A hub that refuses (a name taken) leaves the
    /// agent as it was.
    nonisolated static func join(agent: AgentCore, hub: HubCore, name: String, to: String) throws -> String {
        let made = try agent.hubKey()
        let hostKeys = try hub.enroll(id: made.id, name: name, key: made.key)
        do {
            try agent.hubJoin(to: to, hostKeys: hostKeys)
        } catch {
            // Not left in the hub as one that never links.
            try? hub.remove(id: made.id)
            throw error
        }
        return made.id
    }

    /// The agent taken out of the hub: gone from the hub's members and keys when the hub is reached
    /// (`hub`, and the agent's `id` there), and the agent lets go of its key and what it kept of the
    /// hub.
    nonisolated static func leave(agent: AgentCore, hub: HubCore?, id: String?) throws {
        if let hub, let id {
            // One the hub no longer has is left all the same.
            try? hub.remove(id: id)
        }
        try agent.hubLeave()
    }

    /// A member taken out of the hub from the hub's own settings: gone from the hub's members and
    /// keys, and — when its agent is one this app reaches (`agent`) — the agent lets go of its key and
    /// what it kept of the hub, as one that left. One this app does not reach stays as it was, turned
    /// away by the hub, until it is taken out where it is reached.
    nonisolated static func remove(member id: String, hub: HubCore, agent: AgentCore?) throws {
        try hub.remove(id: id)
        try agent?.hubLeave()
    }
}

extension PlaceColumn {
    /// What the hub's row says under its name: how many agents are in it and how many are linked now,
    /// or how the way to it stands.
    static func hubStatus(_ hub: RemoteHosts.Hub, _ state: RemoteHosts.HubState) -> String {
        if !hub.enabled { return L10n[.Places.statusOff] }
        if state.connected {
            return L10n[.Places.hubRowCount(state.members.count, state.members.filter(\.online).count)]
        }
        if state.busy { return L10n[.Places.statusConnecting] }
        return state.message.isEmpty ? L10n[.Places.statusNotYet] : state.message
    }
}
