import AppKit

/// What a card's row on a diagram does — on the relation panel and on a card map alike,
/// the same for every kind of card (overview §13):
///
/// - pressed once, it is picked: marked on the diagram, and in the property panel — the
///   row, or, for a row with nothing of its own to show there (a note's line, a custom
///   card's field, a card map node's way out), what it belongs to;
/// - twice, or Return on it: where the row points — the card a `ref` or `pointsAt` names,
///   what a store holds, what a row of a process node reads, writes, signs with or hands
///   to, what a document's row mentions, the card a section ghost's row stands for, the
///   node a card map's way leads to. A row pointing nowhere — an enum's value, a goal's
///   row — does what its card's header does;
/// - right-clicked: what it points at, and its path to copy.
///
/// What switches with a press — a goal's choice, a row's check, a fold mark — switches
/// once for a double-click too (`CardCanvasView.mouseDown`).
///
/// The canvas says what was pressed (`CardCanvasView.RowHit`); what that means is worked
/// out here, apart from the screen, so it can be tested; the window does it
/// (`DiagramPresses`).
enum DiagramRows {

    /// What a press picks, for the property panel.
    enum Pick: Equatable {
        case card(String)
        /// A card and a field of it — or of a value object it holds in place.
        case field(String, String)
        case row(String, AppModel.CardRow)
        /// A card ghost, by its header: the ghost itself (`specboard://SpecBoard/Docs/Design:DesignSections` §2).
        case ghost(String)
        /// Nothing: a section ghost's header stands for no card.
        case nothing

        /// The card picked.
        var card: String? {
            switch self {
            case .card(let id), .field(let id, _), .row(let id, _): id
            case .ghost, .nothing: nil
            }
        }
    }

    /// A place of the project a row points at: a card, and the field or the row in it.
    struct Place: Equatable {
        var card: String
        var field: String? = nil
        var row: AppModel.CardRow? = nil
        /// The row to mark on its card where it is drawn (`CardCanvasView.rowKey`).
        var mark: String? = nil

        var pick: Pick {
            if let field { return .field(card, field) }
            if let row { return .row(card, row) }
            return .card(card)
        }
    }

    // MARK: - The relation panel

    /// What a press on `hit` picks: a field, an enum's value — on its enum, though it is
    /// held in place in another card — a store's hold, a flow's node or a process node's
    /// row, a formatdoc's item, a custom card's row; the card for its header and for a row
    /// with nothing of its own; on a section ghost, the card or field a row stands for. On
    /// a card ghost's box, its header picks the ghost, and a row the card's own row — the
    /// box draws the card it leads to (`specboard://SpecBoard/Docs/Design:DesignSections` §2).
    static func pick(_ hit: CardCanvasView.RowHit, in detail: ProjectDetail) -> Pick {
        if SectionGhosts.isGhost(hit.card) {
            guard let field = hit.row?.field, let target = SectionGhosts.target(of: field) else { return .nothing }
            // A function or type of a file: its file's row.
            if WorkDirIds.isFile(target.schema), let symbol = target.field { return .row(target.schema, .symbol(symbol)) }
            return target.field.map { .field(target.schema, $0) } ?? .card(target.schema)
        }
        if let ghost = detail.directory.ghost(id: hit.card) {
            guard hit.row != nil, let card = detail.directory.reached(ghost) else { return .ghost(ghost.id) }
            return pick(CardCanvasView.RowHit(card: card.id, row: hit.row), in: detail)
        }
        guard let card = detail.directory.schema(id: hit.card) else { return .nothing }
        guard let row = hit.row else { return .card(card.id) }
        if let symbol = row.symbol { return .row(card.id, .symbol(symbol.path)) }
        if let field = row.field { return .field(card.id, field.id) }
        if let value = row.value {
            let holder = card.values.contains { $0.id == value.id }
                ? card : detail.schemas.first { $0.values.contains { $0.id == value.id } }
            return holder.map { .row($0.id, .value(value.id)) } ?? .card(card.id)
        }
        if let index = holdIndex(row, on: card) { return .row(card.id, .hold(index)) }
        if let node = nodeRow(row, on: card) { return .row(card.id, node) }
        if card.kind == .formatdoc, row.path != SchemaNode.topRow,
           FormatRows.row(at: PathText.names(row.path), in: card.rows) != nil {
            return .row(card.id, .formatRow(row.path))
        }
        if card.kind.isCustom, CustomRows.row(at: row.path, in: card.customRows) != nil {
            return .row(card.id, .customRow(row.path))
        }
        if let own = ownRow(row, on: card) { return .row(card.id, own) }
        return .card(card.id)
    }

    /// A row of a card of a kind the project defines, picked: one of its rows (`steps.a`); none for
    /// one of its fields or another kind's.
    static func ownRow(_ row: CardRow, on card: SchemaNode) -> AppModel.CardRow? {
        guard card.kind == .custom, let def = card.kindDef,
              OwnRows.walk(def, card.ownRows).contains(where: { $0.path == row.path }) else { return nil }
        return .customRow(row.path)
    }

    /// Every place the row points at, in the order its lines are drawn — the first is
    /// where a double-click goes. None for a header, and for a row pointing nowhere.
    static func targets(_ hit: CardCanvasView.RowHit, in detail: ProjectDetail) -> [Place] {
        guard let row = hit.row else { return [] }
        if SectionGhosts.isGhost(hit.card) {
            guard let field = row.field, let target = SectionGhosts.target(of: field) else { return [] }
            if WorkDirIds.isFile(target.schema), let symbol = target.field {
                return [Place(card: target.schema, row: .symbol(symbol), mark: symbol)]
            }
            return [Place(card: target.schema, field: target.field, mark: target.field)]
        }
        // A card ghost's row is its card's.
        if let ghost = detail.directory.ghost(id: hit.card) {
            guard let card = detail.directory.reached(ghost) else { return [] }
            return targets(CardCanvasView.RowHit(card: card.id, row: row), in: detail)
        }
        guard let card = detail.directory.schema(id: hit.card) else { return [] }
        if let field = row.field {
            // Named from the card that has the field: a value object's, held in place.
            let owner = locateField(detail.schemas, id: field.id)?.schema ?? card
            return referencedSchemas(detail, owner: owner, field).map { Place(card: $0.id) }
        }
        // The project's lines leaving the row as the card draws them (`GraphEdges.leaving`): each
        // where it lands.
        let base = detail.directory.sectionId(of: card)
        var out: [Place] = []
        for line in GraphEdges.leaving(card, row: row.path, in: detail) {
            guard let path = drawnPath(line, in: detail),
                  let place = landing(path, base: base, in: detail), !out.contains(place) else { continue }
            out.append(place)
        }
        return out
    }

    /// Where a line put on the cards (`GraphEdges.placed`) lands, as a path: the card it
    /// reaches in full, and the row it lands on there (a node's `#n.<node>` as the node's
    /// name; a row of the card's own, `#top`, the card).
    static func drawnPath(_ line: Relation, in detail: ProjectDetail) -> String? {
        let directory = detail.directory
        guard let target = directory.schema(id: line.toSchemaId) else { return nil }
        var at = line.at ?? ""
        if let node = SchemaNode.rowNode(at) { at = PathText.quote(node.name) } else if at.hasPrefix("#") { at = "" }
        return directory.full(target, row: at)
    }

    /// The paths that name what `hit` picks, as the project writes them in full: a field's
    /// every one, an item's, a node's, a custom row's; for a row without a path of its own
    /// (an enum's value, a hold, a process node's row, a note's line), its card's or node's.
    static func paths(_ hit: CardCanvasView.RowHit, in detail: ProjectDetail) -> [String] {
        let directory = detail.directory
        switch pick(hit, in: detail) {
        case .card(let id):
            return schemaPaths(detail, id: id)
        case .field(_, let field):
            return fieldPaths(detail, fieldId: field)
        case .row(let id, let row):
            guard let card = directory.schema(id: id) else { return [] }
            switch row {
            case .formatRow(let path), .customRow(let path): return [directory.full(card, row: path)]
            case .symbol(let path): return [SymbolPath.full(directory.full(card), path)]
            case .node(let name), .nodeStep(let name, _): return [directory.full(card, [name])]
            case .value, .hold: return [directory.full(card)]
            }
        case .ghost(let id):
            return directory.ghost(id: id).map { [directory.full($0)] } ?? []
        case .nothing:
            return []
        }
    }

    /// A store's hold row (`#h<n>`): which it is.
    static func holdIndex(_ row: CardRow, on card: SchemaNode) -> Int? {
        guard card.kind == .store, row.path.hasPrefix("#h"), let index = Int(row.path.dropFirst(2)),
              card.connections.filter({ $0.line == Line.holds && $0.from.isEmpty }).indices.contains(index)
        else { return nil }
        return index
    }

    /// A flow's node row on its card (`#n.<node>`), or a process node's row under it
    /// (`#n.<node>.<n>`), as its card editor picks them.
    static func nodeRow(_ row: CardRow, on card: SchemaNode) -> AppModel.CardRow? {
        guard card.kind == .flow, let (name, step) = SchemaNode.rowNode(row.path),
              card.nodes.contains(where: { $0.name == name }) else { return nil }
        guard let step else { return .node(name) }
        return .nodeStep(name, StepEditing.path([.step(step)]))
    }

    /// Where `path` — as the project writes it, read from section `base` — lands; for a line
    /// to another flow's end (`outcome`), that end.
    static func landing(_ path: String, outcome: String? = nil, base: String, in detail: ProjectDetail) -> Place? {
        guard let found = try? MCPViews.locate(detail, path, base: base) else { return nil }
        var out: Place
        switch found {
        case .schema(let card):
            out = Place(card: card.id)
        case .field(let holder, let field, _):
            out = Place(card: holder.id, field: field.id, mark: field.id)
        case .formatRow(let note, let names):
            let item = FormatRows.item(of: names, in: note.rows)
            out = Place(card: note.id, row: .formatRow(PathText.joinNames(names)),
                        mark: item.isEmpty ? SchemaNode.topRow : PathText.joinNames(item))
        case .node(let flow, let node):
            out = Place(card: flow.id, row: .node(node.name), mark: SchemaNode.nodeRow(node.name))
        case .customRow(let card, let names):
            let joined = CardWalk.rowPath(names, in: card)
            out = Place(card: card.id, row: .customRow(joined), mark: joined)
        }
        if let outcome, out.row == nil, out.field == nil, let flow = detail.directory.schema(id: out.card),
           let end = flow.nodes.first(where: { $0.kind == "end" && $0.outcome == outcome }) {
            out.row = .node(end.name)
            out.mark = SchemaNode.nodeRow(end.name)
        }
        return out
    }

    // MARK: - Lines drawn by hand (つなぐ, overview §13)

    /// Where a line drawn from `hit` leaves: the card that holds it and its place there — a
    /// field's names (a value object's field, held in place, as its own card has it), a
    /// formatdoc's item, a flow's node (its own card) or a process node's row (`steps[0]`), a
    /// goal's row (the card it is); the card's head for its header and for a row with no
    /// place of its own (an enum's value, a store's hold, a note's line). A card ghost's rows
    /// and header are the card's it leads to (a ghost holds no lines). None for a section
    /// ghost's header, and a ghost leading nowhere.
    static func lineSource(_ hit: CardCanvasView.RowHit, in detail: ProjectDetail) -> LineSource? {
        let directory = detail.directory
        func head(_ card: SchemaNode) -> LineSource { LineSource(card: card.id, path: directory.full(card)) }
        func field(_ id: String) -> LineSource? {
            guard let found = locateField(detail.schemas, id: id) else { return nil }
            return LineSource(card: found.schema.id, path: directory.full(found.schema), from: PathText.joinNames(found.path))
        }
        if SectionGhosts.isGhost(hit.card) {
            guard let row = hit.row?.field, let target = SectionGhosts.target(of: row),
                  let card = directory.schema(id: target.schema) else { return nil }
            return target.field.flatMap(field) ?? head(card)
        }
        if let ghost = directory.ghost(id: hit.card) {
            guard let card = directory.reached(ghost) else { return nil }
            return lineSource(CardCanvasView.RowHit(card: card.id, row: hit.row), in: detail)
        }
        guard let card = directory.schema(id: hit.card) else { return nil }
        guard let row = hit.row else { return head(card) }
        if let id = row.field?.id { return field(id) ?? head(card) }
        switch pick(hit, in: detail) {
        case .row(_, .node(let name)):
            return nodeSource(card, name, step: nil, in: detail)
        case .row(_, .nodeStep(let name, let path)):
            guard case .step(let index)? = StepEditing.parts(path)?.first else { return nodeSource(card, name, step: nil, in: detail) }
            return nodeSource(card, name, step: index, in: detail)
        case .row(_, .formatRow(let path)):
            return LineSource(card: card.id, path: directory.full(card), from: path)
        case .row(_, .customRow(let path)):
            return inner(card, path, in: detail).map(head) ?? head(card)
        default:
            return head(card)
        }
    }

    /// A flow's node as a line leaves it: its own card, and the process node's row `step`.
    private static func nodeSource(_ flow: SchemaNode, _ name: String, step: Int?, in detail: ProjectDetail) -> LineSource? {
        guard let node = detail.inner.first(where: { $0.container == flow.id && $0.name == name }) else { return nil }
        return LineSource(card: node.id, path: detail.directory.full(flow, [name]), from: step.map { "steps[\($0)]" } ?? "")
    }

    /// The card a goal's row is: the card inside it by the row's names.
    private static func inner(_ goal: SchemaNode, _ path: String, in detail: ProjectDetail) -> SchemaNode? {
        var holder = goal
        for name in PathText.names(path) {
            guard let next = detail.schemas.first(where: { $0.container == holder.id && $0.name == name }) else { return nil }
            holder = next
        }
        return holder
    }

    /// What a line drawn to `hit` reaches, as a path: the card (a card ghost's header the
    /// ghost; a row through it, the ghost's path on into the card), a field, a formatdoc's
    /// item, a flow's node (a process node's row is its node), a goal's row (the card it
    /// is). None for a section ghost's header.
    static func lineTarget(_ hit: CardCanvasView.RowHit, in detail: ProjectDetail) -> String? {
        let directory = detail.directory
        func field(_ id: String) -> String? {
            locateField(detail.schemas, id: id).map { directory.full($0.schema, $0.path) }
        }
        if SectionGhosts.isGhost(hit.card) {
            guard let row = hit.row?.field, let target = SectionGhosts.target(of: row),
                  let card = directory.schema(id: target.schema) else { return nil }
            if WorkDirIds.isFile(card.id), let symbol = target.field { return SymbolPath.full(directory.full(card), symbol) }
            return target.field.flatMap(field) ?? directory.full(card)
        }
        if let ghost = directory.ghost(id: hit.card) {
            let written = directory.full(ghost)
            guard hit.row != nil, let card = directory.reached(ghost),
                  let inside = lineTarget(CardCanvasView.RowHit(card: card.id, row: hit.row), in: detail) else { return written }
            let own = directory.full(card)
            return inside.hasPrefix(own) ? written + inside.dropFirst(own.count) : inside
        }
        guard let card = directory.schema(id: hit.card) else { return nil }
        guard let row = hit.row else { return directory.full(card) }
        if let symbol = row.symbol { return SymbolPath.full(directory.full(card), symbol.path) }
        if let id = row.field?.id, let path = field(id) { return path }
        switch pick(hit, in: detail) {
        case .row(_, .node(let name)), .row(_, .nodeStep(let name, _)):
            return directory.full(card, [name])
        case .row(_, .formatRow(let path)), .row(_, .customRow(let path)):
            return directory.full(card, row: path)
        default:
            return directory.full(card)
        }
    }

    /// Where a line drawn from a node's card on a flow's card map leaves: the node's own card,
    /// a process node's row (`steps[0]`) for its row.
    static func flowMapLineSource(_ hit: CardCanvasView.RowHit, flow: SchemaNode, in detail: ProjectDetail) -> LineSource? {
        let step = hit.row.flatMap { FlowEditor.rowIndex($0.path) }
        return nodeSource(flow, FlowCardMap.node(of: hit.card), step: step, in: detail)
    }

    /// What a line drawn to a node's card on a flow's card map reaches: the node.
    static func flowMapLineTarget(_ hit: CardCanvasView.RowHit, flow: SchemaNode, in detail: ProjectDetail) -> String? {
        let name = FlowCardMap.node(of: hit.card)
        guard detail.inner.contains(where: { $0.container == flow.id && $0.name == name }) else { return nil }
        return detail.directory.full(flow, [name])
    }

    // MARK: - A flow's card map

    /// Where a row of a card map points: another card of the map, a place of the project, or
    /// nowhere — what the card's header does.
    enum MapTarget: Equatable {
        case card(String)
        case place(Place)
        case header
    }

    /// What a press on a node's card picks: a process node's row, else the node — what a
    /// way out, a trigger, a target is written on.
    static func flowMapPick(_ row: CardRow?, node: String) -> AppModel.CardRow {
        if let row, let index = FlowEditor.rowIndex(row.path) { return .nodeStep(node, StepEditing.path([.step(index)])) }
        return .node(node)
    }

    /// Where a row of a node's card points: a way out, the node it leads to; a process
    /// node's row, what it reads, writes, signs with or hands to; what it is (`#k`) — what
    /// a run runs, the flow whose end it waits for — what a begin receives with and an end
    /// answers with, the key. As its row on the relation panel draws its line.
    static func flowMapTarget(_ row: CardRow?, node name: String, flow: SchemaNode, in detail: ProjectDetail) -> MapTarget {
        guard let row else { return .header }
        // A way out: the node it goes to.
        if let node = flow.nodes.first(where: { $0.name == name }), let next = way(row.path, of: node),
           flow.nodes.contains(where: { $0.name == next }) {
            return .card(FlowCardMap.id(next))
        }
        // A process node's row, or what the node is: the node card's lines from there that
        // leave the flow (`relations`), each where it lands.
        let from: String?
        if let index = FlowEditor.rowIndex(row.path) {
            from = "steps[\(index)]"
        } else if ["#k", "#recv", "#ret"].contains(row.path) {
            from = ""
        } else {
            from = nil
        }
        guard let from,
              let card = detail.inner.first(where: { $0.container == flow.id && $0.name == name }) else { return .header }
        let siblings = Set(detail.inner.filter { $0.container == flow.id }.map(\.id))
        let base = detail.directory.sectionId(of: flow)
        for line in detail.relations where line.fromSchemaId == card.id && line.from == from
            && !siblings.contains(line.toSchemaId) {
            guard let target = detail.directory.schema(id: line.toSchemaId) else { continue }
            let at = line.at.flatMap { PathText.hasSection($0) ? nil : $0 } ?? ""
            let path = detail.directory.full(target, row: at)
            let outcome = card.connections.first { $0.id == line.connection }?.text("outcome")
            if let place = landing(path, outcome: line.line == Line.awaits ? outcome : nil, base: base, in: detail) {
                return .place(place)
            }
        }
        return .header
    }

    /// The node a way out of a node goes to, by the row of its card that says it
    /// (`FlowCardMap.rows`: `#next.0`, `#b.1`, `#else`, `#err`, `#timeout`).
    static func way(_ row: String, of node: FlowNode) -> String? {
        func index(_ prefix: String) -> Int? { row.hasPrefix(prefix) ? Int(row.dropFirst(prefix.count)) : nil }
        if let i = index("#next."), node.nexts.indices.contains(i) { return node.nexts[i] }
        if let i = index("#b."), node.branches.indices.contains(i) { return node.branches[i].next }
        switch row {
        case "#else": return node.otherwise
        case "#err": return node.onError
        case "#timeout": return node.onTimeout
        default: return nil
        }
    }

    // MARK: - A goal's card map

    /// What a press on a card of a goal's map picks: a goal inside, its row, as on the
    /// relation panel; else the card. `card` is the project's card the map's stands for.
    static func goalMapPick(_ row: CardRow?, card: SchemaNode) -> Pick {
        if card.kind.isCustom, let row, CustomRows.row(at: row.path, in: card.customRows) != nil {
            return .row(card.id, .customRow(row.path))
        }
        if let row, let own = ownRow(row, on: card) { return .row(card.id, own) }
        return .card(card.id)
    }

    /// The paths that name what a press on a goal's map picks.
    static func goalMapPaths(_ row: CardRow?, card: SchemaNode, in detail: ProjectDetail) -> [String] {
        if case .row(_, .customRow(let path)) = goalMapPick(row, card: card) {
            return [detail.directory.full(card, row: path)]
        }
        return [detail.directory.full(card)]
    }
}

extension AppModel {
    /// What was picked on a diagram, in the property panel (`DiagramRows.Pick`).
    func apply(_ pick: DiagramRows.Pick) {
        switch pick {
        case .card(let id): selectSchema(id, follow: false)
        case .field(let card, let field): selectField(card, field, follow: false)
        case .row(let card, let row): selectRow(card, row)
        case .ghost(let id): selectGhost(id, follow: false)
        case .nothing: break
        }
    }

    /// A place opened in its card's editor, the field or row picked there.
    func open(_ place: DiagramRows.Place) {
        open(.card(place.card))
        apply(place.pick)
    }
}

// MARK: - The window's side

/// The presses of one diagram, wired to the window (`CardCanvasView.onPick`, `onFollow`,
/// `onRowMenu`, `onPickLine`, `canConnect`, `onConnect`): the relation panel's, a flow's card
/// map's, a goal's. What they mean is `DiagramRows`'.
struct DiagramPresses {
    var pick: (CardCanvasView.RowHit) -> Void
    var follow: (CardCanvasView.RowHit) -> Void
    var menu: (CardCanvasView.RowHit?) -> NSMenu?
    /// A line pressed on the diagram (overview §13「線を選ぶ」): the line it draws, picked. Nil,
    /// lines are not picked there (a goal's card map draws none).
    var pickLine: ((Relation) -> Void)? = nil
    /// Whether a line may be drawn by hand from a row or a header — its handle shows there;
    /// nil, nowhere (a diagram only looked at, a goal's card map).
    var canConnect: ((CardCanvasView.RowHit) -> Bool)? = nil
    /// A line dragged from a handle and let go on a card's header or a row: from where, to
    /// what, and where it was let go (the canvas's points) — つなぐ there.
    var connect: (CardCanvasView.RowHit, CardCanvasView.RowHit, CGPoint) -> Void = { _, _, _ in }

    func wire(_ view: CardCanvasView) {
        view.onPick = pick
        view.onFollow = follow
        view.onRowMenu = menu
        view.onPickLine = pickLine
        view.canConnect = canConnect
        view.onConnect = connect
    }

    /// Nothing done: for a diagram only looked at.
    static let none = DiagramPresses(pick: { _ in }, follow: { _ in }, menu: { _ in nil })

    /// A line pressed on a diagram, picked by the card that holds it.
    static func pickLine(_ model: AppModel) -> (Relation) -> Void {
        { relation in
            guard let id = relation.connection, let ref = model.lineRef(connection: id) else { return }
            model.selectLine(ref)
        }
    }

    // MARK: The relation panel

    /// The relation panel's: picked as `DiagramRows.pick`; followed to the first place a
    /// row points at (`go`) — a section ghost's header to its section, any other header
    /// and a row pointing nowhere to its card's editor, the row picked there.
    static func relationPanel(_ model: AppModel, _ graph: CardCanvasController) -> DiagramPresses {
        DiagramPresses(
            pick: { hit in
                guard let detail = model.detail else { return }
                // main's section ghost on the board's: main's card, shown in main.
                if model.showOutside(hit, header: false) { return }
                model.apply(DiagramRows.pick(hit, in: detail))
            },
            follow: { hit in
                guard let detail = model.detail else { return }
                if model.showOutside(hit, header: true) { return }
                if let place = DiagramRows.targets(hit, in: detail).first {
                    go(to: place, model, graph)
                } else if hit.row == nil, let ghost = detail.directory.ghost(id: hit.card) {
                    // A card ghost's header: the card it leads to, in its editor.
                    if let card = detail.directory.reached(ghost) { model.open(.card(card.id)) }
                } else if SectionGhosts.isGhost(hit.card) {
                    let id = String(hit.card.dropFirst(SectionGhosts.prefix.count))
                    let root = detail.directory.section(id: id)?.parentId == nil
                    model.open(root ? model.rootPlace(id) : .section(id))
                } else {
                    let pick = DiagramRows.pick(hit, in: detail)
                    guard let card = pick.card else { return }
                    model.open(.card(card))
                    // A frame opened on the section it holds: nothing of that section's to pick.
                    if model.frameTarget(of: card) == nil { model.apply(pick) }
                }
            },
            menu: { hit in DiagramMenus.relationPanel(hit, model, graph) },
            pickLine: pickLine(model),
            // Not where the cards are read only (a locked main too): a line is only looked at.
            canConnect: { hit in
                guard !model.isReadOnly, let detail = model.detail else { return false }
                return DiagramRows.lineSource(hit, in: detail) != nil
            },
            connect: { from, to, point in
                guard !model.isReadOnly, let detail = model.detail,
                      let source = DiagramRows.lineSource(from, in: detail),
                      let target = DiagramRows.lineTarget(to, in: detail) else { return }
                model.connecting = Connecting(source: source, target: target, anchor: point)
            })
    }

    /// Where a row points, from the relation panel: picked on the diagram when its card is
    /// drawn there — this section's, or another's put whole — or on its own section's
    /// diagram when it is another section's. A value object or an enum, drawn inside the
    /// cards that hold it, opens in its editor.
    static func go(to place: DiagramRows.Place, _ model: AppModel, _ graph: CardCanvasController) {
        guard let detail = model.detail, let card = model.schema(place.card) else { return }
        if graph.draws(place.card) {
            model.apply(place.pick)
            graph.focus(place.card, field: place.mark)
            return
        }
        // Drawn here in a card ghost of the section on screen: there, its row marked.
        if let ghost = model.surroundings.ghosts.first(where: { $0.resolvesTo == place.card && graph.draws($0.id) }) {
            model.apply(place.pick)
            graph.focus(ghost.id, field: place.mark)
            return
        }
        if card.kind == .value || card.kind == .enum {
            model.open(place)
            return
        }
        let section = detail.directory.sectionId(of: card)
        guard section != model.shownSectionKey else {
            // On this section's diagram but not drawn now (filtered out): picked all the same.
            model.apply(place.pick)
            return
        }
        model.open(.section(section))
        model.apply(place.pick)
        graph.focusAfterNextShow(place.card, field: place.mark)
    }

    // MARK: A card's connections diagram

    /// A connections diagram's (`ConnectionsMap`): the cards are the project's own, so picked and
    /// right-clicked as on the relation panel; followed, a row to where it points — on this
    /// diagram when that card is drawn here, else in its card's editor — and a header to its
    /// card's editor. A line is drawn by hand as on the relation panel.
    static func connections(_ model: AppModel, _ controller: CardCanvasController) -> DiagramPresses {
        var presses = relationPanel(model, controller)
        presses.follow = { hit in
            guard let detail = model.detail else { return }
            // main's card on a board's card's diagram: shown in main.
            if let main = model.outsideDetail, main.directory.schema(id: hit.card) != nil {
                model.showInMain(hit.card, inside: hit.row?.field?.id)
                return
            }
            if let place = DiagramRows.targets(hit, in: detail).first {
                if controller.draws(place.card) {
                    model.apply(place.pick)
                    controller.focus(place.card, field: place.mark)
                } else {
                    model.open(place)
                }
                return
            }
            let pick = DiagramRows.pick(hit, in: detail)
            guard let card = pick.card else { return }
            model.open(.card(card))
            model.apply(pick)
        }
        presses.menu = { hit in DiagramMenus.relationPanel(hit, model, controller, adding: false) }
        return presses
    }

    // MARK: A flow's card map

    /// A flow's card map's: a process node's row picked as the row, any other part of a
    /// node's card as the node; followed — a way out to the node it leads to on the map, a
    /// row reading or running something to that card's editor, the rest as the header: a
    /// run opens what it runs. `menu` is the flow editor's.
    static func flowMap(_ model: AppModel, flow: SchemaNode, detail: ProjectDetail, controller: CardCanvasController,
                        menu: @escaping (CardCanvasView.RowHit?) -> NSMenu?) -> DiagramPresses {
        DiagramPresses(
            pick: { hit in
                model.selectRow(flow.id, DiagramRows.flowMapPick(hit.row, node: FlowCardMap.node(of: hit.card)))
            },
            follow: { hit in
                let name = FlowCardMap.node(of: hit.card)
                switch DiagramRows.flowMapTarget(hit.row, node: name, flow: flow, in: detail) {
                case .card(let id):
                    model.selectNode(flow.id, FlowCardMap.node(of: id))
                    controller.focus(id)
                case .place(let place):
                    model.open(place)
                case .header:
                    guard let target = flow.nodes.first(where: { $0.name == name })?.target else { return }
                    model.open(path: target, from: detail.directory.sectionId(of: flow))
                }
            },
            menu: menu,
            pickLine: pickLine(model),
            canConnect: { hit in
                guard !model.isReadOnly, let whole = model.detail else { return false }
                return DiagramRows.flowMapLineSource(hit, flow: flow, in: whole) != nil
            },
            connect: { from, to, point in
                guard !model.isReadOnly, let whole = model.detail,
                      let source = DiagramRows.flowMapLineSource(from, flow: flow, in: whole),
                      let target = DiagramRows.flowMapLineTarget(to, flow: flow, in: whole) else { return }
                model.connecting = Connecting(source: source, target: target, anchor: point)
            })
    }

    // MARK: A goal's card map

    /// A goal's card map's: a task picked as itself, a goal inside's row as that row;
    /// followed, every part of a card as its header — a task's editor, a goal inside's
    /// card map (a map's cards draw no lines: nothing on them points elsewhere).
    static func goalMap(_ model: AppModel, menu: @escaping (CardCanvasView.RowHit?) -> NSMenu?) -> DiagramPresses {
        DiagramPresses(
            pick: { hit in
                guard let card = model.schema(GoalCardMap.card(of: hit.card)) else { return }
                model.apply(DiagramRows.goalMapPick(hit.row, card: card))
            },
            follow: { hit in
                guard let card = model.schema(GoalCardMap.card(of: hit.card)) else { return }
                model.open(.card(card.id))
                if hit.row != nil { model.apply(DiagramRows.goalMapPick(hit.row, card: card)) }
            },
            menu: menu)
    }
}

/// The right-click menus of the diagrams' cards and rows.
enum DiagramMenus {

    /// The relation panel's: on a row, the cards it points at, then the row — its path to
    /// copy and its card's editor; on a header, the card; and a card to add, where cards
    /// may be added (`adding`: not on a connections diagram, which is no section's).
    static func relationPanel(_ hit: CardCanvasView.RowHit?, _ model: AppModel, _ graph: CardCanvasController,
                              adding: Bool = true) -> NSMenu? {
        let menu = NSMenu()
        if let hit, let detail = model.detail {
            // The cards this *row* points at, not its own. The card's own is the same answer
            // on every one of its rows and is one right-click on the header away; what a
            // reader cannot get to from here is the thing the row points at.
            var seen = Set<String>()
            for place in DiagramRows.targets(hit, in: detail) where seen.insert(place.card).inserted {
                guard let target = model.schema(place.card) else { continue }
                card(menu, target, model, graph)
                menu.addItem(.separator())
            }
            if hit.row == nil, let ghost = detail.directory.ghost(id: hit.card) {
                ghostCard(menu, ghost, model, graph)
                connectItem(menu, hit, model, graph)
            } else if let row = hit.row {
                let pick = DiagramRows.pick(hit, in: detail)
                let paths = DiagramRows.paths(hit, in: detail)
                menu.addItem(.sectionHeader(title: paths.first ?? row.path))
                if let owner = pick.card {
                    menu.addItem(ActionMenuItem(L10n[.Diagram.menuOpenCardShowRow]) {
                        model.open(.card(owner))
                        model.apply(pick)
                    })
                }
                menu.addItem(copyPathItem(paths))
                connectItem(menu, hit, model, graph)
            } else if let owner = model.schema(hit.card) {
                card(menu, owner, model, graph)
                connectItem(menu, hit, model, graph)
            }
            menu.addItem(.separator())
        }
        // As the nav panel's: not where the cards are read only (a locked main too).
        if adding && !model.isReadOnly {
            menu.addItem(ActionMenuItem(L10n[.Diagram.menuAddCard]) { model.newSchemaRequest = AppModel.NewSchemaRequest() })
        }
        // A working directory's section map: its folder read again.
        if adding, model.isInWorkDir(section: model.shownSectionKey) {
            menu.addItem(workDirActionsItem(model.shownSectionKey, model))
        }
        if menu.items.last?.isSeparatorItem == true { menu.removeItem(at: menu.items.count - 1) }
        return menu.items.isEmpty ? nil : menu
    }

    /// 「つなぐ…」 (overview §13): from the row or the header, a line to a place picked in the
    /// selector, shown beside it — not where the cards are read only (a locked main too).
    static func connectItem(_ menu: NSMenu, _ hit: CardCanvasView.RowHit, _ model: AppModel,
                            _ graph: CardCanvasController) {
        guard !model.isReadOnly, let detail = model.detail,
              let source = DiagramRows.lineSource(hit, in: detail) else { return }
        menu.addItem(ActionMenuItem(connectWord) {
            model.connecting = Connecting(source: source, anchor: graph.anchor(of: hit))
        })
    }

    /// The menus' word for drawing a line by hand.
    static var connectWord: String { L10n[.Lines.actionConnectMenu] }

    /// The menus' word for a card's connections diagram (`ConnectionsMap`).
    static var connectionsWord: String { L10n[.Diagram.menuConnections] }

    /// A card ghost's own part (`specboard://SpecBoard/Docs/Design:DesignSections` §2): the card it leads to opened,
    /// its path and the card's to copy, how its box is drawn, and the ghost let go — not
    /// where a locked main keeps its cards.
    static func ghostCard(_ menu: NSMenu, _ ghost: CardGhost, _ model: AppModel, _ graph: CardCanvasController) {
        guard let directory = model.detail?.directory else { return }
        let card = directory.reached(ghost)
        menu.addItem(.sectionHeader(title: "\(SchemaDirectory.ghostMark) \(directory.full(ghost))"))
        // Leading nowhere, it has no card to open nor a card's path.
        if let card {
            menu.addItem(ActionMenuItem(L10n[.Diagram.menuOpenReal]) { model.open(.card(card.id)) })
            menu.addItem(ActionMenuItem(connectionsWord) { model.openConnections(card.id) })
        }
        menu.addItem(ActionMenuItem(L10n[.Diagram.menuCopyGhostPath]) { copyToPasteboard(directory.full(ghost)) })
        if let card { menu.addItem(ActionMenuItem(L10n[.Diagram.menuCopyRealPath]) { copyToPasteboard(directory.full(card)) }) }
        if graph.canFold(ghost.id) {
            menu.addItem(ActionMenuItem(L10n[.Common.actionExpandAll]) { graph.foldAll(ghost.id, false) })
            menu.addItem(ActionMenuItem(L10n[.Common.actionCollapseAll]) { graph.foldAll(ghost.id, true) })
        }
        if !model.isReadOnly {
            menu.addItem(.separator())
            menu.addItem(ActionMenuItem(L10n[.Diagram.menuDeleteGhost]) { model.deleteGhost(ghost.id) })
        }
    }

    /// A card's own part: open it, its path, where it may go, how it is drawn here.
    static func card(_ menu: NSMenu, _ schema: SchemaNode, _ model: AppModel, _ graph: CardCanvasController) {
        menu.addItem(.sectionHeader(title: model.detail?.directory.shown(schema) ?? schema.name))
        menu.addItem(ActionMenuItem(L10n[.Common.actionOpen]) { model.open(.card(schema.id)) })
        menu.addItem(ActionMenuItem(connectionsWord) { model.openConnections(schema.id) })
        menu.addItem(copyPathItem(model.detail.map { schemaPaths($0, id: schema.id) } ?? []))
        if let item = actionsItem(schema, model) { menu.addItem(item) }
        stackItems(menu, schema, model)
        // Sorting what the free section holds: into a section of its kind. A working directory's file is
        // read from its folder: never moved nor deleted here.
        if let detail = model.detail, !model.isReadOnly, !model.isReadFromFolder(schema.id) {
            let move = NSMenuItem(title: L10n[.Diagram.menuMoveToSection], action: nil, keyEquivalent: "")
            let submenu = NSMenu()
            for section in detail.homes(for: schema) {
                submenu.addItem(ActionMenuItem(detail.directory.shownFromTop(section: section.id)) {
                    model.moveSchema(schema.id, toSection: section.id)
                })
            }
            move.submenu = submenu
            move.isEnabled = !submenu.items.isEmpty
            menu.addItem(move)
            menu.addItem(.separator())
            menu.addItem(ActionMenuItem(L10n[.Diagram.menuDeleteCard]) {
                if CardDeletion.confirm(schema.name) { model.deleteSchema(schema.id) }
            })
        }
        if graph.canFold(schema.id) {
            menu.addItem(ActionMenuItem(L10n[.Common.actionExpandAll]) { graph.foldAll(schema.id, false) })
            menu.addItem(ActionMenuItem(L10n[.Common.actionCollapseAll]) { graph.foldAll(schema.id, true) })
        }
        if graph.documentMoved(schema.id) {
            menu.addItem(ActionMenuItem(L10n[.Diagram.menuFitCardContents]) { graph.showWholeDocument(schema.id) })
        }
        // How a formatdoc of this section is drawn: as its rows or as the document it reads as.
        // How it looks, so a locked main takes it too.
        if schema.kind == .formatdoc, !model.isArchived, !model.isOutside(schema.id) {
            let document = drawnAsDocument(schema, docView: model.defaultSettings.docView)
            menu.addItem(ActionMenuItem(document ? L10n[.Diagram.menuViewRows] : L10n[.Diagram.menuViewDocument]) {
                model.setDocView(schema.id, document ? "rows" : "document")
            })
        }
        for item in sizeItems(schema, model) { menu.addItem(item) }
        // Put there for a look: let it go, or keep it — another section's card as a card
        // ghost of this section (not in a locked main, `specboard://SpecBoard/Docs/Design:DesignBranches` §8), one of
        // its own value objects or enums shown on its own (how it looks: a locked main too).
        if model.isForNow(schema.id) {
            menu.addItem(.separator())
            menu.addItem(ActionMenuItem(L10n[.Diagram.menuStopShowingTemporary]) { model.stopShowingForNow(schema.id) })
            if model.isOutside(schema.id) ? !model.isReadOnly : model.canSetLooks {
                let keep = model.isOutside(schema.id) ? L10n[.Diagram.menuKeepAsGhost] : L10n[.Diagram.menuKeepStandalone]
                menu.addItem(ActionMenuItem(keep) { model.keepForNow(schema.id) })
            }
        } else if schema.standalone, model.canSetLooks, !model.isOutside(schema.id) {
            menu.addItem(.separator())
            menu.addItem(ActionMenuItem(L10n[.Diagram.menuStopStandalone]) { model.setStandalone(schema.id, false) })
        }
    }

    /// 「最前面へ」「前面へ」「背面へ」「最背面へ」 for a part of a ui section: where the card stands in its
    /// section's order, which is the order parts overlap in (the later one on top). The nav panel's
    /// drag writes the same position, so a locked main takes it the same way.
    static func stackItems(_ menu: NSMenu, _ schema: SchemaNode, _ model: AppModel) {
        guard let detail = model.detail, CardCanvasView.partKinds.contains(schema.kind), !model.isOutside(schema.id),
              detail.directory.sectionKind(of: schema) == .ui else { return }
        let members = detail.directory.members(detail.directory.sectionId(of: schema)).map(\.id)
        guard let at = members.firstIndex(of: schema.id) else { return }
        let allowed = !model.isArchived && model.heldByMCP.isEmpty
        menu.addItem(.separator())
        let moves: [(String, Int, Bool)] = [
            (L10n[.Diagram.menuBringToFront], members.count - 1, at < members.count - 1),
            (L10n[.Diagram.menuBringForward], at + 1, at < members.count - 1),
            (L10n[.Diagram.menuSendBackward], at - 1, at > 0),
            (L10n[.Diagram.menuSendToBack], 0, at > 0),
        ]
        for (title, position, movable) in moves {
            // Without an action the menu leaves it greyed; a target would be enabled again when it opens.
            if allowed && movable {
                menu.addItem(ActionMenuItem(title) { model.putCard(schema.id, at: position) })
            } else {
                let item = NSMenuItem(title: title, action: nil, keyEquivalent: "")
                item.isEnabled = false
                menu.addItem(item)
            }
        }
        menu.addItem(.separator())
    }

    /// 「アクション ▸ 読み込む」 for a working directory's section or a folder in it: its folder read again,
    /// greyed with why where it cannot be now.
    static func workDirActionsItem(_ section: String, _ model: AppModel) -> NSMenuItem {
        let item = NSMenuItem(title: L10n[.Board.actionsMenu], action: nil, keyEquivalent: "")
        let submenu = NSMenu()
        let why = model.whyNotRead(section)
        if why == nil, model.canEditWorkDirs {
            let entry = ActionMenuItem(L10n[.Properties.workDirRead]) { model.readWorkDir(section) }
            entry.toolTip = L10n[.Properties.workDirReadHelp]
            submenu.addItem(entry)
        } else {
            let entry = NSMenuItem(title: L10n[.Properties.workDirRead], action: nil, keyEquivalent: "")
            entry.isEnabled = false
            entry.toolTip = why
            submenu.addItem(entry)
        }
        item.submenu = submenu
        return item
    }

    /// 「アクション ▸」 for a goal or a task: what the core takes now, as the property panel's buttons
    /// (`pressCardAction`); nil when none is.
    static func actionsItem(_ schema: SchemaNode, _ model: AppModel) -> NSMenuItem? {
        let actions = model.availableCardActions(schema.id)
        guard !actions.isEmpty else { return nil }
        let item = NSMenuItem(title: L10n[.Board.actionsMenu], action: nil, keyEquivalent: "")
        let submenu = NSMenu()
        for action in actions {
            let entry = ActionMenuItem(action.label) { model.pressCardAction(schema.id, action) }
            entry.toolTip = action.help
            submenu.addItem(entry)
        }
        item.submenu = submenu
        return item
    }

    /// A card's own size, how it looks (so a locked main takes it too): 「大きさを既定に戻す」 when it
    /// has one, and 「中身に合わせて幅を決める」 on a card drawn as rows. None for a card that
    /// cannot carry a size or is not this section's own.
    static func sizeItems(_ schema: SchemaNode, _ model: AppModel) -> [NSMenuItem] {
        guard schema.sizable, !model.isArchived, !model.isOutside(schema.id) else { return [] }
        var out: [NSMenuItem] = []
        if model.fitsWidth(schema) {
            out.append(ActionMenuItem(L10n[.Diagram.menuFitWidth]) { model.fitWidth(schema.id) })
        }
        if schema.size != nil {
            out.append(ActionMenuItem(L10n[.Diagram.menuResetSize]) { model.setCardSize(schema.id, nil) })
        }
        return out
    }

    /// Where a card map's row points, as items: the node it leads to, or the card of the
    /// project, opened.
    static func mapTarget(_ menu: NSMenu, _ target: DiagramRows.MapTarget, _ model: AppModel,
                          go: @escaping (String) -> Void) {
        switch target {
        case .card(let id):
            let node = FlowCardMap.node(of: id)
            menu.addItem(.sectionHeader(title: "→ \(node)"))
            menu.addItem(ActionMenuItem(L10n[.Diagram.menuGoToNode]) { go(id) })
            menu.addItem(.separator())
        case .place(let place):
            guard let card = model.schema(place.card), let detail = model.detail else { return }
            menu.addItem(.sectionHeader(title: detail.directory.full(card)))
            menu.addItem(ActionMenuItem(L10n[.Common.actionOpen]) { model.open(place) })
            menu.addItem(copyPathItem(schemaPaths(detail, id: card.id)))
            menu.addItem(.separator())
        case .header:
            break
        }
    }

    /// One path copies straight; several become a list to pick from, because an
    /// embedded value schema is genuinely called more than one thing and the app
    /// cannot know which of them is being pasted somewhere.
    static func copyPathItem(_ paths: [String]) -> NSMenuItem {
        guard let first = paths.first else {
            let item = NSMenuItem(title: L10n[.Common.actionCopyPath], action: nil, keyEquivalent: "")
            item.isEnabled = false
            return item
        }
        guard paths.count > 1 else {
            return ActionMenuItem(L10n[.Common.actionCopyPath]) { copyToPasteboard(first) }
        }

        let item = NSMenuItem(title: L10n[.Common.actionCopyPath], action: nil, keyEquivalent: "")
        let submenu = NSMenu()
        for path in paths {
            submenu.addItem(ActionMenuItem(path) { copyToPasteboard(path) })
        }
        if paths.count >= pathCeiling {
            // Say so rather than letting a truncated list read as the whole answer.
            let note = NSMenuItem(title: L10n[.Diagram.menuPathsCutOff(pathCeiling)],
                                  action: nil, keyEquivalent: "")
            note.isEnabled = false
            submenu.addItem(.separator())
            submenu.addItem(note)
        }
        item.submenu = submenu
        return item
    }
}
