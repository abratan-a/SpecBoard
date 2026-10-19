import Foundation

/// A flow's nodes as a graph (`specboard://SpecBoard/Docs/Design:Overview` §11): what its ways reach, what it
/// runs, and which flows run a card. The ways are the ones written — nothing is
/// guessed from what the actions' rows send and read.
nonisolated enum FlowGraph {

    /// Nodes no way reaches from the start: written, and never run.
    static func unreachable(_ flow: SchemaNode) -> [String] {
        guard let begin = flow.nodes.first(where: { $0.kind == "begin" }) else { return [] }
        let byName = Dictionary(uniqueKeysWithValues: flow.nodes.map { ($0.name, $0) })
        var seen: Set<String> = []
        var queue = [begin.name]
        while let name = queue.popLast() {
            guard seen.insert(name).inserted, let node = byName[name] else { continue }
            queue += node.exits.map(\.to)
        }
        return flow.nodes.map(\.name).filter { !seen.contains($0) }
    }

    /// What each `run` runs, found — in the order the nodes are written.
    static func runs(_ flow: SchemaNode, in detail: ProjectDetail) -> [(node: FlowNode, targets: [SchemaNode])] {
        let directory = detail.directory
        let home = directory.sectionId(of: flow)
        return flow.nodes.filter { $0.kind == "run" }.map { node in
            let paths = [node.target].compactMap { $0 } + node.targets
            return (node, paths.compactMap { directory.find($0, from: home, seeing: true).schema })
        }
    }

    /// The flows that run `card` — an action or a flow — in one of their nodes.
    static func runners(of card: SchemaNode, in detail: ProjectDetail) -> [SchemaNode] {
        detail.schemas.filter { $0.kind == .flow }.filter { flow in
            runs(flow, in: detail).contains { $0.targets.contains { $0.id == card.id } }
        }
    }

    /// What a writer of `flow` is told: what is not finished yet, and nodes that
    /// nothing leads to.
    static func warnings(_ flow: SchemaNode, in directory: SchemaDirectory) -> [String] {
        let name = directory.full(flow)
        let language = Language.current
        var out = FlowNode.problems(flow.nodes).unfinished.map { L10n.text(.Flow.warningUnfinished(name, $0), in: language) }
        let lost = unreachable(flow)
        if !lost.isEmpty {
            let nodes = lost.joined(separator: L10n.text(.Common.separatorList, in: language))
            out.append(L10n.text(.Flow.warningUnreachable(name, nodes), in: language))
        }
        return out
    }
}
