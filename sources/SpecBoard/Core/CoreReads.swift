import CoreGraphics
import Foundation
import os

/// What the window reads from the core, as the window's own types (`specboard://SpecBoard/Docs/Design:CoreApi`).
/// A tree is named as the core names it: nil for main, a branch's id, `Board.id` for the
/// board.
extension CoreClient {

    // MARK: - Projects

    /// The projects it keeps, each marked with where (`place`).
    func projects() throws -> [ProjectSummary] {
        try request(Listed<ProjectRow>.self, "projects").items(\.projects).map {
            var project = $0.summary
            project.place = place
            return project
        }
    }

    func publicProjects() throws -> Set<String> {
        Set(try request(Listed<String>.self, "project_public").items(\.projects))
    }

    /// The project as the tree reads it, with the working directories' folders and files in it (`joined`).
    @MainActor
    func detail(_ project: String, branch: String?) throws -> ProjectDetail {
        let reply = try detailReply(project, branch: branch)
        guard case .whole(let whole) = try Self.reading(reply) else { throw Refusal.unreadable(reply) }
        return joined(whole, project, tree: branch ?? "")
    }

    /// What the core says of the project as the tree reads it, not yet read (`detail(reading:)`): from
    /// any thread.
    nonisolated func detailReply(_ project: String, branch: String?) throws -> String {
        try reply("project_detail", ["project": project, "branch": branch ?? ""])
    }

    /// The project read from what the core said of it (`detailReply`).
    @MainActor
    func detail(reading reply: String) throws -> ProjectDetail {
        try Self.detail(reading: reply)
    }

    /// The project read from what the core said of it (`detailReply`), in one pass and on any thread: a big
    /// project is read off the main thread while the window goes on drawing. How long it took is said in
    /// the log beside the requests (`log`).
    nonisolated static func detail(reading reply: String) throws -> ProjectDetail {
        let started = ContinuousClock.now
        guard case .whole(let whole) = try reading(reply) else { throw Refusal.unreadable(reply) }
        let detail = whole.detail
        log.notice("read project_detail \(seconds(since: started), format: .fixed(precision: 3), privacy: .public)s \(reply.utf8.count, privacy: .public)B \(detail.schemas.count, privacy: .public) cards")
        return detail
    }

    nonisolated func layout(_ project: String) throws -> LayoutSettings {
        let read = try request(LayoutRow.self, "layout", ["project": project])
        return LayoutSettings(size: CardSize(rows: read.size.rows, width: read.size.width),
                              margin: read.margin, docView: read.docView)
    }

    nonisolated func writingRules(_ project: String) throws -> [String: String] {
        try request(RulesRow.self, "writing_rules", ["project": project]).rules
    }

    func projectSize(_ project: String) throws -> ProjectSize {
        let read = try request(SizeRow.self, "project_size", ["project": project])
        return ProjectSize(content: read.content,
                        branches: read.branches.map { ProjectSize.Branch(id: $0.id, name: $0.name, history: $0.history) })
    }

    // MARK: - Branches

    nonisolated func branches(_ project: String) throws -> [BranchInfo] {
        try request(Listed<BranchRow>.self, "branches", ["project": project]).items(\.branches)
            .map { BranchInfo(id: $0.id, projectId: $0.projectId, name: $0.name, sortOrder: $0.sortOrder, createdAt: $0.createdAt) }
    }

    /// What a branch marks (`branch_marks`); what it removed read as the project's cards are, with the
    /// project's kinds (`kinds`).
    nonisolated func branchMarks(_ project: String, branch: String?, kinds: CardKinds = CardKinds()) throws -> BranchMarks {
        guard let branch, branch != Board.id else { return BranchMarks() }
        let read = try request(MarksRow.self, "branch_marks", ["project": project, "branch": branch])
        let gone = read.gone ?? GoneRow()
        let sections = gone.sections.map(\.section)
        return BranchMarks(changed: Set(read.changed), made: Set(read.made), removed: Set(read.removed ?? []),
                           mainChanged: Set(read.mainChanged),
                           sectionsChanged: Set(read.sectionsChanged), sectionsMade: Set(read.sectionsMade),
                           sectionsRemoved: Set(read.sectionsRemoved ?? []),
                           gone: BranchMarks.Gone(cards: WholeCard.shown(gone.schemas, inner: gone.inner, ghosts: gone.ghosts,
                                                                         sections: sections, kinds: kinds),
                                                  ghosts: gone.ghosts, nodes: gone.inner, sections: sections),
                           mainMoved: read.mainMoved ?? 0)
    }

    /// Section → what stands there for another section → its cell: other sections' cards by
    /// id, section ghosts by their id on the diagram (`SectionGhosts.prefix` and the section).
    nonisolated func sectionPositions(_ project: String, branch: String?) throws -> [String: [String: CGPoint]] {
        let read = try request(PositionsRow.self, "section_positions", ["project": project, "branch": branch ?? ""])
        var out = read.positions.mapValues { $0.mapValues { CGPoint(x: $0.x, y: $0.y) } }
        for (view, ghosts) in read.sectionGhosts ?? [:] {
            for (section, at) in ghosts {
                out[view, default: [:]][SectionGhosts.prefix + section] = CGPoint(x: at.x, y: at.y)
            }
        }
        return out
    }

    // MARK: - History

    nonisolated func commits(_ project: String, branch: String?, limit: Int = 200) throws -> [CommitInfo] {
        try request(Listed<CommitRow>.self, "commits", ["project": project, "branch": branch ?? "", "limit": limit])
            .items(\.commits).map { $0.info(project: project, branch: branch) }
    }

    nonisolated func holds(_ project: String, branch: String?) throws -> [Hold] {
        try request(Listed<HoldRow>.self, "holds", ["project": project, "branch": branch ?? ""]).items(\.holds)
            .map { Hold(projectId: project, branchId: branch, holder: $0.holder, token: $0.token, since: $0.since,
                        fromSeq: $0.fromSeq, commitId: nil, client: $0.client ?? "") }
    }

    func commitChanges(_ project: String, branch: String?, seq: Int) throws -> [CommitChange] {
        try request(Listed<ChangeRow>.self, "commit_changes", ["project": project, "branch": branch ?? "", "seq": seq])
            .items(\.changes)
            .map { CommitChange(path: $0.path, change: CommitChange.Change(rawValue: $0.change) ?? .change,
                                section: $0.section, details: $0.details) }
    }

    func cutCost(_ project: String, branch: String?, before seq: Int) throws -> (commits: Int, bytes: Int) {
        let read = try request(CostRow.self, "cut_cost", ["project": project, "branch": branch ?? "", "before": seq])
        return (read.commits, read.bytes)
    }

    // MARK: - Merges and requests

    nonisolated func mergeRequests(_ project: String) throws -> [MergeRequest] {
        try request(Listed<RequestRow>.self, "merge_requests", ["project": project]).items(\.requests).map(\.request)
    }

    func mergePreview(_ project: String, source: String?, target: String?, take: MergeTake) throws -> MergePreview {
        try previewed("merge_preview", Self.merging(project, source, target, take))
    }

    /// The merge, written. The core says nothing more than what it merged. With `places`,
    /// every box the source branch moved goes in over the target's (`specboard://SpecBoard/Docs/Design:DesignBranches` §6).
    /// A branch merged into main is set aside, unless `keep`.
    func merge(_ project: String, source: String?, target: String?, take: MergeTake, places: Bool = false,
               keep: Bool = false) throws -> MergeResult {
        var fields = Self.merging(project, source, target, take)
        fields["places"] = places
        fields["keep"] = keep
        return MergeResult(preview: try previewed("merge", fields), warnings: [])
    }

    /// A request approved: merged into main as it was tried, the boxes the branch moved
    /// along when `places` says so (nil: as the request asks).
    func approveMergeRequest(_ project: String, number: Int, places: Bool?) throws {
        var fields: [String: Any] = ["project": project, "number": number]
        if let places { fields["places"] = places }
        _ = try request("merge_request_approve", fields)
    }

    /// What a merge into main would leave, as a branch of the core's to open
    /// (`merge_result_open`): its id, and the merge.
    func openMergeResult(_ project: String, source: String, take: MergeTake, places: Bool) throws -> (branch: String, preview: MergePreview) {
        var fields = Self.merging(project, source, nil, take)
        fields["places"] = places
        return try opened("merge_result_open", fields)
    }

    /// A merge request's, as it was asked for (`merge_request_open`); `places` as the window has
    /// it, nil as it asks.
    func openMergeRequestResult(_ project: String, number: Int, places: Bool?) throws -> (branch: String, preview: MergePreview) {
        var fields: [String: Any] = ["project": project, "number": number]
        if let places { fields["places"] = places }
        return try opened("merge_request_open", fields)
    }

    /// What undoing one commit alone came to (`undo_commit`): written — on main as the branch made for
    /// it —, or nothing written for the places a later commit changed too.
    enum Undid {
        case done(branch: String?)
        case clashed([UndoConflict], later: [Int])
    }

    /// One commit of a tree taken back alone. The units are read in their own order, as the merge
    /// window shows a conflict's values.
    func undoCommit(_ project: String, branch: String?, seq: Int) throws -> Undid {
        let answer = try ordered("undo_commit", ["project": project, "branch": branch ?? "", "seq": seq])
        guard case .object(let body) = answer else { throw Refusal.unreadable(answer.compact()) }
        guard case .array(let rows)? = body["conflicts"] else {
            if case .string(let made)? = body["branch"] { return .done(branch: made) }
            return .done(branch: nil)
        }
        func text(_ sides: OrderedObject, _ key: String) -> String {
            if case .string(let value)? = sides[key] { return value }
            return ""
        }
        func side(_ sides: OrderedObject, _ key: String) -> OJSON? {
            guard let value = sides[key] else { return nil }
            if case .null = value { return nil }
            return value
        }
        let conflicts: [UndoConflict] = rows.compactMap { row in
            guard case .object(let sides) = row else { return nil }
            return UndoConflict(path: text(sides, "path"), unit: text(sides, "unit"), before: side(sides, "before"),
                                after: side(sides, "after"), now: side(sides, "now"))
        }
        var later: [Int] = []
        if case .array(let numbers)? = body["later"] {
            later = numbers.compactMap { if case .number(let number) = $0 { Int(number) } else { nil } }
        }
        return .clashed(conflicts, later: later)
    }

    /// The branch made to show a merge's result, let go (`merge_result_close`).
    func closeMergeResult(_ project: String) throws {
        _ = try request("merge_result_close", ["project": project])
    }

    private func opened(_ op: String, _ fields: [String: Any]) throws -> (branch: String, preview: MergePreview) {
        let answer = try ordered(op, fields)
        guard case .object(let body) = answer, case .string(let branch)? = body["branch"] else {
            throw Refusal(L10n.text(.Model.errorNoBranchInAnswer(op), in: Language.current))
        }
        return (branch, try preview(of: answer))
    }

    /// A merge's answer: its conflicts' values read in their own order, as the window shows them.
    private func previewed(_ op: String, _ fields: [String: Any]) throws -> MergePreview {
        try preview(of: try ordered(op, fields))
    }

    private func preview(of answer: OJSON) throws -> MergePreview {
        var preview = try JSONDecoder().decode(PreviewRow.self, from: Data(answer.compact().utf8)).preview
        guard case .object(let body) = answer, case .array(let conflicts)? = body["conflicts"] else { return preview }
        for (index, conflict) in conflicts.enumerated() where index < preview.conflicts.count {
            guard case .object(let sides) = conflict else { continue }
            func side(_ key: String) -> OJSON? {
                guard let value = sides[key] else { return nil }
                if case .null = value { return nil }
                return value
            }
            preview.conflicts[index].conflict.base = side("base")
            preview.conflicts[index].conflict.source = side("source")
            preview.conflicts[index].conflict.target = side("target")
        }
        return preview
    }

    /// Which side each conflict takes: by path, then by unit.
    typealias MergeTake = [String: [String: Merge3.Side]]

    private static func merging(_ project: String, _ source: String?, _ target: String?, _ take: MergeTake) -> [String: Any] {
        ["project": project, "source": source ?? "", "target": target ?? "",
         "take": take.mapValues { $0.mapValues(\.rawValue) }]
    }
}

// MARK: - The answers' rows

/// An answer that is one list under a key.
nonisolated private struct Listed<Row: Decodable>: Decodable {
    var projects: [Row]?
    var branches: [Row]?
    var dirs: [Row]?
    var commits: [Row]?
    var holds: [Row]?
    var changes: [Row]?
    var requests: [Row]?

    func items(_ key: KeyPath<Listed, [Row]?>) -> [Row] { self[keyPath: key] ?? [] }
}

nonisolated private struct ProjectRow: Decodable {
    var id, name, slug, description, createdAt, updatedAt: String
    var tagGroups: [TagGroup]
    var schemaCount, sectionCount: Int
    var archivedAt, lockedAt: String?
    var aiMerge: Bool?

    var summary: ProjectSummary {
        ProjectSummary(id: id, name: name, slug: slug, description: description, tagGroups: tagGroups,
                    createdAt: createdAt, updatedAt: updatedAt, schemaCount: schemaCount,
                    sectionCount: sectionCount, archivedAt: archivedAt, lockedAt: lockedAt,
                    aiMerge: aiMerge ?? false)
    }
}

nonisolated private struct LayoutRow: Decodable {
    struct Size: Decodable { var rows: Int; var width: Int? }
    var size: Size
    var margin: Int
    var docView: String
}

nonisolated private struct RulesRow: Decodable { var rules: [String: String] }

nonisolated private struct SizeRow: Decodable {
    struct Branch: Decodable { var id: String?; var name: String; var history: Int }
    var content: Int
    var branches: [Branch]
}

nonisolated private struct BranchRow: Decodable {
    var id, projectId, name, createdAt: String
    var sortOrder: Int
}

nonisolated private struct MarksRow: Decodable {
    var changed, made, mainChanged, sectionsChanged, sectionsMade: [String]
    var removed, sectionsRemoved: [String]?
    var gone: GoneRow?
    var mainMoved: Int?
}

/// What a branch removed, in `project_detail`'s parts (`branch_marks`'s `gone`).
nonisolated private struct GoneRow: Decodable {
    var schemas: [WholeCard] = []
    var ghosts: [CardGhost] = []
    var inner: [InnerCard] = []
    var sections: [WholeSection] = []

    private enum Keys: String, CodingKey { case schemas, ghosts, inner, sections }

    init() {}

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        schemas = try c.decodeIfPresent([WholeCard].self, forKey: .schemas) ?? []
        ghosts = try c.decodeIfPresent([CardGhost].self, forKey: .ghosts) ?? []
        inner = try c.decodeIfPresent([InnerCard].self, forKey: .inner) ?? []
        sections = try c.decodeIfPresent([WholeSection].self, forKey: .sections) ?? []
    }
}

nonisolated private struct PositionsRow: Decodable {
    struct Point: Decodable { var x, y: Double }
    var positions: [String: [String: Point]]
    var sectionGhosts: [String: [String: Point]]?
}

nonisolated private struct CommitRow: Decodable {
    struct Line: Decodable {
        var kind: String
        var targetType: String
        var targetId: String?
        var summary: String
    }
    var seq: Int
    var at: String
    var authorKind: String
    var author: String?
    var kind: String
    var name: String?
    var reverted: Bool
    var lines: [Line]

    /// The core names a commit by its tree and number; so does the window.
    func info(project: String, branch: String?) -> CommitInfo {
        let id = "\(branch ?? "").\(seq)"
        return CommitInfo(id: id, seq: seq, at: at, authorKind: authorKind, author: author, kind: kind, name: name,
                          reverted: reverted,
                          lines: lines.enumerated().map { index, line in
                              ChangeEvent(id: "\(id).\(index)", projectId: project, at: at, kind: line.kind,
                                          targetType: ChangeEvent.TargetType(rawValue: line.targetType) ?? .project,
                                          targetId: line.targetId, summary: line.summary)
                          })
    }
}

nonisolated private struct HoldRow: Decodable {
    var holder, token, since: String
    var fromSeq: Int
    var client: String?
}

nonisolated private struct ChangeRow: Decodable {
    var path, change: String
    var section: Bool
    var details: [String]
}

nonisolated private struct CostRow: Decodable { var commits, bytes: Int }

nonisolated private struct RequestRow: Decodable {
    var number: Int
    var branch, branchName, title, description, at, state: String
    var author, reason: String?
    var result: MergeRequest.Result?
    var places: Bool?

    var request: MergeRequest {
        MergeRequest(id: "\(number)", number: number, branchId: branch, branchName: branchName, title: title,
                    description: description, author: author, at: at,
                    state: MergeRequest.State(rawValue: state) ?? .open, reason: reason, decidedAt: nil,
                    mergeSeq: nil, result: result ?? MergeRequest.Result(), places: places ?? false)
    }
}

nonisolated private struct PreviewRow: Decodable {
    struct Item: Decodable { var path: String; var section: Bool; var change: String }
    struct Conflict: Decodable {
        var path: String
        var section: Bool
        var unit: String
        var label: String
    }
    var source: String
    var target: String
    var items: [Item]
    var conflicts: [Conflict]
    var overlaps: [String]
    var places: Int?

    var preview: MergePreview {
        MergePreview(source: source, target: target,
                     items: items.map { MergePreview.Item(path: $0.path, section: $0.section,
                                                          change: MergePreview.Change(rawValue: $0.change) ?? .change) },
                     conflicts: conflicts.map {
                         MergePreview.Conflict(path: $0.path, section: $0.section,
                                               conflict: Merge3.Conflict(unit: $0.unit, label: $0.label,
                                                                         base: nil, source: nil, target: nil))
                     },
                     overlaps: overlaps, places: places ?? 0)
    }
}
