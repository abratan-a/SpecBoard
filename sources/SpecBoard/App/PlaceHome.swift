import AppKit
import SwiftUI

// The places projects are kept in, at the left of the projects' home, and a place's settings at its
// right: how a place is reached, who may read it and what is public are true of the place, so they
// are managed there — not in the app's settings, with the place chosen among segments.

/// The tabs of a place's settings.
enum PlaceSettingsTab: String, CaseIterable, Identifiable, Sendable {
    case connection, localAgent, hub, keys, publishing

    var id: String { rawValue }

    var title: String {
        switch self {
        case .connection: L10n[.Places.tabConnection]
        case .localAgent: L10n[.Places.tabLocalAgent]
        case .hub: L10n[.Places.tabHub]
        case .keys: L10n[.Places.tabKeys]
        case .publishing: L10n[.Places.wordPublic]
        }
    }

    /// The tabs a place has: an agent's four; this Mac's none — it is not reached over SSH, and MCP
    /// reaches none of its projects, so it has no access keys and nothing public.
    static func of(place: UUID?, localAgent: Bool = false) -> [PlaceSettingsTab] {
        if place != nil { return [.connection, .hub, .keys, .publishing] }
        return localAgent ? [.localAgent, .hub, .keys, .publishing] : []
    }

    /// The tab a place's settings open at: its first. This Mac's settings have no tabs while its own
    /// agent is off, and show what the agent's tab shows all the same (`PlaceSettingsPane`): they are
    /// opened at that tab, so that the settings left open as the agent is switched on stay on it.
    static func opening(_ place: UUID?, localAgent: Bool = false) -> PlaceSettingsTab {
        of(place: place, localAgent: localAgent).first ?? .localAgent
    }
}

// MARK: - The places

/// A row of the places' column: every place's projects, this Mac, or an agent.
enum HomeChoice: Equatable {
    case all, thisMac, agent(UUID)
}

/// The home's left column: this Mac, then each agent — one row each, its ⚙ at the right —, the hubs
/// under them, and 「エージェントを追加」 and 「ハブを追加」 at the foot. A row is chosen to show that
/// place's projects; its ⚙ to show its settings. A hub keeps no project: its row shows its settings.
struct PlaceColumn: View {
    var model: AppModel
    var hosts: RemoteHosts
    /// The place chosen: this Mac (nil) or an agent — unless every place's projects are (`allChosen`).
    var selected: UUID?
    var allChosen = false
    var onChooseAll: () -> Void = {}
    var onChoose: (UUID?) -> Void
    var onSettings: (UUID?) -> Void

    @State private var adding = false
    @State private var draft = ""
    /// The form that adds a hub, and what is typed.
    @State private var addingHub = false
    @State private var hubDraft = ""
    /// The host whose MCP port is being set, and what is typed.
    @State private var porting: UUID?
    @State private var portDraft = ""
    /// The host being named, and what is typed (`RemoteHosts.Host.name`).
    @State private var naming: UUID?
    @State private var nameDraft = ""
    @State private var hovered: String?

    /// The rows, top down: every place's projects, this Mac, then the agents as they were added.
    static func rows(hosts: [RemoteHosts.Host]) -> [HomeChoice] {
        [.all, .thisMac] + hosts.map { .agent($0.id) }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ScrollView {
                VStack(alignment: .leading, spacing: 2) {
                    allRow
                    thisMac
                    ForEach(hosts.hosts) { host in hostRow(host) }
                    if !hosts.hubs.isEmpty {
                        Divider().padding(.horizontal, 10).padding(.vertical, 8)
                        ForEach(hosts.hubs) { hub in hubRow(hub) }
                    }
                }
                .padding(.horizontal, 6).padding(.top, 10).padding(.bottom, 8)
            }
            Divider()
            HStack(spacing: 8) {
                Button { draft = ""; adding = true } label: {
                    Label(L10n[.Places.columnAddAgent], systemImage: "plus")
                        .font(.app(12))
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .help(L10n[.Places.columnAddAgentHelp])
                .popover(isPresented: $adding, arrowEdge: .trailing) { form }
                Button { hubDraft = ""; addingHub = true } label: {
                    Text(L10n[.Places.columnAddHub])
                        .font(.app(11.5)).foregroundStyle(.secondary)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .help(L10n[.Places.columnAddHubHelp])
                .popover(isPresented: $addingHub, arrowEdge: .trailing) { hubForm }
            }
            .padding(.horizontal, 14).padding(.vertical, 10)
        }
        .background(Color(nsColor: .controlBackgroundColor).opacity(0.5))
    }

    // MARK: Rows

    private func count(_ place: UUID?) -> Int {
        model.projects.filter { (place == nil ? model.isThisMac($0.place) : $0.place == place) && !$0.isArchived }.count
    }

    /// Every place's projects: no settings of its own, so no ⚙.
    private var allRow: some View {
        row(key: "all", chosen: allChosen, choose: onChooseAll, settings: nil) {
            Image(systemName: "rectangle.stack").font(.app(12)).foregroundStyle(.secondary).frame(width: 18)
            Text(L10n[.Places.rowAll]).font(.app(12.5, weight: .semibold)).lineLimit(1)
            Spacer(minLength: 4)
            Text("\(model.projects.filter { !$0.isArchived }.count)")
                .font(.app(10.5, design: .monospaced)).foregroundStyle(.secondary)
        }
        .help(L10n[.Places.rowAllHelp])
    }

    private var thisMac: some View {
        row(key: "", chosen: !allChosen && selected == nil, choose: { onChoose(nil) }, settings: { onSettings(nil) }) {
            Image(systemName: "desktopcomputer").font(.app(12)).foregroundStyle(.secondary).frame(width: 18)
            VStack(alignment: .leading, spacing: 1) {
                Text(L10n[.Common.wordThisMac]).font(.app(12.5, weight: .semibold)).lineLimit(1)
                // Its own agent, while that is switched on: a dot for whether it is reached, and what it is.
                if let local = hosts.local {
                    let core = hosts.coreState[local.id] ?? RemoteHosts.CoreState()
                    HStack(spacing: 4) {
                        Circle().fill(core.connected ? Color.green : Color.secondary.opacity(0.4)).frame(width: 6, height: 6)
                        Text(Self.localStatus(core)).font(.app(10)).foregroundStyle(.secondary)
                            .lineLimit(1).truncationMode(.tail)
                    }
                }
            }
            Spacer(minLength: 4)
            Text("\(count(nil))").font(.app(10.5, design: .monospaced)).foregroundStyle(.secondary)
        }
        .help(L10n[.Places.rowThisMacHelp])
    }

    private func hostRow(_ host: RemoteHosts.Host) -> some View {
        let core = hosts.coreState[host.id] ?? RemoteHosts.CoreState()
        let connected = host.enabled && core.connected
        let said = Self.status(host, core)
        return row(key: host.id.uuidString, chosen: !allChosen && selected == host.id,
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

    /// A hub's row: how many agents are in it and how many are linked, its ⚙ at the right. It keeps
    /// no project, so a press on it shows its settings too.
    private func hubRow(_ hub: RemoteHosts.Hub) -> some View {
        let state = hosts.hubState[hub.id] ?? RemoteHosts.HubState()
        let said = Self.hubStatus(hub, state)
        return row(key: hub.id.uuidString, chosen: !allChosen && selected == hub.id,
                   choose: { onSettings(hub.id) }, settings: { onSettings(hub.id) }) {
            Image(systemName: "point.3.connected.trianglepath.dotted")
                .font(.app(12)).foregroundStyle(.secondary).frame(width: 18)
            VStack(alignment: .leading, spacing: 1) {
                Text(hub.displayName)
                    .font(hub.name.isEmpty ? .app(11.5, design: .monospaced) : .app(12, weight: .semibold))
                    .lineLimit(1).truncationMode(.middle)
                Text(said).font(.app(10)).foregroundStyle(.secondary)
                    .lineLimit(1).truncationMode(.tail)
            }
            .help("\(hub.destination)\n\(said)")
            Spacer(minLength: 4)
            if state.busy { ProgressView().controlSize(.small) }
        }
        .contextMenu {
            Button(hub.enabled ? L10n[.Places.hostDisconnect] : L10n[.Places.hostConnect]) { hosts.setHubEnabled(hub, !hub.enabled) }
            Button(L10n[.Common.actionRenameMenu]) {
                nameDraft = hub.name
                naming = hub.id
            }
        }
        .popover(isPresented: Binding(get: { naming == hub.id }, set: { if !$0 { naming = nil } }),
                 arrowEdge: .trailing) { hubNameForm(hub) }
    }

    /// One place's row: chosen by a press, its ⚙ at the right.
    private func row<Content: View>(key: String, chosen: Bool, choose: @escaping () -> Void,
                                    settings: (() -> Void)?,
                                    @ViewBuilder content: () -> Content) -> some View {
        HStack(spacing: 8) {
            content()
            if let settings {
                Button(action: settings) { Image(systemName: "gearshape").font(.app(11)) }
                    .buttonStyle(.borderless)
                    .foregroundStyle(chosen ? Color.primary : Color.secondary)
                    .help(L10n[.Places.rowSettingsHelp])
            } else {
                // As wide as the ⚙ of the rows below, so the counts stand in one column.
                Color.clear.frame(width: 14, height: 14)
            }
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
    /// What this Mac's row says under its name while its own agent is switched on: what it is once
    /// it is reached, and until then how far it has come, or why it is not.
    static func localStatus(_ core: RemoteHosts.CoreState) -> String {
        if core.connected { return L10n[.Places.tabLocalAgent] }
        if core.busy || core.message.isEmpty { return L10n[.Places.localAgentStarting] }
        return core.message
    }

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

    private func hubNameForm(_ hub: RemoteHosts.Hub) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(L10n[.Places.nameFormTitle]).font(.app(12, weight: .semibold))
            TextField(hub.destination, text: $nameDraft)
                .textFieldStyle(.roundedBorder).font(.app(12))
                .frame(width: 280)
                .onSubmit { hosts.renameHub(hub, nameDraft); naming = nil }
            HStack {
                Spacer()
                Button(L10n[.Common.actionChange]) { hosts.renameHub(hub, nameDraft); naming = nil }
                    .keyboardShortcut(.defaultAction)
            }
        }
        .padding(14)
    }

    private var hubForm: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(L10n[.Places.columnAddHub]).font(.app(12, weight: .semibold))
            TextField("sbhub@host", text: $hubDraft)
                .textFieldStyle(.roundedBorder).font(.app(12, design: .monospaced))
                .frame(width: 280)
                .onSubmit { commitHub() }
            Text(L10n[.Places.addHubFormNote])
                .font(.app(10.5)).foregroundStyle(.secondary)
                .frame(width: 280, alignment: .leading)
                .fixedSize(horizontal: false, vertical: true)
            HStack {
                Spacer()
                Button(L10n[.Common.actionAdd], action: commitHub)
                    .keyboardShortcut(.defaultAction)
                    .disabled(!RemoteHosts.acceptable(hubDraft.trimmingCharacters(in: .whitespaces)))
            }
        }
        .padding(14)
    }

    private func commitHub() {
        let destination = hubDraft.trimmingCharacters(in: .whitespaces)
        hosts.addHub(destination: destination)
        if let added = hosts.hubs.first(where: { $0.destination == destination }) {
            addingHub = false
            onSettings(added.id)
        }
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

    /// The agent whose settings these are: the place's, or — for this Mac — its own while that is
    /// switched on.
    private var host: RemoteHosts.Host? {
        place.map { id in hosts.hosts.first { $0.id == id } } ?? hosts.local
    }

    /// The tabs shown, and the one chosen among them (the first when what was chosen is not one).
    private var tabs: [PlaceSettingsTab] { PlaceSettingsTab.of(place: place, localAgent: hosts.local != nil) }
    private var chosen: PlaceSettingsTab { tabs.contains(tab) ? tab : tabs.first ?? tab }

    var body: some View {
        if let agent = place ?? hosts.local?.id {
            VStack(alignment: .leading, spacing: 0) {
                HStack(spacing: 12) {
                    Picker("", selection: Binding(get: { chosen }, set: { tab = $0 })) {
                        ForEach(tabs) { Text($0.title).tag($0) }
                    }
                    .pickerStyle(.segmented).labelsHidden().fixedSize()
                    Spacer()
                }
                .padding(.horizontal, 16).padding(.vertical, 10)
                Divider()
                content(agent)
            }
        } else {
            thisMac
        }
    }

    /// What this Mac's settings say: that it has no keys and nothing public, and where they are instead
    /// (also told to a test through `ProjectHomeShown`).
    /// What this Mac's settings say of its own agent: what it is, and — while it is off — what
    /// that means for MCP (also told to a test through `ProjectHomeShown`).
    static var localAgentAbout: String { L10n[.Places.localAgentAbout(LocalAgent.shown)] }
    static var localAgentOff: String { L10n[.Places.localAgentOffNote] }
    /// What an agent's keys and public projects say while it is not connected, and why.
    static var awayNotYet: String { L10n[.Places.awayNotYet] }
    static var awayWhy: String { L10n[.Places.awayWhy] }

    /// This Mac's settings while its own agent is off: what the agent is, its switch, and what its
    /// being off means for MCP.
    private var thisMac: some View {
        ScrollView {
            LocalAgentView(model: model, hosts: hosts)
                .frame(maxWidth: 720, alignment: .leading)
                .padding(16)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
        .preference(key: ProjectHomeShown.self,
                    value: ProjectHomeShown.Value(said: [Self.localAgentAbout, Self.localAgentOff]))
    }

    @ViewBuilder
    private func content(_ place: UUID) -> some View {
        switch chosen {
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
        case .localAgent:
            ScrollView {
                LocalAgentView(model: model, hosts: hosts)
                    .frame(maxWidth: 720, alignment: .leading)
                    .padding(16)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            .preference(key: ProjectHomeShown.self, value: ProjectHomeShown.Value(said: [Self.localAgentAbout]))
        case .hub:
            if let away {
                notReached(away)
            } else if let host {
                ScrollView {
                    // Rebuilt per agent: what it reads and what is typed are the agent's.
                    HostHubView(model: model, hosts: hosts, host: host)
                        .id(host.id)
                        .frame(maxWidth: 760, alignment: .leading)
                        .padding(16)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
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

/// This Mac's own agent (`LocalAgent`), in this Mac's settings: what it is, its switch — asked
/// about before it is thrown, as it moves this Mac's projects — how it stands, and, while it runs,
/// where it listens for MCP.
struct LocalAgentView: View {
    var model: AppModel
    var hosts: RemoteHosts

    @State private var asking: Bool?
    @State private var portDraft = ""

    var body: some View {
        let local = hosts.local
        let core = local.flatMap { hosts.coreState[$0.id] } ?? RemoteHosts.CoreState()
        VStack(alignment: .leading, spacing: 14) {
            Text(PlaceSettingsPane.localAgentAbout)
                .font(.app(11)).foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            VStack(alignment: .leading, spacing: 9) {
                HStack(spacing: 8) {
                    Circle().fill(core.connected ? Color.green : Color.secondary.opacity(0.4)).frame(width: 7, height: 7)
                    Text(L10n[.Places.tabLocalAgent]).font(.app(12.5, weight: .semibold))
                    Spacer()
                    if core.busy { ProgressView().controlSize(.small) }
                    Toggle("", isOn: Binding(get: { local != nil }, set: { asking = $0 }))
                        .toggleStyle(.switch).labelsHidden()
                        .help(L10n[.Places.localAgentSwitchHelp])
                }
                Text(local == nil ? L10n[.Places.localAgentOff]
                     : core.connected ? L10n[.Places.localAgentRunning] : PlaceColumn.localStatus(core))
                    .font(.app(11)).foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                if let local, core.connected {
                    Text(Self.keptText(count: model.projects.filter { $0.place == local.id }.count))
                        .font(.app(11)).foregroundStyle(.secondary)
                }
            }
            .padding(12)
            .background(RoundedRectangle(cornerRadius: 8).fill(Color.secondary.opacity(0.07)))

            if let local {
                VStack(alignment: .leading, spacing: 8) {
                    mcpLine(core)
                    if core.connected {
                        HStack(spacing: 8) {
                            Text(L10n[.Places.connectionPortLabel]).font(.app(11)).foregroundStyle(.secondary)
                            TextField("8787", text: $portDraft)
                                .textFieldStyle(.roundedBorder).font(.app(12, design: .monospaced))
                                .frame(width: 90)
                                .onSubmit { setPort(local) }
                            Button(L10n[.Common.actionChange]) { setPort(local) }
                                .disabled(Int(portDraft).map { !(1...65535).contains($0) } ?? true)
                            Spacer()
                        }
                        Text(L10n[.Places.localAgentPortNote])
                            .font(.app(10.5)).foregroundStyle(.secondary)
                    }
                }
            } else {
                Text(PlaceSettingsPane.localAgentOff)
                    .font(.app(10.5)).foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            if let error = model.lastError, asking == nil, local != nil, !core.connected, !core.busy {
                Text(error).font(.app(10.5)).foregroundStyle(.red)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .onAppear { portDraft = core.mcp.map { String($0.port) } ?? "" }
        .onChange(of: core.mcp?.port) { _, port in if let port { portDraft = String(port) } }
        .alert(asking == true ? L10n[.Places.localAgentTurnOnTitle] : L10n[.Places.localAgentTurnOffTitle],
               isPresented: Binding(get: { asking != nil }, set: { if !$0 { asking = nil } })) {
            Button(L10n[.Common.actionCancel], role: .cancel) { asking = nil }
            if asking == true {
                Button(L10n[.Places.localAgentTurnOnAction]) { hosts.setLocalAgent(true) }
            } else {
                Button(L10n[.Places.localAgentTurnOffAction]) {
                    // Its projects back in the store this app holds first: one that cannot be
                    // leaves the agent on, and why is said.
                    if model.leaveLocalAgent() { hosts.setLocalAgent(false) }
                }
            }
        } message: {
            Text(asking == true
                 ? Self.turnOnText(count: model.projects.filter { $0.place == nil }.count)
                 : Self.turnOffText(reached: core.connected, count: model.projects.filter { $0.place == local?.id }.count))
        }
    }

    private func setPort(_ host: RemoteHosts.Host) {
        guard let port = Int(portDraft), (1...65535).contains(port) else { return }
        hosts.setMCPPort(host, port)
    }

    /// What is asked before the agent is switched on: `count` projects move to its store.
    static func turnOnText(count: Int) -> String { L10n[.Places.localAgentTurnOnText(count, LocalAgent.shown)] }

    /// How many of this Mac's projects the agent keeps.
    static func keptText(count: Int) -> String { L10n[.Places.localAgentKept(LocalAgent.shown, count)] }

    /// What is asked before the agent is switched off: `count` projects come back — or, while the
    /// agent is not reached, that they cannot and where they stay.
    static func turnOffText(reached: Bool, count: Int) -> String {
        reached ? L10n[.Places.localAgentTurnOffText(count)] : L10n[.Places.localAgentTurnOffTextAway(LocalAgent.shown)]
    }

    /// Where the agent answers MCP for the AI sessions on this Mac, or why it does not.
    private func mcpLine(_ state: RemoteHosts.CoreState) -> some View {
        let said: String
        if let mcp = state.mcp {
            said = mcp.listening ? L10n[.Places.localAgentMcpListening(String(mcp.port))]
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
