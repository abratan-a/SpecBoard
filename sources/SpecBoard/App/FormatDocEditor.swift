import SwiftUI

/// A formatdoc opened — its card editor (overview §13), framed as a freedoc's: 表示 reads it as
/// the document it is written out as; 編集 has its rows on the left — items nested and foldable,
/// the picked row edited in the property panel (`FormatRowEditor`) with what an item is
/// connected to — and the document as it reads on the right, as a freedoc's Markdown has its
/// preview.
struct FormatDocEditor: View {
    var model: AppModel
    var doc: SchemaNode
    var editable: Bool

    @Environment(\.colorScheme) private var colorScheme
    private var theme: GraphTheme { colorScheme == .dark ? .dark : .light }

    @State private var folded: Set<String> = []
    /// Its rows being written (編集), rather than read as the document it is written out as
    /// (表示). It opens to be read, as a freedoc does — but to its rows when one of them
    /// is picked (from the navigation, a line, an agent), for that row to be there.
    @State private var writing: Bool

    init(model: AppModel, doc: SchemaNode, editable: Bool) {
        self.model = model
        self.doc = doc
        self.editable = editable
        _writing = State(initialValue: Self.opensToRows(model, doc))
    }

    /// Whether it opens to its rows (編集): when one of them is picked.
    static func opensToRows(_ model: AppModel, _ doc: SchemaNode) -> Bool {
        guard model.selectedSchemaId == doc.id, case .formatRow? = model.selectedRow else { return false }
        return true
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            header
            Divider()
            if writing {
                // Its rows on the left, and what they read as on the right, as a freedoc's text
                // and its preview. The line to add rows by under both, the
                // editor's whole width: half of a narrow editor would stack its buttons.
                HStack(spacing: 0) {
                    rows
                        .frame(maxWidth: .infinity)
                    Divider()
                    reader(padding: 20)
                        .frame(maxWidth: .infinity)
                }
                if editable { adding }
            } else {
                reader(padding: 28)
            }
        }
        .onChange(of: doc.id) { writing = Self.opensToRows(model, doc) }
        .onChange(of: model.selectedRow) { if Self.opensToRows(model, doc) { writing = true } }
    }

    /// The document as it reads: as it is written out.
    @ViewBuilder
    private func reader(padding: CGFloat) -> some View {
        if let detail = model.detail {
            ScrollView {
                if doc.rows.isEmpty {
                    Text(writing ? L10n[.Docs.wordPreview] : L10n[.Docs.formatdocNothingWritten]).foregroundStyle(.tertiary)
                        .padding(padding).frame(maxWidth: .infinity, alignment: .leading)
                } else {
                    MarkdownView(text: Self.document(doc, in: detail), size: 13,
                                 refs: NoteEditor.refs(doc, detail) { model.open(path: $0, from: $1) })
                        .padding(.horizontal, padding).padding(.vertical, padding * 0.7)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
        }
    }

    /// Its rows, or the first ones to add.
    @ViewBuilder
    private var rows: some View {
        if doc.rows.isEmpty {
            VStack(spacing: 10) {
                Text(L10n[.Docs.formatdocNoRows]).foregroundStyle(.secondary)
                if editable {
                    HStack(spacing: 10) {
                        Button(L10n[.Docs.formatdocAddItem]) { add(.item, inside: []) }
                        Button(L10n[.Docs.formatdocAddBody]) { add(.text, inside: []) }
                    }
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else {
            list
        }
    }

    /// The document as it reads: its title, each item a heading as deep as it
    /// is, each row what it says — as it is written out (`DocumentExport.markdown`), but
    /// with the project's paths left for the view to resolve, link and draw.
    static func document(_ doc: SchemaNode, in detail: ProjectDetail) -> String {
        let directory = detail.directory
        let base = directory.sectionId(of: doc)
        func missing(_ target: String) -> String { "~~\(MarkdownRefs.code(target))~~" }
        return "# \(doc.label.isEmpty ? doc.name : doc.label)\n\n"
            + FormatRows.markdown(doc.rows) { row, _, _ in
                switch row.kind {
                case .item:
                    return nil
                case .text:
                    return row.body
                case .table:
                    return FormatRows.table(row.columns, row.cells)
                case .diagram:
                    return "![[\(row.target)]]"
                case .diagrams:
                    guard let charts = FormatRender.charts(in: row.target, from: base, in: detail) else { return missing(row.target) }
                    return charts.map { "**\($0.name)**\n\n![[\(directory.full($0))]]" }.joined(separator: "\n\n")
                case .card:
                    guard let card = directory.schema(row.target, from: base),
                          let table = FormatRender.cardTable(card, in: detail, name: { directory.full($0) }) else {
                        return missing(row.target)
                    }
                    return table
                }
            }
    }

    // MARK: - Above

    private var header: some View {
        let foldable = foldablePaths
        return CardBar(kind: .formatdoc, badge: L10n[.Docs.formatdocBadge], name: doc.name, label: doc.label) {
            EmptyView()
        } tools: {
            if !foldable.isEmpty && writing {
                BarMenu(title: L10n[.Docs.formatdocFoldMenu], systemImage: "rectangle.expand.vertical") {
                    Button(L10n[.Common.actionExpandAll]) { folded = [] }.disabled(folded.isEmpty)
                    Button(L10n[.Common.actionCollapseAll]) { folded = foldable }.disabled(folded == foldable)
                } items: {
                    Button(L10n[.Common.actionExpandAll]) { folded = [] }.disabled(folded.isEmpty)
                    Button(L10n[.Common.actionCollapseAll]) { folded = foldable }.disabled(folded == foldable)
                }
            }
            Picker("", selection: $writing) {
                BarLabel(title: L10n[.Docs.modeView], systemImage: "eye").tag(false)
                BarLabel(title: L10n[.Common.actionEdit], systemImage: "pencil").tag(true)
            }
            .labelsHidden().pickerStyle(.segmented).fixedSize()
            .help(L10n[.Docs.modeFormatHelp])
            CardTools(model: model, card: doc, editable: editable) { model.open(.card($0)) }
        }
        .font(.app(11))
        .padding(.horizontal, 14).padding(.vertical, 8)
    }

    // MARK: - Rows

    private struct Line: Identifiable {
        var row: FormatRow
        var path: [String]
        var depth: Int
        var id: String { PathText.joinNames(path) }
    }

    private var lines: [Line] {
        var out: [Line] = []
        func walk(_ list: [FormatRow], _ trail: [String], _ depth: Int) {
            for row in list {
                let path = trail + [row.name]
                out.append(Line(row: row, path: path, depth: depth))
                if row.kind == .item, !folded.contains(PathText.joinNames(path)) { walk(row.rows, path, depth + 1) }
            }
        }
        walk(doc.rows, [], 0)
        return out
    }

    private var foldablePaths: Set<String> {
        var out: Set<String> = []
        FormatRows.walk(doc.rows) { row, path, _ in
            if row.kind == .item, !row.rows.isEmpty { out.insert(PathText.joinNames(path)) }
        }
        return out
    }

    private var list: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 0) {
                ForEach(lines) { line in
                    row(line)
                    // What the row says, written in place; what it is, in the property panel.
                    if model.selectedRow == .formatRow(line.id) && model.selectedSchemaId == doc.id {
                        FormatRowEditor(model: model, note: doc, path: line.id, editable: editable, part: .content)
                            .padding(12)
                            .background(RoundedRectangle(cornerRadius: 8).fill(Color.secondary.opacity(0.06)))
                            .overlay(RoundedRectangle(cornerRadius: 8).stroke(Color.secondary.opacity(0.2)))
                            .padding(.leading, 28 + CGFloat(line.depth) * 18).padding(.trailing, 14).padding(.vertical, 6)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                }
            }
            .padding(.vertical, 4)
        }
        .background(Color.clear.contentShape(Rectangle()).onTapGesture { model.clearRow() })
    }

    private func row(_ line: Line) -> some View {
        let key = line.id
        let picked = model.selectedRow == .formatRow(key) && model.selectedSchemaId == doc.id
        let hasInside = line.row.kind == .item && !line.row.rows.isEmpty
        return HStack(alignment: .firstTextBaseline, spacing: 6) {
            Button {
                if folded.contains(key) { folded.remove(key) } else { folded.insert(key) }
            } label: {
                Image(systemName: folded.contains(key) ? "chevron.right" : "chevron.down")
                    .font(.app(9)).frame(width: 12)
            }
            .buttonStyle(.plain)
            .opacity(hasInside ? 1 : 0)
            .disabled(!hasInside)
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 8) {
                    if line.row.kind == .item {
                        Text(line.row.title).font(.app(line.depth == 0 ? 12.5 : 12, weight: .semibold))
                    } else {
                        Text(line.row.kind.label).font(.app(10)).foregroundStyle(.secondary)
                            .padding(.horizontal, 4).padding(.vertical, 1)
                            .overlay(RoundedRectangle(cornerRadius: 3).stroke(.tertiary))
                    }
                    Text(line.row.name).font(.app(10.5, design: .monospaced)).foregroundStyle(.secondary)
                }
                let glance = Self.glance(line.row)
                if !glance.isEmpty {
                    Text(glance).font(.app(11)).foregroundStyle(.tertiary).lineLimit(1)
                }
            }
            Spacer(minLength: 6)
        }
        .padding(.leading, 10 + CGFloat(line.depth) * 18).padding(.trailing, 14).padding(.vertical, 5)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(picked ? Color(nsColor: theme.selection) : .clear)
        .contentShape(Rectangle())
        .onTapGesture { model.selectRow(doc.id, .formatRow(key)) }
        .contextMenu {
            if let detail = model.detail {
                // In full, wherever it is pasted (`specboard://SpecBoard/Docs/Design:DesignSections` §8).
                let path = detail.directory.full(doc, line.path)
                Button(L10n[.Common.actionCopyPath]) { copyToPasteboard(path) }
            }
            if editable {
                Divider()
                Menu(L10n[.Docs.menuAddAfter]) {
                    ForEach([FormatRow.Kind.item, .text, .table], id: \.self) { kind in
                        Button(kind.label) { add(kind, after: line.path) }
                    }
                }
                if line.row.kind == .item {
                    Menu(L10n[.Docs.menuAddInside]) {
                        ForEach([FormatRow.Kind.item, .text, .table], id: \.self) { kind in
                            Button(kind.label) { add(kind, inside: line.path) }
                                .disabled(kind == .item && line.path.count >= FormatRows.maxDepth)
                        }
                    }
                }
                Divider()
                Button(L10n[.Common.actionMoveUp]) { model.setRows(doc.id, FormatRows.moving(line.path, by: -1, in: doc.rows)) }
                Button(L10n[.Common.actionMoveDown]) { model.setRows(doc.id, FormatRows.moving(line.path, by: 1, in: doc.rows)) }
                Button(L10n[.Docs.menuOutdent]) { outdent(line.path) }.disabled(line.path.count < 2)
                Button(L10n[.Docs.menuIndent]) { indent(line.path) }
                Divider()
                Button(L10n[.Docs.menuDeleteRow], role: .destructive) {
                    if model.selectedRow == .formatRow(key) { model.clearRow() }
                    model.setRows(doc.id, FormatRows.removing(line.path, in: doc.rows))
                }
            }
        }
    }

    /// The row in a line, for a glance at it.
    static func glance(_ row: FormatRow) -> String {
        switch row.kind {
        case .item: FormatRows.head(row.rows.first { $0.kind == .text }?.body ?? "", lines: 1)
        case .text: FormatRows.head(row.body, lines: 1)
        case .table: L10n[.Docs.tableGlance(row.cells.count, row.columns.joined(separator: L10n[.Common.separatorList]))]
        case .diagram, .diagrams, .card: row.target
        }
    }

    // MARK: - Adding

    /// Where a row added from below goes: inside the picked item, after the picked row,
    /// or at the end.
    private var addingAt: (inside: [String]?, after: [String]?) {
        guard model.selectedSchemaId == doc.id, case .formatRow(let dotted)? = model.selectedRow else { return ([], nil) }
        let path = PathText.names(dotted)
        guard let row = FormatRows.row(at: path, in: doc.rows) else { return ([], nil) }
        return row.kind == .item ? (path, nil) : (nil, path)
    }

    private var adding: some View {
        VStack(spacing: 0) {
            Divider()
            // Its buttons keep their words, or go to marks when narrow (`CardFoot`).
            CardFoot(hint: L10n[.Docs.addHint]) {
                Menu {
                    ForEach([FormatRow.Kind.item, .text, .table], id: \.self) { kind in
                        Button(kind.label) { addHere(kind) }
                    }
                } label: {
                    BarLabel(title: L10n[.Docs.addRows], systemImage: "plus")
                }
                .fixedSize()
                .help(L10n[.Docs.addRows])
                if let detail = model.detail {
                    PathPickButton(title: L10n[.Docs.addDiagram], compactImage: "photo", detail: detail, owner: doc, wanted: .embed) {
                        addHere(.diagram, target: $0)
                    }
                    PathPickButton(title: L10n[.Docs.addSectionDiagrams], compactImage: "square.grid.2x2", detail: detail, owner: doc,
                                   wanted: PathWanted(cards: [], sections: true)) { addHere(.diagrams, target: $0) }
                    PathPickButton(title: L10n[.Docs.addCardTable], compactImage: "tablecells", detail: detail, owner: doc,
                                   wanted: .shapes) { addHere(.card, target: $0) }
                }
            }
            .padding(10)
            .disabled(model.isReadOnly)
        }
    }

    private func addHere(_ kind: FormatRow.Kind, target: String = "") {
        let at = addingAt
        if let after = at.after { add(kind, after: after, target: target) } else { add(kind, inside: at.inside ?? [], target: target) }
    }

    private func fresh(_ kind: FormatRow.Kind, among siblings: [FormatRow], target: String) -> FormatRow {
        let base: String = switch kind {
        case .item: "s"
        case .text: "t"
        case .table: "table"
        case .diagram: "fig"
        case .diagrams: "figs"
        case .card: "card"
        }
        var row = FormatRow(name: FormatRows.freeName("\(base)\(siblings.count + 1)", in: siblings), kind: kind)
        switch kind {
        case .item: row.heading = L10n[.Docs.newItem]
        case .table: row.columns = [L10n[.Docs.newColumn(1)], L10n[.Docs.newColumn(2)]]
        default: row.target = target
        }
        return row
    }

    private func add(_ kind: FormatRow.Kind, after path: [String], target: String = "") {
        let siblings = FormatRows.children(of: Array(path.dropLast()), in: doc.rows)
        let row = fresh(kind, among: siblings, target: target)
        model.setRows(doc.id, FormatRows.inserting(row, after: path, in: doc.rows))
        model.selectRow(doc.id, .formatRow(PathText.joinNames(path.dropLast() + [row.name])))
    }

    private func add(_ kind: FormatRow.Kind, inside path: [String], target: String = "") {
        let siblings = FormatRows.children(of: path, in: doc.rows)
        let row = fresh(kind, among: siblings, target: target)
        model.setRows(doc.id, FormatRows.appending(row, inside: path, in: doc.rows))
        folded.remove(PathText.joinNames(path))
        model.selectRow(doc.id, .formatRow(PathText.joinNames(path + [row.name])))
    }

    private func indent(_ path: [String]) {
        guard let moved = FormatRows.indenting(path, in: doc.rows) else { return }
        model.setRows(doc.id, moved.rows, moves: [(path, moved.path)])
    }

    private func outdent(_ path: [String]) {
        guard let moved = FormatRows.outdenting(path, in: doc.rows) else { return }
        model.setRows(doc.id, moved.rows, moves: [(path, moved.path)])
    }
}

/// A row picked in a formatdoc's card editor (overview §13). Under it, what the row
/// says (`content`): an item's heading, a text's Markdown, a table's columns and cells,
/// what a diagram or a card table draws. In the property panel, what the row is
/// (`properties`): its name, its kind, a diagram's or a card table's target, and for an
/// item the items connected to it — the ones its rows point at, and the ones pointing at it.
struct FormatRowEditor: View {
    enum Part {
        case content
        case properties
    }

    var model: AppModel
    var note: SchemaNode
    /// Dotted, in the formatdoc, as a path writes it: `auth.login`.
    var path: String
    var editable: Bool
    var part: Part

    private var names: [String] { PathText.names(path) }

    var body: some View {
        if let row = FormatRows.row(at: names, in: note.rows), let detail = model.detail {
            switch part {
            case .content:
                VStack(alignment: .leading, spacing: 16) { content(row, detail) }
            case .properties:
                properties(row, detail)
            }
        } else {
            Text(L10n[.Docs.rowGone]).font(.app(11)).foregroundStyle(.secondary)
        }
    }

    @ViewBuilder
    private func properties(_ row: FormatRow, _ detail: ProjectDetail) -> some View {
        PropertyCategory(title: L10n[.Common.wordRow]) {
            PropertyRow("name", help: L10n[.Docs.rowNameHelp]) {
                if editable {
                    CommitField("name", text: row.name, monospaced: true) { name in rename(to: name, row) }
                        .id("\(note.id)|\(path)|name")
                } else {
                    Text(row.name).font(.app(11.5, design: .monospaced))
                }
            }
            PropertyRow("kind", help: Glossary.term("rows")?.meaning ?? "") {
                Text(L10n[.Docs.rowKindAndId(row.kind.label, row.kind.rawValue)]).foregroundStyle(.secondary)
            }
            PropertyRow("path", help: L10n[.Docs.rowPathHelp]) {
                Text(detail.directory.full(note, names)).font(.app(11, design: .monospaced)).textSelection(.enabled)
            }
            if [.diagram, .diagrams, .card].contains(row.kind) {
                PropertyRow("target", help: row.kind == .diagrams ? L10n[.Docs.rowTargetDiagrams]
                            : row.kind == .card ? L10n[.Docs.rowTargetCard] : L10n[.Docs.rowTargetDiagram]) {
                    HStack(spacing: 4) {
                        if editable {
                            CommitField(L10n[.Docs.rowTargetPlaceholder], text: row.target, monospaced: true) { target in
                                var next = row
                                next.target = target
                                save(next)
                            }
                            .id("\(note.id)|\(path)|target")
                            PathPickButton(title: "…", detail: detail, owner: note,
                                           wanted: row.kind == .card ? .shapes
                                               : row.kind == .diagrams ? PathWanted(cards: [], sections: true) : .embed) { target in
                                var next = row
                                next.target = target
                                save(next)
                            }
                        } else {
                            Text(row.target).font(.app(11.5, design: .monospaced))
                        }
                    }
                }
            }
        }
        if row.kind == .item {
            let (mentions, mentionedBy) = RowLinks.connected(detail, note: note, path: names)
            PropertyCategory(title: L10n[.Docs.rowPointsAt], count: mentions.count) {
                links(mentions, empty: L10n[.Docs.rowLinksNone]).padding(8)
            }
            PropertyCategory(title: L10n[.Docs.rowPointedAtBy], count: mentionedBy.count) {
                links(mentionedBy, empty: L10n[.Docs.rowLinksNone]).padding(8)
            }
        }
    }

    private func save(_ row: FormatRow) {
        model.setRows(note.id, FormatRows.replacing(names, with: row, in: note.rows))
    }

    @ViewBuilder
    private func content(_ row: FormatRow, _ detail: ProjectDetail) -> some View {
        let refs = NoteEditor.refs(note, detail) { model.open(path: $0, from: $1) }
        switch row.kind {
        case .item:
            block("heading") {
                if editable {
                    CommitField(L10n[.Docs.rowHeadingPlaceholder], text: row.heading) { heading in
                        var next = row
                        next.heading = heading
                        save(next)
                    }
                    .id("\(note.id)|\(path)|heading")
                } else {
                    Text(row.title).font(.app(12, weight: .semibold))
                }
            }
        case .text:
            block("body") {
                if editable {
                    TextRowEditor(text: row.body, refs: refs) { body in
                        var next = row
                        next.body = body
                        save(next)
                    }
                    .id("\(note.id)|\(path)|body")
                    Text(L10n[.Docs.rowTextHelp])
                        .font(.app(10.5)).foregroundStyle(.tertiary)
                        .fixedSize(horizontal: false, vertical: true)
                } else if row.body.isEmpty {
                    Text(L10n[.Docs.rowNothingYet]).font(.app(10.5)).foregroundStyle(.tertiary)
                } else {
                    MarkdownView(text: row.body, size: 12, refs: refs)
                }
            }
        case .table:
            if editable {
                block("columns") {
                    CommitField(L10n[.Docs.rowColumnsPlaceholder], text: row.columns.joined(separator: ", ")) { text in
                        var next = row
                        next.columns = text.split(separator: ",").map { $0.trimmingCharacters(in: .whitespaces) }
                            .filter { !$0.isEmpty }
                        save(next)
                    }
                    .id("\(note.id)|\(path)|columns")
                }
                block("cells") {
                    Text(L10n[.Docs.rowCellsHelp]).font(.app(10.5)).foregroundStyle(.tertiary)
                    CommitTextEditor(text: row.cells.map { $0.joined(separator: " | ") }.joined(separator: "\n"),
                                     minHeight: 100) { text in
                        var next = row
                        next.cells = text.split(separator: "\n", omittingEmptySubsequences: true).map { line in
                            line.split(separator: "|", omittingEmptySubsequences: false)
                                .map { $0.trimmingCharacters(in: .whitespaces) }
                        }
                        save(next)
                    }
                    .id("\(note.id)|\(path)|cells")
                }
            }
            block(L10n[.Common.wordTable]) { MarkdownView(text: FormatRows.table(row.columns, row.cells), size: 12, refs: refs) }
        case .diagram, .diagrams, .card:
            // What it points at is in the property panel; here, what it draws.
            block(L10n[.Docs.rowAppearance]) { drawn(row, detail, refs) }
        }
    }

    /// What the row draws from the project, as it will read.
    @ViewBuilder
    private func drawn(_ row: FormatRow, _ detail: ProjectDetail, _ refs: MarkdownRefs) -> some View {
        let directory = detail.directory
        let base = directory.sectionId(of: note)
        switch row.kind {
        case .diagram:
            MarkdownView(text: "![[\(row.target)]]", size: 12, refs: refs)
        case .diagrams:
            if let charts = FormatRender.charts(in: row.target, from: base, in: detail) {
                MarkdownView(text: charts.isEmpty ? L10n[.Docs.rowNoFlows]
                             : charts.map { "#### \($0.name)\n\n![[\(directory.full($0))]]" }.joined(separator: "\n\n"),
                             size: 12, refs: refs)
            } else {
                Text(L10n[.Docs.rowNoChartSection]).font(.app(10.5)).foregroundStyle(.tertiary)
            }
        case .card:
            if let card = directory.schema(row.target, from: base),
               let table = FormatRender.cardTable(card, in: detail, name: { directory.full($0) }) {
                MarkdownView(text: table, size: 12, refs: refs)
            } else {
                Text(L10n[.Docs.rowNoSchemaCard]).font(.app(10.5)).foregroundStyle(.tertiary)
            }
        default:
            EmptyView()
        }
    }

    private func rename(to typed: String, _ row: FormatRow) {
        let name: String
        do { name = try Validate.name(typed, field: "name") } catch {
            model.lastError = error.localizedDescription
            return
        }
        guard name != row.name else { return }
        let siblings = FormatRows.children(of: Array(names.dropLast()), in: note.rows)
        guard !siblings.contains(where: { $0.name == name }) else {
            model.lastError = L10n[.Docs.errorRowNameTaken(name)]
            return
        }
        var next = row
        next.name = name
        let to = Array(names.dropLast()) + [name]
        model.setRows(note.id, FormatRows.replacing(names, with: next, in: note.rows), moves: [(names, to)])
    }

    @ViewBuilder
    private func links(_ list: [RowLink], empty: String) -> some View {
        if list.isEmpty {
            Text(empty).font(.app(10.5)).foregroundStyle(.tertiary)
        } else {
            ForEach(list, id: \.written) { link in
                Button { model.open(path: link.written) } label: {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(link.title).font(.app(12, weight: .semibold))
                        Text(link.written).font(.app(10.5, design: .monospaced)).foregroundStyle(.secondary)
                        if !link.head.isEmpty {
                            Text(link.head).font(.app(11)).foregroundStyle(.secondary).lineLimit(3)
                                .fixedSize(horizontal: false, vertical: true)
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(8)
                    .background(RoundedRectangle(cornerRadius: 6).fill(Color.secondary.opacity(0.07)))
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
        }
    }

    private func block<Content: View>(_ title: String, @ViewBuilder _ content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(title).font(.app(10, weight: .semibold)).tracking(1.1).foregroundStyle(.secondary)
            content()
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// A text row written with what it reads as under it, as it is typed: the
/// Markdown, `[[…]]` and `{{…}}` as the document will show them.
private struct TextRowEditor: View {
    var text: String
    var refs: MarkdownRefs
    var onCommit: (String) -> Void
    @State private var live: String

    init(text: String, refs: MarkdownRefs, onCommit: @escaping (String) -> Void) {
        self.text = text
        self.refs = refs
        self.onCommit = onCommit
        _live = State(initialValue: text)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            CommitTextEditor(text: text, minHeight: 160, onDraft: { live = $0 }, onCommit: onCommit)
            Text(L10n[.Docs.wordPreview]).font(.app(10.5)).foregroundStyle(.tertiary)
            Group {
                if live.isEmpty {
                    Text(L10n[.Docs.rowNothingYet]).font(.app(10.5)).foregroundStyle(.tertiary)
                } else {
                    MarkdownView(text: live, size: 12, refs: refs)
                }
            }
            .padding(8)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(RoundedRectangle(cornerRadius: 5).fill(Color.secondary.opacity(0.05)))
        }
        .onChange(of: text) { _, updated in live = updated }
    }
}
