import AppKit
import SwiftUI

// What a section card's card editor is made of, whatever the card (`specboard://SpecBoard/Docs/Design:Overview` §3,
// §13): a bar above — the card, what its kind adds, the whole map shown, the ways to look
// at it, the card's tools — then its card map in the diagram's frame (`DiagramFrame`: the kinds
// of line switched at its top, the relation panel's canvas, a line below saying how it is used).
// The property panel stays on the right (`ContentView`). A flow's (`FlowEditor`) and a goal's
// (`GoalEditor`) are put together from these; what each adds is what it draws (`FlowCardMap`,
// `GoalCardMap`) and what a press, a double-click or a drop does there.

/// The bar above a card map: the card's kind, name and label, then what its kind says
/// beside them (`leading`); on the right, what it adds (`tools`), the whole map shown, the
/// ways to look at it (`modes`), and the card's connections and history. One line, as
/// every card editor's (`CardBar`).
struct CardMapBar<Leading: View, Tools: View, Modes: View>: View {
    var model: AppModel
    var card: SchemaNode
    /// The kind as the badge says it; the kind's name when none.
    var badge: String? = nil
    var editable: Bool
    /// The whole map shown: the canvas's, or another view's.
    var fit: () -> Void
    @ViewBuilder var leading: Leading
    @ViewBuilder var tools: Tools
    @ViewBuilder var modes: Modes

    var body: some View {
        CardBar(kind: card.kind, badge: badge, name: card.name, label: card.label) {
            leading
        } tools: {
            tools
            Button(action: fit) {
                Image(systemName: "arrow.up.left.and.arrow.down.right")
            }
                .buttonStyle(.borderless)
                .help(L10n[.Diagram.helpFit])
            modes
            CardTools(model: model, card: card, editable: editable) { model.open(.card($0)) }
        }
        .padding(.horizontal, 14).padding(.vertical, 8)
    }
}
