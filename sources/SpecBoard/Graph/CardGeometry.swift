import Foundation

/// Card metrics, ported from `src/client/layout/geometry.ts`.
///
/// In the web build these had to agree with what CSS actually rendered, and a
/// browser test measured the DOM to prove it. Here the same numbers *are* the
/// rendering, so there is nothing to disagree with — but they stay identical to
/// the web build's so an imported layout lands where it was left.
enum CardGeometry {
    static let cardWidth: CGFloat = 352
    static let rowHeight: CGFloat = 27
    /// Border-box, so the header's own bottom border is already inside this.
    static let headerHeight: CGFloat = 40
    static let listPadding: CGFloat = 12    // 6 top + 6 bottom
    /// The strip saying a document's text goes on past its picture: one row of the grid, so a card with one
    /// ends on a grid line as one without does.
    static let moreHeight: CGFloat = 27
    /// Half a row, kept above and below the rows of a card with a window whether rows are out of sight that way
    /// or not: the count of those that are goes in it, so the window holds as many rows and the card is as tall
    /// wherever it is scrolled, and the two halves make one row of the grid.
    static let bandHeight: CGFloat = 13.5
    /// The fewest grid rows a card whose rows scroll inside a height of its own is drawn in: its border, header
    /// and padding take two, one row and the half rows above and below it the other two.
    static let leastGridRows = 4
    /// The card's own 1px border, top and bottom.
    static let cardBorder: CGFloat = 1
    /// How many rows a card draws before the rest goes out of the window.
    ///
    /// Fifteen, which with the half rows above and below them puts the tallest card at 486 points.
    /// Height is what decides how many cards can be arranged side by side and
    /// still be worth zooming into, and a card is not a fixed window on its
    /// schema: what falls outside can be scrolled to, and a deep branch can be
    /// folded away.
    static let standardRows = 15
    /// Set by the canvas from the layout on screen (`LayoutSettings`, §12).
    static var maxRows = standardRows
    /// Where a list face's row's disclosure mark starts, from the card's left edge: past the code dot.
    static let listDiscloseLeft: CGFloat = 12
    /// The disclosure mark's box, in the gap the indent leaves to a name's left.
    /// Also the width of the strip a click has to land in to fold a row.
    static let discloseWidth: CGFloat = 11
    /// The narrowest a row's number badge is, which is the width of the column the numbers of a
    /// numbered list sit in (from the same indent as any depth-0 row).
    static let numberWidth: CGFloat = 16
    /// How deep to inline reused value types before giving up and showing the name.
    static let maxInlineDepth = 3
}

/// One line inside a card — a field, or an enum value.
struct CardRow: Identifiable, Hashable {
    /// Dotted path from the card's root, so two inlined copies stay distinct.
    var path: String
    var depth: Int
    var field: FieldNode?
    var value: EnumValue?
    /// Set when the row comes from a reused value type rather than the card's own
    /// fields — `Address` inlined into `Customer.shipping_address`, say.
    var fromType: String?
    /// The name of what a `ref` row points at, resolved while the lookup is at hand.
    /// The renderer has no way to resolve an id, and needs the name for a target
    /// that has no card to draw a line to.
    var refTargetName: String?
    /// Whether this row has anything under it — nested fields, or a value object
    /// unfolded in place. Only these get a disclosure mark and answer a click on it.
    var expandable: Bool = false
    /// Whether its contents are currently folded away.
    var folded: Bool = false
    /// Whether what it unfolds is a value object's or an enum's — another schema,
    /// drawn in place. These start folded: a card reads as its own fields first.
    var unfoldsType: Bool = false
    /// On an action's or a flow's card, a step — or its trigger — and on a store's,
    /// what it holds. What a row's lines reach is the project's lines' (`GraphEdges.placed`).
    var step: StepRow?
    /// A custom card's row drawn as a form: a switch on its right, or a radio
    /// field's options side by side — pressed on the diagram, it changes.
    var form: CardForm.Control? = nil
    /// Its place in the order it is done in, from 1, drawn before what it says: a goal's row
    /// (a task's order is its place in its goal).
    var number: Int? = nil
    /// Folded as the card first shows, as a value object is (`restingFolds`): a container's row 「済み」.
    var startsFolded: Bool = false
    /// The rows it stands for while they are folded into it: their lines leave from it
    /// (`CardIndex`) — a container's cards done, under 「済み」.
    var stands: [String] = []
    /// Its disclosure mark sits in the column the numbers of the rows around it sit in: a group
    /// that stands among numbered rows, where a mark at the card's border would leave that column
    /// looking empty.
    var disclosesInNumbers: Bool = false
    /// A data card's row: its field's name, its type's badge and its value (`InstanceRows`).
    var datum: InstanceDatum? = nil
    /// A working directory's file's row: one of its functions or types, its mark, its name and its line.
    var symbol: CodeSymbol? = nil
    /// A data card's row drawn by its shape's list face: an element's, or a line its fold opens to (`ListFaceRows`).
    var listed: ListedRow? = nil

    var id: String { path }

    /// Where its disclosure mark's box starts, across, in a card whose left edge is at `minX`. The
    /// drawing and the click on it read this one place.
    func discloseLeft(from minX: CGFloat) -> CGFloat {
        // An element's row: right of the dot that marks its code changed, which sits at the card's edge.
        if case .entry? = listed { return minX + CardGeometry.listDiscloseLeft }
        let indent = 12 + CGFloat(depth) * 13
        if disclosesInNumbers {
            return minX + 12 + (CardGeometry.numberWidth - CardGeometry.discloseWidth) / 2
        }
        return minX + indent - CardGeometry.discloseWidth
    }
}

struct CardContent: Hashable {
    /// What the card draws — at most `maxRows`.
    var rows: [CardRow]
    /// How many fell past the cut, above and below together. Zero when everything fits.
    var hidden: Int
    /// Index in `all` of the first drawn row. Zero unless the card is scrolled.
    var first: Int
    /// Every row, cut or not. Edges are read off *this*.
    var all: [CardRow]
    /// How many rows the window holds wherever it is: the project's `maxRows`, or as many as fit a card of its own
    /// height between its half rows.
    var fit: Int = CardGeometry.maxRows
    /// Drawn as the document it is — a picture of its whole text — rather than as
    /// rows: `rows` is empty, `all` still holds what its lines are read off,
    /// and those lines leave from the header.
    var document = false
    /// Whether the rows sit between half rows kept for the counts (`CardGeometry.bandHeight`): a card of its own
    /// height always, whether its rows fit or not, since its height is set; any other card once it has more rows than
    /// the project's `maxRows`, and so a window, while one whose rows fit grows to hold them and keeps none.
    var bands = false

    /// Rows out of sight above and below. `hidden` is their sum.
    var above: Int { first }
    var below: Int { document ? 0 : all.count - (first + rows.count) }
    /// The furthest a card may be scrolled: enough to bring the last row into view.
    var maxScroll: Int { document ? 0 : max(0, all.count - fit) }
    /// Whether a count is drawn in the half row at each end: only for a direction that has rows out of sight.
    var hasAboveStrip: Bool { above > 0 }
    var hasBelowStrip: Bool { below > 0 }

    /// The rows of `all` the window holds with its first row at `from`, kept in range.
    func window(from: Int) -> Range<Int> {
        guard !document else { return 0..<0 }
        guard all.count > fit else { return 0..<all.count }
        let first = min(max(0, from), all.count - fit)
        return first..<(first + fit)
    }

    /// The same rows windowed from `from`.
    func scrolled(to from: Int) -> CardContent {
        guard !document else { return self }
        let shown = window(from: from)
        var out = self
        out.rows = Array(all[shown])
        out.first = shown.lowerBound
        out.hidden = all.count - shown.count
        return out
    }

    /// Where the window starts so that the row at `index` and the rows it opened, to `last`, show as far as
    /// they fit: the row pressed stays in the window, and of the starts that show as many of its rows, the one
    /// that moves least — none when they already show.
    func first(showing index: Int, through last: Int) -> Int {
        guard !document, all.indices.contains(index), last > index else { return first }
        var best = first, most = -1
        for from in min(first, index)...max(first, index) {
            let shown = window(from: from)
            guard shown.contains(index) else { continue }
            let seen = min(last, shown.upperBound - 1) - index
            if seen > most { (best, most) = (shown.lowerBound, seen) }
            if seen == last - index { break }
        }
        return best
    }
}

typealias SchemaLookup = (String) -> SchemaNode?

extension SchemaNode {
    /// The row a formatdoc's rows at the top draw from: the card itself.
    static let topRow = "#top"

    /// A flow's node's row (`#n.<node>`), the name quoted where it needs it, as the core writes a
    /// line's row.
    static func nodeRow(_ name: String) -> String { "#n." + PathText.quote(name) }

    /// A process node's row under its node's (`#n.<node>.<n>`).
    static func stepRow(_ name: String, _ index: Int) -> String { "\(nodeRow(name)).\(index)" }

    /// A node's row read back: the node's name, and the place of a process node's row under it;
    /// none for a row of no node.
    static func rowNode(_ row: String) -> (name: String, step: Int?)? {
        guard row.hasPrefix("#n.") else { return nil }
        let parts = PathText.dots(String(row.dropFirst(3)))
        switch parts.count {
        case 1: return (PathText.unquote(parts[0]), nil)
        case 2: return Int(parts[1]).map { (PathText.unquote(parts[0]), $0) }
        default: return nil
        }
    }
}

/// Rows a graph card draws.
///
/// `inlineReuse` treats a reference to a value object as what it actually is —
/// structure, not a relationship. `Customer.shipping_address` is not a pointer at an
/// Address somewhere; the field simply has Address's shape. Drawing that as an
/// edge buries the few references that really do connect one document to another,
/// so the shape is unfolded in place and no line is drawn.
/// `from` is the first row to draw. A card taller than `maxRows` shows a window on
/// its fields rather than the first few and a count — 500 fields behind a `+485` is a
/// schema the diagram cannot say anything about.
///
/// `tree`: the tree the card is in (`main`, a branch's name) — the paths its rows
/// show are without it, as the tree being looked at (`specboard://SpecBoard/Docs/Design:DesignSections` §8).
func cardRows(
    _ schema: SchemaNode,
    lookup: SchemaLookup? = nil,
    inlineReuse: Bool = false,
    from: Int = 0,
    folded: Set<String> = [],
    tree: String? = nil,
    form: CardForm = CardForm()
) -> CardContent {
    var all: [CardRow] = []
    func window(_ rows: [CardRow], from: Int) -> CardContent {
        let shown = rows.map { row in
            var row = row
            row.step = row.step?.shown(in: tree)
            return row
        }
        if form.document { return CardContent(rows: [], hidden: 0, first: 0, all: shown, fit: 0, document: true) }
        return cardWindow(shown, from: from, height: form.drawnHeight)
    }

    // A working directory's file, and a folder's box: its mark and its name, nothing under them.
    if CompactBox.isCompact(schema) { return CardContent(rows: [], hidden: 0, first: 0, all: [], fit: 0) }
    // A file whose functions and types are listed: a row each, in the file's order — what is in a type
    // right under it, set in — each by its path in the file, where a line to it lands.
    if schema.isCodeFile {
        return window(schema.symbols.map { symbol in
            CardRow(path: symbol.path, depth: symbol.depth, field: nil, value: nil, fromType: nil, symbol: symbol)
        }, from: from)
    }
    // A node of a flow, drawn as a card of its own in the flow's card editor.
    if FlowCardMap.isCard(schema.id), let node = schema.nodes.first {
        return window(FlowCardMap.rows(node), from: from)
    }
    // A stackdoc: what it is and the selector of its versions above, and below
    // them the version open, drawn as a document (`StackRows`).
    if schema.kind == .stackdoc {
        let top = StackRows.rows(schema)
        if form.document { return CardContent(rows: top, hidden: 0, first: 0, all: top, fit: top.count, document: true) }
        return cardWindow(top, from: from, height: form.drawnHeight)
    }
    // A card whose kind says how it is drawn (`face`; a task): its fields
    // above, each a control pressed on the diagram, and below them its Markdown, drawn as a
    // writing card's body is — the card a document under its fields (`form.document`).
    if let face = schema.face, let def = schema.faceKind {
        let top = FaceRows.rows(schema, face: face, def: def).map { row in
            var row = row
            row.step = row.step?.shown(in: tree)
            return row
        }
        if form.document { return CardContent(rows: top, hidden: 0, first: 0, all: top, fit: top.count, document: true) }
        return cardWindow(top, from: from, height: form.drawnHeight)
    }
    // A task on its goal's card map, its kind's face not read: its title with its check. A goal
    // there is drawn as everywhere (below).
    if GoalCardMap.isCard(schema.id), schema.kind == .task {
        return window(GoalCardMap.rows(schema), from: from)
    }

    // A card of a kind the project defines: drawn from its definition — its
    // fields not in its header, a choice as its options side by side and a check as a switch
    // (pressed on the diagram, they change), the rest a row each when written; then its rows,
    // nested as written and foldable, each with its check, its lines leaving from the row the
    // core names (`steps.a`).
    if schema.kind == .custom, let def = schema.kindDef {
        for field in def.ownFields where !def.header.contains(field.name) && field.show != "editor" {
            let path = OwnRows.fieldRow + field.name
            let value = schema.ownFields[field.name]
            switch field.fieldtype {
            case "radio":
                all.append(CardRow(path: path, depth: 0, field: nil, value: nil, fromType: nil,
                                   step: StepRow(label: field.label, text: "", detail: nil),
                                   form: .segments(field: field.name, options: field.options,
                                                   chosen: value?.asText ?? field.defaultValue?.asText ?? "",
                                                   labels: field.optionLabels)))
            case "checkbox":
                all.append(CardRow(path: path, depth: 0, field: nil, value: nil, fromType: nil,
                                   step: StepRow(label: "", text: field.label, detail: nil),
                                   form: .toggle(on: value == .bool(true))))
            default:
                guard let value else { continue }
                let text = OwnFieldText.short(value)
                guard !text.isEmpty else { continue }
                all.append(CardRow(path: path, depth: 0, field: nil, value: nil, fromType: nil,
                                   step: StepRow(label: field.label, text: text, detail: nil)))
            }
        }
        let rows = OwnRows.walk(def, schema.ownRows)
        var hiddenUnder: String?
        for (index, row) in rows.enumerated() {
            if let under = hiddenUnder, row.path.hasPrefix(under + ".") { continue }
            hiddenUnder = nil
            let expandable = rows.indices.contains(index + 1) && rows[index + 1].depth > row.depth
            let isFolded = expandable && folded.contains(row.path)
            if isFolded { hiddenUnder = row.path }
            // What else the row says, after its title: its fields but its title and its check.
            let more = row.kind.fields
                .filter { $0.name != row.kind.titleField && $0.name != row.kind.check }
                .compactMap { field in row.fields[field.name].map(OwnFieldText.short) }
                .filter { !$0.isEmpty }
            all.append(CardRow(path: row.path, depth: row.depth, field: nil, value: nil, fromType: nil,
                               expandable: expandable, folded: isFolded,
                               step: StepRow(label: row.depth == 0 ? row.kind.label : "", text: row.title,
                                             detail: more.isEmpty ? nil : more.joined(separator: " · ")),
                               form: row.checked.map { .toggle(on: $0) }))
        }
        // A container: then the cards inside it, each as its kind's `as_row` says.
        if def.isContainer {
            all += ContainerRows.rows(schema.customRows, prefix: ContainerRows.prefix, folded: folded) { _ in nil }
        }
        return window(all, from: from)
    }

    if schema.kind == .enum {
        for v in schema.values {
            all.append(CardRow(path: v.value, depth: 0, field: nil, value: v, fromType: nil))
        }
        return window(all, from: from)
    }

    /// What a line of the card reaches, as a row says it: the card's name (a card ghost's,
    /// through one) and where it lands.
    func reached(_ line: Connection) -> String {
        let name = line.to.flatMap { lookup?($0)?.name } ?? L10n[.Diagram.cardNoTarget]
        guard let at = line.at, !at.isEmpty else { return name }
        return PathText.hasSection(at) ? at : "\(PathText.quote(name)).\(at)"
    }

    // An action's and a flow's rows are their steps, top to bottom and nested as
    // written, after what starts them; a flow's its nodes (its rules leave its head, with no
    // row); a store's what it holds — a row each of its lines of that kind, its line leaving
    // from there.
    if schema.kind.hasSteps || schema.kind == .store {
        if let trigger = schema.trigger {
            all.append(CardRow(path: "#t", depth: 0, field: nil, value: nil, fromType: nil,
                               step: StepRow(trigger)))
        }
        func emit(_ steps: [Step], _ depth: Int, _ prefix: String) {
            for (index, step) in steps.enumerated() {
                let path = "\(prefix).\(index)"
                all.append(CardRow(path: path, depth: depth, field: nil, value: nil, fromType: nil,
                                   step: StepRow(step)))
                emit(step.steps, depth + 1, path)
                for (caseIndex, item) in step.cases.enumerated() {
                    let casePath = "\(path).c\(caseIndex)"
                    all.append(CardRow(path: casePath, depth: depth + 1, field: nil, value: nil,
                                       fromType: nil, step: StepRow(case: item)))
                    emit(item.steps, depth + 2, casePath)
                }
            }
        }
        emit(schema.steps, 0, "#p")
        // A flow's nodes, the way they go: what each is, and where it leads.
        for node in schema.nodes {
            all.append(CardRow(path: SchemaNode.nodeRow(node.name), depth: 0, field: nil, value: nil, fromType: nil,
                               step: StepRow(node)))
            // A process node's rows below it, each drawing its own lines.
            for (index, step) in node.steps.enumerated() where node.kind == "process" {
                all.append(CardRow(path: SchemaNode.stepRow(node.name, index), depth: 1, field: nil, value: nil, fromType: nil,
                                   step: StepRow(step)))
            }
        }
        if schema.kind == .store {
            let own = schema.connections.filter { $0.from.isEmpty }
            for (index, line) in own.filter({ $0.line == Line.holds }).enumerated() {
                all.append(CardRow(path: "#h\(index)", depth: 0, field: nil, value: nil, fromType: nil,
                                   step: StepRow(label: BuiltinWords.line(Line.holds), text: reached(line), detail: nil)))
            }
        }
        return window(all, from: from)
    }

    // A custom card (`specboard://SpecBoard/Docs/Design:DesignBoard` §3): the fields not in its header, a row
    // each when written; then its rows, nested and foldable, each with what it is
    // joined to leaving from it — from the folded row, what is folded inside it. A field said
    // by its label (状態), written by its name (status). A goal's rows — the
    // cards inside it — numbered in the order they are done in (their place in it).
    if let custom = CustomKind.of(schema.kind) {
        // Its fields as its kind's definition says them (the built-in `goal`'s, read from the project's
        // kinds), a row each when written but those in its header, which the head says
        // (`customKindWord`): a choice a row of buttons, the rest its text.
        let fields: [(name: String, label: String, fieldtype: String, options: [String], labels: [String: String])] =
            schema.definition.map { def in
                def.ownFields.filter { $0.show != "editor" }.map {
                    ($0.name, $0.label.isEmpty ? $0.name : $0.label, $0.fieldtype, $0.options, $0.optionLabels)
                }
            } ?? custom.fields.map { ($0.name, $0.shown, $0.fieldtype, $0.options, $0.optionLabels) }
        let header = schema.definition?.header ?? custom.header
        for field in fields {
            guard !header.contains(field.name), let value = schema.customFields[field.name], !value.isEmpty else { continue }
            if field.fieldtype == "radio" {
                all.append(CardRow(path: "#f.\(field.name)", depth: 0, field: nil, value: nil, fromType: nil,
                                   step: StepRow(label: field.label, text: "", detail: nil),
                                   form: .segments(field: field.name, options: field.options,
                                                   chosen: value, labels: field.labels)))
                continue
            }
            all.append(CardRow(path: "#f.\(field.name)", depth: 0, field: nil, value: nil, fromType: nil,
                               step: StepRow(label: field.label, text: value, detail: nil)))
        }
        all += ContainerRows.rows(schema.customRows, prefix: "", folded: folded) { CustomRows.progress($0, counting: custom.counts) }
        return window(all, from: from)
    }

    // A formatdoc: a row for each item, nested as written, what its rows point at
    // leaving from it — and one for the rows at the top, when they point at anything.
    if schema.kind == .formatdoc {
        // The rows at the top, when they name anything: what they name leaves from there.
        if FormatRows.said(by: [], in: schema.rows).contains(where: { $0.place == "target" ? !$0.text.isEmpty : !NoteText.refs($0.text).isEmpty }) {
            all.append(CardRow(path: SchemaNode.topRow, depth: 0, field: nil, value: nil, fromType: nil,
                               step: StepRow(label: L10n[.Common.wordRow], text: L10n[.Diagram.rowOutsideItems], detail: nil)))
        }
        func emit(_ list: [FormatRow], _ depth: Int, _ prefix: String) {
            for row in list where row.kind == .item {
                let path = prefix.isEmpty ? PathText.quote(row.name) : PathText.member(prefix, row.name)
                let expandable = row.rows.contains { $0.kind == .item }
                let isFolded = expandable && folded.contains(path)
                all.append(CardRow(path: path, depth: depth, field: nil, value: nil, fromType: nil,
                                   expandable: expandable, folded: isFolded,
                                   step: StepRow(label: L10n[.Diagram.rowItem], text: row.title,
                                                 detail: row.heading.isEmpty ? nil : row.name)))
                if !isFolded { emit(row.rows, depth + 1, path) }
            }
        }
        emit(schema.rows, 0, "")
        return window(all, from: from)
    }

    // A data card: its shape's fields, each with its type's badge and its
    // value, nested as the shape is (`InstanceRows`).
    if schema.kind == .instance {
        return window(InstanceRows.rows(schema, lookup: lookup, folded: folded), from: from)
    }

    // A card of no fields (a freedoc, a task): its lines leave from its header.
    if !schema.kind.isShape {
        return window(all, from: from)
    }

    /// `seen` guards against a value type that contains itself, directly or not.
    func emit(
        _ fields: [FieldNode], _ depth: Int, _ prefix: String,
        _ fromType: String?, _ seen: Set<String>
    ) {
        for field in fields {
            let path = prefix.isEmpty ? PathText.quote(field.name) : PathText.member(prefix, field.name)
            let target = field.type.isRef ? field.embeds.flatMap { lookup?($0) } : nil
            // What lies under this row, worked out before it is drawn so the row can
            // carry a disclosure mark. Value objects unfold to their fields and enums
            // to their values; an entity kept inside another is a real containment
            // worth a line.
            let inlined: SchemaNode? = {
                guard field.children.isEmpty, inlineReuse, lookup != nil, let target,
                      target.kind == .value && !target.fields.isEmpty
                        || target.kind == .enum && !target.values.isEmpty,
                      !seen.contains(target.id),
                      depth + 1 < CardGeometry.maxInlineDepth else { return nil }
                return target
            }()
            let expandable = !field.children.isEmpty || inlined != nil
            let isFolded = expandable && folded.contains(path)

            all.append(CardRow(path: path, depth: depth, field: field,
                               value: nil, fromType: fromType,
                               refTargetName: target?.name,
                               expandable: expandable, folded: isFolded,
                               unfoldsType: inlined != nil))

            guard !isFolded else { continue }
            if !field.children.isEmpty {
                emit(field.children, depth + 1, path, fromType, seen)
            } else if let inlined, inlined.kind == .enum {
                for value in inlined.values {
                    all.append(CardRow(path: PathText.member(path, value.value), depth: depth + 1, field: nil,
                                       value: value, fromType: inlined.name))
                }
            } else if let inlined {
                emit(inlined.fields, depth + 1, path, inlined.name, seen.union([inlined.id]))
            }
        }
    }

    emit(schema.fields, 0, "", nil, [schema.id])
    return window(all, from: from)
}

/// The card draws at most `maxRows`, but `all` keeps every row.
///
/// Edges are read off the rows, and a reference must not disappear because the row
/// carrying it fell out of the window — a card can easily hide more rows than it
/// shows, and the only line it has may live in one of them. The card says how many
/// are out of sight; the relationships stay whole.
///
/// `height`: a card of its own height, which holds as many rows as fit between its half rows.
private func cardWindow(_ all: [CardRow], from: Int, height: CGFloat? = nil) -> CardContent {
    let fit = max(1, height.map { rowsFitting($0, count: all.count) } ?? CardGeometry.maxRows)
    // Clamped in `scrolled`, not by the caller, so a stored offset survives the card
    // getting shorter — a filter change or an edit can take rows away underneath it.
    return CardContent(rows: [], hidden: 0, first: 0, all: all, fit: fit,
                       bands: height != nil || all.count > fit).scrolled(to: from)
}

/// How many of `count` rows a card of its own `height` draws: as many as fit between the half rows it always
/// keeps above and below them, so a card just tall enough for its rows draws all but the last, which its count
/// stands for.
func rowsFitting(_ height: CGFloat, count: Int) -> Int {
    min(max(1, count), max(1, Int((rowRoom(height) - 2 * CardGeometry.bandHeight) / CardGeometry.rowHeight)))
}

/// The points a card of `height` has for its rows and half rows: inside its border, under its header and padding.
private func rowRoom(_ height: CGFloat) -> CGFloat {
    height - CardGeometry.cardBorder * 2 - CardGeometry.headerHeight - CardGeometry.listPadding
}

/// How a card is drawn (overview §12): its own size in points — a card with a height of its own,
/// or a document's, which is the project's whole card — or its own width alone, its height its
/// rows'; and whether its body is the document as it reads rather than rows. Every other card: nil
/// and rows.
struct CardForm: Equatable {
    var size: CGSize? = nil
    var document = false
    /// Its own width with no height of its own: as tall as its rows.
    var width: CGFloat? = nil
    /// Drawn at the height set whatever it holds — a frame, which draws no rows — rather than at least as tall as
    /// a row and its half rows need (`drawnHeight`). A note draws its lines as rows, so it is not one: a note too
    /// short for a line and its half rows would draw them past its edge.
    var exactHeight = false

    /// How tall it is drawn when it has a size of its own: the height set, but a card whose rows scroll inside it
    /// never shorter than `CardGeometry.leastGridRows` — a row, and the half rows that say how many rows are out of
    /// sight and where the lines to those rows land, stay inside it. The size kept is the one set.
    var drawnHeight: CGFloat? {
        guard let size else { return nil }
        guard !document, !exactHeight else { return size.height }
        return max(size.height, CGFloat(CardGeometry.leastGridRows) * CGFloat(Grid.unitHeight))
    }

    /// What a custom card's row is pressed as.
    enum Control: Hashable {
        /// A switch: the row done or not.
        case toggle(on: Bool)
        /// A radio field's options, the one chosen marked — or, where they do not fit the row,
        /// a menu, as `FittingChoice` does — said by the row's label (`StepRow.label`).
        case segments(field: String, options: [String], chosen: String, labels: [String: String] = [:])
        /// A dropdown list's: the one chosen on a button, the options in the menu it opens
        /// (`face`).
        case menu(field: String, options: [String], chosen: String, labels: [String: String] = [:])

        /// How a value of a choice is shown: as its kind says it (`optionLabels`), else as it is.
        static func shown(_ option: String, _ labels: [String: String]) -> String { labels[option] ?? option }
    }
}

/// A face's rows (`face.top`): a row each of the fields it names, in its order —
/// a check its switch, a choice its options side by side (or a menu, where they do not fit), a
/// dropdown list a menu, a date or a text as it is written (`—` when nothing is) — each said by
/// its label, its path its field's (`#f.done`), as a kind the project defines has them.
enum FaceRows {
    static func rows(_ schema: SchemaNode, face: CardFace, def: CardKindDef) -> [CardRow] {
        face.top.compactMap { name -> CardRow? in
            guard let field = def.field(name) else { return nil }
            let path = OwnRows.fieldRow + name
            let value = schema.faceValues[name]
            let chosen = value?.asText ?? field.defaultValue?.asText ?? ""
            switch field.fieldtype {
            case "checkbox":
                return CardRow(path: path, depth: 0, field: nil, value: nil, fromType: nil,
                               step: StepRow(label: "", text: field.label, detail: nil),
                               form: .toggle(on: value == .bool(true)))
            case "radio":
                return CardRow(path: path, depth: 0, field: nil, value: nil, fromType: nil,
                               step: StepRow(label: field.label, text: "", detail: nil),
                               form: .segments(field: name, options: field.options, chosen: chosen,
                                               labels: field.optionLabels))
            case "dropdownlist":
                return CardRow(path: path, depth: 0, field: nil, value: nil, fromType: nil,
                               step: StepRow(label: field.label, text: "", detail: nil),
                               form: .menu(field: name, options: field.options, chosen: chosen,
                                           labels: field.optionLabels))
            case KindField.sectionType:
                // Where it points, by the section's path: `?` once the section is gone.
                return CardRow(path: path, depth: 0, field: nil, value: nil, fromType: nil,
                               step: StepRow(label: "", text: "→ " + (schema.faceSections[name] ?? "?"), detail: nil))
            default:
                let text = value.map(OwnFieldText.short) ?? ""
                return CardRow(path: path, depth: 0, field: nil, value: nil, fromType: nil,
                               step: StepRow(label: field.label, text: text.isEmpty ? "—" : text, detail: nil))
            }
        }
    }
}

/// A stackdoc's rows above the version it has open: what it is — its description's first line,
/// when one is written — and the selector of its versions, the one open on its button and every
/// one by its key in the menu it opens. Pressed, the version chosen is the one open (how it
/// looks: no commit).
enum StackRows {
    /// The selector, as its press names it (`CardForm.Control.menu`'s field).
    static let selector = "#version"
    /// The row saying what it is.
    static let about = "#about"

    static func rows(_ schema: SchemaNode) -> [CardRow] {
        var out: [CardRow] = []
        let summary = schema.description.split(separator: "\n", omittingEmptySubsequences: true).first
            .map { $0.trimmingCharacters(in: .whitespaces) } ?? ""
        if !summary.isEmpty {
            out.append(CardRow(path: about, depth: 0, field: nil, value: nil, fromType: nil,
                               step: StepRow(label: L10n[.Diagram.rowSummary], text: summary, detail: nil)))
        }
        let keys = schema.versions.map(\.key)
        out.append(CardRow(path: selector, depth: 0, field: nil, value: nil, fromType: nil,
                           step: StepRow(label: L10n[.Diagram.rowVersion], text: keys.isEmpty ? L10n[.Diagram.rowNoVersions] : "", detail: nil),
                           form: keys.isEmpty ? nil : .menu(field: selector, options: keys, chosen: schema.openVersion?.key ?? "")))
        return out
    }

    /// The version a key pressed in the selector names.
    static func version(_ schema: SchemaNode, key: String) -> StackVersion? {
        schema.versions.first { $0.key == key }
    }
}

/// Whether a card is drawn as a document — its whole text as it reads — rather than
/// as rows: a freedoc and a stackdoc always, a formatdoc unless it, or else the project, says rows.
func drawnAsDocument(_ schema: SchemaNode, docView: String) -> Bool {
    switch schema.kind {
    case .freedoc, .stackdoc, .table: return true
    case .formatdoc: return (schema.docView ?? docView) == "document"
    default: return false
    }
}

/// The form of `schema`'s card with the project's `settings` and card width, one rule for every
/// kind (`specboard://SpecBoard/Docs/Design:Overview` §12): with a height of its own, that size;
/// with a width alone, that width and the height a card of it has with none. With none: a document,
/// a table and a page the project's whole card; a card its kind's face draws a document under its
/// fields when its Markdown says anything (or it has a height of its own), as tall as the core
/// places it (`faceRows`: the page's height estimated from the text at the card's width, the
/// project's whole card at the most); a frame as big as the core places it; every other card its
/// rows.
func cardForm(_ schema: SchemaNode, settings: LayoutSettings, cardWidth: CGFloat) -> CardForm {
    let unit = CGFloat(Grid.unitWidth(cardWidth: Double(cardWidth)))
    let down = CGFloat(Grid.unitHeight)
    if CompactBox.isCompact(schema) {
        return CardForm(size: CGSize(width: CGFloat(CompactBox.columns) * unit, height: CGFloat(CompactBox.rows) * down))
    }
    let own = schema.sizable ? schema.size : nil
    let width = own.map { CGFloat($0.w) * unit }
    let wholeCard = CGFloat(Grid.rows(forRows: settings.size.rows)) * down
    if let face = schema.face {
        let document = schema.facePage != nil || (face.markdown != nil && (schema.faceDrawsMarkdown || own?.h != nil))
        if let width, let h = own?.h {
            return CardForm(size: CGSize(width: width, height: CGFloat(h) * down), document: document,
                            exactHeight: schema.kind == .frame)
        }
        // A frame is the box another screen is put into: as big as the core places it.
        if schema.kind == .frame {
            let rows = schema.faceRows.map { CGFloat($0) * down } ?? wholeCard
            return CardForm(size: CGSize(width: width ?? cardWidth, height: rows), document: false)
        }
        guard document else { return CardForm(width: width) }
        let rows = schema.faceRows.map { CGFloat($0) * down } ?? wholeCard
        return CardForm(size: CGSize(width: width ?? cardWidth, height: rows), document: true)
    }
    // A table is drawn as the table it builds, as a document is.
    let document = (schema.kind.isWriting || schema.kind == .table) && drawnAsDocument(schema, docView: settings.docView)
    if let width, let h = own?.h {
        return CardForm(size: CGSize(width: width, height: CGFloat(h) * down), document: document,
                        exactHeight: schema.kind == .frame)
    }
    guard document else { return CardForm(width: width) }
    return CardForm(size: CGSize(width: width ?? cardWidth, height: wholeCard), document: true)
}

func cardHeight(
    _ schema: SchemaNode, lookup: SchemaLookup? = nil, inlineReuse: Bool = false
) -> CGFloat {
    cardHeight(cardRows(schema, lookup: lookup, inlineReuse: inlineReuse,
                        folded: inlineReuse ? restingFolds(schema, lookup: lookup) : []))
}

/// The rows a card starts with folded: every value object and enum it holds, so
/// it reads as its own fields first and unfolds a type when asked — and a container's
/// cards done, under 「済み」.
func restingFolds(_ schema: SchemaNode, lookup: SchemaLookup?) -> Set<String> {
    Set(cardRows(schema, lookup: lookup, inlineReuse: true).all.filter { $0.unfoldsType || $0.startsFolded }.map(\.path))
}

/// A card's width in grid units: its own when it has one, every other card the project's ten.
func cardGridColumns(_ schema: SchemaNode) -> Int {
    if CompactBox.isCompact(schema) { return CompactBox.columns }
    return (schema.sizable ? schema.size?.w : nil) ?? Grid.cardColumns
}

/// How many grid rows a card takes as it first shows, at `rows` rows at the most
/// (§12) — what an agent is told, so it can place cards that do not overlap.
func cardGridRows(_ schema: SchemaNode, lookup: @escaping SchemaLookup, rows: Int, docView: String = "document") -> Int {
    // A card with a height of its own is that tall, whatever it holds; a document without one is
    // the project's whole card. So too a card its kind's face draws: its height, or with its
    // Markdown as tall as the core places it. Its width alone leaves its height to its rows.
    if CompactBox.isCompact(schema) { return CompactBox.rows }
    if schema.sizable, let h = schema.size?.h {
        // As tall as it is drawn: a card whose rows scroll inside it never shorter than a row and its half rows need.
        var settings = LayoutSettings.standard
        settings.docView = docView
        guard let drawn = cardForm(schema, settings: settings, cardWidth: CardGeometry.cardWidth).drawnHeight
        else { return h }
        return max(h, Grid.rows(forHeight: Double(drawn)))
    }
    if drawnAsDocument(schema, docView: docView) { return Grid.rows(forRows: rows) }
    if schema.face != nil, schema.faceDrawsPage || schema.kind == .frame { return schema.faceRows ?? Grid.rows(forRows: rows) }
    let saved = CardGeometry.maxRows
    CardGeometry.maxRows = rows
    defer { CardGeometry.maxRows = saved }
    return Grid.rows(forHeight: Double(cardHeight(schema, lookup: lookup, inlineReuse: true)))
}

func cardHeight(_ content: CardContent) -> CGFloat {
    CardGeometry.cardBorder * 2 + CardGeometry.headerHeight + CardGeometry.listPadding
        + CGFloat(content.rows.count) * CardGeometry.rowHeight
        + (content.bands ? 2 * CardGeometry.bandHeight : 0)
}

/// Where the drawn rows begin, relative to the card's top edge: under the half row kept above them, when the
/// card keeps them, whether or not rows are out of sight above — so the rows do not move as the card scrolls.
func listTop(_ content: CardContent) -> CGFloat {
    CardGeometry.cardBorder + CardGeometry.headerHeight + CardGeometry.listPadding / 2
        + (content.bands ? CardGeometry.bandHeight : 0)
}

/// The middle of the half row standing in for everything above, and below.
func aboveStripOffset(_ content: CardContent) -> CGFloat {
    CardGeometry.cardBorder + CardGeometry.headerHeight + CardGeometry.listPadding / 2
        + CardGeometry.bandHeight / 2
}

func belowStripOffset(_ content: CardContent) -> CGFloat {
    listTop(content) + CGFloat(content.rows.count) * CardGeometry.rowHeight
        + CardGeometry.bandHeight / 2
}

/// Vertical centre of one drawn row, relative to the card's top edge. The index is
/// into `rows`, not into `all`.
func rowOffset(_ index: Int, in content: CardContent) -> CGFloat {
    listTop(content)
        + CGFloat(index) * CardGeometry.rowHeight + CardGeometry.rowHeight / 2
}

/// Centre of the "+N" line, where edges from rows past the cut anchor instead.

/// A row that is a step, a trigger or a held record: the word for what it does,
/// what it does it to, and how it travels.
struct StepRow: Hashable {
    /// The one a row or an end that names no one deals with: whom the flow's start receives from.
    static var startPartner: String { L10n[.Charts.partnerStart] }

    /// Whom a receiving start that names no one receives from: anyone, or not written (a gap).
    static func beginPartner(_ node: FlowNode) -> String {
        node.anyCaller ? L10n[.Charts.partnerAnyone] : L10n[.Charts.partnerNotWritten]
    }

    var label: String
    var text: String
    var detail: String?

    /// As a row shows it, looked at in `tree`: the paths into that tree without it
    /// (`main@Data:Order` → `Data:Order`), the others as they are.
    func shown(in tree: String?) -> StepRow {
        var out = self
        out.text = SchemaDirectory.shown(text: text, in: tree)
        out.detail = detail.map { SchemaDirectory.shown(text: $0, in: tree) }
        return out
    }

    init(label: String, text: String, detail: String?) {
        self.label = label
        self.text = text
        self.detail = detail
    }

    init(_ node: FlowNode) {
        label = FlowNode.Vocabulary.label(node.kind)
        let what: String
        switch node.kind {
        case "begin":
            switch node.trigger {
            case "event": what = L10n[.Diagram.flowEventOutcome(node.event?.outcome ?? "", node.event?.flow ?? "")]
            case "receive": what = "receive ← \(FlowNode.partners(node.peers, node.program) ?? StepRow.beginPartner(node))"
            default: what = node.trigger ?? ""
            }
        case "process": what = L10n[.Diagram.rowRows(node.steps.count)]
        case "run": what = node.target ?? node.targets.joined(separator: " ∥ ")
        case "join": what = node.join ?? ""
        case "eval": what = node.value ?? node.branches.map(\.when).joined(separator: " / ")
        case "wait": what = node.waitFor ?? ""
        default:
            what = (node.outcome ?? "") + (node.reply.map { " → \(FlowNode.partners($0.peers, $0.program) ?? StepRow.startPartner)" } ?? "")
        }
        text = "\(node.name): \(what)"
        // Where each way goes, and the short word for how: a branch's condition is
        // prose, and on the card it would run over the row.
        let ways = node.exits.map { exit in
            (exit.kind == .branch ? "" : exit.label.map { "\($0) " } ?? "") + "→ \(exit.to)"
        }
        detail = ways.isEmpty ? nil : ways.joined(separator: " · ")
    }

    init(_ step: Step) {
        label = step.op
        // Receiving or answering with no one named: whom the start receives from.
        let other = FlowNode.partners(step.peers, step.program) ?? (["receive", "return"].contains(step.op) ? StepRow.startPartner : "")
        let carried = step.carries.joined(separator: ", ")
        switch step.op {
        case "receive":
            text = carried.isEmpty ? "← \(other)" : "← \(other): \(carried)"
        case "return", "send":
            text = carried.isEmpty ? "→ \(other)" : "→ \(other): \(carried)"
        case "call":
            text = "→ \(other)" + (carried.isEmpty ? "" : ": \(carried)")
                + (step.returns.isEmpty ? "" : " ⇠ \(step.returns.joined(separator: ", "))")
        case "read", "write":
            text = step.paths.joined(separator: ", ")
        case "sign", "verify":
            text = (step.key ?? "") + (step.paths.isEmpty ? "" : " — \(step.paths.joined(separator: ", "))")
        case "action":
            text = (step.action ?? "") + (carried.isEmpty ? "" : ": \(carried)")
        case "loop":
            text = step.over ?? ""
        default:
            text = ""
        }
        var how = [step.via, step.initiator == "self" && ["receive"].contains(step.op) ? "fetch" : nil]
            .compactMap { $0 }
        if step.signWith != nil { how.append("signs") }
        if step.verifyWith != nil { how.append("verifies") }
        detail = how.isEmpty ? nil : how.joined(separator: " · ")
    }

    init(case item: Step) {
        label = "when"
        text = item.when ?? ""
        detail = nil
    }

    init(_ trigger: Trigger) {
        label = "trigger"
        switch trigger.on {
        case "receive":
            let carried = trigger.carries.joined(separator: ", ")
            let other = FlowNode.partners(trigger.peers, trigger.program) ?? L10n[.Charts.partnerNotWritten]
            text = carried.isEmpty ? "← \(other)" : "← \(other): \(carried)"
        case "schedule":
            text = trigger.every ?? "schedule"
        default:
            text = trigger.on
        }
        detail = trigger.via
    }
}

/// A container's rows for the cards inside it (`as_row`): each a row — its check where its kind says
/// a field that means done, numbered in the order they are done in, its other fields written after
/// its text, or how many of what is under it are done — then a row of buttons for each other choice
/// its `as_row` names, and what its kind unfolds nested under it, foldable. The ones done are after
/// the rest, under one row 「済み」 that starts folded: a goal reads as what is left to do; its rows
/// keep their numbers.
nonisolated enum ContainerRows {
    /// Where a project-defined container's rows for its cards are, apart from its own rows.
    static let prefix = "#in."

    /// The row the cards done are folded under, after the rest: its name among the rows beside it.
    static let finished = "#finished"

    /// The field a choice under a row presses: the card's name and the field's (`AppModel.pressForm`),
    /// apart by a character no name holds.
    static func choiceField(_ path: String, _ field: String) -> String { "\(prefix)\(path)\u{1}\(field)" }

    /// A choice's press read back: the row's path (the card's name) and the field.
    static func choice(_ field: String) -> (path: String, field: String)? {
        guard field.hasPrefix(prefix), let mark = field.lastIndex(of: "\u{1}") else { return nil }
        let path = String(field[field.index(field.startIndex, offsetBy: prefix.count)..<mark])
        return path.isEmpty ? nil : (path, String(field[field.index(after: mark)...]))
    }

    static func rows(_ list: [CustomRow], prefix: String, folded: Set<String>,
                     progress: ([CustomRow]) -> (done: Int, of: Int)?) -> [CardRow] {
        var all: [CardRow] = []
        func one(_ row: CustomRow, _ path: String, _ depth: Int, number: Int?) {
            let expandable = !row.rows.isEmpty
            let isFolded = expandable && folded.contains(path)
            let counted = progress(row.rows).map { "\($0.done)/\($0.of)" }
            all.append(CardRow(path: path, depth: depth, field: nil, value: nil, fromType: nil,
                               expandable: expandable, folded: isFolded,
                               step: StepRow(label: "", text: row.text.isEmpty ? row.name : row.text,
                                             detail: counted ?? (row.more.isEmpty ? nil : row.more)),
                               form: row.check ? .toggle(on: row.done) : nil,
                               number: number))
            for choice in row.choices {
                all.append(CardRow(path: "\(path)#c.\(choice.field)", depth: depth + 1, field: nil, value: nil, fromType: nil,
                                   step: StepRow(label: choice.label, text: "", detail: nil),
                                   form: .segments(field: choiceField(path, choice.field), options: choice.options,
                                                   chosen: choice.chosen, labels: choice.labels)))
            }
            if !isFolded { emit(row.rows, depth + 1, path) }
        }
        func emit(_ list: [CustomRow], _ depth: Int, _ above: String) {
            func path(_ row: CustomRow) -> String {
                above.isEmpty ? prefix + PathText.quote(row.name) : PathText.member(above, row.name)
            }
            let numbered = list.enumerated().map { (row: $1, number: depth == 0 ? $0 + 1 : nil) }
            // The cards inside it, not what one of them unfolds: as they are numbered.
            func finished(_ row: CustomRow) -> Bool { depth == 0 && row.check && row.done }
            for (row, number) in numbered where !finished(row) { one(row, path(row), depth, number: number) }
            let done = numbered.filter { finished($0.row) }
            guard !done.isEmpty else { return }
            // Pressed open, they are under it as they were, a step in; folded, their lines leave from it.
            let group = prefix + ContainerRows.finished
            let isFolded = folded.contains(group)
            all.append(CardRow(path: group, depth: depth, field: nil, value: nil, fromType: nil,
                               expandable: true, folded: isFolded,
                               step: StepRow(label: "", text: L10n.text(.Diagram.rowDone, in: Language.current), detail: "\(done.count)"),
                               startsFolded: true, stands: isFolded ? done.map { path($0.row) } : [],
                               disclosesInNumbers: true))
            if !isFolded { for (row, number) in done { one(row, path(row), depth + 1, number: number) } }
        }
        emit(list, 0, "")
        return all
    }
}
