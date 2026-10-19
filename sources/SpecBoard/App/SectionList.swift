import SwiftUI

/// A project's sections, in its ⓘ: the list, a name field each, and a way to add one.
///
/// Only for the open project — the counts come from what is in memory.
struct SectionList: View {
    var model: AppModel
    @State private var draft = ""
    /// Where a new section goes: inside this one — nil, the first of the root's own
    /// (nothing more goes directly under the root).
    @State private var parent: String?

    var body: some View {
        if let detail = model.detail {
            let directory = detail.directory
            VStack(alignment: .leading, spacing: 6) {
                ForEach(detail.sections) { section in
                    let count = directory.members(section.id).count
                    let depth = directory.depth(of: section.id)
                    HStack(spacing: 8) {
                        // The sections are what is open's: a locked main keeps them, the
                        // root's name too (`specboard://SpecBoard/Docs/Design:DesignBranches` §8).
                        CommitField(L10n[.Sections.listNamePlaceholder], text: section.name) {
                            model.renameSection(section.id, to: $0)
                        }
                        .disabled(model.isReadOnly)
                        .frame(width: max(100, 180 - CGFloat(depth) * 16))
                        .padding(.leading, CGFloat(depth) * 16)
                        Text(L10n[.Sections.listCardCount(count)]).font(.app(11)).foregroundStyle(.secondary)
                            .frame(minWidth: 44, alignment: .trailing)
                        // The root says what is open: main, or a branch's view of it.
                        Text(section.parentId != nil ? "" : model.isBoardOpen ? "board" : model.openBranch == nil ? "main" : "branch")
                            .font(.app(10, design: .monospaced)).foregroundStyle(.secondary)
                            .frame(width: 48, alignment: .leading)
                            .help(section.parentId != nil ? ""
                                  : model.openBranch.map { L10n[.Sections.listOpenInBranch($0.name)] }
                                  ?? L10n[.Sections.listOpenInMain])
                        Button(L10n[.Common.actionOpen]) {
                            model.open(section.parentId == nil ? model.rootPlace(section.id) : .section(section.id))
                        }
                            .controlSize(.small)
                            .disabled(detail.sections.count < 2 || section.id == model.shownSectionKey)
                        Button(role: .destructive) {
                            model.deleteSection(section.id)
                        } label: {
                            Image(systemName: "trash")
                        }
                        .buttonStyle(.borderless)
                        .disabled(!NavPanelTree.deletable(section, in: detail)
                                  || count > 0 || !directory.children(of: section.id).isEmpty || model.isReadOnly)
                        .help(section.kind?.isFixed == true
                              ? L10n[.Sections.listCantDeleteFixed]
                              : ProjectSection.isRootOwn(section, root: directory.home) && section.name != ProjectSection.evacuation
                              ? L10n[.Sections.listCantDeleteRootOwn]
                              : count > 0 || !directory.children(of: section.id).isEmpty
                              ? L10n[.Sections.listCantDeleteNotEmpty]
                              : L10n[.Sections.listDelete])
                    }
                }
                HStack(spacing: 8) {
                    TextField(L10n[.Sections.listNewName], text: $draft)
                        .textFieldStyle(.roundedBorder)
                        .frame(width: 150)
                        .onSubmit(add)
                    Picker("", selection: Binding(get: { parent ?? Self.firstOwn(detail) }, set: { parent = $0 })) {
                        ForEach(detail.sections.filter { $0.parentId != nil }) { section in
                            Text(L10n[.Sections.listInside(directory.shown(section: section.id))]).tag(Optional(section.id))
                        }
                    }
                    .labelsHidden()
                    .frame(maxWidth: 170)
                    Button(L10n[.Sections.listAdd], action: add)
                        .controlSize(.small)
                        .disabled(draft.trimmingCharacters(in: .whitespaces).isEmpty || model.isReadOnly)
                    Button(L10n[.Nav.promptCreateBranch]) {
                        let name = draft.trimmingCharacters(in: .whitespaces)
                        guard !name.isEmpty else { return }
                        model.createBranch(name: name)
                        if model.lastError == nil { draft = "" }
                    }
                    .controlSize(.small)
                    .disabled(draft.trimmingCharacters(in: .whitespaces).isEmpty)
                    .help(L10n[.Sections.listCreateBranchHelp])
                }
                Text(detail.sections.count < 2 ? L10n[.Sections.listNoteFew] : L10n[.Sections.listNoteRename])
                    .font(.app(10)).foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }

    private func add() {
        let name = draft.trimmingCharacters(in: .whitespaces)
        guard !name.isEmpty, !model.isReadOnly, let detail = model.detail else { return }
        model.createSection(name: name, parentId: parent ?? Self.firstOwn(detail))
        if model.lastError == nil { draft = "" }
    }

    /// The first of the root's own sections, where a new one goes unless another is chosen.
    private static func firstOwn(_ detail: ProjectDetail) -> String? {
        detail.directory.children(of: detail.directory.home).first.map(\.id)
    }
}
