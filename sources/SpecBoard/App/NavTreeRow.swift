import AppKit
import SwiftUI
import UniformTypeIdentifiers

// The nav panel's rows as views SwiftUI can compare (`NavTreeRow`): each is given values alone — what the
// row says (`RowFace`), worked out once and kept (`NavTreeMemo`) — and reaches the panel's actions through
// one object that stays the same (`NavTreeHands`). A row whose values are as they were is not built again,
// so an update of the panel — a card picked, a section shown, the project read again — builds the rows on
// screen that changed, not every row open.

extension NavPanelTree {
    /// What the rows, their marks and what they say are made from in the project: compared as a whole, so
    /// that a reading that leaves them alike — another tree written, a card's fields or place changed —
    /// reuses what was worked out from them.
    struct Source: Equatable {
        struct Card: Equatable {
            var id: String
            var name: String
            var label: String
            var kind: SchemaKind
            var ownKind: String?
            var tags: [String]
            var sortOrder: Int
            var sectionId: String?
            var container: String?
            var inside: [String]
            var holdsNodes: Bool
            var isCodeFile: Bool
        }

        struct Ghost: Equatable {
            var id: String
            var name: String
            var sectionId: String?
            var target: String?
            var resolvesTo: String?
            var sortOrder: Int
        }

        struct Node: Equatable {
            var id: String
            var name: String
            var kind: String
            var label: String
            var sectionId: String?
            var container: String
            var sortOrder: Int
        }

        struct Around: Equatable {
            var section: String
            var cards: [String]
        }

        var project: String
        var context: String?
        var sections: [ProjectSection]
        var cards: [Card]
        var ghosts: [Ghost]
        var nodes: [Node]
        var tagGroups: [TagGroup]
        var sectionMarks: [String: SectionMark]
        /// What the lines of each section whose section ghosts are open reach (`pointedAt`), by section.
        var around: [String: [Around]]

        init(_ detail: ProjectDetail, openAround: Set<String>) {
            project = detail.project.id
            context = detail.context
            sections = detail.sections
            cards = detail.schemas.map {
                Card(id: $0.id, name: $0.name, label: $0.label, kind: $0.kind, ownKind: $0.ownKind, tags: $0.tags,
                     sortOrder: $0.sortOrder, sectionId: $0.sectionId, container: $0.container, inside: $0.inside,
                     holdsNodes: !$0.nodes.isEmpty, isCodeFile: $0.isCodeFile)
            }
            ghosts = detail.ghosts.map {
                Ghost(id: $0.id, name: $0.name, sectionId: $0.sectionId, target: $0.target, resolvesTo: $0.resolvesTo,
                      sortOrder: $0.sortOrder)
            }
            nodes = detail.inner.map {
                Node(id: $0.id, name: $0.name, kind: $0.kind, label: $0.label, sectionId: $0.sectionId,
                     container: $0.container, sortOrder: $0.sortOrder)
            }
            tagGroups = detail.project.tagGroups
            sectionMarks = detail.sectionMarks
            var around: [String: [Around]] = [:]
            for section in openAround {
                around[section] = NavPanelTree.pointedAt(detail, from: section).map { Around(section: $0.section, cards: $0.cards) }
            }
            self.around = around
        }

        /// The sections whose section ghosts are open in `expanded`: what their lines reach is listed.
        static func openAround(_ expanded: Set<String>) -> Set<String> {
            let head = NavPanelTree.Node.auto("").key
            return Set(expanded.compactMap { $0.hasPrefix(head) ? String($0.dropFirst(head.count)) : nil })
        }
    }

    /// How the root row of the tree open reads: which tree, and what is said of it.
    struct RootLook: Equatable {
        var title: String
        var help: String
        var branch: Bool
        var mainMoved: Int
        var locked: Bool
    }

    /// What a row says, worked out from the project once: everything it draws but what changes as a person
    /// picks, opens and drags (`NavTreeRow`'s other values).
    struct RowFace: Equatable {
        /// A section's mark and colour, and what it says.
        struct Badge: Equatable {
            var symbol: String
            var color: String?
            var help: String
        }

        enum Label: Equatable {
            case root(RootLook, count: Int)
            case section(name: String, badge: Badge?, missingHelp: String?, workdir: Bool, count: Int)
            case goneSection(name: String, badge: Badge?)
            case auto
            case autoSection(String)
            case autoCard(name: String, kind: String)
            case cards(count: Int, help: String)
            case ghost(line: String, help: String, kind: String, reaches: Bool)
            case card(name: String, label: String?, export: ExportMark?, kind: String)
            case node(name: String, label: String?, kind: String)
            case goneCard(name: String, kind: String)
            case goneGhost(line: String, kind: String)
            case goneNode(name: String, kind: String)
            case goneNothing
            case values(count: Int, help: String)
            case none
            case symbol(CodeSymbol)
            /// What the row stands for is not there: nothing drawn.
            case nothing
        }

        var label: Label
        var opens: Bool
        var mark: ChangeMark?
        /// What a pause over the row says (`rowHelp`).
        var help: String?
        /// A card's section, for whether it is hidden on the map on screen.
        var home: String?
    }

    /// What a pause over a row says: what it is called (its label) and what its mark means.
    static func rowHelp(_ node: Node, _ detail: ProjectDetail, mark: ChangeMark?, gone: BranchMarks.Gone,
                        outlines: [String: FileOutline], nodes: [String: InnerCard]? = nil) -> String? {
        let label: String?
        switch node {
        case .card(let id): label = detail.directory.schema(id: id)?.label
        case .node(let id): label = nodes.map { $0[id]?.label } ?? detail.inner.first { $0.id == id }?.label
        case .symbol(let file, let path):
            label = outlines[file]?.symbol(path).map { SymbolBadgeView.help($0, language: outlines[file]?.language) }
        case .gone(let id):
            label = gone.cards.first { $0.id == id }?.label ?? gone.nodes.first { $0.id == id }?.label
        default: label = nil
        }
        let text = NavPanelTreeView.said(label, mark?.meaning)
        return text.isEmpty ? nil : text
    }

    /// What the faces are worked out from beside the project: what the tree open is, the search box, what
    /// the branch removed and the marks, the export marks, the files' functions and types read.
    struct FaceInputs: Equatable {
        /// The screen's language: what a row says in words is said in it.
        var language = Language.current
        var root: RootLook
        var query: String
        var gone: BranchMarks.Gone
        var shownGone: BranchMarks.Gone
        var exportMarks: [String: ExportMark]
        var outlines: [String: FileOutline]
    }

    /// A row's face (`RowFace`). `nodes`: the flows' nodes by id.
    static func face(_ node: Node, _ detail: ProjectDetail, _ inputs: FaceInputs, marks: Marks,
                     nodes: [String: InnerCard], open: Bool = false) -> RowFace {
        let directory = detail.directory
        let filter = Filter(query: inputs.query)
        let mark = marks.of(node, open: open)
        let opens = node.opens(in: directory, gone: inputs.shownGone, outlines: inputs.outlines)
        let help = rowHelp(node, detail, mark: mark, gone: inputs.gone, outlines: inputs.outlines, nodes: nodes)
        func badge(_ section: ProjectSection) -> RowFace.Badge? {
            guard let key = section.kind?.rawValue ?? section.kindName else { return nil }
            let shown = detail.mark(of: section)
            return RowFace.Badge(symbol: shown.symbol, color: shown.color,
                                 help: section.kind?.help ?? L10n[.Nav.treeOwnKindHelp(key)])
        }
        var home: String?
        let label: RowFace.Label
        switch node {
        case .section(let id):
            if let section = directory.section(id: id) {
                if section.parentId == nil {
                    label = .root(inputs.root, count: directory.members(id).count)
                } else {
                    label = .section(name: section.name, badge: badge(section),
                                     missingHelp: section.missing ? L10n[.Nav.treeMissingFolderHelp(section.description)] : nil,
                                     workdir: section.kindName == WorkDirIds.workdirKind, count: directory.members(id).count)
                }
            } else {
                label = .nothing
            }
        case .goneSection(let id):
            if let section = inputs.gone.sections.first(where: { $0.id == id }) {
                label = .goneSection(name: section.name, badge: badge(section))
            } else {
                label = .nothing
            }
        case .auto:
            label = .auto
        case .autoSection(let at, let other):
            label = .autoSection(directory.shown(section: other, within: directory.root(of: at)))
        case .autoCard(_, let card):
            label = directory.schema(id: card).map { .autoCard(name: $0.name, kind: $0.kindName) } ?? .nothing
        case .cards(let id):
            label = .cards(count: directory.members(id).count,
                           help: NavPanelTreeView.said(L10n[.Nav.treeCardsHelp], mark?.meaning))
        case .ghost(let id):
            if let ghost = directory.ghost(id: id) {
                let card = directory.reached(ghost)
                label = .ghost(line: ghostLine(ghost, in: directory),
                               help: NavPanelTreeView.said(directory.ghostProblem(ghost)
                                   ?? "\(directory.full(ghost)) → \(card.map(directory.full) ?? "")", mark?.meaning),
                               kind: card?.kindName ?? "ghost", reaches: card != nil)
            } else {
                label = .nothing
            }
        case .card(let id):
            if let schema = directory.schema(id: id) {
                home = directory.sectionId(of: schema)
                label = .card(name: schema.name, label: filter.isOn && filter.byLabel(schema) ? schema.label : nil,
                              export: inputs.exportMarks[id], kind: schema.kindName)
            } else {
                label = .nothing
            }
        case .node(let id):
            if let held = nodes[id] {
                label = .node(name: held.name, label: filter.isOn && filter.byLabel(held) ? held.label : nil, kind: held.kind)
            } else {
                label = .nothing
            }
        case .gone(let id):
            let gone = inputs.gone
            if let card = gone.cards.first(where: { $0.id == id }) {
                label = .goneCard(name: card.name, kind: card.kindName)
            } else if let ghost = gone.ghosts.first(where: { $0.id == id }) {
                label = .goneGhost(line: ghostLine(ghost, in: directory), kind: directory.reached(ghost)?.kindName ?? "ghost")
            } else if let node = gone.nodes.first(where: { $0.id == id }) {
                label = .goneNode(name: node.name, kind: node.kind)
            } else {
                label = .goneNothing
            }
        case .values(let id):
            label = .values(count: directory.members(id).filter { $0.kind == .value || $0.kind == .enum }.count,
                            help: NavPanelTreeView.said(L10n[.Nav.treeValuesHelp], mark?.meaning))
        case .none:
            label = .none
        case .symbol(let file, let path):
            label = inputs.outlines[file]?.symbol(path).map { .symbol($0) } ?? .nothing
        }
        return RowFace(label: label, opens: opens, mark: mark, help: help, home: home)
    }
}

/// What the tree worked out from the project, kept while what it was worked out from is alike: the rows,
/// their guides, their marks, what each row says. A reference, as `RowHeights` is: what it keeps is read
/// in the panel's body, never drawn from.
final class NavTreeMemo {
    /// What the rows are worked out from.
    struct Shape: Equatable {
        var source: NavPanelTree.Source
        var expanded: Set<String>
        var query: String
        var gone: BranchMarks.Gone
        var outlines: [String: FileOutline]
    }

    private var stamp: Int?
    private var openAround: Set<String> = []
    private var source: NavPanelTree.Source?
    private var shape: Shape?
    private var marksFor: (source: NavPanelTree.Source, marks: BranchMarks)?
    private var facesFor: (source: NavPanelTree.Source, inputs: NavPanelTree.FaceInputs)?

    private(set) var rows: [NavPanelTree.Row] = []
    private(set) var guides: [String: [NavPanelTree.Guide]] = [:]
    /// How the rows hang, for the rows pinned over the tree (`NavSticky`).
    private(set) var outline = NavPanelTree.Outline()
    private(set) var marks = NavPanelTree.Marks()
    private var nodes: [String: InnerCard] = [:]
    private var faces: [String: NavPanelTree.RowFace] = [:]
    private var detail: ProjectDetail?
    private var inputs: NavPanelTree.FaceInputs?

    /// Brought up to `detail` and what else the rows show: what is alike is kept.
    func update(_ detail: ProjectDetail, expanded: Set<String>, filter: NavPanelTree.Filter, marks shown: BranchMarks,
                inputs: NavPanelTree.FaceInputs) {
        let around = NavPanelTree.Source.openAround(expanded)
        if source == nil || stamp != detail.stamp || openAround != around {
            let made = NavPanelTree.Source(detail, openAround: around)
            if made != source {
                source = made
                nodes = Dictionary(detail.inner.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
            }
            stamp = detail.stamp
            openAround = around
        }
        // What a row reads of the project is read from the newest reading, alike as it is.
        self.detail = detail
        guard let source else { return }
        let now = Shape(source: source, expanded: expanded, query: filter.query, gone: shown.gone, outlines: inputs.outlines)
        if now != shape {
            shape = now
            rows = NavPanelTree.rows(detail, expanded: expanded, filter: filter, gone: shown.gone, outlines: inputs.outlines)
            guides = NavPanelTree.guides(rows)
            outline = NavPanelTree.Outline(rows)
            NavPanelTreeView.rowsWorkedOut += 1
        }
        if marksFor.map({ $0.source != source || $0.marks != shown }) ?? true {
            marksFor = (source, shown)
            marks = NavPanelTree.Marks(detail, shown)
            faces = [:]
        }
        if facesFor.map({ $0.source != source || $0.inputs != inputs }) ?? true {
            facesFor = (source, inputs)
            self.inputs = inputs
            faces = [:]
        }
    }

    /// What the row says, worked out the first time it is asked for, open or folded: an open row
    /// leaves ● to the rows inside it.
    func face(_ row: NavPanelTree.Row) -> NavPanelTree.RowFace {
        let open = shape?.expanded.contains(row.node.key) ?? false
        let key = open ? row.id + "\u{1}open" : row.id
        if let kept = faces[key] { return kept }
        guard let detail, let inputs else {
            return NavPanelTree.RowFace(label: .nothing, opens: false, mark: nil, help: nil, home: nil)
        }
        let made = NavPanelTree.face(row.node, detail, inputs, marks: marks, nodes: nodes, open: open)
        faces[key] = made
        return made
    }
}

/// What a row does, reached through one object that stays the same while the panel is on screen: the
/// panel as it was last given (`tree`), whose state and model are the ones on screen, and how tall each
/// row is. A row's values say what it draws; what a press, a menu or a drag does is asked of the panel
/// when it is done, on what is open then.
@MainActor
final class NavTreeHands {
    var tree: NavPanelTreeView?
    /// The band of rows pinned over the tree.
    weak var sticky: NavSticky?
    /// How tall each row is, for which part of it a drag is over.
    var heights: [String: CGFloat] = [:]
}

/// One row of the nav panel's tree, made of values alone (`RowFace` and what changes as a person picks,
/// opens and drags), so that SwiftUI builds it again only when one of them changes.
struct NavTreeRow: View, Equatable {
    var row: NavPanelTree.Row
    var guides: [NavPanelTree.Guide]
    var face: NavPanelTree.RowFace
    /// Open in the tree.
    var open: Bool
    /// Lit as the row on screen, or as the row a press waits on (`NavRow.fill`).
    var lit: Bool
    /// Its name in bold: the section shown, the card picked.
    var bold: Bool
    /// A card of the section on screen that its map does not draw.
    var hidden: Bool
    /// Where a drag over it would be taken, if anywhere.
    var drop: NavPanelTree.Spot?
    /// Whether the column of the marks is there.
    var showsMarks: Bool
    var draggable: Bool
    var hands: NavTreeHands
    /// A copy of the row pinned over the tree (`NavSticky`): a press brings its row into view, and it is neither
    /// dragged nor dropped on — the row in the tree is.
    var pinned = false

    static func == (a: NavTreeRow, b: NavTreeRow) -> Bool {
        a.row == b.row && a.guides == b.guides && a.face == b.face && a.open == b.open && a.lit == b.lit
            && a.bold == b.bold && a.hidden == b.hidden && a.drop == b.drop && a.showsMarks == b.showsMarks
            && a.draggable == b.draggable && a.hands === b.hands && a.pinned == b.pinned
    }

    var body: some View {
        let _ = pinned ? NavSticky.built() : NavPanelTreeView.built(row.id)
        let key = row.id
        let pinned = pinned
        let node = row.node
        let hands = hands
        HStack(spacing: 0) {
            chevron
            label
        }
        // What the row is called and what its mark means, a pause over it away.
        .help(ifAny: face.help)
        .padding(.vertical, 2)
        .padding(.horizontal, 3)
        // From the parent's chevron down to this row: which rows belong to which.
        .background(alignment: .leading) {
            NavGuideLines(guides: guides, indent: NavPanelTreeView.indent, reach: face.opens ? 8 : 18)
        }
        .overlay { dropMarkView }
        .modifier(NavRowSlot(key: key, current: lit, pressed: nil, dropping: drop != nil, gap: NavPanelTreeView.rowGap))
        // Twice — a double-click, as the window counts it (the event's click count, as the
        // diagram reads it): a section or a root opens or shuts in the tree, a card opens in its
        // editor (once has already opened the section, or shown the card on its relation panel).
        .onTapGesture {
            if pinned { hands.sticky?.reveal(key) } else { hands.tree?.press(row) }
        }
        .contextMenu { NavRowMenu(hands: hands, node: node) }
        .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { hands.heights[key] = $0 }
        .modifier(NavDragSource(enabled: draggable && !pinned) {
            hands.tree?.startDragging(node)
            return NSItemProvider(object: NavPanelTreeView.token(node) as NSString)
        })
        .modifier(NavDropTarget(enabled: !pinned, delegate: NavPanelDrop(
            target: node,
            height: { hands.heights[key] ?? 20 },
            dragged: { hands.tree?.draggingNow },
            decide: { dragged, spot in hands.tree?.decide(dragged, on: node, at: spot) },
            mark: { spot in hands.tree?.markDrop(key, spot) },
            perform: { dragged, drop, providers in hands.tree?.dropped(dragged, drop, providers) })))
    }

    /// The indent, the chevron and the gap after it. Where there is a chevron all of
    /// it is the chevron's to click — the arrow alone is too small a thing to hit —
    /// drawn and spaced just as before, so the guide lines still meet it.
    @ViewBuilder
    private var chevron: some View {
        let lead = CGFloat(row.depth) * NavPanelTreeView.indent
        if face.opens {
            let node = row.node
            let hands = hands
            let pinned = pinned
            Button {
                if pinned { hands.sticky?.fold(node) } else { hands.tree?.toggle(node) }
            } label: {
                HStack(spacing: 0) {
                    Color.clear.frame(width: lead + 4, height: 1)
                    Image(systemName: open ? "chevron.down" : "chevron.right")
                        .font(.app(8.5, weight: .semibold))
                        .foregroundStyle(.secondary)
                        .frame(width: 12, height: 14)
                    Color.clear.frame(width: 4, height: 1)
                }
                .frame(maxHeight: .infinity)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .help(open ? L10n[.Nav.rowCollapse] : L10n[.Nav.rowExpand])
        } else {
            Color.clear.frame(width: lead + 4 + 12 + 4, height: 1)
        }
    }

    @ViewBuilder
    private var label: some View {
        let mark = face.mark
        switch face.label {
        case .root(let look, let n):
            // The tree open, plainly, and what kind of tree it is: a
            // branch's name with a tag saying so, main as itself — with a lock while it is
            // locked — the board as itself. The root's own name is not said here: named
            // Main by default, it read as main with a branch open.
            Text(look.title)
                .font(.app(12, weight: .semibold)).lineLimit(1)
                .help(look.help)
            if look.branch {
                tag("branch")
                // What main did since the branch was made: what taking main in brings.
                if look.mainMoved > 0 {
                    Text("↓ \(look.mainMoved)")
                        .font(.app(10, weight: .semibold, design: .monospaced))
                        .foregroundStyle(ChangeMark.Tint.blue.color)
                        .padding(.leading, 5)
                        .help(L10n[.Nav.treeMainMovedHelp(look.mainMoved)])
                }
            } else if look.locked {
                Image(systemName: "lock.fill").font(.app(9.5)).foregroundStyle(.secondary)
                    .padding(.leading, 4)
                    .help(L10n[.Branches.listMainLockedHelp])
            }
            Spacer(minLength: 4)
            count(n)
            markColumn(mark)
        case .section(let name, let badge, let missingHelp, let workdir, let n):
            sectionBadge(badge)
            if let missingHelp {
                // A folder a note is in that reading the folder no longer finds: a stand-in.
                Text(name)
                    .font(.app(12, weight: bold ? .semibold : .regular))
                    .foregroundStyle(.orange).strikethrough(true, color: .orange)
                    .lineLimit(1)
                    .help(missingHelp)
            } else {
                Text(name)
                    .font(.app(12, weight: bold ? .semibold : .regular))
                    .foregroundStyle(nameStyle(mark, else: HierarchicalShapeStyle.primary))
                    .lineLimit(1)
                // A working directory: read from a folder of this Mac, not written here.
                if workdir {
                    Image(systemName: "lock.fill").font(.app(8)).foregroundStyle(.tertiary)
                        .help(L10n[.Nav.treeWorkDirHelp])
                }
            }
            Spacer(minLength: 4)
            count(n)
            markColumn(mark)
        case .goneSection(let name, let badge):
            sectionBadge(badge)
            goneName(name, font: .app(12, weight: bold ? .semibold : .regular))
            Spacer(minLength: 4)
            markColumn(mark)
        case .auto:
            Text(NavPanelTree.autoTitle).font(.app(11)).foregroundStyle(.secondary)
                .help(NavPanelTree.autoHelp)
            Spacer(minLength: 4)
            markColumn(nil)
        case .autoSection(let shown):
            Image(systemName: "rectangle.dashed").font(.app(9.5)).foregroundStyle(.tertiary)
            Text(shown)
                .font(.app(11.5, design: .monospaced)).foregroundStyle(.secondary).lineLimit(1)
            Spacer(minLength: 4)
            markColumn(nil)
        case .autoCard(let name, let kind):
            Text(name).font(.app(11.5, design: .monospaced)).foregroundStyle(.secondary).lineLimit(1)
            Spacer(minLength: 4)
            Text(kind).font(.app(9.5)).foregroundStyle(.tertiary)
            markColumn(nil)
        case .cards(let n, let help):
            Text("cards").font(.app(11)).foregroundStyle(.secondary)
                .help(help)
            Spacer(minLength: 4)
            count(n)
            markColumn(mark)
        case .ghost(let line, let help, let kind, let reaches):
            Image(systemName: NavPanelTree.ghostSymbol).font(.app(9.5))
                .foregroundStyle(reaches ? Color.secondary : Color.red)
                .help(L10n[.Nav.treeGhostHelp])
            Text(line)
                .font(.app(12, weight: bold ? .semibold : .regular, design: .monospaced))
                .foregroundStyle(nameStyle(mark, else: reaches ? HierarchicalShapeStyle.primary : .secondary))
                .lineLimit(1).truncationMode(.middle)
                .help(help)
            Spacer(minLength: 4)
            Text(kind).font(.app(9.5)).foregroundStyle(.tertiary)
            markColumn(mark)
        case .card(let name, let shownLabel, let export, let kind):
            Text(name)
                .font(.app(12, weight: bold ? .semibold : .regular, design: .monospaced))
                .foregroundStyle(nameStyle(mark, else: HierarchicalShapeStyle.primary))
                .lineLimit(1)
            // Its label is in the tooltip; on the row only while the search box finds it by its label.
            if let shownLabel {
                Text(shownLabel).font(.app(10.5)).foregroundStyle(.secondary).lineLimit(1)
            }
            // A file an export writes: which, and how it stands.
            if let export {
                ExportMarkView(mark: export)
            }
            Spacer(minLength: 4)
            if hidden {
                Image(systemName: "eye.slash").font(.app(9)).foregroundStyle(.tertiary)
                    .help(L10n[.Nav.treeHiddenOnPanel])
            }
            Text(kind)
                .font(.app(9.5)).foregroundStyle(.tertiary)
            markColumn(mark)
        case .node(let name, let shownLabel, let kind):
            Text(name).font(.app(12, design: .monospaced))
                .foregroundStyle(nameStyle(mark, else: HierarchicalShapeStyle.primary))
                .lineLimit(1)
            if let shownLabel {
                Text(shownLabel).font(.app(10.5)).foregroundStyle(.secondary).lineLimit(1)
            }
            Spacer(minLength: 4)
            Text(kind).font(.app(9.5)).foregroundStyle(.tertiary)
            markColumn(mark)
        case .goneCard(let name, let kind), .goneNode(let name, let kind):
            goneName(name, font: .app(12, weight: bold ? .semibold : .regular, design: .monospaced))
            Spacer(minLength: 4)
            Text(kind).font(.app(9.5)).foregroundStyle(.tertiary)
            markColumn(mark)
        case .goneGhost(let line, let kind):
            Image(systemName: NavPanelTree.ghostSymbol).font(.app(9.5)).foregroundStyle(.secondary)
            goneName(line, font: .app(12, weight: bold ? .semibold : .regular, design: .monospaced))
            Spacer(minLength: 4)
            Text(kind).font(.app(9.5)).foregroundStyle(.tertiary)
            markColumn(mark)
        case .goneNothing:
            markColumn(mark)
        case .values(let n, let help):
            Text("values / enums").font(.app(11)).foregroundStyle(.secondary)
                .help(help)
            Spacer(minLength: 4)
            count(n)
            markColumn(mark)
        case .none:
            Text(L10n[.Common.wordNone]).font(.app(10.5)).foregroundStyle(.tertiary)
            Spacer(minLength: 4)
            markColumn(nil)
        case .symbol(let symbol):
            SymbolBadgeView(symbol: symbol)
            Text(symbol.name)
                .font(.app(12, weight: bold ? .semibold : .regular, design: .monospaced))
                .lineLimit(1)
                .padding(.leading, 5)
            Spacer(minLength: 4)
            Text("\(symbol.line)").font(.app(9.5, design: .monospaced)).foregroundStyle(.tertiary)
            markColumn(nil)
        case .nothing:
            EmptyView()
        }
    }

    /// A section's mark and colour (`ProjectDetail.mark(of:)`): its own over its kind's — an actor's by its
    /// figure — else a folder in grey.
    @ViewBuilder
    private func sectionBadge(_ badge: NavPanelTree.RowFace.Badge?) -> some View {
        if let badge {
            Image(systemName: badge.symbol).font(.app(10))
                .foregroundStyle(badge.color.flatMap { HexColor.color(hex: $0) } ?? .gray)
                .frame(width: 14)
                .help(badge.help)
        }
    }

    /// Where a drag over the row would be taken: a line above or below it, or the row
    /// lit, into it.
    @ViewBuilder
    private var dropMarkView: some View {
        if let drop {
            switch drop {
            case .into:
                RoundedRectangle(cornerRadius: 5)
                    .fill(Color.accentColor.opacity(0.12))
                    .overlay(RoundedRectangle(cornerRadius: 5).stroke(Color.accentColor, lineWidth: 1.5))
                    .allowsHitTesting(false)
            case .before, .after:
                VStack(spacing: 0) {
                    if drop == .after { Spacer(minLength: 0) }
                    Rectangle().fill(Color.accentColor).frame(height: 2)
                        .padding(.leading, CGFloat(row.depth) * NavPanelTreeView.indent + 16)
                    if drop == .before { Spacer(minLength: 0) }
                }
                .allowsHitTesting(false)
            }
        }
    }

    /// The name of what the open branch removed: in the removed mark's colour, struck through.
    private func goneName(_ name: String, font: Font) -> some View {
        let red = ChangeMark.Tint.red.color
        return Text(name).font(font).foregroundStyle(red).strikethrough(true, color: red).lineLimit(1)
    }

    /// How a name is drawn under its row's mark: in the mark's colour when the mark is its own, as it
    /// is otherwise else.
    private func nameStyle(_ mark: ChangeMark?, else plain: some ShapeStyle) -> AnyShapeStyle {
        guard let mark, mark.isOwn else { return AnyShapeStyle(plain) }
        return AnyShapeStyle(mark.tint.color)
    }

    /// The right-most column, as wide on every row while what a branch did is marked (`showsMarks`):
    /// the row's mark, or nothing.
    @ViewBuilder
    private func markColumn(_ mark: ChangeMark?) -> some View {
        if showsMarks {
            HStack(spacing: 0) {
                if let mark { ChangeMarkText(mark: mark) }
            }
            .frame(width: NavPanelTreeView.markWidth, alignment: .center)
        }
    }

    private func tag(_ text: String) -> some View {
        Text(text)
            .font(.app(9))
            .foregroundStyle(.secondary)
            .padding(.horizontal, 4).padding(.vertical, 1)
            .overlay(RoundedRectangle(cornerRadius: 3).stroke(.tertiary))
    }

    private func count(_ n: Int) -> some View {
        Text("\(n)").font(.app(10, design: .monospaced)).foregroundStyle(.tertiary)
    }
}

/// A row's right-click menu, made from what is open when it is made — not from the row's values, which
/// say only what the row draws.
struct NavRowMenu: View {
    var hands: NavTreeHands
    var node: NavPanelTree.Node

    var body: some View {
        if let tree = hands.tree, let detail = tree.model.detail {
            tree.menu(node, detail)
        }
    }
}

/// A row that may be picked up and dragged (`NavPanelTree.canDrag`); one that may not
/// stays put, and a press on it is a press.
struct NavDragSource: ViewModifier {
    var enabled: Bool
    var item: () -> NSItemProvider

    @ViewBuilder
    func body(content: Content) -> some View {
        if enabled { content.onDrag(item) } else { content }
    }
}

/// A row that may be dropped on; a pinned copy of a row may not.
struct NavDropTarget: ViewModifier {
    var enabled: Bool
    var delegate: NavPanelDrop

    @ViewBuilder
    func body(content: Content) -> some View {
        if enabled { content.onDrop(of: [.text], delegate: delegate) } else { content }
    }
}

/// A row as a place to drop on: what the drag would do there is asked of
/// `NavPanelTree.drop` as it moves over the row, and only where it would do something is
/// the row marked as taking it — elsewhere the drag says it would not be taken.
struct NavPanelDrop: DropDelegate {
    var target: NavPanelTree.Node
    var height: () -> CGFloat
    var dragged: () -> NavPanelTree.Node?
    var decide: (NavPanelTree.Node, NavPanelTree.Spot) -> NavPanelTree.Drop?
    var mark: (NavPanelTree.Spot?) -> Void
    var perform: (NavPanelTree.Node, NavPanelTree.Drop, [NSItemProvider]) -> Void

    private func spot(_ info: DropInfo, _ dragging: NavPanelTree.Node) -> NavPanelTree.Spot {
        NavPanelTree.spot(info.location.y, height: height(), dragging: dragging, over: target)
    }

    func validateDrop(info: DropInfo) -> Bool {
        dragged() != nil && info.hasItemsConforming(to: [.text])
    }

    func dropUpdated(info: DropInfo) -> DropProposal? {
        guard let dragging = dragged() else { return DropProposal(operation: .forbidden) }
        let at = spot(info, dragging)
        guard let drop = decide(dragging, at) else {
            mark(nil)
            return DropProposal(operation: .forbidden)
        }
        mark(at)
        // A card ghost made is a copy of nothing but a pointer: the copy cursor, as an alias.
        if case .ghostOf = drop { return DropProposal(operation: .copy) }
        return DropProposal(operation: .move)
    }

    func dropExited(info: DropInfo) { mark(nil) }

    func performDrop(info: DropInfo) -> Bool {
        mark(nil)
        guard let dragging = dragged(), let drop = decide(dragging, spot(info, dragging)) else { return false }
        perform(dragging, drop, info.itemProviders(for: [.text]))
        return true
    }
}

/// The lines of one row of the tree (`NavPanelTree.guides`): a level's line runs
/// under the chevron of the row it hangs from, and a branch reaches across to the
/// row. Drawn a point past the row at top and bottom, over the gap between rows.
struct NavGuideLines: View {
    let guides: [NavPanelTree.Guide]
    let indent: CGFloat
    /// How far past its own level's line the branch to the row reaches: to the
    /// chevron, or on to the name where there is none.
    let reach: CGFloat

    var body: some View {
        GeometryReader { geo in
            Path { path in
                let top: CGFloat = -1, bottom = geo.size.height + 1, middle = (geo.size.height / 2).rounded() + 0.5
                for (level, guide) in guides.enumerated() {
                    // Under the chevron of a row at this depth: past the padding (3),
                    // the indent, the spacing (4) and half the chevron (6).
                    let x = CGFloat(level) * indent + 13.5
                    switch guide {
                    case .none:
                        continue
                    case .line:
                        path.move(to: CGPoint(x: x, y: top))
                        path.addLine(to: CGPoint(x: x, y: bottom))
                    case .tee, .end:
                        path.move(to: CGPoint(x: x, y: top))
                        path.addLine(to: CGPoint(x: x, y: guide == .tee ? bottom : middle))
                        path.move(to: CGPoint(x: x, y: middle))
                        path.addLine(to: CGPoint(x: x + reach, y: middle))
                    }
                }
            }
            .stroke(Color.secondary.opacity(0.35), lineWidth: 1)
        }
        .allowsHitTesting(false)
    }
}
