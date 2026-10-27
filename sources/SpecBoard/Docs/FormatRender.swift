import Foundation

/// What a formatdoc's rows that are drawn from the project come out as — the same on
/// screen and written out (`specboard://SpecBoard/Docs/Design:DesignActions`「formatdoc」): a card's table, and
/// the cards of a chart section whose diagrams are put in.
enum FormatRender {

    /// A shape's contents as a Markdown table: an entity's or a value's fields — name,
    /// type, required, description, the nested ones under their parent as
    /// `parent.child` — or an enum's values. `name` says a card as it is to be read
    /// (on screen in full, written out by its name); `text` turns a description into
    /// what is shown. Nil for a card that is not a shape.
    static func cardTable(_ card: SchemaNode, in detail: ProjectDetail,
                          name: (SchemaNode) -> String, text: (String) -> String = { $0 }) -> String? {
        let directory = detail.directory
        switch card.kind {
        case .entity, .value:
            let required = L10n[.Common.wordRequired]
            var cells: [[String]] = []
            func walk(_ list: [FieldNode], _ trail: [String]) {
                for field in list {
                    let path = trail + [PathText.quote(field.name)]
                    var type = field.type.isRef
                        ? (field.embeds.flatMap(directory.schema(id:)).map(name) ?? "ref")
                        : field.type.rawValue
                    if field.isArray { type += "[]" }
                    if field.isNullable { type += " | null" }
                    cells.append([MarkdownRefs.code(path.joined(separator: ".")), type, field.isRequired ? required : "",
                                  text(field.label.isEmpty ? field.description
                                       : field.description.isEmpty ? field.label : "\(field.label) — \(field.description)")])
                    walk(field.children, path)
                }
            }
            walk(card.fields, [])
            return FormatRows.table([L10n[.Common.wordName], L10n[.Common.wordType], required,
                                     L10n[.Common.wordDescription]], cells)
        case .enum:
            return FormatRows.table([L10n[.Common.wordValue], L10n[.Common.wordDisplayName]],
                                    card.values.map { ["`\($0.value)`", $0.label] })
        case .table:
            // A table: the table its rules build, as the core builds it.
            return card.tableMarkdown
        case .instance:
            // A data card: its shape's fields, each with its type and its value as
            // JSON; what the shape has lost after them — as the core writes it out.
            guard let shape = card.instanceOf.flatMap(directory.schema(id:)) else { return nil }
            var cells: [[String]] = []
            func shown(_ value: JSONValue) -> String { "`\(value.jsonText)`" }
            func walk(_ list: [FieldNode], _ trail: [String], _ values: [String: JSONValue]?, _ depth: Int) {
                for field in list {
                    let value = values?[field.name]
                    if !field.variants.isEmpty, value == nil { continue }
                    let path = trail + [PathText.quote(field.name)]
                    let type = InstanceRows.badge(field, lookup: directory.schema(id:))
                    // An array, element by element: its fields under each, a plain value on its
                    // row.
                    if field.isArray, let items = value?.arrayValue, !items.isEmpty, depth < 24 {
                        cells.append([MarkdownRefs.code(path.joined(separator: ".")), type, ""])
                        let inner = InstanceRows.element(field, lookup: directory.schema(id:)).fields
                        let kind = String(type.dropLast(2))
                        for (i, item) in items.enumerated() {
                            var at = path
                            at[at.count - 1] += "[\(i)]"
                            if !inner.isEmpty, item.asObject != nil {
                                cells.append([MarkdownRefs.code(at.joined(separator: ".")), kind, ""])
                                walk(inner, at, item.asObject, depth + 1)
                            } else {
                                cells.append([MarkdownRefs.code(at.joined(separator: ".")), kind, shown(item)])
                            }
                        }
                        continue
                    }
                    let inner = InstanceRows.inner(field, lookup: directory.schema(id:)).fields
                    if !inner.isEmpty, value == nil || value?.asObject != nil, depth < 24 {
                        cells.append([MarkdownRefs.code(path.joined(separator: ".")), type, ""])
                        walk(inner, path, value?.asObject, depth + 1)
                    } else {
                        cells.append([MarkdownRefs.code(path.joined(separator: ".")), type, value.map(shown) ?? ""])
                    }
                }
            }
            walk(shape.fields, [], card.data?.asObject, 0)
            for at in card.strayFields.keys.sorted() {
                cells.append(["`\(at)`", L10n[.Docs.tableNotInShape], shown(card.strayFields[at] ?? .null)])
            }
            return FormatRows.table([L10n[.Common.wordName], L10n[.Common.wordType], L10n[.Common.wordValue]], cells)
        default:
            return nil
        }
    }

    /// The flows and actions of a chart or actor section, its inner sections' after its own, in
    /// the order the section lists them. Nil when the section holds no flows.
    static func charts(in target: String, from base: String, in detail: ProjectDetail) -> [SchemaNode]? {
        let directory = detail.directory
        let name = target.hasSuffix(":") ? String(target.dropLast()) : target
        guard let section = directory.section(named: name, from: base), section.kind?.holdsFlows == true else { return nil }
        var out: [SchemaNode] = []
        func walk(_ id: String) {
            out += directory.members(id).filter { $0.kind == .flow || $0.kind == .action }
            for inner in directory.children(of: id) { walk(inner.id) }
        }
        walk(section.id)
        return out
    }
}
