import Foundation

/// A goal's cards drawn the way the relation panel draws cards — its card map: a card each it
/// holds (the cards inside it, `inside`), placed on the grid where the goal's map keeps it
/// (`itemPlaces`). A task is drawn by its kind's face: its check above, its body below as
/// Markdown, sized by hand; a goal inside is drawn as the relation panel draws a goal. Each says
/// its order in the goal — the order it is done in — by its number (1, 2, 3…). The lines between
/// them are drawn (a task's `link` to another); a line out of the goal, or
/// from a card deeper in, is not — the map draws the goal's cards alone.
///
/// As a flow's card map (`FlowCardMap`): the cards are made for the goal's card editor and
/// stand for nothing in the project — their ids carry the map's prefix (`CardMapIds`) — and
/// `detail` is a project of these cards alone. Unlike a node, each is a card of the project with
/// a document of its own, copied as it is.
enum GoalCardMap {
    static let ids = CardMapIds(prefix: "◇")

    static func isCard(_ id: String) -> Bool { ids.isCard(id) }
    static func id(_ card: String) -> String { ids.id(card) }
    /// The project's card a map's card stands for.
    static func card(of id: String) -> String { ids.inside(of: id) }

    /// The row of a task's card with its check, its kind's face not read: pressed, the task is
    /// done or not.
    static let checkRow = "#done"

    /// The goal's cards as a project of their own, to be drawn: each as `project` has it, where
    /// the goal's map keeps it — one with no place there is found room by the canvas — and
    /// numbered in the order the goal holds them.
    static func detail(_ goal: SchemaNode, in project: ProjectDetail) -> ProjectDetail {
        let byId = Dictionary(project.schemas.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        // Only real cards are inside a goal: a card ghost is never one of them.
        let cards = goal.inside.enumerated().compactMap { index, inside -> SchemaNode? in
            guard inside != goal.id, var card = byId[inside] else { return nil }
            let at = goal.itemPlaces[inside]
            card.id = id(inside)
            card.posX = at.map { Double($0.x) }
            card.posY = at.map { Double($0.y) }
            card.pinned = at != nil
            card.sortOrder = index
            card.sectionId = nil
            card.mapNumber = index + 1
            // What a goal inside joins its rows to is not drawn here: no lines on the map.
            card.customRows = card.customRows.map { row in
                var row = row
                row.links = []
                return row
            }
            return card
        }
        // The lines one of its cards writes to another of them, from the card's own map card to
        // the other's; what a text names is not one of them.
        let held = Set(cards.map { card(of: $0.id) })
        let lines = project.relations.compactMap { relation -> Relation? in
            guard relation.connection != nil, relation.fromSchemaId != relation.toSchemaId,
                  held.contains(relation.fromSchemaId), held.contains(relation.toSchemaId) else { return nil }
            var drawn = relation
            drawn.fromSchemaId = id(relation.fromSchemaId)
            drawn.toSchemaId = id(relation.toSchemaId)
            drawn.viaGhostId = nil
            return drawn
        }
        var out = ProjectDetail(project: project.project, schemas: cards, relations: lines)
        out.lineKinds = project.lineKinds
        return out
    }

    /// What a task's card shows on the map when its kind's face is not read: its title (its
    /// label, else its name) with its check.
    static func rows(_ task: SchemaNode) -> [CardRow] {
        [CardRow(path: checkRow, depth: 0, field: nil, value: nil, fromType: nil,
                 step: StepRow(label: "", text: title(task), detail: nil),
                 form: .toggle(on: isDone(task)))]
    }

    /// A card's title on the map: its label, else its name.
    static func title(_ card: SchemaNode) -> String { card.label.isEmpty ? card.name : card.label }

    /// Whether a task is done (its `done`).
    static func isDone(_ task: SchemaNode) -> Bool { task.customFields["done"] == "true" }
}
