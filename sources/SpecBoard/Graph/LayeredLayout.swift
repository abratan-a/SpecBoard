import CoreGraphics
import Foundation

/// Layered left-to-right placement — the ELK replacement.
///
/// The web build handed this to elkjs with `algorithm=layered, direction=RIGHT`.
/// There is no ELK for Swift, so this is the same pipeline written out: break
/// cycles, assign layers, **split long edges**, reduce crossings, then place. It
/// reads like an ER diagram — things that are referenced sit downstream of the
/// things referencing them.
///
/// The long-edge split is not an optimisation, it is what makes the rest work. An
/// edge crossing fifteen columns has no say in how the fourteen columns between
/// its ends are ordered unless it is *present* in them, so without the split its
/// two endpoints drift apart vertically and the diagram fills up with lines
/// sweeping corner to corner. Measured on the real project before this existed: 45%
/// of edges crossed more than one column, with a median length of 2291pt.
///
/// Deliberately deterministic. Nothing here samples or randomises, so the same project
/// always lays out the same way; "自動整列" moving cards around differently each
/// press would make the button untrustworthy.
enum LayeredLayout {

    struct Node {
        var id: String
        var width: CGFloat
        var height: CGFloat
    }

    struct Edge: Hashable {
        var from: String
        var to: String
    }

    struct Placement {
        var id: String
        var posX: CGFloat
        var posY: CGFloat
    }

    struct Spacing {
        /// Between two cards in the same column.
        var nodeToNode: CGFloat = 48
        /// Between a card and a line passing it, or between two passing lines. Much
        /// tighter: a bundle of long edges at card spacing would push the diagram
        /// apart for the sake of empty corridors.
        var nodeToBend: CGFloat = 14
        /// Between columns.
        var betweenLayers: CGFloat = 96
        var padding: CGFloat = 24
    }

    /// How many median/adjust sweeps to run. Four each way is where the crossing
    /// count stops improving measurably on the real project.
    private static let sweeps = 4

    /// A vertex in the expanded graph: either a real card or a bend standing in for
    /// one column's worth of a long edge.
    private struct Vertex {
        var width: CGFloat
        var height: CGFloat
        var isBend: Bool
    }

    static func run(nodes: [Node], edges rawEdges: [Edge],
                    spacing: Spacing = Spacing()) -> [Placement] {
        guard !nodes.isEmpty else { return [] }

        let order = Dictionary(uniqueKeysWithValues: nodes.enumerated().map { ($1.id, $0) })
        let byId = Dictionary(uniqueKeysWithValues: nodes.map { ($0.id, $0) })

        // One edge per ordered pair; duplicates would only add weight, not
        // information. Self-references never reach here — they draw as ↺ on the row.
        var edges: [Edge] = []
        var seenPairs = Set<Edge>()
        for e in rawEdges where e.from != e.to {
            guard byId[e.from] != nil, byId[e.to] != nil else { continue }
            if seenPairs.insert(e).inserted { edges.append(e) }
        }

        let acyclic = breakCycles(nodes: nodes, edges: edges, order: order)
        let layers = assignLayers(nodes: nodes, edges: acyclic, order: order)

        var vertices = Dictionary(uniqueKeysWithValues: nodes.map {
            ($0.id, Vertex(width: $0.width, height: $0.height, isBend: false))
        })
        let (expandedEdges, expandedLayers) = split(
            acyclic, layers: layers, into: &vertices)

        var columns = groupIntoColumns(layers: expandedLayers, order: order,
                                       realCount: nodes.count)
        reduceCrossings(&columns, edges: expandedEdges)

        let placed = place(columns: columns, vertices: vertices,
                           edges: expandedEdges, spacing: spacing)
        return placed.filter { vertices[$0.id]?.isBend == false }
    }

    // MARK: - 1. Break cycles

    /// Reverse back edges so the rest of the pipeline sees a DAG.
    ///
    /// A schema graph cycles constantly — `Order.user` / `User.orders` is one, and
    /// digest chains make longer ones. Reversing rather than dropping keeps the pair
    /// in the same layering conversation; the drawn edge direction comes from the
    /// relation, not from here.
    private static func breakCycles(
        nodes: [Node], edges: [Edge], order: [String: Int]
    ) -> [Edge] {
        var outgoing: [String: [String]] = [:]
        for e in edges { outgoing[e.from, default: []].append(e.to) }
        for key in outgoing.keys {
            outgoing[key]?.sort { (order[$0] ?? 0) < (order[$1] ?? 0) }
        }

        var state: [String: Int] = [:]     // 0 unseen, 1 on stack, 2 done
        var reversed = Set<Edge>()

        // Iterative: 163 nodes is nothing, but a deep digest chain plus recursion in
        // a layout pass is not a stack depth worth betting on.
        for start in nodes.map(\.id) where state[start] == nil {
            var stack: [(node: String, index: Int)] = [(start, 0)]
            state[start] = 1

            while let top = stack.last {
                let children = outgoing[top.node] ?? []
                if top.index < children.count {
                    stack[stack.count - 1].index += 1
                    let next = children[top.index]
                    switch state[next] {
                    case nil:
                        state[next] = 1
                        stack.append((next, 0))
                    case 1:
                        reversed.insert(Edge(from: top.node, to: next))   // back edge
                    default:
                        break
                    }
                } else {
                    state[top.node] = 2
                    stack.removeLast()
                }
            }
        }

        return edges.map { reversed.contains($0) ? Edge(from: $0.to, to: $0.from) : $0 }
    }

    // MARK: - 2. Assign layers

    /// Longest path, then pulled tight.
    ///
    /// Longest path alone leaves a node sitting far to the left of everything it
    /// points at, purely because *one* of its own ancestors was deep. The tightening
    /// pass slides every node as far right as its successors allow, which shortens
    /// the edges that the split below would otherwise have to fill with bends.
    private static func assignLayers(
        nodes: [Node], edges: [Edge], order: [String: Int]
    ) -> [String: Int] {
        var incoming: [String: [String]] = [:]
        var outgoing: [String: [String]] = [:]
        for e in edges {
            incoming[e.to, default: []].append(e.from)
            outgoing[e.from, default: []].append(e.to)
        }

        var indegree: [String: Int] = [:]
        for n in nodes { indegree[n.id] = incoming[n.id]?.count ?? 0 }

        var ready = nodes.map(\.id).filter { indegree[$0] == 0 }
            .sorted { (order[$0] ?? 0) < (order[$1] ?? 0) }

        var layer: [String: Int] = [:]
        var settled: [String] = []
        while let id = ready.first {
            ready.removeFirst()
            settled.append(id)
            layer[id] = (incoming[id] ?? []).compactMap { layer[$0] }.max().map { $0 + 1 } ?? 0

            for next in (outgoing[id] ?? []).sorted(by: { (order[$0] ?? 0) < (order[$1] ?? 0) }) {
                indegree[next]! -= 1
                if indegree[next] == 0 { ready.append(next) }
            }
        }

        // Anything left sat in a cycle the reversal did not fully open. Rather than
        // drop it, put it after whatever of its predecessors is known.
        for n in nodes where layer[n.id] == nil {
            layer[n.id] = ((incoming[n.id] ?? []).compactMap { layer[$0] }.max() ?? -1) + 1
        }

        // Tighten: walk back through the settled order and move each node to just
        // before its earliest successor.
        for id in settled.reversed() {
            guard let successors = outgoing[id], !successors.isEmpty else { continue }
            guard let earliest = successors.compactMap({ layer[$0] }).min() else { continue }
            if earliest - 1 > layer[id]! { layer[id] = earliest - 1 }
        }
        return layer
    }

    // MARK: - 3. Split long edges

    /// Replace every edge crossing more than one column with a chain of bends, one
    /// per column in between. After this, every edge in the graph joins adjacent
    /// columns — which is the precondition both later stages assume.
    private static func split(
        _ edges: [Edge], layers: [String: Int], into vertices: inout [String: Vertex]
    ) -> ([Edge], [String: Int]) {
        var out: [Edge] = []
        var allLayers = layers
        var bend = 0

        for e in edges {
            guard let from = layers[e.from], let to = layers[e.to] else { continue }
            let span = to - from
            guard span > 1 else {
                out.append(e)
                continue
            }
            var previous = e.from
            for step in 1..<span {
                bend += 1
                let id = "·bend·\(bend)"
                vertices[id] = Vertex(width: 0, height: 0, isBend: true)
                allLayers[id] = from + step
                out.append(Edge(from: previous, to: id))
                previous = id
            }
            out.append(Edge(from: previous, to: e.to))
        }
        return (out, allLayers)
    }

    private static func groupIntoColumns(
        layers: [String: Int], order: [String: Int], realCount: Int
    ) -> [[String]] {
        let maxLayer = layers.values.max() ?? 0
        var columns = [[String]](repeating: [], count: maxLayer + 1)
        // Model order inside a column, which is what `considerModelOrder` bought in
        // the ELK configuration: two runs over the same project look the same. Bends
        // sort after the real cards they were created from, by creation order.
        let rank = { (id: String) in order[id] ?? realCount + (Int(id.split(separator: "·").last ?? "0") ?? 0) }
        for (id, layer) in layers.sorted(by: { rank($0.key) < rank($1.key) }) {
            columns[layer].append(id)
        }
        return columns
    }

    // MARK: - 4. Reduce crossings

    /// Median heuristic, swept both directions.
    ///
    /// Each vertex wants to sit at the median position of its neighbours in the
    /// column it is being compared against. Vertices with no neighbours there keep
    /// their place — moving them achieves nothing and only churns the layout.
    private static func reduceCrossings(_ columns: inout [[String]], edges: [Edge]) {
        var successors: [String: [String]] = [:]
        var predecessors: [String: [String]] = [:]
        for e in edges {
            successors[e.from, default: []].append(e.to)
            predecessors[e.to, default: []].append(e.from)
        }

        func sweep(_ index: Int, against neighbours: [String: [String]], reference: [String]) {
            var rank: [String: Int] = [:]
            for (i, id) in reference.enumerated() { rank[id] = i }

            let current = columns[index]
            var position: [String: Double] = [:]
            for (i, id) in current.enumerated() {
                let ranks = (neighbours[id] ?? []).compactMap { rank[$0] }.sorted()
                position[id] = ranks.isEmpty ? Double(i) : median(ranks)
            }
            // Stable on ties, so a vertex with no opinion keeps its neighbours' company.
            columns[index] = current.enumerated()
                .sorted { a, b in
                    let pa = position[a.element]!, pb = position[b.element]!
                    return pa != pb ? pa < pb : a.offset < b.offset
                }
                .map(\.element)
        }

        for _ in 0..<sweeps {
            for i in 1..<max(columns.count, 1) {
                sweep(i, against: predecessors, reference: columns[i - 1])
            }
            for i in stride(from: columns.count - 2, through: 0, by: -1) {
                sweep(i, against: successors, reference: columns[i + 1])
            }
        }
    }

    private static func median(_ sorted: [Int]) -> Double {
        let n = sorted.count
        if n % 2 == 1 { return Double(sorted[n / 2]) }
        return (Double(sorted[n / 2 - 1]) + Double(sorted[n / 2])) / 2
    }

    // MARK: - 5. Place

    /// Columns get an x from the widest card in them; positions within a column come
    /// from Brandes–Köpf, which places blocks of aligned vertices in one pass.
    private static func place(
        columns: [[String]], vertices: [String: Vertex], edges: [Edge], spacing: Spacing
    ) -> [Placement] {
        func size(_ id: String) -> Vertex {
            vertices[id] ?? Vertex(width: 0, height: 0, isBend: true)
        }

        var index: [String: Int] = [:]
        var ids: [String] = []
        for column in columns {
            for id in column where index[id] == nil {
                index[id] = ids.count
                ids.append(id)
            }
        }
        guard !ids.isEmpty else { return [] }

        let heights = ids.map { size($0).height }
        let bends = ids.map { size($0).isBend }

        var positionInLayer = [Int](repeating: 0, count: ids.count)
        for column in columns {
            for (j, id) in column.enumerated() { positionInLayer[index[id]!] = j }
        }

        var predecessors = [[Int]](repeating: [], count: ids.count)
        var successors = [[Int]](repeating: [], count: ids.count)
        for e in edges {
            guard let a = index[e.from], let b = index[e.to] else { continue }
            successors[a].append(b)
            predecessors[b].append(a)
        }
        for i in ids.indices {
            predecessors[i].sort { positionInLayer[$0] < positionInLayer[$1] }
            successors[i].sort { positionInLayer[$0] < positionInLayer[$1] }
        }

        /// A bend is a line passing through, not a card, so it needs far less room on
        /// either side than two cards need between them.
        let gap: (Int, Int) -> CGFloat = { a, b in
            (bends[a] || bends[b]) ? spacing.nodeToBend : spacing.nodeToNode
        }

        let centres = BrandesKoepf.centres(BrandesKoepf.Input(
            layers: columns.map { $0.map { index[$0]! } },
            height: heights, isBend: bends,
            predecessors: predecessors, successors: successors, gap: gap))

        var columnX: [CGFloat] = []
        var x = spacing.padding
        for column in columns {
            columnX.append(x)
            let widest = column.map { size($0).width }.max() ?? 0
            x += widest + spacing.betweenLayers
        }

        let top = zip(centres, heights).map { $0 - $1 / 2 }.min() ?? 0
        let shift = spacing.padding - top

        var out: [Placement] = []
        for (i, column) in columns.enumerated() {
            for id in column {
                let v = index[id]!
                out.append(Placement(
                    id: id, posX: columnX[i].rounded(),
                    posY: (centres[v] - heights[v] / 2 + shift).rounded()))
            }
        }
        return out
    }
}

// MARK: - Driving it from a project

extension LayeredLayout {
    /// Lay out only what is currently shown, so filtering then arranging works.
    static func forProject(
        _ detail: ProjectDetail, visible: Set<String>? = nil, nesting: Nesting = Nesting(),
        settings: LayoutSettings = .standard
    ) -> [Placement] {
        var byId: [String: SchemaNode] = [:]
        for s in detail.schemas { byId[s.id] = s }
        let lookup: SchemaLookup = { byId[$0] }

        let shown = detail.schemas.filter { visible?.contains($0.id) ?? true }
        let shownIds = Set(shown.map(\.id))

        // A card with a size or a width of its own, or drawn as a document, takes that room.
        let nodes = shown.map { schema in
            let form = cardForm(schema, settings: settings, cardWidth: CardGeometry.cardWidth)
            let size = form.size
            return Node(id: schema.id, width: size?.width ?? form.width ?? CardGeometry.cardWidth,
                        height: size?.height ?? cardHeight(schema, lookup: lookup, inlineReuse: nesting.nests(schema.id)))
        }
        // Every line of the project between two cards shown — a flow's nodes' as the flow's, so
        // a flow sits above what it reads and writes instead of floating wherever the
        // layering left room.
        let held = detail.heldIn()
        let edges = detail.relations
            .map { (from: held[$0.fromSchemaId] ?? $0.fromSchemaId, to: held[$0.toSchemaId] ?? $0.toSchemaId) }
            .filter { shownIds.contains($0.from) && shownIds.contains($0.to) && $0.from != $0.to }
            .map { Edge(from: $0.from, to: $0.to) }

        return run(nodes: nodes, edges: edges)
    }
}
