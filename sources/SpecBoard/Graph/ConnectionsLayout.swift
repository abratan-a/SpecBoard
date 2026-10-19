import CoreGraphics
import Foundation

/// Where a connections map's cards go (`ConnectionsMap`), in grid units (§12): the card in the
/// middle, its groups around it like a mind map's branches — each group's cards in a block of about a
/// screen's shape, the groups on a side in columns (rows above and below) no longer than the whole
/// map needs to keep about the shape of a screen — and each step further out beyond the step before.
///
/// The middle — the card, and the groups above and below it — is put first, and the sides outside
/// it, each step outside the one before: what is put never has to be moved for what comes after, so
/// no two frames can meet.
extension ConnectionsMap {
    /// A card to put, its size in grid units.
    struct Piece: Equatable {
        var id: String
        var w: Int
        var h: Int
    }

    /// How much room a group takes, frame and all, in grid units.
    struct Extent: Equatable {
        var w: Int
        var h: Int
    }

    /// A group to put: its cards in the order they go, its step, its side, and the group it hangs from.
    struct Bunch: Equatable {
        var id: String
        var cards: [Piece]
        var step: Int
        var side: Side
        var anchor: String?
    }

    struct Arranged: Equatable {
        /// Each card's top-left.
        var cards: [String: GridPoint]
        /// Each group's frame: its cards and the room its frame and title are drawn in (`Room`).
        var frames: [String: GridRect]
    }

    /// The room a group's frame keeps round its cards, in grid units: a unit at the sides and the
    /// bottom, two at the top for its title — more than the frame is drawn with (`CardCanvasView`),
    /// so the frames of two groups side by side never touch.
    enum Room {
        static let side = 1
        static let top = 2
        static let bottom = 1
        /// Between two groups' frames.
        static let between = 2
        /// Between the card and the groups next to it: room for the lines to spread.
        static let fromCard = (across: 5, down: 4)
        /// The shape the whole map is put in, across to down, in points: about a screen's.
        static let shape = 1.4
    }

    /// Where every card of `bunches` goes round `center`, cards `margin` units apart at the least
    /// inside a group (and never less than two, for the lines between them).
    static func arrange(center: Piece, bunches: [Bunch], margin: Int = 1,
                        cardWidth: CGFloat = CardGeometry.cardWidth) -> Arranged {
        let unit = CGSize(width: Grid.unitWidth(cardWidth: Double(cardWidth)), height: Grid.unitHeight)
        let gap = max(2, 2 * margin)
        let blocks = bunches.map { block($0.cards, gap: gap, unit: unit) }
        let sizes = blocks.map { Extent(w: $0.w + 2 * Room.side, h: $0.h + Room.top + Room.bottom) }

        // The lengths a side's column and a middle row may grow to, tried from the shortest that
        // holds the tallest group to all of them in one, keeping the one closest to the screen's shape.
        let tallest = max(center.h, sizes.map(\.h).max() ?? 0)
        let total = sizes.map { $0.h + Room.between }.reduce(0, +)
        var tries: [Int] = []
        var length = Double(max(tallest, 1))
        while length < Double(total) {
            tries.append(Int(length.rounded(.up)))
            length *= 1.2
        }
        tries.append(max(total, tallest))
        var best: (score: Double, frames: [GridRect])?
        for limit in tries {
            let frames = put(center: center, bunches: bunches, sizes: sizes, limit: limit, unit: unit)
            let whole = frames.reduce(GridRect(x: -center.w / 2, y: -center.h / 2, w: center.w, h: center.h)) { $0.union($1) }
            let shape = Double(whole.w) * unit.width / max(1, Double(whole.h) * unit.height)
            let score = abs(log(shape / Room.shape))
            if best.map({ score < $0.score - 1e-9 }) ?? true { best = (score, frames) }
        }
        let frames = best?.frames ?? []

        // From the top-left of all of it, a unit in.
        let card = GridRect(x: -center.w / 2, y: -center.h / 2, w: center.w, h: center.h)
        let whole = frames.reduce(card) { $0.union($1) }
        let dx = 1 - whole.x, dy = 1 - whole.y
        var out = Arranged(cards: [center.id: GridPoint(x: card.x + dx, y: card.y + dy)], frames: [:])
        for (index, bunch) in bunches.enumerated() {
            let frame = frames[index]
            out.frames[bunch.id] = GridRect(x: frame.x + dx, y: frame.y + dy, w: frame.w, h: frame.h)
            for (id, at) in blocks[index].at {
                out.cards[id] = GridPoint(x: frame.x + dx + Room.side + at.x, y: frame.y + dy + Room.top + at.y)
            }
        }
        return out
    }

    /// A group's cards in rows, as many across as keeps the block closest to a screen's shape on screen
    /// (`Room.shape`), each card's place from the block's top-left. Not to a square: cards are wider than
    /// most are tall, and a few short ones would stand in one column, the tall column the map is not to be.
    /// The block's shape, not the map's: the map is put round the blocks as they are.
    static func block(_ cards: [Piece], gap: Int, unit: CGSize) -> (w: Int, h: Int, at: [String: GridPoint]) {
        guard !cards.isEmpty else { return (Grid.cardColumns, 1, [:]) }
        func laid(_ across: Int) -> (w: Int, h: Int, at: [String: GridPoint]) {
            var widths = [Int](repeating: 0, count: across)
            var heights: [Int] = []
            for (index, card) in cards.enumerated() {
                let column = index % across, row = index / across
                widths[column] = max(widths[column], card.w)
                if row == heights.count { heights.append(0) }
                heights[row] = max(heights[row], card.h)
            }
            var at: [String: GridPoint] = [:]
            for (index, card) in cards.enumerated() {
                let column = index % across, row = index / across
                at[card.id] = GridPoint(x: widths[..<column].reduce(0, +) + gap * column,
                                        y: heights[..<row].reduce(0, +) + gap * row)
            }
            return (widths.reduce(0, +) + gap * (across - 1), heights.reduce(0, +) + gap * (heights.count - 1), at)
        }
        var best: (score: Double, laid: (w: Int, h: Int, at: [String: GridPoint]))?
        for across in 1...cards.count {
            let made = laid(across)
            let score = abs(log(Double(made.w) * unit.width / max(1, Double(made.h) * unit.height) / Room.shape))
            if best.map({ score < $0.score - 1e-9 }) ?? true { best = (score, made) }
        }
        return best!.laid
    }

    /// Each group's frame round the card at the middle, its sides' columns and its middle's rows no
    /// longer than `limit` units down (across, as long on screen).
    private static func put(center: Piece, bunches: [Bunch], sizes: [Extent], limit: Int, unit: CGSize) -> [GridRect] {
        var frames = [GridRect](repeating: GridRect(x: 0, y: 0, w: 0, h: 0), count: bunches.count)
        let card = GridRect(x: -center.w / 2, y: -center.h / 2, w: center.w, h: center.h)
        let across = max(1, Int((Double(limit) * unit.height / unit.width * Room.shape).rounded()))
        let byId = Dictionary(bunches.enumerated().map { ($1.id, $0) }, uniquingKeysWith: { a, _ in a })
        let steps = Set(bunches.map(\.step)).sorted()
        // Where a group wants to be along its band: in the middle of the group it hangs from.
        func wanted(_ index: Int, along horizontal: Bool) -> Int {
            guard let anchor = bunches[index].anchor, let at = byId[anchor] else { return 0 }
            let frame = frames[at]
            return horizontal ? frame.x + frame.w / 2 : frame.y + frame.h / 2
        }

        // The middle: above and below the card, a step's rows outside the step before.
        var top = card.y - Room.fromCard.down, bottom = card.y + card.h + Room.fromCard.down
        for step in steps {
            for side in [Side.top, .bottom] {
                let here = order(bunches.indices.filter { bunches[$0].step == step && bunches[$0].side == side },
                                 step: step, sizes: sizes, wanted: { wanted($0, along: true) })
                for line in lines(here, length: across, size: { sizes[$0].w }) {
                    let tall = line.map { sizes[$0].h }.max() ?? 0
                    let lefts = spread(line, step: step, size: { sizes[$0].w }, wanted: { wanted($0, along: true) })
                    for (index, left) in zip(line, lefts) {
                        // Against the card's side of the row.
                        let y = side == .top ? top - sizes[index].h : bottom
                        frames[index] = GridRect(x: left, y: y, w: sizes[index].w, h: sizes[index].h)
                    }
                    if side == .top { top -= tall + Room.between } else { bottom += tall + Room.between }
                }
            }
        }

        // The sides: outside the middle, a step's columns outside the step before.
        let middle = bunches.indices.filter { bunches[$0].side == .top || bunches[$0].side == .bottom }
            .reduce(card) { $0.union(frames[$1]) }
        var left = middle.x - Room.fromCard.across, right = middle.x + middle.w + Room.fromCard.across
        for step in steps {
            for side in [Side.left, .right] {
                let here = order(bunches.indices.filter { bunches[$0].step == step && bunches[$0].side == side },
                                 step: step, sizes: sizes, wanted: { wanted($0, along: false) })
                for line in lines(here, length: limit, size: { sizes[$0].h }) {
                    let wide = line.map { sizes[$0].w }.max() ?? 0
                    let tops = spread(line, step: step, size: { sizes[$0].h }, wanted: { wanted($0, along: false) })
                    for (index, y) in zip(line, tops) {
                        // Against the card's side of the column.
                        let x = side == .left ? left - sizes[index].w : right
                        frames[index] = GridRect(x: x, y: y, w: sizes[index].w, h: sizes[index].h)
                    }
                    if side == .left { left -= wide + Room.between } else { right += wide + Room.between }
                }
            }
        }
        return frames
    }

    /// The order a band's groups go in: at the first step as given (the biggest first); further out,
    /// along the band by where the group each hangs from is.
    private static func order(_ indices: [Int], step: Int, sizes: [Extent], wanted: (Int) -> Int) -> [Int] {
        guard step > 1 else { return indices }
        return indices.enumerated().sorted { (wanted($0.element), $0.offset) < (wanted($1.element), $1.offset) }.map(\.element)
    }

    /// A band's groups in lines — columns of a side, rows of the middle — each no longer than
    /// `length` but for a group longer alone, the first line nearest the card.
    private static func lines(_ indices: [Int], length: Int, size: (Int) -> Int) -> [[Int]] {
        var out: [[Int]] = []
        var line: [Int] = [], used = 0
        for index in indices {
            let more = (line.isEmpty ? 0 : Room.between) + size(index)
            if !line.isEmpty, used + more > length {
                out.append(line)
                line = []
                used = 0
            }
            used += line.isEmpty ? size(index) : more
            line.append(index)
        }
        if !line.isEmpty { out.append(line) }
        return out
    }

    /// Where along its line each group starts. At the first step the line is centred on the card,
    /// the biggest group in the middle; further out each group goes as near the middle of the group
    /// it hangs from as the ones before it leave room for, and the line is moved as a whole to be as
    /// near them all as it can.
    private static func spread(_ line: [Int], step: Int, size: (Int) -> Int, wanted: (Int) -> Int) -> [Int] {
        if step == 1 {
            // Middle out: the biggest (first) in the middle, the next either side of it in turn.
            var placed: [Int] = []
            for (rank, index) in line.enumerated() {
                if rank % 2 == 0 { placed.append(index) } else { placed.insert(index, at: 0) }
            }
            let length = placed.map(size).reduce(0, +) + Room.between * (placed.count - 1)
            var at = -length / 2
            var starts: [Int: Int] = [:]
            for index in placed {
                starts[index] = at
                at += size(index) + Room.between
            }
            return line.map { starts[$0] ?? 0 }
        }
        var starts: [Int] = []
        var next = Int.min
        for index in line {
            let start = max(wanted(index) - size(index) / 2, next)
            starts.append(start)
            next = start + size(index) + Room.between
        }
        let off = zip(line, starts).map { $1 + size($0) / 2 - wanted($0) }.reduce(0, +) / max(1, line.count)
        return starts.map { $0 - off }
    }
}

extension GridRect {
    /// The least rect holding both.
    func union(_ other: GridRect) -> GridRect {
        let x0 = min(x, other.x), y0 = min(y, other.y)
        let x1 = max(x + w, other.x + other.w), y1 = max(y + h, other.y + other.h)
        return GridRect(x: x0, y: y0, w: x1 - x0, h: y1 - y0)
    }
}
