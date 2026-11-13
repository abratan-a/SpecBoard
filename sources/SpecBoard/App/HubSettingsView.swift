import AppKit
import SwiftUI

/// A hub's settings (`specboard://SpecBoard/Docs/Design:DesignHub` §7): what it is called here and
/// whether the way to it is up, and the agents in it — each one's name, where this app reaches it,
/// since when it is linked or when it was last, and how many projects it shares and uses. An agent is
/// added from its own settings (「ハブ」); one taken out here lets go of the hub too when this app
/// reaches it, and one whose host is gone is out of the hub alone.
struct HubSettingsView: View {
    var model: AppModel
    var hosts: RemoteHosts
    var hub: RemoteHosts.Hub

    @State private var nameDraft = ""
    @FocusState private var naming: Bool
    @State private var removing = false
    @State private var removingMember: RemoteHosts.HubMember?
    /// How many projects each member shares and uses, by its id: of the agents this app reaches.
    @State private var counts: [String: Counts] = [:]
    /// Which reading of them was asked last: an earlier one that answers later is not shown.
    @State private var counting = 0

    struct Counts: Equatable {
        var shares: Int
        var uses: Int
    }

    var body: some View {
        let state = hosts.hubState[hub.id] ?? RemoteHosts.HubState()
        VStack(alignment: .leading, spacing: 14) {
            Text(L10n[.Places.hubSettingsAbout])
                .font(.app(11)).foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            VStack(alignment: .leading, spacing: 6) {
                HStack(spacing: 8) {
                    Circle().fill(hub.enabled && state.connected ? Color.green : Color.secondary.opacity(0.4))
                        .frame(width: 7, height: 7)
                    // What it is called here: empty is its destination.
                    TextField(hub.destination, text: $nameDraft)
                        .textFieldStyle(.plain).font(.app(12.5, weight: .semibold))
                        .fixedSize()
                        .focused($naming)
                        .onSubmit { hosts.renameHub(hub, nameDraft) }
                        .onChange(of: naming) { _, now in if !now { hosts.renameHub(hub, nameDraft) } }
                    if !hub.name.isEmpty {
                        Text(hub.destination).font(.app(11, design: .monospaced)).foregroundStyle(.secondary)
                            .lineLimit(1).truncationMode(.middle).textSelection(.enabled)
                    }
                    Spacer()
                    if state.busy { ProgressView().controlSize(.small) }
                    Toggle("", isOn: Binding(get: { hub.enabled }, set: { hosts.setHubEnabled(hub, $0) }))
                        .toggleStyle(.switch).labelsHidden()
                        .help(hub.enabled ? L10n[.Places.hostDisconnect] : L10n[.Places.hostConnect])
                }
                Text(Self.running(hub, state))
                    .font(.app(11)).foregroundStyle(.secondary).lineLimit(2)
            }
            .padding(12)
            .background(RoundedRectangle(cornerRadius: 8).fill(Color.secondary.opacity(0.07)))

            settingsSection(L10n[.Places.hubSettingsAgents]) {
                VStack(alignment: .leading, spacing: 0) {
                    if state.members.isEmpty {
                        Text(state.connected ? L10n[.Places.hubSettingsNoAgents] : L10n[.Places.hubSettingsUnknown])
                            .font(.app(11.5)).foregroundStyle(.tertiary).padding(.vertical, 6)
                    }
                    ForEach(state.members) { member in
                        memberRow(member)
                        Divider()
                    }
                    Text(L10n[.Places.hubSettingsAddNote])
                        .font(.app(10.5)).foregroundStyle(.secondary)
                        .padding(.top, 8)
                }
            }
            HStack {
                Spacer()
                Button(L10n[.Places.hubSettingsRemove], role: .destructive) { removing = true }
            }
        }
        .onAppear {
            nameDraft = hub.name
            count()
        }
        .onChange(of: model.noticesHeard) { count() }
        .onChange(of: state.members) { count() }
        .alert(L10n[.Places.hubSettingsRemoveTitle], isPresented: $removing) {
            Button(L10n[.Places.hubSettingsRemoveConfirm], role: .destructive) { hosts.removeHub(hub) }
            Button(L10n[.Common.actionCancel], role: .cancel) {}
        } message: {
            Text(L10n[.Places.hubSettingsRemoveMessage])
        }
        .alert(L10n[.Places.hubSettingsRemoveMemberTitle(removingMember?.name ?? "")],
               isPresented: Binding(get: { removingMember != nil }, set: { if !$0 { removingMember = nil } })) {
            Button(L10n[.Places.hubSettingsRemoveMember], role: .destructive) {
                if let member = removingMember { hosts.removeMember(hub, member) }
            }
            Button(L10n[.Common.actionCancel], role: .cancel) {}
        } message: {
            Text(L10n[.Places.hubSettingsRemoveMemberMessage])
        }
    }

    /// How the hub stands, under its name: running, with this app's version when it runs this app's
    /// build (the app puts its own build there as it connects), or how the way to it stands.
    static func running(_ hub: RemoteHosts.Hub, _ state: RemoteHosts.HubState) -> String {
        guard hub.enabled, state.connected else { return PlaceColumn.hubStatus(hub, state) }
        let info = Bundle.main.infoDictionary ?? [:]
        let mine = info["CFBundleVersion"] as? String
        if let build = state.build, !build.isEmpty, build != mine {
            return L10n[.Places.hubSettingsRunningBuild(build)]
        }
        return L10n[.Places.hubSettingsRunning(info["CFBundleShortVersionString"] as? String ?? "")]
    }

    private func memberRow(_ member: RemoteHosts.HubMember) -> some View {
        HStack(spacing: 10) {
            Circle().fill(member.online ? Color.green : Color.secondary.opacity(0.6)).frame(width: 7, height: 7)
            Text(member.name).font(.app(12, weight: .semibold)).lineLimit(1)
                .frame(width: 150, alignment: .leading)
            Text(hosts.host(member: member.id)?.destination ?? "")
                .font(.app(11, design: .monospaced)).foregroundStyle(.secondary)
                .lineLimit(1).truncationMode(.middle)
                .frame(width: 220, alignment: .leading)
            Text(member.said()).font(.app(11)).foregroundStyle(.secondary).lineLimit(1)
                .frame(width: 200, alignment: .leading)
            if let count = counts[member.id] {
                Text(L10n[.Places.hubSettingsCounts(count.shares, count.uses)])
                    .font(.app(11)).foregroundStyle(.tertiary).lineLimit(1)
            }
            Spacer(minLength: 0)
        }
        .padding(.vertical, 8)
        .contentShape(Rectangle())
        .contextMenu {
            Button(L10n[.Places.hubSettingsRemoveMember], role: .destructive) { removingMember = member }
        }
    }

    /// How many projects each member shares and uses: read from the agents this app reaches, off
    /// the main thread — an agent far away answers when it answers, and the window is not held.
    private func count() {
        let cores: [(String, CoreClient)] = (hosts.hubState[hub.id]?.members ?? []).compactMap { member in
            guard let host = hosts.host(member: member.id), let core = model.core(at: host.id) else { return nil }
            return (member.id, core)
        }
        counting += 1
        let asked = counting
        Task {
            let read = await Task.detached(priority: .userInitiated) { () -> [String: [Int]] in
                var read: [String: [Int]] = [:]
                for (id, core) in cores {
                    guard let said = try? core.hubShares() else { continue }
                    read[id] = [said.shares.count, said.shared.count]
                }
                return read
            }.value
            guard asked == counting else { return }
            counts = read.mapValues { Counts(shares: $0[0], uses: $0[1]) }
        }
    }
}
