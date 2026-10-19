import Foundation

/// What an action's and a flow's rows mean for the rest of the project: the places they
/// read and write, the keys they sign and check with, the flows they hand on to — read
/// for the impact of a change (`Impact`), not drawn: the lines are the core's
/// (`project_detail`'s `relations`).
///
/// An action in the old form says the same in `uses`, `signedBy`, `verifiedBy` and
/// `next`. Those are read only while it has no rows; once it has, they are left as
/// they are and not read.
extension SchemaNode {
    /// Whether it says what it does in rows. Then the old fields are not read.
    var speaksInRows: Bool { kind.hasSteps && (!steps.isEmpty || trigger != nil || !nodes.isEmpty) }

    /// Every row with the step it is and the case it sits in, in card order.
    private func eachRow(_ body: (_ row: String, _ step: Step, _ when: String?) -> Void) {
        func walk(_ steps: [Step], _ prefix: String, _ when: String?) {
            for (index, step) in steps.enumerated() {
                let row = "\(prefix).\(index)"
                body(row, step, when)
                walk(step.steps, row, when)
                for (caseIndex, item) in step.cases.enumerated() {
                    walk(item.steps, "\(row).c\(caseIndex)", item.when ?? when)
                }
            }
        }
        walk(steps, "#p", nil)
        // A flow's: what its begin receives, each process node's rows, and what
        // an end answers with — each from its node's row, a process node's rows below it.
        for node in nodes {
            if let row = node.receiveRow { body(SchemaNode.nodeRow(node.name), row, nil) }
            for (index, step) in node.steps.enumerated() where node.kind == "process" {
                body(SchemaNode.stepRow(node.name, index), step, nil)
            }
            if let row = node.returnRow { body(SchemaNode.nodeRow(node.name), row, nil) }
        }
    }

    /// The places it reads and writes, as `uses` said them: from its rows when it
    /// has any. What it receives, sends and gets back counts as read — it has to
    /// read it to pass it on — and so does what it signs or checks.
    var usesInEffect: [ModuleUse] {
        guard speaksInRows else { return uses }
        var out: [ModuleUse] = []
        func add(_ paths: [String], _ role: ModuleUse.Role, _ note: String?) {
            out += paths.map { ModuleUse(path: $0, role: role, note: note) }
        }
        if let trigger { add(trigger.carries, .read, trigger.description) }
        // What a run hands on and gets back.
        for node in nodes where node.kind == "run" {
            add(node.carries, .read, node.description)
            add(node.returns, .read, node.description)
        }
        eachRow { _, step, _ in
            switch step.op {
            case "read": add(step.paths, .read, step.description)
            case "write": add(step.paths, .write, step.description)
            case "receive", "send", "return": add(step.carries, .read, step.description)
            case "call":
                add(step.carries, .read, step.description)
                add(step.returns, .read, step.description)
            case "sign", "verify": add(step.paths, .read, step.description)
            default: break
            }
        }
        return out
    }

    /// The keys it signs with: `sign` rows, and what a send or a return signs with.
    var signingKeysInEffect: [String] {
        guard speaksInRows else { return signedBy }
        var out: [String] = []
        eachRow { _, step, _ in
            if step.op == "sign", let key = step.key { out.append(key) }
            if let key = step.signWith { out.append(key) }
        }
        out += nodes.compactMap { $0.kind == "run" ? $0.signWith : nil }
        return out
    }

    /// The keys it checks with: `verify` rows, and what a receive checks with.
    var verifyingKeysInEffect: [String] {
        guard speaksInRows else { return verifiedBy }
        var out: [String] = trigger?.verifyWith.map { [$0] } ?? []
        eachRow { _, step, _ in
            if step.op == "verify", let key = step.key { out.append(key) }
            if let key = step.verifyWith { out.append(key) }
        }
        out += nodes.compactMap { $0.kind == "run" ? $0.verifyWith : nil }
        return out
    }

    /// The actions it hands on to: each of a send's or a call's peers, with the case it is
    /// taken in.
    var nextInEffect: [ActionNext] {
        guard speaksInRows else { return next }
        var out: [ActionNext] = []
        eachRow { _, step, when in
            if ["send", "call"].contains(step.op) {
                out += step.peers.map { ActionNext(path: $0, when: when) }
            }
        }
        return out
    }

    /// Old fields it still holds while it speaks in rows: not read, and worth
    /// clearing so the two cannot be taken for one another.
    var unreadOldFields: [String] {
        guard speaksInRows else { return [] }
        return [("uses", !uses.isEmpty), ("signedBy", !signedBy.isEmpty),
                ("verifiedBy", !verifiedBy.isEmpty), ("next", !next.isEmpty)]
            .filter(\.1).map(\.0)
    }
}

extension Step {
    /// An action's old fields as rows (`Database.rowing`): its reads, writes and
    /// keys in any order, then the actions it hands on to — plainly, or under the
    /// condition written for them.
    static func rows(uses: [ModuleUse], signedBy: [String], verifiedBy: [String],
                     next: [ActionNext]) -> [Step] {
        var inner: [Step] = []
        for use in uses {
            var row = Step(op: use.role == .read ? "read" : "write")
            row.paths = [use.path]
            row.description = use.note
            inner.append(row)
        }
        for key in signedBy {
            var row = Step(op: "sign")
            row.key = key
            inner.append(row)
        }
        for key in verifiedBy {
            var row = Step(op: "verify")
            row.key = key
            inner.append(row)
        }
        var out: [Step] = []
        if inner.count > 1 {
            var group = Step(op: "unordered")
            group.steps = inner
            out.append(group)
        } else {
            out += inner
        }
        func send(_ peer: String) -> Step {
            var row = Step(op: "send")
            row.peers = [peer]
            return row
        }
        out += next.filter { $0.when == nil }.map { send($0.path) }
        let conditional = next.filter { $0.when != nil }
        if !conditional.isEmpty {
            var choice = Step(op: "switch")
            choice.cases = conditional.map { item in
                // A case is a condition and its rows, with no `do` of its own.
                var branch = Step(op: "")
                branch.when = item.when
                branch.steps = [send(item.path)]
                return branch
            }
            out.append(choice)
        }
        return out
    }
}

extension FlowNode {
    /// Whom a start, a row or an answer deals with, said in one: the flows it names, then the
    /// program; none when it names no one.
    static func partners(_ peers: [String], _ program: String?, path: (String) -> String = { $0 },
                         in language: String = Language.current) -> String? {
        let names = peers.map(path) + (program.map { [$0] } ?? [])
        return names.isEmpty ? nil : names.joined(separator: L10n.text(.Common.separatorList, in: language))
    }

    /// A begin on receive, as the row it would be in an action: for the lines,
    /// the uses and the sequence diagram, which read rows.
    var receiveRow: Step? {
        guard kind == "begin", trigger == "receive" else { return nil }
        var row = Step(op: "receive")
        row.peers = peers
        row.program = program
        row.carries = carries
        row.via = via
        row.initiator = initiator
        row.verifyWith = verifyWith
        return row
    }

    /// An end's answer, as the return row it would be in an action.
    var returnRow: Step? {
        guard kind == "end", let reply else { return nil }
        var row = Step(op: "return")
        row.carries = reply.carries
        row.peers = reply.peers
        row.program = reply.program
        row.signWith = reply.signWith
        row.description = description
        return row
    }

    /// A run, as the call row it would be in an action: the flow it runs, what
    /// it hands on and what comes back.
    var callRow: Step? {
        guard kind == "run", let target else { return nil }
        var row = Step(op: "call")
        row.peers = [target]
        row.carries = carries
        row.returns = returns
        row.via = via
        row.signWith = signWith
        row.verifyWith = verifyWith
        row.description = description
        return row
    }
}
