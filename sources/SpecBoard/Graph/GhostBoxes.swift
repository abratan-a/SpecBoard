import Foundation

/// A section's card ghosts on its diagram (`specboard://SpecBoard/Docs/Design:DesignSections` §2・§6): each a box of
/// its own, where the ghost sits, under the ghost's id — the card it leads to drawn whole in
/// it, its rows the card's own, so a row pressed there is the card's row
/// (`DiagramRows.pick`). A ghost that leads nowhere is a small card saying so.
extension CardCanvasView {
    static func box(for ghost: CardGhost, reals: [String: SchemaNode], problem: String?) -> SchemaNode {
        if let card = ghost.resolvesTo.flatMap({ reals[$0] }) {
            var box = card
            box.id = ghost.id
            box.posX = ghost.posX
            box.posY = ghost.posY
            box.pinned = ghost.pinned
            return box
        }
        // A line of its own, saying why, under the ghost's header.
        var box = SchemaNode(id: ghost.id, projectId: ghost.projectId, name: ghost.name, label: "", kind: .note,
                             description: "", tags: [], metadata: [:], rules: [], absentFields: [],
                             discriminator: nil, enumOpen: false, posX: ghost.posX, posY: ghost.posY,
                             pinned: ghost.pinned, sortOrder: ghost.sortOrder, fields: [], values: [])
        box.body = problem ?? L10n[.Diagram.ghostNoTarget]
        box.sectionId = ghost.sectionId
        return box
    }
}
