import Foundation

/// What a place on screen that takes a path lets be picked (`specboard://SpecBoard/Docs/Design:DesignSections` §8).
struct PathWanted: Hashable {
    /// Cards of these kinds, whole.
    var cards: Set<SchemaKind>
    /// What is inside the cards listed: fields, a formatdoc's rows, a flow's nodes.
    var inside: Bool = false
    /// Sections, written with the `:` that says so: a section's diagram, or a link to its map.
    var sections: Bool = false
    /// What is not offered, by id: a section (and what is in it), a card, a card ghost.
    var leaveOut: Set<String> = []

    /// Any card, and anything inside one: where a line drawn from the panel may land.
    static let link = PathWanted(cards: Set(SchemaKind.allCases), inside: true)
    /// A `[[…]]` in a text: what a line may land on, and a section (a link to its map).
    static let mention = PathWanted(cards: Set(SchemaKind.allCases), inside: true, sections: true)
    /// A `![[…]]`: what can be drawn — a flow, an action, a card of data, a section.
    static let embed = PathWanted(cards: [.flow, .action, .entity, .value, .enum, .store], sections: true)
    /// A row's place: the cards that hold data and their fields.
    static let data = PathWanted(cards: [.entity, .value, .enum, .store], inside: true)
    /// An action's notes: a note, a document, and the items of a formatdoc.
    static let writing = PathWanted(cards: [.note, .freedoc, .formatdoc], inside: true)
    /// A formatdoc's card table: a shape, whole.
    static let shapes = PathWanted(cards: [.entity, .value, .enum])

    /// What a card ghost may be pointed at instead (`card_update`'s `target`): a card, or
    /// another ghost, of the kind it leads to now — of any kind where it leads nowhere — not
    /// in its own section, and not itself (`specboard://SpecBoard/Docs/Design:DesignSections` §2).
    static func retarget(_ ghost: CardGhost, in directory: SchemaDirectory) -> PathWanted {
        let kinds = directory.reached(ghost).map { Set([$0.kind]) } ?? Set(SchemaKind.allCases)
        return PathWanted(cards: kinds, leaveOut: [ghost.id, directory.sectionId(of: ghost)])
    }
}

/// One line of the selector: a tree, a section, a card or a name inside one.
struct PathChoice: Identifiable, Hashable {
    /// `ghost`: a card ghost, by the kind of the card it leads to — picked, its own path.
    /// `symbol`: a function or type of a file, by what it is (`CodeSymbol.of`).
    enum Kind: Hashable { case tree, section, card(SchemaKind), ghost(SchemaKind), field, formatRow, node, symbol(String) }

    var id: String
    /// As it is shown on the line: the name only; the path says the rest.
    var title: String
    var kind: Kind
    /// In full, when it can be picked.
    var path: String?
    /// Nil when there is nothing under it.
    var children: [PathChoice]?
    /// A line standing for what is not read yet: the file whose functions and types are read as it shows.
    var asks: String? = nil
}

/// The project as a card sees it, laid out to pick a path from: the tree → its sections
/// → the cards, and the card ghosts after them → what is inside them. Every path is in
/// full; through a ghost, the ghost's (`Data:Admin.id`, `specboard://SpecBoard/Docs/Design:DesignSections` §2).
enum PathChoices {

    /// `owner`: the card the path is written in — it is not offered itself. `outside`: main's
    /// detail, from the board (`AppModel.outsideDetail`): its tree after the
    /// board's, every path in it in full.
    ///
    /// `outlines`: the files' functions and types read so far (`AppModel.outlines`) — offered inside their
    /// files; a file not read yet holds a line that asks for it (`PathChoice.asks`). Nil: files whole.
    static func tree(_ detail: ProjectDetail, from owner: SchemaNode?, wanted: PathWanted,
                     outside: ProjectDetail? = nil,
                     words: (own: String, outside: String) = (Board.title, SchemaDirectory.mainWord),
                     outlines: [String: FileOutline]? = nil) -> [PathChoice] {
        let own = trees(detail, from: owner, wanted: wanted, outlines: outlines)
        guard let outside else { return own }
        // Both trees, each said by what it is.
        func titled(_ list: [PathChoice], _ word: String) -> [PathChoice] {
            list.map { tree in
                var tree = tree
                tree.title = word
                return tree
            }
        }
        return titled(own, words.own) + titled(trees(outside, from: nil, wanted: wanted), words.outside)
    }

    private static func trees(_ detail: ProjectDetail, from owner: SchemaNode?, wanted: PathWanted,
                              outlines: [String: FileOutline]? = nil) -> [PathChoice] {
        let directory = detail.directory
        let roots = directory.children(of: nil)

        /// What is inside a card, written from `written` — its own path, or a ghost's;
        /// `key` tells the lines apart where a card is under its ghosts too.
        func inside(_ card: SchemaNode, written: String, key: String) -> [PathChoice] {
            guard wanted.inside else { return [] }
            if card.isCodeFile, let outlines { return symbols(card, written: written, outlines) }
            switch card.kind {
            case .formatdoc:
                func rows(_ list: [FormatRow], _ trail: [String]) -> [PathChoice] {
                    list.map { row in
                        let at = trail + [row.name]
                        let under = row.kind == .item ? rows(row.rows, at) : []
                        return PathChoice(id: "k|\(key)|\(PathText.joinNames(at))", title: row.title,
                                          kind: .formatRow, path: at.reduce(written, PathText.member),
                                          children: under.isEmpty ? nil : under)
                    }
                }
                return rows(card.rows, [])
            case .stackdoc:
                // Its versions, by their keys (a version's own path is its name's).
                return card.versions.map { version in
                    PathChoice(id: "v|\(key)|\(version.name)", title: version.key, kind: .formatRow,
                               path: PathText.member(written, version.name), children: nil)
                }
            case .flow:
                return card.nodes.map { node in
                    PathChoice(id: "n|\(key)|\(node.name)", title: node.name, kind: .node,
                               path: PathText.member(written, node.name), children: nil)
                }
            case .instance:
                // Its values, by its shape's fields (nothing inside an array is named).
                guard let shape = card.instanceOf.flatMap(directory.schema(id:)) else { return [] }
                func values(_ list: [FieldNode], _ trail: [String]) -> [PathChoice] {
                    list.map { field in
                        let at = trail + [field.name]
                        let under = field.isArray ? [] : values(field.children, at)
                        return PathChoice(id: "d|\(key)|\(PathText.joinNames(at))", title: field.name, kind: .field,
                                          path: at.reduce(written, PathText.member),
                                          children: under.isEmpty ? nil : under)
                    }
                }
                return values(shape.fields, [])
            default:
                func fields(_ list: [FieldNode], _ trail: [String]) -> [PathChoice] {
                    list.map { field in
                        let at = trail + [field.name]
                        let under = fields(field.children, at)
                        return PathChoice(id: "f|\(key)|\(field.id)", title: field.name, kind: .field,
                                          path: at.reduce(written, PathText.member),
                                          children: under.isEmpty ? nil : under)
                    }
                }
                return fields(card.fields, [])
            }
        }

        /// A file's functions and types, a type's members under it, each by its path after the file's.
        func symbols(_ file: SchemaNode, written: String, _ outlines: [String: FileOutline]) -> [PathChoice] {
            guard let outline = outlines[file.id] else {
                return [PathChoice(id: "p|\(file.id)", title: "…", kind: .symbol(""), path: nil, children: nil, asks: file.id)]
            }
            func under(_ parent: String?) -> [PathChoice] {
                outline.children(of: parent).map { symbol in
                    let inner = under(symbol.path)
                    return PathChoice(id: "y|\(file.id)|\(symbol.path)", title: symbol.name, kind: .symbol(symbol.of),
                                      path: SymbolPath.full(written, symbol.path), children: inner.isEmpty ? nil : inner)
                }
            }
            return under(nil)
        }

        func cardChoice(_ card: SchemaNode) -> PathChoice? {
            guard card.id != owner?.id, wanted.cards.contains(card.kind), !wanted.leaveOut.contains(card.id) else { return nil }
            let written = directory.full(card)
            // The cards inside a container (a goal's), by their paths in it (`Goals:Ship.Design`).
            let held = card.inside.compactMap { directory.schema(id: $0) }.compactMap(cardChoice)
            let under = inside(card, written: written, key: card.id) + held
            return PathChoice(id: "c|\(card.id)", title: card.name, kind: .card(card.kind),
                              path: written, children: under.isEmpty ? nil : under)
        }

        // A card ghost: picked, its own path; what is inside, the card's, written through it.
        func ghost(_ ghost: CardGhost) -> PathChoice? {
            guard let card = directory.reached(ghost), card.id != owner?.id, wanted.cards.contains(card.kind),
                  !wanted.leaveOut.contains(ghost.id) else { return nil }
            let written = directory.full(ghost)
            let under = inside(card, written: written, key: ghost.id)
            return PathChoice(id: "g|\(ghost.id)", title: "\(ghost.name) → \(directory.shownTarget(of: ghost))",
                              kind: .ghost(card.kind), path: written, children: under.isEmpty ? nil : under)
        }

        func sectionChoice(_ section: ProjectSection) -> PathChoice? {
            // A section left out offers nothing of its own; the sections inside it still do.
            let skipped = wanted.leaveOut.contains(section.id)
            let inner = directory.children(of: section.id).compactMap(sectionChoice)
            let cards = skipped ? [] : directory.members(section.id)
                .sorted { $0.name.localizedStandardCompare($1.name) == .orderedAscending }
                .compactMap(cardChoice)
            let ghosts = skipped ? [] : directory.ghosts(in: section.id)
                .sorted { $0.name.localizedStandardCompare($1.name) == .orderedAscending }
                .compactMap(ghost)
            let children = inner + cards + ghosts
            guard (wanted.sections && !skipped) || !children.isEmpty else { return nil }
            return PathChoice(id: "s|\(section.id)", title: section.name, kind: .section,
                              path: wanted.sections && !skipped ? directory.full(section: section.id) + ":" : nil,
                              children: children.isEmpty ? nil : children)
        }

        return roots.compactMap { root in
            let children = directory.children(of: root.id).compactMap(sectionChoice)
            guard !children.isEmpty else { return nil }
            let word = directory.treeWord(root.id)
            let title = word == root.name ? word : L10n[.Nav.selectorTreeNamed(word, root.name)]
            return PathChoice(id: "t|\(root.id)", title: title, kind: .tree, path: nil, children: children)
        }
    }

    /// Everything that can be picked whose path has `query` in it, in tree order —
    /// at most `limit`, so a short query in a big project stays a list.
    static func found(_ detail: ProjectDetail, from owner: SchemaNode?, wanted: PathWanted,
                      query: String, limit: Int = 200, outside: ProjectDetail? = nil,
                      words: (own: String, outside: String) = (Board.title, SchemaDirectory.mainWord),
                      outlines: [String: FileOutline]? = nil) -> [PathChoice] {
        let needle = query.trimmingCharacters(in: .whitespaces).lowercased()
        var out: [PathChoice] = []
        func walk(_ list: [PathChoice]) {
            for choice in list where out.count < limit {
                if let path = choice.path, needle.isEmpty || path.lowercased().contains(needle) {
                    var flat = choice
                    flat.children = nil
                    out.append(flat)
                }
                walk(choice.children ?? [])
            }
        }
        walk(tree(detail, from: owner, wanted: wanted, outside: outside, words: words, outlines: outlines))
        return out
    }
}
