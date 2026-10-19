import CoreGraphics
import Foundation

/// Lines that go round the cards: instead of one curve from a row to
/// the card it reaches, passing under whatever lies between, a way along the cells left clear
/// around the cards (the margin, §12) — the shortest that crosses no card, of those the one
/// that turns least — drawn as runs across and down with their corners rounded. Lines sharing
/// a run are set a little apart. Worked out when cards are put or moved, not as the view moves.
nonisolated enum LineRouter {

    /// One line to route: the card it leaves and the one it reaches, in the diagram's points,
    /// and the heights it leaves at (its row) and lands at (a row, or the header).
    struct Request {
        var key: String
        var from: CGRect
        var to: CGRect
        var tailY: CGFloat
        var landY: CGFloat
    }

    /// How far apart lines sharing a run are set.
    static let laneSpacing: CGFloat = 3

    /// The corners of each line found a way, from the point it leaves its card to the point it
    /// reaches the other; a line for which there is none (the two cards overlap, no clear cell
    /// beside them) is left out, to be drawn as a curve.
    static func route(_ requests: [Request], boxes: [CGRect], cell: CGSize) -> [String: [CGPoint]] {
        guard cell.width > 0, cell.height > 0, !requests.isEmpty else { return [:] }
        var grid = Field(boxes: boxes, cell: cell, lines: requests.flatMap { [$0.tailY, $0.landY] })
        var found: [(key: String, runs: [Run])] = []
        for request in requests {
            guard let runs = grid.search(request) else { continue }
            found.append((request.key, runs))
        }
        Self.spread(&found, cell: cell)
        var out: [String: [CGPoint]] = [:]
        for (key, runs) in found {
            out[key] = Self.points(runs)
        }
        return out
    }

    // MARK: - The cells

    /// A way's directions: east, south, west, north.
    enum Way: Int { case east = 0, south, west, north
        var horizontal: Bool { self == .east || self == .west }
        var step: (dx: Int, dy: Int) {
            switch self {
            case .east: (1, 0)
            case .south: (0, 1)
            case .west: (-1, 0)
            case .north: (0, -1)
            }
        }
        var opposite: Way { Way(rawValue: (rawValue + 2) % 4)! }
    }

    /// One run of a line, across or down: where it is (`at`, the y across or the x down) and
    /// the cells it spans along the other way. The first and the last run are across: a line
    /// leaves and reaches a card at its side.
    struct Run {
        var horizontal: Bool
        var at: CGFloat
        /// The first and last points along it (x across, y down), before corners are known.
        var line: Int
        var span: ClosedRange<Int>
        /// The point the line leaves or reaches the card at, on the first and last run.
        var edge: CGFloat?
        /// How wide the gathered column (or tall the row) it runs in is, in points: the room to
        /// set lines apart in.
        var room: CGFloat
    }

    /// The cells around the cards, gathered: the grid is cut only where something begins or
    /// ends — a card's sides and the cells beside them, the rows lines leave and land at — and
    /// the cells between two cuts, all clear or all a card's, are one. Its size goes with how many
    /// cards and lines there are, not with how far apart they sit. Which a card covers, and how
    /// often a found line passes.
    struct Field {
        let cell: CGSize
        /// Where the gathered columns and rows begin, in cells: column `k` is the cells
        /// `xs[k] ..< xs[k + 1]`.
        let xs: [Int], ys: [Int]
        let cols: Int, rows: Int
        var blocked: [Bool]
        var used: [UInt8]
        /// What a turn costs, in points: so much that a way with fewer turns wins unless it is
        /// far longer.
        let turn: CGFloat
        /// What passing where another line passes costs: a little, to spread them.
        let crowd: CGFloat
        /// The search's own, kept from one line to the next: what reaching each cell (and way)
        /// cost, from where, and which search wrote it (`stamp`).
        private var cost: [CGFloat] = []
        private var from: [Int32] = []
        private var seen: [UInt32] = []
        private var stamp: UInt32 = 0
        private var heap = Heap()

        /// `lines`: the rows the lines leave and land at, each a row of its own.
        init(boxes: [CGRect], cell: CGSize, lines: [CGFloat] = [], pad: Int = 6) {
            self.cell = cell
            func column(_ x: CGFloat) -> Int { Int((x / cell.width).rounded(.down)) }
            func row(_ y: CGFloat) -> Int { Int((y / cell.height).rounded(.down)) }
            var cutX = Set<Int>(), cutY = Set<Int>()
            var cards: [(x0: Int, x1: Int, y0: Int, y1: Int)] = []
            for box in boxes {
                let (x0, x1) = (column(box.minX + 0.5), column(box.maxX - 0.5) + 1)
                let (y0, y1) = (row(box.minY + 0.5), row(box.maxY - 0.5) + 1)
                guard x1 > x0, y1 > y0 else { continue }
                cards.append((x0, x1, y0, y1))
                cutX.formUnion([x0 - 1, x0, x1, x1 + 1])
                cutY.formUnion([y0 - 1, y0, y1, y1 + 1])
            }
            for y in lines {
                cutY.formUnion([row(y), row(y) + 1])
            }
            if cutX.isEmpty { cutX = [0, 1] }
            if cutY.isEmpty { cutY = [0, 1] }
            // Room round the outside, to go round the cards there.
            cutX.formUnion([cutX.min()! - pad, cutX.max()! + pad])
            cutY.formUnion([cutY.min()! - pad, cutY.max()! + pad])
            xs = cutX.sorted()
            ys = cutY.sorted()
            cols = xs.count - 1
            rows = ys.count - 1
            blocked = Array(repeating: false, count: cols * rows)
            used = Array(repeating: 0, count: cols * rows)
            turn = 3 * max(cell.width, cell.height)
            crowd = 0.25 * min(cell.width, cell.height)
            for card in cards {
                let (k0, k1) = (Self.gathered(card.x0, in: xs), Self.gathered(card.x1, in: xs))
                let (j0, j1) = (Self.gathered(card.y0, in: ys), Self.gathered(card.y1, in: ys))
                for j in j0..<j1 {
                    for k in k0..<k1 {
                        blocked[j * cols + k] = true
                    }
                }
            }
        }

        /// The gathered column (or row) cell `c` is in: the last cut at or before it.
        static func gathered(_ c: Int, in cuts: [Int]) -> Int {
            var (lo, hi) = (0, cuts.count - 1)
            while lo < hi {
                let mid = (lo + hi + 1) / 2
                if cuts[mid] <= c { lo = mid } else { hi = mid - 1 }
            }
            return min(lo, cuts.count - 2)
        }

        func column(_ x: CGFloat) -> Int { Self.gathered(Int((x / cell.width).rounded(.down)), in: xs) }
        func row(_ y: CGFloat) -> Int { Self.gathered(Int((y / cell.height).rounded(.down)), in: ys) }
        func inside(_ x: Int, _ y: Int) -> Bool { x >= 0 && x < cols && y >= 0 && y < rows }
        func index(_ x: Int, _ y: Int) -> Int { y * cols + x }
        func free(_ x: Int, _ y: Int) -> Bool { inside(x, y) && !blocked[index(x, y)] }
        /// The middle of a gathered column, and of a row, in points.
        func centerX(_ x: Int) -> CGFloat { CGFloat(xs[x] + xs[x + 1]) / 2 * cell.width }
        func centerY(_ y: Int) -> CGFloat { CGFloat(ys[y] + ys[y + 1]) / 2 * cell.height }

        /// The way for one line, or nil: out of either side of its card at its row, into either
        /// side of the other at its landing — shortest, turning least.
        mutating func search(_ request: Request) -> [Run]? {
            let (a, b) = (request.from, request.to)
            guard !a.intersects(b) else { return nil }
            let tailRow = row(request.tailY), landRow = row(request.landY)
            // Out of the card's right side going east, or its left going west: the cells beside it.
            let starts: [(x: Int, y: Int, way: Way, edge: CGFloat)] = [
                (column(a.maxX - 0.5) + 1, tailRow, .east, a.maxX),
                (column(a.minX + 0.5) - 1, tailRow, .west, a.minX),
            ].filter { free($0.x, $0.y) }
            // Into the other's left side going east, or its right going west.
            let goals: [(x: Int, y: Int, way: Way, edge: CGFloat)] = [
                (column(b.minX + 0.5) - 1, landRow, .east, b.minX),
                (column(b.maxX - 0.5) + 1, landRow, .west, b.maxX),
            ].filter { free($0.x, $0.y) }
            guard !starts.isEmpty, !goals.isEmpty else { return nil }

            let count = cols * rows * 4
            if cost.count != count {
                cost = [CGFloat](repeating: .infinity, count: count)
                from = [Int32](repeating: -1, count: count)
                seen = [UInt32](repeating: 0, count: count)
            }
            stamp &+= 1
            heap.removeAll()
            let (g0x, g0y) = (centerX(goals[0].x), centerY(goals[0].y))
            let (g1x, g1y) = goals.count > 1 ? (centerX(goals[1].x), centerY(goals[1].y)) : (g0x, g0y)
            let (g0r, g1r) = (goals[0].y, goals.count > 1 ? goals[1].y : goals[0].y)
            func key(_ i: Int, _ way: Way) -> Int { i * 4 + way.rawValue }
            // The distance left, and the turns it takes at the least: a line is let into a card
            // across, so one going across at another row turns twice more, one going down once.
            func guess(_ x: Int, _ y: Int, _ way: Way) -> CGFloat {
                let (px, py) = (centerX(x), centerY(y))
                func to(_ gx: CGFloat, _ gy: CGFloat, _ gr: Int) -> CGFloat {
                    let turns: CGFloat = gr == y ? (way.horizontal ? 0 : 1) : (way.horizontal ? 2 : 1)
                    return abs(gx - px) + abs(gy - py) + turns * turn
                }
                return min(to(g0x, g0y, g0r), to(g1x, g1y, g1r))
            }
            func costOf(_ k: Int) -> CGFloat { seen[k] == stamp ? cost[k] : .infinity }
            // Of the ways that look as good, the one further along first: across open room many
            // look the same, and going on with one finds the way without trying each.
            func priority(_ g: CGFloat, _ h: CGFloat) -> CGFloat { g + h * (1 + 1e-6) }
            for start in starts {
                let k = key(index(start.x, start.y), start.way)
                seen[k] = stamp
                cost[k] = 0
                from[k] = -1
                heap.push(priority(0, guess(start.x, start.y, start.way)), k)
            }
            var reached: (key: Int, goal: Int)?
            var best = CGFloat.infinity
            while let (f, k) = heap.pop() {
                if f >= best { break }
                let i = k / 4, way = Way(rawValue: k % 4)!
                let g = cost[k]
                let x = i % cols, y = i / cols
                if f > priority(g, guess(x, y, way)) + 0.001 { continue }
                for n in goals.indices where goals[n].x == x && goals[n].y == y {
                    let total = g + (way == goals[n].way ? 0 : turn)
                    if total < best {
                        best = total
                        reached = (k, n)
                    }
                }
                for next in [Way.east, .south, .west, .north] where next != way.opposite {
                    let (nx, ny) = (x + next.step.dx, y + next.step.dy)
                    guard free(nx, ny) else { continue }
                    let ni = index(nx, ny)
                    let step = next.horizontal ? abs(centerX(nx) - centerX(x)) : abs(centerY(ny) - centerY(y))
                    let ng = g + step + (next == way ? 0 : turn) + crowd * CGFloat(used[ni])
                    let nk = key(ni, next)
                    if ng < costOf(nk) {
                        seen[nk] = stamp
                        cost[nk] = ng
                        from[nk] = Int32(k)
                        heap.push(priority(ng, guess(nx, ny, next)), nk)
                    }
                }
            }
            guard let reached else { return nil }
            // The cells back to the start, each with the way it was entered.
            var steps: [(x: Int, y: Int, way: Way)] = []
            var k = reached.key
            while k >= 0 {
                let i = k / 4
                steps.append((i % cols, i / cols, Way(rawValue: k % 4)!))
                k = Int(from[k])
            }
            steps.reverse()
            for step in steps { used[index(step.x, step.y)] = used[index(step.x, step.y)] &+ 1 }
            let goal = goals[reached.goal]
            let start = starts.first { $0.x == steps[0].x && $0.y == steps[0].y && $0.way == steps[0].way } ?? starts[0]
            return runs(steps, goalWay: goal.way, startEdge: start.edge, goalEdge: goal.edge,
                        tailY: request.tailY, landY: request.landY)
        }

        /// The cells walked, as runs across and down: the first at the row it leaves, the last at
        /// the row it lands.
        private func runs(_ steps: [(x: Int, y: Int, way: Way)], goalWay: Way, startEdge: CGFloat,
                          goalEdge: CGFloat, tailY: CGFloat, landY: CGFloat) -> [Run] {
            // The way into each cell, and out of the last into the card.
            let ways = steps.map(\.way) + [goalWay]
            var out: [Run] = []
            var i = 0
            while i < ways.count {
                var j = i
                while j + 1 < ways.count && ways[j + 1] == ways[i] { j += 1 }
                let cellAt = steps[min(i, steps.count - 1)]
                let horizontal = ways[i].horizontal
                let first = steps[max(i - 1, 0)], last = steps[min(j, steps.count - 1)]
                let span = horizontal ? min(first.x, last.x)...max(first.x, last.x)
                                      : min(first.y, last.y)...max(first.y, last.y)
                out.append(Run(horizontal: horizontal,
                               at: horizontal ? centerY(cellAt.y) : centerX(cellAt.x),
                               line: horizontal ? cellAt.y : cellAt.x, span: span, edge: nil,
                               room: horizontal ? CGFloat(ys[cellAt.y + 1] - ys[cellAt.y]) * cell.height
                                                : CGFloat(xs[cellAt.x + 1] - xs[cellAt.x]) * cell.width))
                i = j + 1
            }
            out[0].at = tailY
            out[0].edge = startEdge
            out[out.count - 1].at = landY
            out[out.count - 1].edge = goalEdge
            // Straight across from one card to the other at two heights: a step down between.
            if out.count == 1 {
                let x = (startEdge + goalEdge) / 2
                let one = out[0]
                out = [Run(horizontal: true, at: tailY, line: one.line, span: one.span, edge: startEdge, room: one.room),
                       Run(horizontal: false, at: x, line: column(x), span: one.span, edge: nil, room: cell.width),
                       Run(horizontal: true, at: landY, line: one.line, span: one.span, edge: goalEdge, room: one.room)]
            }
            return out
        }
    }

    // MARK: - Lines apart

    /// Runs of different lines down the same column, or across the same row, where their spans
    /// meet: each set a lane apart, the lanes centred on the cells' middle. Not the first and
    /// last runs, which are at the rows the line leaves and lands.
    static func spread(_ found: inout [(key: String, runs: [Run])], cell: CGSize) {
        struct Place: Hashable { var horizontal: Bool; var line: Int }
        var byPlace: [Place: [(line: Int, run: Int)]] = [:]
        for (l, item) in found.enumerated() where item.runs.count > 2 {
            for r in 1..<(item.runs.count - 1) {
                let run = item.runs[r]
                byPlace[Place(horizontal: run.horizontal, line: run.line), default: []].append((l, r))
            }
        }
        for (_, members) in byPlace where members.count > 1 {
            var lanes: [(span: ClosedRange<Int>, lane: Int)] = []
            var assigned: [(line: Int, run: Int, lane: Int)] = []
            let sorted = members.sorted { found[$0.line].runs[$0.run].span.lowerBound < found[$1.line].runs[$1.run].span.lowerBound }
            for member in sorted {
                let span = found[member.line].runs[member.run].span
                var lane = 0
                while lanes.contains(where: { $0.lane == lane && $0.span.overlaps(span) }) { lane += 1 }
                lanes.append((span, lane))
                assigned.append((member.line, member.run, lane))
            }
            let most = (assigned.map(\.lane).max() ?? 0)
            guard most > 0 else { continue }
            let room = (assigned.map { found[$0.line].runs[$0.run].room }.min() ?? cell.height) - 4
            let spacing = min(laneSpacing, max(room, 0) / CGFloat(most + 1))
            for (line, run, lane) in assigned {
                found[line].runs[run].at += (CGFloat(lane) - CGFloat(most) / 2) * spacing
            }
        }
    }

    /// The corners of a line from its runs: where it leaves the card, where each run meets the
    /// next, and where it reaches the other card.
    static func points(_ runs: [Run]) -> [CGPoint] {
        guard let first = runs.first, let last = runs.last, let start = first.edge, let end = last.edge else { return [] }
        var out = [CGPoint(x: start, y: first.at)]
        for i in 0..<(runs.count - 1) {
            let (a, b) = (runs[i], runs[i + 1])
            out.append(a.horizontal ? CGPoint(x: b.at, y: a.at) : CGPoint(x: a.at, y: b.at))
        }
        out.append(CGPoint(x: end, y: last.at))
        return out
    }

    // MARK: - Drawing

    /// The line through `points`, its corners rounded.
    static func path(_ points: [CGPoint], radius: CGFloat = 6) -> CGPath {
        let path = CGMutablePath()
        guard let first = points.first else { return path }
        path.move(to: first)
        for i in 1..<max(points.count - 1, 1) where points.count > 2 {
            let (p, c, n) = (points[i - 1], points[i], points[i + 1])
            let r = min(radius, hypot(c.x - p.x, c.y - p.y) / 2, hypot(n.x - c.x, n.y - c.y) / 2)
            if r > 0.5 {
                path.addArc(tangent1End: c, tangent2End: n, radius: r)
            } else {
                path.addLine(to: c)
            }
        }
        if let last = points.last, points.count > 1 { path.addLine(to: last) }
        return path
    }

    /// The point `fraction` of the way along the line.
    static func point(_ points: [CGPoint], at fraction: CGFloat) -> CGPoint {
        guard points.count > 1 else { return points.first ?? .zero }
        let lengths = zip(points, points.dropFirst()).map { hypot($1.x - $0.x, $1.y - $0.y) }
        var left = lengths.reduce(0, +) * min(max(fraction, 0), 1)
        for (i, length) in lengths.enumerated() {
            if left <= length, length > 0 {
                let t = left / length
                return CGPoint(x: points[i].x + (points[i + 1].x - points[i].x) * t,
                               y: points[i].y + (points[i + 1].y - points[i].y) * t)
            }
            left -= length
        }
        return points[points.count - 1]
    }
}

/// A queue of the cheapest first, for the search.
nonisolated struct Heap {
    private var items: [(CGFloat, Int)] = []

    mutating func removeAll() { items.removeAll(keepingCapacity: true) }

    mutating func push(_ priority: CGFloat, _ value: Int) {
        items.append((priority, value))
        var i = items.count - 1
        while i > 0 {
            let parent = (i - 1) / 2
            guard items[i].0 < items[parent].0 else { break }
            items.swapAt(i, parent)
            i = parent
        }
    }

    mutating func pop() -> (CGFloat, Int)? {
        guard !items.isEmpty else { return nil }
        let top = items[0]
        let last = items.removeLast()
        if !items.isEmpty {
            items[0] = last
            var i = 0
            while true {
                let (l, r) = (2 * i + 1, 2 * i + 2)
                var m = i
                if l < items.count, items[l].0 < items[m].0 { m = l }
                if r < items.count, items[r].0 < items[m].0 { m = r }
                if m == i { break }
                items.swapAt(i, m)
                i = m
            }
        }
        return top
    }
}

/// How this Mac draws the lines between cards: round the cards, or as
/// one curve each, passing under what lies between. This Mac's setting — not the project's, nor
/// in its file — set in the settings' 「図」 and in the relation panel's filters, and followed
/// by the relation panel and the card maps alike. Round the cards unless said otherwise: on the
/// largest project's diagrams the ways are found in a few milliseconds (160 cards and 300 lines on
/// one diagram, the most that could be drawn at once, take some 30; the same spread 35 times as
/// far apart, some 450).
nonisolated enum LineRouting {
    static let key = "diagram.linesAvoidCards"
    static let standard = true

    static var avoidsCards: Bool {
        UserDefaults.standard.object(forKey: key) as? Bool ?? standard
    }
}
