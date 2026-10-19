import AppKit
import Observation

/// The toolbar's handle on the canvas.
///
/// The canvas owns its layers and is never rebuilt by SwiftUI, so the two talk
/// through this rather than through view identity: SwiftUI reads `zoom` and
/// `edgeCount` to draw the bar, and calls the actions to drive the view.
@Observable
final class CardCanvasController {
    var zoom: CGFloat = 1
    /// Drawn lines, not relations — merging collapses a reciprocal pair into one.
    var edgeCount: Int = 0
    /// Reported by the canvas: the kinds this project can draw, switched on or not.
    var availableKinds: Set<String> = []
    var filters = GraphFilters()
    /// The canvas in its selection mode (`CardCanvasView.selecting`).
    var isSelecting = false

    @ObservationIgnored weak var view: CardCanvasView?

    /// The selection mode on or off (the frame's 「終える」).
    func setSelecting(_ on: Bool) { view?.setSelecting(on) }

    /// The canvas gone — for a card's editor, a dashboard: what is asked of it until the
    /// next one is made is held, not done to a view no longer on screen.
    func detach(_ view: CardCanvasView) {
        if self.view === view { self.view = nil }
    }

    func attach(_ view: CardCanvasView) {
        self.view = view
        // Deferred: these fire while the canvas is being set up, and SwiftUI will
        // not have state mutated in the middle of building a view.
        view.onZoomChanged = { [weak self] z in
            DispatchQueue.main.async { self?.zoom = z }
        }
        view.onEdgeCountChanged = { [weak self] count in
            DispatchQueue.main.async { self?.edgeCount = count }
        }
        view.onKindsChanged = { [weak self] kinds in
            DispatchQueue.main.async { self?.availableKinds = kinds }
        }
        view.onSelectingChanged = { [weak self] on in
            DispatchQueue.main.async { self?.isSelecting = on }
        }
    }

    /// The kinds of line this project draws here, so the legend lists what is actually
    /// there — a kind of the project's own too, which the row is also the way to switch off:
    /// in the order the definitions come (`kinds.lines`), one the project does not define
    /// after them.
    func drawnKinds(in detail: ProjectDetail?) -> [String] {
        let defined = detail?.lineKinds.names ?? []
        let used = availableKinds.isEmpty ? Set(detail?.relations.map(\.line) ?? []) : availableKinds
        // None drawn: none to switch (the other sections' cards may be switched off, and the lines
        // to them with them) — not every kind the project defines.
        guard !used.isEmpty else { return [] }
        return defined.filter { used.contains($0) } + used.subtracting(defined).sorted()
    }

    /// Fold or unfold every row of one card that has anything under it.
    func foldAll(_ schemaId: String, _ folded: Bool) { view?.foldAll(schemaId, folded) }
    /// A document card zoomed into or moved inside, and the whole page across it again.
    func documentMoved(_ schemaId: String) -> Bool { view?.documentMoved(schemaId) ?? false }
    func showWholeDocument(_ schemaId: String) { view?.showWholeDocument(schemaId) }

    /// Whether that card has any row worth folding — a menu item that would do
    /// nothing is worse than one that is not there.
    func canFold(_ schemaId: String) -> Bool { view?.canFold(schemaId) ?? false }

    func fit() { view?.fitToView() }
    /// Go to a card and select it, and the row of `field` if one is given.
    ///
    /// Held when there is no canvas yet — at launch, the selection restored from last
    /// time arrives before the canvas has drawn a card to go to — and carried out by
    /// the first `shown()`.
    /// `field`: the row to mark — a field's id, or another row's path (`CardCanvasView.rowKey`).
    func focus(_ schemaId: String, field: String? = nil) {
        guard let view else {
            pendingFocus = (schemaId, field)
            return
        }
        view.focus(schemaId, row: field)
    }

    @ObservationIgnored private var pendingFocus: (schema: String, field: String?)?

    /// Where each diagram was looking when its canvas went away — to the card's editor,
    /// a root's dashboard — by section, to look there again when it comes back.
    @ObservationIgnored private var looks: [String: CardCanvasView.Look] = [:]

    func remember(_ look: CardCanvasView.Look?, for section: String) {
        looks[section] = look ?? looks[section]
    }

    func look(for section: String) -> CardCanvasView.Look? { looks[section] }

    /// Select a card, moving the view only when it is not on screen. Held when
    /// there is no canvas yet, and carried out by the first `shown()`.
    func reveal(_ schemaId: String) {
        guard let view else { pendingReveal = (schemaId, false); return }
        view.reveal(schemaId)
    }

    /// A card just put on the diagram by hand: picked, and the view moved to take it whole, the
    /// zoom kept (`CardCanvasView.reveal`'s `whole`) — by the next `shown()`, since it has no box
    /// until the `show` that draws what was written. Asked once what was written has been read
    /// again, so that the next `show` is the one that draws it — on the canvas there, or on one
    /// made for it (a card map's first card, where there was none to draw).
    func revealAfterNextShow(_ schemaId: String) {
        pendingReveal = (schemaId, true)
    }

    @ObservationIgnored private var pendingReveal: (card: String, whole: Bool)?

    /// The section the canvas last drew, set by the bridge. A move to a card in
    /// another section waits for the canvas to catch up with the model.
    @ObservationIgnored var shownSection: String?

    /// Whether that card is on the canvas now — its own section's or another's.
    func draws(_ schemaId: String) -> Bool { view?.frame(ofCard: schemaId) != nil }

    /// Where a row's or a header's handle sits, in the canvas's points: where つなぐ is shown
    /// from it (`ConnectAnchor`). Nil when it is not drawn.
    func anchor(of hit: CardCanvasView.RowHit) -> CGPoint? { view?.anchorInView(of: hit) }

    /// For a card the canvas is about to draw — in a section it has not drawn yet,
    /// for one. Carried out by the next `shown()`.
    func focusAfterNextShow(_ schemaId: String, field: String? = nil) {
        pendingFocus = (schemaId, field)
    }

    /// Called by the bridge after every `show`. A held focus is tried once and then
    /// dropped either way: one whose card is filtered out must not fire an hour
    /// later, on whatever redraw happens to reveal it.
    func shown() {
        if let card = pendingReveal {
            pendingReveal = nil
            view?.reveal(card.card, whole: card.whole)
        }
        guard let pending = pendingFocus else { return }
        pendingFocus = nil
        focus(pending.schema, field: pending.field)
    }
    func zoomIn() { view?.nudgeZoom(1.25) }
    func zoomOut() { view?.nudgeZoom(0.8) }
    /// 100%, keeping the middle of the panel where it is.
    func actualSize() {
        guard let view else { return }
        view.setZoom(1, anchor: CGPoint(x: view.bounds.midX, y: view.bounds.midY))
    }

    private func update(_ change: (inout GraphFilters) -> Void) {
        var next = filters
        change(&next)
        guard next != filters else { return }
        filters = next
        view?.apply(next)
    }

    /// Switching a line off only removes lines, so an empty set is a legible state:
    /// the cards are all still there. A kind hidden from the start by its definition is
    /// switched on and off again (`shownLines`), the rest off and on.
    func toggle(line: String, kinds: LineKinds = LineKinds()) {
        update {
            if kinds[line]?.visible == false {
                if $0.shownLines.contains(line) { $0.shownLines.remove(line) } else { $0.shownLines.insert(line) }
            } else if $0.hiddenLines.contains(line) {
                $0.hiddenLines.remove(line)
            } else {
                $0.hiddenLines.insert(line)
            }
        }
    }

    /// The lines as the project's definitions draw them from the start: nothing switched by hand.
    /// Called when another project is opened — what was switched is not kept.
    func resetLines() {
        update {
            $0.hiddenLines = []
            $0.shownLines = []
        }
    }

    /// The lines switched as `other` has them: a card's connections diagram opened from the
    /// relation panel draws what the panel was drawing.
    func takeLines(from other: CardCanvasController) {
        update {
            $0.hiddenLines = other.filters.hiddenLines
            $0.shownLines = other.filters.shownLines
        }
    }

    /// Kinds decide which cards *exist*, so the last one may not be switched off —
    /// an empty canvas with no obvious way back is not a filter, it is a dead end.
    func toggle(kind: SchemaKind) {
        update {
            var next = $0.kinds
            if next.contains(kind) { next.remove(kind) } else { next.insert(kind) }
            if !next.isEmpty { $0.kinds = next }
        }
    }

    /// Whether another section's schemas get cards: the ones this section points
    /// at, and the ones pointing at it.
    func toggleOutsideReferences() { update { $0.outsideReferences.toggle() } }
    func toggleOutsideConsumers() { update { $0.outsideConsumers.toggle() } }

    var isFiltered: Bool {
        filters.kinds.count != SchemaKind.allCases.count
    }
}
