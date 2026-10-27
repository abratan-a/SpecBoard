import AppKit
import SwiftUI
import UniformTypeIdentifiers

/// The projects, over the whole window (overview §13): where one is chosen and where
/// they are kept, by where they are kept — the places (this Mac, and each agent) at the left, the
/// one chosen at the right.
///
/// Choosing and managing — switching, making, renaming and deleting — are one
/// place. It opens from the project's name above, the nav panel's 「プロジェクト一覧」, or when
/// no project is open: a click opens a project and leaves it (`onOpen`), and everything
/// else about a project hangs off its row.
struct ProjectHome: View {
    var model: AppModel
    /// The agents, beside this Mac: the places projects are kept in.
    var hosts: RemoteHosts
    /// Called once a project has been opened from here — picked, or made — so the
    /// window can shut the sidebar again.
    var onOpen: () -> Void = {}

    @State private var creating = false
    /// Where the project being made goes: this Mac (nil), or the agent on a host.
    @State private var creatingIn: UUID?
    @State private var draftName = ""
    @State private var draftDescription = ""
    /// The row whose details are open in a popover.
    @State private var inspecting: String?
    @State private var hovered: String?
    @State private var confirmingDelete: ProjectSummary?
    /// A project to be moved, and where: this Mac (nil) or an agent.
    @State private var moving: (project: ProjectSummary, to: UUID?)?
    /// A project file whose project is here already: replace it, or keep both.
    @State private var reading: (text: String, name: String)?
    /// The places whose archive is open ("" for this Mac's).
    @State private var archiveOpen: Set<String> = []

    private var openId: String? { model.detail?.project.id }

    /// Narrows the list by name and description, the way the schema list does.
    @State private var query = ""

    /// The place the right shows: the one chosen at the left, while it is still one of the agents.
    private var place: UUID? {
        model.homePlace.flatMap { id in hosts.hosts.contains { $0.id == id } ? id : nil }
    }

    /// The place's projects, narrowed by what is typed.
    private var shown: [ProjectSummary] {
        let here = model.projects.filter { $0.place == place }
        let needle = query.trimmingCharacters(in: .whitespaces).lowercased()
        guard !needle.isEmpty else { return here }
        return here.filter {
            $0.name.lowercased().contains(needle) || $0.description.lowercased().contains(needle)
        }
    }

    /// Worked on, and set aside: the archive is its own group, closed until wanted.
    private var working: [ProjectSummary] { shown.filter { !$0.isArchived } }
    private var archived: [ProjectSummary] { shown.filter(\.isArchived) }

    /// Whether the place is reached: this Mac always, an agent while it is connected.
    private var isReached: Bool {
        place.map { hosts.coreState[$0]?.connected ?? false } ?? true
    }

    /// Two columns: the places at the left, and at the right the one chosen — its projects, the
    /// archive closed until wanted, or its settings.
    var body: some View {
        HStack(spacing: 0) {
            PlaceColumn(model: model, hosts: hosts, selected: place,
                        onChoose: { choose($0, settings: nil) },
                        onSettings: { choose($0, settings: PlaceSettingsTab.opening($0)) })
                .frame(width: 244)
            Divider()
            VStack(spacing: 0) {
                placeHeader
                Divider()
                placeBody
            }
            .frame(maxWidth: .infinity)
        }
        .frame(maxWidth: .infinity)
        .confirmationDialog(
            L10n[.ProjectHome.deleteTitle(confirmingDelete?.name ?? "")],
            isPresented: Binding(get: { confirmingDelete != nil },
                                 set: { if !$0 { confirmingDelete = nil } }),
            titleVisibility: .visible
        ) {
            Button(L10n[.Common.actionDeleteItem], role: .destructive) {
                if let project = confirmingDelete {
                    inspecting = nil
                    model.deleteProject(project.id)
                }
                confirmingDelete = nil
            }
            Button(L10n[.Common.actionCancel], role: .cancel) { confirmingDelete = nil }
        } message: {
            Text(L10n[.ProjectHome.deleteMessage(confirmingDelete?.schemaCount ?? 0)])
        }
        .confirmationDialog(
            L10n[.ProjectHome.moveTitle(moving?.project.name ?? "", placeName(moving?.to))],
            isPresented: Binding(get: { moving != nil }, set: { if !$0 { moving = nil } }),
            titleVisibility: .visible
        ) {
            Button(L10n[.ProjectHome.moveConfirm]) {
                if let moving { model.moveProject(moving.project.id, to: moving.to) }
                moving = nil
            }
            Button(L10n[.Common.actionCancel], role: .cancel) { moving = nil }
        } message: {
            Text(L10n[.ProjectHome.moveMessage(placeName(moving?.project.place))])
        }
        .confirmationDialog(
            L10n[.ProjectHome.readExists(reading?.name ?? "")],
            isPresented: Binding(get: { reading != nil }, set: { if !$0 { reading = nil } }),
            titleVisibility: .visible
        ) {
            Button(L10n[.ProjectHome.readReplace]) {
                if let reading { model.readProjectFile(reading.text, copy: false); onOpen() }
                reading = nil
            }
            Button(L10n[.ProjectHome.readAsCopy]) {
                if let reading { model.readProjectFile(reading.text, copy: true); onOpen() }
                reading = nil
            }
            Button(L10n[.Common.actionCancel], role: .cancel) { reading = nil }
        } message: {
            Text(L10n[.ProjectHome.readExistsMessage])
        }
    }

    // MARK: - Files

    /// A project file, chosen and read in.
    private func readFile() {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [.json]
        panel.message = L10n[.ProjectHome.readPanelMessage]
        guard panel.runModal() == .OK, let url = panel.url else { return }
        guard let text = try? String(contentsOf: url, encoding: .utf8) else {
            model.tell(error: { L10n.text(.ProjectHome.readCantRead(url.lastPathComponent), in: $0) })
            return
        }
        guard let found = model.peekProjectFile(text) else {
            model.tell(error: { L10n.text(.ProjectHome.readNotProject(url.lastPathComponent), in: $0) })
            return
        }
        if model.projectExists(found.id) {
            reading = (text, found.name)
        } else {
            model.readProjectFile(text, copy: false)
            onOpen()
        }
    }

    /// The project as a file, saved where the person says.
    private func writeFile(_ project: ProjectSummary) {
        guard let text = model.projectFile(project.id) else { return }
        let panel = NSSavePanel()
        panel.allowedContentTypes = [.json]
        panel.nameFieldStringValue = "\(project.slug).project.json"
        guard panel.runModal() == .OK, let url = panel.url else { return }
        do {
            try text.write(to: url, atomically: true, encoding: .utf8)
        } catch {
            model.tell(error: { L10n.text(.ProjectHome.writeFailed(ErrorText.said(error)), in: $0) })
        }
    }

    // MARK: - The places

    private func choose(_ chosen: UUID?, settings: PlaceSettingsTab?) {
        model.homePlace = chosen
        model.homeSettings = settings
        inspecting = nil
    }

    /// What the place is called at the head of the right: this Mac, or the agent's name.
    private var headerName: String {
        guard let place else { return L10n[.Common.wordThisMac] }
        return hosts.hosts.first { $0.id == place }?.displayName ?? model.agentNames[place] ?? L10n[.Common.wordAgent]
    }

    /// The right's head: which place, its projects or its settings, and — for the projects — a
    /// filter, reading a file in and making a project in it.
    private var placeHeader: some View {
        HStack(spacing: 10) {
            Text(headerName).font(.app(15, weight: .semibold)).lineLimit(1).truncationMode(.middle)
            // As wide as its segments need, the name giving way (cut with …) rather than it: held to 170
            // points, narrower than the control needs (190 here, more on a later macOS), it was drawn past
            // its place over the end of the name.
            Picker("", selection: Binding(
                get: { model.homeSettings != nil },
                set: { model.homeSettings = $0 ? PlaceSettingsTab.opening(place) : nil })) {
                Text(L10n[.ProjectHome.headerProjects]).tag(false)
                Text(L10n[.Common.wordSettings]).tag(true)
            }
            .pickerStyle(.segmented).labelsHidden().fixedSize()
            if model.homeSettings == nil, isReached {
                Image(systemName: "magnifyingglass")
                    .font(.app(10)).foregroundStyle(.secondary).padding(.leading, 6)
                TextField(L10n[.ProjectHome.headerFilter], text: $query)
                    .textFieldStyle(.plain).font(.app(12))
                if !query.isEmpty {
                    Button { query = "" } label: { Image(systemName: "xmark.circle.fill") }
                        .buttonStyle(.plain).foregroundStyle(.secondary).font(.app(10))
                }
                Text("\(working.count)")
                    .font(.app(10.5, design: .monospaced)).foregroundStyle(.secondary)
                if place == nil {
                    Button { readFile() } label: { Image(systemName: "square.and.arrow.down") }
                        .buttonStyle(.borderless)
                        .help(L10n[.ProjectHome.headerImportHelp])
                }
                Button { newProject(in: place) } label: { Image(systemName: "plus") }
                    .buttonStyle(.borderless)
                    .help(L10n[.ProjectHome.newTitleIn(headerName)])
                    .popover(isPresented: $creating, arrowEdge: .trailing) { newProjectForm }
            } else {
                Spacer()
            }
        }
        .frame(minHeight: 28)
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
    }

    /// The tab of the place's settings shown: the one asked for, when the place has it.
    private var settingsTab: PlaceSettingsTab {
        let tabs = PlaceSettingsTab.of(place: place)
        return model.homeSettings.flatMap { tabs.contains($0) ? $0 : nil } ?? PlaceSettingsTab.opening(place)
    }

    @ViewBuilder
    private var placeBody: some View {
        if model.homeSettings != nil {
            PlaceSettingsPane(model: model, hosts: hosts, place: place,
                              tab: Binding(get: { settingsTab }, set: { model.homeSettings = $0 }))
        } else if !isReached, let host = hosts.hosts.first(where: { $0.id == place }) {
            notReached(host)
        } else if shown.isEmpty {
            empty
        } else {
            list
        }
    }

    /// An agent not connected: why, in place of its projects.
    private func notReached(_ host: RemoteHosts.Host) -> some View {
        let core = hosts.coreState[host.id] ?? RemoteHosts.CoreState()
        let said = PlaceColumn.status(host, core)
        return VStack(spacing: 10) {
            Text(L10n[.ProjectHome.notReachedText]).foregroundStyle(.secondary)
            Text(said).font(.app(11.5)).foregroundStyle(.secondary).textSelection(.enabled)
            HStack(spacing: 10) {
                if !host.enabled { Button(L10n[.ProjectHome.notReachedConnect]) { hosts.setEnabled(host, true) } }
                Button(L10n[.ProjectHome.notReachedSettings]) { choose(host.id, settings: .connection) }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    @ViewBuilder
    private var empty: some View {
        if !query.trimmingCharacters(in: .whitespaces).isEmpty {
            Text(L10n[.ProjectHome.emptyNoMatch])
                .font(.app(12)).foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else {
            VStack(spacing: 10) {
                Text(L10n[.ProjectHome.emptyNone]).foregroundStyle(.secondary)
                Button(L10n[.ProjectHome.emptyCreate]) { newProject(in: place) }
                if place == nil {
                    Button(L10n[.ProjectHome.emptyImport]) { readFile() }
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }

    // MARK: - The list

    /// A click opens the project. The projects are the place's; its archive is a group below them.
    private var list: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 2) {
                ForEach(working, id: \.id) { project in rowCell(project) }
                if !archived.isEmpty {
                    let key = place?.uuidString ?? ""
                    DisclosureGroup(isExpanded: Binding(
                        get: { archiveOpen.contains(key) },
                        set: { if $0 { archiveOpen.insert(key) } else { archiveOpen.remove(key) } })) {
                        ForEach(archived, id: \.id) { project in rowCell(project) }
                    } label: {
                        Label(L10n[.ProjectHome.listArchive(archived.count)], systemImage: "archivebox")
                            .font(.app(11)).foregroundStyle(.secondary)
                            .help(L10n[.ProjectHome.listArchiveHelp])
                    }
                    .padding(.horizontal, 10).padding(.top, 14)
                }
            }
            .padding(.horizontal, 6).padding(.vertical, 10)
        }
        .preference(key: ProjectHomeShown.self, value: ProjectHomeShown.Value(projects: working.map(\.name)))
    }

    private func newProject(in place: UUID?) {
        draftName = ""; draftDescription = ""; creatingIn = place; creating = true
    }

    private func rowCell(_ project: ProjectSummary) -> some View {
        row(project)
            .padding(.horizontal, 4)
            .contentShape(Rectangle())
            // Read off the main thread, the row saying so meanwhile; pressed again, nothing more.
            .onTapGesture { model.open(fromHome: project.id, opened: onOpen) }
    }

    private func row(_ project: ProjectSummary) -> some View {
        let isOpen = project.id == openId
        return VStack(alignment: .leading, spacing: 2) {
            HStack(spacing: 6) {
                if project.isArchived {
                    Image(systemName: "archivebox").font(.app(10)).foregroundStyle(.secondary)
                }
                if project.place != nil, model.publicProjects.contains(project.id) {
                    Image(systemName: "globe").font(.app(10)).foregroundStyle(.secondary)
                        .help(L10n[.ProjectHome.rowPublicHelp])
                }
                Text(project.name)
                    .font(.app(14, weight: .semibold))
                    .foregroundStyle(project.isArchived ? .secondary : .primary)
                    .lineLimit(1)
                if isOpen {
                    Text(L10n[.ProjectHome.rowIsOpen])
                        .font(.app(10))
                        .padding(.horizontal, 6).padding(.vertical, 1)
                        .background(Color.accentColor.opacity(0.15), in: Capsule())
                } else if model.opening?.id == project.id {
                    // Being opened: a big project of an agent's takes a while to read.
                    // Said as the window says it starting up: 「開いています」 is the one open.
                    ProgressView().controlSize(.small)
                    Text(L10n[.ProjectHome.rowLoading]).font(.app(10.5)).foregroundStyle(.secondary)
                }
                Spacer(minLength: 4)
                // Only where it can be aimed at: on every row at once it would be a
                // column of identical icons, read as part of the data.
                Button {
                    inspecting = project.id
                } label: {
                    Image(systemName: "info.circle").font(.app(11))
                }
                .buttonStyle(.borderless)
                .help(L10n[.ProjectHome.rowInfoHelp])
                .opacity(isOpen || hovered == project.id || inspecting == project.id ? 1 : 0)
                .popover(isPresented: Binding(
                    get: { inspecting == project.id },
                    set: { if !$0, inspecting == project.id { inspecting = nil } }),
                         arrowEdge: .trailing) {
                    ProjectDetails(model: model, projectId: project.id) {
                        confirmingDelete = $0
                    }
                }
                Text(L10n[.ProjectHome.rowStats(project.schemaCount, String(project.updatedAt.prefix(10)))])
                    .font(.app(10.5, design: .monospaced)).foregroundStyle(.tertiary)
            }
            if !project.description.isEmpty {
                Text(project.description)
                    .font(.app(11.5)).foregroundStyle(.secondary)
                    .lineLimit(2)
            }
        }
        .padding(.vertical, 7)
        .padding(.horizontal, 10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(RoundedRectangle(cornerRadius: 6)
            .fill(isOpen ? Color.accentColor.opacity(0.16)
                  : hovered == project.id ? Color.secondary.opacity(0.08) : .clear))
        .onHover { inside in
            if inside { hovered = project.id } else if hovered == project.id { hovered = nil }
        }
        .contextMenu {
            Button(project.isArchived ? L10n[.ProjectHome.menuOpenReadOnly] : L10n[.Common.actionOpen]) { model.open(fromHome: project.id, opened: onOpen) }
                .disabled(isOpen || model.opening != nil)
            if project.isArchived {
                Button(L10n[.ProjectHome.menuRestore]) { model.restoreProject(project.id) }
            } else {
                Button(L10n[.ProjectHome.menuEditNameDescription]) { inspecting = project.id }
            }
            Button(L10n[.ProjectHome.menuExportFile]) { writeFile(project) }
            moveItems(project)
            if !project.isArchived {
                Button(L10n[.ProjectHome.menuRunExports]) { model.executeExportsAndSay(project.id) }
            }
            // Who may read it through MCP: every access key of its agent, or those given it. Not this
            // Mac's: MCP reaches none of them.
            if project.place != nil {
                if model.publicProjects.contains(project.id) {
                    Button(L10n[.ProjectHome.menuMakePrivate]) { model.setPublic(project.id, false) }
                } else {
                    Button(L10n[.ProjectHome.menuMakePublic]) { model.setPublic(project.id, true) }
                }
            }
            Divider()
            if !project.isArchived {
                Button(L10n[.ProjectHome.menuArchive]) { model.archiveProject(project.id) }
            }
            Button(L10n[.ProjectHome.menuDelete], role: .destructive) { confirmingDelete = project }
        }
    }

    // MARK: - Moving one

    /// Where a project can be moved: this Mac, and each connected agent but its own place.
    @ViewBuilder
    private func moveItems(_ project: ProjectSummary) -> some View {
        let agents = model.agentNames.sorted { $0.value < $1.value }.filter { $0.key != project.place }
        if project.place != nil {
            Button(L10n[.ProjectHome.moveToThisMac]) { moving = (project, nil) }
        }
        if !agents.isEmpty {
            Menu(project.place == nil ? L10n[.ProjectHome.moveToAgent] : L10n[.ProjectHome.moveToOtherAgent]) {
                ForEach(agents, id: \.key) { id, name in
                    Button(name) { moving = (project, id) }
                }
            }
        }
    }

    private func placeName(_ place: UUID?) -> String {
        place.map { model.agentNames[$0] ?? L10n[.Common.wordAgent] } ?? L10n[.Common.wordThisMac]
    }

    // MARK: - Making one

    /// The place the project being made goes to, by name.
    private var creatingName: String {
        creatingIn.flatMap { place in hosts.hosts.first { $0.id == place } }?.displayName ?? L10n[.Common.wordThisMac]
    }

    private var newProjectForm: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(L10n[.ProjectHome.newTitleIn(creatingName)])
                .font(.app(12, weight: .semibold))
            TextField(L10n[.ProjectHome.newName], text: $draftName)
                .textFieldStyle(.roundedBorder).frame(width: 280)
            TextField(L10n[.ProjectHome.newDescription], text: $draftDescription)
                .textFieldStyle(.roundedBorder).frame(width: 280)
            HStack {
                Spacer()
                Button(L10n[.Common.actionCreate]) {
                    model.createProject(name: draftName, description: draftDescription, in: creatingIn)
                    if model.lastError == nil { creating = false; onOpen() }
                }
                .keyboardShortcut(.defaultAction)
                .disabled(draftName.trimmingCharacters(in: .whitespaces).isEmpty)
            }
        }
        .padding(14)
    }
}

/// The nav panel (ナビパネル), on the left of the window: the open project as a tree. Its
/// 「プロジェクト一覧」 goes to the projects over the whole window (`ProjectHome`).
struct NavPanel: View {
    var model: AppModel
    /// Which schemas have a card right now, for the schema list.
    var drawn: Set<String>

    var body: some View {
        NavPanelTreeView(model: model, drawn: drawn, onProjects: { model.atHome = true })
    }
}

/// One project's name, description and facts, opened from its row.
///
/// Read from the model by id rather than handed a copy, so a rename shows here and
/// in the list at once.
private struct ProjectDetails: View {
    var model: AppModel
    var projectId: String
    var onDelete: (ProjectSummary) -> Void

    var body: some View {
        if let project = model.projects.first(where: { $0.id == projectId }) {
            VStack(alignment: .leading, spacing: 14) {
                HStack(spacing: 10) {
                    Text(project.name).font(.app(14, weight: .semibold)).lineLimit(1)
                    if project.id == model.detail?.project.id {
                        Text(L10n[.ProjectHome.rowIsOpen])
                            .font(.app(10))
                            .padding(.horizontal, 6).padding(.vertical, 2)
                            .background(Color.accentColor.opacity(0.15), in: Capsule())
                    } else {
                        Button(L10n[.Common.actionOpen]) { model.open(fromHome: project.id) }.controlSize(.small)
                            .disabled(model.opening != nil)
                    }
                    Spacer()
                }
                field(L10n[.Common.wordName]) {
                    CommitField(L10n[.ProjectHome.detailsNameField], text: project.name) {
                        model.renameProject(project.id, to: $0)
                    }
                }
                field(L10n[.Common.wordDescription]) {
                    CommitTextEditor(text: project.description, minHeight: 80) {
                        model.updateProjectDescription(project.id, $0)
                    }
                }
                field("slug") {
                    CommitField("slug", text: project.slug, monospaced: true) {
                        model.updateProjectSlug(project.id, $0)
                    }
                    Text(L10n[.ProjectHome.detailsSlugHelp])
                        .font(.app(10)).foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                if project.id == model.detail?.project.id {
                    field(L10n[.Common.wordSection]) { SectionList(model: model) }
                } else if project.sectionCount > 1 {
                    fact(L10n[.Common.wordSection], L10n[.ProjectHome.detailsSections(project.sectionCount)])
                }
                facts(project)
                HStack {
                    Spacer()
                    Button(L10n[.ProjectHome.detailsDelete], role: .destructive) { onDelete(project) }
                        .controlSize(.small)
                }
            }
            .padding(16)
            .frame(width: 400, alignment: .leading)
            // Rebuilt per project, so the committed drafts start from this project's values.
            .id(project.id)
        }
    }

    private func facts(_ project: ProjectSummary) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            // Only for the project already in memory: reading another one whole just to
            // count its fields would cost as much as opening it.
            if project.id == model.detail?.project.id {
                let stats = model.stats
                fact(L10n[.ProjectHome.detailsSize],
                     L10n[.ProjectHome.detailsSizeFull(stats.schemas, stats.fields, stats.relations, stats.depth)])
            } else {
                fact(L10n[.ProjectHome.detailsSize], L10n[.ProjectHome.detailsSizeCards(project.schemaCount)])
            }
            fact(L10n[.ProjectHome.detailsCreated], short(project.createdAt))
            fact(L10n[.ProjectHome.detailsUpdated], short(project.updatedAt))
        }
    }

    private func fact(_ label: String, _ value: String) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 10) {
            Text(label)
                .font(.app(11)).foregroundStyle(.secondary)
                .frame(width: 52, alignment: .leading)
            Text(value)
                .font(.app(12))
                .textSelection(.enabled)
            Spacer()
        }
    }

    private func field<Content: View>(_ label: String,
                                      @ViewBuilder content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 5) {
            Text(label).font(.app(11)).foregroundStyle(.secondary)
            content()
        }
    }

    /// `2026-08-12T03:48:09.913Z` is not something to read off a screen.
    private func short(_ timestamp: String) -> String {
        let date = timestamp.prefix(10)
        let time = timestamp.dropFirst(11).prefix(5)
        return time.isEmpty ? String(date) : "\(date) \(time)"
    }
}

/// What the right of the projects' home shows: the chosen place's projects, what is said in place of a
/// place's settings, and what is public there — for a test to read with `onPreferenceChange`, as nothing
/// in the window needs them (as `AccessKeysShown`). One of them at a time: the right shows one.
struct ProjectHomeShown: PreferenceKey {
    struct Row: Equatable {
        var name: String
        var isPublic: Bool
    }

    struct Value: Equatable {
        /// The place's projects in its list, by name, as they are listed (the archive's apart).
        var projects: [String] = []
        /// What is said in place of a place's settings: this Mac's, which has none, or an agent's keys
        /// and what is public while it is not connected.
        var said: [String] = []
        /// The 「公開」 tab's rows: each of the place's projects, and whether it is public.
        var publicity: [Row] = []
    }

    static let defaultValue = Value()

    static func reduce(value: inout Value, nextValue: () -> Value) {
        let next = nextValue()
        if next != Value() { value = next }
    }
}
