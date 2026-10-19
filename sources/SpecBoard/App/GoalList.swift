import SwiftUI

/// One of the board's goals as `card_list` lists it (`kind: goal`): its path, its name,
/// its fields, and its progress as the core counts it — the list MCP reads, the same one.
nonisolated struct BoardGoal: Decodable, Hashable, Sendable {
    var path: String
    var label: String?
    var status: String?
    var priority: String?
    var due: String?
    var progress: Progress?

    struct Progress: Decodable, Hashable, Sendable {
        var done: Int
        var of: Int
    }

    /// Its name as the list shows it: its label, or the name its path ends with.
    var name: String {
        if let label, !label.isEmpty { return label }
        return PathText.lastName(path)
    }
}

extension AppModel {
    /// The board's goals in the order their labels number them (`card_list`'s `kind: goal`):
    /// the goals of its sections — not those inside a goal (`Goals:Ship.Sub`, a name after the
    /// one holding it), which count in its progress.
    func boardGoals() -> [BoardGoal] {
        struct Listed: Decodable { var cards: [BoardGoal] }
        guard isBoardOpen,
              case .object(let wrapped)? = try? tool("card_list", value: ["kind": "goal", "limit": 200]),
              let json = wrapped["json"],
              let listed = try? JSONDecoder().decode(Listed.self, from: Data(json.compact().utf8)) else { return [] }
        return listed.cards.filter { goal in PathText.dots(PathText.splitSection(goal.path).rest).count == 1 }
    }
}

/// The board's head: its goals by their status, each with its progress, priority and
/// due date — the ones finished folded away — each, pressed, shown on its section's diagram
/// (its card map opens on a press twice there, or on the nav panel).
///
/// A board with more than one section of goals directly under its root (`project_tree`'s
/// `board_sections`) has a tab for each: its goals, the ones of the sections
/// inside it too. One alone, no tabs.
struct GoalList: View {
    var model: AppModel
    var goals: [BoardGoal]

    /// The groups of finished goals opened by hand: they start folded.
    @State private var unfolded: Set<String> = []
    /// The tab picked, by its section's name; the first when none is.
    @State private var tab: String?

    /// The board's sections of goals directly under its root, in the project's order.
    static func tabs(_ detail: ProjectDetail?) -> [String] {
        guard let detail else { return [] }
        let directory = detail.directory
        return directory.children(of: directory.home).filter { $0.kind == .board }.map(\.name)
    }

    /// The section directly under the root a goal is in, by its path (`Goals/Complete:Ship` →
    /// `Goals`), its name as it is.
    static func top(of path: String) -> String {
        let (section, rest) = PathText.splitSection(path)
        return PathText.unquote(PathText.slashes(section ?? rest)[0])
    }

    var body: some View {
        let tabs = Self.tabs(model.detail)
        let picked = tab.flatMap { tabs.contains($0) ? $0 : nil } ?? tabs.first
        let shown = tabs.count > 1 ? goals.filter { Self.top(of: $0.path) == picked } : goals
        if tabs.count > 1 {
            Picker("", selection: Binding(get: { picked ?? "" }, set: { tab = $0 })) {
                ForEach(tabs, id: \.self) { Text($0).tag($0) }
            }
            .labelsHidden()
            .pickerStyle(.segmented)
            .fixedSize()
            .help(L10n[.Board.listTabsHelp])
        }
        if shown.isEmpty {
            Text(L10n[.Board.listEmpty])
                .font(.app(12)).foregroundStyle(.secondary)
        }
        ForEach(Self.groups(shown, status: model.detail?.cardKinds["goal"]?.fields.first { $0.name == "status" }),
                id: \.status) { group in
            if group.finished {
                DisclosureGroup(isExpanded: Binding(get: { unfolded.contains(group.status) },
                                                    set: { open in
                                                        if open { unfolded.insert(group.status) } else { unfolded.remove(group.status) }
                                                    })) {
                    rows(group.goals).padding(.top, 4)
                } label: {
                    title(group)
                }
            } else {
                VStack(alignment: .leading, spacing: 6) {
                    title(group)
                    rows(group.goals)
                }
            }
        }
    }

    struct Group: Equatable {
        var status: String
        /// The status as its definition shows it (`optionLabels`), the status kept being the value.
        var shown: String
        /// Finished (完了・打ち切り): folded until opened.
        var finished: Bool
        var goals: [String]
    }

    /// The goals by status (as paths), in the order the goal kind's `status` field gives its
    /// options, each group in the order the goals came; a status it does not have, after. A
    /// goal with none written has the field's default. Finished are the options from the first
    /// one counted as done on (完了・打ち切り): the definition's word, not the window's.
    static func groups(_ goals: [BoardGoal], status field: KindField?) -> [Group] {
        let all = field?.options ?? []
        let done = (field?.done ?? []).compactMap { value -> String? in
            if case .string(let word) = value { return word }
            return nil
        }
        let finished = Set(all[(all.firstIndex { done.contains($0) } ?? all.endIndex)...])
        let unset: String? = field?.defaultValue.flatMap { if case .string(let word) = $0 { word } else { nil } }
        let by = Dictionary(grouping: goals) { $0.status ?? unset ?? all.first ?? L10n[.Board.listNoStatus] }
        let order = all + by.keys.filter { !all.contains($0) }.sorted()
        return order.compactMap { status in
            by[status].map {
                Group(status: status, shown: field?.shown(status) ?? status, finished: finished.contains(status),
                      goals: $0.map(\.path))
            }
        }
    }

    private func title(_ group: Group) -> some View {
        HStack(spacing: 6) {
            Text(group.shown).font(.app(11, weight: .semibold)).foregroundStyle(.secondary)
            Text("\(group.goals.count)").font(.app(10.5, design: .monospaced)).foregroundStyle(.tertiary)
        }
    }

    private func rows(_ paths: [String]) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            ForEach(paths, id: \.self) { path in
                if let goal = goals.first(where: { $0.path == path }) { row(goal) }
            }
        }
    }

    private func row(_ goal: BoardGoal) -> some View {
        Button {
            model.show(path: goal.path)
        } label: {
            HStack(spacing: 12) {
                Text(goal.name)
                    .font(.app(12.5))
                    .lineLimit(1).truncationMode(.tail)
                    .frame(maxWidth: .infinity, alignment: .leading)
                // As the definition shows the value kept (`高` is High in English): room for longer words.
                Text(goal.priority.map { model.detail?.cardKinds["goal"]?.field("priority")?.shown($0) ?? $0 } ?? "")
                    .font(.app(10.5)).foregroundStyle(.secondary)
                    .fixedSize()
                    .frame(minWidth: 24)
                    .help(L10n[.Board.listPriority])
                progress(goal.progress)
                    .frame(width: 120, alignment: .leading)
                Text(goal.due ?? "")
                    .font(.app(11, design: .monospaced)).foregroundStyle(.secondary)
                    .frame(width: 84, alignment: .trailing)
                    .help(L10n[.Board.listDue])
            }
            .padding(.vertical, 4).padding(.horizontal, 6)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help(goal.path)
    }

    @ViewBuilder
    private func progress(_ progress: BoardGoal.Progress?) -> some View {
        if let progress, progress.of > 0 {
            HStack(spacing: 6) {
                ProgressView(value: Double(progress.done), total: Double(progress.of))
                    .progressViewStyle(.linear)
                    .frame(width: 64)
                Text("\(progress.done)/\(progress.of)")
                    .font(.app(11, design: .monospaced)).foregroundStyle(.secondary)
            }
            .help(L10n[.Board.listProgressHelp])
        } else {
            Text("—").font(.app(11)).foregroundStyle(.tertiary)
        }
    }
}
