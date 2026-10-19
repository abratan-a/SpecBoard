import AppKit
import SwiftUI
import UniformTypeIdentifiers

/// What is open — main, or what one branch sees (`specboard://SpecBoard/Docs/Design:DesignBranches` §2) — as
/// one tree: its sections; in each section the sections inside it, what its lines reach
/// of the others (folded into section ghosts on its diagram), and its cards and card
/// ghosts, folded in a group of their own. Which branch is open is picked above it
/// (`BranchList`).
///
///     main  Main
///       ▸ Docs
///       ▾ Ordering
///         ▸ Payments          ← a section inside
///         ▸ section ghosts    ← worked out: the other sections' cards its lines reach
///         ▾ cards             ← its cards, folded until opened
///             Order
///             ↪ Buyer → Tenant:User   ← a card ghost: a shortcut to another's card
///           ▸ values / enums  ← drawn inside the cards that hold them
///
/// The browser before it went one level at a time behind a trail, so a deep
/// section was always several clicks up and down again. The rows are worked out
/// here, apart from the view, so they can be tested.
enum NavPanelTree {
    enum Node: Hashable {
        /// The root or a section.
        case section(String)
        /// Other sections' cards a section's lines reach — it points at them, or they at
        /// it — drawn folded into its section ghosts (one per section); the ones a card
        /// ghost of it leads to left out. Worked out, never placed.
        case auto(String)
        /// One such section, seen from a section: one of its section ghosts.
        case autoSection(String, String)
        /// One such card, seen from a section: a row of a section ghost.
        case autoCard(String, String)
        /// A section's cards and card ghosts, folded until opened: a section holding many
        /// would otherwise bury the sections around them.
        case cards(String)
        /// A card — and a container's (a goal's, a flow's), opened as a section is to the
        /// cards inside it (`specboard://SpecBoard/Docs/Design:CardKinds`「入れ物のカード」).
        case card(String)
        /// A flow's node: a card inside the flow (`ProjectDetail.inner`), by its id.
        case node(String)
        /// A card ghost (`specboard://SpecBoard/Docs/Design:DesignSections` §2), among its section's cards: a
        /// shortcut to another section's card.
        case ghost(String)
        /// A section's value objects and enums, inside its cards: no cards of their
        /// own on the diagram, so they are listed apart from the ones that have.
        case values(String)
        /// Nothing under an open row.
        case none(String)
        /// What the open branch removed — a card, a card ghost, a flow's node — where it was, by its
        /// id: shown read only, never moved.
        case gone(String)
        /// A section the open branch removed, where it was: what was removed in it under it.
        case goneSection(String)
        /// A function or type of a working directory's file, under its file's row (a method under its
        /// type's): the file's id and the symbol's path in it. Listed from what was read of the file
        /// (`AppModel.outline`), read as the file's row opens.
        case symbol(String, String)

        var key: String {
            switch self {
            case .section(let id): "s|\(id)"
            case .auto(let id): "a|\(id)"
            case .autoSection(let at, let other): "as|\(at)|\(other)"
            case .autoCard(let at, let card): "ac|\(at)|\(card)"
            case .cards(let id): "cs|\(id)"
            case .card(let id): "c|\(id)"
            case .node(let id): "nd|\(id)"
            case .ghost(let id): "gh|\(id)"
            case .values(let id): "v|\(id)"
            case .none(let under): "n|\(under)"
            case .gone(let id): "g|\(id)"
            case .goneSection(let id): "gs|\(id)"
            case .symbol(let file, let path): "sy|\(file)|\(path)"
            }
        }

        var opens: Bool {
            switch self {
            case .section, .auto, .autoSection, .cards, .values, .goneSection: true
            default: false
            }
        }

        /// Whether the row opens, in this project: a container card holding anything opens too —
        /// what the open branch removed from it (`gone`) counted —, a file whose functions and types are
        /// listed, and a type with members among what was read of its file (`outlines`).
        func opens(in directory: SchemaDirectory, gone: BranchMarks.Gone = BranchMarks.Gone(),
                   outlines: [String: FileOutline] = [:]) -> Bool {
            if case .symbol(let file, let path) = self { return outlines[file]?.holds(path) ?? false }
            guard case .card(let id) = self else { return opens }
            guard let card = directory.schema(id: id) else { return false }
            if card.isCodeFile { return true }
            return !card.inside.isEmpty || !card.nodes.isEmpty
                || gone.cards.contains { $0.container == id } || gone.nodes.contains { $0.container == id }
        }
    }

    struct Row: Identifiable, Hashable {
        var node: Node
        var depth: Int
        var id: String { node.key }
    }

    /// What the nav panel's search box finds: its words, and `kind:` and `tag:` — a card of any kind
    /// named with `kind:` (its kind's name, built in or the project's own; a flow's node by its kind
    /// too), with any tag named with `tag:` (a tag group, any tag in it), and every other word in
    /// its name, label or tags. Kinds and tags each one of those named; all of those asked together.
    struct Filter {
        var query = ""

        /// The search box read: the kinds and the tags named, and the words, lowercased.
        struct Asked: Equatable {
            var kinds: [String] = []
            var tags: [String] = []
            var words: [String] = []

            var isEmpty: Bool { kinds.isEmpty && tags.isEmpty && words.isEmpty }
        }

        static let kindWord = "kind:"
        static let tagWord = "tag:"

        static func asked(_ query: String) -> Asked {
            var out = Asked()
            for token in query.split(whereSeparator: \.isWhitespace).map(String.init) {
                let lower = token.lowercased()
                if lower.hasPrefix(kindWord) {
                    if lower.count > kindWord.count { out.kinds.append(String(lower.dropFirst(kindWord.count))) }
                } else if lower.hasPrefix(tagWord) {
                    if lower.count > tagWord.count { out.tags.append(String(lower.dropFirst(tagWord.count))) }
                } else {
                    out.words.append(lower)
                }
            }
            return out
        }

        var asked: Asked { Self.asked(query) }
        var isOn: Bool { !asked.isEmpty }

        /// The tags `tag:` names, a tag group as the tags in it, lowercased.
        static func tags(_ asked: Asked, groups: [TagGroup]) -> Set<String> {
            let named = asked.tags
            let byName = Dictionary(groups.map { ($0.name.lowercased(), $0.name) }, uniquingKeysWith: { a, _ in a })
            let groupsNamed = named.compactMap { byName[$0] }
            return Set(named + expandTags(groups: groups, selected: groupsNamed).map { $0.lowercased() })
        }

        /// A card it finds: of a kind named, with a tag named, every word in its name, label or tags.
        func matches(_ schema: SchemaNode, tags: Set<String>) -> Bool {
            let asked = self.asked
            return Self.kindAndTag(schema, asked, tags)
                && asked.words.allSatisfy { Self.says(schema, $0) }
        }

        /// A card ghost, by its own name or by the card it leads to — of a kind and with a tag
        /// asked, as the card is; one that leads nowhere, by its name alone.
        func matches(_ ghost: CardGhost, leadingTo card: SchemaNode?, tags: Set<String>) -> Bool {
            let asked = self.asked
            if let card {
                guard Self.kindAndTag(card, asked, tags) else { return false }
            } else if !asked.kinds.isEmpty || !asked.tags.isEmpty {
                return false
            }
            return asked.words.allSatisfy { word in
                ghost.name.lowercased().contains(word) || card.map { Self.says($0, word) } == true
            }
        }

        /// Of a kind named, with a tag named — each asked only when any is named.
        private static func kindAndTag(_ schema: SchemaNode, _ asked: Asked, _ tags: Set<String>) -> Bool {
            if !asked.kinds.isEmpty, !asked.kinds.contains(schema.kindName.lowercased()) { return false }
            if !asked.tags.isEmpty, !schema.tags.contains(where: { tags.contains($0.lowercased()) }) { return false }
            return true
        }

        /// Whether a word asked is in a card's label: its row shows its label while the box is written in.
        func byLabel(_ schema: SchemaNode) -> Bool { byLabel(schema.label) }

        /// Whether a word asked is in a node's label.
        func byLabel(_ node: InnerCard) -> Bool { byLabel(node.label) }

        private func byLabel(_ label: String) -> Bool {
            let label = label.lowercased()
            return !label.isEmpty && asked.words.contains { label.contains($0) }
        }

        /// A word in its name, label or tags.
        private static func says(_ schema: SchemaNode, _ word: String) -> Bool {
            schema.name.lowercased().contains(word) || schema.label.lowercased().contains(word)
                || schema.tags.contains { $0.lowercased().contains(word) }
        }

        /// A flow's node it finds: by its words, or by its own kind named (`kind:process`); never by
        /// a tag, and not every node of a flow found by its kind alone.
        func matches(_ node: InnerCard) -> Bool {
            let asked = self.asked
            guard asked.tags.isEmpty else { return false }
            let ownKind = asked.kinds.contains(node.kind.lowercased())
            if !asked.kinds.isEmpty, !ownKind, !asked.kinds.contains(SchemaKind.flow.rawValue) { return false }
            guard ownKind || !asked.words.isEmpty else { return false }
            return asked.words.allSatisfy { node.name.lowercased().contains($0) || node.label.lowercased().contains($0) }
        }

        /// What the box would read with `token` put in: in place of the last word when that is
        /// the start of it (`kind:fl` → `kind:flow`), else after what is there.
        static func putting(_ token: String, into query: String) -> String {
            var words = query.split(whereSeparator: \.isWhitespace).map(String.init)
            if let last = words.last, !query.hasSuffix(" "), token.lowercased().hasPrefix(last.lowercased()) {
                words.removeLast()
            }
            return (words + [token]).joined(separator: " ") + " "
        }
    }

    /// One thing the search box offers while it is being written in: the word put in
    /// (`kind:flow`), how many cards it would find, and what it is.
    struct Suggestion: Equatable, Identifiable {
        var token: String
        var count: Int
        var help: String
        var id: String { token }
    }

    /// What the search box offers: the kinds of card the project has, built in
    /// and its own (each by its own name), and its tags and tag groups — each with how many cards
    /// it finds, most first — those the word being written starts (`kind:fl`, `tag:決`) when it
    /// starts with one of them.
    static func suggestions(_ detail: ProjectDetail, query: String) -> (kinds: [Suggestion], tags: [Suggestion]) {
        let last = query.hasSuffix(" ") ? "" : (query.split(whereSeparator: \.isWhitespace).last.map(String.init) ?? "")
        let lower = last.lowercased()
        let typing = lower.hasPrefix(Filter.kindWord) || lower.hasPrefix(Filter.tagWord)
        func keep(_ token: String) -> Bool { !typing || token.lowercased().hasPrefix(lower) }
        var kinds: [Suggestion] = []
        for kind in SchemaKind.made {
            let count = detail.schemas.filter { $0.kind == kind }.count
            let token = Filter.kindWord + kind.rawValue
            if count > 0, keep(token) {
                kinds.append(Suggestion(token: token, count: count, help: Glossary.term(kind.rawValue)?.meaning ?? ""))
            }
        }
        for def in detail.cardKinds.own {
            let count = detail.schemas.filter { $0.kind == .custom && $0.ownKind == def.kind }.count
            let token = Filter.kindWord + def.kind
            if count > 0, keep(token) {
                kinds.append(Suggestion(token: token, count: count, help: L10n[.Nav.suggestProjectKind(def.label)]))
            }
        }
        var counts: [String: Int] = [:]
        for schema in detail.schemas {
            for tag in Set(schema.tags) { counts[tag, default: 0] += 1 }
        }
        var tags: [Suggestion] = detail.project.tagGroups.compactMap { group in
            let inside = Set(expandTags(groups: detail.project.tagGroups, selected: [group.name]))
            let count = detail.schemas.filter { !inside.isDisjoint(with: $0.tags) }.count
            let token = Filter.tagWord + group.name
            guard count > 0, keep(token) else { return nil }
            return Suggestion(token: token, count: count, help: L10n[.Nav.suggestTagGroup(inside.sorted().joined(separator: L10n[.Common.separatorList]))])
        }
        tags += counts.compactMap { tag, count in
            let token = Filter.tagWord + tag
            return keep(token) ? Suggestion(token: token, count: count, help: L10n[.Nav.suggestTag]) : nil
        }
        .sorted { ($0.count, $1.token) > ($1.count, $0.token) }
        return (kinds, tags)
    }

    /// The rows on screen: what is open, opened; under a filter, only the cards it
    /// finds and the sections above them, all open — the groups holding them too. What the open
    /// branch removed (`gone`) where it was, each in its order among the rest; not under a filter.
    static func rows(_ detail: ProjectDetail, expanded: Set<String>, filter: Filter = Filter(),
                     gone: BranchMarks.Gone = BranchMarks.Gone(), outlines: [String: FileOutline] = [:]) -> [Row] {
        let directory = detail.directory
        var out: [Row] = []
        let filtering = filter.isOn
        let gone = filtering ? BranchMarks.Gone() : gone
        let goneSections = Dictionary(grouping: gone.sections, by: { $0.parentId ?? "" })
        let goneCards = Dictionary(grouping: gone.cards.filter { $0.container == nil }, by: { $0.sectionId ?? directory.home })
        let goneInside = Dictionary(grouping: gone.cards.filter { $0.container != nil }, by: { $0.container ?? "" })
        let goneGhosts = Dictionary(grouping: gone.ghosts, by: { $0.sectionId ?? directory.home })
        let goneNodes = Dictionary(grouping: gone.nodes, by: \.container)
        func byPlace(_ card: SchemaNode) -> (Int, String) { (card.sortOrder, card.name) }
        let tags = Filter.tags(filter.asked, groups: detail.project.tagGroups)
        // The nodes a filter finds by their names, and the cards it finds, with the containers
        // they are in: a card inside one is listed under it.
        let foundNodes = Set(detail.inner.filter(filter.matches).map(\.id))
        var foundCards = Set(detail.schemas.filter { filter.matches($0, tags: tags) }.map(\.id))
            .union(detail.inner.filter { foundNodes.contains($0.id) }.map(\.container))
        for id in foundCards {
            var at = directory.schema(id: id)?.container
            while let up = at, foundCards.insert(up).inserted { at = directory.schema(id: up)?.container }
        }
        // The files found by the names of their functions and types, among those read so far: each
        // symbol found, and what it is in.
        let foundSymbols = filtering ? Self.symbolsFound(filter.asked, outlines: outlines, in: directory) : [:]
        foundCards.formUnion(foundSymbols.keys)
        let found = foundCards
            .union(detail.ghosts.filter { filter.matches($0, leadingTo: directory.reached($0), tags: tags) }.map(\.id))
        var keep: Set<String> = []
        if filtering {
            let sections = detail.schemas.filter { found.contains($0.id) }.map(directory.sectionId(of:))
                + detail.ghosts.filter { found.contains($0.id) }.map(directory.sectionId(of:))
            for section in sections {
                var at: String? = section
                while let id = at {
                    keep.insert(id)
                    at = directory.section(id: id)?.parentId
                }
            }
        }
        func isOpen(_ node: Node) -> Bool { expanded.contains(node.key) }
        // In the section's own order (the cards' `order`), which a drag between them changes.
        func cards(of section: String) -> [SchemaNode] {
            directory.members(section).filter { !filtering || found.contains($0.id) }
        }
        // Its card ghosts, in their own order, after its cards.
        func ghosts(of section: String) -> [CardGhost] {
            directory.ghosts(in: section).filter { !filtering || found.contains($0.id) }
        }

        func walk(_ section: ProjectSection, _ depth: Int, removed: Bool = false) {
            if filtering, !keep.contains(section.id) { return }
            let node = removed ? Node.goneSection(section.id) : Node.section(section.id)
            out.append(Row(node: node, depth: depth))
            guard filtering || isOpen(node) else { return }
            let children = merged(directory.children(of: section.id), goneSections[section.id] ?? []) { ($0.sortOrder, $0.name) }
            for (child, isGone) in children { walk(child, depth + 1, removed: removed || isGone) }
            // A root draws no diagram of its own (its dashboard is there): no section ghosts.
            if !filtering, !removed, section.parentId != nil {
                let auto = Node.auto(section.id)
                out.append(Row(node: auto, depth: depth + 1))
                if isOpen(auto) { autoRows(section, depth + 2) }
            }
            // No group where there is nothing to hold, as for the values below.
            let held = merged(cards(of: section.id), goneCards[section.id] ?? [], order: byPlace)
            let shortcuts = merged(ghosts(of: section.id), goneGhosts[section.id] ?? []) { ($0.sortOrder, $0.name) }
            guard !held.isEmpty || !shortcuts.isEmpty else { return }
            let group = Node.cards(section.id)
            out.append(Row(node: group, depth: depth + 1))
            guard filtering || isOpen(group) else { return }
            func isType(_ card: SchemaNode) -> Bool { card.kind == .value || card.kind == .enum }
            for (card, isGone) in held where !isType(card) {
                out.append(Row(node: isGone ? .gone(card.id) : .card(card.id), depth: depth + 2))
                if !isGone { inside(card, depth + 3) }
            }
            for (ghost, isGone) in shortcuts { out.append(Row(node: isGone ? .gone(ghost.id) : .ghost(ghost.id), depth: depth + 2)) }
            let types = held.filter { isType($0.0) }
            if !types.isEmpty {
                let values = Node.values(section.id)
                out.append(Row(node: values, depth: depth + 2))
                if filtering || isOpen(values) {
                    for (card, isGone) in types { out.append(Row(node: isGone ? .gone(card.id) : .card(card.id), depth: depth + 3)) }
                }
            }
        }
        // What a container holds, under it, as a section's cards are: a goal's cards (and on
        // into a goal inside), a flow's nodes — when it is opened, or a filter found them. A
        // container the filter finds by its own name lists only what it found inside, as a
        // section lists only the cards found in it.
        func inside(_ card: SchemaNode, _ depth: Int) {
            let node = Node.card(card.id)
            guard filtering || isOpen(node) else { return }
            if card.isCodeFile {
                symbols(of: card.id, under: nil, depth)
                return
            }
            if card.kind == .flow {
                let nodes = detail.inner.filter { $0.container == card.id && (!filtering || foundNodes.contains($0.id)) }
                for (held, isGone) in merged(nodes, goneNodes[card.id] ?? [], order: { ($0.sortOrder, $0.name) }) {
                    out.append(Row(node: isGone ? .gone(held.id) : .node(held.id), depth: depth))
                }
                return
            }
            let held = card.inside.compactMap { directory.schema(id: $0) }.filter { !filtering || found.contains($0.id) }
            for (held, isGone) in merged(held, goneInside[card.id] ?? [], order: byPlace) {
                out.append(Row(node: isGone ? .gone(held.id) : .card(held.id), depth: depth))
                if !isGone { inside(held, depth + 1) }
            }
        }
        // A file's functions and types, a type's members under it: as read so far.
        func symbols(of file: String, under parent: String?, _ depth: Int) {
            guard let outline = outlines[file] else { return }
            for symbol in outline.children(of: parent) where !filtering || foundSymbols[file]?.contains(symbol.path) == true {
                let node = Node.symbol(file, symbol.path)
                out.append(Row(node: node, depth: depth))
                if filtering || isOpen(node) { symbols(of: file, under: symbol.path, depth + 1) }
            }
        }
        func autoRows(_ section: ProjectSection, _ depth: Int) {
            let auto = Node.auto(section.id)
            let around = pointedAt(detail, from: section.id)
            if around.isEmpty { out.append(Row(node: .none(auto.key), depth: depth)) }
            for (other, members) in around {
                let node = Node.autoSection(section.id, other)
                out.append(Row(node: node, depth: depth))
                if isOpen(node) {
                    for card in members { out.append(Row(node: .autoCard(section.id, card), depth: depth + 1)) }
                }
            }
        }

        let roots = directory.children(of: nil)
        guard !roots.isEmpty else {
            return cards(of: directory.home).map { Row(node: .card($0.id), depth: 0) }
        }
        for root in roots { walk(root, 0) }
        return out
    }

    /// The functions and types the search box finds by their names, in the files of `directory` read so
    /// far: by file, each found and what it is in. None while it asks a kind or a tag — those are cards'.
    static func symbolsFound(_ asked: Filter.Asked, outlines: [String: FileOutline],
                             in directory: SchemaDirectory) -> [String: Set<String>] {
        guard asked.kinds.isEmpty, asked.tags.isEmpty, !asked.words.isEmpty else { return [:] }
        var out: [String: Set<String>] = [:]
        for (id, outline) in outlines where directory.schema(id: id) != nil {
            var hit: Set<String> = []
            for symbol in outline.symbols where asked.words.allSatisfy({ symbol.name.lowercased().contains($0) }) {
                var at: String? = symbol.path
                while let path = at, hit.insert(path).inserted { at = outline.symbol(path)?.parent }
            }
            if !hit.isEmpty { out[id] = hit }
        }
        return out
    }

    /// The files whose rows are open among `expanded`: what is read of them is listed under them.
    static func openFiles(_ expanded: Set<String>) -> [String] {
        let head = Node.card("").key
        return expanded.compactMap { key in
            guard key.hasPrefix(head) else { return nil }
            let id = String(key.dropFirst(head.count))
            return WorkDirIds.isFile(id) ? id : nil
        }
    }

    /// What is there (in its order) and what was removed from among it, each where its order puts it:
    /// whether each is one removed beside it.
    static func merged<T>(_ there: [T], _ removed: [T], order: (T) -> (Int, String)) -> [(T, Bool)] {
        guard !removed.isEmpty else { return there.map { ($0, false) } }
        var rest = removed.sorted { order($0) < order($1) }
        var out: [(T, Bool)] = []
        for item in there {
            while let first = rest.first, order(first) < order(item) {
                out.append((first, true))
                rest.removeFirst()
            }
            out.append((item, false))
        }
        return out + rest.map { ($0, true) }
    }

    /// The marks on the rows (`specboard://SpecBoard/Docs/Design:UI` §2「変更の印」), by the row's key: a
    /// thing's own — what the open branch did to it, what main did since — and, on a section, a
    /// container card and the groups of a section's cards, ● for what changed inside them, the
    /// sections and containers inside too. A thing's own mark is said before what is inside it.
    struct Marks {
        private var byKey: [String: ChangeMark] = [:]

        init() {}

        init(_ detail: ProjectDetail, _ marks: BranchMarks) {
            guard !marks.isEmpty || !marks.gone.isEmpty else { return }
            let directory = detail.directory
            let gone = marks.gone
            let goneCards = Dictionary(gone.cards.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
            let goneSections = Dictionary(gone.sections.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
            var inside: [String: ChangeMark.Inside] = [:]
            var own: [String: ChangeMark] = [:]
            func container(of id: String) -> String? { directory.schema(id: id)?.container ?? goneCards[id]?.container }
            func sectionOf(card id: String) -> String {
                if let card = directory.schema(id: id) { return directory.sectionId(of: card) }
                return goneCards[id]?.sectionId ?? directory.home
            }
            // Told to every section from `section` up.
            func up(from section: String?, _ flags: ChangeMark.Inside) {
                var at = section
                var seen: Set<String> = []
                while let id = at, seen.insert(id).inserted {
                    inside[Node.section(id).key, default: []].formUnion(flags)
                    at = directory.section(id: id)?.parentId ?? goneSections[id]?.parentId
                }
            }
            // A thing's mark, and what it tells the containers, the groups and the sections it is in.
            func thing(_ key: String, _ mark: ChangeMark?, section: String, container start: String?, type: Bool) {
                guard let mark else { return }
                own[key] = mark
                let flags = ChangeMark.Inside(mark)
                var at = start
                var seen: Set<String> = []
                while let id = at, seen.insert(id).inserted {
                    inside[Node.card(id).key, default: []].formUnion(flags)
                    at = container(of: id)
                }
                inside[Node.cards(section).key, default: []].formUnion(flags)
                if type { inside[Node.values(section).key, default: []].formUnion(flags) }
                up(from: section, flags)
            }
            func isType(_ card: SchemaNode) -> Bool { card.container == nil && (card.kind == .value || card.kind == .enum) }
            for card in detail.schemas {
                thing(Node.card(card.id).key, marks.mark(for: card.id), section: directory.sectionId(of: card),
                      container: card.container, type: isType(card))
            }
            for ghost in detail.ghosts {
                thing(Node.ghost(ghost.id).key, marks.mark(for: ghost.id), section: directory.sectionId(of: ghost),
                      container: nil, type: false)
            }
            for node in detail.inner {
                thing(Node.node(node.id).key, marks.mark(for: node.id), section: node.sectionId ?? sectionOf(card: node.container),
                      container: node.container, type: false)
            }
            for card in gone.cards {
                thing(Node.gone(card.id).key, .removed, section: card.sectionId ?? directory.home,
                      container: card.container, type: isType(card))
            }
            for ghost in gone.ghosts {
                thing(Node.gone(ghost.id).key, .removed, section: ghost.sectionId ?? directory.home, container: nil, type: false)
            }
            for node in gone.nodes {
                thing(Node.gone(node.id).key, .removed, section: node.sectionId ?? sectionOf(card: node.container),
                      container: node.container, type: false)
            }
            for section in detail.sections {
                guard let mark = marks.sectionMark(for: section.id) else { continue }
                own[Node.section(section.id).key] = mark
                up(from: section.parentId, ChangeMark.Inside(mark))
            }
            for section in gone.sections {
                own[Node.goneSection(section.id).key] = .removed
                up(from: section.parentId, .removed)
            }
            byKey = inside.compactMapValues(\.mark).merging(own) { _, mine in mine }
        }

        /// A row's mark.
        func of(_ node: Node) -> ChangeMark? { byKey[node.key] }
    }

    /// How each row hangs from its parent, drawn as lines: one entry per level above
    /// the row. At its own level, a branch to it that goes on to a sibling below
    /// (`tee`) or stops at it (`end`); above, a line running through when that
    /// ancestor has a sibling further down (`line`), else nothing.
    enum Guide: Hashable { case none, line, tee, end }

    static func guides(_ rows: [Row]) -> [String: [Guide]] {
        var out: [String: [Guide]] = [:]
        // Walking up from the bottom: whether a row further down sits at this depth
        // with nothing shallower between.
        var below: [Bool] = []
        for row in rows.reversed() {
            let depth = row.depth
            if below.count < depth + 2 { below += Array(repeating: false, count: depth + 2 - below.count) }
            out[row.id] = (0..<depth).map { level in
                level == depth - 1 ? (below[depth] ? .tee : .end) : (below[level + 1] ? .line : .none)
            }
            below[depth] = true
            for level in (depth + 1)..<below.count { below[level] = false }
        }
        return out
    }

    /// The other sections' cards a section's lines reach — what its diagram folds
    /// into section ghosts — by section, in project order; the ones its card ghosts lead to
    /// left out (they are drawn whole, in the ghosts).
    static func pointedAt(_ detail: ProjectDetail, from sectionId: String) -> [(section: String, cards: [String])] {
        let around = detail.surroundings(of: sectionId)
        let directory = detail.directory
        let placed = Set(around.ghosts.compactMap(\.resolvesTo))
        var order: [String] = []
        var cards: [String: [String]] = [:]
        for id in around.references + around.consumers where !placed.contains(id) {
            // Value objects and enums are inside the cards holding them.
            guard let schema = directory.schema(id: id), schema.kind != .value, schema.kind != .enum else { continue }
            let section = directory.sectionId(of: schema)
            if cards[section] == nil { order.append(section) }
            cards[section, default: []].append(id)
        }
        return order.map { ($0, cards[$0] ?? []) }
    }

    /// What is open when a project is first seen: the root, and the way down to the
    /// section on screen.
    static func opening(_ detail: ProjectDetail, showing section: String) -> Set<String> {
        let directory = detail.directory
        var out = Set(directory.children(of: nil).map { Node.section($0.id).key })
        out.formUnion(way(to: section, in: directory))
        return out
    }

    /// What is open when the tree shows a project again: what was kept, as it was — folded
    /// rows stay folded even above the section on screen, or "Collapse all" would not
    /// last. Only a project never seen before opens its way (`opening`).
    static func reopened(_ kept: [String]?, _ detail: ProjectDetail, showing section: String) -> Set<String> {
        kept.map(Set.init) ?? opening(detail, showing: section)
    }

    /// The rows to open so that `section` shows: it and everything above it.
    static func way(to section: String, in directory: SchemaDirectory) -> Set<String> {
        var out: Set<String> = []
        var at: String? = section
        while let id = at {
            out.insert(Node.section(id).key)
            at = directory.section(id: id)?.parentId
        }
        return out
    }

    /// The rows to open so that a card shows: the way to its section, the section's
    /// cards, and the value objects and enums among them when it is one.
    static func way(toCard schema: SchemaNode, in directory: SchemaDirectory) -> Set<String> {
        let section = directory.sectionId(of: schema)
        var out = way(to: section, in: directory)
        out.insert(Node.cards(section).key)
        if schema.kind == .value || schema.kind == .enum { out.insert(Node.values(section).key) }
        // A card inside a container: the containers above it, open.
        var at = schema.container
        while let up = at, out.insert(Node.card(up).key).inserted { at = directory.schema(id: up)?.container }
        return out
    }

    // MARK: - How a row says what it is

    /// What the group of the other sections' cards a section's lines reach is called: what
    /// they are on its diagram (overview §8).
    /// Named as the app's own section is (`_evacuation_`): what the nav panel
    /// unfolds there is, in a way, a section.
    static let autoTitle = "_section_ghosts_"
    static var autoHelp: String { L10n[.Nav.treeAutoHelp] }
    /// The mark a card ghost carries in the tree: the shortcut's arrow.
    static let ghostSymbol = "arrow.turn.up.right"

    /// A card ghost's line in the tree: its name and what it points at.
    static func ghostLine(_ ghost: CardGhost, in directory: SchemaDirectory) -> String {
        "\(ghost.name) → \(directory.shownTarget(of: ghost))"
    }

    // MARK: - The rows above the branches

    /// A row of those above the branches.
    enum Head: Hashable { case settings, board, inspection }

    /// The one head row lit: the row of what the main panel shows. The project's settings and the
    /// inspection are pages of the project, whatever tree is open, so while one is on screen the board's
    /// row is not lit — two rows lit would read as two picked, and the tree open is still told by its
    /// root row below. Anything of the board on screen (its dashboard, a section, a card, a card's
    /// connections) lights its row; main's and a branch's light none.
    static func litHead(here: AppModel.Place, boardOpen: Bool) -> Head? {
        switch here {
        case .settings: .settings
        case .inspection: .inspection
        case .section, .card, .root, .connections: boardOpen ? .board : nil
        }
    }

    // MARK: - Pressing a row (overview §8)

    /// What pressing a row does.
    enum Press: Equatable {
        /// Opened or shut in the tree.
        case toggle
        /// A section opened on the right — a root on its dashboard — with nothing picked.
        case openSection(String, root: Bool)
        /// A card shown where it is drawn: `section`'s relation panel, the card picked, the
        /// view brought to `focus` — the card, or the section ghost it is folded into — and
        /// `row` of it marked.
        case show(card: String, section: String, focus: String, row: String?)
        /// A card's editor.
        case openCard(String)
        /// A flow's card editor, its node picked.
        case openNode(flow: String, node: String)
        /// A file's text, scrolled to its function or type and its lines tinted, the symbol picked.
        case openSymbol(file: String, symbol: String)
        /// What the open branch removed, picked: shown read only in the property panel.
        case pickGone(String)
        case nothing
    }

    /// Once, a section opens on the right and a card is shown on the relation panel it is
    /// drawn on — a card ghost's on the section it hangs from; twice, a section opens or
    /// shuts in the tree and a card opens in its editor. The rows that only hold others open and
    /// shut at once. Twice is told after once: what twice does comes on top of what once did.
    /// Whether a press is the second of a double-click: the event's click count, as the diagram
    /// reads it (one press with no event, as a test's).
    static func twice(_ event: NSEvent?) -> Bool {
        (event?.clickCount ?? 1) >= 2
    }

    static func press(_ node: Node, twice: Bool, in detail: ProjectDetail) -> Press {
        let directory = detail.directory
        switch node {
        case .section(let id):
            return twice ? .toggle : .openSection(id, root: directory.section(id: id)?.parentId == nil)
        case .auto, .autoSection, .cards, .values:
            return twice ? .nothing : .toggle
        case .card(let id):
            guard let card = directory.schema(id: id) else { return .nothing }
            if twice { return .openCard(id) }
            // A card inside a container is drawn as its container's row: that card, that row —
            // a goal's rows being the cards right inside it, a card deeper in is the row of the
            // goal inside that holds it.
            if card.container != nil {
                var top = card
                var row = card.name
                var seen: Set<String> = [card.id]
                while let up = directory.container(of: top), seen.insert(up.id).inserted {
                    row = top.name
                    top = up
                }
                return .show(card: top.id, section: directory.sectionId(of: top), focus: top.id, row: row)
            }
            return .show(card: id, section: directory.sectionId(of: card), focus: id, row: nil)
        case .node(let id):
            // Drawn as its flow's row: once, that row on the relation panel; twice, the flow's
            // card map with the node picked.
            guard let node = detail.inner.first(where: { $0.id == id }), let flow = directory.schema(id: node.container) else {
                return .nothing
            }
            return twice ? .openNode(flow: flow.id, node: node.name)
                : .show(card: flow.id, section: directory.sectionId(of: flow), focus: flow.id, row: SchemaNode.nodeRow(node.name))
        case .ghost(let id):
            // Once, picked on its section's diagram; twice, the card it leads to in its editor.
            guard let ghost = directory.ghost(id: id) else { return .nothing }
            if twice { return directory.reached(ghost).map { .openCard($0.id) } ?? .nothing }
            return .show(card: id, section: directory.sectionId(of: ghost), focus: id, row: nil)
        case .autoCard(let at, let id):
            // Folded into its section's ghost there: that card, and its row in it.
            guard let card = directory.schema(id: id) else { return .nothing }
            return twice ? .openCard(id)
                : .show(card: id, section: at, focus: SectionGhosts.prefix + directory.sectionId(of: card), row: "§f|\(id)|")
        case .gone(let id):
            return twice ? .nothing : .pickGone(id)
        case .goneSection(let id):
            return twice ? .toggle : .pickGone(id)
        case .symbol(let file, let path):
            // Once, its row on its file's card; twice, the file's text at it.
            guard let card = directory.schema(id: file) else { return .nothing }
            return twice ? .openSymbol(file: file, symbol: path)
                : .show(card: file, section: directory.sectionId(of: card), focus: file, row: path)
        case .none:
            return .nothing
        }
    }

    // MARK: - Dragging a row (overview §8)

    /// Where on a row a drag is let go: above it, on it, below it.
    enum Spot: Equatable { case before, into, after }

    /// What a drop does.
    enum Drop: Equatable {
        /// A section into `parent` — its own parent, to put it elsewhere among its siblings —
        /// at `position` among the sections there, counting from zero, nil for the end:
        /// `section_update`'s `parent` and `position`.
        case section(String, parent: String, position: Int?)
        /// A card into another section, as 「セクションへ移す」 does (`card_update`'s `section`).
        case cardInto(String, section: String)
        /// A card at `position` among its section's cards, counting from zero
        /// (`card_update`'s `position`).
        case cardAt(String, position: Int)
        /// A card ghost made in `section` of a card — or of another ghost — dropped there
        /// with ⌥⌘ held, as Finder makes an alias (`card_create` of the kind `ghost`).
        case ghostOf(String, section: String)
        /// A card ghost moved into another section (`card_update`'s `section`).
        case ghostInto(String, section: String)
        /// A card ghost at `position` among its section's card ghosts (`card_update`'s
        /// `position`).
        case ghostAt(String, position: Int)
    }

    /// What may be changed from here.
    struct Allowed: Equatable {
        /// Sections moved, cards moved into another section: not where the project is read only
        /// — archived, held by an MCP batch, a locked main (`AppModel.isReadOnly`).
        var move: Bool
        /// Cards put in order among their section's: a locked main takes it too
        /// (`specboard://SpecBoard/Docs/Design:DesignBranches` §8) — not an archived project, nor a branch an MCP batch holds.
        var order: Bool
    }

    /// Whether a row may be picked up at all: a section that may move, a card or a card
    /// ghost while it may move or be put in order. Nothing else — a row of a section ghost
    /// stands for a card kept elsewhere.
    static func canDrag(_ node: Node, in detail: ProjectDetail, allowed: Allowed) -> Bool {
        switch node {
        case .section(let id): allowed.move && movable(id, in: detail)
        // A card inside a container is moved with the container (`in` is written elsewhere).
        case .card(let id): detail.directory.schema(id: id).map { $0.container == nil } == true && (allowed.move || allowed.order)
        case .ghost(let id): detail.directory.ghost(id: id) != nil && (allowed.move || allowed.order)
        default: false
        }
    }

    /// Whether ⌥⌘ is held now: a card dropped on a section then leaves a card ghost of it
    /// there, as an alias is made in Finder.
    static var aliasHeld: Bool { NSEvent.modifierFlags.isSuperset(of: [.option, .command]) }

    /// Which part of a row a drop at `y` (of its `height`) is on. A section dragged over a
    /// section: its top and bottom quarters are between it and its neighbours, the rest is
    /// into it. A card over a section: into it, all of it. Over a card: its halves, before
    /// and after it.
    static func spot(_ y: CGFloat, height: CGFloat, dragging: Node, over target: Node) -> Spot {
        switch (dragging, target) {
        case (.section, .section):
            return y < height * 0.25 ? .before : y > height * 0.75 ? .after : .into
        case (_, .section):
            return .into
        default:
            return y < height / 2 ? .before : .after
        }
    }

    /// What dropping `dragged` on `target` at `spot` does, as `section_update` and
    /// `card_update` would take it — nil where it would be refused or change nothing,
    /// where the row then shows no mark of taking it. `alias`: ⌥⌘ held — a card, or a card
    /// ghost, dropped on a section leaves a card ghost of it there (`card_create`).
    static func drop(_ dragged: Node, on target: Node, at spot: Spot, in detail: ProjectDetail,
                     allowed: Allowed, board: Bool, alias: Bool = false) -> Drop? {
        if alias {
            guard allowed.move, case .section(let section) = target else { return nil }
            switch dragged {
            case .card(let id), .ghost(let id):
                return canHoldGhost(of: id, in: section, detail: detail) ? .ghostOf(id, section: section) : nil
            default:
                return nil
            }
        }
        switch (dragged, target) {
        case (.section(let id), .section(let other)):
            guard allowed.move else { return nil }
            return sectionDrop(id, on: other, at: spot, in: detail, board: board)
        case (.card(let id), .section(let section)):
            // 「セクションへ移す」: the sections of a kind that holds it.
            guard allowed.move, let card = detail.directory.schema(id: id),
                  detail.homes(for: card).contains(where: { $0.id == section }) else { return nil }
            return .cardInto(id, section: section)
        case (.card(let id), .card(let other)):
            guard allowed.order, spot != .into else { return nil }
            return cardDrop(id, beside: other, after: spot == .after, in: detail)
        case (.ghost(let id), .section(let section)):
            // Moved, as a card is: where it may stand and what it points at is not.
            let directory = detail.directory
            guard allowed.move, let ghost = directory.ghost(id: id), directory.sectionId(of: ghost) != section,
                  canHoldGhost(of: ghost.target ?? "", in: section, detail: detail) else { return nil }
            return .ghostInto(id, section: section)
        case (.ghost(let id), .ghost(let other)):
            guard allowed.order, spot != .into else { return nil }
            return ghostDrop(id, beside: other, after: spot == .after, in: detail)
        default:
            return nil
        }
    }

    /// Whether a card ghost of `id` — a card or another ghost — may be put in `section`:
    /// a section under the root, not the one `id` is in
    /// (there it is written by its name already, `specboard://SpecBoard/Docs/Design:DesignSections` §2).
    static func canHoldGhost(of id: String, in section: String, detail: ProjectDetail) -> Bool {
        let directory = detail.directory
        guard let place = directory.section(id: section), place.parentId != nil,
              place.kind?.isMainOnly != true else { return false }
        if let card = directory.schema(id: id) { return directory.sectionId(of: card) != section }
        if let ghost = directory.ghost(id: id) { return directory.sectionId(of: ghost) != section }
        return false
    }

    /// A card ghost put before or after another of its section's: its place among them.
    private static func ghostDrop(_ id: String, beside other: String, after: Bool, in detail: ProjectDetail) -> Drop? {
        let directory = detail.directory
        guard id != other, let ghost = directory.ghost(id: id), let beside = directory.ghost(id: other),
              directory.sectionId(of: ghost) == directory.sectionId(of: beside) else { return nil }
        let members = directory.ghosts(in: directory.sectionId(of: ghost)).map(\.id)
        let others = members.filter { $0 != id }
        guard let at = others.firstIndex(of: other) else { return nil }
        let position = at + (after ? 1 : 0)
        guard members.firstIndex(of: id) != position else { return nil }
        return .ghostAt(id, position: position)
    }

    /// Whether a section's menu offers to delete it: not a root, nor what is directly under a root —
    /// the project's, renamed here and taken away with `project_tree` (the tab for kinds) — but
    /// `_evacuation_`; nor one the core keeps in its place (the board's Complete and Cancelled).
    static func deletable(_ section: ProjectSection, in detail: ProjectDetail) -> Bool {
        guard section.parentId != nil, section.kind?.isFixed != true else { return false }
        let root = detail.sections.first { $0.parentId == nil }?.id
        return !ProjectSection.isRootOwn(section, root: root) || section.name == ProjectSection.evacuation
    }

    /// Whether a section may move: not a root, nor one directly under a root — the project's
    /// (`project_tree`; renamed, never moved).
    static func movable(_ id: String, in detail: ProjectDetail) -> Bool {
        let directory = detail.directory
        guard let section = directory.section(id: id), let parent = section.parentId,
              let above = directory.section(id: parent) else { return false }
        return above.parentId != nil
    }

    private static func sectionDrop(_ id: String, on targetId: String, at spot: Spot, in detail: ProjectDetail,
                                    board: Bool) -> Drop? {
        let directory = detail.directory
        guard targetId != id, movable(id, in: detail), let moving = directory.section(id: id),
              let target = directory.section(id: targetId) else { return nil }
        let parent: String
        var position: Int?
        switch spot {
        case .into:
            parent = targetId
        case .before, .after:
            // Beside a root: there is nowhere beside it.
            guard let above = target.parentId else { return nil }
            parent = above
            let siblings = directory.children(of: above).map(\.id).filter { $0 != id }
            guard let at = siblings.firstIndex(of: targetId) else { return nil }
            position = at + (spot == .after ? 1 : 0)
        }
        guard let into = directory.section(id: parent) else { return nil }
        let underRoot = into.parentId == nil
        if parent != moving.parentId {
            // Not directly under a root, not into itself or anything inside it; and one the core
            // keeps in its parent, only in its order there.
            guard !underRoot, moving.kind?.isFixed != true else { return nil }
            var at: String? = parent
            while let here = at {
                if here == id { return nil }
                at = directory.section(id: here)?.parentId
            }
            // A name once among its siblings, whatever the case.
            guard !directory.children(of: parent).contains(where: {
                $0.id != id && $0.name.lowercased() == moving.name.lowercased()
            }) else { return nil }
            // It takes its new parent's kind, and so must what it holds.
            if let kind = into.kind, kind != moving.kind, !holds(id, as: kind, in: detail) { return nil }
        } else {
            // Into its own parent changes nothing; nor does its own place. main's and a
            // branch's top keeps its own order.
            guard let position, !underRoot || board,
                  directory.children(of: parent).map(\.id).firstIndex(of: id) != position else { return nil }
        }
        return .section(id, parent: parent, position: position)
    }

    /// Whether every card in the section and the sections inside it may be in a section of `kind`.
    private static func holds(_ id: String, as kind: SectionKind, in detail: ProjectDetail) -> Bool {
        let directory = detail.directory
        var inside: Set<String> = [id]
        var queue = [id]
        while let at = queue.popLast() {
            for child in directory.children(of: at) where inside.insert(child.id).inserted { queue.append(child.id) }
        }
        return detail.schemas.allSatisfy { !inside.contains(directory.sectionId(of: $0)) || kind.holds.contains($0.kind) }
    }

    /// A card put before or after another of its section's, in the same group — the value
    /// objects and enums are listed apart from the rest: its place among all the section's.
    private static func cardDrop(_ id: String, beside other: String, after: Bool, in detail: ProjectDetail) -> Drop? {
        let directory = detail.directory
        guard id != other, let card = directory.schema(id: id), let beside = directory.schema(id: other) else { return nil }
        let section = directory.sectionId(of: card)
        func isType(_ card: SchemaNode) -> Bool { card.kind == .value || card.kind == .enum }
        guard directory.sectionId(of: beside) == section, isType(card) == isType(beside) else { return nil }
        let members = directory.members(section).map(\.id)
        let others = members.filter { $0 != id }
        guard let at = others.firstIndex(of: other) else { return nil }
        let position = at + (after ? 1 : 0)
        guard members.firstIndex(of: id) != position else { return nil }
        return .cardAt(id, position: position)
    }
}

/// The nav panel's tree, on screen (its rows are `NavPanelTree`'s).
struct NavPanelTreeView: View {
    var model: AppModel
    var drawn: Set<String>
    var onProjects: () -> Void

    @State private var expanded: Set<String> = []
    /// The row being dragged (`NavPanelTree.drop`), from when it is picked up to its drop.
    @State private var dragging: NavPanelTree.Node?
    /// The row a drag over it would be taken by, and where on it: drawn there. None where
    /// it would not be (overview §8).
    @State private var dropMark: DropMark?
    /// The row just pressed, lit as the row on screen until what the press does has run
    /// (`NavRow.pressing`).
    @State private var pressed: String?
    /// How tall each row is, for which part of it a drag is over.
    @State private var heights = RowHeights()
    @State private var loadedFor: String?
    /// The project the tree has settled on. Until it has, what arrives with the switch —
    /// the section and card the project reopens on — does not open the way to itself.
    @State private var settledOn: String?
    @State private var filter = NavPanelTree.Filter()
    /// The search box written in, and the pointer on what it offers (`suggestions`).
    @FocusState private var searching: Bool
    @State private var overSuggestions = false

    /// A section or a branch being made or renamed from here.
    private enum Prompt: Equatable {
        /// Inside a section, of its kind.
        case create(parent: String?, kind: SectionKind?)
        case branch
        case rename(String)
        case renameBranch(String)

        /// Whether the name asked for is a section's, held to the rule of a path's names.
        var namesSection: Bool {
            switch self {
            case .create, .rename: true
            case .branch, .renameBranch: false
            }
        }
    }
    /// Its name being asked for (`NameSheet`).
    @State private var asking: NameAsk?

    /// A branch about to be let go, with what it changed.
    @State private var deleting: BranchInfo?
    /// A card about to be removed, asked first (the right-click is where a card goes).
    @State private var deletingCard: String?
    /// A section whose export settings are open.
    /// A card ghost being pointed elsewhere (`GhostRetargetSheet`).
    @State private var retargeting: String?

    var body: some View {
        if let detail = model.detail {
            // The files whose rows are open: their functions and types, read as they open.
            let _ = model.askOutlines(NavPanelTree.openFiles(expanded))
            let outlines = model.outlines
            let rows = NavPanelTree.rows(detail, expanded: expanded, filter: filter, gone: model.shownMarks.gone,
                                         outlines: outlines)
            let marks = NavPanelTree.Marks(detail, model.shownMarks)
            let guides = NavPanelTree.guides(rows)
            VStack(spacing: 0) {
                header(detail)
                    .confirmationDialog(
                        L10n[.Nav.alertDeleteCard(deletingCard.flatMap { model.schema($0)?.name } ?? "")],
                        isPresented: Binding(get: { deletingCard != nil }, set: { if !$0 { deletingCard = nil } }),
                        titleVisibility: .visible
                    ) {
                        Button(L10n[.Common.actionDeleteItem], role: .destructive) {
                            if let id = deletingCard { model.deleteSchema(id) }
                            deletingCard = nil
                        }
                        Button(L10n[.Common.actionCancel], role: .cancel) { deletingCard = nil }
                    } message: {
                        Text(CardDeletion.message)
                    }
                Divider()
                settingsRow
                Divider()
                boardRow
                inspectionRow
                Divider()
                BranchList(model: model,
                           onCreate: { ask(.branch) },
                           onRename: { ask(.renameBranch($0.id), $0.name) },
                           onDelete: { deleting = $0 })
                Divider()
                controls(detail)
                Divider()
                ScrollViewReader { scroll in
                    List(rows) { row in
                        self.row(row, detail, marks: marks, guides: guides[row.id] ?? [], outlines: outlines)
                            .id(row.id)
                            // The gap around the row is the row's own (`rowGap`), to be pressed.
                            .listRowInsets(EdgeInsets())
                            .listRowSeparator(.hidden)
                    }
                    .listStyle(.plain)
                    .scrollContentBackground(.hidden)
                    .sheet(isPresented: Binding(get: { retargeting != nil }, set: { if !$0 { retargeting = nil } })) {
                        if let id = retargeting, let ghost = model.ghost(id) {
                            GhostRetargetSheet(model: model, ghost: ghost) { retargeting = nil }
                        }
                    }
                    // Picked on the diagram or by an agent: open the way to it — its
                    // section's cards too — and bring it into view, so the two never
                    // disagree about where you are.
                    .onChange(of: model.selectedSchemaId) { _, id in
                        guard settledOn == detail.project.id, let id else { return }
                        let node: NavPanelTree.Node
                        if let schema = model.schema(id) {
                            expanded.formUnion(NavPanelTree.way(toCard: schema, in: detail.directory))
                            node = .card(id)
                        } else if let ghost = model.ghost(id) {
                            // A card ghost, among its section's cards.
                            let section = detail.directory.sectionId(of: ghost)
                            expanded.formUnion(NavPanelTree.way(to: section, in: detail.directory))
                            expanded.insert(NavPanelTree.Node.cards(section).key)
                            node = .ghost(id)
                        } else {
                            return
                        }
                        withAnimation(.easeOut(duration: 0.15)) {
                            scroll.scrollTo(node.key, anchor: .center)
                        }
                    }
                }
            }
            .onAppear { load(detail) }
            // Another project: its tree starts folded, the way a folder does.
            .onChange(of: detail.project.id) { _, _ in load(detail, folded: true) }
            .onChange(of: model.shownSectionKey) { _, key in
                guard settledOn == detail.project.id else { return }
                // The way down to it, so its row is in sight — not the section itself,
                // which opens and shuts by its chevron or a double click on its name.
                var way = NavPanelTree.way(to: key, in: detail.directory)
                way.remove(NavPanelTree.Node.section(key).key)
                expanded.formUnion(way)
            }
            .onChange(of: expanded) { _, now in save(detail.project.id, now) }
            .nameSheet($asking)
            .confirmationDialog(
                L10n[.Nav.alertDeleteBranch(deleting?.name ?? "")],
                isPresented: Binding(get: { deleting != nil }, set: { if !$0 { deleting = nil } }),
                titleVisibility: .visible, presenting: deleting
            ) { branch in
                Button(L10n[.Common.actionDelete], role: .destructive) {
                    model.deleteBranch(branch.id)
                    deleting = nil
                }
                Button(L10n[.Common.actionCancel], role: .cancel) { deleting = nil }
            } message: { branch in
                let count = model.branchChangeCount(branch.id)
                Text(count == 0 ? L10n[.Nav.alertBranchUnchanged] : L10n[.Nav.alertBranchChanges(count)])
            }
        }
    }

    // MARK: - Above the tree

    private func header(_ detail: ProjectDetail) -> some View {
        HStack(spacing: 6) {
            Button(action: onProjects) {
                HStack(spacing: 3) {
                    Image(systemName: "chevron.left").font(.app(10, weight: .semibold))
                    Text(L10n[.Nav.headerProjects]).font(.app(11.5))
                }
            }
            .buttonStyle(.borderless)
            .help(L10n[.Nav.headerProjectsHelp])
            Text(detail.project.name)
                .font(.app(12, weight: .semibold))
                .lineLimit(1)
                .frame(maxWidth: .infinity, alignment: .trailing)
            Button {
                expanded = []
            } label: {
                Image(systemName: "rectangle.compress.vertical").font(.app(11))
            }
            .buttonStyle(.borderless)
            .disabled(expanded.isEmpty)
            .help(L10n[.Common.actionCollapseAll])
            Menu {
                if let here = model.shownSection, here.parentId != nil || model.isBoardOpen {
                    createMenu(in: here, title: L10n[.Nav.menuCreateInside(here.name)])
                } else {
                    Text(L10n[.Nav.menuOpenToCreate])
                }
            } label: {
                Image(systemName: "plus").font(.app(11))
            }
            .menuStyle(.borderlessButton)
            .fixedSize()
            .help(L10n[.Nav.headerCreateHelp])
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 7)
    }

    private func controls(_ detail: ProjectDetail) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 6) {
                Image(systemName: "magnifyingglass").font(.app(10)).foregroundStyle(.secondary)
                TextField(L10n[.Nav.searchPlaceholder], text: $filter.query)
                    .textFieldStyle(.plain).font(.app(12))
                    .focused($searching)
                    .onKeyPress(.escape) {
                        searching = false
                        return .handled
                    }
                if !filter.query.isEmpty {
                    Button { filter.query = "" } label: { Image(systemName: "xmark.circle.fill") }
                        .buttonStyle(.plain).foregroundStyle(.secondary).font(.app(10))
                }
            }
            // While the box is written in — or the pointer is on what it offers, so a press on
            // one is not lost as the box lets go of the keyboard — what may be put in it.
            if searching || overSuggestions {
                suggestions(detail)
                    .onHover { overSuggestions = $0 }
            }
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 7)
    }

    /// The kinds and tags the box offers, each a press away from being put in.
    @ViewBuilder
    private func suggestions(_ detail: ProjectDetail) -> some View {
        let offered = NavPanelTree.suggestions(detail, query: filter.query)
        VStack(alignment: .leading, spacing: 5) {
            if !offered.kinds.isEmpty {
                Text(L10n[.Common.wordKind]).font(.app(9.5, weight: .semibold)).foregroundStyle(.secondary)
                FlowRow(spacing: 5, lineSpacing: 4) {
                    ForEach(offered.kinds) { chip($0) }
                }
            }
            if !offered.tags.isEmpty {
                Text(L10n[.Common.wordTags]).font(.app(9.5, weight: .semibold)).foregroundStyle(.secondary)
                FlowRow(spacing: 5, lineSpacing: 4) {
                    ForEach(offered.tags.prefix(40)) { chip($0) }
                }
            }
            if offered.kinds.isEmpty && offered.tags.isEmpty {
                Text(L10n[.Nav.searchNoKindsOrTags]).font(.app(10)).foregroundStyle(.tertiary)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func chip(_ offered: NavPanelTree.Suggestion) -> some View {
        Button {
            filter.query = NavPanelTree.Filter.putting(offered.token, into: filter.query)
            searching = true
        } label: {
            Text("\(offered.token) \(offered.count)")
                .font(.app(10))
                .padding(.horizontal, 6).padding(.vertical, 1.5)
                .overlay(RoundedRectangle(cornerRadius: 5)
                    .stroke(Color.secondary.opacity(0.25), lineWidth: 1))
        }
        .buttonStyle(.plain)
        .help(offered.help)
    }

    // MARK: - Rows

    @ViewBuilder
    private func row(_ row: NavPanelTree.Row, _ detail: ProjectDetail, marks: NavPanelTree.Marks,
                     guides: [NavPanelTree.Guide], outlines: [String: FileOutline]) -> some View {
        let opens = row.node.opens(in: detail.directory, gone: model.shownMarks.gone, outlines: outlines)
        let mark = marks.of(row.node)
        HStack(spacing: 0) {
            chevron(row.node, depth: row.depth, opens: opens)
            label(row.node, detail, mark: mark)
        }
        // What the row is called and what its mark means, a pause over it away.
        .help(ifAny: rowHelp(row.node, detail, mark: mark))
        .padding(.vertical, 2)
        .padding(.horizontal, 3)
        // From the parent's chevron down to this row: which rows belong to which.
        .background(alignment: .leading) {
            GuideLines(guides: guides, indent: Self.indent, reach: opens ? 8 : 18)
        }
        .overlay { dropMarkView(row) }
        .modifier(NavRowSlot(key: row.id, current: isCurrent(row.node), pressed: pressed,
                             dropping: dropMark?.key == row.id, gap: Self.rowGap))
        // Twice — a double-click, as the window counts it (the event's click count, as the
        // diagram reads it): a section or a root opens or shuts in the tree, a card opens in its
        // editor (once has already opened the section, or shown the card on its relation panel).
        // The second tap gesture this had beside it opened nothing on the window.
        .onTapGesture { press(row, detail) }
        .contextMenu { menu(row.node, detail) }
        .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { heights.of[row.id] = $0 }
        .modifier(DragSource(enabled: NavPanelTree.canDrag(row.node, in: detail, allowed: allowed)) {
            dragging = row.node
            return NSItemProvider(object: Self.token(row.node) as NSString)
        })
        .onDrop(of: [.text], delegate: NavPanelDrop(
            target: row.node,
            height: { heights.of[row.id] ?? 20 },
            dragged: { dragging },
            decide: { dragged, spot in
                // With ⌥⌘ held, a card ghost of it is left there, as an alias is in Finder.
                NavPanelTree.drop(dragged, on: row.node, at: spot, in: detail, allowed: allowed,
                                  board: model.isBoardOpen, alias: NavPanelTree.aliasHeld)
            },
            mark: { spot in
                if let spot { dropMark = DropMark(key: row.id, spot: spot) } else if dropMark?.key == row.id { dropMark = nil }
            },
            perform: { dragged, drop, providers in
                dragging = nil
                dropMark = nil
                // Only the drag that started here: a stale one — let go outside the panel —
                // must not move a row when text from elsewhere is dropped on it.
                guard let provider = providers.first else { return }
                let expected = Self.token(dragged)
                _ = provider.loadObject(ofClass: NSString.self) { object, _ in
                    guard (object as? String) == expected else { return }
                    DispatchQueue.main.async { model.perform(drop) }
                }
            }))
    }

    /// What may be changed from the tree now (`NavPanelTree.Allowed`).
    private var allowed: NavPanelTree.Allowed { NavPanelTree.Allowed(model) }

    /// What a dragged row carries: its key, marked as the tree's own.
    private static func token(_ node: NavPanelTree.Node) -> String { "specboard-nav-panel:\(node.key)" }

    /// Where a drag over the row would be taken: a line above or below it, or the row
    /// lit, into it.
    @ViewBuilder
    private func dropMarkView(_ row: NavPanelTree.Row) -> some View {
        if let mark = dropMark, mark.key == row.id {
            switch mark.spot {
            case .into:
                RoundedRectangle(cornerRadius: 5)
                    .fill(Color.accentColor.opacity(0.12))
                    .overlay(RoundedRectangle(cornerRadius: 5).stroke(Color.accentColor, lineWidth: 1.5))
                    .allowsHitTesting(false)
            case .before, .after:
                VStack(spacing: 0) {
                    if mark.spot == .after { Spacer(minLength: 0) }
                    Rectangle().fill(Color.accentColor).frame(height: 2)
                        .padding(.leading, CGFloat(row.depth) * Self.indent + 16)
                    if mark.spot == .before { Spacer(minLength: 0) }
                }
                .allowsHitTesting(false)
            }
        }
    }

    /// How far each depth steps in.
    private static let indent: CGFloat = 12

    /// The gap around a row, part of its slot (`NavRowSlot`): a point to the rows above and below,
    /// and the list's sides.
    static let rowGap = EdgeInsets(top: 1, leading: 6, bottom: 1, trailing: 8)

    /// The indent, the chevron and the gap after it. Where there is a chevron all of
    /// it is the chevron's to click — the arrow alone is too small a thing to hit —
    /// drawn and spaced just as before, so the guide lines still meet it.
    @ViewBuilder
    private func chevron(_ node: NavPanelTree.Node, depth: Int, opens: Bool) -> some View {
        let lead = CGFloat(depth) * Self.indent
        if opens {
            let open = expanded.contains(node.key)
            Button { toggle(node) } label: {
                HStack(spacing: 0) {
                    Color.clear.frame(width: lead + 4, height: 1)
                    Image(systemName: open ? "chevron.down" : "chevron.right")
                        .font(.app(8.5, weight: .semibold))
                        .foregroundStyle(.secondary)
                        .frame(width: 12, height: 14)
                    Color.clear.frame(width: 4, height: 1)
                }
                .frame(maxHeight: .infinity)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .help(open ? L10n[.Nav.rowCollapse] : L10n[.Nav.rowExpand])
        } else {
            Color.clear.frame(width: lead + 4 + 12 + 4, height: 1)
        }
    }

    @ViewBuilder
    private func label(_ node: NavPanelTree.Node, _ detail: ProjectDetail, mark: ChangeMark?) -> some View {
        let directory = detail.directory
        switch node {
        case .section(let id):
            if let section = directory.section(id: id) {
                if section.parentId == nil {
                    // The tree open, plainly, and what kind of tree it is: a
                    // branch's name with a tag saying so, main as itself — with a lock while it is
                    // locked — the board as itself. The root's own name is not said here: named
                    // Main by default, it read as main with a branch open.
                    Text(model.isBoardOpen ? L10n[.Common.wordProjectBoard] : model.openTree.name)
                        .font(.app(12, weight: .semibold)).lineLimit(1)
                        .help(model.isBoardOpen ? L10n[.Nav.treeBoardOpen]
                              : model.openBranch == nil ? L10n[.Nav.treeMainOpen]
                              : L10n[.Nav.treeBranchOpen(model.openBranch?.name ?? "")])
                    if model.openBranch != nil, !model.isBoardOpen {
                        tag("branch")
                        // What main did since the branch was made: what taking main in brings.
                        let moved = model.branchMarks.mainMoved
                        if moved > 0 {
                            Text("↓ \(moved)")
                                .font(.app(10, weight: .semibold, design: .monospaced))
                                .foregroundStyle(ChangeMark.Tint.blue.color)
                                .padding(.leading, 5)
                                .help(L10n[.Nav.treeMainMovedHelp(moved)])
                        }
                    } else if model.isLockedMainOpen || model.isLockedGroupOpen {
                        Image(systemName: "lock.fill").font(.app(9.5)).foregroundStyle(.secondary)
                            .padding(.leading, 4)
                            .help(L10n[.Branches.listMainLockedHelp])
                    }
                } else if section.missing {
                    // A folder a note is in that reading the folder no longer finds: a stand-in.
                    sectionMark(section, detail)
                    Text(section.name)
                        .font(.app(12, weight: id == model.shownSectionKey ? .semibold : .regular))
                        .foregroundStyle(.orange).strikethrough(true, color: .orange)
                        .lineLimit(1)
                        .help(L10n[.Nav.treeMissingFolderHelp(section.description)])
                } else {
                    sectionMark(section, detail)
                    Text(section.name)
                        .font(.app(12, weight: id == model.shownSectionKey ? .semibold : .regular))
                        .foregroundStyle(nameStyle(mark, else: HierarchicalShapeStyle.primary))
                        .lineLimit(1)
                    // A working directory: read from a folder of this Mac, not written here.
                    if section.kindName == WorkDirIds.workdirKind {
                        Image(systemName: "lock.fill").font(.app(8)).foregroundStyle(.tertiary)
                            .help(L10n[.Nav.treeWorkDirHelp])
                    }
                }
                Spacer(minLength: 4)
                count(directory.members(id).count)
                markColumn(mark)
            }
        case .goneSection(let id):
            if let section = model.branchMarks.gone.sections.first(where: { $0.id == id }) {
                sectionMark(section, detail)
                goneName(section.name, font: .app(12, weight: id == model.pickedGone ? .semibold : .regular))
                Spacer(minLength: 4)
                markColumn(mark)
            }
        case .auto:
            Text(NavPanelTree.autoTitle).font(.app(11)).foregroundStyle(.secondary)
                .help(NavPanelTree.autoHelp)
            Spacer(minLength: 4)
            markColumn(nil)
        case .autoSection(let at, let other):
            Image(systemName: "rectangle.dashed").font(.app(9.5)).foregroundStyle(.tertiary)
            Text(directory.shown(section: other, within: directory.root(of: at)))
                .font(.app(11.5, design: .monospaced)).foregroundStyle(.secondary).lineLimit(1)
            Spacer(minLength: 4)
            markColumn(nil)
        case .autoCard(_, let card):
            if let schema = directory.schema(id: card) {
                Text(schema.name).font(.app(11.5, design: .monospaced)).foregroundStyle(.secondary).lineLimit(1)
                Spacer(minLength: 4)
                Text(schema.kindName).font(.app(9.5)).foregroundStyle(.tertiary)
                markColumn(nil)
            }
        case .cards(let id):
            Text("cards").font(.app(11)).foregroundStyle(.secondary)
                .help(Self.said(L10n[.Nav.treeCardsHelp], mark?.meaning))
            Spacer(minLength: 4)
            count(directory.members(id).count)
            markColumn(mark)
        case .ghost(let id):
            if let ghost = directory.ghost(id: id) {
                let card = directory.reached(ghost)
                Image(systemName: NavPanelTree.ghostSymbol).font(.app(9.5))
                    .foregroundStyle(card == nil ? Color.red : Color.secondary)
                    .help(L10n[.Nav.treeGhostHelp])
                Text(NavPanelTree.ghostLine(ghost, in: directory))
                    .font(.app(12, weight: id == model.selectedSchemaId ? .semibold : .regular, design: .monospaced))
                    .foregroundStyle(nameStyle(mark, else: card == nil ? HierarchicalShapeStyle.secondary : .primary))
                    .lineLimit(1).truncationMode(.middle)
                    .help(Self.said(directory.ghostProblem(ghost) ?? "\(directory.full(ghost)) → \(card.map(directory.full) ?? "")",
                                    mark?.meaning))
                Spacer(minLength: 4)
                Text(card?.kindName ?? "ghost").font(.app(9.5)).foregroundStyle(.tertiary)
                markColumn(mark)
            }
        case .card(let id):
            if let schema = directory.schema(id: id) {
                let hidden = directory.sectionId(of: schema) == model.shownSectionKey && !drawn.contains(id)
                Text(schema.name)
                    .font(.app(12, weight: id == model.selectedSchemaId ? .semibold : .regular, design: .monospaced))
                    .foregroundStyle(nameStyle(mark, else: HierarchicalShapeStyle.primary))
                    .lineLimit(1)
                // Its label is in the tooltip; on the row only while the search box finds it by its label.
                if filter.isOn, filter.byLabel(schema) {
                    Text(schema.label).font(.app(10.5)).foregroundStyle(.secondary).lineLimit(1)
                }
                // A file an export writes: which, and how it stands.
                if let mark = model.exportMarks[id] {
                    ExportMarkView(mark: mark)
                }
                Spacer(minLength: 4)
                if hidden {
                    Image(systemName: "eye.slash").font(.app(9)).foregroundStyle(.tertiary)
                        .help(L10n[.Nav.treeHiddenOnPanel])
                }
                Text(schema.kindName)
                    .font(.app(9.5)).foregroundStyle(.tertiary)
                markColumn(mark)
            }
        case .node(let id):
            if let node = detail.inner.first(where: { $0.id == id }) {
                Text(node.name).font(.app(12, design: .monospaced))
                    .foregroundStyle(nameStyle(mark, else: HierarchicalShapeStyle.primary))
                    .lineLimit(1)
                if filter.isOn, filter.byLabel(node) {
                    Text(node.label).font(.app(10.5)).foregroundStyle(.secondary).lineLimit(1)
                }
                Spacer(minLength: 4)
                Text(node.kind).font(.app(9.5)).foregroundStyle(.tertiary)
                markColumn(mark)
            }
        case .gone(let id):
            let gone = model.branchMarks.gone
            let weight: Font.Weight = id == model.pickedGone ? .semibold : .regular
            if let card = gone.cards.first(where: { $0.id == id }) {
                goneName(card.name, font: .app(12, weight: weight, design: .monospaced))
                Spacer(minLength: 4)
                Text(card.kindName).font(.app(9.5)).foregroundStyle(.tertiary)
            } else if let ghost = gone.ghosts.first(where: { $0.id == id }) {
                Image(systemName: NavPanelTree.ghostSymbol).font(.app(9.5)).foregroundStyle(.secondary)
                goneName(NavPanelTree.ghostLine(ghost, in: directory), font: .app(12, weight: weight, design: .monospaced))
                Spacer(minLength: 4)
                Text(directory.reached(ghost)?.kindName ?? "ghost").font(.app(9.5)).foregroundStyle(.tertiary)
            } else if let node = gone.nodes.first(where: { $0.id == id }) {
                goneName(node.name, font: .app(12, weight: weight, design: .monospaced))
                Spacer(minLength: 4)
                Text(node.kind).font(.app(9.5)).foregroundStyle(.tertiary)
            }
            markColumn(mark)
        case .values(let id):
            Text("values / enums").font(.app(11)).foregroundStyle(.secondary)
                .help(Self.said(L10n[.Nav.treeValuesHelp], mark?.meaning))
            Spacer(minLength: 4)
            count(directory.members(id).filter { $0.kind == .value || $0.kind == .enum }.count)
            markColumn(mark)
        case .none:
            Text(L10n[.Common.wordNone]).font(.app(10.5)).foregroundStyle(.tertiary)
            Spacer(minLength: 4)
            markColumn(nil)
        case .symbol(let file, let path):
            if let symbol = model.outlines[file]?.symbol(path) {
                SymbolBadgeView(symbol: symbol)
                Text(symbol.name)
                    .font(.app(12, weight: isCurrent(node) ? .semibold : .regular, design: .monospaced))
                    .lineLimit(1)
                    .padding(.leading, 5)
                Spacer(minLength: 4)
                Text("\(symbol.line)").font(.app(9.5, design: .monospaced)).foregroundStyle(.tertiary)
                markColumn(nil)
            }
        }
    }

    /// A section's kind, as its mark and colour: what the kind's definition says — a built-in kind's
    /// from the built-in definitions, written over by the project, or the project's own — else a
    /// folder in grey.
    @ViewBuilder
    private func sectionMark(_ section: ProjectSection, _ detail: ProjectDetail) -> some View {
        if let key = section.kind?.rawValue ?? section.kindName {
            let mark = detail.sectionMarks[key] ?? SectionMark()
            Image(systemName: mark.symbol).font(.app(10))
                .foregroundStyle(mark.color.flatMap(HexColor.color(hex:)) ?? .gray)
                .frame(width: 14)
                .help(section.kind?.help ?? L10n[.Nav.treeOwnKindHelp(key)])
        }
    }

    /// The name of what the open branch removed: in the removed mark's colour, struck through.
    private func goneName(_ name: String, font: Font) -> some View {
        let red = ChangeMark.Tint.red.color
        return Text(name).font(font).foregroundStyle(red).strikethrough(true, color: red).lineLimit(1)
    }

    /// How a name is drawn under its row's mark: in the mark's colour when the mark is its own, as it
    /// is otherwise else.
    private func nameStyle(_ mark: ChangeMark?, else plain: some ShapeStyle) -> AnyShapeStyle {
        guard let mark, mark.isOwn else { return AnyShapeStyle(plain) }
        return AnyShapeStyle(mark.tint.color)
    }

    /// The right-most column, as wide on every row while what a branch did is marked (`showsMarks`):
    /// the row's mark, or nothing.
    @ViewBuilder
    private func markColumn(_ mark: ChangeMark?) -> some View {
        if model.showsMarks {
            HStack(spacing: 0) {
                if let mark { ChangeMarkText(mark: mark) }
            }
            .frame(width: Self.markWidth, alignment: .center)
        }
    }

    /// How wide the column of the marks is: room for M↓.
    static let markWidth: CGFloat = 24

    /// Two things said, a line each, where there is anything.
    static func said(_ first: String?, _ second: String?) -> String {
        [first, second].compactMap { $0 }.filter { !$0.isEmpty }.joined(separator: "\n")
    }

    /// What a pause over a row says: what it is called (its label) and what its mark means.
    func rowHelp(_ node: NavPanelTree.Node, _ detail: ProjectDetail, mark: ChangeMark?) -> String? {
        let label: String?
        switch node {
        case .card(let id): label = detail.directory.schema(id: id)?.label
        case .node(let id): label = detail.inner.first { $0.id == id }?.label
        case .symbol(let file, let path):
            label = model.outlines[file]?.symbol(path).map { SymbolBadgeView.help($0, language: model.outlines[file]?.language) }
        case .gone(let id):
            let gone = model.branchMarks.gone
            label = gone.cards.first { $0.id == id }?.label ?? gone.nodes.first { $0.id == id }?.label
        default: label = nil
        }
        let text = Self.said(label, mark?.meaning)
        return text.isEmpty ? nil : text
    }

    private func tag(_ text: String) -> some View {
        Text(text)
            .font(.app(9))
            .foregroundStyle(.secondary)
            .padding(.horizontal, 4).padding(.vertical, 1)
            .overlay(RoundedRectangle(cornerRadius: 3).stroke(.tertiary))
    }

    private func count(_ n: Int) -> some View {
        Text("\(n)").font(.app(10, design: .monospaced)).foregroundStyle(.tertiary)
    }

    private func isCurrent(_ node: NavPanelTree.Node) -> Bool {
        switch node {
        case .section(let id):
            switch model.here {
            case .section(let open), .root(let open, _): id == open && model.selectedSchemaId == nil
            case .card, .settings, .connections, .inspection: false
            }
        case .card(let id), .ghost(let id): id == model.selectedSchemaId
        case .gone(let id), .goneSection(let id): id == model.pickedGone
        case .symbol(let file, let path): model.selectedSymbol.map { $0.file.id == file && $0.path == path } ?? false
        default: false
        }
    }

    // MARK: - Acting

    private func toggle(_ node: NavPanelTree.Node) {
        if expanded.contains(node.key) { expanded.remove(node.key) } else { expanded.insert(node.key) }
    }

    /// What a press on a row does (`NavPanelTree.press`): opening and shutting is the
    /// tree's own, the rest the window's.
    private func act(_ node: NavPanelTree.Node, _ press: NavPanelTree.Press) {
        if press == .toggle { toggle(node) } else { model.perform(press) }
    }

    /// A row pressed: lit at once, and what it does run once the light is on screen
    /// (`NavRow.pressing`). The light goes as the press is done: the row the window is on takes
    /// it from there.
    private func press(_ row: NavPanelTree.Row, _ detail: ProjectDetail) {
        let key = row.id
        NavRow.pressing(row.node, event: NSApp.currentEvent, in: detail,
                        light: { pressed = key }, later: NavRow.afterDrawn) { press in
            act(row.node, press)
            if pressed == key { pressed = nil }
        }
    }

    /// A card's row, seen on the diagram its row is under: what pressing it once does —
    /// a card of the section picked there, a card ghost brought into view in the section it
    /// is placed in, another section's card on the section ghost it is drawn folded into.
    private func showOnDiagram(_ node: NavPanelTree.Node, _ detail: ProjectDetail) {
        model.perform(NavPanelTree.press(node, twice: false, in: detail))
    }

    @ViewBuilder
    private func menu(_ node: NavPanelTree.Node, _ detail: ProjectDetail) -> some View {
        let directory = detail.directory
        switch node {
        case .section(let id):
            if let section = directory.section(id: id) {
                copyPath([directory.full(section: section.id)])
                Divider()
                sectionMenu(section, detail)
            }
        case .node(let id):
            if let node = detail.inner.first(where: { $0.id == id }), let flow = directory.schema(id: node.container) {
                copyPath([directory.full(flow, [node.name])])
            }
        case .autoCard(let at, let card):
            Button(L10n[.Nav.menuViewInRelationPanel]) { showOnDiagram(node, detail) }
            Button(DiagramMenus.connectionsWord) { model.openConnections(card) }
            copyPath(schemaPaths(detail, id: card))
            Button(L10n[.Nav.menuShowTemporarily]) {
                model.selectSection(at)
                model.showForNow(card)
            }
            // A card ghost is a card of the section: a locked main makes none (`specboard://SpecBoard/Docs/Design:DesignBranches` §8).
            if !model.isReadOnly {
                Button(L10n[.Nav.menuKeepAsGhost]) { model.placeGhost(of: card, in: at) }
            }
        case .ghost(let id):
            if let ghost = directory.ghost(id: id) { ghostMenu(ghost, node, detail) }
        case .symbol(let file, let path):
            Button(L10n[.Nav.menuViewInRelationPanel]) { showOnDiagram(node, detail) }
            Button(L10n[.Nav.menuOpenSymbol]) { model.openSymbol(file, path) }
            if let card = directory.schema(id: file) { copyPath([SymbolPath.full(directory.full(card), path)]) }
        case .card(let id):
            Button(L10n[.Nav.menuViewInRelationPanel]) { showOnDiagram(node, detail) }
            Button(DiagramMenus.connectionsWord) { model.openConnections(id) }
            copyPath(schemaPaths(detail, id: id))
            cardActionsMenu(id)
            if let schema = directory.schema(id: id) {
                let home = directory.sectionId(of: schema)
                let readOnly = model.isReadOnly || model.isReadFromFolder(id)
                let type = schema.kind == .value || schema.kind == .enum
                // Not on the diagram on screen: another section's card, or a value
                // object or enum, which is inside the cards holding it.
                if (home != model.shownSectionKey || type) && !model.isForNow(id) {
                    Button(L10n[.Nav.menuShowTemporarilyHere]) { model.showForNow(id) }
                }
                if model.isForNow(id) {
                    Button(L10n[.Nav.menuStopTemporary]) { model.stopShowingForNow(id) }
                }
                // On its own section's diagram as a card of its own: how it looks, so a
                // locked main takes it too.
                if type {
                    Button(schema.standalone ? L10n[.Nav.menuStopStandalone] : L10n[.Nav.menuShowStandalone]) {
                        model.setStandalone(id, !schema.standalone)
                    }
                    .disabled(!model.canSetLooks)
                }
                // A shortcut to it in another section (`specboard://SpecBoard/Docs/Design:DesignSections` §2): not
                // where a locked main keeps its cards.
                if !model.isReadOnly {
                    ghostMenu(for: id, detail)
                }
                // Sorting what the free section holds: into a section of its kind.
                Menu(L10n[.Nav.menuMoveToSection]) {
                    ForEach(detail.homes(for: schema)) { section in
                        Button(directory.shownFromTop(section: section.id)) { model.moveSchema(id, toSection: section.id) }
                    }
                }
                .disabled(readOnly)
                Divider()
                Button(L10n[.Nav.menuDeleteCard], role: .destructive) { deletingCard = id }
                    .disabled(readOnly)
            }
        default:
            EmptyView()
        }
    }

    /// 「アクション ▸」 on a goal's or a task's row: what the core takes now, as the property panel's
    /// buttons (`pressCardAction`).
    @ViewBuilder
    private func cardActionsMenu(_ id: String) -> some View {
        let actions = model.availableCardActions(id)
        if !actions.isEmpty {
            Menu(L10n[.Board.actionsMenu]) {
                ForEach(actions) { action in
                    Button(action.label) { model.pressCardAction(id, action) }
                }
            }
        }
    }

    /// The “Copy path” item, as the diagram's right-click has it: one path copied
    /// straight, several — an embedded value object named from each card holding it — to pick
    /// from.
    @ViewBuilder
    private func copyPath(_ paths: [String]) -> some View {
        if paths.count == 1, let path = paths.first {
            Button(L10n[.Common.actionCopyPath]) { copyToPasteboard(path) }
        } else if paths.count > 1 {
            Menu(L10n[.Common.actionCopyPath]) {
                ForEach(paths, id: \.self) { path in
                    Button(path) { copyToPasteboard(path) }
                }
            }
        }
    }

    /// Which of the rows above the branches is lit (`NavPanelTree.litHead`).
    private var litHead: NavPanelTree.Head? {
        NavPanelTree.litHead(here: model.here, boardOpen: model.isBoardOpen)
    }

    /// The project board (`specboard://SpecBoard/Docs/Design:DesignBoard`): its tree below when picked, as a branch's is.
    private var boardRow: some View {
        let here = litHead == .board
        return Button {
            model.openBoard()
        } label: {
            HStack(spacing: 6) {
                Image(systemName: (model.detail?.sectionMarks[SectionKind.board.rawValue] ?? SectionMark(icon: "checklist")).symbol)
                    .font(.app(11))
                    .foregroundStyle(here ? Color.accentColor : .secondary).frame(width: 14)
                Text(L10n[.Common.wordProjectBoard]).font(.app(12, weight: here ? .semibold : .regular))
                Spacer()
                Text(L10n[.Nav.rowBoardHint]).font(.app(10)).foregroundStyle(.tertiary).lineLimit(1)
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 5)
            .modifier(NavRowSlot(current: here, accent: 0.12, radius: 0))
        }
        .buttonStyle(.plain)
        .help(L10n[.Nav.rowBoardHelp])
    }

    /// What is wrong in a tree, and what was marked as by design: one page for every tree, the tree
    /// picked on it.
    private var inspectionRow: some View {
        let here = litHead == .inspection
        return Button {
            model.open(.inspection)
        } label: {
            HStack(spacing: 6) {
                Image(systemName: "stethoscope").font(.app(11))
                    .foregroundStyle(here ? Color.accentColor : .secondary).frame(width: 14)
                Text(L10n[.Nav.rowInspection]).font(.app(12, weight: here ? .semibold : .regular))
                Spacer()
                Text(L10n[.Nav.rowInspectionHint]).font(.app(10)).foregroundStyle(.tertiary).lineLimit(1)
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 5)
            .modifier(NavRowSlot(current: here, accent: 0.12, radius: 0))
        }
        .buttonStyle(.plain)
        .help(L10n[.Nav.rowInspectionHelp])
    }

    /// The project's settings: above the branches, the project's alone.
    private var settingsRow: some View {
        let here = litHead == .settings
        return Button {
            model.open(.settings(.project))
        } label: {
            HStack(spacing: 6) {
                Image(systemName: "gearshape").font(.app(11)).foregroundStyle(.secondary).frame(width: 14)
                Text(L10n[.Nav.rowProjectSettings]).font(.app(12, weight: here ? .semibold : .regular))
                Spacer()
                Text(L10n[.Nav.rowProjectSettingsHint]).font(.app(10)).foregroundStyle(.tertiary).lineLimit(1)
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 5)
            .modifier(NavRowSlot(current: here, accent: 0.12, radius: 0))
        }
        .buttonStyle(.plain)
        .help(L10n[.Nav.rowProjectSettingsHelp])
    }

    @ViewBuilder
    private func sectionMenu(_ section: ProjectSection, _ detail: ProjectDetail) -> some View {
        cardSectionMenu(section, detail)
    }

    @ViewBuilder
    private func cardSectionMenu(_ section: ProjectSection, _ detail: ProjectDetail) -> some View {
        let directory = detail.directory
        // A working directory's section: its folders and files are read from the folder (a note is put
        // there), and it is read again from the folder by its action.
        let inWorkDir = model.isInWorkDir(section: section.id)
        if inWorkDir {
            WorkDirActionsMenu(model: model, section: section.id)
        }
        Button(L10n[.Nav.menuAddCard]) {
            model.selectSection(section.id)
            model.newSchemaRequest = AppModel.NewSchemaRequest()
        }
        .disabled(model.isReadOnly)
        // The root's name too is main's: a locked main keeps it (`specboard://SpecBoard/Docs/Design:DesignBranches` §8).
        Button(L10n[.Common.actionRenameMenu]) { ask(.rename(section.id), section.name) }
            .disabled(model.isReadOnly || model.isReadFromFolder(section.id))
        // Nothing is made directly under a root: what is there is the project's (`project_tree`); nor in a
        // working directory, whose folders are read.
        if section.parentId != nil, !inWorkDir {
            createMenu(in: section, title: L10n[.Nav.menuCreateInsideThis])
                .disabled(model.isReadOnly)
        }
        Divider()
        // Only once it is empty (`deletable` says which may be).
        if NavPanelTree.deletable(section, in: detail), !model.isReadFromFolder(section.id) {
            let empty = directory.members(section.id).isEmpty && directory.children(of: section.id).isEmpty
            Button(L10n[.Common.actionDelete], role: .destructive) { model.deleteSection(section.id) }
                .disabled(!empty || model.isReadOnly)
        }
    }

    /// A card ghost of a card put in a section picked here (`specboard://SpecBoard/Docs/Design:DesignSections` §2):
    /// any section it may stand in but its own, the ones holding a ghost of it already
    /// left out. Named as the card, or as asked where the section has that name.
    @ViewBuilder
    private func ghostMenu(for id: String, _ detail: ProjectDetail) -> some View {
        let directory = detail.directory
        let places = directory.listed().filter { section in
            NavPanelTree.canHoldGhost(of: id, in: section.id, detail: detail)
                && !directory.ghosts(in: section.id).contains { $0.resolvesTo == id }
        }
        Menu(L10n[.Nav.menuPlaceGhost]) {
            ForEach(places) { section in
                Button(directory.shownFromTop(section: section.id)) { model.placeGhost(of: id, in: section.id) }
            }
        }
        .disabled(places.isEmpty)
    }

    /// A card ghost's row: seen where it is drawn, the card it leads to opened; renamed,
    /// pointed elsewhere, let go — not where a locked main keeps its cards.
    @ViewBuilder
    private func ghostMenu(_ ghost: CardGhost, _ node: NavPanelTree.Node, _ detail: ProjectDetail) -> some View {
        let directory = detail.directory
        Button(L10n[.Nav.menuViewInRelationPanel]) { showOnDiagram(node, detail) }
        if let card = directory.reached(ghost) {
            Button(L10n[.Nav.menuOpenReal]) { model.open(.card(card.id)) }
        }
        // As the diagram's: the ghost's own path, and the card's it leads to.
        Button(L10n[.Nav.menuCopyGhostPath]) { copyToPasteboard(directory.full(ghost)) }
        if let card = directory.reached(ghost) {
            Button(L10n[.Nav.menuCopyRealPath]) { copyToPasteboard(directory.full(card)) }
        }
        if !model.isReadOnly {
            Divider()
            Button(L10n[.Common.actionRenameMenu]) {
                asking = NameAsk(title: L10n[.Nav.promptRenameGhost], placeholder: L10n[.Common.wordName], start: ghost.name,
                                 confirm: "OK", cancel: L10n[.Common.actionCancelDialog], pathName: true) { model.renameGhost(ghost.id, to: $0) }
            }
            Button(L10n[.Nav.menuChangeTarget]) { retargeting = ghost.id }
            Divider()
            Button(L10n[.Nav.menuDeleteGhost], role: .destructive) { model.deleteGhost(ghost.id) }
        }
    }

    // MARK: - Prompts

    /// Making a section in `parent`, of its kind: nothing more goes directly under the
    /// root.
    private func createMenu(in parent: ProjectSection, title: String) -> some View {
        Button(title + "…") { ask(.create(parent: parent.id, kind: parent.kind?.madeInside)) }
    }

    private static func title(_ prompt: Prompt) -> String {
        switch prompt {
        case .create(_, let kind): kind.map { L10n[.Nav.promptCreateKindSection($0.label)] } ?? L10n[.Nav.promptCreateSection]
        case .branch: L10n[.Nav.promptCreateBranch]
        case .rename, .renameBranch: L10n[.Common.actionRename]
        }
    }

    private func ask(_ what: Prompt, _ start: String = "") {
        asking = NameAsk(
            title: Self.title(what),
            message: what == .branch
                ? L10n[.Nav.promptBranchMessage]
                : "",
            placeholder: L10n[.Common.wordName], start: start, confirm: "OK", cancel: L10n[.Common.actionCancelDialog],
            pathName: what.namesSection) { typed in
                commit(what, typed)
            }
    }

    private func commit(_ prompt: Prompt, _ typed: String) {
        switch prompt {
        case .create(let parent, let kind): model.createSection(name: typed, parentId: parent, kind: kind)
        case .branch: model.createBranch(name: typed)
        case .rename(let id): model.renameSection(id, to: typed)
        case .renameBranch(let id): model.renameBranch(id, to: typed)
        }
    }

    // MARK: - What is open, per project

    private static func storageKey(_ projectId: String) -> String { "navigator.open.\(projectId)" }

    /// What was open when the window last showed this project — or, switched to from
    /// another project, nothing: every row folded (overview §8).
    private func load(_ detail: ProjectDetail, folded: Bool = false) {
        guard loadedFor != detail.project.id else { return }
        loadedFor = detail.project.id
        if folded {
            expanded = []
        } else {
            let stored = UserDefaults.standard.stringArray(forKey: Self.storageKey(detail.project.id))
            expanded = NavPanelTree.reopened(stored, detail, showing: model.shownSectionKey)
        }
        // Settled once this update is over: the changes that came with the project are past.
        let project = detail.project.id
        DispatchQueue.main.async { settledOn = project }
    }

    private func save(_ projectId: String, _ open: Set<String>) {
        guard loadedFor == projectId else { return }
        UserDefaults.standard.set(open.sorted(), forKey: Self.storageKey(projectId))
    }
}

// MARK: - Dragging in the tree

/// Where a drag over a row would be taken (`NavPanelTreeView.dropMark`).
private struct DropMark: Equatable {
    var key: String
    var spot: NavPanelTree.Spot
}

/// How tall each row is, by its key: read while a drag is over it, never drawn from, so a
/// reference — changing it redraws nothing.
private final class RowHeights {
    var of: [String: CGFloat] = [:]
}

/// A row that may be picked up and dragged (`NavPanelTree.canDrag`); one that may not
/// stays put, and a press on it is a press.
private struct DragSource: ViewModifier {
    var enabled: Bool
    var item: () -> NSItemProvider

    @ViewBuilder
    func body(content: Content) -> some View {
        if enabled { content.onDrag(item) } else { content }
    }
}

/// A row as a place to drop on: what the drag would do there is asked of
/// `NavPanelTree.drop` as it moves over the row, and only where it would do something is
/// the row marked as taking it — elsewhere the drag says it would not be taken.
private struct NavPanelDrop: DropDelegate {
    var target: NavPanelTree.Node
    var height: () -> CGFloat
    var dragged: () -> NavPanelTree.Node?
    var decide: (NavPanelTree.Node, NavPanelTree.Spot) -> NavPanelTree.Drop?
    var mark: (NavPanelTree.Spot?) -> Void
    var perform: (NavPanelTree.Node, NavPanelTree.Drop, [NSItemProvider]) -> Void

    private func spot(_ info: DropInfo, _ dragging: NavPanelTree.Node) -> NavPanelTree.Spot {
        NavPanelTree.spot(info.location.y, height: height(), dragging: dragging, over: target)
    }

    func validateDrop(info: DropInfo) -> Bool {
        dragged() != nil && info.hasItemsConforming(to: [.text])
    }

    func dropUpdated(info: DropInfo) -> DropProposal? {
        guard let dragging = dragged() else { return DropProposal(operation: .forbidden) }
        let at = spot(info, dragging)
        guard let drop = decide(dragging, at) else {
            mark(nil)
            return DropProposal(operation: .forbidden)
        }
        mark(at)
        // A card ghost made is a copy of nothing but a pointer: the copy cursor, as an alias.
        if case .ghostOf = drop { return DropProposal(operation: .copy) }
        return DropProposal(operation: .move)
    }

    func dropExited(info: DropInfo) { mark(nil) }

    func performDrop(info: DropInfo) -> Bool {
        mark(nil)
        guard let dragging = dragged(), let drop = decide(dragging, spot(info, dragging)) else { return false }
        perform(dragging, drop, info.itemProviders(for: [.text]))
        return true
    }
}

/// The lines of one row of the tree (`NavPanelTree.guides`): a level's line runs
/// under the chevron of the row it hangs from, and a branch reaches across to the
/// row. Drawn a point past the row at top and bottom, over the gap between rows.
private struct GuideLines: View {
    let guides: [NavPanelTree.Guide]
    let indent: CGFloat
    /// How far past its own level's line the branch to the row reaches: to the
    /// chevron, or on to the name where there is none.
    let reach: CGFloat

    var body: some View {
        GeometryReader { geo in
            Path { path in
                let top: CGFloat = -1, bottom = geo.size.height + 1, middle = (geo.size.height / 2).rounded() + 0.5
                for (level, guide) in guides.enumerated() {
                    // Under the chevron of a row at this depth: past the padding (3),
                    // the indent, the spacing (4) and half the chevron (6).
                    let x = CGFloat(level) * indent + 13.5
                    switch guide {
                    case .none:
                        continue
                    case .line:
                        path.move(to: CGPoint(x: x, y: top))
                        path.addLine(to: CGPoint(x: x, y: bottom))
                    case .tee, .end:
                        path.move(to: CGPoint(x: x, y: top))
                        path.addLine(to: CGPoint(x: x, y: guide == .tee ? bottom : middle))
                        path.move(to: CGPoint(x: x, y: middle))
                        path.addLine(to: CGPoint(x: x + reach, y: middle))
                    }
                }
            }
            .stroke(Color.secondary.opacity(0.35), lineWidth: 1)
        }
        .allowsHitTesting(false)
    }
}
