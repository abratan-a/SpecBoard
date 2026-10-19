import AppKit
import Foundation

/// `app_control`: the window read and moved, and pictures of it — the one tool the core hands
/// back to the app (`CoreApp`; `sources/core/src/tools/app.rs`).
///
/// The only tool that touches what the person is looking at. Every other tool reads and
/// writes the store and leaves the window alone — an agent editing a project in the background
/// must not pull the view somewhere else. One tool carries all of it, so that is legible
/// from the name alone: `card_update` changes data and `app_control` changes someone's
/// screen.
///
/// The core has checked the call by the time it is here: a key that may do what the op does
/// with the window (choose and change how it looks, or take a picture of it), the arguments
/// against the tool's schema, the value against its op's form. It is asked on
/// the thread the MCP request is answered on, and everything here touches the window, so it
/// runs on main — which never waits on that thread, so there is nothing to deadlock against.
nonisolated final class AppControl: CoreApp, @unchecked Sendable {

    private weak var model: AppModel?
    /// Whose calls: this Mac's MCP (nil), or the agent's on that host, handed back to this app
    /// for its window.
    private let place: UUID?
    /// The core whose calls these are: what a picture's map is read from (`MapPicture`), apart from
    /// any window.
    private weak var core: CoreClient?

    init(model: AppModel, place: UUID? = nil, core: CoreClient? = nil) {
        self.model = model
        self.place = place
        self.core = core
    }

    /// Whether this app takes `app_control` from the agents it is connected to (the settings'
    /// 「MCP」; on unless switched off). When off, it says no window of theirs (`windowsFor`).
    static let fromAgentsKey = "agent.appControl"
    static var takesFromAgents: Bool {
        UserDefaults.standard.object(forKey: fromAgentsKey) as? Bool ?? true
    }

    /// This window as it is named: the Mac's name — as this app told the agent, for an agent's
    /// call — and its number (one window, for now).
    @MainActor
    static func windowName(_ model: AppModel, _ place: UUID?) -> String {
        "\(place.flatMap { model.core(at: $0)?.client } ?? RemoteHosts.macName):1"
    }

    /// A person asked whether MCP may do what it asked to (`AskCenter`): this Mac's core's question.
    /// An agent's comes by its way (`CoreClient.answerAppControl`).
    func ask(request: String) -> String {
        AskCenter.ask(request)
    }

    /// A map drawn as a picture for `as: picture` (`specboard_core::App::picture`), read from this
    /// core and drawn in no window. Drawn whether or not this app takes `app_control` from the
    /// agents (`takesFromAgents`): it shows nothing of the person's window.
    func picture(request: String) -> String {
        MapPicture.answer(request: request, core: core)
    }

    func appControl(arguments: String, caller: String) -> String {
        let model = self.model
        let place = self.place
        if place != nil, !Self.takesFromAgents {
            return Self.refusal(L10n.text(.AppControl.errorNotTaken, in: Self.language(place)))
        }
        // Asked on main itself (a test calling the core there): main cannot wait for main, so
        // the pages WebKit draws are left out of a picture.
        if Thread.isMainThread {
            return MainActor.assumeIsolated {
                Self.$speaking.withValue(Self.language(place)) { Self.answer(model, place, arguments, caller) }
            }
        }
        return Self.waiting(late: Self.late(place)) {
            await Self.$speaking.withValue(Self.language(place)) { await Self.answer(model, place, arguments, caller) }
        }
    }

    // MARK: - The language of the answers

    /// The language the call being answered is answered in (`language(_:_:)`), bound for the call.
    @TaskLocal private static var speaking: String?

    /// What an answer is said in: the call's language inside a call, the screen's outside one.
    static var answerLanguage: String { speaking ?? Language.current }

    /// An agent's call is an MCP call, answered in the language MCP is answered in: English. One
    /// asked of this Mac's core in the same process is answered in the language the rest of that
    /// core's answer is said in, the screen's.
    nonisolated static func language(_ place: UUID?) -> String {
        place == nil ? Language.current : mcpLanguage
    }

    /// The language MCP is answered in, whatever the screen's (the agent's `ANSWERS_IN`).
    nonisolated static let mcpLanguage = "en"

    /// An answer that is only a refusal, made where the window is not reached.
    private static func refusal(_ message: String) -> String {
        let data = (try? JSONSerialization.data(withJSONObject: ["error": message])) ?? Data()
        return String(decoding: data, as: UTF8.self)
    }

    /// The answer, when what WebKit draws cannot be waited for.
    @MainActor
    private static func answer(_ model: AppModel?, _ place: UUID?, _ arguments: String, _ caller: String) -> String {
        said(Result {
            let (window, given) = try Window.reading(model, place, arguments, caller)
            return try MCPAccess.$inCall.withValue(true) { try window.run(given, pages: []) }
        })
    }

    /// The answer; a picture of the window waits for the WebKit pages on it to say they are
    /// drawn.
    @MainActor
    private static func answer(_ model: AppModel?, _ place: UUID?, _ arguments: String, _ caller: String) async -> String {
        do {
            let (window, given) = try Window.reading(model, place, arguments, caller)
            var pages: [WindowCapture.Page] = []
            if window.args.optionalString("op") == "capture", try window.captureTarget() == .window,
               let root = WindowCapture.contentView {
                pages = await WindowCapture.pages(in: root)
            }
            return said(Result { try MCPAccess.$inCall.withValue(true) { try window.run(given, pages: pages) } })
        } catch {
            return said(.failure(error))
        }
    }

    /// What is said when main did not answer in time.
    private static func late(_ place: UUID?) -> String {
        refusal(L10n.text(.AppControl.errorLate, in: language(place)))
    }

    /// An answer made on main, waited for here: this thread blocks, main stays free to do
    /// what it is waiting for. `late` is said when it does not come in time.
    private static func waiting(late: String, _ body: @escaping @MainActor () async -> String) -> String {
        final class Box: @unchecked Sendable {
            var said: String
            init(_ said: String) { self.said = said }
        }
        let box = Box(late)
        let done = DispatchSemaphore(value: 0)
        Task { @MainActor in
            box.said = await body()
            done.signal()
        }
        _ = done.wait(timeout: .now() + 20)
        return box.said
    }

    /// The answer as the core reads it (`specboard_core::App`).
    @MainActor
    private static func said(_ outcome: Result<Answer, Error>) -> String {
        let out: OrderedObject
        switch outcome {
        case .success(.json(let value)):
            out = OrderedObject([("json", value)])
        case .success(.image(let png, let note)):
            out = OrderedObject([
                ("image", .object(OrderedObject([
                    ("data", .string(png.base64EncodedString())),
                    ("mimeType", .string("image/png")),
                ]))),
                ("note", note),
            ])
        case .failure(let error):
            out = OrderedObject([("error", .string((error as? LocalizedError)?.errorDescription ?? String(describing: error)))])
        }
        return OJSON.object(out).compact()
    }
}

/// What `app_control` answers with: structure, or a picture with a line of structure beside
/// it saying what it is.
private enum Answer {
    case json(OJSON)
    case image(png: Data, note: OJSON)
}

/// The call's arguments, read with messages an agent can act on.
private struct Arguments {
    var raw: OrderedObject

    func string(_ name: String) throws -> String {
        guard case .string(let value)? = raw[name] else {
            throw MCPError(.AppControl.errorRequired(name))
        }
        return value
    }
    func optionalString(_ name: String) -> String? {
        if case .string(let value)? = raw[name] { return value }
        return nil
    }
    /// A number too large for an Int comes back as the nearest end of the range, which
    /// every caller's range check then refuses — `Int(1e300)` would trap.
    func int(_ name: String, default fallback: Int) -> Int {
        guard case .number(let value)? = raw[name] else { return fallback }
        return Int(exactly: value.rounded(.towardZero)) ?? (value < 0 ? .min : .max)
    }
    func number(_ name: String) -> Double? {
        if case .number(let value)? = raw[name] { return value }
        return nil
    }
    func strings(_ name: String) -> [String]? {
        guard case .array(let items)? = raw[name] else { return nil }
        return items.compactMap(\.stringOrNil)
    }
    func object(_ name: String) -> OrderedObject? {
        if case .object(let value)? = raw[name] { return value }
        return nil
    }
}

/// One call, on main: what it reads of the window and what it moves.
@MainActor
private struct Window {
    let model: AppModel
    /// Whose call: this Mac's MCP (nil), or an agent's (its projects, its keys).
    let place: UUID?
    let caller: Caller?
    let args: Arguments

    static let ops = ["state", "select", "view", "capture", "windows"]
    static let views = ["fit", "cards", "area", "topLeft", "center", "zoom"]

    /// The call as the core handed it over — its arguments, and who made it (`{"id", "name"}`; null
    /// for the window's own call, which sees every project) — and its value. What the key may do
    /// with the window the core has judged already, a person asked when it is asked each time.
    static func reading(_ model: AppModel?, _ place: UUID?, _ arguments: String, _ caller: String) throws -> (Window, OrderedObject) {
        guard let model else { throw MCPError(.AppControl.errorNoWindow) }
        guard case .object(let raw)? = OJSON.parse(arguments) else { throw MCPError(.AppControl.errorUnreadableArguments) }
        var who: Caller?
        if case .object(let said)? = OJSON.parse(caller), let id = said["id"]?.stringOrNil {
            who = Caller(keyId: id, name: said["name"]?.stringOrNil ?? "")
        }
        let window = Window(model: model, place: place, caller: who, args: Arguments(raw: raw))
        return (window, try window.value())
    }

    func run(_ given: OrderedObject, pages: [WindowCapture.Page]) throws -> Answer {
        let op = try args.string("op")
        if op == "windows" { return .json(.object(OrderedObject([("windows", .array(windows()))]))) }
        try aimedHere(given, op: op)
        switch op {
        case "state": return .json(.object(state(given)))
        case "select": return .json(.object(try select(given)))
        case "view": return .json(.object(try view(given)))
        case "capture": return try capture(given, pages: pages)
        case let other: throw MCPError(.AppControl.errorUnknownOp(Self.ops.joined(separator: " / "), other))
        }
    }

    // MARK: - The window aimed at

    /// This window, as `op: windows` lists it: when it shows a project the call may read (of the
    /// agent's, for an agent's call).
    func windows() -> [OJSON] {
        guard let detail = model.detail, !model.atHome,
              visibleProjects().contains(where: { $0.id == detail.project.id }) else { return [] }
        var out = OrderedObject()
        out["window"] = .string(AppControl.windowName(model, place))
        for (key, value) in selection().entries where !["field", "row", "ghost", "connections"].contains(key) {
            out[key] = value
            if key == "project" { out["branch"] = .string(model.openTree.name) }
        }
        out["front"] = .bool(NSApp.isActive)
        return [.object(out)]
    }

    /// The call is for this window: the one `value.window` names, and the project it names open in it.
    /// An agent's call is for the agent's project this window shows, one its key may read. A call of
    /// this Mac's (`place` nil), which only the tests make since MCP reaches this Mac's projects no
    /// more, may open the project `select` names.
    func aimedHere(_ given: OrderedObject, op: String) throws {
        let mine = AppControl.windowName(model, place)
        if case .string(let named)? = given["window"], named != mine {
            throw MCPError(.AppControl.errorNoSuchWindow(named, mine))
        }
        let open = model.detail?.project
        if place != nil {
            guard let open, visibleProjects().contains(where: { $0.id == open.id }) else {
                throw MCPError(.AppControl.errorKeyCantReadWindow)
            }
        }
        // A call of this Mac's opens the project `select` names here; an agent's chooses in a window
        // that shows it, and swaps no one's project for another.
        if op != "select" || place != nil, let named = args.optionalString("project") {
            let projects = visibleProjects()
            let wanted = try Self.summary(named, among: projects)
            guard wanted.id == open?.id else {
                throw MCPError(.AppControl.errorProjectNotInWindow(
                    wanted.name, open?.name ?? L10n.text(.AppControl.wordNone, in: AppControl.answerLanguage)))
            }
        }
        // Only select opens a tree. The others read and move what the window shows, so a branch they
        // name is the tree it has open — not answered from another without saying so.
        if op != "select", let raw = args.optionalString("branch") {
            guard let open else { throw MCPError(.AppControl.errorNoProject) }
            let named = try branchNamed(raw, projectId: open.id)
            guard isOpen(named) else {
                throw MCPError(.AppControl.errorBranchNotInWindow(named.name, model.openTree.name))
            }
        }
    }

    // MARK: - Reading the call

    func value() throws -> OrderedObject {
        switch args.raw["value"] {
        case nil: return OrderedObject()
        case .object(let object)?: return object
        default: throw MCPError(.AppControl.errorValueNotObject)
        }
    }

    /// The target, as a list. Nil when none was given.
    func targets() throws -> [String]? {
        switch args.raw["target"] {
        case nil: return nil
        case .string(let one)?:
            let trimmed = one.trimmingCharacters(in: .whitespaces)
            guard !trimmed.isEmpty else { throw MCPError(.AppControl.errorEmptyTarget) }
            return [trimmed]
        case .array(let many)?:
            let list = many.compactMap(\.stringOrNil).map { $0.trimmingCharacters(in: .whitespaces) }
            guard !list.isEmpty, list.count == many.count, !list.contains(where: \.isEmpty) else {
                throw MCPError(.AppControl.errorTargetArray)
            }
            return list
        default:
            throw MCPError(.AppControl.errorTargetType)
        }
    }

    // MARK: - op: state

    /// The project and branch open, the section, the card and field chosen, and how the
    /// diagram is seen.
    func state(_ given: OrderedObject) -> OrderedObject {
        var out = OrderedObject()
        for (key, value) in selection().entries {
            out[key] = value
            // The branch the window has the project open in, after the project.
            if key == "project", model.detail != nil {
                out["branch"] = .string(model.openTree.name)
            }
        }
        if let canvas = try? onScreenCanvas() {
            var all = false
            if case .bool(let value)? = given["all"] { all = value }
            for (key, value) in viewState(canvas, allCards: all).entries where out[key] == nil { out[key] = value }
        } else {
            out["view"] = .string(L10n.text(.AppControl.stateNoRelationPanel, in: AppControl.answerLanguage))
        }
        return out
    }

    /// What is open and chosen: what "this" points at. Nothing else reads it.
    private func selection() -> OrderedObject {
        var out = OrderedObject()
        let directory = model.detail?.directory
        out["project"] = model.detail.map { OJSON.string($0.project.name) } ?? .null
        if let section = model.shownSectionPath { out["section"] = .string(section) }
        // A card ghost picked: the ghost by its path, and the card it leads to as the card
        // (null where it leads nowhere).
        if let ghost = model.selectedGhost, let directory {
            out["ghost"] = .string(directory.full(ghost))
            out["card"] = directory.reached(ghost).map { OJSON.string(directory.full($0)) } ?? .null
        } else {
            out["card"] = model.selectedSchema.map { OJSON.string(directory?.full($0) ?? $0.name) } ?? .null
        }
        out["field"] = model.selectedField.map { location in
            OJSON.string(directory?.full(location.schema, location.path) ?? location.dotted)
        } ?? .null
        // A row of the card picked, as a press on it picks it — a formatdoc's item, a flow's node,
        // a goal's task — by its path: the card alone does not say which.
        if let row = model.selectedRow, let card = model.selectedSchema, let directory,
           let path = Self.rowPath(row, of: card, in: directory) {
            out["row"] = .string(path)
        }
        // A card's connections diagram on screen: which, and how far it reaches.
        if case .connections(_, let depth) = model.here, let card = model.connectionsCard {
            out["connections"] = Self.connectionsValue(card, depth, directory)
        }
        return out
    }

    /// A row picked, by the path that names it in full; nil for a row without a path of its own (an
    /// enum's value, a hold).
    static func rowPath(_ row: AppModel.CardRow, of card: SchemaNode, in directory: SchemaDirectory) -> String? {
        switch row {
        case .formatRow(let path), .customRow(let path): directory.full(card, row: path)
        case .node(let name), .nodeStep(let name, _): directory.full(card, [name])
        case .symbol(let path): SymbolPath.full(directory.full(card), path)
        case .value, .hold: nil
        }
    }

    // MARK: - op: select

    /// Opens the project in the tree named — main when none is, as every tool reads it — and
    /// chooses the section, card or field the target names there, bringing it into view on the
    /// map tab; with `as: connections`, the card's connections diagram opened in its tab instead
    /// (`ConnectionsMap`). Everything that can refuse is looked up in that tree before the window
    /// moves: a refused call leaves it where it was.
    func select(_ given: OrderedObject = OrderedObject()) throws -> OrderedObject {
        guard args.raw["project"] != nil else { throw MCPError(.AppControl.errorSelectNeedsProject) }
        let connections = try Self.connections(given)
        let name = try args.string("project")
        let summary = try Self.summary(name, among: visibleProjects())
        let named = try branchNamed(args.optionalString("branch"), projectId: summary.id)
        let detail = try treeDetail(summary, named)
        let pick = try picked(in: detail)
        let card = try connections.map { _ in
            guard let card = pick.card(in: detail.directory) else { throw MCPError(.AppControl.errorConnectionsTarget) }
            return card
        }
        openTree(summary.id, named)
        var opened = show(pick, in: detail)
        if let depth = connections, let card {
            model.openConnections(card.id, depth: depth)
            opened["connections"] = Self.connectionsValue(card, depth, model.detail?.directory)
        }
        return headed(opened)
    }

    /// The depth `as: connections` asks for; nil when it does not ask.
    static func connections(_ given: OrderedObject) throws -> ConnectionDepth? {
        switch given["as"] {
        case nil: break
        case .string("connections")?: break
        default: throw MCPError(.AppControl.errorSelectAs)
        }
        guard given["as"] != nil else {
            if given["depth"] != nil { throw MCPError(.AppControl.errorDepthWithoutAs) }
            return nil
        }
        guard let raw = given["depth"] else { return .one }
        // `Int(exactly:)`: a number no Int holds (`1e300`) is refused, not trapped on.
        guard case .number(let number) = raw, let whole = Int(exactly: number),
              let depth = ConnectionDepth(rawValue: whole) else {
            throw MCPError(.AppControl.errorDepthRange)
        }
        return depth
    }

    /// A connections diagram, as an answer says it: the card's path and how far it reaches.
    static func connectionsValue(_ card: SchemaNode, _ depth: ConnectionDepth, _ directory: SchemaDirectory?) -> OJSON {
        .object(OrderedObject([
            ("card", .string(directory?.full(card) ?? card.name)),
            ("depth", .number(Double(depth.rawValue))),
        ]))
    }

    /// What a select picks in a tree, found before anything is shown.
    enum Pick {
        case project
        case section(ProjectSection)
        case ghost(CardGhost)
        case found(MCPViews.Target)

        /// The card it is, or is in: the card a field or row is in, the card a ghost leads to.
        func card(in directory: SchemaDirectory) -> SchemaNode? {
            switch self {
            case .project, .section: return nil
            case .ghost(let ghost): return directory.reached(ghost)
            case .found(.schema(let card)), .found(.field(let card, _, _)), .found(.formatRow(let card, _)),
                 .found(.node(let card, _)), .found(.customRow(let card, _)):
                return card
            }
        }
    }

    /// The target looked up in the tree read (`treeDetail`): a section by its path, a card ghost's own
    /// path as the ghost (as a press on its header picks it), any other path as what it reaches.
    private func picked(in detail: ProjectDetail) throws -> Pick {
        guard let target = try targets()?.first else { return .project }
        if !PathText.hasSection(target), let section = try? Self.section(target, in: detail) {
            return .section(section)
        }
        if let written = WrittenPath(target), written.names.isEmpty,
           let ghost = detail.directory.ghost(written.card, from: nil) {
            return .ghost(ghost)
        }
        return .found(try MCPViews.locate(detail, target))
    }

    /// The project as the tree named reads it: the window's own copy when it has that tree open,
    /// the core's otherwise. Read before the window moves, so what a call names is looked for where
    /// it is going to be shown.
    private func treeDetail(_ summary: ProjectSummary, _ named: MCPAccess.Named) throws -> ProjectDetail {
        if model.detail?.project.id == summary.id, isOpen(named), let detail = model.detail { return detail }
        guard let core = model.core(of: summary.id),
              let detail = try? core.detail(summary.id, branch: named.branchId) else {
            throw MCPError(.AppControl.errorCantReadProject(summary.name))
        }
        return detail
    }

    /// Whether the window has the tree named open: main as a person sees it (main, or the branch a
    /// locked main is put right in) for main, that tree otherwise.
    private func isOpen(_ named: MCPAccess.Named) -> Bool {
        let open = model.openTree
        return named.branchId == nil ? open.name == SchemaDirectory.mainWord : open.id == named.branchId
    }

    /// The project shown in the tree named, switched to only where the window has another open.
    private func openTree(_ projectId: String, _ named: MCPAccess.Named) {
        if model.detail?.project.id != projectId { model.select(projectId) }
        if !isOpen(named) { model.openBranch(named.branchId) }
    }

    /// What was picked, chosen in the window, which has its tree open now.
    private func show(_ pick: Pick, in detail: ProjectDetail) -> OrderedObject {
        let directory = detail.directory
        // What is selected is shown on the map tab, brought forward — the tabs open stay, none is added —
        // and not under the project list, where the window now opens.
        defer {
            model.showCards()
            model.atHome = false
        }
        switch pick {
        case .project:
            return OrderedObject([("opened", .string(detail.project.name))])
        case .section(let section):
            model.selectSection(section.id)
            return OrderedObject([
                ("project", .string(detail.project.name)),
                ("opened", .string(directory.full(section: section.id))),
            ])
        case .ghost(let ghost):
            model.selectGhost(ghost.id)
            model.focusRequest = ghost.id
            return OrderedObject([("selected", .string(directory.full(ghost)))])
        case .found(.schema(let schema)):
            model.selectSchema(schema.id)
            model.focusRequest = schema.id
            return OrderedObject([("selected", .string(directory.full(schema)))])
        case .found(.field(let owner, let field, let walked)):
            model.selectField(owner.id, field.id)
            model.focusRequest = owner.id
            return OrderedObject([("selected", .string(directory.full(owner, walked)))])
        case .found(.formatRow(let note, let names)):
            model.selectRow(note.id, .formatRow(PathText.joinNames(names)))
            model.focusRequest = note.id
            return OrderedObject([("selected", .string(directory.full(note, names)))])
        case .found(.node(let flow, let node)):
            model.selectNode(flow.id, node.name)
            model.focusRequest = flow.id
            return OrderedObject([("selected", .string(directory.full(flow, [node.name])))])
        case .found(.customRow(let card, let names)):
            let row = CardWalk.rowPath(names, in: card)
            model.selectRow(card.id, .customRow(row))
            model.focusRequest = card.id
            return OrderedObject([("selected", .string(directory.full(card, row: row)))])
        }
    }

    /// An answer headed with the tree the window has open, once: after `project`, behind `warnings`.
    private func headed(_ body: OrderedObject) -> OrderedObject {
        var out = OrderedObject()
        if let warnings = body["warnings"] { out["warnings"] = warnings }
        if let project = body["project"] { out["project"] = project }
        out["branch"] = .string(model.openTree.name)
        for (key, value) in body.entries where !["warnings", "project", "branch"].contains(key) { out[key] = value }
        return out
    }

    // MARK: - Which projects the key sees

    /// The projects the call's key may see — what it was given, and the public ones — of those
    /// the window lists; every one for the window's own call. A lookup that fails sees
    /// nothing rather than everything (`specboard://SpecBoard/Docs/Design:DesignAccess` §3).
    private func visibleProjects() -> [ProjectSummary] {
        let here = model.projects.filter { $0.place == place }
        guard let caller else { return place == nil ? model.projects : here }
        guard let core = model.core(at: place), let keys = try? core.accessKeys(), let open = try? core.publicProjects() else {
            return []
        }
        let given = keys.first { $0.id == caller.keyId }?.grants ?? [:]
        return here.filter { given[$0.id] != nil || open.contains($0.id) }
    }

    /// The project a call names, among those it may see: one it may not is missing in the same
    /// words as one that is not there. Exact first, then case-insensitively — a project named in
    /// prose comes back with whatever capitalisation the sentence had.
    private static func summary(_ name: String, among projects: [ProjectSummary]) throws -> ProjectSummary {
        let match = projects.first { $0.id == name }
            ?? projects.first { $0.name == name }
            ?? projects.first { $0.slug == name }
            ?? projects.first { $0.name.lowercased() == name.lowercased() }
        guard let summary = match else {
            throw projects.isEmpty
                ? MCPError(.AppControl.errorNoProjectVisible(name))
                : MCPError(.AppControl.errorNoProjectAmong(name, cappedNames(projects.map(\.name))))
        }
        return summary
    }

    /// What a call's `branch` names in a project, as every tool reads it: main for none or `main`; the
    /// project board for `_project_board_`; a branch of the project by its name, in any case; refused,
    /// listing what there is, otherwise — and `_directory_` refused saying where the working directories
    /// are read now.
    private func branchNamed(_ raw: String?, projectId: String) throws -> MCPAccess.Named {
        let name = (raw ?? "").trimmingCharacters(in: .whitespaces)
        guard !name.isEmpty, name.lowercased() != SchemaDirectory.mainWord else {
            return MCPAccess.Named(projectId: projectId, branchId: nil, name: SchemaDirectory.mainWord)
        }
        if name.lowercased() == Board.word {
            return MCPAccess.Named(projectId: projectId, branchId: Board.id, name: Board.word)
        }
        if name.lowercased() == Self.directoryWord {
            throw MCPError(.AppControl.errorDirectoryTree)
        }
        guard let core = model.core(of: projectId) else { throw MCPError(.AppControl.errorStoreNotOpen) }
        let branches = try core.branches(projectId)
        if let found = branches.first(where: { $0.name.lowercased() == name.lowercased() }) {
            return MCPAccess.Named(projectId: projectId, branchId: found.id, name: found.name)
        }
        let all = [SchemaDirectory.mainWord] + branches.map(\.name) + [Board.word]
        throw MCPError(.AppControl.errorNoBranch(name, all.joined(separator: " / ")))
    }

    /// A tree no tool reads: the working directories are sections of main and every branch, so naming it is
    /// refused with where to read them. No branch may be called so.
    static let directoryWord = "_directory_"

    /// The section named, by its path.
    static func section(_ name: String, in detail: ProjectDetail) throws -> ProjectSection {
        let directory = detail.directory
        if let found = directory.section(named: name.trimmingCharacters(in: .whitespaces)) {
            return found
        }
        if detail.sections.isEmpty, name.compare(ProjectSection.defaultName, options: .caseInsensitive) == .orderedSame {
            return ProjectSection(id: directory.home, projectId: detail.project.id, name: ProjectSection.defaultName,
                               description: "", sortOrder: 0)
        }
        throw detail.sections.isEmpty
            ? MCPError(.AppControl.errorNoSection(name, ProjectSection.defaultName))
            : MCPError(directory.sectionProblem(name))
    }

    // MARK: - op: view

    /// The diagram moved: to fit everything, some cards, an area, a point or card at the
    /// top left or the centre, or a zoom about the centre. The view after it is the answer.
    /// What is saved does not change.
    func view(_ given: OrderedObject) throws -> OrderedObject {
        guard let mode = try targets()?.first else {
            throw MCPError(.AppControl.errorViewTarget(Self.views.joined(separator: " / ")))
        }
        guard Self.views.contains(mode) else {
            throw MCPError(.AppControl.errorViewTargetGiven(Self.views.joined(separator: " / "), mode))
        }
        let given = Arguments(raw: given)
        let canvas = try onScreenCanvas()
        let zoom = given.number("zoom")
        let wanted = given.int("margin", default: 32)
        guard (0...400).contains(wanted) else { throw MCPError(.AppControl.errorMarginRange(wanted)) }
        let margin = CGFloat(wanted)
        if let zoom, !canvas.zoomLimits.contains(CGFloat(zoom)) {
            throw MCPError(.AppControl.errorZoomRange("\(canvas.zoomLimits.lowerBound)", "\(canvas.zoomLimits.upperBound)", "\(zoom)"))
        }
        let frames = canvas.cardFrames
        func frame(_ name: String) throws -> CGRect {
            // The diagram is one section, so a bare name is read from it.
            guard let detail = model.detail else { throw MCPError(.AppControl.errorNoProject) }
            let schema: SchemaNode
            switch detail.directory.find(name, from: model.shownSection?.id) {
            case .schema(let hit): schema = hit
            case .failure(let why): throw MCPError("\(name): \(why)")
            }
            guard let found = frames[schema.id] else {
                throw MCPError(.AppControl.errorNotDrawn(name))
            }
            return found
        }
        func point() throws -> CGPoint? {
            guard let raw = given.object("point") else { return nil }
            guard case .number(let x)? = raw["x"], case .number(let y)? = raw["y"] else {
                throw MCPError(.AppControl.errorPointForm)
            }
            return CGPoint(x: x, y: y)
        }
        let cap = CGFloat(zoom ?? 1)
        switch mode {
        case "fit":
            guard !frames.isEmpty else { throw MCPError(.AppControl.errorNoCardsDrawn) }
            canvas.show(area: canvas.diagramExtent, margin: margin, cap: cap)
        case "cards":
            guard let names = given.strings("cards"), !names.isEmpty else {
                throw MCPError(.AppControl.errorCardsNeeded)
            }
            let union = try names.map(frame).reduce(CGRect.null) { $0.union($1) }
            canvas.show(area: union, margin: margin, cap: cap)
        case "area":
            guard let raw = given.object("area"),
                  case .number(let x)? = raw["x"], case .number(let y)? = raw["y"],
                  case .number(let w)? = raw["width"], case .number(let h)? = raw["height"],
                  w > 0, h > 0 else {
                throw MCPError(.AppControl.errorAreaForm)
            }
            canvas.show(area: CGRect(x: x, y: y, width: w, height: h), margin: margin, cap: cap)
        case "topLeft":
            let target: CGPoint
            if let name = given.optionalString("card") {
                target = try frame(name).origin
            } else if let at = try point() {
                target = at
            } else {
                target = canvas.diagramExtent.origin
            }
            canvas.place(target, atTopLeftWith: margin, zoom: CGFloat(zoom ?? 1))
        case "center":
            let target: CGPoint
            if let name = given.optionalString("card") {
                let found = try frame(name)
                target = CGPoint(x: found.midX, y: found.midY)
            } else if let at = try point() {
                target = at
            } else {
                throw MCPError(.AppControl.errorCenterNeeds)
            }
            canvas.center(on: target, zoom: CGFloat(zoom ?? 1))
        default:
            guard let zoom else { throw MCPError(.AppControl.errorZoomNeeded) }
            let visible = canvas.visibleArea
            canvas.center(on: CGPoint(x: visible.midX, y: visible.midY), zoom: CGFloat(zoom))
        }
        return viewState(canvas, allCards: false)
    }

    // MARK: - op: capture

    /// What a picture is of: the whole window, or the diagram as much of it as is on screen.
    func captureTarget() throws -> WindowCapture.Target {
        let what = try targets()?.first ?? "window"
        guard ["window", "view"].contains(what) else {
            throw MCPError(.AppControl.errorCaptureTarget(what))
        }
        return what == "view" ? .graph : .window
    }

    /// The window as a PNG, and what it shows. `pages`: the WebKit pages on screen,
    /// pictured (`WindowCapture.pages(in:)`). Popovers and dialogs are not in it.
    func capture(_ given: OrderedObject, pages: [WindowCapture.Page]) throws -> Answer {
        let target = try captureTarget()
        let maxSize = Arguments(raw: given).int("maxSize", default: WindowCapture.defaultMaxSize)
        guard WindowCapture.sizeRange.contains(maxSize) else {
            throw MCPError(.AppControl.errorMaxSizeRange(maxSize))
        }
        let picture = try WindowCapture.capture(target, maxSize: maxSize, pages: pages)
        var note = OrderedObject()
        note["target"] = .string(target.rawValue)
        note["project"] = model.detail.map { OJSON.string($0.project.name) } ?? .null
        if model.detail != nil { note["branch"] = .string(model.openTree.name) }
        if let section = model.shownSectionPath { note["section"] = .string(section) }
        note["width"] = .number(Double(picture.width))
        note["height"] = .number(Double(picture.height))
        // The zoom and the part on screen are whoever last moved the view — often a person —
        // so the picture says which, rather than leaving it to be guessed from the size of
        // the text.
        if let canvas = try? onScreenCanvas() {
            note["zoom"] = Self.zoomValue(canvas.zoom)
            note["visible"] = Self.rectValue(canvas.visibleArea)
        }
        return .image(png: picture.png, note: .object(note))
    }

    // MARK: - The diagram's viewport

    private func onScreenCanvas() throws -> CardCanvasView {
        guard let canvas = WindowCapture.canvas, canvas.window != nil,
              !canvas.isHiddenOrHasHiddenAncestor, canvas.bounds.width > 0 else {
            throw MCPError(.AppControl.errorNoRelationPanel)
        }
        return canvas
    }

    private static func zoomValue(_ zoom: CGFloat) -> OJSON {
        .number((Double(zoom) * 1000).rounded() / 1000)
    }

    private static func rectValue(_ rect: CGRect) -> OJSON {
        .object(OrderedObject([
            ("x", .number(Double(rect.minX.rounded()))), ("y", .number(Double(rect.minY.rounded()))),
            ("width", .number(Double(rect.width.rounded()))), ("height", .number(Double(rect.height.rounded()))),
        ]))
    }

    /// What a card drawn on the diagram is called in an answer: a card by its path, a
    /// section ghost by the path of the section it stands for (`sectionGhost`), a card ghost
    /// by its own path (`ghost`) — never an id.
    private static func drawnName(_ id: String, directory: SchemaDirectory?,
                                  names: [String: String]) -> (key: String, name: String) {
        if SectionGhosts.isGhost(id), let directory {
            return ("sectionGhost", directory.full(section: String(id.dropFirst(SectionGhosts.prefix.count))))
        }
        if let directory, let ghost = directory.ghost(id: id) { return ("ghost", directory.full(ghost)) }
        return ("card", names[id] ?? id)
    }

    /// What the diagram shows, in diagram coordinates (x to the right, y down).
    private func viewState(_ canvas: CardCanvasView, allCards: Bool) -> OrderedObject {
        let directory = model.detail?.directory
        let names = Dictionary((model.detail?.schemas ?? []).map { ($0.id, directory?.full($0) ?? $0.name) },
                               uniquingKeysWith: { first, _ in first })
        let visible = canvas.visibleArea
        let frames = canvas.cardFrames.sorted {
            ($0.value.minY, $0.value.minX) < ($1.value.minY, $1.value.minX)
        }
        var cards: [OJSON] = []
        var onScreen = 0
        for (id, frame) in frames {
            let shown = visible.contains(frame) ? "all" : visible.intersects(frame) ? "part" : "none"
            if shown != "none" { onScreen += 1 }
            guard allCards || shown != "none" else { continue }
            let said = Self.drawnName(id, directory: directory, names: names)
            var card = OrderedObject([(said.key, .string(said.name))])
            if case .object(let box) = Self.rectValue(frame) {
                for (key, value) in box.entries { card[key] = value }
            }
            card["shown"] = .string(shown)
            cards.append(.object(card))
        }
        var out = OrderedObject()
        out["project"] = model.detail.map { OJSON.string($0.project.name) } ?? .null
        if model.detail != nil { out["branch"] = .string(model.openTree.name) }
        if let section = model.shownSection { out["section"] = .string(section.name) }
        out["zoom"] = Self.zoomValue(canvas.zoom)
        out["viewport"] = .object(OrderedObject([
            ("width", .number(Double(canvas.bounds.width.rounded()))),
            ("height", .number(Double(canvas.bounds.height.rounded()))),
        ]))
        out["visible"] = Self.rectValue(visible)
        out["diagram"] = Self.rectValue(canvas.diagramExtent)
        out["cardsDrawn"] = .number(Double(frames.count))
        out["cardsOnScreen"] = .number(Double(onScreen))
        out["cards"] = .array(cards)
        return out
    }
}

/// Why an `app_control` op was turned away, as the agent hands it on to the AI.
struct MCPError: LocalizedError {
    var errorDescription: String?

    init(_ message: String) { errorDescription = message }

    /// A refusal of the app's own, in the language the call is answered in.
    init(_ message: LocalizedStringResource) { errorDescription = L10n.text(message, in: AppControl.answerLanguage) }
}
