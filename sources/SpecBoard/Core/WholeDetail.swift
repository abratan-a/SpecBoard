import Foundation

/// A project as the core's reading answers it (`project_detail`, `specboard://SpecBoard/Docs/Design:CoreApi`「読む」):
/// its sections, every card's every part — its lines as its document keeps them
/// (`connections`) — the cards put only inside a container (`inner`: a flow's nodes), every
/// line of the project (`relations`), and the kinds of line it has (`kinds`). A part that is
/// absent (nil) is left out; the rest is written, empty or not.
///
/// Read here into what the window draws from: a field's embedding read off its card's embed
/// line (`FieldNode.embeds`), a flow's nodes read off the cards inside it (`FlowNode`), a
/// goal's cards inside it as its rows (`customRows`). Nothing is made of a line here: the
/// lines are the core's.
nonisolated struct WholeDetail: Decodable {
    var project: Project
    var cards: [WholeCard]
    var relations: [Relation]
    var sections: [ProjectSection]
    var context: String?
    var ghosts: [CardGhost]
    var inner: [InnerCard]
    var lineKinds: [LineKind]
    var cardKinds: [CardKindDef]
    /// Where the working directories' files were moved on their sections' maps in this tree, by the file's
    /// id (`filePlaces`): the files themselves are not in the tree's reading (`WorkdirDetail`).
    var filePlaces: [String: FilePlace]

    enum Keys: String, CodingKey { case project, schemas, relations, sections, context, ghosts, inner, kinds, filePlaces, hashes, digest }

    /// What each kind of section holds, by its name (`kinds.sections`).
    var sectionHolds: [String: [String]]
    /// The marks of the kinds of section, by the kind's name (`kinds.sections`): the built-in ones'
    /// (as the project writes them over) and the project's own.
    var sectionMarks: [String: SectionMark]
    /// What a flow's `via` may be (`kinds.vias`): the built-in values, then the project's own.
    var vias: [String]
    /// Each entry's hash as the core worked it out, by its key (`schemas/<id>`, `relations/<id>`, `project`,
    /// `order/schemas`…): what the next reading's changes are put on with (`DetailDelta`). Empty from a core
    /// that sends none.
    var hashes: [String: String]
    /// The digest of `hashes` as the core sent it: what the next reading of the tree is asked from (`since`).
    var digest: String?

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        project = try c.decode(Project.self, forKey: .project)
        cards = try c.decode([WholeCard].self, forKey: .schemas)
        relations = try c.decode([Relation].self, forKey: .relations)
        // The card ghosts, apart from the cards (`specboard://SpecBoard/Docs/Design:CoreApi`「読む」).
        ghosts = try c.decodeIfPresent([CardGhost].self, forKey: .ghosts) ?? []
        inner = try c.decodeIfPresent([InnerCard].self, forKey: .inner) ?? []
        sections = try c.decodeIfPresent([WholeSection].self, forKey: .sections)?.map(\.section) ?? []
        context = try c.decodeIfPresent(String.self, forKey: .context)
        let kinds = try c.decodeIfPresent(WholeKinds.self, forKey: .kinds) ?? WholeKinds()
        lineKinds = kinds.lines
        cardKinds = kinds.cards
        sectionHolds = kinds.holds
        sectionMarks = kinds.marks
        vias = kinds.vias
        filePlaces = try c.decodeIfPresent([String: FilePlace].self, forKey: .filePlaces) ?? [:]
        hashes = try c.decodeIfPresent([String: String].self, forKey: .hashes) ?? [:]
        digest = try c.decodeIfPresent(String.self, forKey: .digest)
    }

    /// The kinds read anew (a reading's `kinds`) put in.
    mutating func take(_ kinds: WholeKinds) {
        lineKinds = kinds.lines
        cardKinds = kinds.cards
        sectionHolds = kinds.holds
        sectionMarks = kinds.marks
        vias = kinds.vias
    }

    /// The project as the window draws from it: the cards read with the paths their lines go to
    /// (`WholeCard.shown`). On any thread: a big project is read off the main thread.
    var detail: ProjectDetail {
        let kinds = CardKinds(cardKinds)
        var detail = ProjectDetail(project: project, schemas: WholeCard.shown(cards, inner: inner, ghosts: ghosts, sections: sections,
                                                                     kinds: kinds),
                                relations: relations)
        detail.sections = sections
        detail.context = context
        detail.ghosts = ghosts
        detail.inner = inner
        detail.lineKinds = LineKinds(lineKinds)
        detail.cardKinds = kinds
        detail.sectionHolds = sectionHolds
        detail.sectionMarks = sectionMarks
        detail.vias = vias
        return detail
    }
}

extension WholeDetail {
    /// A project read from its JSON as the core writes it.
    nonisolated static func read(_ data: Data) throws -> ProjectDetail {
        try JSONDecoder().decode(WholeDetail.self, from: data).detail
    }
}

/// The kinds of a project as `project_detail` answers them (`kinds`), as far as the window reads them: the
/// kinds of line and of card, what each kind of section holds and its mark, and what a flow's `via` may be.
nonisolated struct WholeKinds: Decodable {
    var lines: [LineKind] = []
    var cards: [CardKindDef] = []
    /// What each kind of section holds, by its name.
    var holds: [String: [String]] = [:]
    /// The marks of the kinds of section, by the kind's name.
    var marks: [String: SectionMark] = [:]
    var vias: [String] = []

    private enum Keys: String, CodingKey { case lines, cards, sections, vias }

    /// A kind of section, as far as the window reads it: what it holds, and the mark and colour its
    /// sections carry in the nav panel (the built-in kinds' written over by the project, or the
    /// project's own).
    private struct SectionHolds: Decodable {
        var section: String
        var holds: [String]?
        var icon: String?
        var color: String?

        enum Keys: String, CodingKey { case section, holds, icon }

        /// `icon: {name, color}`. One written the way before (a string) reads as
        /// none: the core lets it go too.
        private struct Mark: Decodable {
            var name: String?
            var color: String?
        }

        init(from decoder: Decoder) throws {
            let c = try decoder.container(keyedBy: Keys.self)
            section = try c.decode(String.self, forKey: .section)
            holds = try? c.decodeIfPresent([String].self, forKey: .holds)
            let mark = (try? c.decodeIfPresent(Mark.self, forKey: .icon)) ?? nil
            icon = mark?.name
            color = mark?.color
        }
    }

    init() {}

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        lines = try c.decodeIfPresent([LineKind].self, forKey: .lines) ?? []
        cards = try c.decodeIfPresent([CardKindDef].self, forKey: .cards) ?? []
        let sections = try c.decodeIfPresent([SectionHolds].self, forKey: .sections) ?? []
        holds = Dictionary(sections.map { ($0.section, $0.holds ?? []) }, uniquingKeysWith: { a, _ in a })
        marks = Dictionary(sections.compactMap { def in
            def.icon == nil && def.color == nil ? nil : (def.section, SectionMark(icon: def.icon, color: def.color))
        }, uniquingKeysWith: { a, _ in a })
        vias = try c.decodeIfPresent([String].self, forKey: .vias) ?? []
    }
}

nonisolated struct WholeCard: Decodable {
    var card: SchemaNode
    /// A card of a kind written by its definition (goal, task): its fields as the
    /// document keeps them (`form.fields`). Not written back: the window writes through
    /// the tools.
    var form: [String: JSONValue] = [:]
    /// Its rows as the document keeps them (`form.rows`): a card of a kind the project defines.
    var formRows: [String: JSONValue] = [:]

    enum Keys: String, CodingKey {
        case id, projectId, name, label, kind, description, tags, metadata, rules, absentFields,
             discriminator, enumOpen, posX, posY, pinned, sortOrder, fields, values, uses, signedBy,
             verifiedBy, next, body, rows, trigger, steps, cell, diagram,
             size, docView, version, sectionId, customFields, customRows, form, itemPlaces, standalone,
             connections, strayFields, faceRows, tableMarkdown, review, codeReview, container = "in"
    }

    private struct Form: Decodable {
        var fields: [String: JSONValue]?
        var rows: [String: JSONValue]?
    }

    /// The cards as the window shows them, read off what the core answers:
    ///
    /// - a field that embeds another card names it (`FieldNode.embeds`), from the card's embed
    ///   line from that field;
    /// - a flow's nodes are the cards inside it (`inner`), each read as the node it is
    ///   (`FlowNode.init(card:)`), the ways between them by name and every other place by its
    ///   path in full;
    /// - a container's cards inside it, in their order (`inside`), and a goal's fields as
    ///   its custom fields and the cards inside it as its rows — each a row with a check,
    ///   named as the card, joined to what the card's `link` lines reach.
    static func shown(_ cards: [WholeCard], inner: [InnerCard] = [], ghosts: [CardGhost] = [],
                      sections: [ProjectSection] = [], kinds: CardKinds = CardKinds()) -> [SchemaNode] {
        let byId = Dictionary(cards.map { ($0.card.id, $0) }, uniquingKeysWith: { a, _ in a })
        // Every card's, card ghost's and node's path in full, as a line's end is written.
        let directory = SchemaDirectory(cards.map(\.card), sections: sections, ghosts: ghosts)
        let flows = Dictionary(grouping: inner, by: \.container)
        func named(_ id: String) -> String? {
            if let card = directory.schema(id: id) { return directory.full(card) }
            if let ghost = directory.ghost(id: id) { return directory.full(ghost) }
            if let node = inner.first(where: { $0.id == id }), let flow = directory.schema(id: node.container) {
                return PathText.member(directory.full(flow), node.name)
            }
            return nil
        }
        func landing(_ line: Connection) -> String? {
            let at = line.at ?? ""
            if PathText.hasSection(at) { return at }
            guard let head = line.to.flatMap(named) else { return nil }
            return at.isEmpty ? head : "\(head).\(at)"
        }
        let inside = Dictionary(grouping: cards.filter { $0.card.container != nil }, by: { $0.card.container ?? "" })
        return cards.map { whole in
            var card = whole.card
            card.fields = embedding(card.fields, card.connections)
            if let nodes = flows[card.id] {
                let names = Dictionary(nodes.map { ($0.id, $0.name) }, uniquingKeysWith: { a, _ in a })
                card.nodes = nodes.map { FlowNode(card: $0, siblings: names, landing: landing) }
            }
            card.inside = (inside[card.id] ?? []).map(\.card.id)
            // A data card: its shape's name, as its header says it.
            if card.kind == .instance || card.kind == .table, let of = card.shapeOf {
                card.shapeName = directory.schema(id: of)?.name ?? directory.ghost(id: of)?.name
            }
            // A card of a kind the project defines: drawn and written by its definition, from its
            // fields and rows as kept.
            if card.kind == .custom {
                card.kindDef = card.ownKind.flatMap { kinds[$0] }
                card.ownFields = whole.form
                card.ownRows = whole.formRows
                // Its field body, where a note's is read and written: opened as a note
                // (`opens: markdown`) and written in place (`edit_in_place`).
                if case .string(let body)? = whole.form["body"] { card.body = body }
            }
            // A kind that says how its card is drawn (`face`; a task): drawn
            // by it, from its fields as kept.
            if let def = kinds[card.kindName], def.face != nil {
                card.faceKind = def
                card.faceValues = whole.form
                // A field holding a section keeps its id: written on the face as the section's path.
                for field in def.fields where field.fieldtype == KindField.sectionType {
                    if case .string(let id)? = whole.form[field.name], directory.section(id: id) != nil {
                        card.faceSections[field.name] = directory.full(section: id)
                    }
                }
            }
            // What the card shows in its own places (label, description, body) is read there.
            for (name, value) in whole.form where !Self.ownPlaces.contains(name) {
                if let text = Self.text(value) { card.customFields[name] = text }
            }
            card.definition = kinds[card.kindName]
            // A container's rows are the cards inside it, whether or not any of its fields is set:
            // each as its kind's `as_row` says it.
            if card.kind == .goal || card.definition?.isContainer == true {
                card.customRows = Self.rowsInside(card.inside, byId: byId, inside: inside, kinds: kinds, landing: landing)
            }
            return card
        }
    }

    /// A card's fields, each that its embed line leaves naming the card it reaches.
    private static func embedding(_ fields: [FieldNode], _ lines: [Connection], above: String = "") -> [FieldNode] {
        fields.map { field in
            var field = field
            let path = above.isEmpty ? PathText.quote(field.name) : PathText.member(above, field.name)
            field.embeds = lines.first { $0.line == Line.embed && $0.from == path }?.to
            field.children = embedding(field.children, lines, above: path)
            return field
        }
    }

    /// The rows of a container for the cards inside it, in their order (`as_row`): each its label (its
    /// name when it has none) — with its check where its kind says a field that means done, the other
    /// choices its `as_row` names under it, its other fields written after its text, and, where its
    /// kind unfolds it, what it holds (or its fields written) nested under it. Joined to what the
    /// card's `link` lines reach.
    private static func rowsInside(_ ids: [String], byId: [String: WholeCard], inside: [String: [WholeCard]],
                                   kinds: CardKinds, landing: (Connection) -> String?, depth: Int = 0) -> [CustomRow] {
        ids.compactMap { id in
            guard let held = byId[id] else { return nil }
            let def = kinds[held.card.kindName]
            let check = def?.rowCheck
            let links = held.card.connections.filter { $0.line == Line.link && $0.from.isEmpty }.compactMap(landing)
            var row = CustomRow(type: Self.heldCard, name: held.card.name, text: held.card.label,
                                done: check.map { field in held.form[field.name].map { field.done.contains($0) } ?? false } ?? false,
                                links: links)
            row.check = check != nil
            let named = (def?.asRow ?? []).compactMap { def?.field($0) }.filter { $0.name != check?.name && $0.name != "label" }
            row.choices = named.filter { $0.fieldtype == "radio" }.map { field in
                CustomRow.Choice(field: field.name, label: field.label, options: field.options,
                                 chosen: held.form[field.name]?.asText ?? field.defaultValue?.asText ?? "",
                                 labels: field.optionLabels)
            }
            row.more = named.filter { $0.fieldtype != "radio" }
                .compactMap { held.form[$0.name].map(OwnFieldText.short) }
                .filter { !$0.isEmpty }
                .joined(separator: " · ")
            if def?.unfolds == true, depth < CustomKind.deepest {
                let held = inside[held.card.id] ?? []
                row.rows = held.isEmpty
                    ? (def?.ownFields ?? []).compactMap { field in
                        guard let value = byId[id]?.form[field.name] else { return nil }
                        let text = OwnFieldText.short(value)
                        return text.isEmpty ? nil : CustomRow(type: Self.unfolded, name: field.name, text: "\(field.label): \(text)")
                    }
                    : rowsInside(held.map(\.card.id), byId: byId, inside: inside, kinds: kinds, landing: landing, depth: depth + 1)
                // Shown only: no check to press, nothing written back.
                for index in row.rows.indices {
                    row.rows[index].type = Self.unfolded
                    row.rows[index].check = false
                }
            }
            return row
        }
    }

    /// The type of a row a container draws by unfolding what is inside one of its cards: shown, not
    /// written back (`AppModel.setCustomRows` leaves it out). `WholeCard.unfolded`.
    static let unfolded = "#unfold"

    /// The row type of a card a container holds, as its rows of before named it.
    static let heldCard = "タスク"

    private static let ownPlaces: Set<String> = ["label", "description", "tags", "metadata", "body"]

    /// A field's value as the window's custom fields hold it; nil for a list or an object.
    private static func text(_ value: JSONValue) -> String? {
        switch value {
        case .string(let text): return text
        case .bool(let flag): return flag ? "true" : "false"
        case .number(let number):
            return number == number.rounded() && abs(number) < 1e15 ? String(Int(number)) : String(number)
        default: return nil
        }
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        func or<T: Decodable>(_ key: Keys, _ empty: T) throws -> T { try c.decodeIfPresent(T.self, forKey: key) ?? empty }
        // A kind this build does not have is one the project defines (`custom`), named as the project does.
        let kindName = try c.decode(String.self, forKey: .kind)
        var card = SchemaNode(id: try c.decode(String.self, forKey: .id),
                              projectId: try c.decode(String.self, forKey: .projectId),
                              name: try c.decode(String.self, forKey: .name),
                              label: try or(.label, ""),
                              kind: try c.decode(SchemaKind.self, forKey: .kind),
                              description: try or(.description, ""),
                              tags: try or(.tags, []),
                              metadata: try or(.metadata, [:]),
                              rules: try or(.rules, []),
                              absentFields: try or(.absentFields, []),
                              discriminator: try c.decodeIfPresent(String.self, forKey: .discriminator),
                              enumOpen: try or(.enumOpen, false),
                              posX: try c.decodeIfPresent(Double.self, forKey: .posX),
                              posY: try c.decodeIfPresent(Double.self, forKey: .posY),
                              pinned: try or(.pinned, false),
                              sortOrder: try or(.sortOrder, 0),
                              fields: try or(.fields, []),
                              values: try or(.values, []))
        card.uses = try or(.uses, [])
        card.signedBy = try or(.signedBy, [])
        card.verifiedBy = try or(.verifiedBy, [])
        card.next = try or(.next, [])
        card.body = try or(.body, "")
        card.rows = try or(.rows, [])
        card.trigger = try c.decodeIfPresent(Trigger.self, forKey: .trigger)
        card.steps = try or(.steps, [])
        card.cell = try c.decodeIfPresent(FlowCell.self, forKey: .cell)
        card.diagram = try c.decodeIfPresent(String.self, forKey: .diagram)
        card.size = try c.decodeIfPresent(GridSize.self, forKey: .size)
        card.docView = try c.decodeIfPresent(String.self, forKey: .docView)
        card.version = try c.decodeIfPresent(String.self, forKey: .version)
        card.sectionId = try c.decodeIfPresent(String.self, forKey: .sectionId)
        card.customFields = try or(.customFields, [:])
        card.customRows = try or(.customRows, [])
        card.itemPlaces = try or(.itemPlaces, [:])
        card.standalone = try or(.standalone, false)
        card.connections = try or(.connections, [])
        card.container = try c.decodeIfPresent(String.self, forKey: .container)
        if card.kind == .custom {
            card.ownKind = kindName
        }
        // What its definition no longer has: a kind the project defines', and a goal's or a task's
        // taken out of theirs (担当 and the like).
        card.strayFields = try or(.strayFields, [:])
        card.faceRows = try c.decodeIfPresent(Int.self, forKey: .faceRows)
        card.tableMarkdown = try c.decodeIfPresent(String.self, forKey: .tableMarkdown)
        card.review = (try? c.decodeIfPresent([ReviewMark].self, forKey: .review)) ?? []
        card.codeReview = (try? c.decodeIfPresent([CodeReviewLine].self, forKey: .codeReview)) ?? []
        self.card = card
        let form = try c.decodeIfPresent(Form.self, forKey: .form)
        self.form = form?.fields ?? [:]
        formRows = form?.rows ?? [:]
        if self.card.kind == .stackdoc {
            self.card.versions = StackVersion.read(formRows["versions"])
        }
        if self.card.kind == .instance {
            self.card.data = self.form["data"]
        }
        if self.card.kind == .table {
            self.card.tableRules = self.form
        }
    }
}

nonisolated struct WholeSection: Decodable {
    var section: ProjectSection

    /// The section's keys in `project_detail`. Its card ghosts are no longer its own: they are
    /// cards of their own (`ProjectDetail.ghosts`).
    enum Keys: String, CodingKey {
        case id, projectId, name, description, sortOrder, parentId, kind, missing
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        func or<T: Decodable>(_ key: Keys, _ empty: T) throws -> T { try c.decodeIfPresent(T.self, forKey: key) ?? empty }
        section = ProjectSection(id: try c.decode(String.self, forKey: .id),
                              projectId: try c.decode(String.self, forKey: .projectId),
                              name: try c.decode(String.self, forKey: .name),
                              description: try or(.description, ""),
                              sortOrder: try or(.sortOrder, 0),
                              parentId: try c.decodeIfPresent(String.self, forKey: .parentId),
                              // A kind this build does not know leaves the section unkinded, not the project unread.
                              kind: (try? c.decodeIfPresent(SectionKind.self, forKey: .kind)) ?? nil)
        section.kindName = try? c.decodeIfPresent(String.self, forKey: .kind)
        section.missing = (try? c.decodeIfPresent(Bool.self, forKey: .missing)) ?? false
    }
}
