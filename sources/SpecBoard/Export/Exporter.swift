import Foundation

// MARK: - Names as code writes them

/// A name made from a flow's words (`FlowNodes`). What a project is written out as is the core's
/// (`export_files`).
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
}
