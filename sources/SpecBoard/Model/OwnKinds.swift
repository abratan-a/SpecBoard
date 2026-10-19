import Foundation

/// The kinds of card a project has, as their definitions say them whole (`project_detail`'s
/// `kinds.cards`, `specboard://SpecBoard/Docs/Design:CardKinds`「カードの種類の定義」): the built-in ones (`builtin`), then
/// the project's own. Each with the fields and rows it takes from its parents first. A card of one of the
/// project's own is drawn and written by its definition (`SchemaKind.custom`, `SchemaNode.kindDef`); the screen
/// holds no table of its own.
nonisolated struct CardKinds: Hashable, Sendable {
    var list: [CardKindDef] = []

    init(_ list: [CardKindDef] = []) { self.list = list }

    /// The kind of that name; none for one the project does not have.
    subscript(name: String) -> CardKindDef? { list.first { $0.kind == name } }

    /// The project's own, in the order it defines them.
    var own: [CardKindDef] { list.filter { !$0.builtin } }

    /// The project's own kinds a section holds, by what it holds (`holds` of the section's kind:
    /// a name, or `*` for any).
    func own(heldBy holds: [String]) -> [CardKindDef] {
        own.filter { holds.contains("*") || holds.contains($0.kind) }
    }
}

/// One kind of card, as its definition says it whole.
nonisolated struct CardKindDef: Decodable, Hashable, Sendable {
    var kind: String
    var label: String
    /// The header's colour, `#RRGGBB`.
    var color: String?
    /// The fields drawn in the header beside the name.
    var header: [String]
    /// What a card of it says as a row of the container it is inside (`as_row`): its fields, in
    /// order; none when it says none.
    /// `nest` is no more read: what goes inside a container is the container's `holds`.
    var asRow: [String]
    /// `as_row: unfold`: what it holds — the cards inside it, or its fields written — nested under its row.
    var unfolds: Bool
    var connectable: Bool
    /// What pressing a card of it twice on a diagram does when it is not the plain one:
    /// `edit_in_place`, its body written where it is (a note). `on_click` is no more read: what a press changes is
    /// a field's, by its type, where the card draws it (`face`'s `top`, its rows, `as_row`).
    var onDoubleClick: String?
    /// For a container: the kinds of card it holds. How a card of it opens: a container on its
    /// card map (`card-map`); one of the project's own on its fields and rows (`fields`, when not
    /// said) or on its body as a note's (`markdown`; `opensAsNote`).
    var holds: [String]
    var opens: String?
    var purpose: String
    var useWhen: String
    var notFor: String
    var fields: [KindField]
    var rows: [KindRow]
    /// How a card of it is drawn on a diagram, when it says (`face`).
    var face: CardFace?
    var builtin: Bool

    /// The fields every kind takes from `base`: drawn and written in their own places.
    static let baseFields: Set<String> = ["label", "description", "tags", "metadata"]

    var isContainer: Bool { !holds.isEmpty }

    /// Opened on its field `body` in the note's editor, its other fields in the property panel, as
    /// a task's are (`opens: markdown`).
    var opensAsNote: Bool { opens == "markdown" && !isContainer }

    /// Its fields but those every kind has.
    var ownFields: [KindField] { fields.filter { !Self.baseFields.contains($0.name) } }

    /// The field of `as_row` that says a card of it is done, drawn as its row's check: a check or a
    /// choice with a value meaning done (a task's 済み, a goal's 状態).
    var rowCheck: KindField? {
        asRow.compactMap(field).first { !$0.done.isEmpty && ($0.fieldtype == "checkbox" || $0.fieldtype == "radio") }
    }

    /// The value its row's check writes, on or off: a check turned over; a choice set to the first value
    /// meaning done, or, let go, to the option just before it (a goal's 完了 and 進行中).
    func rowCheckValue(_ on: Bool) -> JSONValue? {
        guard let check = rowCheck else { return nil }
        if check.fieldtype == "checkbox" { return .bool(on) }
        guard let done = check.done.first?.asText, let at = check.options.firstIndex(of: done) else { return nil }
        if on { return .string(done) }
        let before = check.options.index(before: at)
        return check.options.indices.contains(before) ? .string(check.options[before]) : check.defaultValue
    }

    func field(_ name: String) -> KindField? { fields.first { $0.name == name } }
    func row(_ name: String) -> KindRow? { rows.first { $0.name == name } }

    /// As the badge says it: the kind, with its label when that says more.
    var badge: String {
        label.isEmpty || label == kind ? kind : L10n.text(.Kinds.kindDefBadge(kind, label), in: Language.current)
    }

    /// What it is, for a person: its purpose, when to use it and when not.
    var meaning: String {
        let language = Language.current
        return [purpose,
                useWhen.isEmpty ? "" : L10n.text(.Kinds.kindDefUseWhen(useWhen), in: language),
                notFor.isEmpty ? "" : L10n.text(.Kinds.kindDefNotFor(notFor), in: language)]
            .filter { !$0.isEmpty }.joined(separator: L10n.text(.Kinds.kindDefMeaningSeparator, in: language))
    }

    private enum Keys: String, CodingKey {
        case kind, label, color, header, connectable, holds, opens, context, fields, rows, face, builtin
        case asRow = "as_row", onDoubleClick = "on_double_click"
    }

    private struct Context: Decodable {
        var purpose: String?
        var use_when: String?
        var not_for: String?
    }

    init(kind: String, label: String = "", color: String? = nil, header: [String] = [], asRow: [String] = [],
         unfolds: Bool = false, connectable: Bool = true, onDoubleClick: String? = nil, holds: [String] = [],
         opens: String? = nil, purpose: String = "", useWhen: String = "", notFor: String = "",
         fields: [KindField] = [], rows: [KindRow] = [], face: CardFace? = nil, builtin: Bool = false) {
        self.kind = kind
        self.label = label.isEmpty ? kind : label
        self.color = color
        self.header = header
        self.asRow = asRow
        self.unfolds = unfolds
        self.connectable = connectable
        self.onDoubleClick = onDoubleClick
        self.holds = holds
        self.opens = opens
        self.purpose = purpose
        self.useWhen = useWhen
        self.notFor = notFor
        self.fields = fields
        self.rows = rows
        self.face = face
        self.builtin = builtin
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        kind = try c.decode(String.self, forKey: .kind)
        let said = try c.decodeIfPresent(String.self, forKey: .label) ?? ""
        label = said.isEmpty ? kind : said
        color = try? c.decodeIfPresent(String.self, forKey: .color)
        header = (try? c.decodeIfPresent([String].self, forKey: .header)) ?? []
        let fieldsInRow = try? c.decodeIfPresent([String].self, forKey: .asRow)
        asRow = fieldsInRow ?? []
        unfolds = fieldsInRow == nil && (try? c.decodeIfPresent(String.self, forKey: .asRow)) == "unfold"
        connectable = (try? c.decodeIfPresent(Bool.self, forKey: .connectable)) ?? false
        onDoubleClick = try? c.decodeIfPresent(String.self, forKey: .onDoubleClick)
        holds = (try? c.decodeIfPresent([String].self, forKey: .holds)) ?? []
        opens = try? c.decodeIfPresent(String.self, forKey: .opens)
        let context = (try? c.decodeIfPresent(Context.self, forKey: .context)) ?? nil
        purpose = context?.purpose ?? ""
        useWhen = context?.use_when ?? ""
        notFor = context?.not_for ?? ""
        fields = (try? c.decodeIfPresent([KindField].self, forKey: .fields)) ?? []
        rows = (try? c.decodeIfPresent([KindRow].self, forKey: .rows)) ?? []
        face = (try? c.decodeIfPresent(CardFace.self, forKey: .face)) ?? nil
        builtin = (try? c.decodeIfPresent(Bool.self, forKey: .builtin)) ?? false
    }
}

/// How a card of a kind is drawn on a diagram (`face`; `specboard://SpecBoard/Docs/Design:CardKinds`
/// 「図の上での顔」): under its header its fields `top`, each a control pressed there — a check a
/// switch, a choice its options side by side (or a menu, when they do not fit: as `FittingChoice`
/// does), a dropdown list a menu, a date or a text as written — and below them the field
/// `markdown`, drawn as a writing card's body is (`DocSnapshots`) — or `svg` or `html`, a page of
/// its own drawn the same way, not read as Markdown. The relation panel and a container's card map draw it so; the property
/// panel and the card editor as before. How big it is drawn is the card's own (`SchemaNode.sizable`), so an
/// earlier definition's `resizable` is let go.
nonisolated struct CardFace: Decodable, Hashable, Sendable {
    var top: [String]
    var markdown: String?
    var svg: String?
    var html: String?

    init(top: [String] = [], markdown: String? = nil, svg: String? = nil, html: String? = nil) {
        self.top = top
        self.markdown = markdown
        self.svg = svg
        self.html = html
    }

    private enum Keys: String, CodingKey { case top, markdown, svg, html }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        top = (try? c.decodeIfPresent([String].self, forKey: .top)) ?? []
        markdown = (try? c.decodeIfPresent(String.self, forKey: .markdown)) ?? nil
        svg = (try? c.decodeIfPresent(String.self, forKey: .svg)) ?? nil
        html = (try? c.decodeIfPresent(String.self, forKey: .html)) ?? nil
    }
}

/// One field of a kind of card (`specboard://SpecBoard/Docs/Design:CardKinds`「欄の定義」): what is written in it and how.
nonisolated struct KindField: Decodable, Hashable, Sendable {
    var name: String
    var label: String
    var description: String
    /// `text`・`longtext`・`markdown`・`number`・`checkbox`・`date`・`url`・`radio`・`dropdownlist`・
    /// `person`・`branch`・`pr`・`card`・`path`・`group`・`json`・`map`.
    var fieldtype: String
    /// A list of values (`many`), written as one.
    var many: Bool
    var options: [String]
    /// How a value of `options` is shown when not as it is (`optionLabels`: a built-in kind's in the
    /// screen's language, the values being what cards keep).
    var optionLabels: [String: String]
    var defaultValue: JSONValue?
    /// Required always, or when its condition holds (`required` true or a condition).
    var required: Bool
    /// Where it is shown: `editor` for the card editor alone.
    var show: String?
    /// The values that count as done.
    var done: [JSONValue]
    /// A group's own fields.
    var fields: [KindField]

    init(name: String, label: String = "", description: String = "", fieldtype: String = "text", many: Bool = false,
         options: [String] = [], optionLabels: [String: String] = [:], defaultValue: JSONValue? = nil,
         required: Bool = false, show: String? = nil, done: [JSONValue] = [], fields: [KindField] = []) {
        self.name = name
        self.label = label.isEmpty ? name : label
        self.description = description
        self.fieldtype = fieldtype
        self.many = many
        self.options = options
        self.optionLabels = optionLabels
        self.defaultValue = defaultValue
        self.required = required
        self.show = show
        self.done = done
        self.fields = fields
    }

    private enum Keys: String, CodingKey {
        case name, label, description, fieldtype, many, options, optionLabels, required, show, done, fields
        case defaultValue = "default"
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        name = try c.decode(String.self, forKey: .name)
        let said = try c.decodeIfPresent(String.self, forKey: .label) ?? ""
        label = said.isEmpty ? name : said
        description = (try? c.decodeIfPresent(String.self, forKey: .description)) ?? ""
        fieldtype = (try? c.decodeIfPresent(String.self, forKey: .fieldtype)) ?? "text"
        switch try? c.decodeIfPresent(JSONValue.self, forKey: .many) {
        case .bool(let on)?: many = on
        case .number(let n)?: many = n >= 1
        default: many = false
        }
        options = (try? c.decodeIfPresent([String].self, forKey: .options)) ?? []
        optionLabels = (try? c.decodeIfPresent([String: String].self, forKey: .optionLabels)) ?? [:]
        defaultValue = try? c.decodeIfPresent(JSONValue.self, forKey: .defaultValue)
        switch try? c.decodeIfPresent(JSONValue.self, forKey: .required) {
        case .bool(let on)?: required = on
        case .object?, .array?: required = true
        default: required = false
        }
        show = try? c.decodeIfPresent(String.self, forKey: .show)
        done = (try? c.decodeIfPresent([JSONValue].self, forKey: .done)) ?? []
        fields = (try? c.decodeIfPresent([KindField].self, forKey: .fields)) ?? []
    }

    /// How `option`, one of its values, is shown.
    func shown(_ option: String) -> String { optionLabels[option] ?? option }

    /// Whether its value is chosen from `options`.
    var chosen: Bool { fieldtype == "radio" || fieldtype == "dropdownlist" }

    /// The type of a field that holds a section (a frame's `target`): its id kept, its path read and written.
    static let sectionType = "section"

    /// Whether its value is written as JSON by hand (a list, a group, any JSON, a map).
    var writtenAsJSON: Bool { many || ["group", "json", "map"].contains(fieldtype) }

    /// Whether it is written in a box of several lines.
    var long: Bool { fieldtype == "longtext" || fieldtype == "markdown" }
}

/// One kind of row of a kind of card (`specboard://SpecBoard/Docs/Design:CardKinds`「行の種類の定義」): written as many
/// times as wanted, each with its fields and the rows under it.
nonisolated struct KindRow: Decodable, Hashable, Sendable {
    var name: String
    var label: String
    /// How a row of it is named in a path: by its field `name`, its field `value`, or its place
    /// (`index`).
    var address: String
    var fields: [KindField]
    /// The kinds of row that may go under one of it (one of an ancestor's again is left out).
    var rows: [KindRow]

    init(name: String, label: String = "", address: String = "name", fields: [KindField] = [], rows: [KindRow] = []) {
        self.name = name
        self.label = label.isEmpty ? name : label
        self.address = address
        self.fields = fields
        self.rows = rows
    }

    private enum Keys: String, CodingKey { case name, label, address, fields, rows }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        name = try c.decode(String.self, forKey: .name)
        let said = try c.decodeIfPresent(String.self, forKey: .label) ?? ""
        label = said.isEmpty ? name : said
        address = (try? c.decodeIfPresent(String.self, forKey: .address)) ?? "name"
        fields = (try? c.decodeIfPresent([KindField].self, forKey: .fields)) ?? []
        // A kind of row again (`ref`) has no name of its own: left out.
        rows = ((try? c.decodeIfPresent([Lenient].self, forKey: .rows)) ?? []).compactMap(\.row)
    }

    /// A row's kind under another, or one of an ancestor's again (`{"ref": …}`), which is not read.
    private struct Lenient: Decodable {
        var row: KindRow?
        init(from decoder: Decoder) throws { row = try? KindRow(from: decoder) }
    }

    func field(_ name: String) -> KindField? { fields.first { $0.name == name } }

    /// The field a row of it is told by on screen: its address's, else its first text.
    var titleField: String? {
        if address == "value", field("value") != nil { return "value" }
        if field("name") != nil { return "name" }
        return fields.first { ["text", "longtext", "markdown"].contains($0.fieldtype) }?.name
    }

    /// The check a row of it is done by, drawn as its switch: its first checkbox.
    var check: String? { fields.first { $0.fieldtype == "checkbox" }?.name }
}

/// A card's rows of a kind the project defines, as its document keeps them (`{id, fields, rows}`
/// each), read for the screen: walked with their paths as the core names them (`steps.a`,
/// `steps[0]`), and written back in MCP's form (`card_update`'s `<kind of row>: […]`).
nonisolated enum OwnRows {
    /// The start of a card's row that is one of its fields (`#f.severity`), not a row.
    static let fieldRow = "#f."

    /// One row, where it is.
    struct Placed: Hashable {
        /// Its path in the card: its kind of row and its address, then the same for the rows
        /// above it (`steps.a.checks.b`).
        var path: String
        var kind: KindRow
        var depth: Int
        var fields: [String: JSONValue]
        /// Its kind of row at the top, and its place among the rows beside it, from the top.
        var places: [(kind: String, index: Int)]

        static func == (a: Placed, b: Placed) -> Bool { a.path == b.path && a.fields == b.fields }
        func hash(into hasher: inout Hasher) { hasher.combine(path) }

        /// What it says on screen: its name or value, else its first text, else its place.
        var title: String {
            if let key = kind.titleField, case .string(let text)? = fields[key], !text.isEmpty { return text }
            return path
        }

        /// Whether its check is on (none when its kind has none).
        var checked: Bool? {
            guard let check = kind.check else { return nil }
            return fields[check] == .bool(true)
        }
    }

    /// Every row of the card, depth first, in the order of the kinds of row and of the rows.
    static func walk(_ def: CardKindDef, _ rows: [String: JSONValue]) -> [Placed] {
        var out: [Placed] = []
        func go(_ kind: KindRow, _ items: [JSONValue], _ prefix: String, _ depth: Int, _ above: [(kind: String, index: Int)]) {
            for (index, item) in items.enumerated() {
                let fields = item.asObject?["fields"]?.asObject ?? [:]
                let here = prefix.isEmpty ? PathText.quote(kind.name) : PathText.member(prefix, kind.name)
                let path: String
                switch kind.address {
                case "index": path = "\(here)[\(index)]"
                case "value": path = PathText.member(here, fields["value"]?.asText ?? "\(index)")
                default: path = PathText.member(here, fields["name"]?.asText ?? "\(index)")
                }
                let places = above + [(kind.name, index)]
                out.append(Placed(path: path, kind: kind, depth: depth, fields: fields, places: places))
                let under = item.asObject?["rows"]?.asObject ?? [:]
                for child in kind.rows {
                    go(child, under[child.name]?.arrayValue ?? [], path, depth + 1, places)
                }
            }
        }
        for kind in def.rows { go(kind, rows[kind.name]?.arrayValue ?? [], "", 0, []) }
        return out
    }

    /// One kind of row's rows in MCP's form: each row's fields and the rows under it by name,
    /// without ids (the core keeps a row's id by its address).
    static func wire(_ items: [JSONValue]) -> [Any] {
        items.map { item -> Any in
            var out: [String: Any] = [:]
            for (key, value) in item.asObject?["fields"]?.asObject ?? [:] { out[key] = value.any }
            for (key, list) in item.asObject?["rows"]?.asObject ?? [:] { out[key] = wire(list.arrayValue ?? []) }
            return out
        }
    }

    /// The rows of the kind at the top of `places`, in MCP's form, with the row at `places`
    /// changed by `change` (its fields), or taken out (`change` gives nil).
    static func changing(_ rows: [String: JSONValue], at places: [(kind: String, index: Int)],
                         _ change: ([String: JSONValue]) -> [String: JSONValue]?) -> (kind: String, rows: [Any])? {
        guard let top = places.first else { return nil }
        func go(_ items: [JSONValue], _ rest: ArraySlice<(kind: String, index: Int)>) -> [JSONValue] {
            guard let here = rest.first, items.indices.contains(here.index) else { return items }
            var items = items
            var item = items[here.index].asObject ?? [:]
            if rest.count == 1 {
                guard let fields = change(item["fields"]?.asObject ?? [:]) else {
                    items.remove(at: here.index)
                    return items
                }
                item["fields"] = .object(fields)
            } else {
                let next = rest.dropFirst()
                var under = item["rows"]?.asObject ?? [:]
                if let child = next.first {
                    under[child.kind] = .array(go(under[child.kind]?.arrayValue ?? [], next))
                }
                item["rows"] = .object(under)
            }
            items[here.index] = .object(item)
            return items
        }
        let changed = go(rows[top.kind]?.arrayValue ?? [], places[...])
        return (top.kind, wire(changed))
    }

    /// The rows of `kind` in MCP's form with a new one at the end, its address given.
    static func adding(_ rows: [String: JSONValue], kind: KindRow, title: String) -> [Any] {
        var fields: [String: JSONValue] = [:]
        if let key = kind.titleField, !title.isEmpty { fields[key] = .string(title) }
        let items = (rows[kind.name]?.arrayValue ?? []) + [.object(["fields": .object(fields)])]
        return wire(items)
    }
}

extension JSONValue {
    nonisolated var arrayValue: [JSONValue]? {
        if case .array(let v) = self { return v }
        return nil
    }

    /// The object it is, if it is one (as `objectValue`, from anywhere).
    nonisolated var asObject: [String: JSONValue]? {
        if case .object(let v) = self { return v }
        return nil
    }

    /// The text it is, if it is a string.
    nonisolated var asText: String? {
        if case .string(let v) = self { return v }
        return nil
    }

    /// As `JSONSerialization` takes it.
    nonisolated var any: Any {
        switch self {
        case .null: return NSNull()
        case .bool(let v): return v
        case .number(let v): return v == v.rounded() && abs(v) < 1e15 ? Int(v) as Any : v as Any
        case .string(let v): return v
        case .array(let v): return v.map(\.any)
        case .object(let v): return v.mapValues(\.any)
        }
    }

    /// From what `JSONSerialization` reads.
    nonisolated static func of(_ any: Any) -> JSONValue {
        switch any {
        case is NSNull: return .null
        case let n as NSNumber where CFGetTypeID(n) == CFBooleanGetTypeID(): return .bool(n.boolValue)
        case let n as NSNumber: return .number(n.doubleValue)
        case let s as String: return .string(s)
        case let a as [Any]: return .array(a.map(of))
        case let o as [String: Any]: return .object(o.mapValues(of))
        default: return .null
        }
    }

    /// Written out as JSON text, keys sorted, for a field written by hand.
    nonisolated var jsonText: String {
        guard let data = try? JSONSerialization.data(withJSONObject: any, options: [.sortedKeys, .fragmentsAllowed, .withoutEscapingSlashes]),
              let written = String(data: data, encoding: .utf8) else { return "" }
        return written
    }
}

/// A field's value of a kind the project defines, as a person reads it in a line.
nonisolated enum OwnFieldText {
    static func short(_ value: JSONValue) -> String {
        switch value {
        case .null: return ""
        case .bool(let on): return on ? "✓" : ""
        case .number(let n): return n == n.rounded() && abs(n) < 1e15 ? String(Int64(n)) : String(n)
        case .string(let text): return text.split(separator: "\n", omittingEmptySubsequences: false).first.map(String.init) ?? ""
        case .array(let items): return items.map(short).filter { !$0.isEmpty }.joined(separator: ", ")
        case .object: return value.jsonText
        }
    }
}
