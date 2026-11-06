import SwiftUI

/// A spec opened — its card editor (`specboard://SpecBoard/Docs/Design:CardKinds`「仕様（spec）」): read as its document,
/// or written in place — its story above, then each scenario a box of its title and its steps, the keyword of each
/// chosen on its left, how many tests check it on the box's right, and a scenario added, moved and let go of.
struct SpecEditor: View {
    var model: AppModel
    var doc: SchemaNode
    var editable: Bool
    /// Follow a `[[path]]` written in it, from the card's section.
    var onOpenPath: (String, String?) -> Void

    @State private var writing: Bool
    @State private var deleting: String?

    init(model: AppModel, doc: SchemaNode, editable: Bool, writing: Bool = false,
         onOpenPath: @escaping (String, String?) -> Void) {
        self.model = model
        self.doc = doc
        self.editable = editable
        self.onOpenPath = onOpenPath
        _writing = State(initialValue: writing)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            header
            Divider()
            if writing { form } else { reading }
        }
        .confirmationDialog(L10n[.Common.actionDelete], isPresented: Binding(get: { deleting != nil }, set: { if !$0 { deleting = nil } })) {
            Button(L10n[.Common.actionDelete], role: .destructive) {
                if let name = deleting { model.deleteScenario(doc.id, name) }
                deleting = nil
            }
            Button(L10n[.Common.actionCancel], role: .cancel) { deleting = nil }
        }
    }

    private var header: some View {
        CardBar(kind: doc.kind, name: doc.name, label: doc.label) {
            EmptyView()
        } tools: {
            Picker("", selection: $writing) {
                BarLabel(title: L10n[.Docs.modeView], systemImage: "eye").tag(false)
                BarLabel(title: L10n[.Common.actionEdit], systemImage: "pencil").tag(true)
            }
            .labelsHidden().pickerStyle(.segmented).fixedSize()
            .disabled(!editable)
            CardTools(model: model, card: doc, editable: editable, writing: nil) {
                model.open(.card($0))
            }
        }
        .font(.app(11))
        .padding(.horizontal, 14).padding(.vertical, 8)
    }

    // MARK: - Read

    @ViewBuilder
    private var reading: some View {
        if let detail = model.detail {
            ScrollView {
                MarkdownView(text: SpecDocument.markdown(doc), size: 13,
                             refs: NoteEditor.refs(doc, detail, outside: model.isBoardOpen ? model.outsideDetail : nil, open: onOpenPath))
                    .padding(.horizontal, 24).padding(.vertical, 16)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
    }

    // MARK: - Written in place

    private var form: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 6) {
                Text(L10n[.Diagram.specStory].uppercased()).font(.app(10.5, weight: .semibold)).foregroundStyle(.secondary)
                storyField("As", "as", doc.specStory.as)
                storyField("I want", "iWant", doc.specStory.iWant)
                storyField("So that", "soThat", doc.specStory.soThat)
                storyField(L10n[.Diagram.specAbout], "about", doc.specStory.about)
                HStack {
                    Text(L10n[.Diagram.specScenarios]).font(.app(10.5, weight: .semibold)).foregroundStyle(.secondary)
                    Spacer()
                    Button(L10n[.Diagram.specAddScenario]) { model.addScenario(doc.id) }
                        .disabled(!editable)
                }
                .padding(.top, 10)
                ForEach(Array(doc.scenarios.enumerated()), id: \.element.name) { index, scenario in
                    ScenarioBox(model: model, doc: doc, scenario: scenario, index: index, editable: editable) {
                        deleting = scenario.name
                    }
                }
            }
            .padding(14)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private func storyField(_ title: String, _ key: String, _ value: String) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Text(title).font(.app(11)).foregroundStyle(.secondary).frame(width: 64, alignment: .trailing)
            CommitField("", text: value) { model.setSpecField(doc.id, key, $0) }
                .disabled(!editable)
        }
    }
}

/// One scenario in a spec's editor: its name, its title, how many tests check it, what may be done to it, and its
/// steps, each its keyword chosen and its words.
private struct ScenarioBox: View {
    var model: AppModel
    var doc: SchemaNode
    var scenario: SpecScenario
    var index: Int
    var editable: Bool
    var onDelete: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 6) {
                Text(scenario.name).font(.app(10.5, design: .monospaced)).foregroundStyle(.secondary)
                CommitField("", text: scenario.heading) {
                    model.updateScenario(doc.id, scenario.name, heading: $0, steps: nil)
                }
                .disabled(!editable)
                .font(.app(12, weight: .semibold))
                let tests = SpecRows.tests(doc, scenario.name)
                Text(SpecRows.said(tests: tests))
                    .font(.app(10, design: .monospaced))
                    .foregroundStyle(tests == 0 ? Color.orange : Color.teal)
                    .padding(.horizontal, 5).padding(.vertical, 1)
                    .overlay(RoundedRectangle(cornerRadius: 3).stroke(tests == 0 ? Color.orange : Color.teal, lineWidth: 1))
                Menu {
                    Button(L10n[.Common.actionMoveUp]) { model.moveScenario(doc.id, scenario.name, by: -1) }
                        .disabled(index == 0)
                    Button(L10n[.Common.actionMoveDown]) { model.moveScenario(doc.id, scenario.name, by: 1) }
                        .disabled(index >= doc.scenarios.count - 1)
                    Button(L10n[.Lines.actionConnectMenu]) { LineList.begin(model, source) }
                    Divider()
                    Button(L10n[.Common.actionDelete], role: .destructive, action: onDelete)
                } label: { Image(systemName: "ellipsis") }
                    .menuStyle(.borderlessButton).fixedSize()
                    .disabled(!editable)
            }
            ForEach(Array(scenario.steps.enumerated()), id: \.offset) { at, step in
                HStack(spacing: 6) {
                    Picker("", selection: Binding(get: { step.keyword }, set: { keyword in
                        var steps = scenario.steps
                        steps[at].keyword = keyword
                        model.updateScenario(doc.id, scenario.name, heading: nil, steps: steps)
                    })) {
                        ForEach(SpecStep.keywords, id: \.self) { Text(SpecStep(keyword: $0, text: "").word).tag($0) }
                    }
                    .labelsHidden().fixedSize()
                    .disabled(!editable)
                    CommitField("", text: step.text) { text in
                        var steps = scenario.steps
                        steps[at].text = text
                        model.updateScenario(doc.id, scenario.name, heading: nil, steps: steps)
                    }
                    .disabled(!editable)
                }
            }
            if editable {
                Button(L10n[.Diagram.specAddStep]) {
                    let keyword = scenario.steps.isEmpty ? "given" : "and"
                    model.updateScenario(doc.id, scenario.name, heading: nil,
                                         steps: scenario.steps + [SpecStep(keyword: keyword, text: "")])
                }
                .buttonStyle(.link).font(.app(11))
                .padding(.leading, 76)
            }
        }
        .padding(8)
        .background(RoundedRectangle(cornerRadius: 6).stroke(Color.secondary.opacity(0.3)))
        // The place for a line from it picked beside it, as from the lines' list.
        .popover(isPresented: Binding(
            get: { model.connecting?.anchor == nil && model.connecting?.source == source },
            set: { shown in if !shown, model.connecting?.anchor == nil { model.connecting = nil } }),
                 arrowEdge: .trailing) {
            if let connecting = model.connecting {
                ConnectView(model: model, source: connecting.source, target: connecting.target) {
                    model.connecting = nil
                }
                .id(connecting.id)
            }
        }
    }

    /// Where a line from it leaves: the scenario of the card.
    private var source: LineSource {
        LineSource(card: doc.id, path: model.detail?.directory.full(doc) ?? doc.name, from: scenario.name)
    }
}
