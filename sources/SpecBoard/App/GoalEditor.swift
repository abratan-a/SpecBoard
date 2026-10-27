import AppKit
import SwiftUI

/// A goal opened — its card editor: a section card, so its card map — the cards it holds
/// (`items`) on the relation panel's canvas, each where the goal's map keeps it (`GoalCardMap`)
/// — or, switched to above, its body (Markdown). Its fields are written in the property panel.
///
/// Its frame is every card map's (`CardMapBar`, `DiagramFrame`), as a
/// flow's is (`FlowEditor`). What is a goal's own: what it draws — its tasks, each a card
/// with a document of its own, and the goals inside it — and what a press does there: a
/// task's check makes it done, a double-click opens the card (a task's editor, a goal's
/// card map), a drop puts the card there on the goal's map, and 「task を足す」 makes one
/// in the goal.
struct GoalEditor: View {
    var model: AppModel
    var goal: SchemaNode
    /// The whole project: the cards the goal holds, wherever their section is.
    var detail: ProjectDetail
    var editable: Bool

    enum Mode: CaseIterable {
        case map, body

        var title: String {
            switch self {
            case .map: L10n[.Board.modeCardMap]
            case .body: L10n[.Board.modeBody]
            }
        }
    }

    @State private var controller = CardCanvasController()
    @State private var mode = Mode.map
    /// 「task を足す」 open, and what is written in it.
    @State private var adding = false
    @State private var newName = ""
    @State private var newLabel = ""

    private var writable: Bool { editable && !model.isReadOnly }

    var body: some View {
        let map = GoalCardMap.detail(goal, in: detail)
        VStack(spacing: 0) {
            CardMapBar(model: model, card: goal, badge: BuiltinWords.badge(goal.kind.rawValue), editable: editable,
                       fit: { controller.fit() }) {
                progress
            } tools: {
                Button { adding = true } label: { Label(L10n[.Board.goalAddTask], systemImage: "plus") }
                    .fixedSize()
                    .disabled(!writable)
                    .help(L10n[.Board.goalAddTaskHelp])
                    .popover(isPresented: $adding, arrowEdge: .bottom) { addingTask }
            } modes: {
                Picker("", selection: $mode) {
                    ForEach(Mode.allCases, id: \.self) {
                        BarLabel(title: $0.title, systemImage: $0 == .map ? "rectangle.3.group" : "doc.text").tag($0)
                    }
                }
                .labelsHidden().pickerStyle(.segmented).fixedSize()
                .help(L10n[.Board.goalModesHelp])
            }
            Divider()
            switch mode {
            case .body:
                NoteEditor(model: model, note: goal, editable: editable, compact: true) { model.open(path: $0, from: $1) }
            case .map where map.schemas.isEmpty:
                VStack(spacing: 8) {
                    Text(L10n[.Board.goalEmpty]).foregroundStyle(.secondary)
                    Button(L10n[.Board.goalAddTask]) { adding = true }.disabled(!writable)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            case .map:
                DiagramFrame(model: model, definition: DiagramDefinition(
                    detail: map,
                    surroundings: Surroundings(groups: model.groups(on: goal.id)),
                    key: "goal:\(goal.id)",
                    controller: controller,
                    dataGeneration: model.dataGeneration,
                    card: GoalCardMap.card(of:),
                    // A card picked — its own fields in the property panel — a goal inside's
                    // row as in the relation panel; followed, a task to its card editor, a
                    // goal inside to its card map (`DiagramPresses.goalMap`).
                    presses: DiagramPresses.goalMap(model, menu: menu),
                    onMove: { id, at in model.moveOnGoalMap(goal.id, GoalCardMap.card(of: id), to: at) },
                    // A task sized by its corner (its kind's face): how it looks.
                    onResize: { id, size in model.resizeByHand(GoalCardMap.card(of: id), to: size) },
                    // A task's check; a goal inside's checks and choices.
                    onFormPress: { id, press in model.pressForm(GoalCardMap.card(of: id), press) },
                    // Nothing picked: the goal's own fields.
                    onClickEmpty: { model.selectSchema(goal.id, follow: false) },
                    mermaid: { [path = detail.directory.full(goal)] kinds in
                        model.diagramMermaid("card_read", target: path, kinds: kinds)
                    },
                    groups: model.cardMapGroupHooks(goal.id)),
                    hint: L10n[.Board.goalMapHint])
            }
        }
        .onAppear { pickTheRowOpened() }
    }

    /// Opened by a press twice on one of its rows — on the relation panel, or on the card map of
    /// the goal holding it — the row picked (`DiagramPresses`): the card that row is, picked on the
    /// map and brought into view, so it is found where the goal opens.
    private func pickTheRowOpened() {
        guard model.selectedSchemaId == goal.id, case .customRow(let name)? = model.selectedRow,
              let card = detail.schemas.first(where: { $0.container == goal.id && $0.name == name }) else { return }
        model.selectSchema(card.id, follow: false)
        controller.focus(GoalCardMap.id(card.id))
    }

    // MARK: - Above

    /// How far it has come: of the cards it holds, how many are done — and, all of them
    /// done, the way to call it done (a person does, not the goal by itself).
    @ViewBuilder
    private var progress: some View {
        if let custom = CustomKind.of(goal.kind), let done = CustomRows.progress(goal.customRows, counting: custom.counts) {
            Text("\(done.done)/\(done.of)").font(.app(11, design: .monospaced)).foregroundStyle(.secondary)
                .help(L10n[.Board.goalProgressHelp])
            if done.done == done.of, let finish = Self.finishing(detail.cardKinds[goal.kind.rawValue]),
               goal.customFields[finish.field] != finish.value, writable,
               let action = model.cardActions(goal.id).first(where: { $0.op == "finish" }) {
                // The goal's finish, as the property panel's button: its branch, its fields and its place
                // in one; not pressed while the core would refuse it, and why said over it.
                Button(L10n[.Board.goalCanFinish]) { model.pressCardAction(goal.id, action) }
                    .controlSize(.small)
                    .disabled(!action.available)
                    .help(action.help)
            }
        }
    }

    /// The field that says a card is done — by its name (`status`) and its label (状態) — and
    /// the value that says it, as kept and as shown: the header's first choice with a value its
    /// definition counts as done (`完了`).
    static func finishing(_ def: CardKindDef?) -> (field: String, label: String, value: String, shown: String)? {
        for name in def?.header ?? [] {
            guard let field = def?.field(name), field.chosen,
                  let done = field.done.compactMap(\.asText).first(where: field.options.contains) else { continue }
            return (name, field.label, done, field.shown(done))
        }
        return nil
    }

    /// 「task を足す」: its name (what the project calls it by) and the name people read.
    private var addingTask: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(L10n[.Board.goalAddTask]).font(.app(13, weight: .semibold))
            Text(L10n[.Board.addWhereTo(detail.directory.full(goal))]).font(.app(11)).foregroundStyle(.secondary)
            TextField(L10n[.Board.goalTaskName], text: $newName)
                .textFieldStyle(.roundedBorder)
                .font(.app(12, design: .monospaced))
                .frame(width: 280)
            TextField(L10n[.Board.fieldDisplayName], text: $newLabel)
                .textFieldStyle(.roundedBorder)
                .frame(width: 280)
            HStack {
                Spacer()
                Button(L10n[.Common.actionCancel]) { adding = false }
                    .keyboardShortcut(.cancelAction)
                Button(L10n[.Common.actionAdd]) {
                    // Brought into view on the card map once drawn, when it is the map that is seen.
                    model.addTask(to: goal.id, name: newName, label: newLabel, showingOn: mode == .map ? controller : nil)
                    guard model.lastError == nil else { return }
                    newName = ""
                    newLabel = ""
                    adding = false
                }
                .keyboardShortcut(.defaultAction)
                .disabled(PathText.nameProblem(PathText.trimmed(newName)) != nil)
            }
        }
        .padding(16)
    }

    // MARK: - The map's menu

    /// A card's right-click: open it, and the path of the row pressed — a goal inside's row,
    /// else the card (nothing on a map's card points elsewhere) — and its place in the order the
    /// goal holds them, the order it is done in, one earlier or later; the
    /// empty map's, a task added.
    func menu(_ hit: CardCanvasView.RowHit?) -> NSMenu? {
        let menu = NSMenu()
        // Only real cards are inside a goal: a card ghost never is.
        if let hit, let card = model.schema(GoalCardMap.card(of: hit.card)) {
            menu.addItem(ActionMenuItem(L10n[.Common.actionOpen]) { model.open(.card(card.id)) })
            // In full, wherever it is pasted (`specboard://SpecBoard/Docs/Design:DesignSections` §8).
            menu.addItem(DiagramMenus.copyPathItem(DiagramRows.goalMapPaths(hit.row, card: card, in: detail)))
            if let item = DiagramMenus.actionsItem(card, model) { menu.addItem(item) }
            for item in DiagramMenus.sizeItems(card, model) { menu.addItem(item) }
            if writable, let index = goal.inside.firstIndex(of: card.id) {
                menu.addItem(.separator())
                let earlier = ActionMenuItem(L10n[.Board.menuEarlier]) { model.moveInside(card.id, by: -1) }
                earlier.isEnabled = index > 0
                menu.addItem(earlier)
                let later = ActionMenuItem(L10n[.Board.menuLater]) { model.moveInside(card.id, by: 1) }
                later.isEnabled = index < goal.inside.count - 1
                menu.addItem(later)
            }
        } else if writable {
            menu.addItem(ActionMenuItem(L10n[.Board.goalAddTaskMenu]) { adding = true })
        }
        return menu.items.isEmpty ? nil : menu
    }
}
