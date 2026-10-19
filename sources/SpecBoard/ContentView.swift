import SwiftUI

/// Bridges the AppKit canvas into SwiftUI. The canvas owns its own layers and is
/// never rebuilt from scratch — `show` is called when the project changes, not when
/// the view updates.
struct CardCanvas: NSViewRepresentable {
    var detail: ProjectDetail
    /// Changes when the arrangement was rebuilt on purpose. See `AppModel`.
    var layoutGeneration: Int
    /// Changes when the project was re-read. See `AppModel.dataGeneration`.
    var dataGeneration: Int
    /// What the section on screen draws from the others. See `Surroundings`.
    var surroundings = Surroundings()
    /// Which section that is, for `CardCanvasController.shownSection`.
    var section: String = ""
    var controller: CardCanvasController
    /// What a press, a double-click, Return and a right-click on a card or a row do here
    /// (`DiagramPresses`): the same rule on every diagram.
    var presses: DiagramPresses
    var onMoveByHand: (String, GridPoint) -> Void = { _, _ in }
    /// Whether a box dropped stays where it is dropped, with no margin kept and no jump
    /// (`CardCanvasView.keepsPlaces`).
    var keepsPlaces = false
    /// Whether a card may be moved by hand: every card, unless said.
    var canMoveByHand: (String) -> Bool = { _ in true }
    /// A writing card sized by its corner.
    var onResizeByHand: (String, GridSize) -> Void = { _, _ in }
    /// A custom card's switch or option pressed on the diagram.
    var onFormPress: (String, CardCanvasView.FormPress) -> Void = { _, _ in }
    /// What stands for another section, put where it had no place (`CardCanvasView.onPlaceOutside`):
    /// told after the drawing, not while it is drawn.
    var onPlaceOutside: ([String: GridPoint]) -> Void = { _ in }
    /// A card written where it is when pressed twice (`CardCanvasView.canEditInPlace`): whether
    /// it may be, its text, and what was written. Nowhere when not said.
    var canEditInPlace: (String) -> Bool = { _ in false }
    var inPlaceText: (String) -> String = { _ in "" }
    var onEditInPlace: (String, String) -> Void = { _, _ in }
    /// What a card drawn as a document shows (`CardCanvasView.documentText`).
    var documentText: ((SchemaNode, (() -> Void)?) -> DocumentTexts.Face)? = nil
    /// Whether the section is a ui section's, its parts drawn as their faces (`CardCanvasView.drawsParts`),
    /// what each part's face is, and what is laid over them.
    var drawsParts = false
    var partFace: ((SchemaNode, (() -> Void)?) -> PartFaces.Answer)? = nil
    var partLook = CardCanvasView.PartLook()
    var onClickEmpty: () -> Void = {}
    /// How the diagram lays its cards out (§12).
    var settings = LayoutSettings.standard
    /// Every card's width (§12).
    var cardWidth = CardGeometry.cardWidth
    /// What the open branch changed, made, or saw main change (`BranchMarks`).
    var marks = BranchMarks()
    /// The line picked (`AppModel.selectedLine`), by its id: marked on the diagram.
    var pickedLine: String? = nil
    /// Lines drawn as curves whatever this Mac's setting (`CardCanvasView.curvesOnly`).
    var curvesOnly = false

    func makeNSView(context: Context) -> CardCanvasView {
        let view = CardCanvasView()
        view.curvesOnly = curvesOnly
        context.coordinator.arranged = layoutGeneration
        controller.attach(view)
        WindowCapture.canvas = view
        presses.wire(view)
        view.onMoveByHand = onMoveByHand
        view.keepsPlaces = keepsPlaces
        view.canMoveByHand = canMoveByHand
        view.onResizeByHand = onResizeByHand
        view.onFormPress = onFormPress
        view.onPlaceOutside = { placed in DispatchQueue.main.async { onPlaceOutside(placed) } }
        view.canEditInPlace = canEditInPlace
        view.inPlaceText = inPlaceText
        view.onEditInPlace = onEditInPlace
        view.documentText = documentText
        view.drawsParts = drawsParts
        view.partFace = partFace
        view.partLook = partLook
        view.onClickEmpty = onClickEmpty
        view.show(detail, filters: controller.filters,
                  surroundings: surroundings, settings: settings, cardWidth: cardWidth, marks: marks)
        view.showLine(pickedLine)
        // Made again — back from a card's editor, say: looking where it looked.
        if let look = controller.look(for: section) { view.restore(look) }
        context.coordinator.shown = revision
        context.coordinator.controller = controller
        context.coordinator.section = section
        controller.shownSection = section
        controller.shown()
        return view
    }

    /// Going away — for a card's editor, a dashboard: where it looked is kept.
    static func dismantleNSView(_ view: CardCanvasView, coordinator: Coordinator) {
        coordinator.controller?.remember(view.look, for: coordinator.section)
        coordinator.controller?.detach(view)
    }

    func updateNSView(_ view: CardCanvasView, context: Context) {
        presses.wire(view)
        view.onMoveByHand = onMoveByHand
        view.keepsPlaces = keepsPlaces
        view.canMoveByHand = canMoveByHand
        view.onResizeByHand = onResizeByHand
        view.onFormPress = onFormPress
        view.onPlaceOutside = { placed in DispatchQueue.main.async { onPlaceOutside(placed) } }
        view.canEditInPlace = canEditInPlace
        view.inPlaceText = inPlaceText
        view.onEditInPlace = onEditInPlace
        view.documentText = documentText
        view.partFace = partFace
        view.partLook = partLook
        view.onClickEmpty = onClickEmpty
        context.coordinator.section = section
        view.curvesOnly = curvesOnly
        if view.pickedLine != pickedLine { view.showLine(pickedLine) }
        guard context.coordinator.shown != revision else { return }
        context.coordinator.shown = revision
        // An arrangement rebuilt on purpose moves every card, so the viewport is
        // pointing at wherever the old ones used to be. An ordinary reload after a
        // write must not move it — see `show`.
        let arranged = context.coordinator.arranged != layoutGeneration
        context.coordinator.arranged = layoutGeneration
        view.drawsParts = drawsParts
        view.show(detail, filters: controller.filters,
                  fit: arranged, surroundings: surroundings, settings: settings, cardWidth: cardWidth, marks: marks)
        controller.shownSection = section
        controller.shown()
    }

    func makeCoordinator() -> Coordinator { Coordinator() }

    /// "The drawn content may have changed."
    ///
    /// Card positions move constantly while dragging and must *not* count, since the
    /// canvas already moved that card itself — reloading there would fight the drag.
    /// `dataGeneration` draws exactly that line: every write re-reads the project and
    /// bumps it, while a drag writes coordinates and does not.
    ///
    /// A sample of what the project contains — counts of schemas, top level fields,
    /// relations — would miss every edit that changes nothing countable. See
    /// `AppModel.dataGeneration`.
    ///
    /// `layoutGeneration` is separate because auto-layout changes positions and
    /// nothing else: without it the button wrote new coordinates and the canvas never
    /// looked at them.
    ///
    /// A code file's functions and types count too: they are read from the files beside the
    /// project, after the diagram is first drawn and without the project being read again, so
    /// nothing else here changes when they arrive and the cards would stay header-only.
    var revision: Int {
        var hasher = Hasher()
        hasher.combine(layoutGeneration)
        hasher.combine(dataGeneration)
        hasher.combine(detail.project.id)
        hasher.combine(detail.schemas.map(\.id))
        hasher.combine(section)
        hasher.combine(drawsParts)
        hasher.combine(settings)
        hasher.combine(cardWidth)
        hasher.combine(marks.changed.sorted())
        hasher.combine(marks.made.sorted())
        hasher.combine(marks.removed.sorted())
        hasher.combine(marks.mainChanged.sorted())
        hasher.combine(marks.gone.cards.map(\.id))
        for card in detail.schemas where !card.symbols.isEmpty {
            hasher.combine(card.id)
            hasher.combine(card.symbols)
        }
        return hasher.finalize()
    }

    final class Coordinator {
        var shown: Int?
        var arranged: Int?
        /// For keeping where it looked when it goes away.
        weak var controller: CardCanvasController?
        var section = ""
    }
}

struct ContentView: View {
    @State private var model = AppModel(memory: .standard, readsOffMain: true)
    @State private var graph = CardCanvasController()
    /// A card's connections diagram's canvas (`ConnectionsView`).
    @State private var connections = CardCanvasController()
    @State private var hosts = RemoteHosts()
    @Environment(\.colorScheme) private var colorScheme

    private var theme: GraphTheme { colorScheme == .dark ? .dark : .light }

    /// The nav panel on the left (a project's branches, sections and cards), from the top of the
    /// window down: open unless it was shut, and kept (overview §13 「窓の形」).
    @AppStorage("navigator.open") private var navPanelVisible = true
    /// The window's top: in full screen or not, how far its own buttons reach (`WindowChrome`).
    @State private var chrome = WindowChrome()
    /// The nav panel's width, dragged at its edge and kept.
    @AppStorage("navigator.width") private var navPanelWidth = PaneSplit.navPanelStart
    @State private var navPanelDragStart: CGFloat?
    /// The property panel on the right: open or folded, and how wide — kept.
    @AppStorage("properties.open") private var propertiesOpen = true
    @AppStorage("properties.width") private var propertiesWidth = PaneSplit.propertiesStart
    @State private var propertiesDragStart: CGFloat?

    /// The relation panel's filter window (線の引き方・セクション外・種別), open or not. The kinds
    /// of line are switched at the diagram's top (`LineSwitches`, `DiagramFrame`).
    @State private var filtersOpen = false

    /// Read here so every body in this file re-renders when it changes.
    private var typography: Typography { Typography.shared }
    private var updater: Updater { Updater.shared }

    @State private var draftSchemaName = ""
    @State private var draftSchemaLabel = ""
    @State private var draftSchemaKind = SchemaKind.entity.rawValue
    /// A data card's shape chosen (its id).
    @State private var draftShape = ""
    /// The ui section a new frame is to hold, by id.
    @State private var draftFrameTarget = ""

    /// What a card drawn as a document shows on the diagram, made now (`AppModel.documentTextNow`).
    static func documentText(_ shown: SchemaNode, of model: AppModel) -> String? {
        model.documentTextNow(shown)
    }

    var body: some View {
        // Across the bottom of the window, under the nav panel too: the status bar (`WindowBody`).
        WindowBody(model: model, hosts: hosts, updater: updater) {
            Group {
                switch model.phase {
                case .loading:
                    VStack(spacing: 0) {
                        emptyTopRow
                        status(L10n[.Root.statusLoading])
                    }
                case .failed(let message):
                    VStack(spacing: 0) {
                        emptyTopRow
                        VStack(spacing: 10) {
                            Text(L10n[.Root.statusFailed]).font(.app(14, weight: .semibold))
                            Text(message)
                                .font(.app(11, design: .monospaced))
                                .foregroundStyle(.secondary)
                                .textSelection(.enabled)
                                .frame(maxWidth: 560)
                        }
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                    }
                case .ready:
                    if let detail = model.detail, !model.atHome {
                        // The panels' widths as the window has room for them: the stored widths are
                        // dragged ones, kept for when the window grows.
                        GeometryReader { window in
                            let widths = panelWidths(in: window.size.width)
                            // The nav panel up to the top of the window, the top bar beside it (overview §13 「窓の形」).
                            WindowFrame(
                                pane: navPanelVisible ? .open(width: widths.nav) : .shut,
                                chrome: chrome,
                                onToggleNavPanel: { navPanelVisible.toggle() },
                                onDragNavPanel: { moved in
                                    let start = navPanelDragStart ?? widths.nav
                                    navPanelDragStart = start
                                    let most = max(PaneSplit.navPanelRange.lowerBound,
                                                   PaneSplit.room(window: window.size.width, navShown: true, properties: propertiesSeat)
                                                       - (propertiesSeat == .open ? widths.properties : 0))
                                    navPanelWidth = Double(min(PaneSplit.navPanelWidth(Double(start + moved)), most))
                                    // What the other panel is seen to be, so it does not shrink further as this one is dragged.
                                    if propertiesSeat == .open { propertiesWidth = Double(widths.properties) }
                                },
                                onDragNavPanelEnd: { navPanelDragStart = nil },
                                history: model
                            ) {
                                // A card's row pressed once goes to where it is drawn through the model
                                // (`AppModel.perform`, `focusRequest`), twice to its editor.
                                NavPanel(model: model, drawn: drawnSchemaIds)
                            } bar: {
                                TopBar(over: .project(detail), model: model, updater: updater, settingsPage: $settingsPage)
                            } main: {
                                VStack(spacing: 0) {
                                    if model.isArchived { archivedBanner(detail.project.id) }
                                    HoldBanners(model: model)
                                    if let shown = model.resultView { ResultBanner(model: model, shown: shown) }
                                    Divider()
                                    graphScreen(model.sectionDetail ?? detail, window: window.size.width)
                                }
                            }
                        }
                    } else {
                        // The projects over the whole window: asked for, or nothing open —
                        // not an error state, it knows how to make one.
                        WindowFrame(pane: .absent, chrome: chrome) {
                            EmptyView()
                        } bar: {
                            TopBar(over: .projects, model: model, updater: updater, settingsPage: $settingsPage)
                        } main: {
                            VStack(spacing: 0) {
                                Divider()
                                ProjectHome(model: model, hosts: hosts) { model.atHome = false }
                            }
                        }
                    }
                }
            }
        }
        // Over the whole window, and what it covers stays as it was underneath — not to be pressed
        // or typed into while it is covered.
        .modifier(CoveredBySettings(covered: settingsPage != nil))
        .overlay {
            if settingsPage != nil {
                SettingsScreen(page: $settingsPage, model: model, hosts: hosts, updater: updater)
            }
        }
        // No title bar: the rows at the top are where it was (overview §13 「窓の形」).
        .ignoresSafeArea(.container, edges: .top)
        // How small the window may be: said here, not by what it holds (`WindowLeast`).
        .frame(minWidth: WindowLeast.width, minHeight: WindowLeast.height)
        // Full screen is not something SwiftUI keeps across a quit; this does.
        .background(WindowStateKeeper())
        // Full screen or not, and the window's buttons in the middle of the top row.
        .background(WindowChromeKeeper(chrome: chrome))
        // A `specboard://` link (`AppLink`): opened in this window, not in a new one.
        .onOpenURL { model.openLink($0) }
        .onReceive(NotificationCenter.default.publisher(for: NSApplication.didBecomeActiveNotification)) { _ in
            model.reportWindows()
        }
        .onReceive(NotificationCenter.default.publisher(for: NSApplication.didResignActiveNotification)) { _ in
            model.reportWindows()
        }
        .handlesExternalEvents(preferring: ["*"], allowing: ["*"])
        // What 表示 and 編集 in the menu bar act on: the diagram, when it is up.
        .focusedSceneValue(\.diagram, model.detail != nil && !model.atHome ? diagramActions : nil)
        .sheet(item: $model.newSchemaRequest) { request in newSchemaPrompt(request) }
        .sheet(item: $model.mergeAsk) { request in MergeSheet(model: model, request: request) }
        .sheet(item: $model.undoRefusal) { refusal in UndoConflictsSheet(refusal: refusal) }
        // A name the window asks for itself: a card ghost's, where its section has the name.
        .nameSheet($model.nameAsk)
        // A goal's cancel that throws away what its branch has and main does not, asked first.
        .alert(model.discardAsk.map { L10n[.Board.actionDiscardTitle(model.schema($0.card)?.name ?? "")] } ?? "",
               isPresented: Binding(get: { model.discardAsk != nil }, set: { if !$0 { model.discardAsk = nil } }),
               presenting: model.discardAsk) { ask in
            Button(L10n[.Board.actionDiscardConfirm], role: .destructive) {
                model.runCardAction(ask.card, "cancel", ["discard": true])
            }
            Button(L10n[.Board.actionDiscardKeep], role: .cancel) {}
        } message: { ask in
            Text(L10n[.Board.actionDiscardMessage(ask.branch, ask.changes)])
        }
        .task {
            // A card put in the section on screen is brought into view on its diagram.
            model.relationCanvas = graph
            await model.start()
            // Go back to what was selected last time. The canvas may not have drawn
            // anything yet; the controller holds this until it has.
            if let id = model.selectedSchemaId {
                go(to: id, field: model.selectedFieldId)
            }
            // What the relays kept, which are gone, is forgotten.
            MCPAccess.forgetRelaySettings()
            // The agents' cores: their projects in the list while they are connected, and this window
            // as their MCP's `app_control` sees it (in front or not).
            model.isFront = { NSApp.isActive }
            hosts.onCoreConnected = { model.agentConnected($0, at: $1) }
            hosts.onCoreGone = { model.agentGone($0, said: $1) }
            hosts.onRenamed = { model.agentRenamed($0, to: $1) }
            hosts.start()
            updater.start()
            // The web views that take the documents' pictures, made ready once the window is up rather
            // than as the first section with documents is shown.
            try? await Task.sleep(for: .seconds(1))
            DocSnapshots.shared.prewarm()
        }
        // Another project opened: its lines as its definitions draw them from the start — what was
        // switched by hand is not kept.
        .onChange(of: model.detail?.project.id) { _, _ in
            graph.resetLines()
            connections.resetLines()
        }
        .onChange(of: model.revealRequest) { _, id in
            // Back from a card's editor: the card picked, the view where it was.
            guard let id else { return }
            graph.reveal(id)
            model.revealRequest = nil
        }
        .onChange(of: model.focusRequest) { _, id in
            // An agent asked the window to look at something — or the nav panel, a card's
            // row pressed once, with the row to mark. Honour it and clear the request, so
            // the same call twice moves the view twice.
            guard let id else { return }
            go(to: id, field: model.focusRow)
            model.focusRequest = nil
            model.focusRow = nil
        }
    }

    /// Go to a card: at once, or once the canvas has drawn the section it is in.
    private func go(to id: String, field: String? = nil) {
        if graph.shownSection != model.shownSectionKey {
            // In the section just switched to: the canvas has not drawn it yet.
            graph.focusAfterNextShow(id, field: field)
        } else {
            graph.focus(id, field: field)
        }
    }

    private func status(_ message: String) -> some View {
        VStack(spacing: 12) {
            ProgressView()
            Text(message).foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    /// Before there is a bar: still somewhere to take hold of the window, under its buttons.
    private var emptyTopRow: some View {
        TopRow(room: chrome.room(.home)) { Spacer(minLength: 0) }
    }

    // MARK: - Screens

    private func graphScreen(_ detail: ProjectDetail, window: CGFloat) -> some View {
        // Every section is drawn as its cards and lines, whatever it holds; a card
        // opened is its card editor, and a root its dashboard (`specboard://SpecBoard/Docs/Design:Overview` §9, §13).
        VStack(spacing: 0) {
            // What is open, a tab each, the section's map first (`TabBar`).
            TabBar(model: model)
            Divider()
            GeometryReader { geo in
                HStack(spacing: 0) {
                    if case .root(let root, let tab) = model.here {
                        RootView(model: model, root: root, tab: tab)
                            .frame(maxWidth: .infinity)
                    } else if let card = model.openCard {
                        cardEditor(card, detail)
                            .id(card.id)
                            .frame(maxWidth: .infinity)
                    } else if case .settings(let tab) = model.here {
                        // The project's settings: the project, its rules for each place, where it is written out.
                        ProjectSettingsView(model: model, tab: tab)
                            .frame(maxWidth: .infinity, maxHeight: .infinity)
                    } else if model.here == .inspection {
                        // What is wrong in a tree, any tree of the project's.
                        InspectionView(model: model)
                            .frame(maxWidth: .infinity, maxHeight: .infinity)
                    } else if case .connections(_, let depth) = model.here, let card = model.connectionsCard {
                        // One card's connections, across the sections.
                        ConnectionsView(model: model, card: card, depth: depth,
                                        detail: model.withOutside(Self.editorDetail(model, shown: detail)),
                                        controller: connections, relationPanel: graph)
                            .id(card.id)
                            .frame(maxWidth: .infinity)
                    } else {
                        // The section's cards, in the frame every diagram has (`DiagramFrame`).
                        DiagramFrame(model: model, definition: DiagramDefinition(
                            detail: detail,
                            surroundings: model.surroundings,
                            key: model.shownSectionKey,
                            controller: graph,
                            dataGeneration: model.dataGeneration,
                            presses: .relationPanel(model, graph),
                            onMove: { model.moveByHand($0, to: $1) },
                            keepsPlaces: detail.directory.section(id: model.shownSectionKey)?.kind?.keepsPlaces == true,
                            drawsParts: detail.directory.section(id: model.shownSectionKey)?.kind == .ui,
                            onResize: { model.resizeByHand($0, to: $1) },
                            onFormPress: { model.pressForm($0, $1) },
                            onPlaceOutside: { [section = model.shownSectionKey] placed in model.keepPlaced(placed, on: section) },
                            onClickEmpty: { model.deselect() },
                            marks: model.shownMarks,
                            takesMenu: false,
                            mermaid: { [section = model.shownSectionKey, path = detail.directory.full(section: model.shownSectionKey)] kinds in
                                section.isEmpty ? nil : model.diagramMermaid("section_read", target: path, kinds: kinds)
                            },
                            caption: WorkDirCaption.of(model.shownSectionKey, model),
                            bandTools: AnyView(mapBandTools),
                            bandEnd: AnyView(filterButton(detail))))
                            .frame(maxWidth: .infinity)
                    }

                    propertiesColumn(window: window)
                }
            }
        }
    }

    /// Whether the property panel goes with what is open: a section's cards or a card,
    /// not a root's dashboard, the project's settings nor its inspection.
    private var propertiesFit: Bool {
        switch model.here {
        case .section, .card, .connections: true
        case .root, .settings, .inspection: false
        }
    }

    private var propertiesSeat: PaneSplit.PropertiesSeat {
        propertiesFit ? (propertiesOpen ? .open : .folded) : .absent
    }

    /// The two panels' widths in a window `window` wide (`PaneSplit.widths`).
    private func panelWidths(in window: CGFloat) -> PaneSplit.Widths {
        PaneSplit.widths(window: window, nav: navPanelWidth, navShown: navPanelVisible,
                         properties: propertiesWidth, seat: propertiesSeat)
    }

    /// The property panel, when it is open: beside what is open, its edge dragged.
    @ViewBuilder
    private func propertiesColumn(window: CGFloat) -> some View {
        if propertiesFit {
            if propertiesOpen {
                // Its edge: pressed it shuts, dragged it sets the width.
                PanelStrip(arrow: .right, help: L10n[.Root.propertiesCloseHelp],
                           onToggle: { propertiesOpen = false },
                           onDrag: { moved in
                               let shown = panelWidths(in: window)
                               let start = propertiesDragStart ?? shown.properties
                               propertiesDragStart = start
                               let most = max(PaneSplit.propertiesRange.lowerBound,
                                              PaneSplit.room(window: window, navShown: navPanelVisible, properties: .open)
                                                  - (navPanelVisible ? shown.nav : 0))
                               propertiesWidth = Double(min(PaneSplit.propertiesWidth(Double(start - moved)), most))
                               // What the nav panel is seen to be, so it does not shrink further as this one is dragged.
                               if navPanelVisible { navPanelWidth = Double(shown.nav) }
                           },
                           onDragEnd: { propertiesDragStart = nil })
                // What the open branch removed, picked, is shown as it was: never changed. Nor is what a
                // working directory's folder was read as: a file, or a folder open with nothing picked.
                let fromFolder = model.isReadFromFolder(model.selectedSchemaId ?? model.shownSectionKey)
                PropertyPanel(model: model, editable: !model.isReadOnly && model.goneShown == nil && !fromFolder)
                    .frame(width: panelWidths(in: window).properties)
            } else {
                PanelStrip(arrow: .left, help: L10n[.Root.propertiesOpenHelp],
                           onToggle: { propertiesOpen = true })
            }
        }
    }

    /// What a card editor draws from: the whole project, whatever section is shown. A section's
    /// slice (`AppModel.sectionDetail`) keeps the lines between its cards, a card inside a
    /// container counted as its container — so it has none between a flow's nodes or a goal's
    /// cards, which a card map draws.
    static func editorDetail(_ model: AppModel, shown: ProjectDetail) -> ProjectDetail {
        model.detail ?? shown
    }

    /// A card opened: the card editor its kind has (§13).
    @ViewBuilder
    private func cardEditor(_ card: SchemaNode, _ shown: ProjectDetail) -> some View {
        let detail = Self.editorDetail(model, shown: shown)
        switch card.kind {
        case .flow, .action:
            FlowEditor(model: model, flow: card, detail: detail, editable: !model.isReadOnly)
        case .note, .freedoc, .task:
            NoteEditor(model: model, note: card, editable: !model.isReadOnly) { model.open(path: $0, from: $1) }
        case .formatdoc:
            FormatDocEditor(model: model, doc: card, editable: !model.isReadOnly)
        case .stackdoc:
            StackDocEditor(model: model, doc: card, editable: !model.isReadOnly) { model.open(path: $0, from: $1) }
        case .instance:
            InstanceEditor(model: model, card: card, editable: !model.isReadOnly)
        case .table:
            TableEditor(model: model, card: card, editable: !model.isReadOnly)
        case .svg, .html:
            PageCardEditor(model: model, card: card, editable: !model.isReadOnly)
        case .goal:
            // A section card, as a flow is: its card map (the cards it holds are read from the
            // whole project, wherever their section is).
            GoalEditor(model: model, goal: card, detail: detail, editable: !model.isReadOnly)
        case .custom where card.ownKind == WorkDirIds.fileKind:
            // A working directory's file: what it holds, read only.
            FileView(model: model, card: card)
        case .custom:
            // A kind the project defines: by its definition (`opens`) — a container on its card map, its
            // body as a note's is written (`markdown`; its other fields in the property panel, as a
            // task's), its fields and rows otherwise.
            if card.kindDef?.opensAsNote == true {
                NoteEditor(model: model, note: card, editable: !model.isReadOnly) { model.open(path: $0, from: $1) }
            } else {
                OwnCardEditor(model: model, card: card, detail: detail, editable: !model.isReadOnly)
            }
        default:
            CardEditor(model: model, schema: card, editable: !model.isReadOnly) { id in
                model.open(.card(id))
            }
        }
    }

    /// The menu bar's: zoom and fit on the card map shown, else on the relation panel; 自動整列
    /// on the relation panel alone (a card map's cards are put by hand).
    private var diagramActions: DiagramActions {
        let canvas = { model.mapCanvas ?? graph }
        return DiagramActions(
            toggleNavPanel: { navPanelVisible.toggle() },
            toggleProperties: propertiesFit ? { propertiesOpen.toggle() } : nil,
            zoomIn: { canvas().zoomIn() },
            zoomOut: { canvas().zoomOut() },
            actualSize: { canvas().actualSize() },
            fit: { canvas().fit() },
            autoLayout: { if model.mapCanvas == nil { model.autoLayout(filters: graph.filters) } },
            closesTabs: settingsPage == nil,
            closeTab: model.activeTab == nil ? nil : { model.closeTab() })
    }

    /// Which schemas have a card right now. The list uses it to mark the rows that
    /// the filters have taken off the canvas, so clicking one is not a silent no-op.
    private var drawnSchemaIds: Set<String> {
        guard let detail = model.sectionDetail else { return [] }
        return Set(visibleSchemas(detail, graph.filters).map(\.id))
    }

    // MARK: - The map tab's band

    /// What only the map tab's band has before mermaid (`DiagramDefinition.bandTools`): the cards put
    /// there for a look, and a ui section's switches for what its map lays over its parts.
    @ViewBuilder
    private var mapBandTools: some View {
        temporaryChip
        if model.shownUiSection != nil {
            tagsToggle
            notesToggle
        }
    }

    /// 「見出し」: the parts' tags shown over the map or hidden — kept for the window.
    private var tagsToggle: some View {
        Toggle(isOn: Binding(get: { model.partTags }, set: { model.partTags = $0 })) {
            Label(L10n[.Root.uiTagsTitle], systemImage: "tag").font(.app(10.5))
        }
        .toggleStyle(.button)
        .controlSize(.small)
        .fixedSize()
        .help(L10n[.Root.uiTagsHelp])
    }

    /// 「ノート」: the parts' notes shown over the map or hidden — kept for the window; the review dots stay.
    private var notesToggle: some View {
        Toggle(isOn: Binding(get: { model.partNotes }, set: { model.partNotes = $0 })) {
            Label(L10n[.Root.uiNotesTitle], systemImage: "note.text").font(.app(10.5))
        }
        .toggleStyle(.button)
        .controlSize(.small)
        .fixedSize()
        .help(L10n[.Root.uiNotesHelp])
    }

    /// The cards put there for a look, and a way to let them all go at once.
    @ViewBuilder
    private var temporaryChip: some View {
        if !model.surroundings.temporary.isEmpty {
            Button { model.clearForNow() } label: {
                HStack(spacing: 4) {
                    Text(L10n[.Root.temporaryCount(model.surroundings.temporary.count)]).font(.app(10.5))
                    Image(systemName: "xmark").font(.app(8.5, weight: .semibold))
                }
                .padding(.horizontal, 7).padding(.vertical, 2)
                .overlay(RoundedRectangle(cornerRadius: 5)
                    .strokeBorder(Color(nsColor: theme.brass), style: StrokeStyle(lineWidth: 1, dash: [4, 3])))
            }
            .buttonStyle(.plain)
            .fixedSize()
            .help(L10n[.Root.temporaryClearHelp])
        }
    }

    /// The relation panel's filters and legend, a click away. While one is on, the
    /// button says so, where missing cards would otherwise be a mystery.
    private func filterButton(_ detail: ProjectDetail) -> some View {
        let on = graph.isFiltered
        return Button { filtersOpen.toggle() } label: {
            Label(on ? L10n[.Root.filterActive] : L10n[.Root.filterTitle],
                  systemImage: on ? "line.3.horizontal.decrease.circle.fill" : "line.3.horizontal.decrease.circle")
                .font(.app(10.5)).lineLimit(1)
                .foregroundStyle(on ? Color(nsColor: theme.brassInk) : .primary)
        }
        .buttonStyle(.borderless)
        .fixedSize()
        .help(on ? L10n[.Root.filterActiveHelp] : L10n[.Root.filterHelp])
        .popover(isPresented: $filtersOpen, arrowEdge: .bottom) {
            viewPane(detail)
                .frame(width: 340)
                .frame(minHeight: 200, idealHeight: 440, maxHeight: 600)
        }
    }

    /// The filter window: how the lines go, and what is filtered out, top to bottom. Every row is also the switch for what it names. Not tags: the diagram
    /// is not cut by tag (the root's タグ view gathers cards by tag).
    private func viewPane(_ detail: ProjectDetail) -> some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                // How the lines go: this Mac's, the same switch as the settings' 「図」.
                section(L10n[.Settings.lineRoutingTitle]) { LineRoutingSetting() }

                // Only in a divided project, and only for the switches that would change
                // something: a section with nothing outside has nothing to switch.
                if model.shownSection != nil,
                   !model.surroundings.references.isEmpty || !model.surroundings.consumers.isEmpty {
                    section(L10n[.Root.filterOtherSections]) {
                        VStack(alignment: .leading, spacing: 8) {
                            outsideSwitch(L10n[.Root.filterOutsideReferences], on: graph.filters.outsideReferences,
                                          count: model.surroundings.references.count,
                                          help: L10n[.Root.filterOutsideReferencesHelp]) {
                                graph.toggleOutsideReferences()
                            }
                            outsideSwitch(L10n[.Root.filterOutsideConsumers], on: graph.filters.outsideConsumers,
                                          count: model.surroundings.consumers.count,
                                          help: L10n[.Root.filterOutsideConsumersHelp]) {
                                graph.toggleOutsideConsumers()
                            }
                        }
                    }
                }

                section(L10n[.Root.filterKind]) {
                    FlowRow(spacing: 6, lineSpacing: 6) {
                        // The project's own kinds together, when it has any.
                        ForEach(SchemaKind.made + (model.detail?.cardKinds.own.isEmpty ?? true ? [] : [.custom]), id: \.self) { kind in
                            chip(kind == .custom ? L10n[.Root.filterProjectKinds] : kind.rawValue, on: graph.filters.kinds.contains(kind)) {
                                graph.toggle(kind: kind)
                            }
                        }
                    }
                }
            }
            .padding(14)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private func section<Content: View>(
        _ title: String, @ViewBuilder content: () -> Content
    ) -> some View {
        VStack(alignment: .leading, spacing: 7) {
            Text(title)
                .font(.app(10.5, weight: .semibold))
                .tracking(1.2)
                .foregroundStyle(.secondary)
            content()
        }
    }

    private func chip(_ label: String, on: Bool, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Text(label)
                .font(.app(11.5))
                .padding(.horizontal, 9)
                .padding(.vertical, 4)
                .background(
                    RoundedRectangle(cornerRadius: 5)
                        .fill(on ? Color(nsColor: theme.brassSoft) : Color.clear))
                .overlay(
                    RoundedRectangle(cornerRadius: 5)
                        .stroke(Color(nsColor: on ? theme.brassLine : theme.line2),
                                lineWidth: 1))
                .foregroundStyle(on ? Color(nsColor: theme.brassInk) : .primary)
        }
        .buttonStyle(.plain)
    }

    /// Another section's cards, shown faintly or not: the switch and how many.
    private func outsideSwitch(_ title: String, on: Bool, count: Int, help: String,
                               action: @escaping () -> Void) -> some View {
        Button(action: action) {
            HStack(spacing: 5) {
                RoundedRectangle(cornerRadius: 3)
                    .strokeBorder(style: StrokeStyle(lineWidth: 1, dash: [3, 2]))
                    .frame(width: 14, height: 10)
                Text("\(title) \(count)").font(.app(11.5)).lineLimit(1)
            }
            .fixedSize()
            .opacity(on ? 1 : 0.32)
        }
        .buttonStyle(.plain)
        .help(help)
    }

    // MARK: - Prompts

    /// An archived project is read, never written (`specboard://SpecBoard/Docs/Design:Overview` §10): said once,
    /// above everything, with the way back.
    private func archivedBanner(_ projectId: String) -> some View {
        HStack(spacing: 8) {
            Image(systemName: "archivebox")
            Text(L10n[.Root.archivedBanner])
                .font(.app(11.5)).fixedSize(horizontal: false, vertical: true)
            Spacer()
            Button(L10n[.Root.archivedRestore]) { model.restoreProject(projectId) }.font(.app(11))
        }
        .padding(.horizontal, 14).padding(.vertical, 7)
        .background(Color.secondary.opacity(0.12))
    }

    /// The kinds a new card can be here: what the section on screen holds — any, in
    /// the free section — the project's own kinds too (`AppModel.offeredKinds`).
    private var offeredKinds: [String] { model.offeredKinds }

    /// A new card in the section on screen: its name, the name people read, its kind.
    private func newSchemaPrompt(_ request: AppModel.NewSchemaRequest) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(L10n[.Root.newCardTitle]).font(.app(13, weight: .semibold))
            if let section = model.shownSection, let detail = model.detail, detail.sections.count > 1 {
                Text(L10n[.Root.newCardInSection(detail.directory.shownFromTop(section: section.id))])
                    .font(.app(11)).foregroundStyle(.secondary)
            }
            TextField(L10n[.Root.newCardNamePlaceholder], text: $draftSchemaName)
                .textFieldStyle(.roundedBorder)
                .font(.app(12, design: .monospaced))
                .frame(width: 300)
            TextField(L10n[.Root.newCardLabelPlaceholder], text: $draftSchemaLabel)
                .textFieldStyle(.roundedBorder)
                .frame(width: 300)
            // The project's own kinds beside the built-in ones: then a menu, as they may be many. A
            // data card or a table is made by choosing its shape too.
            let offered = offeredKinds
            let picker = Picker("", selection: $draftSchemaKind) {
                ForEach(offered, id: \.self) { name in
                    Text(model.detail?.cardKinds[name].flatMap { $0.builtin ? nil : $0.badge } ?? name).tag(name)
                }
            }
            .labelsHidden()
            if offered.contains(where: { SchemaKind(rawValue: $0) == nil }) {
                picker.pickerStyle(.menu).fixedSize()
            } else {
                picker.pickerStyle(.segmented)
            }
            if AppModel.asksForShape(draftSchemaKind) {
                shapePicker()
            }
            if draftSchemaKind == SchemaKind.frame.rawValue {
                HStack(spacing: 6) {
                    Text(L10n[.Root.frameTargetTitle]).font(.app(11)).foregroundStyle(.secondary)
                    FrameTargetPicker(model: model, at: model.shownSectionKey, chosen: draftFrameTarget) { draftFrameTarget = $0 }
                }
            }
            HStack {
                Spacer()
                Button(L10n[.Common.actionCancelDialog]) { model.newSchemaRequest = nil }
                    .keyboardShortcut(.cancelAction)
                Button(L10n[.Common.actionAddItem]) {
                    if draftSchemaKind == SchemaKind.instance.rawValue {
                        model.createInstance(name: draftSchemaName, label: draftSchemaLabel, shape: draftShape)
                    } else if draftSchemaKind == SchemaKind.table.rawValue {
                        model.createTable(name: draftSchemaName, label: draftSchemaLabel, shape: draftShape)
                    } else if draftSchemaKind == SchemaKind.frame.rawValue {
                        model.createFrame(name: draftSchemaName, label: draftSchemaLabel, target: draftFrameTarget)
                    } else {
                        model.createSchema(name: draftSchemaName, label: draftSchemaLabel, kindName: draftSchemaKind)
                    }
                    if model.lastError == nil { model.newSchemaRequest = nil }
                }
                .keyboardShortcut(.defaultAction)
                .disabled(PathText.nameProblem(PathText.trimmed(draftSchemaName)) != nil
                          || (AppModel.asksForShape(draftSchemaKind) && draftShape.isEmpty)
                          || (draftSchemaKind == SchemaKind.frame.rawValue && draftFrameTarget.isEmpty))
            }
        }
        .padding(18)
        .onAppear {
            draftSchemaName = ""
            draftSchemaLabel = ""
            let offered = offeredKinds
            draftShape = model.instanceShapes.first?.id ?? ""
            draftFrameTarget = model.frameTargets(at: model.shownSectionKey).first?.id ?? ""
            draftSchemaKind = request.kind.map(\.rawValue).flatMap { offered.contains($0) ? $0 : nil }
                ?? offered.first ?? SchemaKind.entity.rawValue
        }
    }

    /// The project's entities and values to make a data card of, or a table of their data, by their
    /// sections (none of the enums, stores, flows or documents).
    private func shapePicker() -> some View {
        let shapes = model.instanceShapes
        let sections = shapes.map(\.section).reduce(into: [String]()) { if !$0.contains($1) { $0.append($1) } }
        return HStack(spacing: 6) {
            Text(L10n[.Root.shapeTitle]).font(.app(11)).foregroundStyle(.secondary)
            Picker("", selection: $draftShape) {
                ForEach(sections, id: \.self) { section in
                    Section(section) {
                        ForEach(shapes.filter { $0.section == section }, id: \.id) { shape in
                            Text(shape.path).tag(shape.id)
                        }
                    }
                }
            }
            .labelsHidden().pickerStyle(.menu).fixedSize()
            .help(L10n[.Root.shapeHelp])
            if shapes.isEmpty {
                Text(L10n[.Root.shapeNone]).font(.app(11)).foregroundStyle(.secondary)
            }
        }
    }

    // MARK: - Settings

    /// The settings, open over the whole window at this item; nil when shut. Opened from
    /// the top bar's 設定 and MCP (`TopBar`).
    @State private var settingsPage: SettingsPage?
}
