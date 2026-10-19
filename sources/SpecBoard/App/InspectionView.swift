import SwiftUI

/// The inspection of one tree of the project (`inspection_list`, `specboard://SpecBoard/Docs/Design:CoreApi`): what to
/// put right, what only to know, and what was marked as by design, each group under the core's own
/// label. A press on an item shows where it is, in its tree; an item that is as it was meant to be is
/// marked so, saying why, and the mark taken off again.
///
/// Every tree is looked at from here, picked at the top — the one open, when the page opens — so that
/// what is wrong in a tree is not one tab among a root's views of it. Read off the main thread, and only
/// while the page is shown: the whole tree is checked for it.
struct InspectionView: View {
    var model: AppModel
    /// The parts drawn open when the page opens: what to put right.
    var opened: Set<Inspection.Part> = [.fix]

    /// The tree picked, as the inspection names it; the one open while none is.
    @State private var picked: String?
    /// The parts opened or shut by hand, over `opened`.
    @State private var toggled: Set<Inspection.Part> = []
    /// The item whose mark is being written, and why it is as it is.
    @State private var marking: String?
    @State private var reason = ""

    @Environment(\.colorScheme) private var colorScheme
    private var theme: GraphTheme { colorScheme == .dark ? .dark : .light }

    /// The tree looked at.
    private var tree: String { picked ?? model.inspectionTree }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 12) {
                Text(L10n[.Inspection.pageTitle]).font(.app(13, weight: .semibold))
                Picker("", selection: Binding(get: { tree }, set: { picked = $0 })) {
                    ForEach(trees, id: \.word) { tree in
                        Text(tree.title).tag(tree.word)
                    }
                }
                .labelsHidden()
                .fixedSize()
                .help(L10n[.Inspection.pageTreeHelp])
                Spacer()
            }
            .padding(.horizontal, 20)
            .padding(.vertical, 8)
            Divider()
            ScrollView {
                // One child per line, so that only the lines on screen are built: a tree can have hundreds
                // of cards nothing refers to.
                LazyVStack(alignment: .leading, spacing: 0) {
                    ForEach(lines) { line in
                        lineView(line)
                    }
                }
                .padding(16)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .task(id: "\(model.detail?.project.id ?? "")|\(tree)|\(model.dataGeneration)|\(Language.shared.code)") {
            model.readInspection(tree)
        }
    }

    // MARK: - The trees

    /// A tree that can be looked at: as the inspection names it, and as the nav panel does.
    struct Tree: Hashable {
        var word: String
        var title: String
    }

    /// The trees that can be looked at, in the nav panel's order: main, the branches and the board.
    private var trees: [Tree] { Self.trees(model) }

    static func trees(_ model: AppModel) -> [Tree] {
        let main = model.isLockedGroupOpen ? AppModel.groupBranchName : SchemaDirectory.mainWord
        var out = [Tree(word: main, title: SchemaDirectory.mainWord)]
        out += model.branches.map { Tree(word: $0.name, title: $0.name) }
        out.append(Tree(word: Board.word, title: L10n[.Common.wordProjectBoard]))
        return out
    }

    // MARK: - The lines

    /// One child of the page's lazy list.
    enum Line: Identifiable {
        case reading
        case refused(String)
        case partHead(Inspection.Part, count: Int, open: Bool)
        case nothing(String)
        case groupHead(Inspection.Section, part: Inspection.Part)
        case item(Inspection.Item, part: Inspection.Part)

        var id: String {
            switch self {
            case .reading: "reading"
            case .refused: "refused"
            case .partHead(let part, _, _): "p-\(part)"
            case .nothing(let text): "n-\(text)"
            case .groupHead(let section, let part): "g-\(part)-\(section.group.id)"
            case .item(let item, let part): "i-\(part)-\(item.id)"
            }
        }
    }

    /// What the page lists: the three parts, each open with its groups and items, or shut; while the tree
    /// is first read, that it is being read.
    private var lines: [Line] {
        Self.lines(model.inspection(of: tree), open: isOpen)
    }

    static func lines(_ read: AppModel.InspectionRead?, open isOpen: (Inspection.Part) -> Bool) -> [Line] {
        guard let read else { return [.reading] }
        switch read.list {
        case .failure(let refusal):
            return [.refused(refusal.said)]
        case .success(let list):
            var out: [Line] = []
            for part in Inspection.Part.allCases {
                let open = isOpen(part)
                out.append(.partHead(part, count: list.count(part), open: open))
                guard open else { continue }
                let sections = list.part(part)
                if sections.isEmpty, part == .fix { out.append(.nothing(L10n[.Inspection.listNothingToFix])) }
                for section in sections {
                    out.append(.groupHead(section, part: part))
                    out += section.items.map { .item($0, part: part) }
                }
            }
            return out
        }
    }

    private func isOpen(_ part: Inspection.Part) -> Bool {
        opened.contains(part) != toggled.contains(part)
    }

    private static func title(_ part: Inspection.Part) -> String {
        switch part {
        case .fix: L10n[.Inspection.partFix]
        case .info: L10n[.Inspection.partInfo]
        case .accepted: L10n[.Inspection.partAccepted]
        }
    }

    @ViewBuilder
    private func lineView(_ line: Line) -> some View {
        switch line {
        case .reading:
            Text(L10n[.Inspection.listReading]).font(.app(12)).foregroundStyle(.secondary)
        case .refused(let said):
            Text(said).font(.app(11)).foregroundStyle(.red).textSelection(.enabled)
                .fixedSize(horizontal: false, vertical: true)
        case .partHead(let part, let count, let open):
            Button {
                if toggled.contains(part) { toggled.remove(part) } else { toggled.insert(part) }
            } label: {
                HStack(spacing: 6) {
                    Image(systemName: open ? "chevron.down" : "chevron.right")
                        .font(.app(10)).foregroundStyle(.secondary).frame(width: 12)
                    Text(Self.title(part)).font(.app(12.5, weight: .semibold))
                    Text("\(count)").font(.app(11, design: .monospaced)).foregroundStyle(.secondary)
                    Spacer(minLength: 0)
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .padding(.top, part == .fix ? 0 : 20)
        case .nothing(let text):
            Text(text).font(.app(12)).foregroundStyle(.secondary)
                .padding(.leading, 18).padding(.top, 10)
        case .groupHead(let section, _):
            HStack(spacing: 6) {
                Text(section.group.label).font(.app(11, weight: .semibold)).foregroundStyle(.secondary)
                Text("\(section.items.count)").font(.app(10.5, design: .monospaced)).foregroundStyle(.tertiary)
            }
            .padding(.leading, 18).padding(.top, 14)
        case .item(let item, let part):
            itemRow(item, part)
                .padding(.leading, 18).padding(.top, 6)
        }
    }

    // MARK: - An item

    private func itemRow(_ item: Inspection.Item, _ part: Inspection.Part) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 10) {
            Button {
                // Shown where it is drawn, in its tree; its editor opens on a press twice there.
                model.showInspected(item, in: tree)
            } label: {
                VStack(alignment: .leading, spacing: 2) {
                    Text(item.at)
                        .font(.app(11.5, design: .monospaced))
                        .foregroundStyle(Color(nsColor: theme.brassInk))
                        .multilineTextAlignment(.leading)
                    Text(Self.note(item))
                        .font(.app(10.5)).foregroundStyle(.secondary)
                        .multilineTextAlignment(.leading)
                    if let mark = item.accepted {
                        Text(Self.said(mark))
                            .font(.app(10.5)).foregroundStyle(Color(nsColor: theme.brassInk))
                            .multilineTextAlignment(.leading)
                    } else if let before = item.acceptedBefore {
                        Text(L10n[.Inspection.itemWasAsDesigned(Self.said(before))])
                            .font(.app(10.5)).foregroundStyle(.tertiary)
                            .multilineTextAlignment(.leading)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            if part == .accepted {
                Button(L10n[.Inspection.itemPutBack]) { model.markInspection([item.id], reason: nil, in: tree) }
                    .buttonStyle(.borderless).font(.app(10.5))
                    .disabled(model.isArchived)
                    .help(L10n[.Inspection.itemPutBackHelp])
            } else {
                Button(L10n[.Inspection.itemMarkAsDesignedMenu]) {
                    reason = ""
                    marking = item.id
                }
                .buttonStyle(.borderless).font(.app(10.5))
                .disabled(model.isArchived)
                .help(L10n[.Inspection.itemMarkAsDesignedHelp])
                .popover(isPresented: Binding(get: { marking == item.id }, set: { if !$0 { marking = nil } }),
                         arrowEdge: .bottom) {
                    markForm(item)
                }
            }
        }
    }

    /// What is wrong, and how it might be put right, as the core says it.
    static func note(_ item: Inspection.Item) -> String {
        item.hint.map { "\(item.problem)\n\($0)" } ?? item.problem
    }

    /// Why an item is as it is, and who said so.
    static func said(_ mark: Inspection.Mark) -> String {
        L10n[.Inspection.itemMarkedBy(mark.reason, mark.by)]
    }

    private func markForm(_ item: Inspection.Item) -> some View {
        let typed = reason.trimmingCharacters(in: .whitespacesAndNewlines)
        return VStack(alignment: .leading, spacing: 10) {
            Text(L10n[.Inspection.markAsDesigned]).font(.app(13, weight: .semibold))
            Text(item.at).font(.app(11, design: .monospaced)).foregroundStyle(.secondary)
            TextField(L10n[.Inspection.markReason], text: $reason)
                .textFieldStyle(.roundedBorder)
                .frame(width: 320)
            HStack {
                Spacer()
                Button(L10n[.Common.actionCancelDialog]) { marking = nil }
                    .keyboardShortcut(.cancelAction)
                Button(L10n[.Inspection.markAsDesigned]) {
                    model.markInspection([item.id], reason: typed, in: tree)
                    guard model.lastError == nil else { return }
                    marking = nil
                    reason = ""
                }
                .keyboardShortcut(.defaultAction)
                .disabled(typed.isEmpty)
            }
        }
        .padding(16)
    }
}
