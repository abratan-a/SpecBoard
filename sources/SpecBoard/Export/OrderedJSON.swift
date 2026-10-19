import Foundation

/// A JSON value whose object keys keep the order they were added in, serialised
/// exactly the way `JSON.stringify(value, null, 2)` does.
///
/// Both halves matter. The web build emits JSON Schema straight from JavaScript
/// object literals, so the key order in its output *is* the order the code writes
/// them — `$schema`, `$id`, `title`, then the rest. A Swift `Dictionary` would
/// scramble that and produce a file that is correct but unrecognisable next to the
/// one people already have in review.
///
/// Assigning an existing key updates it in place rather than moving it to the end,
/// which is also what JavaScript does, and which the generator depends on: an open
/// enum sets `description` twice and the second write must not move it.
indirect enum OJSON {
    case null
    case bool(Bool)
    case number(Double)
    case string(String)
    case array([OJSON])
    case object(OrderedObject)
}

struct OrderedObject {
    private(set) var keys: [String] = []
    private var values: [String: OJSON] = [:]

    init() {}

    init(_ pairs: [(String, OJSON)]) {
        for (key, value) in pairs { self[key] = value }
    }

    var isEmpty: Bool { keys.isEmpty }

    subscript(key: String) -> OJSON? {
        get { values[key] }
        set {
            guard let newValue else {
                keys.removeAll { $0 == key }
                values.removeValue(forKey: key)
                return
            }
            if values[key] == nil { keys.append(key) }
            values[key] = newValue
        }
    }

    /// `{ ...self, extra }` — the spread keeps this object's order, then appends.
    func merging(_ pairs: [(String, OJSON)]) -> OrderedObject {
        var out = self
        for (key, value) in pairs { out[key] = value }
        return out
    }

    var entries: [(String, OJSON)] { keys.map { ($0, values[$0]!) } }
}

// MARK: - Serialising

extension OJSON {
    /// Matches `JSON.stringify(value, null, 2)`. No trailing newline — the caller
    /// adds it, as the web build does.
    func pretty(indent: String = "") -> String {
        let inner = indent + "  "
        switch self {
        case .null: return "null"
        case .bool(let value): return value ? "true" : "false"
        case .number(let value): return Self.number(value)
        case .string(let value): return Self.quote(value)
        case .array(let items):
            guard !items.isEmpty else { return "[]" }
            let body = items
                .map { inner + $0.pretty(indent: inner) }
                .joined(separator: ",\n")
            return "[\n\(body)\n\(indent)]"
        case .object(let object):
            guard !object.isEmpty else { return "{}" }
            let body = object.entries
                .map { inner + Self.quote($0.0) + ": " + $0.1.pretty(indent: inner) }
                .joined(separator: ",\n")
            return "{\n\(body)\n\(indent)}"
        }
    }

    /// JavaScript prints an integral double without a decimal point, and Swift does
    /// not — `3.0` has to come out as `3` or every integer constant differs.
    private static func number(_ value: Double) -> String {
        guard value.isFinite else { return "null" }
        if value == value.rounded() && abs(value) < 1e15 {
            return String(Int64(value))
        }
        return String(value)
    }

    /// The same escapes `JSON.stringify` applies — and no more. Non-ASCII is left
    /// alone, so Japanese labels stay readable in the generated files.
    private static func quote(_ text: String) -> String {
        var out = "\""
        for scalar in text.unicodeScalars {
            switch scalar {
            case "\"": out += "\\\""
            case "\\": out += "\\\\"
            case "\n": out += "\\n"
            case "\r": out += "\\r"
            case "\t": out += "\\t"
            case "\u{08}": out += "\\b"
            case "\u{0C}": out += "\\f"
            default:
                if scalar.value < 0x20 {
                    out += String(format: "\\u%04x", scalar.value)
                } else {
                    out.unicodeScalars.append(scalar)
                }
            }
        }
        return out + "\""
    }
}

extension OJSON {
    /// `JSON.stringify(value)` with no spacing — what a value embedded inside a
    /// comment needs. A rule parameter that is an array must read as `["a","b"]`,
    /// not spread over three lines in the middle of a sentence.
    func compact() -> String {
        switch self {
        case .null, .bool, .number, .string: return pretty()
        case .array(let items):
            return "[" + items.map { $0.compact() }.joined(separator: ",") + "]"
        case .object(let object):
            return "{" + object.entries
                .map { OJSON.string($0.0).pretty() + ":" + $0.1.compact() }
                .joined(separator: ",") + "}"
        }
    }
}

// MARK: - Bridging the typed model

extension OJSON {
    init(_ value: JSONValue) {
        switch value {
        case .null: self = .null
        case .bool(let v): self = .bool(v)
        case .number(let v): self = .number(v)
        case .string(let v): self = .string(v)
        case .array(let v): self = .array(v.map(OJSON.init))
        case .object(let v):
            // A free-form blob the tool never interprets. Its original key order is
            // not preserved through the typed model, so it is emitted sorted —
            // deterministic, and no meaning rides on the order.
            self = .object(OrderedObject(v.sorted { $0.key < $1.key }
                .map { ($0.key, OJSON($0.value)) }))
        }
    }

    static func string(_ values: [String]) -> OJSON { .array(values.map { .string($0) }) }

    static func map(_ values: [String: String]) -> OJSON {
        .object(OrderedObject(values.sorted { $0.key < $1.key }.map { ($0.key, .string($0.value)) }))
    }

    /// `kind` first, then the parameters — the shape every rule is written in.
    static func rule(_ rule: Rule) -> OJSON {
        var out = OrderedObject()
        out["kind"] = .string(rule.kind)
        for (key, value) in rule.orderedParams {
            out[key] = OJSON(value)
        }
        return .object(out)
    }

    static func rules(_ rules: [Rule]) -> OJSON { .array(rules.map(OJSON.rule)) }

    static func absentFields(_ fields: [AbsentField]) -> OJSON {
        .array(fields.map { field in
            .object(OrderedObject([("name", .string(field.name)),
                                   ("reason", .string(field.reason))]))
        })
    }
}

// MARK: - Parsing

/// A JSON reader that keeps object keys in the order they appear in the text.
///
/// `JSONDecoder` cannot: its keyed containers are backed by a dictionary, so
/// `allKeys` comes back in hash order. That is fine for anything with a fixed
/// shape, and wrong for the blobs this tool promises not to interpret — a rule's
/// parameters are quoted verbatim into generated comments, where
/// `requiredIf(when=…, then=…)` reads as a sentence and the alphabetical version
/// does not.
extension OJSON {
    static func parse(_ text: String) -> OJSON? {
        var scanner = Scanner(Array(text.unicodeScalars))
        guard let value = scanner.value() else { return nil }
        scanner.skipWhitespace()
        return scanner.atEnd ? value : nil
    }

    /// Deeper than any project or request nests, and shallow enough that the recursion
    /// below cannot run out of stack on a document built to make it.
    static let maxDepth = 256

    private struct Scanner {
        let scalars: [Unicode.Scalar]
        var index = 0
        var depth = 0

        init(_ scalars: [Unicode.Scalar]) { self.scalars = scalars }

        var atEnd: Bool { index >= scalars.count }
        private var current: Unicode.Scalar? { atEnd ? nil : scalars[index] }

        mutating func skipWhitespace() {
            while let c = current, c == " " || c == "\n" || c == "\r" || c == "\t" { index += 1 }
        }

        mutating func value() -> OJSON? {
            skipWhitespace()
            guard let c = current else { return nil }
            if c == "{" || c == "[" {
                guard depth < OJSON.maxDepth else { return nil }
                depth += 1
            }
            defer { if c == "{" || c == "[" { depth -= 1 } }
            switch c {
            case "{": return object()
            case "[": return array()
            case "\"": return string().map(OJSON.string)
            case "t": return literal("true", .bool(true))
            case "f": return literal("false", .bool(false))
            case "n": return literal("null", .null)
            default: return number()
            }
        }

        private mutating func literal(_ text: String, _ result: OJSON) -> OJSON? {
            for scalar in text.unicodeScalars {
                guard current == scalar else { return nil }
                index += 1
            }
            return result
        }

        private mutating func object() -> OJSON? {
            index += 1                                  // {
            var out = OrderedObject()
            skipWhitespace()
            if current == "}" { index += 1; return .object(out) }
            while true {
                skipWhitespace()
                guard let key = string() else { return nil }
                skipWhitespace()
                guard current == ":" else { return nil }
                index += 1
                guard let value = value() else { return nil }
                out[key] = value
                skipWhitespace()
                if current == "," { index += 1; continue }
                if current == "}" { index += 1; return .object(out) }
                return nil
            }
        }

        private mutating func array() -> OJSON? {
            index += 1                                  // [
            var out: [OJSON] = []
            skipWhitespace()
            if current == "]" { index += 1; return .array(out) }
            while true {
                guard let value = value() else { return nil }
                out.append(value)
                skipWhitespace()
                if current == "," { index += 1; continue }
                if current == "]" { index += 1; return .array(out) }
                return nil
            }
        }

        private mutating func string() -> String? {
            guard current == "\"" else { return nil }
            index += 1
            var out = String.UnicodeScalarView()
            while let c = current {
                index += 1
                if c == "\"" { return String(out) }
                if c != "\\" { out.append(c); continue }

                guard let escape = current else { return nil }
                index += 1
                switch escape {
                case "\"", "\\", "/": out.append(escape)
                case "n": out.append("\n")
                case "r": out.append("\r")
                case "t": out.append("\t")
                case "b": out.append("\u{08}")
                case "f": out.append("\u{0C}")
                case "u":
                    guard let first = hex() else { return nil }
                    // A surrogate pair arrives as two escapes and has to be rejoined.
                    if first >= 0xD800 && first <= 0xDBFF {
                        guard current == "\\" else { return nil }
                        index += 1
                        guard current == "u" else { return nil }
                        index += 1
                        guard let second = hex(), (0xDC00...0xDFFF).contains(second),
                              let scalar = Unicode.Scalar(
                                0x10000 + ((first - 0xD800) << 10) + (second - 0xDC00))
                        else { return nil }
                        out.append(scalar)
                    } else {
                        guard let scalar = Unicode.Scalar(first) else { return nil }
                        out.append(scalar)
                    }
                default: return nil
                }
            }
            return nil
        }

        private mutating func hex() -> UInt32? {
            var value: UInt32 = 0
            for _ in 0..<4 {
                guard let c = current, let digit = c.hexDigitValue else { return nil }
                value = value << 4 + UInt32(digit)
                index += 1
            }
            return value
        }

        private mutating func number() -> OJSON? {
            let start = index
            if current == "-" { index += 1 }
            while let c = current,
                  (c >= "0" && c <= "9") || c == "." || c == "e" || c == "E"
                    || c == "+" || c == "-" {
                index += 1
            }
            guard start < index,
                  let value = Double(String(String.UnicodeScalarView(scalars[start..<index])))
            else { return nil }
            return .number(value)
        }
    }
}

extension Unicode.Scalar {
    fileprivate var hexDigitValue: Int? {
        switch self {
        case "0"..."9": return Int(value - 48)
        case "a"..."f": return Int(value - 87)
        case "A"..."F": return Int(value - 55)
        default: return nil
        }
    }
}

// MARK: - Back to the typed model

extension JSONValue {
    init(_ value: OJSON) {
        switch value {
        case .null: self = .null
        case .bool(let v): self = .bool(v)
        case .number(let v): self = .number(v)
        case .string(let v): self = .string(v)
        case .array(let v): self = .array(v.map(JSONValue.init))
        case .object(let v):
            self = .object(Dictionary(uniqueKeysWithValues:
                v.entries.map { ($0.0, JSONValue($0.1)) }))
        }
    }
}

extension Rule {
    /// Rules read straight from stored JSON, parameters in the order written.
    static func list(fromJSON raw: String?) -> [Rule] {
        guard let raw, case .array(let items)? = OJSON.parse(raw) else { return [] }
        return items.compactMap { item in
            guard case .object(let object) = item,
                  case .string(let kind)? = object["kind"] else { return nil }
            var params: [String: JSONValue] = [:]
            var order: [String] = []
            for (key, value) in object.entries where key != "kind" {
                params[key] = JSONValue(value)
                order.append(key)
            }
            return Rule(kind: kind, params: params, paramOrder: order)
        }
    }
}

extension OJSON {
    /// The string inside, or nil for JSON null. Used where one argument may either
    /// set a value or clear it — `discriminator: null` means "no discriminator",
    /// which is a different statement from omitting the key.
    var stringOrNil: String? {
        if case .string(let value) = self { return value }
        return nil
    }
}
