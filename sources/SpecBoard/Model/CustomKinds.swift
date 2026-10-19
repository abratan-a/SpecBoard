import Foundation

/// A custom card's row (`specboard://SpecBoard/Docs/Design:DesignBoard` §3): of one of its kind's row types,
/// with a short name the project points at it by (`t1`), what it says, whether it is
/// done, the cards it is joined to, and the rows under it.
nonisolated struct CustomRow: Codable, Hashable, Sendable {
    var type: String
    var name: String
    var text: String = ""
    var done: Bool = false
    var links: [String] = []
    var rows: [CustomRow] = []
    /// A container's row for a card inside it (`as_row`): whether it is drawn with
    /// a check — its kind says a field that means done — the other choices of its `as_row`, each a
    /// row of buttons under it, and its other fields written, said after its text. Not written back.
    var check = true
    var choices: [Choice] = []
    var more = ""

    /// A choice of a card inside a container, drawn under its row.
    struct Choice: Hashable, Sendable {
        var field: String
        var label: String
        var options: [String]
        var chosen: String
        /// How each option is shown, when not as it is (`optionLabels`).
        var labels: [String: String] = [:]
    }

    private enum CodingKeys: String, CodingKey { case type, name, text, done, links, rows }

    init(type: String, name: String, text: String = "", done: Bool = false, links: [String] = [],
         rows: [CustomRow] = []) {
        self.type = type
        self.name = name
        self.text = text
        self.done = done
        self.links = links
        self.rows = rows
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        type = try c.decodeIfPresent(String.self, forKey: .type) ?? ""
        name = try c.decodeIfPresent(String.self, forKey: .name) ?? ""
        text = try c.decodeIfPresent(String.self, forKey: .text) ?? ""
        done = try c.decodeIfPresent(Bool.self, forKey: .done) ?? false
        links = try c.decodeIfPresent([String].self, forKey: .links) ?? []
        rows = try c.decodeIfPresent([CustomRow].self, forKey: .rows) ?? []
    }

    /// Written short: what is empty is left out.
    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(type, forKey: .type)
        try c.encode(name, forKey: .name)
        if !text.isEmpty { try c.encode(text, forKey: .text) }
        if done { try c.encode(done, forKey: .done) }
        if !links.isEmpty { try c.encode(links, forKey: .links) }
        if !rows.isEmpty { try c.encode(rows, forKey: .rows) }
    }
}

/// Walking and reading a custom card's rows.
nonisolated enum CustomRows {
    /// Every row, depth first, with its path: the names from the top, as a path writes them
    /// (`t1.'next step'`).
    static func walk(_ rows: [CustomRow], _ body: (CustomRow, String) -> Void) {
        func go(_ list: [CustomRow], _ prefix: String) {
            for row in list {
                let path = prefix.isEmpty ? PathText.quote(row.name) : PathText.member(prefix, row.name)
                body(row, path)
                go(row.rows, path)
            }
        }
        go(rows, "")
    }

    /// The row at a path, if there is one.
    static func row(at path: String, in rows: [CustomRow]) -> CustomRow? {
        var found: CustomRow?
        walk(rows) { row, at in if at == path { found = row } }
        return found
    }

    /// Done of all, among the rows under a row that count (`done` in their type), or
    /// nil when none count — a row's progress, `2/3`.
    static func progress(_ rows: [CustomRow], counting: (CustomRow) -> Bool) -> (done: Int, of: Int)? {
        let counted = rows.filter(counting)
        guard !counted.isEmpty else { return nil }
        return (counted.filter(\.done).count, counted.count)
    }

    /// Names for the rows that have none, `t1`, `t2`… none already taken on the card.
    static func named(_ rows: [CustomRow]) -> [CustomRow] {
        var taken = Set<String>()
        walk(rows) { row, _ in if !row.name.isEmpty { taken.insert(row.name) } }
        var next = 1
        func fresh() -> String {
            while taken.contains("t\(next)") { next += 1 }
            let name = "t\(next)"
            taken.insert(name)
            return name
        }
        func go(_ list: [CustomRow]) -> [CustomRow] {
            list.map { row in
                var row = row
                if row.name.trimmingCharacters(in: .whitespaces).isEmpty { row.name = fresh() }
                row.rows = go(row.rows)
                return row
            }
        }
        return go(rows)
    }
}

/// The definition of a custom kind (`specboard://SpecBoard/Docs/Design:DesignBoard` §3): how its card is drawn
/// and what may be written on it. Only the built-in goal for now; the project's own
/// definitions come later.
nonisolated struct CustomKind: Sendable {
    /// A field: one value on the card. Its name is what MCP writes it by, its label what a person
    /// reads (in the screen's language, from the kinds' definitions the core reads).
    struct Field: Sendable {
        var fieldtype: String
        var name: String
        var description: String = ""
        var options: [String] = []
        /// How a value of `options` is shown, when not as it is.
        var optionLabels: [String: String] = [:]
        var defaultValue: String? = nil
        var required = false
        var label: String = ""

        /// What a person reads it by: its label, else its name.
        var shown: String { label.isEmpty ? name : label }
    }

    /// A row type: rows of it are written as many times as wanted.
    struct RowType: Sendable {
        var fieldtype: String
        var name: String
        var description: String = ""
        var connectable = true
        /// How many cards a row of it may be joined to; 0 is any number.
        var connectMax = 0
        /// The kinds a row of it may be joined to; empty is any. `spec` is a card of
        /// the specification (Docs・Data・Behavior).
        var connectTo: [String] = []
        /// The row types that may go under a row of it; nil is any.
        var children: [String]? = nil
        /// Whether a row of it counts towards its parent's progress when done.
        var done = false
    }

    var kind: SchemaKind
    /// The name it is defined by, without `custom_`.
    var name: String
    /// The header's colour, `#RRGGBB`.
    var color: String
    var connectable = true
    var connectMax = 0
    var nest = false
    var hasnest = true
    /// The fields shown in the header beside the name.
    var header: [String] = []
    var fields: [Field]
    var rows: [RowType]

    /// How deep rows go under rows.
    static let deepest = 5

    /// The field types there are, and the ones goal uses so far.
    static let fieldTypes = ["text", "longtext", "checkbox", "radio", "dropdownlist", "number", "date", "url",
                             "person", "card", "branch", "mr"]

    /// The built-in goal: a target, the smaller ones under it as rows, each joined to
    /// what says how — a freedoc, a note, another goal, a card of the specification. Its fields by
    /// their names alone: their words, choices and defaults are the kinds' definitions the core reads
    /// (`fields(of:in:)`), in the screen's language — the one place they are written.
    static let goal = CustomKind(
        kind: .goal, name: "goal", color: "#3B82F6",
        connectable: true, connectMax: 0, nest: false, hasnest: false,
        header: ["status"],
        fields: [
            Field(fieldtype: "radio", name: "status"),
            Field(fieldtype: "radio", name: "priority"),
            Field(fieldtype: "date", name: "due"),
            Field(fieldtype: "branch", name: "workBranch"),
            Field(fieldtype: "mr", name: "MR"),
        ],
        rows: [
            RowType(fieldtype: "checkbox", name: WholeCard.heldCard,
                    connectable: true, connectMax: 0, connectTo: ["freedoc", "note", "goal", "spec"],
                    children: [], done: true),
        ])

    static let all: [CustomKind] = [goal]

    /// A task's fields, as the property panel writes them (the kinds' definition, `task`):
    /// a task is no custom card of its own (it is drawn as its goal's row, and by its kind's
    /// face on a card map), but its fields are written as a goal's are. Whether it is done, and
    /// its body in its card editor: the order it is done in is its place in
    /// its goal.
    static let taskFields: [Field] = [
        Field(fieldtype: "checkbox", name: "done"),
    ]

    /// The fields the property panel writes on a card of `kind`: a goal's, a task's.
    static func fields(of kind: SchemaKind) -> [Field] {
        switch kind {
        case .goal: goal.fields
        case .task: taskFields
        default: []
        }
    }

    /// The same, read from the kinds' definitions the project has (`kinds.cards`): a goal's, a task's and
    /// a UI part's fields but those every kind has, those written in the card editor alone (its
    /// body) and those only an action writes (a part's `reviewedAt`) — as written here (`fields(of:)`) while
    /// none is read.
    static func fields(of kind: SchemaKind, in kinds: CardKinds?) -> [Field] {
        guard kind == .goal || kind == .task || kind.isUiPart, let def = kinds?[kind.rawValue] else { return fields(of: kind) }
        return def.ownFields.filter { $0.show != "editor" && $0.show != "action" }.map { field in
            Field(fieldtype: field.fieldtype, name: field.name, description: field.description, options: field.options,
                  optionLabels: field.optionLabels, defaultValue: field.defaultValue?.asText, required: field.required,
                  label: field.label)
        }
    }

    static func of(_ kind: SchemaKind) -> CustomKind? { all.first { $0.kind == kind } }

    func field(_ name: String) -> Field? { fields.first { $0.name == name } }
    func rowType(_ name: String) -> RowType? { rows.first { $0.name == name } }

    /// The values a card starts with: each field's default.
    var defaults: [String: String] {
        Dictionary(fields.compactMap { field in field.defaultValue.map { (field.name, $0) } }, uniquingKeysWith: { a, _ in a })
    }

    /// Whether a row counts towards its parent's progress.
    func counts(_ row: CustomRow) -> Bool { rowType(row.type)?.done ?? false }

    /// What is wrong with values and rows written for a card of this kind, each said
    /// where it is — every one, not the first. Paths and branches are checked
    /// where the project is known.
    func problems(fields given: [String: String], rows: [CustomRow]) -> [String] {
        var out: [String] = []
        let language = Language.current
        let separator = L10n.text(.Common.separatorList, in: language)
        for (name, value) in given.sorted(by: { $0.key < $1.key }) {
            guard let field = field(name) else {
                let has = fields.map(\.name).joined(separator: separator)
                out.append("fields.\(name): " + L10n.text(.Kinds.validationNoField(self.name, name, has), in: language))
                continue
            }
            if let problem = Self.problem(field, value) { out.append("fields.\(name): \(problem)") }
        }
        for field in fields where field.required && (given[field.name] ?? "").isEmpty {
            out.append("fields.\(field.name): " + L10n.text(.Kinds.validationRequired(field.name), in: language))
        }
        var names = Set<String>()
        func check(_ list: [CustomRow], _ at: String, depth: Int, under parent: RowType?) {
            for (index, row) in list.enumerated() {
                let here = "\(at)[\(index)]"
                guard let type = rowType(row.type) else {
                    let has = rows.map(\.name).joined(separator: separator)
                    out.append("\(here).type: " + L10n.text(.Kinds.validationNoRowKind(self.name, row.type, has), in: language))
                    continue
                }
                if let parent, let allowed = parent.children, !allowed.contains(row.type) {
                    let only = allowed.joined(separator: separator)
                    out.append("\(here).type: " + L10n.text(.Kinds.validationNotUnder(parent.name, only, row.type), in: language))
                }
                if depth > Self.deepest {
                    out.append("\(here): " + L10n.text(.Kinds.validationTooDeep(Self.deepest), in: language))
                }
                if !row.name.isEmpty, !names.insert(row.name).inserted {
                    out.append("\(here).name: " + L10n.text(.Kinds.validationNameTaken(row.name), in: language))
                }
                if !row.name.isEmpty, PathText.nameProblem(row.name) != nil {
                    out.append("\(here).name: " + L10n.text(.Kinds.validationNameChars(row.name), in: language))
                }
                if !type.connectable, !row.links.isEmpty {
                    out.append("\(here).links: " + L10n.text(.Kinds.validationNoLinks(type.name), in: language))
                }
                if type.connectMax > 0, row.links.count > type.connectMax {
                    let most = L10n.text(.Kinds.validationLinkCount(type.connectMax), in: language)
                    let has = L10n.text(.Kinds.validationLinkCount(row.links.count), in: language)
                    out.append("\(here).links: " + L10n.text(.Kinds.validationTooManyLinks(type.name, most, has), in: language))
                }
                if !hasnest, !row.rows.isEmpty {
                    out.append("\(here).rows: " + L10n.text(.Kinds.validationNoNesting(self.name), in: language))
                }
                check(row.rows, "\(here).rows", depth: depth + 1, under: type)
            }
        }
        check(rows, "rows", depth: 1, under: nil)
        return out
    }

    /// Why a value cannot be a field's, or nil.
    static func problem(_ field: Field, _ value: String) -> String? {
        guard !value.isEmpty else { return nil }
        let language = Language.current
        switch field.fieldtype {
        case "radio", "dropdownlist":
            let options = field.options.joined(separator: L10n.text(.Common.separatorList, in: language))
            return field.options.contains(value) ? nil
                : L10n.text(.Kinds.validationNotChoice(value, options), in: language)
        case "date":
            let parts = value.split(separator: "-")
            let ok = parts.count == 3 && parts[0].count == 4 && parts[1].count == 2 && parts[2].count == 2
                && parts.allSatisfy { $0.allSatisfy(\.isNumber) }
            return ok ? nil : L10n.text(.Kinds.validationDateFormat(value), in: language)
        case "number", "mr":
            return Double(value) != nil ? nil : L10n.text(.Kinds.validationNumberFormat(value), in: language)
        case "checkbox":
            return ["true", "false"].contains(value) ? nil : L10n.text(.Kinds.validationBoolFormat(value), in: language)
        default:
            return nil
        }
    }

    /// The definition as MCP reads it (`guide`, `project_read`): the shape a person wrote.
    var json: OJSON {
        func field(_ f: Field) -> OJSON {
            var out = OrderedObject([("fieldtype", .string(f.fieldtype)), ("name", .string(f.name))])
            if !f.description.isEmpty { out["description"] = .string(f.description) }
            if !f.options.isEmpty { out["options"] = .array(f.options.map(OJSON.string)) }
            if let value = f.defaultValue { out["default"] = .string(value) }
            if f.required { out["required"] = .bool(true) }
            return .object(out)
        }
        func row(_ r: RowType) -> OJSON {
            var out = OrderedObject([("fieldtype", .string(r.fieldtype)), ("name", .string(r.name))])
            if !r.description.isEmpty { out["description"] = .string(r.description) }
            out["connectable"] = .bool(r.connectable)
            out["connect_max"] = .number(Double(r.connectMax))
            if !r.connectTo.isEmpty { out["connect_to"] = .array(r.connectTo.map(OJSON.string)) }
            if let children = r.children { out["children"] = .array(children.map(OJSON.string)) }
            if r.done { out["done"] = .bool(true) }
            return .object(out)
        }
        return .object(OrderedObject([
            ("kind", .string(name)), ("color", .string(color)),
            ("connectable", .bool(connectable)), ("connect_max", .number(Double(connectMax))),
            ("nest", .bool(nest)), ("hasnest", .bool(hasnest)),
            ("header", .array(header.map(OJSON.string))),
            ("fields", .array(fields.map(field))),
            ("rows", .array(rows.map(row))),
        ]))
    }
}

/// Changes to a custom card's rows, by path (`t1.t3`, as `CustomRows.walk` writes it): what the
/// card editor and the property panel do, apart from the view so a test can do them too.
nonisolated enum CustomRowEditing {
    /// The parent's path and the name (as it is) of the row at `path`.
    static func locate(_ path: String) -> (parent: String, name: String) {
        guard let (parent, name) = PathText.lastDot(path) else { return ("", PathText.unquote(path)) }
        return (parent, PathText.unquote(name))
    }

    /// The rows with the list under `parent` ("" for the top) changed by `body`.
    static func changing(_ rows: [CustomRow], under parent: String, _ body: (inout [CustomRow]) -> Void) -> [CustomRow] {
        guard !parent.isEmpty else {
            var list = rows
            body(&list)
            return list
        }
        let names = PathText.names(parent)
        func go(_ list: [CustomRow], _ depth: Int) -> [CustomRow] {
            list.map { row in
                guard row.name == names[depth] else { return row }
                var row = row
                if depth == names.count - 1 { body(&row.rows) } else { row.rows = go(row.rows, depth + 1) }
                return row
            }
        }
        return go(rows, 0)
    }

    /// The row at `path` changed by `body`.
    static func update(_ rows: [CustomRow], at path: String, _ body: (inout CustomRow) -> Void) -> [CustomRow] {
        let (parent, name) = locate(path)
        return changing(rows, under: parent) { list in
            if let index = list.firstIndex(where: { $0.name == name }) { body(&list[index]) }
        }
    }

    /// A new row after the one at `path` (or at the end of the top, for nil), of `type`.
    static func add(_ rows: [CustomRow], after path: String?, type: String, text: String = "") -> (rows: [CustomRow], path: String) {
        let named = CustomRows.named(rows + [CustomRow(type: type, name: "")])
        let fresh = named.last!.name
        let row = CustomRow(type: type, name: fresh, text: text)
        guard let path else { return (rows + [row], PathText.quote(fresh)) }
        let (parent, name) = locate(path)
        let out = changing(rows, under: parent) { list in
            let at = (list.firstIndex { $0.name == name }).map { $0 + 1 } ?? list.count
            list.insert(row, at: at)
        }
        return (out, parent.isEmpty ? PathText.quote(fresh) : PathText.member(parent, fresh))
    }

    /// The row at `path` taken out, with what is under it.
    static func remove(_ rows: [CustomRow], at path: String) -> [CustomRow] {
        let (parent, name) = locate(path)
        return changing(rows, under: parent) { list in list.removeAll { $0.name == name } }
    }

    /// The row at `path` one place up or down among its siblings.
    static func move(_ rows: [CustomRow], at path: String, by offset: Int) -> [CustomRow] {
        let (parent, name) = locate(path)
        return changing(rows, under: parent) { list in
            guard let index = list.firstIndex(where: { $0.name == name }) else { return }
            let to = index + offset
            guard list.indices.contains(to) else { return }
            list.swapAt(index, to)
        }
    }

    /// The row at `path` under the row above it, last there. Its new path, or nil when
    /// there is no row above it.
    static func indent(_ rows: [CustomRow], at path: String) -> (rows: [CustomRow], path: String)? {
        let (parent, name) = locate(path)
        var moved: String?
        let out = changing(rows, under: parent) { list in
            guard let index = list.firstIndex(where: { $0.name == name }), index > 0 else { return }
            let row = list.remove(at: index)
            list[index - 1].rows.append(row)
            let above = parent.isEmpty ? PathText.quote(list[index - 1].name) : PathText.member(parent, list[index - 1].name)
            moved = PathText.member(above, name)
        }
        return moved.map { (out, $0) }
    }

    /// The row at `path` out from under its parent, right after it. Its new path, or
    /// nil at the top.
    static func outdent(_ rows: [CustomRow], at path: String) -> (rows: [CustomRow], path: String)? {
        let (parent, name) = locate(path)
        guard !parent.isEmpty, let row = CustomRows.row(at: path, in: rows) else { return nil }
        let taken = remove(rows, at: path)
        let (grand, parentName) = locate(parent)
        let out = changing(taken, under: grand) { list in
            let at = (list.firstIndex { $0.name == parentName }).map { $0 + 1 } ?? list.count
            list.insert(row, at: at)
        }
        return (out, grand.isEmpty ? PathText.quote(name) : PathText.member(grand, name))
    }
}
