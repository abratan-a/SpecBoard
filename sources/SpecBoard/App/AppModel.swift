import CoreGraphics
import Foundation
import os
import Observation

/// What the window is looking at.
///
/// The local database is the source of truth.
@Observable
final class AppModel {

    enum Phase {
        case loading
        case ready
        case failed(String)
    }

    private(set) var phase: Phase = .loading
    private(set) var projects: [ProjectSummary] = []
    private(set) var detail: ProjectDetail? {
        didSet {
            if detail?.project.id != oldValue?.project.id {
                remember()
                forgetOutlines()
            }
        }
    }
    /// What the property panel reads of the core beside the project — the board's cards still to do
    /// reaching what it shows (`boardReach`) — kept by what was asked, until the whole project is read
    /// again (`putWhole`); what was kept before shows while it is read again.
    @ObservationIgnored var asideKept: [String: Any] = [:]
    @ObservationIgnored var asideBefore: [String: Any] = [:]
    /// What of it is being read off the main thread now.
    @ObservationIgnored var asideAsked: Set<String> = []
    /// How many requests `aside` has sent to the core.
    @ObservationIgnored var asideSent = 0
    /// Bumped as what was read off the main thread comes in: what shows it is drawn again.
    var asideReads = 0
    /// The working directories' files' functions and types read so far, by the file's id
    /// (`AppModel+Code`): read as a file row opens, a file's card is drawn or a file is opened, and kept
    /// with the stamp of the text they were read from — asked again after the project is read again
    /// (`outlineStale`), answered `unchanged` while the file is as it was.
    @ObservationIgnored var outlineStore: [String: FileOutline] = [:]
    @ObservationIgnored var outlineStale: Set<String> = []
    @ObservationIgnored var outlineAsked: Set<String> = []
    /// Bumped each time `outlineStore` changes: what is worked out from it is worked out again.
    @ObservationIgnored var outlineVersion = 0
    /// Bumped as outlines read off the main thread come in: what shows them is drawn again.
    var outlineReads = 0
    /// The tree beside the one open that its lines reach, read beside it: main's while the board is
    /// open — the cards they reach drawn on its diagram as that tree's section ghosts, offered to つなぐ,
    /// and named in full. Nil while main or a branch is open.
    private(set) var outsideDetail: ProjectDetail?
    private(set) var selectedSchemaId: String? {
        didSet {
            if selectedSchemaId != nil { pickedGone = nil }
            remember()
        }
    }
    private(set) var selectedFieldId: String? { didSet { remember() } }
    /// A line picked — on a diagram, or in the property panel's つながり — by the card that
    /// holds it and its id (overview §13): the property panel shows it, the diagram marks it.
    /// Picking anything else lets it go.
    private(set) var selectedLine: LineRef? { didSet { if selectedLine != nil { pickedGone = nil } } }
    /// What the open branch removed, picked in the nav panel by its id (a card, a card ghost, a
    /// node or a section): the property panel shows it, read only. Picking anything else lets it go.
    private(set) var pickedGone: String?
    /// A line being drawn by hand (つなぐ, overview §13): from where, to what once picked, and
    /// where on the diagram it was asked — the place つなぐ is shown (`ConnectView`).
    var connecting: Connecting?

    /// The section the diagram draws. Nil means the project's first — which, for a project
    /// that has only one, is the whole project.
    private(set) var shownSectionId: String? { didSet { remember() } }

    /// The section on screen, when the project has more than one. Nil otherwise, so
    /// that everything asking "which section" says nothing in a project that is not
    /// divided.
    var shownSection: ProjectSection? {
        guard let detail, detail.sections.count > 1 else { return nil }
        return detail.sections.first { $0.id == shownSectionId } ?? detail.sections.first
    }

    /// The section on screen as it is written in a path — `Store/Ordering`.
    var shownSectionPath: String? {
        guard let detail, let shown = shownSection else { return nil }
        return detail.directory.full(section: shown.id)
    }

    /// The id the diagram is drawing, whether or not the project is divided.
    var shownSectionKey: String {
        guard let detail else { return "" }
        let directory = detail.directory
        return detail.sections.contains { $0.id == shownSectionId } ? shownSectionId! : directory.home
    }

    /// What the diagram draws: the section on screen, and nothing from the others.
    /// Kept for the project as read and the section on screen: asked for many times a
    /// screen update, worked out once. A write re-reads the project, whose new stamp
    /// makes this be worked out again.
    var sectionDetail: ProjectDetail? {
        guard let detail else { return nil }
        let key = shownSectionKey
        _ = outlineReads
        if let kept = sectionSlice, kept.stamp == detail.stamp, kept.section == key {
            // Read again where the project was read again since: what comes in is drawn when it does.
            if kept.files.contains(where: outlineStale.contains) { askOutlines(kept.files) }
            if kept.outlines == outlineVersion { return kept.slice }
        }
        var slice = detail.only(section: key)
        // Its files' functions and types, drawn as their cards' rows: read for the files on it alone.
        let files = slice.schemas.filter(\.isCodeFile).map(\.id)
        askOutlines(files)
        slice = slice.withSymbols(outlineStore)
        sectionSlice = (detail.stamp, key, outlineVersion, files, slice)
        return slice
    }
    @ObservationIgnored private var sectionSlice: (stamp: Int, section: String, outlines: Int, files: [String], slice: ProjectDetail)?

    /// Open a section on the map tab, which comes forward. The selection goes if it was in another one:
    /// it is not on this section's diagram.
    func selectSection(_ id: String) {
        guard let detail, detail.sections.contains(where: { $0.id == id }) else { return }
        showSection(id)
        activeTab = nil
        saveTabs()
    }

    /// The section on screen made `id`, whichever tab is in front: what the nav panel, the property panel
    /// and the map draw from. The selection goes if it was in another one.
    private func showSection(_ id: String) {
        guard let detail, detail.sections.contains(where: { $0.id == id }) else { return }
        guard id != shownSectionKey else { return }
        DiagramTimes.beginSwitch()
        // What is remembered is written once for the switch, not again for each thing it lets go.
        rememberHeld += 1
        defer {
            rememberHeld -= 1
            remember()
        }
        shownSectionId = id
        surround()
        let directory = detail.directory
        if let selected = selectedSchema, directory.sectionId(of: selected) != id {
            selectedSchemaId = nil
            selectedFieldId = nil
        }
        // A card ghost picked in another section goes too (the property panel the section's).
        if let ghost = selectedGhost, directory.sectionId(of: ghost) != id {
            selectedSchemaId = nil
        }
        layoutGeneration += 1
    }

    /// What the section on screen draws from the others: see `Surroundings`.
    /// Worked out when the project is read or the section changes, not on every
    /// redraw — it walks every relation the project has.
    private(set) var surroundings = Surroundings()

    /// Section → schema → where that section put another's card.
    @ObservationIgnored private var sectionPositions: [String: [String: CGPoint]] = [:]

    private func surround() {
        let key = shownSectionKey
        // What a merge looked at would take away from the section, as main has it; what the open
        // branch removed from it, the same way.
        surroundings = Self.surroundings(of: key, in: detail, positions: sectionPositions[key] ?? [:],
                                         outside: outsideDetail,
                                         gone: resultView?.gone ?? shownMarks.gone.cards,
                                         forNow: temporaryCardGhosts[key] ?? [])
    }

    /// What the diagram of section `key` draws from the others (`Surroundings`): its section ghosts where
    /// they were put (`positions`), the cards of `gone` that the section loses, and the cards put there for
    /// a look (`forNow`). Worked out the same for the window and for a picture of the section
    /// (`MapPicture`).
    static func surroundings(of key: String, in detail: ProjectDetail?, positions: [String: CGPoint],
                             outside: ProjectDetail?, gone: [SchemaNode], forNow: [String]) -> Surroundings {
        var out = detail?.surroundings(of: key, positions: positions, outside: outside) ?? Surroundings()
        // Its section ghosts where they were put, by the section each sums up.
        for (id, at) in positions where SectionGhosts.isGhost(id) {
            out.positions[id] = at
        }
        // A working directory's section, or a folder in one: each folder inside as a box, by name — where it
        // was put, or on the grid the files are laid on (`WorkDirGrid`), the folders first.
        if let detail, let section = detail.directory.section(id: key),
           section.kindName == WorkDirIds.workdirKind || section.kindName == WorkDirIds.directoryKind {
            let inside = detail.directory.children(of: key)
                .sorted { $0.name.localizedStandardCompare($1.name) == .orderedAscending }
            out.folders = inside.map { Surroundings.FolderBox(id: SectionGhosts.prefix + $0.id, name: $0.name) }
            out.missingFolders = Set(inside.filter(\.missing).map { SectionGhosts.prefix + $0.id })
            let grid = WorkDirGrid.places(sections: inside.filter { !$0.missing }, files: [])
            for (id, at) in grid where out.positions[id] == nil {
                out.positions[id] = CGPoint(x: at.x, y: at.y)
            }
        }
        // Another section's value objects and enums are inside the cards holding
        // them, on the diagram as everywhere: not cards it points at.
        let types = Set(out.schemas.filter { $0.kind == .value || $0.kind == .enum }.map(\.id))
        out.references.removeAll { types.contains($0) }
        out.consumers.removeAll { types.contains($0) }
        // Only its cards that have a box of their own (a value object or an enum is inside the card
        // holding it).
        out.gone = gone.filter { $0.sectionId == key && $0.container == nil && $0.kind != .value && $0.kind != .enum }
        // What was put there for a look: another section's card whole, one of this
        // section's value objects or enums as its card.
        if let detail {
            let directory = detail.directory
            let seen = Set(out.schemas.map(\.id))
            for id in forNow {
                guard let schema = detail.schemas.first(where: { $0.id == id }) else { continue }
                if directory.sectionId(of: schema) == key {
                    guard schema.kind == .value || schema.kind == .enum, !out.here.contains(id) else { continue }
                    out.here.append(id)
                } else {
                    guard seen.contains(id), !out.forNow.contains(id) else { continue }
                    out.forNow.append(id)
                }
                out.temporary.insert(id)
            }
        }
        return out
    }

    // MARK: - Card ghosts for a look

    /// Cards put on a section's diagram for a look: drawn like card ghosts, kept by
    /// nobody — not in the project, not in what it exports, not in its history. By
    /// section, until the app quits.
    private(set) var temporaryCardGhosts: [String: [String]] = [:]

    /// Put a card on the diagram on screen for now: another section's, whole; one of
    /// this section's value objects or enums, as a card — with a line from each card
    /// on screen that holds it. A card already on it is left as it is.
    func showForNow(_ schemaId: String) {
        let key = shownSectionKey
        guard !(temporaryCardGhosts[key] ?? []).contains(schemaId) else { return }
        temporaryCardGhosts[key, default: []].append(schemaId)
        surround()
        dataGeneration += 1
    }

    func stopShowingForNow(_ schemaId: String) {
        let key = shownSectionKey
        temporaryCardGhosts[key]?.removeAll { $0 == schemaId }
        surround()
        dataGeneration += 1
    }

    func clearForNow() {
        temporaryCardGhosts[shownSectionKey] = nil
        surround()
        dataGeneration += 1
    }

    /// Put there for a look, on the section on screen.
    func isForNow(_ schemaId: String) -> Bool { surroundings.temporary.contains(schemaId) }

    /// Keep one: another section's card as a card ghost of the section on screen (its name
    /// asked when the section has it already, `placeGhost`); one of its own value objects
    /// or enums shown on its own (`standalone` — how it looks, so a locked main takes it).
    func keepForNow(_ schemaId: String) {
        guard let detail, let schema = schema(schemaId) else { return }
        let key = shownSectionKey
        let own = detail.directory.sectionId(of: schema) == key
        guard own ? canSetLooks : !isReadOnly else { return }
        temporaryCardGhosts[key]?.removeAll { $0 == schemaId }
        surround()
        dataGeneration += 1
        if own {
            setStandalone(schemaId, true)
        } else {
            placeGhost(of: schemaId, in: key)
        }
    }

    /// A schema of another section than the one on screen — drawn, if at all, as
    /// somebody else's card.
    func isOutside(_ schemaId: String) -> Bool {
        guard let detail, detail.sections.count > 1, let schema = schema(schemaId) else { return false }
        return detail.directory.sectionId(of: schema) != shownSectionKey
    }

    /// How the open project's diagram lays its cards out (§12). Re-read with the project.
    private(set) var defaultSettings = LayoutSettings.standard
    /// How wide every card of the open project is drawn: the width set, or for `auto`
    /// the widest card of the project as it first shows (§12). A grid unit is a tenth.
    private(set) var cardWidth = CardGeometry.cardWidth
    /// The change history, newest first. Loaded with the project.
    private(set) var events: [ChangeEvent] = []
    /// Who is putting the open branch right in one go (`Hold`). Loaded with the project, and
    /// again after each MCP write.
    private(set) var holds: [Hold] = []
    /// The open branch's commits, newest first (`specboard://SpecBoard/Docs/Design:DesignBranches` §7). Loaded
    /// with the project, and again after each MCP write.
    private(set) var commits: [CommitInfo] = []
    /// How many commits the stop let go when each project was first opened this run.
    private(set) var pruned: [String: Int] = [:]

    /// Bumped only when the *arrangement* is rebuilt.
    ///
    /// The canvas deliberately ignores position changes — it moves a dragged card
    /// itself, and reloading there would fight the drag. But auto-layout changes
    /// nothing *except* positions, so it is indistinguishable from a drag by looking
    /// at the data. This says "that was me, reload" out loud instead.
    private(set) var layoutGeneration = 0

    /// Bumped whenever the project is re-read from the database.
    ///
    /// Every write reads again what it changed (`readWritten`), or the whole project (`show`), so
    /// this changes exactly when the model may have changed and never when it cannot have.
    /// Dragging a card — on a section's diagram, or on a card's own map — writes positions
    /// straight into `detail` and deliberately does not come this way — the canvas
    /// moved that card itself, and reloading there would fight the drag.
    ///
    /// Counting what the project contains — schema count, top level field count,
    /// relation count — would miss every edit that changes nothing countable: a line's
    /// landing moved, a line of the project's own kind added beside one, an embedding pointed
    /// somewhere else. Those are exactly the edits that change a line.
    private(set) var dataGeneration = 0

    /// Where the database lives. Overridable so tests do not touch the real one.
    private let databaseURL: URL?

    /// For tests that read the file behind the model's back.
    var databaseURLForTests: URL? { databaseURL }

    /// Where the selection is kept between launches, or nil to keep nothing.
    @ObservationIgnored private let memory: SelectionMemory?

    /// Whether what a write changed, and another hand's change, is read again off the main thread, the
    /// window drawing meanwhile and the model replaced once it is in (the window); or at once, before the
    /// write returns (the tests, which look at the model right after).
    @ObservationIgnored let readsOffMain: Bool

    /// What the cards drawn as documents show, kept for every diagram of the window (`documentFace`).
    @ObservationIgnored let documentTexts = DocumentTexts()
    /// The ui parts' faces, kept for the window as the documents' pages are (`PartFaces`).
    @ObservationIgnored let partFaces = PartFaces()

    /// What the window is waiting for, said in the status bar (`StatusBar`): a write, or a reading.
    enum Activity: Equatable {
        case writing, reading

        /// One word for both: a person waits the same for either.
        var said: String { L10n[.Model.activityWorking] }
    }
    /// What the window waits for now: what it does on the main thread (`working`), else a reading in the
    /// background.
    private(set) var activity: Activity?
    /// Something said in the status bar for a few seconds (`say`).
    private(set) var said: String?
    /// The status bar's line, drawn at once when the main thread is about to be busy (`working`).
    @ObservationIgnored weak var statusLine: StatusLineView?
    /// What is being done on the main thread now, if anything.
    @ObservationIgnored private var busy: Activity?
    /// The reading in the background the window waits for (nil: none). A reading begun later — or the
    /// whole project read at once — lets an earlier one go, so what is put in is never older than what
    /// was asked last.
    @ObservationIgnored private var pending: PendingReading?
    /// What is to be done once the reading the window waits for is in (a card just made picked).
    @ObservationIgnored private var afterReading: [() -> Void] = []
    /// The one saying in the status bar that is taken away when its time is up.
    @ObservationIgnored private var sayingNumber = 0
    /// What a person does while a reading is on its way, held until it is in (`InputHold`).
    @ObservationIgnored private let inputHold = InputHold()

    init(databaseURL: URL? = nil, memory: SelectionMemory? = nil, readsOffMain: Bool = false) {
        self.databaseURL = databaseURL
        self.memory = memory
        self.readsOffMain = readsOffMain
    }

    /// Written on every change rather than at quit: a relaunch from a rebuild does
    /// not always get to say goodbye.
    private func remember() {
        guard rememberHeld == 0 else { return }
        reportWindows()
        guard let memory else { return }
        let now = detail.map { shown in
            SelectionMemory.Remembered(project: shown.project.id, schema: selectedSchemaId,
                                       field: selectedFieldId,
                                       section: shownSectionId,
                                       place: projects.first { $0.id == shown.project.id }?.place)
        }
        // The same as written last: nothing to write.
        guard now == nil || now != remembered else { return }
        remembered = now
        memory.save(now)
    }
    /// While above zero, what is remembered waits to be written once (`selectSection`).
    @ObservationIgnored private var rememberHeld = 0
    /// What was written last (`remember`).
    @ObservationIgnored private var remembered: SelectionMemory.Remembered?

    /// Put back what was selected when the app last ran, as far as it still exists.
    ///
    /// A schema deleted since leaves the project open with nothing selected; a field
    /// deleted, or moved to another schema, leaves the schema selected.
    private func restore(_ remembered: SelectionMemory.Remembered) {
        guard let detail, detail.project.id == remembered.project else { return }
        if let section = remembered.section, detail.sections.contains(where: { $0.id == section }) {
            shownSectionId = section
        }
        guard let schemaId = remembered.schema,
              let schema = detail.schemas.first(where: { $0.id == schemaId }) else { return }
        // The selection wins over the section: it is what was being looked at.
        let section = detail.directory.sectionId(of: schema)
        if section != shownSectionKey, detail.sections.contains(where: { $0.id == section }) {
            shownSectionId = section
        }
        selectedSchemaId = schemaId
        if let fieldId = remembered.field,
           locateField(detail.schemas, id: fieldId)?.schema.id == schemaId {
            selectedFieldId = fieldId
        }
    }

    /// Projects whose exports are being written now: one write at a time for a project.
    @ObservationIgnored var exportsUnderWay: Set<String> = []
    /// The canvas of the card map shown (a flow's, a goal's, a container's of the project's own
    /// kind), for the menu bar's zoom and fit; nil with none shown (`DiagramFrame`).
    @ObservationIgnored weak var mapCanvas: CardCanvasController?
    /// The relation panel's canvas (`ContentView`'s): where a card a person puts in the section on
    /// screen is brought into view once it is drawn (`bringIntoView`).
    @ObservationIgnored weak var relationCanvas: CardCanvasController?
    /// The Rust core, opened on the projects' store on this Mac (`CoreClient`): what keeps this Mac's
    /// projects, and what answers MCP.
    @ObservationIgnored private(set) var localCore: CoreClient?
    /// The cores of the agents this app is connected to (`RemoteHosts`), by host: what keeps their
    /// projects.
    @ObservationIgnored private(set) var agentCores: [UUID: CoreClient] = [:]
    /// This Mac's projects: what MCP does not reach (an agent's own answers for its projects).
    var localProjects: [ProjectSummary] { projects.filter { $0.place == nil } }

    /// The agents connected, by host: their names, or their hosts as they are reached (`user@host`)
    /// when they have none (`RemoteHosts.Host.displayName`).
    private(set) var agentNames: [UUID: String] = [:]
    /// The agents whose holds of this Mac's were let go of this run.
    @ObservationIgnored private var released: Set<UUID> = []

    /// Where the project open is kept, as it is said: 「この Mac」, or the agent's host.
    var openPlaceName: String {
        guard let place = core?.place else { return L10n[.Common.wordThisMac] }
        return agentNames[place] ?? L10n[.Common.wordAgent]
    }

    /// The core that keeps the project open, which the window reads and writes it through.
    var core: CoreClient? { core(of: detail?.project.id) }

    /// The core that keeps the project `projectId`: this Mac's, or the agent's that keeps it (none while
    /// that agent is not connected). This Mac's for a project in no list.
    func core(of projectId: String?) -> CoreClient? {
        guard let projectId, let place = projects.first(where: { $0.id == projectId })?.place else { return localCore }
        return agentCores[place]
    }

    /// The core of a place: this Mac's for nil, else the agent's on that host.
    func core(at place: UUID?) -> CoreClient? {
        place.map { agentCores[$0] } ?? localCore
    }

    /// The projects `core` keeps, read again into the list beside the other places'.
    func readProjects(_ core: CoreClient) throws {
        let read = try core.projects()
        projects = projects.filter { $0.place != core.place } + read
    }

    /// The projects every access key of `core` may read, read again beside the other places'.
    private func readPublic(_ core: CoreClient) throws {
        let kept = Set(projects.filter { $0.place == core.place }.map(\.id))
        publicProjects = publicProjects.subtracting(kept).union(try core.publicProjects())
    }

    /// A project to open when the agent that keeps it is connected: the one open when the app last
    /// closed, or when its agent went. Opened if nothing else is open by then — underneath the project
    /// list, as a launch opens one, or shown, as it was when its agent went (`shown`).
    @ObservationIgnored private var awaited: (project: String, memory: SelectionMemory.Remembered?, shown: Bool)?

    func start() async {
        do {
            // The core's store, beside where the Swift app kept its own; a new one when there is
            // none.
            let swift = try databaseURL ?? CoreClient.swiftStoreURL()
            let core = try CoreClient(at: CoreClient.storeURL(beside: swift))
            self.localCore = core
            // What another hand wrote (MCP, on its own queue) shows here. The window's own
            // writes run on main and read the project again themselves.
            core.listen { [weak self] notice in
                guard !Thread.isMainThread else { return }
                DispatchQueue.main.async { MainActor.assumeIsolated { self?.heard(notice, from: nil) } }
            }
            // What the window held when it last closed is already one commit: let go.
            _ = try core.request("release_screen_holds")
            // How a line is drawn is its kind's definition's now: the colours and dashes this Mac
            // kept for the lines before are let go of.
            UserDefaults.standard.removeObject(forKey: "graph.edgeStyles")
            // A document's `![[…]]` drawn by the core, in the tree open (`embed_diagram`).
            NoteEmbed.source = { [weak self] project, path, option, section in
                self?.embedDiagram(project, path, option, section)
                    ?? .failure(NoteEmbed.Problem(why: L10n[.Docs.embedNoProject]))
            }
            // A document's `{{…}}` put in by the core, in the tree open (`inserted_values`).
            NoteValues.source = { [weak self] project, paths, section in
                self?.insertedValues(project, paths, section) ?? [:]
            }
            // What the cores said is said in the screen's language: read again when it changes.
            NotificationCenter.default.addObserver(forName: Language.changed, object: nil, queue: .main) { [weak self] _ in
                MainActor.assumeIsolated { self?.languageChanged() }
            }

            let summaries = try core.projects()
            projects = summaries
            publicProjects = (try? core.publicProjects()) ?? []
            // Read before anything is opened: opening a project writes the memory.
            let remembered = memory?.load()
            // A project an agent keeps is opened once that agent is connected (`agentConnected`); the
            // project list until then. One of this Mac's that is gone is not waited for.
            if let remembered, remembered.place != nil, !summaries.contains(where: { $0.id == remembered.project }) {
                awaited = (remembered.project, remembered, false)
            }
            let opening = summaries.first { $0.id == remembered?.project }
                ?? (awaited == nil ? summaries.max(by: { $0.schemaCount < $1.schemaCount }) : nil)
            if let opening {
                show(opening.id)
                if let remembered { restore(remembered) }
                surround()
            }
            // On the project list, the project remembered open underneath — but after 今すぐアップデート, on
            // what was being looked at (`SelectionMemory.takeReopen`). A model
            // with no memory (a test's) opens on the project, as it always has.
            if let memory, !memory.takeReopen() { atHome = true }
            awaited?.shown = !atHome
            phase = .ready
            // A link the app was opened by (`openLink`): opened now that the projects are read.
            if let link = pendingLink {
                pendingLink = nil
                openLink(link)
            }
        } catch {
            phase = .failed(String(describing: error))
        }
    }

    // MARK: - The inspection page

    /// A tree's inspection as the page last read it, and what it was read for: shown while a newer one is
    /// read, so that the page does not empty on each write.
    nonisolated struct InspectionRead: Sendable {
        nonisolated struct Key: Equatable, Sendable {
            var generation: Int
            var marks: Int
            /// The screen's language: the core says the groups and the problems in it.
            var language: String
        }
        var key: Key
        var list: Result<Inspection, InspectionRefusal>
    }

    /// The inspection of each tree the page has looked at, by project and tree (`inspectionSlot`).
    private(set) var inspections: [String: InspectionRead] = [:]
    /// The readings off the main thread waited for, by project and tree: what each is read for. One asked
    /// later lets an earlier one go, so what is put in is never older than what was asked last.
    @ObservationIgnored private var inspectionsAsked: [String: InspectionRead.Key] = [:]
    /// Bumped by each mark the window writes: a mark is not the tree's, so nothing else moves with it.
    @ObservationIgnored private var inspectionMarks = 0
    /// How many times the inspection was asked of the core on the main thread: none while the window
    /// reads off it (`readsOffMain`).
    @ObservationIgnored private(set) var inspectionsReadOnMain = 0

    private func inspectionSlot(_ tree: String) -> String? {
        detail.map { "\($0.project.id)|\(tree)" }
    }

    /// The tree open as the inspection names it (`branch`): main for main, and for main put right in
    /// one go under its lock, which a person sees as main.
    var inspectionTree: String {
        isLockedGroupOpen ? Self.groupBranchName : openTreeArgument["branch"] as? String ?? SchemaDirectory.mainWord
    }

    /// What the page has of `tree`'s inspection: maybe read for an earlier state of the project, while it
    /// is read again. Nil before it was first read.
    func inspection(of tree: String) -> InspectionRead? {
        inspectionSlot(tree).flatMap { inspections[$0] }
    }

    /// `tree`'s inspection read for what the project is now, unless it was (`inspection_list`, every item:
    /// the window pages nothing). Off the main thread when the window reads so (`readsOffMain`) — asking
    /// blocks the thread until the core answers, and the whole tree is checked for it — at once otherwise.
    /// A mark another hand writes is told as a change of the project: read again with it.
    func readInspection(_ tree: String) {
        guard let core, let projectId = detail?.project.id, let slot = inspectionSlot(tree) else { return }
        let key = InspectionRead.Key(generation: dataGeneration, marks: inspectionMarks, language: Language.shared.code)
        guard inspections[slot]?.key != key, inspectionsAsked[slot] != key else { return }
        let request: [String: Any] = ["name": "inspection_list",
                                      "arguments": ["project": projectId, "branch": tree, "value": ["show": "all"]]]
        guard readsOffMain else {
            inspectionsReadOnMain += 1
            inspections[slot] = InspectionRead(key: key, list: Inspection.read(Result { try core.reply("tool", request) }))
            return
        }
        inspectionsAsked[slot] = key
        let sent = Handed(value: request)
        let back = Handed(value: WeakModel(self))
        DispatchQueue.global(qos: .userInitiated).async {
            let list = Inspection.read(Result { try core.reply("tool", sent.value) })
            DispatchQueue.main.async {
                MainActor.assumeIsolated {
                    guard let model = back.value.model, model.inspectionsAsked[slot] == key else { return }
                    model.inspectionsAsked[slot] = nil
                    model.inspections[slot] = InspectionRead(key: key, list: list)
                }
            }
        }
    }

    /// Items of `tree` marked as by design, saying why — or their marks taken off (`reason` nil) — and the
    /// inspection read again. Why the core would not is the window's error, as for any write.
    func markInspection(_ ids: [String], reason: String?, in tree: String) {
        guard let core, let projectId = detail?.project.id, !ids.isEmpty else { return }
        let accept: Any = reason ?? false
        do {
            _ = try ownWrite {
                try core.ordered("tool", ["name": "inspection_update", "arguments": [
                    "project": projectId, "branch": tree, "target": ids, "value": ["accept": accept]]])
            }
            lastError = nil
        } catch {
            lastError = error.localizedDescription
        }
        inspectionMarks += 1
        readInspection(tree)
    }

    /// Where an item of `tree`'s inspection is, shown as a press on a card's name shows it — `tree` opened
    /// first when another is open, as a tree is opened from the nav panel, the place on screen kept for
    /// the item to be shown from. False when it is shown nowhere.
    @discardableResult
    func showInspected(_ item: Inspection.Item, in tree: String) -> Bool {
        if tree != inspectionTree {
            switch tree {
            case SchemaDirectory.mainWord, Self.groupBranchName: openBranch(nil)
            case Board.word: openBranch(Board.id)
            default:
                guard let branch = branches.first(where: { $0.name == tree }) else { return false }
                openBranch(branch.id)
            }
        }
        return show(path: item.place)
    }

    /// What `![[path]]` in a card of `section` draws, as the core draws it in the tree open
    /// (`embed_diagram`): the mermaid, a table's Markdown, or why it cannot be drawn.
    func embedDiagram(_ project: String, _ path: String, _ option: String?, _ section: String)
        -> Result<NoteEmbed.Drawn, NoteEmbed.Problem> {
        guard let core = core(of: project) else {
            return .failure(NoteEmbed.Problem(why: L10n[.Model.embedNotConnected]))
        }
        var request: [String: Any] = ["project": project, "branch": openTrees[project] ?? "", "path": path,
                                      "section": section, "exported": false]
        if let option { request["option"] = option }
        do {
            let answer = try core.request("embed_diagram", request) as? [String: Any]
            if let mermaid = answer?["mermaid"] as? String { return .success(.mermaid(mermaid)) }
            if let table = answer?["markdown"] as? String { return .success(.markdown(table)) }
            return .failure(NoteEmbed.Problem(why: answer?["problem"] as? String ?? L10n[.Model.embedCantDraw]))
        } catch {
            return .failure(NoteEmbed.Problem(why: error.localizedDescription))
        }
    }

    /// What the `{{path}}`s of a card of `section` put in, as the core puts them in in the tree
    /// open (`inserted_values`): the text of each path that puts one in.
    func insertedValues(_ project: String, _ paths: [String], _ section: String) -> [String: String] {
        guard let core = core(of: project) else { return [:] }
        let request: [String: Any] = ["project": project, "branch": openTrees[project] ?? "", "paths": paths,
                                      "section": section]
        let answer = (try? core.request("inserted_values", request)) as? [String: Any]
        return answer?["values"] as? [String: String] ?? [:]
    }

    /// The history of the project on screen, read again — after a write made outside the
    /// window, whose commit is made only as its transaction closes.
    func reloadHistory() {
        guard let core, let projectId = detail?.project.id else { return }
        let tree = openTrees[projectId]
        if let read = try? core.commits(projectId, branch: tree) {
            commits = read
            events = Self.events(of: read)
        }
        if let read = try? core.holds(projectId, branch: tree) { holds = read }
        if let read = try? core.mergeRequests(projectId) { mergeRequests = read }
    }

    /// The merge requests read again, for the tab that shows them (`RootView`): what is open, and what a
    /// write in main left stale. The rest of the window reads them with the whole project.
    func readMergeRequests() {
        guard let core, let projectId = detail?.project.id, let read = try? core.mergeRequests(projectId) else { return }
        if read != mergeRequests { mergeRequests = read }
    }

    /// The change history's lines, newest first, as far back as a hundred.
    private static func events(of commits: [CommitInfo]) -> [ChangeEvent] {
        Array(commits.prefix(100).flatMap { $0.lines.reversed() }.prefix(100))
    }

    /// The stop for a history no one cuts (`prune`): once a run, when the project is first
    /// opened.
    private func pruneOnce(_ projectId: String) {
        guard let core = core(of: projectId), pruned[projectId] == nil else { return }
        pruned[projectId] = ((try? core.request("prune", ["project": projectId])) as? [String: Any])?["pruned"] as? Int ?? 0
    }

    /// The project list over the whole window: where projects are chosen and kept. Left
    /// when one is opened from it; the open project stays open underneath.
    var atHome = false {
        didSet {
            if atHome != oldValue { reportWindows() }
            // Opened again, the home starts from where the open project is kept, and on its projects —
            // read again, as the writes in a project read only what they change in it.
            if atHome && !oldValue {
                for core in [localCore].compactMap({ $0 }) + Array(agentCores.values) { try? readProjects(core) }
                homeSettings = nil
                if let open = detail?.project.id, let at = projects.first(where: { $0.id == open }) {
                    homePlace = at.place
                }
            }
        }
    }

    /// The place the projects' home shows (nil: this Mac) and, when it shows that place's settings,
    /// which tab of them (nil: its projects) — what of a place the right of the home shows. The place
    /// is kept for the next launch.
    var homePlace: UUID? = AppModel.rememberedHomePlace() {
        didSet { UserDefaults.standard.set(homePlace?.uuidString ?? "", forKey: Self.homePlaceKey) }
    }
    var homeSettings: PlaceSettingsTab?
    private static let homePlaceKey = "home.place"
    private static func rememberedHomePlace() -> UUID? {
        UserDefaults.standard.string(forKey: homePlaceKey).flatMap(UUID.init(uuidString:))
    }

    /// A place's settings opened from elsewhere (the app settings' 「MCP」 goes to this Mac's access
    /// keys): the home, on that place and that tab.
    func openPlaceSettings(_ place: UUID?, _ tab: PlaceSettingsTab) {
        atHome = true
        homePlace = place
        homeSettings = tab
    }

    /// A link that came before the app had started (`openLink`), opened once it has.
    @ObservationIgnored var pendingLink: URL?

    func select(_ projectId: String) {
        show(projectId)
    }

    /// The project being opened from the projects' home, until it is: an agent's big project takes a
    /// while to read — 15 seconds for 3,000 cards — and the home says which is being opened while it
    /// is, rather than looking as though nothing was pressed.
    private(set) var opening: ProjectSummary?
    /// What the core said of the project being opened, asked off the main thread: read by `show`.
    @ObservationIgnored private var prefetched: (project: String, tree: String?, reply: String)?

    /// A project opened from the projects' home: what its core says of it asked off the main thread,
    /// the window answering meanwhile, and the project shown once it has been read; then `opened`.
    /// One at a time: pressed again — that one or another — while one is being opened, nothing more is
    /// opened.
    func open(fromHome projectId: String, opened: @escaping () -> Void = {}) {
        guard opening == nil else { return }
        guard projectId != detail?.project.id else {
            opened()
            return
        }
        guard let summary = projects.first(where: { $0.id == projectId }), let core = core(of: projectId) else {
            select(projectId)
            opened()
            return
        }
        opening = summary
        restoreBranch(projectId)
        let tree = openTrees[projectId]
        let prunes = pruned[projectId] == nil
        Task { [weak self] in
            // What the stop lets go of, once a run, before the project is read (`pruneOnce`).
            let (cut, reply) = await Task.detached { () -> (Int?, String?) in
                let cut = prunes ? ((try? core.request("prune", ["project": projectId])) as? [String: Any])?["pruned"] as? Int ?? 0 : nil
                return (cut, try? core.detailReply(projectId, branch: tree))
            }.value
            guard let self else { return }
            if let cut { self.pruned[projectId] = cut }
            if let reply { self.prefetched = (projectId, tree, reply) }
            self.show(projectId)
            self.prefetched = nil
            self.opening = nil
            opened()
        }
    }

    /// A change a core told of, made by another hand: the project on screen read again, and the
    /// list. Several told at once are read once. `place`: the core's (nil for this Mac's).
    private func heard(_ notice: String, from place: UUID?) {
        guard let told = try? JSONSerialization.jsonObject(with: Data(notice.utf8)) as? [String: Any] else { return }
        // An AI's merge let into a locked main without being approved: said above everything.
        if told["op"] as? String == "merged" {
            if let merge = AIMerge(told) { tell(merge) }
            return
        }
        guard told["op"] as? String == "changed" else { return }
        let first = rereads.isEmpty
        rereads.append((place, told["project"] as? String))
        guard first else { return }
        DispatchQueue.main.async { [weak self] in
            MainActor.assumeIsolated {
                guard let self else { return }
                let heard = self.rereads
                self.rereads = []
                for place in Set(heard.map(\.place)) {
                    if let core = self.core(at: place) { try? self.readProjects(core) }
                }
                self.noticesHeard += 1
                if let id = self.detail?.project.id, let core = self.core(of: id),
                   heard.contains(where: { $0.place == core.place && ($0.project == nil || $0.project == id) }) {
                    self.showLater(id)
                }
            }
        }
    }

    /// The screen's language changed: what the cores said in the one before read again in it — the lists,
    /// what pages read from a core themselves (`noticesHeard`), and the project on screen whole (what was
    /// held of each tree let go, so that nothing is put on words of the language before).
    func languageChanged() {
        let cores = [localCore].compactMap { $0 } + Array(agentCores.values)
        for core in cores {
            core.held.clear()
            core.workdirs.clear()
            try? readProjects(core)
        }
        noticesHeard += 1
        if let id = detail?.project.id { showLater(id) }
    }

    /// What was told and not yet read again: the place told of, and the project (nil: any).
    @ObservationIgnored private var rereads: [(place: UUID?, project: String?)] = []
    /// Bumped each time what the cores told of has been read again: what a page reads from a core
    /// itself — the access keys, which project each may touch — it reads again with it. A project
    /// made through MCP is told of, and given to the key that made it, as one write.
    private(set) var noticesHeard = 0

    /// An agent connected (`RemoteHosts`): its projects in the list, what it tells heard, and the project
    /// awaited opened when it keeps it and nothing else is open.
    func agentConnected(_ core: CoreClient, at name: String) {
        guard let place = core.place else { return }
        agentCores[place]?.close()
        agentCores[place] = core
        agentNames[place] = name
        core.listen { [weak self] notice in
            DispatchQueue.main.async { MainActor.assumeIsolated { self?.heard(notice, from: place) } }
        }
        // `app_control` on the agent's projects, handed back for this app's window.
        core.answerAppControl(AppControl(model: self, place: place, core: core))
        reportedWindows[place] = nil
        do {
            // What this Mac's window held there when the app last closed: let go, once a run (a
            // connection lost and back is the same run, its group still the window's).
            if released.insert(place).inserted { _ = try core.request("release_screen_holds") }
            try readProjects(core)
            try readPublic(core)
        } catch {
            lastError = error.localizedDescription
        }
        defer { reportWindows() }
        guard let awaited, projects.contains(where: { $0.id == awaited.project && $0.place == place }) else { return }
        self.awaited = nil
        guard detail == nil else { return }
        show(awaited.project)
        if let memory = awaited.memory { restore(memory) }
        surround()
        if awaited.shown { atHome = false }
    }

    /// What this app's window shows of each agent's projects, told to that agent when it changes:
    /// what its MCP's `app_control` can be aimed at. Nothing while this app takes no
    /// `app_control` from the agents (`AppControl.takesFromAgents`), nor the project list over the window.
    func reportWindows() {
        for (place, core) in agentCores {
            var windows: [[String: Any]] = []
            if AppControl.takesFromAgents, let detail, !atHome, self.core(of: detail.project.id)?.place == place {
                windows = [[
                    "window": 1, "project": detail.project.id, "branch": openTree.name,
                    "section": shownSectionPath ?? NSNull(),
                    "card": selectedSchema.map { detail.directory.full($0) } ?? NSNull(),
                    "front": isFront(),
                ]]
            }
            let said = (try? JSONSerialization.data(withJSONObject: windows, options: [.sortedKeys]))
                .map { String(decoding: $0, as: UTF8.self) } ?? ""
            guard reportedWindows[place] != said else { continue }
            reportedWindows[place] = said
            core.tellWindows(windows)
        }
    }
    /// What each agent was told of the window last.
    @ObservationIgnored private var reportedWindows: [UUID: String] = [:]
    /// Whether the window is in front (the app active): the window's to say (`ContentView`), which
    /// tells the agents again as it changes.
    @ObservationIgnored var isFront: () -> Bool = { true }

    /// An agent given another name here:
    /// what the list, the moves and the notices call it from now on, while it is connected.
    func agentRenamed(_ place: UUID, to name: String) {
        guard agentNames[place] != nil else { return }
        agentNames[place] = name
    }

    /// An agent gone — its connection lost, or let go: its projects out of the list, and the project open
    /// from it closed, to be opened again when it is back.
    func agentGone(_ place: UUID, said: String) {
        guard agentCores.removeValue(forKey: place) != nil else { return }
        agentNames[place] = nil
        reportedWindows[place] = nil
        let open = detail?.project.id
        let wasOpen = open.map { id in projects.contains { $0.id == id && $0.place == place } } ?? false
        projects.removeAll { $0.place == place }
        guard wasOpen, let open else { return }
        // Where it was looked at, as the memory kept it, to go back to — kept for the next launch
        // too (closing the project lets the memory go).
        let kept = memory?.load().flatMap { $0.project == open ? $0 : nil }
        awaited = (open, kept, !atHome)
        resultView = nil
        detail = nil
        outsideDetail = nil
        events = []
        if let kept { memory?.save(kept) }
        atHome = true
        notice = said.isEmpty ? L10n[.Model.noticeAgentLost] : L10n[.Model.noticeAgentLostWhy(said)]
    }

    private func show(_ projectId: String) {
        guard let core = core(of: projectId) else { return }
        working(.reading) {
            // A spotlight names schemas in one project; carrying it across would hide
            // every card in the next one. A section is one project's too, and so is the
            // way back.
            restoreBranch(projectId)
            // Another project, or the board and back: the tab bar of the set left kept, the other's put
            // back once it is read (`tabSet`).
            let set = tabSet(projectId)
            let anotherSet = set != tabsSet
            if anotherSet {
                saveTabs()
                tabs = []
                activeTab = nil
                mapHeld = nil
                tabsSet = set
            }
            if detail?.project.id != projectId {
                // What a merge would leave, looked at in the project before: let go with it.
                if let shown = resultView, let before = detail?.project.id {
                    resultView = nil
                    openTrees[before] = shown.back
                    try? core.closeMergeResult(before)
                }
                shownSectionId = nil
                backward = []
                forward = []
                rowsDraft = nil
                afterReading = []
            }
            pruneOnce(projectId)
            // What is read here is newer than any reading still on its way: that one is let go.
            pending = nil
            showActivity()
            let asked = wholeAsk(projectId)
            if prefetched?.project == projectId { prefetched = nil }
            do {
                putWhole(try Self.readWhole(core, asked), core)
                if anotherSet { restoreTabs(set) }
            } catch {
                failedReading(error, core)
            }
        }
    }

    // MARK: - Reading the project again

    /// What is asked to read the whole of a project: the tree open, the trees kept open though they
    /// are no branch (`branches` does not list them), and what was read of it off the main thread
    /// already, when it was opened from the projects' home.
    nonisolated struct WholeAsk: Sendable {
        var project: String
        var tree: String?
        var kept: Set<String>
        var prefetched: (tree: String?, reply: String)?
    }

    /// The whole of a project as it is read (`show`): everything the window holds of it.
    nonisolated struct WholeRead: @unchecked Sendable {
        var project: String
        var tree: String?
        /// The tree open is no longer there: main is read instead.
        var treeGone: Bool
        var branches: [BranchInfo]
        var detail: ProjectDetail
        var outside: ProjectDetail?
        var marks: BranchMarks
        var commits: [CommitInfo]
        var holds: [Hold]
        var mergeRequests: [MergeRequest]
        var workDirs: [WorkDir]
        var writingRules: [String: String]?
        var layout: LayoutSettings
        var positions: [String: [String: CGPoint]]
        /// What was held of the tree, or of what it reaches beside it, was found not to be what the core has,
        /// and read again whole (`CoreClient.readDetail`).
        var drifted = false
    }

    /// What a write in a tree changes, as it is read after the write (`readWritten`): the tree's cards and
    /// sections, what of them a branch changed, the boxes' places, its history, and the working directories
    /// as the tree's reading put them in (a write may read one again or change how it is read). A write
    /// changes no branch, request, export, rule, layout or hold: those are read with the whole.
    nonisolated struct WrittenRead: @unchecked Sendable {
        var project: String
        var tree: String?
        var detail: ProjectDetail
        var marks: BranchMarks
        var commits: [CommitInfo]
        var positions: [String: [String: CGPoint]]
        var workDirs: [WorkDir]
        /// What was held of the tree was found not to be what the core has, and read again whole.
        var drifted = false
    }

    /// Said in the status bar when what the window held of a project was found not to be what the core has,
    /// and the whole was read again.
    static var driftSaid: String { L10n[.Model.statusDrifted] }

    private func wholeAsk(_ projectId: String) -> WholeAsk {
        let tree = openTrees[projectId]
        var kept: Set<String> = [Board.id]
        if let branch = resultView?.branch { kept.insert(branch) }
        if let branch = groupBranches[projectId] { kept.insert(branch) }
        let read = prefetched.flatMap { $0.project == projectId ? (tree: $0.tree, reply: $0.reply) : nil }
        return WholeAsk(project: projectId, tree: tree, kept: kept, prefetched: read)
    }

    /// The whole of a project, read from its core: on any thread.
    nonisolated static func readWhole(_ core: CoreClient, _ asked: WholeAsk) throws -> WholeRead {
        let project = asked.project
        let branches = try core.branches(project)
        var tree = asked.tree
        var treeGone = false
        if let open = tree, !asked.kept.contains(open), !branches.contains(where: { $0.id == open }) {
            tree = nil
            treeGone = true
        }
        // Asked from what is held of the tree: what changed since, unless it was asked off the main thread as
        // the project was opened.
        let read = try asked.prefetched.flatMap { $0.tree == tree ? $0.reply : nil }
            .map { try core.detailRead(reading: $0, project, branch: tree) } ?? core.readDetail(project, branch: tree)
        // What the board's lines reach, read beside it: main's cards.
        let outside = tree == Board.id ? try? core.readDetail(project, branch: nil) : nil
        return WholeRead(project: project, tree: tree, treeGone: treeGone, branches: branches, detail: read.detail,
                         outside: outside?.detail,
                         marks: try core.branchMarks(project, branch: tree, kinds: read.detail.cardKinds),
                         commits: try core.commits(project, branch: tree),
                         holds: try core.holds(project, branch: tree),
                         mergeRequests: try core.mergeRequests(project),
                         workDirs: Self.workDirs(core, project),
                         writingRules: try? core.writingRules(project),
                         layout: try core.layout(project),
                         positions: try core.sectionPositions(project, branch: tree),
                         drifted: read.drifted || outside?.drifted == true)
    }

    /// The project's working directories as set and as read on the machine its project is kept on, as the
    /// tree's reading put them in it (`CoreClient.joined`): nothing is asked again for them.
    nonisolated static func workDirs(_ core: CoreClient, _ project: String) -> [WorkDir] {
        core.workdirs.get(project)?.dirs ?? []
    }

    /// What a write in `tree` changes, read from the core: on any thread.
    nonisolated static func readWritten(_ core: CoreClient, project: String, tree: String?) throws -> WrittenRead {
        let read = try core.readDetail(project, branch: tree)
        return WrittenRead(project: project, tree: tree, detail: read.detail,
                           marks: try core.branchMarks(project, branch: tree, kinds: read.detail.cardKinds),
                           commits: try core.commits(project, branch: tree),
                           positions: try core.sectionPositions(project, branch: tree),
                           workDirs: workDirs(core, project),
                           drifted: read.drifted)
    }

    /// The whole of a project put in the model, as it was read.
    private func putWhole(_ read: WholeRead, _ core: CoreClient) {
        let started = ContinuousClock.now
        let projectId = read.project
        if read.treeGone { openTrees[projectId] = nil }
        branches = read.branches
        detail = read.detail
        outsideDetail = read.outside
        branchMarks = read.marks
        // The section gone from the tree now open: the first one under its root, as the map tab's would.
        if let shownSectionId, !(detail?.sections.contains { $0.id == shownSectionId } ?? false) {
            self.shownSectionId = firstSectionKey
        }
        commits = read.commits
        events = Self.events(of: commits)
        holds = read.holds
        mergeRequests = read.mergeRequests
        // What the board reaches and what the files carry out are read again, as they are asked for.
        asideBefore.merge(asideKept) { _, kept in kept }
        asideKept = [:]
        asideAsked = []
        staleOutlines()
        workDirs = read.workDirs
        writingRules = read.writingRules ?? [:]
        readExportSettings()
        defaultSettings = read.layout
        settle(projectId, read.tree, positions: read.positions)
        if read.drifted { say(Self.driftSaid) }
        Self.said("put whole", since: started)
    }

    /// What a write changed put in the model, as it was read.
    private func putWritten(_ read: WrittenRead) {
        let started = ContinuousClock.now
        letGoOfCardActions()
        detail = read.detail
        staleOutlines()
        workDirs = read.workDirs
        branchMarks = read.marks
        if let shownSectionId, !(detail?.sections.contains { $0.id == shownSectionId } ?? false) {
            self.shownSectionId = nil
        }
        commits = read.commits
        events = Self.events(of: commits)
        settle(read.project, read.tree, positions: read.positions)
        if read.drifted { say(Self.driftSaid) }
        Self.said("put written", since: started)
    }

    /// What the window works out from a project read, and what was picked in it let go where it is gone.
    private func settle(_ projectId: String, _ tree: String?, positions: [String: [String: CGPoint]]) {
        let measuring = ContinuousClock.now
        measureCards()
        Self.said("measure cards", since: measuring)
        placeUnplaced()
        placeUnplacedOnGoalMaps(projectId, branch: tree)
        sectionPositions = positions
        let surrounding = ContinuousClock.now
        surround()
        Self.said("surround", since: surrounding)
        dataGeneration += 1
        if let selectedSchemaId, schema(selectedSchemaId) == nil, ghost(selectedSchemaId) == nil {
            self.selectedSchemaId = nil
            selectedFieldId = nil
            selectedRow = nil
        }
        // A line taken off, or its card gone: nothing picked.
        if let selectedLine, line(selectedLine) == nil { self.selectedLine = nil }
        // A card or root that went: its tab with it.
        pruneTabs()
        reportWindows()
        let then = afterReading
        afterReading = []
        then.forEach { $0() }
    }

    /// How long putting what was read in the model took, said in the log beside the requests.
    private static func said(_ what: String, since started: ContinuousClock.Instant) {
        let took = ContinuousClock.now - started
        let seconds = Double(took.components.seconds) + Double(took.components.attoseconds) / 1e18
        CoreClient.log.notice("\(what, privacy: .public) \(seconds, format: .fixed(precision: 3), privacy: .public)s")
    }

    /// A reading that failed: an agent's way lost on the way is said, and the project closes when the way is
    /// let go (`agentGone`); this Mac's store failing is the window's end.
    private func failedReading(_ error: Error, _ core: CoreClient) {
        if core.isAgent { lastError = error.localizedDescription } else { phase = .failed(String(describing: error)) }
    }

    /// What a write in the project open changed, read again: off the main thread when the window reads so
    /// (`readsOffMain`), and put in once it is in — `then` after it; at once otherwise, or when the caller
    /// needs it before it goes on (`now`).
    func readWritten(_ projectId: String, now: Bool = false, then: (() -> Void)? = nil) {
        guard let core = core(of: projectId) else { return }
        let tree = openTrees[projectId]
        if let then { afterReading.append(then) }
        guard readsOffMain, !now else {
            pending = nil
            working(.reading) {
                do {
                    putWritten(try Self.readWritten(core, project: projectId, tree: tree))
                } catch {
                    failedReading(error, core)
                }
            }
            return
        }
        begin(PendingReading(core: core, project: projectId, tree: tree) {
            try .written(Self.readWritten(core, project: projectId, tree: tree))
        })
    }

    /// The whole of the project open read again off the main thread (another hand's change), put in
    /// once it is in; at once when the window does not read so.
    private func showLater(_ projectId: String) {
        guard readsOffMain, let core = core(of: projectId), detail?.project.id == projectId else {
            show(projectId)
            return
        }
        let asked = wholeAsk(projectId)
        begin(PendingReading(core: core, project: projectId, tree: asked.tree) { try .whole(Self.readWhole(core, asked)) })
    }

    /// A reading begun in the background, waited for: the status bar says so, and what a person does
    /// meanwhile is held (`InputHold`) — done once it is in, on what was read, as it would have been had
    /// the window waited on the main thread. Read on a queue of its own rather than Swift's own threads:
    /// asking the core blocks the thread that asks until it answers (an agent's way, the store's lock).
    private func begin(_ reading: PendingReading) {
        pending = reading
        showActivity()
        let back = Handed(value: WeakModel(self))
        DispatchQueue.global(qos: .userInitiated).async {
            reading.read = Result { try reading.make() }
            DispatchQueue.main.async {
                MainActor.assumeIsolated { back.value.model?.take(reading) }
            }
        }
    }

    /// A reading still on its way when a write comes: let go — what it brings is dropped — and read again
    /// at once, on the main thread, so that the write works from what the window holds. The main thread
    /// never waits for what is done off it: what is done there may itself wait for the main thread.
    private func settle() {
        guard let reading = pending else { return }
        pending = nil
        showActivity()
        guard detail?.project.id == reading.project, openTrees[reading.project] == reading.tree else {
            afterReading = []
            return
        }
        working(.reading) { put(Result { try reading.make() }, reading) }
    }

    /// A reading in the background is in: put in the model when it is still the one waited for, of the
    /// project and tree still open — else let go, as one begun later (or read at once) has it.
    private func take(_ reading: PendingReading) {
        guard pending === reading, let read = reading.read else { return }
        pending = nil
        showActivity()
        guard detail?.project.id == reading.project, openTrees[reading.project] == reading.tree else {
            afterReading = []
            return
        }
        put(read, reading)
    }

    private func put(_ read: Result<PendingReading.Read, Error>, _ reading: PendingReading) {
        switch read {
        case .success(.whole(let whole)): putWhole(whole, reading.core)
        case .success(.written(let written)): putWritten(written)
        case .failure(let error): failedReading(error, reading.core)
        }
    }

    // MARK: - The status bar

    /// Something the window does on the main thread and waits for — a write, a reading — said in the
    /// status bar while it lasts. The window does not draw while the main thread is busy, so the bar's
    /// line is drawn at once before it starts (`StatusLineView.drawNow`).
    func working<T>(_ what: Activity, _ body: () throws -> T) rethrows -> T {
        let outer = busy
        busy = what
        showActivity()
        statusLine?.drawNow()
        defer {
            busy = outer
            showActivity()
        }
        return try body()
    }

    /// What the window waits for, put in the status bar: what it does now, else a reading in the background.
    /// While a reading is on its way, what a person does is held.
    private func showActivity() {
        let now = busy ?? (pending == nil ? nil : .reading)
        if activity != now { activity = now }
        statusLine?.show(activity: now, said: said)
        inputHold.holding = pending != nil
    }

    /// Something said in the status bar for a few seconds, then taken away — what the window did by
    /// itself and a person may want to know of.
    func say(_ text: String, for seconds: Double = 4) {
        sayingNumber += 1
        let number = sayingNumber
        said = text
        statusLine?.show(activity: activity, said: text)
        Task { [weak self] in
            try? await Task.sleep(for: .seconds(seconds))
            guard let self, self.sayingNumber == number else { return }
            self.said = nil
            self.statusLine?.show(activity: self.activity, said: nil)
        }
    }

    /// Selecting one in another section goes there: the diagram only draws one.
    /// Not when it was picked on the diagram (`follow: false`) — that is another
    /// section's card drawn here, and the point of drawing it is to stay.
    func selectSchema(_ id: String, follow: Bool = true) {
        if follow { self.follow(id) }
        selectedSchemaId = id
        selectedFieldId = nil
        selectedRow = nil
        selectedLine = nil
    }

    /// Nothing picked on the diagram (an empty spot pressed): the property panel shows
    /// the section open.
    func deselect() {
        selectedSchemaId = nil
        selectedFieldId = nil
        selectedRow = nil
        selectedLine = nil
        pickedGone = nil
    }

    /// What the open branch removed, picked (a card, a card ghost, a node or a section, by its id): shown
    /// read only in the property panel, nothing else picked.
    func pickGone(_ id: String) {
        selectedSchemaId = nil
        selectedFieldId = nil
        selectedRow = nil
        selectedLine = nil
        pickedGone = id
    }

    /// What is picked of what the open branch removed, as main held it.
    enum GonePicked: Equatable {
        case card(SchemaNode)
        case ghost(CardGhost)
        /// A flow's node, with the flow it was in.
        case node(InnerCard, flow: SchemaNode?)
        case section(ProjectSection)
    }

    var goneShown: GonePicked? {
        guard let id = pickedGone else { return nil }
        let gone = branchMarks.gone
        if let card = gone.cards.first(where: { $0.id == id }) { return .card(card) }
        if let ghost = gone.ghosts.first(where: { $0.id == id }) { return .ghost(ghost) }
        if let node = gone.nodes.first(where: { $0.id == id }) { return .node(node, flow: schema(node.container)) }
        if let section = gone.sections.first(where: { $0.id == id }) { return .section(section) }
        return nil
    }

    /// A line picked (overview §13): on a diagram, or in the property panel's つながり. What
    /// else was picked goes — the panel shows the line, and from there its two ends.
    func selectLine(_ ref: LineRef) {
        selectedSchemaId = nil
        selectedFieldId = nil
        selectedRow = nil
        selectedLine = ref
    }

    private func follow(_ schemaId: String) {
        guard let detail, let schema = schema(schemaId) else { return }
        let section = detail.directory.sectionId(of: schema)
        if section != shownSectionKey { selectSection(section) }
    }

    // MARK: - Nesting

    /// Every card holds its value objects and enums in place: see `Nesting.always`.
    var nesting: Nesting { .always }

    func selectField(_ schemaId: String, _ fieldId: String, follow: Bool = true) {
        if follow { self.follow(schemaId) }
        selectedSchemaId = schemaId
        selectedFieldId = fieldId
        selectedRow = nil
        selectedLine = nil
    }

    /// A row of a card that is not a field — an enum's value, a store's hold.
    enum CardRow: Hashable {
        case value(String)
        case hold(Int)
        /// A note's clause, by its dotted path in the note (`auth.login`).
        case formatRow(String)
        /// A flow's node, by its name.
        case node(String)
        /// A row of a flow's process node: the node's name, and the row as
        /// `StepEditing` writes its path (`#p.0`).
        case nodeStep(String, String)
        /// A custom card's row, by its path of names (`t1.t3`, `specboard://SpecBoard/Docs/Design:DesignBoard` §3).
        case customRow(String)
        /// A function or type of a working directory's file, by its path in the file (`Order.total`).
        case symbol(String)
    }

    /// The row picked in a card editor, edited under it.
    private(set) var selectedRow: CardRow?

    func selectRow(_ schemaId: String, _ row: CardRow) {
        selectedSchemaId = schemaId
        selectedFieldId = nil
        selectedRow = row
        selectedLine = nil
    }

    /// Nothing inside the card picked: no row's editor is open.
    func clearRow() {
        selectedFieldId = nil
        selectedRow = nil
    }

    // MARK: - Where the window is (overview §13)

    /// The views of main's or a branch's root.
    enum RootTab: String, CaseIterable, Hashable {
        /// The board's goals: the board's alone, where it opens.
        case goals
        case history, diff, mr, tags

        /// What the tab says, in the screen's language.
        var title: String {
            switch self {
            case .goals: L10n[.Root.tabGoals]
            case .history: L10n[.Root.tabHistory]
            case .diff: L10n[.Root.tabDiff]
            case .mr: "MR"
            case .tags: L10n[.Root.tabTags]
            }
        }
    }

    /// The tabs the root of the tree open has, in their order, the one it opens on first: main is
    /// looked at for what was done in it, a branch for how it stands against main. PRs are main's:
    /// requests to merge into it (`specboard://SpecBoard/Docs/Design:DesignBranches` §8). The board has no
    /// main to differ from, nor PRs (`specboard://SpecBoard/Docs/Design:DesignBoard` §2).
    var rootTabs: [RootTab] {
        if isBoardOpen { return [.goals, .history, .tags] }
        if openBranch != nil { return [.diff, .history, .tags] }
        return [.history, .diff, .mr, .tags]
    }

    /// A root opened, on the first of its tabs.
    func rootPlace(_ id: String) -> Place {
        .root(id, rootTabs.first ?? .history)
    }

    /// The views of the project's settings: the project, its rules for each place, its kinds and its
    /// tree, where it is written out, and what MCP may write in it.
    enum SettingsTab: String, CaseIterable, Hashable {
        case project, rules, kinds, exports, mcp

        /// What the tab says, in the screen's language.
        var title: String {
            switch self {
            case .project: L10n[.ProjectSettings.tabProject]
            case .rules: L10n[.ProjectSettings.tabRules]
            case .kinds: L10n[.ProjectSettings.tabKinds]
            case .exports: L10n[.ProjectSettings.tabExports]
            case .mcp: "MCP"
            }
        }
    }

    /// Somewhere the window can be taken to, and taken back to.
    enum Place: Hashable {
        /// A section's cards.
        case section(String)
        /// One card, to read and write.
        case card(String)
        /// Main's or a branch's root, on one of its views.
        case root(String, RootTab)
        /// The project's settings, on one of its views — the project's, whatever branch is open.
        case settings(SettingsTab)
        /// One card's connections diagram (`ConnectionsMap`), as far as it reaches.
        case connections(String, ConnectionDepth)
        /// The inspection of a tree (`InspectionView`): the project's, whatever tree is open, the tree
        /// looked at picked on the page.
        case inspection

        /// Whether two places are one tab: a root's dashboard, the settings and a card's connections
        /// looked at another way are still the same thing.
        func sameTab(_ other: Place) -> Bool {
            switch (self, other) {
            case (.section, .section), (.settings, .settings), (.inspection, .inspection): true
            case (.card(let one), .card(let two)), (.root(let one, _), .root(let two, _)),
                 (.connections(let one, _), .connections(let two, _)): one == two
            default: false
            }
        }

        /// As the tab bar is kept on this Mac (`SelectionMemory.Tabs`).
        var saved: SelectionMemory.SavedPlace {
            switch self {
            case .section(let id): .init(kind: "section", id: id)
            case .card(let id): .init(kind: "card", id: id)
            case .root(let id, let tab): .init(kind: "root", id: id, view: tab.rawValue)
            case .settings(let tab): .init(kind: "settings", view: tab.rawValue)
            case .connections(let id, let depth): .init(kind: "connections", id: id, depth: depth.rawValue)
            case .inspection: .init(kind: "inspection")
            }
        }

        /// A place as the tab bar was kept; nil for one this version does not know.
        init?(saved: SelectionMemory.SavedPlace) {
            switch (saved.kind, saved.id) {
            case ("section", let id?): self = .section(id)
            case ("card", let id?): self = .card(id)
            case ("root", let id?):
                guard let tab = saved.view.flatMap(RootTab.init(rawValue:)) else { return nil }
                self = .root(id, tab)
            case ("settings", _):
                guard let tab = saved.view.flatMap(SettingsTab.init(rawValue:)) else { return nil }
                self = .settings(tab)
            case ("connections", let id?):
                guard let depth = saved.depth.flatMap(ConnectionDepth.init(rawValue:)) else { return nil }
                self = .connections(id, depth)
            case ("inspection", _): self = .inspection
            default: return nil
            }
        }
    }

    /// Whether a ui section's map lays each part's tag over it in this window (the bar's 「見出し」): on until
    /// it is turned off.
    var partTags = true

    /// Whether a ui section's map lays the parts' notes over them in this window (the bar's 「ノート」): on until
    /// it is turned off. The review dots stay either way — they are not notes.
    var partNotes = true

    /// The tabs after the map tab, in the tab bar's order (`specboard://SpecBoard/Docs/Design:UI` §4): one
    /// for each thing opened (`Place.sameTab`), never a section's map — that is the map tab's, always
    /// first.
    private(set) var tabs: [Place] = []
    /// The tab in front, by its index in `tabs`; nil: the map tab.
    private(set) var activeTab: Int?
    /// The section the map tab holds while another tab is in front, shown again when it comes forward.
    /// In front, the map tab shows the section on screen.
    private var mapHeld: String?
    /// The way back, and the way forward again after going back: one for all the tabs, so back is
    /// always what was seen just before. The open project's only: another project starts them empty.
    private(set) var backward: [Place] = []
    private(set) var forward: [Place] = []

    /// Where the window is: the tab in front.
    var here: Place { activeTab.map { tabs[$0] } ?? .section(shownSectionKey) }

    /// The section the map tab shows: the section on screen while it is in front, else the one it
    /// holds — that gone, the first section under the root.
    var mapSectionKey: String {
        guard activeTab != nil else { return shownSectionKey }
        if let mapHeld, detail?.sections.contains(where: { $0.id == mapHeld }) == true { return mapHeld }
        return firstSectionKey
    }

    /// The first section under the root of the tree open (the root itself when it has none): where the
    /// map tab goes when the section it showed is not in the tree.
    private var firstSectionKey: String {
        guard let directory = detail?.directory else { return "" }
        return directory.children(of: directory.home).first?.id ?? directory.home
    }

    /// The card open to read and write, if one is.
    var openCard: SchemaNode? {
        guard case .card(let id) = here else { return nil }
        return schema(id)
    }

    var canGoBack: Bool { !backward.isEmpty }
    var canGoForward: Bool { !forward.isEmpty }

    /// Open a section, a card or a root, and remember where the window was. A frame opens on the ui
    /// section it holds: where it points is all it has to show. A section opens on the map tab; anything
    /// else in its own tab, the one it already has or a new one right of the tab in front.
    func open(_ target: Place) {
        if case .card(let id) = target, let section = frameTarget(of: id) {
            open(.section(section))
            return
        }
        let from = here
        guard let arrived = arrive(at: target), arrived != from else { return }
        backward.append(from)
        forward.removeAll()
    }

    /// A section picked in the nav panel: opened — a root on its dashboard — with
    /// nothing in it picked. A card left picked, the one its editor showed in the same
    /// section, would keep the nav panel's highlight on its own row.
    func pick(section id: String, root: Bool) {
        open(root ? rootPlace(id) : .section(id))
        deselect()
    }

    /// The tab at `index` in `tabs` closed — the one in front when none is named. One behind the front
    /// one goes alone. The front one gives way to the tab on its left, as a step to go back over (back
    /// opens it again); when that is the map tab after a card or a card's connections, the map shows the
    /// card's section with the card picked — the view where it was, moved only when the card is not on
    /// screen. The map tab never closes.
    func closeTab(_ index: Int? = nil) {
        guard let at = index ?? activeTab, tabs.indices.contains(at) else { return }
        guard at == activeTab else {
            tabs.remove(at: at)
            if let front = activeTab, front > at { activeTab = front - 1 }
            saveTabs()
            return
        }
        let from = here
        let map = mapSectionKey
        tabs.remove(at: at)
        // The tab on the left, in front for now: arriving there finds it, and the map tab keeps what it holds.
        activeTab = at > 0 ? at - 1 : nil
        let card: SchemaNode? = switch from {
        case .card(let id), .connections(let id, _): schema(id)
        default: nil
        }
        let arrived: Place?
        if let left = activeTab {
            arrived = arrive(at: tabs[left])
        } else if let card, let detail {
            arrived = arrive(at: .section(detail.directory.sectionId(of: card)))
            selectSchema(card.id, follow: false)
            revealRequest = card.id
        } else {
            arrived = arrive(at: .section(map))
        }
        saveTabs()
        guard let arrived, arrived != from else { return }
        backward.append(from)
        forward.removeAll()
    }

    /// Every tab but the map tab and the one in front closed (the tab bar's 「» N」).
    func closeOtherTabs() {
        tabs = activeTab.map { [tabs[$0]] } ?? []
        activeTab = activeTab == nil ? nil : 0
        saveTabs()
    }

    /// The card a connections diagram is of, when one is on screen.
    var connectionsCard: SchemaNode? {
        guard case .connections(let id, _) = here else { return nil }
        return schema(id)
    }

    /// A card's connections diagram, as far as `depth`: a step to go back
    /// over, as a card opened is.
    func openConnections(_ id: String, depth: ConnectionDepth = .one) {
        open(.connections(id, depth))
    }

    /// The same diagram reaching another way: not a step to go back over, as a root's tabs.
    func showConnections(_ depth: ConnectionDepth) {
        guard let at = activeTab, case .connections(let id, _) = tabs[at] else { return }
        tabs[at] = .connections(id, depth)
        saveTabs()
    }

    /// Back to where the window was before. What is no longer there is passed over.
    func goBack() {
        let from = here
        while let last = backward.popLast() {
            if let arrived = arrive(at: last), arrived != from {
                forward.append(from)
                return
            }
        }
    }

    func goForward() {
        let from = here
        while let next = forward.popLast() {
            if let arrived = arrive(at: next), arrived != from {
                backward.append(from)
                return
            }
        }
    }

    /// A path as the project writes it, opened where it lands: its card, and the field
    /// when it names one. False when it lands nowhere. `from`: the section it was
    /// written in — a branch's names are read in the branch's tree.
    @discardableResult
    func open(path: String, from base: String? = nil) -> Bool {
        guard let detail else { return false }
        // A section named alone (`Store/Ordering:`): its map, a root its own view; main's from the
        // board, in main.
        if let name = PathText.sectionOnly(path) {
            let directory = detail.directory
            if let found = base.flatMap({ directory.section(named: name, from: $0) }) ?? directory.section(named: name) {
                open(found.parentId == nil ? rootPlace(found.id) : .section(found.id))
                return true
            }
            // A folder of a working directory not read here: the working directory's page, which says why.
            if let mount = unreadMount(detail, path) {
                open(.section(mount))
                return true
            }
            guard outsideDetail.map({ $0.directory.section(named: name) != nil || unreadMount($0, path) != nil }) == true
            else { return false }
            openBranch(nil)
            return open(path: path)
        }
        switch try? MCPViews.locate(detail, path, base: base) {
        case .schema(let card)?:
            open(.card(card.id))
        case .field(let card, let field, _)?:
            open(.card(card.id))
            selectField(card.id, field.id, follow: false)
        case .formatRow(let note, let names)?:
            open(.card(note.id))
            selectRow(note.id, .formatRow(PathText.joinNames(names)))
        case .node(let flow, let node)?:
            open(.card(flow.id))
            selectNode(flow.id, node.name)
        case .customRow(let card, let names)?:
            open(.card(card.id))
            selectRow(card.id, .customRow(CardWalk.rowPath(names, in: card)))
        case nil:
            // A function or a type in a working directory's file: the file opened at it.
            if let (file, symbol) = fileSymbol(detail, path, base: base) {
                openSymbol(file, symbol)
                return true
            }
            // A file of a working directory not read here: the working directory's page, which says why.
            if let mount = unreadMount(detail, path) {
                open(.section(mount))
                return true
            }
            // A place of the tree beside this one (main's, written on the board): opened there.
            if inMain(path) || outsideDetail.map({ fileSymbol($0, path, base: nil) != nil || unreadMount($0, path) != nil }) == true {
                openBranch(nil)
                return open(path: path)
            }
            // A card ghost that leads nowhere — a broken path of the dashboard's — is picked
            // on its section's diagram.
            let directory = detail.directory
            guard let ghost = directory.ghost(path, from: base) else { return false }
            perform(.show(card: ghost.id, section: directory.sectionId(of: ghost), focus: ghost.id, row: nil))
        }
        return true
    }

    /// The map tab brought forward on the section on screen, the other tabs left as they are, without it
    /// being a step to go back over: for the window moved from outside — an agent's select, which is
    /// about the diagram.
    func showCards() {
        guard activeTab != nil else { return }
        activeTab = nil
        saveTabs()
    }

    /// Another tab of a root's dashboard: the same place looked at another way, not a step
    /// to go back over — the way back returns to the view it left.
    func showRoot(_ tab: RootTab) {
        guard let at = activeTab, case .root(let id, _) = tabs[at] else { return }
        tabs[at] = .root(id, tab)
        saveTabs()
    }

    /// Another view of the project's settings, in the same way.
    func showSettings(_ tab: SettingsTab) {
        guard let at = activeTab, case .settings = tabs[at] else { return }
        tabs[at] = .settings(tab)
        saveTabs()
    }

    private func exists(_ place: Place) -> Bool {
        guard let detail else { return false }
        switch place {
        case .section(let id): return id == detail.directory.home || detail.sections.contains { $0.id == id }
        case .card(let id), .connections(let id, _): return schema(id) != nil
        case .root(let id, _): return detail.sections.contains { $0.id == id && $0.parentId == nil }
        case .settings, .inspection: return true
        }
    }

    /// Take the window there — a section on the map tab, anything else in its tab — and the tab bar
    /// remembered; where it landed, or nil when that is no longer there.
    @discardableResult
    private func arrive(at target: Place) -> Place? {
        guard exists(target), let detail else { return nil }
        switch target {
        case .section(let id):
            showSection(id)
            activeTab = nil
        case .card(let id), .connections(let id, _):
            guard let card = schema(id) else { return nil }
            leaveMap()
            // Its section comes with it: what the nav panel and the property panel are about.
            showSection(detail.directory.sectionId(of: card))
            selectSchema(id, follow: false)
            front(target)
        case .root(let id, _):
            leaveMap()
            showSection(id)
            front(target)
        case .settings:
            readExports(detail.project.id)
            leaveMap()
            front(target)
        case .inspection:
            leaveMap()
            front(target)
        }
        saveTabs()
        return here
    }

    /// The map tab going behind another: what it shows, held for when it comes forward.
    private func leaveMap() {
        if activeTab == nil { mapHeld = shownSectionKey }
    }

    /// `place` in front in its tab: the tab it has (now showing it as `place` says), else a new one
    /// right of the tab in front.
    private func front(_ place: Place) {
        if let at = tabs.firstIndex(where: { $0.sameTab(place) }) {
            tabs[at] = place
            activeTab = at
        } else {
            let at = activeTab.map { $0 + 1 } ?? 0
            tabs.insert(place, at: at)
            activeTab = at
        }
    }

    /// The tabs whose thing is no longer in the tree open (a card removed, a branch opened that does not
    /// have it) closed; the one in front gone, the map tab comes forward.
    private func pruneTabs() {
        let kept = tabs.filter(keepsTab)
        guard kept != tabs else { return }
        let front = activeTab.map { tabs[$0] }
        let map = mapSectionKey
        tabs = kept
        activeTab = front.flatMap { kept.firstIndex(of: $0) }
        if front != nil, activeTab == nil { showSection(map) }
        saveTabs()
    }

    /// Whether a tab's thing is in the tree open. Stricter than `exists`: a card the open branch removed is
    /// still found (`schema`, to show it struck through), and its tab closes all the same.
    private func keepsTab(_ place: Place) -> Bool {
        switch place {
        case .card(let id), .connections(let id, _): detail?.schemas.contains { $0.id == id } == true
        default: exists(place)
        }
    }

    /// The tab bar as it was left in the project, put back as it opens: what is gone from the tree open
    /// left out.
    private func restoreTabs(_ set: String) {
        guard let saved = savedTabs[set] ?? memory?.loadTabs(of: set) else { return }
        let places = saved.tabs.compactMap { Place(saved: $0) }
        let front = saved.active.flatMap { places.indices.contains($0) ? places[$0] : nil }
        tabs = places.filter(keepsTab)
        activeTab = nil
        if let map = saved.map, detail?.sections.contains(where: { $0.id == map }) == true { showSection(map) }
        if let front, tabs.contains(front) { arrive(at: front) }
        saveTabs()
    }

    /// The tab bar as it is, kept for the project open: for this run, and on this Mac when the window
    /// has a memory (`SelectionMemory`).
    private func saveTabs() {
        guard detail != nil, let set = tabsSet else { return }
        let saved = SelectionMemory.Tabs(tabs: tabs.map(\.saved), active: activeTab, map: mapSectionKey)
        guard savedTabs[set] != saved else { return }
        savedTabs[set] = saved
        memory?.saveTabs(saved, of: set)
    }

    /// Which tab bar a project's tree uses: main and its branches share the project's — a branch opened
    /// keeps what it has too — and the board has one of its own, as nothing of main is in its tree.
    private func tabSet(_ projectId: String) -> String {
        openTrees[projectId] == Board.id ? projectId + "/" + Board.word : projectId
    }
    /// The tab bar `tabs` is (`tabSet`).
    @ObservationIgnored private var tabsSet: String?
    /// The tab bars of the projects opened in this run, by `tabSet` (`saveTabs`).
    @ObservationIgnored private var savedTabs: [String: SelectionMemory.Tabs] = [:]

    /// The schema currently in the property panel.
    var selectedSchema: SchemaNode? {
        guard let selectedSchemaId else { return nil }
        return detail?.schemas.first { $0.id == selectedSchemaId }
    }

    /// The field currently in the property panel, with the schema and path that name it.
    var selectedField: FieldLocation? {
        guard let selectedFieldId else { return nil }
        return fieldLocation(selectedFieldId)
    }

    /// Where a field sits, by id — the same lookup `selectedField` makes, for callers
    /// holding an id rather than a selection.
    func fieldLocation(_ id: String) -> FieldLocation? {
        guard let detail else { return nil }
        return locateField(detail.schemas, id: id)
    }

    /// What this schema points at, and what points at it. Both directions, because
    /// "who depends on this" is the question a change is usually asking.
    /// A flow's lines are its nodes' too, and a goal's the cards' inside it.
    func references(of schemaId: String) -> (outgoing: [Relation], incoming: [Relation]) {
        guard let detail else { return ([], []) }
        let held = detail.heldIn()
        func holder(_ id: String) -> String { held[id] ?? id }
        return (detail.relations.filter { holder($0.fromSchemaId) == schemaId && holder($0.toSchemaId) != schemaId },
                detail.relations.filter { holder($0.toSchemaId) == schemaId && holder($0.fromSchemaId) != schemaId })
    }

    /// Where a card's line goes, as a path is written: the card (or card ghost) it reaches
    /// in full, and where it lands there; nil for a line whose card is gone.
    func linePath(_ line: Connection) -> String? {
        guard let detail else { return nil }
        let at = line.at ?? ""
        if PathText.hasSection(at) { return at }
        let directory = detail.directory
        guard let to = line.to else { return nil }
        let head = directory.schema(id: to).map(directory.full) ?? directory.ghost(id: to).map(directory.full)
        guard let head else { return nil }
        return at.isEmpty ? head : "\(head).\(at)"
    }

    /// What a store keeps, as paths: its `holds` lines, in their order.
    func holds(of store: SchemaNode) -> [String] {
        store.connections.filter { $0.line == Line.holds && $0.from.isEmpty }.map { linePath($0) ?? "" }
    }

    /// A card's lines as `card_update` writes them (`connections`): each as it is, its card by
    /// its id — the core keeps an id a card, a card ghost or a flow's node card of the project has,
    /// as the window wrote it (`tools/lines.rs`) — one whose card is gone kept as it was read
    /// (`to: null`).
    func wire(_ line: Connection) -> [String: Any] {
        var out: [String: Any] = ["line": line.line]
        if !line.from.isEmpty { out["from"] = line.from }
        if let to = line.to {
            let there = detail.map { project in
                project.schemas.contains { $0.id == to } || project.ghosts.contains { $0.id == to }
                    || project.inner.contains { $0.id == to }
            } ?? false
            out["to"] = there ? to : NSNull()
        }
        if let at = line.at { out["at"] = at }
        if let description = line.description { out["description"] = description }
        if !line.fields.isEmpty { out["fields"] = line.fields.mapValues { OJSON($0).foundation } }
        return out
    }

    /// A card's lines of one kind from its head put in place of the ones it has, the rest
    /// kept: what `card_update` writes (`connections`).
    private func lines(_ card: SchemaNode, replacing kind: String, with paths: [String]) -> [[String: Any]] {
        card.connections.filter { !($0.line == kind && $0.from.isEmpty) }.map(wire) + made(kind, paths, from: card)
    }

    /// Lines of one kind from a card's head to these paths, read from the card's section. The
    /// kinds written so (a store's `holds`, a task's `link`) land on a card's head: a path that
    /// reads here as landing inside a card — one not there yet, made in the same write, named
    /// as a place in its container — is written whole, for the core to find the card it names.
    private func made(_ kind: String, _ paths: [String], from card: SchemaNode) -> [[String: Any]] {
        paths.filter { !$0.trimmingCharacters(in: .whitespaces).isEmpty }.map { path -> [String: Any] in
            let end = detail?.directory.lineEnd(path, from: detail?.directory.sectionId(of: card)) ?? (to: path, at: nil)
            return ["line": kind, "to": end.at == nil ? end.to : path]
        }
    }

    /// A card of the project open, by its id — or, while what a merge would leave is looked at, one
    /// it takes away, as main has it (only read there).
    func schema(_ id: String) -> SchemaNode? {
        detail?.schemas.first { $0.id == id } ?? resultView?.gone.first { $0.id == id }
            ?? branchMarks.gone.cards.first { $0.id == id }
    }

    /// A card ghost of the project open, by its id.
    func ghost(_ id: String) -> CardGhost? { detail?.ghosts.first { $0.id == id } }

    /// A card by its id, or the card a ghost of that id leads to — what a `ref` written
    /// through a ghost names (it holds the ghost's id).
    func card(through id: String) -> SchemaNode? { detail?.directory.card(through: id) }

    /// `pointsAt` names its target rather than pointing at an id, so a rename is
    /// visible instead of silently still resolving. Read from `holder`'s section —
    /// a bare name inside a schema is one of its own section's.
    func schemaNamed(_ name: String, from holder: SchemaNode? = nil) -> SchemaNode? {
        guard let detail else { return nil }
        let directory = detail.directory
        return directory.schema(name, from: holder.map(directory.sectionId(of:)) ?? shownSectionKey)
    }

    /// A card a person dropped on a cell, in grid units (§12). Not a design change,
    /// so not in the change history — a locked main takes it too, whatever the box: the
    /// section's own cards and card ghosts, another section's cards and the cards summing a
    /// section up (`specboard://SpecBoard/Docs/Design:DesignBranches` §8). Another section's card is
    /// put where this section wants it, and nowhere else: its own section keeps its own place for it.
    func moveByHand(_ id: String, to point: GridPoint) {
        let spot = CGPoint(x: point.x, y: point.y)
        // A section ghost, and another section's card: where this section's diagram has it.
        if SectionGhosts.isGhost(id) || isOutside(id) {
            let section = shownSectionKey
            sectionPositions[section, default: [:]][id] = spot
            surroundings.positions[id] = spot
            try? place([box(id, on: section, at: point, pinned: true)])
            return
        }
        do {
            // The canvas has put the card there itself; re-reading would fight the hand. A
            // card ghost sits on its section's diagram as a card does (`places`, view "").
            try place([["item": id, "x": point.x, "y": point.y, "pinned": true]])
            if let index = detail?.schemas.firstIndex(where: { $0.id == id }) {
                detail?.schemas[index].posX = Double(point.x)
                detail?.schemas[index].posY = Double(point.y)
                detail?.schemas[index].pinned = true
            }
            if let index = detail?.ghosts.firstIndex(where: { $0.id == id }) {
                detail?.ghosts[index].posX = Double(point.x)
                detail?.ghosts[index].posY = Double(point.y)
                detail?.ghosts[index].pinned = true
            }
            if let index = surroundings.ghosts.firstIndex(where: { $0.id == id }) {
                surroundings.ghosts[index].posX = Double(point.x)
                surroundings.ghosts[index].posY = Double(point.y)
                surroundings.ghosts[index].pinned = true
            }
            lastError = nil
        } catch {
            lastError = error.localizedDescription
        }
    }

    /// What the diagram of `section` put by itself where it had no place — another section's
    /// card, a section ghost — kept there: what is drawn and what is kept stay the same, and MCP
    /// reads it (`section_read`'s `places`). Not pinned: a hand's place is. Not in a project that
    /// cannot be written (archived, or held by MCP); a locked main takes it, as it takes a box moved
    /// by hand.
    func keepPlaced(_ placed: [String: GridPoint], on section: String) {
        guard !isArchived, heldByMCP.isEmpty, section == shownSectionKey else { return }
        var boxes: [[String: Any]] = []
        for (id, at) in placed.sorted(by: { $0.key < $1.key }) {
            let spot = CGPoint(x: at.x, y: at.y)
            guard sectionPositions[section]?[id] != spot else { continue }
            sectionPositions[section, default: [:]][id] = spot
            surroundings.positions[id] = spot
            boxes.append(box(id, on: section, at: at, pinned: false))
        }
        guard !boxes.isEmpty else { return }
        try? place(boxes)
    }

    /// A box on a section's diagram as the window puts it (`places`): a section ghost by the
    /// section it sums up, a card by its id.
    private func box(_ id: String, on section: String, at: GridPoint, pinned: Bool) -> [String: Any] {
        guard SectionGhosts.isGhost(id) else {
            return ["view": section, "item": id, "x": at.x, "y": at.y, "pinned": pinned]
        }
        return ["view": section, "kind": "section", "item": String(id.dropFirst(SectionGhosts.prefix.count)),
                "x": at.x, "y": at.y, "pinned": pinned]
    }

    /// Whether a card pressed twice on the relation panel is written where it is: its kind says
    /// so (`on_double_click: edit_in_place`, a note) and the project may be
    /// written there. Otherwise the press goes where it points, as on any card.
    func canEditInPlace(_ id: String) -> Bool {
        guard !isReadOnly, let card = schema(id), let detail else { return false }
        return detail.cardKinds[card.kindName]?.onDoubleClick == "edit_in_place"
    }

    /// A flow's node dropped by hand on its canvas: where it sits, and nothing else of the
    /// flow — a locked main takes it too (`specboard://SpecBoard/Docs/Design:DesignBranches` §8, §11). On the cell
    /// asked for, or the nearest that keeps the margin clear of the other nodes.
    func moveNodeByHand(_ flowId: String, _ name: String, to point: GridPoint) {
        guard let flow = schema(flowId),
              let node = detail?.inner.first(where: { $0.container == flowId && $0.name == name }),
              let moved = FlowEditing.move(name, to: point, in: flow.nodes, margin: defaultSettings.margin)
                .first(where: { $0.name == name }),
              let at = moved.topLeft else { return }
        // A node is a card inside its flow: its box on the flow's card map, by its id.
        placeOnCardMap(flowId, kind: "card", item: node.id, at: at, moved: at != point)
    }

    /// A card a goal holds dropped by hand on the goal's card map (`GoalCardMap`): where it
    /// sits there — on the cell the canvas found clear — and nothing else of either card.
    func moveOnGoalMap(_ goalId: String, _ cardId: String, to point: GridPoint) {
        placeOnCardMap(goalId, kind: "card", item: cardId, at: point)
    }

    /// What sits on a card's own diagram — a flow's node, a goal's card — put where it was
    /// dropped (`places`, the card's id as the view): no history, and a locked main takes it
    /// too. Not read again: the canvas has put it there itself, as on a section's diagram
    /// (`moveByHand`); only what the window holds of where it is follows. `moved`: whether the
    /// canvas put it elsewhere — a node is put on the nearest cell clear of the others — and
    /// is to be drawn again where it is kept.
    private func placeOnCardMap(_ owner: String, kind: String, item: String, at: GridPoint, moved: Bool = false) {
        do {
            try ownWrite { try place([["view": owner, "kind": kind, "item": item, "x": at.x, "y": at.y, "pinned": true]]) }
            lastError = nil
        } catch {
            lastError = error.localizedDescription
            return
        }
        guard let index = detail?.schemas.firstIndex(where: { $0.id == owner }) else { return }
        if detail?.schemas[index].kind == .flow {
            if let inner = detail?.inner.firstIndex(where: { $0.id == item }) {
                detail?.inner[inner].posX = Double(at.x)
                detail?.inner[inner].posY = Double(at.y)
                if let name = detail?.inner[inner].name,
                   let node = detail?.schemas[index].nodes.firstIndex(where: { $0.name == name }) {
                    detail?.schemas[index].nodes[node].topLeft = at
                }
            }
        } else {
            detail?.schemas[index].itemPlaces[item] = at
        }
        if moved { dataGeneration += 1 }
    }

    /// A card sized by its corner on the diagram: its width alone, or both. The canvas has sized
    /// it itself; re-reading would only draw it again.
    func resizeByHand(_ id: String, to size: GridSize) {
        do {
            try ownWrite { _ = try tool("card_update", target: cardPath(id), value: ["size": ScreenWire.size(size)]) }
            if let index = detail?.schemas.firstIndex(where: { $0.id == id }) {
                detail?.schemas[index].size = size
            }
            lastError = nil
        } catch {
            lastError = error.localizedDescription
        }
    }

    /// A card's size from the property panel or the menu: nil, the project's.
    func setCardSize(_ id: String, _ size: GridSize?) {
        write { try tool("card_update", target: cardPath(id), value: ["size": ScreenWire.size(size)]) }
    }

    /// Whether 「中身に合わせて幅を決める」 measures `schema`: a card drawn as rows, whose rows the
    /// screen's fonts say the width of. Not a document, a page or a card its kind's face draws.
    func fitsWidth(_ schema: SchemaNode) -> Bool {
        schema.sizable && schema.face == nil && !drawnAsDocument(schema, docView: defaultSettings.docView)
    }

    /// A card as wide as its header and every row it shows now, measured once in the screen's
    /// fonts and written as its width in cells (its height of its own kept): what it holds later
    /// does not change it.
    func fitWidth(_ id: String) {
        guard let detail, let schema = schema(id), fitsWidth(schema) else { return }
        let byId = Dictionary(detail.schemas.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        let lookup: SchemaLookup = { byId[$0] }
        let content = cardRows(schema, lookup: lookup, inlineReuse: true,
                               folded: restingFolds(schema, lookup: lookup), tree: detail.directory.treeWord(of: schema))
        let natural = CardRenderer.naturalWidth(schema, content: content)
        setCardSize(id, GridSize(w: Self.cellsWide(natural, cardWidth: cardWidth), h: schema.size?.h))
    }

    /// How many cells across `points` takes on a map whose cards are `cardWidth` wide, rounded up,
    /// within what a card may be.
    static func cellsWide(_ points: CGFloat, cardWidth: CGFloat) -> Int {
        let cells = Int((points / CGFloat(Grid.unitWidth(cardWidth: Double(cardWidth)))).rounded(.up))
        return min(max(cells, GridSize.widths.lowerBound), GridSize.widths.upperBound)
    }

    /// How a formatdoc reads on the diagram: rows, a document, or nil for the project's.
    func setDocView(_ id: String, _ view: String?) {
        write { try tool("card_update", target: cardPath(id), value: ["view": view ?? NSNull()]) }
    }

    /// Cards with no place yet — new, moved from another section, or copied onto a
    /// card — get one where there is room in their section (§12), under the
    /// shortest column at their real height, and it is written: an agent reads it
    /// next. A project that cannot be written keeps none; the canvas finds them room.
    private func placeUnplaced() {
        guard let detail, core != nil,
              detail.schemas.contains(where: { $0.container == nil && ($0.posX == nil || $0.posY == nil) }) else { return }
        let byId = Dictionary(detail.schemas.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        let lookup: SchemaLookup = { byId[$0] }
        let rows = defaultSettings.size.rows
        let directory = detail.directory
        var places: [(id: String, at: GridPoint)] = []
        // A card inside a container sits on its container's card map, not on the section's.
        let bySection = Dictionary(grouping: detail.schemas.filter { $0.container == nil }) { directory.sectionId(of: $0) }
        for (section, cards) in bySection {
            let docView = defaultSettings.docView
            var taken = cards.compactMap { card -> GridRect? in
                guard let x = card.posX, let y = card.posY else { return nil }
                return GridRect(x: Grid.units(x), y: Grid.units(y), w: cardGridColumns(card),
                                h: cardGridRows(card, lookup: lookup, rows: rows, docView: docView))
            }
            // Its card ghosts sit there too, each as big as the card it leads to.
            taken += directory.ghosts(in: section).compactMap { ghost -> GridRect? in
                guard let x = ghost.posX, let y = ghost.posY, let card = directory.reached(ghost) else { return nil }
                return GridRect(x: Grid.units(x), y: Grid.units(y), w: cardGridColumns(card),
                                h: cardGridRows(card, lookup: lookup, rows: rows, docView: docView))
            }
            for card in cards where card.posX == nil || card.posY == nil {
                let at = defaultSettings.nextSpot(avoiding: taken, width: cardGridColumns(card))
                taken.append(GridRect(x: at.x, y: at.y, w: cardGridColumns(card),
                                      h: cardGridRows(card, lookup: lookup, rows: rows, docView: docView)))
                places.append((card.id, at))
            }
        }
        guard (try? place(places.map { ["item": $0.id, "x": $0.at.x, "y": $0.at.y, "pinned": false] })) != nil else { return }
        for (id, at) in places {
            guard let index = self.detail?.schemas.firstIndex(where: { $0.id == id }) else { continue }
            self.detail?.schemas[index].posX = Double(at.x)
            self.detail?.schemas[index].posY = Double(at.y)
        }
    }

    /// The cards goals hold with no place on their card maps — a project moved in, or not
    /// written since — given one by the core in the order each goal holds them
    /// (`place_unplaced`), and the project read again. A project that cannot be written keeps
    /// none; the canvas finds them room.
    private func placeUnplacedOnGoalMaps(_ projectId: String, branch tree: String?) {
        guard let core, let detail,
              detail.schemas.contains(where: { goal in
                  goal.kind == .goal && goal.inside.contains { $0 != goal.id && goal.itemPlaces[$0] == nil }
              }),
              (try? core.request("place_unplaced", ["project": projectId, "branch": tree ?? ""])) != nil,
              let read = try? core.detail(projectId, branch: tree) else { return }
        // What the window placed itself is kept: only the goals' maps are taken from the core.
        var next = detail
        let maps = Dictionary(read.schemas.map { ($0.id, $0.itemPlaces) }, uniquingKeysWith: { a, _ in a })
        for index in next.schemas.indices where next.schemas[index].kind == .goal {
            next.schemas[index].itemPlaces = maps[next.schemas[index].id] ?? next.schemas[index].itemPlaces
        }
        self.detail = next
    }

    /// Every card as wide as the widest, as it first shows — for `auto` (§12).
    private func measureCards() {
        guard let detail else { return }
        cardWidth = Self.cardWidth(of: detail, settings: defaultSettings)
    }

    /// How wide every card of `detail` is drawn by `settings`: the width set, or for `auto` the widest
    /// card's — the same for the window and for a picture (`MapPicture`).
    static func cardWidth(of detail: ProjectDetail, settings: LayoutSettings) -> CGFloat {
        if let width = settings.size.width { return CGFloat(width) }
        let byId = Dictionary(detail.schemas.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        let lookup: SchemaLookup = { byId[$0] }
        let directory = detail.directory
        var widest: CGFloat = 0
        // A card with a width of its own keeps it (in cells, so it follows the measure); a writing
        // card drawn as a document is as wide as it is, so is a card its kind's face draws as a page,
        // and a working directory's file (`CompactBox`).
        for schema in detail.schemas
        where !CompactBox.isCompact(schema)
            && !(schema.sizable && schema.size != nil)
            && !(schema.kind.isWriting && drawnAsDocument(schema, docView: settings.docView))
            && !(schema.face != nil && schema.faceDrawsPage) {
            let content = cardRows(schema, lookup: lookup, inlineReuse: true,
                                   folded: restingFolds(schema, lookup: lookup), tree: directory.treeWord(of: schema))
            widest = max(widest, CardRenderer.naturalWidth(schema, content: content))
        }
        return min(max(widest, CGFloat(CardSize.widthRange.lowerBound)), CGFloat(CardSize.widthRange.upperBound))
    }

    /// Drop the manual pins and rebuild the arrangement.
    ///
    /// Only what is currently shown gets arranged, so filtering and then arranging
    /// is a usable pair — laying out cards that are not on screen would spread the
    /// ones that are.
    @discardableResult
    func autoLayout(filters: GraphFilters = GraphFilters()) -> Int {
        guard let detail, let section = sectionDetail, core != nil else {
            return layoutGeneration
        }
        var filters = filters
        filters.nesting = .always
        let shown = Set(visibleSchemas(section, filters).map(\.id))
        let placements = LayeredLayout.forProject(section, visible: shown, nesting: .always, settings: defaultSettings)
        // Worked out in points for cards of the standard width: in grid units, the
        // same arrangement at any width.
        let boxes: [[String: Any]] = placements.map { p in
            let point = Grid.fromPoints(x: Double(p.posX), y: Double(p.posY), cardWidth: Double(CardGeometry.cardWidth))
            return ["item": p.id, "x": point.x, "y": point.y, "pinned": false]
        }
        try? place(boxes)
        layoutGeneration += 1
        readWritten(detail.project.id, now: true)
        return layoutGeneration
    }

    // MARK: - Editing

    /// The last rejection, for the bubble. Validation messages are written to be
    /// read, so they are shown rather than swallowed — and kept in the bell's list, as the bubble goes.
    /// Said again while it is still up, it is the same one, not another.
    var lastError: String? {
        didSet { if let lastError, lastError != oldValue { noticeLog.add(.error, lastError) } }
    }
    /// The AIs' merges let into a locked main without being approved, as the core told them
    /// (`AppModel+AIMerge`): said in the bubble at the bell.
    var aiMerges: [AIMerge] = []
    /// Something done that is worth saying, and nothing went wrong: kept in the bell's list too.
    var notice: String? {
        didSet { if let notice, notice != oldValue { noticeLog.add(.notice, notice) } }
    }
    /// What the notice bubble said in this window, for the bell in the top bar (`NoticeBell`).
    var noticeLog = NoticeLog()
    /// Whether the bell's list is open: the bubble opens it too, from where it stands.
    var noticeListOpen = false

    /// One operation of the window, through the core: one transaction, so one commit
    /// (`specboard://SpecBoard/Docs/Design:DesignBranches` §7). When it went through, what it changes is
    /// read again (`readWritten`) — the open tree's cards, places and history — and `then` is done once
    /// that is in. Not private: the model's other parts write through it too (`AppModel+Lines`).
    func write(_ body: () throws -> Void, then: (() -> Void)? = nil) {
        guard let projectId = detail?.project.id else { return }
        do {
            try ownWrite(body)
            lastError = nil
            readWritten(projectId, then: then)
        } catch {
            lastError = error.localizedDescription
        }
    }

    /// Project-level edits name their target, because the project screen exists to work on
    /// the ones that are *not* open. `write(_:)` above assumes the open project, which is
    /// right for schemas and fields and wrong for this.
    private func writeProject(_ projectId: String, _ body: (CoreClient) throws -> Void) {
        guard let core = core(of: projectId) else { return }
        do {
            try ownWrite { try body(core) }
            lastError = nil
            try readProjects(core)
            if detail?.project.id == projectId { select(projectId) }
        } catch {
            lastError = error.localizedDescription
        }
    }

    /// The window's own write: what the core tells of it is not heard again (the writer
    /// reads the project itself). Runs on main, where the core tells it; said in the status bar.
    private func ownWrite<T>(_ body: () throws -> T) rethrows -> T {
        try working(.writing) {
            settle()
            return try body()
        }
    }

    /// A tool run as the window (`tool`), on the project on screen — in the tree open, for
    /// the tools that work in one.
    @discardableResult
    func tool(_ name: String, target: Any? = nil, value: [String: Any]? = nil,
                      _ more: [String: Any] = [:], inTree: Bool = true) throws -> OJSON {
        guard let core, let projectId = detail?.project.id else { throw CoreClient.Refusal(L10n[.Common.problemNoProject]) }
        var arguments = more
        arguments["project"] = projectId
        if inTree { arguments.merge(openTreeArgument) { given, _ in given } }
        if let target { arguments["target"] = target }
        if let value { arguments["value"] = value }
        return try core.ordered("tool", ["name": name, "arguments": arguments])
    }

    /// Boxes put as they are, in the tree open (`places`): not in the history.
    private func place(_ boxes: [[String: Any]]) throws {
        guard let core, let projectId = detail?.project.id else { return }
        _ = try core.request("places", ["project": projectId, "branch": openTrees[projectId] ?? "", "boxes": boxes])
    }

    /// A card as the tools name it: in full, from its tree's root.
    func cardPath(_ id: String) throws -> String {
        guard let detail, let card = schema(id) else { throw CoreClient.Refusal(L10n[.Model.errorNoCard]) }
        return detail.directory.full(card)
    }

    /// A field as the tools name it: its card's path and the names down to it.
    private func fieldPath(_ id: String) throws -> String {
        guard let detail, let found = fieldLocation(id) else { throw CoreClient.Refusal(L10n[.Model.errorNoField]) }
        return detail.directory.full(found.schema, found.path)
    }

    /// A section as the tools name it.
    private func sectionPath(_ id: String) throws -> String {
        guard let detail, detail.sections.contains(where: { $0.id == id }) else { throw CoreClient.Refusal(L10n[.Model.errorNoSection]) }
        return detail.directory.full(section: id)
    }

    /// Opens what it creates: a person who just made a project is about to fill it in.
    /// (The MCP tool deliberately does not — an agent creating one in the background
    /// is a different situation.) Made where `place` says: this Mac (nil), or an agent.
    func createProject(name: String, description: String = "", in place: UUID? = nil) {
        guard let core = core(at: place) else { return }
        do {
            let before = Set(projects.map(\.id))
            _ = try ownWrite {
                try core.request("tool", ["name": "project_create",
                                          "arguments": ["project": name, "value": ["description": description]]])
            }
            lastError = nil
            try readProjects(core)
            guard let id = projects.first(where: { !before.contains($0.id) })?.id else { return }
            selectedSchemaId = nil
            selectedFieldId = nil
            select(id)
        } catch {
            lastError = error.localizedDescription
        }
    }

    // MARK: - Project files and the archive (`specboard://SpecBoard/Docs/Design:Overview` §10)

    /// Whether the project on screen is archived: read, never written.
    var isArchived: Bool {
        guard let id = detail?.project.id else { return false }
        return projects.first { $0.id == id }?.isArchived ?? false
    }

    /// The project as a file, to be put somewhere.
    func projectFile(_ projectId: String) -> String? {
        guard let core = core(of: projectId) else { return nil }
        do {
            let text = try (core.request("project_file", ["project": projectId]) as? [String: Any])?["text"] as? String ?? ""
            lastError = nil
            return text
        } catch {
            lastError = error.localizedDescription
            return nil
        }
    }

    /// A project file read in, and opened. `copy`: beside everything as a project of its
    /// own. Otherwise in place of the project with its id — which goes to the archive
    /// first, whole, so nothing that was here is lost; this Mac's settings for it
    /// (the export root, the access keys) stay. Read into this Mac's projects.
    func readProjectFile(_ text: String, copy: Bool) {
        guard let core = localCore else { return }
        do {
            guard let found = peekProjectFile(text) else {
                throw CoreClient.Refusal(L10n[.Model.errorNotProjectFile])
            }
            let id: String
            if copy {
                let names = Set(projects.filter { $0.place == nil }.map(\.name))
                var name = found.name, n = 2
                if names.contains(name) { name = L10n[.Model.importName(found.name)] }
                while names.contains(name) { name = L10n[.Model.importNameNumbered(found.name, n)]; n += 1 }
                let read = try ownWrite {
                    try core.request("project_file_read", ["text": text, "landing": "copy", "name": name])
                }
                id = (read as? [String: Any])?["project"] as? String ?? found.id
            } else {
                // What was here goes to the archive first, whole, so nothing is lost.
                let archiveFirst = projects.contains { $0.id == found.id && $0.place == nil && !$0.isArchived }
                let read = try ownWrite {
                    try core.request("project_file_read", ["text": text, "landing": "replace", "archiveFirst": archiveFirst])
                }
                id = (read as? [String: Any])?["project"] as? String ?? found.id
            }
            lastError = nil
            try readProjects(core)
            selectedSchemaId = nil
            selectedFieldId = nil
            if detail?.project.id == id { refresh(id) } else { select(id) }
        } catch {
            lastError = error.localizedDescription
        }
    }

    /// The project a file holds, its id and name, as the core reads it (`project_file_peek`); nil
    /// for what is no project file.
    func peekProjectFile(_ text: String) -> (id: String, name: String)? {
        guard let read = (try? localCore?.request("project_file_peek", ["text": text])) as? [String: Any],
              let id = read["id"] as? String, let name = read["name"] as? String else { return nil }
        return (id, name)
    }

    /// Whether a project with the file's id is on this Mac already — reading it in would
    /// then mean choosing between replacing it and keeping both.
    func projectExists(_ projectId: String) -> Bool { projects.contains { $0.id == projectId && $0.place == nil } }

    func archiveProject(_ projectId: String) {
        editProjects(projectId) { _ = try $0.request("project_archive", ["project": projectId]) }
    }

    func restoreProject(_ projectId: String) {
        editProjects(projectId) { _ = try $0.request("project_restore", ["project": projectId]) }
    }

    /// The projects every access key may read through MCP (`specboard://SpecBoard/Docs/Design:DesignAccess` §2).
    private(set) var publicProjects: Set<String> = []

    func setPublic(_ projectId: String, _ isPublic: Bool) {
        guard let core = core(of: projectId) else { return }
        do {
            _ = try ownWrite { try core.request("project_public_set", ["project": projectId, "on": isPublic]) }
            try readPublic(core)
            lastError = nil
        } catch {
            lastError = error.localizedDescription
        }
    }

    /// A change to the project `projectId` itself, and the list read again.
    private func editProjects(_ projectId: String, _ body: (CoreClient) throws -> Void) {
        guard let core = core(of: projectId) else { return }
        do {
            try ownWrite { try body(core) }
            lastError = nil
            try readProjects(core)
            if let id = detail?.project.id { refresh(id) }
        } catch {
            lastError = error.localizedDescription
        }
    }

    func renameProject(_ projectId: String, to name: String) {
        writeProject(projectId) { _ = try $0.request("project_rename", ["project": projectId, "name": name]) }
    }

    func updateProjectDescription(_ projectId: String, _ text: String) {
        writeProject(projectId) {
            _ = try $0.request("tool", ["name": "project_update",
                                        "arguments": ["project": projectId, "value": ["description": text]]])
        }
    }

    /// The language the project's exports say their own words in: one for everyone who writes it out.
    func updateExportLanguage(_ projectId: String, _ code: String) {
        writeProject(projectId) {
            _ = try $0.request("tool", ["name": "project_update",
                                        "arguments": ["project": projectId, "value": ["exportLanguage": code]]])
        }
    }

    /// The project's slug changed: what each export kept in its default folder goes with it.
    func updateProjectSlug(_ projectId: String, _ slug: String) {
        writeProject(projectId) {
            _ = try $0.request("tool", ["name": "project_update",
                                        "arguments": ["project": projectId, "value": ["slug": slug]]])
        }
    }

    /// The project moved whole to `place` — this Mac (nil), or a connected agent: everything it has,
    /// its history and ids too, put in there (`project_bundle`), opened there to see it whole, its
    /// file kept on this Mac, and then let go of where it was. What is the store's own stays (the
    /// folders bound, the keys, public). Not while a tree of it is being put right in one go. The file
    /// kept, or nil.
    @discardableResult
    func moveProject(_ projectId: String, to place: UUID?) -> URL? {
        guard let from = core(of: projectId), let to = core(at: place), from !== to,
              let summary = projects.first(where: { $0.id == projectId }) else { return nil }
        do {
            let trees = [nil] + (try from.branches(projectId)).map { Optional($0.id) } + [Board.id]
            if try trees.contains(where: { !(try from.holds(projectId, branch: $0)).isEmpty }) {
                throw CoreClient.Refusal(L10n[.Model.errorMoveWhileGrouping])
            }
            let bundle = try from.request("project_bundle", ["project": projectId])
            _ = try ownWrite { try to.request("project_bundle_read", ["bundle": bundle]) }
            do {
                _ = try to.detail(projectId, branch: nil)
            } catch {
                _ = try? ownWrite { () throws -> Any in try to.request("project_delete", ["project": projectId]) }
                throw CoreClient.Refusal(L10n[.Model.errorMoveCantOpen(error.localizedDescription)])
            }
            let text = (try from.request("project_file", ["project": projectId]) as? [String: Any])?["text"] as? String ?? ""
            let kept = try keepMoved(text, slug: summary.slug)
            _ = try ownWrite { try from.request("project_delete", ["project": projectId]) }
            try readProjects(from)
            try readProjects(to)
            try? readPublic(to)
            if detail?.project.id == projectId { show(projectId) }
            lastError = nil
            notice = L10n[.Model.noticeMoved(summary.name, place.flatMap { agentNames[$0] } ?? L10n[.Common.wordThisMac], kept.path)]
            return kept
        } catch {
            lastError = error.localizedDescription
            return nil
        }
    }

    /// A moved project's file, kept on this Mac beside its store (`moved/<slug>-<when>.project.json`).
    private func keepMoved(_ text: String, slug: String) throws -> URL {
        let folder = try (databaseURL ?? CoreClient.swiftStoreURL()).deletingLastPathComponent()
            .appendingPathComponent("moved", isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let stamp = DateFormatter()
        stamp.locale = Locale(identifier: "en_US_POSIX")
        stamp.dateFormat = "yyyyMMdd-HHmmss"
        let file = folder.appendingPathComponent(Self.movedFileName(slug: slug, stamp: stamp.string(from: Date())))
        try text.write(to: file, atomically: true, encoding: .utf8)
        return file
    }

    /// The name a moved project's file is kept under in `moved/`: its slug with anything but a letter,
    /// a digit, `-` and `_` put as `_` — a slug that came in from elsewhere could hold `/` or `..` and
    /// put the file outside the folder.
    nonisolated static func movedFileName(slug: String, stamp: String) -> String {
        let kept = String(slug.unicodeScalars.map { scalar -> Character in
            CharacterSet.alphanumerics.contains(scalar) || scalar == "-" || scalar == "_"
                ? Character(scalar) : "_"
        })
        return "\(kept.isEmpty ? "project" : kept)-\(stamp).project.json"
    }

    /// Everything in it goes too, which is why the caller asks first.
    func deleteProject(_ projectId: String) {
        guard let core = core(of: projectId) else { return }
        let wasOpen = detail?.project.id == projectId
        do {
            _ = try ownWrite { try core.request("project_delete", ["project": projectId]) }
            lastError = nil
            try readProjects(core)
            // Only move if what was open is what went. Deleting some other project from
            // the project screen must not take the window away from the one in use.
            guard wasOpen else { return }
            selectedSchemaId = nil
            selectedFieldId = nil
            if let next = projects.first(where: { !$0.isArchived }) ?? projects.first {
                select(next.id)
            } else {
                detail = nil; events = []
            }
        } catch {
            lastError = error.localizedDescription
        }
    }

    /// In the section on screen.
    func createSchema(name: String, label: String = "", kind: SchemaKind = .entity) {
        createSchema(name: name, label: label, kindName: kind.rawValue)
    }

    /// In the section on screen, of a kind by its name: a built-in one or one the project defines.
    /// `more`: what else its value says (a data card's shape, `of`). Returns the card made — picked,
    /// and brought into view on the diagram once drawn unless `inView` says not (a card whose editor
    /// opens next: the diagram is not what is seen).
    @discardableResult
    func createSchema(name: String, label: String = "", kindName: String, more: [String: Any] = [:],
                      inView: Bool = true) -> SchemaNode? {
        guard core != nil, let projectId = detail?.project.id, let detail else { return nil }
        do {
            let section = detail.directory.full(section: shownSectionKey)
            var value: [String: Any] = more
            value["kind"] = kindName
            if !label.isEmpty { value["label"] = label }
            let name = PathText.trimmed(name)
            try ownWrite { _ = try tool("card_create", target: PathText.cardPath(section, name), value: value) }
            lastError = nil
            // What was made is handed back, so it is read before this returns.
            readWritten(projectId, now: true)
            if let made = self.detail?.schemas.first(where: {
                $0.name == name && self.detail?.directory.sectionId(of: $0) == shownSectionKey
            }) {
                selectSchema(made.id)
                if inView { bringIntoView(made.id, in: shownSectionKey) }
                return made
            }
        } catch {
            lastError = error.localizedDescription
        }
        return nil
    }

    func updateSchema(_ id: String, _ change: CardEdit) {
        write { try tool("card_update", target: cardPath(id), value: change.wire) }
    }

    // MARK: - A card of a kind the project defines

    /// One of its fields written, as its kind holds it (a number, a check, a list…); nil, or
    /// what says nothing, takes it off.
    func setOwnField(_ id: String, _ name: String, _ value: JSONValue?) {
        write { try tool("card_update", target: cardPath(id), value: [name: value?.any ?? NSNull()]) }
    }

    /// All the rows of one of its kinds of row, in MCP's form (`OwnRows.wire`), written whole.
    func setOwnRows(_ id: String, _ kind: String, _ rows: [Any]) {
        write { try tool("card_update", target: cardPath(id), value: [kind: rows]) }
    }

    /// A row's check (its kind's first checkbox) turned over.
    func toggleOwnRow(_ card: SchemaNode, at path: String) {
        guard let def = card.kindDef, let row = OwnRows.walk(def, card.ownRows).first(where: { $0.path == path }),
              let check = row.kind.check,
              let changed = OwnRows.changing(card.ownRows, at: row.places, { fields in
                  var fields = fields
                  fields[check] = .bool(fields[check] != .bool(true))
                  return fields
              }) else { return }
        setOwnRows(card.id, changed.kind, changed.rows)
    }

    /// What its definition no longer has (「定義に無い欄」) let go of: the one at `at`, or all.
    func dropStrays(_ id: String, at: String? = nil) {
        let which: Any = at.map { [$0: NSNull()] } ?? NSNull()
        write { try tool("card_update", target: cardPath(id), value: ["strayFields": which]) }
    }

    /// A card made inside a container (a goal, or one of the project's own kind): `<container>.<name>`,
    /// of `kind`, at the end of what it holds. Picked once it is there, the container still open — and
    /// brought into view on `map`, its card map, once drawn.
    func addInside(_ containerId: String, kind: String, name: String, label: String = "",
                   showingOn map: CardCanvasController? = nil) {
        guard let detail, let container = schema(containerId) else { return }
        var value: [String: Any] = ["kind": kind]
        let label = label.trimmingCharacters(in: .whitespaces)
        if !label.isEmpty { value["label"] = label }
        let name = PathText.trimmed(name)
        write { try tool("card_create", target: PathText.member(detail.directory.full(container), name), value: value) } then: { [weak self] in
            guard let made = self?.detail?.schemas.first(where: { $0.name == name && $0.container == containerId }) else { return }
            self?.selectSchema(made.id, follow: false)
            map?.revealAfterNextShow(GoalCardMap.id(made.id))
        }
    }

    /// A custom card's field set — empty takes it off (`specboard://SpecBoard/Docs/Design:DesignBoard` §3).
    func setCustomField(_ id: String, _ name: String, _ value: String) {
        write { try tool("card_update", target: cardPath(id), value: [name: value]) }
    }

    /// A field of a card's kind that is a check (a task's `done`), written as one.
    func setCustomFlag(_ id: String, _ name: String, _ on: Bool) {
        write { try tool("card_update", target: cardPath(id), value: [name: on]) }
    }

    /// A field of a card's kind that is a merge request's number (a goal's `MR`), written as a
    /// number — the core takes no words for it; nil takes it off.
    func setCustomNumber(_ id: String, _ name: String, _ number: Int?) {
        let written: Any
        if let number { written = number } else { written = "" }
        write { try tool("card_update", target: cardPath(id), value: [name: written]) }
    }

    /// A task done or not.
    func setTaskDone(_ id: String, _ done: Bool) {
        setCustomFlag(id, "done", done)
    }

    /// A card inside `container`, by its name, pressed on the container's row: one of its choices set
    /// (`field`, `option`), or, with none, its check turned over as its kind's `as_row` says.
    func pressInside(_ container: SchemaNode, name: String, set field: String?, to option: String?) {
        // The row's path: the card's name, as a path writes it.
        let names = PathText.names(name.hasPrefix(ContainerRows.prefix) ? String(name.dropFirst(ContainerRows.prefix.count)) : name)
        guard names.count == 1, let name = names.first,
              let card = container.inside.compactMap({ schema($0) }).first(where: { $0.name == name }) else { return }
        if let field, let option {
            setCustomField(card.id, field, option)
            return
        }
        guard let def = card.definition, let check = def.rowCheck,
              let row = container.customRows.first(where: { $0.name == name }),
              let written = def.rowCheckValue(!row.done) else { return }
        write { try tool("card_update", target: cardPath(card.id), value: [check.name: written.wire]) }
    }

    /// A card inside a container (a goal's task) one place earlier or later among the cards
    /// beside it there (`card_update`'s `position`): a task's order — the order it is done in — is
    /// its place in its goal, numbered on its card map and its goal's rows. Nothing where it would
    /// go past either end.
    func moveInside(_ id: String, by offset: Int) {
        guard let card = schema(id), let container = card.container, let holder = schema(container),
              let index = holder.inside.firstIndex(of: id), holder.inside.indices.contains(index + offset) else { return }
        write { try tool("card_update", target: cardPath(id), value: ["position": index + offset]) }
    }

    /// A task made in a goal (「task を足す」 on its card map): named `name`, inside the goal,
    /// at the end of what it holds — one write, one commit (`card_create` of
    /// `<goal>.<name>`). Picked once it is there, the goal still open — and brought into view on `map`,
    /// the goal's card map, once drawn.
    func addTask(to goalId: String, name: String, label: String = "", showingOn map: CardCanvasController? = nil) {
        guard let detail, let goal = schema(goalId) else { return }
        var value: [String: Any] = ["kind": "task"]
        let label = label.trimmingCharacters(in: .whitespaces)
        if !label.isEmpty { value["label"] = label }
        let name = PathText.trimmed(name)
        write { try tool("card_create", target: PathText.member(detail.directory.full(goal), name), value: value) } then: { [weak self] in
            guard let made = self?.detail?.schemas.first(where: { $0.name == name && $0.container == goalId }) else { return }
            self?.selectSchema(made.id, follow: false)
            map?.revealAfterNextShow(GoalCardMap.id(made.id))
        }
    }

    /// A custom card's rows, all of them as they are now: a goal's are the cards inside it
    /// (`specboard://SpecBoard/Docs/Design:CardKinds`「入れ物のカード」), each written with the tools — a row made here
    /// a task made inside the goal (`card_create` of `<goal>.<name>`), one changed its card
    /// (its label, whether it is done, what it is joined to: its `link` lines), one taken out
    /// let go of (a task) or taken out of the goal (a goal under it: `in: null`), and the
    /// order the cards' (`position`).
    func setCustomRows(_ id: String, _ rows: [CustomRow]) {
        guard let goal = schema(id), let detail else { return }
        // What a kind unfolds under a row is shown only (`WholeCard.unfolded`): not written back.
        let rows = rows.map { row -> CustomRow in
            var row = row
            row.rows.removeAll { $0.type == WholeCard.unfolded }
            return row
        }
        if rows.contains(where: { !$0.rows.isEmpty }) {
            lastError = L10n[.Model.errorNoRowsUnderTask]
            return
        }
        let directory = detail.directory
        let inside = goal.inside.compactMap { schema($0) }
        let byName = Dictionary(inside.map { ($0.name, $0) }, uniquingKeysWith: { a, _ in a })
        let goalPath = directory.full(goal)
        write {
            // The rows made here, every one a task in one call: their `link` lines may name one
            // another, and the cards changed below may name them, whatever order they come in
            // (the core names every card of a call before it writes their lines).
            var making: [String: Any] = [:]
            for row in rows where byName[row.name] == nil {
                var value: [String: Any] = ["kind": "task"]
                if !row.text.isEmpty { value["label"] = row.text }
                if row.done { value["done"] = true }
                if !row.links.isEmpty { value["connections"] = made(Line.link, row.links, from: goal) }
                making[PathText.member(goalPath, row.name)] = value
            }
            if !making.isEmpty { try tool("card_create", value: making) }
            for row in rows {
                guard let card = byName[row.name] else { continue }
                let was = goal.customRows.first { $0.name == row.name }
                var value: [String: Any] = [:]
                if was?.text != row.text { value["label"] = row.text }
                if was?.links != row.links { value["connections"] = lines(card, replacing: Line.link, with: row.links) }
                // Its check, as its kind's `as_row` says it (a task's 済み, a goal's 状態: 完了 or 進行中).
                if was?.done != row.done, let check = card.definition?.rowCheck,
                   let written = card.definition?.rowCheckValue(row.done) {
                    value[check.name] = written.wire
                }
                if !value.isEmpty { try tool("card_update", target: directory.full(card), value: value) }
            }
            // A row taken out: its task goes with it; a goal under it is taken out of this one.
            for card in inside where !rows.contains(where: { $0.name == card.name }) {
                if card.kind == .task {
                    try tool("card_delete", target: directory.full(card))
                } else {
                    try tool("card_update", target: directory.full(card), value: ["in": NSNull()])
                }
            }
            // In the order of the rows — always when cards were made, as one call makes them in
            // an order of its own.
            let kept = rows.map(\.name)
            let now = inside.map(\.name).filter(kept.contains)
                + rows.map(\.name).filter { name in !inside.contains { $0.name == name } }
            if now != kept || !making.isEmpty {
                // In one call, so one commit: each put at its place, in order.
                var places: [String: Any] = [:]
                for (index, name) in kept.enumerated() { places[PathText.member(goalPath, name)] = ["position": index] }
                try tool("card_update", value: places)
            }
        }
    }

    /// A custom card's switch or option pressed on the diagram: the row done or
    /// not, the field set to the option. Nothing where nothing may be written.
    /// A card drawn through a card ghost (a ghost's box) is pressed as the card it leads to.
    func pressForm(_ id: String, _ press: CardCanvasView.FormPress) {
        // A stackdoc's selector: the version open, how it looks — a locked main takes it too.
        if let card = card(through: id), card.kind == .stackdoc {
            if case .choose(StackRows.selector, let key) = press, canSetLooks,
               let version = StackRows.version(card, key: key), version.name != card.openVersion?.name {
                openVersion(card.id, version.name)
            }
            return
        }
        guard !isReadOnly, let card = card(through: id) else { return }
        // A card inside a container, pressed on the container's row (`ContainerRows`): its check
        // turned over, one of its choices set — as its kind's `as_row` says.
        if case .choose(let field, let option) = press, let inside = ContainerRows.choice(field) {
            pressInside(card, name: inside.path, set: inside.field, to: option)
            return
        }
        if card.kind == .custom, case .toggle(let path) = press, path.hasPrefix(ContainerRows.prefix) {
            pressInside(card, name: String(path.dropFirst(ContainerRows.prefix.count)), set: nil, to: nil)
            return
        }
        // A card of a kind the project defines: a check or a choice of its fields, a row's check.
        if card.kind == .custom {
            switch press {
            case .toggle(let path) where path.hasPrefix(OwnRows.fieldRow):
                let name = String(path.dropFirst(OwnRows.fieldRow.count))
                setOwnField(card.id, name, .bool(card.ownFields[name] != .bool(true)))
            case .toggle(let path):
                toggleOwnRow(card, at: path)
            case .choose(let field, let option):
                guard card.ownFields[field] != .string(option) else { return }
                setOwnField(card.id, field, .string(option))
            }
            return
        }
        // A task's check, on its goal's card map or drawn by its kind's face: done or not.
        if card.kind == .task {
            if case .toggle = press { setTaskDone(card.id, !GoalCardMap.isDone(card)) }
            return
        }
        // A field a built-in kind's face puts on the diagram: a check turned
        // over, a choice set.
        if card.face != nil, !card.kind.isCustom {
            switch press {
            case .toggle(let path) where path.hasPrefix(OwnRows.fieldRow):
                let name = String(path.dropFirst(OwnRows.fieldRow.count))
                setCustomFlag(card.id, name, card.faceValues[name] != .bool(true))
            case .choose(let field, let option):
                guard card.faceValues[field] != .string(option) else { return }
                setCustomField(card.id, field, option)
            default:
                break
            }
            return
        }
        guard card.kind.isCustom else { return }
        switch press {
        case .toggle(let path):
            setCustomRows(card.id, CustomRowEditing.update(card.customRows, at: path) { $0.done.toggle() })
        case .choose(let field, let option):
            guard card.customFields[field] != option else { return }
            setCustomField(card.id, field, option)
        }
    }

    func deleteSchema(_ id: String) {
        selectedSchemaId = nil
        selectedFieldId = nil
        write { try tool("card_delete", target: cardPath(id)) }
    }

    func addField(to schemaId: String, parentFieldId: String? = nil,
                  name: String, type: FieldType = FieldType(rawValue: "string")) {
        guard let projectId = detail?.project.id else { return }
        do {
            let under = try parentFieldId.map(fieldPath) ?? cardPath(schemaId)
            let name = PathText.trimmed(name)
            try ownWrite { _ = try tool("card_create", target: PathText.member(under, name), value: ["type": type.rawValue]) }
            lastError = nil
            readWritten(projectId) { [weak self] in
                guard let self else { return }
                let parentPath = parentFieldId.flatMap { self.fieldLocation($0)?.path } ?? []
                if let card = self.schema(schemaId), let made = Self.field(at: parentPath + [name], in: card) {
                    self.selectField(schemaId, made.id)
                }
            }
        } catch {
            lastError = error.localizedDescription
        }
    }

    /// The field at the names given, down from the card.
    private static func field(at names: [String], in card: SchemaNode) -> FieldNode? {
        var level = card.fields
        var found: FieldNode?
        for name in names {
            guard let next = level.first(where: { $0.name == name }) else { return nil }
            found = next
            level = next.children
        }
        return found
    }

    func updateField(_ id: String, _ change: FieldEdit) {
        write { try tool("card_update", target: fieldPath(id), value: change.wire) }
    }

    func deleteField(_ id: String) {
        selectedFieldId = nil
        write { try tool("card_delete", target: fieldPath(id)) }
    }

    /// One of an enum's values as its card editor writes it back: all it says, so a value moved,
    /// renamed or taken out of the list leaves the others whole.
    typealias ValueEntry = (value: String, label: String, description: String)

    /// The values as they are, to be written back changed.
    nonisolated static func entries(_ values: [EnumValue]) -> [ValueEntry] {
        values.map { ($0.value, $0.label, $0.description) }
    }

    /// The values as `card_update` writes them: what a value does not say is left out.
    nonisolated static func valuesWire(_ values: [ValueEntry]) -> [[String: Any]] {
        values.map { value in
            var entry: [String: Any] = ["name": value.value]
            if !value.label.isEmpty { entry["label"] = value.label }
            if !value.description.isEmpty { entry["description"] = value.description }
            return entry
        }
    }

    func addValue(to schemaId: String, value: String, label: String = "") {
        guard let card = schema(schemaId) else { return }
        setValues(schemaId, Self.entries(card.values) + [(value, label, "")])
    }

    func deleteValue(_ id: String) {
        if case .value(id)? = selectedRow { selectedRow = nil }
        guard let card = detail?.schemas.first(where: { $0.values.contains { $0.id == id } }) else { return }
        setValues(card.id, Self.entries(card.values.filter { $0.id != id }))
    }

    /// An enum's values, all of them, in this order — how one is changed, moved or
    /// taken out from its card editor.
    func setValues(_ schemaId: String, _ values: [ValueEntry]) {
        write {
            try tool("card_update", target: cardPath(schemaId), value: ["values": Self.valuesWire(values)])
        }
    }

    /// A flow's process node's rows as they are being written: kept here, not in the
    /// project, while the check on saving refuses them — a row just added has nothing in
    /// it yet — with why (overview §13).
    struct RowsDraft: Equatable {
        var schemaId: String
        /// The process node whose rows these are.
        var node: String
        var steps: [Step]
        var problem: String
    }
    private(set) var rowsDraft: RowsDraft? {
        didSet { if rowsDraft != oldValue { draftGeneration += 1 } }
    }
    /// Changes whenever the draft does, for what draws it — the flow's diagram draws
    /// a process node with its draft rows, and is only redrawn when told.
    private(set) var draftGeneration = 0

    /// A process node's rows as the flow's diagram shows them: the draft, when one is
    /// being written.
    func rows(of flow: SchemaNode, node: String) -> [Step] {
        if let draft = rowsDraft, draft.schemaId == flow.id, draft.node == node { return draft.steps }
        return saved(flow, node: node)
    }

    private func saved(_ flow: SchemaNode, node: String) -> [Step] {
        flow.nodes.first { $0.name == node }?.steps ?? []
    }

    /// A flow as its diagram draws it: a process node with its draft rows.
    func withDraft(_ flow: SchemaNode) -> SchemaNode {
        guard let draft = rowsDraft, draft.schemaId == flow.id,
              let index = flow.nodes.firstIndex(where: { $0.name == draft.node }) else { return flow }
        var drawn = flow
        drawn.nodes[index].steps = draft.steps
        return drawn
    }

    /// Change a process node's rows and save them. What the check refuses stays as a
    /// draft, with the reason, until it is put right or thrown away.
    func editRows(_ id: String, node: String, _ change: (inout [Step]) -> Void) {
        guard core != nil, let projectId = detail?.project.id, let flow = schema(id),
              var held = flow.nodes.first(where: { $0.name == node }) else { return }
        var steps = rows(of: flow, node: node)
        change(&steps)
        if steps == held.steps {
            rowsDraft = nil
            return
        }
        let was = held
        held.steps = steps
        do {
            // The node is a card inside its flow. One row's own keys are written on the row
            // (`steps[n]`, only the keys that changed); a change of the lines, or of more than one row,
            // on the node, as the lines that came and went and the rows.
            try ownWrite {
                guard let value = try nodeChange(flow, from: was, to: held) else { return }
                let place = try PathText.member(cardPath(id), node)
                if value.keys.allSatisfy({ $0 == "steps" }), let row = NodeWrite.rowEdit(from: was, to: held) {
                    try tool("card_update", target: "\(place).steps[\(row.index)]", value: row.value)
                } else {
                    try tool("card_update", target: place, value: value)
                }
            }
            rowsDraft = nil
            lastError = nil
            readWritten(projectId)
        } catch {
            rowsDraft = RowsDraft(schemaId: id, node: node, steps: steps, problem: error.localizedDescription)
        }
    }

    /// One change to a process node's rows, by their places (`steps[n]`).
    enum RowChange: Equatable {
        /// A blank row of this `do`, put where `at` says; the rows from there on move one place on.
        case add(op: String, at: Int)
        /// The row at `from`, put at `to`.
        case move(from: Int, to: Int)
        /// The row at `index`, with the lines that leave it.
        case remove(at: Int)

        /// The rows after the change.
        func applied(to steps: [Step]) -> [Step] {
            var out = steps
            switch self {
            case .add(let op, let at):
                out.insert(StepEditing.blank(op), at: min(max(at, 0), out.count))
            case .move(let from, let to):
                guard out.indices.contains(from), to >= 0, to < out.count else { return out }
                out.insert(out.remove(at: from), at: to)
            case .remove(let index):
                guard out.indices.contains(index) else { return out }
                out.remove(at: index)
            }
            return out
        }
    }

    /// A process node's row added, moved or let go of, as one write of the row itself
    /// (`card_create`, `card_update` of `position`, `card_delete` on `<node>.steps[n]`): the
    /// core carries the lines that leave the rows, and the paths that name them, to their new
    /// places, which a rebuild of the node's lines here would not. While a draft is being written
    /// the change is made on the draft alone — its rows are not the project's places — and
    /// saved with the draft.
    func changeRows(_ id: String, node: String, _ change: RowChange) {
        guard core != nil, let projectId = detail?.project.id, let flow = schema(id),
              flow.nodes.contains(where: { $0.name == node }) else { return }
        let shown = rows(of: flow, node: node)
        let steps = change.applied(to: shown)
        if case .move(let from, let to) = change, from == to || !shown.indices.contains(from) || !shown.indices.contains(to) {
            return
        }
        if case .remove(let index) = change, !shown.indices.contains(index) { return }
        if let draft = rowsDraft, draft.schemaId == id, draft.node == node {
            rowsDraft = RowsDraft(schemaId: id, node: node, steps: steps, problem: draft.problem)
            return
        }
        do {
            try ownWrite {
                let place = try PathText.member(cardPath(id), node)
                switch change {
                case .add(let op, let at):
                    try tool("card_create", target: "\(place).steps[\(at)]", value: NodeWrite.row(StepEditing.blank(op)))
                case .move(let from, let to):
                    try tool("card_update", target: "\(place).steps[\(from)]", value: ["position": to])
                case .remove(let index):
                    try tool("card_delete", target: "\(place).steps[\(index)]")
                }
            }
            rowsDraft = nil
            lastError = nil
            readWritten(projectId)
        } catch {
            rowsDraft = RowsDraft(schemaId: id, node: node, steps: steps, problem: error.localizedDescription)
        }
    }

    /// Back to the rows the project has.
    func discardRowsDraft() {
        if let draft = rowsDraft, let flow = schema(draft.schemaId), case .nodeStep(draft.node, let path)? = selectedRow,
           StepEditing.step(at: path, in: saved(flow, node: draft.node)) == nil {
            selectedRow = nil
        }
        rowsDraft = nil
    }

    /// A formatdoc's rows, all of them, written back — with the rows that moved or were
    /// renamed, so the paths into them follow. The picked row follows too.
    func setRows(_ noteId: String, _ rows: [FormatRow], moves: [(from: [String], to: [String])] = []) {
        write {
            try tool("card_update", target: cardPath(noteId), value: ["rows": try ScreenWire.encoded(rows)],
                     ["moves": moves.map { ["from": $0.from, "to": $0.to] }])
        } then: { [weak self] in
            self?.followRow(noteId, moves: moves)
        }
    }

    /// The row picked, with the rows as written read: moved, it is followed; gone, nothing is picked.
    private func followRow(_ noteId: String, moves: [(from: [String], to: [String])]) {
        guard case .formatRow(let dotted)? = selectedRow else { return }
        let picked = PathText.names(dotted)
        if let move = moves.first(where: { picked.starts(with: $0.from) }) {
            selectedRow = .formatRow(PathText.joinNames(move.to + picked.dropFirst(move.from.count)))
        } else if let note = schema(noteId), FormatRows.row(at: picked, in: note.rows) == nil {
            selectedRow = nil
        }
    }

    /// What a store keeps, all of it, in this order: its `holds` lines in place of the ones
    /// it has (`connections`), its other lines kept.
    func setHolds(_ schemaId: String, _ holds: [String]) {
        if case .hold(let index)? = selectedRow, index >= holds.count { selectedRow = nil }
        guard let store = schema(schemaId) else { return }
        write { try tool("card_update", target: cardPath(schemaId), value: ["connections": lines(store, replacing: Line.holds, with: holds)]) }
    }

    var stats: (schemas: Int, fields: Int, relations: Int, depth: Int) {
        guard let detail else { return (0, 0, 0, 0) }
        return (detail.schemas.count,
                detail.schemas.reduce(0) { $0 + countFields($1.fields) },
                detail.relations.count,
                detail.schemas.map { maxDepth($0.fields) }.max() ?? 0)
    }

    // MARK: - What the MCP server needs

    /// Set when an agent asks the window to look at something. The view clears it
    /// once the canvas has moved.
    var focusRequest: String?
    /// The row of `focusRequest`'s card to mark there (`CardCanvasView.rowKey`): a card's
    /// row in a section ghost, from the nav panel.
    var focusRow: String?
    /// A card to pick on the diagram without moving the view unless it is off screen —
    /// coming back from its editor.
    var revealRequest: String?

    /// Asked for from a right-click menu (the diagram's, a section's in the nav panel,
    /// a chart or document section's list): the window asks for the name. `kind` is
    /// the one to start on, or nil for the first the section holds.
    /// The kinds a new card can be in the section on screen, by name: the built-in ones it
    /// holds, then the project's own ones it holds — by its kind's definition (`holds`; `*` any),
    /// so the free section takes every one of the project's own.
    var offeredKinds: [String] {
        let section = shownSection
        let kind = section?.kind ?? (isBoardOpen ? .board : nil)
        let defined = (section?.kindName ?? (isBoardOpen ? SectionKind.board.rawValue : nil))
            .flatMap { detail?.sectionHolds[$0] }
        let builtin: [SchemaKind]
        switch (kind, defined) {
        case (let kind?, _): builtin = SchemaKind.made.filter(kind.holds.contains)
        // A kind of section the project defines: the built-in kinds its definition holds.
        case (nil, let holds?): builtin = SchemaKind.made.filter { holds.contains("*") || holds.contains($0.rawValue) }
        case (nil, nil): builtin = SchemaKind.made
        }
        let own = (detail?.cardKinds.own(heldBy: defined ?? (kind == nil ? ["*"] : [])) ?? []).map(\.kind)
        return builtin.map(\.rawValue) + own
    }

    struct NewSchemaRequest: Identifiable {
        let id = UUID()
        var kind: SchemaKind?
    }
    var newSchemaRequest: NewSchemaRequest?

    /// Pick the change up in the window. An edit made through MCP has to show, or
    /// the two views of the same data disagree until something else reloads.
    private func refresh(_ projectId: String) {
        if let core = core(of: projectId) { try? readProjects(core) }
        if detail?.project.id == projectId { select(projectId) }
    }

    // How the diagram lays its cards out (§12).

    /// How big the project's cards are, for the project on screen or one named.
    func setDefaultSettings(_ settings: LayoutSettings, projectId given: String? = nil) throws {
        guard let projectId = given ?? detail?.project.id, let core = core(of: projectId) else { return }
        let layout: [String: Any] = ["cardRows": settings.size.rows, "cardWidth": settings.size.widthWord,
                                     "margin": settings.margin, "docView": settings.docView]
        _ = try ownWrite {
            try core.request("tool", ["name": "project_update", "arguments": ["project": projectId, "value": ["layout": layout]]])
        }
        refresh(projectId)
        if detail?.project.id == projectId { layoutGeneration += 1 }
    }

    // MARK: - Sections

    /// The same three for the window, which parks a refusal in `lastError` rather
    /// than throwing it — and opens what it makes, like a new project.
    func createSection(name: String, parentId: String? = nil, kind: SectionKind? = nil) {
        guard core != nil, let projectId = detail?.project.id, let detail else { return }
        do {
            let parent = parentId ?? detail.directory.home
            let name = PathText.trimmed(name)
            let path = detail.sections.contains(where: { $0.id == parent && $0.parentId == nil })
                ? PathText.quote(name) : PathText.child(detail.directory.full(section: parent), name)
            // What it holds is its parent's (`section_create`): `kind` only says which was asked for.
            _ = kind
            try ownWrite { _ = try tool("section_create", target: path) }
            lastError = nil
            readWritten(projectId) { [weak self] in
                if let made = self?.detail?.sections.first(where: { $0.name == name && ($0.parentId ?? "") == parent }) {
                    self?.selectSection(made.id)
                }
            }
        } catch {
            lastError = error.localizedDescription
        }
    }

    /// Say what a section holds — and so what the sections inside it hold.
    /// A flow's nodes as the canvas left them: kept though unfinished — what is left
    /// to do is shown on the nodes — and refused only when wrong.
    /// `renamed`: a node renamed by this edit, from which name to which — the paths
    /// into it from elsewhere follow.
    ///
    /// Each node is a card inside the flow (`specboard://SpecBoard/Docs/Design:CardKinds`「flow のノード」), written
    /// with the tools one at a time — renamed (`card_update` of its `name`: the ways to it
    /// and the paths into it follow), made (`card_create` of `<flow>.<node>`), changed
    /// (`card_update` of what changed, its ways and what its rows name as its lines), let go
    /// of (`card_delete`: the ways to it from the others go with it).
    func setFlowNodes(_ flowId: String, _ nodes: [FlowNode], renamed: (from: String, to: String)? = nil,
                      then: (() -> Void)? = nil) {
        guard let flow = schema(flowId) else { return }
        // What the flow had, as this edit sees it: a node renamed by its new name and the ways
        // to it with it, the ways to a node let go of gone with it (the core takes them away).
        var before = flow.nodes
        if let renamed { before = (try? FlowEditing.rename(renamed.from, to: renamed.to, in: before)) ?? before }
        let wanted = Set(nodes.map(\.name))
        let removed = before.map(\.name).filter { !wanted.contains($0) }
        for name in removed { before = FlowEditing.remove(name, from: before) }
        let was = Dictionary(before.map { ($0.name, $0) }, uniquingKeysWith: { a, _ in a })
        write {
            let flowPath = try cardPath(flowId)
            if let renamed, renamed.from != renamed.to {
                try tool("card_update", target: PathText.member(flowPath, renamed.from), value: ["name": renamed.to])
            }
            // Each new node made with its lines to what is there (what it reads and writes,
            // whom it sends to, its keys, the nodes already in the flow); a way to a node not
            // made yet is written once that node is (the core refuses a line to a card that is
            // not there).
            var there = Set(was.keys)
            var waiting: Set<String> = []
            for node in nodes where was[node.name] == nil {
                var value = nodeMade(flow, node)
                let lines = value["connections"] as? [[String: Any]] ?? []
                let ready = lines.filter { NodeWrite.ready($0, there: there) }
                if ready.isEmpty { value.removeValue(forKey: "connections") } else { value["connections"] = ready }
                if ready.count < lines.count { waiting.insert(node.name) }
                try tool("card_create", target: PathText.member(flowPath, node.name), value: value)
                there.insert(node.name)
            }
            for node in nodes {
                let value: [String: Any]?
                if let old = was[node.name] {
                    value = try nodeChange(flow, from: old, to: node)
                } else if waiting.contains(node.name) {
                    value = nodeMade(flow, node)["connections"].map { ["connections": $0] }
                } else {
                    value = nil
                }
                guard let value else { continue }
                try tool("card_update", target: PathText.member(flowPath, node.name), value: value)
            }
            for name in removed {
                try tool("card_delete", target: PathText.member(flowPath, name))
            }
        } then: { [weak self] in
            self?.followNodes(flowId, renamed: renamed)
            then?()
        }
    }

    /// A node of `kind` put on a flow's card map by hand (`FlowEditing.add`): right of `after`, or in the
    /// first free place; picked, and brought into view on `map` once drawn. Returns its name.
    @discardableResult
    func addNode(_ kind: String, after: String?, to flow: SchemaNode, showingOn map: CardCanvasController? = nil) -> String {
        let added = FlowEditing.add(kind, after: after, to: flow.nodes, margin: defaultSettings.margin)
        setFlowNodes(flow.id, added.nodes) { map?.revealAfterNextShow(FlowCardMap.id(added.name)) }
        selectNode(flow.id, added.name)
        return added.name
    }

    /// The flow's nodes as written, read: a node renamed takes its picked row and its draft along; one no
    /// longer there lets go of them.
    private func followNodes(_ flowId: String, renamed: (from: String, to: String)?) {
        guard selectedSchemaId == flowId || rowsDraft?.schemaId == flowId else { return }
        let names = Set(schema(flowId)?.nodes.map(\.name) ?? [])
        if let renamed {
            if case .node(renamed.from)? = selectedRow { selectedRow = .node(renamed.to) }
            if case .nodeStep(renamed.from, let path)? = selectedRow { selectedRow = .nodeStep(renamed.to, path) }
            if rowsDraft?.schemaId == flowId, rowsDraft?.node == renamed.from { rowsDraft?.node = renamed.to }
        }
        switch selectedRow {
        case .node(let name)? where selectedSchemaId == flowId && !names.contains(name),
             .nodeStep(let name, _)? where selectedSchemaId == flowId && !names.contains(name):
            selectedRow = nil
        default:
            break
        }
        if let draft = rowsDraft, draft.schemaId == flowId, !names.contains(draft.node) { rowsDraft = nil }
    }

    /// A flow's node as its card is made (`NodeWrite.made`), the paths its lines go to read
    /// from the flow's section.
    private func nodeMade(_ flow: SchemaNode, _ node: FlowNode) -> [String: Any] {
        guard let directory = detail?.directory else { return NodeWrite.made(node) { ($0, nil) } }
        let base = directory.sectionId(of: flow)
        return NodeWrite.made(node) { directory.lineEnd($0, from: base) }
    }

    /// What changed of a flow's node, as `card_update` writes it on the node's card.
    private func nodeChange(_ flow: SchemaNode, from was: FlowNode, to now: FlowNode) throws -> [String: Any]? {
        guard let directory = detail?.directory else { return nil }
        let base = directory.sectionId(of: flow)
        return NodeWrite.changed(from: was, to: now) { directory.lineEnd($0, from: base) }
    }

    /// The shape a flow is drawn in unless asked otherwise.
    func setDiagram(_ flowId: String, _ shape: String) {
        write { try tool("card_update", target: cardPath(flowId), value: ["diagram": shape]) }
    }

    /// Picked on the flow's diagram, or let go of.
    func selectNode(_ flowId: String, _ name: String?) {
        if let name { selectRow(flowId, .node(name)) } else { clearRow() }
    }

    func renameSection(_ id: String, to name: String) {
        write { try tool("section_update", target: sectionPath(id), value: ["name": name]) }
    }

    /// A section's description, from the property panel.
    func describeSection(_ id: String, _ text: String) {
        write { try tool("section_update", target: sectionPath(id), value: ["description": text]) }
    }

    // MARK: - Branches (`specboard://SpecBoard/Docs/Design:DesignBranches` §2・§11)

    /// The branches of the project on screen, in order.
    private(set) var branches: [BranchInfo] = []

    /// Which cards of what is open the branch changed, made, or saw main change.
    private(set) var branchMarks = BranchMarks()

    /// Whether what a branch did is marked on what is open: a branch, or what a merge would leave —
    /// not main (put right in one go under its lock too), nor the board.
    var showsMarks: Bool {
        guard let projectId = detail?.project.id, let open = openTrees[projectId] else { return false }
        return open != Board.id && open != groupBranches[projectId]
    }

    /// The marks drawn on what is open: none where nothing is marked (`showsMarks`).
    var shownMarks: BranchMarks { showsMarks ? branchMarks : BranchMarks() }

    /// The tree the project on screen is open in: main (nil), a branch, or the project board
    /// (`Board.id`), with the name a person and MCP call it by.
    var openTree: (id: String?, name: String) {
        if isBoardOpen { return (Board.id, Board.word) }
        if let shown = resultView { return (shown.branch, ResultView.word) }
        // A locked main put right in one go: main, to a person, in a branch of the app's own.
        if isLockedGroupOpen, let projectId = detail?.project.id { return (groupBranches[projectId], SchemaDirectory.mainWord) }
        if let branch = openBranch { return (branch.id, branch.name) }
        return (nil, SchemaDirectory.mainWord)
    }

    /// The project board open, at its root: its tree in the nav panel, its dashboard on
    /// screen. Back to main (or a branch) with `openBranch`, at that tree's root.
    func openBoard() {
        openBranch(Board.id)
        openRootOfOpenTree()
    }

    /// What is open went with the tree it was in: that tree's root instead.
    func openRootOfOpenTree() {
        guard let root = detail?.sections.first(where: { $0.parentId == nil })?.id else { return }
        open(rootPlace(root))
    }

    /// Whether the project on screen is open in its board (`specboard://SpecBoard/Docs/Design:DesignBoard`).
    var isBoardOpen: Bool {
        guard let projectId = detail?.project.id else { return false }
        return openTrees[projectId] == Board.id
    }

    /// The tree each project is open in: a branch's id or `Board.id`; main is not kept. Watched:
    /// what reads the tree open — the nav panel's branches, the tree, the bar — is drawn again
    /// when it changes, even when the tree read is the same as the one before (a branch that
    /// changed nothing yet reads as main does, and its project is not told as changed).
    var openTrees: [String: String] = [:]

    /// The open tree of the project on screen as a tool is told it: a branch by its name,
    /// the board by its word, main not at all.
    var openTreeArgument: [String: Any] {
        guard let projectId = detail?.project.id, let open = openTrees[projectId] else { return [:] }
        if open == Board.id { return ["branch": Board.word] }
        if open == groupBranches[projectId] { return ["branch": Self.groupBranchName] }
        return branches.first { $0.id == open }.map { ["branch": $0.name] } ?? [:]
    }

    /// The branch the project on screen is open in; nil for main — and for the board,
    /// which is no branch (`openTree`).
    var openBranch: BranchInfo? {
        guard let projectId = detail?.project.id, let open = openTrees[projectId] else { return nil }
        return branches.first { $0.id == open }
    }

    private static func branchKey(_ projectId: String) -> String { "branch.open.\(projectId)" }

    /// Open a branch of the project on screen — nil for main. What is open stays with the
    /// project, the next time too; the card and section on screen stay where they are,
    /// which they are in both.
    func openBranch(_ branchId: String?) {
        guard let projectId = detail?.project.id else { return }
        // Another tree opened: what a merge would leave, looked at, let go.
        if resultView != nil {
            resultView = nil
            try? core?.closeMergeResult(projectId)
        }
        // main, while it is put right in one go under its lock: the branch it is put right in.
        if branchId == nil, let group = groupBranches[projectId] {
            openTrees[projectId] = group
            UserDefaults.standard.removeObject(forKey: Self.branchKey(projectId))
            select(projectId)
            return
        }
        openTrees[projectId] = branchId
        UserDefaults.standard.set(branchId, forKey: Self.branchKey(projectId))
        select(projectId)
    }

    // MARK: - main's lock (`specboard://SpecBoard/Docs/Design:DesignBranches` §8)

    /// Whether the project on screen has main locked.
    var isMainLocked: Bool {
        guard let id = detail?.project.id else { return false }
        return projects.first { $0.id == id }?.isLocked ?? false
    }

    /// main is open and locked: its contents are read here, not written.
    var isLockedMainOpen: Bool {
        isMainLocked && openBranch == nil && !isBoardOpen && resultView == nil && !isLockedGroupOpen
    }

    /// Locked or let go, from the window only.
    func setMainLock(_ on: Bool) {
        guard let core, let projectId = detail?.project.id else { return }
        do {
            _ = try ownWrite { try core.request("main_lock", ["project": projectId, "on": on]) }
            lastError = nil
            try readProjects(core)
        } catch {
            lastError = error.localizedDescription
        }
        select(projectId)
    }

    /// An AI's merges into the locked main let in without being approved, or held for a person
    /// again (`AppModel+AIMerge`), from the project's settings only.
    func setAIMerge(_ on: Bool) {
        guard let core, let projectId = detail?.project.id else { return }
        do {
            _ = try ownWrite { try core.request("main_ai_merge", ["project": projectId, "on": on]) }
            lastError = nil
            try readProjects(core)
        } catch {
            lastError = error.localizedDescription
        }
        select(projectId)
    }

    // MARK: - Requests to merge into main (`specboard://SpecBoard/Docs/Design:DesignBranches` §8)

    /// The open project's requests, newest first. Loaded with the project, and again after
    /// each MCP write.
    private(set) var mergeRequests: [MergeRequest] = []

    /// The project's working directories, as set and as read on the machine its project is kept on
    /// (`specboard://SpecBoard/Docs/Design:Overview` §10): read with the project.
    var workDirs: [WorkDir] = []
    /// Main's rules for what to write at each place (`WritingRules`): its Rules section's.
    var writingRules: [String: String] = [:]
    /// The exports (`export_read`), each in full, read with the project and again
    /// after each change to them; and each file of a working directory they write, by the file card's id
    /// (`file:<directory>/<path>`): which export, and how it stands.
    var exports = ExportsRead()
    var exportMarks: [String: ExportMark] = [:]

    /// How many are open and can be approved as they are.
    var openMergeRequests: Int { mergeRequests.filter { $0.state == .open || $0.state == .stale }.count }

    /// Approved on screen: merged into main as it was tried, the boxes the branch moved
    /// along when `places` says so (the window's toggle, which starts as the request asks).
    func approveMergeRequest(_ number: Int, places: Bool) {
        historyStep { core, projectId, _ in try core.approveMergeRequest(projectId, number: number, places: places) }
    }

    /// Rejected on screen, saying why.
    func rejectMergeRequest(_ number: Int, reason: String) {
        historyStep { core, projectId, _ in
            _ = try core.request("merge_request_reject", ["project": projectId, "number": number, "reason": reason])
        }
    }

    // MARK: - The history (`specboard://SpecBoard/Docs/Design:DesignBranches` §7)

    /// The open branch as it was right after commit `seq`: everything after it undone,
    /// as a commit of its own. main is not written: a branch is made of it as it was, and the merge
    /// window opened to take it in — through a merge request while main is locked.
    func revert(to seq: Int) {
        var made: String?
        historyStep { core, projectId, branch in
            let said = try core.request("revert", ["project": projectId, "branch": branch ?? "", "to": seq]) as? [String: Any]
            if branch == nil { made = said?["branch"] as? String }
        }
        if let made { askToMerge(from: made, into: nil) }
    }

    /// One commit of the open tree taken back alone. On a branch or the board it is a commit of its own;
    /// main is not written: a branch is made of it and the merge window opened to take it in. Where a
    /// later commit changed the same places nothing is written, and the window listing them opens.
    func undo(commit seq: Int) {
        var made: String?
        var clash: UndoRefusal?
        historyStep { core, projectId, branch in
            switch try core.undoCommit(projectId, branch: branch, seq: seq) {
            case .done(let branch): made = branch
            case .clashed(let conflicts, let later): clash = UndoRefusal(seq: seq, conflicts: conflicts, later: later)
            }
        }
        if let clash { undoRefusal = clash }
        if let made { askToMerge(from: made, into: nil) }
    }

    /// The window that lists where an undo clashed with later commits (`UndoConflictsSheet`).
    var undoRefusal: UndoRefusal?

    /// Whether main is the tree open: an undo there makes a branch.
    var isMainTreeOpen: Bool {
        guard let projectId = detail?.project.id else { return false }
        return openTrees[projectId] == nil
    }

    /// Whether one commit of the open tree may be undone: where going back may be, and on the newest
    /// commit too (going back to just after it is nothing; undoing it is not).
    var mayUndoCommit: Bool { mayGoBack }

    /// Whether the open tree's history may be gone back in: main's while it is locked too (a branch
    /// is made, merged in later).
    var mayGoBack: Bool {
        !isReadOnly || (isLockedMainOpen && !isArchived && heldByMCP.isEmpty && heldElsewhere.isEmpty)
    }

    /// A name on a commit of the open branch; empty takes it off.
    func nameCommit(_ seq: Int, _ name: String) {
        historyStep { core, projectId, branch in
            _ = try core.request("name_commit", ["project": projectId, "branch": branch ?? "", "seq": seq, "name": name])
        }
    }

    /// Every commit of the open branch before `seq` let go.
    func cut(before seq: Int) {
        historyStep { core, projectId, branch in
            _ = try core.request("cut", ["project": projectId, "branch": branch ?? "", "before": seq])
        }
    }

    /// What a commit of the open branch changed.
    func commitChanges(_ seq: Int) -> [CommitChange] {
        guard let core, let projectId = detail?.project.id else { return [] }
        return (try? core.commitChanges(projectId, branch: openTrees[projectId], seq: seq)) ?? []
    }

    /// How much of the store the project on screen takes.
    func projectSize() -> ProjectSize? {
        guard let core, let projectId = detail?.project.id else { return nil }
        return try? core.projectSize(projectId)
    }

    /// How much of the history a cut before `seq` would let go: commits, and bytes.
    func cutCost(before seq: Int) -> (commits: Int, bytes: Int) {
        guard let core, let projectId = detail?.project.id else { return (0, 0) }
        return (try? core.cutCost(projectId, branch: openTrees[projectId], before: seq)) ?? (0, 0)
    }

    private func historyStep(_ body: (CoreClient, String, String?) throws -> Void) {
        guard let core, let projectId = detail?.project.id else { return }
        do {
            try ownWrite { try body(core, projectId, openTrees[projectId]) }
            lastError = nil
            try readProjects(core)
        } catch {
            lastError = error.localizedDescription
        }
        select(projectId)
    }

    // MARK: - Putting a branch right in one go (`specboard://SpecBoard/Docs/Design:DesignBranches` §5)

    /// The window's "まとめて直す" on the open branch, while it lasts: this Mac's.
    var group: Hold? { holds.first { !$0.byMCP && $0.client == (core?.client ?? "") } }

    /// The MCP batches holding the open branch: while there are any, it is read only here.
    var heldByMCP: [Hold] { holds.filter(\.byMCP) }

    /// Another Mac's window putting the open branch right in one go (an agent's project): read
    /// only here until it is done.
    var heldElsewhere: [Hold] { holds.filter { !$0.byMCP && $0.client != (core?.client ?? "") } }

    /// What the project holds may not be changed here: an archived project, a branch an MCP batch or
    /// another Mac holds, or a locked main (where things sit, tags and card ghosts still move).
    var isReadOnly: Bool {
        isArchived || !heldByMCP.isEmpty || !heldElsewhere.isEmpty || isLockedMainOpen || resultView != nil
    }

    /// Whether "まとめて直す" may begin on what is open: where it may be written, and on a locked
    /// main too (in a branch of the app's own, `groupBranches`) — not while one goes on.
    var canBeginGroup: Bool {
        group == nil && heldElsewhere.isEmpty
            && (!isReadOnly || (isLockedMainOpen && !isArchived && heldByMCP.isEmpty))
    }

    /// Start: what is written in the open branch until it ends is one commit, and MCP
    /// may not write there. On a locked main, it is written in a branch of the app's own the
    /// core makes, opened as main is, and merged into main when it ends.
    func beginGroup() {
        groupStep { core, projectId, branch in
            let begun = try core.request("group_begin", ["project": projectId, "branch": branch ?? ""]) as? [String: Any]
            if let group = begun?["branch"] as? String {
                groupBranches[projectId] = group
                openTrees[projectId] = group
            }
        }
    }

    /// Done: what was written stays — on a locked main, merged into it.
    func endGroup() {
        groupStep { core, projectId, branch in
            _ = try core.request("group_end", ["project": projectId, "branch": branch ?? ""])
            letGroupGo(projectId)
        }
    }

    /// Given up: the branch goes back to where it began — on a locked main, the branch of the
    /// app's own just goes.
    func abandonGroup() {
        groupStep { core, projectId, branch in
            _ = try core.request("group_abandon", ["project": projectId, "branch": branch ?? ""])
            letGroupGo(projectId)
        }
    }

    /// A locked main's group over: main open again, if its branch was.
    private func letGroupGo(_ projectId: String) {
        guard let group = groupBranches.removeValue(forKey: projectId) else { return }
        if openTrees[projectId] == group { openTrees[projectId] = nil }
    }

    /// The branch a locked main is put right in one go in, by the project (`beginGroup`): the
    /// core's, named only by the window. Not kept: when the app opens, the core merges one left.
    var groupBranches: [String: String] = [:]

    /// What the window calls that branch in the tools (`GROUP_BRANCH` of the core).
    static let groupBranchName = "~group"

    /// Whether the project on screen is open in its locked main's group branch.
    var isLockedGroupOpen: Bool {
        guard let projectId = detail?.project.id, let group = groupBranches[projectId] else { return false }
        return openTrees[projectId] == group
    }

    /// The MCP batches holding the open branch, discarded: a person's hand comes first.
    func cancelBatches() {
        groupStep { core, projectId, branch in _ = try core.request("batches_cancel", ["project": projectId, "branch": branch ?? ""]) }
    }

    private func groupStep(_ body: (CoreClient, String, String?) throws -> Void) {
        guard let core, let projectId = detail?.project.id else { return }
        do {
            try ownWrite { try body(core, projectId, openTrees[projectId]) }
            lastError = nil
        } catch {
            lastError = error.localizedDescription
        }
        select(projectId)
    }

    /// A branch made from main as it is now, and opened.
    func createBranch(name: String) {
        guard let core, let projectId = detail?.project.id else { return }
        do {
            try ownWrite { _ = try tool("branch_create", target: name, inTree: false) }
            lastError = nil
            guard let id = try core.branches(projectId).first(where: { $0.name == name })?.id else { return }
            openBranch(id)
        } catch {
            lastError = error.localizedDescription
        }
    }

    /// The branches read again, nothing else: after a write that made or deleted one beside what it
    /// wrote in the tree open (`card_action`).
    func readBranchesAgain() {
        guard let core, let projectId = detail?.project.id, let read = try? core.branches(projectId) else { return }
        branches = read
    }

    func renameBranch(_ id: String, to name: String) {
        guard let projectId = detail?.project.id, let old = branches.first(where: { $0.id == id })?.name else { return }
        do {
            try ownWrite { _ = try tool("branch_update", target: old, value: ["name": name], inTree: false) }
            lastError = nil
            select(projectId)
        } catch {
            lastError = error.localizedDescription
        }
    }

    /// The branch let go, with all it changed. Where it was open, main opens.
    func deleteBranch(_ id: String) {
        guard let projectId = detail?.project.id, let name = branches.first(where: { $0.id == id })?.name else { return }
        do {
            try ownWrite { _ = try tool("branch_delete", target: name, value: ["discard": true], inTree: false) }
            if openTrees[projectId] == id {
                openTrees[projectId] = nil
                UserDefaults.standard.removeObject(forKey: Self.branchKey(projectId))
            }
            lastError = nil
            select(projectId)
        } catch {
            lastError = error.localizedDescription
        }
    }

    /// How many cards and sections the branch changed, made or removed.
    func branchChangeCount(_ id: String) -> Int {
        guard let diff = diff(of: id) else { return 0 }
        return diff.changed.count + diff.made.count + diff.removed.count
            + diff.sectionsChanged.count + diff.sectionsMade.count + diff.sectionsRemoved.count
    }

    /// What a branch changed, and what main did since it was made (`branch_read`, its answer
    /// under `json` as every tool's to the window is).
    private func diff(of branchId: String) -> BranchDiff? {
        guard let name = branches.first(where: { $0.id == branchId })?.name,
              case .object(let answer)? = try? tool("branch_read", target: name, inTree: false),
              case .object(let read)? = answer["json"] else { return nil }
        func strings(_ key: String) -> [String] {
            guard case .array(let items)? = read[key] else { return [] }
            return items.compactMap { if case .string(let text) = $0 { text } else { nil } }
        }
        func cards(_ key: String) -> [BranchDiff.Card] {
            guard case .array(let items)? = read[key] else { return [] }
            return items.compactMap { item in
                guard case .object(let row) = item, case .string(let path)? = row["card"] else { return nil }
                var said: [String] = []
                if case .array(let lines)? = row["changes"] {
                    said = lines.compactMap { if case .string(let text) = $0 { text } else { nil } }
                }
                return BranchDiff.Card(path: path, changes: said)
            }
        }
        var diff = BranchDiff()
        diff.changed = cards("changed")
        diff.made = strings("made")
        diff.removed = strings("removed")
        diff.sectionsChanged = strings("sectionsChanged")
        diff.sectionsMade = strings("sectionsMade")
        diff.sectionsRemoved = strings("sectionsRemoved")
        diff.mainChanged = cards("mainChanged")
        diff.mainMade = strings("mainMade")
        diff.mainRemoved = strings("mainRemoved")
        diff.overlaps = strings("overlaps")
        if case .array(let rows)? = read["elsewhere"] {
            diff.elsewhere = rows.compactMap { item in
                guard case .object(let row) = item, case .string(let path)? = row["path"] else { return nil }
                guard case .array(let names)? = row["branches"] else { return (path, []) }
                return (path, names.compactMap { if case .string(let text) = $0 { text } else { nil } })
            }
        }
        return diff
    }

    /// A merge the window is to show (`MergeSheet`): from where into where, branch ids,
    /// nil for main, and whether the boxes the branch moved go too (off unless said).
    struct MergeAsk: Identifiable {
        let id = UUID()
        var source: String?
        var target: String?
        var places = false
    }
    var mergeAsk: MergeAsk?

    /// What a merge into main would leave, open on screen (`AppModel+MergeResult`); nil when none is.
    var resultView: ResultView?

    /// Open the merge window: from the open branch into main by default.
    func askToMerge(from source: String?, into target: String?) {
        mergeAsk = MergeAsk(source: source, target: target)
    }

    /// What merging `source` into `target` would do, in the project on screen.
    func mergePreview(source: String?, target: String?, take: CoreClient.MergeTake = [:]) -> MergePreview? {
        guard let core, let projectId = detail?.project.id else { return nil }
        do {
            return try core.mergePreview(projectId, source: source, target: target, take: take)
        } catch {
            lastError = error.localizedDescription
            return nil
        }
    }

    /// The merge, written. Nil, with why in `lastError`, when it was refused. With `places`
    /// (the merge window's "branch で動かした位置も main に入れる"), every box the source
    /// branch moved goes in over the target's.
    /// Into main, the branch is set aside unless `keep` (the window's "branch を残す"):
    /// out of the list, and the window back on main when it was open.
    @discardableResult
    func merge(source: String?, target: String?, take: CoreClient.MergeTake, places: Bool = false,
               keep: Bool = false) -> MergeResult? {
        guard let core, let projectId = detail?.project.id else { return nil }
        do {
            let result = try ownWrite {
                try core.merge(projectId, source: source, target: target, take: take, places: places, keep: keep)
            }
            lastError = nil
            select(projectId)
            return result
        } catch {
            lastError = error.localizedDescription
            return nil
        }
    }

    /// What the open branch changed, and what main did since it was made — worked out
    /// once for what is on screen, for the dashboard and the card's details alike.
    func branchDiff() -> BranchDiff? {
        guard let open = openBranch else { return nil }
        if let kept = diffKept, kept.branch == open.id, kept.generation == dataGeneration { return kept.diff }
        guard let diff = diff(of: open.id) else {
            return nil
        }
        diffKept = (open.id, dataGeneration, diff)
        return diff
    }

    @ObservationIgnored private var diffKept: (branch: String, generation: Int, diff: BranchDiff)?

    /// The branch the project opens in: the one it was last open in, if it is still there.
    private func restoreBranch(_ projectId: String) {
        guard openTrees[projectId] == nil, !restored.contains(projectId) else { return }
        restored.insert(projectId)
        guard let kept = UserDefaults.standard.string(forKey: Self.branchKey(projectId)) else { return }
        if kept == Board.id
            || ((try? core(of: projectId)?.branches(projectId)) ?? []).contains(where: { $0.id == kept }) {
            openTrees[projectId] = kept
        } else {
            UserDefaults.standard.removeObject(forKey: Self.branchKey(projectId))
        }
    }
    /// Projects whose open tree was looked up from the last run: once, when first shown.
    @ObservationIgnored private var restored: Set<String> = []

    // MARK: - Card ghosts (`specboard://SpecBoard/Docs/Design:DesignSections` §2)

    /// A name asked for by what the window does, not by one of its views — a card ghost
    /// whose name its section has already (`placeGhost`): the window shows it in a sheet
    /// (`ContentView`).
    var nameAsk: NameAsk?
    /// A goal's cancel that would throw away what its branch has and main does not, asked first
    /// (`pressCardAction`): the window shows it in an alert (`ContentView`).
    var discardAsk: CardDiscardAsk?

    /// Whether how a card looks may be changed here — a value object or an enum shown on its
    /// own (`standalone`): not kept in commits, so a locked main takes it too; not an
    /// archived project, nor a branch an MCP batch holds.
    var canSetLooks: Bool { !isArchived && heldByMCP.isEmpty }

    /// A card ghost of `target` — a card, or another ghost — put in `section` under the name
    /// of what it points at, or, where the section has that name already, under a name
    /// asked for (`nameAsk`). Not where the project is read only: a locked main makes no
    /// ghosts (`specboard://SpecBoard/Docs/Design:DesignBranches` §8).
    func placeGhost(of target: String, in section: String) {
        guard !isReadOnly, let detail else { return }
        let directory = detail.directory
        guard let name = schema(target)?.name ?? ghost(target)?.name else { return }
        guard directory.entry(in: section, name: name) != nil else {
            makeGhost(of: target, in: section, name: name)
            return
        }
        let place = directory.full(section: section)
        nameAsk = NameAsk(
            title: L10n[.Model.ghostNameTitle],
            message: L10n[.Model.ghostNameMessage(place, name)],
            placeholder: L10n[.Common.wordName], start: name, confirm: L10n[.Model.ghostNameConfirm],
            cancel: L10n[.Common.actionCancel], pathName: true) { [weak self] typed in
                self?.makeGhost(of: target, in: section, name: typed)
            }
    }

    /// A card ghost made (`card_create` of the kind `ghost`), and picked where it is drawn.
    func makeGhost(of target: String, in section: String, name: String) {
        guard let detail else { return }
        let directory = detail.directory
        guard let pointed = schema(target).map(directory.full) ?? ghost(target).map(directory.full) else { return }
        let name = PathText.trimmed(name)
        let path = PathText.cardPath(directory.full(section: section), name)
        write { try tool("card_create", target: path, value: ["kind": "ghost", "target": pointed]) } then: { [weak self] in
            guard let self, let made = self.detail?.directory.entry(in: section, name: name),
                  case .ghost(let ghost) = made else { return }
            self.perform(.show(card: ghost.id, section: section, focus: ghost.id, row: nil))
        }
    }

    /// A card ghost given another name (`card_update`'s `name`). What points at it by its
    /// path follows (the core rewrites them).
    func renameGhost(_ id: String, to name: String) {
        write { try tool("card_update", target: ghostPath(id), value: ["name": name]) }
    }

    /// A card ghost pointed at something else (`card_update`'s `target`): a path, in full,
    /// to a card or another ghost — of the kind the ghost leads to now (the core refuses
    /// another kind, a ring, and its own section).
    func retargetGhost(_ id: String, to path: String) {
        write { try tool("card_update", target: ghostPath(id), value: ["target": path]) }
    }

    /// A card ghost let go (`card_delete`): the card it points at stays.
    func deleteGhost(_ id: String) {
        if selectedSchemaId == id { deselect() }
        write { try tool("card_delete", target: ghostPath(id)) }
    }

    /// A value object or an enum shown on its own section's diagram as a card of its own,
    /// or no longer (`card_update`'s `standalone`): how it looks — no commit, and a locked
    /// main takes it too (`specboard://SpecBoard/Docs/Design:DesignBranches` §8).
    func setStandalone(_ id: String, _ on: Bool) {
        guard canSetLooks else { return }
        write { try tool("card_update", target: cardPath(id), value: ["standalone": on]) }
    }

    /// A card ghost picked: on the diagram, in the property panel.
    /// A card ghost picked — on its section's diagram, opened there when `follow` and another is shown
    /// (as a card is: `app_control`'s select).
    func selectGhost(_ id: String, follow: Bool = true) {
        if follow, let detail, let ghost = ghost(id) {
            let section = detail.directory.sectionId(of: ghost)
            if section != shownSectionKey { selectSection(section) }
        }
        selectedSchemaId = id
        selectedFieldId = nil
        selectedRow = nil
        selectedLine = nil
    }

    /// The card ghost picked, if what is picked is one.
    var selectedGhost: CardGhost? { selectedSchemaId.flatMap(ghost) }

    /// A card ghost as the tools name it: in full.
    private func ghostPath(_ id: String) throws -> String {
        guard let detail, let ghost = ghost(id) else { throw CoreClient.Refusal(L10n[.Model.errorNoGhost]) }
        return detail.directory.full(ghost)
    }

    func deleteSection(_ id: String) {
        write { try tool("section_delete", target: sectionPath(id)) }
    }

    /// Move a schema to another section, from the property panel or dropped on a section in the nav
    /// panel — into the section on screen, brought into view there once drawn.
    func moveSchema(_ id: String, toSection section: String) {
        let onScreen = here == .section(section)
        write { try tool("card_update", target: cardPath(id), value: ["section": sectionPath(section)]) } then: { [weak self] in
            guard let self, let schema = self.schema(id), let detail = self.detail else { return }
            self.selectSection(detail.directory.sectionId(of: schema))
            self.selectSchema(id)
            if onScreen { self.bringIntoView(id, in: section) }
        }
    }

    /// A card a person put in `section`: picked on the relation panel, and the view moved to take it
    /// whole once it is drawn — while that panel is what is on screen; elsewhere (a card's editor, a
    /// dashboard) it would move a diagram no one is looking at.
    private func bringIntoView(_ id: String, in section: String) {
        guard here == .section(section) else { return }
        relationCanvas?.revealAfterNextShow(id)
    }

    // MARK: - The nav panel's presses and drags (overview §8)

    /// A row of the nav panel pressed (`NavPanelTree.press`); opening and shutting it in
    /// the tree is the tree's own.
    func perform(_ press: NavPanelTree.Press) {
        switch press {
        case .openSection(let id, let root):
            pick(section: id, root: root)
        case .show(let card, let section, let focus, let row):
            // The relation panel of the section it is drawn on, the card picked there, and
            // the view brought to it once the panel has drawn it (`ContentView`).
            open(.section(section))
            selectSchema(card, follow: false)
            // A card inside a goal is the goal's row: picked as that row, as a press on it on the
            // diagram picks it — the property panel shows the card inside.
            if let row, let schema = detail?.directory.schema(id: card), schema.kind.isCustom,
               CustomRows.row(at: row, in: schema.customRows) != nil {
                selectRow(card, .customRow(row))
            }
            // A function or type of a file: picked as its card's row (not a row of a section ghost it is
            // folded into, which is brought into view instead).
            if let row, WorkDirIds.isFile(card), focus == card { selectRow(card, .symbol(row)) }
            focusRow = row
            focusRequest = focus
        case .openCard(let id):
            open(.card(id))
        case .openNode(let flow, let node):
            open(.card(flow))
            selectNode(flow, node)
        case .openSymbol(let file, let path):
            openSymbol(file, path)
        case .pickGone(let id):
            pickGone(id)
        case .toggle, .nothing:
            break
        }
    }

    /// A row of the nav panel dropped where it is taken (`NavPanelTree.drop`).
    func perform(_ drop: NavPanelTree.Drop) {
        switch drop {
        case .section(let id, let parent, let position):
            moveSection(id, into: parent, position: position)
        case .cardInto(let id, let section):
            moveSchema(id, toSection: section)
        case .cardAt(let id, let position):
            putCard(id, at: position)
        case .ghostOf(let id, let section):
            placeGhost(of: id, in: section)
        case .ghostInto(let id, let section):
            write { try tool("card_update", target: ghostPath(id), value: ["section": sectionPath(section)]) }
        case .ghostAt(let id, let position):
            write { try tool("card_update", target: ghostPath(id), value: ["position": position]) }
        }
    }

    /// A section moved into `parent` — its own to put it elsewhere among its siblings — at
    /// `position` among them (nil: the end), in one write (`section_update`'s `parent` and
    /// `position`): what it holds takes the new parent's kind, the paths into it follow.
    func moveSection(_ id: String, into parent: String, position: Int?) {
        guard let section = detail?.sections.first(where: { $0.id == id }) else { return }
        write {
            var value: [String: Any] = [:]
            if section.parentId != parent { value["parent"] = try sectionPath(parent) }
            if let position { value["position"] = position }
            guard !value.isEmpty else { return }
            try tool("section_update", target: sectionPath(id), value: value)
        }
    }

    /// A card put at `position` among its section's cards, counting from zero
    /// (`card_update`'s `position`): a locked main takes it too (`specboard://SpecBoard/Docs/Design:DesignBranches` §8).
    func putCard(_ id: String, at position: Int) {
        write { try tool("card_update", target: cardPath(id), value: ["position": position]) }
    }
}

extension NavPanelTree.Allowed {
    /// What the nav panel may change in what is open now: moves where the project may be
    /// written; a card's place among its section's also in a locked main.
    init(_ model: AppModel) {
        self.init(move: !model.isReadOnly, order: !model.isArchived && model.heldByMCP.isEmpty)
    }
}
