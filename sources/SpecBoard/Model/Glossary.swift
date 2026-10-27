import Foundation

/// The words the screen and the MCP share (`specboard://SpecBoard/Docs/Design:Overview` §3): the same as the core's
/// glossary (`sources/core/src/mcp/catalog.json` の `glossary`、its English in `mcp/text/en.json`, MCP's
/// `guide` with `about: glossary`), word for word in every language.
///
/// One list, so a person and an agent reading the same project use the same word for
/// the same thing. The tooltips and `specboard://SpecBoard/Docs/Design:Glossary` are made from it; a word is the
/// key it is written with, so what a person sees is what an agent writes. The lines'
/// words are the names of the kinds of line (`Line`), and the legend on screen is made
/// from the kinds' definitions (`LineKinds`: label, what it means, how it is drawn).
///
/// The words and their order are here; what each means, in each language, is in the String Catalog
/// `Strings/Glossary.xcstrings` (`term.<word>`, `legend.<word>`), so the screen says it in its language.
enum Glossary {
    /// The table the meanings are in.
    nonisolated static let table = "Glossary"

    struct Term: Hashable {
        /// As it is written — a key, a kind, a value.
        var word: String

        /// What it means, in the screen's language.
        var meaning: String { meaning(in: Language.shared.code) }

        nonisolated func meaning(in code: String) -> String {
            Glossary.text("term.\(word)", in: code)
        }
    }

    struct Group {
        /// Its title's key in the table (`group.<id>`).
        var id: String
        var terms: [Term]

        init(_ id: String, _ words: [String]) {
            self.id = id
            terms = words.map { Term(word: $0) }
        }

        nonisolated func title(in code: String) -> String { Glossary.text("group.\(id)", in: code) }
    }

    /// A row of the legend's table: one kind of line, or the project's own the move makes.
    struct LegendRow: Hashable {
        var words: [String]

        nonisolated func meaning(in code: String) -> String {
            Glossary.text("legend.\(words.first ?? "")", in: code)
        }
    }

    static let groups: [Group] = [
        Group("project", ["project", "main", "branch", "section", "project_tree", "free", "ui", "mock", "board", "ghost",
                          "standalone", "base", "merge", "path", "in"]),
        Group("kinds", ["entity", "value", "enum", "flow", "store", "instance", "table", "note", "freedoc", "formatdoc",
                        "stackdoc", "svg", "html", "frame", "goal", "task", "file", "workdir", "directory", "symbol",
                        "parsers"]),
        Group("nodes", ["begin", "trigger", "process", "run", "eval", "wait", "join", "end", "diagram", "topLeft"]),
        Group("notes", ["body", "rows", "versions", "version"]),
        Group("fields", ["required", "nullable", "const", "data"]),
        Group("lines", ["connections", "line", "embed", "points", "of", "at", "fromBoard"]),
        Group("steps", ["receive", "return", "call", "send", "read", "write", "sign", "verify", "compute", "via",
                        "initiator"]),
    ]

    static let legend: [LegendRow] = [
        "embed", "points", "link", "holds", "shapeOf", "rules", "implementedBy", "mentions", "next", "condition", "else",
        "onError", "onTimeout", "runs", "awaits", "reads", "writes", "partner", "key",
    ].map { LegendRow(words: [$0]) }

    static func term(_ word: String) -> Term? {
        groups.lazy.flatMap(\.terms).first { $0.word == word }
    }

    /// A text of the glossary's table in `code`.
    nonisolated static func text(_ key: String, in code: String) -> String {
        L10n.text(key: key, table: table, in: code)
    }

    /// The glossary as a document, for a person (the project's `specboard://SpecBoard/Docs/Design:Glossary`, written
    /// in Japanese): the core's glossary, as MCP's `guide` gives it.
    static var markdown: String { markdown(in: "ja") }

    /// The glossary as a document in `code`. The ways a body names a place (`[[パス]]` and the like), written in a
    /// meaning, are put in code: the document is a card of the project, and there they would be read as paths.
    static func markdown(in code: String) -> String {
        let say = { (key: String) in text(key, in: code) }
        let head = "| \(say("document.word")) | \(say("document.meaning")) |\n|---|---|\n"
        var out = "# \(say("document.title"))\n\n\(say("document.lead"))\n"
        for group in groups {
            out += "\n## \(group.title(in: code))\n\n" + head
            for term in group.terms { out += "| `\(term.word)` | \(cell(term.meaning(in: code))) |\n" }
        }
        out += "\n## \(say("legend.title"))\n\n\(say("legend.note"))\n\n" + head
        for row in legend {
            let words = row.words.map { "`\($0)`" }.joined(separator: say("document.wordsJoined"))
            out += "| \(words) | \(cell(row.meaning(in: code))) |\n"
        }
        return out
    }

    /// A meaning as a table cell: a `|` in it would end the cell, even inside code.
    private static func cell(_ text: String) -> String {
        asCode(text).replacingOccurrences(of: "|", with: "\\|")
    }

    /// `[[…]]`, `![[…]]` and `{{…}}` in words, put in code: each where the note scanner closes it
    /// (`NoteText.bracketed`).
    private static func asCode(_ text: String) -> String {
        var out = ""
        var at = text.startIndex
        for span in NoteText.bracketed(text) {
            out += text[at..<span.lowerBound] + "`" + text[span] + "`"
            at = span.upperBound
        }
        return out + text[at...]
    }
}
