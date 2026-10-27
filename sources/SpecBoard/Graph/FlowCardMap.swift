import Foundation

/// A flow's nodes drawn the way the relation panel draws cards (`specboard://SpecBoard/Docs/Design:Overview`
/// §11, §13): a card a node — the cards inside the flow, what it is in the header, its
/// name, and a row for what it runs, decides or waits for and for each way out — placed on
/// the grid where the flow's card map keeps it, and the flow's lines between them (the
/// core's `relations` of the node cards: `next`, `branch`, `else`, `onError`, `onTimeout`)
/// leaving from the rows that say where they go.
///
/// The cards are made for the flow's card editor and stand for nothing in the project:
/// their ids carry the map's prefix (`CardMapIds`), and `detail` is a project of these cards
/// alone, so the names the rows point at are the nodes'.
enum FlowCardMap {
    static let ids = CardMapIds(prefix: "⇢")

    static func isCard(_ id: String) -> Bool { ids.isCard(id) }
    static func id(_ node: String) -> String { ids.id(node) }
    static func node(of id: String) -> String { ids.inside(of: id) }

    /// The flow's nodes as a project of their own, to be drawn, and the flow's lines between
    /// them, each from the row of its node's card that says where it goes.
    static func detail(_ flow: SchemaNode, in project: ProjectDetail) -> ProjectDetail {
        let cards = flow.nodes.enumerated().map { index, node -> SchemaNode in
            var card = SchemaNode(id: id(node.name), projectId: flow.projectId, name: node.name, label: node.label ?? "",
                                  kind: .flow, description: node.description ?? "", tags: [], metadata: [:],
                                  rules: [], absentFields: [], discriminator: nil, enumOpen: false,
                                  posX: node.topLeft.map { Double($0.x) }, posY: node.topLeft.map { Double($0.y) },
                                  pinned: true, sortOrder: index, fields: [], values: [])
            card.nodes = [node]
            return card
        }
        var out = ProjectDetail(project: project.project, schemas: cards, relations: lines(of: flow, in: project))
        out.lineKinds = project.lineKinds
        return out
    }

    /// The flow's lines between its nodes (the core's `relations` of its node cards), as
    /// the map draws them: from the node's card, at the row of that way out, to the card of
    /// the node it goes to.
    static func lines(of flow: SchemaNode, in project: ProjectDetail) -> [Relation] {
        let nodes = project.inner.filter { $0.container == flow.id }
        let names = Dictionary(nodes.map { ($0.id, $0.name) }, uniquingKeysWith: { a, _ in a })
        let byId = Dictionary(nodes.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        return project.relations.compactMap { relation -> Relation? in
            guard let from = names[relation.fromSchemaId], let to = names[relation.toSchemaId],
                  let node = byId[relation.fromSchemaId], let row = row(of: relation, in: node, names: names)
            else { return nil }
            var drawn = relation
            drawn.fromSchemaId = id(from)
            drawn.toSchemaId = id(to)
            drawn.from = row
            drawn.at = nil
            return drawn
        }
    }

    /// The row of a node's card a way out of it is drawn from (`rows`): the k-th `next` or
    /// branch of the node, its `else`, its failure's, its time-out's.
    static func row(of relation: Relation, in node: InnerCard, names: [String: String]) -> String? {
        let ways = node.connections.filter { $0.line == relation.line && $0.from.isEmpty && $0.to.map { names[$0] != nil } == true }
        let index = ways.firstIndex { $0.id == relation.connection } ?? 0
        switch relation.line {
        case Line.next: return "#next.\(index)"
        case Line.condition: return "#b.\(index)"
        case Line.otherwise: return "#else"
        case Line.onError: return "#err"
        case Line.onTimeout: return "#timeout"
        default: return nil
        }
    }

    /// What its card shows, as many as `FlowNode.rowCount` says: what it is, then
    /// each way out, the line leaving from that row.
    static func rows(_ node: FlowNode) -> [CardRow] {
        var out: [CardRow] = []
        func row(_ path: String, _ label: String, _ text: String, _ detail: String? = nil) {
            out.append(CardRow(path: path, depth: 0, field: nil, value: nil, fromType: nil,
                               step: StepRow(label: label, text: text, detail: detail)))
        }
        func ways() {
            if node.nexts.isEmpty { row("#next", BuiltinWords.line(Line.next), "—") }
            for (index, next) in node.nexts.enumerated() {
                row("#next.\(index)", BuiltinWords.line(Line.next), "→ \(next)", node.nexts.count > 1 ? L10n[.Diagram.flowBranches] : nil)
            }
        }
        switch node.kind {
        case "begin":
            let what: String
            switch node.trigger {
            case "event": what = L10n[.Diagram.flowEventOutcome(node.event?.outcome ?? "", node.event?.flow ?? "")]
            case "schedule": what = node.every ?? "schedule"
            default: what = node.trigger ?? "—"
            }
            row("#k", "trigger", what, node.trigger == "event" ? "event" : nil)
            if let received = node.receiveRow { row("#recv", "receive", StepRow(received).text, received.via) }
            ways()
        case "process":
            // Its rows, as an action's card had them.
            if node.steps.isEmpty { row("#s", "steps", "—") }
            for (index, step) in node.steps.enumerated() {
                let shown = StepRow(step)
                row("#s.\(index)", shown.label, shown.text, shown.detail)
            }
            ways()
        case "run":
            row("#k", "target", node.target ?? "—")
            if !node.carries.isEmpty || !node.returns.isEmpty {
                row("#io", "carries", node.carries.joined(separator: ", ")
                    + (node.returns.isEmpty ? "" : " ⇠ \(node.returns.joined(separator: ", "))"))
            }
            ways()
            if let onError = node.onError { row("#err", BuiltinWords.line(Line.onError), "→ \(onError)") }
        case "eval":
            if let value = node.value { row("#k", "value", value) }
            if node.branches.isEmpty { row("#b", "when", "—") }
            for (index, branch) in node.branches.enumerated() {
                row("#b.\(index)", "when", NoteText.transform(branch.when) { $0.shown ?? $0.path }, "→ \(branch.next)")
            }
            row("#else", BuiltinWords.line(Line.otherwise), node.otherwise.map { "→ \($0)" } ?? "—")
        case "wait":
            let what = node.waitFor == "event" ? L10n[.Diagram.flowEventOutcome(node.event?.outcome ?? "", node.event?.flow ?? "")]
                : node.waitFor ?? "—"
            row("#k", "for", what)
            if let timeout = node.timeout {
                row("#timeout", "timeout", timeout, node.onTimeout.map { "→ \($0)" })
            }
            ways()
        case "join":
            row("#k", "join", node.join ?? "—")
            if let partial = node.partial { row("#partial", "partial", partial) }
            ways()
        default:
            row("#k", "outcome", node.outcome ?? "—")
            if let answer = node.returnRow { row("#ret", "return", StepRow(answer).text) }
        }
        return out
    }
}
