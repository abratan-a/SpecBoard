import Foundation

/// Rejections the user is meant to read, ported from `src/server/lib/validate.ts`.
///
/// The messages are the point. A rename that fails has to say *why*, in the words the core
/// refuses it with.
struct Invalid: LocalizedError {
    var errorDescription: String?
    init(_ message: String) { errorDescription = message }
}

enum Validate {

    /// A name a path is made of — a card's, a field's, a row's, a node's — as the core holds it
    /// (`specboard://SpecBoard/Docs/Design:DesignSections` 「名前に使える字と、パスの中での囲み」): trimmed,
    /// 1 to 255 characters, no control character. Any other character is the name's own; a path
    /// quotes the name where it needs it (`PathText`).
    static func name(_ raw: String, field: String) throws -> String {
        let value = PathText.trimmed(raw)
        switch PathText.nameProblem(value) {
        case nil: return value
        case .empty?: throw Invalid(L10n[.Properties.validateEnter(field)])
        case .tooLong?: throw Invalid(L10n[.Properties.validateTooLong(field, PathText.nameMax)])
        case .control?, .spaces?: throw Invalid(L10n[.Properties.validateNameControl(field)])
        }
    }

    static func text(_ raw: String, field: String, max: Int = 400) throws -> String {
        let value = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        guard value.count <= max else { throw Invalid(L10n[.Properties.validateTooLong(field, max)]) }
        return value
    }

    static func required(_ raw: String, field: String, max: Int = 120) throws -> String {
        let value = try text(raw, field: field, max: max)
        guard !value.isEmpty else { throw Invalid(L10n[.Properties.validateEnter(field)]) }
        return value
    }

    static func tags(_ raw: [String], field: String? = nil) throws -> [String] {
        let field = field ?? L10n[.Common.wordTags]
        var out: [String] = []
        for tag in raw {
            let value = tag.trimmingCharacters(in: .whitespacesAndNewlines)
            if value.isEmpty { continue }
            guard value.count <= tagMaxLength else {
                throw Invalid(L10n[.Properties.validateTagTooLong(field, tagMaxLength, value)])
            }
            // Tag lists travel through the diagram spec comma-separated, which is the
            // only reason a comma is refused. Everything else is fine, Japanese included.
            guard !value.contains(",") else { throw Invalid(L10n[.Properties.validateTagComma(value)]) }
            if !out.contains(value) { out.append(value) }
        }
        guard out.count <= tagMaxCount else {
            throw Invalid(L10n[.Properties.validateTagTooMany(field, tagMaxCount, out.count)])
        }
        return out
    }

    /// A fixed value the field always carries. A `nil` clears it, since a constant of
    /// null says nothing that `nullable` does not already say.
    static func constValue(_ raw: JSONValue?, fieldType: FieldType) throws -> JSONValue? {
        guard let raw, !raw.isNull else { return nil }

        if fieldType.isObject || fieldType.isRef {
            throw Invalid(L10n[.Properties.validateConstObjectRef(fieldType.rawValue)])
        }
        let numeric: Set<String> = ["int", "float", "decimal"]
        let type = fieldType.rawValue

        switch raw {
        case .array, .object:
            throw Invalid(L10n[.Properties.validateConstScalar])
        case .number(let n):
            guard numeric.contains(type) else {
                throw Invalid(L10n[.Properties.validateConstNumber(type)])
            }
            if type == "int" && n != n.rounded() {
                throw Invalid(L10n[.Properties.validateConstInt])
            }
        case .bool:
            guard type == "bool" else {
                throw Invalid(L10n[.Properties.validateConstBool(type)])
            }
        case .string:
            guard !numeric.contains(type), type != "bool" else {
                throw Invalid(L10n[.Properties.validateConstString(type)])
            }
        case .null:
            return nil
        }
        return raw
    }
}

// MARK: - The structured extras

extension Validate {

    /// Rules are half-open: known kinds must carry their documented params, unknown
    /// kinds pass through untouched so a rule can be recorded before any exporter
    /// understands it. Expression params are never parsed — only stored.
    static func rules(_ raw: [Rule]) throws -> [Rule] {
        guard raw.count <= 64 else { throw Invalid(L10n[.Properties.validateRulesTooMany]) }

        for rule in raw {
            let kind = rule.kind.trimmingCharacters(in: .whitespaces)
            guard !kind.isEmpty else { throw Invalid(L10n[.Properties.validateRuleNeedsKind]) }
            guard let known = knownRules[kind] else { continue }

            for needed in known.params {
                let value = rule.params[needed]
                let missing = value == nil || value!.isNull
                    || (value!.stringValue?.isEmpty ?? false)
                if missing {
                    throw Invalid(L10n[.Properties.validateRuleNeeds(
                        kind, known.params.joined(separator: L10n[.Properties.validateRuleParamSeparator]), known.note)])
                }
            }
            if kind == "monotonicPerKey", case .array? = rule.params["keys"] {} 
            else if kind == "monotonicPerKey" {
                throw Invalid(L10n[.Properties.validateMonotonicKeys])
            }
            if kind == "sortedBy", let order = rule.string("order"),
               order != "asc", order != "desc" {
                throw Invalid(L10n[.Properties.validateSortedByOrder])
            }
            if kind == "pattern", let regex = rule.string("regex") {
                guard (try? NSRegularExpression(pattern: regex)) != nil else {
                    throw Invalid(L10n[.Properties.validatePatternRegex(regex)])
                }
            }
        }
        return raw
    }

    /// A name here is not an identifier.
    ///
    /// Checking it as one would be wrong twice over: the project this came from asks
    /// only for a non-empty string, and the field being described does not exist —
    /// that is the whole point of the entry — so there is nothing for an identifier
    /// rule to protect. Twenty-nine entries across five schemas name what is absent
    /// in Japanese, and such a check would make them impossible to write back after
    /// reading them out.
    static func absentFields(_ raw: [AbsentField]) throws -> [AbsentField] {
        for field in raw {
            guard !field.name.trimmingCharacters(in: .whitespaces).isEmpty else {
                throw Invalid(L10n[.Properties.validateAbsentNeedsName])
            }
            // The reason is the whole point of recording the absence.
            guard !field.reason.trimmingCharacters(in: .whitespaces).isEmpty else {
                throw Invalid(L10n[.Properties.validateAbsentNeedsReason(field.name)])
            }
        }
        return raw
    }

    /// Paths an action names — its keys, its notes: each something, none twice.
    static func paths(_ raw: [String], field: String) throws -> [String] {
        guard raw.count <= 64 else { throw Invalid(L10n[.Properties.validateListTooMany(field)]) }
        var seen = Set<String>()
        return try raw.map { path in
            let trimmed = path.trimmingCharacters(in: .whitespaces)
            guard !trimmed.isEmpty else { throw Invalid(L10n[.Properties.validatePathEmpty(field)]) }
            guard seen.insert(trimmed).inserted else {
                throw Invalid(L10n[.Properties.validateListDuplicate(field, trimmed)])
            }
            return trimmed
        }
    }

    /// What follows an action: an action each, once each, the condition optional.
    static func next(_ raw: [ActionNext]) throws -> [ActionNext] {
        guard raw.count <= 64 else { throw Invalid(L10n[.Properties.validateNextTooMany]) }
        var seen = Set<String>()
        return try raw.map { entry in
            let path = entry.path.trimmingCharacters(in: .whitespaces)
            guard !path.isEmpty else { throw Invalid(L10n[.Properties.validateNextPathEmpty]) }
            guard seen.insert(path).inserted else {
                throw Invalid(L10n[.Properties.validateNextDuplicate(path)])
            }
            let when = entry.when?.trimmingCharacters(in: .whitespacesAndNewlines)
            return ActionNext(path: path,
                              when: (when?.isEmpty ?? true) ? nil : try text(when!, field: L10n[.Properties.validateNextWhen], max: 2000))
        }
    }

    /// Paths an action uses: something to point at, and not the same thing twice.
    static func uses(_ raw: [ModuleUse]) throws -> [ModuleUse] {
        guard raw.count <= 400 else { throw Invalid(L10n[.Properties.validateUsesTooMany]) }
        var seen = Set<String>()
        return try raw.map { use in
            let path = use.path.trimmingCharacters(in: .whitespaces)
            guard !path.isEmpty else { throw Invalid(L10n[.Properties.validateUsesPathEmpty]) }
            guard seen.insert("\(use.role.rawValue)|\(path)").inserted else {
                throw Invalid(L10n[.Properties.validateUsesDuplicate(use.role.label, path)])
            }
            let note = use.note?.trimmingCharacters(in: .whitespacesAndNewlines)
            return ModuleUse(path: path, role: use.role,
                             note: (note?.isEmpty ?? true) ? nil : try text(note!, field: L10n[.Properties.validateUsesNote]))
        }
    }

    /// The discriminator names a field, so a value that could never be a field's name is a
    /// typo worth catching at the point it is written.
    static func discriminator(_ raw: String?) throws -> String? {
        guard let raw, !raw.trimmingCharacters(in: .whitespaces).isEmpty else { return nil }
        return try name(raw, field: "discriminator")
    }

    static func enumValues(_ raw: [(value: String, label: String)]) throws
    -> [(value: String, label: String)] {
        for entry in raw {
            let value = entry.value.trimmingCharacters(in: .whitespaces)
            guard !value.isEmpty else { throw Invalid(L10n[.Properties.validateEnumEmpty]) }
            guard value.count <= 80 else { throw Invalid(L10n[.Properties.validateEnumTooLong(value)]) }
            guard entry.label.count <= 120 else {
                throw Invalid(L10n[.Properties.validateEnumLabelTooLong(value)])
            }
        }
        return raw
    }

    static func metadata(_ raw: [String: String]) throws -> [String: String] {
        guard raw.count <= 64 else { throw Invalid(L10n[.Properties.validateMetadataTooMany]) }
        for (key, value) in raw {
            guard key.count <= 80 else { throw Invalid(L10n[.Properties.validateMetadataKeyTooLong]) }
            guard value.count <= 2000 else {
                throw Invalid(L10n[.Properties.validateMetadataValueTooLong(key)])
            }
        }
        return raw
    }

    /// A group may name another group; a set of them may not close into a loop.
    ///
    /// Expanding a cycle does not terminate, so the diagram filter would hang on a
    /// selection that looked ordinary. Refused where it is written rather than
    /// where it is read.
    static func acyclic(_ groups: [TagGroup]) throws {
        let byName = Dictionary(groups.map { ($0.name, $0) }, uniquingKeysWith: { a, _ in a })
        func walk(_ group: TagGroup, _ chain: [String]) throws {
            for member in group.tags {
                guard let nested = byName[member] else { continue }
                if chain.contains(member) {
                    throw Invalid(L10n[.Properties.validateGroupCycle((chain + [member]).joined(separator: " → "))])
                }
                try walk(nested, chain + [member])
            }
        }
        for group in groups { try walk(group, [group.name]) }
    }
}
