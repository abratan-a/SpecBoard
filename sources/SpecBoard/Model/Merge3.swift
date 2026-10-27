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
        /// `fields.total`, `values.paid`, `versions.v0_2`, `rows.auth.login`, `(whole)`.
        var label: String
        var base: OJSON?
        var source: OJSON?
        var target: OJSON?
    }

    static let whole = "(whole)"

    /// A unit as the screen says it: the whole card or section in the screen's language (the core names it
    /// `whole`, the same in every language, as a choice is kept by it), any other as it is written.
    static func shown(_ unit: String) -> String {
        unit == whole ? L10n[.Branches.mergeWhole] : unit
    }
}
