import SwiftUI

/// A name — or a reason — asked for: making or renaming a section, a
/// branch or an export directory, a branch from the nav panel's branch list, a merge request's rejection,
/// a commit's name. Asked in a sheet of its own (`NameSheet`), not an alert: an alert with a
/// field in it opened with the focus on its buttons, so what was typed went nowhere.
struct NameAsk: Identifiable {
    let id = UUID()
    var title: String
    /// Said under the title; nothing when empty.
    var message = ""
    /// What the field shows while it is empty.
    var placeholder: String
    /// What is in the field when it opens: the name now, when renaming.
    var start = ""
    /// The button that decides.
    var confirm: String
    /// The button that gives up.
    var cancel: String
    /// Whether an empty field may be decided: only where nothing means something — a
    /// commit's name taken off.
    var allowsEmpty = false
    /// Whether what is asked for is a name a path is made of (a section's, a card ghost's): held
    /// to what the core holds such a name to (`PathText.nameProblem`), so a name it would refuse
    /// cannot be decided.
    var pathName = false
    /// What deciding does, with what was typed, trimmed.
    var onDecide: (String) -> Void
    /// Where what deciding writes may be refused (a name another already has): why, said under the field
    /// with the sheet left open, or nil when it went in. Used in place of `onDecide`.
    var refusing: ((String) -> String?)? = nil

    /// What a decision passes on: what was typed, without the spaces around it.
    static func trimmed(_ text: String) -> String { text.trimmingCharacters(in: .whitespacesAndNewlines) }

    /// Whether the deciding button can be pressed with this typed.
    func canDecide(_ text: String) -> Bool {
        if pathName { return PathText.nameProblem(PathText.trimmed(text)) == nil }
        return allowsEmpty || !Self.trimmed(text).isEmpty
    }

    /// Decided with this typed: what it does is called with it trimmed — unless it cannot be
    /// decided, when nothing is. Whether it was.
    @discardableResult
    func decide(_ text: String) -> Bool {
        guard canDecide(text) else { return false }
        onDecide(pathName ? PathText.trimmed(text) : Self.trimmed(text))
        return true
    }

    /// Decided with this typed where it may be refused: why it was, or nil when it went in (or nothing could be
    /// decided).
    func decideSaying(_ text: String) -> String? {
        guard canDecide(text) else { return nil }
        let typed = pathName ? PathText.trimmed(text) : Self.trimmed(text)
        if let refusing { return refusing(typed) }
        onDecide(typed)
        return nil
    }
}

/// The sheet a name is asked for in (`NameAsk`): the field focused when it opens, Return
/// decides, Esc gives up, and the deciding button cannot be pressed while there is nothing
/// to decide. Made like the sheet a card is added from (`ContentView.newSchemaPrompt`).
struct NameSheet: View {
    var ask: NameAsk
    /// Shuts the sheet: after deciding, and on giving up.
    var close: () -> Void

    @State private var text: String
    @FocusState private var focused: Bool
    /// Decided once: Return and the button are one decision.
    @State private var decided = false
    /// Why what was typed was refused, said under the field until it is changed.
    @State private var refused: String?

    init(ask: NameAsk, close: @escaping () -> Void) {
        self.ask = ask
        self.close = close
        _text = State(initialValue: ask.start)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(ask.title).font(.app(13, weight: .semibold))
            if !ask.message.isEmpty {
                Text(ask.message)
                    .font(.app(11)).foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            TextField(ask.placeholder, text: $text)
                .textFieldStyle(.roundedBorder)
                .font(.app(12))
                .focused($focused)
                .onChange(of: text) { refused = nil }
            if let refused {
                Text(refused).font(.app(10.5)).foregroundStyle(.red)
                    .fixedSize(horizontal: false, vertical: true)
            }
            HStack {
                Spacer()
                Button(ask.cancel, action: close)
                    .keyboardShortcut(.cancelAction)
                Button(ask.confirm, action: decide)
                    .keyboardShortcut(.defaultAction)
                    .disabled(!ask.canDecide(text))
            }
        }
        .padding(18)
        .frame(width: 360, alignment: .leading)
        .defaultFocus($focused, true)
        // The sheet's window takes the focus as it opens; asked again once it has.
        .onAppear { DispatchQueue.main.async { focused = true } }
    }

    private func decide() {
        guard !decided, ask.canDecide(text) else { return }
        if let said = ask.decideSaying(text) {
            refused = said
            return
        }
        decided = true
        close()
    }
}

extension View {
    /// A name asked for in a sheet while `ask` holds one (`NameSheet`), let go when it is
    /// decided or given up.
    func nameSheet(_ ask: Binding<NameAsk?>) -> some View {
        sheet(item: ask) { asked in
            NameSheet(ask: asked) { ask.wrappedValue = nil }
        }
    }
}
