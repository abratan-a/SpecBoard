import Foundation
import WebKit

/// The page an SVG or HTML card's body is drawn as (`specboard://SpecBoard/Docs/Design:CardKinds`「SVG と HTML
/// のカード」): on its card (`DocSnapshots`) and in its editor's preview (`PageCardEditor`). Not read as
/// Markdown — cmark-gfm drops raw HTML — and filling the box it is drawn in rather than as tall as it reads.
///
/// Nothing of the body's runs: the policy is the one an exported HTML page carries (no `script-src`
/// at all), and the views that load these pages also turn the page's scripts off
/// (`allowsContentJavaScript`), so that what the app needs of the page — when it has loaded, how big
/// it is — is asked from the app's side, never told by a script of the page's.
enum PlainPage {

    /// The same policy the core writes into an exported HTML page (`export/pages.rs`), so a page looks
    /// the same on its card and in the file written.
    static let policy = "default-src 'none'; style-src 'unsafe-inline'; img-src data:; font-src data:; "
        + "base-uri 'none'; form-action 'none'"

    /// The page `page` is drawn as. Markdown is not this page's to build (`MarkdownPage`). `clipped`: cut
    /// at the box, as a card's picture is; the editor's preview scrolls instead.
    static func html(_ page: DocumentPage, clipped: Bool = true) -> String {
        switch page.form {
        case .svg: svg(page.text)
        case .html, .markdown: html(page.text, clipped: clipped)
        // Built whole by the core, its own policy and transparent ground in it.
        case .face: page.text
        }
    }

    /// An SVG shown as an image — so its scripts never run and what it refers to outside is never
    /// loaded — fitted inside the box, its sides kept in proportion, on the card's own ground.
    static func svg(_ text: String) -> String {
        let data = Data(text.utf8).base64EncodedString()
        return """
        <!doctype html>
        <html><head><meta charset="utf-8">
        <meta http-equiv="Content-Security-Policy" content="\(policy)">
        <style>
        html, body { margin: 0; padding: 0; width: 100%; height: 100%; overflow: hidden; background: transparent; }
        img { display: block; width: 100%; height: 100%; object-fit: contain; }
        </style>
        </head><body><img alt="" src="data:image/svg+xml;base64,\(data)"></body></html>
        """
    }

    /// An HTML body laid out at the box's width and cut at its edges, under the policy. It keeps its
    /// own colours: on a browser's white unless it says otherwise, whatever the app's appearance.
    ///
    /// The body goes inside the page's own `<body>` whole, as written: a body that is a whole page
    /// (`<html>`, `<head>`) is read by the browser into this one, its `<head>`'s styles kept, while a
    /// policy it carries outside the page's head is not read — it could only have narrowed this one.
    static func html(_ body: String, clipped: Bool = true) -> String {
        """
        <!doctype html>
        <html><head><meta charset="utf-8">
        <meta http-equiv="Content-Security-Policy" content="\(policy)">
        <meta name="color-scheme" content="light">
        <style>html { background: #fff; color: #000; }\(clipped ? " html, body { overflow: hidden; }" : "")</style>
        </head><body>
        \(body)
        </body></html>
        """
    }

    /// Where a navigation in the page may go: as a document's (`MarkdownPage.visit`) — the page loading
    /// itself, a click on `sb-nav:` opening that place in the app, on a web address the browser — and
    /// no further: a mail address is not opened from a page the app did not write.
    static func visit(_ type: WKNavigationType, mainFrame: Bool, url: URL?) -> MarkdownPage.Visit {
        let visit = MarkdownPage.visit(type, mainFrame: mainFrame, url: url)
        if case .browser(let address) = visit, !["http", "https"].contains(address.scheme?.lowercased() ?? "") {
            return .cancel
        }
        return visit
    }
}
