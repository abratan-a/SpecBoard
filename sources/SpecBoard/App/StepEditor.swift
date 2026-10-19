import SwiftUI

/// A row of a flow's process node, picked on its diagram, edited in the property panel
/// (overview §13): its `do`, and the keys that `do` takes — the flow it sends
/// to or receives from picked from the project's flows, paths and keys from the project's
/// cards and fields or typed. Every change saves the node's rows, through the same
/// check an agent's write goes through; what it refuses stays a draft.
struct StepEditor: View {
    var model: AppModel
    var flow: SchemaNode
    /// The process node whose row it is.
    var node: String
    var path: String
    var editable: Bool

    /// What a key means, for the (i) beside it: the words given, then the glossary's.
    static func help(_ key: String, _ words: String = "") -> String {
        [words, Glossary.term(key)?.meaning ?? ""].filter { !$0.isEmpty }.joined(separator: L10n[.Nodes.helpJoiner])
    }

    /// The order the keys are offered in — by the key MCP writes, as the table names
    /// them — with what each is, in words, for the (i).
    static var keyOrder: [(key: String, title: String)] {
        [
            ("peer", L10n[.Nodes.wordPartnerFlow]), ("program", L10n[.Nodes.wordOutsideTool]),
            ("carries", L10n[.Nodes.wordCarries]), ("returns", L10n[.Nodes.wordComesBack]),
            ("paths", L10n[.Nodes.wordReadWritePlaces]), ("key", L10n[.Nodes.wordKey]), ("signWith", L10n[.Nodes.wordSignKey]),
            ("verifyWith", L10n[.Nodes.wordVerifyKey]), ("via", L10n[.Nodes.wordVia]),
            ("initiator", L10n[.Nodes.wordInitiator]),
        ]
    }

    /// The keys a row may carry here: a call goes to a program only — one to a flow
    /// of the project is a run node.
    static func keys(_ op: String) -> Set<String> {
        let keys = Step.Vocabulary.keys[op] ?? []
        return op == "call" ? keys.subtracting(["peer"]) : keys
    }

    var body: some View {
        let rows = model.rows(of: flow, node: node)
        VStack(alignment: .leading, spacing: 0) {
            if let draft = model.rowsDraft, draft.schemaId == flow.id, draft.node == node {
                Text(draft.problem).font(.app(11)).foregroundStyle(.orange)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.horizontal, 18).padding(.vertical, 6)
            }
            if let step = StepEditing.step(at: path, in: rows) {
                PropertyCategory(title: L10n[.Common.wordRow]) { stepEditor(step) }
            } else {
                Text(L10n[.Nodes.emptyRowGone]).font(.app(11)).foregroundStyle(.secondary)
            }
        }
    }

    // MARK: - A row

    @ViewBuilder
    private func stepEditor(_ step: Step) -> some View {
        block("do", help: StepEditor.help(step.op, L10n[.Nodes.helpRow])) {
            if editable {
                Picker("", selection: Binding(get: { step.op }, set: { op in set { $0 = StepEditor.retyped($0, to: op) } })) {
                    ForEach(FlowNode.Vocabulary.processOps, id: \.self) { Text($0).tag($0) }
                }
                .labelsHidden().fixedSize()
            } else {
                Text(step.op).font(.app(12, design: .monospaced))
            }
            if let meaning = Glossary.term(step.op)?.meaning {
                Text(meaning).font(.app(10.5)).foregroundStyle(.secondary)
            }
        }
        let keys = Self.keys(step.op)
        ForEach(Self.keyOrder.filter { keys.contains($0.key) }, id: \.key) { entry in
            block(entry.key, help: Self.help(entry.key, entry.title)) { editor(entry.key, step) }
        }
    }

    @ViewBuilder
    private func editor(_ key: String, _ step: Step) -> some View {
        switch key {
        case "peer":
            PathList(model: model, owner: flow, paths: step.peers, editable: editable, kinds: [.flow],
                     each: L10n[.Nodes.wordPartnerFlow],
                     adding: step.op == "receive" ? L10n[.Nodes.placeholderAddPartnerReceive]
                         : L10n[.Nodes.placeholderAddPartner]) { list in set { $0.peers = list } }
        case "program":
            text(step.program, L10n[.Nodes.placeholderProgramRow]) { value in set { $0.program = value } }
        case "carries":
            PathList(model: model, owner: flow, paths: step.carries, editable: editable) { list in set { $0.carries = list } }
        case "returns":
            PathList(model: model, owner: flow, paths: step.returns, editable: editable) { list in set { $0.returns = list } }
        case "paths":
            PathList(model: model, owner: flow, paths: step.paths, editable: editable) { list in set { $0.paths = list } }
        case "key":
            PathField(model: model, owner: flow, value: step.key, kinds: nil, editable: editable,
                      placeholder: L10n[.Nodes.placeholderKeyPath]) { value in set { $0.key = value } }
        case "signWith":
            PathField(model: model, owner: flow, value: step.signWith, kinds: nil, editable: editable,
                      placeholder: L10n[.Nodes.placeholderPathOf(L10n[.Nodes.wordSignKey])]) { value in set { $0.signWith = value } }
        case "verifyWith":
            PathField(model: model, owner: flow, value: step.verifyWith, kinds: nil, editable: editable,
                      placeholder: L10n[.Nodes.placeholderPathOf(L10n[.Nodes.wordVerifyKey])]) { value in set { $0.verifyWith = value } }
        case "via":
            choice(step.via, model.detail?.vias ?? [], none: L10n[.Nodes.wordWithinPart]) { value in set { $0.via = value } }
        case "initiator":
            choice(step.initiator, Step.Vocabulary.initiators, none: L10n[.Nodes.wordNotWritten]) { value in set { $0.initiator = value } }
        default:
            EmptyView()
        }
    }

    // MARK: - Pieces

    /// A key of the row as a row of the property table; what it means at the panel's foot.
    private func block<Content: View>(_ title: String, help: String? = nil,
                                      @ViewBuilder _ content: () -> Content) -> some View {
        PropertyRow(title, help: help ?? StepEditor.help(title)) {
            VStack(alignment: .leading, spacing: 4) { content() }
        }
    }

    @ViewBuilder
    private func text(_ value: String?, _ placeholder: String, _ commit: @escaping (String?) -> Void) -> some View {
        if editable {
            CommitField(placeholder, text: value ?? "") { commit($0.isEmpty ? nil : $0) }
                .id("\(flow.id)|\(node)|\(path)|\(placeholder)|\(value ?? "")")
        } else {
            Text(value ?? "—").font(.app(12)).foregroundStyle(value == nil ? .tertiary : .primary)
        }
    }

    @ViewBuilder
    private func choice(_ value: String?, _ options: [String], none: String,
                        _ commit: @escaping (String?) -> Void) -> some View {
        if editable {
            Picker("", selection: Binding(get: { value ?? "" }, set: { commit($0.isEmpty ? nil : $0) })) {
                Text(none).tag("")
                ForEach(options, id: \.self) { Text($0).tag($0) }
            }
            .labelsHidden().fixedSize()
        } else {
            Text(value ?? none).font(.app(12, design: .monospaced))
        }
    }

    private func set(_ edit: (inout Step) -> Void) {
        guard var row = StepEditing.step(at: path, in: model.rows(of: flow, node: node)) else { return }
        edit(&row)
        model.editRows(flow.id, node: node) { $0 = StepEditing.replacing(path, with: row, in: $0) }
    }

    /// The row with another `do`: what the new one cannot hold is let go of.
    static func retyped(_ row: Step, to op: String) -> Step {
        var out = row
        out.op = op
        let keys = keys(op)
        if !keys.contains("peer") { out.peers = [] }
        if !keys.contains("program") { out.program = nil }
        if !keys.contains("carries") { out.carries = [] }
        if !keys.contains("returns") { out.returns = [] }
        if !keys.contains("paths") { out.paths = [] }
        if !keys.contains("key") { out.key = nil }
        if !keys.contains("signWith") { out.signWith = nil }
        if !keys.contains("verifyWith") { out.verifyWith = nil }
        if !keys.contains("via") { out.via = nil }
        if !keys.contains("initiator") { out.initiator = nil }
        out.action = nil
        out.over = nil
        return out
    }
}

// MARK: - Paths

/// One path, typed or picked from the project, and whether it lands anywhere.
struct PathField: View {
    var model: AppModel
    /// Whose path it is: it is read from that card's section.
    var owner: SchemaNode
    var value: String?
    /// The kinds of card it may name; nil for any card or field.
    var kinds: Set<SchemaKind>?
    var editable: Bool
    var placeholder: String
    var commit: (String?) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            if editable {
                HStack(spacing: 4) {
                    CommitField(placeholder, text: value ?? "", monospaced: true) { commit($0.isEmpty ? nil : $0) }
                        .id("\(owner.id)|\(placeholder)|\(value ?? "")")
                    PathPickButton(title: L10n[.Nodes.buttonChooseFromProject], systemImage: "list.bullet", detail: model.detail, owner: owner,
                                   wanted: kinds.map { PathWanted(cards: $0) } ?? .data) { commit($0) }
                        .buttonStyle(.borderless)
                }
            } else {
                Text(value ?? "—").font(.app(12, design: .monospaced)).textSelection(.enabled)
            }
            if let value, let detail = model.detail {
                let hit = resolvePath(detail, value, from: owner)
                if !hit.ok {
                    Text(hit.hint ?? L10n[.Nodes.emptyNotFound]).font(.app(10.5)).foregroundStyle(.orange)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
        }
    }
}

/// Several paths: each can be changed or taken off, and one more added.
struct PathList: View {
    var model: AppModel
    var owner: SchemaNode
    var paths: [String]
    var editable: Bool
    /// The kinds of card each may name; nil for any card or field.
    var kinds: Set<SchemaKind>? = nil
    /// What a path in it is, and what the field that adds one says; nil for the plain words for a path.
    var each: String? = nil
    var adding: String? = nil
    var commit: ([String]) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            ForEach(Array(paths.enumerated()), id: \.offset) { index, path in
                HStack(alignment: .top, spacing: 4) {
                    PathField(model: model, owner: owner, value: path, kinds: kinds, editable: editable,
                              placeholder: each ?? L10n[.Nodes.placeholderPath]) { value in
                        var next = paths
                        if let value { next[index] = value } else { next.remove(at: index) }
                        commit(next)
                    }
                    if editable {
                        Button { var next = paths; next.remove(at: index); commit(next) } label: {
                            Image(systemName: "minus.circle")
                        }
                        .buttonStyle(.borderless)
                        .help(L10n[.Common.actionRemove])
                    }
                }
            }
            if editable {
                PathField(model: model, owner: owner, value: nil, kinds: kinds, editable: true,
                          placeholder: adding ?? L10n[.Nodes.placeholderAddPath]) { value in
                    if let value { commit(paths + [value]) }
                }
                .id("\(owner.id)|add|\(paths.count)")
            } else if paths.isEmpty {
                Text("—").foregroundStyle(.tertiary)
            }
        }
    }
}
