import AppKit
import SwiftUI

// The rows above the nav tree's top row, pinned over the tree as it scrolls, as an editor pins the scopes the top
// line is in (`specboard://SpecBoard/Docs/Design:Overview` 画面「ナビパネルの止まる親の行」): which rows pin is worked out from
// how the rows hang (`NavPanelTree.Outline`) and where the list is scrolled (`NavPanelTree.sticky`), never by walking
// the rows; the band is drawn over the top of what the list's scroll view shows (`NavSticky`).

extension NavPanelTree {
    /// How many rows stay pinned at most: deeper, the outer ones are left out, so the band never covers the tree.
    static let stickyLimit = 5

    /// Which row each row hangs from and how far down what is inside it goes, by the rows' places among the rows.
    /// Worked out with the rows, so that a scroll asks it for the rows above the top row without walking the rows.
    struct Outline: Equatable {
        /// Each row's parent, -1 for a row at the top.
        private(set) var parents: [Int] = []
        /// The last row inside each row: itself where nothing is under it.
        private(set) var ends: [Int] = []

        init() {}

        init(_ rows: [Row]) {
            parents = Array(repeating: -1, count: rows.count)
            ends = Array(rows.indices)
            var above: [Int] = []
            for (at, row) in rows.enumerated() {
                while let last = above.last, rows[last].depth >= row.depth { above.removeLast() }
                parents[at] = above.last ?? -1
                above.append(at)
            }
            for at in rows.indices.reversed() where parents[at] >= 0 {
                ends[parents[at]] = max(ends[parents[at]], ends[at])
            }
        }

        var count: Int { parents.count }

        /// Whether rows hang under `row`.
        func holds(_ row: Int) -> Bool { ends[row] > row }

        /// The row right under `parent` — a top row, without one — on the way down to `row`: nil where `row` is
        /// not inside `parent`.
        func step(from parent: Int?, toward row: Int) -> Int? {
            let wanted = parent ?? -1
            guard row >= 0, row < count, row != wanted else { return nil }
            var at = row
            while parents[at] != wanted {
                at = parents[at]
                if at < 0 { return nil }
            }
            return at
        }

        /// Whether `row` is inside `parent`, however deep.
        func isInside(_ row: Int, _ parent: Int) -> Bool { parent < row && ends[parent] >= row }
    }

    /// The rows pinned over the tree, by their places among the rows, the outermost first; and how far the
    /// innermost is pushed up by the row after what it holds (zero or less).
    struct Sticky: Equatable {
        var rows: [Int] = []
        var push: CGFloat = 0

        var isEmpty: Bool { rows.isEmpty }
    }

    /// The rows pinned while the top of what the list shows is at `top`: the parents of the row under the band,
    /// one level at a time — each the row under the last one on the way to the row the band's bottom is on, while
    /// its own row starts above that bottom and rows hang under it. Where what it holds ends inside its slot, the
    /// next row pushes it up by as much and no deeper row pins. Past `limit`, the outermost is left out and the band
    /// is as tall as the ones kept. `rowAt`: the row a height falls on; `frame`: a row's top and height.
    static func sticky(_ outline: Outline, top: CGFloat, rowAt: (CGFloat) -> Int?,
                       frame: (Int) -> (minY: CGFloat, height: CGFloat), limit: Int = stickyLimit) -> Sticky {
        var chain: [Int] = []
        var push: CGFloat = 0
        func band() -> CGFloat { chain.suffix(limit).reduce(0) { $0 + frame($1).height } }
        while chain.count < outline.count {
            let bottom = top + band()
            guard let under = rowAt(bottom), let next = outline.step(from: chain.last, toward: under),
                  outline.holds(next), frame(next).minY < bottom else { break }
            chain.append(next)
            let end = frame(outline.ends[next])
            let slot = top + band()
            if end.minY + end.height < slot {
                push = end.minY + end.height - slot
                break
            }
        }
        return Sticky(rows: Array(chain.suffix(limit)), push: push)
    }
}

/// The band of pinned rows over the nav tree: follows the scroll view of the tree's list — its clip view's bounds, and
/// the table's size as rows are opened and shut — works out which rows pin as it moves, once a move, and lays them over
/// the top of what the list shows. The band is drawn again only when the rows pinned, how far the innermost is pushed or
/// what the rows say change.
@MainActor
final class NavSticky: NSObject {
    /// For the tests: how many times the pinned rows were worked out, the time it took, and the band's rows built.
    static var workedOut = 0
    static var seconds: Double = 0
    static var rowsBuilt = 0

    static func built() { rowsBuilt += 1 }

    /// What the band is made from: the rows on screen and how they hang, a row as the tree draws it, and how a row
    /// is opened and shut in the tree.
    struct Content {
        var rows: [NavPanelTree.Row]
        var outline: NavPanelTree.Outline
        var make: (NavPanelTree.Row) -> NavTreeRow
        var toggle: (NavPanelTree.Node) -> Void
    }

    private(set) var pinned = NavPanelTree.Sticky()
    private(set) weak var scroll: NSScrollView?
    private var band: NavStickyBandHost?
    private var content: Content?
    /// Counts the contents taken, so a fold waits for the rows it makes.
    private var generation = 0
    /// What the band shows was made from older contents than the newest.
    private var stale = true
    private var updateAsked = false
    /// How far in from the band's sides the list sets its rows' cells: the band's rows are set as far in, so each is
    /// drawn where its row in the list is.
    private var inset = NavStickyBand.Inset()
    /// A row folded from the band, to bring into view once the list has taken the fold: its key, and the contents it
    /// was folded on.
    private var revealing: (key: String, after: Int)?

    private var table: NSTableView? { scroll?.documentView as? NSTableView }

    /// The list's scroll view followed: its moves heard, the band laid over it.
    func follow(_ scroll: NSScrollView) {
        guard scroll !== self.scroll else { return }
        let center = NotificationCenter.default
        if let old = self.scroll {
            center.removeObserver(self, name: NSView.boundsDidChangeNotification, object: old.contentView)
            center.removeObserver(self, name: NSView.frameDidChangeNotification, object: old.contentView)
            if let table = old.documentView { center.removeObserver(self, name: NSView.frameDidChangeNotification, object: table) }
        }
        band?.removeFromSuperview()
        self.scroll = scroll
        let clip = scroll.contentView
        clip.postsBoundsChangedNotifications = true
        clip.postsFrameChangedNotifications = true
        center.addObserver(self, selector: #selector(moved), name: NSView.boundsDidChangeNotification, object: clip)
        center.addObserver(self, selector: #selector(moved), name: NSView.frameDidChangeNotification, object: clip)
        if let table {
            table.postsFrameChangedNotifications = true
            center.addObserver(self, selector: #selector(moved), name: NSView.frameDidChangeNotification, object: table)
        }
        // A band another panel left on the same list goes: one band a list.
        for other in scroll.subviews where other is NavStickyBandHost { other.removeFromSuperview() }
        // Over what scrolls, under the scrollers.
        let band = NavStickyBandHost(rootView: NavStickyBand())
        band.sticky = self
        band.isHidden = true
        scroll.addSubview(band, positioned: .above, relativeTo: clip)
        self.band = band
        stale = true
        update()
    }

    /// What the tree shows now: the band follows it on the next turn, once the list has taken it.
    func take(_ content: Content) {
        self.content = content
        generation += 1
        stale = true
        guard !updateAsked else { return }
        updateAsked = true
        DispatchQueue.main.async { [weak self] in
            self?.updateAsked = false
            self?.update()
        }
    }

    @objc private func moved(_ note: Notification) { update() }

    /// The pinned rows worked out where the list is now, and the band laid there.
    func update() {
        guard let scroll, let table, let content, let band else { return }
        let start = DispatchTime.now().uptimeNanoseconds
        defer {
            Self.workedOut += 1
            Self.seconds += Double(DispatchTime.now().uptimeNanoseconds - start) / 1_000_000_000
        }
        let fits = table.numberOfRows == content.outline.count && table.numberOfRows == content.rows.count
        if fits, let wanted = revealing, generation > wanted.after {
            revealing = nil
            reveal(wanted.key)
        }
        let now = fits ? workOut(scroll, table, content.outline) : NavPanelTree.Sticky()
        let changed = now != pinned
        pinned = now
        place(scroll, table, band)
        let cells = now.rows.first.map { cellInset(table, band, $0) } ?? inset
        let shifted = cells != inset
        inset = cells
        if !now.isEmpty, changed || stale || shifted { draw(band, table, content) }
    }

    private func workOut(_ scroll: NSScrollView, _ table: NSTableView, _ outline: NavPanelTree.Outline) -> NavPanelTree.Sticky {
        let clip = scroll.contentView
        let top = table.convert(NSPoint(x: 0, y: clip.bounds.minY + clip.contentInsets.top), from: clip).y
        let width = max(1, table.bounds.width)
        return NavPanelTree.sticky(outline, top: top, rowAt: { y in
            let found = table.rows(in: NSRect(x: 0, y: y, width: width, height: 1))
            return found.length > 0 ? found.location : nil
        }, frame: { row in
            let rect = table.rect(ofRow: row)
            return (rect.minY, rect.height)
        })
    }

    /// Over the top of what the list shows, as tall as the rows pinned less the push — never taller than what it shows.
    private func place(_ scroll: NSScrollView, _ table: NSTableView, _ band: NavStickyBandHost) {
        guard !pinned.isEmpty else {
            if !band.isHidden { band.isHidden = true }
            return
        }
        let clip = scroll.contentView
        let insets = clip.contentInsets
        var seen = clip.bounds
        seen.origin.x += insets.left
        seen.origin.y += insets.top
        seen.size.width = max(0, seen.width - insets.left - insets.right)
        seen.size.height = max(0, seen.height - insets.top - insets.bottom)
        let tall = pinned.rows.reduce(0) { $0 + table.rect(ofRow: $1).height } + pinned.push
        let rect = clip.convert(CGRect(x: seen.minX, y: seen.minY, width: seen.width, height: max(0, min(tall, seen.height))),
                                to: scroll)
        if band.frame != rect { band.frame = rect }
        if band.isHidden { band.isHidden = false }
    }

    /// How far in from the band's sides the cell of `row` is, as the list lays it out.
    private func cellInset(_ table: NSTableView, _ band: NSView, _ row: Int) -> NavStickyBand.Inset {
        guard table.numberOfColumns > 0, row < table.numberOfRows else { return inset }
        let cell = band.convert(table.frameOfCell(atColumn: 0, row: row), from: table)
        return NavStickyBand.Inset(leading: cell.minX - band.bounds.minX, trailing: band.bounds.maxX - cell.maxX)
    }

    private func draw(_ band: NavStickyBandHost, _ table: NSTableView, _ content: Content) {
        stale = false
        let rows = pinned.rows.filter { $0 < content.rows.count }
        band.rootView = NavStickyBand(rows: rows.map { content.make(content.rows[$0]) },
                                      heights: rows.map { table.rect(ofRow: $0).height }, push: pinned.push, inset: inset)
    }

    /// A pinned row pressed: the list scrolled so that the row shows right under the rows pinned above it.
    func reveal(_ key: String) {
        guard let scroll, let table, let content, let row = content.rows.firstIndex(where: { $0.id == key }),
              row < table.numberOfRows else { return }
        let above = pinned.rows.filter { content.outline.isInside(row, $0) }
        let lead = above.reduce(0) { $0 + table.rect(ofRow: $1).height }
        let clip = scroll.contentView
        let y = clip.convert(NSPoint(x: 0, y: table.rect(ofRow: row).minY - lead), from: table).y - clip.contentInsets.top
        var bounds = clip.bounds
        bounds.origin.y = y
        clip.scroll(to: clip.constrainBoundsRect(bounds).origin)
        scroll.reflectScrolledClipView(clip)
    }

    /// A pinned row's ▸ pressed: opened or shut as its row in the tree is, then brought into view once the list has
    /// taken it.
    func fold(_ node: NavPanelTree.Node) {
        guard let content else { return }
        revealing = (node.key, generation)
        content.toggle(node)
    }
}

/// Finds the scroll view of the list it is laid behind and hands it to `sticky`: the list's own scroll view is the
/// one whose frame holds this view's middle.
struct NavStickyFollower: NSViewRepresentable {
    var sticky: NavSticky
    var content: NavSticky.Content

    func makeNSView(context: Context) -> NavStickyFollowerView {
        let view = NavStickyFollowerView()
        view.sticky = sticky
        return view
    }

    func updateNSView(_ view: NavStickyFollowerView, context: Context) {
        view.sticky = sticky
        sticky.take(content)
        view.find()
    }
}

final class NavStickyFollowerView: NSView {
    weak var sticky: NavSticky?
    /// Tries left before the list is given up on: it is laid out a turn or two after this view.
    private var tries = 0

    override func hitTest(_ point: NSPoint) -> NSView? { nil }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        tries = 20
        find()
    }

    override func layout() {
        super.layout()
        find()
    }

    func find() {
        guard let sticky, let window else { return }
        if let scroll = sticky.scroll, scroll.window === window, holdsMiddle(scroll) { return }
        if let scroll = listScroll(in: window) {
            sticky.follow(scroll)
        } else if tries > 0 {
            tries -= 1
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) { [weak self] in self?.find() }
        }
    }

    private var middle: NSPoint { convert(NSPoint(x: bounds.midX, y: bounds.midY), to: nil) }

    private func holdsMiddle(_ scroll: NSScrollView) -> Bool {
        scroll.convert(scroll.bounds, to: nil).contains(middle)
    }

    /// The smallest scroll view of a table whose frame holds this view's middle.
    private func listScroll(in window: NSWindow) -> NSScrollView? {
        guard let root = window.contentView, bounds.width > 0, bounds.height > 0 else { return nil }
        let middle = middle
        var best: (scroll: NSScrollView, area: CGFloat)?
        func look(_ view: NSView) {
            if let scroll = view as? NSScrollView, scroll.documentView is NSTableView {
                let frame = scroll.convert(scroll.bounds, to: nil)
                let area = frame.width * frame.height
                if frame.contains(middle), area < best?.area ?? .infinity { best = (scroll, area) }
            }
            for sub in view.subviews { look(sub) }
        }
        look(root)
        return best?.scroll
    }
}

/// The band's view in the list's scroll view: it draws only inside its own frame — a row pushed up by the next one
/// starts above it, and a view does not clip by default — and hands the wheel to the list, so the list scrolls with
/// the pointer over the band.
final class NavStickyBandHost: NSHostingView<NavStickyBand> {
    weak var sticky: NavSticky?

    required init(rootView: NavStickyBand) {
        super.init(rootView: rootView)
        clipsToBounds = true
        sizingOptions = []
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        super.init(coder: coder)
    }

    override func scrollWheel(with event: NSEvent) {
        superview?.scrollWheel(with: event)
    }
}

/// The pinned rows, drawn as the tree draws them, the outermost at the top: the innermost slides under the others as it
/// is pushed up. The window's background under them, a thin line below.
struct NavStickyBand: View {
    var rows: [NavTreeRow] = []
    var heights: [CGFloat] = []
    var push: CGFloat = 0
    var inset = Inset()

    /// How far in from the band's sides a row is set, as the list sets its cells in from its rows.
    struct Inset: Equatable {
        var leading: CGFloat = 0
        var trailing: CGFloat = 0
    }

    var body: some View {
        VStack(spacing: 0) {
            ForEach(Array(rows.enumerated()), id: \.element.row.id) { at, row in
                let last = at == rows.count - 1
                row.equatable()
                    .padding(.leading, inset.leading)
                    .padding(.trailing, inset.trailing)
                    .frame(height: at < heights.count ? heights[at] : nil)
                    .background(Color(nsColor: .windowBackgroundColor))
                    .offset(y: last ? push : 0)
                    .zIndex(last ? 0 : 1)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .background(Color(nsColor: .windowBackgroundColor))
        .overlay(alignment: .bottom) {
            Rectangle().fill(Color(nsColor: .separatorColor)).frame(height: 1)
        }
        .clipped()
    }
}
