import SwiftUI

/// The project settings' tab for kinds (`specboard://SpecBoard/Docs/Design:Overview` §13,
/// `specboard://SpecBoard/Docs/Design:UI` the Kinds tab): the kinds of card, of section and of line the
/// project has, on the left — the built-in ones to read, the project's own to make, change and take away
/// (`＋`) — and the one picked on the right, as a form of foldable groups or as its JSON («Form | JSON»,
/// kept per Mac), both writing one draft (`KindDraft`), with a preview of it beside the form (`kind_preview`;
/// the form's first group when there is no room beside it). The project's own is tried by the core as it is
/// written (`kind_create`'s and `kind_update`'s `dryRun`), every problem said under it; saved, it is the
/// project's (one line in main's history), as MCP writes it (`kind_create`, `kind_update`, `kind_delete`). A
/// kind of card's field, kind of row or the kind itself is given another name with its values (名前を変える…,
/// or the form renaming a saved one). A built-in kind opens in the same form, only to read, but for what it is
/// for, when to use it and not (`context`) and a kind of section's mark, which the project may write over,
/// each with its own button.
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
    /// What is written, as the JSON editor shows it, and what the core said of it.
    @State private var draft = ""
    @State private var check: AppModel.KindsCheck?
    /// The same draft as the form reads it, and why the form cannot read what the JSON says now.
    @State private var form: KindDraft?
    @State private var formProblem: String?
    @State private var renaming = false
    @State private var removing = false
    /// A built-in kind's context as it is written in the tab: what it is for, when to use it and not.
    @State private var context: [String: String] = [:]
    /// A built-in kind of section's mark as it is being picked in the tab, until it is saved.
    @State private var builtinMark: SectionMark?
    /// The field or kind of row picked in the form, outlined in the preview.
    @State private var selection: KindDraft.Place?
    /// What the core last read of the draft, whether the draft now cannot be read, and the card drawn
    /// (one of the tree's, by id; none for made-up values).
    @State private var preview: AppModel.KindPreview?
    @State private var previewFaded = false
    @State private var sample: String?
    /// The kinds of card a definition may name, with what people call them.
    @State private var cardChoices: [(name: String, label: String)] = []

    @AppStorage(KindFormMemory.modeKey) private var mode = KindFormMemory.form
    @AppStorage(KindFormMemory.openKey) private var openSaid = KindFormMemory.standardOpen
    /// The way a definition is shown, set by a picture of the tab rather than kept.
    @State private var startMode: String?

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
        /// Shown as the form or as the JSON (`KindFormMemory.form`, `.json`); none: as this Mac keeps it.
        var mode: String? = nil
        /// What the preview draws until the draft is read again.
        var preview: AppModel.KindPreview? = nil
        /// The field or kind of row picked in the form.
        var selection: KindDraft.Place? = nil
    }

    init(model: AppModel, start: Start? = nil) {
        self.model = model
        if let start {
            _entries = State(initialValue: start.entries)
            _picked = State(initialValue: start.picked)
            _draft = State(initialValue: start.draft)
            _check = State(initialValue: start.check)
            _removing = State(initialValue: start.removing)
            _startMode = State(initialValue: start.mode)
            _preview = State(initialValue: start.preview)
            _selection = State(initialValue: start.selection)
            _cardChoices = State(initialValue: Self.cardChoices(start.entries))
            if let entry = start.entries.first(where: { $0.id == start.picked }) {
                switch KindDraft.read(start.draft, as: entry.list, saved: entry.text) {
                case .success(let read): _form = State(initialValue: read)
                case .failure(let problem): _formProblem = State(initialValue: problem.said)
                }
            }
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

    /// The list of the definition on the right, written or read.
    private var shownList: AppModel.KindList? { newOf ?? (treePicked ? nil : entry?.list) }

    /// Whether the definition on the right is shown as the form (the project's tree is JSON only).
    private var showsForm: Bool { (startMode ?? mode) == KindFormMemory.form && shownList != nil }

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
        .task(id: "\(picked ?? "")|\(newOf?.rawValue ?? "")|\(draft)|\(currentRenames.count)|\(Language.shared.code)") {
            guard writing != nil || treePicked else { check = nil; return }
            try? await Task.sleep(nanoseconds: 350_000_000)
            guard !Task.isCancelled else { return }
            check = currentCheck()
        }
        // Drawn again as it is written, the same moment after; nothing is written.
        .task(id: "\(picked ?? "")|\(newOf?.rawValue ?? "")|\(draft)|\(sample ?? "")|\(showsForm)|\(model.dataGeneration)|\(Language.shared.code)") {
            guard showsForm, shownList != nil else { return }
            try? await Task.sleep(nanoseconds: 350_000_000)
            guard !Task.isCancelled else { return }
            drawPreview()
        }
        .sheet(isPresented: $renaming) {
            KindRenameSheet(text: draft, strays: strayNames) { what, from, to in rename(what, from: from, to: to) }
        }
        .confirmationDialog(L10n[.Kinds.confirmDeleteTitle], isPresented: $removing) {
            Button(L10n[.Common.actionDelete], role: .destructive) { remove() }
            // Said, as every window's: one left to the system is said in English.
            Button(L10n[.Common.actionCancel], role: .cancel) {}
        } message: {
            Text(L10n[.Kinds.confirmDeleteMessage])
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
                jsonEditor
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
                heading(title: writing.name ?? L10n[.Kinds.listNewTitle(writing.list.noun)], list: writing.list, builtin: false)
                if showsForm {
                    formArea(readOnly: !editable)
                } else {
                    Text(L10n[.Kinds.editorJsonHelp])
                        .font(.app(10.5)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                    jsonEditor
                }
                HStack(alignment: .top, spacing: 12) {
                    VStack(alignment: .leading, spacing: 4) {
                        problemsView
                        if !currentRenames.isEmpty {
                            Text(L10n[.Kinds.editorRenamesPending(currentRenames.map { KindRenameSheet.said($0) }.joined(separator: L10n[.Kinds.editorRenamesSeparator]))])
                                .font(.app(10.5)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                        }
                    }
                    Spacer()
                    Button(L10n[.Kinds.buttonRevert]) { pick(newOf == nil ? picked : nil) }
                        .disabled(!changed)
                    Button(L10n[.Kinds.buttonSave]) { save() }
                        .keyboardShortcut("s", modifiers: .command)
                        .disabled(!editable || !changed || check?.ok != true)
                }
            }
            .padding(16)
            .background { if showsForm { undoKeys } }
        } else if let entry {
            VStack(alignment: .leading, spacing: 10) {
                heading(title: entry.name, list: entry.list, builtin: true)
                Text(L10n[.Kinds.builtinHelp])
                    .font(.app(10.5)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                if showsForm {
                    formArea(readOnly: true)
                } else {
                    ScrollView {
                        Text(entry.text)
                            .font(.app(11.5, design: .monospaced))
                            .textSelection(.enabled)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(8)
                    }
                    .background(Color.secondary.opacity(0.06), in: RoundedRectangle(cornerRadius: 6))
                }
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

    /// The top of the right: the definition's name, its list and what people call it; on the right the
    /// switch between the form and the JSON, 「名前を変える…」 (a kind of card of the project's) and
    /// 「削除」 (one saved).
    private func heading(title: String, list: AppModel.KindList, builtin: Bool) -> some View {
        HStack(spacing: 8) {
            Text(title).font(.app(14, weight: .semibold, design: builtin ? .monospaced : .default))
            Text(builtin ? L10n[.Kinds.builtinTitle(list.title)] : list.title).font(.app(11)).foregroundStyle(.secondary)
            if let label = form?.string(at: .kind, "label"), !label.isEmpty, label != title {
                Text(label).font(.app(11)).foregroundStyle(.secondary).lineLimit(1)
            }
            if builtin, let entry {
                if entry.iconChanged { KindTag(text: L10n[.Kinds.builtinIconChanged], tint: .accentColor) }
                if entry.contextChanged { KindTag(text: L10n[.Kinds.builtinContextChanged], tint: .accentColor) }
            }
            Spacer()
            Picker("", selection: Binding(get: { startMode ?? mode }, set: { switchTo($0) })) {
                Text(L10n[.Kinds.formModeForm]).tag(KindFormMemory.form)
                Text(verbatim: "JSON").tag(KindFormMemory.json)
            }
            .pickerStyle(.segmented).labelsHidden().fixedSize()
            if !builtin, list == .cards {
                Button(L10n[.Common.actionRenameMenu]) { renaming = true }
                    .disabled(!editable)
                    .help(L10n[.Kinds.editorRenameHelp])
            }
            if !builtin, writing?.name != nil {
                Button(L10n[.Common.actionDelete], role: .destructive) { removing = true }.disabled(!editable)
            }
        }
    }

    private var jsonEditor: some View {
        TextEditor(text: $draft)
            .font(.app(12, design: .monospaced))
            .scrollContentBackground(.hidden)
            .padding(6)
            .background(Color(nsColor: .textBackgroundColor), in: RoundedRectangle(cornerRadius: 6))
            .overlay(RoundedRectangle(cornerRadius: 6).stroke(.quaternary))
            .frame(minHeight: 260)
            .disabled(!editable)
    }

    /// The form, with the preview beside it where there is room for both (the form at least 480 wide, the
    /// preview 300), else as the form's first group; or, while the JSON cannot be read as a definition,
    /// why, and the way back to the JSON.
    @ViewBuilder
    private func formArea(readOnly: Bool) -> some View {
        if let formProblem {
            VStack(alignment: .leading, spacing: 8) {
                Label(L10n[.Kinds.formUnreadable(formProblem)], systemImage: "exclamationmark.triangle")
                    .font(.app(11.5)).foregroundStyle(.orange).fixedSize(horizontal: false, vertical: true)
                Button(L10n[.Kinds.formBackToJSON]) { switchTo(KindFormMemory.json) }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        } else if let form, let list = shownList {
            GeometryReader { geo in
                let wide = Self.sideBySide(geo.size.width)
                HStack(alignment: .top, spacing: 16) {
                    ScrollView {
                        kindForm(form, list: list, readOnly: readOnly, narrow: !wide)
                            .padding(.trailing, 6)
                    }
                    .frame(minWidth: wide ? Self.formWidth : nil, maxWidth: .infinity)
                    if wide {
                        ScrollView { previewView(form, list: list, titled: true) }
                            .frame(width: Self.previewWidth)
                    }
                }
            }
        }
    }

    /// Whether a kind of section may be told to hold the kind of card written: only once it is saved as it
    /// is, for the kind of section is saved on its own and names it.
    static func canHoldIn(saved: Bool, changed: Bool, editable: Bool) -> Bool { saved && !changed && editable }

    /// Whether the room right of the list takes the form and the preview side by side.
    static func sideBySide(_ width: CGFloat) -> Bool { width >= formWidth + previewWidth + 16 }

    /// The least the form is given beside the preview, and the preview's width.
    static let formWidth: CGFloat = 480
    static let previewWidth: CGFloat = 300

    private func kindForm(_ form: KindDraft, list: AppModel.KindList, readOnly: Bool, narrow: Bool) -> some View {
        let shown = previewView(form, list: list, titled: false)
        let builtin = entry?.builtin == true && newOf == nil
        return KindForm(
            context: KindFormContext(draft: form, readOnly: readOnly, cardKinds: cardChoices,
                                     edit: { change in edit(change) }, toJSON: { switchTo(KindFormMemory.json) }),
            list: list, entries: entries, entry: newOf == nil ? entry : nil,
            canHold: Self.canHoldIn(saved: writing?.name != nil, changed: changed, editable: editable),
            rowKinds: { name in model.detail?.cardKinds[name]?.rows.map(\.name) ?? [] },
            selection: $selection,
            preview: narrow ? AnyView(shown) : nil, previewSummary: shown.sampleSaid,
            builtinUsage: builtin ? entry.map { AnyView(contextEditor($0)) } : nil,
            builtinMark: builtin && list == .sections ? entry.map { AnyView(builtinMarkEditor($0)) } : nil,
            holdIn: { holdIn($0) }, openProjectTree: { pick(Self.treeId) },
            anyHolders: KindForm.anyHolders(entries, inTree: Set((model.detail?.sections ?? [])
                .filter { $0.name != ProjectSection.evacuation }.compactMap { $0.kindName ?? $0.kind?.rawValue })))
    }

    private func previewView(_ form: KindDraft, list: AppModel.KindList, titled: Bool) -> KindPreviewView {
        KindPreviewView(list: list, preview: preview, faded: previewFaded, draft: form, selection: selection,
                        containerRow: KindFormMemory.groups(openSaid).contains("card.look"),
                        samples: samples, sample: $sample, kinds: model.detail?.cardKinds ?? CardKinds(), titled: titled,
                        makePage: { await model.documentPage($0) })
    }

    /// ⌘Z and ⇧⌘Z in the form: the draft's changes taken back and made again, one by one. Not there while
    /// none can be, so the keys go on to what has focus.
    @ViewBuilder
    private var undoKeys: some View {
        if let form, editable {
            ZStack {
                Button("") { edit { $0.undo() } }
                    .keyboardShortcut("z", modifiers: .command)
                    .disabled(!form.canUndo)
                Button("") { edit { $0.redo() } }
                    .keyboardShortcut("z", modifiers: [.command, .shift])
                    .disabled(!form.canRedo)
            }
            .opacity(0)
            .frame(width: 0, height: 0)
            .allowsHitTesting(false)
            .accessibilityHidden(true)
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
            KindMarkPicker(mark: mark, clears: false, editable: editable) { builtinMark = $0 }
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
        setEntries(model.kindEntries())
        context = entry?.context ?? [:]
        draft = entry?.text ?? ""
        loadForm()
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
        return newOf != nil || !currentRenames.isEmpty || draft != (entry?.text ?? "")
    }

    /// What saving renames (`renames`): the form's renames of the saved fields and kinds of row, and those
    /// 「名前を変える…」 wrote — read against what the JSON says now.
    private var currentRenames: [OJSON] {
        guard writing != nil, var now = form else { return [] }
        _ = now.setText(draft)
        return now.renames
    }

    /// The fields left behind in the project's cards of the kind being written: what a rename may
    /// move to a field it has now.
    private var strayNames: [String] {
        guard let name = writing?.name, writing?.list == .cards else { return [] }
        let names = (model.detail?.schemas ?? []).filter { $0.ownKind == name }.flatMap { $0.strayFields.keys }
        return Array(Set(names)).sorted()
    }

    /// The tree's cards of the kind picked, each by its path: what the preview may draw.
    private var samples: [(id: String, path: String)] {
        guard shownList == .cards, newOf == nil, let name = entry?.name, let detail = model.detail else { return [] }
        return detail.schemas.filter { $0.kindName == name }.prefix(100).map { ($0.id, detail.directory.full($0)) }
    }

    private func setEntries(_ read: [AppModel.KindEntry]) {
        entries = read
        cardChoices = Self.cardChoices(read)
    }

    /// The kinds of card a definition may name: those a card can be made of (not one only to take after,
    /// nor one only the app makes).
    static func cardChoices(_ entries: [AppModel.KindEntry]) -> [(name: String, label: String)] {
        entries.filter { $0.list == .cards }.compactMap { entry in
            guard case .object(let o)? = OJSON.parse(entry.text) else { return nil }
            if case .bool(true)? = o["abstract"] { return nil }
            if case .bool(true)? = o["system_only"] { return nil }
            return (entry.name, entry.label)
        }
    }

    /// The definitions read again (the project changed, here or by MCP): what is being written is
    /// kept, and follows the definition only while it is as it was.
    private func reload() {
        let was = treePicked ? treeSaved : entry?.text
        setEntries(model.kindEntries())
        let tree = model.projectTreeText()
        if treePicked {
            if draft == was { draft = tree }
            treeSaved = tree
            return
        }
        treeSaved = tree
        if let picked, !entries.contains(where: { $0.id == picked }) { self.picked = nil }
        if newOf == nil, draft == (was ?? "") || entry == nil {
            draft = entry?.text ?? ""
            if form.map({ !$0.canUndo }) ?? true { loadForm() }
        }
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
        check = nil
        reset()
    }

    private func startNew(_ list: AppModel.KindList) {
        picked = nil
        newOf = list
        draft = list.template
        check = nil
        reset()
    }

    /// What was drawn and picked for the definition before let go of, and the draft read anew: nothing
    /// to take back.
    private func reset() {
        selection = nil
        sample = nil
        preview = nil
        previewFaded = false
        loadForm()
    }

    /// The draft read by the form as the JSON says it, against the definition as it is saved.
    private func loadForm() {
        guard let list = shownList else { form = nil; formProblem = nil; return }
        switch KindDraft.read(draft, as: list, saved: newOf == nil ? entry?.text : nil) {
        case .success(let read):
            form = read
            formProblem = nil
        case .failure(let problem):
            form = nil
            formProblem = problem.said
        }
    }

    /// The form's draft made to say what the JSON says now (one change, taken back as one); why not, when
    /// it cannot read it.
    private func readText() {
        guard var now = form else { loadForm(); return }
        if let problem = now.setText(draft) {
            formProblem = problem.said
        } else {
            form = now
            formProblem = nil
        }
    }

    /// A change made by the form: written into the draft, and the JSON written from it.
    private func edit(_ change: (inout KindDraft) -> Void) {
        guard var now = form else { return }
        change(&now)
        form = now
        draft = now.text
    }

    /// The definition shown as the form or as the JSON, and kept so on this Mac; to the form, it reads
    /// what the JSON says.
    private func switchTo(_ next: String) {
        if next == KindFormMemory.form { readText() }
        startMode = nil
        mode = next
    }

    /// 「名前を変える…」: written into the draft as the form writes a rename. Why not, when not.
    private func rename(_ what: KindsEditing.Renamed, from: String, to: String) -> KindsEditing.Problem? {
        readText()
        guard form != nil, formProblem == nil else { return KindsEditing.Problem(L10n[.Kinds.problemFixJSONFirst]) }
        var why: KindsEditing.Problem?
        edit { why = $0.rename(what, from: from, to: to) }
        return why
    }

    /// A kind of section made to hold the kind of card being written: its definition opened with the kind
    /// added to what it holds — `section`'s, or a new one's — to be saved on its own.
    private func holdIn(_ section: String?) {
        guard let name = form?.name, !name.isEmpty else { return }
        if let section, let target = entries.first(where: { $0.list == .sections && !$0.builtin && $0.name == section }) {
            pick(target.id)
            edit { $0.setHolds($0.strings(at: .kind, "holds") + [name]) }
        } else {
            startNew(.sections)
            edit { $0.setHolds([name]) }
        }
    }

    /// The draft as the core reads it as though it were written, drawn; the last drawing kept faint while
    /// it cannot be read.
    private func drawPreview() {
        guard let list = shownList else { return }
        guard case .success(let written) = KindsEditing.parse(draft, as: list),
              let read = model.kindPreview(list, replacing: newOf == nil ? entry?.name : nil, with: written, sample: sample),
              read.kind != nil else {
            previewFaded = preview != nil
            return
        }
        preview = read
        previewFaded = false
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
            return model.checkKind(writing.list, replacing: writing.name, with: written, renames: currentRenames)
        }
    }

    private func save() {
        if treePicked {
            guard let parsed = OJSON.parse(draft), model.saveProjectTree(parsed) else { return }
            setEntries(model.kindEntries())
            treeSaved = model.projectTreeText()
            draft = treeSaved
            return
        }
        guard let writing, case .success(let written) = KindsEditing.parse(draft, as: writing.list) else { return }
        guard model.saveKind(writing.list, replacing: writing.name, with: written, renames: currentRenames) else { return }
        setEntries(model.kindEntries())
        var saved: String?
        if case .object(let o) = written, case .string(let name)? = o[writing.list.nameKey] {
            saved = entries.first { $0.list == writing.list && !$0.builtin && $0.name == name }?.id
        }
        let keep = selection
        pick(saved)
        selection = keep
    }

    private func remove() {
        guard let writing, let name = writing.name else { return }
        guard model.deleteKind(name) else { return }
        pick(nil)
        setEntries(model.kindEntries())
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
    /// The rename written into the draft, as the form writes one: why not, when it cannot be.
    var apply: (KindsEditing.Renamed, String, String) -> KindsEditing.Problem?

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
                    if let why = apply(what, from, to) {
                        problem = why.said
                    } else {
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
