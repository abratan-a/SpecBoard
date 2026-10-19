import Foundation

/// A function or type inside a working directory's file, as the core reads it from the file now
/// (`file_outline`, `specboard://SpecBoard/Docs/Design:CoreApi`): never kept in the project — made each
/// time from the file — and named by its path in the file (`Order.total`), each name quoted as a path's
/// is. A line to it is written to its file with the symbol's path as where it lands (`at`).
nonisolated struct CodeSymbol: Hashable, Sendable, Decodable {
    /// Its names from the file's top, joined with `.` (`Order.total`, `Order.'fmt (2)'`).
    var path: String
    var name: String
    /// What it is, the same in every language: `function`, `method`, `type`, `module`, `constant`, `macro`.
    var of: String
    /// What its language calls it (`fn`, `struct`, `protocol`, `extension`).
    var word: String
    /// The line its name is on, from 1.
    var line: Int
    /// Its lines, the comments and attributes above it included.
    var from: Int
    var to: Int
    /// The symbol it is in, by its path; nil at the file's top.
    var parent: String?

    /// Its mark, by what it is (`SymbolBadge`).
    var badge: String { SymbolBadge.of(of) }

    /// How deep it sits under the file's top.
    var depth: Int { parent.map { PathText.dots($0).count } ?? 0 }
}

/// The marks a function or type is drawn with, one per kind of symbol whatever its language: narrow
/// enough for a row, and alike across languages so that a person scans one set of marks; its language's
/// own word is said on the pointer.
nonisolated enum SymbolBadge {
    static func of(_ of: String) -> String {
        switch of {
        case "function", "method": "ƒ"
        case "type": "T"
        case "module": "M"
        case "constant": "C"
        case "macro": "#"
        default: "·"
        }
    }
}

/// One file's functions and types as the core read them (`file_outline`): in the file's order, each right
/// after what it is in. `stamp` names the text they were read from, so the next ask (`since`) is answered
/// `unchanged` while the file is as it was. `problem`: why there are none or not all (too large, no folder
/// bound, a language this machine lacks).
nonisolated struct FileOutline: Hashable, Sendable {
    var id: String
    var language: String?
    var stamp: String?
    var symbols: [CodeSymbol] = []
    var parseErrors: Int?
    var problem: String?

    /// The symbols right inside `parent` (the file's top for nil), in the file's order.
    func children(of parent: String?) -> [CodeSymbol] {
        symbols.filter { $0.parent == parent }
    }

    func symbol(_ path: String) -> CodeSymbol? {
        symbols.first { $0.path == path }
    }

    /// Whether anything is inside `path`.
    func holds(_ path: String) -> Bool {
        symbols.contains { $0.parent == path }
    }

    /// One file of the answer as the core wrote it; nil without an id.
    init?(answer: [String: Any]) {
        guard let id = answer["id"] as? String else { return nil }
        self.id = id
        language = answer["language"] as? String
        stamp = answer["stamp"] as? String
        parseErrors = answer["parseErrors"] as? Int
        problem = answer["problem"] as? String
        if let list = answer["symbols"],
           let data = try? JSONSerialization.data(withJSONObject: list),
           let read = try? JSONDecoder().decode([CodeSymbol].self, from: data) {
            symbols = read
        }
    }

    init(id: String, language: String? = nil, stamp: String? = nil, symbols: [CodeSymbol] = [],
         parseErrors: Int? = nil, problem: String? = nil) {
        self.id = id
        self.language = language
        self.stamp = stamp
        self.symbols = symbols
        self.parseErrors = parseErrors
        self.problem = problem
    }
}

/// The languages the core reads files in, by their ids. Which of them list functions and types: every
/// language but those only coloured — a language an agent added is read for them too, the core saying
/// so when it cannot.
nonisolated enum CodeLanguages {
    /// Coloured, but nothing picked out of them.
    static let highlightOnly: Set<String> = ["json", "yaml", "toml", "html", "css", "markdown"]

    /// Whether a file in `language` lists functions and types: opened in the nav panel, drawn with rows.
    static func hasSymbols(_ language: String?) -> Bool {
        guard let language, !language.isEmpty else { return false }
        return !highlightOnly.contains(language)
    }

    /// A language as a person reads its name.
    static func name(_ id: String) -> String {
        names[id] ?? id
    }

    private static let names: [String: String] = [
        "rust": "Rust", "swift": "Swift", "typescript": "TypeScript", "tsx": "TSX", "javascript": "JavaScript",
        "python": "Python", "go": "Go", "c": "C", "cpp": "C++", "java": "Java", "csharp": "C#", "ruby": "Ruby",
        "php": "PHP", "kotlin": "Kotlin", "sql": "SQL", "bash": "Bash", "json": "JSON", "yaml": "YAML",
        "toml": "TOML", "html": "HTML", "css": "CSS", "markdown": "Markdown",
    ]
}

extension SchemaNode {
    /// A working directory's file's language, as the core decided it from its name (none for a file of no
    /// language, and for any other card).
    nonisolated var codeLanguage: String? {
        guard WorkDirIds.isFile(id), case .string(let language)? = ownFields["language"], !language.isEmpty else { return nil }
        return language
    }

    /// Whether it is a file whose functions and types are listed.
    nonisolated var isCodeFile: Bool { CodeLanguages.hasSymbols(codeLanguage) }
}

/// A function or type of a file as a path writes it: the file's path in full and its own path after a `.`
/// (`repo/src:'a.rs'.Order.total`), and how a person reads it beside the file (`a.rs › Order.total`).
nonisolated enum SymbolPath {
    static func full(_ file: String, _ symbol: String) -> String { "\(file).\(symbol)" }

    static func said(file name: String, _ symbol: String) -> String { "\(name) › \(symbol)" }

    /// The row's key on a file's card and the nav panel's id of a symbol (`sym:<dir>/<file>#<path>`).
    static func id(file: String, _ symbol: String) -> String {
        "sym:" + file.dropFirst("file:".count) + "#" + symbol
    }
}
