import SwiftUI

// The parts of a goal's (a custom card's) that are written in the property panel: one of its
// fields (`CustomFieldControl`), and one of its rows — a card it holds — as the relation panel
// picks it (`CustomRowPane`). The goal itself is opened as its card map (`GoalEditor`).

/// One field of a custom card, written the way its type is: a choice, a branch or a merge request
/// of the project, a date, or text in a line or in a box. A choice is as wide as the value chosen, within the
/// panel's column (`FittingChoice`).
struct CustomFieldControl: View {
    var model: AppModel
    var card: SchemaNode
    var field: CustomKind.Field
    var editable: Bool

    private var value: String { card.customFields[field.name] ?? "" }
    /// What a choice shows: its value, or its default while none is written (a part's note place).
    private var chosenValue: String { value.isEmpty ? field.defaultValue ?? "" : value }

    var body: some View {
        let chosen = Binding(get: { chosenValue }, set: { set($0) })
        Group {
            switch field.fieldtype {
            case "radio":
                FittingChoice(options: .words(field.options, labels: field.optionLabels, none: field.defaultValue == nil), selection: chosen,
                              segmented: true)
            case "dropdownlist":
                FittingChoice(options: .words(field.options, labels: field.optionLabels, none: field.defaultValue == nil), selection: chosen)
            case "branch":
                FittingChoice(options: .branches(model.branches.map(\.name), keeping: value), selection: chosen)
            case "mr":
                FittingChoice(options: .mergeRequests(model.mergeRequests, keeping: value), selection: chosen)
            case "checkbox":
                Toggle("", isOn: Binding(get: { value == "true" }, set: { set($0 ? "true" : "") }))
                    .labelsHidden().toggleStyle(.checkbox)
            case KindField.sectionType:
                // A frame's target: a ui section of the tree, kept by id, written by its path.
                FrameTargetPicker(model: model, at: model.detail?.directory.sectionId(of: card) ?? "", chosen: value) {
                    model.setFrameTarget(card.id, $0)
                }
            case "longtext":
                CommitTextEditor(text: value, minHeight: 52) { set($0) }
                    .id("\(card.id)|\(field.name)")
            default:
                CommitField(field.fieldtype == "date" ? "YYYY-MM-DD" : field.name, text: value) { set($0) }
                    .id("\(card.id)|\(field.name)")
                    .frame(maxWidth: 320)
            }
        }
        .disabled(!editable)
        .help(field.description)
    }

    private func set(_ next: String) {
        let isChoice = field.fieldtype == "radio" || field.fieldtype == "dropdownlist"
        guard next != (isChoice ? chosenValue : value) else { return }
        // A check is written as one (a task's `done`), and a merge request as its number — not as their
        // words, which the core refuses for them.
        if field.fieldtype == "checkbox" {
            model.setCustomFlag(card.id, field.name, next == "true")
        } else if field.fieldtype == "mr" {
            model.setCustomNumber(card.id, field.name, Int(next))
        } else {
            model.setCustomField(card.id, field.name, next)
        }
    }
}

/// A goal's, a task's, a part's or a card with implementation lines' actions the core takes now (`card_read`'s `actions`), one row of buttons under
/// its fields, each said as the core says it; pressed, as the menus' 「アクション」 are
/// (`pressCardAction`). Nothing when none is taken now.
struct CardActionButtons: View {
    var model: AppModel
    var card: SchemaNode

    /// Whether the actions are a card's review of its implementation lines rather than a goal's, a task's or a part's.
    private var reviewsCode: Bool {
        card.kind != .goal && card.kind != .task && !card.kind.isUiPart && !card.codeReview.isEmpty
    }

    var body: some View {
        let actions = model.availableCardActions(card.id)
        if !actions.isEmpty {
            PropertyRow("actions", help: reviewsCode ? L10n[.Board.actionsRowHelpCode] : L10n[.Board.actionsRowHelp]) {
                // Wrapping: at the panel's narrowest the buttons side by side are wider than the column.
                FlowRow(spacing: 6, lineSpacing: 4) {
                    ForEach(actions) { action in
                        Button(action.label) { model.pressCardAction(card.id, action) }
                            .controlSize(.small)
                            .fixedSize()
                            .help(action.help)
                    }
                }
            }
        }
    }
}

/// A custom card's row in the property panel: what it says, whether it is done, its
/// type, and the lines its card has (つながり, `LineList`).
struct CustomRowPane: View {
    var model: AppModel
    var card: SchemaNode
    var path: String
    var editable: Bool

    /// The row's type as the screen says it: a card the container holds by its kind's name in the
    /// screen's language (`WholeCard.heldCard` is an id, not a word).
    private func typeShown(_ row: CustomRow) -> String {
        guard row.type == WholeCard.heldCard else { return row.type }
        let held = card.inside.lazy.compactMap { model.schema($0) }.first { $0.name == row.name }
        return model.detail?.cardKinds[held?.kindName ?? "task"]?.label ?? row.type
    }

    var body: some View {
        if let custom = CustomKind.of(card.kind), let row = CustomRows.row(at: path, in: card.customRows) {
            let type = custom.rowType(row.type)
            VStack(alignment: .leading, spacing: 10) {
                PropertyRow("name", help: L10n[.CardEditor.helpRowName(path)]) {
                    Text(path).font(.app(11.5, design: .monospaced)).textSelection(.enabled)
                }
                PropertyRow("type", help: type?.description ?? "") {
                    if custom.rows.count > 1, editable {
                        // The names are the kind's own, as long as its definer wrote them.
                        FittingChoice(options: custom.rows.map { .init($0.name, $0.name) },
                                      selection: Binding(get: { row.type }, set: { next in
                                          change { CustomRowEditing.update($0, at: path) { $0.type = next } }
                                      }))
                    } else {
                        Text(typeShown(row)).font(.app(11.5))
                    }
                }
                PropertyRow("text", help: L10n[.CardEditor.helpRowText]) {
                    CommitField(typeShown(row), text: row.text) { text in
                        change { CustomRowEditing.update($0, at: path) { $0.text = text } }
                    }
                    .id("\(card.id)|\(path)|text")
                    .disabled(!editable)
                }
                if type?.fieldtype == "checkbox" {
                    PropertyRow("done", help: L10n[.CardEditor.helpRowDone]) {
                        Toggle("", isOn: Binding(get: { row.done }, set: { done in
                            change { CustomRowEditing.update($0, at: path) { $0.done = done } }
                        }))
                        .labelsHidden().toggleStyle(.checkbox).disabled(!editable)
                    }
                }
                // What the row's card is joined to: its lines, as any card's (`LineList`).
                if type?.connectable ?? false, let source = model.lineSource(of: card, row: .customRow(path)) {
                    LineList(model: model, source: source, editable: editable)
                }
            }
        }
    }

    private func change(_ body: ([CustomRow]) -> [CustomRow]) {
        model.setCustomRows(card.id, body(card.customRows))
    }
}
