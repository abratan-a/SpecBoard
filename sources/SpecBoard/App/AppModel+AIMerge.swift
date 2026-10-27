import Foundation

/// An AI's merges into a locked main let in without being approved: the project's switch, set in
/// its settings only, and what the core tells when an AI merges (`{"op": "merged", …}`), said above
/// the bubble at the bell — several of a project in one line — and kept in the bell's list.
extension AppModel {
    /// One merge an AI let into a locked main.
    struct AIMerge: Equatable {
        var project: String
        /// The branch merged, by its name.
        var branch: String
        /// The access key's name it came with.
        var by: String?
        var added, changed, removed: Int

        init(project: String, branch: String, by: String?, added: Int, changed: Int, removed: Int) {
            (self.project, self.branch, self.by) = (project, branch, by)
            (self.added, self.changed, self.removed) = (added, changed, removed)
        }

        /// Out of the core's notice; nil when it is not one.
        init?(_ told: [String: Any]) {
            guard let project = told["project"] as? String, let branch = told["branch"] as? String else { return nil }
            self.init(project: project, branch: branch, by: told["by"] as? String,
                      added: told["added"] as? Int ?? 0, changed: told["changed"] as? Int ?? 0,
                      removed: told["removed"] as? Int ?? 0)
        }
    }

    /// The merges told of, by project, in the order the projects were first told of.
    var aiMergesByProject: [(project: String, merges: [AIMerge])] {
        var order: [String] = []
        var by: [String: [AIMerge]] = [:]
        for merge in aiMerges {
            if by[merge.project] == nil { order.append(merge.project) }
            by[merge.project, default: []].append(merge)
        }
        return order.map { ($0, by[$0] ?? []) }
    }

    /// What the bubble says of a project's merges: one by who and which branch; several, how many,
    /// the last, and what they did together. The project named when it is not the one on screen.
    func aiMergeWords(_ merges: [AIMerge]) -> String {
        guard let last = merges.last else { return "" }
        return Self.aiMergeWords(merges, elsewhere: elsewhere(last.project), in: Language.shared.code)
    }

    /// The project a merge went into, when it is not the one on screen: by its name, or none when it is
    /// not known (`Branches.aiMergeOtherProject`); nil when it is the one on screen.
    private func elsewhere(_ project: String) -> String?? {
        guard project != detail?.project.id else { return nil }
        return .some(projects.first { $0.id == project }?.name)
    }

    /// `aiMergeWords` in `code`, the project named as `elsewhere` says.
    static func aiMergeWords(_ merges: [AIMerge], elsewhere: String??, in code: String) -> String {
        guard let last = merges.last else { return "" }
        let added = merges.reduce(0) { $0 + $1.added }
        let changed = merges.reduce(0) { $0 + $1.changed }
        let removed = merges.reduce(0) { $0 + $1.removed }
        let counts = L10n.text(.Branches.aiMergeCounts(added, changed, removed), in: code)
        let by = last.by ?? "MCP"
        let words = merges.count == 1
            ? L10n.text(.Branches.aiMergeOne(by, last.branch, counts), in: code)
            : L10n.text(.Branches.aiMergeMany(merges.count, by, last.branch, counts), in: code)
        guard case .some(let name) = elsewhere else { return words }
        return L10n.text(.Branches.aiMergeInProject(name ?? L10n.text(.Branches.aiMergeOtherProject, in: code), words),
                         in: code)
    }

    /// A merge told of: in the bubble, with the project's others, and in the bell's list on its own — the
    /// words as they are now, while the project's name is known, and said again so in another language.
    func tell(_ merge: AIMerge) {
        aiMerges.append(merge)
        let elsewhere = elsewhere(merge.project)
        noticeLog.add(.aiMerge, aiMergeWords([merge]), project: merge.project,
                      sayIn: { Self.aiMergeWords([merge], elsewhere: elsewhere, in: $0) })
    }

    /// A project's merges told of, let go.
    func dismissAIMerges(of project: String) {
        aiMerges.removeAll { $0.project == project }
    }

    /// Where a project's merges are read: its main, on the root's history; and they are let go.
    func showAIMergeHistory(of project: String) {
        dismissAIMerges(of: project)
        atHome = false
        if detail?.project.id != project { select(project) }
        guard detail?.project.id == project else { return }
        if openTree.id != nil, !isLockedGroupOpen { openBranch(nil) }
        guard let root = detail?.sections.first(where: { $0.parentId == nil })?.id else { return }
        open(.root(root, .history))
    }

    /// Whether the project on screen lets an AI's merges into its locked main without being approved.
    var isAIMergeOn: Bool {
        guard let id = detail?.project.id else { return false }
        return projects.first { $0.id == id }?.aiMerge ?? false
    }
}
