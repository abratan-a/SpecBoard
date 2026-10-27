import AppKit

/// A data card its shape draws as a list of records (`specboard://SpecBoard/Docs/Design:CardKinds`
/// 「データの一覧の顔」): the core works every row out (`listFace` of `project_detail`'s card) and the screen draws them — a row an
/// element of the list, its mark at its start, its text and a note after it, and under it, opened with its ›, the
/// lines its fold opens to. The core counts the card's height from these rows, every one folded.
nonisolated struct ListFace: Decodable, Hashable, Sendable {
    /// What a row's mark says: each role a symbol and a colour, whatever values a shape maps to it.
    enum Role: String, CaseIterable, Decodable, Sendable {
        case ok, bad, muted, hollow

        var symbol: String {
            switch self {
            case .ok: "✓"
            case .bad: "✗"
            case .muted: "–"
            case .hollow: "○"
            }
        }

        /// The role's colour on a card of `theme`.
        func color(_ theme: GraphTheme) -> NSColor {
            switch self {
            case .ok: EdgeInk.green.color(dark: theme.isDark)
            case .bad: EdgeInk.red.color(dark: theme.isDark)
            case .muted, .hollow: theme.ink3
            }
        }
    }

    /// A field of the head: its name, its label and its value as the head says it (an enum's label); `chip` for
    /// an enum's value, drawn as a chip on the head's right as a data row's type is (`WholeCard.shown` reads it
    /// off the shape).
    struct Head: Decodable, Hashable, Sendable {
        var field: String
        var label: String = ""
        var text: String
        var chip = false

        private enum Keys: String, CodingKey { case field, label, text }

        init(field: String, label: String = "", text: String, chip: Bool = false) {
            self.field = field
            self.label = label
            self.text = text
            self.chip = chip
        }

        init(from decoder: Decoder) throws {
            let c = try decoder.container(keyedBy: Keys.self)
            field = try c.decode(String.self, forKey: .field)
            label = try c.decodeIfPresent(String.self, forKey: .label) ?? field
            text = try c.decodeIfPresent(String.self, forKey: .text) ?? ""
        }
    }

    /// How many rows wear a role's mark (none counted are left out).
    struct Count: Decodable, Hashable, Sendable {
        var role: Role
        var count: Int
    }

    /// A line under a row opened: how far it is set in, the heading of a fold field on its first line, its text.
    struct FoldLine: Decodable, Hashable, Sendable {
        var depth: Int
        var label: String?
        var text: String
    }

    /// One element's row: where the screen's rows name it (`tests[0]`), its path by its key (`tests[name=x]`),
    /// its text — its key or its place where its shape says none (`fallback`) — its note, its mark, the lines
    /// its fold opens to, and whether it opens as the data tree instead (an element of a shape with no face).
    struct Row: Decodable, Hashable, Sendable {
        var at: String
        var path: String
        var text: String
        var fallback = false
        var note: String?
        var mark: Role?
        var fold: [FoldLine] = []
        var tree = false

        private enum Keys: String, CodingKey { case at, path, text, fallback, note, mark, fold, tree }

        init(at: String, path: String? = nil, text: String, fallback: Bool = false, note: String? = nil,
             mark: Role? = nil, fold: [FoldLine] = [], tree: Bool = false) {
            self.at = at
            self.path = path ?? at
            self.text = text
            self.fallback = fallback
            self.note = note
            self.mark = mark
            self.fold = fold
            self.tree = tree
        }

        init(from decoder: Decoder) throws {
            let c = try decoder.container(keyedBy: Keys.self)
            at = try c.decode(String.self, forKey: .at)
            path = try c.decodeIfPresent(String.self, forKey: .path) ?? at
            text = try c.decodeIfPresent(String.self, forKey: .text) ?? ""
            fallback = try c.decodeIfPresent(Bool.self, forKey: .fallback) ?? false
            note = try c.decodeIfPresent(String.self, forKey: .note)
            // A role this build does not know draws no mark rather than leaving the project unread.
            mark = (try? c.decodeIfPresent(String.self, forKey: .mark)).flatMap(Role.init(rawValue:))
            fold = try c.decodeIfPresent([FoldLine].self, forKey: .fold) ?? []
            tree = try c.decodeIfPresent(Bool.self, forKey: .tree) ?? false
        }
    }

    var list: String
    var header: [Head] = []
    var counts: [Count] = []
    var rows: [Row] = []

    private enum Keys: String, CodingKey { case list, header, counts, rows }

    init(list: String, header: [Head] = [], counts: [Count] = [], rows: [Row] = []) {
        self.list = list
        self.header = header
        self.counts = counts
        self.rows = rows
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        list = try c.decode(String.self, forKey: .list)
        header = try c.decodeIfPresent([Head].self, forKey: .header) ?? []
        counts = ((try? c.decodeIfPresent([LossyCount].self, forKey: .counts)) ?? []).compactMap(\.count)
        rows = try c.decodeIfPresent([Row].self, forKey: .rows) ?? []
    }

    /// A count whose role this build may not know: left out then.
    private struct LossyCount: Decodable {
        var count: Count?
        init(from decoder: Decoder) throws { count = try? Count(from: decoder) }
    }

    /// Whether any row wears a mark: the rows keep a column for it then, so their texts start in one line.
    var marks: Bool { rows.contains { $0.mark != nil } }
}

/// The line a list row draws for: an element's row, or a line its fold opens to.
enum ListedRow: Hashable {
    case entry(ListFace.Row)
    case fold(ListFace.FoldLine)
}

enum ListFaceRows {
    /// Where a fold line of the row at `at` is among the card's rows: apart from any place in the data, so no line
    /// lands on it — a line into the opened element lands on its row.
    static func foldPath(_ at: String, _ index: Int) -> String { "\(at)#fold\(index)" }

    /// A data card's rows by its list face: a row an element, each folded until opened (`startsFolded`, as the core
    /// counts the card); opened, the lines its fold opens to, or for an element whose shape has no face its fields
    /// as the data tree draws them. A row stands for its path by its key too, where a line or a code dot names it so.
    static func rows(_ card: SchemaNode, face: ListFace, lookup: SchemaLookup?, folded: Set<String>) -> [CardRow] {
        let shape = card.instanceOf.flatMap { lookup?($0) }
        let field = shape?.fields.first { $0.name == face.list }
        let element = field.map { InstanceRows.element($0, lookup: lookup) }
        let items = card.data?.asObject?[face.list]?.arrayValue ?? []
        var out: [CardRow] = []
        for (index, row) in face.rows.enumerated() {
            let item = items.indices.contains(index) ? items[index] : nil
            let tree = row.tree ? (element?.fields ?? []) : []
            let opens = !row.fold.isEmpty || (!tree.isEmpty && item?.asObject != nil)
            let shut = opens && folded.contains(row.at)
            out.append(CardRow(path: row.at, depth: 0, field: nil, value: nil, fromType: nil,
                               expandable: opens, folded: shut, startsFolded: opens,
                               stands: row.path == row.at ? [] : [row.path], listed: .entry(row)))
            guard opens, !shut else { continue }
            if row.fold.isEmpty {
                let seen = Set([shape?.id, element?.shape?.id].compactMap { $0 })
                out += InstanceRows.tree(tree, values: item?.asObject, depth: 1, prefix: row.at, seen: seen,
                                         lookup: lookup, folded: folded)
            } else {
                for (i, line) in row.fold.enumerated() {
                    out.append(CardRow(path: foldPath(row.at, i), depth: 1 + line.depth, field: nil, value: nil,
                                       fromType: nil, listed: .fold(line)))
                }
            }
        }
        return out
    }

    /// What the counts of a head say on the pointer: a line saying so, then each mark by its symbol and the values
    /// its element shape gives it (`values`, by role: the face's `mark`, as the values are shown), and how many rows
    /// wear it. A role is no word of the screen's: what a mark means is the values the shape gives it.
    static func countsTip(_ face: ListFace, values: [ListFace.Role: [String]] = [:]) -> String? {
        guard !face.counts.isEmpty else { return nil }
        let code = Language.current
        let lines = face.counts.map { count -> String in
            let said = ([count.role.symbol] + (values[count.role] ?? [])).joined(separator: " ")
            return L10n.text(.Diagram.listFaceCountsTipLine(said, count.count), in: code)
        }
        return ([L10n.text(.Diagram.listFaceCountsTipTitle, in: code)] + lines).joined(separator: "\n")
    }

    /// What a row says on the pointer, cut short as it may be on the card: its text and its note, a line each; a
    /// fold line its heading and its text.
    static func tip(_ row: ListedRow) -> String {
        switch row {
        case .entry(let entry):
            return [entry.text, entry.note].compactMap { $0 }.filter { !$0.isEmpty }.joined(separator: "\n")
        case .fold(let line):
            return [line.label, line.text].compactMap { $0 }.filter { !$0.isEmpty }.joined(separator: " ")
        }
    }

    /// The values a data card's element shape gives each mark, as they are shown, in its enum's order.
    static func markValues(_ card: SchemaNode, lookup: @escaping SchemaLookup) -> [ListFace.Role: [String]] {
        guard let face = card.listFace, let shape = card.instanceOf.flatMap(lookup),
              let field = shape.fields.first(where: { $0.name == face.list }),
              let element = InstanceRows.element(field, lookup: lookup).shape,
              let shapeFace = element.shapeFace else { return [:] }
        var out: [ListFace.Role: [String]] = [:]
        for line in ShapeFaceLines.marked(shapeFace, values: ShapeFaceLines.markEnum(element, lookup: lookup)) {
            if let role = line.role { out[role, default: []].append(line.text) }
        }
        return out
    }
}

/// A shape's list face as the property panel says it, read only (it is written over MCP): a line each of what it
/// names — the list, the head's fields, a row's text and note, the mark's field and each value with the mark it
/// gives, what a row's › opens.
enum ShapeFaceLines {
    struct Line: Hashable {
        /// The face's key the line says (`list`, `mark`); empty for a value under the mark.
        var key: String
        /// What it says: for a value under the mark, the value as it is shown (an enum's label), and 値なし for none.
        var text: String
        /// For a value under the mark: the value as written, none for the line of no value.
        var value: String? = nil
        /// For a value under the mark: the mark it gives (its symbol and colour; the role is not said).
        var role: ListFace.Role? = nil
    }

    /// An enum's value and how it is shown: its label, or the value when it has none.
    typealias Shown = (value: String, label: String)

    /// `face` as its lines, a mark's values in the order its enum lists them (`values`), each as it shows it.
    static func lines(_ face: JSONValue, values: [Shown] = []) -> [Line] {
        guard let face = face.asObject else { return [] }
        func names(_ key: String) -> String? {
            switch face[key] {
            case .string(let name)?: return name
            case .array(let items)?:
                let said = items.compactMap(\.asText)
                return said.isEmpty ? nil : said.joined(separator: ", ")
            default: return nil
            }
        }
        var out: [Line] = []
        for key in ["list", "header", "text", "note"] {
            if let said = names(key) { out.append(Line(key: key, text: said)) }
        }
        if let mark = face["mark"]?.asObject {
            out.append(Line(key: "mark", text: mark["field"]?.asText ?? ""))
            out += marked(.object(face), values: values)
        }
        if let said = names("fold") { out.append(Line(key: "fold", text: said)) }
        return out
    }

    /// A face's mark as a line each value it maps (in the enum's order, `values`, the rest after in their own),
    /// each as it is shown, then the line of no value (`none`) as 値なし.
    static func marked(_ face: JSONValue, values: [Shown] = []) -> [Line] {
        guard let mark = face.asObject?["mark"]?.asObject else { return [] }
        let given = mark["values"]?.asObject ?? [:]
        let order = values.map(\.value)
        let keys = order.filter { given[$0] != nil } + given.keys.filter { !order.contains($0) }.sorted()
        var out: [Line] = []
        for key in keys {
            let role = given[key]?.asText.flatMap(ListFace.Role.init(rawValue:))
            let label = values.first { $0.value == key }?.label ?? key
            out.append(Line(key: "", text: label, value: key, role: role))
        }
        if let role = mark["none"]?.asText.flatMap(ListFace.Role.init(rawValue:)) {
            out.append(Line(key: "", text: L10n.text(.Diagram.listFaceNoValue, in: Language.current), role: role))
        }
        return out
    }

    /// The values of the enum a shape's mark is read from, in its order, each with how it is shown: the order the
    /// panel and the counts list them in.
    static func markEnum(_ shape: SchemaNode, lookup: SchemaLookup) -> [Shown] {
        guard let name = shape.shapeFace?.asObject?["mark"]?.asObject?["field"]?.asText,
              let field = shape.fields.first(where: { $0.name == name }),
              let target = field.embeds.flatMap(lookup), target.kind == .enum else { return [] }
        return target.values.map { ($0.value, $0.label.isEmpty ? $0.value : $0.label) }
    }
}
