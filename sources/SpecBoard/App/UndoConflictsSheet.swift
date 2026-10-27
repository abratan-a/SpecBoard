import SwiftUI

/// Where undoing one commit alone clashed with later commits (`specboard://SpecBoard/Docs/Design:UI` §6):
/// each place by its path and unit, the value before the commit, right after it and now, and the later
/// commits that wrote it. Nothing was written, and nothing is chosen here.
struct UndoConflictsSheet: View {
    var refusal: UndoRefusal
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(L10n[.Root.undoConflictsTitle("\(refusal.seq)")]).font(.app(15, weight: .semibold))
            Text(L10n[.Root.undoConflictsMessage])
                .font(.app(11)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
            Divider()
            ScrollView {
                VStack(alignment: .leading, spacing: 8) {
                    ForEach(refusal.conflicts) { conflictRow($0) }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            Spacer(minLength: 0)
            if !refusal.later.isEmpty {
                Text(L10n[.Root.undoLater(Self.numbers(refusal.later))]).font(.app(11)).foregroundStyle(.secondary)
            }
            HStack {
                Spacer()
                Button(L10n[.Common.actionClose]) { dismiss() }.keyboardShortcut(.defaultAction)
            }
        }
        .padding(18)
        // Lists of paths and before/after values: the sheet is dragged larger, and never smaller than this.
        .frame(minWidth: 600, idealWidth: 600, maxWidth: .infinity, minHeight: 520, idealHeight: 520, maxHeight: .infinity)
    }

    /// The commit numbers, each with its #, joined the way the language joins a list.
    static func numbers(_ seqs: [Int], in code: String = Language.shared.code) -> String {
        seqs.map { "#\($0)" }.joined(separator: L10n.text(.Root.undoListSeparator, in: code))
    }

    private func conflictRow(_ conflict: UndoConflict) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(L10n[.Branches.mergeConflictOf(conflict.path, Merge3.shown(conflict.unit))])
                .font(.app(11.5, weight: .semibold, design: .monospaced))
            value(L10n[.Root.undoBefore("\(refusal.seq)")], conflict.before)
            value(L10n[.Root.undoAfter("\(refusal.seq)")], conflict.after)
            value(L10n[.Root.undoNow], conflict.now)
        }
        .padding(8)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(RoundedRectangle(cornerRadius: 6).fill(Color.secondary.opacity(0.08)))
    }

    /// A value as the merge window shows one: compact, cut at 160 characters.
    private func value(_ title: String, _ value: OJSON?) -> some View {
        let text = value.map { $0.compact() } ?? L10n[.Branches.mergeNoValue]
        return HStack(alignment: .top, spacing: 6) {
            Text(title).font(.app(10.5)).foregroundStyle(.secondary).frame(width: 76, alignment: .leading)
            Text(text.count > 160 ? String(text.prefix(160)) + "…" : text)
                .font(.app(10.5, design: .monospaced)).lineLimit(3).textSelection(.enabled)
        }
    }
}
