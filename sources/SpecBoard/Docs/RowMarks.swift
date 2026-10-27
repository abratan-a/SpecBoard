import Foundation

/// A formatdoc as it reads, with where each row starts marked in its text (`FormatDocEditor.document`), so
/// that the page made of it (`MarkdownPage.html`) holds each row in a block of its own: what the right-click
/// on a row and the brackets of the items in the margin are drawn from
/// (`specboard://SpecBoard/Docs/Design:UI` §5「formatdoc の文書の見え方」).
///
/// The marks are in the text rather than beside it because the text is what a card's picture is kept by
/// (`DocumentPage`) and what the references are resolved in (`MarkdownRefs.resolve`, `DocumentParts.page`):
/// they pass through both as they are. A mark is a line of its own starting with U+001E, which a row's own
/// text never holds (`clean`); its fields are percent-encoded, so a heading's `[[…]]` is not read as a
/// reference and no space or line break splits it.
nonisolated enum RowMarks {

    static let lead: Character = "\u{1E}"

    /// The page's left margin, in points, that the brackets are drawn in on the card editor's page; a card's
    /// picture has its own, narrower (`cardGutter`).
    static let gutter: CGFloat = 40
    static let cardGutter: CGFloat = 16
    /// Room kept on the right and below the rows on the card editor's page, for the outline of a lit row to
    /// show there rather than be cut at the page's edge.
    static let edge: CGFloat = 8

    struct Mark: Equatable {
        enum Kind: String { case item, row }

        var kind: Kind
        /// 0 at the top, as `FormatRows.walk` counts it.
        var depth: Int
        /// In the document, as a path writes it: `auth.login`.
        var row: String
        /// In full: `Docs:Spec.auth.login`.
        var path: String
        /// An item's heading; empty for any other row.
        var heading: String = ""
    }

    struct Block: Equatable {
        var mark: Mark
        var markdown: String
    }

    struct Marked: Equatable {
        /// What comes before the first row: the document's title.
        var head: String
        var blocks: [Block]

        /// It has items, and so brackets to draw in the margin.
        var outlined: Bool { blocks.contains { $0.mark.kind == .item } }
    }

    /// The line a row starts with.
    static func line(_ mark: Mark) -> String {
        String(lead) + [mark.kind.rawValue, String(mark.depth), encode(mark.row), encode(mark.path), encode(mark.heading)]
            .joined(separator: " ")
    }

    /// A row's own text with the marks' character taken out, so that nothing it says is read as a mark.
    static func clean(_ text: String) -> String {
        text.replacingOccurrences(of: String(lead), with: "\u{FFFD}")
    }

    /// The text read as its title and its rows, or nil when it has no row marked: any other Markdown.
    static func parse(_ text: String) -> Marked? {
        var head = ""
        var blocks: [Block] = []
        var current: Mark?
        var lines: [String] = []
        func close() {
            let said = lines.joined(separator: "\n").trimmingCharacters(in: .newlines)
            if let current { blocks.append(Block(mark: current, markdown: said)) } else { head = said }
            lines = []
        }
        // Split on the line feed alone: a row's own text may hold "\r\n", which stays as it is written.
        for line in text.components(separatedBy: "\n") {
            if line.first == lead, let mark = mark(line) {
                close()
                current = mark
            } else {
                lines.append(line)
            }
        }
        close()
        return blocks.isEmpty ? nil : Marked(head: head, blocks: blocks)
    }

    /// The text without its marks: the document as it reads, as one Markdown.
    static func plain(_ text: String) -> String {
        guard let marked = parse(text) else { return text }
        return ([marked.head] + marked.blocks.map(\.markdown)).filter { !$0.isEmpty }.joined(separator: "\n\n")
    }

    /// The page's body: the title, then each row a `section` holding what it reads as, a row inside an item
    /// inside the item's section. An item's section starts with its bracket — the outline in the margin —
    /// and its heading carries the tag that shows its heading and path while the pointer rests on the
    /// bracket. Each row is made HTML on its own, so a row's Markdown cannot run on into the next.
    static func html(_ marked: Marked, render: (String) -> String = Markdown.html) -> String {
        var out = marked.head.isEmpty ? "" : render(marked.head)
        var open: [Int] = []
        for block in marked.blocks {
            let mark = block.mark
            while let last = open.last, last >= mark.depth {
                out += "</section>"
                open.removeLast()
            }
            let row = attribute(mark.row)
            switch mark.kind {
            case .item:
                out += "<section class=\"sb-row sb-item\" data-row=\"\(row)\" data-depth=\"\(mark.depth)\">"
                    + "<span class=\"sb-bracket\" aria-hidden=\"true\"></span>"
                    + tagged(render(block.markdown), mark)
                open.append(mark.depth)
            case .row:
                out += "<section class=\"sb-row\" data-row=\"\(row)\">" + render(block.markdown) + "</section>"
            }
        }
        return out + String(repeating: "</section>", count: open.count)
    }

    /// The heading with the tag put at its end, inside it: placed after its words without taking room.
    private static func tagged(_ heading: String, _ mark: Mark) -> String {
        let tag = "<span class=\"sb-tag\" aria-hidden=\"true\" data-heading=\"\(attribute(mark.heading))\" "
            + "data-path=\"\(attribute(mark.path))\"></span>"
        guard let end = heading.range(of: "</h", options: .backwards) else { return heading + tag }
        return heading.replacingCharacters(in: end.lowerBound..<end.lowerBound, with: tag)
    }

    private static func mark(_ line: String) -> Mark? {
        let fields = line.dropFirst().split(separator: " ", omittingEmptySubsequences: false)
        guard fields.count == 5, let kind = Mark.Kind(rawValue: String(fields[0])), let depth = Int(fields[1]), depth >= 0,
              let row = decode(fields[2]), !row.isEmpty, let path = decode(fields[3]), let heading = decode(fields[4])
        else { return nil }
        return Mark(kind: kind, depth: depth, row: row, path: path, heading: heading)
    }

    /// Letters, digits and the few marks a path is mostly made of are left as they are, to be read in a test.
    private static let plainCharacters = CharacterSet(charactersIn: "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_.:/")

    private static func encode(_ text: String) -> String {
        text.addingPercentEncoding(withAllowedCharacters: plainCharacters) ?? ""
    }

    private static func decode(_ text: Substring) -> String? {
        String(text).removingPercentEncoding
    }

    /// A value put in an attribute between double quotes.
    private static func attribute(_ text: String) -> String {
        text.replacingOccurrences(of: "&", with: "&amp;").replacingOccurrences(of: "\"", with: "&quot;")
            .replacingOccurrences(of: "<", with: "&lt;").replacingOccurrences(of: ">", with: "&gt;")
            .replacingOccurrences(of: "'", with: "&#39;")
    }
}
