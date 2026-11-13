import AppKit
import SwiftUI

/// An agent's place in the hub (its 「ハブ」 tab, `specboard://SpecBoard/Docs/Design:DesignHub` §7):
/// whether it has joined one and how its link stands, the projects it shares — with whom and for what,
/// set from each row's popover — and the projects the other agents share with it. Not in a hub, the
/// form that joins it to one of the hubs this app reaches. The shares are the agent's core's
/// (`hub_shares`, `hub_share_set`); the link is the agent's own (`RemoteHosts.HubLink`).
struct HostHubView: View {
    var model: AppModel
    var hosts: RemoteHosts
    var host: RemoteHosts.Host

    @State private var shares = CoreClient.HubShares()
    @State private var error: String?
    /// The project whose shares are being set.
    @State private var sharing: String?
    @State private var leaving = false
    /// The hub chosen to join, and where it is as the agent's host reaches it.
    @State private var chosenHub: UUID?
    @State private var toDraft = ""

    var body: some View {
        let core = hosts.coreState[host.id] ?? RemoteHosts.CoreState()
        VStack(alignment: .leading, spacing: 14) {
            Text(L10n[.Places.hubAbout])
                .font(.app(11)).foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            if let link = core.hub {
                if link.joined {
                    joinedBox(link, core)
                    sharedBy(link)
                    sharedWith(link)
                } else {
                    joinForm(core)
                    Text(L10n[.Places.hubAfterJoin]).font(.app(11)).foregroundStyle(.secondary)
                }
            } else {
                Text(L10n[.Places.hubUnknown]).font(.app(11.5)).foregroundStyle(.tertiary)
            }
            if let error {
                Text(error).font(.app(10.5)).foregroundStyle(.red).lineLimit(3)
            }
        }
        .onAppear(perform: reload)
        .onChange(of: model.noticesHeard) { reload() }
        .onChange(of: core.hub) { reload() }
        .alert(L10n[.Places.hubLeaveTitle], isPresented: $leaving) {
            Button(L10n[.Places.hubLeaveConfirm], role: .destructive) { hosts.leaveHub(host) }
            Button(L10n[.Common.actionCancel], role: .cancel) {}
        } message: {
            Text(L10n[.Places.hubLeaveMessage(host.displayName)])
        }
    }

    // MARK: - In a hub

    private func joinedBox(_ link: RemoteHosts.HubLink, _ core: RemoteHosts.CoreState) -> some View {
        let color: Color = switch link.stands {
        case .up: .green
        case .down: .orange
        case .starting: .yellow
        case .off: .secondary
        }
        let hub = hosts.hub(of: host)
        return VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 8) {
                Circle().fill(color).frame(width: 7, height: 7)
                Text(hub.map { L10n[.Places.hubIn($0.displayName)] } ?? L10n[.Places.hubInUnknown])
                    .font(.app(12.5, weight: .semibold)).lineLimit(1)
                Text(link.to ?? "").font(.app(11, design: .monospaced)).foregroundStyle(.secondary)
                    .lineLimit(1).truncationMode(.middle).textSelection(.enabled)
                Spacer()
                if core.hubBusy { ProgressView().controlSize(.small) }
                Button(L10n[.Places.hubLeave], role: .destructive) { leaving = true }
                    .controlSize(.small)
                    .disabled(core.hubBusy)
            }
            // Where it stands is said against the clock: a wait counts down.
            TimelineView(.periodic(from: .now, by: 1)) { time in
                Text(link.said(now: time.date)).font(.app(11))
                    .foregroundStyle(link.isDown ? AnyShapeStyle(.orange) : AnyShapeStyle(.secondary))
                    .lineLimit(2)
            }
            if let refused = core.hubRefused {
                Text(refused).font(.app(10.5)).foregroundStyle(.red).lineLimit(3)
            }
        }
        .padding(12)
        .background(RoundedRectangle(cornerRadius: 8).fill(Color.secondary.opacity(0.07)))
    }

    /// One of the agent's projects, with whom it is shared.
    struct Row: Equatable, Identifiable {
        var id: String
        var name: String
        var to: [CoreClient.HubShares.To]
    }

    /// The agent's projects that are worked on, by name, each with whom it is shared: the ones
    /// shared with nobody too — a project is shared from its row.
    static func rows(projects: [ProjectSummary], _ shares: CoreClient.HubShares) -> [Row] {
        projects.filter { !$0.isArchived }
            .sorted { $0.name < $1.name }
            .map { project in
                Row(id: project.id, name: project.name, to: shares.shares.first { $0.project == project.id }?.to ?? [])
            }
    }

    static func word(_ level: AccessLevel) -> String {
        level == .write ? L10n[.MCP.keysLevelWrite] : L10n[.MCP.keysLevelRead]
    }

    /// Whom a project is shared with and for what, as its row says it.
    static func chip(_ to: CoreClient.HubShares.To) -> String {
        "\(to.name) · \(word(to.level))"
    }

    /// What the member may do with the project of the row; nil: not shared with it.
    static func level(of member: String, in row: Row) -> AccessLevel? {
        row.to.first { $0.member == member }?.level
    }

    private func sharedBy(_ link: RemoteHosts.HubLink) -> some View {
        let rows = Self.rows(projects: model.projects.filter { $0.place == host.id }, shares)
        return settingsSection(L10n[.Places.hubSharedBy]) {
            VStack(alignment: .leading, spacing: 0) {
                if rows.isEmpty {
                    Text(L10n[.Places.hubNoProjects]).font(.app(11.5)).foregroundStyle(.tertiary).padding(.vertical, 6)
                }
                ForEach(rows) { row in
                    HStack(spacing: 10) {
                        Text(row.name).font(.app(12)).lineLimit(1).frame(width: 140, alignment: .leading)
                        if row.to.isEmpty {
                            Text(L10n[.Places.hubNotShared]).font(.app(11)).foregroundStyle(.tertiary)
                        } else {
                            HStack(spacing: 6) {
                                ForEach(row.to, id: \.member) { to in
                                    HStack(spacing: 0) {
                                        Text("\(to.name) · ")
                                        Text(Self.word(to.level)).foregroundStyle(.secondary)
                                    }
                                    .font(.app(11)).lineLimit(1)
                                    .padding(.horizontal, 8).padding(.vertical, 3)
                                        .background(Capsule().fill(Color.secondary.opacity(0.16)))
                                }
                            }
                        }
                        Spacer(minLength: 8)
                        Button(L10n[.Places.hubShare]) { sharing = row.id }
                            .controlSize(.small)
                            .popover(isPresented: Binding(get: { sharing == row.id }, set: { if !$0 { sharing = nil } }),
                                     arrowEdge: .bottom) { sharePopover(row, link) }
                    }
                    .padding(.vertical, 8)
                    Divider()
                }
            }
        }
    }

    /// What each other member may do with the project of the row: set at once, as it is chosen.
    func sharePopover(_ row: Row, _ link: RemoteHosts.HubLink) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(L10n[.Places.hubShareTitle(row.name)]).font(.app(12.5, weight: .semibold))
            Text(L10n[.Places.hubShareAbout]).font(.app(11)).foregroundStyle(.secondary)
            ForEach(link.others) { member in
                HStack(spacing: 8) {
                    Circle().fill(member.online ? Color.green : Color.secondary.opacity(0.6)).frame(width: 7, height: 7)
                    Text(member.name).font(.app(12)).lineLimit(1).frame(minWidth: 130, alignment: .leading)
                    Spacer()
                    Picker("", selection: Binding(
                        get: { Self.level(of: member.id, in: row) },
                        set: { level in act { try $0.setHubShare(project: row.id, member: member.id, level) } })) {
                        Text(L10n[.Common.wordNone]).tag(AccessLevel?.none)
                        Text(L10n[.MCP.keysLevelRead]).tag(AccessLevel?.some(.read))
                        Text(L10n[.MCP.keysLevelWrite]).tag(AccessLevel?.some(.write))
                    }
                    .pickerStyle(.segmented).labelsHidden().fixedSize()
                }
            }
            if link.others.isEmpty {
                Text(L10n[.Places.hubNoOthers]).font(.app(11)).foregroundStyle(.tertiary)
            }
            HStack {
                Spacer()
                Button(L10n[.Places.hubShareDone]) { sharing = nil }
                    .keyboardShortcut(.defaultAction)
            }
        }
        .padding(14)
        // As wide as its rows — a name and the three choices, wider in some languages — and no
        // narrower than the note reads well in: held to a width, the rows are pushed past both edges.
        .frame(minWidth: 380)
    }

    private func sharedWith(_ link: RemoteHosts.HubLink) -> some View {
        settingsSection(L10n[.Places.hubSharedWith]) {
            VStack(alignment: .leading, spacing: 0) {
                if shares.shared.isEmpty {
                    Text(L10n[.Places.hubNothingShared]).font(.app(11.5)).foregroundStyle(.tertiary).padding(.vertical, 6)
                }
                ForEach(shares.shared) { shared in
                    let away = link.away(shared.owner)
                    HStack(spacing: 10) {
                        Circle().fill(away == nil ? Color.green : Color.secondary.opacity(0.6)).frame(width: 7, height: 7)
                        Text(shared.name).font(.app(12)).lineLimit(1).frame(width: 128, alignment: .leading)
                        Text(L10n[.Places.hubFrom(shared.ownerName)]).font(.app(11)).foregroundStyle(.secondary)
                            .lineLimit(1).frame(width: 180, alignment: .leading)
                        Text(Self.word(shared.level)).font(.app(11)).foregroundStyle(.secondary)
                            .frame(width: 90, alignment: .leading)
                        Text(away ?? L10n[.Places.hubReachable]).font(.app(11))
                            .foregroundStyle(away == nil ? AnyShapeStyle(.secondary) : AnyShapeStyle(.tertiary))
                            .lineLimit(1)
                            .help(shared.why ?? "")
                        Spacer(minLength: 0)
                    }
                    .padding(.vertical, 8)
                    Divider()
                }
                Text(L10n[.Places.hubKeysNote])
                    .font(.app(10.5)).foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.top, 8)
            }
        }
    }

    // MARK: - Not in a hub

    private func joinForm(_ core: RemoteHosts.CoreState) -> some View {
        let hub = hosts.hubs.first { $0.id == chosenHub } ?? hosts.hubs.first
        let reached = hub.map { hosts.hubState[$0.id]?.connected ?? false } ?? false
        let to = toDraft.trimmingCharacters(in: .whitespaces)
        return VStack(alignment: .leading, spacing: 9) {
            HStack(spacing: 8) {
                Circle().fill(Color.secondary.opacity(0.6)).frame(width: 7, height: 7)
                Text(L10n[.Places.hubNotIn]).font(.app(12.5, weight: .semibold))
                Spacer()
                if core.hubBusy { ProgressView().controlSize(.small) }
            }
            if hosts.hubs.isEmpty {
                Text(L10n[.Places.hubNoHubs]).font(.app(11)).foregroundStyle(.secondary)
            } else {
                HStack(spacing: 8) {
                    Text(L10n[.Places.hubFieldHub]).font(.app(11)).foregroundStyle(.secondary)
                        .frame(width: 150, alignment: .leading)
                    Picker("", selection: Binding(get: { hub?.id }, set: { chosen in
                        chosenHub = chosen
                        toDraft = hosts.hubs.first { $0.id == chosen }?.destination ?? ""
                    })) {
                        ForEach(hosts.hubs) { one in
                            Text(one.name.isEmpty ? one.destination : "\(one.displayName) (\(one.destination))").tag(Optional(one.id))
                        }
                    }
                    .labelsHidden().fixedSize().frame(width: 260, alignment: .leading)
                    if hub != nil, !reached {
                        Text(L10n[.Places.hubHubAway]).font(.app(10.5)).foregroundStyle(.orange)
                    }
                }
                HStack(spacing: 8) {
                    Text(L10n[.Places.hubFieldTo]).font(.app(11)).foregroundStyle(.secondary)
                        .frame(width: 150, alignment: .leading)
                    TextField(hub?.destination ?? "", text: $toDraft)
                        .textFieldStyle(.roundedBorder).font(.app(12, design: .monospaced))
                        .frame(width: 260)
                    Text(L10n[.Places.hubFieldToNote]).font(.app(10.5)).foregroundStyle(.secondary)
                }
                Text(L10n[.Places.hubJoinNote])
                    .font(.app(10.5)).foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                if let refused = core.hubRefused {
                    Text(refused).font(.app(10.5)).foregroundStyle(.red).lineLimit(3)
                }
                HStack {
                    Spacer()
                    Button(L10n[.Places.hubJoin]) {
                        if let hub { hosts.joinHub(host, hub, to: to) }
                    }
                    .buttonStyle(.borderedProminent)
                    .disabled(hub == nil || !reached || core.hubBusy || !RemoteHosts.acceptable(to))
                }
            }
        }
        .padding(12)
        .background(RoundedRectangle(cornerRadius: 8).fill(Color.secondary.opacity(0.07)))
        .onAppear {
            if toDraft.isEmpty { toDraft = hub?.destination ?? "" }
        }
    }

    // MARK: - Reading and writing

    /// The shares as the agent's core has them now.
    private func reload() {
        guard let core = model.core(at: host.id) else { return }
        do {
            shares = try core.hubShares()
            error = nil
        } catch {
            self.error = ErrorText.said(error)
        }
    }

    private func act(_ body: (CoreClient) throws -> Void) {
        guard let core = model.core(at: host.id) else { return }
        do {
            try body(core)
            reload()
        } catch {
            self.error = ErrorText.said(error)
        }
    }
}
