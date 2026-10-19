import CryptoKit
import Foundation

/// One flow — or an action — as a mermaid sequence diagram (`specboard://SpecBoard/Docs/Design:Overview` §9):
/// what `SequenceDiagram` builds, written as mermaid for what is written out, a
/// document's embed and MCP. The app draws the same diagram on its own canvas
/// (`SequenceCanvas`).
enum ActionDiagram {
    /// The other end of a receive or a return that names none: whoever called it.
    nonisolated static func caller(in language: String) -> String { SequenceDiagram.caller(in: language) }

    private static func body(_ action: SchemaNode, in detail: ProjectDetail, exported: Bool = false,
                             language: String) -> [String] {
        SequenceDiagram.make(action, in: detail, exported: exported, language: language).mermaid()
    }

    /// `exported`: as it is written out — the app's paths left out, the head only the
    /// mark and the rev (`specboard://SpecBoard/Docs/Design:DesignSections` §7). The rev is the same either way:
    /// it is of the picture as the app draws it. `language`: the words it says for itself in, the
    /// project's for what is written out.
    static func render(_ action: SchemaNode, in detail: ProjectDetail, exported: Bool = false,
                       language: String) -> String {
        let drawn = body(action, in: detail, language: language)
        guard exported else {
            let header = "%% specboard project=\(detail.project.slug) \(action.kind == .flow ? "flow" : "action")="
                + "\(Ident.lineComment(detail.directory.exported(action))) "
                + "shape=sequence rev=\(revision(drawn))"
            return ([header] + drawn).joined(separator: "\n") + "\n"
        }
        return (["%% specboard rev=\(revision(drawn))"] + body(action, in: detail, exported: true, language: language))
            .joined(separator: "\n") + "\n"
    }

    /// What the diagram is drawn from, as the rev on its first line: it changes
    /// exactly when the picture would.
    private static func revision(_ body: [String]) -> String {
        let digest = SHA256.hash(data: Data(body.joined(separator: "\n").utf8))
        return String(digest.map { String(format: "%02x", $0) }.joined().prefix(12))
    }
}
