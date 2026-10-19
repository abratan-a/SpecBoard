import AppKit
import QuartzCore

/// The diagram.
///
/// The whole design follows from one measurement in the web build it replaces: at
/// 163 cards on a retina screen, a Safari profile spent 14.9 of 31.7 seconds
/// compositing and 8.0 in layout, while *painting* everything 1320 times cost 54ms
/// in total. Drawing was never the problem — moving a DOM the size of the diagram
/// was. So here:
///
///   * a card is a texture, drawn once, never re-laid-out
///   * panning and zooming set one transform and touch nothing else
///   * edge geometry is recut only when a card actually moves
///
/// None of that changes what is on screen at different zooms. The diagram is meant
/// to be read while overviewing it.
final class CardCanvasView: NSView {

    // MARK: - Callbacks

    /// What a press landed on (overview §13): a card, and the row of it that was aimed at —
    /// nil for its header, and for a card drawn too small to aim at one of its rows.
    struct RowHit: Equatable {
        var card: String
        var row: CardRow?
    }

    /// The same three for every diagram and every kind of card — the relation panel's and a
    /// card map's; what each means there is the window's (`DiagramPresses`). Pressed once: the
    /// card or row is picked, and its row marked here.
    var onPick: ((RowHit) -> Void)?
    /// Twice, or Return on what is picked: where the row points, or what the header does.
    var onFollow: ((RowHit) -> Void)?
    /// Right-clicked — picked too, so what is aimed at and what the items act on are one
    /// thing — or nil where there is no card.
    var onRowMenu: ((RowHit?) -> NSMenu?)?
    /// A card dropped by hand, on the cell it went nearest, in grid units (§12).
    var onMoveByHand: ((String, GridPoint) -> Void)?
    /// Whether a card — a section ghost too — may be picked up by its header at all; one
    /// that may not is pressed like the canvas, which moves the view. Every card when not
    /// said.
    var canMoveByHand: ((String) -> Bool)?
    /// A click where there is no card — not a drag that moved the view.
    var onClickEmpty: (() -> Void)?
    /// A line pressed (overview §13「線を選ぶ」): the line it draws — `connection` is its id.
    /// It is marked here (`showLine`), and the card picked before let go.
    var onPickLine: ((Relation) -> Void)?
    /// Whether a line may be drawn by hand from this row or header: a handle shows on its
    /// right edge under the pointer, to drag to another card's header or row. Nil, nowhere.
    var canConnect: ((RowHit) -> Bool)?
    /// A line dragged from a handle and let go on a card's header or a row (つなぐ): from
    /// where, to what, and where it was let go, in the view's points.
    var onConnect: ((RowHit, RowHit, CGPoint) -> Void)?
    /// A writing card sized by hand, by its corner, in grid units.
    var onResizeByHand: ((String, GridSize) -> Void)?
    /// What stands for another section — its card, a section ghost — put where it had no
    /// place yet, on the cell it went to: to be kept there. Not what is shown
    /// for a look (`Surroundings.temporary`).
    var onPlaceOutside: (([String: GridPoint]) -> Void)?
    /// Whether pressing a card twice (or Return on it) writes it where it is, over the diagram
    /// (`beginInPlace`): its kind says so (a note's `on_double_click: edit_in_place`) and it may
    /// be written. Nil or false: the press goes where it points (`onFollow`).
    var canEditInPlace: ((String) -> Bool)?
    /// The text a card is written in place from: its body as it is.
    var inPlaceText: ((String) -> String)?
    /// What was written in place, kept: the card, and its text whole.
    var onEditInPlace: ((String, String) -> Void)?
    /// A custom card's form pressed on the diagram: a row's switch, or a radio
    /// field's option.
    enum FormPress: Equatable {
        case toggle(String)
        case choose(String, String)
    }
    var onFormPress: ((String, FormPress) -> Void)?
    /// What a card drawn as a document shows, its references resolved (`DocumentTexts`): asked as
    /// the card is drawn, the second argument called once a newer page is in.
    var documentText: ((SchemaNode, (() -> Void)?) -> DocumentTexts.Face)?
    /// Whether the section drawn is a ui section's (`specboard://SpecBoard/Docs/Design:CardKinds`「UI のセクションと
    /// はめ込み」): its parts (html, svg, frame) drawn as the mock draws them — their faces, with no header,
    /// border or ground —, stacked in the section's order, never raised, and moved by any point of them.
    var drawsParts = false
    /// A part's face, read for the section drawn (`PartFaces`): asked as the part is drawn, the second
    /// argument called once a newer face is in.
    var partFace: ((SchemaNode, (() -> Void)?) -> PartFaces.Answer)?
    /// What the map lays over the parts (`PartDecor`): their tags and their notes, each shown or not.
    struct PartLook: Equatable {
        var tags = true
        var notes = true
    }
    var partLook = PartLook() {
        didSet { if partLook != oldValue { withoutAnimation { for id in cardLayers.keys where isPart(id) { drawDecor(id) } } } }
    }
    /// How big the project's cards are and the gap between cells (§12).
    private(set) var settings = LayoutSettings.standard
    /// Whether a box dropped stays on the cell it was dropped on, not moved off the margin of another
    /// or off one it lies over (`SectionKind.keepsPlaces`).
    var keepsPlaces = false
    /// Every card's width; a grid unit is a tenth of it across and a row down.
    private(set) var cardWidth = CardGeometry.cardWidth
    private var unit: CGSize {
        CGSize(width: Grid.unitWidth(cardWidth: Double(cardWidth)), height: Grid.unitHeight)
    }
    /// One grid unit in diagram points: what a place on the grid is counted in.
    var gridUnit: CGSize { unit }
    private func points(_ at: GridPoint) -> CGPoint {
        CGPoint(x: CGFloat(at.x) * unit.width, y: CGFloat(at.y) * unit.height)
    }
    /// A drawn card's place on the grid.
    private func gridRect(of box: CGRect) -> GridRect {
        GridRect(x: Int((box.minX / unit.width).rounded()), y: Int((box.minY / unit.height).rounded()),
                 w: Int((box.width / unit.width).rounded()), h: Grid.rows(forHeight: Double(box.height)))
    }
    /// How each drawn card is drawn: its own size and whether it is a document.
    private var forms: [String: CardForm] = [:]
    /// A card's box size: its form's, or its own width (the project's when it has none) and as
    /// tall as its rows.
    private func boxSize(_ id: String, _ content: CardContent?) -> CGSize {
        forms[id]?.size ?? CGSize(width: forms[id]?.width ?? cardWidth, height: content.map(CardRenderer.height(of:)) ?? 0)
    }
    /// Fires whenever the viewport scale changes, so the toolbar can show it.
    var onZoomChanged: ((CGFloat) -> Void)?
    /// Fires when the drawn edge count changes — merging collapses reciprocal pairs,
    /// so this is smaller than the project's relation count and worth showing.
    var onEdgeCountChanged: ((Int) -> Void)?
    /// Which reference kinds this project could draw, whether or not they are switched
    /// on. The legend is built from this, and the legend is also the only way to
    /// switch one back on — so it has to list what is *available*, not what is
    /// currently visible, or a kind could be turned off and never recovered.
    var onKindsChanged: ((Set<String>) -> Void)?

    // MARK: - Model

    private var detail: ProjectDetail?
    /// The tree being looked at: paths into it show on the rows without it.
    private var tree: String?
    private var filters = GraphFilters()
    private var schemasById: [String: SchemaNode] = [:]
    /// Schemas of the project's other sections, named as this section writes them
    /// (`Core:Address`): looked up, so a row holding one names it and a nesting card
    /// can unfold it, and drawn, faded, when one is a reference or a consumer.
    private var outsiders: [String: SchemaNode] = [:]
    /// Everything the section's diagram knows about the others. See `Surroundings`.
    private var surroundings = Surroundings()
    /// What the open branch changed, made, or saw main change: said on each card.
    private var marks = BranchMarks()
    /// The other sections' cards drawn right now.
    private(set) var outsideCards: Set<String> = []
    /// A card by its id — a ghost's id too, as the card it leads to (a `ref` written
    /// through a ghost holds the ghost's), named as this section writes the ghost.
    private var lookup: SchemaLookup {
        let own = schemasById, others = outsiders, through = throughGhosts
        return { own[$0] ?? others[$0] ?? through[$0] }
    }
    /// Every card ghost of the project, by id: the card it leads to, named as the section on
    /// screen writes the ghost (`Surroundings.ghostNames`).
    private var throughGhosts: [String: SchemaNode] = [:]
    /// The section's card ghosts drawn now, by id: each a box of its own, the card it
    /// leads to drawn whole in it — or a small card saying it leads nowhere.
    private var ghostNodes: [String: SchemaNode] = [:]
    /// How each one's header is drawn: the shortcut's mark, its name and what it points at.
    private var ghostLooks: [String: CardRenderer.GhostLook] = [:]
    /// The card ghosts drawn now.
    private(set) var ghostCards: Set<String> = []
    /// What a merge looked at would take away, drawn now where main puts them (`Surroundings.gone`):
    /// faded and struck through, no line to them, not moved.
    private(set) var goneCards: Set<String> = []
    /// Those cards by id, as main has them.
    private var goneNodes: [String: SchemaNode] = [:]
    /// How a card ghost drawn now is drawn: its header, and whether it leads nowhere.
    func ghostLook(of id: String) -> CardRenderer.GhostLook? { ghostCards.contains(id) ? ghostLooks[id] : nil }

    /// A card drawn here, its rows to lay out: the section's own, a card ghost's box, or one a
    /// merge looked at takes away.
    private func drawnCard(_ id: String) -> SchemaNode? { schemasById[id] ?? ghostNodes[id] ?? goneNodes[id] }

    private var visible: [SchemaNode] = []
    private var rows: [String: CardIndex] = [:]
    private var boxes: [String: CGRect] = [:]
    private(set) var edges: [GraphEdge] = []
    /// Content-space origin, subtracted from every coordinate so layer geometry
    /// starts at zero even when cards sit at negative coordinates.
    private var origin: CGPoint = .zero
    private var contentSize: CGSize = .zero

    private var selectedSchemaId: String?

    /// How far each card is scrolled, in rows. Absent means the top.
    ///
    /// Kept here rather than on the project: it is where someone is looking right now,
    /// not something about the model. It survives a filter change because the card
    /// is still the same card, and is clamped on every rebuild in case the card got
    /// shorter underneath it.
    private var cardScroll: [String: Int] = [:]
    /// Leftover scroll distance, so a trackpad moves rows at the speed of the hand
    /// rather than one row per event.
    private var cardScrollCarry: [String: CGFloat] = [:]
    /// The card this scroll gesture belongs to, held until the next one begins.
    private var scrolling: String?

    /// Which rows are folded, per card, by path. Where someone is looking rather
    /// than something about the model, so it lives here and not on the project. A path
    /// that no longer exists simply folds nothing.
    private var cardFolds: [String: Set<String>] = [:]
    /// Cards whose folding someone has set by hand. The default in the settings is
    /// about how a card *opens*; once it has been folded or opened here, saying so
    /// again on the next rebuild would undo the choice while they were looking at it.
    private var touchedCards: Set<String> = []

    /// Who lights up for whom. Built once per diagram, read on every hover.
    private var neighbours: [String: Set<String>] = [:]
    private var hovered: String?

    // MARK: - Layers

    private let contentLayer = CALayer()
    private var cardLayers: [String: CALayer] = [:]

    /// Which card is in front of which, back to front.
    ///
    /// One order, read by the drawing and the pointer alike. With two — the pointer
    /// walking the project's schema order, the drawing following the order the layers
    /// happened to be created in — a card that had been filtered away and came back
    /// would be drawn on top while the pointer still treated it as wherever the project
    /// listed it. A click on the card in front would land on the one behind, and a
    /// drag would carry the wrong card off.
    private(set) var stack: [String] = []
    private var edgeLayers: [String: CAShapeLayer] = [:]
    /// The mark at the owner's end. A layer of its own so it can be filled while
    /// the line is stroked, and so a dashed line does not dash the mark.
    private var capLayers: [String: CAShapeLayer] = [:]
    /// The arrowhead at the target's end. Separate again because it is stroked and
    /// never filled — sharing a layer with a filled diamond would fill it too.
    private var headLayers: [String: CAShapeLayer] = [:]
    private var labelLayers: [String: CATextLayer] = [:]
    /// The frames drawn round groups of cards (`Surroundings.frames`), by id: the dashed box and its
    /// title. Behind the lines, so a line to a card in a group is not cut by its frame.
    private var frameLayers: [String: (box: CAShapeLayer, title: CATextLayer)] = [:]
    /// Where each frame is drawn now, in content points: round its cards as they are, dragged too.
    private(set) var groupFrames: [String: CGRect] = [:]

    /// Row highlights ride above the cards as their own layers rather than being
    /// drawn into them. A card is a texture; re-cutting one to tint a single row
    /// would cost the whole card for the width of a pointer move.
    private let hoverRowLayer = CALayer()
    /// The grid, faintly (§12). Drawn in the view's own coordinates for what is on
    /// screen only, and again whenever the view moves: a layer the size of the whole
    /// diagram was re-rasterised at every zoom and weighed on every frame of a drag.
    private let gridLayer = CAShapeLayer()
    private let selectedRowLayer = CALayer()

    private(set) var selectedFieldId: String?
    private var hoveredRow: (schemaId: String, index: Int)?

    /// The line picked, by its id (`Relation.connection`): drawn thicker, glowing.
    private(set) var pickedLine: String?
    /// The handle a line is drawn by hand from (つなぐ): on the right edge of the row or
    /// header under the pointer, where a line may leave it (`canConnect`).
    private let handleLayer = CAShapeLayer()
    private let handleMark = CAShapeLayer()
    /// The line being dragged from a handle, to the pointer.
    private let connectLayer = CAShapeLayer()
    /// The row or header the handle shows on, and where it sits (content points).
    private(set) var handleAt: (hit: RowHit, point: CGPoint)?
    /// Whether the handle is drawn now — for the tests.
    var handleShown: Bool { !handleLayer.isHidden }
    /// A line being dragged from a handle: from what, and from where.
    private var connectDrag: (source: RowHit, from: CGPoint)?
    /// How near the handle a press takes it, in content points.
    private let handleReach: CGFloat = 8
    /// Each drawn line's curve, by its edge's key: where a line is, for a press to find.
    private var wires: [String: GraphEdges.Wire] = [:]
    /// Whether the lines were last cut round the cards: to cut them again when this
    /// Mac's setting changes (`LineRouting`).
    private var routedNow = false
    /// Whether the lines go round the cards now — for the tests.
    var linesRouted: Bool { routedNow }
    /// Whether the lines stay curves whatever this Mac's setting (`LineRouting`): a card's connections
    /// map, whose lines fan in to the card in the middle from the groups round it — cut round the cards
    /// on the grid, they would zig-zag through the other groups.
    var curvesOnly = false {
        didSet { if curvesOnly != oldValue, !edges.isEmpty { withoutAnimation { cutEdges() } } }
    }
    /// Whether the lines are to go round the cards here.
    private var routesLines: Bool { !curvesOnly && LineRouting.avoidsCards }
    /// The corners of the line drawn for the relation `id` (a connection's id), round the
    /// cards; empty for a curve — for the tests.
    func corners(ofLine id: String) -> [CGPoint] {
        guard let edge = edges.first(where: { $0.vias.contains { $0.connection == id } }) else { return [] }
        return (wires[edge.key]?.corners ?? []).map { CGPoint(x: $0.x + origin.x, y: $0.y + origin.y) }
    }

    /// Below this, a field row is too small to hit on purpose, so a click means the
    /// card. Same threshold the web build uses.
    private let rowPickZoom: CGFloat = 0.5

    // MARK: - Viewport

    private(set) var zoom: CGFloat = 1
    private var offset: CGPoint = .zero

    /// Two floors, because "as far out as I can pinch" and "as far out as this
    /// diagram needs" are different numbers. A real project spans 9592 × 18947, which
    /// asks for 4.7% in a 1194 × 944 viewport — clamping that would put two thirds
    /// of the cards off screen and make 全体表示 look like a dead button.
    private let minZoom: CGFloat = 0.1
    private let fitMinZoom: CGFloat = 0.02
    private let maxZoom: CGFloat = 2.5

    /// Debounce for the work that only matters once the hand stops.
    private var settleWork: DispatchWorkItem?

    let probe = FrameProbe()

    private var theme: GraphTheme { GraphTheme.current(for: effectiveAppearance) }

    // MARK: - Setup

    override init(frame: NSRect) {
        super.init(frame: frame)
        setUp()
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        setUp()
    }

    private func setUp() {
        wantsLayer = true
        // This Mac's way of drawing lines changed (`LineRouting`): cut them again that way.
        NotificationCenter.default.addObserver(forName: UserDefaults.didChangeNotification, object: nil, queue: .main) { [weak self] _ in
            MainActor.assumeIsolated {
                guard let self, self.drag == nil, self.resizing == nil, !self.edges.isEmpty,
                      self.routedNow != self.routesLines else { return }
                self.withoutAnimation { self.cutEdges() }
            }
        }
        // The screen's language changed: what the cards say is made again in it. Their rows, the words
        // of their ghosts and their documents' texts are kept from the last time they were made.
        NotificationCenter.default.addObserver(forName: Language.changed, object: nil, queue: .main) { [weak self] _ in
            MainActor.assumeIsolated { self?.speakAgain() }
        }
        // Sublayers are positioned in content coordinates, which run top-left down.
        // `isFlipped` below only settles drawing and event coordinates for the view
        // itself; manually added sublayers keep the layer's own bottom-left origin
        // until this is set.
        layer?.isGeometryFlipped = true
        layer?.backgroundColor = theme.background.cgColor
        // The canvas is a window onto a diagram far larger than itself, so the root
        // layer has to clip. `contentLayer` deliberately does not — its children sit
        // wherever the layout put them, well outside its own bounds — which means
        // without this the cards draw straight over the toolbar above.
        layer?.masksToBounds = true
        contentLayer.anchorPoint = .zero
        contentLayer.masksToBounds = false
        layer?.addSublayer(contentLayer)
        gridLayer.fillColor = nil
        gridLayer.lineWidth = 1
        gridLayer.anchorPoint = .zero
        gridLayer.actions = ["path": NSNull(), "bounds": NSNull(), "position": NSNull(), "hidden": NSNull()]
        layer?.insertSublayer(gridLayer, below: contentLayer)
        restyleGrid()

        for highlight in [selectedRowLayer, hoverRowLayer] {
            highlight.anchorPoint = .zero
            highlight.isHidden = true
            highlight.cornerRadius = 3
            contentLayer.addSublayer(highlight)
        }
        // Above every card: the handle sits on a card's edge, the line dragged crosses them.
        handleLayer.bounds = CGRect(x: 0, y: 0, width: 12, height: 12)
        handleLayer.path = CGPath(ellipseIn: CGRect(x: 1, y: 1, width: 10, height: 10), transform: nil)
        handleLayer.lineWidth = 1.5
        handleLayer.isHidden = true
        handleLayer.zPosition = Depth.card + Depth.cardBand + 1
        let plus = CGMutablePath()
        plus.move(to: CGPoint(x: 6, y: 3.5))
        plus.addLine(to: CGPoint(x: 6, y: 8.5))
        plus.move(to: CGPoint(x: 3.5, y: 6))
        plus.addLine(to: CGPoint(x: 8.5, y: 6))
        handleMark.path = plus
        handleMark.lineWidth = 1.4
        handleMark.lineCap = .round
        handleMark.frame = handleLayer.bounds
        handleLayer.addSublayer(handleMark)
        contentLayer.addSublayer(handleLayer)
        connectLayer.anchorPoint = .zero
        connectLayer.fillColor = nil
        connectLayer.lineWidth = 1.5
        connectLayer.lineDashPattern = [5, 3]
        connectLayer.lineCap = .round
        connectLayer.isHidden = true
        connectLayer.zPosition = Depth.card + Depth.cardBand + 1
        contentLayer.addSublayer(connectLayer)
        // The region drawn in the selection mode.
        marqueeLayer.anchorPoint = .zero
        marqueeLayer.lineWidth = 1
        marqueeLayer.lineDashPattern = [4, 3]
        marqueeLayer.isHidden = true
        marqueeLayer.zPosition = Depth.card + Depth.cardBand + 1
        contentLayer.addSublayer(marqueeLayer)
        for layer in [handleLayer, handleMark, connectLayer, marqueeLayer] {
            layer.actions = ["position": NSNull(), "path": NSNull(), "hidden": NSNull(), "bounds": NSNull()]
        }
        restyleHighlights()

        let area = NSTrackingArea(
            rect: .zero,
            options: [.activeInKeyWindow, .inVisibleRect, .mouseMoved, .mouseEnteredAndExited],
            owner: self)
        addTrackingArea(area)
    }

    /// What sits in front of what.
    ///
    /// Sublayer order would decide this otherwise, and sublayer order is creation
    /// order — which changes as cards and edges come and go with the project. Left to
    /// that, a cardinality label could sit on top of a card whose own line ran behind
    /// it, reading as though the label belonged to the field it happened to land on.
    ///
    /// A line runs *behind* the cards it connects. It is the cards that are being
    /// read; a line crossing one would take a row away to say something about two
    /// other cards. Its label goes with it.
    ///
    /// Cards share a band, one step each in `stack` order. A row highlight sits a
    /// fraction of a step above the card it marks — not above every card, where the
    /// selected row of a card behind would show through the card in front of it, as
    /// a bar across somebody else's fields.
    private enum Depth {
        static let frame: CGFloat = -1
        static let edge: CGFloat = 0
        static let label: CGFloat = 1
        static let card: CGFloat = 5
        static let cardBand: CGFloat = 4
        static let selectedRow: CGFloat = 0.25
        static let hoverRow: CGFloat = 0.5
        /// A ui part's tag, note and dot: above every card, as the mock lays them over every part; and
        /// the outlines of the part under the pointer and of those picked above them, so a part picked
        /// under others is still seen.
        static let decor: CGFloat = card + cardBand + 0.1
        static let decorBand: CGFloat = 0.4
        static let hoverOutline: CGFloat = card + cardBand + 0.6
        static let pickOutline: CGFloat = card + cardBand + 0.7
    }

    private var depthStep: CGFloat { Depth.cardBand / CGFloat(max(stack.count, 1)) }

    private func depth(ofCard id: String) -> CGFloat {
        Depth.card + CGFloat(stack.firstIndex(of: id) ?? 0) * depthStep
    }

    /// Put every card, and the two highlights, at the depth `stack` gives them.
    private func restack() {
        let step = depthStep
        for (rank, id) in stack.enumerated() {
            cardLayers[id]?.zPosition = Depth.card + CGFloat(rank) * step
            partDecorLayers[id]?.zPosition = Depth.decor + CGFloat(rank) * Depth.decorBand / CGFloat(max(stack.count, 1))
        }
        if let id = hoveredRow?.schemaId {
            hoverRowLayer.zPosition = depth(ofCard: id) + Depth.hoverRow * step
        }
        if let id = selectedSchemaId {
            selectedRowLayer.zPosition = depth(ofCard: id) + Depth.selectedRow * step
        }
    }

    /// Bring a card to the front — the one being handled is the one being looked at,
    /// and a card dragged across another should not slide underneath it.
    /// Never in a ui section: its order is the section's, as the mock stacks it.
    func raise(_ id: String) {
        guard !drawsParts, let at = stack.firstIndex(of: id), at != stack.count - 1 else { return }
        stack.append(stack.remove(at: at))
        withoutAnimation { restack() }
    }

    override var isFlipped: Bool { true }
    override var acceptsFirstResponder: Bool { true }

    /// Return or Enter on what is picked does what a double-click on it does (overview §13).
    override func keyDown(with event: NSEvent) {
        // Esc: out of the selection mode.
        if event.keyCode == 53, selecting {
            setSelecting(false)
            return
        }
        let returnKeys: Set<UInt16> = [36, 76]
        if returnKeys.contains(event.keyCode), let hit = picked {
            if canEditInPlace?(hit.card) == true {
                beginInPlace(hit.card)
            } else {
                onFollow?(hit)
            }
            return
        }
        super.keyDown(with: event)
    }

    /// What is picked here: the card, and its row marked — nil when nothing is, or the card
    /// is no longer drawn.
    var picked: RowHit? {
        guard let id = selectedSchemaId, cardLayers[id] != nil else { return nil }
        let row = selectedFieldId.flatMap { key in rows[id]?.content.all.first { Self.rowKey($0) == key } }
        return RowHit(card: id, row: row)
    }

    /// How a row is marked as picked: a field by its id — a field's path can be the same on
    /// two cards' inlined copies — any other row by its path on the card.
    nonisolated static func rowKey(_ row: CardRow) -> String { row.field?.id ?? row.path }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        layer?.backgroundColor = theme.background.cgColor
        restyleGrid()
        redrawAllCards()
        restyleEdges()
        restyleFrames()
        restyleHighlights()
    }

    // MARK: - Loading a project

    /// Section cards that stand for what this section points at — drawn on the
    /// right; the others, what points into it, on the left.
    private var sectionGhostSides: Set<String> = []
    /// The section ghosts drawn now, by id — cards that stand for other cards.
    private var sectionGhostNodes: [String: SchemaNode] = [:]
    /// The boxes of a working directory's folders that reading the folder no longer finds (`Surroundings`).
    private var missingFolders: Set<String> = []
    func show(_ detail: ProjectDetail, filters: GraphFilters = GraphFilters(), fit: Bool = false,
              surroundings: Surroundings = Surroundings(), settings: LayoutSettings = .standard,
              cardWidth: CGFloat = CardGeometry.cardWidth, marks: BranchMarks = BranchMarks()) {
        DiagramTimes.measure("show") {
            showing(detail, filters: filters, fit: fit, surroundings: surroundings, settings: settings,
                    cardWidth: cardWidth, marks: marks)
        }
    }

    private func showing(_ detail: ProjectDetail, filters: GraphFilters, fit: Bool, surroundings: Surroundings,
                         settings: LayoutSettings, cardWidth: CGFloat, marks: BranchMarks) {
        self.marks = marks
        // Only a different project is worth fitting to. Re-reading the same one happens
        // after every write, and pulling the view back to the whole diagram each time
        // takes it away from whatever was being worked on.
        let arrived = self.detail?.project.id != detail.project.id
        self.detail = detail
        let directory = detail.directory
        tree = detail.schemas.first.map { directory.treeWord(of: $0) }
        self.settings = settings
        self.cardWidth = cardWidth
        CardGeometry.maxRows = settings.size.rows
        self.filters = Self.effective(filters).drawing(with: detail.lineKinds)
        schemasById = Dictionary(uniqueKeysWithValues: detail.schemas.map { ($0.id, $0) })
        self.surroundings = surroundings
        outsiders = Dictionary(surroundings.schemas.map { schema in
            var named = schema
            named.name = surroundings.names[schema.id] ?? schema.name
            return (schema.id, named)
        }, uniquingKeysWith: { first, _ in first })
        let reals = Dictionary((detail.schemas + surroundings.schemas).map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        throughGhosts = Dictionary(surroundings.everyGhost.compactMap { ghost in
            guard let card = ghost.resolvesTo.flatMap({ reals[$0] }) else { return nil }
            var named = card
            named.name = surroundings.ghostNames[ghost.id] ?? ghost.name
            return (ghost.id, named)
        }, uniquingKeysWith: { first, _ in first })
        ghostNodes = Dictionary(surroundings.ghosts.map { ghost in
            (ghost.id, Self.box(for: ghost, reals: reals, problem: surroundings.ghostProblems[ghost.id]))
        }, uniquingKeysWith: { first, _ in first })
        goneNodes = Dictionary(surroundings.gone.filter { schemasById[$0.id] == nil }.map { ($0.id, $0) },
                               uniquingKeysWith: { first, _ in first })
        ghostLooks = Dictionary(surroundings.ghosts.map { ghost in
            (ghost.id, CardRenderer.GhostLook(title: surroundings.ghostTitles[ghost.id] ?? ghost.name,
                                              broken: ghost.resolvesTo.flatMap { reals[$0] } == nil))
        }, uniquingKeysWith: { first, _ in first })
        rebuild()
        if arrived || fit { fitToView() }
        layoutInPlace()
        drawUndrawn()
        // The pages of the cards it no longer shows are not drawn for it, nor their pictures held.
        DocSnapshots.shared.withdraw(owner: self, keeping: drawnIds)
        docShown = docShown.filter { drawnIds.contains($0.key) }
        partShown = partShown.filter { drawnIds.contains($0.key) && isPart($0.key) }
    }

    /// The filters as the canvas applies them: every card holds its value objects
    /// and enums in place (`Nesting.always`), whatever the filters were given.
    static func effective(_ filters: GraphFilters) -> GraphFilters {
        var out = filters
        out.nesting = .always
        return out
    }

    func apply(_ filters: GraphFilters) {
        let filters = Self.effective(filters).drawing(with: detail?.lineKinds ?? LineKinds())
        guard filters != self.filters else { return }
        let cardsMayHaveChanged = filters.frameKey != self.filters.frameKey
        self.filters = filters
        rebuild()
        // Switching which cards exist rearranges the whole diagram, so the old
        // viewport is pointing at nothing in particular. Only then is it worth
        // moving the view out from under the user.
        if cardsMayHaveChanged { fitToView() }
        drawUndrawn()
    }

    /// Where the view looks, to come back to: the diagram point in the middle
    /// of the panel, and the zoom — kept apart from the panel's size, which may differ
    /// when it comes back.
    struct Look: Hashable {
        var center: CGPoint
        var zoom: CGFloat
    }

    /// Where it looks now; nil before it has a size, when it is looking at nothing yet.
    var look: Look? {
        guard bounds.width > 0, bounds.height > 0 else { return nil }
        return Look(center: toContent(CGPoint(x: bounds.midX, y: bounds.midY)), zoom: zoom)
    }

    /// A look and a card to show again (`whole` as `reveal` takes it), held until the panel has its size.
    private var pendingLook: Look?
    private var pendingReveal: (card: String, whole: Bool)?

    /// Look where it looked before — at once, or once the panel has its size.
    func restore(_ look: Look) {
        guard bounds.width > 0, bounds.height > 0 else { pendingLook = look; return }
        center(on: look.center, zoom: look.zoom)
    }

    /// Select a card, moving the view only when it is not on screen — coming back to a
    /// diagram from the card's editor leaves the view where it was. `whole`: moved unless all of
    /// it is on screen, as little as takes it in, the zoom kept — a card just put on the diagram is
    /// seen entire where it went, and what was being looked at stays as near as it can.
    func reveal(_ schemaId: String, whole: Bool = false) {
        guard bounds.width > 0, bounds.height > 0, pendingLook == nil else { pendingReveal = (schemaId, whole); return }
        guard let box = boxes[schemaId] else { return }
        let previous = selectedSchemaId
        selectedSchemaId = schemaId
        raise(schemaId)
        selectField(schemaId, nil)
        withoutAnimation {
            if let previous, previous != schemaId {
                drawCard(previous, scale: wantedScale(previous))
            }
            drawCard(schemaId, scale: wantedScale(schemaId))
            syncPartOutlines()
        }
        if whole {
            if !visibleArea.contains(box) { scrollToWhole(box) }
        } else if !visibleArea.intersects(box) {
            center(on: CGPoint(x: box.midX, y: box.midY), zoom: zoom)
        }
    }

    /// How far in from the panel's edge a card brought into view whole sits, in the panel's points.
    private let revealMargin: CGFloat = 24

    /// The view moved, at the zoom it is at, as little as takes `box` in on each side it is off —
    /// `revealMargin` in, where there is room for it; its top or left end in view where it is longer
    /// than the panel, so its header is what is seen.
    private func scrollToWhole(_ box: CGRect) {
        let seen = visibleArea
        let margin = revealMargin / zoom
        // How far the part on screen moves along one side (content points).
        func shift(_ low: CGFloat, _ high: CGFloat, _ seenLow: CGFloat, _ seenHigh: CGFloat) -> CGFloat {
            let room = seenHigh - seenLow
            let pad = max(0, min(margin, (room - (high - low)) / 2))
            if high - low > room || low < seenLow { return low - pad - seenLow }
            if high > seenHigh { return high + pad - seenHigh }
            return 0
        }
        offset.x -= shift(box.minX, box.maxX, seen.minX, seen.maxX) * zoom
        offset.y -= shift(box.minY, box.maxY, seen.minY, seen.maxY) * zoom
        commitViewport()
    }

    /// A card to go to, and the row to mark, held until the panel has its size and has
    /// looked where it looked before — the look put back after would move the view away.
    private var pendingFocus: (card: String, row: String?)?

    /// Bring a card into view and select it — and mark `row` (`rowKey`) when one is given.
    /// References are the main way to move around a project, so following one has to actually
    /// go there.
    func focus(_ schemaId: String, row: String? = nil) {
        guard bounds.width > 0, bounds.height > 0, pendingLook == nil else {
            pendingFocus = (schemaId, row)
            return
        }
        guard let box = boxes[schemaId] else { return }
        let previous = selectedSchemaId
        selectedSchemaId = schemaId
        raise(schemaId)
        selectField(schemaId, row)

        // Close in if the diagram is zoomed out past reading distance, but never
        // pull back from a closer look the reader already chose.
        if zoom < rowPickZoom { zoom = rowPickZoom }
        offset = CGPoint(x: bounds.midX - (box.midX - origin.x) * zoom,
                         y: bounds.midY - (box.midY - origin.y) * zoom)

        withoutAnimation {
            if let previous, previous != schemaId {
                drawCard(previous, scale: wantedScale(previous))
            }
            drawCard(schemaId, scale: wantedScale(schemaId))
            syncPartOutlines()
            applyViewport()
        }
        onZoomChanged?(zoom)
        scheduleSettle()
    }

    /// Zoom a step about the middle of the viewport, for the toolbar's ± buttons.
    func nudgeZoom(_ factor: CGFloat) {
        setZoom(zoom * factor, anchor: CGPoint(x: bounds.midX, y: bounds.midY))
    }

    /// Everything derived from the project. Called when the project or the filters change —
    /// never when the viewport does.
    private func rebuild() {
        DiagramTimes.measure("rebuild") { rebuilding() }
    }

    private func rebuilding() {
        guard let detail else { return }
        let lookup = self.lookup

        // Value objects and enums have no card of their own; the ones this section
        // placed — kept or for now — are drawn whole.
        let placedHere = detail.schemas.filter { surroundings.here.contains($0.id) }
        let own = visibleSchemas(detail, filters) + placedHere
        let ownIds = Set(own.map(\.id))
        // Its card ghosts: boxes of its own, where each sits, the card it leads to drawn
        // whole — as that card's kind is let in by the switches. The card each leads to is
        // drawn there, not again beside the section; the lines to it land on the ghost.
        let ghostBoxes = surroundings.ghosts.compactMap { ghost -> SchemaNode? in
            guard let box = ghostNodes[ghost.id] else { return nil }
            return ghostLooks[ghost.id]?.broken == true || filters.kinds.contains(box.kind) ? box : nil
        }
        let ghostIds = Set(ghostBoxes.map(\.id))
        var standIns: [String: String] = [:]
        for ghost in surroundings.ghosts where ghostIds.contains(ghost.id) {
            guard let card = ghost.resolvesTo, !ownIds.contains(card), standIns[card] == nil else { continue }
            standIns[card] = ghost.id
        }
        // The other sections' cards: the references and consumers the switches let
        // in. Another section's value object or enum is inside the card that holds
        // it, like this section's own: it comes as a card only when placed.
        // In a section with sections inside, also its children's cards that link to
        // one another's: the line between them is drawn too (design-sections §6).
        let childLinks = surroundings.across.filter {
            !surroundings.references.contains($0) && !surroundings.consumers.contains($0)
        }
        let shown = ((filters.outsideReferences ? surroundings.references : [])
            + (filters.outsideConsumers ? surroundings.consumers : []) + childLinks)
            .filter { outsiders[$0].map { $0.kind != .value && $0.kind != .enum } ?? false }
            .filter { standIns[$0] == nil }
        let outsideIds = surroundings.forNow.filter { !shown.contains($0) && standIns[$0] == nil } + shown
        // What the merge takes away: where main puts it, as the cards of the section are.
        let goneBoxes = surroundings.gone.compactMap { goneNodes[$0.id] }
        let goneIds = Set(goneBoxes.map(\.id))
        visible = own + ghostBoxes + goneBoxes + outsideIds.compactMap { outsiders[$0] }
        applyFoldDefault(to: visible, lookup: lookup)
        forms = Dictionary(visible.map { ($0.id, cardForm($0, settings: settings, cardWidth: cardWidth)) },
                           uniquingKeysWith: { a, _ in a })
        rows = GraphEdges.index(visible, lookup: lookup, filters: filters,
                                scroll: cardScroll, folds: cardFolds, tree: tree, forms: forms)
        // Read back what each card settled on. `cardRows` clamps, so a card that
        // lost rows to a filter keeps a legal offset instead of an empty window.
        for (id, card) in rows where cardScroll[id] != nil {
            cardScroll[id] = card.content.first
        }
        cardScroll = cardScroll.filter { rows[$0.key] != nil && $0.value > 0 }

        // Lines are about this section: one end on its own card — or, in a section
        // with sections inside, between two of its children. Resolved against the
        // whole project, with the names each card's own section gives them.
        let everything = detail.schemas + surroundings.schemas
        let childOf = surroundings.children
        // The section's own names: its cards, and its card ghosts — between two of them a
        // line is about the section too.
        let named = ownIds.union(ghostIds)
        func aboutHere(_ relation: Relation) -> Bool {
            if ownIds.contains(relation.fromSchemaId) || ownIds.contains(relation.toSchemaId) { return true }
            if named.contains(relation.fromSchemaId) && named.contains(relation.toSchemaId) { return true }
            guard let from = childOf[relation.fromSchemaId], let to = childOf[relation.toSchemaId] else { return false }
            return from != to
        }
        // The project's lines, put on what is drawn here (`GraphEdges.placed`).
        let holders = GraphEdges.Holders(detail, more: everything + surroundings.inside)
        // The section's own lines and the rest of the project's (`Surroundings.lines`).
        let lines = detail.relations + surroundings.lines
        // None to or from what the merge takes away: it is gone, and its lines with it.
        func notGone(_ relation: Relation) -> Bool {
            !goneIds.contains(relation.fromSchemaId) && !goneIds.contains(relation.toSchemaId)
        }
        var relations = GraphEdges.placed(lines, visible: visible, rows: rows, filters: filters,
                                          holders: holders, standIns: standIns)
            .filter(aboutHere).filter(notGone)
        // Asked with nothing hidden, before the other sections' cards are folded: the kinds
        // the legend can switch.
        var unfiltered = filters
        unfiltered.hiddenLines = []
        let available = Set(GraphEdges.placed(lines, visible: visible, rows: rows, filters: unfiltered,
                                              holders: holders, standIns: standIns)
            .filter(aboutHere).filter(notGone)
            .map(\.line))
        // Another section's card is there for its line. With the line switched off
        // it would stand alone saying nothing. A card ghost, and a card put there for a
        // look, stays whether or not a line reaches it, and is drawn whole.
        let whole = Set(surroundings.forNow).union(ghostIds)
        let linked = Set(relations.flatMap { [$0.fromSchemaId, $0.toSchemaId] }).union(whole)
        visible.removeAll { !ownIds.contains($0.id) && !linked.contains($0.id) && !goneIds.contains($0.id) }
        // The rest go into one card per section, holding only what is pointed at,
        // nested under the card it belongs to.
        let folded = SectionGhosts.fold(relations, own: ownIds, whole: whole, schemas: everything,
                                       names: surroundings.names, projectId: detail.project.id,
                                       groups: childOf, titles: surroundings.childNames)
        relations = folded.relations
        // A working directory's folders: a box each, whether or not a line reaches into it.
        let foldedIds = Set(folded.cards.map(\.id))
        let folderBoxes = surroundings.folders.filter { !foldedIds.contains($0.id) }.map { folder in
            var box = SchemaNode(id: folder.id, projectId: detail.project.id, name: folder.name, label: "", kind: .entity,
                                 description: "", tags: [], metadata: [:], rules: [], absentFields: [],
                                 discriminator: nil, enumOpen: false, posX: nil, posY: nil, pinned: false,
                                 sortOrder: 0, fields: [], values: [])
            box.ownKind = WorkDirIds.directoryKind
            return box
        }
        missingFolders = surroundings.missingFolders
        visible = visible.filter { ownIds.contains($0.id) || whole.contains($0.id) || goneIds.contains($0.id) }
            + folded.cards + folderBoxes
        sectionGhostNodes = Dictionary((folded.cards + folderBoxes).map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        sectionGhostSides = folded.referenced
        forms = Dictionary(visible.map { ($0.id, cardForm($0, settings: settings, cardWidth: cardWidth)) },
                           uniquingKeysWith: { a, _ in a })
        rows = GraphEdges.index(visible, lookup: lookup, filters: filters,
                                scroll: cardScroll, folds: cardFolds, tree: tree, forms: forms)
        outsideCards = Set(visible.map(\.id)).subtracting(ownIds).subtracting(ghostIds).subtracting(goneIds)
        // What a region picked and is no longer drawn, let go.
        picks.formIntersection(Set(visible.map(\.id)))
        ghostCards = ghostIds.intersection(visible.map(\.id))
        goneCards = goneIds.intersection(visible.map(\.id))

        var contents: [String: CardContent] = [:]
        for s in visible {
            contents[s.id] = rows[s.id]?.content ?? cardRows(
                s, lookup: lookup, inlineReuse: filters.nesting.nests(s.id), tree: tree)
        }
        // Where each card sits, in grid units (§12): its own place — drawn there as it
        // is, however tall it has grown — or, for one with none yet, the first free
        // cell. That one is drawn there, not written.
        boxes = [:]
        var taken: [GridRect] = []
        var unplaced: [SchemaNode] = []
        // A card ghost sits where it was put, as a card does.
        for s in visible where ownIds.contains(s.id) || ghostIds.contains(s.id) || goneIds.contains(s.id) {
            guard let content = contents[s.id] else { continue }
            guard let x = s.posX, let y = s.posY else {
                unplaced.append(s)
                continue
            }
            let at = Grid.nearest(x: x, y: y)
            let size = boxSize(s.id, content)
            boxes[s.id] = CGRect(origin: points(at), size: size)
            taken.append(gridRect(of: boxes[s.id]!))
        }
        for s in unplaced {
            guard let content = contents[s.id] else { continue }
            let at = settings.nextSpot(avoiding: taken)
            let size = boxSize(s.id, content)
            boxes[s.id] = CGRect(origin: points(at), size: size)
            taken.append(gridRect(of: boxes[s.id]!))
        }
        placeOutsideCards(contents, taken: taken)

        edges = GraphEdges.merge(relations)
        buildNeighbours()

        let bounds = GraphEdges.bounds(boxes)
        origin = bounds.origin
        contentSize = bounds.size

        withoutAnimation {
            contentLayer.bounds = CGRect(origin: .zero, size: contentSize)
            syncGrid()
            syncCardLayers()
            syncFrameLayers()
            syncEdgeLayers()
            // Both point at a row that may no longer be where it was, or exist.
            hoverRowLayer.isHidden = true
            selectedRowLayer.isHidden = true
            hoveredRow = nil
            hideHandle()
            connectLayer.frame = CGRect(origin: .zero, size: contentSize)
            applyViewport()
        }
        if let id = selectedSchemaId { selectField(id, selectedFieldId) }
        onEdgeCountChanged?(edges.count)
        onKindsChanged?(available)
    }

    /// The grid, faintly, like a spreadsheet's (§12): a line at every unit across
    /// and down, over what is on screen. Zoomed out until units are too close to
    /// tell apart, every fifth or tenth line only.
    private func syncGrid() {
        guard detail != nil, !boxes.isEmpty else {
            gridLayer.path = nil
            return
        }
        gridLayer.frame = bounds
        let across = unit.width * zoom, down = unit.height * zoom
        func every(_ spacing: CGFloat) -> Int { [1, 2, 5, 10, 20, 50].first { spacing * CGFloat($0) >= 6 } ?? 100 }
        let stepX = every(across), stepY = every(down)
        // View x = (unit x - origin) * zoom + offset.
        let firstX = Int(((0 - offset.x) / zoom + origin.x) / unit.width / CGFloat(stepX)).advanced(by: -1) * stepX
        let firstY = Int(((0 - offset.y) / zoom + origin.y) / unit.height / CGFloat(stepY)).advanced(by: -1) * stepY
        let path = CGMutablePath()
        var column = firstX
        while true {
            let x = ((CGFloat(column) * unit.width - origin.x) * zoom + offset.x).rounded() + 0.5
            if x > bounds.width { break }
            if x >= 0 {
                path.move(to: CGPoint(x: x, y: 0))
                path.addLine(to: CGPoint(x: x, y: bounds.height))
            }
            column += stepX
        }
        var row = firstY
        while true {
            let y = ((CGFloat(row) * unit.height - origin.y) * zoom + offset.y).rounded() + 0.5
            if y > bounds.height { break }
            if y >= 0 {
                path.move(to: CGPoint(x: 0, y: y))
                path.addLine(to: CGPoint(x: bounds.width, y: y))
            }
            row += stepY
        }
        gridLayer.path = path
    }

    private func restyleGrid() {
        gridLayer.strokeColor = theme.line2.withAlphaComponent(theme.isDark ? 0.28 : 0.4).cgColor
    }

    /// Where the other sections' cards go (§12): where this section's reader last
    /// put one, or else beside the section — the ones it points at down the right,
    /// the ones pointing at it down the left, in project order, at their own height —
    /// with room between for the lines.
    private func placeOutsideCards(_ contents: [String: CardContent], taken own: [GridRect]) {
        var taken = own
        let margin = settings.margin, aside = max(4, settings.margin)
        let rightX = own.isEmpty ? 0 : own.map { $0.x + $0.w }.max()! + aside
        let leftX = own.isEmpty ? -(Grid.cardColumns + aside) : own.map(\.x).min()! - aside - Grid.cardColumns
        let top = own.map(\.y).min() ?? 0
        var rightY = top, leftY = top
        func stack(at x: Int, from y: inout Int, height: Int) -> GridPoint {
            var at = GridPoint(x: x, y: y)
            while let hit = taken.first(where: {
                $0.intersects(settings.withMargin(GridRect(x: at.x, y: at.y, w: Grid.cardColumns, h: height)))
            }) {
                at.y = hit.y + hit.h + margin
            }
            y = at.y + height + margin
            return at
        }
        let cardGhostOnly = surroundings.forNow.filter {
            !surroundings.references.contains($0) && !surroundings.consumers.contains($0)
        }
        // Section cards: the ones this section points into on the right, the ones
        // pointing into it on the left, like the cards they stand for.
        let sectionGhosts = visible.map(\.id).filter(SectionGhosts.isGhost)
        let rightCards = sectionGhosts.filter { sectionGhostSides.contains($0) }
        let leftCards = sectionGhosts.filter { !sectionGhostSides.contains($0) }
        let order = surroundings.references + cardGhostOnly + rightCards + surroundings.consumers + leftCards
        func height(_ id: String) -> CGFloat { contents[id] == nil ? 0 : boxSize(id, contents[id]).height }
        // Kept places first, so the others go around them — but not on this section's own cards: a card
        // placed since where one was kept (made through MCP, given the first free cell of the section's)
        // keeps its place, and the other goes beside the section again, as one with none kept does.
        for id in order where outsideCards.contains(id) {
            guard let kept = surroundings.positions[id], contents[id] != nil else { continue }
            let at = Grid.nearest(x: Double(kept.x), y: Double(kept.y))
            let box = CGRect(origin: points(at), size: boxSize(id, contents[id]))
            let cell = gridRect(of: box)
            guard !own.contains(where: { $0.intersects(cell) }) else { continue }
            boxes[id] = box
            taken.append(cell)
        }
        var placed: [String: GridPoint] = [:]
        for id in order where outsideCards.contains(id) && boxes[id] == nil && contents[id] != nil {
            let tall = Grid.rows(forHeight: Double(height(id)))
            let right = surroundings.references.contains(id) || cardGhostOnly.contains(id) || rightCards.contains(id)
            let at = right ? stack(at: rightX, from: &rightY, height: tall) : stack(at: leftX, from: &leftY, height: tall)
            boxes[id] = CGRect(origin: points(at), size: boxSize(id, contents[id]))
            taken.append(gridRect(of: boxes[id]!))
            if !surroundings.temporary.contains(id) { placed[id] = at }
        }
        if !placed.isEmpty { onPlaceOutside?(placed) }
    }

    private func buildNeighbours() {
        var near: [String: Set<String>] = [:]
        for edge in edges {
            near[edge.from, default: []].insert(edge.to)
            near[edge.to, default: []].insert(edge.from)
        }
        neighbours = near
    }

    // MARK: - Layers

    private func layerFrame(_ id: String) -> CGRect {
        (boxes[id] ?? .zero).offsetBy(dx: -origin.x, dy: -origin.y)
    }

    private func syncCardLayers() {
        // Cards that stay keep their place; a card arriving goes on top, which is
        // where a new layer has always been drawn.
        let present = Set(visible.map(\.id))
        let stacked = Set(stack)
        if drawsParts {
            // A ui section's parts lie as the mock lays them: in the section's order, the later above.
            var seen = Set<String>()
            stack = visible.map(\.id).filter { seen.insert($0).inserted }
        } else {
            stack = stack.filter { present.contains($0) }
                + visible.map(\.id).filter { !stacked.contains($0) }
        }

        var stale = Set(cardLayers.keys)
        for schema in visible {
            stale.remove(schema.id)
            let layer = cardLayers[schema.id] ?? {
                let l = CALayer()
                l.anchorPoint = .zero
                l.contentsGravity = .resize
                // A shadow per card would be 163 offscreen passes; the border does
                // the same job of lifting the card off the canvas.
                cardLayers[schema.id] = l
                contentLayer.addSublayer(l)
                return l
            }()
            layer.frame = layerFrame(schema.id)
        }
        for id in stale {
            cardLayers[id]?.removeFromSuperlayer()
            cardLayers.removeValue(forKey: id)
        }
        drawnIds = Set(cardLayers.keys)
        drawnDigest = drawnIds.hashValue
        syncPartLayers()
        // Drawn once the view is where it will look (`drawUndrawn`), at the scale it is seen at: not
        // first at the base scale and again finer as it settles.
        undrawn = drawnIds
        restack()
    }

    /// The cards laid out and not drawn yet (`drawUndrawn`).
    private var undrawn: Set<String> = []
    /// The cards that have a layer now, and a digest of them: a reference row names a card that has
    /// none, rather than promising a line.
    private var drawnIds: Set<String> = []
    private var drawnDigest = 0

    /// The cards not drawn yet, drawn at the scale they are seen at — once the view has its size and
    /// looks where it will; until then, as soon as it does (`layout`), or on the next turn at the
    /// latest, for a view that is never laid out.
    private func drawUndrawn() {
        guard !undrawn.isEmpty else { return }
        guard bounds.width > 0, bounds.height > 0, pendingLook == nil else {
            // In a window, it is laid out and drawn there; one that is not is drawn as it is.
            DispatchQueue.main.async { [weak self] in
                guard let self, self.window == nil else { return }
                self.drawUndrawnNow()
            }
            return
        }
        drawUndrawnNow()
    }

    private func drawUndrawnNow() {
        guard !undrawn.isEmpty else { return }
        let ids = undrawn
        undrawn = []
        DiagramTimes.measure("layers") {
            withoutAnimation {
                for id in ids { drawCard(id, scale: wantedScale(id)) }
            }
        }
    }

    /// The scale a card is drawn at where the view looks now: finer on screen once zoomed in far
    /// enough (`settle`), the base scale elsewhere.
    private func wantedScale(_ id: String) -> CGFloat {
        guard bounds.width > 0, bounds.height > 0, zoom >= CardRenderer.sharpFromZoom else { return CardRenderer.baseScale }
        return boxes[id]?.intersects(settleArea) == true ? CardRenderer.sharpScale : CardRenderer.baseScale
    }

    /// What `settle` draws finer: the part on screen, and a card's width around it.
    private var settleArea: CGRect {
        CGRect(origin: toContent(.zero), size: CGSize(width: bounds.width / zoom, height: bounds.height / zoom))
            .insetBy(dx: -CardGeometry.cardWidth, dy: -CardGeometry.cardWidth)
    }

    private func drawCard(_ id: String, scale: CGFloat) {
        guard let schema = schemasById[id] ?? ghostNodes[id] ?? goneNodes[id] ?? outsiders[id] ?? sectionGhostNodes[id],
              let layer = cardLayers[id],
              let content = rows[id]?.content else { return }
        undrawn.remove(id)
        if isPart(id) {
            drawPart(id, schema, layer: layer, scale: scale)
            return
        }
        let size = boxes[id]?.size ?? boxSize(id, content)
        let theme = self.theme
        let selected = selectedSchemaId == id || picks.contains(id)
        let outside = outsideCards.contains(id), temporary = surroundings.temporary.contains(id)
        let gone = goneCards.contains(id)
        let missing = missingFolders.contains(id)
        let mark: ChangeMark? = gone ? .removed : marks.mark(for: id)
        let document = content.document ? documentBody(schema, size: size) : nil
        if document != nil, document?.image == nil { docPending.insert(id) } else { docPending.remove(id) }
        let grip = resizable(id)
        // Everything the face shows: a card drawn before with all of it the same is not drawn again.
        var look = Hasher()
        look.combine(schema)
        look.combine(content)
        look.combine(theme.isDark)
        look.combine(selected)
        look.combine(drawnDigest)
        look.combine(outside)
        look.combine(temporary)
        look.combine(ghostLooks[id])
        look.combine(gone)
        look.combine(missing)
        look.combine(mark)
        look.combine(grip)
        look.combine(Language.current)
        look.combine(CardGeometry.maxRows)
        if let document {
            look.combine(document.picture)
            look.combine(document.image == nil)
            look.combine(document.size.width)
            look.combine(document.size.height)
            look.combine(document.zoom)
            look.combine(document.offset.x)
            look.combine(document.offset.y)
            look.combine(document.cut)
        }
        let key = CardImages.Key(card: id, look: look.finalize(), width: size.width, height: size.height, scale: scale)
        let image: CGImage
        if let kept = CardImages.shared.image(key) {
            image = kept
        } else {
            guard let made = CardRenderer.image(
                for: schema, content: content, theme: theme,
                scale: scale, selected: selected,
                drawn: drawnIds, outside: outside,
                temporary: temporary, ghost: ghostLooks[id],
                gone: gone, missing: missing, mark: mark,
                width: size.width, height: size.height,
                document: document, grip: grip) else { return }
            CardImages.shared.keep(made, for: key)
            image = made
        }
        layer.contents = image
        layer.contentsScale = scale
    }

    // MARK: - UI parts

    /// Whether `id` is drawn as a ui part: an html, svg or frame card of the ui section drawn — not another
    /// section's, a card ghost's box, nor one a merge looked at takes away.
    func isPart(_ id: String) -> Bool {
        guard drawsParts, let card = schemasById[id], !outsideCards.contains(id), !ghostCards.contains(id),
              !goneCards.contains(id) else { return false }
        return Self.partKinds.contains(card.kind)
    }

    static let partKinds: Set<SchemaKind> = [.html, .svg, .frame]

    /// The picture each part was last drawn with: drawn on while a newer face is read or taken, so a part
    /// keeps its look rather than going back to its outline.
    private var partShown: [String: DocSnapshots.Picture] = [:]
    /// The face each part was last drawn from: how fine its picture is asked for follows its size.
    private var partFaces: [String: PartFace] = [:]
    /// How fine each part's picture was last asked for.
    private var partSharp: [String: CGFloat] = [:]
    /// The parts waiting for a newer face (`partFace`): asked to call back once.
    private var partAwaiting: Set<String> = []
    /// Each part's tag, note and dot (`PartDecor`), a layer above every card, and where they lie in its box.
    private var partDecorLayers: [String: CALayer] = [:]
    private var partDecorLayouts: [String: PartDecor.Layout] = [:]
    /// What each part's layer was last drawn from: drawn again only when it changes.
    private var partDecorDrawn: [String: Int] = [:]
    /// The outline of the part under the pointer, and of each part picked.
    private let partHoverLayer = CALayer()
    private var partPickLayers: [String: CALayer] = [:]
    /// The part under the pointer.
    private(set) var hoveredPart: String?

    /// A part's face drawn: its picture in its whole box, nothing round it; while it has none, a thin dashed
    /// outline with its name.
    private func drawPart(_ id: String, _ schema: SchemaNode, layer: CALayer, scale: CGFloat) {
        let (picture, settled) = partPicture(schema)
        if let picture {
            layer.contents = picture.image
        } else {
            layer.contents = placeholder(schema, size: boxes[id]?.size ?? .zero)
        }
        layer.contentsScale = scale
        if settled { docPending.remove(id) } else { docPending.insert(id) }
        drawDecor(id)
    }

    /// A part's picture now, and whether it is the one of its current face (what a picture of the map waits for).
    private func partPicture(_ schema: SchemaNode) -> (DocSnapshots.Picture?, settled: Bool) {
        let id = schema.id
        guard let partFace else { return (partShown[id], true) }
        let asking = partAwaiting.insert(id).inserted
        let answer = partFace(schema, asking ? { [weak self] in
            self?.partAwaiting.remove(id)
            self?.redrawDocument(id)
        } : nil)
        let face: PartFace
        let current: Bool
        switch answer {
        case .none:
            if asking { partAwaiting.remove(id) }
            return (partShown[id], true)
        case .reading:
            return (partShown[id], false)
        case .face(let read, let now):
            if asking, now { partAwaiting.remove(id) }
            face = read
            current = now
        }
        partFaces[id] = face
        let dark = effectiveAppearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
        let sharp = partSharpness(id, face)
        partSharp[id] = sharp
        let urgent = bounds.width > 0 && bounds.height > 0 ? boxes[id].map(visibleArea.intersects) ?? false : true
        let taken = DocSnapshots.shared.picture(face.page, width: CGFloat(face.width), height: CGFloat(face.height),
                                                dark: dark, scale: sharp, urgent: urgent, owner: self, card: id,
                                                ready: { [weak self] in self?.redrawDocument(id) })
        if let taken { partShown[id] = taken }
        return (taken ?? partShown[id], current && taken != nil)
    }

    /// How fine a part's picture is taken: as fine as the screen shows it where the view looks, within what
    /// a picture may hold; the page's own points elsewhere.
    private func partSharpness(_ id: String, _ face: PartFace) -> CGFloat {
        guard bounds.width > 0, bounds.height > 0, boxes[id]?.intersects(settleArea) == true else { return 1 }
        let wanted = zoom * (window?.backingScaleFactor ?? 2)
        return DocSnapshots.scale(wanted, width: CGFloat(face.width), height: CGFloat(face.height))
    }

    /// A part with no picture yet: a thin dashed outline and its name, so where it lies is seen.
    private func placeholder(_ schema: SchemaNode, size: CGSize) -> CGImage? {
        let scale: CGFloat = 2
        let width = Int((size.width * scale).rounded(.up)), height = Int((size.height * scale).rounded(.up))
        guard width > 0, height > 0,
              let ctx = CGContext(data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0,
                                  space: CGColorSpace(name: CGColorSpace.sRGB)!,
                                  bitmapInfo: CGImageAlphaInfo.premultipliedFirst.rawValue) else { return nil }
        ctx.scaleBy(x: scale, y: scale)
        ctx.translateBy(x: 0, y: size.height)
        ctx.scaleBy(x: 1, y: -1)
        let previous = NSGraphicsContext.current
        NSGraphicsContext.current = NSGraphicsContext(cgContext: ctx, flipped: true)
        defer { NSGraphicsContext.current = previous }
        let theme = self.theme
        ctx.setStrokeColor(theme.ink3.cgColor)
        ctx.setLineWidth(1)
        ctx.setLineDash(phase: 0, lengths: [4, 3])
        ctx.stroke(CGRect(origin: .zero, size: size).insetBy(dx: 0.5, dy: 0.5))
        let style = NSMutableParagraphStyle()
        style.lineBreakMode = .byTruncatingTail
        (schema.name as NSString).draw(in: CGRect(x: 6, y: max(0, size.height / 2 - 8), width: max(0, size.width - 12), height: 16),
                                       withAttributes: [.font: NSFont.monospacedSystemFont(ofSize: 11, weight: .medium),
                                                        .foregroundColor: theme.ink3, .paragraphStyle: style])
        return ctx.makeImage()
    }

    /// The layers over the parts made for the parts drawn now, and let go for the others.
    private func syncPartLayers() {
        let parts = Set(cardLayers.keys.filter(isPart))
        for (id, layer) in partDecorLayers where !parts.contains(id) {
            layer.removeFromSuperlayer()
            partDecorLayers[id] = nil
            partDecorLayouts[id] = nil
            partDecorDrawn[id] = nil
        }
        partFaces = partFaces.filter { parts.contains($0.key) }
        partSharp = partSharp.filter { parts.contains($0.key) }
        for id in parts where partDecorLayers[id] == nil {
            let layer = CALayer()
            layer.anchorPoint = .zero
            layer.contentsGravity = .resize
            layer.actions = ["position": NSNull(), "bounds": NSNull(), "contents": NSNull(), "hidden": NSNull()]
            partDecorLayers[id] = layer
            contentLayer.addSublayer(layer)
        }
        if partHoverLayer.superlayer == nil {
            setUpOutline(partHoverLayer)
            partHoverLayer.zPosition = Depth.hoverOutline
            contentLayer.addSublayer(partHoverLayer)
        }
        if let hovered = hoveredPart, !parts.contains(hovered) { hoveredPart = nil }
        syncPartOutlines()
    }

    /// A part's tag, note and dot drawn, as the bar's switches say, where its box is now.
    private func drawDecor(_ id: String) {
        guard let layer = partDecorLayers[id], let schema = schemasById[id], let box = boxes[id] else { return }
        let layout = PartDecor.layout(schema, size: box.size, tags: partLook.tags, notes: partLook.notes)
        let scale = max(2, min(4, zoom * (window?.backingScaleFactor ?? 2)).rounded(.up))
        var look = Hasher()
        look.combine(schema.name)
        look.combine(schema.label)
        look.combine(schema.kind)
        look.combine(schema.customFields["note"])
        look.combine(schema.customFields["notePlace"])
        look.combine(schema.customFields["headerPlace"])
        look.combine(schema.showsReviewDot)
        look.combine(box.width)
        look.combine(box.height)
        look.combine(partLook.tags)
        look.combine(partLook.notes)
        look.combine(scale)
        let drawn = look.finalize()
        partDecorLayouts[id] = layout
        if partDecorDrawn[id] != drawn {
            partDecorDrawn[id] = drawn
            layer.contents = PartDecor.image(schema, layout, scale: scale)
            layer.contentsScale = scale
        }
        placeDecor(id)
    }

    /// A part's layers put where its box is now: moved with it as it is dragged.
    private func placeDecor(_ id: String) {
        guard let layer = partDecorLayers[id], let layout = partDecorLayouts[id] else { return }
        let box = layerFrame(id)
        layer.frame = layout.bounds.offsetBy(dx: box.minX, dy: box.minY)
    }

    private func setUpOutline(_ layer: CALayer) {
        layer.anchorPoint = .zero
        layer.borderColor = NSColor.controlAccentColor.cgColor
        layer.isHidden = true
        layer.actions = ["position": NSNull(), "bounds": NSNull(), "hidden": NSNull(), "borderColor": NSNull(),
                         "borderWidth": NSNull()]
        // The corner a part is sized by: shown only on the part under the pointer and those picked.
        let grip = CAShapeLayer()
        grip.anchorPoint = .zero
        grip.fillColor = nil
        grip.strokeColor = NSColor.controlAccentColor.cgColor
        grip.lineWidth = 1
        let size = CardRenderer.gripSize
        let path = CGMutablePath()
        for step in [CGFloat(4), 8, 12] {
            path.move(to: CGPoint(x: size - 3, y: size - 3 - step))
            path.addLine(to: CGPoint(x: size - 3 - step, y: size - 3))
        }
        grip.path = path
        grip.bounds = CGRect(x: 0, y: 0, width: size, height: size)
        grip.actions = ["position": NSNull(), "bounds": NSNull(), "hidden": NSNull(), "lineWidth": NSNull()]
        layer.addSublayer(grip)
    }

    /// An outline round `id`'s box, its corner's grip shown when the part may be sized.
    private func placeOutline(_ layer: CALayer, on id: String) {
        let frame = layerFrame(id)
        layer.frame = frame
        layer.isHidden = false
        if let grip = layer.sublayers?.first {
            grip.isHidden = !resizable(id)
            grip.frame = CGRect(x: frame.width - CardRenderer.gripSize, y: frame.height - CardRenderer.gripSize,
                                width: CardRenderer.gripSize, height: CardRenderer.gripSize)
        }
    }

    /// The parts picked now: the one selected and those a region took.
    private var pickedParts: Set<String> {
        Set(([selectedSchemaId].compactMap { $0 } + picks).filter(isPart))
    }

    /// The outlines of the part under the pointer and of the parts picked, where their boxes are now.
    private func syncPartOutlines() {
        let picked = pickedParts
        for (id, layer) in partPickLayers where !picked.contains(id) {
            layer.removeFromSuperlayer()
            partPickLayers[id] = nil
        }
        for id in picked {
            let layer = partPickLayers[id] ?? {
                let made = CALayer()
                setUpOutline(made)
                made.zPosition = Depth.pickOutline
                contentLayer.addSublayer(made)
                partPickLayers[id] = made
                return made
            }()
            placeOutline(layer, on: id)
        }
        if let hovered = hoveredPart, boxes[hovered] != nil {
            placeOutline(partHoverLayer, on: hovered)
        } else {
            partHoverLayer.isHidden = true
        }
        outlineWidths()
    }

    /// How thick the outlines are drawn: as many points of the screen whatever the zoom.
    private static let hoverWidth: CGFloat = 1
    private static let pickWidth: CGFloat = 2

    private func outlineWidths() {
        partHoverLayer.borderWidth = Self.hoverWidth / zoom
        for layer in partPickLayers.values { layer.borderWidth = Self.pickWidth / zoom }
        for layer in [partHoverLayer] + Array(partPickLayers.values) {
            (layer.sublayers?.first as? CAShapeLayer)?.lineWidth = 1 / zoom
        }
    }

    /// The part under the pointer, outlined.
    private func hoverPart(_ id: String?) {
        let part = id.flatMap { isPart($0) ? $0 : nil }
        guard part != hoveredPart else { return }
        hoveredPart = part
        withoutAnimation { syncPartOutlines() }
    }

    /// The card picked here let go of: its outline, or its picked border, gone.
    private func letGoOfPick() {
        guard let previous = selectedSchemaId else { return }
        selectedSchemaId = nil
        withoutAnimation {
            drawCard(previous, scale: wantedScale(previous))
            syncPartOutlines()
        }
        selectField(previous, nil)
    }

    /// A part's layers moved with its box.
    private func followPart(_ id: String) {
        guard isPart(id) else { return }
        placeDecor(id)
        if let layer = partPickLayers[id] { placeOutline(layer, on: id) }
        if hoveredPart == id { placeOutline(partHoverLayer, on: id) }
    }

    /// The part whose note lies under `content`, the latest in the section's order first: a note lies
    /// over every part, and stands for its own.
    private func partNote(at content: CGPoint) -> String? {
        guard drawsParts else { return nil }
        return stack.last { id in
            guard let note = partDecorLayouts[id]?.note, let box = boxes[id] else { return false }
            return note.offsetBy(dx: box.minX, dy: box.minY).contains(content)
        }
    }

    /// Where a part's tag, note and dot lie in its box, as drawn now: for the tests.
    func decor(of id: String) -> PartDecor.Layout? { partDecorLayouts[id] }

    /// The outline round the part under the pointer while it shows — its box in content points, its width in
    /// the screen's —: for the tests.
    var hoverOutline: (frame: CGRect, width: CGFloat, depth: CGFloat)? {
        guard !partHoverLayer.isHidden, let id = hoveredPart, let box = boxes[id] else { return nil }
        return (box, (partHoverLayer.borderWidth * zoom * 100).rounded() / 100, partHoverLayer.zPosition)
    }

    /// The outline round a part picked while it shows — its width in the screen's points —: for the tests.
    func pickOutline(of id: String) -> (width: CGFloat, depth: CGFloat, grip: Bool)? {
        guard let layer = partPickLayers[id], !layer.isHidden else { return nil }
        return ((layer.borderWidth * zoom * 100).rounded() / 100, layer.zPosition, layer.sublayers?.first?.isHidden == false)
    }

    /// Whether the hover outline shows the corner's grip: for the tests.
    var hoverGrip: Bool { !partHoverLayer.isHidden && partHoverLayer.sublayers?.first?.isHidden == false }

    /// How far forward a part's tag, note and dot are drawn: for the tests.
    func decorDepth(of id: String) -> CGFloat? { partDecorLayers[id]?.zPosition }

    /// Which picture a part is drawn with now (`DocSnapshots.Picture.number`), nil for its outline: for the
    /// tests, which see whether it was taken again.
    func partPictureNumber(_ id: String) -> Int? {
        guard isPart(id), let layer = cardLayers[id], let shown = partShown[id],
              (layer.contents as AnyObject?) === (shown.image as AnyObject) else { return nil }
        return shown.number
    }

    /// The face a part was last drawn from: for the tests.
    func drawnFace(of id: String) -> PartFace? { partFaces[id] }

    /// The pointer over a point of the view, as a move there puts it: for the tests.
    func hover(atViewPoint point: CGPoint) {
        let content = toContent(point)
        let id = schema(at: content)
        applyHover(id)
        hoverPart(id)
    }

    // MARK: - Writing cards

    /// How far each document card is zoomed into, 1 being its page across the card, and
    /// moved, in the card's points once zoomed. How it is looked at: not kept.
    private var docZoom: [String: CGFloat] = [:]
    private var docScroll: [String: CGPoint] = [:]
    /// The page each document card's picture is of, as last drawn, in its own points.
    private var docPages: [String: CGSize] = [:]
    /// How fine each zoomed card's picture is asked for once its zoom has settled.
    private var docSharp: [String: CGFloat] = [:]
    private var docSettle: [String: DispatchWorkItem] = [:]
    /// The document cards waiting for a newer page (`documentText`): asked to call back once.
    private var docAwaiting: Set<String> = []
    /// The picture each document card was last drawn with, at its width and appearance: drawn on
    /// while the picture of a newer page is being taken, rather than saying it is being drawn.
    private var docShown: [String: (width: Int, dark: Bool, picture: DocSnapshots.Picture)] = [:]
    /// The document cards last drawn saying their page is being drawn.
    private var docPending: Set<String> = []

    /// How many document cards drawn over `area` (diagram points) still say their page is being
    /// drawn: what a picture of it waits for.
    func documentsPending(in area: CGRect) -> Int {
        docPending.filter { drawnIds.contains($0) && boxes[$0]?.intersects(area) == true }.count
    }

    /// A document card's body: its picture when taken — asked for otherwise, and the
    /// card drawn again once it is — zoomed and moved as far as it was.
    private func documentBody(_ schema: SchemaNode, size: CGSize) -> CardRenderer.DocumentBody? {
        let id = schema.id
        guard let documentText else { return nil }
        let asking = docAwaiting.insert(id).inserted
        let face = documentText(schema, asking ? { [weak self] in
            self?.docAwaiting.remove(id)
            self?.redrawDocument(id)
        } : nil)
        let area = documentArea(id, size)
        let page: DocumentPage
        switch face {
        case .none:
            if asking { docAwaiting.remove(id) }
            return nil
        case .drawing:
            return CardRenderer.DocumentBody(image: nil, size: area.size)
        case .page(let made, let current):
            // Nothing to wait for: no call back was left.
            if asking, current { docAwaiting.remove(id) }
            page = made
        }
        let dark = effectiveAppearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
        let width = Int(area.width.rounded())
        // A card on screen is drawn before those that are not.
        let urgent = bounds.width > 0 && bounds.height > 0 ? boxes[id].map(visibleArea.intersects) ?? false : true
        // An SVG or HTML card's page fills the area, so it is drawn at its height too.
        let taken = DocSnapshots.shared.picture(page, width: area.width, height: area.height, dark: dark,
                                                scale: docSharp[id] ?? 1, urgent: urgent, owner: self, card: id,
                                                ready: { [weak self] in self?.redrawDocument(id) })
        guard let picture = taken ?? docShown[id].flatMap({ $0.width == width && $0.dark == dark ? $0.picture : nil }) else {
            return CardRenderer.DocumentBody(image: nil, size: area.size)
        }
        docShown[id] = (width, dark, picture)
        let shown = CGSize(width: area.width, height: picture.height)
        docPages[id] = shown
        let zoom = docZoom[id] ?? 1
        let offset = Self.clamp(docScroll[id] ?? .zero, page: shown, zoom: zoom, area: area.size)
        return CardRenderer.DocumentBody(image: picture.image, size: shown, zoom: zoom, offset: offset, cut: picture.cut,
                                         picture: picture.number)
    }

    /// Draw a document card again at the scale its layer has now.
    private func redrawDocument(_ id: String) {
        guard let layer = cardLayers[id] else { return }
        withoutAnimation { drawCard(id, scale: layer.contentsScale) }
    }

    /// Whether a card is drawn as a document, to be moved and zoomed in with Option held.
    private func isDocument(_ id: String) -> Bool { rows[id]?.content.document == true }

    /// Where a document card's picture is, in a card of `size`: under its header, or under the
    /// fields its kind's face puts above it.
    private func documentArea(_ id: String, _ size: CGSize) -> CGRect {
        CardRenderer.documentArea(size, rows: rows[id]?.content.rows.count ?? 0)
    }

    /// An offset kept to where the zoomed page still covers the card.
    static func clamp(_ offset: CGPoint, page: CGSize, zoom: CGFloat, area: CGSize) -> CGPoint {
        CGPoint(x: min(max(0, offset.x), max(0, page.width * zoom - area.width)),
                y: min(max(0, offset.y), max(0, page.height * zoom - area.height)))
    }

    /// The zoom and offset after zooming by `factor` about `anchor` (in the card's area):
    /// what is under the pointer stays under it. Never below the page across the card.
    static func zoomed(_ offset: CGPoint, zoom: CGFloat, by factor: CGFloat, about anchor: CGPoint,
                       page: CGSize, area: CGSize) -> (offset: CGPoint, zoom: CGFloat) {
        let next = min(max(1, zoom * factor), DocSnapshots.scales.last ?? 8)
        let ratio = next / zoom
        let moved = CGPoint(x: (offset.x + anchor.x) * ratio - anchor.x, y: (offset.y + anchor.y) * ratio - anchor.y)
        return (clamp(moved, page: page, zoom: next, area: area), next)
    }

    /// Move inside a document card, as the diagram is moved.
    func moveDocument(_ id: String, dx: CGFloat, dy: CGFloat) {
        guard let box = boxes[id], let page = docPages[id] else { return }
        let area = documentArea(id, box.size)
        let now = docScroll[id] ?? .zero
        let next = Self.clamp(CGPoint(x: now.x - dx, y: now.y - dy), page: page, zoom: docZoom[id] ?? 1, area: area.size)
        guard next != now else { return }
        docScroll[id] = next
        redrawDocument(id)
    }

    /// Zoom inside a document card about a point in the view. Once it settles,
    /// the picture is taken again as fine as the zoom needs.
    func zoomDocument(_ id: String, by factor: CGFloat, at point: CGPoint) {
        guard let box = boxes[id], let page = docPages[id] else { return }
        let area = documentArea(id, box.size)
        let content = toContent(point)
        let anchor = CGPoint(x: content.x - box.minX - area.minX, y: content.y - box.minY - area.minY)
        let (offset, zoom) = Self.zoomed(docScroll[id] ?? .zero, zoom: docZoom[id] ?? 1, by: factor,
                                         about: anchor, page: page, area: area.size)
        docScroll[id] = offset
        docZoom[id] = zoom
        redrawDocument(id)
        docSettle[id]?.cancel()
        let settle = DispatchWorkItem { [weak self] in
            guard let self else { return }
            let sharp = DocSnapshots.scale(zoom, width: page.width, height: page.height)
            guard sharp != self.docSharp[id] ?? 1 else { return }
            self.docSharp[id] = sharp
            self.redrawDocument(id)
        }
        docSettle[id] = settle
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.25, execute: settle)
    }

    /// Whether a document card is zoomed into or moved: the menu then offers to show it whole.
    func documentMoved(_ id: String) -> Bool {
        (docZoom[id] ?? 1) > 1.001 || (docScroll[id] ?? .zero) != .zero
    }

    /// The whole page across the card again.
    func showWholeDocument(_ id: String) {
        docZoom[id] = nil
        docScroll[id] = nil
        docSharp[id] = nil
        docSettle[id]?.cancel()
        redrawDocument(id)
    }

    /// The document card whose gestures are its own while Option is held, for this gesture.
    private var magnifying: String?

    /// A boxed card of this section's own (`SchemaNode.sizable`), and so sized by hand: not a card
    /// ghost, another section's card, a flow's node or a working directory's file.
    private func resizable(_ id: String) -> Bool {
        guard let schema = schemasById[id], !outsideCards.contains(id) else { return false }
        return schema.sizable
    }

    /// Whether `content` is on a card's corner, where it is sized.
    private func onGrip(_ content: CGPoint, of id: String) -> Bool {
        guard resizable(id), let box = boxes[id] else { return false }
        // A part's corner shows only while it is under the pointer or picked.
        if isPart(id), hoveredPart != id, !pickedParts.contains(id) { return false }
        let reach = CardRenderer.gripSize + 4
        return content.x >= box.maxX - reach && content.y >= box.maxY - reach
    }

    /// The card being sized by its corner: where it was, the size it had as it was pressed and
    /// whether that had a height of its own, and the size it has now.
    private var resizing: (id: String, box: CGRect, start: GridSize, hadHeight: Bool, size: GridSize)?

    /// Size the card being dragged by its corner to the pointer, cell by cell.
    private func resizeCard(to content: CGPoint) {
        guard var resizing else { return }
        let w = Int(((content.x - resizing.box.minX) / unit.width).rounded())
        let h = min(max(Int(((content.y - resizing.box.minY) / unit.height).rounded()), GridSize.heights.lowerBound),
                    GridSize.heights.upperBound)
        let size = sizeAllowed(GridSize(w: min(max(w, GridSize.widths.lowerBound), GridSize.widths.upperBound), h: h),
                               for: resizing.id)
        guard size != resizing.size else { return }
        resizing.size = size
        self.resizing = resizing
        let id = resizing.id
        let points = CGSize(width: CGFloat(size.w) * unit.width, height: CGFloat(size.h ?? h) * unit.height)
        // A card its kind's face draws is a document under its fields once it has a size, even
        // with nothing written yet.
        let document = forms[id]?.document == true || schemasById[id]?.face?.markdown != nil
        forms[id] = CardForm(size: points, document: document)
        if let schema = schemasById[id] {
            let content = cardRows(schema, lookup: lookup, inlineReuse: filters.nesting.nests(id),
                                   from: cardScroll[id] ?? 0, folded: cardFolds[id] ?? [], tree: tree,
                                   form: forms[id] ?? CardForm())
            rows[id] = CardIndex(content)
        }
        boxes[id] = CGRect(origin: resizing.box.origin, size: points)
        withoutAnimation {
            cardLayers[id]?.frame = layerFrame(id)
            drawCard(id, scale: CardRenderer.baseScale)
            followPart(id)
            cutEdges(curved: true)
            updateOffscreenHints()
        }
    }

    /// How far a drag of a card's corner may size it. Into a neighbour too: the card is drawn over
    /// it, and the inspection lists the overlap (`overlaps`) for a tidy to clear, as a size written
    /// by MCP is (`specboard://SpecBoard/Docs/Design:Overview` §12).
    private func sizeAllowed(_ size: GridSize, for id: String) -> GridSize { size }

    /// The size a drag of a card's corner writes, let go at `now` from `start`: nothing when it is
    /// where it began; its width alone when the card had no height of its own and the drag left the
    /// height where it was, so the card stays as tall as its contents; else both.
    static func sizeWritten(start: GridSize, hadHeight: Bool, now: GridSize) -> GridSize? {
        guard now != start else { return nil }
        return hadHeight || now.h != start.h ? now : GridSize(w: now.w)
    }

    /// A card drawn again with `size` as its own, without reading the project again.
    private func resettle(_ id: String, size: GridSize) {
        guard var schema = schemasById[id] else { return }
        schema.size = size
        schemasById[id] = schema
        forms[id] = cardForm(schema, settings: settings, cardWidth: cardWidth)
        let content = cardRows(schema, lookup: lookup, inlineReuse: filters.nesting.nests(id),
                               from: cardScroll[id] ?? 0, folded: cardFolds[id] ?? [], tree: tree,
                               form: forms[id] ?? CardForm())
        rows[id] = CardIndex(content)
        boxes[id]?.size = boxSize(id, content)
        withoutAnimation {
            cardLayers[id]?.frame = layerFrame(id)
            drawCard(id, scale: CardRenderer.baseScale)
            followPart(id)
            cutEdges()
            updateOffscreenHints()
        }
    }

    /// Whether the pointer shows the corner's arrows.
    private var gripCursor = false

    private func showGripCursor(_ on: Bool) {
        guard on != gripCursor else { return }
        gripCursor = on
        (on ? NSCursor.frameResize(position: .bottomRight, directions: .all) : NSCursor.arrow).set()
    }

    private func redrawAllCards() {
        withoutAnimation {
            for id in cardLayers.keys { drawCard(id, scale: wantedScale(id)) }
        }
    }

    /// Every card made again from what was last shown, in the screen's language now: the pages of the
    /// documents are made again in it (`DocumentTexts`), and the cards' pictures kept are another
    /// language's.
    private func speakAgain() {
        guard let detail else { return }
        show(detail, filters: filters, surroundings: surroundings, settings: settings, cardWidth: cardWidth, marks: marks)
    }

    private func syncEdgeLayers() {
        var stale = Set(edgeLayers.keys)
        var staleLabels = Set(labelLayers.keys)

        for edge in edges {
            stale.remove(edge.key)
            let shape = edgeLayers[edge.key] ?? {
                let l = CAShapeLayer()
                l.anchorPoint = .zero
                l.fillColor = nil
                l.lineCap = .round
                l.zPosition = Depth.edge
                edgeLayers[edge.key] = l
                contentLayer.insertSublayer(l, at: 0)
                return l
            }()
            shape.frame = CGRect(origin: .zero, size: contentSize)

            for store in [\CardCanvasView.capLayers, \CardCanvasView.headLayers] {
                let layer = self[keyPath: store][edge.key] ?? {
                    let l = CAShapeLayer()
                    l.anchorPoint = .zero
                    l.lineJoin = .round
                    l.fillColor = nil
                    l.zPosition = Depth.edge
                    self[keyPath: store][edge.key] = l
                    contentLayer.insertSublayer(l, at: 0)
                    return l
                }()
                layer.frame = CGRect(origin: .zero, size: contentSize)
            }

            if let cardinality = edge.cardinality {
                staleLabels.remove(edge.key)
                let label = labelLayers[edge.key] ?? {
                    let l = CATextLayer()
                    l.anchorPoint = .zero
                    l.alignmentMode = .center
                    l.fontSize = 10
                    l.font = NSFont.monospacedSystemFont(ofSize: 10, weight: .medium)
                    l.contentsScale = 2
                    l.zPosition = Depth.label
                    labelLayers[edge.key] = l
                    contentLayer.addSublayer(l)
                    return l
                }()
                label.string = cardinality.rawValue
            }
        }

        for key in stale {
            edgeLayers[key]?.removeFromSuperlayer()
            edgeLayers.removeValue(forKey: key)
            capLayers[key]?.removeFromSuperlayer()
            capLayers.removeValue(forKey: key)
            headLayers[key]?.removeFromSuperlayer()
            headLayers.removeValue(forKey: key)
        }
        for key in staleLabels {
            labelLayers[key]?.removeFromSuperlayer()
            labelLayers.removeValue(forKey: key)
        }

        cutEdges()
        restyleEdges()
    }

    /// Rebuild edge geometry. **Only when a card moves** — not when the viewport does.
    /// `curved`: the plain curves, while a card is being dragged or sized — the way round the
    /// cards (`LineRouter`) is found once it is let go.
    private func cutEdges(curved: Bool = false) {
        placeFrames()
        wires = [:]
        routedNow = !curved && routesLines
        // Worked out once for the whole set: where a line lands depends on the other
        // lines landing beside it, which no single edge can know on its own.
        var shifted: [String: CGRect] = [:]
        var tails: [String: CGFloat] = [:]
        for edge in edges {
            guard let a = boxes[edge.from] else { continue }
            let from = a.offsetBy(dx: -origin.x, dy: -origin.y)
            shifted[edge.from] = from
            if let b = boxes[edge.to] {
                shifted[edge.to] = b.offsetBy(dx: -origin.x, dy: -origin.y)
            }
            let via = edge.vias.first { $0.fromSchemaId == edge.from } ?? edge.vias[0]
            tails[edge.key] = GraphEdges.rowAnchor(rows[edge.from], path: via.from, box: from)
        }
        // Round the cards: every line's way found at once, on the cells the cards sit
        // on, in the diagram's own points — the cards' boxes as they are, not shifted.
        var routed: [String: [CGPoint]] = [:]
        if routedNow {
            let requests: [LineRouter.Request] = edges.compactMap { edge in
                guard let from = shifted[edge.from], let to = shifted[edge.to], edge.from != edge.to else { return nil }
                let landing = edge.landingPath.map { GraphEdges.rowAnchor(rows[edge.to], path: $0, box: to) }
                    ?? GraphEdges.headerAnchor(of: to)
                return LineRouter.Request(key: edge.key, from: from.offsetBy(dx: origin.x, dy: origin.y),
                                          to: to.offsetBy(dx: origin.x, dy: origin.y),
                                          tailY: (tails[edge.key] ?? from.midY) + origin.y, landY: landing + origin.y)
            }
            let boxes = Array(self.boxes.values), cell = unit
            routed = DiagramTimes.measure("routes") { LineRouter.route(requests, boxes: boxes, cell: cell) }
        }

        for edge in edges {
            guard let shape = edgeLayers[edge.key],
                  let from = shifted[edge.from], let to = shifted[edge.to] else { continue }
            let tailY = tails[edge.key] ?? from.midY

            // A `pointsAt.in` names a row in the target, so it lands there. When
            // that row is folded away past the cut, `rowAnchor` returns the "+N"
            // line standing in for it — landing on what is hiding it is the honest
            // answer, and better than landing nowhere.
            let landing = edge.landingPath.map {
                GraphEdges.rowAnchor(rows[edge.to], path: $0, box: to)
            }
            let wire = routed[edge.key].map { points in
                GraphEdges.Wire.routed(points.map { CGPoint(x: $0.x - origin.x, y: $0.y - origin.y) })
            } ?? GraphEdges.wire(from: from, to: to, tailY: tailY, landing: landing)
            wires[edge.key] = wire
            shape.path = wire.path

            // Each mark points back along the line at its own end, which is the
            // direction its nearer control point (a routed line's nearer corner) leaves in.
            func place(_ layer: CAShapeLayer?, _ mark: EdgeCap,
                       tip: CGPoint, toward: CGPoint) {
                guard let layer else { return }
                guard let shapePath = mark.path(size: 9) else { layer.path = nil; return }
                var transform = CGAffineTransform(translationX: tip.x, y: tip.y)
                    .rotated(by: atan2(toward.y - tip.y, toward.x - tip.x))
                layer.path = shapePath.copy(using: &transform)
            }
            // The marks at its ends are its kind's (`look`): what the line is at the card it
            // leaves, and — usually an arrowhead, since lines run beneath the cards and one
            // passing behind a card comes out the far side — at the one it reaches.
            let look = detail?.lineKinds.look(edge.line) ?? LineLook()
            place(capLayers[edge.key], look.begin, tip: wire.start, toward: wire.control1)
            place(headLayers[edge.key], look.end, tip: wire.end, toward: wire.control2)

            if let label = labelLayers[edge.key] {
                let size = CGSize(width: 34, height: 13)
                label.frame = CGRect(x: wire.label.x - size.width / 2,
                                     y: wire.label.y - size.height / 2,
                                     width: size.width, height: size.height)
            }
        }
    }

    // MARK: - Frames round groups

    /// How far a group's frame stands off its cards, and the strip at its top its title is in.
    static let framePad = CGSize(width: 14, height: 12)
    static let frameTitle: CGFloat = 24

    /// A frame's box and title for each of `surroundings.frames`, and none for what is gone.
    private func syncFrameLayers() {
        let wanted = Set(surroundings.frames.map(\.id))
        for (id, layers) in frameLayers where !wanted.contains(id) {
            layers.box.removeFromSuperlayer()
            layers.title.removeFromSuperlayer()
            frameLayers.removeValue(forKey: id)
        }
        for frame in surroundings.frames where frameLayers[frame.id] == nil {
            let box = CAShapeLayer()
            box.anchorPoint = .zero
            box.lineWidth = 1.2
            box.lineDashPattern = [5, 4]
            box.zPosition = Depth.frame
            let title = CATextLayer()
            title.anchorPoint = .zero
            title.truncationMode = .end
            title.contentsScale = 2
            title.zPosition = Depth.frame
            for layer in [box, title] as [CALayer] {
                layer.actions = ["position": NSNull(), "bounds": NSNull(), "path": NSNull(), "hidden": NSNull(),
                                 "contents": NSNull()]
                contentLayer.insertSublayer(layer, at: 0)
            }
            frameLayers[frame.id] = (box, title)
        }
        restyleFrames()
    }

    /// Each frame round the cards of its group drawn now, where they are.
    private func placeFrames() {
        groupFrames = [:]
        for frame in surroundings.frames {
            guard let layers = frameLayers[frame.id] else { continue }
            let drawn = frame.cards.compactMap { boxes[$0] }
            guard let first = drawn.first else {
                layers.box.isHidden = true
                layers.title.isHidden = true
                continue
            }
            let around = drawn.dropFirst().reduce(first) { $0.union($1) }
                .insetBy(dx: -Self.framePad.width, dy: -Self.framePad.height)
            let rect = CGRect(x: around.minX, y: around.minY - Self.frameTitle,
                              width: around.width, height: around.height + Self.frameTitle)
            groupFrames[frame.id] = rect
            let local = rect.offsetBy(dx: -origin.x, dy: -origin.y)
            layers.box.isHidden = false
            layers.title.isHidden = false
            layers.box.frame = local
            layers.box.path = CGPath(roundedRect: CGRect(origin: .zero, size: local.size).insetBy(dx: 0.6, dy: 0.6),
                                     cornerWidth: 10, cornerHeight: 10, transform: nil)
            layers.title.frame = CGRect(x: local.minX + 12, y: local.minY + 6, width: max(0, local.width - 24), height: 18)
        }
    }

    /// The frames in the theme's colours, each titled with its name and how many cards it holds.
    private func restyleFrames() {
        let theme = self.theme
        let byId = Dictionary(surroundings.frames.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        for (id, layers) in frameLayers {
            layers.box.strokeColor = theme.ink3.withAlphaComponent(0.8).cgColor
            layers.box.fillColor = theme.panel.withAlphaComponent(theme.isDark ? 0.16 : 0.32).cgColor
            guard let frame = byId[id] else { continue }
            let title = NSMutableAttributedString(string: frame.title, attributes: [
                .font: NSFont.monospacedSystemFont(ofSize: 12, weight: .semibold),
                .foregroundColor: theme.ink2,
            ])
            title.append(NSAttributedString(string: "  \(frame.cards.count)", attributes: [
                .font: NSFont.monospacedDigitSystemFont(ofSize: 12, weight: .regular),
                .foregroundColor: theme.ink3,
            ]))
            layers.title.string = title
        }
    }

    private func restyleEdges() {
        let theme = self.theme
        for edge in edges {
            guard let shape = edgeLayers[edge.key] else { continue }
            // How it is drawn is its kind's definition's (`look`): no setting of the screen's.
            let look = detail?.lineKinds.look(edge.line) ?? LineLook()
            let colour = look.ink.color(dark: theme.isDark).cgColor
            // The line picked: thicker, glowing in its own colour, above the other lines.
            let picked = pickedLine.map { id in edge.vias.contains { $0.connection == id } } ?? false
            shape.strokeColor = colour
            shape.lineWidth = picked ? 3 : 1.25
            shape.shadowColor = colour
            shape.shadowOffset = .zero
            shape.shadowRadius = picked ? 4 : 0
            shape.shadowOpacity = picked ? 0.7 : 0
            shape.zPosition = picked ? Depth.edge + 0.5 : Depth.edge
            shape.lineDashPattern = look.dash.pattern?.map { NSNumber(value: Double($0)) }

            // Filled says "this is inside me"; hollow says "I only name it". The background
            // shows through an open one so the two never blur; an arrowhead is strokes.
            for (layer, mark) in [(capLayers[edge.key], look.begin), (headLayers[edge.key], look.end)] {
                guard let layer else { continue }
                layer.fillColor = mark.isFilled ? colour : mark.isClosed ? theme.background.cgColor : nil
                layer.strokeColor = colour
                layer.lineWidth = picked ? 2 : 1.25
                layer.zPosition = picked ? Depth.edge + 0.5 : Depth.edge
            }
        }
        for label in labelLayers.values {
            label.foregroundColor = theme.ink3.cgColor
            label.backgroundColor = theme.background.withAlphaComponent(0.85).cgColor
        }
    }

    // MARK: - Viewport

    private func applyViewport() {
        contentLayer.transform = CATransform3DMakeScale(zoom, zoom, 1)
        contentLayer.position = offset
        if drawsParts { outlineWidths() }
        syncGrid()
        updateOffscreenHints()
        layoutInPlace()
    }

    // MARK: - Writing a card where it is

    /// The field a card is written in where it is, over it: the card, and the field.
    private var inPlace: (id: String, scroll: NSScrollView, text: InPlaceTextView)?

    /// The card being written where it is, if one is, and the field it is written in.
    var writingInPlace: String? { inPlace?.id }
    var inPlaceField: NSTextView? { inPlace?.text }

    /// Starts writing `id` where it is (a note's `edit_in_place`): a field over the card below
    /// its header, as the diagram is zoomed, its text in it whole. Kept when the field is left
    /// — another place pressed — or on ⌘Return; let go on Esc.
    func beginInPlace(_ id: String) {
        endInPlace(keep: true)
        guard boxes[id] != nil, let text = inPlaceText?(id) else { return }
        let scroll = NSScrollView()
        scroll.hasVerticalScroller = true
        scroll.autohidesScrollers = true
        scroll.borderType = .lineBorder
        scroll.drawsBackground = true
        let field = InPlaceTextView()
        field.isRichText = false
        field.allowsUndo = true
        field.isAutomaticQuoteSubstitutionEnabled = false
        field.isAutomaticDashSubstitutionEnabled = false
        field.isAutomaticTextReplacementEnabled = false
        field.isVerticallyResizable = true
        field.isHorizontallyResizable = false
        field.autoresizingMask = [.width]
        field.textContainer?.widthTracksTextView = true
        field.maxSize = NSSize(width: CGFloat.greatestFiniteMagnitude, height: CGFloat.greatestFiniteMagnitude)
        field.backgroundColor = theme.isDark ? NSColor(white: 0.12, alpha: 1) : .white
        field.textColor = .labelColor
        field.insertionPointColor = .labelColor
        field.string = text
        field.setAccessibilityLabel(L10n[.Diagram.canvasWriteInPlace])
        field.done = { [weak self] keep, refocus in self?.endInPlace(keep: keep, refocus: refocus) }
        scroll.documentView = field
        addSubview(scroll)
        inPlace = (id, scroll, field)
        layoutInPlace()
        window?.makeFirstResponder(field)
        field.setSelectedRange(NSRange(location: (text as NSString).length, length: 0))
    }

    /// The field over its card, as the diagram is zoomed and moved now: from under the header
    /// to the card's foot — five rows at the least, to write in.
    private func layoutInPlace() {
        guard let inPlace else { return }
        guard let box = boxes[inPlace.id] else {
            endInPlace(keep: true)
            return
        }
        let top = box.minY + CardGeometry.headerHeight
        let foot = max(box.maxY, top + 5 * CardGeometry.rowHeight)
        let from = toView(CGPoint(x: box.minX, y: top)), to = toView(CGPoint(x: box.maxX, y: foot))
        inPlace.scroll.frame = CGRect(x: from.x, y: from.y, width: max(40, to.x - from.x), height: max(24, to.y - from.y))
        inPlace.text.font = .systemFont(ofSize: 12 * zoom)
        inPlace.text.textContainerInset = NSSize(width: 6 * zoom, height: 4 * zoom)
        inPlace.text.minSize = NSSize(width: 0, height: inPlace.scroll.contentSize.height)
        inPlace.text.frame.size.width = inPlace.scroll.contentSize.width
    }

    /// Writing where it is ended: kept — what was written told (`onEditInPlace`) when it
    /// changed — or let go. `refocus`: the diagram takes the keys again (not when another
    /// view was pressed and has them).
    func endInPlace(keep: Bool, refocus: Bool = true) {
        guard let current = inPlace else { return }
        inPlace = nil
        current.text.done = nil
        let written = current.text.string
        current.scroll.removeFromSuperview()
        if refocus { window?.makeFirstResponder(self) }
        if keep, written != inPlaceText?(current.id) { onEditInPlace?(current.id, written) }
    }

    // MARK: - Cards off screen

    /// A faint band on each edge past which there are cards, with how many. Reading
    /// zoomed in, it says which way there is still more; clicking it goes to the
    /// nearest one. Only cards wholly outside count — one cut by the edge is visible.
    enum Side: CaseIterable { case left, right, top, bottom }

    private lazy var hintLayers: [Side: (band: CAGradientLayer, label: CATextLayer)] = {
        var made: [Side: (CAGradientLayer, CATextLayer)] = [:]
        for side in Side.allCases {
            let band = CAGradientLayer()
            band.isHidden = true
            band.zPosition = 1000
            let label = CATextLayer()
            label.font = NSFont.systemFont(ofSize: 12, weight: .semibold)
            label.fontSize = 12
            label.alignmentMode = .center
            label.isHidden = true
            label.zPosition = 1001
            layer?.addSublayer(band)
            layer?.addSublayer(label)
            made[side] = (band, label)
        }
        return made
    }()

    private let hintThickness: CGFloat = 22

    /// The cards wholly outside the view, by side. A card off a corner counts on both.
    var offscreen: [Side: [String]] {
        let visible = visibleArea
        var out: [Side: [String]] = [:]
        for (id, box) in boxes {
            if box.maxX < visible.minX { out[.left, default: []].append(id) }
            if box.minX > visible.maxX { out[.right, default: []].append(id) }
            if box.maxY < visible.minY { out[.top, default: []].append(id) }
            if box.minY > visible.maxY { out[.bottom, default: []].append(id) }
        }
        return out
    }

    private func hintFrame(_ side: Side) -> CGRect {
        switch side {
        case .left: return CGRect(x: 0, y: 0, width: hintThickness, height: bounds.height)
        case .right: return CGRect(x: bounds.maxX - hintThickness, y: 0,
                                   width: hintThickness, height: bounds.height)
        case .top: return CGRect(x: 0, y: 0, width: bounds.width, height: hintThickness)
        case .bottom: return CGRect(x: 0, y: bounds.maxY - hintThickness,
                                    width: bounds.width, height: hintThickness)
        }
    }

    /// Put at once, wherever it is called from: on a resize — a side panel opened or
    /// shut — an implicit animation slid the bands across the panel's old room.
    private func updateOffscreenHints() {
        withoutAnimation { placeOffscreenHints() }
    }

    /// Whether the bands that count the cards off each side are drawn: not on a picture, which
    /// says in words what part of the diagram it is of.
    var showsOffscreenHints = true

    private func placeOffscreenHints() {
        guard showsOffscreenHints, bounds.width > 0, !boxes.isEmpty else {
            for (band, label) in hintLayers.values { band.isHidden = true; label.isHidden = true }
            return
        }
        let counts = offscreen
        let tint = NSColor.controlAccentColor
        let scale = window?.backingScaleFactor ?? 2
        for side in Side.allCases {
            guard let (band, label) = hintLayers[side] else { continue }
            let count = counts[side]?.count ?? 0
            band.isHidden = count == 0
            label.isHidden = count == 0
            guard count > 0 else { continue }
            band.frame = hintFrame(side)
            band.colors = [tint.withAlphaComponent(0.22).cgColor, tint.withAlphaComponent(0).cgColor]
            switch side {
            case .left: band.startPoint = CGPoint(x: 0, y: 0.5); band.endPoint = CGPoint(x: 1, y: 0.5)
            case .right: band.startPoint = CGPoint(x: 1, y: 0.5); band.endPoint = CGPoint(x: 0, y: 0.5)
            case .top: band.startPoint = CGPoint(x: 0.5, y: 0); band.endPoint = CGPoint(x: 0.5, y: 1)
            case .bottom: band.startPoint = CGPoint(x: 0.5, y: 1); band.endPoint = CGPoint(x: 0.5, y: 0)
            }
            let arrow: String
            switch side {
            case .left: arrow = "◀"
            case .right: arrow = "▶"
            case .top: arrow = "▲"
            case .bottom: arrow = "▼"
            }
            label.string = side == .left || side == .right ? "\(arrow)\n\(count)" : "\(arrow) \(count)"
            label.foregroundColor = tint.cgColor
            label.contentsScale = scale
            let size = side == .left || side == .right
                ? CGSize(width: hintThickness, height: 34) : CGSize(width: 70, height: 16)
            let frame = hintFrame(side)
            label.frame = CGRect(x: frame.midX - size.width / 2, y: frame.midY - size.height / 2,
                                 width: size.width, height: size.height)
        }
    }

    /// A click on a band goes to the nearest card that way, at the zoom already chosen.
    private func followHint(at point: CGPoint) -> Bool {
        let counts = offscreen
        for side in Side.allCases {
            guard let ids = counts[side], !ids.isEmpty, hintFrame(side).contains(point) else { continue }
            let visible = visibleArea
            let middle = CGPoint(x: visible.midX, y: visible.midY)
            let nearest = ids.compactMap { boxes[$0] }.min {
                hypot($0.midX - middle.x, $0.midY - middle.y) < hypot($1.midX - middle.x, $1.midY - middle.y)
            }
            guard let nearest else { return false }
            center(on: CGPoint(x: nearest.midX, y: nearest.midY), zoom: zoom)
            return true
        }
        return false
    }

    /// Content point → view point.
    private func toView(_ point: CGPoint) -> CGPoint {
        CGPoint(x: (point.x - origin.x) * zoom + offset.x,
                y: (point.y - origin.y) * zoom + offset.y)
    }

    /// View point → content point.
    private func toContent(_ point: CGPoint) -> CGPoint {
        CGPoint(x: (point.x - offset.x) / zoom + origin.x,
                y: (point.y - offset.y) / zoom + origin.y)
    }

    /// What the cards cover now, with the usual room around it. From the cards
    /// themselves rather than `contentSize`: a drag writes a position without
    /// rebuilding, and a card carried past the old edge still has to be in 全体表示
    /// and within reach of zooming out.
    private var liveExtent: CGRect { GraphEdges.bounds(boxes) }

    func fitToView() {
        let extent = liveExtent
        guard extent.width > 0, bounds.width > 0 else { return }
        let scale = min(bounds.width / extent.width, bounds.height / extent.height)
        zoom = min(maxZoom, max(fitMinZoom, scale))
        offset = CGPoint(x: bounds.midX - (extent.midX - origin.x) * zoom,
                         y: bounds.midY - (extent.midY - origin.y) * zoom)
        withoutAnimation { applyViewport() }
        onZoomChanged?(zoom)
        scheduleSettle()
    }

    // MARK: - Viewport, for agents

    /// What an agent asks for is a view it can reason about: which part of the
    /// diagram, at what size. These take and give diagram coordinates — the same
    /// ones the cards' frames are in — and redraw at once rather than after the
    /// usual pause, because the next thing asked for is usually a picture of it.

    /// The zoom an agent may ask for: as far out as 全体表示 goes, as far in as a pinch.
    var zoomLimits: ClosedRange<CGFloat> { fitMinZoom...maxZoom }

    /// The part of the diagram on screen, in diagram coordinates.
    var visibleArea: CGRect {
        CGRect(origin: toContent(.zero),
               size: CGSize(width: bounds.width / zoom, height: bounds.height / zoom))
    }

    /// Every card drawn, and exactly what they cover together — without the margin
    /// the layers keep around them, so "the diagram's top-left" is the first card's.
    var cardFrames: [String: CGRect] { boxes }
    var diagramExtent: CGRect { boxes.values.reduce(CGRect.null) { $0.union($1) } }

    /// Show `area` as large as fits with `margin` points around it, no closer than `cap`.
    func show(area: CGRect, margin: CGFloat, cap: CGFloat) {
        let room = CGSize(width: max(1, bounds.width - 2 * margin),
                          height: max(1, bounds.height - 2 * margin))
        let scale = min(room.width / max(area.width, 1), room.height / max(area.height, 1))
        zoom = clampForAgent(min(scale, cap))
        offset = CGPoint(x: bounds.midX - (area.midX - origin.x) * zoom,
                         y: bounds.midY - (area.midY - origin.y) * zoom)
        commitViewport()
    }

    /// Put `point` at the panel's top-left corner, `margin` points in.
    func place(_ point: CGPoint, atTopLeftWith margin: CGFloat, zoom next: CGFloat) {
        zoom = clampForAgent(next)
        offset = CGPoint(x: margin - (point.x - origin.x) * zoom,
                         y: margin - (point.y - origin.y) * zoom)
        commitViewport()
    }

    /// Put `point` at the middle of the panel.
    func center(on point: CGPoint, zoom next: CGFloat) {
        zoom = clampForAgent(next)
        offset = CGPoint(x: bounds.midX - (point.x - origin.x) * zoom,
                         y: bounds.midY - (point.y - origin.y) * zoom)
        commitViewport()
    }

    private func clampForAgent(_ value: CGFloat) -> CGFloat {
        min(zoomLimits.upperBound, max(zoomLimits.lowerBound, value))
    }

    private func commitViewport() {
        settleWork?.cancel()
        withoutAnimation { applyViewport() }
        onZoomChanged?(zoom)
        settle()
    }

    /// Move the view. Pure transform — nothing is recomputed, nothing is redrawn.
    func panBy(dx: CGFloat, dy: CGFloat) {
        offset.x += dx
        offset.y += dy
        withoutAnimation { applyViewport() }
    }

    /// How far out a person can zoom: to where every card fits, and no further —
    /// past that the diagram only shrinks into the middle of an empty panel. A small
    /// diagram that fits above 100% still goes down to 100%, or actual size would be
    /// out of reach; a huge one goes below the pinch floor, as 全体表示 does.
    var zoomFloor: CGFloat {
        let extent = liveExtent
        guard extent.width > 0, extent.height > 0, bounds.width > 0 else { return minZoom }
        let fits = min(bounds.width / extent.width, bounds.height / extent.height)
        return min(1, max(fitMinZoom, fits))
    }

    /// Change zoom while pinning the content point under `anchor` (a view point).
    func setZoom(_ next: CGFloat, anchor: CGPoint) {
        let clamped = min(maxZoom, max(zoomFloor, next))
        guard clamped != zoom else { return }
        let content = toContent(anchor)
        zoom = clamped
        offset = CGPoint(x: anchor.x - (content.x - origin.x) * zoom,
                         y: anchor.y - (content.y - origin.y) * zoom)
        withoutAnimation { applyViewport() }
        onZoomChanged?(zoom)
        scheduleSettle()
    }

    // MARK: - Input

    override func scrollWheel(with event: NSEvent) {
        let done = probe.begin()
        defer { done() }

        let point = convert(event.locationInWindow, from: nil)

        // Option scrolls the card under the pointer. Command stays zoom: pointing at
        // a card and zooming into it is the usual way to zoom, and taking that away
        // to add this would be a trade rather than an addition.
        //
        // The target is decided once, when the gesture starts, and held for the rest
        // of it. A trackpad keeps sending momentum events after the fingers lift, by
        // which time Option is usually already released — those arrived as ordinary
        // scrolling and threw the whole canvas across the screen.
        if event.phase.contains(.began) || (event.phase.isEmpty && event.momentumPhase.isEmpty) {
            scrolling = event.modifierFlags.contains(.option)
                ? schema(at: toContent(point)).flatMap {
                    // A part is a picture of a page: nothing in it to move.
                    !isPart($0) && ((rows[$0]?.content.hidden ?? 0) > 0 || isDocument($0)) ? $0 : nil
                }
                : nil
        }
        // A document card is moved inside both ways and zoomed with ⌘, as the diagram is.
        if let id = scrolling, isDocument(id) {
            if event.modifierFlags.contains(.command) {
                zoomDocument(id, by: 1 - event.scrollingDeltaY * 0.005, at: point)
            } else {
                moveDocument(id, dx: event.scrollingDeltaX, dy: event.scrollingDeltaY)
            }
            return
        }
        if let id = scrolling {
            // Swallowed even at the card's end: falling through to panning halfway
            // through a flick is the same surprise in a smaller costume.
            scrollCard(id, by: event.scrollingDeltaY)
            return
        }

        if event.modifierFlags.contains(.command) {
            let factor = 1 - event.scrollingDeltaY * 0.005
            setZoom(zoom * factor, anchor: point)
            return
        }

        // Both axes at once, rather than a scroll container: native scrolling
        // axis-locks a trackpad gesture, and a two-finger diagonal swipe has to move
        // diagonally.
        panBy(dx: event.scrollingDeltaX, dy: event.scrollingDeltaY)
    }

    /// Render the whole diagram as it currently stands, for checking the drawing
    /// without a screen.
    ///
    /// `CALayer.render(in:)` walks the tree itself and does not honour
    /// `isGeometryFlipped`, so what it produces is a vertically mirrored copy of
    /// what the window shows. The counter-flip here is a property of that call, not
    /// of the canvas — on screen the layers are already the right way up.
    func snapshot() -> CGImage? {
        guard let root = layer, bounds.width > 0, bounds.height > 0 else { return nil }
        let scale: CGFloat = 2
        guard let ctx = CGContext(
            data: nil, width: Int(bounds.width * scale), height: Int(bounds.height * scale),
            bitsPerComponent: 8, bytesPerRow: 0,
            space: CGColorSpace(name: CGColorSpace.sRGB)!,
            bitmapInfo: CGImageAlphaInfo.premultipliedFirst.rawValue) else { return nil }
        ctx.scaleBy(x: scale, y: scale)
        ctx.translateBy(x: 0, y: bounds.height)
        ctx.scaleBy(x: 1, y: -1)
        root.render(in: ctx)
        return ctx.makeImage()
    }

    override func magnify(with event: NSEvent) {
        let done = probe.begin()
        defer { done() }
        let point = convert(event.locationInWindow, from: nil)
        // With Option held, a pinch over a document card zooms inside it: decided
        // as the gesture starts and held to its end, as scrolling is.
        if event.phase.contains(.began) || event.phase.isEmpty {
            magnifying = event.modifierFlags.contains(.option)
                ? schema(at: toContent(point)).flatMap { isDocument($0) && !isPart($0) ? $0 : nil } : nil
        }
        if let id = magnifying {
            zoomDocument(id, by: 1 + event.magnification, at: point)
            if event.phase.contains(.ended) || event.phase.contains(.cancelled) { magnifying = nil }
            return
        }
        setZoom(zoom * (1 + event.magnification), anchor: point)
    }

    private var drag: (id: String, grab: CGPoint, box: CGRect)?

    // MARK: - The selection mode

    /// The selection mode: a drag from where no card is draws a region instead of moving the view —
    /// the boxes any of it takes, or with ⌥ the whole of each (MCP's `region`: `intersect`,
    /// `contain`) — and a drag of one of those picked moves them all. Turned on and off from the
    /// menu where no card is; off with Esc. Out of it, nothing is as it was.
    private(set) var selecting = false
    var onSelectingChanged: ((Bool) -> Void)?
    /// The boxes a region picked, drawn as picked.
    private(set) var picks: Set<String> = []
    /// The region being drawn, in content coordinates.
    private var marquee: (from: CGPoint, to: CGPoint)?
    private let marqueeLayer = CAShapeLayer()
    /// The picked boxes moved together: where the pointer took hold, and where each was.
    private var groupDrag: (grab: CGPoint, boxes: [String: CGRect])?

    /// The selection mode on or off; off, what was picked is let go.
    func setSelecting(_ on: Bool) {
        guard selecting != on else { return }
        selecting = on
        marquee = nil
        withoutAnimation { marqueeLayer.isHidden = true }
        if !on { pick([]) }
        onSelectingChanged?(on)
    }

    /// The boxes picked now, drawn so.
    private func pick(_ ids: Set<String>) {
        let changed = picks.symmetricDifference(ids)
        picks = ids
        withoutAnimation {
            for id in changed where cardLayers[id] != nil { drawCard(id, scale: wantedScale(id)) }
            syncPartOutlines()
        }
    }

    /// The boxes the region from `a` to `b` takes (content coordinates): any of each, or with
    /// `whole` the whole of each. Not what a merge looked at takes away.
    func regionPicks(_ a: CGPoint, _ b: CGPoint, whole: Bool) -> Set<String> {
        let rect = CGRect(x: min(a.x, b.x), y: min(a.y, b.y), width: abs(a.x - b.x), height: abs(a.y - b.y))
        return Set(boxes.filter { id, box in
            !goneCards.contains(id) && (whole ? rect.contains(box) : rect.intersects(box))
        }.map(\.key))
    }

    /// The region drawn to where the pointer is now.
    private func stretchMarquee() {
        guard let region = marquee, let dragPoint else { return }
        let to = toContent(dragPoint)
        marquee = (region.from, to)
        let rect = CGRect(x: min(region.from.x, to.x) - origin.x, y: min(region.from.y, to.y) - origin.y,
                          width: abs(to.x - region.from.x), height: abs(to.y - region.from.y))
        withoutAnimation {
            marqueeLayer.path = CGPath(rect: rect, transform: nil)
            marqueeLayer.isHidden = false
        }
    }

    /// The picked boxes put back under the pointer, as they were among themselves.
    private func moveGroup() {
        guard let group = groupDrag, let dragPoint else { return }
        let content = toContent(dragPoint)
        let (dx, dy) = (content.x - group.grab.x, content.y - group.grab.y)
        for (id, was) in group.boxes { boxes[id] = was.offsetBy(dx: dx, dy: dy) }
        withoutAnimation {
            for id in group.boxes.keys {
                cardLayers[id]?.frame = layerFrame(id)
                followPart(id)
            }
            cutEdges(curved: true)
            updateOffscreenHints()
        }
    }

    /// The picked boxes let go: on the cells nearest where they were dropped, kept as they were among
    /// themselves — or, too close to a box left where it is, the nearest place clear of them all (as
    /// MCP's `move` puts them; not where the diagram keeps places as written) — each written as a card
    /// dropped by hand is.
    private func dropGroup(_ group: (grab: CGPoint, boxes: [String: CGRect])) {
        guard let (first, was) = group.boxes.first, let now = boxes[first] else { return }
        let wanted = Grid.nearest(x: Double(now.minX / unit.width), y: Double(now.minY / unit.height))
        let start = gridRect(of: was)
        let moving = group.boxes.values.map { gridRect(of: $0) }
        let others = boxes.filter { group.boxes[$0.key] == nil && !goneCards.contains($0.key) }.map { gridRect(of: $0.value) }
        let by = (wanted.x - start.x, wanted.y - start.y)
        let (dx, dy) = keepsPlaces ? by : clearOffset(moving, avoiding: others, by: by)
        var placed: [(String, GridPoint)] = []
        for (id, was) in group.boxes {
            let at = GridPoint(x: gridRect(of: was).x + dx, y: gridRect(of: was).y + dy)
            boxes[id] = CGRect(origin: points(at), size: was.size)
            placed.append((id, at))
        }
        withoutAnimation {
            for id in group.boxes.keys {
                cardLayers[id]?.frame = layerFrame(id)
                followPart(id)
            }
            cutEdges()
        }
        for (id, at) in placed { onMoveByHand?(id, at) }
    }

    /// How far a group of boxes moves: `by`, when none of them comes within the margin of `others`;
    /// else the nearest offset where all of them are clear (out to 40 cells; failing that, `by`).
    private func clearOffset(_ moving: [GridRect], avoiding others: [GridRect], by: (Int, Int)) -> (Int, Int) {
        func fits(_ dx: Int, _ dy: Int) -> Bool {
            moving.allSatisfy { r in
                let room = settings.withMargin(GridRect(x: r.x + dx, y: r.y + dy, w: r.w, h: r.h))
                return !others.contains { $0.intersects(room) }
            }
        }
        if fits(by.0, by.1) { return by }
        for ring in 1...40 {
            var best: (Int, Int)?
            var bestDistance = Int.max
            for dx in -ring...ring {
                for dy in -ring...ring where max(abs(dx), abs(dy)) == ring {
                    let distance = dx * dx + dy * dy
                    if distance < bestDistance, fits(by.0 + dx, by.1 + dy) {
                        best = (by.0 + dx, by.1 + dy)
                        bestDistance = distance
                    }
                }
            }
            if let best { return best }
        }
        return by
    }
    private var panning: CGPoint?
    /// Where a press on no card began, until the pointer moves far enough to make
    /// it a pan.
    private var pressedEmpty: CGPoint?

    /// Where the pointer was last seen while dragging a card, in view points.
    ///
    /// The auto-scroll needs it every frame while the hand is holding still at the
    /// edge — the pointer is not moving, so no event arrives, but the world under it
    /// is, and the card has to keep up with the pointer rather than with the world.
    private var dragPoint: CGPoint?
    private var edgeLink: CADisplayLink?

    /// How close to the edge starts the scroll, and how fast it goes there.
    private let edgeMargin: CGFloat = 56
    private let edgeSpeed: CGFloat = 16

    /// Which way the view should travel for a pointer at this point, and how fast.
    ///
    /// Zero anywhere but the margin, and ramped across it, so a card nudged near the
    /// edge creeps and one pressed into the corner moves at speed. The sign is the
    /// content's, not the pointer's: reaching past the right edge means the content
    /// has to come left.
    static func edgePull(_ point: CGPoint, in bounds: CGRect,
                         margin: CGFloat, speed: CGFloat) -> CGVector {
        func axis(_ v: CGFloat, _ low: CGFloat, _ high: CGFloat) -> CGFloat {
            guard high - low > margin * 2 else { return 0 }
            if v > high - margin { return -min(1, (v - (high - margin)) / margin) }
            if v < low + margin { return min(1, ((low + margin) - v) / margin) }
            return 0
        }
        return CGVector(dx: axis(point.x, bounds.minX, bounds.maxX) * speed,
                        dy: axis(point.y, bounds.minY, bounds.maxY) * speed)
    }

    override func mouseDown(with event: NSEvent) {
        let point = convert(event.locationInWindow, from: nil)
        let content = toContent(point)
        // The handle on a row's edge: a line drawn from it by hand (つなぐ), not the card moved.
        if let shown = handleAt, onConnect != nil,
           hypot(content.x - shown.point.x, content.y - shown.point.y) <= handleReach {
            connectDrag = (shown.hit, shown.point)
            return
        }
        // The selection mode: a region drawn from where no card is, not the view moved.
        if selecting, schema(at: content) == nil {
            marquee = (content, content)
            return
        }
        // A band at the edge, where no card is under the pointer, is a way to go.
        if schema(at: content) == nil, followHint(at: point) { return }
        // A line, where no card is: picked — and a drag from it still moves the view.
        if schema(at: content) == nil, onPickLine != nil, let line = line(at: content) {
            showLine(line.connection)
            onPickLine?(line)
            panning = point
            return
        }
        if let id = schema(at: content) { raise(id) }

        // What a press changes — a fold mark, a custom card's switch or option — changes
        // once a press: the second press of a double-click is the same press again, not a
        // second change, nor a way to where the row points (overview §13).
        // The disclosure mark first: it sits inside the row, and a click there means
        // "show me what is under this" rather than "select this".
        if let id = schema(at: content), let path = disclosure(at: content, in: id) {
            if event.clickCount < 2 { toggleFold(id, path: path) }
            return
        }

        // A custom card's switch or option, pressed: it changes, and nothing is picked.
        if let id = schema(at: content), let press = formPress(at: content, in: id), let onFormPress {
            if event.clickCount < 2 { onFormPress(id, press) }
            return
        }
        // A menu on a card (a face's dropdown list, a choice whose options do not fit): its
        // options, the one chosen checked; one chosen, as an option pressed.
        if let id = schema(at: content), let menu = formMenu(at: content, in: id), let onFormPress {
            if event.clickCount < 2 { popUpFormMenu(menu, of: id, onFormPress) }
            return
        }

        // A boxed card's corner sizes it.
        if let id = schema(at: content), onGrip(content, of: id), let box = boxes[id] {
            let start = GridSize(w: Int((box.width / unit.width).rounded()), h: Grid.rows(forHeight: Double(box.height)))
            resizing = (id, box, start, schemasById[id]?.size?.h != nil, start)
            // The handle off while its edge moves from under it (as for a drag, below).
            hideHandle()
            return
        }

        // Twice: where the row points, or what the header does — the first press picked it.
        // A card its kind writes where it is (a note): written there.
        if event.clickCount >= 2, let id = schema(at: content) {
            if canEditInPlace?(id) == true {
                beginInPlace(id)
            } else {
                onFollow?(RowHit(card: id, row: aimedRow(at: content, in: id)))
            }
            return
        }

        if let hit = select(at: content), hit.row == nil, let box = boxes[hit.card],
           isPart(hit.card) || content.y - box.minY <= CardGeometry.headerHeight, !goneCards.contains(hit.card),
           canMoveByHand?(hit.card) ?? true {
            // Dragging the header moves the card — a section ghost's too, a ui part by any point of it;
            // anywhere else moves the view, so the whole canvas stays grabbable. One of those a region picked:
            // all of them, as they are among themselves.
            // The handle stays where it was put while the card goes from under it — and a press there
            // would draw a line from where the card no longer is: off until the pointer next moves.
            if picks.count > 1, picks.contains(hit.card) {
                groupDrag = (content, Dictionary(uniqueKeysWithValues: picks.compactMap { id in boxes[id].map { (id, $0) } }))
                hideHandle()
                return
            }
            drag = (hit.card, content, box)
            hideHandle()
            return
        }
        if schema(at: content) == nil { pressedEmpty = point }
        panning = point
    }

    override func menu(for event: NSEvent) -> NSMenu? {
        // Right-clicking selects too. The items act on the selection, so what was
        // aimed at and what they operate on have to be the same thing.
        let content = toContent(convert(event.locationInWindow, from: nil))
        if let id = schema(at: content) { raise(id) }
        // Where no card is: the selection mode, on every diagram.
        guard schema(at: content) == nil else { return onRowMenu?(select(at: content)) }
        let menu = onRowMenu?(nil) ?? NSMenu()
        if !menu.items.isEmpty { menu.addItem(.separator()) }
        let on = selecting
        menu.addItem(ActionMenuItem(on ? L10n[.Diagram.canvasExitSelecting] : L10n[.Diagram.canvasEnterSelecting]) { [weak self] in
            self?.setSelecting(!on)
        })
        return menu
    }

    /// The row aimed at in a card. A row is only a target when it is big enough to aim
    /// at; below that a press means the card, which is the thing the pointer could
    /// actually have been on.
    private func aimedRow(at content: CGPoint, in id: String) -> CardRow? {
        guard zoom >= rowPickZoom, !isPart(id), let index = row(at: content, in: id) else { return nil }
        return rows[id]?.content.rows[index]
    }

    /// Make whatever sits under this point the selection — the card, and the row aimed
    /// at marked — tell it (`onPick`), and say what it was.
    @discardableResult
    private func select(at content: CGPoint) -> RowHit? {
        guard let id = schema(at: content) else { return nil }
        showLine(nil)

        let previous = selectedSchemaId
        selectedSchemaId = id
        withoutAnimation {
            if let previous, previous != id { drawCard(previous, scale: wantedScale(previous)) }
            drawCard(id, scale: wantedScale(id))
        }
        let hit = RowHit(card: id, row: aimedRow(at: content, in: id))
        selectField(id, hit.row.map(Self.rowKey))
        withoutAnimation { syncPartOutlines() }
        onPick?(hit)
        return hit
    }

    override func mouseDragged(with event: NSEvent) {
        let done = probe.begin()
        defer { done() }

        let point = convert(event.locationInWindow, from: nil)

        if let connecting = connectDrag {
            // The line drawn to the pointer, and the row it would land on marked.
            let content = toContent(point)
            let path = CGMutablePath()
            path.move(to: CGPoint(x: connecting.from.x - origin.x, y: connecting.from.y - origin.y))
            path.addLine(to: CGPoint(x: content.x - origin.x, y: content.y - origin.y))
            withoutAnimation {
                connectLayer.path = path
                connectLayer.isHidden = false
            }
            let id = schema(at: content)
            showHoveredRow(id, id.flatMap { row(at: content, in: $0) })
            return
        }
        if resizing != nil {
            resizeCard(to: toContent(point))
            return
        }
        if drag != nil {
            dragPoint = point
            moveDraggedCard()
            updateEdgeScroll()
            return
        }
        if marquee != nil {
            dragPoint = point
            stretchMarquee()
            updateEdgeScroll()
            return
        }
        if groupDrag != nil {
            dragPoint = point
            moveGroup()
            updateEdgeScroll()
            return
        }

        if let pressed = pressedEmpty, hypot(point.x - pressed.x, point.y - pressed.y) > 3 {
            pressedEmpty = nil
        }
        if let start = panning {
            offset.x += point.x - start.x
            offset.y += point.y - start.y
            panning = point
            withoutAnimation { applyViewport() }
        }
    }

    override func mouseUp(with event: NSEvent) {
        stopEdgeScroll()
        if let connecting = connectDrag {
            // Let go on a card's header or a row: つなぐ to it — not to the very place it left.
            connectDrag = nil
            withoutAnimation {
                connectLayer.isHidden = true
                connectLayer.path = nil
            }
            hideHandle()
            let point = convert(event.locationInWindow, from: nil)
            let content = toContent(point)
            if let id = schema(at: content) {
                let target = RowHit(card: id, row: aimedRow(at: content, in: id))
                if target != connecting.source { onConnect?(connecting.source, target, point) }
            }
            return
        }
        if let resizing {
            self.resizing = nil
            // Let go: the lines round the cards again.
            withoutAnimation { cutEdges() }
            if let written = Self.sizeWritten(start: resizing.start, hadHeight: resizing.hadHeight, now: resizing.size) {
                onResizeByHand?(resizing.id, written)
                // A width alone: as tall as its rows again, not as the cell it was let go on.
                if written.h == nil { resettle(resizing.id, size: written) }
            }
            // A part's face is the size of its box: asked again at the size written.
            if isPart(resizing.id) { redrawDocument(resizing.id) }
            return
        }
        if let region = marquee {
            // A region drawn: what it takes, picked (with ⌥, only what it takes whole). A press
            // with no drag lets go of what was picked.
            marquee = nil
            dragPoint = nil
            withoutAnimation { marqueeLayer.isHidden = true }
            let drawn = hypot(region.to.x - region.from.x, region.to.y - region.from.y) > 3
            pick(drawn ? regionPicks(region.from, region.to, whole: event.modifierFlags.contains(.option)) : [])
            if !drawn { onClickEmpty?() }
            return
        }
        if let group = groupDrag {
            groupDrag = nil
            dragPoint = nil
            dropGroup(group)
            return
        }
        if let drag, let box = boxes[drag.id] {
            // On the cell it was dropped nearest, like a spreadsheet's — or, too close
            // to another card, the nearest that keeps the margin clear.
            let wanted = Grid.nearest(x: Double(box.minX / unit.width), y: Double(box.minY / unit.height))
            let others = boxes.filter { $0.key != drag.id }.map { gridRect(of: $0.value) }
            let at = keepsPlaces ? wanted : settings.clearSpot(near: wanted, width: gridRect(of: box).w,
                                                               height: Grid.rows(forHeight: Double(box.height)), avoiding: others)
            boxes[drag.id] = CGRect(origin: points(at), size: box.size)
            withoutAnimation {
                cardLayers[drag.id]?.frame = layerFrame(drag.id)
                followPart(drag.id)
                cutEdges()
            }
            // A part pressed and let go where it was: picked, not moved — nothing to write.
            if !(isPart(drag.id) && boxes[drag.id] == drag.box) { onMoveByHand?(drag.id, at) }
        }
        // A click on no card, not a pan: the reader is done with what was open — in a ui section, with
        // the part picked too, whose outline would otherwise stay.
        if pressedEmpty != nil, drag == nil {
            if drawsParts { letGoOfPick() }
            onClickEmpty?()
        }
        pressedEmpty = nil
        drag = nil
        dragPoint = nil
        panning = nil
    }

    /// Put the dragged card back under the pointer.
    ///
    /// Read from the pointer's *content* position, which is why this also works while
    /// the view is scrolling underneath a hand that is holding still: the view point
    /// is the same and the content point is not.
    private func moveDraggedCard() {
        guard let drag, let dragPoint else { return }
        let content = toContent(dragPoint)
        boxes[drag.id] = CGRect(x: drag.box.minX + content.x - drag.grab.x,
                                y: drag.box.minY + content.y - drag.grab.y,
                                width: drag.box.width, height: drag.box.height)
        withoutAnimation {
            cardLayers[drag.id]?.frame = layerFrame(drag.id)
            followPart(drag.id)
            cutEdges(curved: true)          // only geometry, and only because a card moved
            updateOffscreenHints()
        }
    }

    private func updateEdgeScroll() {
        guard drag != nil || marquee != nil || groupDrag != nil, let dragPoint,
              Self.edgePull(dragPoint, in: bounds,
                            margin: edgeMargin, speed: edgeSpeed) != .zero else {
            stopEdgeScroll()
            return
        }
        guard edgeLink == nil else { return }
        let link = displayLink(target: self, selector: #selector(edgeTick))
        link.add(to: .main, forMode: .common)
        edgeLink = link
    }

    private func stopEdgeScroll() {
        edgeLink?.invalidate()
        edgeLink = nil
    }

    @objc private func edgeTick() {
        let done = probe.begin()
        defer { done() }

        guard drag != nil || marquee != nil || groupDrag != nil, let dragPoint else { stopEdgeScroll(); return }
        let pull = Self.edgePull(dragPoint, in: bounds, margin: edgeMargin, speed: edgeSpeed)
        guard pull != .zero else { stopEdgeScroll(); return }

        // The view first, then the card — the card's new home is worked out from
        // where the pointer now sits in the content, which the pan just changed. A region
        // being drawn, or the picked boxes moved, go the same way.
        panBy(dx: pull.dx, dy: pull.dy)
        if drag != nil { moveDraggedCard() }
        if marquee != nil { stretchMarquee() }
        if groupDrag != nil { moveGroup() }
    }

    override func mouseMoved(with event: NSEvent) {
        let content = toContent(convert(event.locationInWindow, from: nil))
        let id = schema(at: content)
        toolTip = reviewTip(at: content, over: id)
        applyHover(id)
        hoverPart(id)
        showHoveredRow(id, id.flatMap { row(at: content, in: $0) })
        showGripCursor(id.map { onGrip(content, of: $0) } ?? false)
        updateHandle(at: content, over: id)
    }

    override func mouseExited(with event: NSEvent) {
        toolTip = nil
        showGripCursor(false)
        applyHover(nil)
        hoverPart(nil)
        showHoveredRow(nil, nil)
        hideHandle()
    }

    /// What a review dot says when the pointer is on it. A part's (or a card's) head dot: the cards that changed
    /// after the part, one to a line with when, as the mock's dot says them, and the implementation lines whose
    /// code changed since they were recorded, under a line saying so. The dot at the left of a row: the lines that
    /// leave it. The dot is small, so it is reached from a little beyond it — at least 10 points of the view
    /// across, whatever the zoom.
    func reviewTip(at content: CGPoint, over id: String?) -> String? {
        guard let id, let card = drawnCard(id), card.showsReviewDot, let box = boxes[id] else { return nil }
        let dot = CardRenderer.reviewDotRect(in: box.size).offsetBy(dx: box.minX, dy: box.minY)
        let reach = max(2, (10 / zoom - dot.width) / 2)
        if dot.insetBy(dx: -reach, dy: -reach).contains(content) {
            let code = card.codeReview.filter(\.isChanged)
            return (card.review.map(\.line) + (code.isEmpty ? [] : CardRenderer.codeTip(code))).joined(separator: "\n")
        }
        guard let shown = rows[id]?.content else { return nil }
        for (row, lines) in CardRenderer.codeRows(card, shown).sorted(by: { $0.key < $1.key }) {
            let rowDot = CardRenderer.codeDotRect(row: row, in: shown).offsetBy(dx: box.minX, dy: box.minY)
            if rowDot.insetBy(dx: -reach, dy: -reach).contains(content) {
                return CardRenderer.codeTip(lines).joined(separator: "\n")
            }
        }
        return nil
    }

    /// The same for a point of the view, for the tests.
    func reviewTipAtViewPoint(_ point: CGPoint) -> String? {
        let content = toContent(point)
        return reviewTip(at: content, over: schema(at: content))
    }

    // MARK: - Lines picked and drawn by hand (overview §13)

    /// The handle shown on the row or header under the pointer, where a line may leave it —
    /// kept while the pointer is on it, half of which sits outside the card.
    private func updateHandle(at content: CGPoint, over id: String?) {
        if let shown = handleAt, hypot(content.x - shown.point.x, content.y - shown.point.y) <= handleReach { return }
        guard zoom >= rowPickZoom, connectDrag == nil, drag == nil, groupDrag == nil, resizing == nil,
              let id, let canConnect else {
            hideHandle()
            return
        }
        let hit = RowHit(card: id, row: aimedRow(at: content, in: id))
        guard canConnect(hit), let point = anchor(of: hit) else {
            hideHandle()
            return
        }
        handleAt = (hit, point)
        withoutAnimation {
            handleLayer.position = CGPoint(x: point.x - origin.x, y: point.y - origin.y)
            handleLayer.isHidden = false
        }
    }

    private func hideHandle() {
        guard handleAt != nil || !handleLayer.isHidden else { return }
        handleAt = nil
        withoutAnimation { handleLayer.isHidden = true }
    }

    /// Where a row's or a header's handle sits: on the card's right edge, at its middle.
    private func anchor(of hit: RowHit) -> CGPoint? {
        guard let box = boxes[hit.card] else { return nil }
        guard let row = hit.row else { return CGPoint(x: box.maxX, y: GraphEdges.headerAnchor(of: box)) }
        guard let card = rows[hit.card],
              let index = card.content.rows.firstIndex(where: { Self.rowKey($0) == Self.rowKey(row) }) else { return nil }
        return CGPoint(x: box.maxX, y: box.minY + rowOffset(index, in: card.content))
    }

    /// Where a row's or a header's handle sits, in the view: where つなぐ is shown from it.
    func anchorInView(of hit: RowHit) -> CGPoint? { anchor(of: hit).map(toView) }

    /// The line drawn nearest a point where no card is, within a few points of the view: the
    /// one its kind is drawn as, or another of the edge that is a card's line (what a text
    /// names is not one to pick).
    func line(at content: CGPoint) -> Relation? {
        let local = CGPoint(x: content.x - origin.x, y: content.y - origin.y)
        let reach = max(3, 6 / zoom)
        for edge in edges.reversed() {
            guard let path = edgeLayers[edge.key]?.path else { continue }
            let drawn = edge.vias.first { $0.fromSchemaId == edge.from } ?? edge.vias[0]
            guard let line = drawn.connection != nil ? drawn : edge.vias.first(where: { $0.connection != nil }) else { continue }
            let band = path.copy(strokingWithWidth: reach * 2, lineCap: .round, lineJoin: .round, miterLimit: 1)
            if band.contains(local) { return line }
        }
        return nil
    }

    /// The line `id` marked as picked — thicker, glowing — and the card picked here before
    /// let go; nil, none.
    func showLine(_ id: String?) {
        guard id != pickedLine else { return }
        if id != nil, let previous = selectedSchemaId {
            selectedSchemaId = nil
            withoutAnimation {
                drawCard(previous, scale: wantedScale(previous))
                syncPartOutlines()
            }
            selectField(previous, nil)
        }
        pickedLine = id
        withoutAnimation { restyleEdges() }
    }

    /// A point of the drawn line `id` where no card covers it — its middle, else the nearest
    /// to it — in the view: for the tests, which press it.
    func viewPoint(ofLine id: String) -> CGPoint? {
        guard let edge = edges.first(where: { $0.vias.contains { $0.connection == id } }),
              let wire = wires[edge.key] else { return nil }
        func at(_ t: CGFloat) -> CGPoint {
            let p = wire.point(at: t)
            return CGPoint(x: p.x + origin.x, y: p.y + origin.y)
        }
        let tries: [CGFloat] = [0.5, 0.45, 0.55, 0.4, 0.6, 0.35, 0.65, 0.3, 0.7, 0.25, 0.75, 0.2, 0.8, 0.15, 0.85]
        let free = tries.map(at).first { point in !boxes.values.contains { $0.insetBy(dx: -4, dy: -4).contains(point) } }
        return free.map(toView)
    }

    /// Whether the line `id` is drawn marked as picked — for the tests.
    func isMarked(line id: String) -> Bool {
        edges.contains { edge in
            edge.vias.contains { $0.connection == id } && (edgeLayers[edge.key]?.lineWidth ?? 0) > 2
        }
    }

    /// The card under this point: the one in front, by the same order the drawing
    /// uses.
    /// What was removed is drawn where it was, never pressed: a press goes through it.
    func schema(at content: CGPoint) -> String? {
        if let part = partNote(at: content) { return part }
        return stack.last { boxes[$0]?.contains(content) == true && !goneCards.contains($0) }
    }

    /// Where a card sits, in content coordinates.
    func frame(ofCard id: String) -> CGRect? { boxes[id] }

    /// The card under a point of the view, for the tests.
    func schemaAtViewPoint(_ point: CGPoint) -> String? { schema(at: toContent(point)) }

    /// Where a point of the content is in the view, for the tests.
    func viewPoint(ofContent point: CGPoint) -> CGPoint { toView(point) }

    /// Where a point of the view is in the content, for the tests.
    func contentPoint(ofView point: CGPoint) -> CGPoint { toContent(point) }

    /// Where a press on a drawn row's words lands — past its fold mark, left of any switch
    /// on its right — in the view, by the row's `rowKey`; nil when it is not drawn. For the
    /// tests, which press rows as a person does.
    func viewPoint(ofRow key: String, in id: String) -> CGPoint? {
        guard let box = boxes[id], let card = rows[id],
              let index = card.content.rows.firstIndex(where: { Self.rowKey($0) == key }) else { return nil }
        let row = card.content.rows[index]
        return toView(CGPoint(x: box.minX + 26 + CGFloat(row.depth) * 13, y: box.minY + rowOffset(index, in: card.content)))
    }

    /// The middle of a card's header, in the view — for the tests.
    func viewPoint(ofHeader id: String) -> CGPoint? {
        guard let box = boxes[id] else { return nil }
        return toView(CGPoint(x: box.minX + 30, y: box.minY + CardGeometry.headerHeight / 2))
    }

    /// How far forward a card is drawn.
    func drawnDepth(ofCard id: String) -> CGFloat? { cardLayers[id]?.zPosition }

    /// How far forward the selected row's mark is drawn, while it shows.
    var selectedRowDepth: CGFloat? { selectedRowLayer.isHidden ? nil : selectedRowLayer.zPosition }

    private func restyleHighlights() {
        let theme = self.theme
        hoverRowLayer.backgroundColor = theme.ink.withAlphaComponent(0.10).cgColor
        selectedRowLayer.backgroundColor = theme.selection.cgColor
        selectedRowLayer.borderColor = theme.brass.cgColor
        selectedRowLayer.borderWidth = 1
        handleLayer.fillColor = theme.brass.cgColor
        handleLayer.strokeColor = theme.background.cgColor
        handleMark.strokeColor = theme.background.cgColor
        connectLayer.strokeColor = theme.brass.cgColor
        marqueeLayer.strokeColor = theme.brass.cgColor
        marqueeLayer.fillColor = theme.brass.withAlphaComponent(0.08).cgColor
    }

    /// Which drawn row sits under this point, if any. Rows past the cut are not
    /// drawn, so they cannot be pointed at.
    /// What a press at `content` does on a custom card's form, when it lands on
    /// a switch or an option — where the card draws them (`CardRenderer.drawForm`).
    func formPress(at content: CGPoint, in id: String) -> FormPress? {
        guard !isPart(id), let box = boxes[id], let card = rows[id], let index = row(at: content, in: id),
              let form = card.content.rows[index].form else { return nil }
        let local = CGPoint(x: content.x - box.minX, y: content.y - box.minY)
        let rect = CGRect(x: 1.5, y: listTop(card.content) + CGFloat(index) * CardGeometry.rowHeight,
                          width: box.width - 3, height: CardGeometry.rowHeight)
        switch form {
        case .toggle:
            return CardRenderer.toggleRect(in: rect).insetBy(dx: -4, dy: -4).contains(local)
                ? .toggle(card.content.rows[index].path) : nil
        case .segments(let field, let options, _, let labels):
            // Said by its label, as it is drawn; where the options do not fit, a menu (`formMenu`).
            let label = Self.formLabel(card.content.rows[index], field)
            let shown = options.map { CardForm.Control.shown($0, labels) }
            guard CardRenderer.segmentsFit(in: rect, label: label, options: shown) else { return nil }
            let boxes = CardRenderer.segmentRects(in: rect, label: label, count: options.count)
            guard let at = boxes.firstIndex(where: { $0.contains(local) }) else { return nil }
            return .choose(field, options[at])
        case .menu:
            return nil
        }
    }

    /// What a row's control is said by: its label, else its field's name.
    private static func formLabel(_ row: CardRow, _ field: String) -> String {
        guard let label = row.step?.label, !label.isEmpty else { return field }
        return label
    }

    /// A menu a press opens on a card: a face's dropdown list, or a choice
    /// whose options do not fit its row — its field, its options, the one chosen, and its button
    /// in the card's content. Where the card draws it (`CardRenderer.drawMenu`).
    struct FormMenu: Equatable {
        var field: String
        var options: [String]
        var chosen: String
        var button: CGRect
        /// How each option is shown, when not as it is.
        var labels: [String: String] = [:]
    }

    func formMenu(at content: CGPoint, in id: String) -> FormMenu? {
        guard !isPart(id), let box = boxes[id], let card = rows[id], let index = row(at: content, in: id),
              let form = card.content.rows[index].form else { return nil }
        let local = CGPoint(x: content.x - box.minX, y: content.y - box.minY)
        let rect = CGRect(x: 1.5, y: listTop(card.content) + CGFloat(index) * CardGeometry.rowHeight,
                          width: box.width - 3, height: CardGeometry.rowHeight)
        let (field, options, chosen, labels): (String, [String], String, [String: String])
        switch form {
        case .menu(let f, let o, let c, let l): (field, options, chosen, labels) = (f, o, c, l)
        case .segments(let f, let o, let c, let l)
            where !CardRenderer.segmentsFit(in: rect, label: Self.formLabel(card.content.rows[index], f),
                                            options: o.map { CardForm.Control.shown($0, l) }):
            (field, options, chosen, labels) = (f, o, c, l)
        default: return nil
        }
        let button = CardRenderer.menuRect(in: rect, label: Self.formLabel(card.content.rows[index], field))
        guard button.insetBy(dx: -2, dy: -3).contains(local) else { return nil }
        return FormMenu(field: field, options: options, chosen: chosen, button: button.offsetBy(dx: box.minX, dy: box.minY),
                        labels: labels)
    }

    /// The menu of `menu`'s options under its button, the one chosen checked: one chosen is
    /// pressed as an option is (`FormPress.choose`).
    private func popUpFormMenu(_ menu: FormMenu, of id: String,
                               _ press: @escaping (String, FormPress) -> Void) {
        let list = NSMenu()
        for option in menu.options {
            list.addItem(ActionMenuItem(CardForm.Control.shown(option, menu.labels), on: option == menu.chosen) {
                press(id, .choose(menu.field, option))
            })
        }
        let under = viewPoint(ofContent: CGPoint(x: menu.button.minX, y: menu.button.maxY))
        list.popUp(positioning: nil, at: CGPoint(x: under.x, y: under.y + 2), in: self)
    }

    private func row(at content: CGPoint, in schemaId: String) -> Int? {
        guard let box = boxes[schemaId], let card = rows[schemaId] else { return nil }
        let top = box.minY + listTop(card.content)
        let index = Int(((content.y - top) / CardGeometry.rowHeight).rounded(.down))
        guard index >= 0, index < card.content.rows.count else { return nil }
        return index
    }

    /// Give every untouched card its resting folds: each value object and enum it
    /// holds folded to one row with its name (`restingFolds`). A card someone has
    /// folded or opened keeps what they did.
    private func applyFoldDefault(to schemas: [SchemaNode], lookup: @escaping SchemaLookup) {
        for s in schemas where !touchedCards.contains(s.id) {
            let resting = restingFolds(s, lookup: lookup)
            if resting.isEmpty { cardFolds.removeValue(forKey: s.id) }
            else { cardFolds[s.id] = resting }
        }
    }

    /// Fold or unfold one row of a card.
    ///
    /// The rows below it come and go, so the card is redrawn and its lines recut —
    /// a line whose row folded away lands on the row that hides it, which
    /// `nearestRow` already arranges.
    private func toggleFold(_ schemaId: String, path: String) {
        var folds = cardFolds[schemaId] ?? []
        if folds.contains(path) { folds.remove(path) } else { folds.insert(path) }
        setFolds(schemaId, folds)
    }

    /// Every row that has anything under it, or none at all.
    func foldAll(_ schemaId: String, _ folded: Bool) {
        guard let card = rows[schemaId] else { return }
        // Read off `all`, not the window: folding what is on screen and leaving the
        // rest is not what "すべて" means. Once folded, the rows that were expandable
        // deeper down are gone from `all` — so this is taken from the *unfolded*
        // shape, which is what the card would look like with nothing folded at all.
        let open = openRows(schemaId)
        setFolds(schemaId, folded ? Set(open.filter(\.expandable).map(\.path)) : [])
    }

    /// The card's rows with nothing folded, for deciding what "fold everything" means.
    private func openRows(_ schemaId: String) -> [CardRow] {
        guard let schema = drawnCard(schemaId) else { return [] }
        let lookup = self.lookup
        return cardRows(schema, lookup: lookup,
                        inlineReuse: filters.nesting.nests(schemaId), tree: tree).all
    }

    func canFold(_ schemaId: String) -> Bool {
        openRows(schemaId).contains(where: \.expandable)
    }

    private func setFolds(_ schemaId: String, _ folded: Set<String>) {
        touchedCards.insert(schemaId)
        guard cardFolds[schemaId] ?? [] != folded else { return }
        if folded.isEmpty { cardFolds.removeValue(forKey: schemaId) }
        else { cardFolds[schemaId] = folded }

        guard let schema = drawnCard(schemaId) else { return }
        let lookup = self.lookup
        let content = cardRows(schema, lookup: lookup,
                               inlineReuse: filters.nesting.nests(schemaId),
                               from: cardScroll[schemaId] ?? 0, folded: folded, tree: tree,
                               form: forms[schemaId] ?? CardForm())
        rows[schemaId] = CardIndex(content)
        cardScroll[schemaId] = content.first
        boxes[schemaId]?.size = boxSize(schemaId, content)
        withoutAnimation {
            cardLayers[schemaId]?.frame = layerFrame(schemaId)
            drawCard(schemaId, scale: CardRenderer.baseScale)
            cutEdges()
            selectField(schemaId, selectedFieldId)
            showHoveredRow(nil, nil)
        }
    }

    /// Whether this point is on a row's disclosure mark rather than on the row.
    func disclosure(at content: CGPoint, in schemaId: String) -> String? {
        guard zoom >= rowPickZoom, !isPart(schemaId),
              let box = boxes[schemaId], let card = rows[schemaId],
              let index = row(at: content, in: schemaId),
              index < card.content.rows.count else { return nil }
        let candidate = card.content.rows[index]
        guard candidate.expandable else { return nil }

        let left = candidate.discloseLeft(from: box.minX)
        guard content.x >= left, content.x <= left + CardGeometry.discloseWidth else {
            return nil
        }
        return candidate.path
    }

    /// Move one card's window. Returns whether it had anywhere to go, so a card
    /// already at its end hands the gesture back rather than swallowing it.
    @discardableResult
    private func scrollCard(_ schemaId: String, by delta: CGFloat) -> Bool {
        guard let card = rows[schemaId], card.content.hidden > 0 else { return false }

        // Swiping up reveals what is below, the way it does everywhere else.
        let carry = (cardScrollCarry[schemaId] ?? 0) - delta
        let steps = Int((carry / CardGeometry.rowHeight).rounded(.towardZero))
        cardScrollCarry[schemaId] = carry - CGFloat(steps) * CardGeometry.rowHeight
        guard steps != 0 else { return true }

        let next = min(max(0, card.content.first + steps), card.content.maxScroll)
        guard next != card.content.first else { return true }
        cardScroll[schemaId] = next

        guard let schema = drawnCard(schemaId) else { return false }
        let lookup = self.lookup
        let content = cardRows(schema, lookup: lookup,
                               inlineReuse: filters.nesting.nests(schemaId), from: next,
                               folded: cardFolds[schemaId] ?? [], tree: tree,
                               form: forms[schemaId] ?? CardForm())
        rows[schemaId] = CardIndex(content)
        // A strip is only drawn for a direction that has rows out of sight, so the
        // card gains one leaving the top and loses one at the end. Its box has to
        // follow, or the texture is drawn into a frame of the wrong height — unless
        // it has a size of its own.
        boxes[schemaId]?.size = boxSize(schemaId, content)
        withoutAnimation {
            cardLayers[schemaId]?.frame = layerFrame(schemaId)
            drawCard(schemaId, scale: CardRenderer.baseScale)
            // The rows moved, so every line anchored to one of them has to be recut —
            // and the two highlights are positions in the window, not in the field
            // list, so they have to be asked again too.
            cutEdges()
            selectField(schemaId, selectedFieldId)
            if let hoveredRow, hoveredRow.schemaId == schemaId {
                showHoveredRow(nil, nil)
            }
        }
        return true
    }

    private func rowRect(_ schemaId: String, _ index: Int) -> CGRect? {
        guard let box = boxes[schemaId], let card = rows[schemaId] else { return nil }
        let centre = box.minY + rowOffset(index, in: card.content)
        return CGRect(x: box.minX + 2, y: centre - CardGeometry.rowHeight / 2,
                      width: box.width - 4, height: CardGeometry.rowHeight)
            .offsetBy(dx: -origin.x, dy: -origin.y)
    }

    private func showHoveredRow(_ schemaId: String?, _ index: Int?) {
        let next = zip([schemaId], [index]).compactMap { id, i -> (String, Int)? in
            guard let id, let i else { return nil }
            return (id, i)
        }.first

        if next?.0 == hoveredRow?.schemaId && next?.1 == hoveredRow?.index { return }
        hoveredRow = next.map { (schemaId: $0.0, index: $0.1) }

        withoutAnimation {
            guard let hoveredRow, let rect = rowRect(hoveredRow.schemaId, hoveredRow.index) else {
                hoverRowLayer.isHidden = true
                return
            }
            hoverRowLayer.frame = rect
            hoverRowLayer.zPosition = depth(ofCard: hoveredRow.schemaId) + Depth.hoverRow * depthStep
            hoverRowLayer.isHidden = false
        }
    }

    /// Mark a row of a card as picked: a field by its id, any other row by its path
    /// (`rowKey`) — nil, none.
    func selectField(_ schemaId: String, _ fieldId: String?) {
        selectedFieldId = fieldId
        withoutAnimation {
            guard let fieldId,
                  let card = rows[schemaId],
                  let index = card.content.rows.firstIndex(where: { Self.rowKey($0) == fieldId }),
                  let rect = rowRect(schemaId, index)
            else {
                selectedRowLayer.isHidden = true
                return
            }
            selectedRowLayer.frame = rect
            selectedRowLayer.zPosition = depth(ofCard: schemaId) + Depth.selectedRow * depthStep
            selectedRowLayer.isHidden = false
        }
    }

    /// Hover is not state.
    ///
    /// In the web build, making highlight a render dependency rebuilt all 163 cards
    /// and 304 edges on every pointer move — a 216ms median frame against 16.7ms
    /// over seven cards, for a change the browser resolved to eight nodes. Here it
    /// sets `opacity` on layers that already exist, and nothing is rebuilt at all.
    private func applyHover(_ id: String?) {
        guard hovered != id else { return }
        hovered = id
        // A ui section is looked at as the screen its parts make: nothing fades, the part is outlined.
        guard !drawsParts else { return }

        withoutAnimation {
            guard let id else {
                for layer in cardLayers.values { layer.opacity = 1 }
                for key in edgeLayers.keys { fade(key, to: 1) }
                return
            }
            let near = neighbours[id] ?? []
            for (key, layer) in cardLayers {
                layer.opacity = (key == id || near.contains(key)) ? 1 : 0.18
            }
            for edge in edges {
                let touches = edge.from == id || edge.to == id
                fade(edge.key, to: touches ? 1 : 0.08)
            }
        }
    }

    /// A line and everything drawn as part of it.
    ///
    /// The mark at each end and the multiplicity beside it are layers of their own —
    /// the caps have to be filled while the line is stroked, and a dash must not dash
    /// the mark. Only the line itself was being dimmed, which left the diamonds and
    /// arrowheads of every hidden line sitting at full strength over a dimmed
    /// diagram, pointing at nothing a reader could follow.
    private func fade(_ key: String, to opacity: Float) {
        edgeLayers[key]?.opacity = opacity
        capLayers[key]?.opacity = opacity
        headLayers[key]?.opacity = opacity
        labelLayers[key]?.opacity = opacity
    }

    // MARK: - Settling

    /// Work that only matters once the hand stops: sharpening what is on screen.
    /// Doing it per frame would be the web build's mistake in a new costume.
    private func scheduleSettle() {
        settleWork?.cancel()
        let work = DispatchWorkItem { [weak self] in self?.settle() }
        settleWork = work
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.16, execute: work)
    }

    private func settle() {
        DiagramTimes.measure("settle") { settling() }
        DiagramTimes.endSwitch()
    }

    private func settling() {
        drawUndrawnNow()
        withoutAnimation {
            for (id, layer) in cardLayers {
                let want = wantedScale(id)
                if layer.contentsScale != want {
                    drawCard(id, scale: want)
                } else if let face = partFaces[id], partSharp[id] != partSharpness(id, face) {
                    // A part's picture taken as fine as it is now seen.
                    drawCard(id, scale: want)
                } else if isPart(id) {
                    drawDecor(id)
                }
            }
            // Vector edges rasterise at their own scale; without this they soften
            // as you zoom in.
            let edgeScale = max(1, min(4, zoom * (window?.backingScaleFactor ?? 2)))
            for layer in edgeLayers.values where layer.contentsScale != edgeScale {
                layer.contentsScale = edgeScale
            }
            for layers in frameLayers.values where layers.title.contentsScale != edgeScale {
                layers.box.contentsScale = edgeScale
                layers.title.contentsScale = edgeScale
            }
        }
    }

    override func layout() {
        super.layout()
        // What was held until the panel had its size: where it looked, then the card.
        if bounds.width > 0, bounds.height > 0 {
            if let look = pendingLook {
                pendingLook = nil
                center(on: look.center, zoom: look.zoom)
            }
            if let card = pendingReveal {
                pendingReveal = nil
                reveal(card.card, whole: card.whole)
            }
            if let going = pendingFocus {
                pendingFocus = nil
                focus(going.card, row: going.row)
            }
            drawUndrawnNow()
        }
        withoutAnimation { syncGrid() }
        updateOffscreenHints()
        scheduleSettle()
    }

    private func withoutAnimation(_ body: () -> Void) {
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        body()
        CATransaction.commit()
    }
}

// MARK: - Measurement

/// Records how long each interaction step takes to apply.
///
/// The web build's numbers were 172ms per pan step and 209ms per zoom step at this
/// size. Keeping the same measurement here is the only way to say whether going
/// native actually bought anything.
final class FrameProbe {
    private(set) var samples: [Double] = []

    func begin() -> () -> Void {
        let start = CFAbsoluteTimeGetCurrent()
        return { [weak self] in
            self?.samples.append((CFAbsoluteTimeGetCurrent() - start) * 1000)
        }
    }

    func reset() { samples.removeAll() }

    var report: String {
        guard !samples.isEmpty else { return "no samples" }
        let sorted = samples.sorted()
        func percentile(_ p: Double) -> Double { sorted[min(sorted.count - 1, Int(Double(sorted.count) * p))] }
        return String(format: "n=%d  median %.2fms  p95 %.2fms  max %.2fms",
                      sorted.count, percentile(0.5), percentile(0.95), sorted.last!)
    }
}

/// The field a card is written in where it is (`CardCanvasView.beginInPlace`): ⌘Return keeps
/// what was written, Esc lets it go, and leaving it — another place pressed — keeps it.
final class InPlaceTextView: NSTextView {
    /// Done: kept or let go, and whether the diagram takes the keys again.
    var done: ((_ keep: Bool, _ refocus: Bool) -> Void)?

    override func keyDown(with event: NSEvent) {
        if event.modifierFlags.contains(.command), [36, 76].contains(event.keyCode) {
            done?(true, true)
            return
        }
        super.keyDown(with: event)
    }

    override func cancelOperation(_ sender: Any?) {
        done?(false, true)
    }

    override func resignFirstResponder() -> Bool {
        let resigned = super.resignFirstResponder()
        // Left for another place: kept once the press that took the keys is over.
        if resigned, done != nil {
            DispatchQueue.main.async { [weak self] in self?.done?(true, false) }
        }
        return resigned
    }
}
