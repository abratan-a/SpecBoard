import Foundation

// MARK: - Names as code writes them

/// What the window's own diagrams write names and words in: a flow's chart and sequence
/// diagram as mermaid (`FlowDiagram`, `ActionDiagram`) and a name made from a flow's words
/// (`FlowNodes`). What a project is written out as is the core's (`export_files`).
enum Ident {

    /// Make a name safe to use as a TypeScript identifier.
    static func safe(_ name: String) -> String {
        var cleaned = String(name.map { char -> Character in
            char.isLetter && char.isASCII || char.isNumber && char.isASCII
                || char == "_" || char == "$" ? char : "_"
        })
        if let first = cleaned.first, first.isNumber { cleaned = "_" + cleaned }
        return cleaned.isEmpty ? "_" : cleaned
    }

    /// Text for a `//` comment, kept on its line: a line break in a name or a note
    /// would put the rest of it in the code.
    static func lineComment(_ text: String) -> String {
        let breaks: Set<Unicode.Scalar> = ["\n", "\r", "\u{2028}", "\u{2029}"]
        return String(String.UnicodeScalarView(text.unicodeScalars.map { breaks.contains($0) ? " " : $0 }))
    }

    /// Text for a mermaid label, kept inside its quotes and on its line: `"` would end
    /// the label, `#` starts an entity, and a line break a new statement.
    static func mermaidText(_ text: String) -> String {
        lineComment(text).replacingOccurrences(of: "\"", with: "'").replacingOccurrences(of: "#", with: "＃")
    }
}
