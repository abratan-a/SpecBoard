import CoreGraphics
import Foundation
import os

/// Where a working directory's file sits on its section's map in a tree, once it was moved there
/// (`project_detail`'s `filePlaces`), in grid units as a card's place is.
nonisolated struct FilePlace: Decodable, Hashable, Sendable {
    var x: Double
    var y: Double
    var pinned: Bool = false
    var size: GridSize?

    private enum Keys: String, CodingKey { case x, y, pinned, size }

    init(x: Double, y: Double, pinned: Bool = false, size: GridSize? = nil) {
        self.x = x
        self.y = y
        self.pinned = pinned
        self.size = size
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        x = try c.decode(Double.self, forKey: .x)
        y = try c.decode(Double.self, forKey: .y)
        pinned = try c.decodeIfPresent(Bool.self, forKey: .pinned) ?? false
        size = try c.decodeIfPresent(GridSize.self, forKey: .size)
    }
}

/// What the project's working directories hold as they are read on the machine its project is kept on
/// (`workdir_detail`, `specboard://SpecBoard/Docs/Design:CoreApi`): each working directory as set and read, the
/// folders in them as sections of the kind `directory`, the files as cards of the kind `file`. The same for
/// every tree: put into a tree where the tree has the working directory's section (`ProjectDetail.joining`).
/// Read once for each reading of the folders (`digest`), as the files may be many.
nonisolated struct WorkdirDetail: Sendable {
    var digest: String
    var dirs: [WorkDir]
    var sections: [ProjectSection]
    /// The files as the window shows cards, in the core's order.
    var files: [SchemaNode]
    /// Where each file, and each folder's box (`SectionGhosts.prefix` and its id), sits on its section's map
    /// before anyone moves it: by name, the folders first (`WorkDirGrid`).
    var grid: [String: GridPoint]

    init(digest: String, dirs: [WorkDir], sections: [ProjectSection], files: [SchemaNode]) {
        self.digest = digest
        self.dirs = dirs
        self.sections = sections
        self.files = files
        grid = WorkDirGrid.places(sections: sections, files: files)
    }

    /// The working directory whose section is `id`, or that a folder or file of id `id` is in.
    func dir(of id: String) -> WorkDir? {
        let mount = WorkDirIds.mount(of: id) ?? id
        return dirs.first { $0.section == mount }
    }
}

/// Where the files and folders of a working directory's section sit before anyone moves them: a grid in
/// the order of their names, the folders' boxes first, then the files drawn as compact boxes
/// (`CompactBox`), each apart from the next by a unit; then, below them, the files whose functions and
/// types are listed, a card's width and a card's tallest apart — their rows are read later, and a card
/// that grows then must not run into the next.
nonisolated enum WorkDirGrid {
    /// How many boxes a row of the grid holds.
    static let across = 5
    /// How many files of rows a row of the grid holds.
    static let codeAcross = 3
    /// How far apart files of rows are, in units: a card's width, and its height at the most rows a card
    /// shows before the rest scroll (`CardGeometry.standardRows`), each with a unit between.
    static let codeStepX = Grid.cardColumns + 1
    static let codeStepY = Grid.rows(forRows: 15) + 1

    static func places(sections: [ProjectSection], files: [SchemaNode]) -> [String: GridPoint] {
        var out: [String: GridPoint] = [:]
        let folders = Dictionary(grouping: sections.filter { $0.parentId != nil }, by: { $0.parentId ?? "" })
        let filed = Dictionary(grouping: files, by: { $0.sectionId ?? "" })
        let stepX = CompactBox.columns + 1, stepY = CompactBox.rows + 1
        for section in Set(folders.keys).union(filed.keys) {
            let boxes = (folders[section] ?? []).sorted { $0.name.localizedStandardCompare($1.name) == .orderedAscending }
            var row = 0
            for (i, folder) in boxes.enumerated() {
                out[SectionGhosts.prefix + folder.id] = GridPoint(x: (i % across) * stepX, y: (i / across) * stepY)
                row = i / across + 1
            }
            let sorted = (filed[section] ?? []).sorted { $0.name.localizedStandardCompare($1.name) == .orderedAscending }
            let cards = sorted.filter(CompactBox.isCompact)
            for (i, file) in cards.enumerated() {
                out[file.id] = GridPoint(x: (i % across) * stepX, y: (row + i / across) * stepY)
            }
            let top = (cards.isEmpty ? row : row + (cards.count - 1) / across + 1) * stepY
            for (i, file) in sorted.filter({ !CompactBox.isCompact($0) }).enumerated() {
                out[file.id] = GridPoint(x: (i % codeAcross) * codeStepX, y: top + (i / codeAcross) * codeStepY)
            }
        }
        return out
    }
}

/// How big a working directory's file is drawn on its section's map, and a folder's box: its mark and its name.
nonisolated enum CompactBox {
    static let columns = 6
    static let rows = 2

    /// Whether `schema` is drawn as one: a file — but one whose functions and types are listed, drawn as a
    /// card of rows —, or the box a folder is drawn as (`ownKind` `directory`).
    static func isCompact(_ schema: SchemaNode) -> Bool {
        (schema.ownKind == WorkDirIds.fileKind && !schema.isCodeFile) || schema.ownKind == WorkDirIds.directoryKind
    }
}

/// The working directories' readings held, the last of each project's, so that the next is asked from it
/// (`since`) and its files are not read again while the folders were not.
nonisolated final class HeldWorkdirs: @unchecked Sendable {
    private let held = OSAllocatedUnfairLock<[String: WorkdirDetail]>(uncheckedState: [:])

    func get(_ project: String) -> WorkdirDetail? {
        held.withLockUnchecked { $0[project] }
    }

    func put(_ project: String, _ reading: WorkdirDetail) {
        held.withLockUnchecked { $0[project] = reading }
    }

    func clear() {
        held.withLockUnchecked { $0.removeAll() }
    }
}

extension CoreClient {
    /// What the project's working directories hold, asked from the reading held of them: the one held
    /// when the folders were not read again since, else the new one, held. On any thread.
    nonisolated func workdirDetail(_ project: String) throws -> WorkdirDetail {
        let base = workdirs.get(project)
        var fields: [String: Any] = ["project": project]
        if let since = base?.digest { fields["since"] = since }
        let reply = try reply("workdir_detail", fields)
        let answer = try JSONDecoder().decode(WorkdirAnswer.self, from: Data(reply.utf8))
        if let error = answer.error { throw Refusal(error) }
        guard let result = answer.result else { throw Refusal.unreadable(reply) }
        if result.unchanged == true, let base, base.digest == result.digest { return base }
        let sections = (result.sections ?? []).map(\.section)
        let files = WholeCard.shown(result.schemas ?? [], sections: sections)
        let read = WorkdirDetail(digest: result.digest, dirs: result.dirs ?? [], sections: sections, files: files)
        workdirs.put(project, read)
        return read
    }

    /// `whole` as the window draws it, with the working directories' folders and files put in where the
    /// tree has their sections — not on the board, which has none. A core that cannot say them leaves the
    /// tree as it is.
    nonisolated func joined(_ whole: WholeDetail, _ project: String, tree: String) -> ProjectDetail {
        let detail = whole.detail
        guard tree != Board.id, detail.sections.contains(where: { $0.kindName == WorkDirIds.workdirKind }),
              let workdirs = try? workdirDetail(project) else { return detail }
        return detail.joining(workdirs, places: whole.filePlaces)
    }
}

/// What the core answers to `workdir_detail`.
nonisolated private struct WorkdirAnswer: Decodable {
    struct Result: Decodable {
        var digest: String
        var unchanged: Bool?
        var dirs: [WorkDir]?
        var sections: [WholeSection]?
        var schemas: [WholeCard]?
    }
    var result: Result?
    var error: String?
}

extension ProjectDetail {
    /// This tree with the working directories' folders and files put in (`WorkdirDetail`): those under a
    /// working directory's section the tree has, but not the ones it has already — a folder a note is in is
    /// the tree's own, and so is the stand-in for one that is gone (`ProjectSection.missing`). A file sits
    /// where it was moved to in this tree (`places`), else on the grid by name; a line to it is one of the
    /// tree's.
    nonisolated func joining(_ workdirs: WorkdirDetail, places: [String: FilePlace]) -> ProjectDetail {
        let mounts = Set(sections.filter { $0.kindName == WorkDirIds.workdirKind }.map(\.id))
        guard !mounts.isEmpty else { return self }
        func mounted(_ id: String) -> Bool { WorkDirIds.mount(of: id).map(mounts.contains) ?? false }
        let haveSections = Set(sections.map(\.id))
        let haveCards = Set(schemas.map(\.id))
        let fileKind = cardKinds[WorkDirIds.fileKind]
        var added: [SchemaNode] = []
        added.reserveCapacity(workdirs.files.count)
        for file in workdirs.files where mounted(file.id) && !haveCards.contains(file.id) {
            var card = file
            card.kindDef = fileKind
            card.definition = fileKind
            if let place = places[file.id] {
                card.posX = place.x
                card.posY = place.y
                card.pinned = place.pinned
            } else if let at = workdirs.grid[file.id] {
                card.posX = Double(at.x)
                card.posY = Double(at.y)
            }
            added.append(card)
        }
        var out = self
        let folders = workdirs.sections.filter { mounted($0.id) && !haveSections.contains($0.id) }
        if !folders.isEmpty { out.sections = sections + folders }
        if !added.isEmpty {
            out.schemas = schemas + added
            let files = Set(added.map(\.id))
            if relations.contains(where: { $0.outside && files.contains($0.toSchemaId) }) {
                out.relations = relations.map { line in
                    var line = line
                    if files.contains(line.toSchemaId) { line.outside = false }
                    return line
                }
            }
        }
        return out
    }
}
