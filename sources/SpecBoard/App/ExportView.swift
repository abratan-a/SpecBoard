import AppKit
import SwiftUI

/// The exports (書き出し; `specboard://SpecBoard/Docs/Design:Overview` §10), in the project's settings, its
/// 「エクスポート」 tab: each a name, what it writes from where (a card, or a
/// section's cards and those of the sections under it, into a file or a folder) and what it leaves
/// out — the project's settings, not a branch's, made, changed and let go here as MCP does with
/// `export_create`, `export_update` and `export_delete` — and, kept for the machine the project is
/// on, the folder it is written to by default (`Behavior/Screen:SetExportFolder`). Under each, the
/// files it writes with their cards and how each stands against what it wrote last (the core keeps
/// that beside them in the folder written to, `.specboard/<project>/<export>.json`), what writing
/// again takes away, and the write (`export_execute`) — main, whatever branch is open, into its
/// folder; or into one chosen for the once, the tree chosen too, the one open to begin with (the
/// default folders are main's).
struct ExportView: View {
    var model: AppModel

    @State private var making = false
    @State private var adding: String?
    @State private var leaving: String?
    @State private var renaming: String?
    @State private var deleting: String?
    @State private var problem: String?
    /// A folder being typed (an agent's host path): the export's default, or the folder written to once.
    @State private var naming: NameAsk?
    /// The tree written into a folder chosen for the once, when one is chosen: else the one open.
    @State private var chosenTree: String?

    var body: some View {
        if let detail = model.detail {
            VStack(alignment: .leading, spacing: 18) {
                header(detail)
                if let problem {
                    Text(problem).font(.app(11)).foregroundStyle(.red).textSelection(.enabled)
                        .fixedSize(horizontal: false, vertical: true)
                }
                if !model.exports.problems.isEmpty { problemList }
                if model.exports.exports.isEmpty {
                    Text(L10n[.Export.listEmpty])
                        .font(.app(11.5)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                        .readingWidth()
                }
                ForEach(model.exports.exports) { export in exportBlock(export, detail) }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            // Read again whenever the project changes: what each would write, and how it stands.
            .task(id: "\(model.dataGeneration)|\(model.workDirs.map { "\($0.id)=\($0.folder ?? "")" }.joined(separator: ","))") {
                model.readExportSettings()
            }
            .nameSheet($naming)
            .onChange(of: model.openBranch?.id) { chosenTree = nil }
            .sheet(isPresented: $making) {
                MakeExportSheet(model: model) { making = false }
            }
            .sheet(isPresented: Binding(get: { adding != nil }, set: { if !$0 { adding = nil } })) {
                if let adding {
                    EntrySheet(model: model, export: adding) { self.adding = nil }
                }
            }
            .sheet(isPresented: Binding(get: { leaving != nil }, set: { if !$0 { leaving = nil } })) {
                if let leaving {
                    ExcludeSheet(model: model, export: leaving) { self.leaving = nil }
                }
            }
            .sheet(isPresented: Binding(get: { renaming != nil }, set: { if !$0 { renaming = nil } })) {
                if let renaming {
                    RenameExportSheet(model: model, export: renaming) { self.renaming = nil }
                }
            }
            .confirmationDialog(
                L10n[.Export.deleteTitle(deleting ?? "")],
                isPresented: Binding(get: { deleting != nil }, set: { if !$0 { deleting = nil } }),
                titleVisibility: .visible
            ) {
                Button(L10n[.Common.actionDelete], role: .destructive) {
                    if let deleting { problem = model.deleteExport(deleting) }
                    deleting = nil
                }
                Button(L10n[.Common.actionCancel], role: .cancel) { deleting = nil }
            } message: {
                Text(L10n[.Export.deleteMessage])
            }
        }
    }

    private var editable: Bool { model.canEditExports }

    /// The folder an export is written to by default, chosen (this Mac's panel) or typed (an agent's host
    /// path): not the project's — this machine's.
    private func chooseFolder(for export: ExportRead) {
        if model.core?.isAgent ?? false {
            naming = NameAsk(title: L10n[.Export.folderAskTitle(model.openPlaceName, export.name)],
                             placeholder: "~/repo", start: export.path ?? "", confirm: L10n[.Export.folderDecide],
                             cancel: L10n[.Common.actionCancel]) { typed in
                problem = model.setExportFolder(export.name, to: typed)
            }
        } else if let chosen = FolderPanel.choose(
            message: L10n[.Export.folderPanelMessage(export.name)]) {
            problem = model.setExportFolder(export.name, to: chosen)
        }
    }

    /// The exports `names` (all, for none) of the tree chosen written once into another folder: chosen on
    /// this Mac, typed for an agent's host.
    private func writeElsewhere(_ projectId: String, names: [String]?) {
        let tree = Self.writeTree(chosenTree, of: model)
        let branch = tree.isEmpty ? nil : tree
        let said = Self.scope(branch: branch, names: names)
        if model.core?.isAgent ?? false {
            naming = NameAsk(title: L10n[.Export.elsewhereAskTitle(model.openPlaceName, said)],
                             message: L10n[.Export.elsewhereAskMessage],
                             placeholder: "~/repo/.claude/worktrees/x", confirm: L10n[.Export.actionExport],
                             cancel: L10n[.Common.actionCancel]) { typed in
                model.executeExportsAndSay(projectId, names: names, to: typed, branch: branch)
            }
        } else if let chosen = FolderPanel.choose(
            message: L10n[.Export.elsewherePanelMessage(said)]) {
            model.executeExportsAndSay(projectId, names: names, to: chosen, branch: branch)
        }
    }

    /// What is written to a folder chosen for the once, said inside a sentence: the exports `names` (all, for
    /// none) of main or of `branch`.
    private static func scope(branch: String?, names: [String]?) -> String {
        let listed = names.map { $0.joined(separator: L10n[.Common.separatorList]) }
        switch (branch, listed) {
        case (nil, nil): return L10n[.Export.scopeAllFromMain]
        case (let branch?, nil): return L10n[.Export.scopeAllFromBranch(branch)]
        case (nil, let listed?): return L10n[.Export.scopeNamesFromMain(listed)]
        case (let branch?, let listed?): return L10n[.Export.scopeNamesFromBranch(listed, branch)]
        }
    }

    /// The trees a folder chosen for the once may be written from: main (`""`) and the branches.
    static func writeTrees(of model: AppModel) -> [String] {
        [""] + model.branches.map(\.name).filter { !$0.hasPrefix("~") }
    }

    /// The tree a folder chosen for the once is written from: the one chosen while it is there, else the
    /// one open (main for the board).
    static func writeTree(_ chosen: String?, of model: AppModel) -> String {
        let trees = writeTrees(of: model)
        if let chosen, trees.contains(chosen) { return chosen }
        guard let open = model.openBranch?.name, trees.contains(open) else { return "" }
        return open
    }

    private var treePicker: some View {
        HStack(spacing: 4) {
            Text(L10n[.Export.treeLabel]).font(.app(11)).foregroundStyle(.secondary)
            Picker("", selection: Binding(get: { Self.writeTree(chosenTree, of: model) }, set: { chosenTree = $0 })) {
                ForEach(Self.writeTrees(of: model), id: \.self) { tree in
                    Text(tree.isEmpty ? SchemaDirectory.mainWord : tree).tag(tree)
                }
            }
            .labelsHidden().pickerStyle(.menu).fixedSize()
        }
        .help(L10n[.Export.treeHelp])
    }

    private func header(_ detail: ProjectDetail) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .firstTextBaseline) {
                Text(L10n[.Common.wordExport]).font(.app(15, weight: .semibold))
                Spacer()
                Button(L10n[.Export.headerNew]) { making = true }
                    .disabled(!editable)
                Button(L10n[.Export.actionExportAll]) { model.executeExportsAndSay(detail.project.id) }
                    .disabled(model.isArchived || model.exports.exports.isEmpty)
                    .help(L10n[.Export.headerExportAllHelp(model.openPlaceName)])
                treePicker
                Button(L10n[.Export.actionExportElsewhere]) { writeElsewhere(detail.project.id, names: nil) }
                    .disabled(model.isArchived || model.exports.exports.isEmpty)
                    .help(L10n[.Export.headerExportElsewhereHelp])
            }
            Text(L10n[.Export.headerAbout(model.openPlaceName)])
                .font(.app(11)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                .readingWidth()
        }
    }

    private var problemList: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(L10n[.Export.problemsTitle]).font(.app(11.5, weight: .semibold)).foregroundStyle(.red)
            ForEach(Array(model.exports.problems.enumerated()), id: \.offset) { _, line in
                Text(line).font(.app(11)).foregroundStyle(.red).textSelection(.enabled)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(10)
        .background(RoundedRectangle(cornerRadius: 6).fill(Color.red.opacity(0.06)))
    }

    @ViewBuilder
    private func exportBlock(_ export: ExportRead, _ detail: ProjectDetail) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Image(systemName: "square.and.arrow.up").foregroundStyle(.secondary)
                Text(export.name).font(.app(13, weight: .semibold, design: .monospaced))
                Text("→").foregroundStyle(.tertiary)
                Text(export.path ?? L10n[.Export.rowNoFolder(model.openPlaceName)])
                    .font(.app(11, design: export.path == nil ? .default : .monospaced))
                    .foregroundStyle(export.path == nil ? AnyShapeStyle(.orange) : AnyShapeStyle(.secondary))
                    .lineLimit(1).truncationMode(.middle).textSelection(.enabled)
                    .help(L10n[.Export.rowFolderHelp(model.openPlaceName)])
                Button(export.path == nil ? L10n[.Export.rowSetFolder] : L10n[.Export.rowChangeFolder]) { chooseFolder(for: export) }
                    .disabled(!editable)
                Spacer()
                Button(L10n[.Export.actionExport]) { model.executeExportsAndSay(detail.project.id, names: [export.name]) }
                    .disabled(model.isArchived || export.path == nil)
                Menu("…") {
                    Button(L10n[.Export.actionExportElsewhere]) { writeElsewhere(detail.project.id, names: [export.name]) }
                        .disabled(model.isArchived)
                    if export.path != nil {
                        Button(L10n[.Export.menuRemoveFolder]) { problem = model.setExportFolder(export.name, to: nil) }
                            .disabled(!editable)
                    }
                    Divider()
                    Button(L10n[.Common.actionRenameMenu]) { renaming = export.name }
                    Divider()
                    Button(L10n[.Export.menuDelete], role: .destructive) { deleting = export.name }
                }
                .menuStyle(.borderlessButton).fixedSize()
                .disabled(!editable)
            }
            section(L10n[.Export.sectionEntries]) {
                ForEach(export.entries, id: \.self) { entry in entryRow(export, entry, detail) }
                if editable {
                    Button(L10n[.Export.entryAdd]) { adding = export.name }.controlSize(.small)
                }
            }
            if !export.exclude.isEmpty || editable {
                section(L10n[.Export.sectionExclude]) {
                    ForEach(export.exclude, id: \.self) { path in
                        HStack(spacing: 6) {
                            pathLink(path, detail)
                            if editable {
                                Button { problem = model.updateExport(export.name, ["exclude": ["remove": [path]]]) } label: {
                                    Image(systemName: "xmark.circle").foregroundStyle(.tertiary)
                                }
                                .buttonStyle(.borderless).help(L10n[.Export.excludeRemoveHelp])
                            }
                        }
                    }
                    if editable {
                        Button(L10n[.Export.excludeAdd]) { leaving = export.name }.controlSize(.small)
                    }
                }
            }
            section(L10n[.Export.sectionFiles]) {
                if export.files.isEmpty {
                    Text(L10n[.Export.filesNone]).font(.app(11)).foregroundStyle(.tertiary)
                } else {
                    VStack(alignment: .leading, spacing: 3) {
                        ForEach(Self.tree(export.files)) { row in treeRow(row, detail) }
                    }
                }
                if !export.removing.isEmpty {
                    Text(L10n[.Export.filesRemoving(export.removing.joined(separator: L10n[.Common.separatorList]))])
                        .font(.app(11)).foregroundStyle(.orange).fixedSize(horizontal: false, vertical: true)
                        .readingWidth()
                }
                if let note = export.note {
                    Text(note).font(.app(10.5)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                        .readingWidth()
                } else if let at = export.writtenAt {
                    Text(L10n[.Export.filesWrittenAt(at)]).font(.app(10.5)).foregroundStyle(.tertiary)
                }
            }
        }
        .padding(12)
        .background(RoundedRectangle(cornerRadius: 8).stroke(Color.secondary.opacity(0.25)))
    }

    private func section<Content: View>(_ title: String, @ViewBuilder content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title).font(.app(11, weight: .semibold)).foregroundStyle(.secondary)
            content().padding(.leading, 8)
        }
    }

    private func entryRow(_ export: ExportRead, _ entry: ExportRead.Entry, _ detail: ProjectDetail) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 6) {
            pathLink(entry.from, detail)
            Text("→").foregroundStyle(.tertiary)
            Text(entry.to).font(.app(11.5, design: .monospaced)).textSelection(.enabled)
            if let format = entry.format {
                Text(ExportWords.formatLabel(format)).font(.app(9.5)).foregroundStyle(.secondary)
            }
            if let kinds = entry.kinds, !kinds.isEmpty {
                Text(kinds.joined(separator: L10n[.Common.separatorList])).font(.app(9.5)).foregroundStyle(.secondary)
                    .padding(.horizontal, 4).overlay(RoundedRectangle(cornerRadius: 3).stroke(.tertiary))
            }
            Spacer(minLength: 4)
            if editable {
                Button { problem = model.updateExport(export.name, ["entries": ["remove": [["from": entry.from, "to": entry.to]]]]) } label: {
                    Image(systemName: "xmark.circle").foregroundStyle(.tertiary)
                }
                .buttonStyle(.borderless).help(L10n[.Export.entryRemoveHelp])
            }
        }
    }

    /// A card's or a section's path: shown where it is drawn, when it is there.
    @ViewBuilder
    private func pathLink(_ path: String, _ detail: ProjectDetail) -> some View {
        if let card = detail.directory.schema(path, from: nil) {
            Button(path) { model.showCard(card.id) }.buttonStyle(.link).font(.app(11.5, design: .monospaced))
        } else {
            Text(path).font(.app(11.5, design: .monospaced)).foregroundStyle(.primary).textSelection(.enabled)
        }
    }

    @ViewBuilder
    private func treeRow(_ row: TreeRow, _ detail: ProjectDetail) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 6) {
            Color.clear.frame(width: CGFloat(row.depth) * 16, height: 1)
            if let file = row.file {
                Image(systemName: "doc.text").font(.app(10)).foregroundStyle(.secondary)
                Text(row.name).font(.app(12, design: .monospaced))
                Text(ExportWords.label(file.content)).font(.app(9.5)).foregroundStyle(.secondary)
                    .padding(.horizontal, 4).padding(.vertical, 1)
                    .overlay(RoundedRectangle(cornerRadius: 3).stroke(.tertiary))
                if let status = file.status { ExportStatusTag(status: status) }
                FlowRow(spacing: 6, lineSpacing: 2) {
                    ForEach(file.cards, id: \.self) { path in
                        let changed = file.changedCards?.contains(path) == true
                        if let card = detail.directory.schema(path, from: nil) {
                            // Shown where it is drawn; its editor opens on a press twice there.
                            Button(path) { model.showCard(card.id) }
                                .buttonStyle(.link)
                                .font(.app(11, weight: changed ? .semibold : .regular, design: .monospaced))
                                .help(changed ? L10n[.Export.filesChangedCardHelp] : "")
                        } else {
                            Text(path).font(.app(11, design: .monospaced)).foregroundStyle(.secondary)
                        }
                    }
                }
            } else {
                Image(systemName: "folder").font(.app(10)).foregroundStyle(.tertiary)
                Text(row.name).font(.app(12, design: .monospaced)).foregroundStyle(.secondary)
            }
        }
    }

    // MARK: - Worked out

    struct TreeRow: Identifiable, Hashable {
        var id: String
        var depth: Int
        var name: String
        var file: ExportRead.File?
    }

    /// The files as a tree: each folder once, above what is in it.
    static func tree(_ files: [ExportRead.File]) -> [TreeRow] {
        var out: [TreeRow] = []
        var seen: Set<String> = []
        for file in files.sorted(by: { $0.path.localizedStandardCompare($1.path) == .orderedAscending }) {
            let parts = file.path.split(separator: "/").map(String.init)
            for depth in 0..<max(parts.count - 1, 0) {
                let folder = parts[0...depth].joined(separator: "/")
                if seen.insert(folder).inserted {
                    out.append(TreeRow(id: "d|" + folder, depth: depth, name: parts[depth] + "/", file: nil))
                }
            }
            out.append(TreeRow(id: "f|" + file.path, depth: max(parts.count - 1, 0), name: parts.last ?? file.path, file: file))
        }
        return out
    }
}

/// How a file an export writes stands, in a word and a colour.
struct ExportStatusTag: View {
    var status: ExportStatus

    var body: some View {
        Text(status.said).font(.app(9.5, weight: .medium))
            .foregroundStyle(color)
            .padding(.horizontal, 5).padding(.vertical, 1)
            .background(Capsule().fill(color.opacity(0.12)))
    }

    private var color: Color {
        switch status {
        case .current: .green
        case .changed: .orange
        case .edited, .missing: .red
        case .unwritten: .secondary
        }
    }
}

/// A new export: its name (what it writes and where it is written to are decided after).
private struct MakeExportSheet: View {
    var model: AppModel
    var done: () -> Void

    @State private var name = ""
    @State private var problem: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(L10n[.Export.makeTitle]).font(.app(14, weight: .semibold))
            TextField(L10n[.Export.makeNamePlaceholder], text: $name).textFieldStyle(.roundedBorder)
            Text(L10n[.Export.makeNote]).font(.app(10.5)).foregroundStyle(.secondary)
            if let problem { Text(problem).font(.app(11)).foregroundStyle(.red).fixedSize(horizontal: false, vertical: true) }
            HStack {
                Spacer()
                Button(L10n[.Common.actionCancel], role: .cancel, action: done).keyboardShortcut(.cancelAction)
                Button(L10n[.Common.actionCreate]) {
                    if let said = model.createExport(name) { problem = said } else { done() }
                }
                .keyboardShortcut(.defaultAction)
                .disabled(name.trimmingCharacters(in: .whitespaces).isEmpty)
            }
        }
        .padding(20).frame(width: 420)
    }
}

/// An entry added: a card or a section by its path, the file or folder it goes to, its format, and the
/// kinds of card a section's entry takes.
private struct EntrySheet: View {
    var model: AppModel
    var export: String
    var done: () -> Void

    @State private var from = ""
    @State private var to = ""
    @State private var format = ""
    @State private var kinds = ""
    @State private var problem: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(L10n[.Export.entryTitle(export)]).font(.app(14, weight: .semibold))
            TextField(L10n[.Export.entryFromPlaceholder], text: $from)
                .textFieldStyle(.roundedBorder).font(.app(12, design: .monospaced))
            TextField(L10n[.Export.entryToPlaceholder], text: $to)
                .textFieldStyle(.roundedBorder).font(.app(12, design: .monospaced))
            Picker(L10n[.Export.entryFormat], selection: $format) {
                Text(L10n[.Export.entryFormatFromExtension]).tag("")
                ForEach(ExportWords.codeFormats, id: \.self) { Text(ExportWords.formatLabel($0)).tag($0) }
            }
            TextField(L10n[.Export.entryKindsPlaceholder], text: $kinds)
                .textFieldStyle(.roundedBorder).font(.app(12, design: .monospaced))
            if let problem { Text(problem).font(.app(11)).foregroundStyle(.red).fixedSize(horizontal: false, vertical: true) }
            HStack {
                Spacer()
                Button(L10n[.Common.actionCancel], role: .cancel, action: done).keyboardShortcut(.cancelAction)
                Button(L10n[.Common.actionAdd]) {
                    var entry: [String: Any] = ["from": from.trimmingCharacters(in: .whitespaces),
                                                "to": to.trimmingCharacters(in: .whitespaces)]
                    if !format.isEmpty { entry["format"] = format }
                    let named = kinds.split(whereSeparator: { $0 == "," || $0 == "、" || $0 == " " })
                        .map { String($0) }.filter { !$0.isEmpty }
                    if !named.isEmpty { entry["kinds"] = named }
                    if let said = model.updateExport(export, ["entries": ["add": [entry]]]) { problem = said } else { done() }
                }
                .keyboardShortcut(.defaultAction)
                .disabled(from.isEmpty || to.isEmpty)
            }
        }
        .padding(20).frame(width: 520)
    }
}

/// A card or a section left out of the entries of the sections above it.
private struct ExcludeSheet: View {
    var model: AppModel
    var export: String
    var done: () -> Void

    @State private var path = ""
    @State private var problem: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(L10n[.Export.excludeTitle(export)]).font(.app(14, weight: .semibold))
            TextField(L10n[.Export.excludePathPlaceholder], text: $path)
                .textFieldStyle(.roundedBorder).font(.app(12, design: .monospaced))
            if let problem { Text(problem).font(.app(11)).foregroundStyle(.red).fixedSize(horizontal: false, vertical: true) }
            HStack {
                Spacer()
                Button(L10n[.Common.actionCancel], role: .cancel, action: done).keyboardShortcut(.cancelAction)
                Button(L10n[.Common.actionRemove]) {
                    let said = model.updateExport(export, ["exclude": ["add": [path.trimmingCharacters(in: .whitespaces)]]])
                    if let said { problem = said } else { done() }
                }
                .keyboardShortcut(.defaultAction).disabled(path.isEmpty)
            }
        }
        .padding(20).frame(width: 440)
    }
}

/// An export renamed: what it kept beside what it wrote follows.
private struct RenameExportSheet: View {
    var model: AppModel
    var export: String
    var done: () -> Void

    @State private var name = ""
    @State private var problem: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(L10n[.Export.renameTitle(export)]).font(.app(14, weight: .semibold))
            TextField(L10n[.Export.renameNewName], text: $name).textFieldStyle(.roundedBorder)
            if let problem { Text(problem).font(.app(11)).foregroundStyle(.red).fixedSize(horizontal: false, vertical: true) }
            HStack {
                Spacer()
                Button(L10n[.Common.actionCancel], role: .cancel, action: done).keyboardShortcut(.cancelAction)
                Button(L10n[.Common.actionChange]) {
                    if let said = model.updateExport(export, ["name": name]) { problem = said } else { done() }
                }
                .keyboardShortcut(.defaultAction).disabled(name.trimmingCharacters(in: .whitespaces).isEmpty)
            }
        }
        .padding(20).frame(width: 380)
        .onAppear { name = export }
    }
}

/// Where a card is written out, in its details (`specboard://SpecBoard/Docs/Design:Overview` §10): each file, and
/// the export that writes it — read only; the exports are set in the project's settings.
struct CardExports: View {
    var model: AppModel
    var card: SchemaNode

    /// The files it is written to, as the core works them out (`card_read`'s `writtenTo`).
    @State private var files: [WrittenTo] = []

    var body: some View {
        if model.detail != nil {
            VStack(alignment: .leading, spacing: 4) {
                if files.isEmpty {
                    Text(L10n[.Export.cardNotExported])
                        .font(.app(11)).foregroundStyle(.tertiary)
                }
                ForEach(files, id: \.self) { item in
                    HStack(alignment: .firstTextBaseline, spacing: 6) {
                        // One line each: a file's path has no space to wrap at, so it would break
                        // inside a word. The end (the file's name) is the part to read, and what
                        // gives way first is the export's name after it.
                        Text(item.file)
                            .font(.app(11, design: .monospaced)).textSelection(.enabled)
                            .lineLimit(1).truncationMode(.head).layoutPriority(1).help(item.file)
                        if let format = item.format {
                            Text(ExportWords.formatLabel(format)).font(.app(9.5)).foregroundStyle(.secondary)
                                .lineLimit(1).layoutPriority(2)
                        }
                        Spacer(minLength: 4)
                        if let export = item.export {
                            let label = L10n[.Export.labelExport(export)]
                            Text(label).font(.app(10)).foregroundStyle(.tertiary)
                                .lineLimit(1).truncationMode(.head).help(label)
                        }
                    }
                }
                Button(L10n[.Export.cardOpen]) { model.showSettings(.exports) }
                    .controlSize(.small)
                    .help(L10n[.Export.cardOpenHelp])
            }
            // Asked again when the project changes.
            .task(id: "\(model.dataGeneration)|\(card.id)") {
                files = model.writtenTo(card.id)
            }
        }
    }

    /// Whether a card of this kind is written out at all.
    static func exportable(_ kind: SchemaKind) -> Bool {
        kind.isShape || kind == .flow || kind == .action || kind.isWriting || kind.isPage
    }
}
