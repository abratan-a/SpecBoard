import AppKit
import Foundation
import Synchronization

/// What a section holds (`specboard://SpecBoard/Docs/Design:Overview` §9): data, the behaviour over it, or
/// the documents about both — each drawn its own way. A section's children are of
/// its kind; the top level is free.
///
/// `export` is the one section main has for where things are written out
/// (`specboard://SpecBoard/Docs/Design:Overview` §10): its sections are the export directories, and it holds no
/// card. It is made with the project, never by hand.
enum SectionKind: String, Codable, CaseIterable, Sendable {
    case schema
    /// A definition promised to the outside (the shapes of API arguments, configuration files and
    /// messages): its shapes beside their data cards and tables, which is why it holds both. After
    /// `schema`, so that a shape's home stays there and a data card's is here.
    case definition
    case chart, document, export, rules, free
    /// A section of the project board (`specboard://SpecBoard/Docs/Design:DesignBoard` §2): where what is to be done
    /// is kept — goals and their tasks, freedocs and notes. Only there. The core calls it
    /// `goals` (its top holds one, Goals, and every other section inside it).
    case board = "goals"
    /// The two sections the core keeps in every board's Goals: where a finished goal (and a done task
    /// not inside a goal) goes, and where a cancelled one goes (`card_action`). They hold and look as
    /// Goals does; the core alone makes them, and refuses to delete them or move them out of their
    /// Goals, so the window offers neither. Known by kind, never by name: people rename them.
    case complete, cancelled
    /// A screen's design built of parts (`specboard://SpecBoard/Docs/Design:CardKinds`「UI のセクションとはめ込み」):
    /// html and svg cards where they sit, frames holding other ui sections, and notes beside them. Opened
    /// on its map, which draws the parts as the page the core builds of them (`CardCanvasView.drawsParts`).
    /// Only where a project's tree puts one; the sections inside it are ui too.
    case ui

    /// The built-in kinds a section directly under main's root may be of — what a new project has
    /// there (Docs・Schemas・Definitions・Charts・Free). Which are there, and in what order, is each
    /// project's (`project_tree`, `specboard://SpecBoard/Docs/Design:DesignSections` §1).
    static let creatable: [SectionKind] = [.document, .schema, .definition, .chart, .free]

    /// The heading a person sees.
    var label: String {
        switch self {
        case .schema: L10n[.Sections.kindSchema]
        case .definition: L10n[.Sections.kindDefinition]
        case .chart: L10n[.Sections.kindChart]
        case .document: L10n[.Sections.kindDocument]
        case .export: L10n[.Sections.kindExport]
        case .rules: L10n[.Sections.kindRules]
        case .free: L10n[.Sections.kindFree]
        case .board: L10n[.Sections.kindBoard]
        case .complete: L10n[.Sections.kindComplete]
        case .cancelled: L10n[.Sections.kindCancelled]
        case .ui: L10n[.Sections.kindUi]
        }
    }

    /// What it holds, in a line, for a tooltip.
    var holding: String {
        switch self {
        case .schema: L10n[.Sections.holdingSchema]
        case .definition: L10n[.Sections.holdingDefinition]
        case .chart: L10n[.Sections.holdingChart]
        case .document: L10n[.Sections.holdingDocument]
        case .export: L10n[.Sections.holdingExport]
        case .rules: L10n[.Sections.holdingRules]
        case .free: L10n[.Sections.holdingFree]
        case .board, .complete, .cancelled: L10n[.Sections.holdingBoard]
        case .ui: L10n[.Sections.holdingUi]
        }
    }

    /// The heading followed by the word for a section: what a section of this kind is called in a sentence.
    var named: String { L10n[.Sections.kindNamed(label)] }

    /// The tooltip of a section of this kind: what it is and what it holds.
    var help: String { "\(named): \(holding)" }

    /// The cards a section of this kind holds.
    var holds: Set<SchemaKind> {
        switch self {
        case .schema: [.entity, .value, .enum, .store, .note]
        case .definition: [.entity, .value, .enum, .store, .instance, .table, .note]
        case .chart: [.flow, .note]
        case .document: [.freedoc, .formatdoc, .stackdoc, .table, .note, .svg, .html]
        case .export, .rules: []
        // A frame goes only where a section names it: not in the free one, which holds every other kind.
        case .free: Set(SchemaKind.made).subtracting([.frame])
        case .board, .complete, .cancelled: [.freedoc, .note, .goal, .task]
        case .ui: [.html, .svg, .note, .frame]
        }
    }

    /// Whether the core keeps the section in its place: never deleted, never moved out of its
    /// parent.
    var isFixed: Bool { self == .complete || self == .cancelled }

    /// The kind a section made inside one of this kind is: a fixed one's are ordinary goals
    /// sections, as the core makes them.
    var madeInside: SectionKind { isFixed ? .board : self }

    /// The fixed sections only main has, at the top of its tree: not in a branch's
    /// view, a person's to change under the lock (`specboard://SpecBoard/Docs/Design:Overview` §10, §5「書く場所」).
    var isMainOnly: Bool { self == .export || self == .rules }

    /// Whether a box dropped here stays on the cell it was dropped on: the parts of a mock sit edge to
    /// edge and one over another, so no margin is kept and no overlap is jumped off. The core's rule
    /// for the same kind of section (`grid::keeps_places`).
    var keepsPlaces: Bool { self == .ui }

    /// The kind of section a card of this kind belongs in: a note, which may sit in
    /// any, in a document section when nothing else says where.
    static func home(of kind: SchemaKind) -> SectionKind {
        kind == .note ? .document : kind == .action ? .chart : allCases.first { $0.holds.contains(kind) } ?? .schema
    }
}

/// The project board (`specboard://SpecBoard/Docs/Design:DesignBoard`): a tree beside main and the branches that is
/// never merged, locked nor copied — where what is to be done is kept. Its rows are a
/// branch's rows under a fixed id, `branches` has no row for it, and it is spread out
/// without main beneath it.
nonisolated enum Board {
    /// Its rows' `branch_id`, and what a working copy is open in when it is open.
    static let id = "~board"
    /// What MCP's `branch` names it by (wrapped in `_`, as the app's own names are); no
    /// branch may be called this.
    static let word = "_project_board_"
    /// What the window calls it.
    static var title: String { L10n.text(.Common.wordProjectBoard, in: Language.current) }
    /// Its root's name, and the section made in it to start with.
    static let rootName = "Board"
    static let firstSection = "Goals"
}

/// A part of a project: a named set of its schemas.
///
/// Sections form a tree under the project's root. A branch is not a tree of its own: it
/// is the same tree as main, with what the branch changed (`specboard://SpecBoard/Docs/Design:DesignBranches`
/// §11). A schema's name is unique in its section, and a path names its section and
/// the schema (`specboard://SpecBoard/Docs/Design:DesignSections` §2), and no tree — which branch is read is
/// said apart from the path:
///
///     Core:User.id                the User in Core
///
/// What is stored and what MCP answers is always written in full; a path written
/// short is read from where it was written and stored in full. No card sits in the
/// root.
///
/// Not `Section`: that is SwiftUI's, and a type of this module by the same name
/// would quietly take its place in every view file.
struct ProjectSection: Codable, Hashable, Identifiable, Sendable {
    var id: String
    var projectId: String
    var name: String
    var description: String
    var sortOrder: Int
    /// The section it sits in, or nil at the top. A name is unique among its
    /// siblings, and a path to it names its parents: `Store/Ordering`.
    var parentId: String? = nil
    /// What it holds. Nil for a tree's root, and for a section from before kinds
    /// whose cards are of more than one — unsorted, until someone sorts them.
    var kind: SectionKind? = nil
    /// Its kind as the project names it — one of the project's own too (`kind` is nil for one of
    /// those). Out of the coding keys: it is read from the core alone.
    var kindName: String? = nil
    /// A folder of a working directory that a note is in but that reading the folder no longer finds: a
    /// stand-in the core keeps so the note has a place (`project_detail`'s `missing`). Read from the core
    /// alone.
    var missing = false

    enum CodingKeys: String, CodingKey {
        case id, projectId, name, description, sortOrder, parentId, kind
    }

    /// What a project's first section is called, and what every project written before
    /// sections existed was given.
    static let defaultName = "Main"
    /// The section a section taken out of the project's tree with something in it goes into, last under
    /// its root (`kinds::EVACUATION`): the app's, of the kind free.
    static let evacuation = "_evacuation_"
}

/// A card ghost (`specboard://SpecBoard/Docs/Design:DesignSections` §2「カードゴースト」): a
/// shortcut — macOS's alias — that a section holds among its cards. It has a name of its
/// own in its section, and points at another section's card, or at another card ghost, by
/// id (`target`). Through it the card is written by the ghost's name in its section, and
/// as `secA:User` from elsewhere; the card itself stays where it is, one of it.
///
/// The core keeps it as a card of the kind `ghost` and answers it apart from the cards
/// (`project_detail`'s `ghosts`), with the card its chain of ghosts reaches (`resolvesTo`) —
/// the core's answer, so the window never follows the chain itself.
nonisolated struct CardGhost: Codable, Hashable, Identifiable, Sendable {
    var id: String
    var projectId: String
    var name: String
    /// The section it sits in. Nil reads as the project's first, as a card's does.
    var sectionId: String? = nil
    /// What it points at, as it holds it: a card's id or another ghost's. Nil once what
    /// it pointed at is gone.
    var target: String? = nil
    /// The card its chain leads to; nil when it leads nowhere — what it pointed at was
    /// removed, or the ghosts it passes go round.
    var resolvesTo: String? = nil
    var sortOrder: Int = 0
    /// Where it sits on its section's diagram, in grid units, as a card does.
    var posX: Double? = nil
    var posY: Double? = nil
    var pinned: Bool = false
}

extension ProjectSection {
    /// Parents before children, siblings in the order given. A section whose parent
    /// is gone is treated as top-level rather than lost.
    static func treeOrder(_ flat: [ProjectSection]) -> [ProjectSection] {
        let known = Set(flat.map(\.id))
        var children: [String: [ProjectSection]] = [:]
        for section in flat {
            let parent = section.parentId.flatMap { known.contains($0) ? $0 : nil } ?? ""
            children[parent, default: []].append(section)
        }
        var out: [ProjectSection] = []
        var seen = Set<String>()
        func visit(_ parent: String) {
            for section in children[parent] ?? [] where seen.insert(section.id).inserted {
                out.append(section)
                visit(section.id)
            }
        }
        visit("")
        // A loop has no way in from the top; keep its members all the same.
        for section in flat where !seen.contains(section.id) { out.append(section) }
        return out
    }
}

/// `Core:User.id` → (`Core`, `User.id`); `User.id` → (nil, `User.id`): cut at the first `:`
/// outside a quoted name (`PathText`), the section's trimmed.
func splitSection(_ raw: String) -> (section: String?, rest: String) {
    let (section, rest) = PathText.splitSection(raw)
    return (section?.trimmingCharacters(in: .whitespaces), rest)
}

/// `main@Data/Core` → (`main`, `Data/Core`); `Data/Core` → (nil, `Data/Core`): the
/// tree a path from before names, or another word put there, to read it without or
/// refuse it. Only an `@` before every other separator and outside a quoted name is one
/// (`PathText.treePrefix`).
func splitTree(_ raw: String) -> (tree: String?, rest: String) {
    guard let (tree, rest) = PathText.treePrefix(raw) else { return (nil, raw) }
    return (tree.trimmingCharacters(in: .whitespaces), rest.trimmingCharacters(in: .whitespaces))
}

/// Who is called what, section by section, tree by tree.
///
/// The one table, keyed by section, and the one place that decides what a name
/// means. A `[name: schema]` built by each resolver would only be a table while
/// names are unique in the project.
///
/// A path names no tree (`Core:User`): what is read is one view — main, or what a
/// branch sees — so a path always lands in it. `main@` written before the tree went
/// is read without it (`main@` alone is the root); the root is named by its own name.
///
/// Names are written three ways, each for its reader: in full for what is stored
/// and what MCP answers (`full`), short for the screen (`shown`), and for what is
/// exported (`exported`, bare in a project of one section) — `specboard://SpecBoard/Docs/Design:DesignSections`
/// §2・§7.
struct SchemaDirectory {
    let sections: [ProjectSection]
    /// Main's root: the project's first section, or `""` for a project built without any
    /// (tests, old fixtures), where every schema is in it.
    let home: String
    /// The root of the tree names are written from. Main's unless given.
    let context: String
    private var byKey: [String: SchemaNode] = [:]
    private var bySection: [String: [SchemaNode]] = [:]
    /// Path from the top by the roots' own names (`Main/Store/Ordering`), as a path writes it
    /// and in any ASCII case (`SchemaDirectory.key`) → section: how paths were written before
    /// trees were named with `@`.
    private var sectionsByPath: [String: ProjectSection] = [:]
    private var sectionsById: [String: ProjectSection] = [:]
    /// The same paths, as written.
    private var fullPaths: [String: String] = [:]
    private var roots: [String: String] = [:]
    private var schemasById: [String: SchemaNode] = [:]
    /// The card ghosts: by their name in their section, by id, by section — names as a
    /// card's are, a card first where both have one (`specboard://SpecBoard/Docs/Design:DesignSections` §2).
    private var ghostsByKey: [String: CardGhost] = [:]
    private var ghostsById: [String: CardGhost] = [:]
    private var ghostsBySection: [String: [CardGhost]] = [:]
    /// Every card ghost, in the order the project lists them.
    let ghosts: [CardGhost]
    /// Whether a section may be named by its own name alone when only one has it
    /// (`Rules` for `Tenant/Rules`). Off: with the same name in two places it rarely
    /// resolves, and where it does the reader has to guess which. Only
    /// `Repository.qualifyLeafPaths` reads paths written that old way, once, to
    /// write them out in full.
    var leavesAllowed = false
    /// Whether a tree may be named the old way, by its root's name and `/`
    /// (`Main/Core`, `refunds/Store`). Off: that is refused with the way it is written
    /// now. Only the one move to full paths reads it, once.
    var oldPathsAllowed = false

    /// What main is called in a path, whatever its root is named.
    static let mainWord = "main"

    init(_ detail: ProjectDetail) {
        self.init(detail.schemas, sections: detail.sections, context: detail.context, ghosts: detail.ghosts)
    }

    init(_ schemas: [SchemaNode], sections: [ProjectSection], context: String? = nil, ghosts: [CardGhost] = []) {
        self.sections = sections
        self.ghosts = ghosts
        home = sections.first?.id ?? ""
        for section in sections { sectionsById[section.id] = section }
        for section in sections {
            var names = [section.name]
            var root = section.id
            var parent = section.parentId
            var guardSet: Set<String> = [section.id]
            while let id = parent, let above = sectionsById[id], guardSet.insert(id).inserted {
                names.insert(above.name, at: 0)
                root = id
                parent = above.parentId
            }
            let path = PathText.joinSections(names)
            fullPaths[section.id] = path
            roots[section.id] = root
            sectionsByPath[PathText.fold(path)] = section
        }
        let known = roots
        self.context = context.flatMap { known[$0] } ?? home
        for schema in schemas {
            let section = schema.sectionId ?? home
            schemasById[schema.id] = schema
            // A card inside a container is named in it, and is not one of its section's own
            // (`specboard://SpecBoard/Docs/Design:CardKinds`「入れ物のカード」).
            if let container = schema.container, !container.isEmpty {
                byKey[Self.key(container, schema.name)] = schema
                continue
            }
            byKey[Self.key(section, schema.name)] = schema
            bySection[section, default: []].append(schema)
        }
        for ghost in ghosts {
            let section = ghost.sectionId ?? home
            ghostsById[ghost.id] = ghost
            ghostsByKey[Self.key(section, ghost.name)] = ghost
            ghostsBySection[section, default: []].append(ghost)
        }
    }

    private static func key(_ section: String, _ name: String) -> String { section + "\u{1}" + name }

    /// A section's path as written, as it is looked up: each name quoted only where it needs it,
    /// in any ASCII case.
    private static func key(_ written: String) -> String {
        PathText.fold(PathText.joinSections(PathText.slashes(written).map(PathText.unquote)))
    }

    // MARK: - The tree

    /// The root of the tree a section is in: main's root. A branch is not a tree of
    /// its own: it is seen as main's, spread out with what it changed
    /// (`specboard://SpecBoard/Docs/Design:DesignBranches` §11).
    func root(of sectionId: String) -> String { roots[sectionId] ?? home }

    /// The word a path once named the tree with: `main`. Read and taken off; a text
    /// still holding it is shown without it.
    func treeWord(_ root: String) -> String { Self.mainWord }

    /// The root the word of a path from before names: `main` (in any case); no other
    /// word names anything.
    func treeRoot(named word: String) -> String? {
        PathText.same(PathText.unquote(word), Self.mainWord) ? home : nil
    }

    /// Whether `key` (`SchemaDirectory.key`) is the root's own name: how a path names the root.
    private func isRoot(_ key: String, of root: String) -> Bool {
        guard let section = sectionsById[root] else { return false }
        return PathText.fold(PathText.quote(section.name)) == key
    }

    private func inTree(_ root: String) -> [ProjectSection] {
        sections.filter { self.root(of: $0.id) == root }
    }

    /// Whether names need their section to be told apart: the tree has more than one
    /// section.
    var isSectioned: Bool { inTree(context).count > 1 }

    func schema(id: String) -> SchemaNode? { schemasById[id] }

    func sectionId(of schema: SchemaNode) -> String { schema.sectionId ?? home }

    /// The kind of the section a card is in; nil for one of no kind.
    func sectionKind(of schema: SchemaNode) -> SectionKind? { sectionsById[sectionId(of: schema)]?.kind }

    // MARK: - Naming sections

    /// A section named to act on it: under the root, or the root by its own name.
    /// `main@` from before is read without it; no other word before `@` names anything.
    func section(named name: String) -> ProjectSection? {
        let raw = name.trimmingCharacters(in: .whitespaces)
        let (tree, rest) = splitTree(raw)
        if let tree {
            guard let root = treeRoot(named: tree) else { return nil }
            return rest.isEmpty ? sectionsById[root] : relative(Self.key(rest), in: root)
        }
        let key = Self.key(raw)
        if isRoot(key, of: context) { return sectionsById[context] }
        if let found = relative(key, in: context) { return found }
        if oldPathsAllowed, let found = sectionsByPath[key] { return found }
        return leavesAllowed ? leaf(raw, among: sections) : nil
    }

    /// A section named in a path written in `reader`: in the reader's tree, the root by
    /// its own name; `main@` from before read without it.
    func section(named name: String, from reader: String) -> ProjectSection? {
        let raw = name.trimmingCharacters(in: .whitespaces)
        let (treeName, rest) = splitTree(raw)
        if let treeName {
            guard let root = treeRoot(named: treeName) else { return nil }
            return rest.isEmpty ? sectionsById[root] : relative(Self.key(rest), in: root)
        }
        let key = Self.key(raw)
        let tree = root(of: reader)
        if isRoot(key, of: tree) { return sectionsById[tree] }
        if let found = relative(key, in: tree) { return found }
        if oldPathsAllowed, let found = sectionsByPath[key] { return found }
        return leavesAllowed ? leaf(raw, among: inTree(tree)) : nil
    }

    private func relative(_ key: String, in root: String) -> ProjectSection? {
        guard let rootPath = fullPaths[root] else { return nil }
        return sectionsByPath[PathText.fold(rootPath) + "/" + key]
    }

    /// The old way of naming a section from the root's name (`Main/Core`) as it is
    /// written now (`Core`), or nil when `name` is not written that way.
    func nowWritten(_ name: String) -> String? {
        let raw = name.trimmingCharacters(in: .whitespaces)
        let parts = PathText.slashes(raw)
        guard PathText.treePrefix(raw) == nil, parts.count > 1 else { return nil }
        let head = PathText.unquote(parts[0])
        guard sections.contains(where: { $0.parentId == nil && PathText.same($0.name, head) }) else { return nil }
        return String(decoding: raw.utf8.dropFirst(parts[0].utf8.count + 1), as: UTF8.self)
    }

    /// The one section of the candidates named `written` alone (a name, not a path).
    private func leaf(_ written: String, among candidates: [ProjectSection]) -> ProjectSection? {
        guard PathText.slashes(written).count == 1 else { return nil }
        let name = PathText.unquote(written)
        let leaves = candidates.filter { PathText.same($0.name, name) }
        return leaves.count == 1 ? leaves[0] : nil
    }

    /// Why a section name found nothing, in words to act on — from `reader` when it
    /// was a path in a card, where the trees it cannot see are the likely reason.
    func sectionProblem(_ name: String, from reader: String? = nil) -> String {
        if let now = nowWritten(name) {
            return pathText(.Sections.problemOldPath(name, now))
        }
        let raw = name.trimmingCharacters(in: .whitespaces)
        let (treeName, rest) = splitTree(raw)
        if let treeName {
            guard let root = treeRoot(named: treeName) else {
                return pathText(.Sections.problemTreeInPath("\(treeName)@", Self.branchesAreNamed))
            }
            return pathText(.Sections.problemNoSection(rest, sectionList(in: root)))
        }
        let single = PathText.slashes(raw).count == 1
        let leaf = PathText.unquote(raw)
        let tree = reader.map(root(of:)) ?? context
        let leaves = inTree(tree).filter { single && $0.id != tree && PathText.same($0.name, leaf) }
        if leaves.count > 1 {
            return pathText(.Sections.problemSectionAmbiguous(
                name, leaves.count, leaves.map { full(section: $0.id) }.joined(separator: " / ")))
        }
        if let only = leaves.first {
            return pathText(.Sections.problemSectionFromTop(full(section: only.id)))
        }
        return pathText(.Sections.problemNoSection(name, sectionList(in: tree)))
    }

    /// A tree's sections in full, for a message listing what there is.
    private func sectionList(in root: String) -> String {
        let listed = inTree(root).filter { $0.id != root }.map { full(section: $0.id) }
        return listed.isEmpty ? pathText(.Sections.problemNoSectionsAtAll) : listed.joined(separator: " / ")
    }

    func section(id: String) -> ProjectSection? { sectionsById[id] }

    // MARK: - Writing names

    /// A section's path under its tree's root: `Store/Ordering`. The root itself is `""`.
    func localPath(of sectionId: String) -> String {
        let root = self.root(of: sectionId)
        guard sectionId != root, let full = fullPaths[sectionId], let rootPath = fullPaths[root],
              full.utf8.count > rootPath.utf8.count else { return "" }
        return String(decoding: full.utf8.dropFirst(rootPath.utf8.count + 1), as: UTF8.self)
    }

    /// In full, as stored and as MCP answers: `Store/Ordering`; the root by its own
    /// name (`Main`).
    func full(section sectionId: String) -> String { exported(section: sectionId) }

    /// Where a branch is read and written instead of a tree in the path: said where a
    /// path names one.
    static var branchesAreNamed: String { pathText(.Sections.problemBranchesAreNamed) }

    /// For the screen: under the root inside `root`'s tree (`Store/Ordering`), a root by
    /// its own name, anything else in full.
    func shown(section sectionId: String, within root: String) -> String {
        if sectionsById[sectionId]?.parentId == nil {
            return PathText.quote(sectionsById[sectionId]?.name ?? ProjectSection.defaultName)
        }
        guard self.root(of: sectionId) == root else { return full(section: sectionId) }
        return localPath(of: sectionId)
    }

    /// For the screen, from the context's tree.
    func shown(section sectionId: String) -> String { shown(section: sectionId, within: context) }

    /// For the screen, where sections of every tree are listed together: a root by
    /// its own name, any other section in full.
    func shownFromTop(section sectionId: String) -> String {
        sectionsById[sectionId]?.parentId == nil
            ? PathText.quote(sectionsById[sectionId]?.name ?? ProjectSection.defaultName)
            : full(section: sectionId)
    }

    /// For what is exported — main alone — without the tree: `Store/Ordering`; the
    /// root by its own name.
    func exported(section sectionId: String) -> String {
        sectionsById[sectionId]?.parentId == nil
            ? PathText.quote(sectionsById[sectionId]?.name ?? ProjectSection.defaultName)
            : localPath(of: sectionId)
    }

    /// The sections directly inside one — or at the top, for nil. The root's are in the project's
    /// order (`project_tree`, which the core gives as their `sortOrder`), `_evacuation_` last.
    func children(of sectionId: String?) -> [ProjectSection] {
        let inside = sections.filter { section in
            section.parentId.flatMap { sectionsById[$0] == nil ? nil : $0 } == sectionId
        }
        guard let sectionId, sectionsById[sectionId]?.parentId == nil else { return inside }
        return inside.enumerated().sorted { a, b in
            let (left, right) = (a.element.sortOrder, b.element.sortOrder)
            return left != right ? left < right : a.offset < b.offset
        }.map(\.element)
    }

    /// Every section, parents before children, in the order the nav panel lists them:
    /// the root's own sections by their kind.
    func listed() -> [ProjectSection] {
        var out: [ProjectSection] = []
        var seen = Set<String>()
        func visit(_ id: String?) {
            for section in children(of: id) where seen.insert(section.id).inserted {
                out.append(section)
                visit(section.id)
            }
        }
        visit(nil)
        return out
    }

    /// How deep a section sits: zero at the top.
    func depth(of sectionId: String) -> Int {
        guard let path = fullPaths[sectionId], !path.isEmpty else { return -1 }
        return PathText.slashes(path).count - 1
    }

    /// The schemas of one section, in project order.
    func members(_ sectionId: String) -> [SchemaNode] { bySection[sectionId] ?? [] }

    /// Everything in the same section as this one — what a discriminated pointer
    /// picks its candidates from.
    func neighbours(of schema: SchemaNode) -> [SchemaNode] { members(sectionId(of: schema)) }

    // MARK: - Card ghosts (`specboard://SpecBoard/Docs/Design:DesignSections` §2)

    /// What a name in a section is: one of its cards, or one of its card ghosts.
    enum Entry: Equatable {
        case card(SchemaNode)
        case ghost(CardGhost)
    }

    func ghost(id: String) -> CardGhost? { ghostsById[id] }

    /// The card ghosts of one section, in the order the project lists them.
    func ghosts(in sectionId: String) -> [CardGhost] { ghostsBySection[sectionId] ?? [] }

    func sectionId(of ghost: CardGhost) -> String { ghost.sectionId ?? home }

    /// The card a ghost leads to, at the end of its chain — the core's answer
    /// (`resolvesTo`), never followed here. Nil when it leads nowhere.
    func reached(_ ghost: CardGhost) -> SchemaNode? { ghost.resolvesTo.flatMap { schemasById[$0] } }

    /// A card by its id, or the card a ghost of that id leads to: what a `ref` or a
    /// pointer written through a ghost lands on (it holds the ghost's id).
    func card(through id: String) -> SchemaNode? { schemasById[id] ?? ghostsById[id].flatMap(reached) }

    /// A ghost in full, as stored and as MCP answers: `Data:Admin`.
    func full(_ ghost: CardGhost) -> String { PathText.cardPath(full(section: sectionId(of: ghost)), ghost.name) }

    /// A working directory's file a line holds by its id (`file:<id>/<path>`) with no card of it here — its
    /// working directory not read on this machine, or the file not there (yet) — in full as MCP writes it
    /// (`repo/src:'a.rs'`): the working directory's section and the folders read below it, then the rest of
    /// the path as the id names it. Nil when this tree has not that working directory's section.
    func full(fileId id: String) -> String? {
        guard WorkDirIds.isFile(id), let mount = WorkDirIds.mount(of: id), sectionsById[mount] != nil else { return nil }
        let path = id.dropFirst("file:".count + mount.count - "dir:".count + 1)
        var parts = path.split(separator: "/", omittingEmptySubsequences: false).map(String.init)
        guard let name = parts.popLast(), !name.isEmpty, !parts.contains(where: \.isEmpty) else { return nil }
        // A folder's section is its parent's id and its own name: as far down as they were read.
        var section = mount
        var rest = parts[...]
        while let next = rest.first, sectionsById["\(section)/\(next)"] != nil {
            section += "/\(next)"
            rest = rest.dropFirst()
        }
        let folder = rest.reduce(full(section: section)) { PathText.child($0, $1) }
        return PathText.cardPath(folder, name)
    }

    /// For the screen, as a card in `base` would read it: bare in its own section.
    func shown(_ ghost: CardGhost, from base: String) -> String {
        let section = sectionId(of: ghost)
        return section == base ? PathText.quote(ghost.name)
            : PathText.cardPath(shown(section: section, within: root(of: base)), ghost.name)
    }

    /// What a ghost points at, in full — a card's path, or another ghost's — or nil
    /// once it is gone.
    func targetPath(of ghost: CardGhost) -> String? {
        guard let target = ghost.target else { return nil }
        if let card = schemasById[target] { return full(card) }
        return ghostsById[target].map(full)
    }

    /// The shortcut's mark, for the screen.
    static let ghostMark = "↪"

    /// What a ghost points at, as its section reads it — a card or another ghost — or
    /// that it points at nothing now.
    func shownTarget(of ghost: CardGhost) -> String {
        let base = sectionId(of: ghost)
        guard let target = ghost.target else { return L10n[.Sections.ghostNoTarget] }
        if let card = schemasById[target] { return shown(card, from: base) }
        return ghostsById[target].map { shown($0, from: base) } ?? L10n[.Sections.ghostNoTarget]
    }

    /// A ghost's header on its diagram: the mark, its name and what it points at
    /// (`↪ Admin → Data/Tenant:User`) — only the mark and what it points at where its name
    /// is the card's (`↪ Data/Tenant:User`).
    func ghostTitle(_ ghost: CardGhost) -> String {
        let target = shownTarget(of: ghost)
        if let card = reached(ghost), card.name == ghost.name { return "\(Self.ghostMark) \(target)" }
        return "\(Self.ghostMark) \(ghost.name) → \(target)"
    }

    /// What something of this name in a section is: a card, else a card ghost. A card inside
    /// a container is named on from the container (`Launch.Design`), the container's own
    /// name first. `written`: the names as a path writes them (`'release notes.md'`).
    func entry(in sectionId: String, named written: String) -> Entry? {
        let names = PathText.names(written)
        guard let first = names.first else { return nil }
        guard names.count > 1 else { return entry(in: sectionId, name: first) }
        guard case .card(let head)? = entry(in: sectionId, name: first) else { return nil }
        return inside(head.id, names: Array(names.dropFirst()))
    }

    /// What a name (as it is, not written in a path) is in a section: a card, else a card ghost.
    func entry(in sectionId: String, name: String) -> Entry? {
        let key = Self.key(sectionId, name)
        if let card = byKey[key] { return .card(card) }
        return ghostsByKey[key].map { .ghost($0) }
    }

    /// The card inside a container of that name (as it is), and on into one inside it by the
    /// names that follow (`Spec`, `Review`).
    func inside(_ container: String, names: [String]) -> Entry? {
        guard let head = names.first, let card = byKey[Self.key(container, head)] else { return nil }
        return names.count == 1 ? .card(card) : inside(card.id, names: Array(names.dropFirst()))
    }

    /// The card inside a container of that name, as it is.
    func inside(_ container: String, named name: String) -> Entry? {
        inside(container, names: [name])
    }

    /// The container a card is inside, when it is there.
    func container(of schema: SchemaNode) -> SchemaNode? {
        schema.container.flatMap { schemasById[$0] }
    }

    /// The names of a card inside containers, from the outermost container's (`Launch.Design`),
    /// as a path writes them: its own name for a card in a section.
    func names(of schema: SchemaNode) -> String {
        var names = [schema.name]
        var at = schema
        var seen: Set<String> = [schema.id]
        while let up = container(of: at), seen.insert(up.id).inserted {
            names.insert(up.name, at: 0)
            at = up
        }
        return PathText.joinNames(names)
    }

    /// Every name a section has, its cards' and then its card ghosts': what a name can
    /// be told it is close to.
    func names(in sectionId: String) -> [String] {
        members(sectionId).map(\.name) + ghosts(in: sectionId).map(\.name)
    }

    /// Whether a ghost's chain comes back to a ghost it passed — said apart from one whose
    /// card is gone, though either leads nowhere.
    func goesRound(_ ghost: CardGhost) -> Bool {
        var seen: Set<String> = [ghost.id]
        var at = ghost
        while let target = at.target, let next = ghostsById[target] {
            guard seen.insert(next.id).inserted else { return true }
            at = next
        }
        return false
    }

    /// Why a name that is a card ghost leads to no card, as the core says it.
    func deadEnd(_ ghost: CardGhost, _ raw: String) -> String {
        goesRound(ghost)
            ? L10n[.Sections.ghostDeadEndRound(raw)]
            : L10n[.Sections.ghostDeadEndGone(raw)]
    }

    /// Why a ghost leads to no card, as a broken path says it (`brokenPaths`); nil when it
    /// leads to one.
    func ghostProblem(_ ghost: CardGhost) -> String? {
        guard reached(ghost) == nil else { return nil }
        if goesRound(ghost) { return L10n[.Sections.ghostProblemRound] }
        guard let target = targetPath(of: ghost) else {
            return L10n[.Sections.ghostProblemGone]
        }
        return L10n[.Sections.ghostProblemNotCard(target)]
    }

    /// In full, as stored and as MCP answers: `Core:User`. A card still in the root — a
    /// project from before cards were kept out of it — is named from the root (`Main:User`).
    func full(_ schema: SchemaNode) -> String {
        // A project read with no sections at all — the web build's form, only ever in
        // memory — names its cards as it always did.
        guard !sections.isEmpty else { return names(of: schema) }
        return full(section: sectionId(of: schema)) + ":" + names(of: schema)
    }

    /// In full with the names inside the card (as they are): `Core:User.profile.email`.
    func full(_ schema: SchemaNode, _ names: [String]) -> String {
        names.reduce(full(schema), PathText.member)
    }

    /// In full with a row of the card as it draws it, written as a path (`auth.'log in'`).
    func full(_ schema: SchemaNode, row: String) -> String {
        row.isEmpty ? full(schema) : "\(full(schema)).\(row)"
    }

    /// For the screen, from the context's tree: `Core:User` where names need their
    /// section, `User` where they do not; a card of another tree in full.
    func shown(_ schema: SchemaNode) -> String {
        let section = sectionId(of: schema)
        guard isSectioned || root(of: section) != context else { return names(of: schema) }
        return shown(section: section) + ":" + names(of: schema)
    }

    /// For the screen, as a card in `base` would read it: bare in its own section, with
    /// the section under the root in the same tree, in full from another tree.
    func shown(_ target: SchemaNode, from base: String) -> String {
        let section = sectionId(of: target)
        return section == base ? names(of: target)
            : shown(section: section, within: root(of: base)) + ":" + names(of: target)
    }

    /// For what is exported — main alone — without the tree: `Core:User` in a project
    /// with sections, `User` without.
    func exported(_ schema: SchemaNode) -> String {
        guard isSectioned else { return names(of: schema) }
        return exported(section: sectionId(of: schema)) + ":" + names(of: schema)
    }

    /// For what is exported, as a card in `base` names `target`: bare in its own
    /// section, without the tree anywhere else (`Core:Address`).
    func exported(_ target: SchemaNode, from base: String) -> String {
        sectionId(of: target) == base ? names(of: target) : exported(section: sectionId(of: target)) + ":" + names(of: target)
    }

    /// Where a line goes, as a path to it is written (`NodeWrite.End`): the card, card ghost
    /// or card inside a container it names — as written — and the names after it there.
    /// A path naming nothing is the card as written, for the core to refuse.
    func lineEnd(_ path: String, from base: String?) -> (to: String, at: String?) {
        guard let written = WrittenPath(path) else { return (path, nil) }
        var to = written.card
        var names = written.names
        guard case .entry(let entry) = findEntry(written.card, from: base, seeing: true) else { return (path, nil) }
        var at: SchemaNode?
        switch entry {
        case .card(let card): at = card
        case .ghost(let ghost): at = reached(ghost)
        }
        // On into a container, as long as the names are of cards inside it.
        while let holder = at, let first = names.first,
              case .card(let inner)? = inside(holder.id, named: PathText.plain(first)) {
            to += "." + PathText.quote(PathText.plain(first))
            names.removeFirst()
            at = inner
        }
        // A flow's node is a card inside it, by its name.
        if let holder = at, holder.kind == .flow, let first = names.first,
           holder.nodes.contains(where: { $0.name == PathText.plain(first) }) {
            to += "." + PathText.quote(PathText.plain(first))
            names.removeFirst()
        }
        return (to, names.isEmpty ? nil : names.joined(separator: "."))
    }

    /// Paths in `text` as a line shows them: without the tree `tree` a path from before
    /// still names (`main@Data:Order` → `Data:Order`), others whole.
    static func shown(text: String, in tree: String?) -> String {
        guard let tree, !tree.isEmpty, text.contains("@") else { return text }
        // Not inside a quoted name (`'main@x'`), where an at is the name's own.
        let pattern = "(?<![\\p{L}\\p{N}_\\-'])" + NSRegularExpression.escapedPattern(for: tree) + "@"
        return text.replacingOccurrences(of: pattern, with: "", options: [.regularExpression, .caseInsensitive])
    }

    /// The tree a card is in, as a path writes it.
    func treeWord(of schema: SchemaNode) -> String { treeWord(root(of: sectionId(of: schema))) }

    /// A path as it is exported: without a tree from before (`main@Core:User.id` →
    /// `Core:User.id`). Anything else as it is.
    nonisolated static func exported(path stored: String) -> String {
        PathText.withoutTree(stored)
    }

    /// For what is written out (`specboard://SpecBoard/Docs/Design:DesignSections` §7): a path without the
    /// app's sections — the card's name and the fields below it, `Core:User.id` →
    /// `User.id`; a section by its own name, `Store/Ordering:` → `Ordering`.
    static func plain(path stored: String) -> String {
        if let section = PathText.sectionOnly(exported(path: stored)) {
            return PathText.slashes(section).last?.trimmingCharacters(in: .whitespaces) ?? section
        }
        let parts = plainParts(stored)
        return parts.name + (parts.field.map { ".\($0)" } ?? "")
    }

    /// A stored path cut where it is written out: the card as the app names it (to
    /// find it), the card's name, and the fields below it — each as written (quoted where it is).
    static func plainParts(_ stored: String) -> (card: String, name: String, field: String?) {
        let bare = exported(path: stored)
        let (section, rest) = PathText.splitSection(bare)
        let head = section.map { "\($0):" } ?? ""
        let name = PathText.dots(rest)[0]
        guard name.utf8.count < rest.utf8.count else { return (head + rest, rest, nil) }
        let field = String(decoding: rest.utf8.dropFirst(name.utf8.count + 1), as: UTF8.self)
        return (head + name, name, field.isEmpty ? nil : field)
    }

    enum Found {
        case schema(SchemaNode)
        /// Why not, in words written to be acted on.
        case failure(String)

        var schema: SchemaNode? {
            if case .schema(let found) = self { return found }
            return nil
        }
    }

    /// The schema a name refers to, read from `base` — through a card ghost, the card it
    /// leads to.
    ///
    /// `base` nil means the reader has no section of its own — an MCP call that did
    /// not say one. Where the context's tree has one section that is no problem;
    /// with several an unqualified name is refused rather than guessed, and the
    /// answer lists where that name does exist.
    ///
    /// `seeing`: the name is a path written in a card, held to the trees its reader
    /// sees. A name given to a tool is not.
    func find(_ raw: String, from base: String?, seeing: Bool = false) -> Found {
        switch findEntry(raw, from: base, seeing: seeing) {
        case .entry(.card(let card)):
            return .schema(card)
        case .entry(.ghost(let ghost)):
            guard let card = reached(ghost) else {
                return .failure(deadEnd(ghost, raw.trimmingCharacters(in: .whitespaces)))
            }
            return .schema(card)
        case .failure(let why):
            return .failure(why)
        }
    }

    /// What a name refers to, a card ghost as itself — `find` before a ghost is followed.
    enum FoundEntry {
        case entry(Entry)
        case failure(String)

        /// The ghost a name is written through, when it is one.
        var ghost: CardGhost? {
            if case .entry(.ghost(let ghost)) = self { return ghost }
            return nil
        }
    }

    /// What a name names, read from `base` as `find` reads it: a card, or a card ghost as
    /// itself. A name the section has as both is the card's. Where nothing has it, the
    /// sections that do are said — and, for a path written in a card (`seeing`), that a
    /// card ghost there would let it be written by its name alone (as the
    /// core's `find_problem` says it).
    func findEntry(_ raw: String, from base: String?, seeing: Bool = false) -> FoundEntry {
        let (sectionName, name) = splitSection(raw.trimmingCharacters(in: .whitespaces))
        if let sectionName {
            let reader = base ?? context
            guard let section = seeing ? self.section(named: sectionName, from: reader)
                                       : (base.map { self.section(named: sectionName, from: $0) } ?? nil)
                                         ?? self.section(named: sectionName) else {
                return .failure(sectionProblem(sectionName, from: seeing ? reader : nil))
            }
            if let hit = entry(in: section.id, named: name) { return .entry(hit) }
            return .failure(missingSchema(name, among: names(in: section.id).map(PathText.quote),
                                          in: full(section: section.id))
                            + elsewhere(name, except: section.id))
        }
        if PathText.treePrefix(name) != nil {
            return .failure(pathText(.Sections.problemNoSectionBefore(name, exampleSection, splitTree(name).rest)))
        }
        let single = inTree(context).count == 1 ? context : nil
        guard let base = base ?? (sections.isEmpty ? home : single) else {
            let found = sections.compactMap { section in
                entry(in: section.id, named: name).map { _ in "\(full(section: section.id)):\(name)" }
            }
            return .failure(pathText(.Sections.problemWriteWithSection(
                exampleSection, name,
                found.isEmpty
                    ? missingSchema(name, among: allQualified())
                    : pathText(.Sections.problemFoundIn(name, found.joined(separator: " / "))))))
        }
        if let hit = entry(in: base, named: name) { return .entry(hit) }
        let others = elsewhere(name, except: base)
        var hint = ""
        if seeing, !others.isEmpty, let first = sections.first(where: { $0.id != base && entry(in: $0.id, named: name) != nil }) {
            hint = pathText(.Sections.problemElsewhereHint(full(section: first.id), name, full(section: base)))
        }
        return .failure(missingSchema(name, among: names(in: base).map(PathText.quote), in: full(section: base))
                        + others + hint)
    }

    /// The card ghost a written name lands on as its head, read as `find` reads it — nil
    /// for a card, and for what names nothing.
    func ghost(_ raw: String, from base: String?, seeing: Bool = false) -> CardGhost? {
        findEntry(raw, from: base, seeing: seeing).ghost
    }

    /// A section of main to show in an example: its first, or `Schemas`.
    private var exampleSection: String {
        sections.first { $0.parentId == home }.map { localPath(of: $0.id) } ?? "Schemas"
    }

    /// Shorthand for callers that only need the hit.
    func schema(_ raw: String, from base: String?) -> SchemaNode? { find(raw, from: base).schema }

    /// Every card in full, for a message listing what there is.
    func allQualified() -> [String] {
        sections.isEmpty
            ? members(home).map { PathText.quote($0.name) }
            : sections.flatMap { section in members(section.id).map { full($0) } }
    }

    /// Said when a name missing here is present in another section: the usual
    /// mistake with sections is leaving one off.
    private func elsewhere(_ name: String, except sectionId: String) -> String {
        let found = sections.filter { $0.id != sectionId && entry(in: $0.id, named: name) != nil }
        guard !found.isEmpty else { return "" }
        return pathText(.Sections.problemElsewhere(found.map { "\(full(section: $0.id)):\(name)" }.joined(separator: " / ")))
    }
}

extension ProjectDetail {
    /// Built once for these contents and kept: the screen and the tools ask for it
    /// dozens of times over one project read.
    var directory: SchemaDirectory { DirectoryCache.directory(for: self) }

    /// Its cards by id, made once for these contents: every path a document names is walked
    /// through them, and making them for each path cost a pass over the whole project.
    var cardsById: [String: SchemaNode] {
        built.cards { Dictionary(schemas.map { ($0.id, $0) }, uniquingKeysWith: { _, last in last }) }
    }

    /// The sections a card can be moved to, in tree order: each that holds its kind,
    /// but the one it is in — for sorting what the free section holds.
    func homes(for card: SchemaNode) -> [ProjectSection] {
        let directory = directory
        let here = directory.sectionId(of: card)
        return sections.filter { section in
            section.parentId != nil && section.id != here && (section.kind?.holds.contains(card.kind) ?? true)
                && section.kind?.isMainOnly != true
        }
    }

    /// Every section a card can be in, in full and in tree order — for listing what
    /// there is. The roots and the export section hold no cards.
    var sectionPaths: [String] {
        let directory = directory
        return sections.filter { $0.parentId != nil && !($0.kind?.isMainOnly ?? false) }.map { directory.full(section: $0.id) }
    }

    /// Only the schemas of one section, and the relations among them — the diagram
    /// of that section as it was before sections existed. The rest of the project is
    /// still reachable through `sections`, so a name that crosses over is resolved
    /// by whoever asks for it rather than lost.
    func only(section sectionId: String) -> ProjectDetail {
        let directory = self.directory
        guard directory.isSectioned else { return self }
        var out = self
        // A card inside a container is drawn in it — a goal's as its rows — not again on its own.
        out.schemas = schemas.filter { directory.sectionId(of: $0) == sectionId && $0.container == nil }
        // Its lines: those between two of its cards (a card inside a container counted as its
        // container). The lines to the other sections' cards come with `surroundings`.
        out.relations = lines(of: sectionId).inside
        // Its card ghosts are its own; what they lead to comes with `surroundings`.
        out.ghosts = ghosts.filter { directory.sectionId(of: $0) == sectionId }
        return out
    }
}

/// Directories kept with the contents they were built from (`ProjectDetail.built`): the
/// open project's, its sections', what a tool is reading. Any change to a project gives it
/// a new stamp and a place of its own for its directory, so nothing kept is ever stale.
///
/// Kept with the contents, not beside them: kept by stamp, up to 32 and then all let go, every
/// read of a big project held one — each of every card the project had — long after the read
/// was gone; a project of 1,020 cards open while an AI wrote to it grew the app by 10MB a
/// write, for 32 writes, and held it while nothing happened.
enum DirectoryCache {
    static func directory(for detail: ProjectDetail) -> SchemaDirectory {
        detail.built.directory { SchemaDirectory(detail) }
    }
}

/// Where a project's directory is kept once it is built (`ProjectDetail.built`): a box the
/// unchanged copies share. Behind a lock: the app asks from the main thread, but the tests ask
/// from many at once.
nonisolated final class BuiltDirectory: Sendable {
    private let kept = Mutex<SchemaDirectory?>(nil)
    private let byId = Mutex<[String: SchemaNode]?>(nil)

    /// How many boxes hold a directory now: what a test counts.
    static let holding = Atomic<Int>(0)

    /// The directory, built by `make` the first time it is asked for.
    func directory(_ make: () -> SchemaDirectory) -> SchemaDirectory {
        if let hit = kept.withLock({ $0 }) { return hit }
        let made = make()
        // Two built at once from the same contents: either is right, the first kept.
        let first = kept.withLock { held -> Bool in
            guard held == nil else { return false }
            held = made
            return true
        }
        if first { Self.holding.add(1, ordering: .relaxed) }
        return made
    }

    /// The cards by id, made by `make` the first time they are asked for.
    func cards(_ make: () -> [String: SchemaNode]) -> [String: SchemaNode] {
        if let hit = byId.withLock({ $0 }) { return hit }
        let made = make()
        byId.withLock { held in if held == nil { held = made } }
        return made
    }

    deinit {
        if kept.withLock({ $0 != nil }) { Self.holding.subtract(1, ordering: .relaxed) }
    }
}

// MARK: - The other sections, seen from one

/// What a section's diagram draws from the rest of the project.
///
/// A section is drawn on its own, but what it holds and what holds it do not stop
/// at its edge. The schemas its cards point at in other sections come in as
/// reference cards, and the ones in other sections that point at its cards come in
/// as the cards that use it — both faded, both named with their section.
struct Surroundings: Equatable {
    /// Every schema of the other sections, with their real names — what the lines
    /// are resolved against. Not the cards inside a container: those are drawn in it.
    var schemas: [SchemaNode] = []
    /// The cards inside a container (a goal's goals and tasks), of every section: a line
    /// of one is drawn from or to its container's row for it.
    var inside: [SchemaNode] = []
    /// The project's lines that are not between two of the section's own cards (`ProjectDetail.lines`):
    /// the diagram draws those of them about the section — to and from the other sections'
    /// cards, between its children.
    var lines: [Relation] = []
    /// Each of those as the section on screen writes it: `Core:Address`.
    var names: [String: String] = [:]
    /// Ones this section's cards point at, in project order.
    var references: [String] = []
    /// Ones that point at this section's cards, in project order. A schema that is
    /// both is a reference.
    var consumers: [String] = []
    /// Where this section put them, when someone moved one. The rest are placed
    /// beside the section: references on the right, consumers on the left.
    var positions: [String: CGPoint] = [:]
    /// Its card ghosts (`specboard://SpecBoard/Docs/Design:DesignSections` §2): each a box of its own on its
    /// diagram, where it sits, the card it leads to drawn whole in it.
    var ghosts: [CardGhost] = []
    /// What a merge would take away from the section, looked at as main would be once merged
    /// (`AppModel.ResultView.gone`): main's cards, where main has them — drawn
    /// faded and struck through, no line to them, only read.
    var gone: [SchemaNode] = []
    /// Each ghost's header: the shortcut's mark, its name and what it points at — the name
    /// left out where it is the card's (`SchemaDirectory.ghostTitle`).
    var ghostTitles: [String: String] = [:]
    /// Of its ghosts, the ones that lead nowhere, and why, in a few words.
    var ghostProblems: [String: String] = [:]
    /// Every card ghost of the project — what a path or a `ref` written through one is read
    /// through — and each as this section writes it (`secA:Admin`, bare in its own).
    var everyGhost: [CardGhost] = []
    var ghostNames: [String: String] = [:]
    /// Other sections' cards put on it for a look (`AppModel.showForNow`), drawn whole,
    /// whether or not anything points at them, and whatever the switches say.
    var forNow: [String] = []
    /// Its own value objects and enums drawn as cards of their own: the ones it shows on
    /// their own (`standalone`), and the ones put there for a look. They have no card
    /// otherwise.
    var here: [String] = []
    /// Of `forNow` and `here`, the ones put there for a look (`AppModel`), not kept.
    var temporary: Set<String> = []
    /// For a section with sections inside: each card under one of them, by the child
    /// it is in — the section ghost it goes into (design-sections §6).
    var children: [String: String] = [:]
    /// Each of those children as the section on screen writes it, for its card's title.
    var childNames: [String: String] = [:]
    /// Children's cards that link to another child's, in project order: drawn with the
    /// line between the two, which touches neither end of this section's own.
    var across: [String] = []
    /// Frames drawn behind the cards, each round some of them, titled: a card's connections map draws
    /// one round each group of its cards (`ConnectionsMap.Group`).
    var frames: [CardFrame] = []
    /// For a working directory's section or a folder in one: the folders inside, by name, each drawn as a
    /// box whether or not a line reaches it (`CompactBox`) — by its section ghost's id, with its name.
    var folders: [FolderBox] = []
    /// Of those, the ones reading the folder no longer finds, kept for the notes in them.
    var missingFolders: Set<String> = []

    /// A folder drawn as a box on its working directory's section map.
    struct FolderBox: Equatable {
        var id: String
        var name: String
    }

    var isEmpty: Bool { schemas.isEmpty }
}

/// A frame drawn round some cards of a diagram, behind them, with a title and how many they are.
struct CardFrame: Equatable {
    var id: String
    var title: String
    var cards: [String]
}

extension ProjectDetail {
    /// The board's detail with main's beside it (`AppModel.outsideDetail`): one project of both
    /// trees, for what follows lines across them — a board's card's connections diagram. Each card
    /// keeps its own tree's section, and is named from the board as a card of another tree is: in
    /// full.
    func joined(with outside: ProjectDetail) -> ProjectDetail {
        var out = self
        let mine = Set(schemas.map(\.id))
        out.schemas += outside.schemas.filter { !mine.contains($0.id) }
        out.sections += outside.sections
        out.relations += outside.relations
        out.ghosts += outside.ghosts
        out.inner += outside.inner
        return out
    }

    /// What each card inside a container is drawn as on a section's diagram, by its id: a
    /// flow's node as its flow, a card inside a goal as the outermost container it is in.
    /// A card in a section of its own is not in it: it is drawn as itself.
    func heldIn() -> [String: String] {
        let byId = Dictionary(schemas.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        func top(_ id: String) -> String {
            var at = id
            var seen: Set<String> = [id]
            while let up = byId[at]?.container, seen.insert(up).inserted { at = up }
            return at
        }
        var out: [String: String] = [:]
        for card in schemas where card.container != nil { out[card.id] = top(card.id) }
        for node in inner { out[node.id] = top(node.container) }
        return out
    }

    /// The project's lines as a section's diagram takes them: `inside`, between two of its cards —
    /// each end the card, or the container holding it, of the section — and the rest,
    /// `outside`. A line between two cards inside one container is in neither: that
    /// container's card map draws it.
    func lines(of sectionId: String) -> (inside: [Relation], outside: [Relation]) {
        let directory = directory
        let held = heldIn()
        let here = Set(schemas.filter { $0.container == nil && directory.sectionId(of: $0) == sectionId }.map(\.id))
        var inside: [Relation] = [], outside: [Relation] = []
        for relation in relations {
            let from = held[relation.fromSchemaId] ?? relation.fromSchemaId
            let to = held[relation.toSchemaId] ?? relation.toSchemaId
            if from == to && (from != relation.fromSchemaId || to != relation.toSchemaId) { continue }
            if here.contains(from) && here.contains(to) {
                inside.append(relation)
            } else {
                outside.append(relation)
            }
        }
        return (inside, outside)
    }

    /// `Surroundings` for one section, from every line the project has (`relations`).
    /// `outside`: main's detail, for the board (`AppModel.outsideDetail`) — the main cards its
    /// cards reach come in as the other sections' cards do.
    func surroundings(of sectionId: String,
                      positions: [String: CGPoint] = [:], outside: ProjectDetail? = nil) -> Surroundings {
        let directory = directory
        guard directory.isSectioned else { return Surroundings() }
        var out = Surroundings()
        out.schemas = schemas.filter { directory.sectionId(of: $0) != sectionId && $0.container == nil }
        out.inside = schemas.filter { $0.container != nil }
        out.lines = lines(of: sectionId).outside
        for schema in out.schemas {
            out.names[schema.id] = directory.shown(schema, from: sectionId)
        }
        var section: [String: String] = [:]
        for schema in out.schemas { section[schema.id] = directory.sectionId(of: schema) }
        for schema in schemas where directory.sectionId(of: schema) == sectionId {
            section[schema.id] = sectionId
        }
        // main's cards a board's card reaches: as another section's cards, in main's sections —
        // folded into a section ghost for each (`SectionGhosts`) — and named in full.
        if let outside {
            let mains = Dictionary(outside.schemas.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
            let mainDirectory = outside.directory
            for relation in relations where relation.outside {
                guard var card = mains[relation.toSchemaId], out.names[card.id] == nil else { continue }
                card.sectionId = mainDirectory.sectionId(of: card)
                out.schemas.append(card)
                out.names[card.id] = mainDirectory.full(card)
                section[card.id] = card.sectionId
            }
        }

        var references = Set<String>(), consumers = Set<String>()
        // Every line of the project, between the cards it is drawn between: a node's is its
        // flow's, a card inside a goal its goal's.
        let held = heldIn()
        func holder(of id: String) -> String { held[id] ?? id }
        for relation in relations {
            let (fromId, toId) = (holder(of: relation.fromSchemaId), holder(of: relation.toSchemaId))
            guard let from = section[fromId], let to = section[toId], from != to else { continue }
            if from == sectionId { references.insert(toId) }
            if to == sectionId { consumers.insert(fromId) }
        }
        consumers.subtract(references)
        out.references = out.schemas.map(\.id).filter(references.contains)
        out.consumers = out.schemas.map(\.id).filter(consumers.contains)
        out.positions = positions.filter { id, _ in section[id] != nil && section[id] != sectionId }
        out.ghosts = directory.ghosts(in: sectionId)
        for ghost in out.ghosts {
            out.ghostTitles[ghost.id] = directory.ghostTitle(ghost)
            if directory.reached(ghost) == nil {
                out.ghostProblems[ghost.id] = directory.goesRound(ghost)
                    ? L10n[.Sections.ghostDrawnRound] : L10n[.Sections.ghostDrawnGone]
            }
        }
        out.everyGhost = ghosts
        for ghost in ghosts { out.ghostNames[ghost.id] = directory.shown(ghost, from: sectionId) }
        out.here = directory.members(sectionId)
            .filter { ($0.kind == .value || $0.kind == .enum) && $0.standalone }.map(\.id)
        // A section with sections inside is drawn with a card for each of them, what
        // is deeper going into the child it is under, and what the children do to
        // each other drawn between their cards.
        let children = directory.children(of: sectionId)
        if !children.isEmpty {
            let childIds = Set(children.map(\.id))
            for child in children { out.childNames[child.id] = directory.shown(section: child.id) }
            for schema in out.schemas {
                var at: String? = directory.sectionId(of: schema)
                while let id = at, !childIds.contains(id) { at = directory.section(id: id)?.parentId }
                if let child = at { out.children[schema.id] = child }
            }
            var linked = Set<String>()
            for relation in relations {
                let (fromId, toId) = (holder(of: relation.fromSchemaId), holder(of: relation.toSchemaId))
                guard let from = out.children[fromId], let to = out.children[toId], from != to else { continue }
                linked.insert(fromId)
                linked.insert(toId)
            }
            out.across = out.schemas.map(\.id).filter(linked.contains)
        }
        return out
    }

}


/// What a kind of section's definition says its sections carry in the nav panel —
/// `icon: {name, color}`: an SF Symbols name and a colour (`#RRGGBB`). Said of neither, a folder in grey.
nonisolated struct SectionMark: Hashable, Sendable {
    var icon: String?
    var color: String?

    static let standardIcon = "folder"

    /// The symbol drawn: the one said, when the system has it, else a folder.
    var symbol: String {
        guard let icon, NSImage(systemSymbolName: icon, accessibilityDescription: nil) != nil else { return Self.standardIcon }
        return icon
    }
}
