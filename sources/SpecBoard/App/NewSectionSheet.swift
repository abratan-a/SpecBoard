import SwiftUI

/// A section about to be made inside another, from the nav panel's menu (`NavPanelTree.newSections`):
/// the kind picked there (an actor's figure with it), and the mark picked in the window — written with
/// its name in one `section_create`, so making it is one step in the history.
struct NewSectionAsk: Identifiable {
    let id = UUID()
    var parentId: String
    var choice: NavPanelTree.NewSection
    /// What the window says at its top.
    var title: String
    /// The mark the section takes for what it is not given: its kind's, an actor's by its figure.
    var kindsMark: SectionMark
    /// What "Kind's mark" names: the kind (and figure) whose mark it is.
    var kindsMarkWord: String
    /// The mark picked so far; nothing picked writes no mark.
    var mark = SectionMark()
    /// What deciding does: the section made with the name typed and the mark picked.
    var onDecide: (String, SectionMark) -> Void

    /// Asked for `choice` inside `parent`, made through `model`. `single`: the only kind the menu offers,
    /// the parent's own, made as the core makes one with no kind asked for (no `type`).
    init(_ choice: NavPanelTree.NewSection, in parent: ProjectSection, model: AppModel, single: Bool) {
        parentId = parent.id
        self.choice = choice
        title = L10n[.Nav.promptCreateIn(parent.name, choice.kind.isEmpty ? L10n[.Nav.promptSectionWord] : choice.title)]
        let detail = model.detail
        kindsMark = detail?.kindMark(choice.kind, figure: choice.figure) ?? SectionMark()
        kindsMarkWord = choice.figure.map(KindsEditing.figureWord)
            ?? SectionKind(rawValue: choice.kind)?.label ?? detail?.sectionLabels[choice.kind] ?? choice.kind
        let type = single || choice.kind.isEmpty ? nil : choice.kind
        let figure = choice.figure
        let parentId = parent.id
        onDecide = { [weak model] name, mark in
            model?.createSection(name: name, parentId: parentId, type: type, figure: figure,
                                 icon: mark.icon == nil && mark.color == nil ? nil : mark)
        }
    }

    /// Whether the name typed may be decided: held to what the core holds a section's name to.
    func canDecide(_ text: String) -> Bool { PathText.nameProblem(PathText.trimmed(text)) == nil }

    /// Decided with this typed and the mark picked: whether it was.
    @discardableResult
    func decide(_ text: String) -> Bool {
        guard canDecide(text) else { return false }
        onDecide(PathText.trimmed(text), mark)
        return true
    }
}

/// The window a new section is named in (`NewSectionAsk`): its name, and its mark — the kind's to
/// start with, picked from the kinds' marks and the marks a kind may take, with a colour.
struct NewSectionSheet: View {
    var ask: NewSectionAsk
    var close: () -> Void

    @State private var text = ""
    @State private var mark = SectionMark()
    @FocusState private var focused: Bool
    @State private var decided = false

    init(ask: NewSectionAsk, close: @escaping () -> Void) {
        self.ask = ask
        self.close = close
        _mark = State(initialValue: ask.mark)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(ask.title).font(.app(13, weight: .semibold))
            SwiftUI.Grid(alignment: .leading, horizontalSpacing: 10, verticalSpacing: 8) {
                GridRow {
                    Text(L10n[.Common.wordName]).font(.app(11)).foregroundStyle(.secondary)
                    TextField(L10n[.Common.wordName], text: $text)
                        .textFieldStyle(.roundedBorder)
                        .font(.app(12))
                        .focused($focused)
                }
                GridRow {
                    Text(L10n[.Nav.promptMark]).font(.app(11)).foregroundStyle(.secondary)
                    KindMarkPicker(mark: mark, defaultMark: ask.kindsMark, label: "", kindsMarkWord: ask.kindsMarkWord,
                                   editable: true) { mark = $0 }
                }
            }
            HStack {
                Spacer()
                Button(L10n[.Common.actionCancelDialog], action: close)
                    .keyboardShortcut(.cancelAction)
                Button("OK", action: decide)
                    .keyboardShortcut(.defaultAction)
                    .disabled(!ask.canDecide(text))
            }
        }
        .padding(18)
        .frame(width: 380, alignment: .leading)
        .defaultFocus($focused, true)
        .onAppear { DispatchQueue.main.async { focused = true } }
    }

    private func decide() {
        var asked = ask
        asked.mark = mark
        guard !decided, asked.decide(text) else { return }
        decided = true
        close()
    }
}
