import Foundation

/// The places a piece of Markdown names in the project.
///
///     [[Core:Customer.email]]      a place: checked like any declared path, drawn
///                                  as a line, followed by the impact read
///     [[Core:Customer|客]]          a place shown by other words; the path alone is what
///                                  is followed
///     [[Store/Ordering:]]          a section: its path with the `:` and nothing after it
///     {{Ordering:Status}}          a value the project holds, put in where it is shown
///     ![[Ordering:Checkout]]       a diagram, drawn in its place (`NoteEmbed`);
///                                  `|sequence` after the path says how
///
/// Code is not read. Inside backticks and fenced blocks (``` or ~~~) the text is an
/// example — TOML's `[[part]]` is the usual one — so nothing there is a reference.
/// One scanner does every job — finding, rewriting and rendering — so what is
/// checked and what is shown can never disagree about where a reference is. A name quoted in the
/// path (`[[Docs:'release notes.md']]`) is one name to the scanner: a space, `]]`, `}}` or `|`
/// inside its quotes ends or splits nothing; a space outside them has it read as words.
nonisolated enum NoteText {

    struct Ref: Hashable {
        enum Kind: Hashable { case place, value, embed }
        var kind: Kind
        /// Between the brackets, trimmed — for an embed, before the `|`.
        var path: String
        /// The whole `[[…]]`, `{{…}}` or `![[…]]`, in the text it was found in.
        var range: Range<String.Index>
        /// An embed's `|sequence`: how to draw it, when said.
        var option: String? = nil
        /// A place's `|shown text`: what the link reads as instead of its path, when said. A
        /// rename rewrites the path and keeps it.
        var shown: String? = nil

        /// The reference as it was written, naming `path` instead.
        func written(_ path: String) -> String {
            switch kind {
            case .place: "[[\(path)\(shown.map { "|\($0)" } ?? "")]]"
            case .value: "{{\(path)}}"
            case .embed: "![[\(path)\(option.map { "|\($0)" } ?? "")]]"
            }
        }
    }

    static func refs(_ text: String) -> [Ref] {
        var out: [Ref] = []
        let bytes = Bytes(text: text, b: Array(text.utf8))
        forEachProse(text) { segment in out += refs(in: bytes, segment) }
        return out
    }

    /// The text with each reference replaced by what `replace` returns for it —
    /// nil leaves it as written.
    static func transform(_ text: String, _ replace: (Ref) -> String?) -> String {
        var out = ""
        var at = text.startIndex
        for ref in refs(text) {
            guard let replacement = replace(ref) else { continue }
            out += text[at..<ref.range.lowerBound]
            out += replacement
            at = ref.range.upperBound
        }
        out += text[at...]
        return out
    }

    /// Every `[[…]]`, `![[…]]` and `{{…}}` written in the text, closed where the scanner closes a
    /// reference (a quoted name in it ends nothing), whether or not what is in it is a path: what
    /// a text about references puts in code.
    static func bracketed(_ text: String) -> [Range<String.Index>] {
        let bytes = Bytes(text: text, b: Array(text.utf8))
        let to = bytes.b.count
        var out: [Range<String.Index>] = []
        var i = 0
        scan: while i < to {
            for (open, close) in [("![[", "]]"), ("[[", "]]"), ("{{", "}}")] where bytes.starts(i, to, with: open) {
                if let end = closing(bytes, i + open.utf8.count, to, close) {
                    out.append(bytes.index(i)..<bytes.index(end + 2))
                    i = end + 2
                    continue scan
                }
            }
            i += 1
        }
        return out
    }

    // MARK: - Finding the prose

    /// Calls `body` with each stretch of the text that is not code.
    private static func forEachProse(_ text: String, _ body: (Range<String.Index>) -> Void) {
        var fence: (char: Character, count: Int)?
        var lineStart = text.startIndex
        while lineStart < text.endIndex {
            let lineEnd = text[lineStart...].firstIndex(of: "\n") ?? text.endIndex
            let line = text[lineStart..<lineEnd]
            let trimmed = line.drop { $0 == " " }
            let indent = line.count - trimmed.count
            if indent <= 3, let first = trimmed.first, first == "`" || first == "~" {
                let run = trimmed.prefix { $0 == first }.count
                if run >= 3 {
                    if let open = fence {
                        if open.char == first && run >= open.count
                            && trimmed.dropFirst(run).allSatisfy(\.isWhitespace) {
                            fence = nil
                        }
                    } else {
                        fence = (first, run)
                    }
                    lineStart = lineEnd < text.endIndex ? text.index(after: lineEnd) : lineEnd
                    continue
                }
            }
            if fence == nil { outsideInlineCode(text, lineStart..<lineEnd, body) }
            lineStart = lineEnd < text.endIndex ? text.index(after: lineEnd) : lineEnd
        }
    }

    /// Within one line, the parts outside backtick code spans.
    private static func outsideInlineCode(_ text: String, _ line: Range<String.Index>,
                                          _ body: (Range<String.Index>) -> Void) {
        var start = line.lowerBound
        var i = line.lowerBound
        while i < line.upperBound {
            guard text[i] == "`" else { i = text.index(after: i); continue }
            var runEnd = i
            while runEnd < line.upperBound, text[runEnd] == "`" { runEnd = text.index(after: runEnd) }
            let run = text.distance(from: i, to: runEnd)
            // The closing run of the same length, or none: then it is a backtick.
            var j = runEnd
            var close: Range<String.Index>?
            while j < line.upperBound {
                guard text[j] == "`" else { j = text.index(after: j); continue }
                var k = j
                while k < line.upperBound, text[k] == "`" { k = text.index(after: k) }
                if text.distance(from: j, to: k) == run { close = j..<k; break }
                j = k
            }
            guard let close else { i = runEnd; continue }
            if start < i { body(start..<i) }
            start = close.upperBound
            i = close.upperBound
        }
        if start < line.upperBound { body(start..<line.upperBound) }
    }

    // MARK: - Finding the references

    /// A stretch of the text by UTF-8 offsets: what the references are found in, as the core finds
    /// them (`notetext.rs`). Every character that opens, closes or splits a reference is ASCII, so
    /// an offset where one is found is the edge of a character.
    private struct Bytes {
        let text: String
        let b: [UInt8]

        func offset(_ i: String.Index) -> Int { text.utf8.distance(from: text.startIndex, to: i) }
        func index(_ o: Int) -> String.Index { text.utf8.index(text.startIndex, offsetBy: o) }
        func string(_ from: Int, _ to: Int) -> String { String(decoding: b[from..<to], as: UTF8.self) }
        func starts(_ at: Int, _ to: Int, with prefix: String) -> Bool {
            let p = Array(prefix.utf8)
            return at + p.count <= to && b[at..<(at + p.count)].elementsEqual(p)
        }
    }

    private static func refs(in bytes: Bytes, _ segment: Range<String.Index>) -> [Ref] {
        let to = bytes.offset(segment.upperBound)
        var out: [Ref] = []
        var i = bytes.offset(segment.lowerBound)
        while i < to {
            if bytes.starts(i, to, with: "![["), let (ref, end) = embed(bytes, from: i, to: to) {
                out.append(ref); i = end; continue
            }
            if bytes.starts(i, to, with: "[["), let (ref, end) = place(bytes, from: i, to: to) {
                out.append(ref); i = end; continue
            }
            if bytes.starts(i, to, with: "{{"), let (ref, end) = value(bytes, from: i, to: to) {
                out.append(ref); i = end; continue
            }
            i += 1
        }
        return out
    }

    /// Where `close` (`]]`, `}}`) first comes in a line from `open`, outside the names quoted in
    /// the path and its `[…]` (`subjects[]`); none past the line's end.
    private static func closing(_ bytes: Bytes, _ open: Int, _ to: Int, _ close: String) -> Int? {
        let lineEnd = bytes.b[open..<to].firstIndex(of: UInt8(ascii: "\n")) ?? to
        let line = Array(bytes.b[open..<lineEnd])
        let c = Array(close.utf8)
        return PathText.findOutside(line) { i in
            i + c.count <= line.count && line[i..<(i + c.count)].elementsEqual(c)
        }.map { open + $0 }
    }

    /// Whether what is written between the brackets is a path: not empty, and no space or tab
    /// outside its quoted names (prose in brackets is not one).
    private static func isPath(_ path: String) -> Bool {
        let b = Array(path.utf8)
        return !b.isEmpty && PathText.findOutside(b, { b[$0] == UInt8(ascii: " ") || b[$0] == UInt8(ascii: "\t") }) == nil
    }

    /// `[[` … `]]`, where the path may hold its own `[]` — `subjects[]`. A `|` outside the path's
    /// quoted names ends the path; what follows it to the first `]]` is the text shown, read as
    /// prose (quotes, dots and spaces in it are its own). An empty one is as if none were written.
    private static func place(_ bytes: Bytes, from start: Int, to: Int) -> (Ref, Int)? {
        let open = start + 2
        let lineEnd = bytes.b[open..<to].firstIndex(of: UInt8(ascii: "\n")) ?? to
        let line = Array(bytes.b[open..<lineEnd])
        let bar = UInt8(ascii: "|"), close = UInt8(ascii: "]")
        func closes(_ i: Int) -> Bool { i + 1 < line.count && line[i] == close && line[i + 1] == close }
        guard let stop = PathText.findOutside(line, { line[$0] == bar || closes($0) }) else { return nil }
        var end = stop + 2
        var shown: String?
        if line[stop] == bar {
            guard let shut = (stop + 1..<line.count).first(where: closes) else { return nil }
            let said = String(decoding: line[(stop + 1)..<shut], as: UTF8.self).trimmingCharacters(in: .whitespaces)
            shown = said.isEmpty ? nil : said
            end = shut + 2
        }
        let path = String(decoding: line[..<stop], as: UTF8.self).trimmingCharacters(in: .whitespaces)
        guard isPath(path) else { return nil }
        return (Ref(kind: .place, path: path, range: bytes.index(start)..<bytes.index(open + end), shown: shown),
                open + end)
    }

    /// `![[` … `]]`: a place with a `!` before it, and what follows a `|` (outside the path's
    /// quoted names) in it.
    private static func embed(_ bytes: Bytes, from start: Int, to: Int) -> (Ref, Int)? {
        let open = start + 3
        guard let i = closing(bytes, open, to, "]]") else { return nil }
        let inner = bytes.string(open, i).trimmingCharacters(in: .whitespaces)
        var path = inner
        var option: String?
        if let bar = PathText.firstOutside(inner, "|") {
            path = inner[..<bar].trimmingCharacters(in: .whitespaces)
            option = String(inner[inner.index(after: bar)...])
        }
        guard isPath(path), !(option?.contains(" ") ?? false) else { return nil }
        return (Ref(kind: .embed, path: path, range: bytes.index(start)..<bytes.index(i + 2), option: option), i + 2)
    }

    private static func value(_ bytes: Bytes, from start: Int, to: Int) -> (Ref, Int)? {
        let open = start + 2
        guard let close = closing(bytes, open, to, "}}") else { return nil }
        let path = bytes.string(open, close).trimmingCharacters(in: .whitespaces)
        guard isPath(path) else { return nil }
        return (Ref(kind: .value, path: path, range: bytes.index(start)..<bytes.index(close + 2)), close + 2)
    }
}
