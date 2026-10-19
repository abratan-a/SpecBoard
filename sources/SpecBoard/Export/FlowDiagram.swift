import CryptoKit
import Foundation

/// A flow drawn as a flowchart (`specboard://SpecBoard/Docs/Design:Overview` §11): each node in the shape of
/// what it is, each way labelled as written — left to right, the way it goes.
///
///     開始・終了 ([…])   処理・実行 […]   判断 {…}   待つ {{…}}   合流 [[…]]
///
/// A process node's box says its rows, five at most and how many more.
///
/// The first line says what it was drawn from, with a rev that changes exactly when
/// the picture would.
nonisolated enum FlowDiagram {

    /// `exported`: as it is written out — the app's paths left out, the head only the
    /// mark and the rev (`specboard://SpecBoard/Docs/Design:DesignSections` §7). The rev is the same either way:
    /// it is of the picture as the app draws it. `language`: the words the chart says for itself in,
    /// the project's for what is written out and the screen's for what the app draws.
    static func render(_ flow: SchemaNode, in detail: ProjectDetail, exported: Bool = false,
                       language: String) -> String {
        let drawn = body(flow, in: detail, language: language)
        guard exported else {
            return "%% specboard project=\(detail.project.slug) flow=\(Ident.lineComment(detail.directory.exported(flow))) "
                + "shape=flowchart rev=\(revision(drawn))\n" + drawn.joined(separator: "\n") + "\n"
        }
        return "%% specboard rev=\(revision(drawn))\n"
            + body(flow, in: detail, exported: true, language: language).joined(separator: "\n") + "\n"
    }

    /// What the chart is drawn from, as the rev on its first line: it changes exactly when
    /// the picture would.
    private static func revision(_ body: [String]) -> String {
        String(SHA256.hash(data: Data(body.joined(separator: "\n").utf8))
            .map { String(format: "%02x", $0) }.joined().prefix(12))
    }

    private static func body(_ flow: SchemaNode, in detail: ProjectDetail, exported: Bool = false,
                             language: String) -> [String] {
        let path: (String) -> String = { exported ? SchemaDirectory.plain(path: $0) : SchemaDirectory.exported(path: $0) }
        var lines = ["flowchart LR"]
        guard !flow.nodes.isEmpty else {
            lines.append("    empty([\"\(L10n.text(.Charts.flowNoNodes, in: language))\"])")
            return lines
        }
        let ids = Self.ids(flow.nodes)
        func id(_ name: String) -> String { ids[name] ?? Self.id(name) }
        for node in flow.nodes {
            // Lines of a box are the title's, each made safe and broken with <br/> —
            // under the node's heading when it has one.
            let heading = node.label.flatMap { $0.isEmpty ? nil : [$0] } ?? []
            let text = (heading + title(node, path: path, language: language).components(separatedBy: "\n")).map(safe).joined(separator: "<br/>")
            let shape: String
            switch node.kind {
            case "begin", "end": shape = "([\"\(text)\"])"
            case "eval": shape = "{\"\(text)\"}"
            case "wait": shape = "{{\"\(text)\"}}"
            case "join": shape = "[[\"\(text)\"]]"
            default: shape = "[\"\(text)\"]"
            }
            lines.append("    \(id(node.name))\(shape)")
        }
        for node in flow.nodes {
            for exit in node.exits(in: language) {
                let arrow = exit.kind == .error || exit.kind == .timeout ? "-.->" : "-->"
                let label = exit.label.map { "|\"\(safe(plain($0, path: path)))\"|" } ?? ""
                lines.append("    \(id(node.name)) \(arrow)\(label) \(id(exit.to))")
            }
        }
        return lines
    }

    /// What a node says on the chart: what it is, and what it runs, decides or waits for.
    static func title(_ node: FlowNode, path: (String) -> String = SchemaDirectory.exported(path:),
                      language: String) -> String {
        func say(_ resource: LocalizedStringResource) -> String { L10n.text(resource, in: language) }
        func caller() -> String { ActionDiagram.caller(in: language) }
        let head = FlowNode.Vocabulary.label(node.kind, in: language)
        let eventFlow = node.event.map { path($0.flow) } ?? ""
        func event() -> String { say(.Charts.titleEvent(eventFlow, node.event?.outcome ?? "")) }
        switch node.kind {
        case "begin":
            switch node.trigger {
            case "event": return "\(head): \(event())"
            case "schedule": return "\(head): \(node.every ?? say(.Charts.titlePeriodic))"
            case "receive":
                let from = FlowNode.partners(node.peers, node.program, path: path, in: language) ?? caller()
                guard !node.carries.isEmpty else { return say(.Charts.titleReceive(head, from)) }
                return say(.Charts.titleReceiveCarrying(head, from, node.carries.map(path).joined(separator: ", ")))
            case "startup": return say(.Charts.titleStartup(head))
            default: return say(.Charts.titleManual(head))
            }
        case "process":
            // Its rows, short: what each does and where.
            let shown = node.steps.prefix(rowsShown).map { step -> String in
                let other = FlowNode.partners(step.peers, step.program, path: path, in: language)
                switch step.op {
                case "read", "write": return "\(step.op) \(step.paths.map(path).joined(separator: ", "))"
                case "sign", "verify": return "\(step.op) \(step.key.map(path) ?? "")"
                // A row that passes nothing to anyone says no one it goes to.
                case "compute": return step.op
                default: return "\(step.op) \(other ?? caller())"
                    + (step.carries.isEmpty ? "" : ": \(step.carries.map(path).joined(separator: ", "))")
                }
            }
            let more = node.steps.count > rowsShown ? [say(.Charts.titleMoreRows(String(node.steps.count - rowsShown)))] : []
            return ([head] + shown + more).joined(separator: "\n")
        case "run":
            if let target = node.target { return "\(head): \(path(target))" }
            return say(.Charts.titleRunParallel(head, node.join ?? "all", node.targets.map(path).joined(separator: " ∥ ")))
        case "eval": return "\(head)" + (node.value.map { ": \(path($0))" } ?? "")
        case "wait":
            let what = node.waitFor == "event" ? event() : (node.waitFor ?? "")
            guard let timeout = node.timeout else { return "\(head): \(what)" }
            return say(.Charts.titleWaitUntil(head, what, timeout))
        case "join": return say(.Charts.titleJoin(head, node.join ?? "all"))
        default:
            let outcome = node.outcome ?? ""
            guard let reply = node.reply else { return "\(head): \(outcome)" }
            let carries = reply.carries.isEmpty ? "" : ": \(reply.carries.map(path).joined(separator: ", "))"
            let to = FlowNode.partners(reply.peers, reply.program, path: path, in: language) ?? caller()
            return say(.Charts.titleEndReply(head, outcome, to, carries))
        }
    }

    /// How many of a process node's rows its box says before counting the rest.
    static let rowsShown = 5

    private static func id(_ name: String) -> String { "n_" + Ident.safe(name) }

    /// Each node's id in the chart, by its name, as the core writes them: told apart with a
    /// number where two names come to one (`a b` and `a_b`, two names of letters of other scripts).
    private static func ids(_ nodes: [FlowNode]) -> [String: String] {
        var taken: Set<String> = []
        var out: [String: String] = [:]
        for node in nodes {
            let base = id(node.name)
            var made = base
            var n = 2
            while !taken.insert(made).inserted {
                made = "\(base)_\(n)"
                n += 1
            }
            out[node.name] = made
        }
        return out
    }

    private static func plain(_ text: String, path: (String) -> String) -> String {
        NoteText.transform(text) { $0.shown ?? path($0.path) }
    }

    private static func safe(_ text: String) -> String { Ident.mermaidText(text) }
}
