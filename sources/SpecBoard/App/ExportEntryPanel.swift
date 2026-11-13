import SwiftUI

/// A row of an export picked on the exports tab (`specboard://SpecBoard/Docs/Features/Out:Export`「行を選ぶと、
/// プロパティパネルにその行の設定が出て、その場で直せる」): by its export's name and its own from and to, as
/// `export_update` picks a row to change.
nonisolated struct ExportEntryRef: Equatable, Hashable, Sendable {
    var export: String
    var from: String
    var to: String
}

/// What the property panel says of a row picked, apart from how it is drawn: its settings by the keys
/// MCP writes them by, and what it writes.
enum ExportEntryPanel {
    struct Row: Equatable {
        var name: String
        var value: String
    }

    /// What a row writes: its files, how many cards in them, and how they stand in a word.
    struct Writes: Equatable {
        var files: [String]
        var cards: Int
        var status: String
    }

    /// The row's settings: `lines` only where it can be written (a row of the cards' lines).
    static func settings(_ entry: ExportRead.Entry) -> [Row] {
        var rows = [
            Row(name: "from", value: entry.from),
            Row(name: "to", value: entry.to),
            Row(name: "format", value: entry.format.map(ExportWords.formatLabel) ?? L10n[.Export.entryFormatFromExtension]),
            Row(name: "kinds", value: names(entry.kinds)),
        ]
        if narrowsLines(entry) { rows.append(Row(name: "lines", value: names(entry.lines))) }
        return rows
    }

    static func narrowsLines(_ entry: ExportRead.Entry) -> Bool { entry.format == "json-connections" }

    /// Names as they are written in a field: separated by commas.
    static func names(_ list: [String]?) -> String { (list ?? []).joined(separator: ", ") }

    /// Names as they are read from a field: split at commas — a Japanese comma typed between names is
    /// read as one — and not at spaces, which a name may hold; the spaces around each and the empty ones
    /// dropped.
    static func names(in written: String) -> [String] {
        written.split(whereSeparator: { $0 == "," || $0 == "、" })
            .map { $0.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty }
    }

    /// What the row numbered `row` (from 1) of `export` writes, as the export's files say it: a file two
    /// rows write is each one's, with all its cards.
    static func writes(_ export: ExportRead, row: Int) -> Writes {
        let files = export.files.filter { $0.rows?.contains(row) == true }
        let stand = ExportStatus.allCases.compactMap { status -> String? in
            let count = files.filter { $0.status == status }.count
            return count == 0 ? nil : (count == files.count ? status.said : "\(status.said) \(count)")
        }
        return Writes(files: files.map(\.path), cards: Set(files.flatMap(\.cards)).count,
                      status: stand.joined(separator: " · "))
    }
}

/// The property panel's table for a row of an export picked: its settings — read, and where the exports
/// may be changed, changed in place, the row staying where it stands — and what it writes. A change the
/// core refuses is said under its field, which goes back to what the row has.
struct ExportEntryProperties: View {
    var model: AppModel
    var export: ExportRead
    var entry: ExportRead.Entry
    var row: Int
    var editable: Bool

    /// Why the format chosen last was refused: a menu has no field to say it under.
    @State private var refusal: String?

    private var ref: ExportEntryRef { ExportEntryRef(export: export.name, from: entry.from, to: entry.to) }
    private var key: String { "\(export.name)|\(entry.from)|\(entry.to)" }

    /// The row changed; what was said of a format refused before goes with any change.
    private func change(_ set: [String: Any]) -> String? {
        refusal = nil
        return model.changeExportEntry(ref, set: set)
    }

    /// A field left the screen still focused, with nowhere under it to say a refusal: said as the window
    /// says one.
    private func changeLeaving(_ set: [String: Any]) {
        if let said = model.changeExportEntry(ref, set: set) { model.lastError = said }
    }

    var body: some View {
        PropertyCategory(title: L10n[.Export.panelRow]) {
            PropertyRow("from", help: L10n[.Export.panelFromHelp]) {
                Text(entry.from).font(.app(11.5, design: .monospaced)).textSelection(.enabled)
            }
            PropertyRow("to", help: L10n[.Export.panelToHelp]) {
                if editable {
                    CommitField.refusing(L10n[.Export.entryToPlaceholder], text: entry.to, monospaced: true, refuse: { written in
                        let to = written.trimmingCharacters(in: .whitespaces)
                        return to == entry.to ? nil : change(["to": to])
                    }, leave: { written in
                        let to = written.trimmingCharacters(in: .whitespaces)
                        if to != entry.to { changeLeaving(["to": to]) }
                    })
                    .id("export-to-\(key)")
                } else {
                    Text(entry.to).font(.app(11.5, design: .monospaced)).textSelection(.enabled)
                }
            }
            PropertyRow("format", help: L10n[.Export.panelFormatHelp]) {
                VStack(alignment: .leading, spacing: 2) {
                    if editable {
                        ChoiceMenu(options: [(value: "", text: L10n[.Export.entryFormatFromExtension])]
                                       + ExportWords.formats.map { (value: $0, text: ExportWords.formatLabel($0)) },
                                   selection: Binding(get: { entry.format ?? "" }, set: { chosen in
                                       guard chosen != (entry.format ?? "") else { return }
                                       refusal = change(["format": chosen.isEmpty ? NSNull() : chosen as Any])
                                   }))
                            .fixedSize()
                    } else {
                        Text(entry.format.map(ExportWords.formatLabel) ?? L10n[.Export.entryFormatFromExtension])
                    }
                    if let refusal {
                        Text(refusal).font(.app(10.5)).foregroundStyle(.red).fixedSize(horizontal: false, vertical: true)
                    }
                }
            }
            names("kinds", entry.kinds, help: L10n[.Export.panelKindsHelp])
            if ExportEntryPanel.narrowsLines(entry) {
                names("lines", entry.lines, help: L10n[.Export.panelLinesHelp])
            }
        }
        // Another row picked: what was said of the last one's format goes.
        .onChange(of: key) { refusal = nil }
        let writes = ExportEntryPanel.writes(export, row: row)
        PropertyCategory(title: L10n[.Export.panelWrites]) {
            PropertyRow("files", help: L10n[.Export.panelFilesHelp]) {
                if writes.files.isEmpty {
                    Text(L10n[.Export.filesNone]).foregroundStyle(.tertiary)
                } else {
                    VStack(alignment: .leading, spacing: 2) {
                        ForEach(writes.files.prefix(Self.filesShown), id: \.self) { path in
                            Text(path).font(.app(11, design: .monospaced)).lineLimit(1).truncationMode(.head)
                                .textSelection(.enabled).help(path)
                        }
                        if writes.files.count > Self.filesShown {
                            Text(L10n[.Export.panelFilesMore(writes.files.count - Self.filesShown)])
                                .font(.app(10.5)).foregroundStyle(.secondary)
                        }
                    }
                }
            }
            PropertyRow("cards", help: L10n[.Export.panelCardsHelp]) {
                Text("\(writes.cards)").font(.app(11.5, design: .monospaced))
            }
            if !writes.status.isEmpty {
                PropertyRow("status", help: L10n[.Export.panelStatusHelp]) { Text(writes.status) }
            }
            // What it leaves out, and why: as under the row where it is listed.
            ForEach(entry.unwritten, id: \.self) { line in
                Text(line).font(.app(10.5)).foregroundStyle(.red).fixedSize(horizontal: false, vertical: true)
                    .padding(.horizontal, 18).padding(.vertical, 3)
            }
        }
    }

    /// How many of a row's files are listed before the rest are counted.
    static let filesShown = 8

    /// A list of names — the kinds of card, the kinds of line — written in one field, separated by commas;
    /// emptied, the row is not narrowed by them.
    private func names(_ name: String, _ list: [String]?, help: String) -> some View {
        PropertyRow(name, help: help) {
            let written = ExportEntryPanel.names(list)
            if editable {
                CommitField.refusing(L10n[.Export.panelAll], text: written, monospaced: true, refuse: { typed in
                    let named = ExportEntryPanel.names(in: typed)
                    return named == (list ?? []) ? nil : change([name: named.isEmpty ? NSNull() : named as Any])
                }, leave: { typed in
                    let named = ExportEntryPanel.names(in: typed)
                    if named != (list ?? []) { changeLeaving([name: named.isEmpty ? NSNull() : named as Any]) }
                })
                .id("export-\(name)-\(key)-\(written)")
            } else if written.isEmpty {
                Text(L10n[.Export.panelAll]).foregroundStyle(.tertiary)
            } else {
                Text(written).font(.app(11.5, design: .monospaced)).textSelection(.enabled)
            }
        }
    }
}
