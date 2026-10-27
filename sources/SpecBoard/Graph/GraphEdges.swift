import CoreGraphics
import Foundation

/// The drawn graph: what the relation panel shows of the project's lines.
///
/// The lines are the core's (`project_detail`'s `relations`, every line of every card): the
/// screen makes none. What it does is put each on what it draws — the card it leaves, or the card
/// it is a row of (a flow's node, a goal's task, a value object unfolded in its holder), the row it
/// leaves from, the box it arrives at (a card ghost's, where the line was written through one or
/// the card has one here) and the row it lands on — and gather the lines between two cards
/// (`merge`).

/// What the relation panel is currently showing. Ported from `filters.ts`.
struct GraphFilters: Equatable {
    /// Which cards unfold the value objects they hold. Set by the canvas from the
    /// layout on screen — never by the filter controls. See `Nesting`.
    var nesting = Nesting()
    var kinds: Set<SchemaKind> = Set(SchemaKind.allCases)
    /// Which kinds of line are switched *off*, by name.
    ///
    /// Stated as an exclusion rather than a whitelist, because a project defines kinds of
    /// line of its own: a whitelist would silently draw nothing for one it does not list.
    /// The legend is built from what the project draws, so every kind has a row and anything
    /// unfamiliar starts visible.
    var hiddenLines: Set<String> = []
    /// Which kinds of line hidden from the start by their definition (`visible: false`) are
    /// switched *on*, by name. Not kept: a project opened again starts from its definitions.
    var shownLines: Set<String> = []
    // No tags here: the diagram is not cut by tag. Cards are gathered by tag on the
    // root's タグ view, the nav panel's search and `card_list` (overview §13).


    /// In a project with sections: whether the other sections' schemas this one points
    /// at, and the ones pointing at it, get faded cards of their own.
    var outsideReferences: Bool = true
    var outsideConsumers: Bool = true

    /// Whether a kind of line is drawn: not switched off, and — hidden from the start by its
    /// definition — switched on.
    func shows(_ line: String, kinds: LineKinds) -> Bool {
        if hiddenLines.contains(line) { return false }
        if kinds[line]?.visible == false { return shownLines.contains(line) }
        return true
    }

    /// As the drawing reads them: the kinds hidden from the start and not switched on, hidden.
    func drawing(with kinds: LineKinds) -> GraphFilters {
        var out = self
        for kind in kinds.list where !kind.visible && !shownLines.contains(kind.line) {
            out.hiddenLines.insert(kind.line)
        }
        return out
    }

    /// Changes exactly when the set of drawn *cards* might have. Line switches only add
    /// or remove lines, so they are deliberately not part of it.
    var frameKey: String {
        [String(nesting.base), nesting.flipped.sorted().joined(separator: ","),
         kinds.map(\.rawValue).sorted().joined(separator: ","),
         String(outsideReferences), String(outsideConsumers)]
            .joined(separator: "|")
    }
}

/// Which cards are drawn at all: the kinds switched on.
func visibleSchemas(_ detail: ProjectDetail, _ f: GraphFilters) -> [SchemaNode] {
    let candidates = detail.schemas.filter { f.kinds.contains($0.kind) }
    return withoutValueCards(candidates, relations: detail.relations)
}

/// Value objects and enums have no card of their own: they are drawn inside the
/// cards that hold them, folded to a row with their name. A card for each, with
/// lines to it from everything that uses it, would be lines in every direction from
/// every card. One is drawn otherwise only when placed — as a card ghost, kept or for
/// now — and that is decided by whoever draws, not here.
///
/// Except one that no card drawn here holds — nothing in the section, or only
/// another section's cards: it keeps a card of its own, or it would be drawn nowhere. What that
/// card holds is folded into it, so holders go before what they hold, and a ring of them that
/// nothing else holds falls back to project order.
func withoutValueCards(_ candidates: [SchemaNode], relations: [Relation]) -> [SchemaNode] {
    func isType(_ schema: SchemaNode) -> Bool { schema.kind == .value || schema.kind == .enum }
    var holds: [String: [String]] = [:]
    for relation in relations where relation.line == Line.embed {
        holds[relation.fromSchemaId, default: []].append(relation.toSchemaId)
    }
    var reached = Set<String>()
    func reach(from id: String) {
        for held in holds[id] ?? [] where reached.insert(held).inserted { reach(from: held) }
    }
    var kept = Set<String>()
    for card in candidates where !isType(card) {
        kept.insert(card.id)
        reach(from: card.id)
    }
    var waiting = candidates.filter { isType($0) && !reached.contains($0.id) }
    while !waiting.isEmpty {
        let heldByWaiting = Set(waiting.flatMap { holds[$0.id] ?? [] })
        let next = waiting.first { !heldByWaiting.contains($0.id) } ?? waiting[0]
        kept.insert(next.id)
        reached.insert(next.id)
        reach(from: next.id)
        waiting.removeAll { reached.contains($0.id) }
    }
    return candidates.filter { kept.contains($0.id) }
}

/// A card's rows plus the lookup that turns a dotted path into a row index, so
/// anchoring an edge is a lookup rather than a scan.
struct CardIndex {
    var content: CardContent
    var indexByPath: [String: Int]

    init(_ content: CardContent) {
        self.content = content
        var index: [String: Int] = [:]
        for (i, row) in content.all.enumerated() where index[row.path] == nil {
            index[row.path] = i
        }
        // Rows folded into one, a container's cards done under 「済み」: found at it.
        for (i, row) in content.all.enumerated() {
            for path in row.stands where index[path] == nil { index[path] = i }
        }
        self.indexByPath = index
    }
}

/// One drawable edge per pair of boxes. Reciprocal lines (`User.orders` / `Order.user`)
/// describe one relationship, so they collapse into a single 1:N line.
struct GraphEdge {
    var key: String
    var from: String
    var to: String
    /// Nil when the pair says none — an embedding does not.
    var cardinality: Cardinality?
    var vias: [Relation]

    /// The kind of line it is drawn as: the first of its lines leaving `from`.
    var line: String {
        (vias.first { $0.fromSchemaId == from } ?? vias[0]).line
    }

    /// The row this line lands on, when it names one.
    var landingPath: String? {
        let via = vias.first { $0.fromSchemaId == from } ?? vias[0]
        return via.at.flatMap { $0.isEmpty ? nil : $0 }
    }
}

enum GraphEdges {

    /// Rows for every visible schema, computed once and used for both the card body
    /// and the edge anchors.
    /// `tree`: the tree being looked at — its paths show on the rows without it.
    static func index(
        _ schemas: [SchemaNode], lookup: @escaping SchemaLookup, filters: GraphFilters,
        scroll: [String: Int] = [:], folds: [String: Set<String>] = [:], tree: String? = nil,
        forms: [String: CardForm] = [:]
    ) -> [String: CardIndex] {
        var out: [String: CardIndex] = [:]
        for s in schemas {
            out[s.id] = CardIndex(cardRows(s, lookup: lookup,
                                           inlineReuse: filters.nesting.nests(s.id),
                                           from: scroll[s.id] ?? 0,
                                           folded: folds[s.id] ?? [], tree: tree,
                                           form: forms[s.id] ?? CardForm()))
        }
        return out
    }

    /// Where a card is drawn: as a box of its own (no row), or as a row of the box of
    /// what holds it — a flow's node as its flow's `#n.<node>`, a goal's card as the goal's
    /// row of its name. Nil when it is not drawn here.
    struct Drawn: Equatable {
        var box: String
        var row: String
    }

    /// What the project says of the cards inside containers, to find where they are drawn.
    struct Holders {
        /// A flow's node card, by its id: its flow and its name.
        var nodes: [String: (flow: String, name: String)] = [:]
        /// The project's cards, by id.
        var cards: [String: SchemaNode] = [:]
        /// Every card's lines, the nodes' too, by the card's id.
        var lines: [String: [Connection]] = [:]
        /// The same by each line's id (the first of an id): a relation's line found without walking the
        /// card's lines, which a card with many lines would do once for each of them.
        private var byId: [String: [String: Connection]] = [:]
        /// For a store's held lines, each line's place among its card's lines of that kind (`sourceRow`),
        /// by the card's id and the line's id.
        private var heldPlaces: [String: [String: Int]] = [:]

        init(_ detail: ProjectDetail, more: [SchemaNode] = []) {
            for node in detail.inner {
                nodes[node.id] = (node.container, node.name)
                lines[node.id] = node.connections
            }
            for card in detail.schemas + more where cards[card.id] == nil {
                cards[card.id] = card
                lines[card.id] = card.connections
                if let kind = GraphEdges.heldLine(of: card.kind) {
                    let held = card.connections.filter { $0.line == kind && $0.from.isEmpty }
                    heldPlaces[card.id] = Dictionary(held.enumerated().map { ($1.id, $0) }, uniquingKeysWith: { first, _ in first })
                }
            }
            for (id, list) in lines where !list.isEmpty {
                byId[id] = Dictionary(list.map { ($0.id, $0) }, uniquingKeysWith: { first, _ in first })
            }
        }

        /// The line a relation is, in the card that holds it.
        func line(of relation: Relation) -> Connection? {
            guard let id = relation.connection else { return nil }
            return byId[relation.fromSchemaId]?[id]
        }

        /// `GraphEdges.sourceRow` of the relation's own card, as the project has it: a held line's place
        /// read from what was counted once for the card.
        func sourceRow(_ relation: Relation) -> String {
            let card = cards[relation.fromSchemaId]
            guard let card, let kind = GraphEdges.heldLine(of: card.kind), relation.line == kind,
                  let places = heldPlaces[card.id] else {
                return GraphEdges.sourceRow(relation, of: card)
            }
            let index = relation.connection.flatMap { places[$0] }
            return index.map { "#h\($0)" } ?? ""
        }
    }

    /// The kind of line a card of `kind` draws one row for each of, by its place among them: a store's
    /// held shapes. A flow's notes leave its head with no row of their own.
    static func heldLine(of kind: SchemaKind) -> String? {
        switch kind {
        case .store: Line.holds
        default: nil
        }
    }

    /// Where the card `id` is drawn among `visible` (`Drawn`): itself, or the box of its
    /// container (up to the one drawn), or a card ghost of it drawn here (`standIns`).
    static func drawn(_ id: String, visible: Set<String>, holders: Holders,
                      standIns: [String: String] = [:]) -> Drawn? {
        if visible.contains(id) { return Drawn(box: id, row: "") }
        if let node = holders.nodes[id] {
            guard let flow = drawn(node.flow, visible: visible, holders: holders, standIns: standIns) else { return nil }
            return Drawn(box: flow.box, row: flow.row.isEmpty ? SchemaNode.nodeRow(node.name) : flow.row)
        }
        if let container = holders.cards[id]?.container,
           let holder = drawn(container, visible: visible, holders: holders, standIns: standIns) {
            // A goal's card is its row; a card inside one of those, that row.
            return Drawn(box: holder.box, row: holder.row.isEmpty ? holders.cards[id].map { PathText.quote($0.name) } ?? "" : holder.row)
        }
        if let ghost = standIns[id] { return Drawn(box: ghost, row: "") }
        return nil
    }

    /// The row of the card `card` (drawn as a box) a line of `relation` leaves from: a
    /// field's names, a formatdoc's item (`#top` for the rows at the top), a store's held row
    /// (by the line's place among its kind's), the card's header for the rest (a note's, whose
    /// body is drawn as one page, and a flow's rules among them).
    static func sourceRow(_ relation: Relation, of card: SchemaNode?) -> String {
        guard let card else { return relation.from }
        switch (card.kind, relation.line) {
        case (.store, Line.holds):
            let index = card.connections.filter { $0.line == Line.holds && $0.from.isEmpty }
                .firstIndex { $0.id == relation.connection }
            return index.map { "#h\($0)" } ?? ""
        case (.formatdoc, Line.mentions):
            return relation.from.isEmpty ? SchemaNode.topRow : relation.from
        default:
            return relation.from
        }
    }

    /// The row a line lands on in what it reaches: the row the reached card is drawn as
    /// (a goal's task, a flow's node), else where the line says it lands — a field's names,
    /// an item, a flow's node (`#n.<node>`); for a line waiting on another flow's end, the end
    /// with the outcome it waits for. None for the card itself, and for a landing written in
    /// full elsewhere.
    static func landingRow(_ relation: Relation, reached: Drawn, card: SchemaNode?, line: Connection?) -> String? {
        if !reached.row.isEmpty { return reached.row }
        guard let card else { return relation.at }
        if card.kind == .flow {
            if relation.line == Line.awaits, let outcome = line?.text("outcome"),
               let end = card.nodes.first(where: { $0.kind == "end" && $0.outcome == outcome }) {
                return SchemaNode.nodeRow(end.name)
            }
            if let at = relation.at, PathText.isOneName(at), card.nodes.contains(where: { $0.name == PathText.unquote(at) }) {
                return SchemaNode.nodeRow(PathText.unquote(at))
            }
        }
        // An ID pointing at a card with one primary key lands on it, saying no row of its own:
        // the value is that key. None or several: its header.
        if relation.at?.isEmpty ?? true, relation.line == Line.points, let key = primaryKey(of: card) {
            return key
        }
        guard let at = relation.at, !PathText.hasSection(at) else { return nil }
        return at
    }

    /// A card's one primary key field, when it has exactly one (`primaryKey`): the row an ID
    /// pointing at the card lands on, as its row's path.
    static func primaryKey(of card: SchemaNode) -> String? {
        let keys = card.fields.filter(\.isPk)
        return keys.count == 1 ? PathText.quote(keys[0].name) : nil
    }

    /// The project's lines put on what is drawn (`specboard://SpecBoard/Docs/Design:DesignSections` §6): each from the
    /// box and row of the card it leaves — or, for a line from a field, from every row that
    /// field is drawn as (a value object unfolded in the cards holding it says its lines
    /// from there) — to the box it arrives at, landing on its row. A line neither end of
    /// which is drawn, or both ends in one box, is not drawn. Lines of a kind switched off
    /// are left out (`filters.hiddenLines`).
    ///
    /// `ghosts`: the project's card ghosts, which a line may be written through — it arrives at
    /// the ghost's box where that is drawn, else at the card's, else at a card ghost of the
    /// card drawn here (`standIns`: a card → the ghost's box drawn for it).
    static func placed(
        _ relations: [Relation], visible: [SchemaNode], rows: [String: CardIndex],
        filters: GraphFilters, holders: Holders, standIns: [String: String] = [:]
    ) -> [Relation] {
        let visibleIds = Set(visible.map(\.id))
        let shown = Dictionary(visible.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        // Every row a field is drawn as: its own card's, and each card unfolding it.
        var fieldRows: [String: [(box: String, row: String)]] = [:]
        for card in visible {
            for row in rows[card.id]?.content.all ?? [] {
                guard let field = row.field else { continue }
                fieldRows[field.id, default: []].append((card.id, row.path))
            }
        }
        var out: [Relation] = []
        for relation in relations where !filters.hiddenLines.contains(relation.line) {
            // Where it arrives: the ghost it was written through, where that is drawn; else
            // the card it reaches, or what holds it, or a ghost of it.
            let reached: Drawn? = relation.viaGhostId.flatMap { visibleIds.contains($0) ? Drawn(box: $0, row: "") : nil }
                ?? drawn(relation.toSchemaId, visible: visibleIds, holders: holders, standIns: standIns)
            guard let reached else { continue }
            let target = holders.cards[relation.toSchemaId] ?? shown[reached.box]
            let landing = landingRow(relation, reached: reached, card: target, line: holders.line(of: relation))
            // Where it leaves.
            var sources: [(box: String, row: String)] = []
            if !relation.fieldId.isEmpty, let drawnRows = fieldRows[relation.fieldId] {
                sources = drawnRows
            } else if relation.fieldId.isEmpty,
                      let from = drawn(relation.fromSchemaId, visible: visibleIds, holders: holders, standIns: standIns) {
                if from.row.isEmpty {
                    sources = [(from.box, holders.cards[relation.fromSchemaId] != nil
                        ? holders.sourceRow(relation) : sourceRow(relation, of: shown[from.box]))]
                } else if from.row.hasPrefix("#n."), let index = stepIndex(relation.from) {
                    // A process node's row, below its node's.
                    sources = [(from.box, "\(from.row).\(index)")]
                } else {
                    sources = [(from.box, from.row)]
                }
            }
            for source in sources where source.box != reached.box {
                var placed = relation
                placed.fromSchemaId = source.box
                placed.toSchemaId = reached.box
                placed.from = source.row
                placed.at = landing
                out.append(placed)
            }
        }
        // A field pointing at four or more cards is stating a rule, not four
        // relationships. Drawn out, it buries whatever else the diagram was for —
        // nine of nineteen lines on one project, all saying the same sentence. The
        // card's own row says it instead, in one place.
        let wide = wideFans(out)
        return out.filter { !isWideFan($0, wide) }
    }

    /// A node's row a line leaves (`steps[3]` → 3).
    static func stepIndex(_ from: String) -> Int? {
        guard from.hasPrefix("steps["), from.hasSuffix("]") else { return nil }
        return Int(from.dropFirst(6).dropLast())
    }

    /// The project's lines that leave a row of `card` as it is drawn on its own (`placed`, with
    /// only it shown): what a double-click on the row follows, in the order they are drawn.
    static func leaving(_ card: SchemaNode, row: String, in detail: ProjectDetail) -> [Relation] {
        let content = cardRows(card, lookup: { id in detail.directory.schema(id: id) }, inlineReuse: true)
        let rows = [card.id: CardIndex(content)]
        let holders = Holders(detail)
        var filters = GraphFilters()
        filters.hiddenLines = []
        // Everything else is a box it could reach, so no line is lost for want of one.
        let everything = detail.schemas.filter { $0.id != card.id && $0.container == nil }
        let placedLines = placed(detail.relations, visible: [card] + everything, rows: rows,
                                 filters: filters, holders: holders)
        return placedLines.filter { $0.fromSchemaId == card.id && $0.from == row }
    }

    /// Collapse a *reciprocal* pair into the one line that gets drawn.
    ///
    /// `User.orders` and `Order.user` are one relationship seen from both ends, and
    /// drawing both says it twice — so those collapse, and the cardinality decides
    /// which way the surviving line runs.
    ///
    /// Two lines running the *same* way are two statements and stay two lines.
    /// Collapsing them too would quietly drop everything but the first: the line
    /// would leave one of the source rows and the rest would have none, and for a
    /// kind that lands on a row rather than a header it would take the landing with
    /// it. If a pair ever has so many lines that it cannot be read, that is the project
    /// saying something, and the diagram should say it too rather than tidy it away.
    ///
    /// Self-references get no edge: a loop back to the same card costs a lot of space
    /// to say one thing. The field row shows ↺ instead.
    static func merge(_ relations: [Relation]) -> [GraphEdge] {
        var order: [String] = []
        var map: [String: GraphEdge] = [:]
        /// Edge keys by the direction they run, so a relation can find its own reverse.
        var byDirection: [String: [String]] = [:]
        /// Edges that have already taken a reverse. A relationship has two ends, not
        /// three — a third line onto the same pair is a third statement.
        var paired = Set<String>()

        for r in relations where r.fromSchemaId != r.toSchemaId {
            let reverse = "\(r.toSchemaId)|\(r.fromSchemaId)|\(r.line)"
            // Two documents naming each other say two things, each from its own
            // clause to the other's: neither is the far end of the other.
            if r.line != Line.mentions,
               let key = byDirection[reverse]?.first(where: { !paired.contains($0) }),
               var edge = map[key] {
                paired.insert(key)
                if edge.cardinality != .oneToMany && r.cardinality == .oneToMany {
                    // Draw from the "one" side toward the "many" side, once we learn
                    // which is which.
                    edge.from = r.fromSchemaId
                    edge.to = r.toSchemaId
                    edge.cardinality = .oneToMany
                } else if edge.cardinality == nil, let c = r.cardinality {
                    // An embed brings none; a declared one on the same pair is the
                    // better answer.
                    edge.cardinality = c
                }
                edge.vias.append(r)
                map[key] = edge
                continue
            }

            let direction = "\(r.fromSchemaId)|\(r.toSchemaId)|\(r.line)"
            // The row and landing are in the key because they are what makes this
            // line a different statement from its neighbour; the count keeps two
            // identical declarations apart. Keys name layers, so they have to be the
            // same from one redraw to the next.
            let key = "\(direction)|\(r.from)|\(r.at ?? "")"
                + "|\(byDirection[direction]?.count ?? 0)"
            order.append(key)
            map[key] = GraphEdge(key: key, from: r.fromSchemaId, to: r.toSchemaId,
                                 cardinality: r.cardinality, vias: [r])
            byDirection[direction, default: []].append(key)
        }
        return order.compactMap { map[$0] }
    }

    /// The row for this path, or for the nearest ancestor the card actually draws.
    ///
    /// A declared path names a place in the *document*, which goes deeper than the
    /// card does. `StoreSettings.catalog.categories[].featured_ids` lives
    /// inside `Category`, and unless value objects are unfolded in place the deepest
    /// row on the card is `catalog.categories` — so the exact path is not there to
    /// find. Landing on the row that contains it is the honest answer, and the same
    /// rule the strips already follow for rows out of the window. Falling through to
    /// the card's midpoint instead put the line beside an unrelated field and read as
    /// if that field were the target.
    static func nearestRow(_ card: CardIndex, _ path: String) -> Int? {
        if let index = card.indexByPath[path] { return index }
        // A place in the document as its card draws it: its names quoted only where they need
        // it, no `[]`; a row of the card's own (`#n.<node>.<n>`) as it is.
        var parts = PathText.dots(path)
        if !path.hasPrefix("#") { parts = parts.map { PathText.canonicalStep(PathText.withoutAll($0)) } }
        while !parts.isEmpty {
            if let index = card.indexByPath[parts.joined(separator: ".")] { return index }
            parts.removeLast()
        }
        return nil
    }

    static func rowAnchor(_ card: CardIndex?, path: String, box: CGRect) -> CGFloat {
        // No path means the claim is about the whole document rather than about any
        // row in it — `payloads.signedBy` is the one that does this. The header is
        // where the card says its own name, so that is where such a line belongs.
        // Falling through to the midpoint below would put the tail beside an
        // unrelated field and read as if that field were the signer.
        guard !path.isEmpty else { return headerAnchor(of: box) }
        // A card drawn as its document has no rows to leave from: its lines leave
        // from its header, as the whole document's.
        if card?.content.document == true { return headerAnchor(of: box) }
        guard let card, let index = nearestRow(card, path) else {
            return box.minY + box.height / 2
        }
        // The index is absolute — `indexByPath` runs over every row — so it has to
        // come through the window the card is currently showing. A row scrolled out
        // of sight anchors to the near edge, which keeps the line pointing at the
        // card and says which way its row went.
        let content = card.content
        // Out of the window: the strip standing in for that direction, not the row
        // now sitting at that end. Landing on the first drawn row is indistinguishable
        // from pointing at it: a line to a scrolled-away field would look like
        // `ref_0` naming `field_17`.
        if index < content.first { return box.minY + aboveStripOffset(content) }
        if index >= content.first + content.rows.count {
            return box.minY + belowStripOffset(content)
        }
        return box.minY + rowOffset(index - content.first, in: content)
    }

    /// A cubic from the source row to the target card, routed around whichever way
    /// the two cards actually sit.
    struct Wire {
        var start: CGPoint
        var control1: CGPoint
        var control2: CGPoint
        var end: CGPoint
        /// Where the cardinality label sits — the cubic's midpoint, lifted clear.
        var label: CGPoint
        /// A line routed round the cards (`LineRouter`): its corners from `start` to `end`.
        /// Empty for the curve. Its marks point along its first and last runs (`control1` and
        /// `control2` are the corners next to its ends) and its label sits at its middle.
        var corners: [CGPoint] = []

        /// A line routed round the cards through `points`, its ends the first and the last.
        static func routed(_ points: [CGPoint]) -> Wire {
            let middle = LineRouter.point(points, at: 0.5)
            return Wire(start: points[0], control1: points[1], control2: points[points.count - 2],
                        end: points[points.count - 1], label: CGPoint(x: middle.x, y: middle.y - 5), corners: points)
        }

        /// The path it is drawn as.
        var path: CGPath {
            if !corners.isEmpty { return LineRouter.path(corners) }
            let path = CGMutablePath()
            path.move(to: start)
            path.addCurve(to: end, control1: control1, control2: control2)
            return path
        }

        /// The point `t` of the way along it (0 its start, 1 its end).
        func point(at t: CGFloat) -> CGPoint {
            if !corners.isEmpty { return LineRouter.point(corners, at: t) }
            let u = 1 - t
            let a = u * u * u, b = 3 * u * u * t, c = 3 * u * t * t, d = t * t * t
            return CGPoint(x: a * start.x + b * control1.x + c * control2.x + d * end.x,
                           y: a * start.y + b * control1.y + c * control2.y + d * end.y)
        }
    }

    /// Which way a line runs between two cards.
    enum Side { case right, left, below, above }

    static func side(from a: CGRect, to b: CGRect) -> Side {
        if b.minX >= a.maxX + 8 { return .right }
        if b.maxX <= a.minX - 8 { return .left }
        return b.minY >= a.maxY ? .below : .above
    }

    /// A line that names no row lands on the header, which is the one part of a card
    /// that means the whole of it — beside `updated_at` it would read as pointing at
    /// `updated_at`. `landing` overrides that for the lines which do name a row.
    ///
    /// Several lines arriving at one header are not spread apart within the header
    /// band. A declared path lands on the row it names, so the ones sharing a header
    /// are the ones that genuinely point at the same card, and nudging them apart
    /// would say they arrived at slightly different places when they did not.
    static func wire(from a: CGRect, to b: CGRect, tailY: CGFloat,
                     landing: CGFloat? = nil) -> Wire {
        let ar = a.maxX, ab = a.maxY, acx = a.midX
        let br = b.maxX, bcy = b.midY, bcx = b.midX

        var p0, p1, p2, p3: CGPoint

        // Always the header, entered from whichever side the line comes from.
        let land = landing ?? headerAnchor(of: b)
        switch side(from: a, to: b) {
        case .right:
            let dx = max(46, (b.minX - ar) / 1.6)
            p0 = CGPoint(x: ar, y: tailY)
            p1 = CGPoint(x: ar + dx, y: tailY)
            p2 = CGPoint(x: b.minX - dx, y: land)
            p3 = CGPoint(x: b.minX, y: land)
        case .left:
            let dx = max(46, (a.minX - br) / 1.6)
            p0 = CGPoint(x: a.minX, y: tailY)
            p1 = CGPoint(x: a.minX - dx, y: tailY)
            p2 = CGPoint(x: br + dx, y: land)
            p3 = CGPoint(x: br, y: land)
        case .below, .above:
            // Out of the source at its row, along the outside of the target, and in
            // at the header. The detour is the point: the line has to arrive
            // somewhere that means "this card" rather than "this row of it", and the
            // underside of a card is its last field.
            let right = bcx >= acx
            let exit = CGPoint(x: right ? ar : a.minX, y: tailY)
            let entry = CGPoint(x: right ? b.minX : br, y: land)
            let reach = max(40, abs(entry.x - exit.x) / 2)
            p0 = exit
            p1 = CGPoint(x: exit.x + (right ? reach : -reach), y: tailY)
            p2 = CGPoint(x: entry.x - (right ? reach : -reach), y: land)
            p3 = entry
        }

        // Cubic midpoint: (P0 + 3P1 + 3P2 + P3) / 8
        let label = CGPoint(
            x: (p0.x + 3 * p1.x + 3 * p2.x + p3.x) / 8,
            y: (p0.y + 3 * p1.y + 3 * p2.y + p3.y) / 8 - 5)

        return Wire(start: p0, control1: p1, control2: p2, end: p3, label: label)
    }

    /// The middle of a card's header, in the same space as the boxes.
    static func headerAnchor(of box: CGRect) -> CGFloat {
        box.minY + CardGeometry.cardBorder + CardGeometry.headerHeight / 2
    }

    /// Content bounds. Cards may sit at negative coordinates — there is no origin
    /// corner the diagram has to stay inside.
    static func bounds(_ boxes: [String: CGRect]) -> CGRect {
        guard !boxes.isEmpty else { return CGRect(x: 0, y: 0, width: 640, height: 400) }
        var union = CGRect.null
        for box in boxes.values { union = union.union(box) }
        // Room for edge labels and the card shadow.
        return union.insetBy(dx: -64, dy: -64)
    }
}
