import Foundation

/// What is typed into a field that adds one thing — a field, a value, a row, a tag — and why it was refused:
/// said under the field until what is typed changes, as the sheets that add say theirs, rather than only in
/// the bubble at the bell, which goes in a moment while the field still holds what was refused.
struct AdderDraft: Equatable {
    var text = "" {
        didSet { if text != oldValue { refused = nil } }
    }
    private(set) var refused: String?

    /// What is typed, without the spaces around it; nil while nothing is.
    var typed: String? {
        let trimmed = text.trimmingCharacters(in: .whitespaces)
        return trimmed.isEmpty ? nil : trimmed
    }

    /// `write` what is typed: gone in, the field is emptied; refused, why is kept here and taken from the
    /// bubble, so that it is said once, where it was typed.
    @MainActor mutating func add(in model: AppModel, _ write: (String) -> Void) {
        guard let typed else { return }
        write(typed)
        if let said = model.lastError {
            refused = said
            model.lastError = nil
        } else {
            text = ""
        }
    }
}
