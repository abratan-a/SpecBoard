import Foundation

/// One tree's inspection as the core lists it (`inspection_list` with `show: all`,
/// `specboard://SpecBoard/Docs/Design:CoreApi`): every item, the ones marked as by design among them, in the
/// core's order. The words — a group's label, what is wrong — are the core's: the window says nothing of
/// its own about what it found, so that a person and MCP read the same thing.
nonisolated struct Inspection: Sendable, Equatable {
    /// Whether a group is something to put right or something only to know.
    enum Kind: String, Sendable {
        case fix, info
    }

    /// Why an item was marked as by design, by whom and when.
    struct Mark: Sendable, Equatable {
        var reason: String
        var by: String
        var at: String

        init?(_ json: Any?) {
            guard let json = json as? [String: Any], let reason = json["reason"] as? String else { return nil }
            self.reason = reason
            by = json["by"] as? String ?? ""
            at = json["at"] as? String ?? ""
        }
    }

    struct Group: Sendable, Equatable, Identifiable {
        /// The group's name, as the core names it (`brokenPaths`).
        var id: String
        var kind: Kind
        var label: String
        /// The items not marked.
        var count: Int
        /// The items marked as by design.
        var accepted: Int
    }

    struct Item: Sendable, Equatable, Identifiable {
        /// The same for the same item in every tree and every reading: what a mark is kept by.
        var id: String
        var group: String
        var at: String
        var problem: String
        var hint: String?
        /// The mark it has, when it is marked as by design.
        var accepted: Mark?
        /// A mark it had before what it is about changed: it shows again, with what was said then.
        var acceptedBefore: Mark?
        /// Its kind, when it is not its group's: what was found in a card done (a goal finished, a task
        /// done) is a record, to know rather than to put right.
        var kind: Kind?

        /// Where a press takes the window: a flow's row is shown at its node, the window showing a
        /// node rather than one of its rows.
        var place: String {
            guard let (head, last) = PathText.lastDot(at), last.hasPrefix("steps[") else { return at }
            return head
        }
    }

    /// The groups that have items, in the core's order (what to put right first).
    var groups: [Group]
    var items: [Item]
    /// Each part's groups with their items, worked out once for the list rather than on each drawing.
    private(set) var parts: [Part: [Section]] = [:]

    /// A group as one part of the page has it: the items of the group that are in that part.
    struct Section: Sendable, Equatable {
        var group: Group
        var items: [Item]
    }

    init(groups: [Group], items: [Item]) {
        self.groups = groups
        self.items = items
        parts = Self.parts(groups, items)
    }

    /// From `inspection_list`'s answer. The groups are ordered as their items come: every group listed has
    /// at least one item, and the items come in the groups' order.
    init?(_ json: [String: Any]) {
        guard let groups = json["groups"] as? [String: [String: Any]], let rows = json["items"] as? [[String: Any]] else {
            return nil
        }
        let items: [Item] = rows.compactMap { row in
            guard let id = row["id"] as? String, let group = row["group"] as? String else { return nil }
            return Item(id: id, group: group, at: row["at"] as? String ?? "", problem: row["problem"] as? String ?? "",
                        hint: (row["hint"] as? String).flatMap { $0.isEmpty ? nil : $0 },
                        accepted: Mark(row["accepted"]), acceptedBefore: Mark(row["acceptedBefore"]),
                        kind: Kind(rawValue: row["kind"] as? String ?? ""))
        }
        var seen = Set<String>()
        let order = items.map(\.group).filter { seen.insert($0).inserted }
            + groups.keys.sorted().filter { !seen.contains($0) }
        self.init(groups: order.compactMap { name in
            guard let group = groups[name] else { return nil }
            return Group(id: name, kind: Kind(rawValue: group["kind"] as? String ?? "") ?? .info,
                         label: group["label"] as? String ?? name,
                         count: (group["count"] as? NSNumber)?.intValue ?? 0,
                         accepted: (group["accepted"] as? NSNumber)?.intValue ?? 0)
        }, items: items)
    }

    /// One of the page's three parts: what to put right, what only to know, and what was marked as by design.
    enum Part: String, CaseIterable, Sendable {
        case fix, info, accepted
    }

    /// A part's groups, each with its items: a marked item is under the group it is of, in the part of marked ones,
    /// and an item of a group to put right said as one only to know is under its group in the part of those.
    func part(_ part: Part) -> [Section] {
        parts[part] ?? []
    }

    /// How many items a part has.
    func count(_ part: Part) -> Int {
        self.part(part).reduce(0) { $0 + $1.items.count }
    }

    private static func parts(_ groups: [Group], _ items: [Item]) -> [Part: [Section]] {
        var byGroup: [String: [Item]] = [:]
        for item in items { byGroup[item.group, default: []].append(item) }
        var out: [Part: [Section]] = [:]
        for group in groups {
            let mine = byGroup[group.id] ?? []
            let open = mine.filter { $0.accepted == nil }, marked = mine.filter { $0.accepted != nil }
            for part in [Part.fix, .info] {
                let here = open.filter { ($0.kind ?? group.kind) == .fix ? part == .fix : part == .info }
                if !here.isEmpty { out[part, default: []].append(Section(group: group, items: here)) }
            }
            if !marked.isEmpty { out[.accepted, default: []].append(Section(group: group, items: marked)) }
        }
        return out
    }

    /// A reply of the core's to `inspection_list`, as it wrote it: the list, or why it was refused.
    static func read(_ reply: Result<String, Error>) -> Result<Inspection, InspectionRefusal> {
        switch reply {
        case .failure(let error):
            return .failure(InspectionRefusal(said: error.localizedDescription))
        case .success(let text):
            guard let answer = try? JSONSerialization.jsonObject(with: Data(text.utf8)) as? [String: Any] else {
                return .failure(InspectionRefusal(said: L10n.text(.Common.problemCoreUnreadable, in: Language.current)))
            }
            if let error = answer["error"] as? String { return .failure(InspectionRefusal(said: error)) }
            guard let result = answer["result"] as? [String: Any], let json = result["json"] as? [String: Any],
                  let list = Inspection(json) else {
                return .failure(InspectionRefusal(said: L10n.text(.Common.problemCoreUnreadable, in: Language.current)))
            }
            return .success(list)
        }
    }
}

/// Why the core would not list a tree's inspection: said where the list would be.
nonisolated struct InspectionRefusal: Error, Sendable, Equatable {
    var said: String
}
