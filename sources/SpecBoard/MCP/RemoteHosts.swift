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
        /// This Mac's own agent (`LocalAgent`): put and started on this Mac, not over SSH. Not one
        /// of the list (`RemoteHosts.local`), and not kept with it.
        var local = false

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
        /// Where the agent stands with the hub it joined; nil until it says.
        var hub: HubLink?
        /// Joining or leaving the hub now.
        var hubBusy = false
        /// Why joining or leaving was refused last.
        var hubRefused: String?
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
    /// This Mac's own agent while its switch is on (this Mac's settings): reached and kept up as the
    /// list's agents are, shown as this Mac.
    private(set) var local: Host?
    private(set) var state: [UUID: State] = [:]
    private(set) var coreState: [UUID: CoreState] = [:]
    /// The hubs this app reaches, and the state of the way to each (`HubCore`).
    private(set) var hubs: [Hub] = []
    private(set) var hubState: [UUID: HubState] = [:]
    @ObservationIgnored private var hubWays: [UUID: HubCore] = [:]
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
    private static let hubsKey = "mcp.hubs"
    /// Where the switch of this Mac's own agent is kept (its place, while it is on), for a release's
    /// build and for any other: the two share the app's settings, and neither takes up the other's
    /// switch — each has its agent in a home of its own (`LocalAgent.home`).
    static func localKey(release: Bool) -> String { release ? "mcp.localAgent" : "mcp.localAgent.dev" }

    private static var localKey: String {
        localKey(release: LocalAgent.isRelease(build: Bundle.main.infoDictionary?["CFBundleVersion"] as? String ?? ""))
    }
    /// Whether this is the app's own list — kept, and reaching its agents — or one only shown
    /// (`init(showing:)`).
    @ObservationIgnored private let live: Bool
    /// Where this Mac's own agent is kept when it is not the app's (`LocalAgent.home`): a home of a
    /// test's own, with which a list only shown puts and starts the agent all the same.
    @ObservationIgnored private let localHome: URL?

    @ObservationIgnored private var quitting: NSObjectProtocol?
    @ObservationIgnored private var switched: NSObjectProtocol?

    init() {
        live = true
        localHome = nil
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
        // The switch of this Mac's own agent is the app's, not a window's: thrown in one window,
        // the others follow (each window has its own list).
        switched = NotificationCenter.default.addObserver(
            forName: UserDefaults.didChangeNotification, object: UserDefaults.standard, queue: nil
        ) { [weak self] _ in
            Task { @MainActor in self?.followSwitch() }
        }
    }

    /// This Mac's own agent taken up or let go of as the switch kept says, when another window
    /// threw it: the way to it alone — its projects and its daemon are the thrower's to see to.
    private func followSwitch() {
        guard live, !Language.underTests else { return }
        let kept = UserDefaults.standard.string(forKey: Self.localKey).flatMap(UUID.init(uuidString:))
        guard kept != local?.id else { return }
        if let was = local {
            stopCore(was)
            local = nil
            coreState[was.id] = nil
        }
        if let kept {
            let host = Self.localHost(kept)
            local = host
            ensureCore(host)
        }
    }

    /// The list as given, kept nowhere and reaching nothing, the ways to the agents as said: what a
    /// picture of the window draws (`PictureTests`).
    init(showing hosts: [Host], cores: [UUID: CoreState] = [:], hubs: [Hub] = [], hubStates: [UUID: HubState] = [:],
         local: Host? = nil, localAgentHome: URL? = nil) {
        live = false
        localHome = localAgentHome
        self.hosts = hosts
        self.local = local
        for host in hosts { state[host.id] = State() }
        coreState = cores
        self.hubs = hubs
        hubState = hubStates
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
        // This Mac's own agent's daemon runs on, as a host's does: only the way to it goes.
        if let local { stopCore(local) }
        for hub in hubs {
            stopHub(hub)
            hub.ssh.disconnect()
        }
    }

    /// The list as kept.
    private func reload() {
        if let data = UserDefaults.standard.data(forKey: Self.storageKey),
           let stored = try? JSONDecoder().decode([Host].self, from: data) {
            hosts = stored.filter { Self.acceptable($0.destination) }
        }
        for host in hosts where state[host.id] == nil { state[host.id] = State() }
        // Not under the tests: the app that hosts them shows what a test gives it, and starts no
        // agent of the machine's user.
        if !Language.underTests,
           let id = UserDefaults.standard.string(forKey: Self.localKey).flatMap(UUID.init(uuidString:)) {
            local = Self.localHost(id)
        }
        if let data = UserDefaults.standard.data(forKey: Self.hubsKey),
           let stored = try? JSONDecoder().decode([Hub].self, from: data) {
            hubs = stored.filter { Self.acceptable($0.destination) }
        }
    }

    private func save() {
        UserDefaults.standard.set(try? JSONEncoder().encode(hosts), forKey: Self.storageKey)
        UserDefaults.standard.set(try? JSONEncoder().encode(hubs), forKey: Self.hubsKey)
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

    /// The agent no longer among this app's places. It stays a member of the hub it joined: taken out
    /// of the hub from its own settings first (「ハブ」), or from the hub's.
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

    // MARK: - This Mac's own agent

    /// The hosts whose agents are kept up: the list's, and this Mac's own.
    var reached: [Host] { hosts + (local.map { [$0] } ?? []) }

    /// This Mac's own agent as a host: named as this Mac is to the agents it connects to, which is
    /// what a hub it joins calls it.
    private static func localHost(_ id: UUID) -> Host {
        var host = Host(destination: "")
        host.id = id
        host.name = macName
        host.local = true
        return host
    }

    /// This Mac's own agent switched on or off (this Mac's settings). On: put under the user's home
    /// and started, kept up from then on (`ensureCore`). Off: the way to it let go of and its daemon
    /// stopped — its projects are moved back first by who switches it (`AppModel.leaveLocalAgent`).
    func setLocalAgent(_ on: Bool) {
        if on {
            guard local == nil else { return }
            let host = Self.localHost(UUID())
            local = host
            if live { UserDefaults.standard.set(host.id.uuidString, forKey: Self.localKey) }
            guard live || localHome != nil else { return }
            ensureCore(host)
        } else {
            guard let host = local else { return }
            stopCore(host)
            local = nil
            coreState[host.id] = nil
            if live { UserDefaults.standard.removeObject(forKey: Self.localKey) }
            guard live || localHome != nil else { return }
            // Not behind the hosts' connections (`queue`): while it runs on, MCP still writes
            // into a store this Mac no longer shows.
            let home = localHome
            DispatchQueue.global(qos: .userInitiated).async { Self.stopLocal(home) }
        }
    }

    /// This Mac's own agent put under the user's home unless it is there already, and how it is
    /// started: `attach`, on this Mac.
    nonisolated private static func localLaunch(_ given: URL?) throws -> AgentDeploy.Launch {
        guard let bundled = AgentDeploy.bundled() else {
            throw AgentDeploy.Failure(L10n.text(.Places.errorAgentMissing(AgentDeploy.Carried.macOS.label), in: Language.current))
        }
        let build = Bundle.main.infoDictionary?["CFBundleVersion"] as? String ?? ""
        let home = given ?? LocalAgent.home
        // A home given is not the release's, whatever the build.
        LocalAgent.settle(home: home, release: given == nil && LocalAgent.isRelease(build: build))
        let placed = try LocalAgent.place(bundled, version: AgentDeploy.version(), home: home)
        // A daemon started from the file that was replaced runs the one before: stopped, so that
        // `attach` starts this one.
        if placed.replaced { LocalAgent.stop(placed.agent, home: home) }
        return LocalAgent.attach(placed.agent, home: home)
    }

    /// This Mac's own agent's daemon stopped.
    nonisolated private static func stopLocal(_ given: URL?) {
        let home = given ?? LocalAgent.home
        LocalAgent.stop(LocalAgent.path(version: AgentDeploy.version(), home: home), home: home)
    }

    // MARK: - The agents' cores

    /// Bring up the way to every enabled host's agent and keep them up. Called when the app starts.
    func start() {
        for host in reached where host.enabled { ensureCore(host) }
        for hub in hubs where hub.enabled { ensureHub(hub) }

        // Supervision runs for as long as the app does, not just while the panel is
        // open. A host may restart often; an agent that only gets started again when
        // somebody happens to be looking at it is not one anyone can rely on.
        supervisor?.invalidate()
        supervisor = Timer.scheduledTimer(withTimeInterval: 5, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.superviseTick() }
        }
    }

    private func superviseTick() {
        for host in reached where host.enabled { superviseCore(host) }
        for hub in hubs where hub.enabled { superviseHub(hub) }
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
        let localHome = localHome
        var begun = coreState[host.id] ?? CoreState()
        begun.busy = true
        begun.lastTry = Date()
        coreState[host.id] = begun
        queue.async { [weak self] in
            var made: CoreClient?
            var madeWay: AgentCore?
            var failure = ""
            do {
                let launch: AgentDeploy.Launch
                if host.local {
                    launch = try Self.localLaunch(localHome)
                } else {
                    if !ssh.check().ok {
                        let connected = ssh.connect()
                        if !connected.ok {
                            throw AgentDeploy.Failure(connected.error.isEmpty
                                                      ? L10n.text(.Places.errorCantConnect, in: Language.current) : connected.error)
                        }
                    }
                    launch = AgentDeploy.attach(ssh, version: try Self.placeBundled(ssh))
                }
                let way = AgentCore()
                let name = Self.macName
                let client = CoreClient(agent: way, place: host.id, client: name, onThisMac: host.local)
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
                way.onHub = { said in
                    let told = HubLink(said)
                    Task { @MainActor in
                        guard self?.cores[host.id] === client else { return }
                        self?.coreState[host.id]?.hub = told
                    }
                }
                try way.start(launch, client: name)
                made = client
                madeWay = way
            } catch {
                failure = ErrorText.said(error)
            }
            let client = made, way = madeWay, said = failure
            Task { @MainActor in
                guard let self else { client?.close(); return }
                var next = self.coreState[host.id] ?? CoreState()
                next.busy = false
                // Let go of meanwhile, or gone already.
                guard let client, self.reached.contains(where: { $0.id == host.id && $0.enabled }) else {
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
                // What the agent before said of the hub: this one says its own.
                next.hubRefused = nil
                self.coreState[host.id] = next
                // Asked once the way is this place's: what it says before that is not taken.
                way?.tellHub()
                let name = self.reached.first { $0.id == host.id }?.displayName ?? host.displayName
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

    // MARK: - The hubs

    func addHub(destination: String) {
        let trimmed = destination.trimmingCharacters(in: .whitespaces)
        guard Self.acceptable(trimmed), !hubs.contains(where: { $0.destination == trimmed }) else { return }
        let hub = Hub(destination: trimmed)
        hubs.append(hub)
        save()
        ensureHub(hub)
    }

    /// The hub no longer among this app's places: it runs on, with its members.
    func removeHub(_ hub: Hub) {
        stopHub(hub)
        let ssh = hub.ssh
        queue.async { ssh.disconnect() }
        hubs.removeAll { $0.id == hub.id }
        hubState[hub.id] = nil
        save()
    }

    func renameHub(_ hub: Hub, _ name: String) {
        guard let index = hubs.firstIndex(where: { $0.id == hub.id }) else { return }
        let given = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard hubs[index].name != given else { return }
        hubs[index].name = given
        save()
    }

    func setHubEnabled(_ hub: Hub, _ enabled: Bool) {
        guard let index = hubs.firstIndex(where: { $0.id == hub.id }) else { return }
        hubs[index].enabled = enabled
        save()
        if enabled {
            ensureHub(hubs[index])
        } else {
            stopHub(hubs[index])
        }
    }

    /// The way to the hub up unless it is: the connection put back, this app's build of the agent put
    /// on the hub's host (the hub is its `hub` role), and `hub attach` started over the connection —
    /// which starts the hub there.
    func ensureHub(_ hub: Hub) {
        guard hub.enabled, hubWays[hub.id] == nil, !(hubState[hub.id]?.busy ?? false) else { return }
        let ssh = hub.ssh
        var begun = hubState[hub.id] ?? HubState()
        begun.busy = true
        begun.lastTry = Date()
        hubState[hub.id] = begun
        queue.async { [weak self] in
            var made: HubCore?
            var failure = ""
            do {
                if !ssh.check().ok {
                    let connected = ssh.connect()
                    if !connected.ok {
                        throw AgentDeploy.Failure(connected.error.isEmpty
                                                  ? L10n.text(.Places.errorCantConnect, in: Language.current) : connected.error)
                    }
                }
                let agent = try AgentDeploy.agent(for: ssh)
                let version = AgentDeploy.version()
                try AgentDeploy.place(ssh, agent: agent, version: version, under: AgentDeploy.hubHome)
                let way = HubCore()
                way.onMembers = { members in
                    Task { @MainActor in
                        guard self?.hubWays[hub.id] === way else { return }
                        self?.hubState[hub.id]?.members = members
                    }
                }
                way.onEnd = { said in
                    Task { @MainActor in self?.hubEnded(hub.id, way, said) }
                }
                try way.start(AgentDeploy.hubAttach(ssh, version: version))
                made = way
            } catch {
                failure = ErrorText.said(error)
            }
            let way = made, said = failure
            Task { @MainActor in
                guard let self else { way?.stop(); return }
                var next = self.hubState[hub.id] ?? HubState()
                next.busy = false
                guard let way, self.hubs.contains(where: { $0.id == hub.id && $0.enabled }) else {
                    way?.stop()
                    next.connected = false
                    next.misses += 1
                    if !said.isEmpty { next.message = said }
                    self.hubState[hub.id] = next
                    return
                }
                self.hubWays[hub.id] = way
                next.connected = true
                next.misses = 0
                next.message = ""
                next.build = way.hub["build"] as? String
                next.members = way.members
                self.hubState[hub.id] = next
            }
        }
    }

    private func superviseHub(_ hub: Hub) {
        let current = hubState[hub.id] ?? HubState()
        guard hubWays[hub.id] == nil, !current.busy else { return }
        if current.misses > 0, let last = current.lastTry {
            let wait = TimeInterval(min(current.misses, 6) * 5)
            if Date().timeIntervalSince(last) < wait { return }
        }
        ensureHub(hub)
    }

    private func hubEnded(_ id: UUID, _ way: HubCore, _ said: String) {
        guard hubWays[id] === way else { return }
        hubWays[id] = nil
        var next = hubState[id] ?? HubState()
        next.connected = false
        next.message = said.isEmpty ? L10n[.Places.errorConnectionLost] : said
        next.lastTry = Date()
        hubState[id] = next
    }

    /// The way to the hub let go of (the hub runs on).
    func stopHub(_ hub: Hub) {
        guard let way = hubWays.removeValue(forKey: hub.id) else { return }
        way.stop()
        hubState[hub.id]?.connected = false
        hubState[hub.id]?.message = ""
    }

    /// The hub an agent is a member of, among the hubs this app reaches: the one whose members have
    /// the agent's id.
    func hub(of host: Host) -> Hub? {
        guard let id = coreState[host.id]?.hub?.id else { return nil }
        return hubs.first { hubState[$0.id]?.members.contains { $0.id == id } ?? false }
    }

    /// The agent of this app's places that is the hub's member `id`, if one is.
    func host(member id: String) -> Host? {
        reached.first { coreState[$0.id]?.hub?.id == id }
    }

    /// The agent made a member of the hub (`join`), under the name of its place here: what the hub or
    /// the agent refuses comes to the host's state (`hubRefused`).
    func joinHub(_ host: Host, _ hub: Hub, to: String) {
        guard let agent = ways[host.id], let way = hubWays[hub.id], !(coreState[host.id]?.hubBusy ?? false) else { return }
        coreState[host.id]?.hubBusy = true
        coreState[host.id]?.hubRefused = nil
        let name = host.displayName
        queue.async { [weak self] in
            var refused: String?
            do {
                _ = try Self.join(agent: agent, hub: way, name: name, to: to)
            } catch {
                refused = ErrorText.said(error)
            }
            let said = refused
            Task { @MainActor in
                self?.coreState[host.id]?.hubBusy = false
                self?.coreState[host.id]?.hubRefused = said
            }
        }
    }

    /// The agent taken out of the hub it joined (`leave`): out of the hub's members when the hub is
    /// one this app reaches, and its own key let go either way.
    func leaveHub(_ host: Host) {
        guard let agent = ways[host.id], !(coreState[host.id]?.hubBusy ?? false) else { return }
        let way = hub(of: host).flatMap { hubWays[$0.id] }
        let id = coreState[host.id]?.hub?.id
        coreState[host.id]?.hubBusy = true
        coreState[host.id]?.hubRefused = nil
        queue.async { [weak self] in
            var refused: String?
            do {
                try Self.leave(agent: agent, hub: way, id: id)
            } catch {
                refused = ErrorText.said(error)
            }
            let said = refused
            Task { @MainActor in
                self?.coreState[host.id]?.hubBusy = false
                self?.coreState[host.id]?.hubRefused = said
            }
        }
    }

    /// A member taken out of the hub from the hub's own settings (`remove`): its agent lets go of the
    /// hub too when it is one of this app's places and reached now. One whose host is gone is out of
    /// the hub alone; its agent, if it comes back, is turned away.
    func removeMember(_ hub: Hub, _ member: HubMember) {
        guard let way = hubWays[hub.id] else { return }
        let agent = host(member: member.id).flatMap { ways[$0.id] }
        queue.async { [weak self] in
            var refused = ""
            do {
                try Self.remove(member: member.id, hub: way, agent: agent)
            } catch {
                refused = ErrorText.said(error)
            }
            let said = refused
            Task { @MainActor in
                if !said.isEmpty { self?.hubState[hub.id]?.message = said }
            }
        }
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
