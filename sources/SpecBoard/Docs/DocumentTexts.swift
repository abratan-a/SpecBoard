import Foundation

/// What each card drawn as a document shows — its Markdown with its references resolved — kept for
/// the window, not for one diagram: a section shown again, a card picked, the diagram moved or
/// zoomed draw what is kept and ask nothing.
///
/// What a document's references put in is read from the whole project (a diagram it embeds, a value,
/// whether a place is there), so what is kept is good for the project as it was read (`stamp`), in the
/// screen's language. Once the project changes, the text is made again off the main thread — every
/// document asked of the core at once (`document_parts`) — and until it is in, the text kept before
/// is what is drawn, so that a card does not go back to saying it is being drawn.
@MainActor
final class DocumentTexts {

    /// What a card shows now: its page — `current`, or the one made before while a newer is being
    /// made —, a page still being made (none yet), or no document at all (the card is not in the
    /// project read).
    enum Face {
        case page(DocumentPage, current: Bool)
        case drawing
        case none
    }

    private struct Place: Hashable {
        var project: String
        var tree: String
        var card: String
    }

    /// What a page was made from: the project as read, in a language.
    private struct Version: Hashable {
        var stamp: Int
        var language: String
    }

    private struct Kept {
        var version: Version
        var page: DocumentPage
    }

    /// What is being made at once: the cards of one reading of the project, in one tree.
    private struct Batch {
        var detail: ProjectDetail
        var tree: String
        var core: CoreClient
        var version: Version
        var cards: [String: SchemaNode] = [:]
    }

    private var kept: [Place: Kept] = [:]
    /// The version being made for each card, until it is in.
    private var making: [Place: Version] = [:]
    private var waiting: [Place: [() -> Void]] = [:]
    private var gathering: [Batch] = []
    private var flushing = false
    private var settledWaiters: [CheckedContinuation<Void, Never>] = []
    /// Asked one after another, off the main thread: the core is asked for a section's documents at
    /// once, and the next waits for it rather than reading the project beside it.
    private let queue = DispatchQueue(label: "SpecBoard.documentTexts", qos: .userInitiated)

    /// The most pages kept; past it, those of other projects and trees are let go first.
    static let mostKept = 4000

    /// What `shown` (a card, or a goal's card map's box of one) shows, read from `detail` in the tree
    /// `tree` (`""` for main). When what is kept is not of this reading, it is made again off the main
    /// thread and `ready` is called once it is in; until then the page kept before is the answer.
    func face(of shown: SchemaNode, in detail: ProjectDetail, tree: String, core: CoreClient,
              ready: (() -> Void)? = nil) -> Face {
        let id = GoalCardMap.isCard(shown.id) ? GoalCardMap.card(of: shown.id) : shown.id
        guard let card = detail.cardsById[id] else { return .none }
        let place = Place(project: detail.project.id, tree: tree, card: id)
        let version = Version(stamp: detail.stamp, language: Language.current)
        let held = kept[place]
        if let held, held.version == version { return .page(held.page, current: true) }
        if let ready { waiting[place, default: []].append(ready) }
        if making[place] != version {
            making[place] = version
            gather(card, into: detail, tree: tree, core: core, version: version)
        }
        return held.map { .page($0.page, current: false) } ?? .drawing
    }

    /// Waits until no page is being made: for a test, which has no diagram to redraw.
    func settled() async {
        while !making.isEmpty || !gathering.isEmpty {
            await withCheckedContinuation { settledWaiters.append($0) }
        }
    }

    /// Whether a page is being made now.
    var busy: Bool { !making.isEmpty || !gathering.isEmpty }

    private func gather(_ card: SchemaNode, into detail: ProjectDetail, tree: String, core: CoreClient,
                        version: Version) {
        if let at = gathering.firstIndex(where: {
            $0.detail.project.id == detail.project.id && $0.tree == tree && $0.version == version
        }) {
            gathering[at].cards[card.id] = card
        } else {
            gathering.append(Batch(detail: detail, tree: tree, core: core, version: version, cards: [card.id: card]))
        }
        guard !flushing else { return }
        flushing = true
        // Once what is being drawn now has asked for all it shows: a section's documents in one request.
        DispatchQueue.main.async { MainActor.assumeIsolated { self.flush() } }
    }

    private func flush() {
        flushing = false
        let batches = gathering
        gathering = []
        for batch in batches {
            let sources = DiagramTimes.measure("document sources") {
                batch.cards.values.map { DocumentSource.of($0, in: batch.detail) }
            }
            let project = batch.detail.project.id, tree = batch.tree, core = batch.core, version = batch.version
            let back = Handed(value: { [weak self] (pages: [String: DocumentPage]) in
                self?.take(pages, project: project, tree: tree, version: version)
            })
            queue.async {
                let pages = Self.make(sources, core, project: project, tree: tree)
                DispatchQueue.main.async { MainActor.assumeIsolated { back.value(pages) } }
            }
        }
    }

    /// The pages of `sources`, made on the thread it is called on.
    nonisolated static func make(_ sources: [DocumentParts.Source], _ core: CoreClient, project: String,
                                 tree: String) -> [String: DocumentPage] {
        let interval = DiagramTimes.begin("documents")
        defer { DiagramTimes.end(interval) }
        let said = DocumentParts.ask(sources, of: core, project: project, tree: tree)
        return Dictionary(sources.map { ($0.card, DocumentPage(DocumentParts.page($0, said[$0.card] ?? .init()), form: $0.form)) },
                          uniquingKeysWith: { first, _ in first })
    }

    private func take(_ pages: [String: DocumentPage], project: String, tree: String, version: Version) {
        for (card, page) in pages {
            let place = Place(project: project, tree: tree, card: card)
            let latest = making[place] == version
            // One of an older reading, in after a newer was asked: drawn only while there is nothing.
            guard latest || kept[place] == nil else { continue }
            if latest { making[place] = nil }
            kept[place] = Kept(version: version, page: page)
            for ready in waiting.removeValue(forKey: place) ?? [] { ready() }
        }
        trim(keeping: project, tree)
        if making.isEmpty, gathering.isEmpty {
            let waiters = settledWaiters
            settledWaiters = []
            waiters.forEach { $0.resume() }
        }
    }

    private func trim(keeping project: String, _ tree: String) {
        guard kept.count > Self.mostKept else { return }
        kept = kept.filter { $0.key.project == project && $0.key.tree == tree }
        while kept.count > Self.mostKept, let any = kept.keys.first { kept.removeValue(forKey: any) }
    }
}

/// The text of a card drawn as a document, before its references are resolved: what its face shows.
enum DocumentSource {
    /// A freedoc's text, a formatdoc as it reads (`FormatDocEditor.document`), a stackdoc's version
    /// open (its heading and body), a table's Markdown, and the Markdown a card's face draws below
    /// its fields (a task's body).
    static func of(_ card: SchemaNode, in detail: ProjectDetail) -> DocumentParts.Source {
        // A table: the table it builds, its cells naming cards links as a body's paths are.
        if card.kind == .table { return DocumentParts.Source(card: card.id, text: card.tableMarkdown ?? "", placesOnly: true) }
        // An SVG or HTML page: its text as written, nothing in it read.
        if let page = card.facePage {
            let text = card.facePageText
            guard !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
                return DocumentParts.Source(card: card.id, text: L10n[.Root.documentEmpty], plain: true)
            }
            return DocumentParts.Source(card: card.id, text: text, plain: true, form: page.form)
        }
        let text = card.face?.markdown != nil ? card.faceMarkdown
            : card.kind == .formatdoc ? FormatDocEditor.document(card, in: detail)
            : card.kind == .stackdoc ? card.openVersion?.page ?? "" : card.body
        guard !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            return DocumentParts.Source(card: card.id, text: L10n[.Root.documentEmpty], plain: true)
        }
        return DocumentParts.Source(card: card.id, text: text)
    }
}
