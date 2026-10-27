import AppKit
import SwiftUI

// The places projects are kept in, at the left of the projects' home, and a place's settings at its
// right: how a place is reached, who may read it and what is public are true of the place, so they
// are managed there — not in the app's settings, with the place chosen among segments.

/// The tabs of a place's settings.
enum PlaceSettingsTab: String, CaseIterable, Identifiable, Sendable {
    case connection, keys, publishing

    var id: String { rawValue }

    var title: String {
        switch self {
        case .connection: L10n[.Places.tabConnection]
        case .keys: L10n[.Places.tabKeys]
        case .publishing: L10n[.Places.wordPublic]
        }
    }

    /// The tabs a place has: an agent's three; this Mac's none — it is not reached over SSH, and MCP
    /// reaches none of its projects, so it has no access keys and nothing public.
    static func of(place: UUID?) -> [PlaceSettingsTab] {
        place == nil ? [] : [.connection, .keys, .publishing]
    }

    /// The tab a place's settings open at: its first. This Mac's settings have none, and are opened
    /// by `.keys` all the same (`PlaceSettingsPane` shows them without tabs).
    static func opening(_ place: UUID?) -> PlaceSettingsTab {
        of(place: place).first ?? .keys
    }
}

// MARK: - The places

/// The home's left column: this Mac, then each agent — one row each, its ⚙ at the right — and
/// 「エージェントを足す」 under them. A row is chosen to show that place's projects; its ⚙ to show its
/// settings.
struct PlaceColumn: View {
    var model: AppModel
    var hosts: RemoteHosts
    /// The place chosen: this Mac (nil) or an agent.
    var selected: UUID?
    var onChoose: (UUID?) -> Void
    var onSettings: (UUID?) -> Void

    @State private var adding = false
    @State private var draft = ""
    /// The host whose MCP port is being set, and what is typed.
    @State private var porting: UUID?
    @State private var portDraft = ""
    /// The host being named, and what is typed (`RemoteHosts.Host.name`).
    @State private var naming: UUID?
    @State private var nameDraft = ""
    @State private var hovered: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ScrollView {
                VStack(alignment: .leading, spacing: 2) {
                    thisMac
                    ForEach(hosts.hosts) { host in hostRow(host) }
                }
                .padding(.horizontal, 6).padding(.top, 10).padding(.bottom, 8)
            }
            Divider()
            Button { draft = ""; adding = true } label: {
                Label(L10n[.Places.columnAddAgent], systemImage: "plus")
                    .font(.app(12))
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .padding(.horizontal, 14).padding(.vertical, 10)
            .help(L10n[.Places.columnAddAgentHelp])
            .popover(isPresented: $adding, arrowEdge: .trailing) { form }
        }
        .background(Color(nsColor: .controlBackgroundColor).opacity(0.5))
    }

    // MARK: Rows

    private func count(_ place: UUID?) -> Int {
        model.projects.filter { $0.place == place && !$0.isArchived }.count
    }

    private var thisMac: some View {
        row(key: "", chosen: selected == nil, choose: { onChoose(nil) }, settings: { onSettings(nil) }) {
            Image(systemName: "desktopcomputer").font(.app(12)).foregroundStyle(.secondary).frame(width: 18)
            Text(L10n[.Common.wordThisMac]).font(.app(12.5, weight: .semibold)).lineLimit(1)
            Spacer(minLength: 4)
            Text("\(count(nil))").font(.app(10.5, design: .monospaced)).foregroundStyle(.secondary)
        }
        .help(L10n[.Places.rowThisMacHelp])
    }

    private func hostRow(_ host: RemoteHosts.Host) -> some View {
        let core = hosts.coreState[host.id] ?? RemoteHosts.CoreState()
        let connected = host.enabled && core.connected
        let said = Self.status(host, core)
        return row(key: host.id.uuidString, chosen: selected == host.id,
                   choose: { onChoose(host.id) }, settings: { onSettings(host.id) }) {
            Circle()
                .fill(connected ? Color.green : host.enabled && core.misses > 0 ? Color.orange : Color.secondary.opacity(0.4))
                .frame(width: 7, height: 7).padding(.horizontal, 5.5)
            VStack(alignment: .leading, spacing: 1) {
                // Its name, or where it is reached: a long destination is cut in the middle, both ends
                // saying which host it is.
                Text(host.displayName)
                    .font(host.name.isEmpty ? .app(11.5, design: .monospaced) : .app(12, weight: .semibold))
                    .lineLimit(1).truncationMode(.middle)
                Text(said).font(.app(10)).foregroundStyle(.secondary)
                    .lineLimit(1).truncationMode(.tail)
            }
            .help("\(host.destination)\n\(said)")
            Spacer(minLength: 4)
            if core.busy { ProgressView().controlSize(.small) }
            if connected {
                Text("\(count(host.id))").font(.app(10.5, design: .monospaced)).foregroundStyle(.secondary)
            }
        }
        .contextMenu {
            Button(host.enabled ? L10n[.Places.hostDisconnect] : L10n[.Places.hostConnect]) { hosts.setEnabled(host, !host.enabled) }
            Button(L10n[.Common.actionRenameMenu]) {
                nameDraft = host.name
                naming = host.id
            }
            if connected {
                Button(L10n[.Places.menuMcpPort]) {
                    portDraft = hosts.coreState[host.id]?.mcp.map { String($0.port) } ?? ""
                    porting = host.id
                }
            }
            Button(L10n[.Common.actionRemove], role: .destructive) { hosts.remove(host) }
        }
        .popover(isPresented: Binding(get: { porting == host.id }, set: { if !$0 { porting = nil } }),
                 arrowEdge: .trailing) { portForm(host) }
        .popover(isPresented: Binding(get: { naming == host.id }, set: { if !$0 { naming = nil } }),
                 arrowEdge: .trailing) { nameForm(host) }
    }

    /// One place's row: chosen by a press, its ⚙ at the right.
    private func row<Content: View>(key: String, chosen: Bool, choose: @escaping () -> Void,
                                    settings: @escaping () -> Void,
                                    @ViewBuilder content: () -> Content) -> some View {
        HStack(spacing: 8) {
            content()
            Button(action: settings) { Image(systemName: "gearshape").font(.app(11)) }
                .buttonStyle(.borderless)
                .foregroundStyle(chosen ? Color.primary : Color.secondary)
                .help(L10n[.Places.rowSettingsHelp])
        }
        .padding(.vertical, 6).padding(.horizontal, 10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(RoundedRectangle(cornerRadius: 6)
            .fill(chosen ? Color.accentColor.opacity(0.16)
                  : hovered == key ? Color.secondary.opacity(0.08) : .clear))
        .contentShape(Rectangle())
        .onTapGesture(perform: choose)
        .onHover { inside in
            if inside { hovered = key } else if hovered == key { hovered = nil }
        }
    }

    // MARK: Pieces

    /// The way to the agent's projects.
    static func status(_ host: RemoteHosts.Host, _ core: RemoteHosts.CoreState) -> String {
        if !host.enabled { return L10n[.Places.statusOff] }
        if core.connected { return L10n[.Places.statusConnected] }
        if core.busy { return L10n[.Places.statusConnecting] }
        return core.message.isEmpty ? L10n[.Places.statusNotYet] : core.message
    }

    private func portForm(_ host: RemoteHosts.Host) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(L10n[.Places.portFormTitle(host.displayName)]).font(.app(12, weight: .semibold))
            TextField("8787", text: $portDraft)
                .textFieldStyle(.roundedBorder).font(.app(12, design: .monospaced))
                .frame(width: 120)
                .onSubmit { setPort(host) }
            Text(L10n[.Places.portFormNote])
                .font(.app(10.5)).foregroundStyle(.secondary)
                .frame(width: 280, alignment: .leading)
                .fixedSize(horizontal: false, vertical: true)
            HStack {
                Spacer()
                Button(L10n[.Common.actionChange]) { setPort(host) }
                    .keyboardShortcut(.defaultAction)
                    .disabled(Int(portDraft).map { !(1...65535).contains($0) } ?? true)
            }
        }
        .padding(14)
    }

    private func setPort(_ host: RemoteHosts.Host) {
        guard let port = Int(portDraft), (1...65535).contains(port) else { return }
        hosts.setMCPPort(host, port)
        porting = nil
    }

    private func nameForm(_ host: RemoteHosts.Host) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(L10n[.Places.nameFormTitle]).font(.app(12, weight: .semibold))
            TextField(host.destination, text: $nameDraft)
                .textFieldStyle(.roundedBorder).font(.app(12))
                .frame(width: 280)
                .onSubmit { rename(host) }
            Text(L10n[.Places.nameFormNote(host.destination)])
                .font(.app(10.5)).foregroundStyle(.secondary)
                .frame(width: 280, alignment: .leading)
                .fixedSize(horizontal: false, vertical: true)
            HStack {
                Spacer()
                Button(L10n[.Common.actionChange]) { rename(host) }
                    .keyboardShortcut(.defaultAction)
            }
        }
        .padding(14)
    }

    private func rename(_ host: RemoteHosts.Host) {
        hosts.rename(host, nameDraft)
        naming = nil
    }

    private var form: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(L10n[.Places.columnAddAgent]).font(.app(12, weight: .semibold))
            TextField("user@build-host", text: $draft)
                .textFieldStyle(.roundedBorder).font(.app(12, design: .monospaced))
                .frame(width: 280)
                .onSubmit { commit() }
            Text(L10n[.Places.addFormNote])
                .font(.app(10.5)).foregroundStyle(.secondary)
                .frame(width: 280, alignment: .leading)
                .fixedSize(horizontal: false, vertical: true)
            HStack {
                Spacer()
                Button(L10n[.Common.actionAdd], action: commit)
                    .keyboardShortcut(.defaultAction)
                    .disabled(!RemoteHosts.acceptable(draft.trimmingCharacters(in: .whitespaces)))
            }
        }
        .padding(14)
    }

    private func commit() {
        hosts.add(destination: draft)
        if hosts.hosts.contains(where: { $0.destination == draft.trimmingCharacters(in: .whitespaces) }) {
            adding = false
        }
    }
}

// MARK: - A place's settings

/// What the right of the home shows for a place when its ⚙ was pressed: the tabs of an agent's
/// settings, or what this Mac's are not.
struct PlaceSettingsPane: View {
    var model: AppModel
    var hosts: RemoteHosts
    var place: UUID?
    @Binding var tab: PlaceSettingsTab

    private var host: RemoteHosts.Host? { place.flatMap { id in hosts.hosts.first { $0.id == id } } }

    var body: some View {
        if let place {
            VStack(alignment: .leading, spacing: 0) {
                HStack(spacing: 12) {
                    Picker("", selection: $tab) {
                        ForEach(PlaceSettingsTab.of(place: place)) { Text($0.title).tag($0) }
                    }
                    .pickerStyle(.segmented).labelsHidden().fixedSize()
                    Spacer()
                }
                .padding(.horizontal, 16).padding(.vertical, 10)
                Divider()
                content(place)
            }
        } else {
            thisMac
        }
    }

    /// What this Mac's settings say: that it has no keys and nothing public, and where they are instead
    /// (also told to a test through `ProjectHomeShown`).
    static var thisMacNone: String { L10n[.Places.thisMacNone] }
    static var thisMacWhere: String { L10n[.Places.thisMacWhere] }
    /// What an agent's keys and public projects say while it is not connected, and why.
    static var awayNotYet: String { L10n[.Places.awayNotYet] }
    static var awayWhy: String { L10n[.Places.awayWhy] }

    /// This Mac's settings: none, and why.
    private var thisMac: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(Self.thisMacNone)
                .font(.app(12.5))
            Text(Self.thisMacWhere)
                .font(.app(11)).foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
        }
        .frame(maxWidth: 560, alignment: .leading)
        .padding(16)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .preference(key: ProjectHomeShown.self, value: ProjectHomeShown.Value(said: [Self.thisMacNone, Self.thisMacWhere]))
    }

    @ViewBuilder
    private func content(_ place: UUID) -> some View {
        switch tab {
        case .connection:
            if let host {
                ScrollView {
                    // Rebuilt per agent: what is typed is the agent's.
                    HostConnectionView(hosts: hosts, host: host)
                        .id(host.id)
                        .frame(maxWidth: 720, alignment: .leading)
                        .padding(16)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                .onAppear { hosts.startPolling() }
                .onDisappear { hosts.stopPolling() }
            } else {
                Text(L10n[.Places.settingsNoConnection]).font(.app(11.5)).foregroundStyle(.tertiary).padding(16)
            }
        case .keys:
            if let away {
                notReached(away)
            } else {
                // Rebuilt per place: it reads its keys once as it is made.
                AccessKeysView(model: model, hosts: hosts, place: place)
                    .id(place)
                    .padding(16)
            }
        case .publishing:
            if let away {
                notReached(away)
            } else {
                ScrollView {
                    PlacePublicProjects(model: model, place: place)
                        .padding(16)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
        }
    }

    /// The agent, when this is its settings and it is not connected: its keys and what is public are
    /// in its own database, so they are read only while it is.
    private var away: RemoteHosts.Host? {
        guard let host, !(hosts.coreState[host.id]?.connected ?? false) else { return nil }
        return host
    }

    private func notReached(_ host: RemoteHosts.Host) -> some View {
        VStack(spacing: 8) {
            Text(Self.awayNotYet).foregroundStyle(.secondary)
            Text(Self.awayWhy)
                .font(.app(11)).foregroundStyle(.tertiary)
                .multilineTextAlignment(.center)
                .frame(maxWidth: 420)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .preference(key: ProjectHomeShown.self, value: ProjectHomeShown.Value(said: [Self.awayNotYet, Self.awayWhy]))
    }
}

/// One agent's connection: what it is called here, whether it is reached and how, switched on and
/// off, its MCP, and letting go of it (what was settings' 「エージェント」).
struct HostConnectionView: View {
    var hosts: RemoteHosts
    var host: RemoteHosts.Host

    @State private var portDraft = ""
    @State private var nameDraft = ""
    @FocusState private var naming: Bool

    var body: some View {
        let state = hosts.state[host.id] ?? RemoteHosts.State()
        let core = hosts.coreState[host.id] ?? RemoteHosts.CoreState()
        let probe = state.probe
        VStack(alignment: .leading, spacing: 14) {
            Text(L10n[.Places.connectionAbout])
                .font(.app(11)).foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            VStack(alignment: .leading, spacing: 9) {
                HStack(spacing: 8) {
                    Circle().fill(state.alive ? Color.green : Color.secondary.opacity(0.4)).frame(width: 7, height: 7)
                    // What it is called here: empty is its destination (`RemoteHosts.Host.name`).
                    TextField(host.destination, text: $nameDraft)
                        .textFieldStyle(.plain).font(.app(12.5, weight: .semibold))
                        .focused($naming)
                        .onSubmit { hosts.rename(host, nameDraft) }
                        .onChange(of: naming) { _, now in if !now { hosts.rename(host, nameDraft) } }
                        .help(L10n[.Places.connectionNameHelp])
                    Spacer()
                    if state.busy || core.busy { ProgressView().controlSize(.small) }
                    Toggle("", isOn: Binding(get: { host.enabled }, set: { hosts.setEnabled(host, $0) }))
                        .toggleStyle(.switch).labelsHidden()
                        .help(host.enabled ? L10n[.Places.hostDisconnect] : L10n[.Places.hostConnect])
                }
                HStack(spacing: 6) {
                    Text(host.destination).font(.app(11, design: .monospaced)).foregroundStyle(.secondary)
                        .lineLimit(1).truncationMode(.middle)
                        .textSelection(.enabled)
                    if !probe.host.isEmpty {
                        Text(probe.host).font(.app(11)).foregroundStyle(.tertiary)
                    }
                }
                HStack(spacing: 14) {
                    badge(L10n[.Places.tabConnection], state.alive)
                    badge(probe.claudeRuntime ?? L10n[.Places.connectionClaudeMissing], probe.claudeRuntime != nil)
                    Spacer()
                    if let last = state.lastProbe {
                        // A frozen value has to look frozen, or it reads as current.
                        Text(last.formatted(Date.FormatStyle(date: .omitted, time: .standard, locale: Language.shared.locale)))
                            .font(.app(10)).foregroundStyle(.tertiary)
                    }
                    Button(L10n[.Places.connectionProbeNow]) { hosts.probeNow(host) }.font(.app(11))
                }
                if !probe.reachable {
                    Text(probe.message.isEmpty ? L10n[.Places.connectionNotReached] : probe.message)
                        .font(.app(11)).foregroundStyle(.secondary)
                }
                Text(PlaceColumn.status(host, core))
                    .font(.app(11)).foregroundStyle(.secondary)
            }
            .padding(12)
            .background(RoundedRectangle(cornerRadius: 8).fill(Color.secondary.opacity(0.07)))

            VStack(alignment: .leading, spacing: 8) {
                mcpLine(core)
                if core.connected {
                    HStack(spacing: 8) {
                        Text(L10n[.Places.connectionPortLabel]).font(.app(11)).foregroundStyle(.secondary)
                        TextField("8787", text: $portDraft)
                            .textFieldStyle(.roundedBorder).font(.app(12, design: .monospaced))
                            .frame(width: 90)
                            .onSubmit(setPort)
                        Button(L10n[.Common.actionChange], action: setPort)
                            .disabled(Int(portDraft).map { !(1...65535).contains($0) } ?? true)
                        Spacer()
                    }
                    Text(L10n[.Places.connectionPortNote])
                        .font(.app(10.5)).foregroundStyle(.secondary)
                }
            }
            HStack {
                Spacer()
                Button(L10n[.Places.connectionRemoveAgent], role: .destructive) { hosts.remove(host) }
            }
        }
        .onAppear {
            portDraft = core.mcp.map { String($0.port) } ?? ""
            nameDraft = host.name
        }
    }

    private func setPort() {
        guard let port = Int(portDraft), (1...65535).contains(port) else { return }
        hosts.setMCPPort(host, port)
    }

    /// Where the agent's own answers MCP for the AI sessions of its user, or why it does not.
    private func mcpLine(_ state: RemoteHosts.CoreState) -> some View {
        let said: String
        if let mcp = state.mcp {
            said = mcp.listening ? L10n[.Places.mcpListening(String(mcp.port))]
                : mcp.problem.isEmpty ? L10n[.Places.mcpNotListening] : L10n[.Places.mcpNotListeningWhy(mcp.problem)]
        } else {
            said = L10n[.Places.mcpUnknown]
        }
        return VStack(alignment: .leading, spacing: 2) {
            Text(said).font(.app(10.5))
                .foregroundStyle(state.mcp?.listening ?? true ? AnyShapeStyle(.secondary) : AnyShapeStyle(.orange))
                .lineLimit(2)
            if let refused = state.portRefused {
                Text(refused).font(.app(10.5)).foregroundStyle(.red).lineLimit(2)
            }
        }
    }

    private func badge(_ label: String, _ on: Bool) -> some View {
        HStack(spacing: 4) {
            Image(systemName: on ? "checkmark.circle.fill" : "circle")
                .font(.app(9))
                .foregroundStyle(on ? Color.green : Color.secondary.opacity(0.5))
            Text(label).font(.app(10.5)).foregroundStyle(.secondary)
        }
    }
}

/// Which of a place's projects every access key may read (`specboard://SpecBoard/Docs/Design:DesignAccess` §2): the
/// place's own — a project is public where it is kept. Also switched from the project's row.
struct PlacePublicProjects: View {
    var model: AppModel
    var place: UUID

    var body: some View {
        let projects = model.projects.filter { $0.place == place }
        VStack(alignment: .leading, spacing: 14) {
            Text(L10n[.Places.publishAbout])
                .font(.app(11)).foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
                .readingWidth()
            if projects.isEmpty {
                Text(L10n[.Places.publishNoProjects]).font(.app(11.5)).foregroundStyle(.tertiary)
            }
            VStack(alignment: .leading, spacing: 0) {
                ForEach(projects) { project in
                    let isPublic = model.publicProjects.contains(project.id)
                    HStack(spacing: 8) {
                        Image(systemName: isPublic ? "globe" : "lock")
                            .font(.app(11)).foregroundStyle(isPublic ? Color.accentColor : .secondary)
                            .frame(width: 16)
                        Text(project.name).font(.app(12)).lineLimit(1)
                        if project.isArchived {
                            Text(L10n[.Places.publishArchived]).font(.app(10)).foregroundStyle(.tertiary)
                        }
                        Spacer()
                        Picker("", selection: Binding(get: { isPublic }, set: { model.setPublic(project.id, $0) })) {
                            Text(L10n[.Places.wordPrivate]).tag(false)
                            Text(L10n[.Places.wordPublic]).tag(true)
                        }
                        .pickerStyle(.segmented).labelsHidden().fixedSize()
                    }
                    .padding(.vertical, 6)
                    Divider()
                }
            }
        }
        .preference(key: ProjectHomeShown.self, value: ProjectHomeShown.Value(publicity: projects.map {
            ProjectHomeShown.Row(name: $0.name, isPublic: model.publicProjects.contains($0.id))
        }))
    }
}
