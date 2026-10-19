import AppKit
import Foundation

/// A map drawn as a picture for MCP's `as: picture` (`specboard://SpecBoard/Docs/Design:Overview`;
/// the flow is `Behavior/Screen:DrawPicture`): a section map, a card's connections map, a flow's
/// diagram or a container's content map, drawn by the same canvas the screen draws them on —
/// made for the picture and put in no window — and given back as a PNG.
///
/// What it is drawn from is read from the core the request came through, not from a window: the
/// person's window is not moved, and what it looks like there (the selection, the zoom, the lines
/// switched off) is not in the picture. A picture is always in the light appearance, with the lines
/// the kind definitions draw (or the kinds asked for).
///
/// The request is the core's (`specboard_core::App::picture`), already checked there; the answer
/// is `{"image": {data, mimeType}, "note": {…}}`, or `{"error": why}` in the request's language.
///
/// A ui section's mock is no map: its page is drawn as it is (`MockPicture`). Its map (`view: map`) is
/// drawn as the screen draws it: the parts as their faces, their tags and notes over them.
enum MapPicture {

    /// What is asked: which map, of what, framed how, how big.
    nonisolated struct Request: @unchecked Sendable {
        enum Map: String {
            case sectionMap, connectionsMap, flowDiagram, contentMap
        }

        var map: Map
        var project: String
        /// As `project_detail`'s `branch` takes it: `""` for main.
        var tree: String
        var section: String?
        var card: String?
        var depth = 1
        /// The kinds of line to draw; nil for what the kind definitions draw.
        var kinds: [String]?
        /// The cards to frame, by id.
        var cards: [String]?
        /// The grid range to frame: its top left and bottom right corners.
        var area: (topLeft: GridPoint, bottomRight: GridPoint)?
        /// Grid units around `cards`, `area` or the whole map.
        var margin = 2
        var maxSize = WindowCapture.defaultMaxSize
        var lang = AppControl.mcpLanguage

        init?(_ text: String) {
            guard let asked = (try? JSONSerialization.jsonObject(with: Data(text.utf8))) as? [String: Any],
                  let map = (asked["picture"] as? String).flatMap(Map.init(rawValue:)),
                  let project = asked["project"] as? String else { return nil }
            self.map = map
            self.project = project
            tree = asked["tree"] as? String ?? ""
            section = asked["section"] as? String
            card = asked["card"] as? String
            depth = asked["depth"] as? Int ?? depth
            kinds = asked["kinds"] as? [String]
            cards = asked["cards"] as? [String]
            if let area = asked["area"] as? [String: Any],
               let from = Self.point(area["topLeft"]), let to = Self.point(area["bottomRight"]) {
                self.area = (from, to)
            }
            margin = asked["margin"] as? Int ?? margin
            maxSize = asked["maxSize"] as? Int ?? maxSize
            lang = asked["lang"] as? String ?? lang
        }

        private static func point(_ value: Any?) -> GridPoint? {
            guard let point = value as? [String: Any], let x = point["x"] as? Int, let y = point["y"] as? Int else {
                return nil
            }
            return GridPoint(x: x, y: y)
        }

        /// Whether the map is a section's or a container's, whose part drawn is said in grid units.
        var takesRange: Bool { map == .sectionMap || map == .contentMap }
    }

    /// What a picture is drawn from, read off the main thread.
    nonisolated struct Read: @unchecked Sendable {
        var detail: ProjectDetail
        /// The tree beside it that its lines reach, as the window reads it beside the tree open: main
        /// beside the board.
        var outside: ProjectDetail?
        var layout: LayoutSettings
        var positions: [String: [String: CGPoint]] = [:]
        /// What a branch changed, made or saw main change, marked as the window marks it.
        var marks = BranchMarks()
        /// The tree as MCP names it, for reading a ui section's faces; nil when it has no such name.
        var faceTree: String? = ""
    }

    /// Why no picture is drawn, in the request's language.
    struct Refused: Error {
        var why: String
    }

    /// How long a picture may take in all: the agent's daemon waits 30 seconds for it.
    nonisolated static let budget: Duration = .seconds(20)
    /// How long document pages are waited for.
    nonisolated static let documentsPatience: Duration = .seconds(10)

    // MARK: - The answer

    /// The answer to `request` (the core's JSON), read through `core` — the core it came through.
    /// From any thread: off main, read there and drawn on main; on main itself (main cannot wait for
    /// main), drawn at once, with the document pages not yet drawn counted as pending.
    nonisolated static func answer(request text: String, core: CoreClient?) -> String {
        // A ui section's mock: the page the core built, drawn as it is (`MockPicture`).
        if let asked = (try? JSONSerialization.jsonObject(with: Data(text.utf8))) as? [String: Any],
           asked["picture"] as? String == "mock" {
            return MockPicture.answer(asked)
        }
        guard let request = Request(text) else {
            return refusal(L10n.text(.AppControl.pictureUnreadableRequest, in: AppControl.mcpLanguage))
        }
        guard let core else { return refusal(L10n.text(.AppControl.pictureNoCore, in: request.lang)) }
        let clock = ContinuousClock()
        let started = clock.now
        if Thread.isMainThread {
            return MainActor.assumeIsolated {
                said(Result {
                    let read = try Self.read(request, core)
                    return try drawnNow(request, read, core)
                })
            }
        }
        // While the app is in the background, App Nap would hold back the pages WebKit draws.
        let activity = ProcessInfo.processInfo.beginActivity(options: .userInitiated, reason: "Drawing a map for MCP")
        defer { ProcessInfo.processInfo.endActivity(activity) }
        let read: Read
        do {
            read = try Self.read(request, core)
        } catch {
            return refusal((error as? Refused)?.why ?? error.localizedDescription)
        }
        final class Box: @unchecked Sendable {
            var said = ""
        }
        let box = Box()
        let done = DispatchSemaphore(value: 0)
        let deadline = started + budget
        Task { @MainActor in
            let documents = min(clock.now + documentsPatience, deadline - .seconds(2))
            do {
                box.said = said(.success(try await drawn(request, read, core, documentsUntil: documents)))
            } catch {
                box.said = said(.failure(error))
            }
            done.signal()
        }
        let left = deadline - clock.now
        let seconds = Double(left.components.seconds) + Double(left.components.attoseconds) / 1e18
        // Not read once given up on: what main says after that is said to nobody.
        guard done.wait(timeout: .now() + max(0, seconds)) == .success else {
            return refusal(L10n.text(.AppControl.pictureLate, in: request.lang))
        }
        return box.said
    }

    nonisolated private static func refusal(_ why: String) -> String {
        json(["error": why])
    }

    nonisolated private static func json(_ object: [String: Any]) -> String {
        let data = (try? JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])) ?? Data()
        return String(decoding: data, as: UTF8.self)
    }

    /// A picture drawn, as the core reads it; or why not.
    nonisolated private static func said(_ outcome: Result<Drawn, Error>) -> String {
        switch outcome {
        case .success(let drawn):
            return json(["image": ["data": drawn.png.base64EncodedString(), "mimeType": "image/png"],
                         "note": drawn.note])
        case .failure(let error):
            return refusal((error as? Refused)?.why ?? error.localizedDescription)
        }
    }

    // MARK: - Reading

    /// What the map is drawn from, read from `core` in the tree asked for — and the tree beside it
    /// that its lines reach, as the window reads it. On any thread.
    nonisolated static func read(_ request: Request, _ core: CoreClient) throws -> Read {
        let project = request.project
        let tree: String? = request.tree.isEmpty ? nil : request.tree
        do {
            let detail = try core.readDetail(project, branch: tree).detail
            var read = Read(detail: detail, layout: try core.layout(project))
            guard request.map == .sectionMap || request.map == .connectionsMap else { return read }
            if tree == Board.id {
                read.outside = try? core.readDetail(project, branch: nil).detail
            }
            if request.map == .sectionMap {
                read.positions = try core.sectionPositions(project, branch: tree)
                read.faceTree = faceTree(tree, project, core)
            }
            // Marked where a branch is drawn: not main nor the board.
            if let tree, tree != Board.id {
                read.marks = (try? core.branchMarks(project, branch: tree, kinds: detail.cardKinds)) ?? BranchMarks()
            }
            return read
        } catch {
            throw Refused(why: L10n.text(.AppControl.pictureUnreadable(error.localizedDescription), in: request.lang))
        }
    }

    /// The tree `tree` (as `project_detail` takes it) as MCP's `branch` names it: none for main, the board's
    /// word, a branch's name; nil for a tree no tool names.
    nonisolated private static func faceTree(_ tree: String?, _ project: String, _ core: CoreClient) -> String? {
        guard let tree else { return "" }
        if tree == Board.id { return Board.word }
        return (try? core.branches(project))?.first { $0.id == tree }?.name
    }

    // MARK: - Drawing

    /// A picture taken: the PNG and what it is of.
    struct Drawn: @unchecked Sendable {
        var png: Data
        var note: [String: Any]
    }

    /// What is drawn, ready to be taken: a canvas framed on what is asked, or a sequence diagram.
    private enum Made {
        case canvas(CardCanvasView, Frame, texts: DocumentTexts, faces: PartFaces)
        case sequence(SequenceView, scale: CGFloat, cards: Int)
    }

    /// The part of a canvas drawn, in diagram points, and how big.
    private struct Frame {
        var rect: CGRect
        /// The picture's size against the screen's 100% (at most 1).
        var scale: CGFloat
        /// The grid range drawn, said for a section's or a container's map.
        var area: GridRect?
    }

    /// Drawn without waiting for document pages: on main itself.
    @MainActor
    private static func drawnNow(_ request: Request, _ read: Read, _ core: CoreClient) throws -> Drawn {
        let made = try make(request, read, core)
        defer { letGo(made) }
        return try shoot(request, made)
    }

    /// Drawn once the document pages on it are drawn, or `documentsUntil` has come.
    @MainActor
    private static func drawn(_ request: Request, _ read: Read, _ core: CoreClient,
                              documentsUntil: ContinuousClock.Instant) async throws -> Drawn {
        let made = try make(request, read, core)
        defer { letGo(made) }
        if case .canvas(let canvas, let frame, let texts, let faces) = made {
            let clock = ContinuousClock()
            while canvas.documentsPending(in: frame.rect) > 0 || texts.busy || faces.busy, clock.now < documentsUntil {
                try? await Task.sleep(for: .milliseconds(50))
            }
        }
        return try shoot(request, made)
    }

    /// The pages still asked for this picture's canvas are not drawn for it once it is taken.
    @MainActor
    private static func letGo(_ made: Made) {
        if case .canvas(let canvas, _, _, _) = made {
            DocSnapshots.shared.withdraw(owner: canvas, keeping: [])
        }
    }

    /// The view drawn for the picture, framed on what is asked: what the window would draw for the
    /// same map, in no window.
    @MainActor
    private static func make(_ request: Request, _ read: Read, _ core: CoreClient) throws -> Made {
        var made: Made?
        var failure: Error?
        NSAppearance(named: .aqua)?.performAsCurrentDrawingAppearance {
            do { made = try making(request, read, core) } catch { failure = error }
        }
        if let failure { throw failure }
        guard let made else { throw refused(.AppControl.pictureNotDrawn, request) }
        return made
    }

    @MainActor
    private static func refused(_ why: LocalizedStringResource, _ request: Request) -> Refused {
        Refused(why: L10n.text(why, in: request.lang))
    }

    /// What one map draws, as the screen gives it to the canvas.
    private struct Content {
        var detail: ProjectDetail
        var surroundings = Surroundings()
        var marks = BranchMarks()
        var curvesOnly = false
        /// The ids the cards to frame have on it.
        var framed: [String]?
        /// A ui section's map: its parts drawn as their faces.
        var drawsParts = false
    }

    @MainActor
    private static func making(_ request: Request, _ read: Read, _ core: CoreClient) throws -> Made {
        let detail = read.detail
        let content: Content
        switch request.map {
        case .sectionMap:
            guard let key = request.section,
                  detail.sections.contains(where: { $0.id == key }) || (detail.sections.isEmpty && key == detail.directory.home)
            else { throw refused(.AppControl.pictureSectionGone, request) }
            let surroundings = AppModel.surroundings(of: key, in: detail, positions: read.positions[key] ?? [:],
                                                     outside: read.outside, gone: read.marks.gone.cards, forNow: [])
            content = Content(detail: detail.only(section: key), surroundings: surroundings, marks: read.marks,
                              framed: request.cards, drawsParts: detail.directory.section(id: key)?.kind == .ui)
        case .connectionsMap:
            let whole = read.outside.map { detail.joined(with: $0) } ?? detail
            let filters = filters(request, read)
            guard let card = request.card,
                  let map = ConnectionsMap.make(card, depth: ConnectionDepth(rawValue: request.depth) ?? .one, in: whole,
                                                shows: { filters.shows($0, kinds: whole.lineKinds) },
                                                settings: read.layout)
            else { throw refused(.AppControl.pictureCardGone, request) }
            content = Content(detail: map.detail, surroundings: map.surroundings, marks: read.marks, curvesOnly: true)
        case .flowDiagram:
            guard let flow = detail.schemas.first(where: { $0.id == request.card }) else {
                throw refused(.AppControl.pictureCardGone, request)
            }
            guard !flow.nodes.isEmpty else { throw refused(.AppControl.pictureNothingDrawn, request) }
            if flow.diagramShape == "sequence" {
                return try sequence(flow, in: detail, request)
            }
            content = Content(detail: FlowCardMap.detail(flow, in: detail))
        case .contentMap:
            guard let card = detail.schemas.first(where: { $0.id == request.card }) else {
                throw refused(.AppControl.pictureCardGone, request)
            }
            content = Content(detail: GoalCardMap.detail(card, in: detail), framed: request.cards?.map(GoalCardMap.id))
        }
        return try canvas(content, request, read, core)
    }

    /// The lines drawn: the kinds asked for and no other — of every kind the trees read have —, or
    /// what the kind definitions draw.
    @MainActor
    private static func filters(_ request: Request, _ read: Read) -> GraphFilters {
        var filters = GraphFilters()
        guard let kinds = request.kinds else { return filters }
        var every = Set<String>()
        for detail in [read.detail] + (read.outside.map { [$0] } ?? []) {
            every.formUnion(detail.lineKinds.names)
            every.formUnion(detail.relations.map(\.line))
        }
        filters.hiddenLines = every.subtracting(kinds)
        filters.shownLines = Set(kinds)
        return filters
    }

    @MainActor
    private static func canvas(_ content: Content, _ request: Request, _ read: Read, _ core: CoreClient) throws -> Made {
        let canvas = CardCanvasView(frame: .zero)
        canvas.appearance = NSAppearance(named: .aqua)
        // Its layers are coloured as it is made, in the app's appearance; coloured again in the light.
        canvas.viewDidChangeEffectiveAppearance()
        canvas.showsOffscreenHints = false
        canvas.curvesOnly = content.curvesOnly
        // A document card's page, read in the tree asked for — or the one beside it, for a card of
        // that tree's drawn on a connections map.
        let texts = DocumentTexts()
        let here = Set(read.detail.schemas.map(\.id))
        let besides = Set(read.outside?.schemas.map(\.id) ?? [])
        canvas.documentText = { [read, tree = request.tree] shown, ready in
            let id = GoalCardMap.isCard(shown.id) ? GoalCardMap.card(of: shown.id) : shown.id
            if !here.contains(id), besides.contains(id), let outside = read.outside {
                return texts.face(of: shown, in: outside, tree: "", core: core, ready: ready)
            }
            return texts.face(of: shown, in: read.detail, tree: tree, core: core, ready: ready)
        }
        // A ui part's face, read for its section in the tree asked for.
        let faces = PartFaces()
        canvas.drawsParts = content.drawsParts
        canvas.partFace = { [read] shown, ready in
            faces.face(of: shown, in: read.detail, tree: read.faceTree, core: core, ready: ready)
        }
        // Drawing sets the rows a card shows for the whole app: the window's are put back.
        let rows = CardGeometry.maxRows
        canvas.show(content.detail, filters: filters(request, read), surroundings: content.surroundings,
                    settings: read.layout, cardWidth: AppModel.cardWidth(of: read.detail, settings: read.layout),
                    marks: content.marks)
        CardGeometry.maxRows = rows

        let boxes = canvas.cardFrames
        guard !boxes.isEmpty else { throw refused(.AppControl.pictureNothingDrawn, request) }
        let unit = canvas.gridUnit
        let margin = CGFloat(request.margin)
        var rect: CGRect
        if let framed = content.framed, !framed.isEmpty {
            rect = framed.compactMap { boxes[$0] }.reduce(CGRect.null) { $0.union($1) }
            guard !rect.isNull else { throw refused(.AppControl.pictureCardsNotOnMap, request) }
        } else if let area = request.area {
            // The bottom right is the last cell taken in, as a box's is said.
            rect = CGRect(x: CGFloat(area.topLeft.x) * unit.width, y: CGFloat(area.topLeft.y) * unit.height,
                          width: CGFloat(area.bottomRight.x - area.topLeft.x + 1) * unit.width,
                          height: CGFloat(area.bottomRight.y - area.topLeft.y + 1) * unit.height)
        } else {
            rect = canvas.groupFrames.values.reduce(canvas.diagramExtent) { $0.union($1) }
        }
        rect = rect.insetBy(dx: -margin * unit.width, dy: -margin * unit.height)
        rect.size = CGSize(width: max(rect.width, unit.width), height: max(rect.height, unit.height))

        // Never past the screen's 100%; the longest side within maxSize.
        let scale = min(1, CGFloat(request.maxSize) / max(rect.width, rect.height))
        // Drawn no smaller than the canvas zooms out to, and made smaller once taken.
        let zoom = max(scale, canvas.zoomLimits.lowerBound)
        canvas.setFrameSize(CGSize(width: (rect.width * zoom).rounded(.up), height: (rect.height * zoom).rounded(.up)))
        canvas.show(area: rect, margin: 0, cap: zoom)

        var area: GridRect?
        if request.takesRange {
            // A hair of rounding is not a cell more.
            func down(_ value: CGFloat) -> Int { Int((value + 0.001).rounded(.down)) }
            func up(_ value: CGFloat) -> Int { Int((value - 0.001).rounded(.up)) }
            let x = down(rect.minX / unit.width), y = down(rect.minY / unit.height)
            area = GridRect(x: x, y: y, w: up(rect.maxX / unit.width) - x, h: up(rect.maxY / unit.height) - y)
        }
        return .canvas(canvas, Frame(rect: rect, scale: scale, area: area), texts: texts, faces: faces)
    }

    /// A flow kept as a sequence diagram, drawn as its editor draws it.
    @MainActor
    private static func sequence(_ flow: SchemaNode, in detail: ProjectDetail, _ request: Request) throws -> Made {
        let view = SequenceView(frame: .zero)
        view.appearance = NSAppearance(named: .aqua)
        let diagram = SequenceDiagram.make(flow, in: detail, language: Language.current)
        view.update(diagram: diagram, kinds: Dictionary(detail.schemas.map { ($0.id, $0.kind) }, uniquingKeysWith: { a, _ in a }),
                    lines: detail.lineKinds, selected: nil)
        let size = view.bounds.size
        guard size.width > 0, size.height > 0 else { throw refused(.AppControl.pictureNothingDrawn, request) }
        let scale = min(1, CGFloat(request.maxSize) / max(size.width, size.height))
        return .sequence(view, scale: scale, cards: diagram.participants.count)
    }

    // MARK: - Taking it

    @MainActor
    private static func shoot(_ request: Request, _ made: Made) throws -> Drawn {
        var note: [String: Any] = ["picture": request.map.rawValue]
        let image: CGImage
        let scale: CGFloat
        switch made {
        case .canvas(let canvas, let frame, _, _):
            guard let shot = canvas.snapshot() else { throw refused(.AppControl.pictureNotDrawn, request) }
            let width = max(1, Int((frame.rect.width * frame.scale).rounded()))
            let height = max(1, Int((frame.rect.height * frame.scale).rounded()))
            guard let sized = resized(shot, width: width, height: height) else {
                throw refused(.AppControl.pictureNotDrawn, request)
            }
            image = sized
            scale = frame.scale
            if let area = frame.area {
                note["area"] = ["topLeft": ["x": area.x, "y": area.y],
                                "bottomRight": ["x": area.bottomRight.x, "y": area.bottomRight.y]]
            }
            note["cards"] = canvas.cardFrames.values.filter { $0.intersects(frame.rect) }.count
            let pending = canvas.documentsPending(in: frame.rect)
            if pending > 0 { note["documentsPending"] = pending }
        case .sequence(let view, let sequenceScale, let cards):
            let size = view.bounds.size
            let width = max(1, Int((size.width * sequenceScale).rounded()))
            let height = max(1, Int((size.height * sequenceScale).rounded()))
            guard let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: width, pixelsHigh: height,
                                             bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
                                             colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0) else {
                throw refused(.AppControl.pictureNotDrawn, request)
            }
            rep.size = size
            NSAppearance(named: .aqua)?.performAsCurrentDrawingAppearance {
                view.cacheDisplay(in: view.bounds, to: rep)
            }
            guard let drawn = rep.cgImage else { throw refused(.AppControl.pictureNotDrawn, request) }
            image = drawn
            scale = sequenceScale
            note["cards"] = cards
        }
        let picture = try WindowCapture.encode(image)
        note["width"] = picture.width
        note["height"] = picture.height
        note["scale"] = (Double(scale) * 1000).rounded() / 1000
        return Drawn(png: picture.png, note: note)
    }

    /// `image` drawn at `width` × `height` pixels, smoothly.
    private static func resized(_ image: CGImage, width: Int, height: Int) -> CGImage? {
        guard let ctx = CGContext(data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0,
                                  space: CGColorSpace(name: CGColorSpace.sRGB)!,
                                  bitmapInfo: CGImageAlphaInfo.premultipliedFirst.rawValue) else { return nil }
        ctx.interpolationQuality = .high
        ctx.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
        return ctx.makeImage()
    }
}
