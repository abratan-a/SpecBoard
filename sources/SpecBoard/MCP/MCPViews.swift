import Foundation

/// What the window reads of a path the way MCP names it (`locate`), and a flow's row as
/// JSON (`stepJSON`). The MCP tools themselves are the core's (`sources/core/src/tools/`).
enum MCPViews {

    /// A step as it was written, `do` first. Keys it does not use are left out.
    static func stepJSON(_ step: Step) -> OJSON {
        var out = OrderedObject()
        if !step.op.isEmpty { out["do"] = .string(step.op) }
        if !step.peers.isEmpty { out["peers"] = .string(step.peers) }
        if let program = step.program { out["program"] = .string(program) }
        if let action = step.action { out["action"] = .string(action) }
        if !step.carries.isEmpty { out["carries"] = .string(step.carries) }
        if !step.returns.isEmpty { out["returns"] = .string(step.returns) }
        if !step.paths.isEmpty { out["paths"] = .string(step.paths) }
        if let key = step.key { out["key"] = .string(key) }
        if let key = step.signWith { out["signWith"] = .string(key) }
        if let key = step.verifyWith { out["verifyWith"] = .string(key) }
        if let via = step.via { out["via"] = .string(via) }
        if let who = step.initiator { out["initiator"] = .string(who) }
        if let when = step.when { out["when"] = .string(when) }
        if let over = step.over { out["over"] = .string(over) }
        if let text = step.description { out["description"] = .string(text) }
        if !step.steps.isEmpty { out["steps"] = .array(step.steps.map(stepJSON)) }
        if !step.cases.isEmpty { out["cases"] = .array(step.cases.map(stepJSON)) }
        return .object(out)
    }

    static func triggerJSON(_ trigger: Trigger) -> OJSON {
        var out = OrderedObject([("on", .string(trigger.on))])
        if !trigger.peers.isEmpty { out["peers"] = .string(trigger.peers) }
        if let program = trigger.program { out["program"] = .string(program) }
        if !trigger.carries.isEmpty { out["carries"] = .string(trigger.carries) }
        if let via = trigger.via { out["via"] = .string(via) }
        if let who = trigger.initiator { out["initiator"] = .string(who) }
        if let key = trigger.verifyWith { out["verifyWith"] = .string(key) }
        if let every = trigger.every { out["every"] = .string(every) }
        if let text = trigger.description { out["description"] = .string(text) }
        return .object(out)
    }

    // MARK: - Path walking

    enum Target {
        case schema(SchemaNode)
        /// A field, with the card that holds it and the names from that card to it.
        case field(SchemaNode, FieldNode, [String])
        /// A row of a formatdoc, by its names from the top.
        case formatRow(SchemaNode, [String])
        /// A node of a flow.
        case node(SchemaNode, FlowNode)
        /// A custom card's row, by its names from the top (`specboard://SpecBoard/Docs/Design:DesignBoard` §3).
        case customRow(SchemaNode, [String])
    }

    /// Walks a dotted path, saying what is actually at the level that failed.
    ///
    /// A reference is *not* stepped through: `User.org.name` does not reach into
    /// `Org`, because `org`'s own JSON is where the path ends as far as this project
    /// is concerned. Following it would make one path mean two different things
    /// depending on whether the target still existed.
    /// `base` is the section a path without one is read from: the call's `section`,
    /// or nil when it gave none — fine in a project with one section, refused with a
    /// list of where the name does exist in a project with several.
    static func locate(_ detail: ProjectDetail, _ path: String, base: String? = nil) throws -> Target {
        guard let written = WrittenPath(path) else {
            throw MCPError(.AppControl.errorEmptyPath)
        }
        let directory = detail.directory
        // Not the whole list when it is missing. At 163 schemas that is 2.5KB of
        // names, which contradicts this server's own instruction not to read the
        // project whole — and a caller who mistyped needs the near misses, not the census.
        let schema: SchemaNode
        switch directory.find(written.card, from: base) {
        case .schema(let found): schema = found
        case .failure(let why): throw MCPError(why)
        }
        var byId: [String: SchemaNode] = [:]
        for card in detail.schemas { byId[card.id] = card }

        switch CardWalk.walk(schema, written.names, cards: byId, throughRefs: false) {
        case .success(let walk):
            switch walk.place {
            case .card: return .schema(schema)
            case .formatRow(let names): return .formatRow(schema, names)
            case .node(let node): return .node(schema, node)
            case .field(let field): return .field(walk.holder, field, walk.within)
            case .customRow(let names): return .customRow(schema, names)
            }
        case .failure(let miss):
            let at = miss.holder.kind == .instance
                ? directory.full(miss.holder, row: miss.walked.joined(separator: "."))
                : directory.full(miss.holder, miss.walked)
            /// What there is, or a word saying there is none.
            func there(_ none: LocalizedStringResource) -> String {
                miss.there.isEmpty ? L10n.text(none, in: AppControl.answerLanguage)
                    : cappedNames(miss.there.map(PathText.quote))
            }
            switch miss.kind {
            case .formatRow:
                throw miss.walked.isEmpty || FormatRows.row(at: miss.walked, in: miss.holder.rows)?.kind == .item
                    ? MCPError(.AppControl.errorRowNotFound(path, at, there(.AppControl.listNoRows)))
                    : MCPError(.AppControl.errorNotAnItemRow(path, at))
            case .node:
                throw miss.walked.isEmpty
                    ? MCPError(.AppControl.errorNodeNotFound(path, at, there(.AppControl.listNoNodes)))
                    : MCPError(.AppControl.errorInsideNode(path, at))
            case .field:
                throw MCPError(.AppControl.errorFieldNotFound(path, at, there(.AppControl.listNoFields)))
            case .customRow:
                throw MCPError(.AppControl.errorCustomRowNotFound(path, at, there(.AppControl.listNoRows)))
            }
        }
    }

}
