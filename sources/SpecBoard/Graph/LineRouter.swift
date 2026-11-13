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
    struct Request: Equatable {
        var key: String
        var from: CGRect
        var to: CGRect
        var tailY: CGFloat
        var landY: CGFloat
    }

    /// How far apart lines sharing a run are set.
    static let laneSpacing: CGFloat = 3

    /// How near a box that came, went or changed a way passes to be found again, in cells: a way
    /// round a card keeps to the cells beside it, so one that went round a card that left is within this.
    static let near: CGFloat = 2

    /// The corners of each line found a way, from the point it leaves its card to the point it
    /// reaches the other; a line for which there is none (the two cards overlap, no clear cell
    /// beside them) is left out, to be drawn as a curve.
    static func route(_ requests: [Request], boxes: [CGRect], cell: CGSize) -> [String: [CGPoint]] {
        find(requests, boxes: boxes, cell: cell).routes
    }

    /// What was found for a diagram, with what it was found for: what the next search of the diagram
    /// starts from (`find`'s `from`), so that a card moved has only the lines near it found again.
    struct Found: Equatable {
        var requests: [Request] = []
        var boxes: [CGRect] = []
        var cell: CGSize = .zero
        /// Each line's corners as it is drawn.
        var routes: [String: [CGPoint]] = [:]
        /// How many lines a way was searched for; the others kept the way they had.
        var searched = 0
        /// Each line's runs as it is drawn: where a way kept runs, for the ways found beside it
        /// later to keep off.
        fileprivate var runs: [String: [Run]] = [:]
    }

    /// The ways for the lines, with what the next search starts from. From `before` — what was found
    /// for the diagram as it was — a line keeps its way when its ends are as they were, the way
    /// passes no box that came, went or changed nor the cells near one (`near`), and no card sits on
    /// it now: only the other lines are searched, the ways kept counted as passing where they pass.
    /// A way kept stays to the point; the ways found are set a lane off the kept ones they run
    /// beside. Every line is searched when most would lose their way anyway — the ways then are as
    /// if nothing had been found before — and when the cells are not the size they were. A line that
    /// had no way (its cards overlap) is searched each time, and is not counted among those.
    ///
    /// A way kept is not always the one a search of everything would find now: a card moved no
    /// longer changes the lines it has nothing to do with.
    static func find(_ requests: [Request], boxes: [CGRect], cell: CGSize, from before: Found? = nil) -> Found {
        var out = Found(requests: requests, boxes: boxes, cell: cell)
        guard cell.width > 0, cell.height > 0, !requests.isEmpty else { return out }
        var grid = Field(boxes: boxes, cell: cell, lines: requests.flatMap { [$0.tailY, $0.landY] })
        var kept: [String: [Run]] = [:]
        if let before, before.cell == cell {
            let was = Dictionary(before.requests.map { ($0.key, $0) }, uniquingKeysWith: { first, _ in first })
            let changed = boxes.filter { !before.boxes.contains($0) } + before.boxes.filter { !boxes.contains($0) }
            let around = changed.map { $0.insetBy(dx: -near * cell.width, dy: -near * cell.height) }
            for request in requests {
                guard was[request.key] == request, let way = before.routes[request.key],
                      let runs = before.runs[request.key],
                      !Self.passes(way, around), grid.clear(way, of: request) else { continue }
                kept[request.key] = runs.map { grid.kept($0) }
            }
            // A line that had no way and is as it was loses none: looked for again, which is soon
            // done, without making the others be.
            let lost = requests.filter {
                kept[$0.key] == nil && !(was[$0.key] == $0 && before.routes[$0.key] == nil)
            }.count
            if lost * 2 > requests.count { kept = [:] }
            for request in requests where kept[request.key] != nil {
                grid.count(before.routes[request.key] ?? [], of: request)
            }
        }
        var found: [(key: String, runs: [Run])] = []
        for request in requests where kept[request.key] == nil {
            out.searched += 1
            guard let runs = grid.search(request) else { continue }
            found.append((request.key, runs))
        }
        // Not the first and last runs, which are at the rows a line leaves and lands.
        let fixed = kept.values.flatMap { $0.count > 2 ? Array($0[1..<($0.count - 1)]) : [] }
        Self.spread(&found, cell: cell, beside: fixed)
        for (key, runs) in found {
            out.runs[key] = runs
            out.routes[key] = Self.points(runs)
        }
        for (key, runs) in kept {
            out.runs[key] = runs
            out.routes[key] = before?.routes[key]
        }
        return out
    }

    /// Whether a run of the way passes through any of the boxes.
    private static func passes(_ way: [CGPoint], _ boxes: [CGRect]) -> Bool {
        guard !boxes.isEmpty else { return false }
        return zip(way, way.dropFirst()).contains { p, q in
            // A little thick: a run has no width of its own to meet a box with.
            let run = CGRect(x: min(p.x, q.x), y: min(p.y, q.y), width: abs(q.x - p.x) + 0.01, height: abs(q.y - p.y) + 0.01)
            return boxes.contains { $0.intersects(run) }
        }
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
    struct Run: Equatable {
        var horizontal: Bool
        var at: CGFloat
        /// The first and last points along it (x across, y down), before corners are known.
        var line: Int
        var span: ClosedRange<Int>
        /// What `span` covers, in points, and whether it is counted across (in columns): what a run
        /// kept from an earlier search is placed by on the cells of a later one, cut elsewhere.
        var reach: ClosedRange<CGFloat> = 0...0
        var across = true
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
            // Counted up to its most and kept there: a count that wrapped would make the most crowded cell look free.
            for step in steps where used[index(step.x, step.y)] < .max { used[index(step.x, step.y)] += 1 }
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
                               line: horizontal ? cellAt.y : cellAt.x, span: span,
                               reach: reach(span, across: horizontal), across: horizontal, edge: nil,
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
                out = [Run(horizontal: true, at: tailY, line: one.line, span: one.span, reach: one.reach, edge: startEdge, room: one.room),
                       Run(horizontal: false, at: x, line: column(x), span: one.span, reach: one.reach, edge: nil, room: cell.width),
                       Run(horizontal: true, at: landY, line: one.line, span: one.span, reach: one.reach, edge: goalEdge, room: one.room)]
            }
            return out
        }

        // MARK: Ways kept from an earlier search

        /// What the gathered columns (or rows) `span` cover, in points.
        private func reach(_ span: ClosedRange<Int>, across: Bool) -> ClosedRange<CGFloat> {
            across ? CGFloat(xs[span.lowerBound]) * cell.width...CGFloat(xs[span.upperBound + 1]) * cell.width
                   : CGFloat(ys[span.lowerBound]) * cell.height...CGFloat(ys[span.upperBound + 1]) * cell.height
        }

        /// A run found on other cells, placed on these: where it runs and what it reaches are as
        /// they were, in points; the column or row it is in and the ones it spans are these cells'.
        /// The room to set lines apart in is no more than it was found with.
        func kept(_ run: Run) -> Run {
            var out = run
            out.line = run.horizontal ? row(run.at) : column(run.at)
            let (low, high) = (run.reach.lowerBound + 0.5, run.reach.upperBound - 0.5)
            let (first, last) = run.across ? (column(low), column(high)) : (row(low), row(high))
            out.span = min(first, last)...max(first, last)
            let room = run.horizontal ? CGFloat(ys[out.line + 1] - ys[out.line]) * cell.height
                                      : CGFloat(xs[out.line + 1] - xs[out.line]) * cell.width
            out.room = min(run.room, room)
            return out
        }

        /// The cells a way passes, corner to corner, from the cell beside the card it leaves to the
        /// cell beside the card it reaches.
        private func cells(_ way: [CGPoint], of request: Request) -> [Int] {
            guard way.count >= 2 else { return [] }
            var out: [Int] = []
            let last = way.count - 2
            for i in 0...last {
                let (p, q) = (way[i], way[i + 1])
                if p.y == q.y && (p.x != q.x || i % 2 == 0) {
                    let east = q.x >= p.x
                    var (from, to) = (column(p.x), column(q.x))
                    // Not the card's own cells: a line leaves and reaches a card at its side.
                    if i == 0 {
                        from = east ? column(request.from.maxX - 0.5) + 1 : column(request.from.minX + 0.5) - 1
                    }
                    if i == last {
                        to = east ? column(request.to.minX + 0.5) - 1 : column(request.to.maxX - 0.5) + 1
                    }
                    let y = row(p.y)
                    guard east ? from <= to : from >= to else { continue }
                    for x in min(from, to)...max(from, to) where inside(x, y) { out.append(index(x, y)) }
                } else {
                    let x = column(p.x)
                    let (from, to) = (row(min(p.y, q.y)), row(max(p.y, q.y)))
                    for y in from...to where inside(x, y) { out.append(index(x, y)) }
                }
            }
            return out
        }

        /// Whether no card sits on the way now.
        func clear(_ way: [CGPoint], of request: Request) -> Bool {
            cells(way, of: request).allSatisfy { !blocked[$0] }
        }

        /// A way kept counted as passing where it passes, as one found here is.
        mutating func count(_ way: [CGPoint], of request: Request) {
            for i in Set(cells(way, of: request)) where used[i] < .max { used[i] += 1 }
        }
    }

    // MARK: - Lines apart

    /// Runs of different lines down the same column, or across the same row, where their spans
    /// meet: each set a lane apart, the lanes centred on the cells' middle. Not the first and
    /// last runs, which are at the rows the line leaves and lands. `fixed`: the runs of ways kept
    /// from an earlier search, which stay where they are — the runs of a column or row they run in
    /// are set off them instead (`setOff`).
    static func spread(_ found: inout [(key: String, runs: [Run])], cell: CGSize, beside fixed: [Run] = []) {
        struct Place: Hashable { var horizontal: Bool; var line: Int }
        var byPlace: [Place: [(line: Int, run: Int)]] = [:]
        for (l, item) in found.enumerated() where item.runs.count > 2 {
            for r in 1..<(item.runs.count - 1) {
                let run = item.runs[r]
                byPlace[Place(horizontal: run.horizontal, line: run.line), default: []].append((l, r))
            }
        }
        if !fixed.isEmpty {
            let (across, down) = (fixed.filter(\.horizontal), fixed.filter { !$0.horizontal })
            for (place, members) in byPlace {
                // The kept runs within the members' column or row, or a lane beside it.
                let at = members.map { found[$0.line].runs[$0.run] }
                let low = (at.map { $0.at - $0.room / 2 }.min() ?? 0) - laneSpacing
                let high = (at.map { $0.at + $0.room / 2 }.max() ?? 0) + laneSpacing
                let kept = (place.horizontal ? across : down).filter { $0.at >= low && $0.at <= high }
                guard !kept.isEmpty else { continue }
                Self.setOff(&found, members, from: kept)
                byPlace[place] = nil
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

    /// The runs found down a column, or across a row, where ways kept run: each put at the place
    /// nearest its own middle — the middle, then a lane to either side, then two — that is a lane off
    /// every kept run, and every run put before it, whose span meets its own. Where its room holds
    /// no such place (the kept runs are closer than a lane apart), at the place furthest from them,
    /// by half lanes: never on top of one while there is room beside it.
    private static func setOff(_ found: inout [(key: String, runs: [Run])], _ members: [(line: Int, run: Int)], from kept: [Run]) {
        var taken: [(span: ClosedRange<Int>, at: CGFloat)] = kept.map { ($0.span, $0.at) }
        let sorted = members.sorted { found[$0.line].runs[$0.run].span.lowerBound < found[$1.line].runs[$1.run].span.lowerBound }
        for member in sorted {
            let run = found[member.line].runs[member.run]
            let reach = max(run.room - 4, 0) / 2
            func free(_ offset: CGFloat) -> Bool {
                !taken.contains { $0.span.overlaps(run.span) && abs($0.at - (run.at + offset)) < laneSpacing - 0.01 }
            }
            var offset: CGFloat = 0
            var step = 0
            while !free(offset) {
                step += 1
                let aside = CGFloat(step) * laneSpacing
                guard aside <= reach else {
                    // How far the nearest run whose span meets its own is from each place.
                    func clearance(_ offset: CGFloat) -> CGFloat {
                        taken.filter { $0.span.overlaps(run.span) }.map { abs($0.at - (run.at + offset)) }.min() ?? .infinity
                    }
                    let half = laneSpacing / 2
                    let places = (0...Int(reach / half)).flatMap { n -> [CGFloat] in
                        n == 0 ? [0] : [CGFloat(n) * half, -CGFloat(n) * half]
                    }
                    // The first of the furthest: the nearest the middle of them.
                    offset = places.reduce(0) { clearance($1) > clearance($0) + 0.01 ? $1 : $0 }
                    break
                }
                if free(aside) { offset = aside } else if free(-aside) { offset = -aside } else { offset = aside }
            }
            found[member.line].runs[member.run].at += offset
            taken.append((run.span, run.at + offset))
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

/// The ways round the cards found for the diagrams seen last (`LineRouter.find`), each with what it was
/// found for — the lines' ends, the boxes, the cells: going back to a diagram whose boxes and lines are as
/// they were takes its ways rather than searching again, so it is drawn as it was left. What is kept
/// passes no card of those boxes, whatever it was found from (a search of everything, or one from the
/// ways before a card was put): a write that moved a box or a line's end asks for another. The
/// most recent `diagrams` are kept, and no more than `lines` lines in all, the oldest let go first.
struct RouteMemory {
    private struct Kept {
        var hash: Int
        var requests: [LineRouter.Request]
        var boxes: [String: CGRect]
        var cell: CGSize
        var found: LineRouter.Found
    }

    let diagrams: Int
    let lines: Int
    /// The most recent last.
    private var kept: [Kept] = []

    init(diagrams: Int = 16, lines: Int = 20_000) {
        self.diagrams = diagrams
        self.lines = lines
    }

    /// How many diagrams are kept now.
    var count: Int { kept.count }

    /// What was found for these, if kept — then the most recent.
    mutating func found(_ requests: [LineRouter.Request], boxes: [String: CGRect], cell: CGSize) -> LineRouter.Found? {
        let hash = Self.hash(requests, boxes, cell)
        guard let at = kept.lastIndex(where: {
            $0.hash == hash && $0.cell == cell && $0.requests == requests && $0.boxes == boxes
        }) else { return nil }
        let found = kept.remove(at: at)
        kept.append(found)
        return found.found
    }

    /// What was found for these, kept as the most recent; the oldest let go past the limits.
    mutating func keep(_ found: LineRouter.Found, for requests: [LineRouter.Request], boxes: [String: CGRect], cell: CGSize) {
        let hash = Self.hash(requests, boxes, cell)
        kept.removeAll { $0.hash == hash && $0.cell == cell && $0.requests == requests && $0.boxes == boxes }
        kept.append(Kept(hash: hash, requests: requests, boxes: boxes, cell: cell, found: found))
        var total = kept.reduce(0) { $0 + $1.requests.count }
        while kept.count > 1, kept.count > diagrams || total > lines {
            total -= kept.removeFirst().requests.count
        }
    }

    /// Alike for the same lines, boxes and cells, whatever order the boxes are held in.
    private static func hash(_ requests: [LineRouter.Request], _ boxes: [String: CGRect], _ cell: CGSize) -> Int {
        func rect(_ r: CGRect, into h: inout Hasher) {
            h.combine(r.origin.x); h.combine(r.origin.y); h.combine(r.size.width); h.combine(r.size.height)
        }
        var h = Hasher()
        h.combine(cell.width)
        h.combine(cell.height)
        h.combine(requests.count)
        for r in requests {
            h.combine(r.key)
            rect(r.from, into: &h)
            rect(r.to, into: &h)
            h.combine(r.tailY)
            h.combine(r.landY)
        }
        var boxSum = 0
        for (key, box) in boxes {
            var one = Hasher()
            one.combine(key)
            rect(box, into: &one)
            boxSum = boxSum &+ one.finalize()
        }
        h.combine(boxes.count)
        h.combine(boxSum)
        return h.finalize()
    }
}
