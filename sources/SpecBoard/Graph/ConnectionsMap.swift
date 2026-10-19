import CoreGraphics
import Foundation

/// How far a card's connections map reaches: the cards a line away, two, or every card
/// that can be reached.
enum ConnectionDepth: Int, CaseIterable, Hashable, Sendable {
    case one = 1
    case two = 2
    case all = 0

    /// The steps it reaches; nil, as many as there are.
    var steps: Int? { self == .all ? nil : rawValue }

    var label: String {
        switch self {
        case .one: L10n[.Lines.depthOne]
        case .two: L10n[.Lines.depthTwo]
        case .all: L10n[.Lines.depthAll]
        }
    }
}

/// One card's connections, drawn for a look (`specboard://SpecBoard/Docs/Design:Overview` §13「つながりの図」): the
/// card in the middle and, around it, the cards its lines reach, in a group for each section — what
/// only leads to it on the left, what it only leads to on the right, what goes both ways above and
/// below it, and each step further out beyond the group it hangs from. Only the lines drawn count
/// (`shows`): a kind switched off brings no card.
///
/// The cards are the project's own, by their own ids — picked, opened, right-clicked as on the
/// section map — named as the card's section writes them, and put by this (`arrange`) each
/// time it is drawn: nothing of where they sit is written.
enum ConnectionsMap {
    struct Made {
        /// The cards and the lines between them, as a section's diagram is given them.
        var detail: ProjectDetail
        /// What the cards are drawn against: the rest of the project's cards, read but not drawn —
        /// and the groups' frames (`Surroundings.frames`).
        var surroundings: Surroundings
        /// Each card's column: 0 the card, below 0 what leads to it, above 0 what it leads to —
        /// how many steps away it is, and which way it was first reached.
        var columns: [String: Int]
        /// The kinds of line that touch these cards, drawn or not — what can be switched under it.
        var kinds: [String]
        /// The groups the cards are put in, nearest first.
        var groups: [Group]
    }

    /// Which side of the card a group goes on.
    enum Side: Equatable, Sendable {
        /// What only leads to the card.
        case left
        /// What the card only leads to.
        case right
        /// What both leads to it and is led to from it, split between above and below.
        case top, bottom
    }

    /// The cards of one section at one step — at the first step, of one way too, so a group's
    /// side says how its cards are joined to the card.
    struct Group: Equatable {
        var id: String
        var section: String
        var title: String
        var step: Int
        var side: Side
        var cards: [String]
        /// Further out than the first step: the group of the step before it is put beside.
        var anchor: String? = nil
        /// The lines joining it to the card (at the first step) or to the group it is put beside.
        var lines = 0
    }

    /// The map of `focus`'s connections in `whole`, as deep as `depth`, by the kinds of line
    /// `shows` lets through. `moved`: where a card was put by hand while it was looked at.
    static func make(_ focus: String, depth: ConnectionDepth, in whole: ProjectDetail,
                     shows: (String) -> Bool, moved: [String: GridPoint] = [:],
                     settings: LayoutSettings = .standard) -> Made? {
        let byId = Dictionary(whole.schemas.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        guard let center = byId[focus] else { return nil }
        let innerIn = Dictionary(whole.inner.map { ($0.id, $0.container) }, uniquingKeysWith: { a, _ in a })

        // What a line's end is drawn as: the card itself, or the outermost container it is in
        // — but the card looked at stands for itself and what is inside it.
        func unit(_ id: String) -> String? {
            var at = id
            var seen: Set<String> = []
            while seen.insert(at).inserted {
                if at == focus { return focus }
                if let up = byId[at]?.container {
                    at = up
                } else if byId[at] == nil, let flow = innerIn[at] {
                    at = flow
                } else {
                    return byId[at] == nil ? nil : at
                }
            }
            return byId[at] == nil ? nil : at
        }

        // The project's lines between two cards, each end what it is drawn as.
        var ends: [(from: String, to: String, relation: Relation)] = []
        for relation in whole.relations {
            guard let from = unit(relation.fromSchemaId), let to = unit(relation.toSchemaId) else { continue }
            // A line inside one card (between a flow's nodes, a goal's cards) is its own contents map's.
            if from == to && (from != relation.fromSchemaId || to != relation.toSchemaId) { continue }
            ends.append((from, to, relation))
        }

        // A step at a time each way: out along the lines, in against them.
        var ahead: [String: [String]] = [:], behind: [String: [String]] = [:]
        for end in ends where end.from != end.to && shows(end.relation.line) {
            ahead[end.from, default: []].append(end.to)
            behind[end.to, default: []].append(end.from)
        }
        func reach(_ next: [String: [String]]) -> [String: Int] {
            var steps = [focus: 0]
            var frontier = [focus]
            var step = 0
            while !frontier.isEmpty, depth.steps.map({ step < $0 }) ?? true {
                step += 1
                var found: [String] = []
                for id in frontier {
                    for other in next[id] ?? [] where steps[other] == nil {
                        steps[other] = step
                        found.append(other)
                    }
                }
                frontier = found
            }
            return steps
        }
        let outward = reach(ahead), inward = reach(behind)
        var columns: [String: Int] = [:]
        for id in Set(outward.keys).union(inward.keys) {
            switch (outward[id], inward[id]) {
            case let (out?, back?): columns[id] = back < out ? -back : out
            case let (out?, nil): columns[id] = out
            case let (nil, back?): columns[id] = -back
            case (nil, nil): break
            }
        }

        // The cards, in the project's order, each named as the card's own section writes it.
        let directory = whole.directory
        let base = directory.sectionId(of: center)
        var cards: [SchemaNode] = []
        for schema in whole.schemas where columns[schema.id] != nil {
            var card = schema
            card.name = directory.shown(schema, from: base)
            if card.id == focus { card.container = nil }
            cards.append(card)
        }
        let chosen = Set(cards.map(\.id))

        // Every line between two of them, drawn as the switches say; through a card ghost, to
        // the card it leads to (none is drawn here).
        let between = ends.filter { chosen.contains($0.from) && chosen.contains($0.to) }
        let lines = between.map { end -> Relation in
            var line = end.relation
            line.viaGhostId = nil
            return line
        }
        let touching = Set(ends.filter { chosen.contains($0.from) || chosen.contains($0.to) }.map(\.relation.line))
        let defined = whole.lineKinds.names
        let kinds = defined.filter(touching.contains) + touching.subtracting(defined).sorted()

        // A value object or an enum another of them holds is drawn inside it, not put on its own.
        let drawn = Set(withoutValueCards(cards, relations: lines).map(\.id)).union([focus])
        let shownCards = cards.filter { drawn.contains($0.id) }
        let groups = group(shownCards, focus: focus, columns: columns,
                           leadsTo: Set(ahead[focus] ?? []), ledFrom: Set(behind[focus] ?? []),
                           lines: between.filter { shows($0.relation.line) }.map { ($0.from, $0.to) },
                           directory: directory, base: base)
        // Each card as many grid units as it takes as it first shows: what keeps two apart.
        let lookup: SchemaLookup = { byId[$0] }
        var pieces: [String: Piece] = [:]
        for card in shownCards {
            pieces[card.id] = Piece(id: card.id, w: cardGridColumns(card),
                                    h: cardGridRows(card, lookup: lookup, rows: settings.size.rows, docView: settings.docView))
        }
        let arranged = arrange(
            center: pieces[focus] ?? Piece(id: focus, w: Grid.cardColumns, h: Grid.rows(forRows: 1)),
            bunches: groups.map { group in
                Bunch(id: group.id, cards: group.cards.compactMap { pieces[$0] }, step: group.step,
                      side: group.side, anchor: group.anchor)
            },
            margin: settings.margin)
        // Where each sits here, not where its own section has it.
        for index in cards.indices {
            let at = moved[cards[index].id] ?? arranged.cards[cards[index].id]
            cards[index].posX = at.map { Double($0.x) }
            cards[index].posY = at.map { Double($0.y) }
            cards[index].pinned = false
        }

        var detail = whole
        detail.schemas = cards
        detail.relations = lines
        detail.ghosts = []

        var around = Surroundings()
        // Read, not drawn: what a card's fields are made of, wherever it is.
        around.schemas = whole.schemas.filter { $0.container == nil && !chosen.contains($0.id) }
        for schema in around.schemas { around.names[schema.id] = directory.shown(schema, from: base) }
        around.inside = whole.schemas.filter { $0.container != nil && !chosen.contains($0.id) }
        around.everyGhost = whole.ghosts
        for ghost in whole.ghosts { around.ghostNames[ghost.id] = directory.shown(ghost, from: base) }
        // A value object or an enum looked at is drawn as a card of its own, held or not.
        if center.kind == .value || center.kind == .enum { around.here = [focus] }
        around.frames = groups.map { CardFrame(id: $0.id, title: $0.title, cards: $0.cards) }
        return Made(detail: detail, surroundings: around, columns: columns, kinds: kinds, groups: groups)
    }
}

// MARK: - Groups

extension ConnectionsMap {
    /// The cards but the one looked at, in a group for each section at each step: at the first, apart
    /// by the way they are joined to it (`leadsTo`, `ledFrom`); further out, on the side of the group
    /// of the step before that most lines join them to. In the order they go out from the card.
    static func group(_ cards: [SchemaNode], focus: String, columns: [String: Int],
                      leadsTo: Set<String>, ledFrom: Set<String>, lines: [(String, String)],
                      directory: SchemaDirectory, base: String) -> [Group] {
        let root = directory.root(of: base)
        var made: [Group] = []
        var index: [String: Int] = [:]
        var inGroup: [String: Int] = [:]
        for card in cards where card.id != focus {
            guard let column = columns[card.id] else { continue }
            let step = abs(column)
            let section = directory.sectionId(of: card)
            let side: Side
            let way: String
            if step == 1 {
                switch (ledFrom.contains(card.id), leadsTo.contains(card.id)) {
                case (true, true): side = .top; way = "both"
                case (true, false): side = .left; way = "in"
                default: side = .right; way = "out"
                }
            } else {
                // Settled below, from the group it hangs from.
                side = column < 0 ? .left : .right
                way = ""
            }
            let id = "\(step)|\(way)|\(section)"
            inGroup[card.id] = index[id] ?? made.count
            if let at = index[id] {
                made[at].cards.append(card.id)
            } else {
                index[id] = made.count
                made.append(Group(id: id, section: section,
                                  title: directory.shown(section: section, within: root),
                                  step: step, side: side, cards: [card.id]))
            }
        }
        // Nearest first; at the first step the most lines to the card first — put nearest it, so few long
        // lines cross the others — then the biggest, then by name, so the same map is put the same way.
        for (a, b) in lines where a != b && (a == focus || b == focus) {
            guard let at = inGroup[a == focus ? b : a] else { continue }
            made[at].lines += 1
        }
        made.sort {
            ($0.step, -$0.lines, -$0.cards.count, $0.title, $0.id) < ($1.step, -$1.lines, -$1.cards.count, $1.title, $1.id)
        }

        // Both ways: above and below in turn, by how wide each side has grown.
        var above = 0, below = 0
        for at in made.indices where made[at].step == 1 && made[at].side == .top {
            if below < above {
                made[at].side = .bottom
                below += made[at].cards.count
            } else {
                above += made[at].cards.count
            }
        }

        // Further out: beside the group of the step before that most lines join them to.
        var groupOf: [String: String] = [:]
        for group in made { for card in group.cards { groupOf[card] = group.id } }
        var joins: [String: [String: Int]] = [:]
        for (a, b) in lines where a != b {
            guard let ga = groupOf[a], let gb = groupOf[b], ga != gb else { continue }
            joins[ga, default: [:]][gb, default: 0] += 1
            joins[gb, default: [:]][ga, default: 0] += 1
        }
        var sideOf = Dictionary(made.map { ($0.id, $0.side) }, uniquingKeysWith: { a, _ in a })
        for at in made.indices where made[at].step > 1 {
            let before = made.filter { $0.step == made[at].step - 1 }
            let counts = joins[made[at].id] ?? [:]
            guard let hung = before.max(by: { (counts[$0.id] ?? 0, $1.id) < (counts[$1.id] ?? 0, $0.id) }),
                  (counts[hung.id] ?? 0) > 0, let side = sideOf[hung.id] else { continue }
            made[at].side = side
            made[at].anchor = hung.id
            made[at].lines = counts[hung.id] ?? 0
            sideOf[made[at].id] = side
        }
        return made
    }
}
