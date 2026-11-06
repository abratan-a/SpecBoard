import Foundation

/// A spec's story (`specboard://SpecBoard/Docs/Design:CardKinds`「仕様（spec）」): whose it is, what they want and why,
/// and what is written after it.
nonisolated struct SpecStory: Equatable, Hashable, Sendable {
    var `as` = ""
    var iWant = ""
    var soThat = ""
    var about = ""
}

/// One step of a scenario: its keyword (`given`, `when`, `then`, `and`, `but`) and its words.
nonisolated struct SpecStep: Equatable, Hashable, Sendable {
    var keyword: String
    var text: String
}

/// A scenario of a spec: its name for paths, its title, its steps and what is written after them.
nonisolated struct SpecScenario: Equatable, Hashable, Sendable {
    var id: String = ""
    var name: String
    var heading: String = ""
    var steps: [SpecStep] = []
    var note: String = ""
}

extension SpecStory {
    /// The story as the core keeps it (`form.fields`).
    static func read(_ fields: [String: JSONValue]) -> SpecStory {
        func text(_ key: String) -> String {
            if case .string(let value)? = fields[key] { return value }
            return ""
        }
        return SpecStory(as: text("as"), iWant: text("iWant"), soThat: text("soThat"), about: text("about"))
    }
}

extension SpecStep {
    /// The keywords a step may have, in the order they are told.
    static let keywords = ["given", "when", "then", "and", "but"]

    /// The keyword as Gherkin writes it.
    var word: String { keyword.prefix(1).uppercased() + keyword.dropFirst() }

    /// As MCP writes it: its keyword its one key.
    var wire: [String: Any] { [keyword: text] }
}

extension SpecScenario {
    /// The scenarios as the core keeps them (`form.rows.scenarios`: each its `id`, `fields`, and its
    /// steps in `rows.steps`).
    static func read(_ rows: JSONValue?) -> [SpecScenario] {
        guard case .array(let list)? = rows else { return [] }
        return list.compactMap { item in
            guard case .object(let row) = item, case .object(let fields)? = row["fields"] else { return nil }
            func text(_ key: String) -> String {
                if case .string(let value)? = fields[key] { return value }
                return ""
            }
            var steps: [SpecStep] = []
            if case .object(let inner)? = row["rows"], case .array(let given)? = inner["steps"] {
                for step in given {
                    guard case .object(let s) = step, case .object(let f)? = s["fields"] else { continue }
                    for keyword in SpecStep.keywords {
                        if case .string(let said)? = f[keyword] {
                            steps.append(SpecStep(keyword: keyword, text: said))
                            break
                        }
                    }
                }
            }
            var id = ""
            if case .string(let given)? = row["id"] { id = given }
            return SpecScenario(id: id, name: text("name"), heading: text("heading"), steps: steps, note: text("note"))
        }
    }

    /// Its title as it is shown: its name when none is written.
    var shownHeading: String {
        heading.trimmingCharacters(in: .whitespaces).isEmpty ? name : heading
    }

    /// A name for a scenario added last: `scenario` and the next number free among `taken`.
    static func name(taken: Set<String>) -> String {
        var n = taken.count + 1
        while taken.contains("scenario\(n)") { n += 1 }
        return "scenario\(n)"
    }
}

/// A spec as its document reads (the core's Markdown of it, `export/documents.rs`): its title, its story under the
/// heading in the screen's language, what follows it, then each scenario with its steps, marked (`RowMarks`) with
/// how many tests check it — the page draws that beside its heading; what is written out has none.
enum SpecDocument {
    static func markdown(_ card: SchemaNode) -> String {
        // What is written is someone's text: it brings no mark of its own (`RowMarks.clean`).
        let clean = RowMarks.clean
        let title = clean(card.label.isEmpty ? card.name : card.label)
        var parts = ["# Feature: \(title)"]
        let story = card.specStory
        let lines = [("As", story.as), ("I want", story.iWant), ("So that", story.soThat)]
            .filter { !$0.1.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }
            .map { "- **\($0.0)** \(clean($0.1))" }
        if !lines.isEmpty { parts.append("## \(L10n[.Diagram.specStory])\n\n" + lines.joined(separator: "\n")) }
        if !story.about.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty { parts.append(clean(story.about)) }
        for scenario in card.scenarios {
            // Marked (`RowMarks`): drawn with how many tests check it beside its heading.
            let tests = SpecRows.tests(card, scenario.name)
            let mark = RowMarks.Mark(kind: tests == 0 ? .untested : .tested, depth: 0, row: scenario.name,
                                     path: scenario.name, heading: SpecRows.said(tests: tests))
            parts.append(RowMarks.line(mark) + "\n## Scenario: \(clean(scenario.shownHeading))")
            if !scenario.steps.isEmpty {
                parts.append(scenario.steps.map { "- **\($0.word)** \(clean($0.text))" }.joined(separator: "\n"))
            }
            if !scenario.note.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty { parts.append(clean(scenario.note)) }
        }
        return parts.joined(separator: "\n\n")
    }
}
