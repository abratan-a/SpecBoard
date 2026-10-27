import Foundation
import Synchronization

/// The vocabulary, ported from the web build's `src/shared/types.ts`.
///
/// Property names match the REST payload exactly wherever they can, so importing
/// a project is a plain `Codable` decode. The two places they cannot — `as` and `in`
/// are Swift keywords — carry explicit `CodingKeys`.

// MARK: - Scalars and kinds

enum ScalarType: String, Codable, CaseIterable, Sendable {
    case string, text, int, float, decimal, bool
    case uuid, url, email, date, datetime, json
}

/// `object` = a schema nested inside this one. `ref` = a pointer at another schema.
///
/// Stored as a plain string rather than a closed enum: the column is open in the
/// web build too, and a type this build has not heard of must survive a round trip
/// rather than fail the import.
struct FieldType: RawRepresentable, Codable, Hashable, Sendable {
    var rawValue: String
    init(rawValue: String) { self.rawValue = rawValue }
    init(from decoder: Decoder) throws {
        rawValue = try decoder.singleValueContainer().decode(String.self)
    }
    func encode(to encoder: Encoder) throws {
        var c = encoder.singleValueContainer()
        try c.encode(rawValue)
    }

    static let object = FieldType(rawValue: "object")
    static let ref = FieldType(rawValue: "ref")

    var isObject: Bool { self == .object }
    var isRef: Bool { self == .ref }
    var scalar: ScalarType? { ScalarType(rawValue: rawValue) }
}

enum SchemaKind: String, Codable, CaseIterable, Sendable {
    case entity, value, `enum`
    /// One thing the system does — 注文を受ける, 返金を審査する, 在庫を引き当てる. Not a shape: it has
    /// no fields and is never a generated type. What it has is what sets it off
    /// (`trigger`), its rows (`steps`) — what it reads and writes, the keys it signs
    /// and checks with, the actions it calls or sends to — and the notes that set its
    /// rules. The part of the system that does it is the section it sits in.
    ///
    /// `module` is its old name: a section already says which part of the system
    /// something belongs to, and what a module held was always one action's worth.
    /// A project or a caller saying `module` means this.
    case action
    /// A note pinned beside the cards, in any section: a body in Markdown — a memo, a
    /// change log. `[[path]]` in the body names a place in the project and is checked like
    /// any declared path; `{{path}}` stands for a value the project holds, put in wherever
    /// it is shown.
    case note
    /// A way through several parts — signing up, rotating a key: which actions run,
    /// in what order, and where it branches, waits and ends (`nodes`). It refers to
    /// actions and never makes them again; an action lives once, in its part's section.
    case flow
    /// Where records are kept — a table, a collection, a ledger, a file: what it
    /// `holds`. What an action reads and writes can name one.
    case store
    /// A document written freely, in a document section: a body in Markdown, one file
    /// when written out — a scenario, an essay. What it points at is drawn from the card.
    case freedoc
    /// A document in form, in a document section: items with headings and the rows
    /// inside them — text, tables, diagrams, the diagrams of a section, a card's table
    /// (`rows`) — one file when written out. What an item points at is drawn from it.
    case formatdoc
    /// A document of versions stacked one on another, in a document section
    /// (`specboard://SpecBoard/Docs/Design:CardKinds`「積み重ね文書」): a change log, release notes — the
    /// newest on top, one read at a time by its selector (`versions`, `StackVersion`), each a
    /// path of its own. Written out whole, or one version alone.
    case stackdoc
    /// A data card (`specboard://SpecBoard/Docs/Design:CardKinds`「データ（instance）と定義のセクション」): the values
    /// of a shape — an entity or a value, the line `of` goes to (`instanceOf`) — held as one JSON as
    /// nested as the shape (`data`). Drawn as its shape is, each field with its type's badge and its
    /// value.
    case instance
    /// A table of data cards (`specboard://SpecBoard/Docs/Design:CardKinds`「表（table）」): the data cards of a
    /// shape — the one its line `of` goes to (`shapeOf`) — gathered by its rules (`tableRules`) into a
    /// table, which the core builds (`tableMarkdown`). Drawn as that table, as a document is.
    case table
    /// A picture in SVG (`specboard://SpecBoard/Docs/Design:CardKinds`「SVG と HTML のカード」): its body, drawn
    /// as an image fitted to its box — read as an image, so nothing in it runs or loads.
    case svg
    /// A page in HTML, its CSS in it (the same design item): drawn at its box's width and cut at its
    /// box, under a policy that lets no script run and nothing load from outside.
    case html
    /// A frame in a ui section (`specboard://SpecBoard/Docs/Design:CardKinds`「UI のセクションとはめ込み」): another
    /// ui section's mock put into its box at 1:1, cut at the box. Its field `target` holds that section by
    /// id, so it follows a rename or a move; MCP reads and writes the section's path.
    case frame
    /// A card of a kind a person defines (`specboard://SpecBoard/Docs/Design:DesignBoard` §3): its fields, its
    /// rows and the lines its rows draw come from the kind's definition (`CustomKind`).
    /// Built in for now: the goal, a target and the tasks inside it, each its own card
    /// (`task`), shown as its rows.
    case goal
    /// One thing to do inside a goal: a body in Markdown — what it is, how far it got —
    /// and whether it is done. Shown in its goal as a row with a check.
    case task
    /// A card of a kind the project defines (`projects.kinds.cards`): which one
    /// is the card's `ownKind`, drawn and written by that definition (`CardKindDef`).
    case custom

    /// The kind a caller named, `module` included.
    static func named(_ raw: String) -> SchemaKind? {
        raw == "module" ? .action : SchemaKind(rawValue: raw)
    }

    /// A kind as a document names it: one built in, or else one the project defines (`custom`).
    init(from decoder: Decoder) throws {
        let raw = try decoder.singleValueContainer().decode(String.self)
        self = SchemaKind.named(raw) ?? .custom
    }

    /// The kinds a card can be made as, or turned into: every one but `action`, which
    /// is a flow now (`specboard://SpecBoard/Docs/Design:DesignActions`「action と flow の統合」). The core
    /// refuses one written as `action` (what it was is written as a flow).
    static var made: [SchemaKind] { allCases.filter { $0 != .action && $0 != .custom } }

    /// Why no action is made, and what is written instead.
    static var actionGone: String { L10n[.Model.errorActionGone] }

    /// Kinds that are not shapes: they have no fields and generate no types.
    var isShape: Bool { self == .entity || self == .value || self == .enum }
    /// Kinds whose content is rows of steps.
    var hasSteps: Bool { self == .action || self == .flow }
    /// An enum is a shape — it has a type — but its content is its values, and
    /// fields on one are shown nowhere.
    var holdsFields: Bool { self == .entity || self == .value }
    /// Kinds that are writing: a note and the documents.
    var isWriting: Bool { self == .note || self == .freedoc || self == .formatdoc || self == .stackdoc }
    /// Kinds whose content is one body of Markdown.
    var hasBody: Bool { self == .note || self == .freedoc }
    /// The documents: what a document section is for.
    var isDocument: Bool { self == .freedoc || self == .formatdoc || self == .stackdoc }
    /// A kind a person defines: drawn and checked by its definition (`CustomKind`).
    var isCustom: Bool { self == .goal }
    /// A kind the project defines (`custom`): drawn and written by its definition (`CardKindDef`).
    var isOwn: Bool { self == .custom }
    /// Kinds held inside another card (a goal's tasks): drawn as its rows, not on their own.
    var isNested: Bool { self == .task }
    /// Kinds whose body is a page of its own, not Markdown: drawn as it is, on a card and in its editor.
    var isPage: Bool { self == .svg || self == .html }
    /// The parts of a UI section's mock (html, svg, frame): the cards that carry a note and a review mark.
    var isUiPart: Bool { self == .html || self == .svg || self == .frame }
}

/// What follows an action, and when.
///
/// Several may follow one — a review that either issues a change or sends it back
/// — and a later one may lead back to an earlier one, or several into one. When
/// is written as a sentence: `[[path]]` in it names the field that decides, and is
/// checked like any other declared path.
struct ActionNext: Hashable, Sendable, Codable {
    var path: String
    var when: String?
}

/// One place a module reads or writes.
///
/// A path, not a schema: `StoreSettings` is all of it, and
/// `StoreSettings.catalog.categories[]` only what is under that — so a change
/// to `warehouse` does not count against a module that never looks there. A declared
/// path, like `pointsAt.in`: a rename that misses it shows in `brokenPaths`.
struct ModuleUse: Hashable, Sendable, Codable {
    enum Role: String, Codable, CaseIterable, Sendable {
        /// Parses it: a change means the reading code changes.
        case read
        /// Produces it: a change means what it puts out changes.
        case write

        var label: String { self == .read ? L10n[.Model.roleRead] : L10n[.Model.roleWrite] }
    }

    var path: String
    var role: Role
    /// Why, or what for — optional.
    var note: String?
}

nonisolated enum Cardinality: String, Codable, CaseIterable, Sendable {
    case oneToOne = "1:1"
    case oneToMany = "1:N"
    case manyToOne = "N:1"
    case manyToMany = "N:N"
}

// MARK: - Cardinality, and which half of it is a decision

/// Cardinality asks two independent questions, and only one is a design decision.
///
///     A : B
///     ↑   └─ how many of the target this field holds — `isArray` already says this
///     └───── how many of us may point at the same target — a real choice
///
/// The right-hand side therefore *usually* follows from `isArray` — but it is still
/// stored as written, because "usually" is not "always": an array of digests that
/// each designate one target is N:1, and one of this project's own fields says exactly
/// that. A call that states both and contradicts itself gets a warning; nothing is
/// rewritten. The left-hand side is a fact about instances that no amount of reading
/// schemas can settle, so unset stays unset and draws nothing.
func targetSide(isArray: Bool) -> String { isArray ? "N" : "1" }

func cardinalityFits(_ value: Cardinality, isArray: Bool) -> Bool {
    value.rawValue.hasSuffix(":\(targetSide(isArray: isArray))")
}

/// Keep the declared left side, correct the right side from `isArray`.
func alignCardinality(_ value: Cardinality?, isArray: Bool) -> Cardinality {
    let left = value.map { String($0.rawValue.prefix(1)) } ?? "N"
    return Cardinality(rawValue: "\(left):\(targetSide(isArray: isArray))") ?? .manyToOne
}

// MARK: - The kinds of line

/// The names of the kinds of line the core defines (`sources/core/src/kinds/builtin.json`,
/// `specboard://SpecBoard/Docs/Design:CardKinds`「線の種類の定義」): the one table of them. A line's look and the
/// words the legend says it in are its definition's (`LineKinds`), read with the project;
/// a project's own kinds of line (`digest`, `signedBy` …) come by name the same way.
nonisolated enum Line {
    /// The field's type is the card it reaches (前の `type: ref`).
    static let embed = "embed"
    /// The field's value names the card it reaches by its id.
    static let points = "points"
    /// A line of no rule: only that the two are joined.
    static let link = "link"
    /// What a store keeps.
    static let holds = "holds"
    /// A data card's shape: written as its key `of`, drawn as its header's word.
    static let shapeOf = "shapeOf"
    /// A flow's rules: a note, a document or an item of one.
    static let rules = "rules"
    /// What a note's or a document's text names (`[[ ]]`): read from the text, never written.
    static let mentions = "mentions"
    /// A card of main's or a branch's carried out in a working directory's file.
    static let implementedBy = "implementedBy"
    // A flow's nodes, between them: the way on, a branch's condition, the way when none
    // holds, when it fails, when it runs out of time.
    static let next = "next"
    static let condition = "condition"
    static let otherwise = "else"
    static let onError = "onError"
    static let onTimeout = "onTimeout"
    // A flow's nodes, out of the flow: what a run runs, the end a begin or a wait waits
    // for, what a row reads and writes, whom it sends to or hears from, the key it uses.
    static let runs = "runs"
    static let awaits = "awaits"
    static let reads = "reads"
    static let writes = "writes"
    static let partner = "partner"
    static let key = "key"

    /// The ways a flow goes from node to node, in the order a node's card says them.
    static let flowWays = [next, condition, otherwise, onError, onTimeout]
}

// MARK: - Rules

/// A rule the data must satisfy beyond its type.
///
/// `kind` is open on purpose: known kinds are validated and compiled into the
/// exporters, unknown ones are stored untouched and degrade to a comment. That way
/// a new rule can be recorded before anything knows how to generate it.
///
/// Expression-shaped params (`when`) are stored verbatim and never evaluated — this
/// tool records the rule, it does not interpret it.
struct Rule: Codable, Hashable, Sendable {
    var kind: String
    /// Everything besides `kind`, flattened into the same JSON object on the wire.
    var params: [String: JSONValue]
    /// The order the parameters go in: as given when a rule is made here; by name when it is read from
    /// the core, whose JSON a decoder reads without its order (the core keeps the order written, and
    /// writes it out itself).
    var paramOrder: [String]

    init(kind: String, params: [String: JSONValue] = [:], paramOrder: [String]? = nil) {
        self.kind = kind
        self.params = params
        self.paramOrder = paramOrder ?? params.keys.sorted()
    }

    /// Parameters in the order they were written.
    var orderedParams: [(key: String, value: JSONValue)] {
        paramOrder.compactMap { key in params[key].map { (key, $0) } }
    }

    private struct Key: CodingKey {
        var stringValue: String
        var intValue: Int? { nil }
        init?(stringValue: String) { self.stringValue = stringValue }
        init?(intValue: Int) { nil }
        init(_ s: String) { stringValue = s }
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: Key.self)
        kind = (try? c.decode(String.self, forKey: Key("kind"))) ?? ""
        var rest: [String: JSONValue] = [:]
        var order: [String] = []
        for key in c.allKeys where key.stringValue != "kind" {
            rest[key.stringValue] = try c.decode(JSONValue.self, forKey: key)
            order.append(key.stringValue)
        }
        params = rest
        paramOrder = order.sorted()
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: Key.self)
        try c.encode(kind, forKey: Key("kind"))
        for (k, v) in orderedParams { try c.encode(v, forKey: Key(k)) }
    }

    /// Convenience for the params that are almost always plain strings.
    func string(_ name: String) -> String? { params[name]?.stringValue }
}

/// Params each known kind requires. Anything else is accepted as-is.
struct KnownRule {
    var params: [String]
    var optional: [String] = []
    var note: String
}

/// Computed on each use: the notes follow the screen's language.
var knownRules: [String: KnownRule] {
    [
        "sortedBy": .init(params: [], optional: ["field", "order"], note: L10n[.Model.ruleSortedBy]),
        "uniqueBy": .init(params: [], optional: ["field"], note: L10n[.Model.ruleUniqueBy]),
        "requiredIf": .init(params: ["when", "then"], note: L10n[.Model.ruleRequiredIf]),
        "absentIf": .init(params: ["when", "then"], note: L10n[.Model.ruleAbsentIf]),
        "derivedFrom": .init(params: ["from"], optional: ["note"], note: L10n[.Model.ruleDerivedFrom]),
        "monotonicPerKey": .init(params: ["keys"], note: L10n[.Model.ruleMonotonicPerKey]),
        "chainedBy": .init(params: ["field"], note: L10n[.Model.ruleChainedBy]),
        "memberOf": .init(params: ["target"], note: L10n[.Model.ruleMemberOf]),
        "lteRef": .init(params: ["target"], note: L10n[.Model.ruleLteRef]),
        "gteRef": .init(params: ["target"], note: L10n[.Model.ruleGteRef]),
        "pattern": .init(params: ["regex"], note: L10n[.Model.rulePattern]),
    ]
}

// MARK: - Absences, tags

/// A field deliberately not present. Recorded because the absence is the design.
struct AbsentField: Codable, Hashable, Sendable {
    var name: String
    var reason: String
}

let tagMaxLength = 40
let tagMaxCount = 32
let tagGroupMaxCount = 12

/// A named bundle of tags.
///
/// Tags are flat and stay flat — a group does not change what a tag means, it just
/// gives a set of them one handle. A member may name another group, which is how a
/// bundle of bundles is built.
struct TagGroup: Codable, Hashable, Sendable {
    var name: String
    /// Tag names, or the names of other groups. Ordered; the UI follows this order.
    var tags: [String]
    /// Whether a schema must carry exactly one of these.
    ///
    /// Ordinary tags overlap on purpose — `管理画面` and `公開API` are
    /// both true of plenty of things. A layer does not: nothing is both a signed
    /// artifact and a database table. Saying so turns two mistakes into findings,
    /// and neither is reachable otherwise: two layers is a contradiction nobody
    /// would go looking for, and no layer cannot show up in any search for tags.
    ///
    /// Absent in anything written before this existed, hence the default.
    var exclusive: Bool = false

    /// Written out by hand because a default value on a property is *not* what the
    /// synthesised decoder falls back on — it throws `keyNotFound` instead. A
    /// `tag_groups` blob in the old form has no such key, so the generated version
    /// would refuse to open any project that holds one.
    init(name: String, tags: [String], exclusive: Bool = false) {
        self.name = name
        self.tags = tags
        self.exclusive = exclusive
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        name = try c.decode(String.self, forKey: .name)
        tags = try c.decode([String].self, forKey: .tags)
        exclusive = try c.decodeIfPresent(Bool.self, forKey: .exclusive) ?? false
    }
}

/// Resolve names to the tags they ultimately stand for, following nested groups.
func expandTags(groups: [TagGroup], selected: [String]) -> [String] {
    var out = Set<String>()
    func walk(_ names: [String], _ seen: Set<String>) {
        for name in names {
            guard let group = groups.first(where: { $0.name == name }) else {
                out.insert(name); continue
            }
            if seen.contains(name) { continue }   // a cycle resolves to what it reached
            walk(group.tags, seen.union([name]))
        }
    }
    walk(selected, [])
    return out.sorted()
}

// MARK: - The stored records

struct Project: Codable, Hashable, Identifiable, Sendable {
    var id: String
    var name: String
    var slug: String
    var description: String
    /// Named bundles of this project's tags. Purely a convenience for selecting them.
    var tagGroups: [TagGroup]
    var createdAt: String
    var updatedAt: String
    /// The language its exports say their own words in (`projects.export_language`), as the core reads
    /// it; none from a core of before, which wrote them in Japanese.
    var exportLanguage: String? = nil

    /// The language what it writes out (an export, a copied mermaid) says its own words in.
    var exportedIn: String {
        exportLanguage.flatMap { Language.supported.contains($0) ? $0 : nil } ?? "ja"
    }
}

/// Project plus the counts the navigation panel shows without loading each project.
struct ProjectSummary: Codable, Hashable, Identifiable, Sendable {
    var id: String
    var name: String
    var slug: String
    var description: String
    var tagGroups: [TagGroup]
    var createdAt: String
    var updatedAt: String
    var schemaCount: Int
    /// Out of the coding keys, like `SchemaNode.uses`: the web build's list has no
    /// such key, and one the decoder insisted on would refuse it.
    var sectionCount: Int = 1
    /// When it was set aside to be read and not written; nil for a project worked on.
    /// Out of the coding keys for the same reason as `sectionCount`.
    var archivedAt: String? = nil
    /// When main was locked (`specboard://SpecBoard/Docs/Design:DesignBranches` §8); nil when it is not. Out of
    /// the coding keys for the same reason.
    var lockedAt: String? = nil
    /// Whether an AI's merge into a locked main goes in without being approved (the project's
    /// settings). Out of the coding keys for the same reason.
    var aiMerge = false
    /// Where it is kept: nil for this Mac, else the host of the agent that keeps it
    /// (`RemoteHosts.Host.id`). Out of the coding keys: the app marks it as it reads the list.
    var place: UUID? = nil

    var isArchived: Bool { archivedAt != nil }
    var isLocked: Bool { lockedAt != nil }

    enum CodingKeys: String, CodingKey {
        case id, name, slug, description, tagGroups, createdAt, updatedAt, schemaCount
    }
}

struct FieldNode: Codable, Hashable, Identifiable, Sendable {
    var id: String
    var parentFieldId: String?
    var name: String
    var label: String
    var type: FieldType
    /// For `type == .ref`: the card whose shape this field's JSON is — where its card's
    /// embed line from this field goes (`connections`, a card ghost's id when written
    /// through one). Read from the lines, not kept with the field.
    var embeds: String? = nil
    var isArray: Bool
    var isRequired: Bool
    /// Whether `null` is a legal value. Orthogonal to `isRequired`: presence and
    /// nullability are different questions, and a strict wire format needs both.
    ///
    ///      required && !nullable  →  T
    ///      required &&  nullable  →  T | null
    ///     !required && !nullable  →  T | undefined
    var isNullable: Bool
    /// Fixed value this field always carries. `.null`/nil means "no constant".
    var constValue: JSONValue?
    var isUnique: Bool
    var isPk: Bool
    /// The value's rules in words: format, unit, range, what empty and null mean
    /// (`specboard://SpecBoard/Docs/Design:Overview` §5「書く場所」).
    var description: String
    var rules: [Rule]
    /// Discriminator values this field appears under. Empty means "always".
    var variants: [String]
    /// The other JSON types it also takes (`string`・`integer`・`number`・`boolean`・`object`, and
    /// `array` for a list of its own type), as "one or a list" is written. Out of the web build's
    /// project JSON, so optional: nil and empty are the same.
    var also: [String]? = nil
    var sortOrder: Int
    /// Populated for `type == .object` — the nested schema's own fields.
    var children: [FieldNode]
}

struct EnumValue: Codable, Hashable, Identifiable, Sendable {
    var id: String
    var value: String
    var label: String
    /// What the value stands for, in one line: shown on the pointer over its row, not on the map.
    var description: String = ""
    var sortOrder: Int

    private enum CodingKeys: String, CodingKey { case id, value, label, description, sortOrder }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        value = try c.decode(String.self, forKey: .value)
        label = try c.decode(String.self, forKey: .label)
        // A detail of a core that knows no descriptions says none.
        description = try c.decodeIfPresent(String.self, forKey: .description) ?? ""
        sortOrder = try c.decode(Int.self, forKey: .sortOrder)
    }

    init(id: String, value: String, label: String, description: String = "", sortOrder: Int) {
        self.id = id
        self.value = value
        self.label = label
        self.description = description
        self.sortOrder = sortOrder
    }
}

struct SchemaNode: Codable, Hashable, Identifiable, Sendable {
    var id: String
    var projectId: String
    var name: String
    var label: String
    var kind: SchemaKind
    var description: String
    /// Free-form labels used to slice the diagram.
    var tags: [String]
    /// Free-form key/value the tool never interprets — signer, purpose, source doc…
    var metadata: [String: String]
    var rules: [Rule]
    var absentFields: [AbsentField]
    /// Field name whose value selects which variant's fields apply.
    var discriminator: String?
    /// For enums: the listed values are the known ones, not the only ones.
    var enumOpen: Bool
    var posX: Double?
    var posY: Double?
    /// True once a human dragged the card — auto-layout stops moving it.
    var pinned: Bool
    var sortOrder: Int
    var fields: [FieldNode]
    var values: [EnumValue]
    /// For a module: where it reads and writes. Empty for every other kind.
    ///
    /// Out of the coding keys: it is kept in a table of this app's own, not in the
    /// project JSON the web build wrote, and a key the decoder insists on would stop
    /// every project written without it from opening.
    var uses: [ModuleUse] = []
    /// For an action: the keys it signs with and checks with — paths to them, like
    /// a payload's `signedBy` / `verifiedBy`, and drawn the same way.
    var signedBy: [String] = []
    var verifiedBy: [String] = []
    /// For an action: what follows it.
    var next: [ActionNext] = []
    /// For a note and a freedoc: the body, in Markdown.
    var body: String = ""
    /// For a formatdoc: its rows.
    var rows: [FormatRow] = []
    /// For an action: what starts it.
    var trigger: Trigger? = nil
    /// For an action: its rows, top to bottom.
    var steps: [Step] = []
    /// For a flow: its nodes — the cards inside it (`ProjectDetail.inner`), each read as the
    /// node it is (`FlowNode.init(card:)`): its fields and rows, and where its lines go.
    var nodes: [FlowNode] = []
    /// Its cell in `Default` when that is on cells (§12).
    var cell: FlowCell? = nil
    /// A flow's diagram as it is drawn unless asked otherwise: `sequence`, or a
    /// flowchart — none.
    var diagram: String?
    /// What a flow is drawn as unless asked otherwise.
    var diagramShape: String { diagram ?? "flowchart" }
    /// A writing card's own size on the diagram, in grid units (§12): nil for
    /// the project's card size. How it looks, kept with where it sits — out of the coding
    /// keys for the same reason as `uses`.
    var size: GridSize? = nil
    /// How a formatdoc is drawn on the diagram: `rows`, `document`, or nil for
    /// the project's default.
    var docView: String? = nil
    static let docViews = ["rows", "document"]
    /// For a stackdoc: its versions, the newest on top (`form.rows.versions`), and the one it has
    /// open by its name (how it looks, as `docView`: nil for the one on top). Out of the coding
    /// keys for the same reason as `uses`.
    var versions: [StackVersion] = []
    var version: String? = nil
    /// For a data card (`instance`): its values, as nested as its shape (`form.fields.data`), and its
    /// shape's name as its header says it (read with the project). Out of the coding keys for the
    /// same reason as `uses`.
    var data: JSONValue? = nil
    var shapeName: String? = nil
    /// For a data card its shape draws as a list of records: the rows the core worked out (`listFace` of
    /// `project_detail`'s card). Out of the coding keys for the same reason as `uses`.
    var listFace: ListFace? = nil
    /// For an entity or a value: its list face as written (`face`, `specboard://SpecBoard/Docs/Design:CardKinds`
    /// 「データの一覧の顔」), shown in the property panel, written over MCP. Out of the coding keys for the same
    /// reason as `uses`.
    var shapeFace: JSONValue? = nil
    /// For a table (`table`): its rules as its document keeps them (`within`・`where`・`columns`・`sort`・
    /// `unnest`), and the table they build now as Markdown (the core's, `tableMarkdown`). Out of the coding
    /// keys for the same reason as `uses`.
    var tableRules: [String: JSONValue] = [:]
    var tableMarkdown: String? = nil
    static let diagramShapes = ["flowchart", "sequence"]
    /// The section it is in. Nil reads as the project's first — which is every schema
    /// of a project built without sections, such as the fixtures. Out of the coding
    /// keys for the same reason as `uses`.
    var sectionId: String? = nil
    /// For a custom card (`specboard://SpecBoard/Docs/Design:DesignBoard` §3): its fields' values, by the
    /// field's name, and its rows. Out of the coding keys for the same reason as `uses`.
    var customFields: [String: String] = [:]
    var customRows: [CustomRow] = []
    /// The cards inside it, by id, in their order, for a container (a goal: its goals and
    /// tasks): the cards whose `in` is this one's (`WholeDetail`). A flow's are its nodes.
    var inside: [String] = []
    /// The container it is inside (a goal's id for a goal or task it holds): its name is its
    /// own there, and its path goes on from the container's (`Goals:Launch.Design`).
    var container: String? = nil
    /// Its lines, as its document keeps them (`specboard://SpecBoard/Docs/Design:CardKinds`「カードの線」): what the
    /// screen reads of where it points, besides `relations`. Written with `card_update`'s
    /// `connections`.
    var connections: [Connection] = []
    /// A goal's card map: where each card it holds sits on it, by the card's id, in grid
    /// units (`GoalCardMap`). Out of the coding keys for the same reason as `uses`.
    var itemPlaces: [String: GridPoint] = [:]
    /// A value object or an enum drawn on its own section's diagram as a card of its own,
    /// not only inside the cards holding it (`standalone`): how it
    /// looks, kept apart from the project — out of the coding keys for the same reason as `uses`.
    var standalone: Bool = false
    /// A card of a kind the project defines (`kind` is `custom`): the kind's
    /// name, its definition whole (`CardKindDef`), its fields and rows as its document keeps
    /// them (`form`), and what it holds that the definition no longer has (`strayFields`,
    /// 「定義に無い欄」). Out of the coding keys for the same reason as `uses`.
    var ownKind: String? = nil
    var kindDef: CardKindDef? = nil
    /// Its kind's definition, whatever the kind (a goal's too): what its rows are made of where it
    /// is drawn by its definition. Out of the coding keys for the same reason as `uses`.
    var definition: CardKindDef? = nil
    var ownFields: [String: JSONValue] = [:]
    var ownRows: [String: JSONValue] = [:]
    var strayFields: [String: JSONValue] = [:]
    /// A card whose kind says how it is drawn on a diagram (`face`; a task, or
    /// a kind the project defines with one): the kind's definition (`kinds.cards`) and the card's
    /// fields as its document keeps them (`form.fields`). Out of the coding keys for the same
    /// reason as `uses`.
    var faceKind: CardKindDef? = nil
    var faceValues: [String: JSONValue] = [:]
    /// Its fields that hold a section (`fieldtype: section`, a frame's `target`) as their sections' paths, by
    /// field: what its face writes for the id kept. None for a section that is gone. Out of the coding keys
    /// for the same reason as `uses`.
    var faceSections: [String: String] = [:]
    /// How many grid rows it takes, drawn by its face with Markdown under its fields and no size
    /// of its own: the core's, as it places the card — the page's height estimated from the text,
    /// the project's whole card at the most (`grid::face_rows`).
    /// Out of the coding keys for the same reason as `uses`.
    var faceRows: Int? = nil
    /// Its place among the cards of the container whose card map draws it, from 1 (`GoalCardMap`):
    /// a task's order in its goal (the order the goal holds them in). Nil
    /// elsewhere. Out of the coding keys for the same reason as `uses`.
    var mapNumber: Int? = nil
    /// The cards a UI part's lines reach that changed after it did (`review`), as the core read them; empty
    /// for a part with no mark and for every other card. Out of the coding keys for the same reason as `uses`.
    var review: [ReviewMark] = []
    /// The lines of this card to a working directory's files whose code changed since it was recorded, or was
    /// never recorded, on this machine (`codeReview` of `project_detail`'s card); empty for a card with none.
    /// Out of the coding keys for the same reason as `uses`.
    var codeReview: [CodeReviewLine] = []
    /// Whether the card wears the orange dot: a part with cards to look at again, or a card with an implementation
    /// line whose code changed.
    var showsReviewDot: Bool { !review.isEmpty || codeReview.contains(where: \.isChanged) }
    /// A working directory's file's functions and types as read so far (`ProjectDetail.withSymbols`), drawn
    /// as its card's rows; empty until they are read, and for every other card. Out of the coding keys for
    /// the same reason as `uses`.
    var symbols: [CodeSymbol] = []

    /// The kind as the project names it: the project's own kind's name, or the built-in kind's.
    var kindName: String { kind == .custom ? (ownKind ?? "custom") : kind.rawValue }

    /// How its kind draws it on a diagram, when it says (`face`).
    var face: CardFace? { faceKind?.face }

    /// Whether it may carry a size of its own (`specboard://SpecBoard/Docs/Design:Overview` §12): every
    /// boxed card. Not a flow's node on the flow's map (drawn by its node's shape), nor a working
    /// directory's file or folder (laid out by the window); a card ghost is drawn from the card it
    /// leads to and never sized itself.
    var sizable: Bool {
        !FlowCardMap.isCard(id) && ownKind != WorkDirIds.fileKind && ownKind != WorkDirIds.directoryKind
    }

    /// The text its face draws as Markdown below its fields (`face.markdown`): what is written
    /// there, empty when nothing is or its face draws none.
    var faceMarkdown: String {
        guard let name = face?.markdown else { return "" }
        if case .string(let text)? = faceValues[name] { return text }
        return name == "body" ? body : ""
    }

    /// Whether its face has Markdown to draw: the card then reads as a document under its
    /// fields, the project's whole card unless it has a size of its own — as a writing card's.
    var faceDrawsMarkdown: Bool {
        face?.markdown != nil && !faceMarkdown.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    /// The field its face draws as a page of its own (`face.svg`, `face.html`), and which: none when
    /// its face draws none.
    var facePage: (field: String, form: DocumentPage.Form)? {
        if let field = face?.svg { return (field, .svg) }
        if let field = face?.html { return (field, .html) }
        return nil
    }

    /// What its face's page holds: the field's text, empty when nothing is written.
    var facePageText: String {
        guard let field = facePage?.field else { return "" }
        if case .string(let text)? = faceValues[field] { return text }
        return field == "body" ? body : ""
    }

    /// Whether its face draws a page below its fields: Markdown that says anything, or an SVG or
    /// HTML page — drawn even with nothing written, as its box is what its picture fills.
    var faceDrawsPage: Bool { facePage != nil || faceDrawsMarkdown }

    enum CodingKeys: String, CodingKey {
        case id, projectId, name, label, kind, description, tags, metadata, rules,
             absentFields, discriminator, enumOpen, posX, posY, pinned, sortOrder, fields,
             values
    }
}

/// A card that changed after the UI part whose mark this is, and when (`review` of `project_detail`'s card).
nonisolated struct ReviewMark: Codable, Hashable, Sendable {
    /// The changed card's path in full.
    var path: String
    /// When it changed: ISO 8601, UTC.
    var at: String

    /// One line of the tooltip that says it, as the mock's dot says it.
    var line: String { "\(path) (\(at))" }
}

/// One implementation line of a card whose code is to be looked at: the line (`id`), the row it leaves (`from`,
/// empty for the head), the file or function it reaches as the tools write it (`to`) and how its code stands
/// against what was recorded when it was linked or last reviewed (`codeReview` of `project_detail`'s card).
nonisolated struct CodeReviewLine: Codable, Hashable, Sendable {
    enum State: String, Codable, Sendable {
        /// The code differs from what was recorded.
        case changed
        /// Nothing was recorded of it.
        case unrecorded
        /// What it points at is not in the code read on this machine (the function named anew, the file taken
        /// away): `why` says which.
        case broken
    }

    var id: String
    var line: String = ""
    var from: String = ""
    var to: String
    var state: State
    /// When what the code differs from was recorded: ISO 8601, UTC (`changed` only).
    var recordedAt: String?
    /// What is not there, in the core's words (`broken` only).
    var why: String?

    private enum CodingKeys: String, CodingKey { case id, line, from, to, state, recordedAt, why }

    init(id: String, line: String = "implementedBy", from: String = "", to: String, state: State, recordedAt: String? = nil,
         why: String? = nil) {
        self.id = id
        self.line = line
        self.from = from
        self.to = to
        self.state = state
        self.recordedAt = recordedAt
        self.why = why
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decodeIfPresent(String.self, forKey: .id) ?? ""
        line = try c.decodeIfPresent(String.self, forKey: .line) ?? ""
        from = try c.decodeIfPresent(String.self, forKey: .from) ?? ""
        to = try c.decode(String.self, forKey: .to)
        state = try c.decode(State.self, forKey: .state)
        recordedAt = try c.decodeIfPresent(String.self, forKey: .recordedAt)
        why = try c.decodeIfPresent(String.self, forKey: .why)
    }

    /// Whether it is the orange dot's: its code changed (one never recorded is said in the panel only).
    var isChanged: Bool { state == .changed }

    /// When it was recorded as a person reads it: the date and the minute, as the history says a time.
    var linkedAt: String? { recordedAt.map { $0.prefix(16).replacingOccurrences(of: "T", with: " ") } }

    /// What it says it reaches: the row it leaves, an arrow and the target, as the tooltip and the panel write it.
    var reached: String { from.isEmpty ? to : "\(from) → \(to)" }
}

/// One line of a card (`specboard://SpecBoard/Docs/Design:CardKinds`「カードの線」): of a kind of line by name, from
/// a row of the card (`from`, empty for its head; a node's row `steps[0]`) to a card or a
/// card ghost by id (`to`; none for a pointer told apart by its discriminator), landing in
/// it at `at` (its names there, or a path in full elsewhere).
nonisolated struct Connection: Codable, Hashable, Sendable {
    var id: String = ""
    var line: String
    var from: String = ""
    var to: String? = nil
    var at: String? = nil
    var description: String? = nil
    var fields: [String: JSONValue] = [:]

    init(line: String, from: String = "", to: String? = nil, at: String? = nil,
         description: String? = nil, fields: [String: JSONValue] = [:]) {
        self.line = line
        self.from = from
        self.to = to
        self.at = at
        self.description = description
        self.fields = fields
    }

    private enum CodingKeys: String, CodingKey { case id, line, from, to, at, description, fields }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decodeIfPresent(String.self, forKey: .id) ?? ""
        line = try c.decode(String.self, forKey: .line)
        from = try c.decodeIfPresent(String.self, forKey: .from) ?? ""
        to = try c.decodeIfPresent(String.self, forKey: .to)
        at = try c.decodeIfPresent(String.self, forKey: .at)
        description = try c.decodeIfPresent(String.self, forKey: .description)
        fields = try c.decodeIfPresent([String: JSONValue].self, forKey: .fields) ?? [:]
    }

    /// A text field of the line's (`when`, `outcome`, `use`).
    func text(_ key: String) -> String? {
        if case .string(let text)? = fields[key] { return text }
        return nil
    }
}

/// One line of a card, as the core reads every line of the project (`project_detail`'s
/// `relations`, `specboard://SpecBoard/Docs/Design:CoreApi`「読む」): from the card that holds it to the card it
/// reaches — through a card ghost, the card the ghost leads to (`viaGhostId` says the
/// ghost) — of a kind of line (`line`), from a row of the card (`from`, empty for its
/// head), landing at `at`. What a note's or a document's text names is one too
/// (`mentions`, with the path as `written`). The screen makes none of its own: it draws
/// these, put on the cards and rows it draws (`GraphEdges.placed`), where `from` and `at`
/// become the rows a line leaves and lands on.
nonisolated struct Relation: Codable, Hashable, Sendable {
    var fromSchemaId: String
    var toSchemaId: String
    /// The kind of line, by name (`Line`, or a project's own).
    var line: String
    /// Where it leaves: the names in the card (a field's, a node's row `steps[0]`), empty
    /// for its head. Drawn: the row it leaves from.
    var from: String = ""
    /// Where it lands in the card it reaches, when it says. Drawn: the row it lands on.
    var at: String? = nil
    /// The card ghost it was written through, when it was: the line is to the card the
    /// ghost leads to, and lands on the ghost where the ghost is drawn.
    var viaGhostId: String? = nil
    /// The field it leaves, for a line from one.
    var fieldId: String = ""
    var isArray: Bool = false
    /// Nil for a line that says none — see the cardinality note above.
    var cardinality: Cardinality? = nil
    /// The line's id in its card's `connections`; none for what a text names.
    var connection: String? = nil
    /// What a text names: the path as it is written there.
    var written: String? = nil
    /// A board's card's line, or its text, reaching one of main's cards: the card is in main's detail
    /// (`AppModel.outsideDetail`), not this one.
    var outside: Bool = false

    init(fromSchemaId: String, toSchemaId: String, line: String, from: String = "", at: String? = nil,
         viaGhostId: String? = nil, fieldId: String = "", isArray: Bool = false,
         cardinality: Cardinality? = nil, connection: String? = nil, written: String? = nil,
         outside: Bool = false) {
        self.fromSchemaId = fromSchemaId
        self.toSchemaId = toSchemaId
        self.line = line
        self.from = from
        self.at = at
        self.viaGhostId = viaGhostId
        self.fieldId = fieldId
        self.isArray = isArray
        self.cardinality = cardinality
        self.connection = connection
        self.written = written
        self.outside = outside
    }

    private enum CodingKeys: String, CodingKey {
        case fromSchemaId, toSchemaId, line, from, at, viaGhostId, fieldId, isArray, cardinality, connection, written,
             outside
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        fromSchemaId = try c.decode(String.self, forKey: .fromSchemaId)
        toSchemaId = try c.decode(String.self, forKey: .toSchemaId)
        line = try c.decode(String.self, forKey: .line)
        from = try c.decodeIfPresent(String.self, forKey: .from) ?? ""
        at = try c.decodeIfPresent(String.self, forKey: .at)
        viaGhostId = try c.decodeIfPresent(String.self, forKey: .viaGhostId)
        fieldId = try c.decodeIfPresent(String.self, forKey: .fieldId) ?? ""
        isArray = try c.decodeIfPresent(Bool.self, forKey: .isArray) ?? false
        // A cardinality this build does not know is no cardinality, not an unread project.
        cardinality = (try? c.decodeIfPresent(Cardinality.self, forKey: .cardinality)) ?? nil
        connection = try c.decodeIfPresent(String.self, forKey: .connection)
        written = try c.decodeIfPresent(String.self, forKey: .written)
        outside = try c.decodeIfPresent(Bool.self, forKey: .outside) ?? false
    }
}

struct ProjectDetail: Codable, Hashable, Sendable {
    var project: Project { didSet { stamp = ProjectStamp.next() } }
    var schemas: [SchemaNode] { didSet { stamp = ProjectStamp.next() } }
    var relations: [Relation] { didSet { stamp = ProjectStamp.next() } }
    /// In order. Empty for a project built in memory without any, which then behaves
    /// as a project with one — see `SchemaDirectory`.
    var sections: [ProjectSection] = [] { didSet { stamp = ProjectStamp.next() } }
    /// The root of the tree names are written from — a branch's, when reading one.
    /// Nil is main's.
    var context: String? = nil { didSet { stamp = ProjectStamp.next() } }
    /// Its card ghosts (`CardGhost`): kept apart from the cards, named with them.
    var ghosts: [CardGhost] = [] { didSet { stamp = ProjectStamp.next() } }
    /// The cards put only inside a container (a flow's nodes, `inner`): read as their flow's
    /// `nodes`, and kept as they are for what is written to each and where it sits.
    var inner: [InnerCard] = [] { didSet { stamp = ProjectStamp.next() } }
    /// The kinds of line the project has (`kinds.lines`): the built-in ones, then its own — what a
    /// line is called and how it is drawn.
    var lineKinds = LineKinds() { didSet { stamp = ProjectStamp.next() } }
    /// The kinds of card the project has (`kinds.cards`): the built-in ones, then its own — what
    /// a card of the project's own kind is drawn and written by.
    var cardKinds = CardKinds() { didSet { stamp = ProjectStamp.next() } }
    /// What each kind of section holds, by its name (`kinds.sections`): the kinds of card it
    /// takes, `*` for any.
    var sectionHolds: [String: [String]] = [:] { didSet { stamp = ProjectStamp.next() } }
    /// The mark and colour the sections of a kind the project defines carry in the nav panel, by the
    /// kind's name (`kinds.sections`'s `icon`・`color`).
    var sectionMarks: [String: SectionMark] = [:] { didSet { stamp = ProjectStamp.next() } }
    /// The kinds of section each kind's sections may have inside, by the kind's name (`kinds.sections`'s
    /// `sections`): the first is what a section made inside without a kind asked for is.
    var sectionInside: [String: [String]] = [:] { didSet { stamp = ProjectStamp.next() } }
    /// What each kind of section is called, by its name (`kinds.sections`'s `label`): a project's own
    /// kind's heading.
    var sectionLabels: [String: String] = [:] { didSet { stamp = ProjectStamp.next() } }
    /// What a flow's `via` may be, one list for a begin, a process node's rows and a run
    /// (`kinds.vias`): the built-in values, then those the project adds. The core holds it and
    /// checks a node by it as it writes.
    var vias: [String] = [] { didSet { stamp = ProjectStamp.next() } }
    /// Which contents these are: a new number whenever any of them changes, and the
    /// same for a copy that has not. What is worked out from the contents alone is
    /// kept with it (`built`). Not part of what a project is — left out of equality
    /// and of coding.
    var stamp = ProjectStamp.next() { didSet { built = BuiltDirectory() } }
    /// The directory built from these contents once it is asked for (`directory`): shared by the
    /// copies that have not changed, and let go of with the last of them. Not part of what a project
    /// is.
    var built = BuiltDirectory()

    enum CodingKeys: String, CodingKey { case project, schemas, relations }

    nonisolated static func == (a: ProjectDetail, b: ProjectDetail) -> Bool {
        a.project == b.project && a.schemas == b.schemas && a.relations == b.relations
            && a.sections == b.sections && a.context == b.context && a.ghosts == b.ghosts
            && a.inner == b.inner && a.lineKinds == b.lineKinds && a.cardKinds == b.cardKinds
            && a.sectionHolds == b.sectionHolds && a.sectionMarks == b.sectionMarks
            && a.sectionInside == b.sectionInside && a.sectionLabels == b.sectionLabels
    }

    nonisolated func hash(into hasher: inout Hasher) {
        hasher.combine(project)
        hasher.combine(schemas)
        hasher.combine(relations)
        hasher.combine(sections)
        hasher.combine(context)
        hasher.combine(ghosts)
        hasher.combine(inner)
    }
}

/// Numbers for `ProjectDetail.stamp`, never the same twice.
nonisolated enum ProjectStamp {
    private static let last = Atomic<Int>(0)

    static func next() -> Int { last.add(1, ordering: .relaxed).newValue }
}

/// One commit of a branch: what one operation changed, and who made it
/// (`specboard://SpecBoard/Docs/Design:DesignBranches` §7). Its lines are what was done, one each.
struct CommitInfo: Hashable, Identifiable, Sendable {
    var id: String
    /// Numbered within its branch, from 1.
    var seq: Int
    var at: String
    /// `screen` or `mcp`, and the access key's name for one made through a key.
    var authorKind: String
    var author: String?
    /// `edit`, `merge`, `revert`, `batch`, `export`, `branch.*`, or `record` for a line
    /// of the history from before commits, which keeps nothing to go back to.
    var kind: String
    /// A name given by hand: a mark on the history.
    var name: String?
    /// Undone by a later revert.
    var reverted: Bool
    var lines: [ChangeEvent]

    /// Who made it, as the history shows it.
    var authorLabel: String {
        authorKind == "mcp" ? author.map { L10n[.Model.authorMcp($0)] } ?? "MCP" : L10n[.Model.authorScreen]
    }

    /// A mark on the history: a merge, an export, a name.
    var isMark: Bool { kind == "merge" || kind == "export" || name != nil }
}

/// What one commit changed (`specboard://SpecBoard/Docs/Design:DesignBranches` §7): each card and section it
/// wrote, as it was before against as it was after — the next commit's before, or now.
struct CommitChange: Hashable, Sendable {
    enum Change: String, Hashable, Sendable { case add, change, remove }
    var path: String
    var change: Change
    var section: Bool
    /// What changed in it, said as a branch's diff says it.
    var details: [String]
}

/// How much of the store a project takes (`specboard://SpecBoard/Docs/Design:DesignBranches` §7): its documents,
/// and its history per branch, in bytes — the lengths of what the rows hold.
struct ProjectSize: Hashable, Sendable {
    struct Branch: Hashable, Sendable {
        /// nil for main.
        var id: String?
        var name: String
        var history: Int
    }
    var content: Int
    /// main first.
    var branches: [Branch]

    var history: Int { branches.reduce(0) { $0 + $1.history } }

    /// "中身 0.6 MB・履歴 3.2 MB（main 0.4 MB・refunds 2.8 MB）".
    var label: String {
        let shown = branches.filter { $0.history > 0 }
        guard shown.count > 1 else { return L10n[.Model.sizeLabel(Self.amount(content), Self.amount(history))] }
        let each = shown.map { "\($0.name) \(Self.amount($0.history))" }.joined(separator: L10n[.Common.separatorList])
        return L10n[.Model.sizeLabelByBranch(Self.amount(content), Self.amount(history), each)]
    }

    /// Bytes as a person reads them: MB from a tenth of one, KB below.
    static func amount(_ bytes: Int) -> String {
        if bytes >= 100_000 {
            let mb = Double(bytes) / 1_000_000
            return String(format: mb >= 10 ? "%.0f MB" : "%.1f MB", mb)
        }
        return bytes >= 1_000 ? "\(Int((Double(bytes) / 1_000).rounded())) KB" : "\(bytes) B"
    }
}

struct ChangeEvent: Codable, Hashable, Identifiable, Sendable {
    enum TargetType: String, Codable, Sendable {
        case project, schema, field, enumValue, layout, section
    }
    var id: String
    var projectId: String
    var at: String
    var kind: String
    var targetType: TargetType
    var targetId: String?
    var summary: String
}

// MARK: - Nesting

/// Which cards unfold the value objects and enums they hold, instead of drawing a
/// line to them: a base, and the cards switched away from it. A card added later
/// follows the base.
///
/// The app draws with `always` — every card unfolds them — and offers no switch.
/// The base-and-exceptions form is what the filters carry (`GraphFilters.nesting`)
/// and a card's rows are measured by, so a caller can still ask for cards drawn
/// another way.
nonisolated struct Nesting: Hashable, Sendable {
    var base: Bool = false
    var flipped: Set<String> = []

    /// What the diagram draws: every card holds its value objects and enums in
    /// place, and they have no card of their own unless placed as a card ghost. There is
    /// no switch between the two.
    static let always = Nesting(base: true)

    func nests(_ schemaId: String) -> Bool { base != flipped.contains(schemaId) }

    /// Everything on, or everything off.
    mutating func setAll(_ on: Bool) {
        base = on
        flipped = []
    }

    mutating func toggle(_ schemaId: String) {
        if flipped.contains(schemaId) { flipped.remove(schemaId) } else { flipped.insert(schemaId) }
    }

    enum State: Equatable { case on, off, mixed }

    /// Over the cards it could change. A card with nothing to unfold says nothing.
    func state(over ids: some Sequence<String>) -> State {
        var on = false, off = false
        for id in ids {
            if nests(id) { on = true } else { off = true }
            if on && off { return .mixed }
        }
        return on ? .on : (off ? .off : (base ? .on : .off))
    }
}

/// Whether a card has anything that its nesting would change: a reference to a
/// value object or an enum, at any depth of its own fields.
func canNest(_ schema: SchemaNode, lookup: SchemaLookup) -> Bool {
    var found = false
    walkFields(schema.fields) { field, _, _ in
        guard !found, field.type.isRef, let id = field.embeds,
              let target = lookup(id), target.kind != .entity else { return }
        found = true
    }
    return found
}

