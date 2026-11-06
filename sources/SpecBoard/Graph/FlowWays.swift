import Foundation

/// Where a flow's card says each node goes: to the node below it, a ↓ above that node between their kind
/// words — with which way it is, on a node that has more than one — and anywhere else on the card, a dashed
/// arrow in a narrow strip on the card's left, one lane each where they would cross. Read down the card,
/// the way on is seen at once, and where it jumps is drawn to the row it lands on rather than written at
/// the row's end, where a second way was cut off.
struct FlowWays: Equatable {
    /// A way to the node below: the row index of that node, the ↓ drawn above it.
    struct Down: Equatable {
        let above: Int
        let word: String?
    }

    /// A way to a node other than the one below, by the row indexes it leaves and lands on.
    struct Jump: Equatable {
        let from: Int
        let to: Int
        let way: FlowNode.Exit
        let word: String?
        var lane = 0
    }

    var downs: [Down] = []
    var jumps: [Jump] = []

    /// The room a lane of the strip takes: its way's word stands on it, turned along it.
    static let laneWidth: CGFloat = 12

    /// How wide the strip on the card's left is: none on a card that jumps nowhere, which keeps its width.
    var gutter: CGFloat {
        guard let widest = jumps.map(\.lane).max() else { return 0 }
        return CGFloat(widest + 1) * Self.laneWidth + 4
    }

    /// The ways of `flow`'s nodes, as the rows of its card (`cardRows`, all of them) place them.
    static func of(_ flow: SchemaNode, rows: [CardRow]) -> FlowWays {
        guard !flow.nodes.isEmpty else { return FlowWays() }
        var index: [String: Int] = [:]
        for (at, row) in rows.enumerated() where index[row.path] == nil { index[row.path] = at }
        var out = FlowWays()
        for (n, node) in flow.nodes.enumerated() {
            guard let from = index[SchemaNode.nodeRow(node.name)] else { continue }
            let below = n + 1 < flow.nodes.count ? flow.nodes[n + 1].name : nil
            var downDone = false
            // A way said once: two branches to one node are one arrow, a next and an error to it are two.
            var seen: Set<String> = []
            for exit in node.exits {
                guard let to = index[SchemaNode.nodeRow(exit.to)], seen.insert("\(exit.kind)|\(exit.to)").inserted
                else { continue }
                if exit.to == below, !downDone {
                    downDone = true
                    out.downs.append(Down(above: to, word: downWord(node, exit.kind, exit.label)))
                } else {
                    out.jumps.append(Jump(from: from, to: to, way: exit.kind,
                                          word: exit.kind == .branch ? L10n[.Diagram.flowWayMatched] : exit.label))
                }
            }
        }
        out.laneJumps()
        return out
    }

    /// The names of `flow`'s nodes its card draws the ways to: what its rows leave unwritten.
    static func drawn(_ flow: SchemaNode) -> Set<String> { Set(flow.nodes.map(\.name)) }

    /// Said beside the ↓ only where the node has another way: which of them this is.
    private static func downWord(_ node: FlowNode, _ way: FlowNode.Exit, _ label: String?) -> String? {
        guard node.exits.count > 1 else { return nil }
        switch way {
        case .branch: return L10n[.Diagram.flowWayMatched]
        case .next: return node.onTimeout != nil ? L10n[.Diagram.flowWayInTime] : nil
        case .otherwise, .error, .timeout: return label
        }
    }

    /// Each jump in the lane nearest the rows that no jump over the same rows has: the short ones nearest.
    private mutating func laneJumps() {
        var taken: [(lane: Int, span: ClosedRange<Int>)] = []
        let order = jumps.indices.sorted {
            let (a, b) = (jumps[$0], jumps[$1])
            return (abs(a.to - a.from), $0) < (abs(b.to - b.from), $1)
        }
        for at in order {
            let span = min(jumps[at].from, jumps[at].to)...max(jumps[at].from, jumps[at].to)
            var lane = 0
            while taken.contains(where: { $0.lane == lane && $0.span.overlaps(span) }) { lane += 1 }
            jumps[at].lane = lane
            taken.append((lane, span))
        }
    }
}
