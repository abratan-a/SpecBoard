import Foundation

/// One row of a formatdoc (`specboard://SpecBoard/Docs/Design:DesignActions`「formatdoc」): an item — a heading
/// and the rows inside it — or text, a table, a diagram, the diagrams of a section, or
/// the table of a card's contents.
///
/// Every row has a name the paths use, like a nested field: `Docs:Spec.auth.login` is
/// the row `login` inside the item `auth`. Only an item holds rows; what the rows inside
/// an item point at is drawn from that item's line on the diagram.
nonisolated struct FormatRow: Codable, Hashable, Sendable {
    enum Kind: String, Codable, CaseIterable, Sendable {
        /// 項目: a heading, and the rows inside it.
        case item
        /// 本文: Markdown, where `[[…]]` and `{{…}}` are read.
        case text
        /// 表: the columns' headings and rows of cells, each Markdown.
        case table
        /// 図: a card or a section, drawn as `![[…]]` draws it.
        case diagram
        /// セクションの図: the diagram of every flow and action in a chart section.
        case diagrams
        /// カードの表: a shape's contents as a table — its fields, or its values.
        case card

        /// What a person sees for it.
        var label: String {
            switch self {
            case .item: L10n.text(.Docs.kindItem, in: Language.current)
            case .text: L10n.text(.Docs.kindText, in: Language.current)
            case .table: L10n.text(.Common.wordTable, in: Language.current)
            case .diagram: L10n.text(.Docs.kindDiagram, in: Language.current)
            case .diagrams: L10n.text(.Docs.kindDiagrams, in: Language.current)
            case .card: L10n.text(.Docs.kindCard, in: Language.current)
            }
        }

        /// Whether the row names a place in the project in its `target`.
        var hasTarget: Bool { self == .diagram || self == .diagrams || self == .card }
    }

    var name: String
    var kind: Kind = .item
    /// An item's heading.
    var heading: String = ""
    /// A text's Markdown.
    var body: String = ""
    /// The rows inside an item.
    var rows: [FormatRow] = []
    /// A table's columns' headings.
    var columns: [String] = []
    /// A table's rows of cells.
    var cells: [[String]] = []
    /// What a diagram, the diagrams of a section, or a card's table is of: a card
    /// (`Data:Order`), or a section ending in `:` (`Behavior/Flows:`).
    var target: String = ""

    init(name: String, kind: Kind = .item, heading: String = "", body: String = "", rows: [FormatRow] = [],
         columns: [String] = [], cells: [[String]] = [], target: String = "") {
        self.name = name
        self.kind = kind
        self.heading = heading
        self.body = body
        self.rows = rows
        self.columns = columns
        self.cells = cells
        self.target = target
    }

    enum CodingKeys: String, CodingKey { case name, kind, heading, body, rows, columns, cells, target }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        name = try c.decode(String.self, forKey: .name)
        kind = try c.decodeIfPresent(Kind.self, forKey: .kind) ?? .item
        heading = try c.decodeIfPresent(String.self, forKey: .heading) ?? ""
        body = try c.decodeIfPresent(String.self, forKey: .body) ?? ""
        rows = try c.decodeIfPresent([FormatRow].self, forKey: .rows) ?? []
        columns = try c.decodeIfPresent([String].self, forKey: .columns) ?? []
        cells = try c.decodeIfPresent([[String]].self, forKey: .cells) ?? []
        target = try c.decodeIfPresent(String.self, forKey: .target) ?? ""
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(name, forKey: .name)
        try c.encode(kind, forKey: .kind)
        if !heading.isEmpty { try c.encode(heading, forKey: .heading) }
        if !body.isEmpty { try c.encode(body, forKey: .body) }
        if !rows.isEmpty { try c.encode(rows, forKey: .rows) }
        if !columns.isEmpty { try c.encode(columns, forKey: .columns) }
        if !cells.isEmpty { try c.encode(cells, forKey: .cells) }
        if !target.isEmpty { try c.encode(target, forKey: .target) }
    }

    /// What a reader sees for it: an item's heading, or its name.
    var title: String { kind == .item && !heading.isEmpty ? heading : name }

    /// The Markdown it holds, where `[[…]]` and `{{…}}` are read: a text's body, a
    /// table's headings and cells. Each with where it is in the row (`body`,
    /// `columns[0]`, `cells[1][2]`), as a broken path says it.
    var texts: [(place: String, text: String)] {
        switch kind {
        case .text:
            return [("body", body)]
        case .table:
            return columns.enumerated().map { ("columns[\($0.offset)]", $0.element) }
                + cells.enumerated().flatMap { r, line in line.enumerated().map { ("cells[\(r)][\($0.offset)]", $0.element) } }
        default:
            return []
        }
    }

    /// The same row, the Markdown in it and in every row inside passed through `text`
    /// and every target through `path` — how a rename reaches them.
    func mapped(text: (String) -> String, path: (String) -> String = { $0 }) -> FormatRow {
        var out = self
        out.body = text(body)
        out.columns = columns.map(text)
        out.cells = cells.map { $0.map(text) }
        if !target.isEmpty { out.target = path(target) }
        out.rows = rows.map { $0.mapped(text: text, path: path) }
        return out
    }
}

/// A formatdoc's rows as a tree: walked, read and changed by path.
enum FormatRows {
    /// How deep items go: the export writes the top ones as `##` and the deepest as
    /// `######`, `#` being the document's own title.
    static let maxDepth = 5
    static let maxCount = 1000
    static let maxText = 200_000

    /// Every row, an item before what is inside it, with its path from the top and
    /// how deep it is (0 at the top).
    static func walk(_ rows: [FormatRow], _ visit: (FormatRow, [String], Int) -> Void) {
        func go(_ list: [FormatRow], _ trail: [String], _ depth: Int) {
            for row in list {
                let path = trail + [row.name]
                visit(row, path, depth)
                if row.kind == .item { go(row.rows, path, depth + 1) }
            }
        }
        go(rows, [], 0)
    }

    static func count(_ rows: [FormatRow]) -> Int {
        var n = 0
        walk(rows) { _, _, _ in n += 1 }
        return n
    }

    static func row(at path: [String], in rows: [FormatRow]) -> FormatRow? {
        guard let first = path.first, let found = rows.first(where: { $0.name == first }) else { return nil }
        if path.count == 1 { return found }
        guard found.kind == .item else { return nil }
        return row(at: Array(path.dropFirst()), in: found.rows)
    }

    /// The item a path lands in, for what is drawn from it: the row itself when it is
    /// an item, else the item it is in — empty for a row at the top.
    static func item(of path: [String], in rows: [FormatRow]) -> [String] {
        guard let found = row(at: path, in: rows) else { return [] }
        return found.kind == .item ? path : Array(path.dropLast())
    }

    /// Whether `path` names a row, or the start of one: `auth.login.x` lands in
    /// `auth.login` and says the rest is not there.
    static func deepest(_ path: [String], in rows: [FormatRow]) -> (found: [String], rest: [String]) {
        var list = rows
        var found: [String] = []
        for (index, name) in path.enumerated() {
            guard let next = list.first(where: { $0.name == name }) else {
                return (found, Array(path[index...]))
            }
            found.append(name)
            guard next.kind == .item else {
                return (found, Array(path[(index + 1)...]))
            }
            list = next.rows
        }
        return (found, [])
    }

    /// The list the row is in and where, handed to `body` to change.
    @discardableResult
    static func within(_ rows: inout [FormatRow], _ path: [String],
                       _ body: (inout [FormatRow], Int) -> Bool) -> Bool {
        guard let first = path.first, let index = rows.firstIndex(where: { $0.name == first }) else { return false }
        if path.count == 1 { return body(&rows, index) }
        guard rows[index].kind == .item else { return false }
        return within(&rows[index].rows, Array(path.dropFirst()), body)
    }

    static func replacing(_ path: [String], with row: FormatRow, in rows: [FormatRow]) -> [FormatRow] {
        var out = rows
        within(&out, path) { list, index in list[index] = row; return true }
        return out
    }

    static func removing(_ path: [String], in rows: [FormatRow]) -> [FormatRow] {
        var out = rows
        within(&out, path) { list, index in list.remove(at: index); return true }
        return out
    }

    static func inserting(_ row: FormatRow, after path: [String], in rows: [FormatRow]) -> [FormatRow] {
        var out = rows
        within(&out, path) { list, index in list.insert(row, at: index + 1); return true }
        return out
    }

    /// At the end inside the item `path`, or at the end of the top when it is empty.
    static func appending(_ row: FormatRow, inside path: [String], in rows: [FormatRow]) -> [FormatRow] {
        guard !path.isEmpty else { return rows + [row] }
        var out = rows
        within(&out, path) { list, index in
            guard list[index].kind == .item else { return false }
            list[index].rows.append(row)
            return true
        }
        return out
    }

    static func moving(_ path: [String], by offset: Int, in rows: [FormatRow]) -> [FormatRow] {
        var out = rows
        within(&out, path) { list, index in
            let to = index + offset
            guard list.indices.contains(to) else { return false }
            list.swapAt(index, to)
            return true
        }
        return out
    }

    /// Into the item above it, at its end: one level deeper. Nil when what is above it
    /// at its level is not an item.
    static func indenting(_ path: [String], in rows: [FormatRow]) -> (rows: [FormatRow], path: [String])? {
        var out = rows
        var moved: [String]?
        within(&out, path) { list, index in
            guard index > 0, list[index - 1].kind == .item else { return false }
            let row = list.remove(at: index)
            guard !list[index - 1].rows.contains(where: { $0.name == row.name }) else {
                list.insert(row, at: index)
                return false
            }
            list[index - 1].rows.append(row)
            moved = Array(path.dropLast()) + [list[index - 1].name, row.name]
            return true
        }
        return moved.map { (out, $0) }
    }

    /// Out of the item it is in, just after it: one level up. Nil at the top.
    static func outdenting(_ path: [String], in rows: [FormatRow]) -> (rows: [FormatRow], path: [String])? {
        guard path.count >= 2 else { return nil }
        let parent = Array(path.dropLast())
        guard let row = row(at: path, in: rows) else { return nil }
        var out = removing(path, in: rows)
        let grand = Array(parent.dropLast())
        let siblings = grand.isEmpty ? out : (self.row(at: grand, in: out)?.rows ?? [])
        guard !siblings.contains(where: { $0.name == row.name }) else { return nil }
        out = inserting(row, after: parent, in: out)
        return (out, grand + [row.name])
    }

    /// A name not yet used in the list: `base`, then `base2`, `base3`…
    static func freeName(_ base: String, in list: [FormatRow]) -> String {
        let taken = Set(list.map(\.name))
        if !taken.contains(base) { return base }
        var n = 2
        while taken.contains("\(base)\(n)") { n += 1 }
        return "\(base)\(n)"
    }

    /// The rows under `path` — the top when it is empty.
    static func children(of path: [String], in rows: [FormatRow]) -> [FormatRow] {
        path.isEmpty ? rows : (row(at: path, in: rows)?.rows ?? [])
    }

    /// What an item says itself, not counting the items inside it: its rows' Markdown
    /// and targets, each with where it is (`login.body`, `fields.target`).
    static func said(by path: [String], in rows: [FormatRow]) -> [(row: [String], place: String, text: String)] {
        children(of: path, in: rows).filter { $0.kind != .item }.flatMap { row -> [([String], String, String)] in
            let at = path + [row.name]
            return row.texts.map { (at, $0.place, $0.text) } + (row.target.isEmpty ? [] : [(at, "target", row.target)])
        }
    }

    // MARK: - From the clauses of a note

    /// A note's clauses, as the project kept them before formatdocs: each clause an
    /// item with its heading, its text a text row inside it named `text` (or the first
    /// free name after it), and the clauses inside it items after that. The names do not
    /// change, so a path to a clause is a path to its item.
    static func fromClauses(_ raw: OJSON?) -> [FormatRow] {
        guard case .array(let list)? = raw else { return [] }
        return list.compactMap { entry -> FormatRow? in
            guard case .object(let clause) = entry, case .string(let name)? = clause["name"] else { return nil }
            let inner = fromClauses(clause["clauses"])
            var rows: [FormatRow] = []
            if case .string(let body)? = clause["body"], !body.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                rows.append(FormatRow(name: freeName("text", in: inner), kind: .text, body: body))
            }
            return FormatRow(name: name, kind: .item, heading: clause["heading"]?.stringOrNil ?? "", rows: rows + inner)
        }
    }

    // MARK: - As a document

    /// The rows as Markdown: each item a heading as deep as it is (the top ones `##`),
    /// each other row what `render` makes of it — text by default as it is.
    static func markdown(_ rows: [FormatRow],
                         render: (FormatRow, [String], Int) -> String? = { row, _, _ in row.kind == .text ? row.body : nil }) -> String {
        var parts: [String] = []
        walk(rows) { row, path, depth in
            if row.kind == .item {
                parts.append(String(repeating: "#", count: min(depth + 2, 6)) + " " + row.title)
            } else if let text = render(row, path, depth)?.trimmingCharacters(in: .newlines), !text.isEmpty {
                parts.append(text)
            }
        }
        return parts.joined(separator: "\n\n")
    }

    /// A table as Markdown: the headings, the line under them, and each row of cells
    /// as wide as the headings.
    static func table(_ columns: [String], _ cells: [[String]], cell: (String) -> String = { $0 }) -> String {
        let width = max(columns.count, cells.map(\.count).max() ?? 0)
        guard width > 0 else { return "" }
        func line(_ items: [String]) -> String {
            let padded = items + Array(repeating: "", count: max(0, width - items.count))
            return "| " + padded.prefix(width).map { cell($0).replacingOccurrences(of: "|", with: "\\|")
                .replacingOccurrences(of: "\n", with: " ") }.joined(separator: " | ") + " |"
        }
        return ([line(columns), "|" + Array(repeating: "---|", count: width).joined()] + cells.map(line))
            .joined(separator: "\n")
    }

    /// The first lines of a text, for a glance at what it says.
    static func head(_ body: String, lines: Int = 3) -> String {
        body.split(separator: "\n", omittingEmptySubsequences: true)
            .prefix(lines).joined(separator: "\n")
    }

    /// What an item says, for a glance: its text rows' first lines.
    static func head(of path: [String], in rows: [FormatRow]) -> String {
        head(children(of: path, in: rows).filter { $0.kind == .text }.map(\.body).joined(separator: "\n"))
    }
}

extension Validate {
    /// Names each a name and unique among their siblings, each row only what its kind
    /// holds, items five levels deep at the most, and not too much of it. Refused whole,
    /// with where and why.
    static func rows(_ raw: [FormatRow]) throws -> [FormatRow] {
        var count = 0
        var text = 0
        let columnsAndCells = ["columns", "cells"].joined(separator: L10n[.Common.separatorList])
        func check(_ list: [FormatRow], at place: String, depth: Int) throws -> [FormatRow] {
            var seen: Set<String> = []
            var out: [FormatRow] = []
            for (index, row) in list.enumerated() {
                let here = "\(place)[\(index)]"
                var next = row
                next.name = try name(row.name, field: "\(here).name")
                guard seen.insert(next.name).inserted else {
                    throw Invalid(L10n[.Docs.validateNameTwice(here, next.name)])
                }
                func refuse(_ key: String, _ present: Bool) throws {
                    if present {
                        throw Invalid(L10n[.Docs.validateRowHasNo(here, row.kind.rawValue, row.kind.label, key)])
                    }
                }
                try refuse("heading", row.kind != .item && !row.heading.isEmpty)
                try refuse("rows", row.kind != .item && !row.rows.isEmpty)
                try refuse("body", row.kind != .text && !row.body.isEmpty)
                try refuse(columnsAndCells, row.kind != .table && (!row.columns.isEmpty || !row.cells.isEmpty))
                try refuse("target", !row.kind.hasTarget && !row.target.isEmpty)
                switch row.kind {
                case .item:
                    guard depth < FormatRows.maxDepth else {
                        throw Invalid(L10n[.Docs.validateItemDepth(here, FormatRows.maxDepth)])
                    }
                    next.heading = try Validate.text(row.heading, field: "\(here).heading", max: 200)
                case .text:
                    next.body = row.body.trimmingCharacters(in: .whitespacesAndNewlines)
                case .table:
                    guard !row.columns.isEmpty else { throw Invalid(L10n[.Docs.validateTableNeedsColumns(here)]) }
                    guard row.columns.count <= 30 else { throw Invalid(L10n[.Docs.validateTableTooWide(here)]) }
                    if let wide = row.cells.firstIndex(where: { $0.count > row.columns.count }) {
                        throw Invalid(L10n[.Docs.validateCellsWider(here, wide, row.columns.count)])
                    }
                case .diagram, .diagrams, .card:
                    next.target = row.target.trimmingCharacters(in: .whitespaces)
                    // A section, said as one: what a section's rename rewrites.
                    if row.kind == .diagrams, !next.target.isEmpty, !next.target.hasSuffix(":") { next.target += ":" }
                    guard !next.target.isEmpty else {
                        let needs: LocalizedStringResource = switch row.kind {
                        case .diagrams: .Docs.validateNeedsTargetDiagrams(here, row.kind.label)
                        case .card: .Docs.validateNeedsTargetCard(here, row.kind.label)
                        default: .Docs.validateNeedsTargetDiagram(here, row.kind.label)
                        }
                        throw Invalid(L10n[needs])
                    }
                }
                count += 1
                text += next.texts.reduce(0) { $0 + $1.text.count }
                guard count <= FormatRows.maxCount else { throw Invalid(L10n[.Docs.validateTooManyRows(FormatRows.maxCount)]) }
                guard text <= FormatRows.maxText else { throw Invalid(L10n[.Docs.validateTooMuchText(FormatRows.maxText)]) }
                if row.kind == .item { next.rows = try check(row.rows, at: "\(here).rows", depth: depth + 1) }
                out.append(next)
            }
            return out
        }
        return try check(raw, at: "rows", depth: 0)
    }
}

/// A row of a formatdoc, or a note or freedoc taken whole, as the list of what is
/// connected to an item shows it (`specboard://SpecBoard/Docs/Design:DesignActions`「影響」).
struct RowLink: Hashable {
    var note: SchemaNode
    /// The item's names in the formatdoc; empty for a card taken whole.
    var path: [String]
    /// As the project writes it, from the top: `Docs:Spec.auth.login`.
    var written: String
    var title: String
    /// The first lines of what it says.
    var head: String
}

enum RowLinks {
    /// What an item's rows point at that is a document card or an item, and the items —
    /// or notes and freedocs — whose text points at it or at something inside it.
    static func connected(_ detail: ProjectDetail, note: SchemaNode,
                          path: [String]) -> (mentions: [RowLink], mentionedBy: [RowLink]) {
        let directory = detail.directory
        guard FormatRows.row(at: path, in: note.rows) != nil else { return ([], []) }

        func link(_ holder: SchemaNode, _ at: [String]) -> RowLink? {
            let written = directory.full(holder, at)
            if at.isEmpty {
                let text = holder.kind == .formatdoc ? FormatRows.markdown(holder.rows)
                    : holder.kind == .stackdoc ? holder.openVersion?.body ?? "" : holder.body
                return RowLink(note: holder, path: [], written: written,
                               title: holder.label.isEmpty ? holder.name : holder.label,
                               head: FormatRows.head(text))
            }
            guard let found = FormatRows.row(at: at, in: holder.rows) else { return nil }
            return RowLink(note: holder, path: at, written: written, title: found.title,
                           head: FormatRows.head(of: at, in: holder.rows))
        }
        /// Where a `[[path]]` written in `holder` lands, when that is a document card or
        /// an item.
        func target(_ raw: String, in holder: SchemaNode) -> (SchemaNode, [String])? {
            guard let landed = landing(raw, from: directory.sectionId(of: holder), in: directory),
                  landed.schema.kind.isWriting else { return nil }
            let at = PathText.names(landed.row)
            guard landed.schema.kind == .formatdoc, !at.isEmpty else { return (landed.schema, []) }
            let (found, _) = FormatRows.deepest(at, in: landed.schema.rows)
            let item = FormatRows.item(of: found, in: landed.schema.rows)
            return item.isEmpty ? (landed.schema, []) : (landed.schema, item)
        }

        var mentions: [RowLink] = []
        var seen = Set<String>()
        for said in FormatRows.said(by: path, in: note.rows) {
            for ref in NoteText.refs(said.text) where ref.kind == .place {
                guard let (holder, at) = target(ref.path, in: note),
                      !(holder.id == note.id && at == path),
                      let found = link(holder, at), seen.insert(found.written).inserted else { continue }
                mentions.append(found)
            }
        }

        var mentionedBy: [RowLink] = []
        var cited = Set<String>()
        func points(_ text: String, from holder: SchemaNode) -> Bool {
            NoteText.refs(text).contains { ref in
                guard ref.kind == .place, let (landed, at) = target(ref.path, in: holder) else { return false }
                return landed.id == note.id && at.starts(with: path)
            }
        }
        for other in detail.schemas where other.kind.isWriting {
            guard other.kind == .formatdoc else {
                let text = other.kind == .stackdoc ? other.versions.map(\.body).joined(separator: "\n") : other.body
                if other.id != note.id, points(text, from: other),
                   let found = link(other, []), cited.insert(found.written).inserted {
                    mentionedBy.append(found)
                }
                continue
            }
            FormatRows.walk(other.rows) { row, at, _ in
                guard row.kind == .item, !(other.id == note.id && at.starts(with: path)),
                      FormatRows.said(by: at, in: other.rows).contains(where: { points($0.text, from: other) }),
                      let found = link(other, at), cited.insert(found.written).inserted else { return }
                mentionedBy.append(found)
            }
        }
        return (mentions, mentionedBy)
    }
}
