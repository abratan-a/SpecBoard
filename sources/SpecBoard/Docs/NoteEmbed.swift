import Foundation

/// A diagram put into a note with `![[path]]` (`specboard://SpecBoard/Docs/Design:Overview` §9):
///
///     ![[Behavior/Operations:Checkout]]            a flow, as its flowchart
///     ![[Behavior/Payments:Refund|sequence]]       a flow, as a sequence diagram
///     ![[Data/Tenant:Order]]                       a card, and what it touches
///     ![[Data/Tenant:]]                            a section, as its diagram
///     ![[Data/Hosts:hosts]]                        a table, as its table
///
/// Drawn by the core from the project each time the note is shown (`embed_diagram`), with
/// the rev on its first line, so a copy that has fallen behind the project can be told — the
/// same diagram the export writes. The window draws no project's lines of its own.
enum NoteEmbed {

    /// What draws a diagram: the project's id, the tree it is read in, what `![[…]]` names, its
    /// `|option`, and the section the note is in — the mermaid (a table's Markdown for a table), or why
    /// it cannot be drawn. Set by the window when it has the core (`AppModel.start`).
    typealias Source = (_ project: String, _ path: String, _ option: String?, _ section: String) -> Result<Drawn, Problem>

    /// What `![[…]]` puts in: a diagram, or a table (`table`).
    enum Drawn {
        case mermaid(String)
        case markdown(String)

        /// What it says, either way.
        var text: String {
            switch self {
            case .mermaid(let text), .markdown(let text): text
            }
        }
    }

    struct Problem: Error {
        var why: String
    }

    static var source: Source?

    /// What a note sees: what is open — main, or what one branch sees.
    static func view(_ detail: ProjectDetail, for note: SchemaNode) -> ProjectDetail { detail }

    /// One `![[…]]` of `note`, drawn: a mermaid block, or a line saying why not.
    static func block(_ ref: NoteText.Ref, of note: SchemaNode, in seen: ProjectDetail) -> String {
        let directory = seen.directory
        let drawn = source?(seen.project.id, ref.path, ref.option, directory.sectionId(of: note))
            ?? .failure(Problem(why: L10n[.Docs.embedNoProject]))
        switch drawn {
        case .success(.mermaid(let mermaid)): return DocumentParts.block(ref, .mermaid(mermaid))
        case .success(.markdown(let table)): return DocumentParts.block(ref, .markdown(table))
        case .failure(let problem): return DocumentParts.block(ref, .problem(problem.why))
        }
    }

    /// The note's Markdown with each `![[…]]` drawn, and the rest left as written.
    static func expand(_ body: String, of note: SchemaNode, in detail: ProjectDetail) -> String {
        let seen = view(detail, for: note)
        return NoteText.transform(body) { ref in
            ref.kind == .embed ? block(ref, of: note, in: seen) : nil
        }
    }
}

/// What a note's `{{path}}`s put in (`specboard://SpecBoard/Docs/Design:DesignActions`): a field's
/// fixed value, the values of an enum, a data card's value — put in by the core from the project
/// each time the note is shown (`inserted_values`), the same the export writes, a note's paths in one
/// request.
enum NoteValues {

    /// What puts the values in: the project's id, the paths, and the section the note is in — the
    /// text of each path that puts one in. Set by the window when it has the core (`AppModel.start`).
    typealias Source = (_ project: String, _ paths: [String], _ section: String) -> [String: String]

    static var source: Source?
}
