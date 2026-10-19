import SwiftUI

/// A flow's node picked on its diagram, edited in the property panel (overview §13):
/// every choice from a list — the nodes to go to, the actions and flows to run — and
/// only names and conditions typed.
struct NodeInspector: View {
    var model: AppModel
    var flow: SchemaNode
    var detail: ProjectDetail
    var node: FlowNode
    var editable = true

    private var others: [String] { flow.nodes.map(\.name).filter { $0 != node.name } }
    private var home: String { detail.directory.sectionId(of: flow) }

    /// What a run may run, written as the flow reads it.
    /// A path as this flow's tree reads it: without the tree when it is this one.
    private func shown(_ path: String) -> String {
        SchemaDirectory.shown(text: path, in: detail.directory.treeWord(of: flow))
    }

    var body: some View {
        PropertyCategory(title: L10n[.Flow.wordNode]) {
            PropertyRow("kind", help: FlowNode.Vocabulary.holding(node.kind)) {
                Text(L10n[.Flow.kindWithId(FlowNode.Vocabulary.label(node.kind), node.kind)]).foregroundStyle(.secondary)
            }
            ForEach(FlowEditing.unfinished(node, in: flow.nodes), id: \.self) { problem in
                Label(problem, systemImage: "exclamationmark.circle").font(.app(10.5)).foregroundStyle(.orange)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.horizontal, 18).padding(.vertical, 3)
            }
            row("name", help: L10n[.Nodes.helpNodeName]) {
                CommitField(L10n[.Common.wordName], text: node.name, monospaced: true) { name in
                    guard let renamed = try? FlowEditing.rename(node.name, to: name, in: flow.nodes) else {
                        model.lastError = L10n[.Nodes.errorBadNodeName]
                        return
                    }
                    model.setFlowNodes(flow.id, renamed, renamed: (node.name, name))
                    model.selectNode(flow.id, name)
                }
                .id("\(flow.id)|\(node.name)|name")
            }
            row("label", help: L10n[.Nodes.helpNodeLabel]) {
                CommitField(L10n[.Nodes.placeholderAnyLanguage], text: node.label ?? "") { change { $0.label = $1.isEmpty ? nil : $1 }($0) }
                    .id("\(flow.id)|\(node.name)|label")
            }
            fields
            HStack(spacing: 8) {
                if node.kind == "run", let target = node.target {
                    Button(L10n[.Nodes.buttonOpenTarget(shown(target))]) { model.open(path: target, from: home) }
                }
                Spacer()
                Button(L10n[.Flow.menuDeleteNode], role: .destructive) {
                    model.clearRow()
                    model.setFlowNodes(flow.id, FlowEditing.remove(node.name, from: flow.nodes))
                }
                .help(L10n[.Nodes.helpDeleteNode])
            }
            .controlSize(.small)
            .padding(.horizontal, 18).padding(.vertical, 6)
        }
    }

    @ViewBuilder
    private var fields: some View {
        switch node.kind {
        case "begin":
            row("trigger", help: L10n[.Nodes.helpTrigger]) {
                choice(node.trigger, FlowNode.Vocabulary.triggers, words: NodeInspector.triggerWords) { value in
                    change { node, _ in
                        node.trigger = value
                        if value != "event" { node.event = nil }
                        if value != "schedule" { node.every = nil }
                        if value != "receive" {
                            node.peers = []; node.program = nil; node.carries = []
                            node.via = nil; node.initiator = nil; node.verifyWith = nil
                        }
                    }("")
                }
            }
            if node.trigger == "receive" { receiveRows }
            if node.trigger == "event" { eventRows }
            if node.trigger == "schedule" {
                row("every", help: L10n[.Nodes.helpEvery]) { CommitField(L10n[.Nodes.placeholderEvery], text: node.every ?? "") { change { $0.every = $1.isEmpty ? nil : $1 }($0) } }
            }
            nextRows
        case "process":
            processRows
            nextRows
        case "run":
            row("target", help: L10n[.Nodes.helpRunTarget]) {
                chooser(node.target, PathWanted(cards: [.flow])) { value in change { $0.target = value }("") }
            }
            row("carries", help: StepEditor.help("carries", L10n[.Nodes.wordHandsOver])) {
                PathList(model: model, owner: flow, paths: node.carries, editable: editable) { list in change { $0.carries = list }("") }
            }
            row("returns", help: StepEditor.help("returns", L10n[.Nodes.wordComesBack])) {
                PathList(model: model, owner: flow, paths: node.returns, editable: editable) { list in change { $0.returns = list }("") }
            }
            row("via", help: StepEditor.help("via", L10n[.Nodes.wordVia])) {
                choice(node.via, detail.vias, words: [:], none: L10n[.Nodes.wordWithinPart]) { value in change { $0.via = value }("") }
            }
            keyRow("signWith", node.signWith, L10n[.Nodes.wordSignKey]) { value in change { $0.signWith = value }("") }
            keyRow("verifyWith", node.verifyWith, L10n[.Nodes.wordVerifyKey]) { value in change { $0.verifyWith = value }("") }
            nextRows
            way("onError", node.onError, optional: true, help: L10n[.Nodes.helpOnError]) { value in change { $0.onError = value }("") }
        case "eval":
            row("value", help: L10n[.Nodes.helpValue]) {
                CommitField(L10n[.Nodes.placeholderValuePath], text: node.value ?? "", monospaced: true) {
                    change { $0.value = $1.isEmpty ? nil : $1 }($0)
                }
            }
            ForEach(Array(node.branches.enumerated()), id: \.offset) { index, branch in
                VStack(alignment: .leading, spacing: 4) {
                    HStack {
                        CommitField(L10n[.Nodes.placeholderCondition], text: branch.when) { when in
                            change { $0.branches[index].when = $1 }(when)
                        }
                        Button { change { $0.branches.remove(at: index) }("") } label: { Image(systemName: "minus.circle") }
                            .buttonStyle(.borderless)
                    }
                    way("next", branch.next, help: L10n[.Nodes.helpBranchNext]) { value in change { if let value { $0.branches[index].next = value } }("") }
                }
                .padding(6).background(RoundedRectangle(cornerRadius: 6).fill(Color.secondary.opacity(0.06)))
            }
            Button(L10n[.Nodes.buttonAddBranch]) {
                guard let first = others.first else { return }
                change { $0.branches.append(FlowBranch(when: L10n[.Nodes.defaultCondition], next: first)) }("")
            }
            .controlSize(.small).disabled(others.isEmpty)
            way("else", node.otherwise, help: L10n[.Nodes.helpElse]) { value in change { $0.otherwise = value }("") }
        case "wait":
            row("for", help: L10n[.Nodes.helpWaitFor]) {
                choice(node.waitFor, FlowNode.Vocabulary.waits, words: ["approval": L10n[.Nodes.waitApproval], "timer": L10n[.Nodes.waitTimer],
                                                                      "event": L10n[.Nodes.wordOtherFlowEnd]]) { value in
                    change { node, _ in node.waitFor = value; if value != "event" { node.event = nil } }("")
                }
            }
            if node.waitFor == "event" { eventRows }
            row("timeout", help: L10n[.Nodes.helpTimeout]) { CommitField(L10n[.Nodes.placeholderTimeout], text: node.timeout ?? "") { change { node, text in
                node.timeout = text.isEmpty ? nil : text
                if text.isEmpty { node.onTimeout = nil }
            }($0) } }
            if node.timeout != nil {
                way("onTimeout", node.onTimeout, optional: true, help: L10n[.Nodes.helpOnTimeout]) { value in change { $0.onTimeout = value }("") }
            }
            nextRows
        case "join":
            row("join", help: L10n[.Nodes.helpJoin]) {
                choice(node.join, FlowNode.Vocabulary.joins, words: ["all": L10n[.Nodes.joinAll], "any": L10n[.Nodes.joinAny],
                                                                    "first": L10n[.Nodes.joinFirst]]) { value in
                    change { $0.join = value }("")
                }
            }
            row("partial", help: L10n[.Nodes.helpPartial]) {
                choice(node.partial ?? "fail", FlowNode.Vocabulary.partials,
                       words: ["fail": L10n[.Nodes.partialFail], "continue": L10n[.Nodes.partialContinue]]) { value in
                    change { $0.partial = value }("")
                }
            }
            nextRows
        default:
            row("outcome", help: L10n[.Nodes.helpOutcome]) {
                choice(node.outcome, FlowNode.Vocabulary.outcomes, words: NodeInspector.outcomeWords) { value in
                    change { $0.outcome = value }("")
                }
            }
            returnRows
        }
    }

    static var triggerWords: [String: String] {
        ["manual": L10n[.Nodes.triggerManual], "receive": L10n[.Nodes.triggerReceive], "event": L10n[.Nodes.wordOtherFlowEnd],
         "schedule": L10n[.Nodes.triggerSchedule], "startup": L10n[.Nodes.triggerStartup]]
    }

    static var outcomeWords: [String: String] {
        ["success": L10n[.Nodes.outcomeSuccess], "failure": L10n[.Nodes.outcomeFailure], "cancelled": L10n[.Nodes.outcomeCancelled]]
    }

    /// What a begin that receives receives, and from whom.
    @ViewBuilder
    private var receiveRows: some View {
        row("peer", help: StepEditor.help("peer", L10n[.Nodes.helpPeerReceive])) {
            PathList(model: model, owner: flow, paths: node.peers, editable: editable, kinds: [.flow],
                     each: L10n[.Nodes.wordPartnerFlow], adding: L10n[.Nodes.placeholderAddPartnerCaller]) { list in change { $0.peers = list }("") }
        }
        row("program", help: StepEditor.help("program", L10n[.Nodes.wordOutsideTool])) {
            CommitField(L10n[.Nodes.placeholderProgramBegin], text: node.program ?? "") { change { $0.program = $1.isEmpty ? nil : $1 }($0) }
                .id("\(flow.id)|\(node.name)|program")
        }
        row("carries", help: StepEditor.help("carries", L10n[.Nodes.wordReceives])) {
            PathList(model: model, owner: flow, paths: node.carries, editable: editable) { list in change { $0.carries = list }("") }
        }
        row("via", help: StepEditor.help("via", L10n[.Nodes.wordVia])) {
            choice(node.via, detail.vias, words: [:], none: L10n[.Nodes.wordWithinPart]) { value in change { $0.via = value }("") }
        }
        row("initiator", help: StepEditor.help("initiator", L10n[.Nodes.wordInitiator])) {
            choice(node.initiator, Step.Vocabulary.initiators, words: [:], none: L10n[.Nodes.wordNotWritten]) { value in
                change { $0.initiator = value }("")
            }
        }
        keyRow("verifyWith", node.verifyWith, L10n[.Nodes.wordVerifyKey]) { value in change { $0.verifyWith = value }("") }
    }

    /// A process node's rows: picked to be edited, added from the menu.
    @ViewBuilder
    private var processRows: some View {
        let steps = model.rows(of: flow, node: node.name)
        row("steps", help: L10n[.Nodes.helpSteps]) {
            ForEach(Array(steps.enumerated()), id: \.offset) { index, step in
                Button {
                    model.selectRow(flow.id, .nodeStep(node.name, StepEditing.path([.step(index)])))
                } label: {
                    let shown = StepRow(step).shown(in: detail.directory.treeWord(of: flow))
                    HStack(spacing: 6) {
                        Text("\(index + 1). \(shown.label)").font(.app(11, design: .monospaced)).foregroundStyle(.secondary)
                        Text(shown.text.isEmpty ? "—" : shown.text).font(.app(11.5, design: .monospaced)).lineLimit(1)
                        Spacer(minLength: 0)
                    }
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
            Menu(L10n[.Flow.menuAddRow]) {
                ForEach(FlowNode.Vocabulary.processOps, id: \.self) { op in
                    Button(op) {
                        model.changeRows(flow.id, node: node.name, .add(op: op, at: steps.count))
                        model.selectRow(flow.id, .nodeStep(node.name, StepEditing.path([.step(steps.count)])))
                    }
                    .help(Glossary.term(op)?.meaning ?? "")
                }
            }
            .fixedSize().controlSize(.small)
        }
    }

    /// What an end hands back to whoever called the flow.
    @ViewBuilder
    private var returnRows: some View {
        row("return", help: L10n[.Nodes.helpReturn]) {
            Toggle(L10n[.Nodes.toggleReturn], isOn: Binding(get: { node.reply != nil }, set: { on in
                change { $0.reply = on ? ($0.reply ?? FlowReturn()) : nil }("")
            }))
        }
        if let reply = node.reply {
            row("carries", help: StepEditor.help("carries", L10n[.Nodes.wordReturns])) {
                PathList(model: model, owner: flow, paths: reply.carries, editable: editable) { list in
                    change { $0.reply?.carries = list }("")
                }
            }
            row("peer", help: StepEditor.help("peer", L10n[.Nodes.helpPeerReturn])) {
                PathList(model: model, owner: flow, paths: reply.peers, editable: editable, kinds: [.flow],
                         each: L10n[.Nodes.wordPartnerFlow], adding: L10n[.Nodes.placeholderAddPartnerCaller]) { list in
                    change { $0.reply?.peers = list }("")
                }
            }
            row("program", help: StepEditor.help("program", L10n[.Nodes.wordOutsideTool])) {
                CommitField(L10n[.Nodes.placeholderProgramEnd], text: reply.program ?? "") { change { $0.reply?.program = $1.isEmpty ? nil : $1 }($0) }
                    .id("\(flow.id)|\(node.name)|return.program")
            }
            keyRow("signWith", reply.signWith, L10n[.Nodes.wordSignKey]) { value in change { $0.reply?.signWith = value }("") }
        }
    }

    private func keyRow(_ key: String, _ value: String?, _ words: String, _ set: @escaping (String?) -> Void) -> some View {
        row(key, help: StepEditor.help(key, words)) {
            PathField(model: model, owner: flow, value: value, kinds: nil, editable: editable,
                      placeholder: L10n[.Nodes.placeholderPathOf(words)], commit: set)
        }
    }

    /// Where it goes: one way, or more to part the way — each part going on to the
    /// same join.
    @ViewBuilder
    private var nextRows: some View {
        row("next", help: node.nexts.count > 1 ? L10n[.Nodes.helpNextMany] : L10n[.Nodes.helpNextOne]) {
            if node.nexts.isEmpty {
                way(nil, nil) { value in change { $0.nexts = value.map { [$0] } ?? [] }("") }
            }
            ForEach(Array(node.nexts.enumerated()), id: \.offset) { index, next in
                HStack {
                    way(nil, next) { value in
                        change { node, _ in
                            if let value { node.nexts[index] = value } else { node.nexts.remove(at: index) }
                        }("")
                    }
                    if node.nexts.count > 1 {
                        Button { change { $0.nexts.remove(at: index) }("") } label: { Image(systemName: "minus.circle") }
                            .buttonStyle(.borderless)
                    }
                }
            }
            if !node.nexts.isEmpty {
                Button(L10n[.Nodes.buttonAddNext]) {
                    guard let spare = others.first(where: { !node.nexts.contains($0) }) else { return }
                    change { $0.nexts.append(spare) }("")
                }
                .controlSize(.small)
                .disabled(others.allSatisfy { node.nexts.contains($0) })
                .help(L10n[.Nodes.helpAddNext])
            }
        }
    }

    @ViewBuilder
    private var eventRows: some View {
        row("event", help: L10n[.Nodes.helpEvent]) {
            chooser(node.event?.flow, PathWanted(cards: [.flow])) { value in
                change { node, _ in node.event = value.map { FlowEvent(flow: $0, outcome: node.event?.outcome ?? "success") } }("")
            }
        }
        if node.event != nil {
            row("outcome", help: L10n[.Nodes.helpEventOutcome]) {
                choice(node.event?.outcome, FlowNode.Vocabulary.outcomes, words: NodeInspector.outcomeWords) { value in
                    change { node, _ in node.event?.outcome = value ?? "success" }("")
                }
            }
        }
    }

    // MARK: - Parts

    /// Edits this node with `body`, given what was typed, and stores the nodes.
    private func change(_ body: @escaping (inout FlowNode, String) -> Void) -> (String) -> Void {
        { typed in
            var edited = node
            body(&edited, typed)
            model.setFlowNodes(flow.id, FlowEditing.replace(edited, in: flow.nodes))
        }
    }

    private func change(_ body: @escaping (inout FlowNode) -> Void) -> (String) -> Void {
        change { node, _ in body(&node) }
    }

    /// A property of the node as a row of the table; what it means at the panel's foot.
    private func row<Content: View>(_ title: String, help: String? = nil, @ViewBuilder _ content: () -> Content) -> some View {
        PropertyRow(title, help: help ?? StepEditor.help(title)) {
            VStack(alignment: .leading, spacing: 4) { content() }
        }
    }

    /// Where a way goes: one of the other nodes, or — when it may — nowhere.
    @ViewBuilder
    private func way(_ title: String?, _ value: String?, optional: Bool = false, help: String = "",
                     _ set: @escaping (String?) -> Void) -> some View {
        let picker = Picker("", selection: Binding(get: { value ?? "" }, set: { set($0.isEmpty ? nil : $0) })) {
            Text(optional ? L10n[.Common.wordNone] : L10n[.Nodes.wordUndecided]).tag("")
            ForEach(others, id: \.self) { Text($0).tag($0) }
        }
        .labelsHidden()
        if let title { row(title, help: help) { picker } } else { picker }
    }

    /// A card picked with the selector (`specboard://SpecBoard/Docs/Design:DesignSections` §8), stored in full.
    private func chooser(_ value: String?, _ wanted: PathWanted, _ set: @escaping (String?) -> Void) -> some View {
        HStack(spacing: 6) {
            Text(value.map(shown) ?? L10n[.Nodes.wordUndecided])
                .font(.app(11.5, design: .monospaced))
                .foregroundStyle(value == nil ? .secondary : .primary)
                .lineLimit(1).truncationMode(.head)
                .help(value ?? "")
            Spacer(minLength: 0)
            PathPickButton(title: L10n[.Nodes.buttonChoose], detail: detail, owner: flow, wanted: wanted) { set($0) }
                .controlSize(.small)
        }
    }

    private func choice(_ value: String?, _ options: [String], words: [String: String], none: String? = nil,
                        _ set: @escaping (String?) -> Void) -> some View {
        Picker("", selection: Binding(get: { value ?? "" }, set: { set($0.isEmpty ? nil : $0) })) {
            if let none { Text(none).tag("") } else if value == nil { Text(L10n[.Nodes.wordUndecided]).tag("") }
            ForEach(options, id: \.self) { Text(words[$0] ?? $0).tag($0) }
        }
        .labelsHidden()
    }
}
