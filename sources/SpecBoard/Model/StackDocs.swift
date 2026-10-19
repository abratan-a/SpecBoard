import Foundation

/// A version of a stackdoc (`specboard://SpecBoard/Docs/Design:CardKinds`「積み重ね文書」):
/// a row of its `versions` — its name for paths (`Docs:Changelog.v0_2_21`), its key for the selector
/// (`0.2.21`), its heading (the key when none) and its body in Markdown.
nonisolated struct StackVersion: Equatable, Hashable, Sendable {
    var id: String = ""
    var name: String
    var key: String
    var heading: String = ""
    var body: String = ""

    /// Its heading as it is shown: the key when none is written.
    var shownHeading: String {
        heading.trimmingCharacters(in: .whitespaces).isEmpty ? key : heading
    }

    /// The versions as the core keeps them (`form.rows.versions`: each its `id` and `fields`).
    static func read(_ rows: JSONValue?) -> [StackVersion] {
        guard case .array(let list)? = rows else { return [] }
        return list.compactMap { item in
            guard case .object(let row) = item, case .object(let fields)? = row["fields"] else { return nil }
            func text(_ key: String) -> String {
                if case .string(let value)? = fields[key] { return value }
                return ""
            }
            var id = ""
            if case .string(let given)? = row["id"] { id = given }
            return StackVersion(id: id, name: text("name"), key: text("key"), heading: text("heading"), body: text("body"))
        }
    }

    /// As MCP writes it (`card_update`'s `versions`, or a version's own `value`): what is written.
    var wire: [String: Any] {
        var out: [String: Any] = ["name": name, "key": key]
        if !heading.isEmpty { out["heading"] = heading }
        if !body.isEmpty { out["body"] = body }
        return out
    }

    /// The version as a page: its heading, then its body.
    var page: String {
        let text = body.trimmingCharacters(in: .whitespacesAndNewlines)
        return text.isEmpty ? "## \(shownHeading)" : "## \(shownHeading)\n\n\(text)"
    }

    /// A name for a version added on top: from its key (`0.2.24` → `v0_2_24`), made unique among `taken`.
    static func name(for key: String, taken: Set<String>) -> String {
        let cleaned = key.map { $0.isASCII && ($0.isLetter || $0.isNumber) ? String($0) : "_" }.joined()
        var base = cleaned.trimmingCharacters(in: CharacterSet(charactersIn: "_"))
        if base.isEmpty { base = "version" }
        if let first = base.first, !first.isLetter { base = "v" + base }
        var name = base
        var n = 2
        while taken.contains(name) {
            name = "\(base)_\(n)"
            n += 1
        }
        return name
    }
}

extension SchemaNode {
    /// A stackdoc's version open: the one its look names, else the one on top.
    var openVersion: StackVersion? {
        version.flatMap { name in versions.first { $0.name == name } } ?? versions.first
    }
}
