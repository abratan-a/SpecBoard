import Foundation
import cmark_gfm
import cmark_gfm_extensions

/// Markdown to HTML, GitHub's flavour: tables, strikethrough, task lists and bare
/// links, on top of CommonMark.
///
/// What an agent writes into a description is data, not a page, so raw HTML in it
/// is never passed through — cmark-gfm leaves it out unless told otherwise, and
/// `tagfilter` would disarm `<script>` and friends even then. What comes out is
/// markup this app chose to emit, and nothing else.
///
/// A single line break is kept as one. That is how descriptions have always been
/// shown — as written — and prose in this app is written that way, a line at a time.
nonisolated enum Markdown {

    static let extensions = ["table", "strikethrough", "autolink", "tagfilter", "tasklist"]

    static func html(_ text: String) -> String {
        cmark_gfm_core_extensions_ensure_registered()
        let options = CMARK_OPT_DEFAULT | CMARK_OPT_HARDBREAKS | CMARK_OPT_VALIDATE_UTF8
        guard let parser = cmark_parser_new(options) else { return escaped(text) }
        defer { cmark_parser_free(parser) }
        for name in extensions {
            if let found = cmark_find_syntax_extension(name) {
                cmark_parser_attach_syntax_extension(parser, found)
            }
        }
        let bytes = Array(text.utf8)
        bytes.withUnsafeBufferPointer { buffer in
            buffer.baseAddress?.withMemoryRebound(to: CChar.self, capacity: buffer.count) {
                cmark_parser_feed(parser, $0, buffer.count)
            }
        }
        guard let document = cmark_parser_finish(parser) else { return escaped(text) }
        defer { cmark_node_free(document) }
        guard let rendered = cmark_render_html(document, options,
                                               cmark_parser_get_syntax_extensions(parser))
        else { return escaped(text) }
        defer { free(rendered) }
        return String(cString: rendered)
    }

    /// Whether any fenced block is a mermaid diagram — the only thing that needs a
    /// script to show.
    static func hasDiagram(_ html: String) -> Bool {
        html.contains("<code class=\"language-mermaid\">")
    }

    /// Plain text, made safe to sit in HTML. The fallback when parsing fails.
    static func escaped(_ text: String) -> String {
        "<p>" + text
            .replacingOccurrences(of: "&", with: "&amp;")
            .replacingOccurrences(of: "<", with: "&lt;")
            .replacingOccurrences(of: ">", with: "&gt;")
            .replacingOccurrences(of: "\n", with: "<br>") + "</p>"
    }
}
