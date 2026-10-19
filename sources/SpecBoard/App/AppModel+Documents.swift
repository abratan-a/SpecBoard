import Foundation

/// The cards drawn as documents (`DocumentTexts`): what each shows, read from the whole project open —
/// a diagram holds only its section, and a path to another section would read as one that is not there.
extension AppModel {
    /// What `shown` shows on a diagram now; made off the main thread when the project has changed
    /// since, `ready` called once it is in.
    func documentFace(_ shown: SchemaNode, ready: (() -> Void)?) -> DocumentTexts.Face {
        guard let detail, let core = core(of: detail.project.id) else { return .none }
        return documentTexts.face(of: shown, in: detail, tree: openTrees[detail.project.id] ?? "", core: core,
                                  ready: ready)
    }

    /// What `shown` shows, made now on this thread, the core asked and waited for: what a picture of
    /// it is taken of (`DocSnapshots`), the same page `documentFace` makes.
    func documentTextNow(_ shown: SchemaNode) -> String? {
        documentPageNow(shown)?.text
    }

    /// The same, with what it is drawn as: Markdown, or an SVG or HTML card's page.
    func documentPageNow(_ shown: SchemaNode) -> DocumentPage? {
        guard let detail, let core = core(of: detail.project.id) else { return nil }
        let id = GoalCardMap.isCard(shown.id) ? GoalCardMap.card(of: shown.id) : shown.id
        guard let card = detail.cardsById[id] else { return nil }
        let source = DocumentSource.of(card, in: detail)
        let said = DocumentParts.ask([source], of: core, project: detail.project.id,
                                     tree: openTrees[detail.project.id] ?? "")
        return DocumentPage(DocumentParts.page(source, said[card.id] ?? .init()), form: source.form)
    }

    /// What the ui part `shown` shows on the map: its face, read for its section in the tree open (`PartFaces`).
    /// None while what a merge would leave is looked at: its tree is the app's, and no tool names it.
    func partFace(_ shown: SchemaNode, ready: (() -> Void)?) -> PartFaces.Answer {
        guard resultView == nil, let detail else { return .none }
        let tree = openTreeArgument["branch"] as? String ?? ""
        return partFaces.face(of: shown, in: detail, tree: tree, core: core(of: detail.project.id), ready: ready)
    }

    /// The open section, when it is a ui section: what the bar's switches for the parts' tags and notes are for.
    var shownUiSection: ProjectSection? {
        guard case .section(let id) = here, let section = detail?.directory.section(id: id), section.kind == .ui else {
            return nil
        }
        return section
    }
}
