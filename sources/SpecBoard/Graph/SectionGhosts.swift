import Foundation

/// Other sections' cards, folded: one card per section holding only what this
/// section points at — or what points into it — nested under the card it belongs
/// to. A card ghost is drawn whole instead.
///
/// Drawing every card pointed at in full would put a dozen faint cards around a
/// section to show a dozen fields. One card per section, with the rows the lines
/// land on, says the same with a fraction of the ink:
///
///     Main/db1          section
///       Customer
///         name          ← a line lands here
///       Order
///         total
enum SectionGhosts {
    static let prefix = "§"
    /// What its badge says it is: the built-in kind of card the app alone draws (`section`), as
    /// the definitions name it.
    static let kindWord = "section"

    /// What its badge says where the section is an actor's.
    static let actorWord = "actor"

    static func isGhost(_ id: String) -> Bool { id.hasPrefix(prefix) }

    /// How a section ghost's head is drawn: the section's mark (`ProjectDetail.mark(of:)`), and for an
    /// actor a tint of the actors' blue, the badge `actor`, and a dashed edge for one outside.
    struct Look: Hashable {
        var mark: SectionMark
        var figure: ActorFigure?
    }

    /// The head of the section ghost `id` stands for; nil for an id that is no section's.
    @MainActor
    static func look(of id: String, in detail: ProjectDetail) -> Look? {
        guard isGhost(id), let section = detail.directory.section(id: String(id.dropFirst(prefix.count))) else { return nil }
        return Look(mark: detail.mark(of: section), figure: section.actorFigure)
    }

    /// The card or field a section ghost's row stands for: its rows carry
    /// `§f|<schema>|<field>` as their id — the field's id, or the names of a place the card does
    /// not have as a field, as a path writes them (which may hold a `|` of their own).
    static func target(of field: FieldNode) -> (schema: String, field: String?)? {
        let parts = field.id.split(separator: "|", maxSplits: 2, omittingEmptySubsequences: false).map(String.init)
        guard parts.count == 3, parts[0] == "§f" else { return nil }
        return (parts[1], parts[2].isEmpty ? nil : parts[2])
    }

    struct Folded {
        var relations: [Relation]
        var cards: [SchemaNode]
        /// The section ghosts this section points into — drawn on its right.
        var referenced: Set<String>
    }

    /// `own`: the cards of the section on screen. `whole`: what is drawn in full besides —
    /// its card ghosts' boxes, other sections' cards put there for a look — which lines
    /// keep landing on. `names`: each other
    /// card as this section writes it (`Main/db1:Customer`), for the card's title.
    /// `groups`: a card that goes into another section's card than its own — the
    /// child of the section on screen it is under — and `titles` those cards' names.
    static func fold(_ relations: [Relation], own: Set<String>, whole: Set<String>,
                     schemas: [SchemaNode], names: [String: String], projectId: String,
                     groups: [String: String] = [:], titles groupTitles: [String: String] = [:]) -> Folded {
        var byId: [String: SchemaNode] = [:]
        for schema in schemas where byId[schema.id] == nil { byId[schema.id] = schema }
        var order: [String] = []
        var trees: [String: [FieldNode]] = [:]
        var titles: [String: String] = [:]
        var referenced: Set<String> = []

        func card(for schema: SchemaNode) -> String {
            let section = groups[schema.id] ?? schema.sectionId ?? ""
            let id = prefix + section
            if trees[id] == nil {
                order.append(id)
                trees[id] = []
                let written = names[schema.id] ?? schema.name
                titles[id] = groupTitles[section] ?? PathText.splitSection(written).section ?? written
            }
            return id
        }

        func insert(_ nodes: inout [FieldNode], _ chain: ArraySlice<(id: String, name: String, type: FieldType, ref: String?)>) {
            guard let first = chain.first else { return }
            if let index = nodes.firstIndex(where: { $0.id == first.id }) {
                insert(&nodes[index].children, chain.dropFirst())
            } else {
                var made = node(first.id, first.name, first.type, order: nodes.count)
                // A reference keeps what it refers to, or it reads as pointing nowhere.
                made.embeds = first.ref
                insert(&made.children, chain.dropFirst())
                nodes.append(made)
            }
        }

        /// The row for `path` inside `schema`, made on its section ghost if new;
        /// the row's path on that card.
        func row(_ schema: SchemaNode, _ path: [String], on cardId: String) -> String {
            var chain: [(id: String, name: String, type: FieldType, ref: String?)] = [
                ("§f|\(schema.id)|", schema.name, FieldType(rawValue: schema.kind.rawValue), nil),
            ]
            var list = schema.fields
            for (index, step) in path.enumerated() {
                let real = list.first { $0.name == step }
                let last = index == path.count - 1
                chain.append(("§f|\(schema.id)|\(real?.id ?? PathText.joinNames(path[...index]))",
                              step, last ? (real?.type ?? FieldType(rawValue: "string")) : .object,
                              last ? real?.embeds : nil))
                list = real?.children ?? []
            }
            insert(&trees[cardId, default: []], chain[...])
            return PathText.joinNames([schema.name] + path)
        }

        /// The row a line landing at `landing` in `target` arrives at: the place's names nested under the
        /// card's row; for a function or type of a file, a row of its own said beside its file
        /// (`a.rs › Order.total`) — one for each symbol, its names being the file's and not the card's.
        func land(_ target: SchemaNode, _ landing: String, on cardId: String) -> String {
            if WorkDirIds.isFile(target.id), !landing.isEmpty, !landing.hasPrefix("#") {
                let name = SymbolPath.said(file: target.name, landing)
                let chain: [(id: String, name: String, type: FieldType, ref: String?)] = [
                    ("§f|\(target.id)|\(landing)", name, FieldType(rawValue: "symbol"), nil),
                ]
                insert(&trees[cardId, default: []], chain[...])
                return PathText.quote(name)
            }
            let path = landing.hasPrefix("#") ? [] : PathText.names(landing).filter { !$0.isEmpty }
            return row(target, path, on: cardId)
        }

        var out: [Relation] = []
        for var relation in relations {
            let fromOwn = own.contains(relation.fromSchemaId)
            let toOwn = own.contains(relation.toSchemaId)
            if fromOwn, !toOwn, !whole.contains(relation.toSchemaId), let target = byId[relation.toSchemaId] {
                let id = card(for: target)
                relation.at = land(target, relation.at ?? "", on: id)
                relation.toSchemaId = id
                referenced.insert(id)
            } else if toOwn, !fromOwn, !whole.contains(relation.fromSchemaId), let source = byId[relation.fromSchemaId] {
                let id = card(for: source)
                relation.from = row(source, [], on: id)
                relation.fromSchemaId = id
            } else if !fromOwn, !toOwn, let source = byId[relation.fromSchemaId], let target = byId[relation.toSchemaId],
                      let a = groups[source.id], let b = groups[target.id], a != b {
                // Between two of the children: both ends go into their cards.
                let to = card(for: target)
                relation.at = land(target, relation.at ?? "", on: to)
                relation.toSchemaId = to
                referenced.insert(to)
                let from = card(for: source)
                relation.from = row(source, [], on: from)
                relation.fromSchemaId = from
            }
            out.append(relation)
        }

        let cards = order.map { id in
            SchemaNode(id: id, projectId: projectId, name: titles[id] ?? "", label: "", kind: .entity,
                       description: "", tags: [], metadata: [:], rules: [], absentFields: [],
                       discriminator: nil, enumOpen: false, posX: nil, posY: nil, pinned: false,
                       sortOrder: 0, fields: trees[id] ?? [], values: [])
        }
        return Folded(relations: out, cards: cards, referenced: referenced)
    }

    private static func node(_ id: String, _ name: String, _ type: FieldType, order: Int) -> FieldNode {
        FieldNode(id: id, parentFieldId: nil, name: name, label: "", type: type,
                  isArray: false, isRequired: false, isNullable: false, constValue: nil,
                  isUnique: false, isPk: false, description: "", rules: [], variants: [], sortOrder: order,
                  children: [])
    }
}
