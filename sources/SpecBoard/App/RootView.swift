import SwiftUI

/// The root of what is open — main, or a branch (overview §13) — opened: the project as
/// a whole — what changed, how a branch stands against main, what is asked to merge into
/// main, and what its tags gather. The board's opens on its goals. What is wrong with a tree
/// is the inspection page's (`InspectionView`), for any tree.
///
/// Everything here is already known to the project — the change log, the branch's diff
/// (`Repository.branchDiff`), the tags. It is put where it can be read and followed: each
/// entry goes to the card it is about.
struct RootView: View {
    var model: AppModel
    var root: String
    var tab: AppModel.RootTab
    /// A merge request to show opened, the first time the tab is drawn.
    var openRequest: Int? = nil

    @Environment(\.colorScheme) private var colorScheme
    private var theme: GraphTheme { colorScheme == .dark ? .dark : .light }

    /// 履歴, narrowed to one section's changes, and to what mentions this.
    @State private var historySection: String?
    @State private var historyQuery = ""
    /// The open branch against main, worked out when the root is shown and again
    /// after each write (`AppModel.dataGeneration`).
    @State private var branchDiff: BranchDiff?
    /// 履歴: the runs of commits folded away, what each opened commit changed, and
    /// what is being asked or named.
    @State private var folded: Set<String> = []
    @State private var changes: [String: [CommitChange]] = [:]
    @State private var asking: Ask?
    /// MR: the one opened.
    @State private var shownRequest: String?
    /// MR: "branch で動かした位置も main に入れる" as the person set it, by number; one not
    /// set yet is as the request asks (`MergeRequest.places`).
    @State private var requestPlaces: [Int: Bool] = [:]
    /// A commit's name, or why a merge request is rejected, being asked for (`NameSheet`).
    @State private var naming: NameAsk?
    /// 目標: the board's goals, as the core lists them.
    @State private var goals: [BoardGoal] = []
    /// What the lists are worked out from, kept between drawings (`RootTabsMemo`).
    @State private var memo = RootTabsMemo()

    var body: some View {
        if let detail = model.detail, let section = detail.directory.section(id: root) {
            VStack(alignment: .leading, spacing: 0) {
                HStack(spacing: 10) {
                    // The branch open, plainly; the root's own name small, as in the nav panel.
                    Text(model.isBoardOpen ? L10n[.Common.wordProjectBoard] : model.openTree.name)
                        .font(.app(14, weight: .semibold))
                    Text(section.name)
                        .font(.app(10)).foregroundStyle(.secondary)
                        .padding(.horizontal, 5).padding(.vertical, 1)
                        .overlay(RoundedRectangle(cornerRadius: 3).stroke(.tertiary))
                    Spacer()
                    Picker("", selection: Binding(get: { shown }, set: { model.showRoot($0) })) {
                        ForEach(model.rootTabs, id: \.self) { tab in
                            Text(tab == .mr && model.openMergeRequests > 0 ? "MR \(model.openMergeRequests)" : tab.title).tag(tab)
                        }
                    }
                    .labelsHidden()
                    .pickerStyle(.segmented)
                    .fixedSize()
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 10)
                Divider()
                ScrollView {
                    switch shown {
                    case .goals:
                        VStack(alignment: .leading, spacing: 18) {
                            GoalList(model: model, goals: goals)
                        }
                        .padding(16)
                        .frame(maxWidth: .infinity, alignment: .leading)
                    default:
                        // One child per line, so that only the lines on screen are built: a history of
                        // commits that each changed thousands of lines would otherwise be laid out whole.
                        LazyVStack(alignment: .leading, spacing: 0) {
                            ForEach(pieces(of: shown, detail)) { piece in
                                pieceView(piece, detail)
                            }
                        }
                        .padding(16)
                        .frame(maxWidth: .infinity, alignment: .leading)
                    }
                }
            }
            .task(id: "\(model.openTree.id ?? "")|\(model.dataGeneration)") {
                branchDiff = model.branchDiff()
                changes = [:]
                goals = model.boardGoals()
            }
            // The requests are read as the tab opens, and again after each write while it is open: a write
            // in main can leave one stale. A write elsewhere reads only what it changes.
            .task(id: "\(shown)|\(model.dataGeneration)") {
                if shown == .mr { model.readMergeRequests() }
            }
            .onAppear {
                if let openRequest { shownRequest = model.mergeRequests.first { $0.number == openRequest }?.id }
            }
            .confirmationDialog(asking.map(title) ?? "", isPresented: Binding(get: { asking != nil }, set: { if !$0 { asking = nil } }),
                                presenting: asking) { ask in
                switch ask {
                case .revert(let commit, _):
                    Button(L10n[.Root.commitRevert]) { model.revert(to: commit.seq) }
                case .undo(let commit):
                    Button(L10n[.Root.commitUndo]) { model.undo(commit: commit.seq) }
                case .cut(let commit, _, _):
                    Button(L10n[.Root.commitCut], role: .destructive) { model.cut(before: commit.seq) }
                }
                Button(L10n[.Common.actionCancel], role: .cancel) {}
            } message: { ask in
                switch ask {
                case .revert:
                    Text(L10n[.Root.askRevertMessage])
                case .undo:
                    Text(L10n[.Root.askUndoMessage])
                case .cut:
                    Text(L10n[.Root.askCutMessage])
                }
            }
            .nameSheet($naming)
        }
    }

    /// The tab drawn: one the tree open does not have is its first.
    private var shown: AppModel.RootTab {
        let tabs = model.rootTabs
        return tabs.contains(tab) ? tab : tabs.first ?? .history
    }


    /// The key the lists' kept workings are kept under: the project, the tree open, and what was read of it.
    private var memoKey: String {
        "\(model.detail?.project.id ?? "")|\(model.openTree.id ?? "")|\(model.dataGeneration)"
    }

    // MARK: - The lists, one piece at a time

    /// One child of a tab's lazy list: what it draws, the space above it, and the box it is a part of —
    /// a commit, a merge request — when it is one. A box is drawn piece by piece, each with its share of
    /// the box's background, so that a commit of thousands of lines is thousands of children, each built
    /// only when it comes on screen.
    struct Piece: Identifiable {
        indirect enum Kind {
            case note(String)
            case nothing(String)
            case size(ProjectSize)
            case takeInMain(String)
            case groupHead(String, count: Int)
            case link(String, path: String?, note: String?)
            case cardLink(SchemaNode)
            case plain(String)
            case tagGroup(TagGroup)
            case historyFilter
            case fold(id: String, count: Int, from: Int, to: Int)
            case commitHead(CommitInfo, latest: Int, oldest: Int)
            case commitLine(ChangeEvent)
            case noChanges
            case change(CommitChange)
            case diffHead
            case requestHead(MergeRequest, open: Bool)
            case requestText(String)
            case requestReason(String)
            case resultSection(String)
            case resultItem(MergeRequest.Result.Item, opening: Int?)
            case conflict(MergeRequest.Result.Conflict)
            case warning(String)
            case stale
            case requestActions(MergeRequest)
            /// A piece with what a branch did to it before it (`ChangeMark`): A, M, D in their colours.
            case marked(ChangeMark, Kind)
        }
        enum Tint { case commit, mark, live, closed }
        /// The box a piece is a part of: rounded at its first piece's top and its last piece's bottom.
        struct Box {
            var tint: Tint
            var opacity: Double
            var first: Bool
            var last: Bool
        }
        var id: String
        var kind: Kind
        /// The space above it: inside the box for a box's later pieces, outside for the rest.
        var gap: CGFloat
        var box: Box?
    }

    /// A tab's list as it is put together: each entry 18 below the one before, and the pieces of one
    /// entry as close as its own lines are.
    struct Pieces {
        private(set) var all: [Piece] = []
        private var outer: CGFloat { all.isEmpty ? 0 : 18 }

        mutating func add(_ id: String, _ kind: Piece.Kind) {
            all.append(Piece(id: id, kind: kind, gap: outer))
        }

        /// A title with its count, and its entries 6 apart.
        mutating func group(_ id: String, _ title: String, count: Int, _ items: [(id: String, kind: Piece.Kind)]) {
            add(id, .groupHead(title, count: count))
            all.reserveCapacity(all.count + items.count)
            for item in items { all.append(Piece(id: id + "/" + item.id, kind: item.kind, gap: 6)) }
        }

        /// One box, each part with the space above it inside the box (the first part's is the box's own).
        mutating func box(_ id: String, tint: Piece.Tint, opacity: Double,
                          _ parts: [(id: String, kind: Piece.Kind, gap: CGFloat)]) {
            let gap = outer
            all.reserveCapacity(all.count + parts.count)
            for (index, part) in parts.enumerated() {
                let box = Piece.Box(tint: tint, opacity: opacity, first: index == 0, last: index == parts.count - 1)
                all.append(Piece(id: id + "/" + part.id, kind: part.kind, gap: index == 0 ? gap : part.gap, box: box))
            }
        }
    }

    private func pieces(of tab: AppModel.RootTab, _ detail: ProjectDetail) -> [Piece] {
        var list = Pieces()
        switch tab {
        case .history: historyPieces(detail, &list)
        case .diff: diffPieces(&list)
        case .mr: requestPieces(&list)
        case .tags: tagPieces(detail, &list)
        case .goals: break
        }
        return list.all
    }

    @ViewBuilder
    private func pieceView(_ piece: Piece, _ detail: ProjectDetail) -> some View {
        if let box = piece.box {
            let top: CGFloat = box.first ? 6 : 0, bottom: CGFloat = box.last ? 6 : 0
            pieceContent(piece.kind, detail)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 8)
                .padding(.top, box.first ? 6 : piece.gap)
                .padding(.bottom, bottom)
                .background(UnevenRoundedRectangle(topLeadingRadius: top, bottomLeadingRadius: bottom,
                                                   bottomTrailingRadius: bottom, topTrailingRadius: top)
                    .fill(tint(box.tint)))
                .opacity(box.opacity)
                .padding(.top, box.first ? piece.gap : 0)
        } else {
            pieceContent(piece.kind, detail)
                .padding(.top, piece.gap)
        }
    }

    private func tint(_ tint: Piece.Tint) -> Color {
        switch tint {
        case .commit: Color.secondary.opacity(0.04)
        case .mark: Color(nsColor: theme.brassInk).opacity(0.08)
        case .live: Color.secondary.opacity(0.07)
        case .closed: Color.secondary.opacity(0.03)
        }
    }

    @ViewBuilder
    private func pieceContent(_ kind: Piece.Kind, _ detail: ProjectDetail) -> some View {
        switch kind {
        case .note(let text):
            Text(text).font(.app(11)).foregroundStyle(.secondary)
        case .nothing(let text):
            nothing(text)
        case .size(let size):
            sizeLine(size)
        case .takeInMain(let branch):
            // What main did since: taken in from here (`specboard://SpecBoard/Docs/Design:DesignBranches` §6).
            Button(L10n[.Root.diffTakeInMain]) { model.askToMerge(from: nil, into: branch) }
                .controlSize(.small)
                .disabled(model.isReadOnly)
                .help(L10n[.Root.diffTakeInMainHelp])
        case .groupHead(let title, let count):
            HStack(spacing: 6) {
                Text(title).font(.app(11, weight: .semibold)).foregroundStyle(.secondary)
                Text("\(count)").font(.app(10.5, design: .monospaced)).foregroundStyle(.tertiary)
            }
        case .link(let text, let path, let note):
            link(text, path, note: note)
        case .cardLink(let card):
            let path = detail.directory.full(card)
            link(path, path)
        case .plain(let text):
            Text(text).font(.app(11.5, design: .monospaced))
        case .tagGroup(let tagGroup):
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Text(tagGroup.name).font(.app(11.5, weight: .semibold))
                if tagGroup.exclusive {
                    Text(L10n[.Root.tagGroupExclusive]).font(.app(10)).foregroundStyle(.secondary)
                }
                Text(tagGroup.tags.joined(separator: L10n[.Common.separatorList])).font(.app(11)).foregroundStyle(.secondary)
            }
        case .historyFilter:
            historyFilter(detail)
        case .fold(let id, let count, let from, let to):
            Button {
                if folded.contains(id) { folded.remove(id) } else { folded.insert(id) }
            } label: {
                HStack(spacing: 6) {
                    Image(systemName: folded.contains(id) ? "chevron.right" : "chevron.down")
                    Text(L10n[.Root.historyFold(count, "\(from)", "\(to)")])
                }
                .font(.app(10.5)).foregroundStyle(.secondary)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
        case .commitHead(let commit, let latest, let oldest):
            commitHead(commit, latest: latest, oldest: oldest)
        case .commitLine(let event):
            lineRow(event, detail)
        case .noChanges:
            Text(L10n[.Root.historyNoDocumentChanges]).font(.app(10.5)).foregroundStyle(.tertiary)
        case .change(let change):
            changeRow(change)
        case .diffHead:
            diffHead()
        case .requestHead(let request, let open):
            requestHead(request, open: open)
        case .requestText(let text):
            Text(text).font(.app(11.5)).textSelection(.enabled).fixedSize(horizontal: false, vertical: true)
        case .requestReason(let reason):
            Text(L10n[.Root.requestReason(reason)]).font(.app(11)).foregroundStyle(.secondary)
        case .resultSection(let section):
            Text(section).font(.app(10.5, weight: .semibold)).foregroundStyle(.secondary).padding(.top, 2)
                .padding(.leading, 6)
        case .resultItem(let item, let number):
            resultItem(item, opening: number)
        case .conflict(let conflict):
            conflictView(conflict)
        case .warning(let warning):
            Text(L10n[.Root.requestWarning(warning)]).font(.app(10.5)).foregroundStyle(.orange).padding(.leading, 6)
        case .stale:
            Text(L10n[.Root.requestStale])
                .font(.app(11)).foregroundStyle(.orange)
        case .requestActions(let request):
            requestActions(request)
        case .marked(let mark, let inner):
            HStack(alignment: .firstTextBaseline, spacing: 6) {
                ChangeMarkText(mark: mark).frame(minWidth: 12, alignment: .leading)
                AnyView(pieceContent(inner, detail))
            }
        }
    }

    // MARK: - Going to what an entry is about

    private func link(_ text: String, _ path: String?, note: String? = nil) -> some View {
        Button {
            // Shown where it is drawn; its editor opens on a press twice there.
            if let path { model.show(path: path, from: nil) }
        } label: {
            VStack(alignment: .leading, spacing: 2) {
                Text(text)
                    .font(.app(11.5, design: .monospaced))
                    .foregroundStyle(path == nil ? Color.primary : Color(nsColor: theme.brassInk))
                    .multilineTextAlignment(.leading)
                if let note {
                    Text(note).font(.app(10.5)).foregroundStyle(.secondary)
                        .multilineTextAlignment(.leading)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(path == nil)
    }

    private func nothing(_ text: String) -> some View {
        Text(text).font(.app(12)).foregroundStyle(.secondary)
    }

    // MARK: - The size of the project

    /// How much of the store the project takes (`specboard://SpecBoard/Docs/Design:DesignBranches` §7), and the way
    /// to the history of the branch whose history is biggest, to cut it.
    private func sizeLine(_ size: ProjectSize) -> some View {
        let biggest = size.branches.max { $0.history < $1.history }
        return HStack(spacing: 10) {
            Text(size.label).font(.app(11)).foregroundStyle(.secondary)
                .help(L10n[.Root.sizeHelp])
            if let biggest, biggest.history > 0 {
                Button(L10n[.Root.sizeCutLink(biggest.name)]) {
                    if biggest.id != model.openBranch?.id { model.openBranch(biggest.id) }
                    model.showRoot(.history)
                }
                .buttonStyle(.borderless).font(.app(10.5))
            }
        }
    }

    // MARK: - 履歴

    /// The open branch's commits, newest first (`specboard://SpecBoard/Docs/Design:DesignBranches` §7): who made
    /// each and what it did, line by line, with the marks — a merge, an export, a name —
    /// standing out and what is between them folded away on asking. From each: what it
    /// changed, going back to right after it, a name, and letting go of what is before it.
    /// Unfiltered, nothing is looked up for a line until it is drawn; filtered, each line is looked up
    /// in what was worked out once for what was read (`RootTabsMemo`).
    private func historyPieces(_ detail: ProjectDetail, _ list: inout Pieces) {
        // Asked of the core once for what was read, not on each drawing.
        Self.historyPieces(detail, commits: model.commits, pruned: model.pruned[detail.project.id],
                           section: historySection, query: historyQuery, folded: folded, changes: changes,
                           memo: memo, key: memoKey, size: memo.size(memoKey, model.projectSize), into: &list)
    }

    /// The history's list: how big the project is — the history goes with it, where it is cut — the line
    /// said when old commits were let go, the filter, and each commit a box of its head and its lines — one
    /// piece a line — the commits between two marks foldable.
    static func historyPieces(_ detail: ProjectDetail, commits: [CommitInfo], pruned: Int?, section: String?, query: String,
                              folded: Set<String>, changes: [String: [CommitChange]], memo: RootTabsMemo, key: String,
                              size: ProjectSize? = nil, into list: inout Pieces) {
        let directory = detail.directory
        if let size { list.add("size", .size(size)) }
        if let gone = pruned, gone > 0 {
            list.add("pruned", .note(L10n[.Root.historyPruned(gone, "\(HistoryStop.keptCommits)", "\(HistoryStop.keptDays)")]))
        }
        list.add("filter", .historyFilter)
        let query = query.trimmingCharacters(in: .whitespaces).lowercased()
        let filtering = section != nil || !query.isEmpty
        let shown: [Shown] = commits.compactMap { commit in
            guard filtering else { return Shown(commit: commit, lines: commit.lines) }
            let lines = commit.lines.filter { event in
                let card = memo.locate(event, detail, key)?.card
                if let section {
                    guard let card, directory.sectionId(of: card) == section || event.targetId == section else { return false }
                }
                if !query.isEmpty, !event.summary.lowercased().contains(query),
                   !(card.map { memo.searchPath($0, directory).contains(query) } ?? false) {
                    return false
                }
                return true
            }
            return lines.isEmpty ? nil : Shown(commit: commit, lines: lines)
        }
        if shown.isEmpty { list.add("nothing", .nothing(L10n[.Root.historyEmpty])) }
        let latest = commits.first?.seq ?? 0, oldest = commits.last?.seq ?? 0
        for run in runs(shown) {
            if run.foldable {
                list.add("fold-" + run.id, .fold(id: run.id, count: run.items.count, from: run.items.last?.commit.seq ?? 0,
                                                 to: run.items.first?.commit.seq ?? 0))
            }
            guard !run.foldable || !folded.contains(run.id) else { continue }
            for item in run.items {
                let commit = item.commit
                var parts: [(id: String, kind: Piece.Kind, gap: CGFloat)] = [("head", .commitHead(commit, latest: latest, oldest: oldest), 0)]
                parts.reserveCapacity(item.lines.count + 1)
                for event in item.lines { parts.append(("l-" + event.id, .commitLine(event), 4)) }
                if let said = changes[commit.id] {
                    if said.isEmpty { parts.append(("none", .noChanges, 4)) }
                    for (index, change) in said.enumerated() { parts.append(("x-\(index)", .change(change), index == 0 ? 4 : 3)) }
                }
                list.box("c-" + commit.id, tint: commit.isMark ? .mark : .commit, opacity: commit.reverted ? 0.6 : 1, parts)
            }
        }
    }

    private func historyFilter(_ detail: ProjectDetail) -> some View {
        HStack(spacing: 8) {
            Picker("", selection: $historySection) {
                Text(L10n[.Root.historyAllSections]).tag(String?.none)
                ForEach(detail.sections) { section in
                    Text(detail.directory.shownFromTop(section: section.id)).tag(Optional(section.id))
                }
            }
            .labelsHidden()
            .fixedSize()
            TextField(L10n[.Root.historyQueryPlaceholder], text: $historyQuery)
                .textFieldStyle(.roundedBorder)
                .frame(maxWidth: 260)
            Spacer(minLength: 0)
            Text(L10n[.Root.historyMenuHint])
                .font(.app(10.5)).foregroundStyle(.tertiary)
        }
    }

    /// A commit and the lines of it that are shown.
    private struct Shown {
        var commit: CommitInfo
        var lines: [ChangeEvent]
    }

    /// A mark on its own, or the commits between two marks — folded away on asking
    /// when there are a few of them.
    private struct Run: Identifiable {
        var items: [Shown]
        var foldable: Bool
        var id: String { items.first?.commit.id ?? "" }
    }

    private static func runs(_ shown: [Shown]) -> [Run] {
        var out: [Run] = [], between: [Shown] = []
        func close() {
            guard !between.isEmpty else { return }
            out.append(Run(items: between, foldable: between.count >= 3))
            between = []
        }
        for item in shown {
            if item.commit.isMark {
                close()
                out.append(Run(items: [item], foldable: false))
            } else {
                between.append(item)
            }
        }
        close()
        return out
    }

    /// What kind of commit it is, when it is not a plain edit.
    private static func badge(_ commit: CommitInfo) -> String? {
        switch commit.kind {
        case "merge": L10n[.Root.commitBadgeMerge]
        case "export": L10n[.Common.wordExport]
        case "revert": L10n[.Root.commitRevert]
        case "batch": L10n[.Root.commitBadgeBatch]
        case "record": L10n[.Root.commitBadgeRecord]
        case "form": L10n[.Root.commitBadgeForm]
        default: nil
        }
    }

    private func commitHead(_ commit: CommitInfo, latest: Int, oldest: Int) -> some View {
        let open = changes[commit.id] != nil
        return HStack(alignment: .firstTextBaseline, spacing: 8) {
            Text("#\(commit.seq)").font(.app(10.5, weight: .semibold, design: .monospaced)).foregroundStyle(.secondary)
            Text(Self.when(commit.at)).font(.app(10.5, design: .monospaced)).foregroundStyle(.tertiary)
            Text(commit.authorLabel).font(.app(10.5)).foregroundStyle(.secondary)
            if let badge = Self.badge(commit) {
                Text(badge).font(.app(10)).foregroundStyle(.secondary)
                    .padding(.horizontal, 5).padding(.vertical, 1)
                    .overlay(RoundedRectangle(cornerRadius: 3).stroke(.tertiary))
            }
            if let name = commit.name {
                Text(L10n[.Root.commitNameQuoted(name)]).font(.app(11.5, weight: .semibold)).foregroundStyle(Color(nsColor: theme.brassInk))
            }
            if commit.reverted {
                Text(L10n[.Root.commitReverted]).font(.app(10)).foregroundStyle(.secondary)
            }
            Spacer(minLength: 0)
            Button(open ? L10n[.Root.commitHideChanges] : L10n[.Root.commitShowChanges]) {
                changes[commit.id] = open ? nil : model.commitChanges(commit.seq)
            }
            .buttonStyle(.borderless).font(.app(10.5))
            Menu {
                Button(L10n[.Root.commitRevertToHere]) {
                    asking = .revert(commit, count: model.commits.filter { $0.seq > commit.seq }.count)
                }
                .disabled(commit.seq == latest || !model.mayGoBack)
                // The newest commit may be undone too: only going back to just after it is nothing.
                Button(L10n[.Root.commitUndoOne]) { asking = .undo(commit) }
                    .disabled(!model.mayUndoCommit)
                Button(commit.name == nil ? L10n[.Root.commitAddName] : L10n[.Common.actionRenameMenu]) {
                    // Emptied, a name is taken off: so empty is a decision only when there is one.
                    naming = NameAsk(title: L10n[.Root.commitNameTitle], message: L10n[.Root.commitNameMessage],
                                     placeholder: L10n[.Root.commitNamePlaceholder], start: commit.name ?? "",
                                     confirm: L10n[.Root.commitNameConfirm],
                                     cancel: L10n[.Common.actionCancel], allowsEmpty: commit.name != nil) { typed in
                        model.nameCommit(commit.seq, typed)
                    }
                }
                .disabled(model.isArchived)
                Divider()
                Button(L10n[.Root.commitCutBefore]) {
                    let cost = model.cutCost(before: commit.seq)
                    asking = .cut(commit, commits: cost.commits, bytes: cost.bytes)
                }
                .disabled(commit.seq == oldest || model.isArchived)
            } label: {
                Image(systemName: "ellipsis.circle")
            }
            .menuStyle(.borderlessButton)
            .menuIndicator(.hidden)
            .fixedSize()
        }
    }

    /// One line of a commit: where it is is looked up as it is drawn.
    private func lineRow(_ event: ChangeEvent, _ detail: ProjectDetail) -> some View {
        let directory = detail.directory
        let located = memo.locate(event, detail, memoKey)
        let (card, field) = (located?.card, located?.field)
        // A section's change goes to the section, when it is still there.
        let section = event.targetType == .section
            ? event.targetId.flatMap { directory.section(id: $0) } : nil
        return Button {
            if let card {
                model.showCard(card.id)
                if let field { model.selectField(card.id, field.id, follow: false) }
            } else if let section {
                model.open(section.parentId == nil ? model.rootPlace(section.id) : .section(section.id))
            }
        } label: {
            VStack(alignment: .leading, spacing: 1) {
                Text(event.summary).font(.app(11.5)).multilineTextAlignment(.leading)
                if let card {
                    Text(field.map { PathText.member(directory.shown(card), $0.name) } ?? directory.shown(card))
                        .font(.app(10.5, design: .monospaced))
                        .foregroundStyle(Color(nsColor: theme.brassInk))
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(card == nil && section == nil)
    }

    /// One thing a commit changed, before against after.
    private func changeRow(_ change: CommitChange) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 6) {
            // At least as wide as the Japanese words, wider where a language's are.
            Text(change.change == .add ? L10n[.Root.changeAdded]
                 : change.change == .remove ? L10n[.Root.changeDeleted] : L10n[.Root.changeEdited])
                .font(.app(10)).foregroundStyle(.secondary).frame(minWidth: 34, alignment: .leading).fixedSize()
            VStack(alignment: .leading, spacing: 1) {
                Text(change.path + (change.section ? L10n[.Root.itemSectionSuffix] : "")).font(.app(10.5, design: .monospaced))
                ForEach(change.details, id: \.self) { line in
                    Text(line).font(.app(10.5)).foregroundStyle(.secondary).textSelection(.enabled)
                }
            }
        }
        .padding(.leading, 6)
    }

    /// What is asked before it is done: going back, and letting go.
    enum Ask: Identifiable {
        case revert(CommitInfo, count: Int)
        case undo(CommitInfo)
        case cut(CommitInfo, commits: Int, bytes: Int)
        var id: String {
            switch self {
            case .revert(let commit, _): "revert-\(commit.id)"
            case .undo(let commit): "undo-\(commit.id)"
            case .cut(let commit, _, _): "cut-\(commit.id)"
            }
        }
    }

    private func title(_ ask: Ask) -> String {
        switch ask {
        case .revert(let commit, let count):
            model.openBranch == nil
                ? L10n[.Root.askRevertMain(count, "\(commit.seq)")]
                : L10n[.Root.askRevertBranch(count, "\(commit.seq)")]
        case .undo(let commit):
            model.isMainTreeOpen
                ? L10n[.Root.askUndoMain("\(commit.seq)")]
                : L10n[.Root.askUndoBranch("\(commit.seq)")]
        case .cut(let commit, let commits, let bytes):
            L10n[.Root.askCut(commits, "\(commit.seq)", ProjectSize.amount(bytes))]
        }
    }

    private enum Stamps {
        static let fractional: ISO8601DateFormatter = {
            let reader = ISO8601DateFormatter()
            reader.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
            return reader
        }()
        static let whole = ISO8601DateFormatter()
        static let shown: DateFormatter = {
            let writer = DateFormatter()
            writer.locale = Locale(identifier: "en_US_POSIX")
            writer.dateFormat = "MM-dd HH:mm"
            return writer
        }()
    }

    /// `2026-09-28T10:04:12.123Z` as `09-28 10:04`, local.
    static func when(_ stamp: String) -> String {
        guard let date = Stamps.fractional.date(from: stamp) ?? Stamps.whole.date(from: stamp) else { return stamp }
        return Stamps.shown.string(from: date)
    }

    /// One thing a branch is told of main or of the other branches: what, and more about it.
    struct Notice {
        var text: String
        var note: String?
    }

    /// What a branch is told of main and of the other branches, at the head of its diff
    /// (`specboard://SpecBoard/Docs/Design:DesignBranches` §3・§11): what main did since the branch was made,
    /// what the branch cannot show because main has it at the same path, and what
    /// another branch has at a path this one has.
    static func notices(_ diff: BranchDiff) -> [(title: String, items: [Notice])] {
        [
            (L10n[.Root.noticeMainChanged], diff.mainChanged.map {
                Notice(text: $0.path, note: $0.changes.isEmpty ? nil : $0.changes.joined(separator: "\n"))
            }),
            (L10n[.Root.noticeMainMade], diff.mainMade.map { Notice(text: $0) }),
            (L10n[.Root.noticeMainRemoved], diff.mainRemoved.map { Notice(text: $0) }),
            (L10n[.Root.noticeOverlaps], diff.overlaps.map { Notice(text: $0) }),
            (L10n[.Root.noticeElsewhere], diff.elsewhere.map {
                Notice(text: $0.path, note: $0.branches.joined(separator: L10n[.Common.separatorList]))
            }),
        ].filter { !$0.items.isEmpty }
    }

    // MARK: - MR

    /// Requests to merge a branch into main (`specboard://SpecBoard/Docs/Design:DesignBranches` §8): each with
    /// what the merge was tried to do, to approve as it is or reject saying why. The one opened lists
    /// what goes in by section (a list of every path said nothing of what changes), each conflict with
    /// the three sides and what was taken, and the paths it would break.
    private func requestPieces(_ list: inout Pieces) {
        if model.openBranch != nil {
            list.add("main", .nothing(L10n[.Root.mrSeeAtMain]))
            return
        }
        if model.mergeRequests.isEmpty {
            list.add("none", .nothing(L10n[.Root.mrNone]))
            return
        }
        list.add("intro", .note(L10n[.Root.mrIntro]))
        for request in model.mergeRequests {
            let open = shownRequest == request.id
            let live = request.state == .open || request.state == .stale
            var parts: [(id: String, kind: Piece.Kind, gap: CGFloat)] = [("head", .requestHead(request, open: open), 0)]
            if open {
                if !request.description.isEmpty { parts.append(("text", .requestText(request.description), 6)) }
                if let reason = request.reason, request.state == .rejected || request.state == .withdrawn {
                    parts.append(("reason", .requestReason(reason), 6))
                }
                // The result's own lines are 3 apart, and 6 below what is above them.
                var gap: CGFloat = 6
                func result(_ id: String, _ kind: Piece.Kind) {
                    parts.append((id, kind, gap))
                    gap = 3
                }
                // A card that goes in or changes, pressed, opens what main would be once merged on it
                // (`opening`: the request's number while it is out).
                for (index, group) in Self.bySection(request.result.items).enumerated() {
                    result("s\(index)", .resultSection(group.section))
                    for (at, item) in group.items.enumerated() {
                        result("s\(index)-\(at)", .resultItem(item, opening: live ? request.number : nil))
                    }
                }
                for (index, conflict) in request.result.conflicts.enumerated() { result("k\(index)", .conflict(conflict)) }
                for (index, warning) in request.result.warnings.enumerated() { result("w\(index)", .warning(warning)) }
                if request.state == .stale { parts.append(("stale", .stale, 6)) }
                if live { parts.append(("act", .requestActions(request), 6)) }
            }
            list.box("r-" + request.id, tint: live ? .live : .closed, opacity: live ? 1 : 0.7, parts)
        }
    }

    private func requestHead(_ request: MergeRequest, open: Bool) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Button {
                shownRequest = open ? nil : request.id
            } label: {
                HStack(alignment: .firstTextBaseline, spacing: 8) {
                    Image(systemName: open ? "chevron.down" : "chevron.right").font(.app(9.5)).foregroundStyle(.secondary)
                    Text("#\(request.number)").font(.app(10.5, weight: .semibold, design: .monospaced)).foregroundStyle(.secondary)
                    Text(request.title).font(.app(12, weight: .semibold))
                    Text("\(request.branchName) → main").font(.app(10.5, design: .monospaced)).foregroundStyle(.secondary)
                    Spacer(minLength: 0)
                    Text(request.stateLabel).font(.app(10)).foregroundStyle(request.state == .stale ? Color.orange : .secondary)
                        .padding(.horizontal, 5).padding(.vertical, 1)
                        .overlay(RoundedRectangle(cornerRadius: 3).stroke(.tertiary))
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            HStack(spacing: 8) {
                Text(request.authorLabel).font(.app(10.5)).foregroundStyle(.secondary)
                Text(Self.when(request.at)).font(.app(10.5, design: .monospaced)).foregroundStyle(.tertiary)
            }
        }
    }

    private func resultItem(_ item: MergeRequest.Result.Item, opening number: Int?) -> some View {
        HStack(spacing: 6) {
            ChangeMarkText(mark: item.change == "add" ? .made : item.change == "remove" ? .removed : .changed, size: 10)
                .frame(minWidth: 12, alignment: .leading)
            if let number, item.change != "remove", !item.section {
                Button(item.path) { model.openRequestResult(number, card: item.path) }
                    .buttonStyle(.link).font(.app(10.5, design: .monospaced))
                    .help(L10n[.Root.mrOpenResultHelp])
            } else {
                Text(item.path + (item.section ? L10n[.Root.itemSectionSuffix] : "")).font(.app(10.5, design: .monospaced))
            }
        }
        .padding(.leading, 8)
        .padding(.leading, 6)
    }

    private func conflictView(_ conflict: MergeRequest.Result.Conflict) -> some View {
        VStack(alignment: .leading, spacing: 1) {
            Text(L10n[.Root.conflictTitle(conflict.path, Merge3.shown(conflict.unit),
                                          conflict.took == "source" ? L10n[.Root.conflictTookBranch] : L10n[.Root.conflictKeptMain])])
                .font(.app(10.5, weight: .semibold))
            ForEach([(L10n[.Root.conflictBase], conflict.base), ("branch", conflict.source), ("main", conflict.target)], id: \.0) { side in
                Text("\(side.0): \(side.1.map { $0.count > 120 ? String($0.prefix(120)) + "…" : $0 } ?? L10n[.Root.conflictAbsent])")
                    .font(.app(10, design: .monospaced)).foregroundStyle(.secondary).lineLimit(2)
            }
        }
        .padding(.leading, 6)
    }

    private func requestActions(_ request: MergeRequest) -> some View {
        let places = Binding(get: { requestPlaces[request.number] ?? request.places },
                             set: { requestPlaces[request.number] = $0 })
        return VStack(alignment: .leading, spacing: 6) {
            Toggle(MergeSheet.placesLabel(intoMain: true, count: request.result.places ?? 0)
                   + (request.places ? L10n[.Root.requestByAI] : ""), isOn: places)
                .toggleStyle(.checkbox)
                .font(.app(11))
                .disabled(request.state != .open)
                .help(L10n[.Root.requestPlacesHelp])
            HStack {
                // What main would be once merged, looked at as a tree.
                Button(L10n[.Root.requestView]) { model.openRequestResult(request.number, places: places.wrappedValue) }
                    .help(L10n[.Root.requestViewHelp])
                Spacer()
                Button(L10n[.Root.requestReject]) {
                    naming = NameAsk(title: L10n[.Root.requestRejectTitle], message: L10n[.Root.requestRejectMessage],
                                     placeholder: L10n[.Root.requestRejectPlaceholder], confirm: L10n[.Root.requestRejectConfirm],
                                     cancel: L10n[.Common.actionCancel]) { typed in
                        model.rejectMergeRequest(request.number, reason: typed)
                    }
                }
                Button(L10n[.Root.requestApprove]) {
                    model.approveMergeRequest(request.number, places: places.wrappedValue)
                }
                .disabled(request.state != .open)
            }
        }
    }

    /// A result's items by the section each is in (a section's own by itself), in the order they come.
    static func bySection(_ items: [MergeRequest.Result.Item]) -> [(section: String, items: [MergeRequest.Result.Item])] {
        var out: [(section: String, items: [MergeRequest.Result.Item])] = []
        var at: [String: Int] = [:]
        for item in items {
            let section = item.section ? item.path : PathText.splitSection(item.path).section ?? ""
            if let index = at[section] {
                out[index].items.append(item)
            } else {
                at[section] = out.count
                out.append((section, [item]))
            }
        }
        return out
    }

    // MARK: - 差分

    private func diffPieces(_ list: inout Pieces) {
        Self.diffPieces(branchDiff, open: model.openBranch, into: &list)
    }

    /// The diff's list: for a branch, what it is told of main and the other branches, the way to take main in,
    /// then how it stands against main.
    static func diffPieces(_ branchDiff: BranchDiff?, open: BranchInfo?, into list: inout Pieces) {
        guard let open else {
            list.add("none", .nothing(L10n[.Root.diffNeedsBranch]))
            return
        }
        guard let diff = branchDiff else { return }
        // What main and the other branches did, said first: what main did since is taken in from here.
        let notices = Self.notices(diff)
        if !diff.mainChanged.isEmpty || !diff.mainMade.isEmpty || !diff.mainRemoved.isEmpty {
            list.add("takeIn", .takeInMain(open.id))
        }
        for entry in notices {
            list.group("n-" + entry.title, entry.title, count: entry.items.count, entry.items.enumerated().map {
                ("\($0.offset)", .link($0.element.text, path: nil, note: $0.element.note))
            })
        }
        list.add("head", .diffHead)
        if diff.isClean { list.add("clean", .nothing(L10n[.Root.diffClean])) }
        // Each path with the letter of what the branch did to it before it.
        if !diff.changed.isEmpty {
            list.group("changed", L10n[.Root.diffChangedCards], count: diff.changed.count, diff.changed.enumerated().map {
                ("\($0.offset)", .marked(.changed, .link($0.element.path, path: $0.element.path,
                                                         note: $0.element.changes.joined(separator: "\n"))))
            })
        }
        let lists: [(String, String, [String], ChangeMark, Bool)] = [
            ("made", L10n[.Root.diffMadeCards], diff.made, .made, true),
            ("removed", L10n[.Root.diffRemovedCards], diff.removed, .removed, false),
            ("sectionsChanged", L10n[.Root.diffChangedSections], diff.sectionsChanged, .changed, false),
            ("sectionsMade", L10n[.Root.diffMadeSections], diff.sectionsMade, .made, false),
            ("sectionsRemoved", L10n[.Root.diffRemovedSections], diff.sectionsRemoved, .removed, false),
        ]
        for (id, title, paths, mark, linked) in lists where !paths.isEmpty {
            list.group(id, title, count: paths.count, paths.enumerated().map {
                ("\($0.offset)", .marked(mark, linked ? .link($0.element, path: $0.element, note: nil) : .plain($0.element)))
            })
        }
    }

    private func diffHead() -> some View {
        HStack {
            Text(L10n[.Root.diffHead]).font(.app(11)).foregroundStyle(.secondary)
            Spacer()
            if let open = model.openBranch {
                Button(L10n[.Root.diffMerge]) { model.askToMerge(from: open.id, into: nil) }
                    .controlSize(.small)
                    .disabled(model.isReadOnly)
                    .help(L10n[.Root.diffMergeHelp])
            }
        }
    }

    // MARK: - タグ

    private func tagPieces(_ detail: ProjectDetail, _ list: inout Pieces) {
        let used = projectTags(detail.schemas)
        let tagGroups = detail.project.tagGroups
        if used.isEmpty && tagGroups.isEmpty { list.add("none", .nothing(L10n[.Root.tagsNone])) }
        if !tagGroups.isEmpty {
            list.group("groups", L10n[.Root.tagsGroups], count: tagGroups.count, tagGroups.enumerated().map {
                ("\($0.offset)", .tagGroup($0.element))
            })
        }
        // Each tag's cards in one pass over the cards, rather than one pass per tag.
        var tagged: [String: [SchemaNode]] = [:]
        for card in detail.schemas {
            for tag in card.tags where tagged[tag]?.last?.id != card.id { tagged[tag, default: []].append(card) }
        }
        for item in used {
            list.group("t-" + item.tag, item.tag, count: item.count, (tagged[item.tag] ?? []).map { (id: $0.id, kind: .cardLink($0)) })
        }
    }
}

/// What the root's tabs work their lists out from, kept between drawings rather than worked out on
/// each (a keystroke in the history's filter, the window resized): where each change of the history
/// is — its card, and its field — each card's path as the filter reads it, and how big the project is.
/// Worked out again when the project is read again (the key: `RootView.memoKey`).
final class RootTabsMemo {
    struct Located {
        var card: SchemaNode
        var field: FieldNode?
    }

    private var key: String?
    private var cards: [String: SchemaNode] = [:]
    private var fields: [String: Located] = [:]
    /// An enum value's card, and an enum's own: changes to them are recorded against the enum.
    private var enums: [String: SchemaNode] = [:]
    private var searchPaths: [String: String] = [:]
    private var sized: (key: String, size: ProjectSize?)?

    /// The card (and the field) a change was made to, looked up in what was worked out once for `key`.
    func locate(_ event: ChangeEvent, _ detail: ProjectDetail, _ key: String) -> Located? {
        guard let id = event.targetId else { return nil }
        index(detail, key)
        switch event.targetType {
        case .schema: return cards[id].map { Located(card: $0) }
        case .field: return fields[id]
        case .enumValue: return enums[id].map { Located(card: $0) }
        default: return nil
        }
    }

    /// A card's full path in lower case, as the history's filter matches it.
    func searchPath(_ card: SchemaNode, _ directory: SchemaDirectory) -> String {
        if let kept = searchPaths[card.id] { return kept }
        let path = directory.full(card).lowercased()
        searchPaths[card.id] = path
        return path
    }

    /// How big the project is, asked of the core once for `key`.
    func size(_ key: String, _ make: () -> ProjectSize?) -> ProjectSize? {
        if let sized, sized.key == key { return sized.size }
        let size = make()
        sized = (key, size)
        return size
    }

    /// The first card, in the project's order, that has each id — as a search from the top would find it.
    private func index(_ detail: ProjectDetail, _ key: String) {
        guard key != self.key else { return }
        self.key = key
        cards = [:]
        fields = [:]
        enums = [:]
        searchPaths = [:]
        func walk(_ list: [FieldNode], _ card: SchemaNode) {
            for field in list {
                if fields[field.id] == nil { fields[field.id] = Located(card: card, field: field) }
                if !field.children.isEmpty { walk(field.children, card) }
            }
        }
        for card in detail.schemas {
            if cards[card.id] == nil { cards[card.id] = card }
            if enums[card.id] == nil { enums[card.id] = card }
            for value in card.values where enums[value.id] == nil { enums[value.id] = card }
            walk(card.fields, card)
        }
    }
}
