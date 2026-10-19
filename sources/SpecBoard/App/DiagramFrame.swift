import SwiftUI

/// What a diagram on the canvas is (図の定義): what it draws — a section's cards (the relation panel), a
/// flow's or a goal's card map, a card of a container kind the project defines, a card's connections
/// — where a box dropped by hand is written, what a press does there, and the switches and the marks
/// it draws by. Every diagram is shown in the same frame (`DiagramFrame`).
struct DiagramDefinition {
    var detail: ProjectDetail
    /// What a section's diagram draws from the others (`Surroundings`).
    var surroundings = Surroundings()
    /// Which diagram it is, for where it was last looked at (`CardCanvasController.look`).
    var key: String
    var controller: CardCanvasController
    /// Changes when what it draws was read again (`AppModel.dataGeneration`).
    var dataGeneration: Int
    /// The project's card a box on it stands for (a card map's boxes are named apart:
    /// `GoalCardMap.card(of:)`); the box's own name elsewhere.
    var card: (String) -> String = { $0 }
    var presses: DiagramPresses
    /// A box dropped by hand: where it is written (or held, where nothing is kept).
    var onMove: (String, GridPoint) -> Void
    /// Whether a box dropped stays on the cell it was dropped on (`SectionKind.keepsPlaces`).
    var keepsPlaces = false
    /// Whether it is a ui section's map, its parts drawn as the mock draws them (`CardCanvasView.drawsParts`).
    var drawsParts = false
    /// A card sized by its corner.
    var onResize: (String, GridSize) -> Void = { _, _ in }
    /// A card's switch or choice pressed on the diagram.
    var onFormPress: (String, CardCanvasView.FormPress) -> Void = { _, _ in }
    /// What stands for another section, put where it had no place.
    var onPlaceOutside: ([String: GridPoint]) -> Void = { _ in }
    var onClickEmpty: () -> Void
    /// What the open branch changed, made, or saw main change.
    var marks = BranchMarks()
    /// The kinds of line it draws, switched at its top; what the canvas drew when not said.
    var drawnLines: [String]? = nil
    /// Whether the menu bar's zoom and fit act on it while it is shown (a card map, a card's
    /// connections); the relation panel's canvas is theirs otherwise (`ContentView.diagramActions`).
    var takesMenu = true
    /// The diagram as mermaid, of the kinds of line shown: what MCP's `as: diagram` gives, the
    /// core's (`AppModel.diagramMermaid`). None where it is not drawn so.
    var mermaid: (([String]) -> String?)? = nil
    /// Its lines drawn as curves whatever this Mac's setting (`CardCanvasView.curvesOnly`).
    var curvesOnly = false
    /// Said at its top beside a lock, where what it draws is read and not written here: a working
    /// directory's section (`WorkDirCaption`).
    var caption: WorkDirCaption? = nil
    /// What else the band at its top holds on the right, before mermaid: a section map's cards shown for a
    /// look and a ui section's switches (the map tab's alone, `specboard://SpecBoard/Docs/Design:UI` §4).
    var bandTools: AnyView? = nil
    /// What ends the band, after mermaid: a section map's filter.
    var bandEnd: AnyView? = nil
}

/// What a working directory's section map says at its top: what it is drawn from, and, for a folder a note
/// is in that reading the folder no longer finds, that it is gone.
struct WorkDirCaption: Equatable {
    var text: String
    var missing = false

    /// The caption of the section `id`, or nil for a section no working directory has.
    static func of(_ id: String, _ model: AppModel) -> WorkDirCaption? {
        guard model.isInWorkDir(section: id), let section = model.detail?.directory.section(id: id) else { return nil }
        if section.missing { return WorkDirCaption(text: L10n[.Diagram.workDirMissingCaption(section.description)], missing: true) }
        return WorkDirCaption(text: L10n[.Diagram.workDirCaption])
    }
}

extension DiagramDefinition {
    /// Whether the box `id` is written where it is when pressed twice: its card's kind says so
    /// (`on_double_click: edit_in_place`, a note) — on every diagram, the same card wherever it
    /// is drawn.
    func writesInPlace(_ id: String, _ model: AppModel) -> Bool { model.canEditInPlace(card(id)) }

    /// The body written in place, as its card keeps it.
    func inPlaceText(_ id: String, _ model: AppModel) -> String { model.schema(card(id))?.body ?? "" }

    /// What was written in place, kept as its card's body.
    func writeInPlace(_ id: String, _ text: String, _ model: AppModel) {
        model.updateSchema(card(id), .init(body: text))
    }
}

/// A diagram in its frame, the same for every diagram (`DiagramDefinition`): the kinds of line it
/// draws switched at its top — under the bar above it — then the canvas, and a line under it
/// saying how it is used, when it says one.
struct DiagramFrame: View {
    var model: AppModel
    var definition: DiagramDefinition
    /// How it is used, said under it (`CardFoot`).
    var hint: String? = nil
    /// Shown as mermaid rather than on the canvas.
    @State private var asMermaid = false

    var body: some View {
        let d = definition
        let kinds = model.detail?.lineKinds ?? d.detail.lineKinds
        let drawn = d.drawnLines ?? d.controller.drawnKinds(in: d.detail)
        // The kinds switched on: what the mermaid draws too.
        let shown = drawn.filter { d.controller.filters.shows($0, kinds: kinds) }
        VStack(spacing: 0) {
            // One line while it fits; else the kinds of line wrap and the things on the right go to
            // their marks alone.
            ViewThatFits(in: .horizontal) {
                band(d, drawn: drawn, kinds: kinds, shown: shown, compact: false)
                band(d, drawn: drawn, kinds: kinds, shown: shown, compact: true)
            }
            .padding(.horizontal, 14).padding(.vertical, 6)
            Divider()
            if asMermaid, let mermaid = d.mermaid {
                FlowMermaid(code: mermaid(shown) ?? "%% " + L10n[.Diagram.mermaidCantDraw])
            } else {
            CardCanvas(
                detail: d.detail,
                layoutGeneration: model.layoutGeneration,
                dataGeneration: d.dataGeneration,
                surroundings: d.surroundings,
                section: d.key,
                controller: d.controller,
                // A card or row picked, followed, right-clicked: the same rule on every diagram
                // (`DiagramPresses`, overview §13).
                presses: d.presses,
                onMoveByHand: d.onMove,
                keepsPlaces: d.keepsPlaces,
                onResizeByHand: d.onResize,
                onFormPress: d.onFormPress,
                onPlaceOutside: d.onPlaceOutside,
                canEditInPlace: { d.writesInPlace($0, model) },
                inPlaceText: { d.inPlaceText($0, model) },
                onEditInPlace: { d.writeInPlace($0, $1, model) },
                // A card drawn as a document (a writing card, a task's body): read from the whole project,
                // kept for the window.
                documentText: { model.documentFace($0, ready: $1) },
                // A ui part's face: read for its section in the tree open, kept for the window.
                drawsParts: d.drawsParts,
                partFace: { model.partFace($0, ready: $1) },
                partLook: CardCanvasView.PartLook(tags: model.partTags, notes: model.partNotes),
                onClickEmpty: d.onClickEmpty,
                settings: model.defaultSettings,
                cardWidth: model.cardWidth,
                marks: d.marks,
                pickedLine: model.selectedLine?.id,
                curvesOnly: d.curvesOnly)
                // つなぐ, asked on the diagram, shown where it was asked (overview §13).
                .modifier(ConnectAnchor(model: model))
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .onAppear { if d.takesMenu { model.mapCanvas = d.controller } }
                .onDisappear { if d.takesMenu, model.mapCanvas === d.controller { model.mapCanvas = nil } }
            }
            if let hint {
                Divider()
                CardFoot(hint: hint) { EmptyView() }
                    .padding(.horizontal, 14).padding(.vertical, 6)
            }
        }
    }
}

extension DiagramFrame {
    /// The band at the diagram's top: the selection mode, the kinds of line, and on the right what the
    /// diagram says of itself and the ways to look at it. `compact`: the kinds wrap, the right says its marks.
    @ViewBuilder
    private func band(_ d: DiagramDefinition, drawn: [String], kinds: LineKinds, shown: [String],
                      compact: Bool) -> some View {
        HStack(alignment: .top, spacing: 12) {
            if d.controller.isSelecting {
                // The selection mode, said where it is seen, and the way out (Esc too).
                HStack(spacing: 6) {
                    Text(L10n[.Diagram.selectMode]).font(.app(10.5, weight: .semibold))
                    Text(L10n[.Diagram.selectHint])
                        .font(.app(10.5)).foregroundStyle(.secondary).lineLimit(1)
                    Button(L10n[.Diagram.selectExit]) { d.controller.setSelecting(false) }.controlSize(.small).font(.app(10.5))
                }
                .padding(.horizontal, 6).padding(.vertical, 2)
                .background(RoundedRectangle(cornerRadius: 5).fill(Color.accentColor.opacity(0.14)))
                .fixedSize()
            }
            if compact {
                LineSwitches(drawn: drawn, kinds: kinds,
                             filters: d.controller.filters) { d.controller.toggle(line: $0, kinds: kinds) }
            } else {
                LineSwitches(drawn: drawn, kinds: kinds,
                             filters: d.controller.filters) { d.controller.toggle(line: $0, kinds: kinds) }
                    .fixedSize()
                Spacer(minLength: 0)
            }
            if let caption = d.caption {
                if compact { Spacer(minLength: 0) }
                Label(caption.text, systemImage: caption.missing ? "exclamationmark.triangle" : "lock.fill")
                    .font(.app(10.5)).foregroundStyle(caption.missing ? AnyShapeStyle(.orange) : AnyShapeStyle(.secondary))
                    .lineLimit(1).truncationMode(.middle)
                    .help(caption.text)
            }
            HStack(spacing: 10) {
                if let tools = d.bandTools { tools }
                if let mermaid = d.mermaid {
                    Toggle(isOn: $asMermaid) { BarLabel(title: "mermaid", systemImage: "chevron.left.forwardslash.chevron.right") }
                        .toggleStyle(.button).controlSize(.small).font(.app(10.5))
                        .help(L10n[.Diagram.mermaidHelp])
                    Button { if let text = mermaid(shown) { copyToPasteboard(text) } } label: {
                        BarLabel(title: L10n[.Diagram.mermaidCopy], systemImage: "doc.on.doc")
                    }
                    .controlSize(.small).font(.app(10.5))
                    .help(L10n[.Diagram.mermaidCopy])
                }
                if let end = d.bandEnd { end }
            }
            .fixedSize()
            .labelStyle(compact ? AnyLabelStyle(.iconOnly) : AnyLabelStyle(.titleAndIcon))
            .environment(\.cardBarCompact, compact)
        }
    }
}

/// A label style chosen as the band is laid out (`DiagramFrame.band`).
struct AnyLabelStyle: LabelStyle {
    private let make: (Configuration) -> AnyView

    init<S: LabelStyle>(_ style: S) {
        make = { AnyView(style.makeBody(configuration: $0)) }
    }

    func makeBody(configuration: Configuration) -> some View { make(configuration) }
}

extension AppModel {
    /// A diagram as mermaid, as MCP's `as: diagram` draws it — the core's (`section_read` of a
    /// section, `card_read` of a card: a container's card map, a card's surroundings), in the tree
    /// open, of the kinds of line `kinds` names (every kind when none). None while what a merge
    /// would leave is looked at: its branch is the app's, and no tool names it.
    func diagramMermaid(_ name: String, target: String, kinds: [String], _ more: [String: Any] = [:]) -> String? {
        guard case .string(let text)? = diagramAnswer(name, target: target, kinds: kinds, more)?["mermaid"] else {
            return nil
        }
        return text
    }

    /// The answer `diagramMermaid` reads its mermaid from (`mermaid`), with what the diagram was
    /// drawn from beside it (`rev`, `kinds`, …).
    func diagramAnswer(_ name: String, target: String, kinds: [String], _ more: [String: Any] = [:]) -> OrderedObject? {
        guard resultView == nil else { return nil }
        var value = more
        value["as"] = "diagram"
        if !kinds.isEmpty { value["kinds"] = kinds }
        let answer: OJSON
        do {
            answer = try tool(name, target: target, value: value)
        } catch {
            return nil
        }
        guard case .object(let said) = answer, case .object(let drawn)? = said["json"] else { return nil }
        return drawn
    }
}
