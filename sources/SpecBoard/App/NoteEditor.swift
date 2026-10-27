import SwiftUI

/// A note or a freedoc opened — its card editor (overview §13): the body read — its
/// `![[…]]` drawn — or written, with the links and diagrams a menu away. A note is a
/// memo pinned beside the cards; a freedoc is a document written freely. A task's body is
/// written here too (its fields in the property panel), and a goal's under its card
/// editor's bar (`GoalEditor`, `compact`); so is a card of the project's own kind that opens so
/// (`opens: markdown`, its field `body`).
struct NoteEditor: View {
    var model: AppModel
    var note: SchemaNode
    var editable: Bool
    /// Under another card editor's bar, which says what the card is: only the ways to
    /// read and write the body.
    var compact = false
    /// Follow a `[[path]]` written in this note, from its section.
    var onOpenPath: (String, String?) -> Void

    init(model: AppModel, note: SchemaNode, editable: Bool, compact: Bool = false,
         onOpenPath: @escaping (String, String?) -> Void) {
        self.model = model
        self.note = note
        self.editable = editable
        self.compact = compact
        self.onOpenPath = onOpenPath
    }

    /// What the badge calls the card.
    private var badge: String {
        note.kind == .custom ? note.kindDef?.badge ?? note.kindName : BuiltinWords.badge(note.kind.rawValue)
    }

    @State private var writing = false
    @State private var draft = ""
    /// The card `draft` was taken from: a draft is only ever saved back to it.
    @State private var draftOf: String?
    @State private var selection: TextSelection?

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            header
            Divider()
            freeText
        }
        .onDisappear { save() }
    }

    // MARK: - Above

    private var header: some View {
        CardBar(kind: note.kind, badge: badge, name: note.name, label: note.label, titled: !compact) {
            EmptyView()
        } tools: {
            if writing, let detail = model.detail { pickers(detail) }
            Picker("", selection: Binding(get: { writing }, set: { $0 ? startWriting() : finishWriting() })) {
                BarLabel(title: L10n[.CardEditor.modeView], systemImage: "eye").tag(false)
                BarLabel(title: L10n[.Common.actionEdit], systemImage: "pencil").tag(true)
            }
            .labelsHidden().pickerStyle(.segmented).fixedSize()
            .disabled(!editable)
            .help(L10n[.CardEditor.helpMode])
            if !compact {
                // While the body is written, its connections are the body as written so far:
                // it is saved only when it is read again.
                CardTools(model: model, card: note, editable: editable, writing: writing ? draft : nil) {
                    model.open(.card($0))
                }
            }
        }
        .font(.app(11))
        .padding(.horizontal, 14).padding(.vertical, 8)
    }

    // MARK: - Free text

    @ViewBuilder
    private var freeText: some View {
        if writing {
            // What is written, and beside it what it reads as, as it is typed.
            HStack(spacing: 0) {
                TextEditor(text: $draft, selection: $selection)
                    .font(.app(13, design: .monospaced))
                    .scrollContentBackground(.hidden)
                    .padding(12)
                    .frame(maxWidth: .infinity)
                Divider()
                ScrollView {
                    if let detail = model.detail, !draft.isEmpty {
                        MarkdownView(text: draft, size: 13, refs: NoteEditor.refs(note, detail, outside: model.isBoardOpen ? model.outsideDetail : nil, open: onOpenPath))
                            .padding(.horizontal, 20).padding(.vertical, 16)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    } else {
                        Text(L10n[.CardEditor.previewTitle]).foregroundStyle(.tertiary)
                            .padding(16).frame(maxWidth: .infinity, alignment: .leading)
                    }
                }
                .frame(maxWidth: .infinity)
            }
        } else if note.body.isEmpty {
            VStack(spacing: 10) {
                Text(L10n[.CardEditor.emptyNothingWritten]).foregroundStyle(.secondary)
                if editable { Button(L10n[.CardEditor.buttonWrite]) { startWriting() } }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else if let detail = model.detail {
            ScrollView {
                MarkdownView(text: note.body, size: 13, refs: NoteEditor.refs(note, detail, outside: model.isBoardOpen ? model.outsideDetail : nil, open: onOpenPath))
                    .padding(.horizontal, 24).padding(.vertical, 16)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
    }

    /// A link or a diagram put in where the cursor is, each picked from the project
    /// (`specboard://SpecBoard/Docs/Design:DesignSections` §8) and written in full.
    private func pickers(_ detail: ProjectDetail) -> some View {
        HStack(spacing: 6) {
            // A path in a body crosses from the board to main alone; a working directory's file is a card
            // of the tree open, picked as any.
            PathPickButton(title: L10n[.CardEditor.buttonInsertLink], compactImage: "link", detail: detail, owner: note, wanted: .mention,
                           outside: model.isBoardOpen ? model.outsideDetail : nil) { path in
                insert(inline: "[[\(path)]]")
            }
            .help(L10n[.CardEditor.helpInsertLink])
            PathPickButton(title: L10n[.CardEditor.buttonInsertDiagram], compactImage: "photo.on.rectangle", detail: detail, owner: note, wanted: .embed) { path in
                insert("![[\(path)]]")
            }
            .help(L10n[.CardEditor.helpInsertDiagram])
        }
        .fixedSize()
    }

    private func startWriting() {
        draft = note.body
        draftOf = note.id
        selection = nil
        writing = true
    }

    private func finishWriting() {
        save()
        writing = false
    }

    private func save() {
        guard let id = draftOf else { return }
        draftOf = nil
        guard let current = model.schema(id), current.body != draft else { return }
        model.updateSchema(id, .init(body: draft))
    }

    /// Where the cursor is — or at the end — in the line as it is.
    private func insert(inline text: String) {
        var at = draft.endIndex
        if let selection, case .selection(let range) = selection.indices {
            at = range.upperBound
        }
        draft.insert(contentsOf: text, at: at)
        selection = TextSelection(insertionPoint: draft.index(at, offsetBy: text.count))
    }

    /// On a line of its own, where the cursor is — or at the end.
    private func insert(_ line: String) {
        var at = draft.endIndex
        if let selection, case .selection(let range) = selection.indices {
            at = range.upperBound
        }
        let before = at == draft.startIndex || draft[draft.index(before: at)] == "\n" ? "" : "\n"
        let after = at < draft.endIndex && draft[at] == "\n" ? "" : "\n"
        let text = before + line + after
        draft.insert(contentsOf: text, at: at)
        let end = draft.index(at, offsetBy: text.count)
        selection = TextSelection(insertionPoint: end)
    }

    /// `[[…]]`, `{{…}}` and `![[…]]` as the note reads them. `outside`: main's detail, for a card
    /// of the board's (`AppModel.outsideDetail`) — a path in full naming one of
    /// main's places is found there, and opened in main.
    static func refs(_ note: SchemaNode, _ detail: ProjectDetail, outside: ProjectDetail? = nil,
                     open: @escaping (String, String?) -> Void) -> MarkdownRefs {
        let directory = detail.directory
        let base = directory.sectionId(of: note)
        let seen = NoteEmbed.view(detail, for: note)
        func resolved(_ raw: String) -> PathHit {
            let here = resolvePlace(detail, raw, from: note, directory: directory)
            guard !here.ok, let outside, PathText.hasSection(raw) else { return here }
            let there = resolvePlace(outside, raw)
            return there.ok ? there : here
        }
        return MarkdownRefs(
            exists: { resolved($0).ok },
            values: { NoteValues.source?(detail.project.id, $0, base) ?? [:] },
            open: { open($0, base) },
            embed: { NoteEmbed.block($0, of: note, in: seen) },
            // Why one is struck through, on the pointer: the sections that have the name,
            // and that a card ghost would let it be written by its name alone.
            problem: { resolved($0).hint })
    }
}
