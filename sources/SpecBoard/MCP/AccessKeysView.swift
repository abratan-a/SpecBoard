import AppKit
import SwiftUI

/// The access keys MCP asks for, and what each may touch (`specboard://SpecBoard/Docs/Design:DesignAccess`).
///
/// A key's value is never shown as text: right after it is made there is a copy
/// button, and the command that registers it opens only when asked for. The core keeps
/// no more than its digest, so once this page is left the value is gone. What a key
/// may touch changes here alone — no tool changes it, so an agent cannot give itself more.
/// Read and set through the core (`CoreAccess`).
struct AccessKeysView: View {
    var model: AppModel
    var hosts: RemoteHosts

    /// Whose keys: an agent's — its own database's, which its own MCP checks. Given by the place
    /// whose settings these are. This Mac has none: MCP reaches no project of it.
    let place: UUID

    @State private var keys: [AccessKey] = []
    /// Key → how many projects it was given.
    @State private var counts: [String: Int] = [:]
    @State private var selected: String?
    @State private var grants: [String: AccessLevel] = [:]
    @State private var draftName = ""
    @FocusState private var nameFocused: Bool
    @State private var naming = false
    @State private var renaming = ""
    /// The key just made, with its value: until this page is left.
    @State private var made: Made?
    @State private var showingCommand = false
    @State private var said: String?
    @State private var revoking: AccessKey?
    @State private var regenerating: AccessKey?
    @State private var error: String?

    private struct Made: Equatable {
        var key: AccessKey
        var value: String
        /// Made again: the same key with a new value, not a new key.
        var again = false
    }

    /// What the page starts asking — for a test: whether to make a key's value again, or take it back.
    enum Asking {
        case regenerate(String)
        case revoke(String)
    }

    /// Read at once, so the first drawing already has the keys.
    init(model: AppModel, hosts: RemoteHosts, place: UUID, asking: Asking? = nil) {
        self.model = model
        self.hosts = hosts
        self.place = place
        let read = Self.read(model, place: place, selected: nil)
        _keys = State(initialValue: read.keys)
        _counts = State(initialValue: read.counts)
        _selected = State(initialValue: read.selected)
        _grants = State(initialValue: read.grants)
        _renaming = State(initialValue: read.keys.first { $0.id == read.selected }?.name ?? "")
        switch asking {
        case .regenerate(let id): _regenerating = State(initialValue: read.keys.first { $0.id == id })
        case .revoke(let id): _revoking = State(initialValue: read.keys.first { $0.id == id })
        case nil: break
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text(L10n[.MCP.keysIntro])
                .font(.app(11)).foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)

            placeNote
            if let made { madePanel(made) }
            maker
            if let error {
                Text(error).font(.app(11)).foregroundStyle(.red)
            }

            HStack(alignment: .top, spacing: 0) {
                list.frame(width: 280)
                Divider()
                detail.frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            }
            .background(RoundedRectangle(cornerRadius: 8).fill(Color.secondary.opacity(0.05)))
            .frame(maxHeight: .infinity)
        }
        .onAppear(perform: reload)
        // What the core told of: a project made through MCP is the making key's to write.
        .onChange(of: model.noticesHeard) { refresh() }
        .onDisappear { made = nil }
        .preference(key: AccessKeysShown.self, value: AccessKeysShown.Value(
            lines: Dictionary(uniqueKeysWithValues: keys.map { ($0.id, Self.summary($0, projects: counts[$0.id] ?? 0)) }),
            grants: grants.mapValues(\.rawValue)))
        .confirmationDialog(revoking.map { L10n[.MCP.keysRevokeTitle($0.name)] } ?? "",
                            isPresented: Binding(get: { revoking != nil }, set: { if !$0 { revoking = nil } }),
                            presenting: revoking) { key in
            Button(L10n[.MCP.keysRevoke], role: .destructive) {
                act { try $0.revokeAccessKey(key.id) }
                if made?.key.id == key.id { made = nil }
            }
            Button(L10n[.Common.actionCancel], role: .cancel) { revoking = nil }
        } message: { _ in
            Text(L10n[.MCP.keysRevokeMessage])
        }
        .confirmationDialog(regenerating.map { L10n[.MCP.keysRegenerateTitle($0.name)] } ?? "",
                            isPresented: Binding(get: { regenerating != nil }, set: { if !$0 { regenerating = nil } }),
                            presenting: regenerating) { key in
            Button(L10n[.MCP.keysRegenerate], role: .destructive) { regenerate(key) }
            // Said, as every window's: one left to the system is said in English.
            Button(L10n[.Common.actionCancel], role: .cancel) { regenerating = nil }
        } message: { _ in
            Text(L10n[.MCP.keysRegenerateMessage])
        }
    }

    // MARK: - Whose keys (the place the settings are of)

    private var placeNote: some View {
        // The port as digits: a number put in a Text is written as the Mac's region writes numbers (8,787).
        Text(L10n[.MCP.keysPlaceNote(String(agentPort))])
            .font(.app(10.5)).foregroundStyle(.secondary)
            .fixedSize(horizontal: false, vertical: true)
    }

    /// Where the chosen agent listens for MCP.
    private var agentPort: UInt16 {
        hosts.coreState[place]?.mcp.map { UInt16(clamping: $0.port) } ?? 8787
    }

    // MARK: - Making one

    @ViewBuilder
    private var maker: some View {
        if naming {
            HStack(spacing: 8) {
                TextField(L10n[.MCP.keysNamePlaceholder], text: $draftName)
                    .textFieldStyle(.roundedBorder).frame(maxWidth: 320)
                    .focused($nameFocused)
                    .onSubmit(make)
                    // Typed into as it shows, as a name sheet is.
                    .onAppear { DispatchQueue.main.async { nameFocused = true } }
                Button(L10n[.Common.actionCreate], action: make)
                    .disabled(draftName.trimmingCharacters(in: .whitespaces).isEmpty)
                Button(L10n[.Common.actionCancel]) { naming = false; draftName = "" }
            }
        } else {
            Button(L10n[.MCP.keysCreateKey]) { naming = true; said = nil }
        }
    }

    private func make() {
        guard let core = model.core(at: place) else { return }
        do {
            let created = try core.createAccessKey(name: draftName)
            made = Made(key: created.key, value: created.value)
            selected = created.key.id
            showingCommand = false
            said = nil
            naming = false
            draftName = ""
            reload()
        } catch {
            self.error = ErrorText.said(error)
        }
    }

    /// The same key with a new value, handed out as a new key's is.
    private func regenerate(_ key: AccessKey) {
        guard let core = model.core(at: place) else { return }
        do {
            let again = try core.regenerateAccessKey(key.id)
            made = Made(key: again.key, value: again.value, again: true)
            selected = again.key.id
            showingCommand = false
            said = nil
            reload()
        } catch {
            self.error = ErrorText.said(error)
        }
    }

    /// The one place a value is handed out: copied, never drawn, until the command is asked for.
    private func madePanel(_ made: Made) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(made.again ? L10n[.MCP.keysRegenerated(made.key.name)] : L10n[.MCP.keysCreated(made.key.name)])
                .font(.app(11.5))
                .fixedSize(horizontal: false, vertical: true)
            HStack(spacing: 8) {
                Button(L10n[.MCP.keysCopyKey]) { copy(made.value, L10n[.MCP.keysCopiedKey]) }
                Button(showingCommand ? L10n[.MCP.keysHideCommand] : L10n[.MCP.keysShowCommand]) { showingCommand.toggle() }
                if let said { Text(said).font(.app(10.5)).foregroundStyle(.secondary) }
                Spacer()
                Button(L10n[.Common.actionClose]) { self.made = nil }
                    .buttonStyle(.plain).font(.app(10.5)).foregroundStyle(.secondary)
            }
            if showingCommand {
                let command = MCPAccess.registerCommand(key: made.value, port: agentPort)
                VStack(alignment: .leading, spacing: 6) {
                    Text(command)
                        .font(.app(10.5, design: .monospaced))
                        .textSelection(.enabled)
                        .fixedSize(horizontal: false, vertical: true)
                    HStack {
                        Button(L10n[.MCP.keysCopyCommand]) { copy(command, L10n[.MCP.keysCopiedCommand]) }
                        Spacer()
                    }
                }
                .padding(10)
                .background(RoundedRectangle(cornerRadius: 6).fill(Color(nsColor: .textBackgroundColor)))
            }
        }
        .padding(12)
        .background(RoundedRectangle(cornerRadius: 8).fill(Color.accentColor.opacity(0.1)))
    }

    private func copy(_ text: String, _ message: String) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
        said = message
    }

    // MARK: - The keys

    private var list: some View {
        List(selection: Binding(get: { selected }, set: { selected = $0; reload() })) {
            if keys.isEmpty {
                Text(L10n[.MCP.keysEmpty]).font(.app(11.5)).foregroundStyle(.tertiary)
            }
            ForEach(keys) { key in
                VStack(alignment: .leading, spacing: 3) {
                    Text(key.name).font(.app(12, weight: .semibold)).lineLimit(1)
                    Text(L10n[.MCP.keysCreatedOn(Self.when(key.createdAt))])
                        .font(.app(10)).foregroundStyle(.secondary)
                    Text(key.usedAt.map { L10n[.MCP.keysLastUsed(Self.when($0))] } ?? L10n[.MCP.keysNeverUsed])
                        .font(.app(10)).foregroundStyle(.secondary)
                    Text(Self.summary(key, projects: counts[key.id] ?? 0))
                        .font(.app(10)).foregroundStyle(.tertiary)
                }
                .padding(.vertical, 3)
                .tag(key.id)
            }
        }
        .listStyle(.inset)
        .scrollContentBackground(.hidden)
    }

    @ViewBuilder
    private var detail: some View {
        if let key = keys.first(where: { $0.id == selected }) {
            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    HStack(spacing: 8) {
                        TextField(L10n[.Common.wordName], text: $renaming)
                            .textFieldStyle(.roundedBorder).frame(maxWidth: 320)
                            .onSubmit { act { try $0.renameAccessKey(key.id, to: renaming) } }
                        Spacer()
                        Button(L10n[.MCP.keysRegenerateButton]) { regenerating = key }
                            .help(L10n[.MCP.keysRegenerateHelp])
                        Button(L10n[.MCP.keysRevokeButton], role: .destructive) { revoking = key }
                    }

                    settingsSection(L10n[.MCP.keysSectionProjects]) {
                        VStack(alignment: .leading, spacing: 0) {
                            ForEach(model.projects.filter { $0.place == place }) { project in grantRow(key, project) }
                        }
                    }

                    settingsSection(L10n[.MCP.keysSectionScreen]) {
                        VStack(alignment: .leading, spacing: 6) {
                            permit(key, .control, L10n[.MCP.keysPermitControl])
                            permit(key, .capture, L10n[.MCP.keysPermitCapture])
                            Text(L10n[.MCP.keysScreenNote])
                                .font(.app(10.5)).foregroundStyle(.secondary)
                                .fixedSize(horizontal: false, vertical: true)
                        }
                    }

                    settingsSection(L10n[.Common.wordProject]) {
                        VStack(alignment: .leading, spacing: 6) {
                            permit(key, .create, L10n[.MCP.keysPermitCreate])
                            Text(L10n[.MCP.keysProjectsNote])
                                .font(.app(10.5)).foregroundStyle(.secondary)
                                .fixedSize(horizontal: false, vertical: true)
                        }
                    }
                }
                .padding(16)
            }
        } else {
            Text(keys.isEmpty ? L10n[.MCP.keysEmptyHint] : L10n[.MCP.keysChooseHint])
                .font(.app(11.5)).foregroundStyle(.tertiary)
                .padding(16)
        }
    }

    /// One of what the key may do besides its projects: 許可, 都度承認 or 拒否, set at once.
    private func permit(_ key: AccessKey, _ permit: CoreClient.Permit, _ title: String) -> some View {
        HStack(spacing: 8) {
            Text(title).font(.app(12)).fixedSize(horizontal: false, vertical: true)
            Spacer()
            MCPStateChoice(state: key.state(permit)) { state in
                act { try $0.setPermit(key.id, permit, state) }
            }
        }
    }

    /// The projects it was given and what else it may do, in a line of the list: what is not as a key is
    /// made.
    static func summary(_ key: AccessKey, projects: Int) -> String {
        var said = [L10n[.MCP.keysSummaryProjects(projects)]]
        for permit in [CoreClient.Permit.control, .capture, .create] where key.state(permit) != permit.byDefault {
            let state = key.state(permit).title
            let line: String = switch permit {
            case .control: L10n[.MCP.keysSummaryControl(state)]
            case .capture: L10n[.MCP.keysSummaryCapture(state)]
            case .create: L10n[.MCP.keysSummaryCreate(state)]
            }
            said.append(line)
        }
        return said.joined(separator: L10n[.Common.separatorList])
    }

    private func grantRow(_ key: AccessKey, _ project: ProjectSummary) -> some View {
        let isPublic = model.publicProjects.contains(project.id)
        return VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 8) {
                Image(systemName: isPublic ? "globe" : "lock")
                    .font(.app(10.5)).foregroundStyle(.secondary).frame(width: 14)
                Text(project.name).font(.app(12)).lineLimit(1)
                if isPublic {
                    Text(L10n[.MCP.keysPublicNote]).font(.app(10)).foregroundStyle(.tertiary)
                }
                Spacer()
                Picker("", selection: Binding(
                    get: { grants[project.id] },
                    set: { level in act { try $0.setGrant(keyId: key.id, projectId: project.id, level) } })) {
                    Text(L10n[.Common.wordNone]).tag(AccessLevel?.none)
                    Text(L10n[.MCP.keysLevelRead]).tag(AccessLevel?.some(.read))
                    Text(L10n[.MCP.keysLevelWrite]).tag(AccessLevel?.some(.write))
                }
                .pickerStyle(.segmented).labelsHidden().fixedSize()
            }
            .padding(.vertical, 6)
            Divider()
        }
    }

    // MARK: - Reading and writing

    private struct Read {
        var keys: [AccessKey] = []
        var counts: [String: Int] = [:]
        var selected: String?
        var grants: [String: AccessLevel] = [:]
        var error: String?
    }

    /// The keys, how many projects each was given, and the chosen one's grants — the
    /// first key's when the chosen one is gone.
    private static func read(_ model: AppModel, place: UUID, selected: String?) -> Read {
        var read = Read()
        guard let core = model.core(at: place) else { return read }
        do {
            let rows = try core.accessKeys()
            read.keys = rows.map(\.key)
            for row in rows { read.counts[row.id] = row.levels.count }
            read.selected = read.keys.contains { $0.id == selected } ? selected : read.keys.first?.id
            read.grants = rows.first { $0.id == read.selected }?.levels ?? [:]
        } catch {
            read.error = ErrorText.said(error)
        }
        return read
    }

    private func reload() {
        let read = Self.read(model, place: place, selected: selected)
        keys = read.keys
        counts = read.counts
        selected = read.selected
        grants = read.grants
        renaming = keys.first { $0.id == selected }?.name ?? ""
        error = read.error
    }

    /// Read again for what another hand changed: the name being written in is left as it is, while
    /// the same key stays chosen.
    private func refresh() {
        let read = Self.read(model, place: place, selected: selected)
        if read.selected != selected { renaming = read.keys.first { $0.id == read.selected }?.name ?? "" }
        keys = read.keys
        counts = read.counts
        selected = read.selected
        grants = read.grants
        error = read.error
    }

    private func act(_ body: (CoreClient) throws -> Void) {
        guard let core = model.core(at: place) else { return }
        do {
            try body(core)
            reload()
        } catch {
            self.error = ErrorText.said(error)
        }
    }

    /// `2026-09-30 16:24` in this Mac's time zone, from a stored time — the same
    /// whatever language the Mac is set to.
    static func when(_ stamp: String) -> String {
        let reader = ISO8601DateFormatter()
        reader.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        guard let date = reader.date(from: stamp) else { return stamp }
        let writer = DateFormatter()
        writer.locale = Locale(identifier: "en_US_POSIX")
        writer.dateFormat = "yyyy-MM-dd HH:mm"
        return writer.string(from: date)
    }
}

/// What the access keys page shows: the line under each key's name, by key, and the projects the
/// chosen key may touch — for a test to read with `onPreferenceChange`, as nothing in the window needs
/// them.
struct AccessKeysShown: PreferenceKey {
    struct Value: Equatable {
        var lines: [String: String] = [:]
        /// Project → `read` or `write`.
        var grants: [String: String] = [:]
    }

    static let defaultValue = Value()

    static func reduce(value: inout Value, nextValue: () -> Value) {
        let next = nextValue()
        if next != Value() { value = next }
    }
}
