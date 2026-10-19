import Foundation

/// A merge request (MR): a request to merge a branch into a locked main — what an AI's
/// `branch_merge` into it makes — with what the merge would do when it was tried
/// (`specboard://SpecBoard/Docs/Design:DesignBranches` §8): a person approves it — and it goes in as
/// it was tried — or rejects it, saying why.
struct MergeRequest: Hashable, Identifiable {
    enum State: String, Hashable {
        case open, merged, rejected, withdrawn
        /// Open, but the branch or main moved since it was tried: it cannot go in as it
        /// was. Never kept: worked out each time it is read.
        case stale
    }

    /// What the merge would do, as it was tried.
    struct Result: Codable, Hashable {
        struct Item: Codable, Hashable {
            var path: String
            var section: Bool
            /// `add`, `change` or `remove`.
            var change: String
        }
        /// A conflict, with what each side had and which the request took.
        struct Conflict: Codable, Hashable {
            var path: String
            var unit: String
            var took: String
            var base: String?
            var source: String?
            var target: String?
        }
        var items: [Item] = []
        var conflicts: [Conflict] = []
        /// Paths the merge would leave unresolved in main.
        var warnings: [String] = []
        /// How many more boxes the branch moved would change main, were they taken along,
        /// than go in without — asked for or not. Nil when none would.
        var places: Int?
    }

    var id: String
    /// Numbered within its project, from 1.
    var number: Int
    var branchId: String
    /// Its name when the request was made: still said when the branch is gone.
    var branchName: String
    var title: String
    var description: String
    /// The access key's name it came through; nil for a call without one.
    var author: String?
    var at: String
    var state: State
    var reason: String?
    var decidedAt: String?
    /// main's commit that merged it.
    var mergeSeq: Int?
    var result: Result
    /// Whether it asks for the boxes the branch moved to go into main too (`places`). The
    /// person approving it may say otherwise.
    var places = false

    var stateLabel: String {
        switch state {
        case .open: L10n[.Branches.requestStateOpen]
        case .stale: L10n[.Branches.requestStateStale]
        case .merged: L10n[.Branches.requestStateMerged]
        case .rejected: L10n[.Branches.requestStateRejected]
        case .withdrawn: L10n[.Branches.requestStateWithdrawn]
        }
    }

    var authorLabel: String { author.map { L10n[.Branches.requestAuthorMCP($0)] } ?? "MCP" }
}
