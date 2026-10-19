import AppKit
import SwiftUI

/// A flow opened — its card editor (`specboard://SpecBoard/Docs/Design:Overview` §11, §13): its nodes drawn the
/// way the relation panel draws cards — a card a node on the grid, the ways as
/// lines from the rows that say where they go, the same zoom, drag and picking —
/// with what is left to do above and what the lines mean below; or the same flow as
/// a sequence diagram, opened in the shape its card keeps. A node picked — or a
/// row of a process node — is edited in the property panel (`NodeInspector`,
/// `StepEditor`). Its frame — the bar, the kinds of line switched under it, the canvas, the line
/// below — is every card map's (`CardMapBar`, `DiagramFrame`); a goal's is made of the same.
struct FlowEditor: View {
    var model: AppModel
    var flow: SchemaNode
    var detail: ProjectDetail
    /// Nodes are added, moved and changed — read only, in a locked main or an archived project.
    var editable: Bool

    @State private var controller = CardCanvasController()
    @State private var sequence = SequenceController()
    /// The shape switched to above; nil for the one the card keeps.
    @State private var askedShape: String?
    /// Drawn as mermaid — as it is written out — rather than on the app's canvas.
    @State private var asMermaid = false

    /// What is picked in the property panel, as the sequence diagram names where it came from.
    private var selectedOrigin: SequenceDiagram.Origin? {
        guard model.selectedSchemaId == flow.id, let row = model.selectedRow else { return nil }
        switch row {
        case .node(let name): return .init(node: name)
        case .nodeStep(let name, let path):
            if case .step(let index)? = StepEditing.parts(path)?.first { return .init(node: name, row: index) }
            return .init(node: name)
        default: return nil
        }
    }

    /// A node, or a row of a process node, picked on the sequence diagram.
    private func pick(_ origin: SequenceDiagram.Origin?) {
        guard let origin, flow.nodes.contains(where: { $0.name == origin.node }) else { model.clearRow(); return }
        if let row = origin.row {
            model.selectRow(flow.id, .nodeStep(origin.node, StepEditing.path([.step(row)])))
        } else {
            model.selectNode(flow.id, origin.node)
        }
    }

    /// The words for a shape a flow is drawn in.
    static func shapeWord(_ shape: String) -> String {
        shape == "sequence" ? L10n[.Flow.shapeSequence] : L10n[.Flow.shapeCardMap]
    }

    private var shape: String { askedShape ?? flow.diagramShape }

    var body: some View {
        let warnings = FlowGraph.warnings(flow, in: detail.directory)
        // A process node's rows being written are drawn as they are (`AppModel.RowsDraft`).
        let drawn = model.withDraft(flow)
        VStack(spacing: 0) {
            header
            if let draft = model.rowsDraft, draft.schemaId == flow.id {
                draftBanner(draft)
            }
            Divider()
            if !flow.description.isEmpty || !warnings.isEmpty {
                VStack(alignment: .leading, spacing: 4) {
                    if !flow.description.isEmpty { Text(flow.description).font(.app(11.5)).foregroundStyle(.secondary) }
                    ForEach(warnings.prefix(4), id: \.self) { warning in
                        Label(warning, systemImage: "exclamationmark.triangle").font(.app(10.5)).foregroundStyle(.orange)
                            .lineLimit(2)
                    }
                    if warnings.count > 4 { Text(L10n[.Flow.warningMore(warnings.count - 4)]).font(.app(10.5)).foregroundStyle(.orange) }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 14).padding(.vertical, 6)
                Divider()
            }
            if asMermaid && !flow.nodes.isEmpty {
                // The same picture as written out, a document's embed and MCP give.
                FlowMermaid(code: Self.mermaid(drawn, in: detail, shape: shape))
                Divider()
                Text(L10n[.Flow.mermaidNoteExported])
                    .font(.app(10.5)).foregroundStyle(.tertiary)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 14).padding(.vertical, 6)
            } else if shape == "sequence" && !flow.nodes.isEmpty {
                // Drawn on the app's own canvas, as the card map is.
                SequenceCanvas(diagram: Self.drawn(drawn, in: detail, language: Language.shared.code),
                               detail: detail, selected: selectedOrigin,
                               controller: sequence, onPick: pick, onOpen: { model.open(.card($0)) })
                Divider()
                Text(L10n[.Flow.sequenceNote])
                    .font(.app(10.5)).foregroundStyle(.tertiary)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 14).padding(.vertical, 6)
            } else if flow.nodes.isEmpty {
                VStack(spacing: 8) {
                    Text(L10n[.Flow.emptyNoNodes]).foregroundStyle(.secondary)
                    Button(L10n[.Flow.emptyPlaceStart]) { add("begin", after: nil) }
                        .disabled(model.isReadOnly || !editable)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                // The ways between the nodes switched at its top, as on every diagram (`DiagramFrame`):
                // the flow's ways, before the canvas has drawn any.
                let ways = controller.drawnKinds(in: nil)
                DiagramFrame(model: model, definition: DiagramDefinition(
                    detail: FlowCardMap.detail(drawn, in: detail),
                    key: "flow:\(flow.id)",
                    controller: controller,
                    // Both only ever go up, so their sum changes whenever either does.
                    dataGeneration: model.dataGeneration &+ model.draftGeneration,
                    // A node or a process node's row picked for the property panel; a way out
                    // followed to its node, a row to what it reads or runs, a run's header to
                    // what it runs (`DiagramPresses.flowMap`).
                    presses: presses(drawn),
                    onMove: { id, at in model.moveNodeByHand(flow.id, FlowCardMap.node(of: id), to: at) },
                    onClickEmpty: { model.clearRow() },
                    drawnLines: ways.isEmpty ? Line.flowWays : ways),
                    hint: L10n[.Flow.mapHint])
            }
        }
        // Another flow opens in the shape its card keeps.
        .onChange(of: flow.id) { askedShape = nil }
    }

    private var header: some View {
        CardMapBar(model: model, card: flow, editable: editable,
                   fit: { if shape == "sequence" { sequence.fit() } else { controller.fit() } }) {
            EmptyView()
        } tools: {
            Menu {
                ForEach(FlowNode.Vocabulary.kinds, id: \.self) { kind in
                    Button("\(FlowNode.Vocabulary.label(kind)) — \(FlowNode.Vocabulary.holding(kind))") {
                        add(kind, after: picked)
                    }
                    .disabled(kind == "begin" && flow.nodes.contains { $0.kind == "begin" })
                }
            } label: {
                Label(L10n[.Flow.wordNode], systemImage: "plus")
            }
            .fixedSize()
            .disabled(model.isReadOnly || !editable || shape == "sequence" || asMermaid)
            .help(L10n[.Flow.toolbarAddNodeHelp])
        } modes: {
            Picker("", selection: Binding(get: { shape }, set: { askedShape = $0 })) {
                ForEach(SchemaNode.diagramShapes, id: \.self) {
                    BarLabel(title: FlowEditor.shapeWord($0),
                             systemImage: $0 == "sequence" ? "arrow.left.arrow.right" : "rectangle.3.group").tag($0)
                }
            }
            .labelsHidden().pickerStyle(.segmented).fixedSize()
            .help(L10n[.Flow.toolbarShapeHelp])
            BarMenu(title: L10n[.Flow.mermaidMenuTitle], systemImage: "chevron.left.forwardslash.chevron.right") {
                Toggle("mermaid", isOn: $asMermaid)
                    .toggleStyle(.button).controlSize(.small)
                    .help(L10n[.Flow.mermaidViewHelp])
                Button(L10n[.Flow.mermaidCopy]) { copyMermaid() }
                    .controlSize(.small)
            } items: {
                Toggle(L10n[.Flow.mermaidView], isOn: $asMermaid)
                Button(L10n[.Flow.mermaidCopy]) { copyMermaid() }
            }
        }
    }

    /// The flow as mermaid, as it is written out, on the pasteboard.
    private func copyMermaid() {
        copyToPasteboard(Self.mermaid(flow, in: detail, shape: shape))
    }

    /// The flow as mermaid, shown and copied as it is written out: in the project's export language, so
    /// that what is pasted is what an export of it says, whoever copies it.
    static func mermaid(_ flow: SchemaNode, in detail: ProjectDetail, shape: String) -> String {
        ChartDiagram.render(flow, in: detail, shape: shape, exported: true, language: detail.project.exportedIn)
    }

    /// The flow's sequence diagram drawn in the editor: in the language of the screen that shows it.
    static func drawn(_ flow: SchemaNode, in detail: ProjectDetail, language: String) -> SequenceDiagram {
        SequenceDiagram.make(flow, in: detail, language: language)
    }

    private var picked: String? {
        guard model.selectedSchemaId == flow.id else { return nil }
        switch model.selectedRow {
        case .node(let name)?, .nodeStep(let name, _)?: return name
        default: return nil
        }
    }

    /// The row of a process node's card a path names (`FlowCardMap.rows`: `#s.0`).
    static func rowIndex(_ path: String) -> Int? {
        guard path.hasPrefix("#s.") else { return nil }
        return Int(path.dropFirst(3))
    }

    private func draftBanner(_ draft: AppModel.RowsDraft) -> some View {
        HStack(alignment: .top, spacing: 8) {
            Image(systemName: "exclamationmark.triangle").foregroundStyle(.orange)
            VStack(alignment: .leading, spacing: 2) {
                Text(L10n[.Flow.draftUnsaved(draft.node ?? "")]).font(.app(11.5, weight: .semibold))
                Text(draft.problem).font(.app(11)).foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            Spacer()
            Button(L10n[.Common.actionUndo]) { model.discardRowsDraft() }.controlSize(.small)
        }
        .padding(.horizontal, 14).padding(.vertical, 8)
        .background(Color.orange.opacity(0.08))
    }

    private func rowsMenu(_ title: String, _ pick: @escaping (String) -> Void) -> NSMenuItem {
        let item = NSMenuItem(title: title, action: nil, keyEquivalent: "")
        let ops = NSMenu()
        for op in FlowNode.Vocabulary.processOps {
            ops.addItem(ActionMenuItem(op) { pick(op) })
        }
        item.submenu = ops
        return item
    }

    /// A node put on the flow: brought into view on the card map once drawn — not while the flow is
    /// seen as a sequence diagram or as mermaid, where the map is not what is seen.
    private func add(_ kind: String, after: String?) {
        model.addNode(kind, after: after, to: flow, showingOn: shape == "sequence" || asMermaid ? nil : controller)
    }

    /// What the card map's presses do (`DiagramPresses.flowMap`), for the flow as drawn.
    func presses(_ drawn: SchemaNode) -> DiagramPresses {
        DiagramPresses.flowMap(model, flow: drawn, detail: model.detail ?? detail, controller: controller, menu: mapMenu)
    }

    /// A node's right-click: where the row pressed points, its path, and adding after it or
    /// taking it away; the empty grid's, adding one.
    func mapMenu(_ hit: CardCanvasView.RowHit?) -> NSMenu? {
        let menu = NSMenu()
        let after = hit.map { FlowCardMap.node(of: $0.card) }
        let fieldId = hit?.row?.path
        if let hit, let after, let whole = model.detail {
            DiagramMenus.mapTarget(menu, DiagramRows.flowMapTarget(hit.row, node: after, flow: model.withDraft(flow), in: whole),
                                   model) { id in
                model.selectNode(flow.id, FlowCardMap.node(of: id))
                controller.focus(id)
            }
        }
        if let after, let detail = model.detail {
            // In full, wherever it is pasted (`specboard://SpecBoard/Docs/Design:DesignSections` §8).
            let path = detail.directory.full(flow, [after])
            menu.addItem(ActionMenuItem(L10n[.Common.actionCopyPath]) { copyToPasteboard(path) })
        }
        guard !model.isReadOnly, editable else { return menu.items.isEmpty ? nil : menu }
        if !menu.items.isEmpty { menu.addItem(.separator()) }
        // A line from the node or its row, to a place picked beside it (overview §13).
        if let hit, let whole = model.detail, let source = DiagramRows.flowMapLineSource(hit, flow: flow, in: whole) {
            menu.addItem(ActionMenuItem(DiagramMenus.connectWord) {
                model.connecting = Connecting(source: source, anchor: controller.anchor(of: hit))
            })
            menu.addItem(.separator())
        }
        // A process node's rows: added, moved and taken away here.
        if let after, let node = model.withDraft(flow).nodes.first(where: { $0.name == after }), node.kind == "process" {
            if let index = fieldId.flatMap(FlowEditor.rowIndex), node.steps.indices.contains(index) {
                let path = StepEditing.path([.step(index)])
                if index > 0 {
                    menu.addItem(ActionMenuItem(L10n[.Flow.menuRowUp]) {
                        model.changeRows(flow.id, node: after, .move(from: index, to: index - 1))
                        model.selectRow(flow.id, .nodeStep(after, StepEditing.path([.step(index - 1)])))
                    })
                }
                if index < node.steps.count - 1 {
                    menu.addItem(ActionMenuItem(L10n[.Flow.menuRowDown]) {
                        model.changeRows(flow.id, node: after, .move(from: index, to: index + 1))
                        model.selectRow(flow.id, .nodeStep(after, StepEditing.path([.step(index + 1)])))
                    })
                }
                menu.addItem(rowsMenu(L10n[.Flow.menuAddRowAfter]) { op in
                    model.changeRows(flow.id, node: after, .add(op: op, at: index + 1))
                    model.selectRow(flow.id, .nodeStep(after, StepEditing.added(after: path, in: node.steps)))
                })
                menu.addItem(ActionMenuItem(L10n[.Flow.menuDeleteRow]) {
                    if model.selectedRow == .nodeStep(after, path) { model.selectNode(flow.id, after) }
                    model.changeRows(flow.id, node: after, .remove(at: index))
                })
            } else {
                menu.addItem(rowsMenu(L10n[.Flow.menuAddRow]) { op in
                    model.changeRows(flow.id, node: after, .add(op: op, at: node.steps.count))
                    model.selectRow(flow.id, .nodeStep(after, StepEditing.path([.step(node.steps.count)])))
                })
            }
            menu.addItem(.separator())
        }
        let adding = NSMenuItem(title: after == nil ? L10n[.Flow.menuAddNode] : L10n[.Flow.menuAddNodeAfter], action: nil, keyEquivalent: "")
        let kinds = NSMenu()
        for kind in FlowNode.Vocabulary.kinds where kind != "begin" || !flow.nodes.contains(where: { $0.kind == "begin" }) {
            kinds.addItem(ActionMenuItem(L10n[.Flow.kindWithId(FlowNode.Vocabulary.label(kind), kind)]) { add(kind, after: after) })
        }
        adding.submenu = kinds
        menu.addItem(adding)
        if let after {
            menu.addItem(.separator())
            menu.addItem(ActionMenuItem(L10n[.Flow.menuDeleteNode]) {
                model.clearRow()
                model.setFlowNodes(flow.id, FlowEditing.remove(after, from: flow.nodes))
            })
        }
        return menu
    }
}

private extension NSAppearance {
    var isDark: Bool { bestMatch(from: [.darkAqua, .aqua]) == .darkAqua }
}

/// The kind of a card, as the header of its card editor writes it.
struct KindBadge: View {
    var kind: SchemaKind
    var text: String?

    var body: some View {
        Text(text ?? kind.rawValue)
            .font(.app(10, design: .monospaced))
            // Never broken over lines: a bar too narrow cuts the name short instead (`CardBar`).
            .lineLimit(1).fixedSize()
            .padding(.horizontal, 5).padding(.vertical, 1)
            .overlay(RoundedRectangle(cornerRadius: 3).stroke(.tertiary))
            .foregroundStyle(.secondary)
            .help(Glossary.term(kind.rawValue)?.meaning ?? "")
    }
}

/// A flow drawn as mermaid, in the flow's editor. Scrolled down only: the page is
/// as wide as the pane and a diagram wider than it scrolls sideways in the page, its
/// scroll bar showing. Inside a sideways scroll view too, the page was given no
/// width and drew nothing.
struct FlowMermaid: View {
    var code: String

    var body: some View {
        ScrollView(.vertical) {
            MarkdownView(text: "```mermaid\n\(code.hasSuffix("\n") ? code : code + "\n")```", size: 13)
                .padding(16)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}
