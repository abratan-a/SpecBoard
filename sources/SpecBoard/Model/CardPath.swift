import Foundation

/// A path as it is written, taken apart: the card it starts from, as written
/// (`Core:User`, `User`, or `main@Core:User` from before), and the names inside the card as
/// written — quoted where they were (`'e mail'`), an element picked kept (`ports[0]`) — with
/// their `[]` markers dropped (`specboard://SpecBoard/Docs/Design:DesignSections` §2). A quoted
/// name is one name whatever it holds (`PathText`).
struct WrittenPath: Equatable {
    /// The card with its sections, as written.
    var card: String
    /// The names after the card, in order, as written.
    var names: [String]
    /// Whether every name between the dots is there (`User..id` is not).
    var wellFormed: Bool

    /// Nil when there is no card to start from.
    init?(_ raw: String) {
        let (section, rest) = splitSection(raw.trimmingCharacters(in: .whitespaces))
        let parts = PathText.dots(rest).map(PathText.withoutAll)
        wellFormed = !parts.contains(where: \.isEmpty)
        let named = parts.filter { !$0.isEmpty }
        guard let head = named.first else { return nil }
        card = section.map { "\($0):\(head)" } ?? head
        names = Array(named.dropFirst())
    }
}

/// Where the names inside a card lead: a field — on through a `ref` into the card it
/// embeds when more names follow — a formatdoc's row, or a flow's node. The one walk
/// every reader of a path makes: the MCP target, the check for broken paths, the
/// links between items, the values a note puts in, the impact of a change, and the
/// identities a rename is followed by.
struct CardWalk {
    enum Place: Equatable {
        case card
        case field(FieldNode)
        case formatRow([String])
        case node(FlowNode)
        /// A custom card's row, by its names from the top (`specboard://SpecBoard/Docs/Design:DesignBoard` §3).
        case customRow([String])
    }

    /// The card the path starts from.
    var head: SchemaNode
    /// The card the last name is in: `head`, or one a `ref` on the way embeds.
    var holder: SchemaNode
    /// The names from `holder` to the place.
    var within: [String]
    var place: Place
    /// The fields passed, in order, across `ref`s.
    var fields: [FieldNode]
    /// Each card entered, with the names walked in it, outermost first.
    var legs: [(card: SchemaNode, names: [String])]

    /// Where the walk stopped, when a name is not there.
    struct Miss: Error {
        enum Kind { case field, formatRow, node, customRow }
        var kind: Kind
        /// The card it stopped in, and the names walked in it before the missing one.
        var holder: SchemaNode
        var walked: [String]
        /// The names walked from the head before the missing one.
        var fromHead: [String]
        /// What is there instead.
        var there: [String]
    }

    /// The row a line to this place lands on in `head`'s card, as a path writes it: the names
    /// from the head for a field, the item a formatdoc's row is (or is in) — `#top` for a row at
    /// the top — `#n.<name>` for a node, empty for the card.
    var row: String {
        switch place {
        case .card: ""
        case .node(let node): SchemaNode.nodeRow(node.name)
        case .field: PathText.joinNames(legs.flatMap(\.names))
        case .formatRow(let names):
            {
                let item = FormatRows.item(of: names, in: head.rows)
                return item.isEmpty ? SchemaNode.topRow : PathText.joinNames(item)
            }()
        case .customRow(let names): Self.rowPath(names, in: head)
        }
    }

    /// A custom row's names as its card draws its row: a data card's as written (each with the
    /// element it picks), any other's quoted where they need it.
    static func rowPath(_ names: [String], in card: SchemaNode) -> String {
        card.kind == .instance ? names.joined(separator: ".") : PathText.joinNames(names)
    }

    /// `written`: the names as a path writes them (`WrittenPath.names`); the walk, its legs and
    /// its place hold the names as they are. A data card's names keep the element they pick.
    /// `cards`: every card of the project by id, for the `ref`s on the way.
    /// `throughRefs`: whether a name after a `ref` is looked for in the card it
    /// embeds. An MCP target does not go through (`MCPViews.locate`): the card a
    /// field is in is named for itself.
    static func walk(_ head: SchemaNode, _ names: [String], cards: [String: SchemaNode],
                     throughRefs: Bool = true) -> Result<CardWalk, Miss> {
        walk(head, names, card: { cards[$0] }, throughRefs: throughRefs)
    }

    static func walk(_ head: SchemaNode, _ names: [String], in directory: SchemaDirectory,
                     throughRefs: Bool = true) -> Result<CardWalk, Miss> {
        walk(head, names, card: directory.schema(id:), throughRefs: throughRefs)
    }

    static func walk(_ head: SchemaNode, _ written: [String], card: (String) -> SchemaNode?,
                     throughRefs: Bool = true) -> Result<CardWalk, Miss> {
        var out = CardWalk(head: head, holder: head, within: [], place: .card, fields: [], legs: [(head, [])])
        guard !written.isEmpty else { return .success(out) }
        let names = written.map(PathText.plain)

        switch head.kind {
        case .formatdoc:
            let (found, missing) = FormatRows.deepest(names, in: head.rows)
            guard missing.isEmpty else {
                return .failure(Miss(kind: .formatRow, holder: head, walked: found, fromHead: found,
                                     there: FormatRows.children(of: found, in: head.rows).map(\.name)))
            }
            out.within = names
            out.legs = [(head, names)]
            out.place = .formatRow(names)
            return .success(out)
        case .goal:
            // A goal's rows are the cards inside it, and on into a goal inside it
            // (`Goals:Launch.Spec.Review`).
            var holder = head
            var found: [String] = []
            for name in names {
                let inside = holder.inside.compactMap(card)
                guard let inner = inside.first(where: { $0.name == name }) else {
                    return .failure(Miss(kind: .customRow, holder: head, walked: found, fromHead: found,
                                         there: inside.map(\.name)))
                }
                found.append(name)
                holder = inner
            }
            out.within = names
            out.legs = [(head, names)]
            out.place = .customRow(names)
            return .success(out)
        case .stackdoc:
            // A version by its name (`Docs:Changelog.v0_2_21`), and nothing inside it.
            guard names.count == 1, head.versions.contains(where: { $0.name == names[0] }) else {
                let there = head.versions.contains { $0.name == names[0] }
                return .failure(Miss(kind: .customRow, holder: head, walked: there ? [names[0]] : [],
                                     fromHead: there ? [names[0]] : [],
                                     there: there ? [] : head.versions.map(\.name)))
            }
            out.within = names
            out.legs = [(head, names)]
            out.place = .customRow(names)
            return .success(out)
        case .instance:
            // A data card's value by its shape's names (`Data:first.customer.name`), an array's element
            // picked (`Data:host.ports[number=8200].number`, `ports[0]`).
            guard let shape = head.instanceOf.flatMap(card) else {
                return .failure(Miss(kind: .customRow, holder: head, walked: [], fromHead: [], there: []))
            }
            guard InstanceRows.fields(along: written, of: shape, card: card) != nil else {
                var walked: [String] = []
                for name in written {
                    guard InstanceRows.fields(along: walked + [name], of: shape, card: card) != nil else { break }
                    walked.append(name)
                }
                let there = walked.isEmpty ? shape.fields.map(\.name)
                    : InstanceRows.fields(along: walked, of: shape, card: card)?.last.map { last -> [String] in
                        if last.isArray { return [] }
                        if last.type.isObject { return last.children.map(\.name) }
                        return last.embeds.flatMap(card)?.fields.map(\.name) ?? []
                    } ?? []
                return .failure(Miss(kind: .customRow, holder: head, walked: walked, fromHead: walked, there: there))
            }
            // A data card's names stay as written, each with the element it picks.
            let steps = written.map(PathText.canonicalStep)
            out.within = steps
            out.legs = [(head, steps)]
            out.place = .customRow(steps)
            return .success(out)
        case .flow:
            guard names.count == 1, let node = head.nodes.first(where: { $0.name == names[0] }) else {
                let first = names.count == 1 ? [] : [names[0]]
                let there = head.nodes.contains { $0.name == names[0] } ? [] : head.nodes.map(\.name)
                return .failure(Miss(kind: .node, holder: head, walked: first, fromHead: first, there: there))
            }
            out.within = names
            out.legs = [(head, names)]
            out.place = .node(node)
            return .success(out)
        default:
            break
        }

        var list = head.fields
        var fromHead: [String] = []
        for (index, name) in names.enumerated() {
            guard let next = list.first(where: { $0.name == name }) else {
                return .failure(Miss(kind: .field, holder: out.holder, walked: out.within, fromHead: fromHead,
                                     there: list.map(\.name)))
            }
            fromHead.append(name)
            out.within.append(name)
            out.legs[out.legs.count - 1].names.append(name)
            out.fields.append(next)
            out.place = .field(next)
            // A reference is a step of its own: the rest of the path lives over there.
            if throughRefs, index < names.count - 1, next.type.isRef,
               let target = next.embeds.flatMap(card) {
                out.holder = target
                out.within = []
                out.legs.append((target, []))
                list = target.fields
            } else {
                list = next.children
            }
        }
        return .success(out)
    }
}
