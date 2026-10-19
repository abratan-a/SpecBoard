import Foundation

/// What to write where (`specboard://SpecBoard/Docs/Design:Overview` §5「書く場所」): the places a project's free
/// text goes, the app's own rule for each, and the project's — kept in its Rules
/// section — over them.
///
/// A place is named broadly or narrowly — `card.description`, `card.action.description` —
/// and a rule is looked for from the narrowest name the project has set, then the
/// broader one, then the app's.
nonisolated enum WritingRules {

    /// The Rules section's name: one in main, where the project keeps its rules.
    static let sectionName = "Rules"

    struct Place: Hashable, Sendable {
        var name: String
        /// What a person sees for it.
        var label: String
        /// The broader place this one falls back to, if any.
        var broader: String?
    }

    /// The broader place each narrow place falls back to: a name apart from its label, so that looking a
    /// rule up does not make every label.
    private static let broaderOf: [String: String] = {
        var out = ["field.entity.description": "field.description", "field.value.description": "field.description"]
        for kind in SchemaKind.made { out["card.\(kind.rawValue).description"] = "card.description" }
        return out
    }()

    /// Every place, broad before narrow, in the order the Rules section lists them. Made when asked, in the
    /// screen's language.
    static var places: [Place] {
        let language = Language.current
        func place(_ name: String, _ label: LocalizedStringResource) -> Place {
            Place(name: name, label: L10n.text(label, in: language), broader: broaderOf[name])
        }
        var out = [
            place("project.description", .ProjectSettings.placeProjectDescription),
            place("section.description", .ProjectSettings.placeSectionDescription),
            place("card.description", .ProjectSettings.placeCardDescription),
        ]
        for kind in SchemaKind.made {
            out.append(place("card.\(kind.rawValue).description", .ProjectSettings.placeKindDescription(kind.rawValue)))
        }
        out += [
            place("field.description", .ProjectSettings.placeFieldDescription),
            place("field.entity.description", .ProjectSettings.placeEntityFieldDescription),
            place("field.value.description", .ProjectSettings.placeValueFieldDescription),
            place("step.description", .ProjectSettings.placeStepDescription),
            place("node.description", .ProjectSettings.placeNodeDescription),
            place("note.body", .ProjectSettings.placeNoteBody),
            place("document.body", .ProjectSettings.placeDocumentBody),
        ]
        return out
    }

    static var byName: [String: Place] { Dictionary(uniqueKeysWithValues: places.map { ($0.name, $0) }) }

    /// The app's own rules, for the broad places; a narrow place without its own falls
    /// back to the broad one's.
    static var defaults: [String: String] {
        let language = Language.current
        let rules: [(String, LocalizedStringResource)] = [
            ("project.description", .ProjectSettings.defaultProjectDescription),
            ("section.description", .ProjectSettings.defaultSectionDescription),
            ("card.description", .ProjectSettings.defaultCardDescription),
            ("field.description", .ProjectSettings.defaultFieldDescription),
            ("step.description", .ProjectSettings.defaultStepDescription),
            ("node.description", .ProjectSettings.defaultNodeDescription),
            ("note.body", .ProjectSettings.defaultNoteBody),
            ("document.body", .ProjectSettings.defaultDocumentBody),
        ]
        return Dictionary(uniqueKeysWithValues: rules.map { ($0.0, L10n.text($0.1, in: language)) })
    }

    /// Where a rule came from.
    enum Source: Equatable, Sendable {
        /// The project's, set at this place (the one asked about, or a broader one).
        case project(String)
        /// The app's, for this place.
        case app(String)
    }

    /// The rule for a place: the project's at the narrowest name it has set, else the app's.
    static func rule(_ place: String, project rules: [String: String]) -> (text: String, source: Source)? {
        var chain: [String] = []
        var at: String? = place
        while let name = at {
            chain.append(name)
            at = broaderOf[name]
        }
        for name in chain {
            if let text = rules[name], !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                return (text, .project(name))
            }
        }
        let defaults = Self.defaults
        for name in chain {
            if let text = defaults[name] { return (text, .app(name)) }
        }
        return nil
    }

    /// What to write at a place, said for a person where it is written (the property
    /// panel's foot): the rule, and whether it is the project's or the app's.
    static func helpText(_ place: String, project rules: [String: String]) -> String {
        guard let found = rule(place, project: rules) else { return "" }
        switch found.source {
        case .project(let at): return L10n.text(.ProjectSettings.helpProject(at, found.text), in: Language.current)
        case .app(let at): return L10n.text(.ProjectSettings.helpApp(at, found.text), in: Language.current)
        }
    }

    /// The place a card's description is.
    static func cardPlace(_ kind: SchemaKind) -> String { "card.\(kind.rawValue).description" }

    /// The place a field's description is, in a card of this kind.
    static func fieldPlace(_ kind: SchemaKind) -> String {
        kind == .entity || kind == .value ? "field.\(kind.rawValue).description" : "field.description"
    }

    /// Why a set of rules cannot be kept, or nil: each place one this app knows.
    static func problem(_ rules: [String: String]) -> String? {
        let language = Language.current
        let known = places.map(\.name)
        let unknown = rules.keys.filter { !known.contains($0) }.sorted()
        if !unknown.isEmpty {
            return L10n.text(
                .ProjectSettings.errorUnknownPlace(
                    unknown.joined(separator: L10n.text(.Common.separatorList, in: language)),
                    known.joined(separator: " / ")),
                in: language)
        }
        if let long = rules.first(where: { $0.value.count > 4000 }) {
            return L10n.text(.ProjectSettings.errorRuleTooLong(long.key), in: language)
        }
        return nil
    }
}
