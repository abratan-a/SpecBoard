import AppKit

/// The palette, taken from the web build's `styles.css` custom properties so an
/// imported diagram looks like the one people already read.
///
/// Nesting is teal and references are brass, and that is not decoration: choosing
/// between the two *is* the schema design decision, so the two must never be
/// mistaken for each other at a glance.
struct GraphTheme {
    var background: NSColor
    var panel: NSColor
    var panel2: NSColor
    var panel3: NSColor
    var ink: NSColor
    var ink2: NSColor
    var ink3: NSColor
    var line: NSColor
    var line2: NSColor
    var brass: NSColor
    var brassInk: NSColor
    var brassSoft: NSColor
    var brassLine: NSColor
    var teal: NSColor
    var tealSoft: NSColor
    /// The default line colour, still used for anything not drawn as a reference
    /// — the leader lines and the arrow heads.
    var edge: NSColor
    var selection: NSColor

    static let light = GraphTheme(
        background: .hex(0xEAEDF1), panel: .hex(0xFFFFFF),
        panel2: .hex(0xF5F7FA), panel3: .hex(0xEDF1F5),
        ink: .hex(0x121A24), ink2: .hex(0x48586C), ink3: .hex(0x7B8B9E),
        line: .hex(0xD9E0E8), line2: .hex(0xC2CCD8),
        brass: .hex(0x8F6410), brassInk: .hex(0x6E4C08),
        brassSoft: .hex(0xF4E9D2), brassLine: .hex(0xDCC28A),
        teal: .hex(0x0E6469), tealSoft: .hex(0xDFEEEE),
        edge: .hex(0xA9781F), selection: NSColor(srgbRed: 143/255, green: 100/255, blue: 16/255, alpha: 0.10))

    static let dark = GraphTheme(
        background: .hex(0x0A0F16), panel: .hex(0x121A24),
        panel2: .hex(0x18222E), panel3: .hex(0x1E2937),
        ink: .hex(0xE7EDF4), ink2: .hex(0xA3B2C3), ink3: .hex(0x6C7D91),
        line: .hex(0x232F3D), line2: .hex(0x33414F),
        brass: .hex(0xE0A951), brassInk: .hex(0xF0C57F),
        brassSoft: .hex(0x33290F), brassLine: .hex(0x5A4718),
        teal: .hex(0x57BEBE), tealSoft: .hex(0x13292B),
        edge: .hex(0xC08B33), selection: NSColor(srgbRed: 224/255, green: 169/255, blue: 81/255, alpha: 0.12))

    static func current(for appearance: NSAppearance) -> GraphTheme {
        let dark = appearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
        return dark ? .dark : .light
    }

    /// Header tint by schema kind — entity, value object, enum read differently.
    func headerFill(for kind: SchemaKind) -> NSColor {
        switch kind {
        case .entity: return panel2
        case .value: return tealSoft
        case .enum: return brassSoft
        // Not a shape at all, so not a colour the three shapes use.
        case .action: return EdgeInk.violet.color(dark: isDark).withAlphaComponent(isDark ? 0.22 : 0.12)
        case .note: return EdgeInk.blue.color(dark: isDark).withAlphaComponent(isDark ? 0.18 : 0.09)
        // The documents: the note's blue, a shade deeper — writing all three — and
        // the one written in rows deeper again, so the two read apart.
        // A picture or a page: drawn as a document is, in a freedoc's colour. A frame, the box such pages
        // are put into, the same.
        case .freedoc, .svg, .html, .frame: return EdgeInk.blue.color(dark: isDark).withAlphaComponent(isDark ? 0.26 : 0.14)
        case .formatdoc: return EdgeInk.blue.color(dark: isDark).withAlphaComponent(isDark ? 0.40 : 0.24)
        // The versions stacked: between the two.
        case .stackdoc: return EdgeInk.blue.color(dark: isDark).withAlphaComponent(isDark ? 0.33 : 0.19)
        // A flow is the way through a chart: the chart section's red, so it is not
        // taken for an action. A store is where records rest: green, apart from
        // the shapes it keeps.
        case .flow: return EdgeInk.red.color(dark: isDark).withAlphaComponent(isDark ? 0.20 : 0.10)
        case .store: return EdgeInk.green.color(dark: isDark).withAlphaComponent(isDark ? 0.18 : 0.10)
        // A data card: its kind's colour, apart from the shape it holds the values of.
        case .instance, .table: return Self.instanceInk(dark: isDark).withAlphaComponent(isDark ? 0.24 : 0.12)
        // A kind a person defines: its own colour (`specboard://SpecBoard/Docs/Design:DesignBoard` §3).
        // A task is its goal's: the goal's colour.
        case .goal, .task: return Self.custom(.goal).withAlphaComponent(isDark ? 0.30 : 0.16)
        // A kind the project defines: its definition's colour (`headerFill(for card:)`), else plain.
        case .custom: return panel2
        }
    }

    /// A card's header tint: a kind the project defines by its definition's colour, any other
    /// by its kind.
    func headerFill(for card: SchemaNode) -> NSColor {
        guard card.kind == .custom, let color = Self.hexColor(card.kindDef?.color) else { return headerFill(for: card.kind) }
        return color.withAlphaComponent(isDark ? 0.30 : 0.16)
    }

    /// The kind's word on a card's header, the same way.
    func accent(for card: SchemaNode) -> NSColor {
        guard card.kind == .custom, let color = Self.hexColor(card.kindDef?.color) else { return accent(for: card.kind) }
        return color
    }

    /// `#RRGGBB` as a colour; none for anything else.
    static func hexColor(_ hex: String?) -> NSColor? {
        guard let hex, hex.count == 7, hex.hasPrefix("#"), let value = UInt32(hex.dropFirst(), radix: 16) else { return nil }
        return .hex(value)
    }

    /// A custom kind's colour, as its definition says.
    static func custom(_ kind: SchemaKind) -> NSColor {
        let hex = (CustomKind.of(kind)?.color ?? "#888888").dropFirst()
        return .hex(UInt32(hex, radix: 16) ?? 0x888888)
    }

    func accent(for kind: SchemaKind) -> NSColor {
        switch kind {
        case .entity: return ink2
        case .value: return teal
        case .enum: return brass
        case .action: return EdgeInk.violet.color(dark: isDark)
        case .note, .freedoc, .formatdoc, .stackdoc, .svg, .html, .frame: return EdgeInk.blue.color(dark: isDark)
        case .flow: return EdgeInk.red.color(dark: isDark)
        case .store: return EdgeInk.green.color(dark: isDark)
        case .instance, .table: return Self.instanceInk(dark: isDark)
        case .goal, .task: return Self.custom(.goal)
        case .custom: return ink2
        }
    }

    /// A data card's colour, its definition's (`#0E7C86`), lighter on a dark ground.
    static func instanceInk(dark: Bool) -> NSColor {
        dark ? .hex(0x3FB7C1) : .hex(0x0E7C86)
    }

    /// A flow's node on its card editor, by what it is: where it begins green,
    /// what it runs in the action's violet, a decision amber, a wait teal, a join
    /// grey and an end red — so a way through reads at a glance.
    static func nodeInk(_ kind: String) -> EdgeInk {
        switch kind {
        case "begin": .green
        case "run": .violet
        case "eval": .amber
        case "wait": .teal
        case "join": .grey
        case "end": .red
        default: .grey
        }
    }

    func headerFill(forNode kind: String) -> NSColor {
        Self.nodeInk(kind).color(dark: isDark).withAlphaComponent(isDark ? 0.24 : 0.13)
    }

    func accent(forNode kind: String) -> NSColor { Self.nodeInk(kind).color(dark: isDark) }

    /// Whether this palette is the dark one, so an `EdgeInk` can resolve against it.
    var isDark: Bool { background.brightnessComponentApproximation < 0.5 }

    /// A function's or type's mark, by what it is (`CodeSymbol.of`): functions in the action's violet,
    /// types in the value's teal, constants brass, macros amber, modules grey — a card's rows read by
    /// kind at a glance, as the code colours them.
    static func symbolInk(_ of: String, dark: Bool) -> NSColor {
        switch of {
        case "function", "method": EdgeInk.violet.color(dark: dark)
        case "type": EdgeInk.teal.color(dark: dark)
        case "constant": EdgeInk.blue.color(dark: dark)
        case "macro": EdgeInk.amber.color(dark: dark)
        default: EdgeInk.grey.color(dark: dark)
        }
    }

    /// The colours a file's text is drawn in, by the class the core names each run with (`file_text`'s
    /// `classes`): one palette for every language, close to Xcode's own default so that code reads as it
    /// does there, light and dark. A class with none — punctuation, operators — is drawn as the text is.
    static let codePalette: [String: (light: UInt32, dark: UInt32)] = [
        "keyword": (0x9B2393, 0xFF7AB2),
        "string": (0xC41A16, 0xFF8170),
        "comment": (0x5D6C79, 0x7F8C98),
        "number": (0x1C00CF, 0xD9C97C),
        "type": (0x0B4F79, 0x5DD8FF),
        "function": (0x326D74, 0x67B7A4),
        "constant": (0x6C36A9, 0xB281EB),
        "property": (0x326D74, 0x67B7A4),
        "variable.builtin": (0x9B2393, 0xFF7AB2),
        "attribute": (0x643820, 0xFD8F3F),
        "tag": (0x9B2393, 0xFF7AB2),
        "escape": (0xC41A16, 0xFF8170),
        "label": (0x643820, 0xFD8F3F),
        "module": (0x3900A0, 0xDABAFF),
    ]

    /// A class's colour, following the window between light and dark; none for one drawn as the text is.
    static func codeInk(_ name: String) -> NSColor? {
        guard let pair = codePalette[name] else { return nil }
        return NSColor(name: nil) { appearance in
            appearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua ? .hex(pair.dark) : .hex(pair.light)
        }
    }
}

extension NSColor {
    /// Enough to tell the two themes apart; `brightnessComponent` is only defined
    /// for a few colour spaces and these are sRGB literals.
    var brightnessComponentApproximation: CGFloat {
        guard let rgb = usingColorSpace(.sRGB) else { return 1 }
        return 0.299 * rgb.redComponent + 0.587 * rgb.greenComponent + 0.114 * rgb.blueComponent
    }

    nonisolated static func hex(_ value: UInt32) -> NSColor {
        NSColor(srgbRed: CGFloat((value >> 16) & 0xFF) / 255,
                green: CGFloat((value >> 8) & 0xFF) / 255,
                blue: CGFloat(value & 0xFF) / 255,
                alpha: 1)
    }
}
