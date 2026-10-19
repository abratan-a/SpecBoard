import AppKit

/// The kinds of line a project has, as their definitions say them (`project_detail`'s
/// `kinds.lines`, `specboard://SpecBoard/Docs/Design:CardKinds`「線の種類の定義」): the built-in ones, then the project's
/// own. What a line is called (`label`, its name for the key), what it means, and how it is
/// drawn (`look`) are its kind's — the screen holds no table of its own and no person's
/// setting (線の見た目は定義が持つ).
nonisolated struct LineKinds: Hashable, Sendable {
    var list: [LineKind] = []

    init(_ list: [LineKind] = []) { self.list = list }

    /// The kind of line of that name; none for one the project does not define.
    subscript(name: String) -> LineKind? { list.first { $0.line == name } }

    /// How a line of that kind is drawn: its definition's look, or the plain one.
    func look(_ name: String) -> LineLook { self[name]?.look ?? LineLook() }

    /// What the legend says of it: its label and what it means — or, for a kind the project
    /// does not define, its name alone.
    func label(_ name: String) -> String { self[name]?.label ?? name }
    func meaning(_ name: String) -> String { self[name]?.description ?? "" }

    /// The names in the order the definitions come: the built-in ones, then the project's.
    var names: [String] { list.map(\.line) }
}

/// One kind of line, as its definition says it.
nonisolated struct LineKind: Codable, Hashable, Sendable {
    var line: String
    var label: String
    var description: String
    /// What it means: `embed` (the field's type is what it reaches), `designate` (the
    /// value names it), `link` (no rule).
    var base: String
    var look: LineLook
    /// Each line's own fields (`fields`): what the property panel offers on a line picked.
    var fields: [LineField]
    /// Built in (`builtin.json`), not the project's own.
    var builtin: Bool
    /// Drawn from the start (`visible`): false hides it on every diagram until
    /// it is switched on (`GraphFilters.shownLines`).
    var visible: Bool

    private enum CodingKeys: String, CodingKey { case line, label, description, base, look, fields, builtin, visible }

    init(line: String, label: String = "", description: String = "", base: String = "link",
         look: LineLook = LineLook(), fields: [LineField] = [], builtin: Bool = false, visible: Bool = true) {
        self.line = line
        self.label = label.isEmpty ? line : label
        self.description = description
        self.base = base
        self.look = look
        self.fields = fields
        self.builtin = builtin
        self.visible = visible
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        line = try c.decode(String.self, forKey: .line)
        let said = try c.decodeIfPresent(String.self, forKey: .label) ?? ""
        label = said.isEmpty ? line : said
        description = try c.decodeIfPresent(String.self, forKey: .description) ?? ""
        base = try c.decodeIfPresent(String.self, forKey: .base) ?? "link"
        look = try c.decodeIfPresent(LineLook.self, forKey: .look) ?? LineLook()
        fields = (try? c.decodeIfPresent([LineField].self, forKey: .fields)) ?? []
        builtin = try c.decodeIfPresent(Bool.self, forKey: .builtin) ?? false
        visible = (try? c.decodeIfPresent(Bool.self, forKey: .visible)) ?? true
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(line, forKey: .line)
        try c.encode(label, forKey: .label)
        try c.encode(description, forKey: .description)
        try c.encode(base, forKey: .base)
        try c.encode(look, forKey: .look)
        try c.encode(fields, forKey: .fields)
        try c.encode(builtin, forKey: .builtin)
        if !visible { try c.encode(visible, forKey: .visible) }
    }
}

/// One field of a kind of line (`fields`, written as a card's field is, `specboard://SpecBoard/Docs/Design:CardKinds`
/// 「欄の定義」): what the property panel shows of it — its name, its words, the kind of value
/// and the options, whether it is needed, and a group's own fields.
nonisolated struct LineField: Codable, Hashable, Sendable, Identifiable {
    var name: String
    var label: String
    var description: String
    var fieldtype: String
    var options: [String]
    /// Needed always (`required: true`); a condition reads as not.
    var required: Bool
    /// 0 one value, 1 a list of them.
    var many: Int
    /// A group's own fields.
    var fields: [LineField]

    var id: String { name }

    private enum CodingKeys: String, CodingKey { case name, label, description, fieldtype, options, required, many, fields }

    init(name: String, label: String = "", description: String = "", fieldtype: String = "text",
         options: [String] = [], required: Bool = false, many: Int = 0, fields: [LineField] = []) {
        self.name = name
        self.label = label
        self.description = description
        self.fieldtype = fieldtype
        self.options = options
        self.required = required
        self.many = many
        self.fields = fields
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        name = try c.decode(String.self, forKey: .name)
        label = (try? c.decodeIfPresent(String.self, forKey: .label)) ?? ""
        description = (try? c.decodeIfPresent(String.self, forKey: .description)) ?? ""
        fieldtype = (try? c.decodeIfPresent(String.self, forKey: .fieldtype)) ?? "text"
        options = (try? c.decodeIfPresent([String].self, forKey: .options)) ?? []
        required = (try? c.decodeIfPresent(Bool.self, forKey: .required)) ?? false
        if let flag = try? c.decodeIfPresent(Bool.self, forKey: .many) {
            many = flag == true ? 1 : 0
        } else {
            many = (try? c.decodeIfPresent(Int.self, forKey: .many)) ?? 0
        }
        fields = (try? c.decodeIfPresent([LineField].self, forKey: .fields)) ?? []
    }

    /// As the panel names it: its label, or its name.
    var said: String { label.isEmpty ? name : label }
}

/// How a kind of line is drawn (`look`): its colour, its dash, and the marks at its two
/// ends. A word this build does not know reads as the plain one.
nonisolated struct LineLook: Codable, Hashable, Sendable {
    var ink: EdgeInk = .grey
    var dash: EdgeDash = .solid
    /// The mark where it leaves, and where it arrives.
    var begin: EdgeCap = .none
    var end: EdgeCap = .arrow

    init(ink: EdgeInk = .grey, dash: EdgeDash = .solid, begin: EdgeCap = .none, end: EdgeCap = .arrow) {
        self.ink = ink
        self.dash = dash
        self.begin = begin
        self.end = end
    }

    private enum CodingKeys: String, CodingKey { case color, dash, begin, end }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        func word(_ key: CodingKeys) -> String? { (try? c.decodeIfPresent(String.self, forKey: key)) ?? nil }
        ink = word(.color).flatMap(EdgeInk.init(rawValue:)) ?? .grey
        dash = word(.dash).flatMap(EdgeDash.init(rawValue:)) ?? .solid
        begin = word(.begin).flatMap(EdgeCap.init(rawValue:)) ?? .none
        end = word(.end).flatMap(EdgeCap.init(rawValue:)) ?? .arrow
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(ink.rawValue, forKey: .color)
        try c.encode(dash.rawValue, forKey: .dash)
        try c.encode(begin.rawValue, forKey: .begin)
        try c.encode(end.rawValue, forKey: .end)
    }
}

/// Named rather than stored as a colour, because the same name has to resolve
/// differently on a light and a dark background — a yellow that reads on charcoal
/// disappears on paper. The words are the definitions' (`look.color`).
nonisolated enum EdgeInk: String, Codable, CaseIterable, Sendable {
    case amber, blue, green, teal, violet, red, grey

    func color(dark: Bool) -> NSColor {
        switch self {
        case .amber:  return dark ? .hex(0xE0A951) : .hex(0xA9781F)
        case .blue:   return dark ? .hex(0x6FA8DC) : .hex(0x1F5FA9)
        case .green:  return dark ? .hex(0x6FC08A) : .hex(0x1E7A44)
        case .teal:   return dark ? .hex(0x57BEBE) : .hex(0x0E6469)
        case .violet: return dark ? .hex(0xB08CE0) : .hex(0x6A3FA0)
        case .red:    return dark ? .hex(0xE08A8A) : .hex(0xA83232)
        case .grey:   return dark ? .hex(0x8A98A8) : .hex(0x6B7787)
        }
    }
}

/// A line's dash (`look.dash`).
nonisolated enum EdgeDash: String, Codable, CaseIterable, Sendable {
    case solid, dashed, dotted

    var pattern: [CGFloat]? {
        switch self {
        case .solid: return nil
        case .dashed: return [7, 5]
        case .dotted: return [2, 4]
        }
    }
}

/// The marks at the ends of a line (`look.begin`, `look.end`).
///
/// The **owner's end** says what kind of relationship this is: a filled diamond for
/// what holds the other's shape (embed), an open one for what names it by its id, nothing
/// for a plain line — the split UML makes and mermaid draws (`*--`, `o--`, `..>`). The
/// **target's end** usually gets an arrowhead: lines are drawn beneath the cards, so one
/// passing behind a card disappears and comes out the other side, and an end with no mark
/// on it is indistinguishable from a line that carried on.
nonisolated enum EdgeCap: String, Codable, CaseIterable, Sendable {
    case filledDiamond = "filled-diamond"
    case openDiamond = "open-diamond"
    case arrow
    case triangle
    case none

    /// Drawn at the origin, pointing along +x; the caller rotates and translates it.
    func path(size: CGFloat) -> CGPath? {
        let path = CGMutablePath()
        switch self {
        case .none:
            return nil
        case .filledDiamond, .openDiamond:
            path.move(to: CGPoint(x: 0, y: 0))
            path.addLine(to: CGPoint(x: size * 0.55, y: size * 0.4))
            path.addLine(to: CGPoint(x: size * 1.1, y: 0))
            path.addLine(to: CGPoint(x: size * 0.55, y: -size * 0.4))
            path.closeSubpath()
        case .arrow:
            path.move(to: CGPoint(x: size, y: size * 0.45))
            path.addLine(to: CGPoint(x: 0, y: 0))
            path.addLine(to: CGPoint(x: size, y: -size * 0.45))
        case .triangle:
            path.move(to: CGPoint(x: 0, y: 0))
            path.addLine(to: CGPoint(x: size, y: size * 0.45))
            path.addLine(to: CGPoint(x: size, y: -size * 0.45))
            path.closeSubpath()
        }
        return path
    }

    /// Filled with the line's colour; the others are open, the background showing through.
    var isFilled: Bool { self == .filledDiamond || self == .triangle }
    /// A closed shape (filled or open), rather than strokes.
    var isClosed: Bool { self == .filledDiamond || self == .openDiamond || self == .triangle }
}
