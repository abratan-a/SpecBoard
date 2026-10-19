import Foundation

/// One row of what an action does, or of the way a flow goes (`specboard://SpecBoard/Docs/Design:Overview` §5).
///
/// Rows run top to bottom. A group holds rows of its own: `sequence` in order,
/// `unordered` in any, `switch` one of its `cases` by a condition written as text,
/// `loop` once for each of what `over` says. Conditions are never evaluated — a
/// `[[path]]` in one names the field that decides, and is checked like any path.
struct Step: Codable, Hashable, Sendable {
    /// What the row does — `do` on the wire.
    var op: String
    /// The others at the other end, in their lines' order: received from, returned to, called,
    /// sent to (a send goes to each).
    var peers: [String] = []
    /// The other end when it is not an action of the project — a program run with `exec`; it may
    /// be one beside the peers.
    var program: String?
    /// What travels: received, returned or sent; for `call`, what is sent.
    var carries: [String] = []
    /// For `call`: what comes back.
    var returns: [String] = []
    /// For `read` and `write`: where.
    var paths: [String] = []
    /// For `sign` and `verify`: the key.
    var key: String?
    /// Signs what it puts out with this key — a `send`, `return` or `call` that signs.
    var signWith: String?
    /// Checks what it takes in with this key — a `receive` or `call` that verifies.
    var verifyWith: String?
    /// How it travels: `stdio`, `file`, `exec` or `http`. None is within the part.
    var via: String?
    /// Who opens the exchange — `self` or `peer` — apart from which way data flows:
    /// a part that fetches its work with a long GET receives, and still is the one
    /// that calls.
    var initiator: String?
    /// For a flow's row: the action at this point of the way.
    var action: String?
    /// For a case of a `switch`: when it is taken.
    var when: String?
    /// For a `loop`: what it goes once for each of.
    var over: String?
    /// A word on the row, for whoever reads it (`specboard://SpecBoard/Docs/Design:Overview` §5「書く場所」).
    var description: String?
    /// A group's rows, or a case's.
    var steps: [Step] = []
    /// Lines of the row that are not of the kind it does (`pathSplit`).
    var foreign: [Foreign] = []
    /// What its lines say beyond what the row holds, by the line (`LineExtra`); not coded.
    var lineExtras: [LineKey: LineExtra] = [:]
    /// A `switch`'s ways.
    var cases: [Step] = []

    init(op: String) { self.op = op }

    enum CodingKeys: String, CodingKey {
        case op = "do", peers, program, carries, returns, paths, key, signWith, verifyWith,
             via, initiator, action, when, over, description, steps, cases
        /// The name it had before, read only (old projects, history).
        case comment
        /// The one other end a row named before, read only.
        case peer
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        // A case has no `do` of its own: it is a condition and its rows.
        op = try c.decodeIfPresent(String.self, forKey: .op) ?? ""
        peers = try c.decodeIfPresent([String].self, forKey: .peers) ?? []
        if let one = try c.decodeIfPresent(String.self, forKey: .peer), !peers.contains(one) { peers.append(one) }
        program = try c.decodeIfPresent(String.self, forKey: .program)
        carries = try c.decodeIfPresent([String].self, forKey: .carries) ?? []
        returns = try c.decodeIfPresent([String].self, forKey: .returns) ?? []
        paths = try c.decodeIfPresent([String].self, forKey: .paths) ?? []
        key = try c.decodeIfPresent(String.self, forKey: .key)
        signWith = try c.decodeIfPresent(String.self, forKey: .signWith)
        verifyWith = try c.decodeIfPresent(String.self, forKey: .verifyWith)
        via = try c.decodeIfPresent(String.self, forKey: .via)
        initiator = try c.decodeIfPresent(String.self, forKey: .initiator)
        action = try c.decodeIfPresent(String.self, forKey: .action)
        when = try c.decodeIfPresent(String.self, forKey: .when)
        over = try c.decodeIfPresent(String.self, forKey: .over)
        description = try c.decodeIfPresent(String.self, forKey: .description)
            ?? c.decodeIfPresent(String.self, forKey: .comment)
        steps = try c.decodeIfPresent([Step].self, forKey: .steps) ?? []
        cases = try c.decodeIfPresent([Step].self, forKey: .cases) ?? []
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        if !op.isEmpty { try c.encode(op, forKey: .op) }
        if !peers.isEmpty { try c.encode(peers, forKey: .peers) }
        try c.encodeIfPresent(program, forKey: .program)
        if !carries.isEmpty { try c.encode(carries, forKey: .carries) }
        if !returns.isEmpty { try c.encode(returns, forKey: .returns) }
        if !paths.isEmpty { try c.encode(paths, forKey: .paths) }
        try c.encodeIfPresent(key, forKey: .key)
        try c.encodeIfPresent(signWith, forKey: .signWith)
        try c.encodeIfPresent(verifyWith, forKey: .verifyWith)
        try c.encodeIfPresent(via, forKey: .via)
        try c.encodeIfPresent(initiator, forKey: .initiator)
        try c.encodeIfPresent(action, forKey: .action)
        try c.encodeIfPresent(when, forKey: .when)
        try c.encodeIfPresent(over, forKey: .over)
        try c.encodeIfPresent(description, forKey: .description)
        if !steps.isEmpty { try c.encode(steps, forKey: .steps) }
        if !cases.isEmpty { try c.encode(cases, forKey: .cases) }
    }

    /// The words a row may start with, and the keys each may carry.
    enum Vocabulary {
        static let exchange = ["receive", "return", "call", "send"]
        static let actionOps = exchange + ["read", "write", "sign", "verify", "compute"]
        static let groups = ["sequence", "unordered", "switch", "loop"]
        static let flowOps = ["action"]
        static let initiators = ["self", "peer"]
        static let triggers = ["receive", "schedule", "startup"]

        static func ops(for kind: SchemaKind) -> [String] {
            (kind == .flow ? flowOps : actionOps) + groups
        }

        /// Beside `do` and `description`, which every row may have.
        static let keys: [String: Set<String>] = [
            "receive": ["peer", "program", "carries", "verifyWith", "via", "initiator"],
            "return": ["peer", "program", "carries", "signWith", "via", "initiator"],
            "call": ["peer", "program", "carries", "returns", "signWith", "verifyWith", "via", "initiator"],
            "send": ["peer", "program", "carries", "signWith", "via", "initiator"],
            "read": ["paths"],
            "write": ["paths"],
            "sign": ["key", "paths"],
            "verify": ["key", "paths"],
            "action": ["action", "carries", "via"],
            "sequence": ["steps"],
            "unordered": ["steps"],
            "switch": ["cases"],
            "loop": ["over", "steps"],
        ]
    }

    /// The keys this row has set, beside `do` and `description`.
    var setKeys: Set<String> {
        var out: Set<String> = []
        if !peers.isEmpty { out.insert("peer") }
        if program != nil { out.insert("program") }
        if !carries.isEmpty { out.insert("carries") }
        if !returns.isEmpty { out.insert("returns") }
        if !paths.isEmpty { out.insert("paths") }
        if key != nil { out.insert("key") }
        if signWith != nil { out.insert("signWith") }
        if verifyWith != nil { out.insert("verifyWith") }
        if via != nil { out.insert("via") }
        if initiator != nil { out.insert("initiator") }
        if action != nil { out.insert("action") }
        if when != nil { out.insert("when") }
        if over != nil { out.insert("over") }
        if !steps.isEmpty { out.insert("steps") }
        if !cases.isEmpty { out.insert("cases") }
        return out
    }

    /// Every path the row names and where: its peers and `action` name an action.
    /// Conditions are text; the `[[…]]` in them are the paths.
    func declared(at place: String) -> [(via: String, path: String, expects: SchemaKind?)] {
        var out: [(via: String, path: String, expects: SchemaKind?)] = []
        for (index, peer) in peers.enumerated() { out.append(("\(place).peers[\(index)]", peer, .action)) }
        if let action { out.append(("\(place).action", action, .action)) }
        for (key, list) in [("carries", carries), ("returns", returns), ("paths", paths)] {
            for (index, path) in list.enumerated() { out.append(("\(place).\(key)[\(index)]", path, nil)) }
        }
        for (key, value) in [("key", key), ("signWith", signWith), ("verifyWith", verifyWith)] {
            if let value { out.append(("\(place).\(key)", value, nil)) }
        }
        for (index, step) in steps.enumerated() { out += step.declared(at: "\(place).steps[\(index)]") }
        for (index, item) in cases.enumerated() { out += item.declared(at: "\(place).cases[\(index)]") }
        return out
    }

    /// The conditions, where they are — the text `[[…]]` is read from.
    func conditions(at place: String) -> [(via: String, text: String)] {
        var out: [(via: String, text: String)] = []
        if let when { out.append(("\(place).when", when)) }
        if let over { out.append(("\(place).over", over)) }
        for (index, step) in steps.enumerated() { out += step.conditions(at: "\(place).steps[\(index)]") }
        for (index, item) in cases.enumerated() { out += item.conditions(at: "\(place).cases[\(index)]") }
        return out
    }

    /// The same row with every path passed through `path` and every condition
    /// through `text` — how a rename reaches into rows.
    func mapped(path: (String) -> String, text: (String) -> String) -> Step {
        var out = self
        out.peers = peers.map(path)
        out.action = action.map(path)
        out.carries = carries.map(path)
        out.returns = returns.map(path)
        out.paths = paths.map(path)
        out.key = key.map(path)
        out.signWith = signWith.map(path)
        out.verifyWith = verifyWith.map(path)
        out.when = when.map(text)
        out.over = over.map(text)
        out.steps = steps.map { $0.mapped(path: path, text: text) }
        out.cases = cases.map { $0.mapped(path: path, text: text) }
        return out
    }
}

/// What starts an action: something received, a schedule, or the part starting up.
struct Trigger: Codable, Hashable, Sendable {
    var on: String
    /// Who sends it, in their lines' order; with the program, all it takes from.
    var peers: [String] = []
    var program: String?
    var carries: [String] = []
    var via: String?
    var initiator: String?
    var verifyWith: String?
    /// For `schedule`: how often, in words.
    var every: String?
    var description: String?

    init(on: String) { self.on = on }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        on = try c.decode(String.self, forKey: .on)
        peers = try c.decodeIfPresent([String].self, forKey: .peers) ?? []
        if let one = try c.decodeIfPresent(String.self, forKey: .peer), !peers.contains(one) { peers.append(one) }
        program = try c.decodeIfPresent(String.self, forKey: .program)
        carries = try c.decodeIfPresent([String].self, forKey: .carries) ?? []
        via = try c.decodeIfPresent(String.self, forKey: .via)
        initiator = try c.decodeIfPresent(String.self, forKey: .initiator)
        verifyWith = try c.decodeIfPresent(String.self, forKey: .verifyWith)
        every = try c.decodeIfPresent(String.self, forKey: .every)
        description = try c.decodeIfPresent(String.self, forKey: .description)
            ?? c.decodeIfPresent(String.self, forKey: .comment)
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(on, forKey: .on)
        if !peers.isEmpty { try c.encode(peers, forKey: .peers) }
        try c.encodeIfPresent(program, forKey: .program)
        if !carries.isEmpty { try c.encode(carries, forKey: .carries) }
        try c.encodeIfPresent(via, forKey: .via)
        try c.encodeIfPresent(initiator, forKey: .initiator)
        try c.encodeIfPresent(verifyWith, forKey: .verifyWith)
        try c.encodeIfPresent(every, forKey: .every)
        try c.encodeIfPresent(description, forKey: .description)
    }

    private enum CodingKeys: String, CodingKey {
        case on, peers, program, carries, via, initiator, verifyWith, every, description
        /// The name it had before, read only.
        case comment
        /// The one sender it named before, read only.
        case peer
    }

    func declared() -> [(via: String, path: String, expects: SchemaKind?)] {
        var out: [(via: String, path: String, expects: SchemaKind?)] = []
        for (index, peer) in peers.enumerated() { out.append(("trigger.peers[\(index)]", peer, .action)) }
        for (index, path) in carries.enumerated() { out.append(("trigger.carries[\(index)]", path, nil)) }
        if let verifyWith { out.append(("trigger.verifyWith", verifyWith, nil)) }
        return out
    }

    func mapped(path: (String) -> String) -> Trigger {
        var out = self
        out.peers = peers.map(path)
        out.carries = carries.map(path)
        out.verifyWith = verifyWith.map(path)
        return out
    }
}

extension Validate {
    /// Rows as the kind allows them, each with the keys its `do` takes and the
    /// ones it needs. Refused whole, with where and why, rather than stored and
    /// shown nowhere.
    static func steps(_ raw: [Step], for kind: SchemaKind) throws -> [Step] {
        let language = Language.current
        func say(_ resource: LocalizedStringResource) -> String { L10n.text(resource, in: language) }
        var count = 0
        func check(_ rows: [Step], at place: String, depth: Int) throws {
            guard depth <= 8 else { throw Invalid(say(.Nodes.problemNestingLimit(place))) }
            for (index, row) in rows.enumerated() {
                let here = "\(place)[\(index)]"
                count += 1
                guard count <= 300 else { throw Invalid(say(.Nodes.problemStepsLimit)) }
                let allowed = Step.Vocabulary.ops(for: kind)
                guard allowed.contains(row.op) else {
                    throw Invalid(doOneOf(here, allowed, row.op, in: language))
                }
                let keys = Step.Vocabulary.keys[row.op] ?? []
                let stray = row.setKeys.subtracting(keys)
                guard stray.isEmpty else {
                    let strays = stray.sorted().joined(separator: ", ")
                    throw Invalid(keys.isEmpty
                                  ? say(.Nodes.problemStrayNoKeys(here, row.op, strays, row.op))
                                  : say(.Nodes.problemStrayKeys(here, row.op, strays, keys.sorted().joined(separator: ", "))))
                }
                // What `via` may be is the project's (`ProjectDetail.vias`): the core checks it as it writes.
                if let who = row.initiator, !Step.Vocabulary.initiators.contains(who) {
                    throw Invalid(say(.Nodes.problemInitiator(here, who)))
                }
                // A receive or a return naming no one deals with whoever called it — the actions
                // whose call rows name it.
                switch row.op {
                case "call", "send":
                    guard !row.peers.isEmpty || row.program != nil else {
                        throw Invalid(say(.Nodes.problemNeedsPartner(here, row.op)))
                    }
                case "read", "write":
                    guard !row.paths.isEmpty else { throw Invalid(say(.Nodes.problemNeedsKey(here, row.op, "paths"))) }
                case "sign", "verify":
                    guard row.key != nil else { throw Invalid(say(.Nodes.problemNeedsKey(here, row.op, "key"))) }
                case "action":
                    guard row.action != nil else { throw Invalid(say(.Nodes.problemNeedsAction(here))) }
                case "sequence", "unordered":
                    guard !row.steps.isEmpty else { throw Invalid(say(.Nodes.problemNeedsKey(here, row.op, "steps"))) }
                    try check(row.steps, at: "\(here).steps", depth: depth + 1)
                case "loop":
                    guard row.over?.isEmpty == false else { throw Invalid(say(.Nodes.problemNeedsKey(here, "loop", "over"))) }
                    guard !row.steps.isEmpty else { throw Invalid(say(.Nodes.problemNeedsKey(here, "loop", "steps"))) }
                    try check(row.steps, at: "\(here).steps", depth: depth + 1)
                case "switch":
                    guard !row.cases.isEmpty else { throw Invalid(say(.Nodes.problemNeedsKey(here, "switch", "cases"))) }
                    for (index, item) in row.cases.enumerated() {
                        let place = "\(here).cases[\(index)]"
                        guard item.op.isEmpty, item.setKeys.isSubset(of: ["when", "steps"]) else {
                            throw Invalid(say(.Nodes.problemCaseShape(place)))
                        }
                        guard item.when?.isEmpty == false else { throw Invalid(say(.Nodes.problemCaseNeedsWhen(place))) }
                        // No rows: in that case, nothing is done.
                        try check(item.steps, at: "\(place).steps", depth: depth + 1)
                    }
                default:
                    break
                }
            }
        }
        try check(raw, at: "steps", depth: 0)
        return raw
    }

    /// What is said of a row's `do` that is not one of `allowed`.
    private static func doOneOf(_ here: String, _ allowed: [String], _ op: String, in language: String) -> String {
        let list = allowed.joined(separator: " / ")
        return op.isEmpty ? L10n.text(.Nodes.problemDoOneOfNone(here, list), in: language)
            : L10n.text(.Nodes.problemDoOneOfGot(here, list, op), in: language)
    }

    /// A process node's rows: the rows an action had, but straight — no group
    /// and no return, which are a flow's nodes — and a call only to a program outside
    /// the project: a flow is run by a run node, which waits for its end.
    static func processRows(_ rows: [Step], at place: String) throws {
        let language = Language.current
        for (index, row) in rows.enumerated() {
            let here = "\(place)[\(index)]"
            guard FlowNode.Vocabulary.processOps.contains(row.op) else {
                if row.op == "return" {
                    throw Invalid(L10n.text(.Nodes.problemProcessNoReturn(here), in: language))
                }
                if Step.Vocabulary.groups.contains(row.op) {
                    throw Invalid(L10n.text(.Nodes.problemProcessNoGroup(here, row.op), in: language))
                }
                throw Invalid(doOneOf(here, FlowNode.Vocabulary.processOps, row.op, in: language))
            }
            if row.op == "call", !row.peers.isEmpty {
                throw Invalid(L10n.text(.Nodes.problemProcessCallFlow(here), in: language))
            }
        }
        do { _ = try steps(rows, for: .action) } catch {
            throw Invalid(place.replacingOccurrences(of: ".steps", with: ".") + error.localizedDescription)
        }
    }

    static func trigger(_ raw: Trigger) throws -> Trigger {
        let language = Language.current
        func say(_ resource: LocalizedStringResource) -> String { L10n.text(resource, in: language) }
        guard Step.Vocabulary.triggers.contains(raw.on) else {
            throw Invalid(say(.Nodes.problemTriggerOnOneOf(Step.Vocabulary.triggers.joined(separator: " / "), raw.on)))
        }
        if let who = raw.initiator, !Step.Vocabulary.initiators.contains(who) {
            throw Invalid(say(.Nodes.problemInitiator("trigger", who)))
        }
        switch raw.on {
        case "receive":
            // Neither peers nor a program: whoever calls it.
            guard raw.every == nil else { throw Invalid(say(.Nodes.problemEveryOnlySchedule)) }
        case "schedule":
            guard raw.every?.isEmpty == false else { throw Invalid(say(.Nodes.problemScheduleNeedsEvery)) }
            guard raw.peers.isEmpty, raw.program == nil, raw.carries.isEmpty else {
                throw Invalid(say(.Nodes.problemScheduleNoPartner))
            }
        default:
            guard raw.peers.isEmpty, raw.program == nil, raw.carries.isEmpty, raw.every == nil else {
                throw Invalid(say(.Nodes.problemStartupOnlyDescription))
            }
        }
        return raw
    }
}
