import AppKit
import WebKit

/// Pictures of documents, for the cards that show one (overview §12).
///
/// A freedoc's card, and a formatdoc's when it reads as a document, shows the whole
/// Markdown as it reads — diagrams drawn — without a web view of its own on the
/// canvas: the page is drawn once out of sight at the card's width, and a picture of
/// it is what the card shows. It is drawn again only when the text, the width or the
/// appearance changes, or the card is zoomed into past what the picture holds;
/// anything else the canvas does reuses the picture.
///
/// An SVG or HTML card's page (`PlainPage`) is drawn the same way, at its box's width and height: what
/// it holds past the box is cut, and as none of its scripts run, it is taken once it has loaded.
///
/// A picture is kept by the text's digest (`DocumentPage`), so asking for one already taken costs a
/// lookup: the page's HTML is built only when it is drawn. The cards on screen are drawn first, and
/// what a diagram no longer shows is let go of before it is drawn (`withdraw`).
@MainActor
final class DocSnapshots: NSObject {
    static let shared = DocSnapshots()

    /// How long a page that has loaded is waited for to say it is ready before it is taken as
    /// it is (a script that fails before it can say so).
    let patience: TimeInterval

    /// `patience`: the shared one's eight seconds; a test's own may wait less.
    init(patience: TimeInterval = 8) {
        self.patience = patience
        super.init()
    }

    struct Key: Hashable {
        var digest: DocumentPage.Digest
        var width: Int
        /// The box a page that fills it is drawn in (`DocumentPage.fillsBox`); 0 for a document, as
        /// tall as it reads.
        var height: Int = 0
        var dark: Bool
        /// How much finer than the page's own points, in hundredths (`scales`).
        var scale: Int = 100

        var same: Key { Key(digest: digest, width: width, height: height, dark: dark) }
    }

    struct Picture {
        var image: CGImage
        /// The picture's height in the page's points: the whole document's, up to `tallest`.
        var height: CGFloat
        /// The document goes on past `tallest`.
        var cut = false
        /// How much finer than the page's points it was taken.
        var scale: CGFloat = 1
        /// Which picture it is, never the same for two taken: what a card drawn of it is kept by.
        var number = 0
    }

    /// The tallest a picture is taken: past this a card says the rest is in the
    /// editor. A picture is held in memory for as long as the card shows.
    static let tallest: CGFloat = 3000
    /// What the pictures kept may take, in bytes; the least recently asked for let go first.
    static let budget = 256 << 20
    /// The finer pictures a zoomed card is drawn from, and the most pixels one may have.
    static let scales: [CGFloat] = [1, 1.5, 2, 3, 4, 6, 8]
    static let mostPixels: CGFloat = 24_000_000
    /// Pages drawn at once.
    static let drawers = 3
    /// The text size on a card, in points.
    static let textSize: CGFloat = 11.5

    /// Who waits for a picture: a diagram's card, or no one in particular (a test's `take`).
    private struct Waiter {
        var owner: ObjectIdentifier?
        var card: String?
        var call: () -> Void
    }

    /// The pictures taken, by page, width and appearance, then by scale: the finest of a page
    /// found without looking at the others.
    private var done: [Key: [Int: Picture]] = [:]
    private var used = LeastRecent<Key>()
    private var held = 0
    /// The pages waiting to be drawn, those of cards on screen first.
    private var urgent: [Key] = []
    private var later: [Key] = []
    /// Each page waiting or being drawn: its HTML is built as it is drawn.
    private var pages: [Key: DocumentPage] = [:]
    private var waiting: [Key: [Waiter]] = [:]
    private var idle: [Drawer] = []
    private var busy = 0
    private var taken = 0

    /// The page a card's Markdown is drawn as: every diagram within the column.
    static func page(_ markdown: String) -> String {
        MarkdownPage.html(markdown, size: textSize, card: true)
    }

    /// The page a card's page is drawn as: its Markdown so, or an SVG or HTML card's own.
    static func page(_ page: DocumentPage) -> String {
        page.fillsBox ? PlainPage.html(page) : Self.page(page.text)
    }

    /// The finest of `scales` up to `wanted` that a picture of a page `width` by `height`
    /// points may be taken at, keeping within `mostPixels` (at the screen's two pixels a point).
    static func scale(_ wanted: CGFloat, width: CGFloat, height: CGFloat) -> CGFloat {
        let room = (mostPixels / max(1, width * height * 4)).squareRoot()
        return scales.last { $0 <= max(1, min(wanted, room)) } ?? 1
    }

    private static func key(_ page: DocumentPage, width: CGFloat, height: CGFloat, dark: Bool, scale: CGFloat) -> Key {
        Key(digest: page.digest, width: Int(width.rounded()), height: page.fillsBox ? max(1, Int(height.rounded())) : 0,
            dark: dark, scale: Int((scale * 100).rounded()))
    }

    /// The picture for `markdown` at `width` points and `scale`, when it has been taken.
    /// When it has not, it is asked for, `ready` is called once it has been, and in the
    /// meantime the finest picture of the page taken so far is the answer, if any is.
    func picture(_ markdown: String, width: CGFloat, dark: Bool, scale: CGFloat = 1,
                 ready: @escaping () -> Void) -> Picture? {
        picture(DocumentPage(markdown), width: width, dark: dark, scale: scale, ready: ready)
    }

    /// The same for a page whose digest is known. `height`: the box a page that fills it is drawn in
    /// (unread for Markdown). `urgent`: the card is on screen, its page drawn
    /// before those that are not. `owner` and `card`: the diagram and the card that wait for it,
    /// let go of when the diagram no longer shows the card (`withdraw`).
    func picture(_ page: DocumentPage, width: CGFloat, height: CGFloat = 0, dark: Bool, scale: CGFloat = 1,
                 urgent: Bool = false, owner: AnyObject? = nil, card: String? = nil,
                 ready: @escaping () -> Void) -> Picture? {
        let key = Self.key(page, width: width, height: height, dark: dark, scale: scale)
        if let found = done[key.same]?[key.scale] {
            used.touch(key)
            return found
        }
        waiting[key, default: []].append(Waiter(owner: owner.map(ObjectIdentifier.init), card: card, call: ready))
        if pages[key] == nil {
            pages[key] = page
            if urgent { self.urgent.append(key) } else { later.append(key) }
        } else if urgent, let at = later.firstIndex(of: key) {
            later.remove(at: at)
            self.urgent.append(key)
        }
        next()
        return done[key.same]?.max { $0.key < $1.key }?.value
    }

    /// Takes the picture, then calls back — for a test, which has no canvas to redraw.
    func take(_ markdown: String, width: CGFloat, dark: Bool, scale: CGFloat = 1) async -> Picture? {
        await take(DocumentPage(markdown), width: width, dark: dark, scale: scale)
    }

    /// The same for any page: an SVG or HTML card's in a box `height` tall.
    func take(_ page: DocumentPage, width: CGFloat, height: CGFloat = 0, dark: Bool, scale: CGFloat = 1) async -> Picture? {
        let key = Self.key(page, width: width, height: height, dark: dark, scale: scale)
        if let found = done[key.same]?[key.scale] { return found }
        _ = picture(page, width: width, height: height, dark: dark, scale: scale, ready: {})
        await withCheckedContinuation { (going: CheckedContinuation<Void, Never>) in
            waiting[key, default: []].append(Waiter(call: { going.resume() }))
        }
        return done[key.same]?[key.scale]
    }

    /// What `owner` waits for of the cards it no longer shows let go of: a page no one waits for any
    /// more is not drawn (one being drawn is finished and kept).
    func withdraw(owner: AnyObject, keeping cards: Set<String>) {
        let id = ObjectIdentifier(owner)
        for (key, waiters) in waiting {
            let left = waiters.filter { $0.owner != id || $0.card.map(cards.contains) == true }
            guard left.count != waiters.count else { continue }
            waiting[key] = left.isEmpty ? nil : left
            guard left.isEmpty else { continue }
            if let at = urgent.firstIndex(of: key) {
                urgent.remove(at: at)
                pages[key] = nil
            } else if let at = later.firstIndex(of: key) {
                later.remove(at: at)
                pages[key] = nil
            }
        }
    }

    /// The web views that draw the pages made ready ahead, and mermaid's script read: the first
    /// pictures are not kept waiting for them. Called once the window is up, not as a section is shown.
    func prewarm() {
        DocResources.preload()
        while idle.count + busy < Self.drawers {
            let drawer = Drawer(patience: patience)
            drawer.warm()
            idle.append(drawer)
        }
    }

    private func next() {
        while busy < Self.drawers, let key = urgent.first ?? later.first {
            if urgent.first != nil { urgent.removeFirst() } else { later.removeFirst() }
            guard let page = pages[key] else { continue }
            busy += 1
            // A finer picture of a page one of them holds is taken without loading it again.
            let holding = idle.firstIndex { $0.holds == key.same }
            let drawer = holding.map { idle.remove(at: $0) } ?? idle.popLast() ?? Drawer(patience: patience)
            let interval = DiagramTimes.begin("snapshot")
            drawer.draw(key, fills: page.fillsBox, page: { Self.page(page) }) { [weak self] picture in
                DiagramTimes.end(interval)
                guard let self else { return }
                self.busy -= 1
                self.idle.append(drawer)
                self.pages[key] = nil
                if var picture {
                    self.taken += 1
                    picture.number = self.taken
                    self.keep(picture, for: key)
                }
                let calls = self.waiting.removeValue(forKey: key) ?? []
                self.next()
                // Told on the main thread's next turn, not inside WebKit's reply: a card drawn again is drawn
                // as any other drawing is, whatever WebKit is in the middle of, and a diagram gone by then
                // finds nothing to draw (its callers hold it weakly).
                guard !calls.isEmpty else { return }
                DispatchQueue.main.async { calls.forEach { $0.call() } }
            }
        }
    }

    private func keep(_ picture: Picture, for key: Key) {
        func bytes(_ picture: Picture) -> Int { picture.image.bytesPerRow * picture.image.height }
        if let before = done[key.same]?[key.scale] { held -= bytes(before) }
        done[key.same, default: [:]][key.scale] = picture
        held += bytes(picture)
        used.touch(key)
        while held > Self.budget, used.count > 1, let oldest = used.removeOldest() {
            guard let gone = done[oldest.same]?.removeValue(forKey: oldest.scale) else { continue }
            held -= bytes(gone)
            if done[oldest.same]?.isEmpty == true { done[oldest.same] = nil }
        }
    }

    /// One web view in a window that is never shown, drawing a page at a time.
    ///
    /// A page is taken when it says it is ready, or — loaded, and never saying so — after
    /// `patience`; at the height it has as it is taken. A page that fills its box (`PlainPage`) runs no
    /// script of its own and so says nothing: it is taken once it has loaded and its images are decoded,
    /// asked from the app's side, at its box's height. What a page loaded before says is not
    /// this page's: the wait was counted from the load asked for, so with the main thread busy past
    /// it (the app reading a big project again on each write of an AI's) the picture was taken before
    /// the page's height was heard, a point tall, and kept — the card's body blank for as long as its
    /// text stayed the same; and the late word of that page could have the next page taken as it.
    private final class Drawer: NSObject, WKScriptMessageHandler, WKNavigationDelegate {
        private let window: NSWindow
        private let view: WKWebView
        private let patience: TimeInterval
        private var height: CGFloat = 1
        /// The page drawn fills its box, `height` tall, its own scripts off (`PlainPage`).
        private var fills = false
        private var finish: ((Picture?) -> Void)?
        private var width: CGFloat = 0
        private var scale: CGFloat = 1
        private var timer: Timer?
        /// The page loaded and drawn, at its width and appearance: a finer picture of it
        /// is taken as it is.
        private(set) var holds: Key?
        private var loading: Key?
        /// Which load the page is — its address (`sb-doc://page/<n>`), which what it says
        /// carries — and that load's navigation, until it has finished.
        private var load = 0
        private var navigation: WKNavigation?

        init(patience: TimeInterval) {
            self.patience = patience
            let config = WKWebViewConfiguration()
            config.setURLSchemeHandler(DocResources.shared, forURLScheme: MarkdownPage.scheme)
            config.websiteDataStore = .nonPersistent()
            view = WKWebView(frame: CGRect(x: 0, y: 0, width: 300, height: 100), configuration: config)
            window = NSWindow(contentRect: CGRect(x: -20000, y: -20000, width: 300, height: 100),
                              styleMask: .borderless, backing: .buffered, defer: false)
            super.init()
            // A handler is held strongly by the controller; the drawers live as long as
            // the app, so the loop is never in the way.
            config.userContentController.add(self, name: "size")
            config.userContentController.add(self, name: "ready")
            view.navigationDelegate = self
            PassiveWebView.transparent(view)
            window.isReleasedWhenClosed = false
            window.backgroundColor = .clear
            window.isOpaque = false
            window.contentView = view
        }

        /// Where the page of load `load` is: what tells its words from an earlier page's.
        private static func address(_ load: Int) -> URL? {
            URL(string: "\(MarkdownPage.scheme)://page/\(load)")
        }

        /// A blank page loaded, so that the process that draws pages is up before the first is asked.
        func warm() {
            view.loadHTMLString("<!doctype html><html><body></body></html>", baseURL: nil)
        }

        func draw(_ key: Key, fills: Bool, page: () -> String, then: @escaping (Picture?) -> Void) {
            finish = then
            scale = CGFloat(key.scale) / 100
            if holds == key.same {
                snap()
                return
            }
            holds = nil
            loading = key.same
            self.fills = fills
            width = CGFloat(key.width)
            height = fills ? CGFloat(max(1, key.height)) : 1
            let appearance = NSAppearance(named: key.dark ? .darkAqua : .aqua)
            window.appearance = appearance
            view.appearance = appearance
            // A page that fills its box is laid out in it from the start: how tall it is is the box's.
            let start = fills ? height : 100
            window.setContentSize(CGSize(width: width, height: start))
            view.frame = CGRect(x: 0, y: 0, width: width, height: start)
            timer?.invalidate()
            timer = nil
            load += 1
            navigation = view.loadHTMLString(page(), baseURL: Self.address(load))
        }

        func userContentController(_ controller: WKUserContentController, didReceive message: WKScriptMessage) {
            // Said by a page loaded before this one: nothing of this page's.
            guard message.frameInfo.isMainFrame, message.frameInfo.request.url == Self.address(load) else { return }
            if message.name == "ready" {
                holds = loading
                snap()
                return
            }
            if let value = message.body as? NSNumber { height = max(CGFloat(truncating: value), 1) }
        }

        /// Loaded: a page that never says it is ready (a script that fails before it can) is
        /// taken as it is after a while, rather than leaving the card waiting. Counted from
        /// here, not from the load asked for: a page is never taken before it is there.
        func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
            guard navigation === self.navigation else { return }
            self.navigation = nil
            guard finish != nil, holds == nil else { return }
            timer?.invalidate()
            timer = Timer.scheduledTimer(withTimeInterval: patience, repeats: false) { [weak self] _ in
                MainActor.assumeIsolated {
                    guard let self else { return }
                    self.holds = self.loading
                    self.snap()
                }
            }
            if fills { settle() }
        }

        /// A page whose own scripts are off, loaded: taken once its images are decoded (an SVG card's
        /// picture is one), a second at the most — asked from the app's side, which the page's policy
        /// and its scripts being off leave alone. Should the asking fail, `patience` still takes it.
        private func settle() {
            let asked = load
            view.callAsyncJavaScript(Self.decoded, arguments: [:], in: nil, in: .page) { [weak self] _ in
                MainActor.assumeIsolated {
                    guard let self, self.load == asked, self.finish != nil, self.holds == nil else { return }
                    self.holds = self.loading
                    self.snap()
                }
            }
        }

        private static let decoded = """
            await Promise.race([
                Promise.all(Array.from(document.images, image => image.decode().catch(() => {}))),
                new Promise(done => setTimeout(done, 1000))
            ]);
            """

        /// The page could not be loaded, or its process went: no picture, and the card asks
        /// again when it is drawn again.
        func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: any Error) {
            guard navigation === self.navigation else { return }
            fail()
        }

        func webView(_ webView: WKWebView, didFailProvisionalNavigation navigation: WKNavigation!,
                     withError error: any Error) {
            guard navigation === self.navigation else { return }
            fail()
        }

        func webViewWebContentProcessDidTerminate(_ webView: WKWebView) {
            fail()
        }

        private func fail() {
            timer?.invalidate()
            timer = nil
            navigation = nil
            holds = nil
            loading = nil
            // Its page gone: the next page asked of it is loaded afresh — after a while, so a page
            // that cannot be loaded is not tried again and again at once.
            load += 1
            guard let done = finish else { return }
            finish = nil
            DispatchQueue.main.asyncAfter(deadline: .now() + patience) { done(nil) }
        }

        private func snap() {
            guard finish != nil else { return }
            timer?.invalidate()
            timer = nil
            // As tall as its box: what it holds past it is cut.
            if fills {
                take()
                return
            }
            // Its height as it is now: what it said last may not have been heard yet.
            view.evaluateJavaScript("Math.ceil(document.body.getBoundingClientRect().height)") { [weak self] value, _ in
                MainActor.assumeIsolated {
                    guard let self else { return }
                    if let value = value as? NSNumber { self.height = max(CGFloat(truncating: value), 1) }
                    self.take()
                }
            }
        }

        private func take() {
            guard finish != nil else { return }
            let shown = fills ? height : min(height, DocSnapshots.tallest)
            window.setContentSize(CGSize(width: width, height: shown))
            view.frame = CGRect(x: 0, y: 0, width: width, height: shown)
            let config = WKSnapshotConfiguration()
            config.rect = CGRect(x: 0, y: 0, width: width, height: shown)
            config.afterScreenUpdates = false
            // Drawn finer, not enlarged: WebKit paints the page at this width.
            config.snapshotWidth = NSNumber(value: Double(width * scale))
            let scale = self.scale
            let whole = height
            // Laid out again at its full height before it is taken.
            view.evaluateJavaScript("document.body.getBoundingClientRect().height") { [weak self] _, _ in
                MainActor.assumeIsolated {
                    self?.view.takeSnapshot(with: config) { image, _ in
                        MainActor.assumeIsolated {
                            guard let self, let done = self.finish else { return }
                            self.finish = nil
                            let picture = image?.cgImage(forProposedRect: nil, context: nil, hints: nil)
                                .map { Picture(image: $0, height: shown, cut: !self.fills && whole > shown, scale: scale) }
                            done(picture)
                        }
                    }
                }
            }
        }

        /// The page loading itself, and nothing else — a page that fills its box with its own scripts off.
        func webView(_ webView: WKWebView, decidePolicyFor action: WKNavigationAction, preferences: WKWebpagePreferences,
                     decisionHandler: @escaping @MainActor (WKNavigationActionPolicy, WKWebpagePreferences) -> Void) {
            let mainFrame = action.targetFrame?.isMainFrame ?? false
            let visit = fills ? PlainPage.visit(action.navigationType, mainFrame: mainFrame, url: action.request.url)
                : MarkdownPage.visit(action.navigationType, mainFrame: mainFrame, url: action.request.url)
            preferences.allowsContentJavaScript = !fills
            decisionHandler(visit == .allow ? .allow : .cancel, preferences)
        }
    }
}
