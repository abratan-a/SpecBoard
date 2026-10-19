import SwiftUI

/// The merge window (`specboard://SpecBoard/Docs/Design:DesignBranches` §6): from which branch into which, what
/// goes in, the conflicts to choose one side of, and the paths that would hold two
/// different things. Merged only when every conflict is chosen and no path overlaps.
/// From a branch, "branch で動かした位置も main に入れる" (off by default) takes every box
/// the branch moved along, over where the target places it.
struct MergeSheet: View {
    var model: AppModel
    @Environment(\.dismiss) private var dismiss
    /// Branch ids; empty for main.
    @State private var source: String
    @State private var target: String
    @State private var take: CoreClient.MergeTake = [:]
    /// The boxes the branch moved go in too (`places`).
    @State private var places: Bool
    /// The branch stays once merged into main, rather than set aside (`keep`).
    @State private var keep = false
    @State private var preview: MergePreview?
    @State private var refused: String?

    init(model: AppModel, request: AppModel.MergeAsk) {
        self.model = model
        _source = State(initialValue: request.source ?? "")
        _target = State(initialValue: request.target ?? "")
        _places = State(initialValue: request.places)
    }

    private var branches: [(id: String, name: String)] {
        [("", SchemaDirectory.mainWord)] + model.branches.map { ($0.id, $0.name) }
    }

    private func id(_ picked: String) -> String? { picked.isEmpty ? nil : picked }

    /// The boxes go along only from a branch: main taken in leaves the branch's where they are.
    private var takingPlaces: Bool { places && !source.isEmpty }

    /// Kept only from a branch into main: what goes elsewhere stays in any case.
    private var keeping: Bool { keep && !source.isEmpty && target.isEmpty }

    /// Every conflict chosen, nothing overlapping, and something to merge — the boxes
    /// the branch moved count when they go too.
    private var ready: Bool {
        guard let preview, source != target, preview.overlaps.isEmpty else { return false }
        guard !preview.isEmpty || (takingPlaces && preview.places > 0) else { return false }
        return preview.conflicts.allSatisfy { take[$0.path]?[$0.conflict.label] != nil }
    }

    /// The toggle's words: into main, as the owner named it; into a branch, the same.
    static func placesLabel(intoMain: Bool, count: Int?) -> String {
        guard let count else {
            return intoMain ? L10n[.Branches.mergePlacesToMain] : L10n[.Branches.mergePlacesToTarget]
        }
        if intoMain {
            return count > 0 ? L10n[.Branches.mergePlacesToMainCount(count)] : L10n[.Branches.mergePlacesToMainNone]
        }
        return count > 0 ? L10n[.Branches.mergePlacesToTargetCount(count)] : L10n[.Branches.mergePlacesToTargetNone]
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(L10n[.Branches.mergeMerge]).font(.app(15, weight: .semibold))
            HStack(spacing: 10) {
                picker(L10n[.Branches.mergeSource], $source)
                Image(systemName: "arrow.right").foregroundStyle(.secondary)
                picker(L10n[.Branches.mergeTarget], $target)
            }
            Text(source.isEmpty ? L10n[.Branches.mergeExplainMainIn] : L10n[.Branches.mergeExplainBranch])
                .font(.app(11)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
            if !source.isEmpty {
                Toggle(Self.placesLabel(intoMain: target.isEmpty, count: source == target ? nil : preview?.places),
                       isOn: $places)
                    .toggleStyle(.checkbox)
                    .font(.app(12))
                    .help(L10n[.Branches.mergePlacesHelp])
            }
            if !source.isEmpty && target.isEmpty {
                Toggle(L10n[.Branches.mergeKeepBranch], isOn: $keep)
                    .toggleStyle(.checkbox)
                    .font(.app(12))
                    .help(L10n[.Branches.mergeKeepHelp])
            }
            Divider()
            if source == target {
                Text(L10n[.Branches.mergeSameBranch]).font(.app(12)).foregroundStyle(.secondary)
            } else if let preview {
                ScrollView { content(preview) }
            }
            Spacer(minLength: 0)
            if let refused {
                Text(refused).font(.app(11)).foregroundStyle(.red).fixedSize(horizontal: false, vertical: true)
            }
            HStack {
                // What main would be once merged, looked at before it is.
                if target.isEmpty && !source.isEmpty {
                    Button(L10n[.Branches.mergeViewResult]) {
                        model.openMergeResult(source: source, take: take, places: takingPlaces, keep: keeping)
                        dismiss()
                    }
                    .disabled(!ready)
                    .help(L10n[.Branches.mergeViewResultHelp])
                }
                Spacer()
                Button(L10n[.Common.actionCancel]) { dismiss() }.keyboardShortcut(.cancelAction)
                Button(L10n[.Branches.mergeMerge]) { run() }.keyboardShortcut(.defaultAction).disabled(!ready)
            }
        }
        .padding(18)
        // Lists of paths and before/after values: the sheet is dragged larger, and never smaller than this.
        .frame(minWidth: 600, idealWidth: 600, maxWidth: .infinity, minHeight: 520, idealHeight: 520, maxHeight: .infinity)
        .onAppear(perform: reload)
        .onChange(of: source) { reload() }
        .onChange(of: target) { reload() }
    }

    private func picker(_ title: String, _ selection: Binding<String>) -> some View {
        Picker(title, selection: selection) {
            ForEach(branches, id: \.id) { branch in Text(branch.name).tag(branch.id) }
        }
        .frame(maxWidth: 240)
    }

    @ViewBuilder
    private func content(_ preview: MergePreview) -> some View {
        VStack(alignment: .leading, spacing: 14) {
            if preview.isEmpty && !(takingPlaces && preview.places > 0) {
                Text(L10n[.Branches.mergeNothing]).font(.app(12)).foregroundStyle(.secondary)
            }
            if !preview.overlaps.isEmpty {
                group(L10n[.Branches.mergeOverlapsTitle]) {
                    ForEach(preview.overlaps, id: \.self) { path in
                        Text(path).font(.app(11.5, design: .monospaced)).foregroundStyle(.red)
                    }
                }
            }
            if !preview.conflicts.isEmpty {
                group(L10n[.Branches.mergeConflictsTitle(preview.conflicts.count)]) {
                    ForEach(Array(preview.conflicts.enumerated()), id: \.offset) { _, conflict in
                        conflictRow(conflict)
                    }
                }
            }
            if !preview.items.isEmpty {
                group(L10n[.Branches.mergeItemsTitle(preview.items.count)]) {
                    ForEach(Array(preview.items.enumerated()), id: \.offset) { _, item in
                        HStack(spacing: 8) {
                            ChangeMarkText(mark: Self.mark(item.change)).frame(minWidth: 12, alignment: .leading)
                            Text(item.section ? L10n[.Branches.mergeItemSection(item.path)] : item.path)
                                .font(.app(11.5, design: .monospaced))
                        }
                    }
                }
            }
            if !source.isEmpty && preview.places > 0 {
                group(L10n[.Branches.mergePlacesTitle(preview.places)]) {
                    Text(takingPlaces
                         ? L10n[.Branches.mergePlacesTaking(preview.places)]
                         : L10n[.Branches.mergePlacesNotTaking(preview.places)])
                        .font(.app(11.5)).foregroundStyle(takingPlaces ? .primary : .secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func group<Content: View>(_ title: String, @ViewBuilder _ content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 5) {
            Text(title).font(.app(11, weight: .semibold)).foregroundStyle(.secondary)
            content()
        }
    }

    private func conflictRow(_ conflict: MergePreview.Conflict) -> some View {
        let choice = Binding<Merge3.Side?>(
            get: { take[conflict.path]?[conflict.conflict.label] },
            set: { take[conflict.path, default: [:]][conflict.conflict.label] = $0 })
        return VStack(alignment: .leading, spacing: 3) {
            Text(L10n[.Branches.mergeConflictOf(conflict.path, conflict.conflict.label)]).font(.app(11.5, weight: .semibold, design: .monospaced))
            value(L10n[.Branches.mergeBase], conflict.conflict.base)
            value(L10n[.Branches.mergeSource], conflict.conflict.source)
            value(L10n[.Branches.mergeTarget], conflict.conflict.target)
            Picker("", selection: choice) {
                Text(L10n[.Branches.mergeTakeSource]).tag(Optional(Merge3.Side.source))
                Text(L10n[.Branches.mergeKeepTarget]).tag(Optional(Merge3.Side.target))
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .fixedSize()
        }
        .padding(8)
        .background(RoundedRectangle(cornerRadius: 6).fill(Color.secondary.opacity(0.08)))
    }

    private func value(_ title: String, _ value: OJSON?) -> some View {
        let text = value.map { $0.compact() } ?? L10n[.Branches.mergeNoValue]
        return HStack(alignment: .top, spacing: 6) {
            Text(title).font(.app(10.5)).foregroundStyle(.secondary).frame(width: 56, alignment: .leading)
            Text(text.count > 160 ? String(text.prefix(160)) + "…" : text)
                .font(.app(10.5, design: .monospaced)).lineLimit(3).textSelection(.enabled)
        }
    }

    /// What an item that goes in does, as the letter before its path.
    static func mark(_ change: MergePreview.Change) -> ChangeMark {
        switch change {
        case .add: .made
        case .change: .changed
        case .remove: .removed
        }
    }

    private func reload() {
        refused = nil
        guard source != target else { preview = nil; return }
        preview = model.mergePreview(source: id(source), target: id(target))
        if preview == nil { refused = model.lastError }
    }

    private func run() {
        if model.merge(source: id(source), target: id(target), take: take, places: takingPlaces, keep: keeping) != nil {
            dismiss()
        } else {
            refused = model.lastError
        }
    }
}
