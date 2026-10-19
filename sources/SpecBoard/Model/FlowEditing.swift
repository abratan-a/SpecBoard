import Foundation

/// What the flow canvas does to a flow's nodes (`specboard://SpecBoard/Docs/Design:Overview` §11, §12), worked
/// out apart from the view: moving one to a cell, adding one, taking one away,
/// renaming one. Each returns the nodes to store; the store keeps them though
/// unfinished (`Validate.nodes(complete: false)`), and says what is left to do.
nonisolated enum FlowEditing {

    /// Put at `point` on the grid — or, when that is too close to another node, the
    /// nearest place that keeps the margin clear (`LayoutSettings.clearSpot`).
    static func move(_ name: String, to point: GridPoint, in nodes: [FlowNode], margin: Int = 1) -> [FlowNode] {
        guard let i = nodes.firstIndex(where: { $0.name == name }) else { return nodes }
        var settings = LayoutSettings.standard
        settings.margin = margin
        let others = nodes.filter { $0.name != name }.compactMap { node in node.topLeft.map { FlowLayout.rect(node, at: $0) } }
        let spot = settings.clearSpot(near: point, height: FlowLayout.rows(nodes[i]), avoiding: others)
        guard nodes[i].topLeft != spot else { return nodes }
        var out = nodes
        out[i].topLeft = spot
        return out
    }

    /// A node of `kind`, named after it (`run2`), right of `after` — or in the first
    /// free place — and, when `after` goes nowhere yet, where it goes. Returns the
    /// nodes and the new one's name.
    static func add(_ kind: String, after: String?, to nodes: [FlowNode], margin: Int = 1) -> (nodes: [FlowNode], name: String) {
        var used = Set(nodes.map(\.name))
        var name = kind, n = 2
        while used.contains(name) { name = "\(kind)\(n)"; n += 1 }
        used.insert(name)
        var node = FlowNode(name: name, kind: kind)
        switch kind {
        case "begin": node.trigger = "manual"
        case "wait": node.waitFor = "approval"
        case "join": node.join = "all"
        case "end": node.outcome = "success"
        default: break
        }
        var out = nodes
        var settings = LayoutSettings.standard
        settings.margin = margin
        let taken = nodes.compactMap { other in other.topLeft.map { FlowLayout.rect(other, at: $0) } }
        let from = after.flatMap { name in nodes.first { $0.name == name } }
        if let place = from?.topLeft {
            let wanted = GridPoint(x: place.x + Grid.cardColumns + 2 * margin, y: place.y)
            node.topLeft = settings.clearSpot(near: wanted, height: FlowLayout.rows(node), avoiding: taken)
        } else {
            node.topLeft = settings.nextSpot(avoiding: taken, columns: max(4, nodes.count + 1))
        }
        if let from, let i = out.firstIndex(where: { $0.name == from.name }), out[i].nexts.isEmpty,
           FlowNode.Vocabulary.keys[out[i].kind]?.contains("next") == true, kind != "begin" {
            out[i].nexts = [name]
        }
        out.append(node)
        return (out, name)
    }

    /// Taken away, and every way to it with it — the nodes it came from are left
    /// unfinished, which the canvas shows, rather than refusing.
    static func remove(_ name: String, from nodes: [FlowNode]) -> [FlowNode] {
        nodes.filter { $0.name != name }.map { node in
            var node = node
            node.nexts.removeAll { $0 == name }
            if node.otherwise == name { node.otherwise = nil }
            if node.onError == name { node.onError = nil }
            if node.onTimeout == name { node.onTimeout = nil }
            node.branches.removeAll { $0.next == name }
            return node
        }
    }

    /// Named anew, every way to it following.
    static func rename(_ old: String, to raw: String, in nodes: [FlowNode]) throws -> [FlowNode] {
        let language = Language.current
        let name = try Validate.name(raw, field: L10n.text(.Flow.fieldNodeName, in: language))
        guard name != old else { return nodes }
        guard !nodes.contains(where: { $0.name != old && PathText.same($0.name, name) }) else {
            throw Invalid(L10n.text(.Flow.problemNodeExists(name), in: language))
        }
        func follow(_ way: String?) -> String? { way == old ? name : way }
        return nodes.map { node in
            var node = node
            if node.name == old { node.name = name }
            node.nexts = node.nexts.map { $0 == old ? name : $0 }
            node.otherwise = follow(node.otherwise)
            node.onError = follow(node.onError)
            node.onTimeout = follow(node.onTimeout)
            node.branches = node.branches.map { FlowBranch(when: $0.when, next: $0.next == old ? name : $0.next) }
            // What the lines to it say follows it too.
            node.lineExtras = Dictionary(node.lineExtras.map { key, extra in
                (Line.flowWays.contains(key.line) && key.target == old
                    ? LineKey(line: key.line, target: name, mark: key.mark) : key, extra)
            }, uniquingKeysWith: { first, _ in first })
            return node
        }
    }

    /// The node named so, replaced.
    static func replace(_ node: FlowNode, in nodes: [FlowNode]) -> [FlowNode] {
        nodes.map { $0.name == node.name ? node : $0 }
    }

    /// What is not finished yet in `nodes` and is about this node.
    static func unfinished(_ node: FlowNode, in nodes: [FlowNode]) -> [String] {
        FlowNode.check(nodes).unfinished.filter { $0.node == node.name }.map(\.text)
    }
}
