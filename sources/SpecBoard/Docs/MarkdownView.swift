import AppKit
import CryptoKit
import SwiftUI
import WebKit
import os

/// Markdown, shown as a document: headings, lists, tables, code, and mermaid
/// diagrams drawn in place.
///
/// Drawn by WebKit, sized to what it holds, inside whatever scrolls around it. It
/// never touches the network: the page is built here, and the one script it may
/// load — mermaid — comes out of the app's own bundle through `sb-doc:`. A link is
/// opened in the browser only when someone clicks it.
struct MarkdownView: View {
    var text: String
    /// Points, before the app's text scale — the same number `.app(_:)` takes.
    var size: CGFloat = 12
    /// For a note: how its `[[path]]` and `{{path}}` read. Nil leaves them as text.
    var refs: MarkdownRefs? = nil

    @State private var height: CGFloat = 18

    var body: some View {
        MarkdownWebView(page: MarkdownPage.html(text, size: size * Typography.shared.scale, refs: refs),
                        height: $height, open: refs?.open)
            .frame(height: height)
            .frame(maxWidth: .infinity)
    }
}

/// How a note's references are shown: a place that exists becomes a link the app
/// follows, a value is put in, and one that does not resolve is struck through — with why
/// a pointer's rest away, where that is known (`problem`).
struct MarkdownRefs {
    var exists: (String) -> Bool
    /// What the text's `{{…}}` put in, by path, all asked at once — those it leaves out put in
    /// nothing, and are struck through.
    var values: ([String]) -> [String: String]
    var open: (String) -> Void
    /// A `![[…]]`, drawn. Nil leaves it as a place.
    var embed: ((NoteText.Ref) -> String)? = nil
    /// Why a place does not resolve, as the core says it (`resolvePath`'s hint): shown on
    /// the struck-through place when the pointer rests on it. Nil says nothing more.
    var problem: ((String) -> String?)? = nil
    /// Only its `[[…]]` read: a table's, whose cells hold data's values, not a body to fill in.
    var placesOnly = false

    /// The Markdown with its references written out, before it is rendered. What an embed
    /// draws has its `[[…]]` read too — a table's cells naming cards — and nothing more, so
    /// one embed never draws another.
    func resolve(_ text: String) -> String {
        if placesOnly { return places(text) }
        var asked: [String] = []
        for ref in NoteText.refs(text) where ref.kind == .value && !asked.contains(ref.path) {
            asked.append(ref.path)
        }
        let put = asked.isEmpty ? [:] : values(asked)
        return NoteText.transform(text) { ref in
            switch ref.kind {
            case .place:
                return place(ref)
            case .value:
                return DocumentParts.value(put[ref.path], for: ref)
            case .embed:
                return embed.map { places($0(ref)) } ?? Self.code(ref.written(ref.path))
            }
        }
    }

    /// The Markdown with its `[[…]]` written out, its `{{…}}` and `![[…]]` as written.
    func places(_ text: String) -> String {
        NoteText.transform(text) { ref in ref.kind == .place ? place(ref) : nil }
    }

    /// One `[[…]]`: a link the app follows, or struck through when it does not resolve.
    private func place(_ ref: NoteText.Ref) -> String {
        let found = exists(ref.path)
        return DocumentParts.place(ref, exists: found, why: found ? nil : problem?(ref.path))
    }

    /// A link's title, which the page shows as the pointer rests on it: on one line, its
    /// quotes and backslashes kept as they are.
    nonisolated static func title(_ text: String) -> String {
        let line = text.components(separatedBy: .newlines).joined(separator: " ")
        return "\"" + line.replacingOccurrences(of: "\\", with: "\\\\").replacingOccurrences(of: "\"", with: "\\\"") + "\""
    }

    /// Inline code that survives any backticks in it.
    nonisolated static func code(_ text: String) -> String {
        let longest = text.split(separator: "`", omittingEmptySubsequences: false).count > 1
            ? text.components(separatedBy: CharacterSet(charactersIn: "`").inverted)
                .map(\.count).max() ?? 0 : 0
        let fence = String(repeating: "`", count: longest + 1)
        let pad = text.hasPrefix("`") || text.hasSuffix("`") ? " " : ""
        return "\(fence)\(pad)\(text)\(pad)\(fence)"
    }
}

/// The page around a piece of Markdown. Kept apart from the view so a test can
/// read what the view would load.
enum MarkdownPage {

    nonisolated static let scheme = "sb-doc"

    enum Visit: Equatable { case allow, cancel, browser(URL), inApp(String) }

    /// Where a navigation in the page may go: the page loading itself, and nothing else
    /// on its own. A click opens a web or mail address in the browser, and a note's
    /// `sb-nav:` link in the app; any other click goes nowhere.
    static func visit(_ type: WKNavigationType, mainFrame: Bool, url: URL?) -> Visit {
        if type == .other, mainFrame, url?.scheme == scheme || url?.scheme == "about" { return .allow }
        guard type == .linkActivated, let url else { return .cancel }
        let kind = url.scheme?.lowercased() ?? ""
        if kind == navigation {
            let raw = String(url.absoluteString.dropFirst(navigation.count + 1))
            return .inApp(raw.removingPercentEncoding ?? raw)
        }
        return ["http", "https", "mailto"].contains(kind) ? .browser(url) : .cancel
    }
    /// A link inside the app — a note's `[[path]]`. Never loaded; the click is
    /// handed back to whoever showed the page.
    nonisolated static let navigation = "sb-nav"
    /// A place that does not resolve, struck through: a link only to carry why as its title
    /// (`MarkdownRefs.problem`). Drawn as text, and a click on it goes nowhere.
    nonisolated static let broken = "sb-broken"

    /// `card`: the page a card's picture is taken of — nothing scrolls in a picture,
    /// so every diagram is shrunk to the column however far that takes it and code wraps;
    /// a table is laid out by its content and cut at the column's edge when wider. The
    /// card is widened or zoomed into to read them.
    static func html(_ markdown: String, size: CGFloat, refs: MarkdownRefs? = nil, card: Bool = false) -> String {
        let body = Markdown.html(refs?.resolve(markdown) ?? markdown)
        let diagram = Markdown.hasDiagram(body)
        let px = String(format: "%.1f", size)
        return """
        <!doctype html>
        <html><head><meta charset="utf-8">
        <meta name="color-scheme" content="light dark">
        <meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src data:; \
        style-src 'unsafe-inline'; font-src data:; script-src \(scheme): \(scriptHashes); \
        base-uri 'none'; form-action 'none'">
        <style>\(css(px))\(card ? cardCSS : "")</style>
        </head><body\(card ? " data-shrink=\"all\"" : "")><article>\(body)</article>
        <script>\(sizing)</script>
        \(diagram ? "<script src=\"\(scheme)://resources/mermaid.min.js\"></script><script>\(diagrams)</script>"
                  : "<script>\(readyNow)</script>")
        </body></html>
        """
    }

    /// The page's own scripts, each allowed by its digest, and no other inline
    /// script: one that got into the page some other way does not run.
    static let scriptHashes = [sizing, diagrams, readyNow]
        .map { "'sha256-\(Data(SHA256.hash(data: Data($0.utf8))).base64EncodedString())'" }
        .joined(separator: " ")

    /// The page's look. Words break and tables are laid out as GitHub's and Forgejo's: a word too
    /// long for the line breaks (`break-word`; `anywhere` let a table squeeze its first column to a
    /// letter or two), a table as wide as it needs up to the column and scrolled sideways past it,
    /// its cells padded 6px 13px at 13px, a line above each row and every other row shaded, its
    /// heads semibold — in the app's translucent greys and its own size of letter.
    private static func css(_ px: String) -> String {
        """
        html, body { margin: 0; padding: 0; background: transparent; overflow: hidden; }
        body { font: \(px)px -apple-system, "Hiragino Sans", sans-serif; line-height: 1.55;
               color: -apple-system-label; overflow-wrap: break-word; -webkit-user-select: text; }
        article > :first-child { margin-top: 0; }
        article > :last-child { margin-bottom: 0; }
        p, ul, ol, pre, table, blockquote, .mermaid { margin: 0 0 0.65em; }
        ul, ol { padding-left: 1.4em; }
        li > p { margin: 0; }
        h1, h2, h3, h4, h5, h6 { margin: 0.9em 0 0.4em; line-height: 1.3; }
        h1 { font-size: 1.35em; } h2 { font-size: 1.2em; } h3 { font-size: 1.08em; }
        h4, h5, h6 { font-size: 1em; }
        code { font: 0.92em ui-monospace, Menlo, monospace;
               background: rgba(127, 127, 127, 0.14); padding: 0.05em 0.3em; border-radius: 3px; }
        pre { overflow-x: auto; padding: 0.55em 0.75em; background: rgba(127, 127, 127, 0.12);
              border-radius: 5px; }
        pre code { background: none; padding: 0; }
        table { border-collapse: collapse; border-spacing: 0; display: block; width: max-content; max-width: 100%;
                overflow-x: auto; }
        tr { border-top: 1px solid rgba(127, 127, 127, 0.35); }
        tr:nth-child(2n) { background: rgba(127, 127, 127, 0.07); }
        th, td { border: 1px solid rgba(127, 127, 127, 0.35); padding: 0.46em 1em; text-align: left; }
        th { font-weight: 600; }
        blockquote { margin-left: 0; padding-left: 0.8em; border-left: 3px solid rgba(127, 127, 127, 0.4);
                     color: -apple-system-secondary-label; }
        hr { border: none; border-top: 1px solid rgba(127, 127, 127, 0.35); }
        a { color: -webkit-link; }
        a[href^="sb-broken:"] { color: inherit; text-decoration: none; cursor: help; }
        input[type=checkbox] { margin: 0 0.35em 0 0; }
        .mermaid { overflow-x: auto; overflow-y: hidden; }
        .mermaid svg { display: block; max-width: none; }
        .mermaid::-webkit-scrollbar, pre::-webkit-scrollbar, table::-webkit-scrollbar { height: 8px; }
        .mermaid::-webkit-scrollbar-track, pre::-webkit-scrollbar-track, table::-webkit-scrollbar-track {
            background: rgba(127, 127, 127, 0.12); border-radius: 4px; }
        .mermaid::-webkit-scrollbar-thumb, pre::-webkit-scrollbar-thumb, table::-webkit-scrollbar-thumb {
            background: rgba(127, 127, 127, 0.5); border-radius: 4px; }
        .more-right { -webkit-mask-image: linear-gradient(to right, #000 calc(100% - 40px), transparent); }
        .more-left { -webkit-mask-image: linear-gradient(to left, #000 calc(100% - 40px), transparent); }
        .more-left.more-right { -webkit-mask-image: linear-gradient(to right, transparent, #000 40px,
            #000 calc(100% - 40px), transparent); }
        """
    }

    /// What a card's page does differently: code wraps and a diagram's box cuts what is past it. A table is
    /// not made to fit it: it keeps the base rule (laid out by its content, a word never broken), and a
    /// table wider than the column is cut at the right with the fade, as far as the card is wide —
    /// widening the card shows the rest. Equal fixed columns would break words in the narrow ones.
    private static let cardCSS = """
        pre { white-space: pre-wrap; overflow: hidden; }
        .mermaid { overflow: hidden; }
        """

    /// Tells the view how tall the page is, whenever that changes. What scrolls sideways —
    /// a diagram, a table, code wider than the column — has its scroll bar shown and fades
    /// at the side that has more, so it reads as more to see.
    private static let sizing = """
        const post = () => window.webkit.messageHandlers.size.postMessage(
            Math.ceil(document.body.getBoundingClientRect().height));
        new ResizeObserver(post).observe(document.body);
        post();
        const edges = box => {
            const over = box.scrollWidth - box.clientWidth;
            box.classList.toggle("more-left", over > 1 && box.scrollLeft > 1);
            box.classList.toggle("more-right", over > 1 && box.scrollLeft < over - 1);
        };
        const watch = () => {
            for (const box of document.querySelectorAll(".mermaid, pre, table")) {
                edges(box);
                if (!box.dataset.watched) {
                    box.dataset.watched = "1";
                    box.addEventListener("scroll", () => edges(box), { passive: true });
                }
            }
        };
        addEventListener("resize", watch);
        const ready = () => { watch(); window.webkit.messageHandlers.ready?.postMessage(1); };
        """

    /// Without a diagram, the page is ready as soon as it is sized.
    private static let readyNow = "ready();"

    /// A fenced ```mermaid block becomes the diagram it describes, in the page's own
    /// type and size. Each is drawn at the size it is, shrunk no further than to 80% to
    /// fit the column, and scrolled sideways past that — a wide flow shrunk to the column
    /// is too small to read. On a card's page (`data-shrink="all"`), shrunk all the way:
    /// a picture does not scroll, and the card zooms. What a diagram draws past its own edges (a long
    /// message between two parts with another between them) is taken into it, not cut off.
    private static let diagrams = """
        for (const code of document.querySelectorAll("code.language-mermaid")) {
            const box = document.createElement("div");
            box.className = "mermaid";
            box.textContent = code.textContent;
            code.parentElement.replaceWith(box);
        }
        const text = getComputedStyle(document.body);
        const family = text.fontFamily, size = parseFloat(text.fontSize);
        const wide = { useMaxWidth: false };
        mermaid.initialize({ startOnLoad: false, securityLevel: "strict",
            theme: matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "default",
            fontFamily: family, fontSize: size,
            themeVariables: { fontFamily: family, fontSize: size + "px" },
            flowchart: wide, class: wide,
            sequence: { useMaxWidth: false, actorFontFamily: family, actorFontSize: size,
                        messageFontFamily: family, messageFontSize: size,
                        noteFontFamily: family, noteFontSize: size } });
        const drawn = [];
        const floor = document.body.dataset.shrink === "all" ? 0 : 0.8;
        const fit = () => {
            for (const { box, svg, width, height } of drawn) {
                const room = box.clientWidth;
                const scale = width > room ? Math.max(room / width, floor) : 1;
                svg.style.width = (width * scale) + "px";
                svg.style.height = (height * scale) + "px";
            }
        };
        mermaid.run().catch(() => {}).finally(() => {
            for (const box of document.querySelectorAll(".mermaid")) {
                const svg = box.querySelector("svg");
                if (!svg) continue;
                const pad = 4, all = svg.getBBox();
                svg.setAttribute("viewBox", [all.x - pad, all.y - pad, all.width + 2 * pad, all.height + 2 * pad].join(" "));
                svg.removeAttribute("width");
                svg.removeAttribute("height");
                svg.style.maxWidth = "none";
                drawn.push({ box, svg, width: all.width + 2 * pad, height: all.height + 2 * pad });
            }
            fit();
            addEventListener("resize", () => { fit(); watch(); });
            post(); ready();
        });
        """
}

/// Serves `sb-doc://resources/<name>` from the app bundle, and nothing else.
final class DocResources: NSObject, WKURLSchemeHandler {
    static let shared = DocResources()
    private nonisolated static let served: Set<String> = ["mermaid.min.js"]

    /// The file in the bundle an address names, or nil for anything not served.
    nonisolated static func file(for url: URL) -> URL? {
        guard url.scheme == MarkdownPage.scheme, url.host() == "resources", served.contains(url.lastPathComponent)
        else { return nil }
        return Bundle.main.url(forResource: url.deletingPathExtension().lastPathComponent, withExtension: url.pathExtension)
    }

    /// What is served, read once and kept: mermaid's script is 3.5 MB, and every page that draws a
    /// diagram asks for it.
    private nonisolated static let read = OSAllocatedUnfairLock<[URL: Data]>(initialState: [:])

    /// What is served, read from the bundle the first time it is asked for.
    nonisolated static func data(of file: URL) -> Data? {
        if let kept = read.withLock({ $0[file] }) { return kept }
        guard let data = try? Data(contentsOf: file) else { return nil }
        read.withLock { $0[file] = data }
        return data
    }

    /// Everything served, read off the main thread ahead of the first page that asks for it.
    nonisolated static func preload() {
        DispatchQueue.global(qos: .utility).async {
            for name in served {
                let url = URL(string: "\(MarkdownPage.scheme)://resources/\(name)")
                if let file = url.flatMap(Self.file(for:)) { _ = data(of: file) }
            }
        }
    }

    func webView(_ webView: WKWebView, start task: any WKURLSchemeTask) {
        guard let url = task.request.url, let file = Self.file(for: url), let data = Self.data(of: file) else {
            task.didFailWithError(URLError(.fileDoesNotExist))
            return
        }
        task.didReceive(URLResponse(url: url, mimeType: "text/javascript",
                                    expectedContentLength: data.count, textEncodingName: "utf-8"))
        task.didReceive(data)
        task.didFinish()
    }

    func webView(_ webView: WKWebView, stop task: any WKURLSchemeTask) {}
}

/// A web view that lets the scroll wheel through: it is sized to its page, so it
/// has nothing to scroll, and a panel with one inside would otherwise stop
/// scrolling whenever the pointer crossed it.
final class PassiveWebView: WKWebView {
    /// The page is drawn — its diagrams too — and says so. A picture of the window
    /// waits for this (`WindowCapture`), or it shows a page half made.
    var ready = false

    override func scrollWheel(with event: NSEvent) {
        // Sideways is the page's own: a diagram wider than the column scrolls in it.
        if abs(event.scrollingDeltaX) > abs(event.scrollingDeltaY) {
            super.scrollWheel(with: event)
            return
        }
        nextResponder?.scrollWheel(with: event)
    }

    /// Transparent, so the page sits on the panel rather than on a background of its
    /// own. WebKit has the setter as `_setDrawsBackground:` — what key-value coding finds
    /// for the key — so looking only for `setDrawsBackground:` missed it, and the page
    /// was drawn on grey (white in the light appearance).
    static func transparent(_ view: WKWebView) {
        if view.responds(to: NSSelectorFromString("_setDrawsBackground:"))
            || view.responds(to: NSSelectorFromString("setDrawsBackground:")) {
            view.setValue(false, forKey: "drawsBackground")
        }
        view.underPageBackgroundColor = .clear
    }
}

struct MarkdownWebView: NSViewRepresentable {
    var page: String
    @Binding var height: CGFloat
    /// A click on an `sb-nav:` link, with the path it names.
    var open: ((String) -> Void)? = nil

    func makeCoordinator() -> Coordinator { Coordinator(height: $height) }

    func makeNSView(context: Context) -> PassiveWebView {
        let config = WKWebViewConfiguration()
        config.setURLSchemeHandler(DocResources.shared, forURLScheme: MarkdownPage.scheme)
        config.websiteDataStore = .nonPersistent()
        config.userContentController.add(context.coordinator, name: "size")
        config.userContentController.add(context.coordinator, name: "ready")
        let view = PassiveWebView(frame: .zero, configuration: config)
        view.navigationDelegate = context.coordinator
        PassiveWebView.transparent(view)
        view.setAccessibilityLabel(L10n[.Common.wordDescription])
        load(page, into: view, context.coordinator)
        return view
    }

    func updateNSView(_ view: PassiveWebView, context: Context) {
        view.setAccessibilityLabel(L10n[.Common.wordDescription])
        context.coordinator.height = $height
        context.coordinator.open = open
        guard context.coordinator.page != page else { return }
        load(page, into: view, context.coordinator)
    }

    static func dismantleNSView(_ view: PassiveWebView, coordinator: Coordinator) {
        view.configuration.userContentController.removeScriptMessageHandler(forName: "size")
        view.configuration.userContentController.removeScriptMessageHandler(forName: "ready")
    }

    private func load(_ page: String, into view: PassiveWebView, _ coordinator: Coordinator) {
        view.ready = false
        coordinator.page = page
        coordinator.open = open
        view.loadHTMLString(page, baseURL: URL(string: "\(MarkdownPage.scheme)://page/"))
    }

    final class Coordinator: NSObject, WKNavigationDelegate, WKScriptMessageHandler {
        var height: Binding<CGFloat>
        var page: String?
        var open: ((String) -> Void)?

        init(height: Binding<CGFloat>) { self.height = height }

        func userContentController(_ controller: WKUserContentController,
                                   didReceive message: WKScriptMessage) {
            if message.name == "ready" {
                (message.webView as? PassiveWebView)?.ready = true
                return
            }
            guard let value = message.body as? NSNumber else { return }
            let next = max(CGFloat(truncating: value), 1)
            if abs(next - height.wrappedValue) > 0.5 { height.wrappedValue = next }
        }

        /// The page itself, and nothing else. A link somebody clicks opens in the
        /// browser, and only if it is a web address.
        func webView(_ webView: WKWebView, decidePolicyFor action: WKNavigationAction,
                     decisionHandler: @escaping @MainActor (WKNavigationActionPolicy) -> Void) {
            switch MarkdownPage.visit(action.navigationType, mainFrame: action.targetFrame?.isMainFrame ?? false,
                                      url: action.request.url) {
            case .allow:
                decisionHandler(.allow)
                return
            case .browser(let url): NSWorkspace.shared.open(url)
            case .inApp(let path): open?(path)
            case .cancel: break
            }
            decisionHandler(.cancel)
        }
    }
}
