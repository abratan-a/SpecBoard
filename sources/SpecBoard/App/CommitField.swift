import SwiftUI

/// A text field that writes back when you finish, not on every keystroke.
///
/// Every commit here is a database write plus a reload of the project, and a rename is
/// validated — typing `User` one letter at a time would reject `U`, `Us`, `Use` on
/// the way. So the draft is local until Enter or focus leaves — or the field leaves the
/// screen still focused (another tab of the settings chosen: a tab's control takes no focus,
/// so focus never left, and what was typed went with the field).
struct CommitField: View {
    var placeholder: String
    var text: String
    var monospaced: Bool = false
    /// Hands on what was written; false when it was refused then and there, and the field shows the value
    /// it has again.
    var onCommit: (String) -> Bool

    @State private var draft: String
    @FocusState private var focused: Bool
    /// What was handed on last, until the value comes back: not handed on twice.
    @State private var said: String?

    init(_ placeholder: String, text: String, monospaced: Bool = false,
         onCommit: @escaping (String) -> Void) {
        self.placeholder = placeholder
        self.text = text
        self.monospaced = monospaced
        self.onCommit = { onCommit($0); return true }
        _draft = State(initialValue: text)
    }

    /// One whose writing may be refused at once (a name another already has): refused, `accept` gives false
    /// and the field goes back to `text` — which then does not change, so would not bring it back — rather than
    /// keep showing what was not written.
    static func refusable(_ placeholder: String, text: String, monospaced: Bool = false,
                          accept: @escaping (String) -> Bool) -> CommitField {
        var field = CommitField(placeholder, text: text, monospaced: monospaced) { _ in }
        field.onCommit = accept
        return field
    }

    var body: some View {
        TextField(placeholder, text: $draft)
            .textFieldStyle(.roundedBorder)
            .font(.app(12, design: monospaced ? .monospaced : .default))
            .focused($focused)
            .onSubmit(commit)
            .onChange(of: focused) { _, nowFocused in if !nowFocused { commit() } }
            // A rejected edit leaves the stored value unchanged; the draft has to
            // follow it back or the field keeps showing something that is not true. What
            // comes back for what was handed on is taken while still focused too: the core
            // may have made it its own way (a slug, lowercased and dashed).
            .onChange(of: text) { _, updated in
                let answered = said != nil
                said = nil
                if !focused || answered { draft = updated }
            }
            .onDisappear(perform: commit)
    }

    private func commit() {
        let trimmed = draft.trimmingCharacters(in: .whitespacesAndNewlines)
        guard trimmed != text, trimmed != said else { return }
        said = trimmed
        if !onCommit(trimmed) {
            said = nil
            draft = text
        }
    }
}

/// Multi-line variant for descriptions and notes.
struct CommitTextEditor: View {
    var text: String
    var minHeight: CGFloat = 64
    /// Shown faintly while it is empty: what to write here (`WritingRules`).
    var placeholder: String = ""
    /// What is written so far, as it is typed — for a preview beside it.
    var onDraft: ((String) -> Void)?
    var onCommit: (String) -> Void

    @State private var draft: String
    @FocusState private var focused: Bool
    /// What was handed on last, until the value comes back: not handed on twice.
    @State private var said: String?

    init(text: String, minHeight: CGFloat = 64, placeholder: String = "",
         onDraft: ((String) -> Void)? = nil, onCommit: @escaping (String) -> Void) {
        self.text = text
        self.minHeight = minHeight
        self.placeholder = placeholder
        self.onDraft = onDraft
        self.onCommit = onCommit
        _draft = State(initialValue: text)
    }

    var body: some View {
        TextEditor(text: $draft)
            .font(.app(12))
            .frame(minHeight: minHeight)
            .scrollContentBackground(.hidden)
            .padding(4)
            .background(alignment: .topLeading) {
                if draft.isEmpty && !placeholder.isEmpty {
                    Text(placeholder)
                        .font(.app(11.5))
                        .foregroundStyle(.tertiary)
                        .padding(.horizontal, 9).padding(.vertical, 4)
                        .allowsHitTesting(false)
                }
            }
            .overlay(RoundedRectangle(cornerRadius: 5)
                .stroke(Color.secondary.opacity(0.35), lineWidth: 1))
            .focused($focused)
            .onChange(of: focused) { _, nowFocused in if !nowFocused { commit() } }
            .onChange(of: text) { _, updated in
                said = nil
                if !focused { draft = updated }
            }
            .onChange(of: draft) { _, now in onDraft?(now) }
            // Gone from the screen still focused (another tab chosen): what was typed is kept.
            .onDisappear(perform: commit)
    }

    private func commit() {
        guard draft != text, draft != said else { return }
        said = draft
        onCommit(draft)
    }
}
