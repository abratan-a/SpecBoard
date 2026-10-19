import Foundation

/// A data card (`instance`,
/// `specboard://SpecBoard/Docs/Design:CardKinds`「データ（instance）と定義のセクション」) read against its
/// shape: the entity or value its line `of` goes to, whose fields are its rows — each its name, its
/// type's badge and its value — the values held as one JSON as nested as the shape (`data`). What a
/// value is held to is the core's to say (`card_update`), not the window's.
extension SchemaNode {
    /// A data card's shape, by id: where its line `of` goes.
    var instanceOf: String? {
        kind == .instance ? connections.first { $0.line == Line.of }?.to : nil
    }

    /// The shape a data card is of, or whose data a table gathers: where its line
    /// `of` goes.
    var shapeOf: String? {
        kind == .instance || kind == .table ? connections.first { $0.line == Line.of }?.to : nil
    }
}

/// A data card's row as its card draws it: the field's name, its type's badge — drawn as the nav
/// panel's branch badge is, so it is not read as the value — and its value in a line (none for one
/// not written, and for an object unfolded under it); marked when a required field has none.
struct InstanceDatum: Hashable {
    var name: String
    var badge: String
    var text: String?
    var missing: Bool = false
}

enum InstanceRows {
    /// How deep the shapes a data card's fields embed are unfolded.
    static let maxDepth = 6

    /// A field's type as its badge says it: its type, an object's `object`, the name of what a `ref`
    /// embeds (an enum's, a value's) — its own name, though the diagram names a card of another section
    /// by its section too (`Schemas:Status`); `[]` after it for an array.
    static func badge(_ field: FieldNode, lookup: SchemaLookup?) -> String {
        let embedded = field.embeds.flatMap { lookup?($0) }.map { card -> String in
            let (section, rest) = PathText.splitSection(card.name)
            return section == nil ? card.name : PathText.unquote(rest)
        }
        let base = field.type.isRef ? (embedded ?? "ref")
            : field.type.isObject ? "object" : field.type.rawValue
        return field.isArray ? base + "[]" : base
    }

    /// A value in a line: text as it is (its first line), a number, `true`・`false`・`null`, an
    /// array or an object as JSON.
    static func text(_ value: JSONValue) -> String {
        switch value {
        case .null: return "null"
        case .bool(let on): return on ? "true" : "false"
        case .number: return OwnFieldText.short(value)
        case .string(let text): return text.split(separator: "\n", omittingEmptySubsequences: false).first.map(String.init) ?? ""
        case .array, .object: return value.jsonText
        }
    }

    /// The fields a field unfolds into: an object's own, a value's or an entity's it embeds — not an
    /// array's (its elements are rows of their own) nor an enum's.
    static func inner(_ field: FieldNode, lookup: SchemaLookup?) -> (fields: [FieldNode], shape: SchemaNode?) {
        guard !field.isArray else { return ([], nil) }
        return element(field, lookup: lookup)
    }

    /// The fields an element of a field has — the field's own when it is not an array.
    static func element(_ field: FieldNode, lookup: SchemaLookup?) -> (fields: [FieldNode], shape: SchemaNode?) {
        if field.type.isObject { return (field.children, nil) }
        guard field.type.isRef, let target = field.embeds.flatMap({ lookup?($0) }), target.kind.holdsFields else { return ([], nil) }
        return (target.fields, target)
    }

    /// A data card's rows: its shape's fields, nested as the shape is, each its value from `data`; a
    /// field of a discriminator's branch only when it is written. An array's elements are rows under
    /// it, each named by its place (`ports[0]`) with its fields under it; folded, the array says its
    /// value in a line. None when its shape is gone.
    static func rows(_ card: SchemaNode, lookup: SchemaLookup?, folded: Set<String>) -> [CardRow] {
        guard let shape = card.instanceOf.flatMap({ lookup?($0) }), shape.kind.holdsFields else { return [] }
        var out: [CardRow] = []
        func row(_ path: String, _ depth: Int, _ datum: InstanceDatum, expandable: Bool = false, folded: Bool = false) {
            out.append(CardRow(path: path, depth: depth, field: nil, value: nil, fromType: nil,
                               expandable: expandable, folded: folded, datum: datum))
        }
        func emit(_ fields: [FieldNode], _ values: [String: JSONValue]?, _ depth: Int, _ prefix: String, _ seen: Set<String>) {
            for field in fields {
                let value = values?[field.name]
                if !field.variants.isEmpty, value == nil { continue }
                let path = prefix.isEmpty ? PathText.quote(field.name) : PathText.member(prefix, field.name)
                let label = badge(field, lookup: lookup)
                let missing = field.isRequired && value == nil
                if field.isArray {
                    let items = value?.arrayValue ?? []
                    let unfolds = !items.isEmpty && depth + 1 < maxDepth
                    let isFolded = unfolds && folded.contains(path)
                    row(path, depth, InstanceDatum(name: field.name, badge: label,
                                                   text: unfolds && !isFolded ? nil : value.map(text), missing: missing),
                        expandable: unfolds, folded: isFolded)
                    guard unfolds, !isFolded else { continue }
                    let (inner, embedded) = element(field, lookup: lookup)
                    let kind = String(label.dropLast(2))
                    for (i, item) in items.enumerated() {
                        let at = "\(path)[\(i)]"
                        let opens = !inner.isEmpty && depth + 2 < maxDepth && item.asObject != nil
                            && embedded.map { !seen.contains($0.id) } != false
                        let shut = opens && folded.contains(at)
                        row(at, depth + 1, InstanceDatum(name: "[\(i)]", badge: kind, text: opens && !shut ? nil : text(item)),
                            expandable: opens, folded: shut)
                        if opens, !shut {
                            emit(inner, item.asObject, depth + 2, at, seen.union(embedded.map { [$0.id] } ?? []))
                        }
                    }
                    continue
                }
                let (inner, embedded) = inner(field, lookup: lookup)
                let unfolds = !inner.isEmpty && depth + 1 < maxDepth
                    && embedded.map { !seen.contains($0.id) } != false
                    && (value == nil || value?.asObject != nil)
                let isFolded = unfolds && folded.contains(path)
                row(path, depth, InstanceDatum(name: field.name, badge: label,
                                               text: unfolds ? nil : value.map(text), missing: missing),
                    expandable: unfolds, folded: isFolded)
                if unfolds, !isFolded {
                    emit(inner, value?.asObject, depth + 1, path, seen.union(embedded.map { [$0.id] } ?? []))
                }
            }
        }
        emit(shape.fields, card.data?.asObject, 0, "", [shape.id])
        return out
    }

    /// A place in a data card's rows (`ports[0]`, `ports[0].number`) written as つなぐ keeps it: each
    /// element picked by the fields its shape keys it by (`ports[number=8200,protocol=tcp]`), so
    /// the line goes with the element when the array is put in another order. An element of a
    /// shape with no key, or of an array of plain values, by its place, as it is.
    static func keyed(_ card: SchemaNode, _ path: String, lookup: SchemaLookup?) -> String {
        guard card.kind == .instance, path.contains("["),
              let shape = card.instanceOf.flatMap({ lookup?($0) }), shape.kind.holdsFields else { return path }
        var fields = shape.fields
        var here: JSONValue? = card.data
        var out: [String] = []
        for name in PathText.dots(path) {
            let (base, index) = PathNames.placed(name)
            guard let field = fields.first(where: { $0.name == base }) else { return path }
            here = here?.asObject?[base]
            let (inner, _) = element(field, lookup: lookup)
            fields = inner
            guard let index else { out.append(name); continue }
            let item = here?.arrayValue.flatMap { $0.indices.contains(index) ? $0[index] : nil }
            here = item
            let keys = inner.filter(\.isPk)
            let pairs = keys.compactMap { key in
                item?.asObject?[key.name].flatMap(PathNames.literal).map { "\(PathText.quoteKey(key.name))=\($0)" }
            }
            out.append(!keys.isEmpty && pairs.count == keys.count
                       ? "\(PathText.quote(base))[\(pairs.joined(separator: ","))]" : name)
        }
        return out.joined(separator: ".")
    }

    /// The value at a path in a data card's values (`customer.name`, `ports[0].number`, each name as
    /// a path writes it): none when it is not written. An element by its place only (a pick by
    /// values is the core's to read).
    static func value(_ data: JSONValue?, at names: [String]) -> JSONValue? {
        var here = data
        for name in names {
            let (base, index) = PathNames.placed(name)
            guard let next = here?.asObject?[base] else { return nil }
            here = next
            if let index {
                guard let items = here?.arrayValue, items.indices.contains(index) else { return nil }
                here = items[index]
            }
        }
        return here
    }

    /// The fields of a data card's shape along a path (each name as a path writes it), through what
    /// each embeds and, past an array, its element picked (`ports[number=8200].number`,
    /// `ports[0]`): none past an array with no element picked, or a name the shape does not have.
    static func fields(along names: [String], of shape: SchemaNode, card: (String) -> SchemaNode?) -> [FieldNode]? {
        var list = shape.fields
        var out: [FieldNode] = []
        var open = false
        for name in names {
            if open { return nil }
            let (base, picked) = PathNames.picked(name)
            guard let field = list.first(where: { $0.name == base }) else { return nil }
            if picked, !field.isArray { return nil }
            out.append(field)
            open = field.isArray && !picked
            if field.type.isObject {
                list = field.children
            } else if field.type.isRef, let target = field.embeds.flatMap(card), target.kind.holdsFields {
                list = target.fields
            } else {
                list = []
            }
        }
        return out.isEmpty ? nil : out
    }
}

/// One name of a data card's path as written (`ports[0]`, `'user list'[]`), read: the name and
/// the element it picks, as the core reads them (`PathText.step`).
enum PathNames {
    /// A name with what is in its brackets: whether an element is picked (`ports[]` picks none).
    static func picked(_ written: String) -> (base: String, picked: Bool) {
        let (name, rest) = PathText.step(written)
        return (name, !inner(rest).isEmpty)
    }

    /// A name with the place its brackets pick (`ports[0]`), none for one with no place.
    static func placed(_ written: String) -> (base: String, index: Int?) {
        let (name, rest) = PathText.step(written)
        return (name, rest.isEmpty ? nil : Int(inner(rest)))
    }

    /// What is between a pick's brackets, spaces around it dropped.
    private static func inner(_ pick: String) -> String {
        guard pick.hasPrefix("["), pick.hasSuffix("]"), pick.count >= 2 else { return pick }
        return String(pick.dropFirst().dropLast()).trimmingCharacters(in: .whitespaces)
    }

    /// A field's value as a pick writes it (`tools::picks::Lit::of`): a number or a truth as it is,
    /// a text bare unless it would read as more than itself, quoted then. None for anything else.
    static func literal(_ value: JSONValue) -> String? {
        switch value {
        case .number: return OwnFieldText.short(value)
        case .bool(let on): return on ? "true" : "false"
        case .string(let text):
            let plain = !text.isEmpty && text.trimmingCharacters(in: CharacterSet(charactersIn: " \t")) == text
                && !text.contains(where: { ",[]=\"\\".contains($0) })
                && Double(text) == nil && !["true", "false", "null"].contains(text)
            if plain { return text }
            return "\"" + text.replacingOccurrences(of: "\\", with: "\\\\").replacingOccurrences(of: "\"", with: "\\\"") + "\""
        default: return nil
        }
    }
}
