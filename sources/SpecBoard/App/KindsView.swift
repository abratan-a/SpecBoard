import SwiftUI

/// The project settings' tab for kinds (`specboard://SpecBoard/Docs/Design:Overview` §13): the kinds
/// of card, of section and of line the project has, on the left — the built-in ones to read, the
/// project's own to make, change and take away
/// (`＋`) — and the one picked on the right, its definition as JSON. The project's own is written
/// there and tried by the core as it is written (`kind_create`'s and `kind_update`'s `dryRun`), every
/// problem said under it; saved, it is the project's (one line in main's history), as MCP writes it
/// (`kind_create`, `kind_update`, `kind_delete`). A kind of card's field, kind of row or the kind
/// itself is given another name with its values (名前を変える…). A built-in kind's definition is to
/// read, but for what it is for, when to use it and not (`context`), which the project may write
/// over. A form to put a definition together comes once the shape settles (not yet).
///
/// Above them, the project's tree (`project_tree`): what is directly under main's
/// root (and every branch's) and the board's, written as JSON; checked as it is written, what
/// saving it does under each root — made, renamed, moved into `_evacuation_`, gone — said under it.
struct KindsView: View {
    var model: AppModel

    /// The project's tree's row, picked as a definition is.
    static let treeId = "project_tree"
    /// The project's tree as it is saved, to tell what is written from it.
    @State private var treeSaved = ""

    @State private var entries: [AppModel.KindEntry] = []
    /// The definition picked (`KindEntry.id`), or a new one of a list being written.
    @State private var picked: String?
    @State private var newOf: AppModel.KindList?
    /// What is written in the editor, and what the core said of it.
    @State private var draft = ""
    @State private var check: AppModel.KindsCheck?
    /// The renames to send with the next save.
    @State private var renames: [OJSON] = []
    @State private var renaming = false
    @State private var removing = false
    /// A built-in kind's context as it is written in the tab: what it is for, when to use it and not.
    @State private var context: [String: String] = [:]
    /// A built-in kind of section's mark as it is being picked in the tab, until it is saved.
    @State private var builtinMark: SectionMark?

    /// Where the tab starts — for a picture of it (a test): the definitions, the one picked,
    /// what is written and what the core said of it. None: as the window opens it, read once
    /// it is on screen.
    struct Start {
        var entries: [AppModel.KindEntry]
        var picked: String?
        var draft: String
        var check: AppModel.KindsCheck?
        /// Asking whether to take the one picked away.
        var removing = false
    }

    init(model: AppModel, start: Start? = nil) {
        self.model = model
        if let start {
            _entries = State(initialValue: start.entries)
            _picked = State(initialValue: start.picked)
            _draft = State(initialValue: start.draft)
            _check = State(initialValue: start.check)
            _removing = State(initialValue: start.removing)
        }
    }

    private var editable: Bool { !model.isArchived }

    private var entry: AppModel.KindEntry? { entries.first { $0.id == picked } }

    /// Whether the project's tree is picked.
    private var treePicked: Bool { picked == Self.treeId && newOf == nil }

    /// The list being written in, and the name of the one replaced (none for a new one).
    private var writing: (list: AppModel.KindList, name: String?)? {
        if let newOf { return (newOf, nil) }
        guard let entry, !entry.builtin else { return nil }
        return (entry.list, entry.name)
    }

    var body: some View {
        HStack(spacing: 0) {
            sidebar
                .frame(width: 240)
            Divider()
            detailPane
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        }
        .task(id: model.dataGeneration) { reload() }
        // Checked as it is written, a moment after the last key; again when the screen's language changes,
        // for the check's texts are made in it.
        .task(id: "\(picked ?? "")|\(newOf?.rawValue ?? "")|\(draft)|\(renames.count)|\(Language.shared.code)") {
            guard writing != nil || treePicked else { check = nil; return }
            try? await Task.sleep(nanoseconds: 350_000_000)
            guard !Task.isCancelled else { return }
            check = currentCheck()
        }
        .sheet(isPresented: $renaming) {
            KindRenameSheet(text: draft, strays: strayNames) { text, rename, kind in
                draft = text
                if let kind { renames = KindsEditing.renaming(renames, kind: kind.from, to: kind.to) }
                renames.append(rename)
            }
        }
        .confirmationDialog(L10n[.Kinds.confirmDeleteTitle], isPresented: $removing) {
            Button(L10n[.Common.actionDelete], role: .destructive) { remove() }
            // Said, as every window's: one left to the system is said in English.
            Button(L10n[.Common.actionCancel], role: .cancel) {}
        } message: {
            Text(L10n[.Kinds.confirmDeleteMessage])
        }
    }

    // MARK: - A kind of section's mark

    /// A kind of section's mark and colour in the nav panel (`icon: {name, color}`), picked rather than
    /// written: each writes its part of `icon` in the definition being written, saved with it.
    private var sectionMark: some View {
        let mark = KindsEditing.mark(in: draft)
        return markPicker(mark, defaultMark: nil, editable: editable) { next in
            if let text = KindsEditing.setting(next, in: draft) { draft = text }
        }
    }

    /// The pickers for a mark: the SF Symbol from the candidates, the colour from a colour well, each
    /// given back (`defaultMark`, when there is one, is what is shown for a part not said).
    private func markPicker(_ mark: SectionMark, defaultMark: SectionMark?, clears: Bool = true, editable: Bool,
                            set: @escaping (SectionMark) -> Void) -> some View {
        let shown = SectionMark(icon: mark.icon ?? defaultMark?.icon, color: mark.color ?? defaultMark?.color)
        return HStack(spacing: 12) {
            Text(L10n[.Kinds.markLabel]).font(.app(11)).foregroundStyle(.secondary)
            Menu {
                ForEach(KindsEditing.sectionIcons, id: \.symbol) { item in
                    Button {
                        var next = mark
                        next.icon = item.symbol
                        set(next)
                    } label: { Label(item.word, systemImage: item.symbol) }
                }
                if clears {
                    Divider()
                    Button(defaultMark == nil ? L10n[.Kinds.markUnsetFolder] : L10n[.Kinds.markRestoreBuiltin]) {
                        var next = mark
                        next.icon = nil
                        set(next)
                    }
                }
            } label: {
                Label(shown.icon ?? L10n[.Kinds.markUnset], systemImage: shown.symbol)
            }
            .fixedSize()
            .disabled(!editable)
            ColorPicker(L10n[.Kinds.markColor], selection: Binding(
                get: { shown.color.flatMap(HexColor.color(hex:)) ?? .gray },
                set: { color in
                    var next = mark
                    next.color = HexColor.hex(color)
                    set(next)
                }), supportsOpacity: false)
                .fixedSize()
                .disabled(!editable)
            if clears, mark.color != nil {
                Button(defaultMark == nil ? L10n[.Kinds.markClearColor] : L10n[.Kinds.markRestoreColor]) {
                    var next = mark
                    next.color = nil
                    set(next)
                }
                .buttonStyle(.borderless).font(.app(11))
                .disabled(!editable)
            }
            Spacer()
        }
    }

    // MARK: - Left: the lists

    private var sidebar: some View {
        List(selection: Binding(get: { newOf == nil ? picked : nil }, set: { pick($0) })) {
            Section {
                // The tag is the id itself, not an Optional of it: a row put in a list on its own (not made
                // by a ForEach) is not picked when it is tagged `Optional(id)` — the list takes no
                // selection from it. The ForEach's rows below are.
                Label("project_tree", systemImage: "list.bullet.indent")
                    .font(.app(11.5, design: .monospaced))
                    .help(L10n[.Kinds.projectTreeHelp])
                    .tag(Self.treeId)
            } header: {
                Text(L10n[.Kinds.projectTreeTitle]).font(.app(11, weight: .semibold))
            }
            ForEach(AppModel.KindList.allCases) { list in
                Section {
                    ForEach(entries.filter { $0.list == list && !$0.builtin }) { item in
                        row(item).tag(Optional(item.id))
                    }
                    if newOf == list {
                        Label(L10n[.Kinds.listNewDefinition], systemImage: "square.and.pencil")
                            .font(.app(11.5)).foregroundStyle(Color.accentColor)
                    }
                    ForEach(entries.filter { $0.list == list && $0.builtin }) { item in
                        row(item).tag(Optional(item.id))
                    }
                } header: {
                    HStack {
                        Text(list.title).font(.app(11, weight: .semibold))
                        Spacer()
                        Button { startNew(list) } label: { Image(systemName: "plus") }
                            .buttonStyle(.borderless)
                            .disabled(!editable)
                            .help(L10n[.Kinds.listAddHelp(list.noun)])
                    }
                }
            }
        }
        // Opaque, as the panels around it: a sidebar's list lets the desktop
        // through.
        .listStyle(.inset)
    }

    private func row(_ item: AppModel.KindEntry) -> some View {
        HStack(spacing: 6) {
            Image(systemName: item.builtin ? "lock" : "square.stack.3d.up")
                .font(.app(10)).foregroundStyle(item.builtin ? .tertiary : .secondary)
            Text(item.name).font(.app(11.5, design: .monospaced)).lineLimit(1)
            if item.label != item.name {
                Text(item.label).font(.app(10.5)).foregroundStyle(.secondary).lineLimit(1)
            }
            if item.noPlace {
                Text(L10n[.Kinds.rowNoPlace]).font(.app(9.5)).foregroundStyle(.orange)
                    .padding(.horizontal, 4).padding(.vertical, 1)
                    .overlay(RoundedRectangle(cornerRadius: 3).stroke(.orange.opacity(0.6)))
            }
        }
        .foregroundStyle(item.builtin ? .secondary : .primary)
        .help(item.builtin ? L10n[.Kinds.rowHelpBuiltin]
              : item.noPlace ? L10n[.Kinds.rowHelpNoPlace]
              : L10n[.Kinds.rowHelpOwn])
    }

    // MARK: - Right: the definition

    @ViewBuilder
    private var detailPane: some View {
        if treePicked {
            VStack(alignment: .leading, spacing: 10) {
                HStack(spacing: 8) {
                    Text(L10n[.Kinds.projectTreeTitle]).font(.app(14, weight: .semibold))
                    Text("project_tree").font(.app(11, design: .monospaced)).foregroundStyle(.secondary)
                    Spacer()
                }
                Text(L10n[.Kinds.projectTreeExplain])
                    .font(.app(10.5)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                TextEditor(text: $draft)
                    .font(.app(12, design: .monospaced))
                    .scrollContentBackground(.hidden)
                    .padding(6)
                    .background(Color(nsColor: .textBackgroundColor), in: RoundedRectangle(cornerRadius: 6))
                    .overlay(RoundedRectangle(cornerRadius: 6).stroke(.quaternary))
                    .frame(minHeight: 260)
                    .disabled(!editable)
                problemsView
                HStack {
                    Spacer()
                    Button(L10n[.Kinds.buttonRevert]) { draft = treeSaved }
                        .disabled(!changed)
                    Button(L10n[.Kinds.buttonSave]) { save() }
                        .keyboardShortcut("s", modifiers: .command)
                        .disabled(!editable || !changed || check?.ok != true)
                }
            }
            .padding(16)
        } else if let writing {
            VStack(alignment: .leading, spacing: 10) {
                HStack(spacing: 8) {
                    Text(writing.name ?? L10n[.Kinds.listNewTitle(writing.list.noun)]).font(.app(14, weight: .semibold))
                    Text(writing.list.title).font(.app(11)).foregroundStyle(.secondary)
                    Spacer()
                    if writing.list == .cards {
                        Button(L10n[.Common.actionRenameMenu]) { renaming = true }
                            .disabled(!editable)
                            .help(L10n[.Kinds.editorRenameHelp])
                    }
                    if writing.name != nil {
                        Button(L10n[.Common.actionDelete], role: .destructive) { removing = true }.disabled(!editable)
                    }
                }
                Text(L10n[.Kinds.editorJsonHelp])
                    .font(.app(10.5)).foregroundStyle(.secondary)
                if writing.list == .sections { sectionMark }
                TextEditor(text: $draft)
                    .font(.app(12, design: .monospaced))
                    .scrollContentBackground(.hidden)
                    .padding(6)
                    .background(Color(nsColor: .textBackgroundColor), in: RoundedRectangle(cornerRadius: 6))
                    .overlay(RoundedRectangle(cornerRadius: 6).stroke(.quaternary))
                    .frame(minHeight: 260)
                    .disabled(!editable)
                problemsView
                if !renames.isEmpty {
                    Text(L10n[.Kinds.editorRenamesPending(renames.map { KindRenameSheet.said($0) }.joined(separator: L10n[.Kinds.editorRenamesSeparator]))])
                        .font(.app(10.5)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                }
                HStack {
                    Spacer()
                    Button(L10n[.Kinds.buttonRevert]) { pick(newOf == nil ? picked : nil) }
                        .disabled(!changed)
                    Button(L10n[.Kinds.buttonSave]) { save() }
                        .keyboardShortcut("s", modifiers: .command)
                        .disabled(!editable || !changed || check?.ok != true)
                }
            }
            .padding(16)
        } else if let entry {
            VStack(alignment: .leading, spacing: 10) {
                HStack(spacing: 8) {
                    Text(entry.name).font(.app(14, weight: .semibold, design: .monospaced))
                    Text(L10n[.Kinds.builtinTitle(entry.list.title)]).font(.app(11)).foregroundStyle(.secondary)
                    if entry.iconChanged {
                        Text(L10n[.Kinds.builtinIconChanged]).font(.app(9.5)).foregroundStyle(Color.accentColor)
                            .padding(.horizontal, 4).padding(.vertical, 1)
                            .overlay(RoundedRectangle(cornerRadius: 3).stroke(Color.accentColor.opacity(0.6)))
                    }
                    if entry.contextChanged {
                        Text(L10n[.Kinds.builtinContextChanged]).font(.app(9.5)).foregroundStyle(Color.accentColor)
                            .padding(.horizontal, 4).padding(.vertical, 1)
                            .overlay(RoundedRectangle(cornerRadius: 3).stroke(Color.accentColor.opacity(0.6)))
                    }
                }
                Text(L10n[.Kinds.builtinHelp])
                    .font(.app(10.5)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                if entry.list == .sections { builtinMarkEditor(entry) }
                contextEditor(entry)
                ScrollView {
                    Text(entry.text)
                        .font(.app(11.5, design: .monospaced))
                        .textSelection(.enabled)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(8)
                }
                .background(Color.secondary.opacity(0.06), in: RoundedRectangle(cornerRadius: 6))
            }
            .padding(16)
        } else {
            VStack(alignment: .leading, spacing: 8) {
                Text(L10n[.Kinds.emptyTitle]).font(.app(14, weight: .semibold))
                Text(L10n[.Kinds.emptyHelp])
                    .font(.app(11)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
            }
            .padding(16)
        }
    }

    /// A built-in kind of section's mark written over by the project (`kind_update`'s `icon`): what
    /// the picked entry says is its own (`builtin.json`'s) or the project's, and the pickers write the
    /// project's, saved with the button, or given back.
    private func builtinMarkEditor(_ entry: AppModel.KindEntry) -> some View {
        let saved = KindsEditing.mark(in: entry.text)
        let mark = builtinMark ?? saved
        return VStack(alignment: .leading, spacing: 6) {
            // A built-in kind's mark is given back whole ("組み込みに戻す"): the part left out is its own.
            markPicker(mark, defaultMark: nil, clears: false, editable: editable) { builtinMark = $0 }
            HStack {
                Text(entry.iconChanged ? L10n[.Kinds.markChanged] : L10n[.Kinds.markDefault])
                    .font(.app(10.5)).foregroundStyle(.secondary)
                Spacer()
                if entry.iconChanged {
                    Button(L10n[.Kinds.builtinRestore]) {
                        if model.setBuiltinIcon(entry.name, nil) { builtinMark = nil; reloadPicked() }
                    }
                    .disabled(!editable)
                }
                Button(L10n[.Kinds.markSave]) {
                    if model.setBuiltinIcon(entry.name, mark) { builtinMark = nil; reloadPicked() }
                }
                .disabled(!editable || mark == saved)
            }
        }
    }

    /// A built-in kind's context written over (`kind_update`'s `context`): what it is for, when to
    /// use it and not, each a line; saved, or given back its own.
    private func contextEditor(_ entry: AppModel.KindEntry) -> some View {
        let fields = [("purpose", L10n[.Kinds.contextPurpose]), ("use_when", L10n[.Kinds.contextUseWhen]),
                      ("not_for", L10n[.Kinds.contextNotFor])]
        return VStack(alignment: .leading, spacing: 6) {
            ForEach(fields, id: \.0) { key, word in
                HStack(alignment: .firstTextBaseline, spacing: 8) {
                    Text(word).font(.app(11)).foregroundStyle(.secondary).frame(width: 100, alignment: .trailing)
                    TextField(word, text: Binding(get: { context[key] ?? "" }, set: { context[key] = $0 }), axis: .vertical)
                        .textFieldStyle(.roundedBorder).font(.app(11.5))
                        .disabled(!editable)
                }
            }
            HStack {
                Spacer()
                if entry.contextChanged {
                    Button(L10n[.Kinds.builtinRestore]) {
                        if model.setBuiltinContext(entry.name, nil) { reloadPicked() }
                    }
                    .disabled(!editable)
                }
                Button(L10n[.Kinds.contextSave]) {
                    if model.setBuiltinContext(entry.name, context) { reloadPicked() }
                }
                .disabled(!editable || context == entry.context)
            }
        }
    }

    /// The definitions read again with the one picked, after its context was written.
    private func reloadPicked() {
        entries = model.kindEntries()
        context = entry?.context ?? [:]
        draft = entry?.text ?? ""
    }

    @ViewBuilder
    private var problemsView: some View {
        if let check {
            if check.ok {
                Label(changed ? L10n[.Kinds.checkOkCanSave] : L10n[.Kinds.checkOk], systemImage: "checkmark.circle")
                    .font(.app(11)).foregroundStyle(.green)
                ForEach(check.moved, id: \.self) { moved in
                    Text(moved).font(.app(10.5)).foregroundStyle(.secondary)
                }
            } else {
                VStack(alignment: .leading, spacing: 3) {
                    ForEach(check.problems, id: \.self) { problem in
                        Text(problem)
                            .font(.app(11)).foregroundStyle(.red)
                            .fixedSize(horizontal: false, vertical: true)
                            .textSelection(.enabled)
                    }
                }
            }
        }
    }

    // MARK: - What it does

    /// Whether what is written is not what is saved.
    private var changed: Bool {
        if treePicked { return draft != treeSaved }
        return newOf != nil || !renames.isEmpty || draft != (entry?.text ?? "")
    }

    /// The fields left behind in the project's cards of the kind being written: what a rename may
    /// move to a field it has now.
    private var strayNames: [String] {
        guard let name = writing?.name, writing?.list == .cards else { return [] }
        let names = (model.detail?.schemas ?? []).filter { $0.ownKind == name }.flatMap { $0.strayFields.keys }
        return Array(Set(names)).sorted()
    }

    /// The definitions read again (the project changed, here or by MCP): what is being written is
    /// kept, and follows the definition only while it is as it was.
    private func reload() {
        let was = treePicked ? treeSaved : entry?.text
        entries = model.kindEntries()
        let tree = model.projectTreeText()
        if treePicked {
            if draft == was { draft = tree }
            treeSaved = tree
            return
        }
        treeSaved = tree
        if let picked, !entries.contains(where: { $0.id == picked }) { self.picked = nil }
        if newOf == nil, draft == (was ?? "") || entry == nil { draft = entry?.text ?? "" }
    }

    private func pick(_ id: String?) {
        newOf = nil
        picked = id
        if id == Self.treeId {
            treeSaved = model.projectTreeText()
            draft = treeSaved
        } else {
            draft = entry?.text ?? ""
        }
        context = entry?.context ?? [:]
        builtinMark = nil
        renames = []
        check = nil
    }

    private func startNew(_ list: AppModel.KindList) {
        picked = nil
        newOf = list
        draft = list.template
        renames = []
        check = nil
    }

    /// The project's own list with what is written in it, checked by the core.
    private func currentCheck() -> AppModel.KindsCheck? {
        if treePicked {
            guard let parsed = OJSON.parse(draft), case .object = parsed else {
                return AppModel.KindsCheck(ok: false, problems: [L10n[.Kinds.checkNotObject]])
            }
            return model.checkProjectTree(parsed)
        }
        guard let writing else { return nil }
        switch KindsEditing.parse(draft, as: writing.list) {
        case .failure(let why): return AppModel.KindsCheck(ok: false, problems: [why.said])
        case .success(let written):
            return model.checkKind(writing.list, replacing: writing.name, with: written, renames: renames)
        }
    }

    private func save() {
        if treePicked {
            guard let parsed = OJSON.parse(draft), model.saveProjectTree(parsed) else { return }
            entries = model.kindEntries()
            treeSaved = model.projectTreeText()
            draft = treeSaved
            return
        }
        guard let writing, case .success(let written) = KindsEditing.parse(draft, as: writing.list) else { return }
        guard model.saveKind(writing.list, replacing: writing.name, with: written, renames: renames) else { return }
        renames = []
        newOf = nil
        entries = model.kindEntries()
        if case .object(let o) = written, case .string(let name)? = o[writing.list.nameKey] {
            picked = entries.first { $0.list == writing.list && !$0.builtin && $0.name == name }?.id
        }
        draft = entry?.text ?? ""
    }

    private func remove() {
        guard let writing, let name = writing.name else { return }
        guard model.deleteKind(name) else { return }
        pick(nil)
        entries = model.kindEntries()
    }
}

/// 「名前を変える…」 in the tab for kinds: a kind of card's field, kind of row or the kind itself
/// given another name in its JSON, and its cards' values moved with it when it is saved
/// (`kind_update`'s `name` and `renames`). A field the cards still hold that the definition no longer has
/// (「定義に無い欄」) may be moved to one it has.
struct KindRenameSheet: View {
    var text: String
    /// The fields left behind in its cards (「定義に無い欄」), offered as what to move from.
    var strays: [String]
    /// The JSON with the rename written in, the rename to send, and the kind's own rename.
    var done: (String, OJSON, (from: String, to: String)?) -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var what = KindsEditing.Renamed.field
    @State private var from = ""
    @State private var to = ""
    @State private var problem: String?

    /// A rename as the tab says it before it is saved.
    static func said(_ rename: OJSON) -> String {
        guard case .object(let o) = rename else { return "" }
        let text = { (key: String) -> String? in if case .string(let s)? = o[key] { s } else { nil } }
        return [text("kind"), text("row"), text("field")].compactMap { $0 }.joined(separator: ".") + " → " + (text("to") ?? "")
    }

    /// The names the definition gives what is being renamed.
    private var names: [String] {
        guard case .success(let parsed) = KindsEditing.parse(text, as: AppModel.KindList.cards), case .object(let o) = parsed else { return [] }
        let key = what == .row ? "rows" : "fields"
        guard case .array(let items)? = o[key] else { return [] }
        return items.compactMap { item in
            if case .object(let i) = item, case .string(let n)? = i["name"] { n } else { nil }
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(L10n[.Common.actionRename]).font(.app(13, weight: .semibold))
            Text(L10n[.Kinds.renameSheetHelp])
                .font(.app(11)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
            Picker("", selection: $what) {
                ForEach(KindsEditing.Renamed.allCases) { Text($0.title).tag($0) }
            }
            .labelsHidden().pickerStyle(.segmented).fixedSize()
            if what != .kind {
                HStack {
                    TextField(L10n[.Kinds.renameSheetPreviousName], text: $from).textFieldStyle(.roundedBorder).frame(width: 200)
                    Menu(L10n[.Kinds.renameSheetPick]) {
                        ForEach(names, id: \.self) { name in Button(name) { from = name } }
                        if what == .field, !strays.isEmpty {
                            Divider()
                            ForEach(strays, id: \.self) { name in Button(L10n[.Kinds.renameSheetStray(name)]) { from = name } }
                        }
                    }
                    .fixedSize()
                }
                Text(what == .field ? L10n[.Kinds.renameSheetHelpField] : L10n[.Kinds.renameSheetHelpRow])
                    .font(.app(10.5)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
            }
            TextField(L10n[.Kinds.renameSheetNewName], text: $to).textFieldStyle(.roundedBorder).frame(width: 200)
            if let problem { Text(problem).font(.app(11)).foregroundStyle(.red) }
            HStack {
                Spacer()
                Button(L10n[.Common.actionCancel]) { dismiss() }.keyboardShortcut(.cancelAction)
                Button(L10n[.Kinds.renameSheetApply]) {
                    var old = from
                    if what == .kind, case .success(let parsed) = KindsEditing.parse(text, as: AppModel.KindList.cards),
                       case .object(let o) = parsed, case .string(let kind)? = o["kind"] { old = kind }
                    switch KindsEditing.rename(text, what: what, from: from, to: to) {
                    case .failure(let why): problem = why.said
                    case .success(let made):
                        done(made.text, made.rename, what == .kind ? (old, to) : nil)
                        dismiss()
                    }
                }
                .keyboardShortcut(.defaultAction)
                .disabled(to.trimmingCharacters(in: .whitespaces).isEmpty)
            }
        }
        .padding(18)
        .frame(width: 420)
    }
}
