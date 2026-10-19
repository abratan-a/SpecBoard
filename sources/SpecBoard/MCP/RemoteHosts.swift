import AppKit
import Foundation
import Observation
import SystemConfiguration

/// The hosts whose agents keep projects for this app, and the state of reaching them.
///
/// Each host gets one multiplexed SSH connection carrying the way to its agent's own core
/// (`attach`) and the polling. Two tiers, because they cost very different
/// amounts: liveness is a question to a local socket (11ms, no network), the full probe is a remote
/// command riding the existing connection (31ms, no handshake). The AI sessions
/// there reach the agent's own MCP, for the agent's projects: this app relays nothing.
@Observable
@MainActor
final class RemoteHosts {

    struct Host: Identifiable, Codable, Hashable {
        var id = UUID()
        /// `user@build-host`, or any alias `~/.ssh/config` knows.
        var destination: String
        /// Keep the agent up while the app runs.
        var enabled: Bool = true
        /// What the agent is called here, in this Mac's settings alone; empty: by its
        /// destination.
        var name: String = ""

        var ssh: SSH { SSH(destination: destination) }

        /// What the agent is shown as: its name, or where it is reached when it has none.
        var displayName: String {
            let given = name.trimmingCharacters(in: .whitespacesAndNewlines)
            return given.isEmpty ? destination : given
        }

        enum CodingKeys: String, CodingKey {
            case id, destination, enabled, name
        }
    }

    struct State {
        var alive = false
        var probe = HostProbe.Probe()
        var busy = false
        /// How many consecutive failures — the poll backs off rather than hammering
        /// a host that is in the middle of rebooting.
        var misses = 0
        var lastProbe: Date?
    }

    /// The way to a host's agent's own core (`AgentCore`): up while the host is enabled.
    struct CoreState {
        var connected = false
        var busy = false
        /// Why it is not connected, as said last.
        var message = ""
        var misses = 0
        var lastTry: Date?
        /// Where the daemon listens for MCP for its user's AI sessions (the user's setting), and
        /// why it does not (`AgentKeys`); nil until it says.
        var mcp: AgentMCP?
        /// Why the port asked for last was refused.
        var portRefused: String?
    }

    /// Where an agent's daemon listens for MCP.
    nonisolated struct AgentMCP: Equatable, Sendable {
        var port: Int
        var listening: Bool
        var problem: String

        init(_ said: [String: Any]) {
            port = said["port"] as? Int ?? 0
            listening = said["listening"] as? Bool ?? false
            problem = said["problem"] as? String ?? ""
        }
    }

    private(set) var hosts: [Host] = []
    private(set) var state: [UUID: State] = [:]
    private(set) var coreState: [UUID: CoreState] = [:]
    /// The agents' cores connected, by host, and the ways they are reached by.
    @ObservationIgnored private var cores: [UUID: CoreClient] = [:]
    @ObservationIgnored private var ways: [UUID: AgentCore] = [:]
    /// Told when a host's core is connected, with the agent's name (the app reads its projects) …
    @ObservationIgnored var onCoreConnected: ((CoreClient, String) -> Void)?
    /// … when it is gone, with what was said …
    @ObservationIgnored var onCoreGone: ((UUID, String) -> Void)?
    /// … and when the agent is given another name.
    @ObservationIgnored var onRenamed: ((UUID, String) -> Void)?
    @ObservationIgnored private var fast: Timer?
    @ObservationIgnored private var slow: Timer?
    @ObservationIgnored private var supervisor: Timer?
    @ObservationIgnored private let queue = DispatchQueue(label: "SpecBoard.ssh",
                                                          qos: .utility)

    private static let storageKey = "mcp.hosts"

    @ObservationIgnored private var quitting: NSObjectProtocol?

    init() {
        reload()
        // The app quitting takes its connections with it:
        // a master left up (`ControlPersist`) outlives the app, and macOS, seeing the app's
        // process run on without it, says SpecBoard is running in the background and lists it
        // among the login items.
        quitting = NotificationCenter.default.addObserver(
            forName: NSApplication.willTerminateNotification, object: nil, queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.closeOnQuit() }
        }
    }

    /// The list as given, kept nowhere and reaching nothing, the ways to the agents as said: what a
    /// picture of the window draws (`PictureTests`).
    init(showing hosts: [Host], cores: [UUID: CoreState] = [:]) {
        self.hosts = hosts
        for host in hosts { state[host.id] = State() }
        coreState = cores
    }

    /// Every host's way to its agent let go of and its master connection closed, waited for: the app
    /// is about to end, and nothing it started is to run on after it.
    func closeOnQuit() {
        stopPolling()
        supervisor?.invalidate(); supervisor = nil
        for host in hosts {
            stopCore(host)
            host.ssh.disconnect()
        }
    }

    /// The list as kept.
    private func reload() {
        if let data = UserDefaults.standard.data(forKey: Self.storageKey),
           let stored = try? JSONDecoder().decode([Host].self, from: data) {
            hosts = stored.filter { Self.acceptable($0.destination) }
        }
        for host in hosts where state[host.id] == nil { state[host.id] = State() }
    }

    private func save() {
        UserDefaults.standard.set(try? JSONEncoder().encode(hosts), forKey: Self.storageKey)
    }

    // MARK: - The list

    /// A destination is the last thing on `ssh`'s command line, and `ssh` reads
    /// options after it: `-oProxyCommand=…` typed as a host would run a command. So
    /// nothing that starts with `-`, and nothing with spaces, controls or the `%`
    /// that `ssh` expands.
    static func acceptable(_ destination: String) -> Bool {
        // By scalar, not by character: `-` with a combining mark after it is one
        // character that is not "-", and still reaches ssh as an option.
        !destination.isEmpty && destination.unicodeScalars.first != "-"
            && !destination.unicodeScalars.contains {
                $0 == "%" || CharacterSet.whitespacesAndNewlines.contains($0)
                    || CharacterSet.controlCharacters.contains($0)
            }
    }

    func add(destination: String) {
        let trimmed = destination.trimmingCharacters(in: .whitespaces)
        guard Self.acceptable(trimmed), !hosts.contains(where: { $0.destination == trimmed })
        else { return }
        let host = Host(destination: trimmed)
        hosts.append(host)
        state[host.id] = State()
        save()
        probeNow(host)
        ensureCore(host)
    }

    func remove(_ host: Host) {
        stopCore(host)
        let ssh = host.ssh
        queue.async { ssh.disconnect() }
        hosts.removeAll { $0.id == host.id }
        state[host.id] = nil
        coreState[host.id] = nil
        save()
    }

    /// The agent called `name` here from now on (empty: by its destination), and told to whoever
    /// shows it by name (`onRenamed`).
    func rename(_ host: Host, _ name: String) {
        guard let index = hosts.firstIndex(where: { $0.id == host.id }) else { return }
        let given = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard hosts[index].name != given else { return }
        hosts[index].name = given
        save()
        onRenamed?(host.id, hosts[index].displayName)
    }

    func setEnabled(_ host: Host, _ enabled: Bool) {
        guard let index = hosts.firstIndex(where: { $0.id == host.id }) else { return }
        hosts[index].enabled = enabled
        save()
        if enabled {
            ensureCore(hosts[index])
        } else {
            stopCore(hosts[index])
        }
    }

    // MARK: - The agents' cores

    /// Bring up the way to every enabled host's agent and keep them up. Called when the app starts.
    func start() {
        for host in hosts where host.enabled { ensureCore(host) }

        // Supervision runs for as long as the app does, not just while the panel is
        // open. A host may restart often; an agent that only gets started again when
        // somebody happens to be looking at it is not one anyone can rely on.
        supervisor?.invalidate()
        supervisor = Timer.scheduledTimer(withTimeInterval: 5, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.superviseTick() }
        }
    }

    private func superviseTick() {
        for host in hosts where host.enabled { superviseCore(host) }
    }

    /// The agent this app carries for the host (macOS on Apple silicon, or Linux on x86_64) put there unless it is there
    /// already: its version.
    nonisolated private static func placeBundled(_ ssh: SSH) throws -> String {
        let agent = try AgentDeploy.agent(for: ssh)
        let version = AgentDeploy.version()
        try AgentDeploy.place(ssh, agent: agent, version: version)
        return version
    }

    /// This Mac, as the agents are told who connects (its name in System Settings' Sharing).
    nonisolated static var macName: String {
        SCDynamicStoreCopyComputerName(nil, nil) as String? ?? ProcessInfo.processInfo.hostName
    }

    /// The host's agent's own core connected unless it is: the connection put back, the agent put
    /// on the host, and `attach` started over the connection (which starts the daemon there).
    func ensureCore(_ host: Host) {
        guard host.enabled, cores[host.id] == nil, !(coreState[host.id]?.busy ?? false) else { return }
        let ssh = host.ssh
        var begun = coreState[host.id] ?? CoreState()
        begun.busy = true
        begun.lastTry = Date()
        coreState[host.id] = begun
        queue.async { [weak self] in
            var made: CoreClient?
            var madeWay: AgentCore?
            var failure = ""
            do {
                if !ssh.check().ok {
                    let connected = ssh.connect()
                    if !connected.ok {
                        throw AgentDeploy.Failure(connected.error.isEmpty
                                                  ? L10n.text(.Places.errorCantConnect, in: Language.current) : connected.error)
                    }
                }
                let version = try Self.placeBundled(ssh)
                let way = AgentCore()
                let name = Self.macName
                let client = CoreClient(agent: way, place: host.id, client: name)
                way.onEnd = { said in
                    Task { @MainActor in self?.coreEnded(host.id, client, said) }
                }
                way.onMCP = { said, refused in
                    let told = AgentMCP(said)
                    Task { @MainActor in
                        guard self?.cores[host.id] === client else { return }
                        self?.coreState[host.id]?.mcp = told
                        self?.coreState[host.id]?.portRefused = refused
                    }
                }
                try way.start(AgentDeploy.attach(ssh, version: version), client: name)
                made = client
                madeWay = way
            } catch {
                failure = error.localizedDescription
            }
            let client = made, way = madeWay, said = failure
            Task { @MainActor in
                guard let self else { client?.close(); return }
                var next = self.coreState[host.id] ?? CoreState()
                next.busy = false
                // Let go of meanwhile, or gone already.
                guard let client, self.hosts.contains(where: { $0.id == host.id && $0.enabled }) else {
                    client?.close()
                    next.connected = false
                    next.misses += 1
                    if !said.isEmpty { next.message = said }
                    self.coreState[host.id] = next
                    return
                }
                self.cores[host.id] = client
                self.ways[host.id] = way
                next.connected = true
                next.misses = 0
                next.message = ""
                next.mcp = (way?.daemon["mcp"] as? [String: Any]).map(AgentMCP.init)
                next.portRefused = nil
                self.coreState[host.id] = next
                let name = self.hosts.first { $0.id == host.id }?.displayName ?? host.displayName
                self.onCoreConnected?(client, name)
            }
        }
    }

    /// The host's agent asked to listen for MCP at `port` from now on: what it says comes to the
    /// host's state (`mcp`, or why it refused).
    func setMCPPort(_ host: Host, _ port: Int) {
        coreState[host.id]?.portRefused = nil
        ways[host.id]?.movePort(port)
    }

    /// Tried again while it is not up, less often while the host does not answer.
    private func superviseCore(_ host: Host) {
        let current = coreState[host.id] ?? CoreState()
        guard cores[host.id] == nil, !current.busy else { return }
        if current.misses > 0, let last = current.lastTry {
            let wait = TimeInterval(min(current.misses, 6) * 5)
            if Date().timeIntervalSince(last) < wait { return }
        }
        ensureCore(host)
    }

    private func coreEnded(_ hostId: UUID, _ client: CoreClient, _ said: String) {
        guard cores[hostId] === client else { return }
        cores[hostId] = nil
        ways[hostId] = nil
        var next = coreState[hostId] ?? CoreState()
        next.connected = false
        next.message = said.isEmpty ? L10n[.Places.errorConnectionLost] : said
        next.lastTry = Date()
        coreState[hostId] = next
        onCoreGone?(hostId, next.message)
    }

    /// The way to the host's core let go of (the daemon there runs on).
    func stopCore(_ host: Host) {
        ways[host.id] = nil
        guard let client = cores.removeValue(forKey: host.id) else { return }
        client.close()
        coreState[host.id]?.connected = false
        coreState[host.id]?.message = ""
        onCoreGone?(host.id, "")
    }

    // MARK: - Polling

    /// Only while the panel is open. A background app has nobody to show this to.
    func startPolling() {
        stopPolling()
        fast = Timer.scheduledTimer(withTimeInterval: 1.5, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.tickFast() }
        }
        slow = Timer.scheduledTimer(withTimeInterval: 12, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.tickSlow() }
        }
        tickSlow()
    }

    func stopPolling() {
        fast?.invalidate(); fast = nil
        slow?.invalidate(); slow = nil
    }

    private func tickFast() {
        for host in hosts {
            let ssh = host.ssh
            queue.async { [weak self] in
                let alive = HostProbe.liveness(ssh)
                Task { @MainActor in self?.state[host.id]?.alive = alive }
            }
        }
    }

    private func tickSlow() {
        for host in hosts {
            let current = state[host.id] ?? State()
            guard !current.busy else { continue }
            // Back off while a host is down: 12s, then 24, 36… up to a minute. A
            // rebooting machine is not made to come back faster by being asked.
            if current.misses > 0 {
                let wait = TimeInterval(min(current.misses, 5) * 12)
                if let last = current.lastProbe, Date().timeIntervalSince(last) < wait { continue }
            }
            probeNow(host)
        }
    }

    func probeNow(_ host: Host) {
        let ssh = host.ssh
        mark(host, busy: true)
        queue.async { [weak self] in
            let probe = HostProbe.probe(ssh)
            Task { @MainActor in self?.apply(host, probe) }
        }
    }

    private func mark(_ host: Host, busy: Bool) {
        var current = state[host.id] ?? State()
        current.busy = busy
        state[host.id] = current
    }

    private func apply(_ host: Host, _ probe: HostProbe.Probe) {
        var current = state[host.id] ?? State()
        current.probe = probe
        current.busy = false
        current.alive = probe.reachable
        current.misses = probe.reachable ? 0 : current.misses + 1
        current.lastProbe = Date()
        state[host.id] = current
    }
}

extension RemoteHosts.Host {
    /// A host kept before it had a name has none; what it kept of the relay that went (`relays`) is
    /// left unread.
    nonisolated init(from decoder: Decoder) throws {
        let given = try decoder.container(keyedBy: CodingKeys.self)
        id = try given.decode(UUID.self, forKey: .id)
        destination = try given.decode(String.self, forKey: .destination)
        enabled = try given.decodeIfPresent(Bool.self, forKey: .enabled) ?? true
        name = try given.decodeIfPresent(String.self, forKey: .name) ?? ""
    }
}
