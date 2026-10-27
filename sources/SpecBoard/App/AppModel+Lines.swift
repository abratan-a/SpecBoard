import CoreGraphics
import Foundation

// Lines drawn by hand and picked (overview §13 「つなぐ」「線を選ぶ」). Which kinds
// of line a place may draw to another is the core's to say (`card_read`'s `as: lineKinds`): the
// window asks it and writes what was chosen into the card's `connections`, as MCP would.

/// One of the board's cards still to do that reaches a card of main's (`card_read`'s `fromBoard`):
/// where it is on the board, its kind, its label, how far it has come — the field its kind says it
/// by (a goal's `status`), when that says more than not done — and where in main's card it lands
/// (a row's names).
struct BoardReach: Hashable, Identifiable {
    var path: String
    var kind: String
    var label: String
    var state: String?
    /// The field `state` is the value of, to show it as its kind's definition says it.
    var stateField: String?
    var at: String?

    var id: String { path }

    init?(_ read: [String: Any]) {
        guard let path = read["path"] as? String, let kind = read["kind"] as? String else { return nil }
        self.path = path
        self.kind = kind
        label = read["label"] as? String ?? ""
        at = read["at"] as? String
        let known: Set<String> = ["path", "kind", "label", "at"]
        let said = read.first { !known.contains($0.key) && $0.value is String }
        state = said?.value as? String
        stateField = said?.key
    }
}

/// A line of the project, by the card that holds it — a card, or a flow's node card — and its
/// id in that card's `connections`.
struct LineRef: Hashable, Identifiable {
    var card: String
    var id: String
}

/// Where a line is drawn from: the card that holds it (a flow's node its own card, a card
/// ghost's row the card it leads to), as the tools name it (`path`), and its place there
/// (`from`: a field's names, a formatdoc's item, a process node's row `steps[0]`; empty for
/// its head).
struct LineSource: Hashable {
    var card: String
    var path: String
    var from: String = ""

    /// As a person reads it: the card and the place in it — a process node's row by its
    /// number, as the property panel counts them.
    var said: String {
        if from.isEmpty { return path }
        if let index = GraphEdges.stepIndex(from) { return L10n[.Lines.sourceRow(index + 1, path)] }
        return "\(path).\(from)"
    }
}

/// What a place may draw to another, as the core says it (`card_read`'s `as: lineKinds`): the
/// card or card ghost the line reaches and where it lands there — as `connections` writes
/// them — the kinds of line it may draw, by their definitions' words, and those it has as
/// many of as its kind lets it (`full`).
struct DrawableLines: Decodable, Equatable {
    struct Kind: Decodable, Equatable, Identifiable {
        var line: String
        var label: String
        var description: String?
        var base: String?
        var max: Int?
        var count: Int?
        /// The line's fields it cannot be written without.
        var required: [String]?

        var id: String { line }
    }

    var to: String?
    var at: String?
    var lines: [Kind]
    var full: [Kind]

    private enum CodingKeys: String, CodingKey { case to, at, lines, full }

    init(to: String? = nil, at: String? = nil, lines: [Kind] = [], full: [Kind] = []) {
        self.to = to
        self.at = at
        self.lines = lines
        self.full = full
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        to = try c.decodeIfPresent(String.self, forKey: .to)
        at = try c.decodeIfPresent(String.self, forKey: .at)
        lines = try c.decodeIfPresent([Kind].self, forKey: .lines) ?? []
        full = try c.decodeIfPresent([Kind].self, forKey: .full) ?? []
    }
}

/// A line being drawn by hand (つなぐ): from where, to what once it is picked (dropped on, or
/// chosen in the selector), and where on the diagram it was asked — in the canvas's own
/// points, where つなぐ is shown (`ConnectAnchor`); none from the property panel, which shows
/// it beside its button. Every way in comes to the same choice of kinds (`ConnectView`).
struct Connecting: Equatable, Identifiable {
    var id = UUID()
    var source: LineSource
    var target: String? = nil
    var anchor: CGPoint? = nil
}

extension AppModel {
    /// The card that holds a line: a card of the project or a flow's node card — its id, how the
    /// tools name it, its kind, and its lines.
    struct LineHolder {
        var id: String
        var path: String
        var kind: String
        var connections: [Connection]
    }

    /// A card or a flow's node card that holds lines, by its id.
    func lineHolder(_ id: String) -> LineHolder? {
        guard let detail else { return nil }
        let directory = detail.directory
        if let card = schema(id) {
            return LineHolder(id: id, path: directory.full(card), kind: card.kindName, connections: card.connections)
        }
        if let node = detail.inner.first(where: { $0.id == id }), let flow = schema(node.container) {
            return LineHolder(id: id, path: directory.full(flow, [node.name]), kind: node.kind,
                              connections: node.connections)
        }
        return nil
    }

    /// A line of the project, with the card that holds it; nil when it is gone.
    func line(_ ref: LineRef) -> (holder: LineHolder, line: Connection)? {
        guard let holder = lineHolder(ref.card), let line = holder.connections.first(where: { $0.id == ref.id })
        else { return nil }
        return (holder, line)
    }

    /// How the code a line of a card reaches stands, when it is to be looked at: changed since it was
    /// recorded, or never recorded (`codeReview`). None for a line that is not, and for one of a flow's node.
    func codeReview(of ref: LineRef) -> CodeReviewLine? {
        schema(ref.card)?.codeReview.first { $0.id == ref.id }
    }

    /// The line a drawn relation is (`Relation.connection`): found in the card that holds it,
    /// a flow's node card too. None for what a text names (`mentions`).
    func lineRef(connection id: String) -> LineRef? {
        guard let detail else { return nil }
        if let card = detail.schemas.first(where: { $0.connections.contains { $0.id == id } }) {
            return LineRef(card: card.id, id: id)
        }
        if let node = detail.inner.first(where: { $0.connections.contains { $0.id == id } }) {
            return LineRef(card: node.id, id: id)
        }
        return nil
    }

    /// The lines leaving a place, in the order its card holds them.
    func lines(leaving source: LineSource) -> [LineRef] {
        (lineHolder(source.card)?.connections ?? [])
            .filter { $0.from == source.from }
            .map { LineRef(card: source.card, id: $0.id) }
    }

    /// The lines leaving a card's fields and rows — not its head — by where each leaves, in
    /// the order they are written: what a card picked shows under its head's own in つながり.
    func linesInside(_ card: String) -> [(from: String, refs: [LineRef])] {
        var out: [(from: String, refs: [LineRef])] = []
        for line in lineHolder(card)?.connections ?? [] where !line.from.isEmpty {
            let ref = LineRef(card: card, id: line.id)
            if let at = out.firstIndex(where: { $0.from == line.from }) {
                out[at].refs.append(ref)
            } else {
                out.append((line.from, [ref]))
            }
        }
        return out
    }

    /// The lines that reach a card from elsewhere: written by the cards they
    /// leave, so each is read from the project's lines (`relations`), one a line. With `field`, those
    /// landing on that field of it — said so (`at`), or an ID pointing at the card with no row
    /// said, when that field is its one primary key (`GraphEdges.primaryKey`). A text naming the
    /// card is not a line of its own.
    func linesArriving(at card: String, field: String? = nil) -> [Relation] {
        guard let detail else { return [] }
        let key = schema(card).flatMap(GraphEdges.primaryKey(of:))
        var seen: Set<String> = []
        return detail.relations.filter { relation in
            guard relation.toSchemaId == card, relation.fromSchemaId != card, let id = relation.connection,
                  seen.insert(relation.fromSchemaId + "|" + id).inserted else { return false }
            guard let field else { return true }
            let at = relation.at ?? ""
            return at == field || (at.isEmpty && relation.line == Line.points && key == field)
        }
    }

    /// Where a line arriving here leaves, as a person reads it: its card in full, and the field or
    /// row it leaves from.
    func leaving(_ relation: Relation) -> String {
        let head = linePath(to: relation.fromSchemaId) ?? L10n[.Lines.sourceDeletedCard]
        return relation.from.isEmpty ? head : "\(head).\(relation.from)"
    }

    /// Where a line leaves, as つなぐ starts from it.
    func source(of ref: LineRef) -> LineSource? {
        guard let (holder, line) = line(ref) else { return nil }
        return LineSource(card: holder.id, path: holder.path, from: line.from)
    }

    /// Where a line reaches, as a path is written: its card or card ghost in full and where it
    /// lands there — or, landing outside it (a full path in `at`), that place. Nil when its
    /// card is gone.
    func landing(of line: Connection) -> String? {
        if let at = line.at, PathText.hasSection(at) { return at }
        guard let to = line.to, let head = linePath(to: to) else { return nil }
        guard let at = line.at, !at.isEmpty else { return head }
        return "\(head).\(at)"
    }

    /// A card, a card ghost, a flow's node card or a working directory's file a line reaches, in full.
    func linePath(to id: String) -> String? {
        guard let detail else { return nil }
        let directory = detail.directory
        if let card = directory.schema(id: id) { return directory.full(card) }
        if let ghost = directory.ghost(id: id) { return directory.full(ghost) }
        if let node = detail.inner.first(where: { $0.id == id }), let flow = schema(node.container) {
            return directory.full(flow, [node.name])
        }
        // A file the core holds the line to but this window has no card of: its path all the same, as MCP says it.
        if let file = directory.full(fileId: id) { return file }
        // main's card a board's line reaches: in full there — and main's file the same way.
        if let main = outsideDetail {
            if let card = main.directory.schema(id: id) { return main.directory.full(card) }
            if let file = main.directory.full(fileId: id) { return file }
        }
        return nil
    }

    /// What `source` may draw to `target` (a path: a card, a field, a row, a node, a card
    /// ghost), as the core says it (`card_read`'s `as: lineKinds`) — the one rule of which kinds
    /// of line go where, MCP's too.
    func drawable(from source: LineSource, to target: String) throws -> DrawableLines {
        var value: [String: Any] = ["as": "lineKinds", "to": target]
        if !source.from.isEmpty, GraphEdges.stepIndex(source.from) != nil { value["from"] = source.from }
        let path = GraphEdges.stepIndex(source.from) == nil && !source.from.isEmpty
            ? "\(source.path).\(source.from)" : source.path
        let answer = try tool("card_read", target: path, value: value)
        guard case .object(let wrapped) = answer, let json = wrapped["json"] else {
            throw CoreClient.Refusal(L10n[.Common.problemCoreUnreadable])
        }
        return try JSONDecoder().decode(DrawableLines.self, from: Data(json.compact().utf8))
    }

    /// A line drawn (つなぐ): of the kind `line`, from `source` to `to` (a card or card ghost,
    /// as the core said it), landing at `at`, with its fields — added after the lines its
    /// card has, in one write. Picked once it is there, つなぐ done.
    func connect(_ source: LineSource, line: String, to: String, at: String?, fields: [String: JSONValue] = [:]) {
        guard !isReadOnly, let holder = lineHolder(source.card) else { return }
        var made: [String: Any] = ["line": line, "to": to]
        // An element of a data card's array: kept by the fields its shape keys it by.
        let lookup: SchemaLookup = { [weak self] in self?.detail?.directory.schema(id: $0) }
        let from = schema(source.card).map { InstanceRows.keyed($0, source.from, lookup: lookup) } ?? source.from
        if !from.isEmpty { made["from"] = from }
        if let at, !at.isEmpty {
            let reached = detail?.directory.schema(to, from: nil)
            made["at"] = reached.map { InstanceRows.keyed($0, at, lookup: lookup) } ?? at
        }
        if !fields.isEmpty { made["fields"] = fields.mapValues { OJSON($0).foundation } }
        let before = Set(holder.connections.map(\.id))
        write {
            try tool("card_update", target: holder.path,
                     value: ["connections": holder.connections.map(wire) + [made]])
        } then: { [weak self] in
            guard let self, let now = self.lineHolder(source.card),
                  let new = now.connections.first(where: { !before.contains($0.id) && $0.line == line && $0.from == from })
            else { return }
            self.selectLine(LineRef(card: source.card, id: new.id))
        }
        guard lastError == nil else { return }
        connecting = nil
    }

    /// A line taken off its card. An embedding's field goes back to `string` (the core's; the
    /// window says so before, `LineRemoval`).
    func removeLine(_ ref: LineRef) {
        guard !isReadOnly, let (holder, _) = line(ref) else { return }
        if selectedLine == ref { deselect() }
        write {
            try tool("card_update", target: holder.path,
                     value: ["connections": holder.connections.filter { $0.id != ref.id }.map(wire)])
        }
    }

    /// One line's own description: what the written-out notes say of it.
    func setLineDescription(_ ref: LineRef, _ text: String) {
        let said = text.trimmingCharacters(in: .whitespacesAndNewlines)
        changeLine(ref) { $0.description = said.isEmpty ? nil : said }
    }

    /// One of a line's fields (its kind's `fields`): set, or taken off with nil.
    func setLineField(_ ref: LineRef, _ name: String, _ value: JSONValue?) {
        changeLine(ref) { line in
            if let value { line.fields[name] = value } else { line.fields.removeValue(forKey: name) }
        }
    }

    /// A line of a card changed, the others as they are: the line keeps its id (its kind, its
    /// place, its card and where it lands are the same).
    private func changeLine(_ ref: LineRef, _ change: (inout Connection) -> Void) {
        guard !isReadOnly, let (holder, _) = line(ref) else { return }
        let lines = holder.connections.map { line -> [String: Any] in
            var next = line
            if next.id == ref.id { change(&next) }
            return wire(next)
        }
        write { try tool("card_update", target: holder.path, value: ["connections": lines]) }
    }

    /// Where a line leaves, picked: its card's head, the field, the formatdoc's item, the
    /// flow's node or the process node's row.
    func pickSource(of ref: LineRef) {
        guard let detail, let (holder, line) = line(ref) else { return }
        if let node = detail.inner.first(where: { $0.id == holder.id }) {
            if let index = GraphEdges.stepIndex(line.from) {
                selectRow(node.container, .nodeStep(node.name, StepEditing.path([.step(index)])))
            } else {
                selectRow(node.container, .node(node.name))
            }
            return
        }
        guard let card = schema(holder.id) else { return }
        if line.from.isEmpty { selectSchema(card.id, follow: false); return }
        let names = PathText.names(line.from)
        var found: FieldNode?
        walkFields(card.fields) { field, path, _ in
            if found == nil, path == names { found = field }
        }
        if let found {
            selectField(card.id, found.id, follow: false)
        } else if card.kind == .formatdoc {
            selectRow(card.id, .formatRow(line.from))
        } else {
            selectSchema(card.id, follow: false)
        }
    }

    /// Where a line leaves, shown (the line's 「from」 in the property panel): its
    /// card on the diagram it is drawn on, brought into view and picked as a press on the nav panel
    /// does it — then the field, item, node or row it leaves picked. The card's editor opens on a
    /// press twice, on the nav panel or on the diagram, not here.
    func showSource(of ref: LineRef) {
        guard let detail, let (holder, _) = line(ref) else { return }
        let inner = detail.inner.contains { $0.id == holder.id }
        showCard(holder.id)
        // A card inside a container is its container's row, picked as that row (`perform`).
        if !inner, schema(holder.id)?.container != nil { return }
        pickSource(of: ref)
    }

    /// A card shown where it is drawn — on its section's diagram, brought into view and picked, a
    /// node on its flow's row, a card inside a goal on the goal's — as a press on the nav panel
    /// shows it: what a press on a card's name does in the property panel, on
    /// the board's goals, on the dashboard and on the export screen. Its editor opens on a press
    /// twice, on the nav panel or on the diagram.
    func showCard(_ id: String) {
        guard let detail else { return }
        let node: NavPanelTree.Node = detail.inner.contains { $0.id == id } ? .node(id) : .card(id)
        perform(NavPanelTree.press(node, twice: false, in: detail))
    }

    /// A place — a card, or a field, item, node or row in it — shown as `showCard` shows a card
    /// (the line's 「to」 and 「at」, the board's goals, the dashboard), what is in it picked.
    /// False when the path leads nowhere.
    @discardableResult
    func show(path: String, from base: String? = nil) -> Bool {
        guard let detail else { return false }
        func shown(_ card: SchemaNode) { showCard(card.id) }
        switch try? MCPViews.locate(detail, path, base: base) {
        case .schema(let card)?:
            shown(card)
        case .field(let card, let field, _)?:
            shown(card)
            selectField(card.id, field.id, follow: false)
        case .formatRow(let note, let names)?:
            shown(note)
            selectRow(note.id, .formatRow(PathText.joinNames(names)))
        case .node(let flow, let node)?:
            perform(.show(card: flow.id, section: detail.directory.sectionId(of: flow), focus: flow.id,
                          row: SchemaNode.nodeRow(node.name)))
            selectNode(flow.id, node.name)
        case .customRow(let card, let names)?:
            shown(card)
            selectRow(card.id, .customRow(CardWalk.rowPath(names, in: card)))
        case nil:
            // A place of the tree beside this one (main's, from the board): shown there.
            if inMain(path) {
                openBranch(nil)
                return show(path: path)
            }
            // Through a card ghost: the ghost, where it is.
            let directory = detail.directory
            guard let ghost = directory.ghost(path, from: base) else { return false }
            perform(.show(card: ghost.id, section: directory.sectionId(of: ghost), focus: ghost.id, row: nil))
        }
        return true
    }

    /// The board's cards still to do that reach what `source` is — a card of main's or a branch's,
    /// or a field or row in it — as the core reads them (`card_read`'s `fromBoard`, the property
    /// panel's 関わるやること). None on the board, and none where the core cannot say. Read once for what
    /// is asked until the whole project is read again (`aside`): the core reads the whole board for it.
    func boardReach(of source: LineSource) -> [BoardReach] {
        guard !isBoardOpen, let projectId = detail?.project.id else { return [] }
        let picks = PathText.dots(source.from).contains { !PathText.step($0).rest.isEmpty }
        let path = source.from.isEmpty || picks ? source.path : "\(source.path).\(source.from)"
        let key = "reach|\(projectId)|\(openTrees[projectId] ?? "")|\(path)"
        return aside(key, "card_read", target: path, value: ["include": ["from_board"]]) { json in
            (json?["fromBoard"] as? [[String: Any]] ?? []).compactMap(BoardReach.init)
        }
    }

    /// What a tool answers beside the project for the property panel, as `read` reads its JSON: kept by
    /// `key` until the whole project is read again. Asked off the main thread when the window reads so
    /// (`readsOffMain`) — on a queue of its own, as asking blocks the thread until the core answers;
    /// drawn again once it is in, and meanwhile what was kept for it before, or nothing — and at once
    /// otherwise.
    ///
    /// `paged`: a list read to its end — asked again from where its `next` says until it has none, the
    /// `cards` of every page together.
    func aside<T>(_ key: String, _ name: String, target: String? = nil, value: [String: Any]?, paged: Bool = false,
                          _ read: @escaping ([String: Any]?) -> T) -> T {
        _ = asideReads
        if let kept = asideKept[key] as? T { return kept }
        guard let core, let projectId = detail?.project.id else { return read(nil) }
        var arguments = openTreeArgument
        arguments["project"] = projectId
        if let target { arguments["target"] = target }
        if let value { arguments["value"] = value }
        let request: [String: Any] = ["name": name, "arguments": arguments]
        guard readsOffMain else {
            asideSent += 1
            let found = read(Self.ask(core, request, paged: paged))
            asideKept[key] = found
            return found
        }
        if asideAsked.insert(key).inserted {
            asideSent += 1
            let sent = Handed(value: request)
            let back = Handed(value: (model: WeakModel(self), read: read))
            DispatchQueue.global(qos: .utility).async {
                let json = Handed(value: Self.ask(core, sent.value, paged: paged))
                DispatchQueue.main.async {
                    MainActor.assumeIsolated {
                        guard let self = back.value.model.model, self.asideAsked.remove(key) != nil else { return }
                        self.asideKept[key] = back.value.read(json.value)
                        self.asideReads += 1
                    }
                }
            }
        }
        return asideBefore[key] as? T ?? read(nil)
    }

    /// A tool's answer, or a list's every page's (`aside`'s `paged`). Asks the core, so it is not asked on the
    /// main thread when reads are kept off it.
    nonisolated private static func ask(_ core: CoreClient, _ request: [String: Any], paged: Bool) -> [String: Any]? {
        guard paged else { return toolJSON(try? core.reply("tool", request)) }
        var cards: [Any] = []
        var after: String?
        var request = request
        while true {
            var arguments = request["arguments"] as? [String: Any] ?? [:]
            var value = arguments["value"] as? [String: Any] ?? [:]
            if let after { value["after"] = after }
            arguments["value"] = value
            request["arguments"] = arguments
            guard let page = toolJSON(try? core.reply("tool", request)) else { return nil }
            cards += page["cards"] as? [Any] ?? []
            guard let next = page["next"] as? String else {
                var whole = page
                whole["cards"] = cards
                return whole
            }
            if next == after { return nil }
            after = next
        }
    }

    /// A tool's answer as the core wrote it (`reply`): the JSON it answers with, when it is an object.
    nonisolated private static func toolJSON(_ reply: String?) -> [String: Any]? {
        guard let reply, let answer = try? JSONSerialization.jsonObject(with: Data(reply.utf8)) as? [String: Any],
              let result = answer["result"] as? [String: Any] else { return nil }
        return result["json"] as? [String: Any]
    }

    /// One of main's cards (or a place in it), shown on main's diagram — main opened for it.
    func showInMainTree(_ path: String) {
        openBranch(nil)
        if !show(path: path) { openRootOfOpenTree() }
    }

    /// One of the board's cards, shown where the board draws it — the board opened for it.
    func showOnBoard(_ path: String) {
        openBranch(Board.id)
        if !show(path: path) { openRootOfOpenTree() }
    }

    /// `detail` with main's beside it while the board is open (`ProjectDetail.joined`): what a board's
    /// card's connections diagram follows its lines across.
    func withOutside(_ detail: ProjectDetail) -> ProjectDetail {
        outsideDetail.map { detail.joined(with: $0) } ?? detail
    }

    /// How the board and main beside it are called where both are offered (つなぐ).
    var outsideWords: (own: String, outside: String) { (Board.title, SchemaDirectory.mainWord) }

    /// Whether `path`, written on the board, names a place of main's: written in full, found in
    /// main's detail. The names directly under the roots are one each across both trees, so it
    /// is not the board's.
    func inMain(_ path: String) -> Bool {
        guard let main = outsideDetail, PathText.hasSection(path) else { return false }
        return (try? MCPViews.locate(main, path, base: nil)) != nil
    }

    /// A press on one of main's section ghosts on the board's diagram — a row, standing for main's
    /// card or what in it a line reaches; with `header`, its header too — shown in main: main
    /// opened, the card on its diagram, or the section's. False for anything else, left to the
    /// press as it is.
    func showOutside(_ hit: CardCanvasView.RowHit, header: Bool) -> Bool {
        guard SectionGhosts.isGhost(hit.card), let main = outsideDetail else { return false }
        if let field = hit.row?.field, let target = SectionGhosts.target(of: field) {
            guard main.directory.schema(id: target.schema) != nil else { return false }
            showInMain(target.schema, inside: target.field)
            return true
        }
        let section = String(hit.card.dropFirst(SectionGhosts.prefix.count))
        guard header, hit.row == nil, main.sections.contains(where: { $0.id == section }) else { return false }
        openBranch(nil)
        open(.section(section))
        return true
    }

    /// main's card a board's card reaches, shown on main's diagram as `showCard` shows one — main
    /// opened for it — with what in it is reached picked: `inside`, a field by its id or the names
    /// of a row in it.
    func showInMain(_ id: String, inside: String? = nil) {
        guard let main = outsideDetail, let card = main.directory.schema(id: id) else { return }
        var path = main.directory.full(card)
        if let inside, !inside.isEmpty {
            if let found = locateField(main.schemas, id: inside) {
                path = main.directory.full(found.schema, found.path)
            } else {
                path += "." + inside
            }
        }
        openBranch(nil)
        if !show(path: path) { showCard(id) }
    }
}

extension AppModel {
    /// Where a line drawn from what is picked in the property panel leaves: a card's head, a
    /// formatdoc's item, a flow's node or a process node's row (its node's own card), a goal's
    /// row (the card it is). None for a row with no place of its own (an enum's value, a
    /// store's hold).
    func lineSource(of card: SchemaNode, row: CardRow?) -> LineSource? {
        guard let detail else { return nil }
        let directory = detail.directory
        guard let row else { return LineSource(card: card.id, path: directory.full(card)) }
        switch row {
        case .node(let name), .nodeStep(let name, _):
            guard let node = detail.inner.first(where: { $0.container == card.id && $0.name == name }) else { return nil }
            var from = ""
            if case .nodeStep(_, let path) = row, case .step(let index)? = StepEditing.parts(path)?.first {
                from = "steps[\(index)]"
            }
            return LineSource(card: node.id, path: directory.full(card, [name]), from: from)
        case .formatRow(let path):
            return LineSource(card: card.id, path: directory.full(card), from: path)
        // A row of a card of a kind the project defines leaves from its path in the card (`steps.a`).
        case .customRow(let path) where card.kind == .custom:
            return LineSource(card: card.id, path: directory.full(card), from: path)
        case .customRow(let path):
            var holder = card
            for name in PathText.names(path) {
                guard let next = detail.schemas.first(where: { $0.container == holder.id && $0.name == name }) else { return nil }
                holder = next
            }
            return LineSource(card: holder.id, path: directory.full(holder))
        // Nothing leaves a function or type of a file: lines only land there.
        case .value, .hold, .symbol:
            return nil
        }
    }

    /// Where a line drawn from a field leaves: the card that has it, and its names there.
    func lineSource(of field: FieldLocation) -> LineSource? {
        guard let detail else { return nil }
        return LineSource(card: field.schema.id, path: detail.directory.full(field.schema),
                          from: PathText.joinNames(field.path))
    }
}
