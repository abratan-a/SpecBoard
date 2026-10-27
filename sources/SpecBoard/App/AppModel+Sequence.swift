import Foundation

/// The sequence diagrams the window draws, asked of the core in the tree open
/// (`specboard://SpecBoard/Docs/Design:Overview` §11「シーケンス図の生命線と帯」): a flow's (its card editor) and an
/// actor's (the actor's section, `Map | Sequence`). The window builds none itself.
extension AppModel {
    /// The chart of the flow `card`, or of the actor section `section`, with the actors chosen on this Mac
    /// (`lifelines`; nil for the core's default depth) and the calls opened on it (`opened`: their keys),
    /// in `language` (the screen's when none). Asked once for each project as it is: a view drawn again draws
    /// what it was given.
    func sequenceChart(card: String? = nil, section: String? = nil, lifelines: [String]?, opened: [String]? = nil,
                       language asked: String? = nil) -> SequenceDiagram {
        let language = asked ?? Language.shared.code
        guard let detail else { return SequenceDiagram(refused: L10n[.Model.embedCantDraw], language: language) }
        let project = detail.project.id
        guard let core = core(of: project) else {
            return SequenceDiagram(refused: L10n[.Model.embedNotConnected], language: language)
        }
        let fields = SequenceDiagram.request(project: project, tree: openTrees[project] ?? "", card: card, section: section,
                                             lifelines: lifelines, opened: opened, language: language)
        let asked = (try? JSONSerialization.data(withJSONObject: fields, options: [.sortedKeys]))
            .map { String(decoding: $0, as: UTF8.self) } ?? ""
        let key = "\(dataGeneration) \(asked)"
        if let kept = sequenceCharts[key] { return kept }
        let drawn: SequenceDiagram
        do {
            drawn = SequenceDiagram(chart: try core.request("sequence_chart", fields), detail: detail, language: language)
        } catch {
            drawn = SequenceDiagram(refused: ErrorText.said(error), language: language)
        }
        // Only what the project is now is asked again.
        if sequenceCharts.count >= 16 { sequenceCharts.removeAll() }
        sequenceCharts[key] = drawn
        return drawn
    }

    /// The flow as mermaid in `shape`, as it is written out — the core's, in the project's export language,
    /// so that what is pasted is what an export of it says, whoever copies it.
    func flowMermaid(_ flow: SchemaNode, shape: String) -> String {
        guard let detail, let core = core(of: detail.project.id) else { return "%% " + L10n[.Model.embedNotConnected] }
        let project = detail.project.id
        let request: [String: Any] = ["project": project, "branch": openTrees[project] ?? "",
                                      "path": detail.directory.full(flow), "option": shape,
                                      "section": detail.directory.sectionId(of: flow), "exported": true,
                                      "lang": detail.project.exportedIn]
        do {
            let answer = try core.request("embed_diagram", request) as? [String: Any]
            if let mermaid = answer?["mermaid"] as? String { return mermaid }
            return "%% " + (answer?["problem"] as? String ?? L10n[.Model.embedCantDraw])
        } catch {
            return "%% " + ErrorText.said(error)
        }
    }

    /// A picked item of a sequence diagram: its row, its node, or its card.
    func pickSequence(_ origin: SequenceDiagram.Origin?) {
        guard let origin, let card = origin.card, let flow = schema(card) else { clearRow(); return }
        guard let node = origin.node, flow.nodes.contains(where: { $0.name == node }) else {
            selectSchema(card, follow: false)
            return
        }
        if let row = origin.row {
            selectRow(card, .nodeStep(node, StepEditing.path([.step(row)])))
        } else {
            selectNode(card, node)
        }
    }

    /// What is picked, as a sequence diagram's origin names it.
    var sequenceSelection: SequenceDiagram.Origin? {
        guard let card = selectedSchemaId else { return nil }
        switch selectedRow {
        case .node(let name)?: return .init(card: card, node: name)
        case .nodeStep(let name, let path)?:
            if case .step(let index)? = StepEditing.parts(path)?.first { return .init(card: card, node: name, row: index) }
            return .init(card: card, node: name)
        default: return .init(card: card)
        }
    }

    /// A flow's row pressed once in the nav panel while the map tab shows an actor's Sequence that frames the flow:
    /// the chart stays; the frame is opened (kept on this Mac, as a press on its head keeps it), the card is picked,
    /// and the chart is asked to bring the frame under its heads (`sequenceFocus`). False for any other press — a
    /// flow with no frame there, another actor's flow, a section on show — which opens the card's own section.
    func pickFrame(of card: String) -> Bool {
        guard case .section(let id) = here, let detail, detail.sections.first(where: { $0.id == id })?.isActor == true,
              ActorView.showsSequence(project: detail.project.id, actor: id),
              ActorSequence.frames(ActorSequence.drawn(model: self, actor: id)).contains(card) else { return false }
        ActorView.setOpen(true, flow: card, project: detail.project.id, actor: id)
        selectSchema(card, follow: false)
        sequenceFocus = card
        return true
    }
}
