import Foundation

/// What the window writes, in the shape the core's tools take it (`tool`,
/// `specboard://SpecBoard/Docs/Design:CoreApi`): the same keys and values MCP writes.
extension OJSON {
    /// As `JSONSerialization` takes it.
    var foundation: Any {
        switch self {
        case .null: return NSNull()
        case .bool(let flag): return flag
        case .number(let number): return number
        case .string(let text): return text
        case .array(let items): return items.map(\.foundation)
        case .object(let object):
            return Dictionary(object.entries.map { ($0.0, $0.1.foundation) }, uniquingKeysWith: { a, _ in a })
        }
    }
}

enum ScreenWire {
    /// A value that codes itself as the tools write it (a formatdoc's rows, an export
    /// setting), as `JSONSerialization` takes it.
    static func encoded<T: Encodable>(_ value: T) throws -> Any {
        try JSONSerialization.jsonObject(with: JSONEncoder().encode(value), options: [.fragmentsAllowed])
    }

    /// A grid size as the tools write it — `{w}` with no height of its own — or null to go back to
    /// the project's.
    static func size(_ size: GridSize?) -> Any {
        guard let size else { return NSNull() }
        guard let h = size.h else { return ["w": size.w] }
        return ["w": size.w, "h": h]
    }
}
