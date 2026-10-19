import Foundation

/// Everything derived from the model and nothing else — no database, no I/O.
/// Ported from `src/shared/types.ts` and the pure half of `src/server/lib/repo.ts`.

// MARK: - Walking the field tree

/// Walk a field tree depth-first, handing back the dotted path and nesting depth.
func walkFields(
    _ fields: [FieldNode],
    path: [String] = [],
    depth: Int = 0,
    visit: (FieldNode, [String], Int) -> Void
) {
    for f in fields {
        let next = path + [f.name]
        visit(f, next, depth)
        if !f.children.isEmpty {
            walkFields(f.children, path: next, depth: depth + 1, visit: visit)
        }
    }
}

func countFields(_ fields: [FieldNode]) -> Int {
    var n = 0
    walkFields(fields) { _, _, _ in n += 1 }
    return n
}

func maxDepth(_ fields: [FieldNode]) -> Int {
    var d = 0
    walkFields(fields) { _, _, depth in d = max(d, depth + 1) }
    return d
}

extension SchemaNode {
    /// Discriminator values in play, derived from the top-level fields.
    var variantValues: [String] {
        guard discriminator != nil else { return [] }
        var seen: [String] = []
        for f in fields {
            for v in f.variants where !seen.contains(v) { seen.append(v) }
        }
        return seen
    }

    var fieldCount: Int { countFields(fields) }
}

/// Every tag in use, most-used first. Drives the filter chips and MCP overviews.
func projectTags(_ schemas: [SchemaNode]) -> [(tag: String, count: Int)] {
    var counts: [String: Int] = [:]
    for s in schemas {
        for t in s.tags { counts[t, default: 0] += 1 }
    }
    return counts.map { (tag: $0.key, count: $0.value) }
        .sorted { $0.count != $1.count ? $0.count > $1.count : $0.tag < $1.tag }
}

/// Schemas breaking an exclusive group's rule, and by how much.
///
/// Only schemas that carry at least one tag take part. In the project this is for, all
/// 36 entities are tagged and all 109 values and enums are not — a check that swept
/// everything would open with 109 findings and be switched off the same day. A
/// schema outside the tagging scheme is not missing a layer.
func tagGroupViolations(_ detail: ProjectDetail)
-> [(group: String, at: String, count: Int)] {
    var out: [(group: String, at: String, count: Int)] = []
    let directory = detail.directory
    for group in detail.project.tagGroups where group.exclusive {
        let members = Set(expandTags(groups: detail.project.tagGroups, selected: [group.name]))
        for schema in detail.schemas where !schema.tags.isEmpty {
            let held = schema.tags.filter { members.contains($0) }.count
            if held != 1 { out.append((group.name, directory.full(schema), held)) }
        }
    }
    return out
}

/// Where a field sits: which schema owns it, and the dotted path to it.
struct FieldLocation {
    var schema: SchemaNode
    var field: FieldNode
    /// Path within the schema, e.g. `identityKeys.retiring`.
    var path: [String]

    /// What a person and the MCP both write to name this: `Schema.a.b`, each name quoted where it
    /// needs it.
    var dotted: String { PathText.joinNames([schema.name] + path) }
}

/// Find a field by id anywhere in a project. Ids never appear in the UI, but the
/// canvas hands one back when a row is clicked and the property panel has to find it.
func locateField(_ schemas: [SchemaNode], id: String) -> FieldLocation? {
    for schema in schemas {
        var found: FieldLocation?
        walkFields(schema.fields) { field, path, _ in
            if found == nil && field.id == id {
                found = FieldLocation(schema: schema, field: field, path: path)
            }
        }
        if let found { return found }
    }
    return nil
}

/// The ids of the fields a field is nested inside, outermost first.
///
/// `FieldLocation.path` carries names, which is what a person reads; this carries
/// ids, which is what a folded tree is keyed on. Being sent to look at a field has
/// to unfold everything above it, or the destination is not on screen at all.
///
/// `nil` when the field is not in these fields — which is the common case, since a
/// selection can move to a different card entirely.
func fieldAncestry(_ fields: [FieldNode], id: String) -> [String]? {
    for field in fields {
        if field.id == id { return [] }
        if let deeper = fieldAncestry(field.children, id: id) { return [field.id] + deeper }
    }
    return nil
}

/// Schemas within `depth` hops of the given ones, following relations either way.
///
/// The same reachability a diagram's `roots`/`depth` walks, so asking the window
/// to show a neighbourhood and asking for the mermaid of one land on the same set.
/// Direction is ignored on purpose: on screen a card that points *at* the subject is
/// as much a part of its surroundings as one it points to.
func neighbourhood(_ detail: ProjectDetail, from roots: Set<String>, depth: Int) -> Set<String> {
    guard depth > 0, !roots.isEmpty else { return roots }
    var keep = roots
    var frontier = roots
    for _ in 0..<depth {
        var next = Set<String>()
        for relation in detail.relations {
            if frontier.contains(relation.fromSchemaId), !keep.contains(relation.toSchemaId) {
                next.insert(relation.toSchemaId)
            }
            if frontier.contains(relation.toSchemaId), !keep.contains(relation.fromSchemaId) {
                next.insert(relation.fromSchemaId)
            }
        }
        if next.isEmpty { break }
        keep.formUnion(next)
        frontier = next
    }
    return keep
}

/// Where a declared path lands, read from a schema in `base`: the schema it starts
/// from, and the row in it. `Core:User.id[]` lands on `User` in Core, row `id`.
func landing(_ path: String, from base: String,
             in directory: SchemaDirectory) -> (schema: SchemaNode, row: String)? {
    guard let written = WrittenPath(path), let target = directory.schema(written.card, from: base) else { return nil }
    // The row as the card draws it: a field's or a clause's names, `#n.<name>` for a
    // flow's node. One that is not there is said as written, and lands on the card.
    if case .success(let walk) = CardWalk.walk(target, written.names, in: directory) {
        return (target, walk.row)
    }
    return (target, written.names.map(PathText.canonicalStep).joined(separator: "."))
}

/// The cards a field's lines reach, in the order its lines go: what it embeds, what it
/// names — read off the project's lines (`relations`), each the card it reaches (through a
/// card ghost, the card the ghost leads to). Not a pointer told apart by its
/// discriminator: it names its cards by a rule, thirty of them in a real project, and thirty
/// cards is not a menu — the field's own view lists them.
///
/// This is what the canvas's context menu offers on a row. The card's own schema is
/// the same answer on every row of it, so a menu that offered that instead said
/// nothing about the row that was clicked.
func referencedSchemas(_ detail: ProjectDetail, owner: SchemaNode,
                       _ field: FieldNode) -> [SchemaNode] {
    var out: [SchemaNode] = []
    var seen = Set<String>()
    let directory = detail.directory
    let byRule = Set(owner.connections.filter { $0.to == nil }.map(\.id))
    for relation in detail.relations where relation.fieldId == field.id {
        if let line = relation.connection, byRule.contains(line) { continue }
        guard let card = directory.schema(id: relation.toSchemaId), seen.insert(card.id).inserted else { continue }
        out.append(card)
    }
    return out
}

// MARK: - Naming a field from outside

/// How many paths are worth listing before the list stops being a list.
let pathCeiling = 40

/// Every dotted path that names an occurrence of this schema.
///
/// A value schema is written once and embedded in many places, so it has a name of
/// its own — `Address` — and a name at each site that embeds it, such as
/// `StoreSettings.warehouse.primary.address`. Both are real answers to
/// "what is this called"; which one is wanted depends on what is being written.
///
/// Only `ref` is followed. `pointsAt` designates something that lives elsewhere, so
/// it never lends its path to the target — that is the whole distinction between the
/// two, and flattening it here would put a foreign key's owner in the middle of a
/// path that no document ever contains.
///
/// In a project with sections each path starts with its section — `Core:Address` — so
/// it can be pasted anywhere and still mean one thing.
func schemaPaths(_ detail: ProjectDetail, id: String, limit: Int = pathCeiling) -> [String] {
    schemaPaths(detail.schemas, id: id, limit: limit, directory: detail.directory)
}

func schemaPaths(_ schemas: [SchemaNode], id: String, limit: Int = pathCeiling,
                 directory: SchemaDirectory? = nil) -> [String] {
    var out: [String] = []
    var seen: Set<String> = []

    func expand(_ schemaId: String, _ suffix: [String], _ chain: Set<String>) {
        // A schema that embeds itself, directly or through others, would name itself
        // forever. It is not legal, but this runs on real data that can be mid-edit.
        guard out.count < limit, !chain.contains(schemaId),
              let schema = schemas.first(where: { $0.id == schemaId }) else { return }

        let own = suffix.reduce(directory?.full(schema) ?? PathText.quote(schema.name), PathText.member)
        if seen.insert(own).inserted { out.append(own) }

        let deeper = chain.union([schemaId])
        for other in schemas {
            walkFields(other.fields) { field, path, _ in
                guard field.type.isRef, field.embeds == schemaId else { return }
                expand(other.id, path + suffix, deeper)
            }
        }
    }
    expand(id, [], [])

    // Shortest first: the schema's own name is the one most often wanted, and a long
    // path is easier to recognise once the short ones are out of the way.
    return out.sorted {
        let a = PathText.dots(PathText.splitSection($0).rest).count
        let b = PathText.dots(PathText.splitSection($1).rest).count
        return a == b ? $0 < $1 : a < b
    }
}

/// Every dotted path that reaches this field. See `schemaPaths`.
func fieldPaths(_ detail: ProjectDetail, fieldId: String, limit: Int = pathCeiling) -> [String] {
    fieldPaths(detail.schemas, fieldId: fieldId, limit: limit, directory: detail.directory)
}

func fieldPaths(_ schemas: [SchemaNode], fieldId: String, limit: Int = pathCeiling,
                directory: SchemaDirectory? = nil) -> [String] {
    guard let location = locateField(schemas, id: fieldId) else { return [] }
    return schemaPaths(schemas, id: location.schema.id, limit: limit, directory: directory)
        .map { location.path.reduce($0, PathText.member) }
}

// MARK: - Path resolution

struct PathHit {
    var ok: Bool
    var kind: String?          // "schema" | "field"
    var type: String?
    /// What exists at the level that failed, so a citation can be corrected.
    var hint: String?
}

struct PathResolution: Codable, Hashable, Sendable {
    var path: String
    var resolved: Bool
    var kind: String?
    var type: String?
    var hint: String?
}

/// Resolve one dotted path against a project.
///
/// Three things a naive splitter gets wrong, each of which turns a correct path
/// into a false alarm — and a checker that cries wolf is worse than none:
///
///     policies[].id      array markers are notation, not a step
///     config.policy.id   a `ref` is a step: the path continues in the target
///     keys.active.pub    a rule's path is relative to the schema that declares it
///
/// And one that sections add: `Core:User.id` starts in Core, and `User.id` in the
/// section it is read from — `from`'s, or `base` when nothing declares it. With
/// neither, a project of several sections cannot say which `User` is meant, and the
/// hint says where there is one.
func resolvePath(_ detail: ProjectDetail, _ raw: String, from: SchemaNode? = nil,
                 base: String? = nil, directory given: SchemaDirectory? = nil) -> PathHit {
    guard let written = WrittenPath(raw) else { return PathHit(ok: false, hint: pathText(.Inspection.pathEmpty)) }

    let directory = given ?? detail.directory
    let byId = detail.cardsById
    let reader = from.map { directory.sectionId(of: $0) } ?? base

    func hit(_ walked: Result<CardWalk, CardWalk.Miss>, shown: String) -> PathHit {
        switch walked {
        case .success(let walk):
            switch walk.place {
            case .card: return PathHit(ok: true, kind: "schema", type: walk.head.kind.rawValue)
            case .field(let field): return PathHit(ok: true, kind: "field", type: field.type.rawValue)
            case .formatRow(let names):
                return PathHit(ok: true, kind: "row", type: FormatRows.row(at: names, in: walk.head.rows)?.kind.rawValue ?? "row")
            case .node(let node): return PathHit(ok: true, kind: "node", type: node.kind)
            case .customRow(let names):
                return PathHit(ok: true, kind: "row",
                               type: CustomRows.row(at: CardWalk.rowPath(names, in: walk.head), in: walk.head.customRows)?.type ?? "row")
            }
        case .failure(let miss):
            let where_ = miss.fromHead.reduce(shown) { at, name in
                miss.holder.kind == .instance ? "\(at).\(name)" : PathText.member(at, name)
            }
            let there = miss.there.map(PathText.quote).joined(separator: " / ")
            switch miss.kind {
            case .formatRow:
                return PathHit(ok: false, hint: pathText(
                    miss.walked.isEmpty || FormatRows.row(at: miss.walked, in: miss.holder.rows)?.kind == .item
                        ? .Inspection.pathRowsInside(where_, there.isEmpty ? pathText(.Inspection.pathNoRows) : there)
                        : .Inspection.pathNotAnItem(where_)))
            case .node:
                return PathHit(ok: false, hint: pathText(
                    miss.fromHead.isEmpty
                        ? .Inspection.pathNodesIn(where_, there.isEmpty ? pathText(.Inspection.pathNoNodes) : there)
                        : .Inspection.pathIsANode(where_)))
            case .field:
                return PathHit(ok: false, hint: pathText(
                    .Inspection.pathFieldsUnder(where_, there.isEmpty ? pathText(.Inspection.pathNoFields) : there)))
            case .customRow:
                return PathHit(ok: false, hint: pathText(
                    .Inspection.pathRowsUnder(where_, there.isEmpty ? pathText(.Inspection.pathNoRows) : there)))
            }
        }
    }

    // Held to the trees the reader sees when it is a path written in a card.
    let found = directory.find(written.card, from: reader, seeing: from != nil)
    let head = found.schema
    let card = splitSection(written.card).rest
    // A function or a type inside a working directory's file: the window does not read the file to
    // draw a link (the check and the documents the core draws say one that is not there).
    if let head, WorkDirIds.isFile(head.id), !written.names.isEmpty {
        return PathHit(ok: true, kind: "symbol", type: "symbol")
    }
    if head == nil, intoUnreadWorkDir(detail, raw, directory: directory) {
        return PathHit(ok: true, kind: "schema", type: WorkDirIds.fileKind)
    }
    let absolute = head.map { hit(CardWalk.walk($0, written.names, cards: byId), shown: card) }
    if absolute?.ok == true { return absolute! }

    // Rules and payload exclusions are written from inside their own schema.
    if let from, splitSection(written.card).section == nil {
        let relative = hit(CardWalk.walk(from, [written.card] + written.names, cards: byId),
                           shown: PathText.quote(from.name))
        if relative.ok { return relative }
        // Found nowhere, and not inside its own card either: what the directory says of the
        // name — the sections that have it, and a card ghost to write it by its name alone —
        // is what helps, not what the card itself holds (as the core says it).
        if head == nil {
            if case .failure(let why) = found { return PathHit(ok: false, hint: why) }
            return relative
        }
    }

    if let absolute { return absolute }
    if case .failure(let why) = found { return PathHit(ok: false, hint: why) }
    return PathHit(ok: false, hint: missingSchema(card, among: directory.allQualified()))
}

/// How every caller says "no such schema". It lives here rather than in the tool
/// layer because the store needs it too.
///
/// `section`: where `names` were taken from, when it was one section and not the
/// whole project — the count is of that section's, and says so.
func missingSchema(_ wanted: String, among names: [String], in section: String? = nil) -> String {
    pathText(.Inspection.namesNoCard(wanted, nearestNames(wanted, among: names, total: names.count, in: section)))
}

/// Candidates worth showing: the ones that look like what was asked for, and a
/// count of the rest. `card_list`'s query is the tool for browsing; an error is not. Names are
/// held up to each other as they are, not as a path quotes them.
func nearestNames(_ wanted: String, among names: [String], total: Int, in section: String? = nil) -> String {
    let needle = PathText.unquote(wanted).lowercased()
    let close = names.filter {
        let name = PathText.unquote($0).lowercased()
        return name.hasPrefix(needle) || needle.hasPrefix(name) || name.contains(needle)
    }
    if let section {
        return pathText(close.isEmpty ? .Inspection.namesNoneCloseSection(total, section)
                                      : .Inspection.namesNearSection(total, section, cappedNames(close)))
    }
    return pathText(close.isEmpty ? .Inspection.namesNoneCloseProject(total)
                                  : .Inspection.namesNearProject(total, cappedNames(close)))
}

/// What a `[[…]]` in a text names: a section when written with its `:` and nothing after it
/// (`Store/Ordering:`), read from the writer's section as a card path's section is; else a card or
/// a place in one (`resolvePath`). A section is not a place a line, a path field or a rule may
/// name, so `resolvePath` stays cards only (the core's `resolve_place`).
func resolvePlace(_ detail: ProjectDetail, _ raw: String, from: SchemaNode? = nil,
                  base: String? = nil, directory given: SchemaDirectory? = nil) -> PathHit {
    let directory = given ?? detail.directory
    let reader = from.map { directory.sectionId(of: $0) } ?? base
    func section(_ name: String) -> ProjectSection? {
        reader.flatMap { directory.section(named: name, from: $0) } ?? directory.section(named: name)
    }
    if let name = PathText.sectionOnly(raw) {
        guard let found = section(name) else {
            if intoUnreadWorkDir(detail, raw, directory: directory) {
                return PathHit(ok: true, kind: "section", type: WorkDirIds.workdirKind)
            }
            return PathHit(ok: false, hint: pathText(.Inspection.pathNoSection(name)))
        }
        return PathHit(ok: true, kind: "section", type: found.kindName ?? found.kind?.rawValue)
    }
    let hit = resolvePath(detail, raw, from: from, base: base, directory: directory)
    let trimmed = raw.trimmingCharacters(in: .whitespaces)
    if !hit.ok, !PathText.hasSection(trimmed), section(trimmed) != nil {
        return PathHit(ok: false, hint: pathText(.Inspection.pathSectionColon(trimmed)))
    }
    return hit
}

/// Whether a path goes into a working directory this machine does not read: its section is there and
/// none of its files is. Not said missing, as the core does not say it — what its folder holds is not
/// known here.
func intoUnreadWorkDir(_ detail: ProjectDetail, _ raw: String, directory: SchemaDirectory) -> Bool {
    guard let sections = PathText.splitSection(PathText.withoutTree(raw.trimmingCharacters(in: .whitespaces))).section,
          let head = PathText.slashes(sections).first,
          let mount = directory.section(named: head),
          WorkDirIds.mount(of: mount.id) == mount.id else { return false }
    return !detail.schemas.contains { WorkDirIds.mount(of: $0.id) == mount.id }
}

/// At most a dozen, then a count. Long enough to correct a typo, short enough
/// to read.
func cappedNames(_ names: [String], limit: Int = 12) -> String {
    guard names.count > limit else { return names.joined(separator: " / ") }
    return names.prefix(limit).joined(separator: " / ")
        + " " + pathText(.Inspection.namesMore(names.count - limit))
}

/// A text of a path's miss, in the language the call being answered is answered in: an agent's call
/// in the agent's, the screen's otherwise. The texts are spliced into the answers of both.
func pathText(_ resource: LocalizedStringResource) -> String {
    L10n.text(resource, in: AppControl.answerLanguage)
}

/// Batch on purpose: a CI job extracts every path cited across a document set and
/// asks once.
///
/// There is deliberately no alias or rename history — a path that no longer exists
/// must fail, so the document gets fixed rather than quietly drifting.
func resolvePaths(_ detail: ProjectDetail, _ paths: [String], base: String? = nil) -> [PathResolution] {
    let directory = detail.directory
    return paths.map { raw in
        let path = raw.trimmingCharacters(in: .whitespaces)
        let hit = resolvePath(detail, path, base: base, directory: directory)
        return PathResolution(path: path, resolved: hit.ok,
                              kind: hit.kind, type: hit.type, hint: hit.hint)
    }
}

// MARK: - Paths the project asserts about itself

/// A fan wide enough that the list is the rule, not the relationships.
///
/// A discriminated pointer resolves itself: `ActivityLog.subject_refs` points at
/// whatever declares `record_type`, which in a project like that is every record.
/// Drawn one edge per candidate, that is nine lines out of nineteen all saying the
/// same sentence — and the spine of the model, `Store → Catalog →
/// Product`, sits at the same weight underneath them.
///
/// Four is where a handful stops being a list you read and starts being a set you
/// summarise. Below it the candidates are worth seeing individually; at or above it
/// they are one statement, and the diagram says so once.
let wideFanThreshold = 4

/// Group a field's outgoing relations, and say which groups are too wide to draw.
///
/// Keyed by the field rather than by the pointer's kind, because the fan is a
/// property of one field naming many targets — a discriminated pointer is the usual
/// cause, but several `pointsAt` entries on one field look the same to a reader.
func wideFans(_ relations: [Relation]) -> [String: Int] {
    var counts: [String: Int] = [:]
    // Schema-level kinds are excluded along with containment: they all share the
    // empty path, so counting them together would call a schema with four signed
    // payloads a fan and summarise away four separate keys.
    for relation in relations where fans(relation) {
        counts["\(relation.fromSchemaId)|\(relation.from)", default: 0] += 1
    }
    return counts.filter { $0.value >= wideFanThreshold }
}

func fanKey(_ relation: Relation) -> String {
    "\(relation.fromSchemaId)|\(relation.from)"
}

/// Whether this particular line is part of a fan wide enough to summarise.
///
/// Containment never is, even when it shares a field with one. `ActivityLog
/// .subject_refs` both *contains* a `TypedRecordRef` and *designates* thirty
/// records through it — one structural fact and one rule, on the same field.
/// Folding them together took the structure off the diagram along with the fan.
func isWideFan(_ relation: Relation, _ wide: [String: Int]) -> Bool {
    fans(relation) && wide[fanKey(relation)] != nil
}

/// Whether a line can be part of a fan at all. Not containment, not a line from the
/// card's header — a claim about the whole card, all sharing the empty row — and not a
/// document's mentions: a clause naming nine places says nine things, each worth its
/// line (overview §9).
private func fans(_ relation: Relation) -> Bool {
    relation.line != Line.embed && relation.line != Line.mentions && !relation.from.isEmpty
}

// MARK: - Names

func slugify(_ name: String) -> String {
    let lowered = name.trimmingCharacters(in: .whitespaces).lowercased()
    var out = ""
    var pendingDash = false
    for ch in lowered {
        if ch.isLetter || ch.isNumber {
            if pendingDash && !out.isEmpty { out.append("-") }
            pendingDash = false
            out.append(ch)
        } else {
            pendingDash = true
        }
    }
    return out.isEmpty ? "project" : out
}
