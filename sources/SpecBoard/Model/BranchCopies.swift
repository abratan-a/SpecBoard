import Foundation

/// A branch against main (`specboard://SpecBoard/Docs/Design:DesignBranches` §3・§11): what it changed, made
/// and removed, and what main did since it was made — worked out from the documents
/// (`Repository.branchDiff`).
struct BranchDiff {
    struct Card: Hashable {
        var path: String
        var changes: [String]
    }

    /// main's cards the branch changed, with how (from its base).
    var changed: [Card] = []
    /// Cards made in the branch.
    var made: [String] = []
    /// main's cards the branch removed.
    var removed: [String] = []
    /// main's sections the branch changed, made and removed.
    var sectionsChanged: [String] = []
    var sectionsMade: [String] = []
    var sectionsRemoved: [String] = []
    /// main's cards that changed in main after the branch was made — or, for a card
    /// the branch changed, after it first changed it — with how where it is known.
    var mainChanged: [Card] = []
    /// Cards main made after the branch was made.
    var mainMade: [String] = []
    /// Cards main removed after the branch was made.
    var mainRemoved: [String] = []
    /// main's cards and sections at a path the branch has one at too, left out of
    /// what the branch sees (§11 "名前の重なり").
    var overlaps: [String] = []
    /// Paths of what this branch changed or made that another branch has changed or
    /// made too, with those branches' names — names and paths only, nothing of what
    /// they hold (§2).
    var elsewhere: [(path: String, branches: [String])] = []

    /// Nothing changed in the branch.
    var isClean: Bool {
        changed.isEmpty && made.isEmpty && removed.isEmpty
            && sectionsChanged.isEmpty && sectionsMade.isEmpty && sectionsRemoved.isEmpty
    }

    /// What the branch's checks have to say: main moved, overlaps, elsewhere.
    var hasNotices: Bool {
        !mainChanged.isEmpty || !mainMade.isEmpty || !mainRemoved.isEmpty || !overlaps.isEmpty || !elsewhere.isEmpty
    }
}
