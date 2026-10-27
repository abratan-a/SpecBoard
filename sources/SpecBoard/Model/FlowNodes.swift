import Foundation

/// One node of a flow (`specboard://SpecBoard/Docs/Design:Overview` §11): where it starts, what it runs, where
/// it decides, where it waits, and how it ends — joined by the next each one names,
/// and by nothing guessed from the actions' rows.
///
///     begin   trigger (manual / receive / event / schedule / startup), next; on receive
///             peers and program (none: not written, a gap — or anyCaller, anyone), carries,
///             via, initiator, verifyWith
///     process steps — rows read, written, signed, checked, called, sent and
///             received, in order; next
///     run     target — a flow (an action, until they are moved); carries, returns,
///             via, signWith, verifyWith; next, onError
///     eval    value (a path), branches [{when, next}], else
///     wait    for (approval / timer / event), timeout and onTimeout, next
///     join    join (all / any / first), partial (fail / continue), next: where ways
///             that parted meet again
///     end     outcome (success / failure / cancelled): what the flow returns; and
///             `return`, what it answers whoever called it with
///
/// A `next` of two or more parts the way: each goes on by itself, until they meet
/// at a join.
///
/// Conditions are text, never evaluated; a `[[path]]` in one is checked like any path.
nonisolated struct FlowNode: Codable, Hashable, Sendable {
    var name: String
    var kind: String
    var label: String?
    /// A word on the node, for whoever reads it (`specboard://SpecBoard/Docs/Design:Overview` §5「書く場所」).
    var description: String?
    // begin
    var trigger: String?
    /// begin on an event, or wait for one: another flow's end, by its outcome.
    var event: FlowEvent?
    var every: String?
    // run
    var target: String?
    /// The old way of running in parallel, read only to be moved (`modernized`).
    var targets: [String] = []
    // join
    var join: String?
    var partial: String?
    // run
    var onError: String?
    // eval
    var value: String?
    var branches: [FlowBranch] = []
    /// `else` on the wire.
    var otherwise: String?
    // wait
    /// `for` on the wire.
    var waitFor: String?
    var timeout: String?
    var onTimeout: String?
    // end
    var outcome: String?
    /// What an end answers whoever called the flow with. None: it ends, and
    /// says nothing back.
    var reply: FlowReturn?
    // process
    /// A process node's rows, in order: what is read, written, signed, checked, called
    /// (a program outside the project), sent and received. No group and no return:
    /// a flow's ways are its nodes.
    var steps: [Step] = []
    // begin on receive, and run
    /// The flows that send what starts it, in their lines' order; with the program, all it takes
    /// from (none: whoever calls the flow).
    var peers: [String] = []
    var program: String?
    /// What it receives (begin), or hands to the flow it runs (run).
    var carries: [String] = []
    /// What the flow it runs gives back.
    var returns: [String] = []
    var via: String?
    var initiator: String?
    /// A begin on receive that takes anyone: no partner named, and none missing.
    var anyCaller = false
    var verifyWith: String?
    var signWith: String?
    // all but end and eval: one, or more to part the way.
    var nexts: [String] = []
    /// Where its card sits on the flow's diagram, in grid units (§12).
    var topLeft: GridPoint?
    /// What its lines say beyond what the node holds, by the line (`LineExtra`); not coded.
    var lineExtras: [LineKey: LineExtra] = [:]
    /// Its cell before the flow was on the grid, read only to be moved (`modernized`).
    var at: FlowCell?

    /// A receiving begin that names no one and takes no caller: a gap, drawn and checked as one.
    var namesNoPartner: Bool { kind == "begin" && trigger == "receive" && peers.isEmpty && program == nil && !anyCaller }

    /// A begin's trigger set: what only another trigger keeps goes with it.
    mutating func setTrigger(_ value: String?) {
        trigger = value
        if value != "event" { event = nil }
        if value != "schedule" { every = nil }
        if value != "receive" {
            peers = []; program = nil; carries = []
            via = nil; initiator = nil; verifyWith = nil; anyCaller = false
        }
    }

    /// Anyone may call it, or not: anyone takes the place of the partners named, which go.
    mutating func setAnyone(_ on: Bool) {
        anyCaller = on
        if on { peers = []; program = nil }
    }

    /// Where it goes, when that is one way — or the first of several.
    var next: String? {
        get { nexts.first }
        set { nexts = newValue.map { [$0] } ?? [] }
    }

    init(name: String, kind: String) {
        self.name = name
        self.kind = kind
    }

    /// This node with `change` made — for building one in a line.
    func with(_ change: (inout FlowNode) -> Void) -> FlowNode {
        var out = self
        change(&out)
        return out
    }

    enum CodingKeys: String, CodingKey {
        case name, kind, label, description, trigger, event, every, target, targets, join, partial, onError,
             value, branches, otherwise = "else", waitFor = "for", timeout, onTimeout, outcome, next, topLeft, at,
             reply = "return", steps, peers, program, carries, returns, via, initiator, anyCaller, verifyWith, signWith
        /// The name it had before, read only (old projects, history).
        case note
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        name = try c.decode(String.self, forKey: .name)
        kind = try c.decode(String.self, forKey: .kind)
        label = try c.decodeIfPresent(String.self, forKey: .label)
        description = try c.decodeIfPresent(String.self, forKey: .description)
            ?? c.decodeIfPresent(String.self, forKey: .note)
        trigger = try c.decodeIfPresent(String.self, forKey: .trigger)
        event = try c.decodeIfPresent(FlowEvent.self, forKey: .event)
        every = try c.decodeIfPresent(String.self, forKey: .every)
        target = try c.decodeIfPresent(String.self, forKey: .target)
        targets = try c.decodeIfPresent([String].self, forKey: .targets) ?? []
        join = try c.decodeIfPresent(String.self, forKey: .join)
        partial = try c.decodeIfPresent(String.self, forKey: .partial)
        onError = try c.decodeIfPresent(String.self, forKey: .onError)
        value = try c.decodeIfPresent(String.self, forKey: .value)
        branches = try c.decodeIfPresent([FlowBranch].self, forKey: .branches) ?? []
        otherwise = try c.decodeIfPresent(String.self, forKey: .otherwise)
        waitFor = try c.decodeIfPresent(String.self, forKey: .waitFor)
        timeout = try c.decodeIfPresent(String.self, forKey: .timeout)
        onTimeout = try c.decodeIfPresent(String.self, forKey: .onTimeout)
        outcome = try c.decodeIfPresent(String.self, forKey: .outcome)
        reply = try c.decodeIfPresent(FlowReturn.self, forKey: .reply)
        steps = try c.decodeIfPresent([Step].self, forKey: .steps) ?? []
        peers = try c.decodeIfPresent([String].self, forKey: .peers) ?? []
        program = try c.decodeIfPresent(String.self, forKey: .program)
        carries = try c.decodeIfPresent([String].self, forKey: .carries) ?? []
        returns = try c.decodeIfPresent([String].self, forKey: .returns) ?? []
        via = try c.decodeIfPresent(String.self, forKey: .via)
        initiator = try c.decodeIfPresent(String.self, forKey: .initiator)
        anyCaller = try c.decodeIfPresent(Bool.self, forKey: .anyCaller) ?? false
        verifyWith = try c.decodeIfPresent(String.self, forKey: .verifyWith)
        signWith = try c.decodeIfPresent(String.self, forKey: .signWith)
        if let one = try? c.decodeIfPresent(String.self, forKey: .next) {
            nexts = [one]
        } else {
            nexts = try c.decodeIfPresent([String].self, forKey: .next) ?? []
        }
        topLeft = try c.decodeIfPresent(GridPoint.self, forKey: .topLeft)
        at = try c.decodeIfPresent(FlowCell.self, forKey: .at)
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(name, forKey: .name)
        try c.encode(kind, forKey: .kind)
        try c.encodeIfPresent(label, forKey: .label)
        try c.encodeIfPresent(description, forKey: .description)
        try c.encodeIfPresent(trigger, forKey: .trigger)
        try c.encodeIfPresent(event, forKey: .event)
        try c.encodeIfPresent(every, forKey: .every)
        try c.encodeIfPresent(target, forKey: .target)
        if !targets.isEmpty { try c.encode(targets, forKey: .targets) }
        try c.encodeIfPresent(join, forKey: .join)
        try c.encodeIfPresent(partial, forKey: .partial)
        try c.encodeIfPresent(onError, forKey: .onError)
        try c.encodeIfPresent(value, forKey: .value)
        if !branches.isEmpty { try c.encode(branches, forKey: .branches) }
        try c.encodeIfPresent(otherwise, forKey: .otherwise)
        try c.encodeIfPresent(waitFor, forKey: .waitFor)
        try c.encodeIfPresent(timeout, forKey: .timeout)
        try c.encodeIfPresent(onTimeout, forKey: .onTimeout)
        try c.encodeIfPresent(outcome, forKey: .outcome)
        try c.encodeIfPresent(reply, forKey: .reply)
        if !steps.isEmpty { try c.encode(steps, forKey: .steps) }
        if !peers.isEmpty { try c.encode(peers, forKey: .peers) }
        try c.encodeIfPresent(program, forKey: .program)
        if !carries.isEmpty { try c.encode(carries, forKey: .carries) }
        if !returns.isEmpty { try c.encode(returns, forKey: .returns) }
        try c.encodeIfPresent(via, forKey: .via)
        try c.encodeIfPresent(initiator, forKey: .initiator)
        if anyCaller { try c.encode(true, forKey: .anyCaller) }
        try c.encodeIfPresent(verifyWith, forKey: .verifyWith)
        try c.encodeIfPresent(signWith, forKey: .signWith)
        if nexts.count == 1 { try c.encode(nexts[0], forKey: .next) } else if !nexts.isEmpty { try c.encode(nexts, forKey: .next) }
        try c.encodeIfPresent(topLeft, forKey: .topLeft)
        try c.encodeIfPresent(at, forKey: .at)
    }

    // MARK: - Vocabulary

    enum Vocabulary {
        static let kinds = ["begin", "process", "run", "eval", "wait", "join", "end"]
        static let triggers = ["manual", "receive", "event", "schedule", "startup"]
        /// What a process node's rows may do: no group, no return.
        static let processOps = ["read", "write", "sign", "verify", "call", "send", "receive", "compute"]
        /// The keys only a begin on receive has.
        static let receiving = ["peer", "program", "carries", "via", "initiator", "anyCaller", "verifyWith"]
        static let joins = ["all", "any", "first"]
        static let partials = ["fail", "continue"]
        static let waits = ["approval", "timer", "event"]
        static let outcomes = ["success", "failure", "cancelled"]

        /// Beside `name`, `kind`, `label`, `description` and `topLeft`, which every node may have.
        static let keys: [String: [String]] = [
            "begin": ["trigger", "event", "every", "next"] + receiving,
            "process": ["steps", "next"],
            "run": ["target", "carries", "returns", "via", "signWith", "verifyWith", "onError", "next"],
            "eval": ["value", "branches", "else"],
            "wait": ["for", "event", "timeout", "onTimeout", "next"],
            "join": ["join", "partial", "next"],
            "end": ["outcome", "return"],
        ]

        /// The heading a person sees: the kind's definition's label (`BuiltinWords`).
        static func label(_ kind: String) -> String { BuiltinWords.label(kind) }

        /// What it is, in a line: the first sentence of the kind's purpose.
        static func holding(_ kind: String) -> String { BuiltinWords.purpose(kind) }
    }

    /// The keys it was written with, as the wire names them.
    var givenKeys: [String] {
        var out: [String] = []
        if trigger != nil { out.append("trigger") }
        if event != nil { out.append("event") }
        if every != nil { out.append("every") }
        if target != nil { out.append("target") }
        if !targets.isEmpty { out.append("targets") }
        if join != nil { out.append("join") }
        if partial != nil { out.append("partial") }
        if onError != nil { out.append("onError") }
        if value != nil { out.append("value") }
        if !branches.isEmpty { out.append("branches") }
        if otherwise != nil { out.append("else") }
        if waitFor != nil { out.append("for") }
        if timeout != nil { out.append("timeout") }
        if onTimeout != nil { out.append("onTimeout") }
        if outcome != nil { out.append("outcome") }
        if reply != nil { out.append("return") }
        if !steps.isEmpty { out.append("steps") }
        if !peers.isEmpty { out.append("peer") }
        if program != nil { out.append("program") }
        if !carries.isEmpty { out.append("carries") }
        if !returns.isEmpty { out.append("returns") }
        if via != nil { out.append("via") }
        if initiator != nil { out.append("initiator") }
        if anyCaller { out.append("anyCaller") }
        if verifyWith != nil { out.append("verifyWith") }
        if signWith != nil { out.append("signWith") }
        if !nexts.isEmpty { out.append("next") }
        return out
    }

    /// Where it goes, with what each way is called on the canvas: none for `next`, a branch's
    /// condition, and the others by their kinds of line (`else`, `onError`, `onTimeout`; `BuiltinWords`).
    var exits: [(to: String, label: String?, kind: Exit)] {
        var out: [(String, String?, Exit)] = []
        for next in nexts { out.append((next, nil, .next)) }
        for branch in branches { out.append((branch.next, branch.when, .branch)) }
        if let otherwise { out.append((otherwise, BuiltinWords.line(Line.otherwise), .otherwise)) }
        if let onError { out.append((onError, BuiltinWords.line(Line.onError), .error)) }
        if let onTimeout { out.append((onTimeout, BuiltinWords.line(Line.onTimeout), .timeout)) }
        return out
    }

    enum Exit: Hashable, Sendable { case next, branch, otherwise, error, timeout }

    /// As JSON with its keys always in the same order — what MCP answers with and the
    /// store keeps, so a project's file changes only where a node did.
    var ojson: OJSON {
        var out = OrderedObject([("name", .string(name)), ("kind", .string(kind))])
        func put(_ key: String, _ value: String?) { if let value { out[key] = .string(value) } }
        put("label", label); put("description", description); put("trigger", trigger)
        if !peers.isEmpty { out["peers"] = .string(peers) }
        put("program", program)
        if let event { out["event"] = .object(OrderedObject([("flow", .string(event.flow)), ("outcome", .string(event.outcome))])) }
        put("every", every)
        if !steps.isEmpty { out["steps"] = .array(steps.map(MCPViews.stepJSON)) }
        put("target", target)
        if !carries.isEmpty { out["carries"] = .string(carries) }
        if !returns.isEmpty { out["returns"] = .string(returns) }
        put("via", via); put("initiator", initiator)
        if anyCaller { out["anyCaller"] = .bool(true) }
        put("verifyWith", verifyWith); put("signWith", signWith)
        if !targets.isEmpty { out["targets"] = .array(targets.map(OJSON.string)) }
        put("join", join); put("partial", partial); put("onError", onError); put("value", value)
        if !branches.isEmpty {
            out["branches"] = .array(branches.map { .object(OrderedObject([("when", .string($0.when)), ("next", .string($0.next))])) })
        }
        put("else", otherwise); put("for", waitFor); put("timeout", timeout); put("onTimeout", onTimeout)
        put("outcome", outcome)
        if let reply { out["return"] = reply.ojson }
        if nexts.count == 1 { out["next"] = .string(nexts[0]) } else if !nexts.isEmpty { out["next"] = .array(nexts.map(OJSON.string)) }
        if let topLeft {
            out["topLeft"] = .object(OrderedObject([("x", .number(Double(topLeft.x))), ("y", .number(Double(topLeft.y)))]))
        }
        if let at { out["at"] = .object(OrderedObject([("r", .number(Double(at.r))), ("c", .number(Double(at.c)))])) }
        return .object(out)
    }

    static func stored(_ nodes: [FlowNode]) -> String { OJSON.array(nodes.map(\.ojson)).compact() }

    /// Every path it names, passed through `path`; conditions through `text`.
    func mapped(path: (String) -> String, text: (String) -> String) -> FlowNode {
        var out = self
        out.target = target.map(path)
        out.targets = targets.map(path)
        out.value = value.map(path)
        out.event = event.map { FlowEvent(flow: path($0.flow), outcome: $0.outcome) }
        out.branches = branches.map { FlowBranch(when: text($0.when), next: $0.next) }
        out.steps = steps.map { $0.mapped(path: path, text: text) }
        out.peers = peers.map(path)
        out.carries = carries.map(path)
        out.returns = returns.map(path)
        out.verifyWith = verifyWith.map(path)
        out.signWith = signWith.map(path)
        out.reply = reply.map { $0.mapped(path: path) }
        return out
    }

    /// The paths it names, with what each has to be: `(where, path, kinds)`.
    var declared: [(via: String, path: String, kinds: Set<SchemaKind>)] {
        var out: [(String, String, Set<SchemaKind>)] = []
        if let target { out.append(("nodes.\(name).target", target, [.flow])) }
        for (index, peer) in peers.enumerated() { out.append(("nodes.\(name).peers[\(index)]", peer, [.flow])) }
        for (key, list) in [("carries", carries), ("returns", returns)] {
            for (index, path) in list.enumerated() { out.append(("nodes.\(name).\(key)[\(index)]", path, [])) }
        }
        for (key, value) in [("verifyWith", verifyWith), ("signWith", signWith)] {
            if let value { out.append(("nodes.\(name).\(key)", value, [])) }
        }
        for (index, step) in steps.enumerated() {
            out += step.declared(at: "nodes.\(name).steps[\(index)]").map { ($0.via, $0.path, $0.expects.map { _ in [.flow] } ?? []) }
        }
        if let reply {
            for (index, peer) in reply.peers.enumerated() { out.append(("nodes.\(name).return.peers[\(index)]", peer, [.flow])) }
            for (index, path) in reply.carries.enumerated() { out.append(("nodes.\(name).return.carries[\(index)]", path, [])) }
            if let key = reply.signWith { out.append(("nodes.\(name).return.signWith", key, [])) }
        }
        for (index, one) in targets.enumerated() { out.append(("nodes.\(name).targets[\(index)]", one, [.flow])) }
        if let event { out.append(("nodes.\(name).event.flow", event.flow, [.flow])) }
        if let value { out.append(("nodes.\(name).value", value, [])) }
        return out
    }

    /// The conditions it carries, for the `[[path]]` in them.
    var conditions: [(via: String, text: String)] {
        branches.enumerated().map { ("nodes.\(name).branches[\($0)].when", $1.when) }
    }
}

/// What an end answers whoever called the flow with: what it carries back, to
/// whom when it is not the caller (the flows, in their lines' order, and the program), and the
/// key it signs it with.
nonisolated struct FlowReturn: Codable, Hashable, Sendable {
    var carries: [String] = []
    var peers: [String] = []
    var program: String?
    var signWith: String?

    init() {}

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        carries = try c.decodeIfPresent([String].self, forKey: .carries) ?? []
        peers = try c.decodeIfPresent([String].self, forKey: .peers) ?? []
        program = try c.decodeIfPresent(String.self, forKey: .program)
        signWith = try c.decodeIfPresent(String.self, forKey: .signWith)
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        if !carries.isEmpty { try c.encode(carries, forKey: .carries) }
        if !peers.isEmpty { try c.encode(peers, forKey: .peers) }
        try c.encodeIfPresent(program, forKey: .program)
        try c.encodeIfPresent(signWith, forKey: .signWith)
    }

    private enum CodingKeys: String, CodingKey { case carries, peers, program, signWith }

    var ojson: OJSON {
        var out = OrderedObject()
        if !carries.isEmpty { out["carries"] = .string(carries) }
        if !peers.isEmpty { out["peers"] = .string(peers) }
        if let program { out["program"] = .string(program) }
        if let signWith { out["signWith"] = .string(signWith) }
        return .object(out)
    }

    func mapped(path: (String) -> String) -> FlowReturn {
        var out = self
        out.carries = carries.map(path)
        out.peers = peers.map(path)
        out.signWith = signWith.map(path)
        return out
    }
}

nonisolated struct FlowEvent: Codable, Hashable, Sendable {
    var flow: String
    var outcome: String
}

nonisolated struct FlowBranch: Codable, Hashable, Sendable {
    var when: String
    var next: String
}

nonisolated struct FlowCell: Codable, Hashable, Sendable {
    var r: Int
    var c: Int
}

// MARK: - Checking

extension Validate {

    /// A flow's nodes as they may be stored (`specboard://SpecBoard/Docs/Design:Overview` §11), nodes given no
    /// cell placed (`FlowLayout.place`). What is wrong is refused wherever it comes
    /// from; what is only unfinished — a node with no next yet, a run with nothing to
    /// run — is refused when `complete` (MCP), and kept, and said, when not (the
    /// canvas, where a node just added is always unfinished).
    nonisolated static func nodes(_ nodes: [FlowNode], complete: Bool = true, margin: Int = 1) throws -> [FlowNode] {
        let found = FlowNode.problems(nodes)
        if let wrong = found.wrong { throw Invalid(wrong) }
        if complete, let first = found.unfinished.first { throw Invalid(first) }
        return FlowLayout.place(nodes, margin: margin)
    }
}

extension FlowNode {

    /// What is wrong with a flow's nodes — the first thing, said so it can be put
    /// right — and what is not finished yet.
    static func problems(_ nodes: [FlowNode]) -> (wrong: String?, unfinished: [String]) {
        let found = check(nodes)
        return (found.wrong, found.unfinished.map(\.text))
    }

    /// `problems`, each unfinished thing with the node it is about: nil when it is about the flow or
    /// several nodes.
    static func check(_ nodes: [FlowNode]) -> (wrong: String?, unfinished: [(node: String?, text: String)]) {
        let language = Language.current
        func say(_ resource: LocalizedStringResource) -> String { L10n.text(resource, in: language) }
        let joiner = say(.Common.separatorList)
        var unfinished: [(node: String?, text: String)] = []
        guard !nodes.isEmpty else { return (nil, []) }
        var names: Set<String> = []
        // Two names that differ only in ASCII case are one name, as the core holds them.
        var folded: Set<String> = []
        for node in nodes {
            do { _ = try Validate.name(node.name, field: say(.Flow.fieldNodeNameOf(node.name))) } catch {
                return (ErrorText.said(error), [])
            }
            names.insert(node.name)
            guard folded.insert(PathText.fold(node.name)).inserted else {
                return (say(.Flow.problemDuplicateName(node.name)), [])
            }
        }
        let begins = nodes.filter { $0.kind == "begin" }
        if begins.count > 1 {
            return (say(.Flow.problemManyStarts(begins.count, begins.map(\.name).joined(separator: joiner))), [])
        }
        if begins.isEmpty { unfinished.append((nil, say(.Flow.problemNoStart))) }
        func one(_ value: String?, of list: [String], _ key: String, _ node: FlowNode) -> String? {
            guard let value, !list.contains(value) else { return nil }
            return say(.Flow.problemNotOneOf(node.name, key, list.joined(separator: " / "), value))
        }
        func needs(_ present: Bool, _ key: String, _ node: FlowNode, _ why: @autoclosure () -> String? = nil) {
            if present { return }
            let head = Vocabulary.label(node.kind)
            let text = why().map { say(.Flow.problemMissingWith(node.name, head, key, $0)) }
                ?? say(.Flow.problemMissing(node.name, head, key))
            unfinished.append((node.name, text))
        }
        for node in nodes {
            guard let allowed = Vocabulary.keys[node.kind] else {
                return (say(.Flow.problemNotOneOf(node.name, "kind", Vocabulary.kinds.joined(separator: " / "), node.kind)), [])
            }
            if !node.targets.isEmpty {
                return (say(.Flow.problemTargetsUnused(node.name)), [])
            }
            let stray = node.givenKeys.filter { !allowed.contains($0) }
            guard stray.isEmpty else {
                return (say(.Flow.problemStrayNodeKeys(node.name, node.kind, stray.joined(separator: joiner), node.kind,
                                                       allowed.joined(separator: joiner))), [])
            }
            for bad in [one(node.trigger, of: Vocabulary.triggers, "trigger", node),
                        one(node.join, of: Vocabulary.joins, "join", node),
                        one(node.partial, of: Vocabulary.partials, "partial", node),
                        one(node.waitFor, of: Vocabulary.waits, "for", node),
                        one(node.outcome, of: Vocabulary.outcomes, "outcome", node),
                        one(node.event?.outcome, of: Vocabulary.outcomes, "event.outcome", node)] {
                if let bad { return (bad, []) }
            }
            if node.kind == "begin", node.trigger != "receive",
               let stray = Vocabulary.receiving.first(where: { node.givenKeys.contains($0) }) {
                return (say(.Flow.problemReceiveOnly(node.name, stray)), [])
            }
            // What `via` may be is the project's (`ProjectDetail.vias`): the core checks it as it writes.
            if let bad = one(node.initiator, of: Step.Vocabulary.initiators, "initiator", node) { return (bad, []) }
            if node.kind == "process" {
                do { try Validate.processRows(node.steps, at: "nodes.\(node.name).steps") } catch {
                    return (ErrorText.said(error), [])
                }
            }
            switch node.kind {
            case "begin":
                needs(node.trigger != nil, "trigger", node, say(.Flow.whyTrigger))
                needs(node.trigger != "event" || node.event != nil, "event", node, say(.Flow.whyTriggerEvent))
                needs(!node.nexts.isEmpty, "next", node)
            case "process":
                needs(!node.steps.isEmpty, "steps", node, say(.Flow.whySteps))
                needs(!node.nexts.isEmpty, "next", node)
            case "run":
                needs(node.target != nil, "target", node, say(.Flow.whyTarget))
                needs(!node.nexts.isEmpty, "next", node)
            case "join":
                needs(node.join != nil, "join", node, say(.Flow.whyJoin))
                needs(!node.nexts.isEmpty, "next", node)
            case "eval":
                for (index, branch) in node.branches.enumerated() where branch.when.trimmingCharacters(in: .whitespaces).isEmpty {
                    return (say(.Flow.problemEmptyWhen(node.name, index)), [])
                }
                needs(!node.branches.isEmpty, "branches", node, say(.Flow.whyBranches))
                needs(node.otherwise != nil, "else", node, say(.Flow.whyElse))
            case "wait":
                if node.onTimeout != nil && node.timeout == nil {
                    return (say(.Flow.problemOnTimeoutNeedsTimeout(node.name)), [])
                }
                needs(node.waitFor != nil, "for", node, say(.Flow.whyFor))
                needs(node.waitFor != "event" || node.event != nil, "event", node, say(.Flow.whyForEvent))
                needs(!node.nexts.isEmpty, "next", node)
            default:
                needs(node.outcome != nil, "outcome", node, say(.Flow.whyOutcome))
            }
            for exit in node.exits where !names.contains(exit.to) {
                return (say(.Flow.problemNoSuchNode(node.name, exit.to, nodes.map(\.name).joined(separator: joiner))), [])
            }
            if Set(node.nexts).count != node.nexts.count {
                return (say(.Flow.problemDuplicateNext(node.name)), [])
            }
        }
        unfinished += partings(nodes, in: language).map { (node: nil as String?, text: $0) }
        return (nil, unfinished)
    }

    /// Ways that part and do not meet again: each way out of a `next` of two or more
    /// has to reach one and the same join, whatever it does on the way — and not end
    /// first. A failure's or a time-out's way may end on its own.
    static func partings(_ nodes: [FlowNode], in language: String = Language.current) -> [String] {
        func say(_ resource: LocalizedStringResource) -> String { L10n.text(resource, in: language) }
        let joiner = say(.Common.separatorList)
        let byName = Dictionary(nodes.map { ($0.name, $0) }, uniquingKeysWith: { a, _ in a })
        var out: [String] = []
        for split in nodes where split.nexts.count >= 2 {
            var met: Set<String> = []
            var said = false
            for start in split.nexts {
                var seen: Set<String> = []
                var queue = [start]
                var ends = false
                while let name = queue.popLast() {
                    guard seen.insert(name).inserted, let node = byName[name] else { continue }
                    if node.kind == "join" { met.insert(name); continue }
                    if node.kind == "end" { ends = true; continue }
                    queue += node.nexts + node.branches.map(\.next) + [node.otherwise].compactMap { $0 }
                }
                if ends {
                    out.append(say(.Flow.problemPartEnds(split.name, start)))
                    said = true
                }
            }
            if met.count > 1 {
                out.append(say(.Flow.problemPartMeetMany(split.name, met.sorted().joined(separator: joiner))))
            } else if met.isEmpty && !said {
                out.append(say(.Flow.problemPartNoJoin(split.name)))
            }
        }
        return out
    }
}

// MARK: - Placing

/// Nodes on the flow's grid (§12): kept where they were put, and a node with no
/// place given one the way it goes — a column for each step from the start, left to
/// right, the ways that part stacked down the same column — or, where that is
/// taken, the nearest place that keeps the margin clear.
nonisolated enum FlowLayout {
    /// A node's card, in grid units: ten across, and down as many as its rows take
    /// (`FlowCardMap.rows`).
    static func rect(_ node: FlowNode, at point: GridPoint) -> GridRect {
        GridRect(x: point.x, y: point.y, w: Grid.cardColumns, h: rows(node))
    }

    /// How many grid units tall a node's card is: its frame and a row each.
    static func rows(_ node: FlowNode) -> Int {
        Grid.rows(forHeight: 2 + 40 + 12 + Double(node.rowCount) * Grid.unitHeight)
    }

    static func place(_ nodes: [FlowNode], margin: Int = 1) -> [FlowNode] {
        guard nodes.contains(where: { $0.topLeft == nil }) else { return nodes }
        var settings = LayoutSettings.standard
        settings.margin = margin
        // How many steps from the start each node is, the first way it is reached.
        let index = Dictionary(nodes.enumerated().map { ($1.name, $0) }, uniquingKeysWith: { a, _ in a })
        var column: [Int: Int] = [:]
        var order: [Int] = []
        var queue = nodes.indices.filter { nodes[$0].kind == "begin" }
        for i in queue { column[i] = 0 }
        while !queue.isEmpty {
            let i = queue.removeFirst()
            order.append(i)
            for exit in nodes[i].exits {
                guard let j = index[exit.to], column[j] == nil else { continue }
                column[j] = column[i]! + 1
                queue.append(j)
            }
        }
        // What nothing reaches, a column beyond the rest.
        let beyond = (column.values.max() ?? -1) + 1
        for i in nodes.indices where column[i] == nil {
            column[i] = beyond
            order.append(i)
        }
        var out = nodes
        var taken = nodes.compactMap { node in node.topLeft.map { rect(node, at: $0) } }
        var depth: [Int: Int] = [:]
        for i in order {
            let c = column[i]!
            let wanted = GridPoint(x: c * (Grid.cardColumns + 2 * margin), y: depth[c] ?? 0)
            depth[c] = (depth[c] ?? 0) + rows(out[i]) + 2 * margin
            guard out[i].topLeft == nil else { continue }
            let at = settings.clearSpot(near: wanted, height: rows(out[i]), avoiding: taken)
            out[i].topLeft = at
            taken.append(rect(out[i], at: at))
        }
        return out
    }
}

extension FlowNode {
    /// How many rows its card has (`FlowCardMap.rows`): what it is, each way out, and
    /// what a kind says beside.
    var rowCount: Int {
        switch kind {
        case "begin": return 1 + (trigger == "receive" ? 1 : 0) + max(1, nexts.count)
        case "process": return max(1, steps.count) + max(1, nexts.count)
        case "run": return 1 + (carries.isEmpty && returns.isEmpty ? 0 : 1) + max(1, nexts.count) + (onError == nil ? 0 : 1)
        case "eval": return (value == nil ? 0 : 1) + max(1, branches.count) + 1
        case "wait": return 1 + (timeout == nil ? 0 : 1) + max(1, nexts.count)
        case "join": return 1 + (partial == nil ? 0 : 1) + max(1, nexts.count)
        default: return 1 + (reply == nil ? 0 : 1)
        }
    }

    /// Nodes written the old ways, in the new one — nil when there is nothing old in
    /// them (§11, §12): cells (`at`) become grid places the same way round, spaced by
    /// the tallest node and the margin; a run of several targets becomes ways parting
    /// to a run each and meeting at a join. What leads to such a run by `next` parts
    /// there itself; anything else is led to a join that parts the way.
    static func modernized(_ nodes: [FlowNode], margin: Int = 1) -> [FlowNode]? {
        let old = nodes.contains { $0.at != nil || !$0.targets.isEmpty }
        guard old else { return nil }
        var out = nodes
        let tallest = out.map(FlowLayout.rows).max() ?? 1
        for i in out.indices {
            if let at = out[i].at {
                if out[i].topLeft == nil {
                    out[i].topLeft = GridPoint(x: at.c * (Grid.cardColumns + 2 * margin), y: at.r * (tallest + 2 * margin))
                }
                out[i].at = nil
            }
        }
        var used = Set(out.map(\.name))
        func fresh(_ base: String) -> String {
            var name = base, n = 2
            while used.contains(name) { name = "\(base)\(n)"; n += 1 }
            used.insert(name)
            return name
        }
        for parallel in out.filter({ !$0.targets.isEmpty }) {
            let targets = [parallel.target].compactMap { $0 } + parallel.targets
            let meet = fresh("\(parallel.name)_join")
            var join = FlowNode(name: meet, kind: "join")
            join.join = parallel.join ?? "all"
            join.partial = parallel.partial
            join.nexts = parallel.nexts
            let runs = targets.enumerated().map { index, target -> FlowNode in
                var run = FlowNode(name: fresh("\(parallel.name)_\(index + 1)"), kind: "run")
                run.target = target
                run.onError = parallel.onError
                run.nexts = [meet]
                return run
            }
            let names = runs.map(\.name)
            let byNext = out.allSatisfy { node in
                node.branches.allSatisfy { $0.next != parallel.name } && node.otherwise != parallel.name
                    && node.onError != parallel.name && node.onTimeout != parallel.name
            }
            guard let at = out.firstIndex(where: { $0.name == parallel.name }) else { continue }
            if byNext {
                out.remove(at: at)
                for i in out.indices where out[i].nexts.contains(parallel.name) {
                    out[i].nexts = out[i].nexts.flatMap { $0 == parallel.name ? names : [$0] }
                }
                out.insert(contentsOf: runs + [join], at: min(at, out.count))
            } else {
                // Reached by a condition or a failure: a join of its one way in parts it.
                var parting = FlowNode(name: parallel.name, kind: "join")
                parting.label = parallel.label
                parting.description = parallel.description
                parting.join = "all"
                parting.nexts = names
                parting.topLeft = parallel.topLeft
                out[at] = parting
                out.insert(contentsOf: runs + [join], at: at + 1)
            }
            // The first run takes the parallel run's place; the rest find room.
            if let place = parallel.topLeft, let first = out.firstIndex(where: { $0.name == names.first }), byNext {
                out[first].topLeft = place
            }
        }
        return FlowLayout.place(out, margin: margin)
    }
}

// MARK: - From the old steps

extension FlowNode {

    /// A flow written as steps (before §11) as nodes: the way it went, the start and
    /// a successful end added. Each stage runs its action; a `switch` decides, a
    /// `loop` decides and comes back, an `unordered` of stages runs them in parallel.
    static func nodes(from steps: [Step]) -> [FlowNode] {
        var out: [FlowNode] = []
        var used: Set<String> = []
        func fresh(_ base: String) -> String {
            let stem = Ident.safe(PathText.lastName(base))
            var name = stem.isEmpty ? "node" : stem, n = 2
            while used.contains(name) { name = "\(stem)_\(n)"; n += 1 }
            used.insert(name)
            return name
        }
        /// Adds the nodes for `steps`, each going on to what follows; returns the
        /// first node's name, or `then` when there are none.
        func chain(_ steps: [Step], then: String) -> String {
            var next = then
            for step in steps.reversed() { next = node(step, then: next) }
            return next
        }
        func node(_ step: Step, then: String) -> String {
            switch step.op {
            case "action":
                var run = FlowNode(name: fresh(step.action ?? "run"), kind: "run")
                run.target = step.action
                run.description = step.description
                run.next = then
                out.append(run)
                return run.name
            case "unordered":
                let stages = step.steps.compactMap { $0.op == "action" ? $0.action : nil }
                if stages.count == step.steps.count, stages.count >= 2 {
                    var run = FlowNode(name: fresh("parallel"), kind: "run")
                    run.targets = stages
                    run.join = "all"
                    run.description = step.description
                    run.next = then
                    out.append(run)
                    return run.name
                }
                return chain(step.steps, then: then)
            case "switch":
                var eval = FlowNode(name: fresh("decide"), kind: "eval")
                eval.description = step.description
                eval.branches = step.cases.map { item in
                    FlowBranch(when: item.when ?? "", next: chain(item.steps, then: then))
                }.filter { !$0.when.isEmpty }
                eval.otherwise = then
                if eval.branches.isEmpty { return then }
                out.append(eval)
                return eval.name
            case "loop":
                var eval = FlowNode(name: fresh("repeat"), kind: "eval")
                eval.description = step.description
                eval.otherwise = then
                out.append(eval)
                let body = chain(step.steps, then: eval.name)
                if let i = out.firstIndex(where: { $0.name == eval.name }) {
                    let when = step.over ?? L10n.text(.Flow.defaultLoopWhen, in: Language.current)
                    out[i].branches = [FlowBranch(when: when, next: body)]
                }
                return eval.name
            default:
                return chain(step.steps, then: then)
            }
        }
        guard !steps.isEmpty else { return [] }
        used = ["start", "done"]
        var end = FlowNode(name: "done", kind: "end")
        end.outcome = "success"
        out.append(end)
        var begin = FlowNode(name: "start", kind: "begin")
        begin.trigger = "manual"
        begin.next = chain(steps, then: "done")
        out.append(begin)
        // In the way they go, the start first.
        var ordered: [FlowNode] = []
        var seen: Set<String> = []
        let byName = Dictionary(uniqueKeysWithValues: out.map { ($0.name, $0) })
        func visit(_ name: String) {
            guard let node = byName[name], seen.insert(name).inserted else { return }
            ordered.append(node)
            for exit in node.exits { visit(exit.to) }
        }
        visit("start")
        let all = ordered + out.filter { !seen.contains($0.name) }
        return modernized(all) ?? FlowLayout.place(all)
    }
}
