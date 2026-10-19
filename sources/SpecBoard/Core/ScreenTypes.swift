import Foundation

// What the window reads from the core and the MCP entrance hands on, as the window's own
// types (`CoreReads`, `CoreAccess`). They were the Swift store's; the store is the core's now.

/// A branch of a project: a project for a change, made from main (`specboard://SpecBoard/Docs/Design:DesignBranches`
/// §3・§11).
struct BranchInfo: Codable, Hashable, Identifiable, Sendable {
    var id: String
    var projectId: String
    var name: String
    var sortOrder: Int
    var createdAt: String
}

/// What the open branch did to its cards and sections against main, and what main did since
/// (`specboard://SpecBoard/Docs/Design:DesignBranches` §11, `specboard://SpecBoard/Docs/Design:UI` §2「変更の印」):
/// what the screen marks, by id, as `branch_read` counts it (`branch_marks`).
struct BranchMarks: Equatable {
    var changed: Set<String> = []
    var made: Set<String> = []
    var removed: Set<String> = []
    var mainChanged: Set<String> = []
    var sectionsChanged: Set<String> = []
    var sectionsMade: Set<String> = []
    var sectionsRemoved: Set<String> = []
    /// What the branch removed, as main held it, to be shown where it was.
    var gone = Gone()
    /// How many cards main changed, made or removed since the branch was made: what taking main in brings.
    var mainMoved = 0

    /// The cards, card ghosts, flows' nodes and sections a branch removed, read as the project's are.
    struct Gone: Equatable {
        var cards: [SchemaNode] = []
        var ghosts: [CardGhost] = []
        var nodes: [InnerCard] = []
        var sections: [ProjectSection] = []

        var isEmpty: Bool { cards.isEmpty && ghosts.isEmpty && nodes.isEmpty && sections.isEmpty }
    }

    var isEmpty: Bool {
        changed.isEmpty && made.isEmpty && removed.isEmpty && mainChanged.isEmpty
            && sectionsChanged.isEmpty && sectionsMade.isEmpty && sectionsRemoved.isEmpty
    }

    /// A card's, a card ghost's or a node's own mark: what the branch did — main's change beside it.
    func mark(for id: String) -> ChangeMark? {
        if removed.contains(id) { return .removed }
        if mainChanged.contains(id) { return made.contains(id) || changed.contains(id) ? .changedBoth : .mainChanged }
        if made.contains(id) { return .made }
        if changed.contains(id) { return .changed }
        return nil
    }

    /// A section's own mark.
    func sectionMark(for id: String) -> ChangeMark? {
        if sectionsRemoved.contains(id) { return .removed }
        if sectionsMade.contains(id) { return .made }
        if sectionsChanged.contains(id) { return .changed }
        return nil
    }
}

/// A mark the screen puts on what a branch changed, git's way (`specboard://SpecBoard/Docs/Design:UI` §2
/// 「変更の印」): A, M, D, ↓, M↓ on a thing itself, ● on what holds things that changed.
enum ChangeMark: Hashable, Sendable {
    /// Made in the branch.
    case made
    /// Changed in the branch: renamed, or moved to another section, too.
    case changed
    /// Removed in the branch.
    case removed
    /// Changed in main since the branch was made.
    case mainChanged
    /// Changed in main, and in the branch too.
    case changedBoth
    /// Something inside changed, told by what changed there.
    case inside(Tint)

    enum Tint: Hashable, Sendable { case green, yellow, red, blue }

    /// Its letters, each in its tint: the branch's M and main's ↓ side by side.
    var letters: [(text: String, tint: Tint)] {
        switch self {
        case .made: [("A", .green)]
        case .changed: [("M", .yellow)]
        case .removed: [("D", .red)]
        case .mainChanged: [("↓", .blue)]
        case .changedBoth: [("M", .yellow), ("↓", .blue)]
        case .inside(let tint): [("●", tint)]
        }
    }

    var text: String { letters.map(\.text).joined() }

    /// The tint of the thing's name: what the branch did before what main did.
    var tint: Tint { letters[0].tint }

    /// Whether it is a thing's own mark, which its name takes on, rather than what is inside it.
    var isOwn: Bool {
        if case .inside = self { return false }
        return true
    }

    /// What it means, as the tooltip says it.
    var meaning: String {
        switch self {
        case .made: L10n[.Model.markMeaningMade]
        case .changed: L10n[.Model.markMeaningChanged]
        case .removed: L10n[.Model.markMeaningRemoved]
        case .mainChanged: L10n[.Model.markMeaningMainChanged]
        case .changedBoth: L10n[.Model.markMeaningChangedBoth]
        case .inside(.green): L10n[.Model.markInsideMade]
        case .inside(.red): L10n[.Model.markInsideRemoved]
        case .inside(.yellow): L10n[.Model.markInsideChanged]
        case .inside(.blue): L10n[.Model.markInsideMainChanged]
        }
    }

    /// What changed inside something, gathered from the marks of what it holds.
    struct Inside: OptionSet, Hashable, Sendable {
        let rawValue: Int
        static let added = Inside(rawValue: 1)
        static let changed = Inside(rawValue: 2)
        static let removed = Inside(rawValue: 4)
        static let main = Inside(rawValue: 8)

        init(rawValue: Int) { self.rawValue = rawValue }

        /// What one mark inside says.
        init(_ mark: ChangeMark) {
            switch mark {
            case .made, .inside(.green): self = .added
            case .changed, .inside(.yellow): self = .changed
            case .removed, .inside(.red): self = .removed
            case .mainChanged, .inside(.blue): self = .main
            case .changedBoth: self = [.changed, .main]
            }
        }

        /// The ● it makes: green when the branch only added, red when it only removed, yellow for
        /// any other change of the branch's, blue when only main changed; none when nothing did.
        var mark: ChangeMark? {
            let branch = intersection([.added, .changed, .removed])
            if branch.isEmpty { return contains(.main) ? .inside(.blue) : nil }
            if branch == .added { return .inside(.green) }
            if branch == .removed { return .inside(.red) }
            return .inside(.yellow)
        }
    }
}

/// Putting a branch right in one go (`specboard://SpecBoard/Docs/Design:DesignBranches` §5): the window's
/// "まとめて直す", or an MCP batch. While one side holds a branch, the other side's
/// writes to it are refused where the commit is made (`History.flush`); the same side's
/// writes, and writes to other branches, go through.
struct Hold: Hashable, Sendable {
    var projectId: String
    /// nil for main.
    var branchId: String?
    /// `screen` or `mcp`.
    var holder: String
    /// The batch's token, for MCP; empty for the window.
    var token: String
    var since: String
    /// The window's: the last commit before it began, which "やめる" goes back to, and
    /// the commit its writes are added to.
    var fromSeq: Int
    var commitId: String?
    /// The window's: the Mac it is on, as the agent's core was told it (`CoreClient.client`);
    /// empty for the app that holds the store itself.
    var client: String = ""

    var byMCP: Bool { holder == "mcp" }

    /// When it began, as the banner says it.
    var sinceLabel: String {
        guard let date = ISO8601DateFormatter.fractional.date(from: since) else { return since }
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: Language.current)
        formatter.setLocalizedDateFormatFromTemplate(Calendar.current.isDateInToday(date) ? "jmm" : "Mdjmm")
        return formatter.string(from: date)
    }
}

/// A merge from one branch into another — main among them (`specboard://SpecBoard/Docs/Design:DesignBranches`
/// §6): what the source changed, compared unit by unit with what the target holds
/// (`Merge3`), and written into the target the way the target keeps it.
///
/// Read and written on the documents themselves, apart from what is spread out; what
/// is spread out is let go after, and spread out again on the next read.
struct MergePreview {
    enum Change: String { case add, change, remove }

    struct Item {
        var path: String
        var section: Bool
        var change: Change
    }

    struct Conflict {
        /// The card's or the section's path, as the target names it.
        var path: String
        var section: Bool
        var conflict: Merge3.Conflict
    }

    /// The source and the target by name; `main` for main.
    var source: String
    var target: String
    /// What goes into the target.
    var items: [Item] = []
    /// What is left to choose.
    var conflicts: [Conflict] = []
    /// Paths where the target would hold two different things: a merge is refused
    /// until one is renamed.
    var overlaps: [String] = []
    /// How many more boxes the source moved would change the target, were they taken along
    /// (`places`; `specboard://SpecBoard/Docs/Design:DesignBranches` §6), than go in without: a card the merge brings
    /// that the target places nowhere goes in either way, and is not counted. None from main.
    var places = 0

    /// Nothing of the cards and sections goes in (the boxes are apart: `places`).
    var isEmpty: Bool { items.isEmpty && conflicts.isEmpty && overlaps.isEmpty }
}

/// What a merge wrote.
struct MergeResult {
    var preview: MergePreview
    /// Paths the merge left unresolved in the target.
    var warnings: [String]
}

/// How MCP stands with one of what it may do (a project's items, `mcp_writes`; a key's,
/// `AccessKey.permits`): let through, asked of a person each time (the call says `request: true`), or
/// turned away.
nonisolated enum MCPState: String, Decodable, Sendable, CaseIterable, Identifiable {
    case allow, ask, deny

    var id: String { rawValue }
}

/// A key as the window lists it.
nonisolated struct AccessKey: Equatable, Sendable, Identifiable {
    var id: String
    var name: String
    /// Its items (`app_control`, `app_control.capture`, `project_create`) and how it stands with
    /// each: the window, where any project can show, apart from the projects it was given.
    var permits: [String: MCPState]
    var createdAt: String
    var usedAt: String?

    func state(_ permit: CoreClient.Permit) -> MCPState { permits[permit.rawValue] ?? permit.byDefault }
}

/// What MCP may do in a project, one group of it as the project's settings show it
/// (`mcp_writes`, `specboard://SpecBoard/Docs/Design:DesignAccess` §6): the group's choice sets every item of it.
nonisolated struct MCPWriteGroup: Decodable, Equatable, Sendable, Identifiable {
    var id: String
    var name: String
    /// What it covers, where its name does not say it all.
    var about: String
    var items: [MCPWriteItem]

    /// The state every item of it is in; nil while they differ.
    var state: MCPState? {
        let states = Set(items.map(\.state))
        return states.count == 1 ? states.first : nil
    }
}

nonisolated struct MCPWriteItem: Decodable, Equatable, Sendable, Identifiable {
    /// As the core and MCP name it: the tool's name, or `<tool>.<part>` (`project_update.rules`).
    var id: String
    /// What it does, as the window shows it beside the id.
    var name: String
    var about: String
    var state: MCPState
    /// As it is until a person says otherwise.
    var byDefault: MCPState

    /// A part of a tool, shown under it.
    var isPart: Bool { id.contains(".") }
}

nonisolated enum AccessLevel: String, Sendable, Comparable {
    case read, write

    static func < (a: Self, b: Self) -> Bool { a == .read && b == .write }
}

/// Who is asking, as the MCP entrance found them by their key.
nonisolated struct Caller: Equatable, Sendable {
    var keyId: String
    var name: String
}

/// The stop for a history no one cuts (`prune`, `specboard://SpecBoard/Docs/Design:DesignBranches` §7): per branch,
/// the commits past the newest `keptCommits` that are also older than `keptDays`.
enum HistoryStop {
    static let keptCommits = 1_000
    static let keptDays = 90
}

extension ProjectSection {
    /// One of the root's own sections: directly under it — the project's (`project_tree`) and
    /// `_evacuation_` — made, taken away and put in order by the project alone.
    static func isRootOwn(_ section: ProjectSection, root: String?) -> Bool {
        section.parentId != nil && section.parentId == root
    }
}

extension ISO8601DateFormatter {
    /// The shape the core writes its times in (with fractions of a second).
    static let fractional: ISO8601DateFormatter = {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return formatter
    }()
}

/// One unit that undoing a commit would write over a later commit's change: the card's or section's
/// path, the unit as a person names it, and its value before the commit, right after it and now
/// (none: it was not there).
struct UndoConflict: Identifiable {
    let id = UUID()
    var path: String
    var unit: String
    var before: OJSON?
    var after: OJSON?
    var now: OJSON?
}

/// An undo of one commit that wrote nothing, for the later commits changed what it changed too.
struct UndoRefusal: Identifiable {
    let id = UUID()
    var seq: Int
    var conflicts: [UndoConflict]
    /// The later commits that wrote those places.
    var later: [Int]
}
