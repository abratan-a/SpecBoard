import SwiftUI

/// The ui section a frame holds (`specboard://SpecBoard/Docs/Design:CardKinds`「UI のセクションとはめ込み」), chosen
/// from the ui sections of the tree open by their paths: in the property panel for a frame's `target`, and
/// when a frame is made. The frame's own section is not offered; a choice that would come back round to it
/// the core refuses, its reason said as any refusal is.
struct FrameTargetPicker: View {
    var model: AppModel
    /// The section the frame is in, by id.
    var at: String
    /// The section chosen, by id; empty for none.
    var chosen: String
    var choose: (String) -> Void

    var body: some View {
        let targets = model.frameTargets(at: at)
        let known = targets.contains { $0.id == chosen }
        Picker("", selection: Binding(get: { known ? chosen : Self.none }, set: { if $0 != Self.none { choose($0) } })) {
            if !known {
                Text(chosen.isEmpty ? L10n[.Properties.frameTargetNone] : L10n[.Properties.frameTargetGone]).tag(Self.none)
            }
            ForEach(targets, id: \.id) { Text($0.path).tag($0.id) }
        }
        .labelsHidden().pickerStyle(.menu).fixedSize()
        .disabled(targets.isEmpty)
        .help(targets.isEmpty ? L10n[.Properties.frameTargetNoUi] : L10n[.Properties.frameTargetHelp])
    }

    /// The choice that stands for no section in the menu: never a section's id.
    private static let none = "#none"
}

extension AppModel {
    /// The ui sections a frame in `section` may hold: every ui section of the tree open but its own, by id and
    /// path, in the tree's order.
    func frameTargets(at section: String) -> [(id: String, path: String)] {
        guard let detail else { return [] }
        let directory = detail.directory
        let tree = directory.root(of: section)
        return ProjectSection.treeOrder(detail.sections)
            .filter { $0.kind == .ui && $0.id != section && directory.root(of: $0.id) == tree }
            .map { ($0.id, directory.full(section: $0.id)) }
    }

    /// The section the frame `id` holds, by id: none for a card that is no frame, or a frame whose section
    /// is gone.
    func frameTarget(of id: String) -> String? {
        guard let detail, let card = schema(id), card.kind == .frame,
              case .string(let target)? = card.faceValues["target"], detail.directory.section(id: target) != nil
        else { return nil }
        return target
    }

    /// A frame pointed at the ui section `section` (by id), written as its path. Refused — a section the frame
    /// would come back round to its own through — the core's reason is the notice.
    func setFrameTarget(_ id: String, _ section: String) {
        guard let detail, detail.directory.section(id: section) != nil else { return }
        setCustomField(id, "target", detail.directory.full(section: section))
    }

    /// A frame made in the section on screen, holding the ui section `target` (by id).
    @discardableResult
    func createFrame(name: String, label: String = "", target: String) -> SchemaNode? {
        guard let detail, detail.directory.section(id: target) != nil else { return nil }
        return createSchema(name: name, label: label, kindName: SchemaKind.frame.rawValue,
                            more: ["target": detail.directory.full(section: target)])
    }
}
