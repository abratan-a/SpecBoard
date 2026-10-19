import CoreGraphics
import Foundation

/// Coordinate assignment for a layered graph — Brandes & Köpf, "Fast and Simple
/// Horizontal Coordinate Assignment" (2002). This is the pass ELK uses.
///
/// An iterative pass — averaging each vertex toward its neighbours and pulling the
/// rest of its column along — looks reasonable and diverges: each sweep can spread
/// a column further than the last, with nothing pulling back. Measured on the real
/// project, four such sweeps turned a column needing 528pt into one occupying 81156pt —
/// a single gap that was, by itself, the whole diagram's height.
///
/// This does not iterate. Vertices are aligned into blocks, blocks are placed once,
/// and four runs (aligning up/down × packing toward either side) are averaged, which
/// is what keeps long edges straight without letting anything run away.
///
/// Coordinates here are positions *within* a layer. The diagram is drawn left to
/// right, so that is y — the paper's x.
enum BrandesKoepf {

    struct Input {
        /// Vertex indices per layer, in the order crossing reduction settled on.
        var layers: [[Int]]
        var height: [CGFloat]
        /// A bend standing in for one layer's worth of a long edge.
        var isBend: [Bool]
        /// Neighbours in the previous layer, ordered by their position there.
        var predecessors: [[Int]]
        /// Neighbours in the next layer, likewise.
        var successors: [[Int]]
        /// Minimum space between the *edges* of two vertices sharing a layer.
        var gap: (Int, Int) -> CGFloat
    }

    /// Centre position of every vertex within its layer.
    static func centres(_ input: Input) -> [CGFloat] {
        let count = input.height.count
        guard count > 0, !input.layers.isEmpty else { return [] }

        let conflicts = markConflicts(input)

        // Four candidate layouts: align toward the upper or lower neighbour, and pack
        // toward the near or far side. Averaging them is what removes the bias each
        // one has on its own.
        var candidates: [[CGFloat]] = []
        for reverseLayers in [false, true] {
            for reverseOrder in [false, true] {
                candidates.append(runOne(input, conflicts: conflicts,
                                         reverseLayers: reverseLayers,
                                         reverseOrder: reverseOrder))
            }
        }

        // Align every candidate to the narrowest one, then take the average. The
        // paper takes the median of four; with non-uniform heights the average is
        // steadier and the difference is not visible at this size.
        let widths = candidates.map { ($0.max() ?? 0) - ($0.min() ?? 0) }
        let narrowest = widths.firstIndex(of: widths.min() ?? 0) ?? 0
        let target = candidates[narrowest].min() ?? 0
        for i in candidates.indices {
            let shift = target - (candidates[i].min() ?? 0)
            for v in 0..<count { candidates[i][v] += shift }
        }

        return (0..<count).map { v in
            candidates.reduce(0) { $0 + $1[v] } / CGFloat(candidates.count)
        }
    }

    // MARK: - Type-1 conflicts

    /// A type-1 conflict is a segment between two bends crossed by one that has a
    /// real card at an end. The bend-to-bend segment wins: it is the middle of a long
    /// edge, and bending it is what makes a diagram look scattered.
    private static func markConflicts(_ input: Input) -> Set<[Int]> {
        var marked = Set<[Int]>()
        let layers = input.layers
        guard layers.count >= 4 else { return marked }

        var position: [Int: Int] = [:]
        for layer in layers {
            for (i, v) in layer.enumerated() { position[v] = i }
        }

        /// The bend feeding this bend from the previous layer, if both are bends.
        func innerNeighbour(_ v: Int) -> Int? {
            guard input.isBend[v] else { return nil }
            return input.predecessors[v].first { input.isBend[$0] }
        }

        for i in 1..<(layers.count - 1) {
            let lower = layers[i + 1]
            var k0 = 0
            var l = 0
            for l1 in lower.indices {
                let v = lower[l1]
                let inner = innerNeighbour(v)
                if l1 == lower.count - 1 || inner != nil {
                    let k1 = inner.flatMap { position[$0] } ?? (layers[i].count - 1)
                    while l <= l1 {
                        for u in input.predecessors[lower[l]] {
                            guard let pu = position[u] else { continue }
                            if pu < k0 || pu > k1 { marked.insert([u, lower[l]]) }
                        }
                        l += 1
                    }
                    k0 = k1
                }
            }
        }
        return marked
    }

    // MARK: - One of the four

    private static func runOne(
        _ input: Input, conflicts: Set<[Int]>,
        reverseLayers: Bool, reverseOrder: Bool
    ) -> [CGFloat] {
        let count = input.height.count

        // The four variants are one algorithm applied to a transformed graph:
        // reversing the layer order swaps which side we align toward, reversing each
        // layer's contents swaps which side we pack toward.
        var layers = input.layers
        if reverseOrder { layers = layers.map { $0.reversed() } }
        if reverseLayers { layers.reverse() }

        var upper = reverseLayers ? input.successors : input.predecessors
        if reverseOrder { upper = upper.map { $0.reversed() } }

        var position = [Int](repeating: 0, count: count)
        var layerOf = [Int](repeating: 0, count: count)
        for (i, layer) in layers.enumerated() {
            for (j, v) in layer.enumerated() { position[v] = j; layerOf[v] = i }
        }

        // Vertical alignment: every vertex tries to join its median upper neighbour,
        // forming blocks that will be placed as a unit.
        var root = Array(0..<count)
        var align = Array(0..<count)

        for layer in layers.dropFirst() {
            var r = -1
            for v in layer {
                let neighbours = upper[v].filter { layerOf[$0] == layerOf[v] - 1 }
                    .sorted { position[$0] < position[$1] }
                guard !neighbours.isEmpty else { continue }

                let mid = (neighbours.count - 1) / 2
                for m in (neighbours.count % 2 == 1 ? [mid] : [mid, mid + 1]) {
                    guard align[v] == v else { break }
                    let u = neighbours[m]
                    let edge = reverseLayers ? [v, u] : [u, v]
                    guard !conflicts.contains(edge), r < position[u] else { continue }
                    align[u] = v
                    root[v] = root[u]
                    align[v] = root[v]
                    r = position[u]
                }
            }
        }

        // Horizontal compaction: place each block once, then pull classes together.
        var sink = Array(0..<count)
        var shift = [CGFloat](repeating: .greatestFiniteMagnitude, count: count)
        var coordinate = [CGFloat?](repeating: nil, count: count)

        /// Minimum distance between the centres of two vertices side by side.
        func separation(_ a: Int, _ b: Int) -> CGFloat {
            (input.height[a] + input.height[b]) / 2 + input.gap(a, b)
        }

        func placeBlock(_ v: Int) {
            guard coordinate[v] == nil else { return }
            coordinate[v] = 0
            var w = v
            repeat {
                if position[w] > 0 {
                    let predecessorInLayer = layers[layerOf[w]][position[w] - 1]
                    let u = root[predecessorInLayer]
                    placeBlock(u)
                    if sink[v] == v { sink[v] = sink[u] }
                    let needed = separation(predecessorInLayer, w)
                    if sink[v] != sink[u] {
                        shift[sink[u]] = min(shift[sink[u]],
                                             coordinate[v]! - coordinate[u]! - needed)
                    } else {
                        coordinate[v] = max(coordinate[v]!, coordinate[u]! + needed)
                    }
                }
                w = align[w]
            } while w != v
        }

        for v in 0..<count where root[v] == v { placeBlock(v) }

        var out = [CGFloat](repeating: 0, count: count)
        for v in 0..<count {
            out[v] = coordinate[root[v]] ?? 0
            let s = shift[sink[root[v]]]
            if s < .greatestFiniteMagnitude { out[v] += s }
        }

        // Undo the transform: packing toward the far side means the axis was flipped.
        if reverseOrder { for v in 0..<count { out[v] = -out[v] } }
        return out
    }
}
