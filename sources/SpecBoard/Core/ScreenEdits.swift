import Foundation

/// A change to a card from the window: only what is set changes, written as `card_update`
/// takes it.
struct CardEdit {
    var name: String?
    var label: String?
    var kind: SchemaKind?
    var description: String?
    var tags: [String]?
    /// A note's and a freedoc's body.
    var body: String?

    var wire: [String: Any] {
        var out: [String: Any] = [:]
        if let name { out["name"] = name }
        if let label { out["label"] = label }
        if let kind { out["kind"] = kind.rawValue }
        if let description { out["description"] = description }
        if let tags { out["tags"] = tags }
        if let body { out["body"] = body }
        return out
    }
}

/// A change to a field from the window, written as `card_update` takes it on the field's
/// path.
struct FieldEdit {
    var name: String?
    var label: String?
    var type: FieldType?
    var isArray: Bool?
    var isRequired: Bool?
    var isNullable: Bool?
    var isUnique: Bool?
    var isPk: Bool?
    var description: String?
    var also: [String]?

    var wire: [String: Any] {
        var out: [String: Any] = [:]
        if let name { out["name"] = name }
        if let label { out["label"] = label }
        if let type { out["type"] = type.rawValue }
        if let also { out["also"] = also }
        if let isArray { out["array"] = isArray }
        if let isRequired { out["required"] = isRequired }
        if let isNullable { out["nullable"] = isNullable }
        if let isUnique { out["unique"] = isUnique }
        if let isPk { out["primaryKey"] = isPk }
        if let description { out["description"] = description }
        return out
    }
}
