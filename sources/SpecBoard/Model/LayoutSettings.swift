import Foundation

/// How big a card is drawn (`specboard://SpecBoard/Docs/Design:Overview` §12): the rows it shows before the
/// rest scrolls inside it, and its width — so many points, or as wide as the widest
/// card of the project, every card the same.
nonisolated struct CardSize: Hashable, Sendable {
    var rows: Int = 15
    /// Points, or nil: `auto`, as wide as the project's widest card.
    var width: Int? = 352

    static let standard = CardSize()
    static let rowsRange = 3...40
    static let widthRange = 200...800

    /// `auto`, or the number — as MCP and the store write it.
    var widthWord: String { width.map(String.init) ?? "auto" }

    /// From `auto` or a number; why not, when neither.
    static func width(_ raw: String) throws -> Int? {
        let trimmed = raw.trimmingCharacters(in: .whitespaces).lowercased()
        if trimmed == "auto" { return nil }
        guard let number = Int(trimmed.replacingOccurrences(of: "px", with: "")), widthRange.contains(number) else {
            throw Invalid(L10n.text(.ProjectSettings.errorWidth(widthRange.lowerBound, widthRange.upperBound, raw), in: Language.current))
        }
        return number
    }

    static func rows(_ value: Int) throws -> Int {
        guard rowsRange.contains(value) else {
            throw Invalid(L10n.text(.ProjectSettings.errorRows(rowsRange.lowerBound, rowsRange.upperBound, value), in: Language.current))
        }
        return value
    }
}

/// How a project's cards are drawn (§12), for the whole project: how big each card is, and
/// how many grid units are kept clear on each of its four sides.
nonisolated struct LayoutSettings: Hashable, Sendable {
    var size: CardSize = .standard
    /// Grid units kept clear around a card, on all four sides.
    var margin: Int = 1
    /// How a formatdoc's card is drawn when the card does not say: `document`
    /// — the whole of it as it reads — or `rows`, its items one a row.
    var docView: String = "document"

    static let marginRange = 0...10

    static func margin(_ value: Int) throws -> Int {
        guard marginRange.contains(value) else {
            throw Invalid(L10n.text(.ProjectSettings.errorMargin(marginRange.lowerBound, marginRange.upperBound, value), in: Language.current))
        }
        return value
    }

    /// The room a card takes with its clear margin.
    func withMargin(_ rect: GridRect) -> GridRect {
        GridRect(x: rect.x - margin, y: rect.y - margin, w: rect.w + 2 * margin, h: rect.h + 2 * margin)
    }

    /// The place nearest `wanted` where a card `height` units tall keeps its margin
    /// clear of every card in `taken` — where a card dropped too close goes. Looked
    /// for out to `reach` units; failing that, `wanted` as it is.
    func clearSpot(near wanted: GridPoint, width: Int = Grid.cardColumns, height: Int,
                   avoiding taken: [GridRect], reach: Int = 40) -> GridPoint {
        func fits(_ p: GridPoint) -> Bool {
            let room = withMargin(GridRect(x: p.x, y: p.y, w: width, h: height))
            return !taken.contains { $0.intersects(room) }
        }
        if fits(wanted) { return wanted }
        for ring in 1...max(1, reach) {
            var best: GridPoint?
            var bestDistance = Int.max
            for dx in -ring...ring {
                for dy in -ring...ring where max(abs(dx), abs(dy)) == ring {
                    let p = GridPoint(x: wanted.x + dx, y: wanted.y + dy)
                    let distance = dx * dx + dy * dy
                    if distance < bestDistance, fits(p) { best = p; bestDistance = distance }
                }
            }
            if let best { return best }
        }
        return wanted
    }

    static let standard = LayoutSettings()

    /// Where a card goes when nobody said (§12): under the shortest of `columns`
    /// columns, each a card wide and its margin apart — nothing is below it there to
    /// run into as it grows.
    func nextSpot(avoiding taken: [GridRect], columns: Int = 4, width: Int = Grid.cardColumns) -> GridPoint {
        let step = Grid.cardColumns + margin
        var best: GridPoint?
        for column in 0..<max(1, columns) {
            let x = column * step
            let bottom = taken.filter { $0.x < x + width && x < $0.x + $0.w }
                .map { $0.y + $0.h + margin }.max() ?? 0
            if best.map({ bottom < $0.y }) ?? true { best = GridPoint(x: x, y: bottom) }
        }
        return best ?? GridPoint(x: 0, y: 0)
    }
}

/// The grid cards are placed on (§12), like a spreadsheet's cells: a unit is a tenth
/// of a card's width across and one of its rows down. A card sits on any cell, ten
/// units wide and as many down as it shows. People and agents read and write the
/// same units, so an agent can place cards that do not overlap.
nonisolated enum Grid {
    static let cardColumns = 10
    /// A card's row, in points (`CardGeometry.rowHeight`).
    static let unitHeight: Double = 27
    /// A card's frame around its rows at its tallest, in points: border, header,
    /// the list's padding, and the strips that say more rows are above and below
    /// (both, while it is scrolled to the middle).
    static let frame: Double = 2 + 40 + 12 + 2 * 27

    /// Units down for a card `points` tall.
    static func rows(forHeight points: Double) -> Int { Int((points / unitHeight).rounded(.up)) }

    /// Units down for a card showing `rows` rows, at the most.
    static func rows(forRows rows: Int) -> Int { Self.rows(forHeight: frame + Double(rows) * unitHeight) }

    /// Units across for a card `width` points wide: a unit's width.
    static func unitWidth(cardWidth: Double) -> Double { cardWidth / Double(cardColumns) }

    /// How far a card may sit from the origin, in units, either way. A place read from
    /// a file or the store is brought back within it (`units`), and MCP refuses one past
    /// it: sums of places and sizes stay far inside `Int`.
    static let reach = 1_000_000

    /// Whole units for a place given in units: the nearest, within `reach`.
    static func units(_ value: Double) -> Int {
        guard !value.isNaN else { return 0 }
        return Int(min(max(value.rounded(), Double(-reach)), Double(reach)))
    }

    /// The cell nearest a place given in units.
    static func nearest(x: Double, y: Double) -> GridPoint {
        GridPoint(x: units(x), y: units(y))
    }

    // Positions given in points — an automatic layout's result, or an old form's —
    // and the cells of the older way of putting cards on cells, read into grid units.

    static func fromPoints(x: Double, y: Double, cardWidth: Double) -> GridPoint {
        nearest(x: x / unitWidth(cardWidth: cardWidth), y: y / unitHeight)
    }

    static func fromCell(row: Int, column: Int, rows: Int, gap: Int) -> GridPoint {
        GridPoint(x: column * (cardColumns + gap), y: row * (self.rows(forRows: rows) + gap))
    }
}

nonisolated struct GridPoint: Codable, Hashable, Sendable {
    var x: Int
    var y: Int
}

/// A card's own size in grid units (§12): so many columns across and, when one is written, rows
/// down, as a person drags it or MCP says. With no height, the card is as tall as its contents.
nonisolated struct GridSize: Codable, Hashable, Sendable {
    var w: Int
    var h: Int? = nil

    /// No smaller than this, no larger than that.
    static let widths = 4...60
    static let heights = 2...120

    /// Within the bounds, or why not.
    static func checked(_ size: GridSize) throws -> GridSize {
        guard widths.contains(size.w), size.h.map(heights.contains) ?? true else {
            throw Invalid(L10n.text(
                .ProjectSettings.errorSize(widths.lowerBound, widths.upperBound, heights.lowerBound, heights.upperBound, size.said),
                in: Language.current))
        }
        return size
    }

    /// How it is written, as MCP takes it: `{"w": 20}`, or `20×8` with a height.
    var said: String { h.map { "\(w)×\($0)" } ?? "{\"w\": \(w)}" }
}

nonisolated extension GridPoint {
    /// Read within `Grid.reach`, as every place read from a file or the store is.
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        func within(_ value: Int) -> Int { min(max(value, -Grid.reach), Grid.reach) }
        self.init(x: within(try c.decode(Int.self, forKey: .x)), y: within(try c.decode(Int.self, forKey: .y)))
    }
}

nonisolated struct GridRect: Hashable, Sendable {
    var x: Int
    var y: Int
    var w: Int
    var h: Int

    /// The last cell it covers, inclusive — the bottom right of a spreadsheet range.
    var bottomRight: GridPoint { GridPoint(x: x + w - 1, y: y + h - 1) }

    func intersects(_ other: GridRect) -> Bool {
        x < other.x + other.w && other.x < x + w && y < other.y + other.h && other.y < y + h
    }
}
