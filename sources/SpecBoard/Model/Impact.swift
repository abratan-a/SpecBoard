import Foundation

/// Which actions a change reaches — and which notes talk about what changed, and
/// what a key can be used to write.
///
/// An action says where it reads and writes with paths (`ModuleUse`). A change is a
/// path too. The two meet when one lies inside the other — an action reading
/// `StoreSettings.warehouse` is reached by a change to `warehouse.primary` inside it,
/// and an action reading `warehouse.primary` is reached by a change to all of `warehouse`.
///
/// Containment is followed. A `ref` puts the target's whole shape inside the
/// holder, so reading `StoreSettings.return_address` reads every field of the value
/// object it holds, and a change to that value object reaches every action reading
/// any document that holds it — which is the case a tag per module would not catch:
/// the tag would sit on the document, and the change is made to the value object.
///
/// Designation is not followed. A field that names another document by id or digest
/// holds a string; the named document can change shape without the reading code
/// noticing.
///
/// A change is matched by the way it is written, like a key (`keyUse`): one written
/// from its holder (`Config.anchor.value`) is that holder's, and reaches what reads
/// through it — not every action reading some other holder of the same type. One
/// written as the type itself (`Key.value`) is every holder's.
enum Impact {

    /// A place in a document: a schema, and a dotted path inside it (`""` for all of
    /// it). The path stops at a schema boundary — what a `ref` holds is addressed
    /// from its own schema.
    struct Location: Hashable, Sendable {
        var schemaId: String
        var path: String
    }

    struct Hit: Hashable, Sendable {
        /// The action, or a note — a note's `[[path]]` is read like a use that reads.
        var action: SchemaNode
        var use: ModuleUse
        /// How the change reaches it, for a person to check: the use's own path, and
        /// the documents it passes through on the way.
        var via: String
        /// The change encloses what the action uses, rather than lying inside it.
        var encloses: Bool
    }

    /// The card a hit is, written in full — for a formatdoc, the item that names what
    /// changed (its row, kept as the use's note), or the card when a row at the top does.
    static func written(_ hit: Hit, in directory: SchemaDirectory) -> String {
        guard hit.action.kind == .formatdoc, let row = hit.use.note, row != SchemaNode.topRow else {
            return directory.full(hit.action)
        }
        return directory.full(hit.action, row: row)
    }

    /// Actions reached by a change at `path`, and notes naming what changed. Nil
    /// when the path does not resolve.
    ///
    /// `path` is read from `base`; each action's uses and each note's references
    /// from that schema's own section.
    static func of(_ path: String, in detail: ProjectDetail, base: String? = nil) -> [Hit]? {
        let index = Index(detail)
        guard let wanted = index.chain(path, from: base, enterFinalRef: false), let first = wanted.first else {
            return nil
        }
        var hits: [Hit] = []
        for module in detail.schemas where module.kind == .action || module.kind == .flow || module.kind.isWriting {
            let home = index.directory.sectionId(of: module)
            let uses = module.kind == .action || module.kind == .flow ? module.usesInEffect
                // A formatdoc's: the item a mention is in, said as the use's note.
                : Self.mentions(module).map { ModuleUse(path: $0.path, role: .read, note: $0.row) }
            for use in uses {
                guard let used = index.chain(use.path, from: home, enterFinalRef: true) else { continue }

                // A use that ends on a `ref` reads what it holds; say so in the trail,
                // since the path alone stops at the field.
                var start = use.path
                if used.count > 1, let last = used.last, last.path.isEmpty,
                   let held = index.byId[last.schemaId] {
                    start += " → \(index.directory.full(held))"
                }

                // The change lies inside what the action uses: its way runs on from
                // where the use ends, or starts somewhere inside what the use holds.
                if Self.runsOn(from: used, into: wanted) {
                    hits.append(Hit(action: module, use: use, via: start, encloses: false))
                    continue
                }
                if let trail = index.covered(from: used.last!, trail: start)[first] {
                    hits.append(Hit(action: module, use: use, via: trail, encloses: false))
                    continue
                }
                // Or encloses it: the change's way is a run of the way to what the
                // action uses.
                if Self.isRun(wanted, of: used) {
                    hits.append(Hit(action: module, use: use, via: use.path, encloses: true))
                }
            }
        }
        return hits
    }

    /// What a note's or a document's text names, once each, in the order it first names
    /// them — for a formatdoc, with the item whose rows name it (`#top` for the rows at the
    /// top), for a stackdoc with the version whose body names it — as the core says what it
    /// mentions (`mentions`).
    static func mentions(_ card: SchemaNode) -> [(path: String, row: String?)] {
        switch card.kind {
        case .note, .freedoc:
            var seen = Set<String>()
            return NoteText.refs(card.body).filter { seen.insert($0.path).inserted }.map { ($0.path, nil) }
        case .formatdoc:
            var out: [(path: String, row: String?)] = []
            func from(_ item: [String], row: String) {
                var seen = Set<String>()
                for said in FormatRows.said(by: item, in: card.rows) {
                    let paths = said.place == "target" ? [said.text] : NoteText.refs(said.text).map(\.path)
                    for path in paths where seen.insert(path).inserted { out.append((path, row)) }
                }
            }
            from([], row: SchemaNode.topRow)
            FormatRows.walk(card.rows) { row, path, _ in
                if row.kind == .item { from(path, row: PathText.joinNames(path)) }
            }
            return out
        case .stackdoc:
            var out: [(path: String, row: String?)] = []
            for version in card.versions {
                var seen = Set<String>()
                for ref in NoteText.refs(version.body) where seen.insert(ref.path).inserted {
                    out.append((ref.path, PathText.quote(version.name)))
                }
            }
            return out
        default:
            return []
        }
    }

    /// The use's way ends where the change's way begins or passes: the last few
    /// places of one are the first few of the other.
    static func runsOn(from used: [Location], into wanted: [Location]) -> Bool {
        (1...min(used.count, wanted.count)).contains { Array(used.suffix($0)) == Array(wanted.prefix($0)) }
    }

    /// `part` is a contiguous stretch of `route`.
    static func isRun(_ part: [Location], of route: [Location]) -> Bool {
        guard route.count >= part.count, !part.isEmpty else { return false }
        return (0...(route.count - part.count)).contains { Array(route[$0..<($0 + part.count)]) == part }
    }

    /// What a key is used for: which actions sign or check with it, what those that
    /// sign can write, and which documents it signs or checks. The question is "if
    /// this key were stolen, what could be written with it" — and, for the checking
    /// half, "who would believe it".
    struct KeyUse: Sendable {
        var signers: [(action: SchemaNode, writes: [String])] = []
        var checkers: [SchemaNode] = []

        var isEmpty: Bool { signers.isEmpty && checkers.isEmpty }
    }

    /// Nil when the path does not resolve.
    ///
    /// A key is matched by the way to it, not by where it lands. A path from its
    /// holder (`Staff:StaffEntry.credentials[].key.public_key`) is the keys on
    /// that route — not every key of the same type. A path to what holds it
    /// (`GatewayConfig.signing`) is every key under it, and a path to the type
    /// itself (`Common:PublicKey.public_key`) is every key of that type, wherever
    /// it is kept: each is a run of the route a key's path takes.
    static func keyUse(_ path: String, in detail: ProjectDetail, base: String? = nil) -> KeyUse? {
        let index = Index(detail)
        guard let wanted = index.chain(path, from: base, enterFinalRef: false), !wanted.isEmpty else {
            return nil
        }
        func matches(_ key: String, from owner: SchemaNode) -> Bool {
            guard let route = index.chain(key, from: index.directory.sectionId(of: owner),
                                          enterFinalRef: false) else { return false }
            return isRun(wanted, of: route)
        }
        var out = KeyUse()
        for schema in detail.schemas {
            if schema.kind == .action || schema.kind == .flow {
                if schema.signingKeysInEffect.contains(where: { matches($0, from: schema) }) {
                    out.signers.append((schema, schema.usesInEffect.filter { $0.role == .write }.map(\.path)))
                }
                if schema.verifyingKeysInEffect.contains(where: { matches($0, from: schema) }) {
                    out.checkers.append(schema)
                }
            }
        }
        return out
    }

    /// Lookups shared by every question asked of one project.
    struct Index {
        var byId: [String: SchemaNode] = [:]
        let directory: SchemaDirectory

        init(_ detail: ProjectDetail) {
            directory = detail.directory
            for s in detail.schemas { byId[s.id] = s }
        }

        /// Every location a path passes through, outermost first.
        ///
        /// `enterFinalRef`: whether a path ending on a `ref` goes on into its target.
        /// For something a module reads, yes — reading the field is reading what it
        /// holds. For something being changed, no — changing the field `return_address`
        /// is not changing every `Address` in the project.
        func chain(_ raw: String, from base: String?, enterFinalRef: Bool) -> [Location]? {
            guard let written = WrittenPath(raw), let schema = directory.schema(written.card, from: base),
                  case .success(let walk) = CardWalk.walk(schema, written.names, cards: byId) else { return nil }
            // A card and its fields: what changing or reading a field reaches.
            switch walk.place {
            case .card, .field: break
            case .formatRow, .node, .customRow: return nil
            }
            var out: [Location] = []
            for leg in walk.legs {
                out.append(Location(schemaId: leg.card.id, path: ""))
                for end in leg.names.indices {
                    out.append(Location(schemaId: leg.card.id, path: PathText.joinNames(leg.names[...end])))
                }
            }
            if enterFinalRef, case .field(let field) = walk.place, field.type.isRef,
               let target = field.embeds.flatMap({ byId[$0] }) {
                out.append(Location(schemaId: target.id, path: ""))
            }
            return out
        }

        /// Everything inside a location, through containment, each with the first
        /// way it was reached.
        func covered(from start: Location, trail: String) -> [Location: String] {
            var out: [Location: String] = [start: trail]
            guard let schema = byId[start.schemaId] else { return out }

            var fields = schema.fields
            if !start.path.isEmpty {
                var list = schema.fields
                var found: FieldNode?
                for step in PathText.names(start.path) {
                    found = list.first { $0.name == step }
                    list = found?.children ?? []
                }
                guard let found else { return out }
                fields = found.children
            }

            var entered: Set<String> = [start.schemaId]
            func walk(_ fields: [FieldNode], in schemaId: String, prefix: String, trail: String) {
                for field in fields {
                    let path = prefix.isEmpty ? PathText.quote(field.name) : PathText.member(prefix, field.name)
                    let here = Location(schemaId: schemaId, path: path)
                    if out[here] == nil { out[here] = trail }
                    if !field.children.isEmpty {
                        walk(field.children, in: schemaId, prefix: path, trail: trail)
                    }
                    if field.type.isRef, let id = field.embeds, let target = byId[id],
                       entered.insert(id).inserted {
                        let into = "\(trail) → \(directory.full(target))"
                        let root = Location(schemaId: id, path: "")
                        if out[root] == nil { out[root] = into }
                        walk(target.fields, in: id, prefix: "", trail: into)
                    }
                }
            }
            walk(fields, in: start.schemaId, prefix: start.path, trail: trail)
            return out
        }
    }
}
