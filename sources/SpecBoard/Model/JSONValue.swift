import Foundation

/// A JSON value the tool stores but never interprets.
///
/// `const`, rule params and `metadata` all hold whatever the designer put there.
/// The web build leans on `unknown` for these; Swift needs the shape spelled out,
/// so this exists purely to round-trip a value unchanged through the API and the
/// database. Nothing here inspects the contents.
nonisolated enum JSONValue: Codable, Hashable, Sendable {
    case null
    case bool(Bool)
    case number(Double)
    case string(String)
    case array([JSONValue])
    case object([String: JSONValue])

    init(from decoder: Decoder) throws {
        let c = try decoder.singleValueContainer()
        if c.decodeNil() { self = .null; return }
        // Bool first: a JSON `true` would otherwise be read as a number by some
        // encoders, and `const: true` must not come back as `1`.
        if let v = try? c.decode(Bool.self) { self = .bool(v); return }
        if let v = try? c.decode(Double.self) { self = .number(v); return }
        if let v = try? c.decode(String.self) { self = .string(v); return }
        if let v = try? c.decode([JSONValue].self) { self = .array(v); return }
        if let v = try? c.decode([String: JSONValue].self) { self = .object(v); return }
        throw DecodingError.dataCorruptedError(in: c, debugDescription: "unrecognised JSON value")
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.singleValueContainer()
        switch self {
        case .null: try c.encodeNil()
        case .bool(let v): try c.encode(v)
        case .number(let v): try c.encode(v)
        case .string(let v): try c.encode(v)
        case .array(let v): try c.encode(v)
        case .object(let v): try c.encode(v)
        }
    }
}

extension JSONValue {
    var isNull: Bool { if case .null = self { return true }; return false }

    var stringValue: String? { if case .string(let v) = self { return v }; return nil }

    var objectValue: [String: JSONValue]? {
        if case .object(let v) = self { return v }
        return nil
    }

    /// How the value reads to a person — property panel chips, generated comments.
    /// Numbers lose a trailing `.0` because `const: 3` should not display as `3.0`.
    var display: String {
        switch self {
        case .null: return "null"
        case .bool(let v): return v ? "true" : "false"
        case .number(let v):
            return v == v.rounded() && abs(v) < 1e15
                ? String(Int64(v)) : String(v)
        case .string(let v): return v
        case .array(let v): return "[" + v.map(\.display).joined(separator: ", ") + "]"
        case .object(let v):
            return "{" + v.sorted { $0.key < $1.key }
                .map { "\($0.key): \($0.value.display)" }.joined(separator: ", ") + "}"
        }
    }
}

// MARK: - Text form, for the JSON columns

extension JSONValue {
    /// Decode a JSON column. A hand-edited database should not crash a read, so
    /// anything unparseable reads as the caller's fallback — same posture as
    /// `parseJson` in the web build's repo.ts.
    static func decode<T: Decodable>(_ raw: String?, as type: T.Type, fallback: T) -> T {
        guard let raw, !raw.isEmpty, let data = raw.data(using: .utf8) else { return fallback }
        return (try? JSONDecoder().decode(T.self, from: data)) ?? fallback
    }

    static func encodeToString<T: Encodable>(_ value: T) -> String? {
        guard let data = try? JSONEncoder().encode(value) else { return nil }
        return String(data: data, encoding: .utf8)
    }
}

extension JSONValue {
    /// As `JSONSerialization` takes it — what the window writes through the tools.
    nonisolated var wire: Any {
        switch self {
        case .null: return NSNull()
        case .bool(let flag): return flag
        case .number(let number): return number
        case .string(let text): return text
        case .array(let items): return items.map(\.wire)
        case .object(let object): return object.mapValues(\.wire)
        }
    }
}
