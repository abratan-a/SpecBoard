import Foundation

/// The cards a card map draws (`specboard://SpecBoard/Docs/Design:Overview` §3): what is inside a section card — a
/// flow's nodes (`FlowCardMap`), a goal's cards (`GoalCardMap`) — drawn as cards of their
/// own on its card editor's canvas. They stand for nothing in the project: their ids carry
/// the map's own prefix, and the detail the canvas draws is a project of these cards alone,
/// so nothing of the relation panel's — its lines, its card ghosts — is drawn there.
nonisolated struct CardMapIds: Sendable {
    let prefix: String

    func isCard(_ id: String) -> Bool { id.hasPrefix(prefix) }
    /// The map's card for what is inside: a node's name, a card's id.
    func id(_ inside: String) -> String { prefix + inside }
    /// What a map's card stands for.
    func inside(of id: String) -> String { String(id.dropFirst(prefix.count)) }
}
