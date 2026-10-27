import SwiftUI

/// What MCP may do in the project, in its settings (`specboard://SpecBoard/Docs/Design:DesignAccess`
/// §6): an item for each tool that writes, by group (`card_*`…), each 許可, 都度承認 (the call asks a
/// person on the window, `request: true`) or 拒否. A group folds; its choice sets every item of it, and
/// says nothing while they differ. Only a person changes it — MCP reads it
/// (`project_read`'s `mcpAsk`, `mcpDenied`) — and it holds for every access key that may write the project;
/// which projects a key may write, and what it may do with the window, are the key's (アクセスキーの設定).
/// The window's own writes are never held to it. Read and set through the core (`mcp_writes`,
/// `mcp_writes_set`); a change is a line in main's history, under main's lock too.
struct MCPWritesView: View {
    /// What a project of this Mac's is told here: MCP does not reach it.
    static var thisMac: String { L10n[.MCP.writesThisMac] }

    var model: AppModel

    @State private var groups: [MCPWriteGroup] = []
    @State private var open: Set<String> = []
    @State private var error: String?

    var body: some View {
        if let project = model.detail?.project {
            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    VStack(alignment: .leading, spacing: 6) {
                        Text(L10n[.MCP.writesTitle]).font(.app(15, weight: .semibold))
                        Text(L10n[.MCP.writesIntro])
                            .font(.app(11)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                            .readingWidth()
                    }
                    // MCP reaches only an agent's projects.
                    if model.projects.first(where: { $0.id == project.id })?.place == nil {
                        Text(Self.thisMac)
                            .font(.app(11.5)).foregroundStyle(.orange)
                            .fixedSize(horizontal: false, vertical: true)
                            .readingWidth()
                    }
                    if let error {
                        Text(error).font(.app(11)).foregroundStyle(.red)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                    ForEach(groups) { group in groupBox(group, project: project.id) }
                }
                .padding(20)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .disabled(model.isArchived)
            .onAppear { load(project.id, opening: true) }
            .onChange(of: project.id) { _, id in load(id, opening: true) }
            // Read again whenever the project is: a project file read in its place brings its own, and a
            // person's "always" on a question sets it.
            .onChange(of: project.updatedAt) { load(project.id, opening: false) }
            // The items' names and what they are for are the core's, said in the screen's language.
            .onChange(of: Language.shared.code) { load(project.id, opening: false) }
        }
    }

    private func groupBox(_ group: MCPWriteGroup, project: String) -> some View {
        let isOpen = open.contains(group.id)
        return VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .center, spacing: 8) {
                Button {
                    if isOpen { open.remove(group.id) } else { open.insert(group.id) }
                } label: {
                    HStack(spacing: 6) {
                        Image(systemName: isOpen ? "chevron.down" : "chevron.right")
                            .font(.app(10, weight: .semibold)).foregroundStyle(.secondary).frame(width: 12)
                        Text(group.name).font(.app(12.5, weight: .semibold))
                        Text(group.items.filter { !$0.isPart }.map(\.id).joined(separator: L10n[.Common.separatorList]))
                            .font(.app(10, design: .monospaced)).foregroundStyle(.tertiary).lineLimit(1)
                    }
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .help(isOpen ? L10n[.MCP.writesCollapse] : L10n[.MCP.writesExpand])
                Spacer()
                if group.state == nil {
                    Text(L10n[.MCP.writesPerItem]).font(.app(10.5, weight: .medium)).foregroundStyle(.orange)
                        .padding(.horizontal, 6).padding(.vertical, 1)
                        .background(Capsule().fill(Color.orange.opacity(0.12)))
                }
                MCPStateChoice(state: group.state) { state in
                    write { try $0.setMCPWrite(project, group: group.id, state) }
                }
                .help(L10n[.MCP.writesGroupHelp(group.name)])
            }
            if !group.about.isEmpty {
                Text(group.about).font(.app(10.5)).foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                    .readingWidth()
            }
            if isOpen {
                VStack(alignment: .leading, spacing: 0) {
                    ForEach(group.items) { item in itemRow(item, project: project) }
                }
                .padding(.leading, 18)
            }
        }
        .padding(12)
        .background(RoundedRectangle(cornerRadius: 8).fill(Color.secondary.opacity(0.05)))
    }

    private func itemRow(_ item: MCPWriteItem, project: String) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(alignment: .center, spacing: 8) {
                VStack(alignment: .leading, spacing: 2) {
                    HStack(alignment: .firstTextBaseline, spacing: 6) {
                        MCPItemID(id: item.id)
                        Text(item.name).font(.app(12))
                        if item.byDefault != .allow {
                            Text(L10n[.MCP.writesDefaultIs(item.byDefault.title)]).font(.app(10)).foregroundStyle(.tertiary)
                        }
                    }
                    if !item.about.isEmpty {
                        Text(item.about).font(.app(10.5)).foregroundStyle(.secondary)
                            .fixedSize(horizontal: false, vertical: true)
                            .readingWidth()
                    }
                }
                .padding(.leading, item.isPart ? 16 : 0)
                Spacer()
                MCPStateChoice(state: item.state) { state in
                    write { try $0.setMCPWrite(project, item: item.id, state) }
                }
                .controlSize(.small)
            }
            .padding(.vertical, 6)
            Divider()
        }
    }

    // MARK: - Reading and writing

    private func load(_ project: String, opening: Bool) {
        guard let core = model.core else { return }
        do {
            groups = try core.mcpWrites(project)
            // Open at first: the groups whose items differ.
            if opening { open = Set(groups.filter { $0.state == nil }.map(\.id)) }
            error = nil
        } catch {
            self.error = ErrorText.said(error)
        }
    }

    /// A change, and the groups as the core answers them after it.
    private func write(_ body: (CoreClient) throws -> [MCPWriteGroup]) {
        guard let core = model.core else { return }
        do {
            groups = try body(core)
            error = nil
        } catch {
            self.error = ErrorText.said(error)
        }
    }
}

extension MCPState {
    /// As the window says it, in the screen's language.
    var title: String {
        switch self {
        case .allow: L10n[.MCP.stateAllow]
        case .ask: L10n[.MCP.stateAsk]
        case .deny: L10n[.MCP.stateDeny]
        }
    }
}

/// 許可・都度承認・拒否, one chosen at once: an item of MCP's, a group of them, or a key's. None shown while
/// what it stands for differs (`state` nil).
struct MCPStateChoice: View {
    var state: MCPState?
    var set: (MCPState) -> Void

    var body: some View {
        Picker("", selection: Binding(get: { state }, set: { if let chosen = $0 { set(chosen) } })) {
            ForEach(MCPState.allCases) { choice in
                Text(choice.title).tag(Optional(choice))
            }
        }
        .pickerStyle(.segmented)
        .labelsHidden()
        .fixedSize()
    }
}

/// An item's id (`project_update.main_tree`) on one line, however narrow the row: it is read whole and
/// copied into MCP calls, so the name and the default give way to it, not it to them.
struct MCPItemID: View {
    var id: String

    var body: some View {
        Text(id).font(.app(11, design: .monospaced)).lineLimit(1).fixedSize(horizontal: true, vertical: false)
    }
}
