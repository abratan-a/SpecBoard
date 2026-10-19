import SwiftUI

/// A stackdoc opened — its card editor (`specboard://SpecBoard/Docs/Design:CardKinds`「積み重ね文書」): on
/// top the selector of its versions, the one open chosen there (how it looks: no commit), with a
/// version added on top, the one open let go of and moved up or down; under it the version open read
/// — its heading and its body, its `![[…]]` drawn — or its body written, as a freedoc's is. The
/// version's name, key and heading are written in the property panel.
struct StackDocEditor: View {
    var model: AppModel
    var doc: SchemaNode
    var editable: Bool
    /// Follow a `[[path]]` written in the version, from the card's section.
    var onOpenPath: (String, String?) -> Void

    @State private var writing = false
    @State private var draft = ""
    /// The card and the version `draft` was taken from: a draft is only ever saved back to it.
    @State private var draftOf: (card: String, version: String)?
    @State private var selection: TextSelection?
    @State private var adding = false
    @State private var newKey = ""
    @State private var deleting = false

    private var open: StackVersion? { doc.openVersion }
    private var openIndex: Int? { open.flatMap { version in doc.versions.firstIndex { $0.name == version.name } } }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            header
            Divider()
            versionBar
            Divider()
            content
        }
        .onDisappear { save() }
        .onChange(of: open?.name) { _, _ in
            // Another version open: what was written goes to the one it was taken from.
            if writing { finishWriting() }
        }
        .alert(L10n[.Docs.versionAdd], isPresented: $adding) {
            TextField(L10n[.Docs.versionKeyPlaceholder], text: $newKey)
            Button(L10n[.Common.actionAdd]) {
                let key = newKey.trimmingCharacters(in: .whitespaces)
                guard !key.isEmpty else { return }
                save()
                writing = false
                model.addVersion(doc.id, key: key)
            }
            Button(L10n[.Common.actionCancel], role: .cancel) {}
        } message: {
            Text(L10n[.Docs.versionAddMessage])
        }
        .confirmationDialog(L10n[.Docs.versionDeleteTitle(open?.key ?? "")], isPresented: $deleting) {
            Button(L10n[.Common.actionDelete], role: .destructive) {
                guard let open else { return }
                draftOf = nil
                writing = false
                model.deleteVersion(doc.id, open.name)
            }
            // Said, as every window's: one left to the system is said in English.
            Button(L10n[.Common.actionCancel], role: .cancel) {}
        } message: {
            Text(L10n[.Docs.versionDeleteMessage])
        }
    }

    // MARK: - Above

    private var header: some View {
        CardBar(kind: doc.kind, badge: L10n[.Docs.stackdocBadge], name: doc.name, label: doc.label) {
            EmptyView()
        } tools: {
            if writing, let detail = model.detail { pickers(detail) }
            Picker("", selection: Binding(get: { writing }, set: { $0 ? startWriting() : finishWriting() })) {
                BarLabel(title: L10n[.Docs.modeView], systemImage: "eye").tag(false)
                BarLabel(title: L10n[.Common.actionEdit], systemImage: "pencil").tag(true)
            }
            .labelsHidden().pickerStyle(.segmented).fixedSize()
            .disabled(!editable || open == nil)
            .help(L10n[.Docs.modeStackHelp])
            CardTools(model: model, card: doc, editable: editable, writing: nil) {
                model.open(.card($0))
            }
        }
        .font(.app(11))
        .padding(.horizontal, 14).padding(.vertical, 8)
    }

    /// The selector of its versions, and what is done to them.
    private var versionBar: some View {
        HStack(spacing: 8) {
            Text(L10n[.Docs.versionLabel]).font(.app(11)).foregroundStyle(.secondary)
            if doc.versions.isEmpty {
                Text(L10n[.Docs.versionNone]).font(.app(11.5)).foregroundStyle(.secondary)
            } else {
                Picker("", selection: Binding(get: { open?.name ?? "" },
                                              set: { name in
                                                  guard name != open?.name else { return }
                                                  model.openVersion(doc.id, name)
                                              })) {
                    ForEach(doc.versions, id: \.name) { version in Text(version.key).tag(version.name) }
                }
                .labelsHidden().pickerStyle(.menu).fixedSize()
                .disabled(!model.canSetLooks)
                .help(L10n[.Docs.versionSelectHelp])
            }
            Spacer(minLength: 8)
            if editable {
                Button { newKey = ""; adding = true } label: { BarLabel(title: L10n[.Docs.versionAdd], systemImage: "plus") }
                    .help(L10n[.Docs.versionAddHelp])
                Button { if let open { save(); model.moveVersion(doc.id, open.name, by: -1) } } label: {
                    BarLabel(title: L10n[.Common.actionMoveUp], systemImage: "arrow.up")
                }
                .disabled((openIndex ?? 0) == 0)
                .help(L10n[.Docs.versionMoveUpHelp])
                Button { if let open { save(); model.moveVersion(doc.id, open.name, by: 1) } } label: {
                    BarLabel(title: L10n[.Common.actionMoveDown], systemImage: "arrow.down")
                }
                .disabled(openIndex.map { $0 >= doc.versions.count - 1 } ?? true)
                .help(L10n[.Docs.versionMoveDownHelp])
                Button(role: .destructive) { deleting = true } label: { BarLabel(title: L10n[.Common.actionDelete], systemImage: "trash") }
                    .disabled(open == nil)
                    .help(L10n[.Docs.versionDeleteHelp])
            }
        }
        .controlSize(.small)
        .font(.app(11))
        .padding(.horizontal, 14).padding(.vertical, 6)
    }

    // MARK: - The version open

    @ViewBuilder
    private var content: some View {
        if let open {
            if writing {
                HStack(spacing: 0) {
                    TextEditor(text: $draft, selection: $selection)
                        .font(.app(13, design: .monospaced))
                        .scrollContentBackground(.hidden)
                        .padding(12)
                        .frame(maxWidth: .infinity)
                    Divider()
                    ScrollView {
                        if let detail = model.detail {
                            MarkdownView(text: StackVersion(name: open.name, key: open.key, heading: open.heading, body: draft).page,
                                         size: 13, refs: refs(detail))
                                .padding(.horizontal, 20).padding(.vertical, 16)
                                .frame(maxWidth: .infinity, alignment: .leading)
                        }
                    }
                    .frame(maxWidth: .infinity)
                }
            } else if let detail = model.detail {
                ScrollView {
                    VStack(alignment: .leading, spacing: 10) {
                        MarkdownView(text: open.page, size: 13, refs: refs(detail))
                        if open.body.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                            HStack(spacing: 10) {
                                Text(L10n[.Docs.versionNoBody]).foregroundStyle(.secondary)
                                if editable { Button(L10n[.Docs.versionWrite]) { startWriting() } }
                            }
                        }
                    }
                    .padding(.horizontal, 24).padding(.vertical, 16)
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
        } else {
            VStack(spacing: 10) {
                Text(L10n[.Docs.versionNone]).foregroundStyle(.secondary)
                if editable { Button(L10n[.Docs.versionAdd]) { newKey = ""; adding = true } }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }

    private func refs(_ detail: ProjectDetail) -> MarkdownRefs {
        NoteEditor.refs(doc, detail, outside: model.isBoardOpen ? model.outsideDetail : nil, open: onOpenPath)
    }

    /// A link or a diagram put in where the cursor is, each picked from the project.
    private func pickers(_ detail: ProjectDetail) -> some View {
        HStack(spacing: 6) {
            PathPickButton(title: L10n[.Docs.insertLink], compactImage: "link", detail: detail, owner: doc, wanted: .mention,
                           outside: model.isBoardOpen ? model.outsideDetail : nil) { path in
                insert(inline: "[[\(path)]]")
            }
            .help(L10n[.Docs.insertLinkHelp])
            PathPickButton(title: L10n[.Docs.insertDiagram], compactImage: "photo.on.rectangle", detail: detail, owner: doc, wanted: .embed) { path in
                insert("![[\(path)]]")
            }
            .help(L10n[.Docs.insertDiagramHelp])
        }
        .fixedSize()
    }

    private func startWriting() {
        guard let open else { return }
        draft = open.body
        draftOf = (doc.id, open.name)
        selection = nil
        writing = true
    }

    private func finishWriting() {
        save()
        writing = false
    }

    private func save() {
        guard let taken = draftOf else { return }
        draftOf = nil
        guard let current = model.schema(taken.card)?.versions.first(where: { $0.name == taken.version }),
              current.body != draft else { return }
        model.updateVersion(taken.card, taken.version, ["body": draft])
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
        selection = TextSelection(insertionPoint: draft.index(at, offsetBy: text.count))
    }
}
