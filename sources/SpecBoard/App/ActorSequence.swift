import SwiftUI

/// A section on the map tab (`specboard://SpecBoard/Docs/Design:UI`「アクターのシーケンス」): its map — and, for an
/// actor's section, a `Map | Sequence` switch at the end of the map's band, the choice kept for each actor on
/// this Mac. Sequence shows the actor's chart (`ActorSequence`).
struct SectionScreen<Map: View>: View {
    var model: AppModel
    var detail: ProjectDetail
    /// The actor's section on screen; nil for any other section.
    var actor: ProjectSection?
    /// The section's map, given the switch to put in its band (none for a section that is no actor).
    @ViewBuilder var map: (AnyView?) -> Map

    /// Counts the switch pressed, so that the view reads the choice again.
    @State private var switched = 0

    var body: some View {
        let _ = switched
        if let actor, ActorView.showsSequence(project: detail.project.id, actor: actor.id) {
            ActorSequence(model: model, actor: actor, detail: detail, switcher: AnyView(switcher(actor)))
        } else {
            map(actor.map { AnyView(switcher($0)) })
        }
    }

    private func switcher(_ actor: ProjectSection) -> some View {
        Picker("", selection: Binding(
            get: { ActorView.showsSequence(project: detail.project.id, actor: actor.id) },
            set: {
                ActorView.setShowsSequence($0, project: detail.project.id, actor: actor.id)
                switched &+= 1
            })) {
            Text(L10n[.Diagram.actorMap]).tag(false)
            Text(L10n[.Diagram.actorSequence]).tag(true)
        }
        .labelsHidden().pickerStyle(.segmented).fixedSize()
        .controlSize(.small).font(.app(10.5))
        .help(L10n[.Diagram.actorSwitchHelp])
    }
}

extension ActorView {
    /// Whether a section's map offers `Map | Sequence`: an actor's alone.
    static func offersSwitch(_ section: ProjectSection) -> Bool { section.isActor }
}

/// An actor's chart (`specboard://SpecBoard/Docs/Design:Overview` §11, the core's `sequence_chart` with `section`):
/// one named frame for each flow it provides, all open until one is folded to one line, a press on a
/// frame's head folding or opening it, a double-click on it or its ↗ opening its flow; the lifelines' ⌃ and ⌄ and
/// the calls' ⌄ and ⌃ as on a flow's. The folded frames are kept for each actor on this Mac.
struct ActorSequence: View {
    var model: AppModel
    var actor: ProjectSection
    var detail: ProjectDetail
    var switcher: AnyView

    @State private var controller = SequenceController()
    /// Counts the chips pressed, so that the chart is drawn again from what is kept.
    @State private var pressed = 0

    var body: some View {
        let _ = pressed
        let diagram = Self.drawn(model: model, actor: actor.id)
        VStack(spacing: 0) {
            HStack(spacing: 10) {
                Text(L10n[.Diagram.actorSequenceHint]).font(.app(10.5)).foregroundStyle(.secondary).lineLimit(1)
                Spacer(minLength: 0)
                switcher
            }
            .padding(.horizontal, 14).padding(.vertical, 6)
            Divider()
            SequenceCanvas(diagram: diagram, detail: detail, selected: model.sequenceSelection, controller: controller,
                           folded: Self.folded(diagram, project: detail.project.id, actor: actor.id),
                           onPick: { model.pickSequence($0) },
                           onOpen: { model.open(.card($0)) },
                           onOpenSection: { model.open(.section($0)) },
                           onPress: { chip in
                               Self.pressed(chip, model: model, actor: actor.id)
                               pressed &+= 1
                           })
        }
        // A flow pressed in the nav panel: its frame, opened, brought under the heads once drawn.
        .onChange(of: model.sequenceFocus) { _, flow in
            guard let flow else { return }
            controller.reveal(flow: flow)
            model.sequenceFocus = nil
        }
    }

    /// The actor's chart as the core builds it, with the actors chosen for it on this Mac and the calls opened
    /// on it.
    static func drawn(model: AppModel, actor: String) -> SequenceDiagram {
        let project = model.detail?.project.id
        let chosen = project.flatMap { SequenceLifelines.kept(project: $0, of: actor) }
        let opened = project.flatMap { SequenceCalls.kept(project: $0, of: actor) }
        return model.sequenceChart(section: actor, lifelines: chosen, opened: opened)
    }

    /// The provided flows' frames, by the flow's id, in the chart's order.
    static func frames(_ diagram: SequenceDiagram) -> [String] {
        diagram.items.compactMap { item in
            if case .block(let block) = item, block.kind == .flow { return block.flow }
            return nil
        }
    }

    /// The frames drawn folded: the ones kept folded; the rest are open.
    static func folded(_ diagram: SequenceDiagram, project: String, actor: String) -> Set<String> {
        ActorView.folded(frames(diagram), project: project, actor: actor)
    }

    /// A chip pressed on the actor's chart: a lifeline folded or opened, a frame folded or opened, a call
    /// opened or folded — kept on this Mac —, or a frame's or a called flow opened.
    static func pressed(_ chip: SequenceLayout.Chip, model: AppModel, actor: String) {
        guard let detail = model.detail else { return }
        let project = detail.project.id
        switch chip {
        case .lifeline(let press): SequenceLifelines.press(press, project: project, of: actor, in: detail)
        case .foldFrame(let flow): ActorView.setOpen(false, flow: flow, project: project, actor: actor)
        case .openFrame(let flow): ActorView.setOpen(true, flow: flow, project: project, actor: actor)
        case .openFlow(let flow): model.open(.card(flow))
        case .openCall(let call): SequenceCalls.setOpen(true, call: call, project: project, of: actor)
        case .foldCall(let call): SequenceCalls.setOpen(false, call: call, project: project, of: actor)
        }
    }
}
