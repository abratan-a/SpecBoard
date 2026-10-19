import SwiftUI

/// A card's connections map on screen (`ConnectionsMap`, overview §13「つながりの図」): a bar with the
/// card, how far it reaches and the whole shown; the cards on the section map's canvas — the card in
/// the middle, the others round it in a frame for each section's group — in the frame every diagram
/// has (`DiagramFrame`: the kinds of line it draws switched at its top, starting as the section map
/// has them). Where a card is dropped by hand holds while it is looked at, and is written nowhere.
struct ConnectionsView: View {
    var model: AppModel
    var card: SchemaNode
    var depth: ConnectionDepth
    /// The whole project, whatever section is shown.
    var detail: ProjectDetail
    var controller: CardCanvasController
    /// The section map's: the lines switched as it has them, when this opens.
    var relationPanel: CardCanvasController

    @State private var moved: [String: GridPoint] = [:]

    var body: some View {
        let kinds = detail.lineKinds
        let filters = controller.filters
        let made = ConnectionsMap.make(card.id, depth: depth, in: detail,
                                       shows: { filters.shows($0, kinds: kinds) },
                                       moved: moved, settings: model.defaultSettings)
        VStack(spacing: 0) {
            bar(joined: max((made?.columns.count ?? 1) - 1, 0))
            Divider()
            if let made {
                // In the frame every diagram has (`DiagramFrame`); where a card is dropped is held
                // while it is looked at, and written nowhere.
                DiagramFrame(model: model, definition: DiagramDefinition(
                    detail: made.detail,
                    surroundings: made.surroundings,
                    key: "connections:\(card.id):\(depth.rawValue)",
                    controller: controller,
                    dataGeneration: model.dataGeneration,
                    presses: .connections(model, controller),
                    onMove: { moved[$0] = $1 },
                    onFormPress: { model.pressForm($0, $1) },
                    onClickEmpty: { model.deselect() },
                    marks: model.shownMarks,
                    drawnLines: made.kinds,
                    // The card's surroundings both ways, as far as it reaches (all: as far as MCP goes).
                    mermaid: { [path = detail.directory.full(card), steps = depth.steps ?? 10] kinds in
                        model.diagramMermaid("card_read", target: path, kinds: kinds,
                                             ["depth": steps, "direction": "both"])
                    },
                    // Fanning in to the card from the groups round it: cut round the cards on the
                    // grid, the lines would zig-zag through the other groups.
                    curvesOnly: true))
            }
        }
        // The lines switched as the section map has them, when this opens.
        .onAppear { controller.takeLines(from: relationPanel) }
        // Reaching further brings cards the view has not seen: the whole of it shown again.
        .onChange(of: depth) { DispatchQueue.main.async { controller.fit() } }
    }

    private func bar(joined: Int) -> some View {
        HStack(spacing: 10) {
            Image(systemName: "point.3.filled.connected.trianglepath.dotted")
                .foregroundStyle(.secondary)
            Text(detail.directory.shown(card))
                .font(.app(14, weight: .semibold, design: .monospaced))
                .lineLimit(1).truncationMode(.middle)
            if !card.label.isEmpty {
                Text(card.label).font(.app(12)).foregroundStyle(.secondary).lineLimit(1)
            }
            Text(L10n[.Lines.barJoined(joined)]).font(.app(11)).foregroundStyle(.tertiary).fixedSize()
            Spacer(minLength: 8)
            Text(L10n[.Lines.barDirection]).font(.app(10.5)).foregroundStyle(.tertiary).fixedSize()
            Picker("", selection: Binding(get: { depth }, set: { model.showConnections($0) })) {
                ForEach(ConnectionDepth.allCases, id: \.self) { step in
                    Text(step.label).tag(step)
                }
            }
            .labelsHidden().pickerStyle(.segmented).fixedSize()
            .help(L10n[.Lines.barHelpDepth])
            Button { controller.fit() } label: {
                Image(systemName: "arrow.up.left.and.arrow.down.right")
            }
            .buttonStyle(.borderless)
            .help(L10n[.Diagram.helpFit])
            Button(L10n[.Common.actionOpen]) { model.open(.card(card.id)) }
                .help(L10n[.Lines.barHelpOpenEditor])
        }
        .font(.app(11))
        .controlSize(.small)
        .padding(.horizontal, 14).padding(.vertical, 8)
    }
}
