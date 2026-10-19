import Foundation

/// Paths as they are written, and the names in them (`specboard://SpecBoard/Docs/Design:DesignSections`
/// 「名前に使える字と、パスの中での囲み」): the app's one place a path is taken apart into its names and
/// written back from them, read as the core reads it (`pathtext.rs`) so that the two never disagree
/// on where one name ends.
///
///     Store/Ordering:Order.lines[].sku     sections / card . names, an array's elements
///     repo/docs/design:'CardKinds.md'      a name holding a character a path splits on, quoted
///     Notes:'Owner''s guide'               a quote inside a quoted name, doubled
///
/// A name is quoted only when it holds whitespace or one of `needsQuotes`; everything else
/// (Japanese, `-`, `&`, `+`, …) is written bare. Reading, a name that starts with `'` runs to the
/// quote that closes it; a bare one is split where paths were always split, so a path written
/// before names could hold these reads the same.
///
/// The core's case table (`sources/core/tests/fixtures/paths.json`) holds what this reads and
/// writes, and the app's tests read the same file.
///
/// Positions are UTF-8 offsets, as the core's are: every character a path splits on is ASCII, so
/// an offset where one is found is always the edge of a character.
nonisolated enum PathText {

    /// The most characters a name has.
    static let nameMax = 255

    /// The characters that have a name quoted in a path, besides whitespace: the path's own
    /// separators (`/ : . [ ]`), the quote, the tree of before (`@`), what ends or splits a
    /// reference in a text (`| { }`), a line's row address (`#`), a pick's quoted value (`"`) and
    /// Markdown's code (`` ` ``).
    static let quoted: Set<Unicode.Scalar> = ["/", ":", ".", "[", "]", "'", "@", "|", "{", "}", "#", "\"", "`"]

    // MARK: - Names

    /// Why a name cannot be one.
    enum NameProblem: String {
        case empty
        case tooLong
        /// A line break, a tab or another control character.
        case control
        /// Whitespace before or after it (a name is kept trimmed).
        case spaces
    }

    /// Why `name` (as it is kept, trimmed) cannot be a name: none when it can.
    static func nameProblem(_ name: String) -> NameProblem? {
        if name.isEmpty { return .empty }
        if name.unicodeScalars.count > nameMax { return .tooLong }
        if name.unicodeScalars.contains(where: isControl) { return .control }
        if trimmed(name) != name { return .spaces }
        return nil
    }

    /// A name as it is typed, made the name it is kept as: spaces around it dropped.
    static func trimmed(_ name: String) -> String {
        let scalars = name.unicodeScalars
        guard let first = scalars.firstIndex(where: { !$0.properties.isWhitespace }),
              let last = scalars.lastIndex(where: { !$0.properties.isWhitespace }) else { return "" }
        return String(scalars[first...last])
    }

    /// A name as it is compared with another in one place: ASCII letters in any case are one (as
    /// the core's store compares them), every other character as it is (`É` and `é` are two).
    static func fold(_ name: String) -> String {
        String(decoding: name.utf8.map(lower), as: UTF8.self)
    }

    /// Whether two names are the same name (`fold`).
    static func same(_ a: String, _ b: String) -> Bool {
        a.utf8.count == b.utf8.count && zip(a.utf8, b.utf8).allSatisfy { lower($0) == lower($1) }
    }

    /// Whether a name is quoted in a path.
    static func needsQuotes(_ name: String) -> Bool {
        name.isEmpty || name.unicodeScalars.contains {
            $0.properties.isWhitespace || isControl($0) || quoted.contains($0)
        }
    }

    /// A name as it is written in a path: quoted when it needs it, a quote inside doubled.
    static func quote(_ name: String) -> String {
        needsQuotes(name) ? "'" + name.replacingOccurrences(of: "'", with: "''") + "'" : name
    }

    /// A pick's key (a field's name inside `[…]`) as it is written: quoted as a name is, and when
    /// it holds what splits a pick (`=` `,`).
    static func quoteKey(_ name: String) -> String {
        if (name.contains("=") || name.contains(",")) && !needsQuotes(name) {
            return "'" + name.replacingOccurrences(of: "'", with: "''") + "'"
        }
        return quote(name)
    }

    /// Sections' names written as a section's path (`Store/Ordering`).
    static func joinSections<S: Sequence>(_ names: S) -> String where S.Element: StringProtocol {
        names.map { quote(String($0)) }.joined(separator: "/")
    }

    /// Names written one after another inside a card (`Checkout.review`, `lines.sku`).
    static func joinNames<S: Sequence>(_ names: S) -> String where S.Element: StringProtocol {
        names.map { quote(String($0)) }.joined(separator: ".")
    }

    /// A card's path: its section's path (as written) and its name.
    static func cardPath(_ section: String, _ card: String) -> String {
        "\(section):\(quote(card))"
    }

    /// A path (as written) and a name inside what it names.
    static func member(_ path: String, _ name: String) -> String {
        "\(path).\(quote(name))"
    }

    /// A section's path (as written, none for the top) and the name of a section inside it.
    static func child(_ parent: String?, _ name: String) -> String {
        guard let parent, !parent.isEmpty else { return quote(name) }
        return "\(parent)/\(quote(name))"
    }

    // MARK: - Reading

    /// Why a path cannot be read.
    enum PathError: String, Error {
        /// Nothing written.
        case empty
        /// A quote that opens a name and never closes.
        case unclosedQuote
        /// Something right after a quoted name or a `[…]`, before the next separator.
        case stray
        /// A name with nothing in it (`Store::User`, `a..b`, `''`).
        case emptyName
        /// A `[` that never closes, or a `"` in it.
        case unclosedBracket
        /// `[…]` after a section's name.
        case bracketInSection
    }

    /// The end of a quoted name that starts at `open` (its `'`): the offset just past the quote
    /// that closes it; none when it never closes.
    static func quotedEnd(_ b: [UInt8], _ open: Int) -> Int? {
        var i = open + 1
        while i < b.count {
            if b[i] == quote8 {
                if i + 1 < b.count, b[i + 1] == quote8 { i += 2; continue }
                return i + 1
            }
            i += 1
        }
        return nil
    }

    /// The end of a pick's quoted value that starts at `open` (its `"`, `\` escaping the next
    /// character): the offset just past the closing `"`.
    private static func stringEnd(_ b: [UInt8], _ open: Int) -> Int? {
        var i = open + 1
        while i < b.count {
            switch b[i] {
            case UInt8(ascii: "\\"): i += 2
            case UInt8(ascii: "\""): return i + 1
            default: i += 1
            }
        }
        return nil
    }

    /// The end of a `[…]` that starts at `open` (its `[`): the offset just past the `]` that closes
    /// it — keys quoted as names, values quoted with `"`, and `[…]` inside it counted.
    static func bracketEnd(_ b: [UInt8], _ open: Int) -> Int? {
        var depth = 0
        var keyStart = true
        var i = open
        while i < b.count {
            let c = b[i]
            if keyStart && isSpace(c) { i += 1; continue }
            if keyStart && c == quote8 && depth == 1 {
                guard let end = quotedEnd(b, i) else { return nil }
                i = end
                keyStart = false
                continue
            }
            keyStart = false
            switch c {
            case UInt8(ascii: "\"") where depth > 0:
                guard let end = stringEnd(b, i) else { return nil }
                i = end
                continue
            case UInt8(ascii: "["):
                depth += 1
                keyStart = depth == 1
            case UInt8(ascii: "]"):
                depth -= 1
                if depth == 0 { return i + 1 }
            case UInt8(ascii: ",") where depth == 1:
                keyStart = true
            default:
                break
            }
            i += 1
        }
        return nil
    }

    /// A separator of a path outside every quoted name and `[…]`, and where it is.
    private struct Mark {
        var at: Int
        var sep: UInt8
    }

    /// The separators of a written path (`/ : . @`) outside quoted names and `[…]`, and the first
    /// reason it cannot be read, when there is one (the separators up to there are found).
    private static func marks(_ b: [UInt8]) -> (marks: [Mark], error: PathError?) {
        var out: [Mark] = []
        var error: PathError?
        var start = true
        // After a quoted name only its `[…]` or spaces until the next separator; after a `[…]`,
        // only spaces.
        var quotedName = false, picked = false
        var i = 0
        while i < b.count {
            let c = b[i]
            if start && isSpace(c) { i += 1; continue }
            if start && c == quote8 {
                guard let end = quotedEnd(b, i) else { return (out, error ?? .unclosedQuote) }
                i = end
                start = false
                quotedName = true
                continue
            }
            start = false
            switch c {
            case UInt8(ascii: "/"), UInt8(ascii: ":"), UInt8(ascii: "."), UInt8(ascii: "@"):
                out.append(Mark(at: i, sep: c))
                start = true
                (quotedName, picked) = (false, false)
            case UInt8(ascii: " "), UInt8(ascii: "\t"):
                break
            case UInt8(ascii: "[") where !picked:
                guard let end = bracketEnd(b, i) else { return (out, error ?? .unclosedBracket) }
                i = end
                picked = true
                continue
            default:
                if quotedName || picked { error = error ?? .stray }
            }
            i += 1
        }
        return (out, error)
    }

    /// Where the first offset `wanted` takes lies in text that starts with a written path, outside
    /// quoted names and `[…]`: a quote opens a name at the start and after a separator (`/ : . @`),
    /// spaces before it let be. None when it is not there, or a quote or a `[` before it never
    /// closes.
    static func findOutside(_ b: [UInt8], _ wanted: (Int) -> Bool) -> Int? {
        var start = true
        var i = 0
        while i < b.count {
            if wanted(i) { return i }
            let c = b[i]
            if start && c == quote8 {
                guard let end = quotedEnd(b, i) else { return nil }
                i = end
                start = false
                continue
            }
            switch c {
            case UInt8(ascii: "/"), UInt8(ascii: ":"), UInt8(ascii: "."), UInt8(ascii: "@"):
                start = true
                i += 1
                continue
            case UInt8(ascii: "["):
                guard let end = bracketEnd(b, i) else { return nil }
                i = end
                start = false
                continue
            default:
                break
            }
            if !isSpace(c) { start = false }
            i += 1
        }
        return nil
    }

    /// The first offset in `text` (a written path) where `byte` stands outside quoted names and
    /// `[…]`: an ASCII character the path's own reading lets through (`|`, a space).
    static func firstOutside(_ text: String, _ byte: Character) -> String.Index? {
        guard let ascii = byte.asciiValue else { return nil }
        let b = Array(text.utf8)
        guard let at = findOutside(b, { b[$0] == ascii }) else { return nil }
        return text.utf8.index(text.startIndex, offsetBy: at)
    }

    /// The first `:` outside quoted names and `[…]`: where a path's sections end and its card
    /// begins, as an offset.
    private static func colon(_ b: [UInt8]) -> Int? {
        marks(b).marks.first { $0.sep == colon8 }?.at
    }

    /// Whether a path says its section (`Core:User`, not `User`).
    static func hasSection(_ text: String) -> Bool {
        colon(Array(text.utf8)) != nil
    }

    /// The sections of a path that names a section alone (`Store/Ordering:`): a `:` written outside
    /// quoted names with nothing after it. Nil for a card's path and for a path without a `:` — a
    /// section is written with its `:`, so it is never read as a card of the same name.
    static func sectionOnly(_ text: String) -> String? {
        let b = trimSpaces(Array(text.utf8))
        guard let c = colon(b), trimSpaces(Array(b[(c + 1)...])).isEmpty else { return nil }
        let section = trimSpaces(Array(b[..<c]))
        return section.isEmpty ? nil : slice(section, 0, section.count)
    }

    /// A path cut at its `:`: the sections (none without one) and what follows.
    static func splitSection(_ text: String) -> (section: String?, rest: String) {
        let b = Array(text.utf8)
        guard let c = colon(b) else { return (nil, text) }
        return (slice(b, 0, c), slice(b, c + 1, b.count))
    }

    /// A path written with the tree of before (`main@Store:Order`): the tree and the rest. Only an
    /// `@` outside quoted names and before every other separator is one.
    static func treePrefix(_ text: String) -> (tree: String, rest: String)? {
        let b = Array(text.utf8)
        guard let first = marks(b).marks.first, first.sep == at8 else { return nil }
        return (slice(b, 0, first.at), slice(b, first.at + 1, b.count))
    }

    /// A path with the tree of before dropped (`main@Core:User` → `Core:User`, `main@:User` →
    /// `User`); one without it as it is.
    static func withoutTree(_ text: String) -> String {
        guard let (_, rest) = treePrefix(text) else { return text }
        return rest.hasPrefix(":") ? String(rest.dropFirst()) : rest
    }

    private static func split(_ text: String, at sep: UInt8) -> [String] {
        let b = Array(text.utf8)
        var out: [String] = []
        var from = 0
        for m in marks(b).marks where m.sep == sep {
            out.append(slice(b, from, m.at))
            from = m.at + 1
        }
        out.append(slice(b, from, b.count))
        return out
    }

    /// A section's path split into its names as written (`Store/'设计'` → `Store`, `'设计'`).
    static func slashes(_ text: String) -> [String] {
        split(text, at: slash8)
    }

    /// The names after a card's section split as written: the card's, then the names inside it,
    /// each with its `[…]` (`Order.lines[].sku`, `Hosts.ports[ip=192.0.2.1]`).
    static func dots(_ text: String) -> [String] {
        split(text, at: dot8)
    }

    /// A section's path cut before its last name: the parent's path and the name as written.
    static func lastSlash(_ text: String) -> (parent: String, name: String)? {
        let b = Array(text.utf8)
        guard let m = marks(b).marks.last(where: { $0.sep == slash8 }) else { return nil }
        return (slice(b, 0, m.at), slice(b, m.at + 1, b.count))
    }

    /// A path cut before its last name inside a card: the path before it and the name as written.
    static func lastDot(_ text: String) -> (head: String, name: String)? {
        let b = Array(text.utf8)
        guard let m = marks(b).marks.last(where: { $0.sep == dot8 }) else { return nil }
        return (slice(b, 0, m.at), slice(b, m.at + 1, b.count))
    }

    /// The last name a path names, read: the last inside a card, else the card's, else its last
    /// section's. One that cannot be read is its own name.
    static func lastName(_ text: String) -> String {
        guard case .success(let parts) = parse(text) else { return text }
        return parts.names.last ?? parts.sections.last ?? parts.tree ?? text
    }

    /// One name of a path as written, read: its quotes taken off (`'a''b'` → `a'b`), a bare one as
    /// it is; spaces around it dropped. Its `[…]`, if any, is the rest (`step`).
    static func unquote(_ written: String) -> String {
        step(written).name
    }

    /// One name of a path as written, read into the name and what follows it: the `[…]` that picks
    /// in it (`ports[0]` → `ports`, `[0]`), or nothing.
    static func step(_ written: String) -> (name: String, rest: String) {
        let b = trimSpaces(Array(written.utf8))
        if b.first == quote8, let end = quotedEnd(b, 0) {
            let inner = slice(b, 1, end - 1)
            return (inner.replacingOccurrences(of: "''", with: "'"),
                    String(decoding: trimSpaces(Array(b[end...])), as: UTF8.self))
        }
        if let open = b.firstIndex(of: UInt8(ascii: "[")) {
            return (String(decoding: trimSpaces(Array(b[..<open])), as: UTF8.self), slice(b, open, b.count))
        }
        return (String(decoding: b, as: UTF8.self), "")
    }

    /// One name of a path as written, written again as it is kept: the name quoted only where it
    /// needs it, what it picks as written (`'ports'[0]` → `ports[0]`).
    static func canonicalStep(_ written: String) -> String {
        let (name, rest) = step(written)
        return quote(name) + rest
    }

    /// One name of a path as written, read for a place that picks no element: the name, `[]`
    /// dropped; one that picks (`ports[0]`) as it is written, which names nothing there.
    static func plain(_ written: String) -> String {
        let text = String(decoding: trimSpaces(Array(written.utf8)), as: UTF8.self)
        let (name, rest) = step(text)
        let r = trimSpaces(Array(rest.utf8))
        if r.isEmpty || (r.first == UInt8(ascii: "[") && r.last == UInt8(ascii: "]") && r.count >= 2
                         && trimSpaces(Array(r[1..<(r.count - 1)])).isEmpty) {
            return name
        }
        return text
    }

    /// The names written one after another inside a card, read (`lines[].sku` → `lines`, `sku`):
    /// each unquoted, its `[…]` dropped.
    static func names(_ written: String) -> [String] {
        dots(written).map(unquote)
    }

    /// Names written one after another inside a card, written again as they are kept (quoted only
    /// where they need it), their `[…]` dropped.
    static func canonicalNames(_ written: String) -> String {
        joinNames(names(written))
    }

    /// Whether a path is one name alone (no section, nothing inside it): a name beside another.
    static func isOneName(_ written: String) -> Bool {
        let text = trimmed(written)
        return !text.isEmpty && marks(Array(text.utf8)).marks.isEmpty
    }

    /// Names written one after another with each `[]` dropped (`lines[].sku` → `lines.sku`), as
    /// written otherwise.
    static func withoutAlls(_ written: String) -> String {
        dots(written).map(withoutAll).joined(separator: ".")
    }

    /// A name of a path as written with a `[]` at its end dropped (`lines[]` → `lines`), quotes and
    /// all.
    static func withoutAll(_ written: String) -> String {
        let b = trimSpaces(Array(written.utf8))
        let text = String(decoding: b, as: UTF8.self)
        let (_, rest) = step(text)
        guard String(decoding: trimSpaces(Array(rest.utf8)), as: UTF8.self) == "[]" else { return text }
        let open = b.count - rest.utf8.count
        return String(decoding: trimSpaces(Array(b[..<open])), as: UTF8.self)
    }

    /// A pick (what is inside `[…]`) cut into its parts at its `,`: each a key as written (empty
    /// for `=value`) and the value as written (none for a key alone).
    static func pickParts(_ inner: String) -> [(key: String, value: String?)] {
        let b = Array(inner.utf8)
        var out: [(key: String, value: String?)] = []
        var from = 0
        var keyStart = true
        var eq: Int?
        var i = 0
        func push(_ from: Int, _ to: Int, _ eq: Int?) {
            if let eq {
                out.append((slice(b, from, eq), slice(b, eq + 1, to)))
            } else {
                out.append((slice(b, from, to), nil))
            }
        }
        while i < b.count {
            let c = b[i]
            if keyStart && isSpace(c) { i += 1; continue }
            if keyStart && c == quote8 {
                keyStart = false
                i = quotedEnd(b, i) ?? b.count
                continue
            }
            keyStart = false
            switch c {
            case UInt8(ascii: "\""):
                i = stringEnd(b, i) ?? b.count
                continue
            case UInt8(ascii: "=") where eq == nil:
                eq = i
            case UInt8(ascii: ","):
                push(from, i, eq)
                from = i + 1
                eq = nil
                keyStart = true
            default:
                break
            }
            i += 1
        }
        push(from, b.count, eq)
        return out
    }

    /// A path read into its names.
    struct Parts: Equatable {
        /// The tree written before an `@` (`main@Store:Order`), of before.
        var tree: String?
        /// The sections' names, from the top (before the `:`, or the whole of a section's path).
        var sections: [String] = []
        /// Whether a `:` was written: the names after it are a card's, and none after it names the
        /// section itself (`![[Data/Tenant:]]`).
        var colon = false
        /// The names after the `:` (or of a path with neither `:` nor `/`): the card's, then those
        /// inside it.
        var names: [String] = []
        /// What each name picks, by its `[…]` as written inside the brackets (`""` for `[]`); none
        /// without one. As many as `names`.
        var picks: [String?] = []
    }

    /// A path read into its names, or why it cannot be.
    static func parse(_ text: String) -> Result<Parts, PathError> {
        do throws(PathError) {
            return .success(try read(text))
        } catch {
            return .failure(error)
        }
    }

    private static func read(_ text: String) throws(PathError) -> Parts {
        let text = trimmed(text)
        if text.isEmpty { throw .empty }
        if let error = marks(Array(text.utf8)).error { throw error }
        var out = Parts()
        var body = text
        if let (tree, rest) = treePrefix(text) {
            out.tree = try readSection(tree)
            body = String(decoding: trimSpaces(Array(rest.utf8)), as: UTF8.self)
            if body.isEmpty { return out }
            // `main@:Order`: the tree's root, then a card.
            if body.hasPrefix(":") {
                out.colon = true
                return try readNames(String(body.dropFirst()), out)
            }
        }
        let (sections, rest) = splitSection(body)
        if let sections {
            if !trimSpaces(Array(sections.utf8)).isEmpty {
                out.sections = try readSections(sections)
            }
            out.colon = true
            if trimSpaces(Array(rest.utf8)).isEmpty { return out }
            return try readNames(rest, out)
        }
        if marks(Array(rest.utf8)).marks.contains(where: { $0.sep == slash8 }) {
            out.sections = try readSections(rest)
            return out
        }
        return try readNames(rest, out)
    }

    private static func readSections(_ written: String) throws(PathError) -> [String] {
        var out: [String] = []
        for name in slashes(written) { out.append(try readSection(name)) }
        return out
    }

    private static func readNames(_ rest: String, _ parts: Parts) throws(PathError) -> Parts {
        var out = parts
        for written in dots(rest) {
            let (name, pick) = try readStep(written)
            out.names.append(name)
            out.picks.append(pick)
        }
        return out
    }

    /// One name after the `:` read: the name and its pick.
    private static func readStep(_ written: String) throws(PathError) -> (String, String?) {
        let (name, rest) = step(written)
        if name.isEmpty { throw .emptyName }
        if rest.isEmpty { return (name, nil) }
        guard rest.hasPrefix("["), rest.hasSuffix("]"), rest.utf8.count >= 2 else { throw .stray }
        let inner = String(decoding: trimSpaces(Array(rest.utf8.dropFirst().dropLast())), as: UTF8.self)
        return (name, canonicalPick(inner))
    }

    /// A pick written again with its keys quoted only where they need it.
    private static func canonicalPick(_ inner: String) -> String {
        if inner.isEmpty || inner.utf8.allSatisfy({ (48...57).contains($0) }) { return inner }
        return pickParts(inner).map { key, value in
            let k = String(decoding: trimSpaces(Array(key.utf8)), as: UTF8.self)
            let written = k.isEmpty ? "" : quoteKey(unquote(k))
            guard let value else { return written }
            return "\(written)=\(String(decoding: trimSpaces(Array(value.utf8)), as: UTF8.self))"
        }.joined(separator: ",")
    }

    /// A section's name read.
    private static func readSection(_ written: String) throws(PathError) -> String {
        let (name, rest) = step(written)
        if !rest.isEmpty { throw rest.hasPrefix("[") ? .bracketInSection : .stray }
        if name.isEmpty { throw .emptyName }
        return name
    }

    /// A path written from its names, each quoted only when it needs it.
    static func write(_ parts: Parts) -> String {
        var out = ""
        if let tree = parts.tree { out += quote(tree) + "@" }
        out += joinSections(parts.sections)
        if parts.colon { out += ":" }
        out += parts.names.enumerated().map { i, name in
            if i < parts.picks.count, let pick = parts.picks[i] { return "\(quote(name))[\(pick)]" }
            return quote(name)
        }.joined(separator: ".")
        return out
    }

    /// A path written again as it is kept: each name quoted only when it needs it. One that cannot
    /// be read is kept as it was written.
    static func canonical(_ text: String) -> String {
        guard case .success(let parts) = parse(text) else { return text }
        return write(parts)
    }

    // MARK: - Bytes

    private static let quote8 = UInt8(ascii: "'")
    private static let colon8 = UInt8(ascii: ":")
    private static let slash8 = UInt8(ascii: "/")
    private static let dot8 = UInt8(ascii: ".")
    private static let at8 = UInt8(ascii: "@")

    private static func isSpace(_ c: UInt8) -> Bool {
        c == UInt8(ascii: " ") || c == UInt8(ascii: "\t")
    }

    private static func isControl(_ s: Unicode.Scalar) -> Bool {
        s.properties.generalCategory == .control
    }

    private static func lower(_ c: UInt8) -> UInt8 {
        (65...90).contains(c) ? c + 32 : c
    }

    private static func trimSpaces(_ b: [UInt8]) -> [UInt8] {
        guard let first = b.firstIndex(where: { !isSpace($0) }),
              let last = b.lastIndex(where: { !isSpace($0) }) else { return [] }
        return Array(b[first...last])
    }

    private static func slice(_ b: [UInt8], _ from: Int, _ to: Int) -> String {
        String(decoding: b[from..<to], as: UTF8.self)
    }
}
