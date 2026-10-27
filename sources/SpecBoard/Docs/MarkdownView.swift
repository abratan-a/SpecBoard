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
    /// For a formatdoc read as a document (`RowMarks`): the right-click on a row, with the row's path in the
    /// document, rearranges the menu WebKit is about to open — true when it did.
    var rowMenu: ((String, NSMenu) -> Bool)? = nil

    @State private var height: CGFloat = 18

    var body: some View {
        MarkdownWebView(page: MarkdownPage.html(text, size: size * Typography.shared.scale, refs: refs),
                        height: $height, open: refs?.open, rowMenu: rowMenu)
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
    ///
    /// A formatdoc's text with its rows marked (`RowMarks`) is drawn a row to a block, its items' brackets in
    /// the left margin; on the card editor's page (not `card`), a row right-clicked is told to the app.
    static func html(_ markdown: String, size: CGFloat, refs: MarkdownRefs? = nil, card: Bool = false) -> String {
        let resolved = refs?.resolve(markdown) ?? markdown
        let rows = RowMarks.parse(resolved)
        let body = rows.map { RowMarks.html($0) } ?? Markdown.html(resolved)
        let diagram = Markdown.hasDiagram(body)
        let px = String(format: "%.1f", size)
        let classes = rows == nil ? [] : ["sb-rows"] + (rows?.outlined == true ? ["sb-outline"] : [])
        let look = css(px) + (card ? cardCSS : "")
            + (rows == nil ? "" : outlineCSS + (card ? cardOutlineCSS : editorOutlineCSS))
        let menu = rows != nil && !card
        return """
        <!doctype html>
        <html><head><meta charset="utf-8">
        <meta name="color-scheme" content="light dark">
        <meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src data:; \
        style-src 'unsafe-inline'; font-src data:; script-src \(scheme): \(scriptHashes); \
        base-uri 'none'; form-action 'none'">
        <style>\(look)</style>
        </head><body\(classes.isEmpty ? "" : " class=\"\(classes.joined(separator: " "))\"")\
        \(card ? " data-shrink=\"all\"" : "")><article>\(body)</article>
        <script>\(sizing)</script>
        \(menu ? "<script>\(rowMenu)</script>" : "")
        \(diagram ? "<script src=\"\(scheme)://resources/mermaid.min.js\"></script><script>\(diagrams)</script>"
                  : "<script>\(readyNow)</script>")
        </body></html>
        """
    }

    /// The page's own scripts, each allowed by its digest, and no other inline
    /// script: one that got into the page some other way does not run.
    static let scriptHashes = [sizing, diagrams, readyNow, rowMenu]
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

    /// A formatdoc's rows as blocks (`RowMarks.html`): each item's bracket in the left margin, a colour to a
    /// depth. The blocks keep the document's spacing — a section has no margin, padding or border of its own,
    /// so its first and last child's margins pass through it — and draw a lit row and its item with shadows,
    /// which take no room.
    private static let outlineCSS = """
        :root { --sb-c0: #4f6f94; --sb-c1: #4c6b58; --sb-c2: #7a5f3f; --sb-c3: #6a5c88; --sb-c4: #4f7f7a;
                --sb-accent: #c98a1e; --sb-lit: rgba(224, 169, 81, 0.22); --sb-held: rgba(224, 169, 81, 0.06);
                --sb-held-line: rgba(201, 138, 30, 0.6); }
        @media (prefers-color-scheme: dark) {
            :root { --sb-c0: #6f8db0; --sb-c1: #5f7f6c; --sb-c2: #8a7357; --sb-c3: #8a7db0; --sb-c4: #6f9a96;
                    --sb-accent: #e0a951; --sb-lit: rgba(224, 169, 81, 0.14); --sb-held: rgba(224, 169, 81, 0.04);
                    --sb-held-line: rgba(224, 169, 81, 0.55); }
        }
        section.sb-row { position: relative; border-radius: 3px; }
        .sb-bracket { position: absolute; top: 0.25em; bottom: 0; }
        .sb-bracket::before { content: ""; position: absolute; top: 0; bottom: 0; left: 0;
            border: solid var(--sb-c); border-right: none; border-radius: 2px 0 0 2px; }
        section[data-depth="0"] > .sb-bracket { --sb-c: var(--sb-c0); }
        section[data-depth="1"] > .sb-bracket { --sb-c: var(--sb-c1); }
        section[data-depth="2"] > .sb-bracket { --sb-c: var(--sb-c2); }
        section[data-depth="3"] > .sb-bracket { --sb-c: var(--sb-c3); }
        section[data-depth="4"] > .sb-bracket { --sb-c: var(--sb-c4); }
        .sb-tag { display: none; }
        """

    /// On the card editor's page: brackets 7pt apart, each 7pt wide to rest the pointer on; one rested on
    /// lights its item and shows the item's heading and path after the heading, placed where the heading's
    /// words end without taking room. A row right-clicked (`rowMenu`) is lit and its item outlined.
    private static let editorOutlineCSS = """
        body.sb-rows { padding: 0 \(Int(RowMarks.edge))px \(Int(RowMarks.edge))px 0; }
        body.sb-outline { padding-left: \(Int(RowMarks.gutter))px; }
        .sb-bracket { width: 7px; cursor: default; }
        .sb-bracket::before { width: 5px; border-width: 1.5px; }
        section[data-depth="0"] > .sb-bracket { left: \(Int(-RowMarks.gutter) + 6)px; }
        section[data-depth="1"] > .sb-bracket { left: \(Int(-RowMarks.gutter) + 13)px; }
        section[data-depth="2"] > .sb-bracket { left: \(Int(-RowMarks.gutter) + 20)px; }
        section[data-depth="3"] > .sb-bracket { left: \(Int(-RowMarks.gutter) + 27)px; }
        section[data-depth="4"] > .sb-bracket { left: \(Int(-RowMarks.gutter) + 34)px; }
        section.sb-item > .sb-bracket:hover { --sb-c: var(--sb-accent); }
        section.sb-item > .sb-bracket:hover::before { border-width: 2px; border-right: none; }
        section.sb-item:has(> .sb-bracket:hover), .sb-held { background: var(--sb-held);
            box-shadow: 0 0 0 6px var(--sb-held), 0 0 0 7px var(--sb-held-line); border-radius: 5px; }
        .sb-lit { background: var(--sb-lit); box-shadow: 0 0 0 3px var(--sb-lit), 0 0 0 4px var(--sb-accent); }
        section.sb-item:has(> .sb-bracket:hover) > :is(h1, h2, h3, h4, h5, h6) > .sb-tag {
            display: inline-block; position: absolute; z-index: 2; margin: 0.15em 0 0 0.6em; padding: 0.1em 0.5em;
            font-size: 11px; font-weight: 400; line-height: 1.4; white-space: nowrap; -webkit-user-select: none;
            border-radius: 4px; border: 0.5px solid rgba(127, 127, 127, 0.4); background: -apple-system-control-background;
            box-shadow: 0 2px 8px rgba(0, 0, 0, 0.25); }
        .sb-tag::before { content: attr(data-heading); }
        .sb-tag::after { content: attr(data-path); margin-left: 0.4em; font-family: ui-monospace, Menlo, monospace;
            color: -apple-system-secondary-label; }
        """

    /// On a card's picture: thinner brackets 3pt apart in a narrower margin. Nothing on a picture is pointed at.
    private static let cardOutlineCSS = """
        body.sb-outline { padding-left: \(Int(RowMarks.cardGutter))px; }
        .sb-bracket { width: 3px; }
        .sb-bracket::before { width: 3px; border-width: 1px; }
        section[data-depth="0"] > .sb-bracket { left: \(Int(-RowMarks.cardGutter) + 1)px; }
        section[data-depth="1"] > .sb-bracket { left: \(Int(-RowMarks.cardGutter) + 4)px; }
        section[data-depth="2"] > .sb-bracket { left: \(Int(-RowMarks.cardGutter) + 7)px; }
        section[data-depth="3"] > .sb-bracket { left: \(Int(-RowMarks.cardGutter) + 10)px; }
        section[data-depth="4"] > .sb-bracket { left: \(Int(-RowMarks.cardGutter) + 13)px; }
        """

    /// A right-click tells the app which row is under the pointer — the innermost, by its path in the
    /// document; "" outside every row — before WebKit opens its menu, which the app rearranges as it opens
    /// (`PassiveWebView.willOpenMenu`). The row is lit and the item holding it outlined until the menu closes
    /// (`sbUnlight`, called by the app).
    private static let rowMenu = """
        const sbLit = [];
        const sbUnlight = () => { for (const box of sbLit.splice(0)) box.classList.remove("sb-lit", "sb-held"); };
        addEventListener("contextmenu", event => {
            sbUnlight();
            const row = event.target instanceof Element ? event.target.closest("section.sb-row") : null;
            window.webkit.messageHandlers.row?.postMessage(row ? row.dataset.row : "");
            if (!row) return;
            row.classList.add("sb-lit");
            sbLit.push(row);
            const item = row.parentElement?.closest("section.sb-item");
            if (item) {
                item.classList.add("sb-held");
                sbLit.push(item);
            }
        });
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
    /// The row the page said is under the pointer of the right-click being opened (`MarkdownPage.rowMenu`),
    /// by its path in the document: taken by the menu that opens next, and by no other.
    var rowHit: String?
    /// What the menu of a right-click on a row is made into (`MarkdownView.rowMenu`); nil leaves WebKit's.
    var rowMenu: ((String, NSMenu) -> Bool)?

    override func willOpenMenu(_ menu: NSMenu, with event: NSEvent) {
        if let row = rowHit, let rowMenu { _ = rowMenu(row, menu) }
        rowHit = nil
        super.willOpenMenu(menu, with: event)
    }

    /// The row lit for the menu is let go of once the menu is gone, whatever was picked in it.
    override func didCloseMenu(_ menu: NSMenu, with event: NSEvent?) {
        super.didCloseMenu(menu, with: event)
        guard rowMenu != nil else { return }
        evaluateJavaScript("if (typeof sbUnlight === \"function\") sbUnlight();")
    }

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
    var rowMenu: ((String, NSMenu) -> Bool)? = nil

    func makeCoordinator() -> Coordinator { Coordinator(height: $height) }

    func makeNSView(context: Context) -> PassiveWebView {
        let config = WKWebViewConfiguration()
        config.setURLSchemeHandler(DocResources.shared, forURLScheme: MarkdownPage.scheme)
        config.websiteDataStore = .nonPersistent()
        config.userContentController.add(context.coordinator, name: "size")
        config.userContentController.add(context.coordinator, name: "ready")
        config.userContentController.add(context.coordinator, name: "row")
        let view = PassiveWebView(frame: .zero, configuration: config)
        view.rowMenu = rowMenu
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
        view.rowMenu = rowMenu
        guard context.coordinator.page != page else { return }
        load(page, into: view, context.coordinator)
    }

    static func dismantleNSView(_ view: PassiveWebView, coordinator: Coordinator) {
        view.configuration.userContentController.removeScriptMessageHandler(forName: "size")
        view.configuration.userContentController.removeScriptMessageHandler(forName: "ready")
        view.configuration.userContentController.removeScriptMessageHandler(forName: "row")
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
            if message.name == "row" {
                let row = message.body as? String ?? ""
                (message.webView as? PassiveWebView)?.rowHit = row.isEmpty ? nil : row
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
