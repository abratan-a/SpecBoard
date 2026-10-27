import Foundation

/// One ui part's own page, as `section_read` `{as: "mock", faces: true}` answers it
/// (`specboard://SpecBoard/Docs/Design:CardKinds`「UI のセクションとはめ込み」): the part alone at the top left of a
/// transparent page its box's size, built exactly as the mock builds it — so the map and the mock never
/// draw a part two ways.
nonisolated struct PartFace: Equatable, Sendable {
    /// The card's full path, which is what it is matched to its card by.
    var path: String
    var kind: String
    /// The page's size in CSS pixels: the part's box on the grid, rounded up.
    var width: Int
    var height: Int
    /// The page, given as it is (`DocumentPage.Form.face`): what its picture is kept by.
    var page: DocumentPage

    init(path: String, kind: String, width: Int, height: Int, html: String) {
        self.path = path
        self.kind = kind
        self.width = width
        self.height = height
        page = DocumentPage(html, form: .face)
    }

    /// Read from one of the answer's `faces`; nil when it is not one.
    init?(_ said: [String: Any]) {
        guard let path = said["path"] as? String, let html = said["html"] as? String,
              let width = (said["width"] as? NSNumber)?.intValue, let height = (said["height"] as? NSNumber)?.intValue,
              width > 0, height > 0 else { return nil }
        self.init(path: path, kind: said["kind"] as? String ?? "", width: width, height: height, html: html)
    }

    /// The faces of a `section_read` answer (`json`), by path.
    static func faces(of said: [String: Any]) -> [String: PartFace] {
        let listed = (said["faces"] as? [[String: Any]] ?? []).compactMap(PartFace.init)
        return Dictionary(listed.map { ($0.path, $0) }, uniquingKeysWith: { first, _ in first })
    }

    static func == (a: PartFace, b: PartFace) -> Bool {
        a.path == b.path && a.width == b.width && a.height == b.height && a.page.digest == b.page.digest
    }
}

/// The faces of the ui sections' parts, kept for the window, not for one diagram: a section shown again, a
/// part picked, the diagram moved or zoomed draw what is kept and read nothing.
///
/// A section's faces are read at once, off the main thread, for the project as it was read (`stamp`):
/// once the project changes — a write, another hand's change, another tree open — they are read again, and
/// until they are in, what was kept before is drawn, so a part does not blink back to its outline.
@MainActor
final class PartFaces {

    /// What a part shows now: its face — `current`, or the one read before while a newer is read —, a
    /// face still being read (none yet), or none at all (no such part, or nothing can be read there).
    enum Answer: Equatable {
        case face(PartFace, current: Bool)
        case reading
        case none
    }

    /// One section's faces in one tree of one project.
    private struct Place: Hashable {
        var project: String
        /// The tree as MCP names it (`branch`); empty for main.
        var tree: String
        var section: String
    }

    private struct Kept {
        var stamp: Int
        var faces: [String: PartFace]
    }

    private var kept: [Place: Kept] = [:]
    /// The reading asked for each section, until it is in.
    private var reading: [Place: Int] = [:]
    private var waiting: [Place: [() -> Void]] = [:]
    private var settledWaiters: [CheckedContinuation<Void, Never>] = []
    /// Why a section's faces could not be read: the core's refusal.
    private(set) var problems: [String: String] = [:]
    /// How many sections' faces were asked of the core: for the tests, which count the readings.
    private(set) var reads = 0
    /// Read one after another, off the main thread.
    private let queue = DispatchQueue(label: "SpecBoard.partFaces", qos: .userInitiated)

    /// The most sections kept; past it, those of other projects and trees are let go first.
    static let mostKept = 64

    /// What `card`, a part of `detail`'s, shows, its section read in the tree MCP names `tree` (nil: it
    /// cannot be named, and nothing is read). When what is kept is not of this reading of the project, it
    /// is read again and `ready` is called once it is in; until then what was kept before is the answer.
    func face(of card: SchemaNode, in detail: ProjectDetail, tree: String?, core: CoreClient?,
              ready: (() -> Void)? = nil) -> Answer {
        guard let tree, let core else { return .none }
        let directory = detail.directory
        let place = Place(project: detail.project.id, tree: tree, section: directory.full(section: directory.sectionId(of: card)))
        let path = directory.full(card)
        let held = kept[place]
        if let held, held.stamp == detail.stamp {
            return held.faces[path].map { .face($0, current: true) } ?? .none
        }
        if let ready { waiting[place, default: []].append(ready) }
        if reading[place] != detail.stamp {
            reading[place] = detail.stamp
            read(place, stamp: detail.stamp, core: core)
        }
        if let before = held?.faces[path] { return .face(before, current: false) }
        return .reading
    }

    /// Whether a section's faces are being read now.
    var busy: Bool { !reading.isEmpty }

    /// Waits until no face is being read: for a test, which has no diagram to redraw.
    func settled() async {
        while busy { await withCheckedContinuation { settledWaiters.append($0) } }
    }

    private func read(_ place: Place, stamp: Int, core: CoreClient) {
        reads += 1
        var arguments: [String: Any] = ["project": place.project, "target": place.section,
                                        "value": ["as": "mock", "faces": true, "notes": false]]
        if !place.tree.isEmpty { arguments["branch"] = place.tree }
        let asking = Handed(value: (core, arguments))
        let back = Handed(value: { [weak self] (result: Result<[String: PartFace], Error>) in
            self?.take(result, place: place, stamp: stamp)
        })
        queue.async {
            let result = Result { try Self.read(asking.value.0, asking.value.1) }
            DispatchQueue.main.async { MainActor.assumeIsolated { back.value(result) } }
        }
    }

    /// A section's faces, read on the thread it is called on: the core's answer by path, or its refusal thrown.
    nonisolated static func read(_ core: CoreClient, _ arguments: [String: Any]) throws -> [String: PartFace] {
        let said = try core.request("tool", ["name": "section_read", "arguments": arguments])
        guard let json = (said as? [String: Any])?["json"] as? [String: Any] else {
            throw CoreClient.Refusal.unreadable(String(describing: said))
        }
        return PartFace.faces(of: json)
    }

    private func take(_ result: Result<[String: PartFace], Error>, place: Place, stamp: Int) {
        let latest = reading[place] == stamp
        if latest { reading[place] = nil }
        switch result {
        case .success(let faces):
            problems[place.section] = nil
            // One of an older reading, in after a newer was asked: kept only while there is nothing.
            if latest || kept[place] == nil { kept[place] = Kept(stamp: stamp, faces: faces) }
        case .failure(let error):
            problems[place.section] = ErrorText.said(error)
            // Nothing to show for this reading: what was kept before stays, as the reading's own.
            if latest { kept[place] = Kept(stamp: stamp, faces: kept[place]?.faces ?? [:]) }
        }
        if latest { for ready in waiting.removeValue(forKey: place) ?? [] { ready() } }
        trim(keeping: place)
        if !busy {
            let waiters = settledWaiters
            settledWaiters = []
            waiters.forEach { $0.resume() }
        }
    }

    private func trim(keeping place: Place) {
        guard kept.count > Self.mostKept else { return }
        kept = kept.filter { $0.key.project == place.project && $0.key.tree == place.tree }
        while kept.count > Self.mostKept, let any = kept.keys.first(where: { $0 != place }) { kept.removeValue(forKey: any) }
    }
}
