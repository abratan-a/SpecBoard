import Foundation

/// What a written path lands on, by identity rather than by name: the schema it
/// starts from — nil when it is written from inside its own schema — and the field
/// at each step. Names change and these do not, so a path read before a rename can
/// be written again after it and still mean the same thing.
struct PathIdentity: Equatable {
    var head: String?
    var steps: [String]
    /// For a path into a formatdoc's rows — their names from the top — or to a flow's
    /// node, its name. Neither has an identity but its place, so one that moves or
    /// is renamed is followed by being told where it went (`Repository.followRenames`).
    var inner: [String]? = nil
}

/// One project's paths, read into identities and written back out.
///
/// Reading follows `resolvePath` step for step — the section it is read from, a
/// `ref` as a step into the target, a rule's path from inside its own schema — so a
/// path is followed exactly when the checker would call it resolved.
struct PathIdentities {
    let directory: SchemaDirectory
    private let schemas: [String: SchemaNode]
    private let fields: [String: FieldNode]

    /// `leaves`: read section names the way paths were written before a section had
    /// to be named from the top (`SchemaDirectory.leavesAllowed`). `oldPaths`: read a
    /// tree named the way it was before `@` (`SchemaDirectory.oldPathsAllowed`).
    init(_ detail: ProjectDetail, leaves: Bool = false, oldPaths: Bool = false) {
        var directory = detail.directory
        directory.leavesAllowed = leaves
        directory.oldPathsAllowed = oldPaths
        self.directory = directory
        var schemas: [String: SchemaNode] = [:]
        var fields: [String: FieldNode] = [:]
        for schema in detail.schemas {
            schemas[schema.id] = schema
            walkFields(schema.fields) { field, _, _ in fields[field.id] = field }
        }
        self.schemas = schemas
        self.fields = fields
    }

    func schema(_ id: String) -> SchemaNode? { schemas[id] }
    func field(_ id: String) -> FieldNode? { fields[id] }

    /// Nil when the path does not resolve — a broken path stays as it was written.
    func identify(_ raw: String, from owner: SchemaNode?) -> PathIdentity? {
        guard let written = WrittenPath(raw), written.wellFormed else { return nil }
        let reader = owner.map { directory.sectionId(of: $0) }
        if let head = directory.find(written.card, from: reader).schema,
           case .success(let walk) = CardWalk.walk(head, written.names, cards: schemas) {
            switch walk.place {
            case .formatRow(let names): return PathIdentity(head: head.id, steps: [], inner: names)
            case .node(let node): return PathIdentity(head: head.id, steps: [], inner: [node.name])
            case .customRow(let names): return PathIdentity(head: head.id, steps: [], inner: names)
            case .card, .field: return PathIdentity(head: head.id, steps: walk.fields.map(\.id))
            }
        }
        // A rule's path, or a payload's, from inside its own card: fields only.
        if let owner, splitSection(raw).section == nil, let card = schemas[owner.id],
           case .success(let walk) = CardWalk.walk(card, [written.card] + written.names, cards: schemas),
           case .field = walk.place {
            return PathIdentity(head: nil, steps: walk.fields.map(\.id))
        }
        return nil
    }

    /// `raw` again, naming what `identity` names in this project — in full, as every
    /// stored path is (`Core:User.id`, `specboard://SpecBoard/Docs/Design:DesignSections` §2). Its `[]`
    /// markers stay as they were written; the names are what they are now, quoted where they need
    /// it. A path written from inside its own card (no head) takes that card, `owner`, as its head.
    func write(_ raw: String, _ identity: PathIdentity, owner: SchemaNode? = nil) -> String? {
        let (_, rest) = splitSection(raw)
        var marked = PathText.dots(rest).map(Self.marked)
        var names: [String] = []
        let headCard: SchemaNode
        if let id = identity.head {
            guard let schema = schemas[id] else { return nil }
            headCard = schema
        } else {
            guard let owner, let schema = schemas[owner.id] else { return nil }
            headCard = schema
            marked.insert(false, at: 0)
        }
        names.append(headCard.name)
        for id in identity.steps {
            guard let field = fields[id] else { return nil }
            names.append(field.name)
        }
        if let inner = identity.inner {
            // A clause can move to another depth: the path is as long as it is now.
            names += inner
            marked = Array(marked.prefix(1)) + inner.map { _ in false }
        }
        guard names.count == marked.count else { return nil }
        var segments = names.indices.map { PathText.quote(names[$0]) + (marked[$0] ? "[]" : "") }
        segments[0] = directory.full(headCard) + (marked[0] ? "[]" : "")
        return segments.joined(separator: ".")
    }

    /// Whether a name as written marks every element (`lines[]`).
    private static func marked(_ written: String) -> Bool {
        PathText.step(written).rest.trimmingCharacters(in: .whitespaces) == "[]"
    }
}
