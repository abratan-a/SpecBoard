import Foundation

/// A card put only inside a container — a flow's node — as `project_detail` reads it apart
/// from the cards (`inner`, `specboard://SpecBoard/Docs/Design:CoreApi`「読む」): its name and kind, its flow
/// (`in`), its place among the flow's cards and on the flow's card map, its fields and rows
/// as its kind's definition keeps them (`form`), and its lines.
nonisolated struct InnerCard: Codable, Hashable, Sendable {
    var id: String
    var name: String
    var kind: String
    var sectionId: String?
    /// The container it is inside: its flow's id.
    var container: String
    var sortOrder: Int
    var posX: Double?
    var posY: Double?
    var label: String
    var description: String
    /// Its fields, by name, as its document keeps them.
    var fields: [String: JSONValue]
    /// Its rows of each kind (a process node's `steps`): each row's fields.
    var rows: [String: [[String: JSONValue]]]
    var connections: [Connection]

    private enum Keys: String, CodingKey {
        case id, name, kind, sectionId, container = "in", sortOrder, posX, posY, label, description, form, connections
    }

    private struct Form: Decodable {
        var fields: [String: JSONValue]?
        var rows: [String: [Row]]?
    }

    private struct Row: Decodable {
        var fields: [String: JSONValue]?
    }

    init(id: String, name: String, kind: String, container: String, sectionId: String? = nil, sortOrder: Int = 0,
         posX: Double? = nil, posY: Double? = nil, label: String = "", description: String = "",
         fields: [String: JSONValue] = [:], rows: [String: [[String: JSONValue]]] = [:], connections: [Connection] = []) {
        self.id = id
        self.name = name
        self.kind = kind
        self.container = container
        self.sectionId = sectionId
        self.sortOrder = sortOrder
        self.posX = posX
        self.posY = posY
        self.label = label
        self.description = description
        self.fields = fields
        self.rows = rows
        self.connections = connections
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        id = try c.decode(String.self, forKey: .id)
        name = try c.decode(String.self, forKey: .name)
        kind = try c.decode(String.self, forKey: .kind)
        sectionId = try c.decodeIfPresent(String.self, forKey: .sectionId)
        container = try c.decodeIfPresent(String.self, forKey: .container) ?? ""
        sortOrder = try c.decodeIfPresent(Int.self, forKey: .sortOrder) ?? 0
        posX = try c.decodeIfPresent(Double.self, forKey: .posX)
        posY = try c.decodeIfPresent(Double.self, forKey: .posY)
        label = try c.decodeIfPresent(String.self, forKey: .label) ?? ""
        description = try c.decodeIfPresent(String.self, forKey: .description) ?? ""
        let form = try c.decodeIfPresent(Form.self, forKey: .form)
        fields = form?.fields ?? [:]
        rows = (form?.rows ?? [:]).mapValues { $0.map { $0.fields ?? [:] } }
        connections = try c.decodeIfPresent([Connection].self, forKey: .connections) ?? []
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: Keys.self)
        try c.encode(id, forKey: .id)
        try c.encode(name, forKey: .name)
        try c.encode(kind, forKey: .kind)
        try c.encodeIfPresent(sectionId, forKey: .sectionId)
        try c.encode(container, forKey: .container)
        try c.encode(sortOrder, forKey: .sortOrder)
        try c.encodeIfPresent(posX, forKey: .posX)
        try c.encodeIfPresent(posY, forKey: .posY)
        try c.encode(label, forKey: .label)
        try c.encode(description, forKey: .description)
        try c.encode(connections, forKey: .connections)
    }

    /// Where its card sits on its flow's card map, in grid units.
    var topLeft: GridPoint? {
        guard let posX, let posY else { return nil }
        return GridPoint(x: Int(posX.rounded()), y: Int(posY.rounded()))
    }
}

// MARK: - A node card read as the node it is

extension FlowNode {
    /// A node card read as the node it is (`FlowNode`), the way the core reads it for its
    /// checks and its diagrams (`detail::node_before`): its fields and rows as they are, and
    /// where its lines go — the ways to the other nodes of the flow by their names
    /// (`siblings`), every other place by its path as `landing` says it.
    init(card: InnerCard, siblings: [String: String], landing: (Connection) -> String?) {
        self.init(name: card.name, kind: card.kind)
        let f = card.fields
        func text(_ key: String) -> String? { f[key]?.stringValue.flatMap { $0.isEmpty ? nil : $0 } }
        func list(_ value: JSONValue?) -> [String] {
            switch value {
            case .array(let items)?: return items.compactMap(\.stringValue)
            case .string(let one)?: return [one]
            default: return []
            }
        }
        label = card.label.isEmpty ? nil : card.label
        description = card.description.isEmpty ? nil : card.description
        trigger = text("trigger")
        every = text("every")
        program = text("program")
        carries = list(f["carries"])
        returns = list(f["returns"])
        via = text("via")
        initiator = text("initiator")
        if case .bool(true)? = f["anyCaller"] { anyCaller = true }
        value = text("value")
        waitFor = text("for")
        timeout = text("timeout")
        join = text("join")
        partial = text("partial")
        outcome = text("outcome")
        if case .object(let answer)? = f["return"] {
            var reply = FlowReturn()
            reply.carries = list(answer["carries"])
            reply.program = answer["program"]?.stringValue
            self.reply = reply
        }
        // Its head's lines: the ways to the other nodes, what it runs and waits for, whom
        // it hears from or answers, the keys it uses. What a line says beyond what the node
        // holds (its description, other fields) is kept by the line, to be written again with it.
        var extras: [LineKey: LineExtra] = [:]
        func remember(_ line: Connection, _ target: String?) {
            if let target, let extra = line.extra { extras[LineKey(line: line.line, target: target, mark: line.mark)] = extra }
        }
        for line in card.connections where line.from.isEmpty {
            let way = line.to.flatMap { siblings[$0] }
            switch line.line {
            case Line.next: if let way { nexts.append(way); remember(line, way) }
            case Line.condition:
                if let way { branches.append(FlowBranch(when: line.text("when") ?? "", next: way)); remember(line, way) }
            case Line.otherwise: otherwise = way; remember(line, way)
            case Line.onError: onError = way; remember(line, way)
            case Line.onTimeout: onTimeout = way; remember(line, way)
            case Line.runs: target = landing(line); remember(line, target)
            case Line.awaits:
                if let flow = landing(line) {
                    event = FlowEvent(flow: flow, outcome: line.text("outcome") ?? "")
                    remember(line, flow)
                }
            case Line.partner:
                // Every one of them, in the lines' order: a write gives back all it read.
                guard let path = landing(line) else { continue }
                remember(line, path)
                if card.kind == "end" {
                    var reply = self.reply ?? FlowReturn()
                    reply.peers.append(path)
                    self.reply = reply
                } else {
                    peers.append(path)
                }
            case Line.key:
                guard let path = landing(line) else { continue }
                remember(line, path)
                if card.kind == "end" {
                    var reply = self.reply ?? FlowReturn()
                    reply.signWith = path
                    self.reply = reply
                } else if line.text("use") == "sign" {
                    signWith = path
                } else {
                    verifyWith = path
                }
            default: break
            }
        }
        lineExtras = extras
        // Its rows, each with what its lines name.
        steps = (card.rows["steps"] ?? []).enumerated().map { index, row in
            var step = Step(op: row["do"]?.stringValue ?? "")
            step.description = row["description"]?.stringValue
            step.program = row["program"]?.stringValue
            step.carries = list(row["carries"])
            step.returns = list(row["returns"])
            step.paths = list(row["paths"])
            step.via = row["via"]?.stringValue
            step.initiator = row["initiator"]?.stringValue
            let from = "steps[\(index)]"
            for line in card.connections where line.from == from {
                guard let path = landing(line) else { continue }
                var held = false
                switch line.line {
                case Line.reads, Line.writes:
                    // Listed with the row's paths as before, but one that is not the kind the row does
                    // (a `reads` line on a write) stays its own line: written back as it was.
                    step.paths.append(path)
                    if line.line != (step.op == "read" ? Line.reads : step.op == "write" ? Line.writes : "") {
                        step.foreign.append(Step.Foreign(line: line.line, path: path))
                    }
                    held = true
                case Line.partner: step.peers.append(path); held = true
                case Line.key where step.op == "sign" || step.op == "verify": step.key = path; held = true
                case Line.key:
                    if line.text("use") == "sign" { step.signWith = path } else { step.verifyWith = path }
                    held = true
                default: break
                }
                if held, let extra = line.extra {
                    step.lineExtras[LineKey(line: line.line, target: path, mark: line.mark)] = extra
                }
            }
            return step
        }
        topLeft = card.topLeft
    }
}

// MARK: - A node written as its card

/// A node of a flow written as the card it is (`card_create`, `card_update`;
/// `specboard://SpecBoard/Docs/Design:CardKinds`「flow のノード」): its kind's fields and its rows, and where it goes
/// and what its rows name as its lines (`connections`). The other way of `FlowNode.init(card:)`.
enum NodeWrite {
    /// What a node's kind keeps, as its value says it: the fields of every node, then the
    /// kind's own (`builtin.json`).
    static func fields(of kind: String) -> [String] {
        let own: [String] = switch kind {
        case "begin": ["trigger", "every", "program", "carries", "via", "initiator", "anyCaller"]
        case "run": ["carries", "returns", "via"]
        case "eval": ["value"]
        case "wait": ["for", "timeout"]
        case "join": ["join", "partial"]
        case "end": ["outcome", "return"]
        default: []
        }
        return ["label", "description"] + own
    }

    /// The node's fields as its value writes them, by name; a field it does not have is not
    /// there.
    static func values(_ node: FlowNode) -> [String: Any] {
        var out: [String: Any] = [:]
        func put(_ key: String, _ text: String?) { if let text, !text.isEmpty { out[key] = text } }
        func put(_ key: String, _ list: [String]) { if !list.isEmpty { out[key] = list } }
        put("label", node.label)
        put("description", node.description)
        switch node.kind {
        case "begin":
            put("trigger", node.trigger)
            put("every", node.every)
            put("program", node.program)
            put("carries", node.carries)
            put("via", node.via)
            put("initiator", node.initiator)
            if node.anyCaller { out["anyCaller"] = true }
        case "run":
            put("carries", node.carries)
            put("returns", node.returns)
            put("via", node.via)
        case "eval":
            put("value", node.value)
        case "wait":
            put("for", node.waitFor)
            put("timeout", node.timeout)
        case "join":
            put("join", node.join)
            put("partial", node.partial)
        case "end":
            put("outcome", node.outcome)
            if let reply = node.reply, !reply.carries.isEmpty || reply.program != nil {
                var answer: [String: Any] = [:]
                if !reply.carries.isEmpty { answer["carries"] = reply.carries }
                if let program = reply.program { answer["program"] = program }
                out["return"] = answer
            }
        default:
            break
        }
        return out
    }

    /// A process node's rows as its value writes them (`steps`): each row's fields, without
    /// what is its lines (what a row reads and writes, whom it sends to, its key).
    static func rows(_ node: FlowNode) -> [[String: Any]] {
        node.steps.map(row)
    }

    /// One row as its value writes it, without what is its lines.
    static func row(_ step: Step) -> [String: Any] {
        var row: [String: Any] = ["do": step.op]
        if let description = step.description, !description.isEmpty { row["description"] = description }
        if step.op == "sign" || step.op == "verify", !step.ownPaths.isEmpty { row["paths"] = step.ownPaths }
        if let program = step.program { row["program"] = program }
        if !step.carries.isEmpty { row["carries"] = step.carries }
        if !step.returns.isEmpty { row["returns"] = step.returns }
        if let via = step.via { row["via"] = via }
        if let initiator = step.initiator { row["initiator"] = initiator }
        return row
    }

    /// Where a line goes, as a path is written: the card (or card ghost, or card inside a
    /// container) it names, and where it lands in it.
    typealias End = (to: String, at: String?)

    /// The node's lines as its value writes them (`connections`): the ways to the other
    /// nodes by their names, every other place by `end` — what it runs and waits for, whom
    /// it hears from or answers, its keys; then its rows' lines, from each row
    /// (`steps[0]`).
    static func lines(_ node: FlowNode, end: (String) -> End) -> [[String: Any]] {
        var out: [[String: Any]] = []
        // What the line already there says beyond what the model holds goes with it.
        func said(_ made: inout [String: Any], _ line: String, _ target: String, _ fields: [String: Any],
                  _ extras: [LineKey: LineExtra]) {
            let mark = (fields["when"] ?? fields["outcome"] ?? fields["use"]) as? String ?? ""
            guard let extra = extras[LineKey(line: line, target: target, mark: mark)] else { return }
            if let description = extra.description { made["description"] = description }
            if !extra.fields.isEmpty {
                var all = made["fields"] as? [String: Any] ?? [:]
                for (key, value) in extra.fields where all[key] == nil { all[key] = value.any }
                made["fields"] = all
            }
        }
        func way(_ line: String, _ to: String?, _ fields: [String: Any] = [:]) {
            guard let to else { return }
            var made: [String: Any] = ["line": line, "to": PathText.quote(to)]
            if !fields.isEmpty { made["fields"] = fields }
            said(&made, line, to, fields, node.lineExtras)
            out.append(made)
        }
        func path(_ line: String, _ path: String?, from: String = "", _ fields: [String: Any] = [:],
                  extras: [LineKey: LineExtra]? = nil) {
            guard let path, !path.isEmpty else { return }
            let reached = end(path)
            var made: [String: Any] = ["line": line, "to": reached.to]
            if !from.isEmpty { made["from"] = from }
            if let at = reached.at, !at.isEmpty { made["at"] = at }
            if !fields.isEmpty { made["fields"] = fields }
            said(&made, line, path, fields, extras ?? node.lineExtras)
            out.append(made)
        }
        for next in node.nexts { way(Line.next, next) }
        for branch in node.branches { way(Line.condition, branch.next, ["when": branch.when]) }
        way(Line.otherwise, node.otherwise)
        way(Line.onError, node.onError)
        way(Line.onTimeout, node.onTimeout)
        path(Line.runs, node.target)
        if let event = node.event { path(Line.awaits, event.flow, ["outcome": event.outcome]) }
        for peer in node.kind == "end" ? (node.reply?.peers ?? []) : node.peers { path(Line.partner, peer) }
        if node.kind == "end" {
            path(Line.key, node.reply?.signWith, ["use": "sign"])
        } else {
            path(Line.key, node.signWith, ["use": "sign"])
            path(Line.key, node.verifyWith, ["use": "verify"])
        }
        for (index, step) in node.steps.enumerated() {
            let from = "steps[\(index)]"
            let extras = step.lineExtras
            let split = step.pathSplit
            switch step.op {
            case "read": for one in split.own { path(Line.reads, one, from: from, extras: extras) }
            case "write": for one in split.own { path(Line.writes, one, from: from, extras: extras) }
            default: break
            }
            // The other kind of line on a read or write row: not the row's to change, only to let go of
            // with the path.
            for other in split.foreign { path(other.line, other.path, from: from, extras: extras) }
            for peer in step.peers { path(Line.partner, peer, from: from, extras: extras) }
            if step.op == "sign" || step.op == "verify" {
                path(Line.key, step.key, from: from, ["use": step.op], extras: extras)
            }
            path(Line.key, step.signWith, from: from, ["use": "sign"], extras: extras)
            path(Line.key, step.verifyWith, from: from, ["use": "verify"], extras: extras)
        }
        return out
    }

    /// Whether a node can be made with this line of its, when the nodes named in `there` are
    /// in its flow: all but a way to a node not made yet (the core refuses a line to a card
    /// that is not there), which is written once that node is.
    static func ready(_ line: [String: Any], there: Set<String>) -> Bool {
        guard let kind = line["line"] as? String, Line.flowWays.contains(kind), let to = line["to"] as? String else {
            return true
        }
        return there.contains(PathText.unquote(to))
    }

    /// The node made as a card inside its flow (`card_create`): its kind, fields, rows,
    /// lines, and where it sits.
    static func made(_ node: FlowNode, end: (String) -> End) -> [String: Any] {
        var value = values(node)
        value["kind"] = node.kind
        if node.kind == "process" { value["steps"] = rows(node) }
        let lines = lines(node, end: end)
        if !lines.isEmpty { value["connections"] = lines }
        if let at = node.topLeft { value["topLeft"] = ["x": at.x, "y": at.y] }
        return value
    }

    /// What changed of a node, as `card_update` writes it on the node's card: its name, each
    /// field that changed (null for one taken away), its rows when their keys changed, and the lines
    /// that came or went (`lineEdits`) — the lines that stayed are not written again. Nil when
    /// nothing did.
    static func changed(from was: FlowNode, to now: FlowNode, end: (String) -> End) -> [String: Any]? {
        var value: [String: Any] = [:]
        if was.name != now.name { value["name"] = now.name }
        let before = values(was), after = values(now)
        for key in fields(of: now.kind) {
            let a = before[key].map(OJSONBox.init), b = after[key].map(OJSONBox.init)
            guard a != b else { continue }
            // A list taken away is written empty: the core reads null in a list's place as a list
            // of one null, which is not a path, nor allowed where the field is not (a begin no
            // longer receiving).
            value[key] = after[key] ?? (before[key] is [Any] ? [Any]() : NSNull())
        }
        if now.kind == "process", OJSONBox(rows(was)) != OJSONBox(rows(now)) {
            value["steps"] = rows(now)
        }
        // A way to a node renamed in the same edit is the same way: compared by where the
        // ways go once the rename is taken into account.
        if let edits = lineEdits(from: lines(was, end: end), to: lines(now, end: end)) {
            value["connections"] = edits
        }
        return value.isEmpty ? nil : value
    }

    /// The keys of the one row that differs between two lists of a node's rows (null for a key
    /// taken away), with its place; nil when the lists are the same length and differ in more or
    /// fewer than one row.
    static func rowEdit(from was: FlowNode, to now: FlowNode) -> (index: Int, value: [String: Any])? {
        let a = rows(was), b = rows(now)
        guard a.count == b.count else { return nil }
        let differing = a.indices.filter { OJSONBox(a[$0]) != OJSONBox(b[$0]) }
        guard differing.count == 1, let index = differing.first else { return nil }
        var value: [String: Any] = [:]
        for key in Set(a[index].keys).union(b[index].keys) {
            let x = a[index][key].map(OJSONBox.init), y = b[index][key].map(OJSONBox.init)
            if x != y { value[key] = b[index][key] ?? NSNull() }
        }
        return (index, value)
    }

    /// The lines that came and went between two lists of a node's lines, as `connections` takes them
    /// (`{remove, add}`): the card keeps the lines that stayed as they are, with whatever they say
    /// that the window does not hold. Lines of one kind from one place (the ways `next` or
    /// `branch`, a row's `reads`) are in an order that means something: when the lines that came and
    /// went would leave them in another order than `now` says, the whole group is written again.
    /// Nil when no line came or went.
    static func lineEdits(from was: [[String: Any]], to now: [[String: Any]]) -> [String: Any]? {
        func group(_ line: [String: Any]) -> String { "\(line["line"] as? String ?? "")|\(line["from"] as? String ?? "")" }
        var groups: [String] = []
        for line in was + now where !groups.contains(group(line)) { groups.append(group(line)) }
        var removed: [[String: Any]] = [], added: [[String: Any]] = []
        for key in groups {
            let old = was.filter { group($0) == key }, new = now.filter { group($0) == key }
            if old.map(OJSONBox.init) == new.map(OJSONBox.init) { continue }
            var gone = old
            var coming: [[String: Any]] = []
            for line in new {
                if let at = gone.firstIndex(where: { OJSONBox($0) == OJSONBox(line) }) {
                    gone.remove(at: at)
                } else {
                    coming.append(line)
                }
            }
            // What would stand there: the old lines that stay, then the new ones.
            var pool = gone
            var staying: [[String: Any]] = []
            for line in old {
                if let at = pool.firstIndex(where: { OJSONBox($0) == OJSONBox(line) }) {
                    pool.remove(at: at)
                } else {
                    staying.append(line)
                }
            }
            if (staying + coming).map(OJSONBox.init) == new.map(OJSONBox.init) {
                removed += gone
                added += coming
            } else {
                removed += old
                added += new
            }
        }
        if removed.isEmpty && added.isEmpty { return nil }
        var out: [String: Any] = [:]
        if !removed.isEmpty { out["remove"] = removed }
        if !added.isEmpty { out["add"] = added }
        return out
    }
}

/// How the window's model tells a line it holds from another of the same node or row: its kind, where it
/// goes (the node's name for a way, the path for any other), and the one field that tells lines of one
/// kind apart (a branch's `when`, an event's `outcome`, a key's `use`).
nonisolated struct LineKey: Hashable, Sendable {
    var line: String
    var target: String
    var mark: String
}

/// What a line says that the model has no place for: its description and the fields it does not hold.
nonisolated struct LineExtra: Hashable, Sendable {
    var description: String?
    var fields: [String: JSONValue]
}

nonisolated extension Connection {
    /// The fields the model holds of a line, whose value is `mark`.
    private static let held: Set<String> = ["when", "outcome", "use"]

    var mark: String { text("when") ?? text("outcome") ?? text("use") ?? "" }

    /// What the line says beyond the fields the model holds; nil when it says nothing more.
    var extra: LineExtra? {
        let more = fields.filter { !Self.held.contains($0.key) }
        let said = description.flatMap { $0.isEmpty ? nil : $0 }
        return said == nil && more.isEmpty ? nil : LineExtra(description: said, fields: more)
    }
}

extension Step {
    /// A line of a read or write row that is not of the kind the row does — a `reads` line on a write row:
    /// its path is listed with the row's `paths`, and the line is written as it is.
    nonisolated struct Foreign: Hashable, Sendable {
        var line: String
        var path: String
    }

    /// The row's paths that are its own, and the other kind of lines that are still listed (one taken off
    /// the list is let go of).
    nonisolated var pathSplit: (own: [String], foreign: [Foreign]) {
        var own = paths
        var kept: [Foreign] = []
        for other in foreign {
            guard let at = own.lastIndex(of: other.path) else { continue }
            own.remove(at: at)
            kept.append(other)
        }
        return (own, kept)
    }

    /// The row's paths, without those of lines that are not of its kind.
    nonisolated var ownPaths: [String] { pathSplit.own }
}

/// A value as `JSONSerialization` takes it, compared as JSON (for what changed).
nonisolated private struct OJSONBox: Equatable {
    let text: String
    init(_ value: Any) {
        let data = (try? JSONSerialization.data(withJSONObject: ["v": value], options: [.sortedKeys])) ?? Data()
        text = String(decoding: data, as: UTF8.self)
    }
}
