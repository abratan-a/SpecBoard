import Foundation

/// A flow's or an action's diagram, drawn the way it is unless asked otherwise
/// (`specboard://SpecBoard/Docs/Design:DesignActions`「action と flow の統合」): a flow in the shape its
/// card keeps — its flowchart, or its sequence diagram — and an action as its sequence diagram. The
/// one way the export, a document's `![[…]]` and MCP pick the picture.
enum ChartDiagram {
    /// The shape it is drawn in: `asked` for a flow, else the one its card keeps.
    static func shape(of card: SchemaNode, asked: String? = nil) -> String {
        card.kind == .action ? "sequence" : (asked ?? card.diagramShape)
    }

    static func render(_ card: SchemaNode, in detail: ProjectDetail, shape asked: String? = nil,
                       exported: Bool = false, language: String) -> String {
        shape(of: card, asked: asked) == "sequence"
            ? ActionDiagram.render(card, in: detail, exported: exported, language: language)
            : FlowDiagram.render(card, in: detail, exported: exported, language: language)
    }
}
