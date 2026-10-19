import Foundation

/// A merge's conflicts as the window shows and settles them (`specboard://SpecBoard/Docs/Design:DesignBranches`
/// §6). The merge itself is the core's (`merge_preview`, `merge`): unit by unit, from
/// what both sides last had in common.
enum Merge3 {
    /// Which side a conflict takes.
    enum Side: String { case source, target }

    struct Conflict {
        /// The unit, as it is kept apart: a key, a field or a row by its place, or
        /// `(whole)` for one side removing what the other changed.
        var unit: String
        /// The unit as a person names it, one of a card's alone: `description`,
        /// `fields.total`, `values.paid`, `versions.v0_2`, `rows.auth.login`, `（カード）`.
        var label: String
        var base: OJSON?
        var source: OJSON?
        var target: OJSON?
    }

    static let whole = "(whole)"
}
