import Foundation
import Synchronization

/// The words the screen says the built-in kinds with — sections, cards, their fields and rows, lines —
/// as their definitions say them in the screen's language, the way the core reads them
/// (`specboard://SpecBoard/Docs/Design:Localization` §4): the screen keeps no copy of its own, so it says
/// what MCP and an export say, and a definition changed is a word changed.
///
/// Taken from each project read (`WholeKinds`): the built-in definitions are the same in every project,
/// so what draws without a project at hand (a card's rows, a node's ways) reads them here. Before any
/// is read, a kind is said by its name.
nonisolated enum BuiltinWords {
    private struct Words: Sendable {
        var sections: [String: (label: String, meaning: String)] = [:]
        var cards: [String: CardKindDef] = [:]
        var lines: [String: LineKind] = [:]
    }

    private static let kept = Mutex(Words())

    /// The built-in kinds of a project read, in the language it was read in.
    static func take(_ kinds: WholeKinds) {
        var words = Words()
        for (kind, label) in kinds.labels where kinds.builtinSections.contains(kind) {
            words.sections[kind] = (label, kinds.meanings[kind] ?? "")
        }
        for card in kinds.cards where card.builtin { words.cards[card.kind] = card }
        for line in kinds.lines where line.builtin { words.lines[line.line] = line }
        kept.withLock { $0 = words }
    }

    /// What a kind of section is called.
    static func section(_ kind: String) -> String { kept.withLock { $0.sections[kind]?.label } ?? kind }

    /// What a kind of section holds, in a line.
    static func sectionMeaning(_ kind: String) -> String { kept.withLock { $0.sections[kind]?.meaning } ?? "" }

    /// What a kind of card (a flow's node too) is called.
    static func label(_ kind: String) -> String { kept.withLock { $0.cards[kind]?.label } ?? kind }

    /// What a kind of card is for, in its first sentence.
    static func purpose(_ kind: String) -> String {
        let said = kept.withLock { $0.cards[kind]?.purpose } ?? ""
        // A sentence ends at a full stop, in either language's way of writing one.
        guard let end = said.firstIndex(where: { $0 == "\u{3002}" || $0 == "." }) else { return said }
        return String(said[..<end])
    }

    /// A kind of card as its badge says it: the kind, with its label.
    static func badge(_ kind: String) -> String { kept.withLock { $0.cards[kind]?.badge } ?? kind }

    /// What a kind's field is called, and what it means.
    static func field(_ kind: String, _ name: String) -> String { kindField(kind, name)?.label ?? name }
    static func fieldMeaning(_ kind: String, _ name: String) -> String { kindField(kind, name)?.description ?? "" }

    /// What a field of a kind's row is called.
    static func rowField(_ kind: String, _ row: String, _ name: String) -> String {
        kept.withLock { $0.cards[kind]?.rows.first { $0.name == row }?.fields.first { $0.name == name }?.label } ?? name
    }

    /// What a setting of a record's field is called (the row `fields` of entity and value alike).
    static func recordField(_ name: String) -> String { rowField("entity", "fields", name) }

    /// What a kind of line is called — a flow's way out too (`next`, `onError`, `else`…).
    static func line(_ name: String) -> String { kept.withLock { $0.lines[name]?.label } ?? name }

    private static func kindField(_ kind: String, _ name: String) -> KindField? {
        kept.withLock { $0.cards[kind]?.field(name) }
    }
}
