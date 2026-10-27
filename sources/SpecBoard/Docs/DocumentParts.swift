import CryptoKit
import Foundation

/// A document's Markdown as a card shows it, its references resolved — or an SVG or HTML card's
/// body, as written — and a digest that tells it apart from any other: what its picture is kept by
/// (`DocSnapshots`), so that a page that has not changed is neither built nor drawn again.
nonisolated struct DocumentPage: Hashable, Sendable {
    /// How the text is made into the page drawn: Markdown read, or a picture or page of its own
    /// (`PlainPage`), never read as Markdown. `face`: a whole page the core built (a ui part's, `PartFace`),
    /// drawn as it is given.
    enum Form: Hashable, Sendable {
        case markdown, svg, html, face
    }

    let text: String
    let form: Form
    let digest: Digest

    init(_ text: String, form: Form = .markdown) {
        self.text = text
        self.form = form
        // The same text drawn another way is another picture.
        digest = Digest(form == .markdown ? text : "\(form)\u{0}\(text)")
    }

    /// Drawn at its box's size, not as tall as it reads (`PlainPage`).
    var fillsBox: Bool { form != .markdown }

    /// The first 128 bits of the text's SHA-256: two texts that differ never share one in practice.
    struct Digest: Hashable, Sendable {
        let high: UInt64
        let low: UInt64

        init(_ text: String) {
            var high: UInt64 = 0, low: UInt64 = 0
            for (at, byte) in SHA256.hash(data: Data(text.utf8)).enumerated() where at < 16 {
                if at < 8 { high = high << 8 | UInt64(byte) } else { low = low << 8 | UInt64(byte) }
            }
            self.high = high
            self.low = low
        }
    }

    static func == (a: DocumentPage, b: DocumentPage) -> Bool { a.digest == b.digest }
    func hash(into hasher: inout Hasher) { hasher.combine(digest) }
}

/// What a document's `[[…]]`, `{{…}}` and `![[…]]` become as it is shown: asked of the core for many
/// documents at once (`document_parts`), on any thread, and put into the text the same way the
/// document's editor puts them in (`MarkdownRefs.resolve`, `NoteEmbed.block`).
nonisolated enum DocumentParts {

    /// The text of a card drawn as a document, before its references are resolved.
    struct Source: Sendable {
        var card: String
        var text: String
        /// Only its `[[…]]` read: a table's, whose cells hold data's values, not a body to fill in.
        var placesOnly = false
        /// Shown as it is, nothing in it read: what is said of a document with nothing written.
        var plain = false
        /// What the text is drawn as: an SVG or HTML card's body is shown as it is (`plain`).
        var form: DocumentPage.Form = .markdown
    }

    /// One `![[path|option]]`.
    struct Embed: Hashable, Sendable {
        var path: String
        var option: String?
    }

    /// What one `![[…]]` puts in: a diagram, a table, or why it cannot be drawn.
    enum Drawn: Sendable {
        case mermaid(String)
        case markdown(String)
        case problem(String)
    }

    /// What the core said of one document's references.
    struct Said: Sendable {
        /// The places that do not resolve, and why (empty when nothing more is known).
        var missing: [String: String] = [:]
        var values: [String: String] = [:]
        var embeds: [Embed: Drawn] = [:]
        /// Why the core could not be asked: said of each diagram, and the places left unlinked.
        var failure: String?
    }

    // MARK: - Asking

    /// What one document asks: its card and the references its text holds, each once — none when
    /// it asks nothing.
    static func asked(_ source: Source) -> [String: Any]? {
        guard !source.plain else { return nil }
        var places: [String] = [], values: [String] = [], embeds: [Embed] = []
        for ref in NoteText.refs(source.text) {
            switch ref.kind {
            case .place:
                if !places.contains(ref.path) { places.append(ref.path) }
            case .value where !source.placesOnly:
                if !values.contains(ref.path) { values.append(ref.path) }
            case .embed where !source.placesOnly:
                let embed = Embed(path: ref.path, option: ref.option)
                if !embeds.contains(embed) { embeds.append(embed) }
            default:
                break
            }
        }
        guard !places.isEmpty || !values.isEmpty || !embeds.isEmpty else { return nil }
        return ["card": source.card, "places": places, "values": values,
                "embeds": embeds.map { embed -> [String: Any] in
                    var asked: [String: Any] = ["path": embed.path]
                    if let option = embed.option { asked["option"] = option }
                    return asked
                }]
    }

    /// What the core says of `sources`' references, by card, in one request in the tree `tree`
    /// (`""` for main). Blocks the thread it is asked on until the core answers.
    static func ask(_ sources: [Source], of core: CoreClient, project: String, tree: String) -> [String: Said] {
        var documents: [[String: Any]] = []
        var order: [String] = []
        for source in sources {
            guard let asked = asked(source) else { continue }
            documents.append(asked)
            order.append(source.card)
        }
        guard !documents.isEmpty else { return [:] }
        do {
            let answer = try core.request("document_parts", ["project": project, "branch": tree, "documents": documents])
            return read(answer, documents)
        } catch {
            let failed = Said(failure: ErrorText.said(error))
            return Dictionary(order.map { ($0, failed) }, uniquingKeysWith: { first, _ in first })
        }
    }

    /// The core's answer read, by card: its embeds matched to what each document asked, in order.
    static func read(_ answer: Any, _ documents: [[String: Any]]) -> [String: Said] {
        let said = (answer as? [String: Any])?["documents"] as? [[String: Any]] ?? []
        var out: [String: Said] = [:]
        for (asked, told) in zip(documents, said) {
            guard let card = asked["card"] as? String else { continue }
            var one = Said()
            one.missing = told["missing"] as? [String: String] ?? [:]
            one.values = told["values"] as? [String: String] ?? [:]
            let embeds = asked["embeds"] as? [[String: Any]] ?? []
            for (embed, drawn) in zip(embeds, told["embeds"] as? [[String: Any]] ?? []) {
                let key = Embed(path: embed["path"] as? String ?? "", option: embed["option"] as? String)
                if let mermaid = drawn["mermaid"] as? String {
                    one.embeds[key] = .mermaid(mermaid)
                } else if let table = drawn["markdown"] as? String {
                    one.embeds[key] = .markdown(table)
                } else {
                    one.embeds[key] = .problem(drawn["problem"] as? String ?? "")
                }
            }
            out[card] = one
        }
        return out
    }

    // MARK: - Putting in

    /// The document's Markdown with what the core said put in.
    static func page(_ source: Source, _ said: Said) -> String {
        if source.plain { return source.text }
        if source.placesOnly { return places(source.text, said) }
        return NoteText.transform(source.text) { ref in
            switch ref.kind {
            case .place:
                return place(ref, said)
            case .value:
                return value(said.values[ref.path], for: ref)
            case .embed:
                let drawn = said.embeds[Embed(path: ref.path, option: ref.option)]
                    ?? .problem(said.failure ?? "")
                // What an embed draws has its `[[…]]` read too — a table's cells naming cards.
                return places(block(ref, drawn), said)
            }
        }
    }

    /// The Markdown with its `[[…]]` written out, its `{{…}}` and `![[…]]` as written.
    private static func places(_ text: String, _ said: Said) -> String {
        NoteText.transform(text) { ref in ref.kind == .place ? place(ref, said) : nil }
    }

    private static func place(_ ref: NoteText.Ref, _ said: Said) -> String {
        guard said.failure == nil else { return MarkdownRefs.code(ref.path) }
        return place(ref, exists: said.missing[ref.path] == nil, why: said.missing[ref.path])
    }

    /// One `[[…]]`: a link the app follows, or struck through when it does not resolve — with why,
    /// shown on the pointer, where it is known. A section's is the same link, which opens its map.
    /// Written with shown text, the link reads as that text, and the pointer shows the path.
    static func place(_ ref: NoteText.Ref, exists: Bool, why: String?) -> String {
        let code = MarkdownRefs.code(ref.path)
        let said = ref.shown.map(prose) ?? code
        guard exists else {
            let title = [ref.shown == nil ? nil : ref.path, why].compactMap { $0?.isEmpty == false ? $0 : nil }
            guard !title.isEmpty else { return "~~\(said)~~" }
            return "~~[\(said)](\(MarkdownPage.broken): \(MarkdownRefs.title(title.joined(separator: " — "))))~~"
        }
        let target = ref.path.addingPercentEncoding(withAllowedCharacters: .alphanumerics) ?? ref.path
        guard ref.shown != nil else { return "[\(code)](\(MarkdownPage.navigation):\(target))" }
        return "[\(said)](\(MarkdownPage.navigation):\(target) \(MarkdownRefs.title(ref.path)))"
    }

    /// Words as Markdown prints them: none of its marks (brackets, stars, underscores, backticks,
    /// backslashes, angle brackets) read as anything else.
    static func prose(_ text: String) -> String {
        var out = ""
        for c in text {
            if "[]*_`\\<>".contains(c) { out.append("\\") }
            out.append(c)
        }
        return out
    }

    /// One `{{…}}`: what it puts in, or itself struck through when it puts in nothing.
    static func value(_ found: String?, for ref: NoteText.Ref) -> String {
        guard let found else { return "~~\(MarkdownRefs.code("{{\(ref.path)}}"))~~" }
        return MarkdownRefs.code(found)
    }

    /// One `![[…]]`, drawn: a mermaid block, a table, or a line saying why not.
    static func block(_ ref: NoteText.Ref, _ drawn: Drawn) -> String {
        switch drawn {
        case .mermaid(let mermaid):
            return "\n```mermaid\n\(mermaid)\(mermaid.hasSuffix("\n") ? "" : "\n")```\n"
        case .markdown(let table):
            return "\n\(table)"
        case .problem(let why):
            return "> ⚠︎ \(MarkdownRefs.code(ref.written(SchemaDirectory.exported(path: ref.path)))) — \(why)"
        }
    }
}
