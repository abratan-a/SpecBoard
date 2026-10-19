import Foundation

/// The project's kinds of card, of section and of line as the project settings' tab for kinds
/// reads and writes them: read with `kind_read`, and the project's own made, changed and let go one
/// by one with `kind_create`, `kind_update` and `kind_delete` — tried as they are written without
/// writing (`dryRun`) — and a built-in kind's context written over with `kind_update` too: the same
/// tools MCP writes them with. What is directly under each root is the project's (`project_read`'s
/// `projectTree`, `project_update`'s `project_tree`). Each definition is JSON written by hand for
/// now; its keys go in the order written.
extension AppModel {
    /// The lists of definitions, in the order the tab shows them.
    enum KindList: String, CaseIterable, Identifiable, Sendable {
        case cards, sections, lines

        var id: String { rawValue }

        /// The key a definition is named by in the list.
        var nameKey: String {
            switch self {
            case .cards: "kind"
            case .sections: "section"
            case .lines: "line"
            }
        }

        /// What `kind_create`'s `of` calls a kind of the list.
        var of: String {
            switch self {
            case .cards: "card"
            case .sections: "section"
            case .lines: "line"
            }
        }

        var title: String {
            switch self {
            case .cards: L10n[.Common.wordCard]
            case .sections: L10n[.Common.wordSection]
            case .lines: L10n[.Common.wordLine]
            }
        }

        /// The same word for the middle of a sentence.
        var noun: String {
            switch self {
            case .cards: L10n[.Kinds.listNounCard]
            case .sections: L10n[.Kinds.listNounSection]
            case .lines: L10n[.Kinds.listNounLine]
            }
        }

        /// A text put between the quotes of a JSON string.
        private static func escaped(_ text: String) -> String {
            text.replacingOccurrences(of: "\\", with: "\\\\").replacingOccurrences(of: "\"", with: "\\\"")
                .replacingOccurrences(of: "\n", with: "\\n")
        }

        /// A new definition's JSON, to be written over: the least each kind of list needs.
        var template: String {
            switch self {
            case .cards:
                let label = Self.escaped(L10n[.Kinds.templateNewKind])
                let state = Self.escaped(L10n[.Kinds.templateStateLabel])
                let todo = Self.escaped(L10n[.Kinds.templateStateTodo])
                let done = Self.escaped(L10n[.Kinds.templateStateDone])
                return """
                {
                  "kind": "new_kind",
                  "label": "\(label)",
                  "color": "#6E7781",
                  "fields": [
                    { "name": "state", "label": "\(state)", "fieldtype": "radio", "options": ["\(todo)", "\(done)"], "default": "\(todo)" }
                  ],
                  "rows": []
                }
                """
            case .sections:
                let label = Self.escaped(L10n[.Kinds.templateNewSection])
                return """
                {
                  "section": "new_section",
                  "label": "\(label)",
                  "holds": ["note"]
                }
                """
            case .lines:
                let name = Self.escaped(L10n[.Kinds.templateNewLineName])
                let label = Self.escaped(L10n[.Kinds.templateNewLineLabel])
                return """
                {
                  "line": "\(name)",
                  "label": "\(label)",
                  "base": "link",
                  "from": [{ "kinds": ["*"], "at": "card" }],
                  "to": { "kinds": ["*"], "at": ["card"] }
                }
                """
            }
        }
    }

    /// One definition as the tab lists it: which list, its name, how people call it, whether
    /// it is built in (read only, but for its context), and its JSON as written.
    struct KindEntry: Identifiable, Hashable {
        var list: KindList
        var name: String
        var label: String
        var builtin: Bool
        var text: String
        /// One of the project's own the project has no place for (`noPlace`): no
        /// section of its tree holds it, or no such card has a place at either end of the line.
        var noPlace = false
        /// What it is for, when to use it and not (`context`: `purpose`, `use_when`, `not_for`), as the
        /// project has it — a built-in one's written over by the project (`contextChanged`).
        var context: [String: String] = [:]
        var contextChanged = false
        /// A built-in kind of section's mark written over by the project (`iconChanged`).
        var iconChanged = false

        var id: String { "\(list.rawValue)|\(builtin ? "builtin" : "own")|\(name)" }
    }

    /// What a check of the definitions said: whether they may be written, every problem with
    /// them, and how many cards each rename moves.
    struct KindsCheck: Equatable {
        var ok: Bool
        var problems: [String]
        var moved: [String] = []
    }

    /// Every definition the project has, built-in ones first, list by list (`kind_read`).
    func kindEntries() -> [KindEntry] {
        guard let answer = try? tool("kind_read", inTree: false),
              case .object(let wrapped) = answer, case .object(let body)? = wrapped["json"] else { return [] }
        var out: [KindEntry] = []
        for list in KindList.allCases {
            guard case .array(let items)? = body[list.rawValue] else { continue }
            for item in items {
                guard case .object(var object) = item, case .string(let name)? = object[list.nameKey] else { continue }
                let flag = { (key: String) in object[key].map { if case .bool(true) = $0 { true } else { false } } ?? false }
                let builtin = flag("builtin")
                let noPlace = flag("noPlace")
                let contextChanged = flag("contextChanged")
                let iconChanged = flag("iconChanged")
                for key in ["builtin", "noPlace", "contextChanged", "iconChanged", "of"] { object[key] = nil }
                var context: [String: String] = [:]
                if case .object(let said)? = object["context"] {
                    for (key, value) in said.entries { if case .string(let text) = value { context[key] = text } }
                }
                let label = object["label"].flatMap { if case .string(let s) = $0 { s } else { nil } } ?? name
                out.append(KindEntry(list: list, name: name, label: label, builtin: builtin, text: OJSON.object(object).pretty(),
                                     noPlace: noPlace, context: context, contextChanged: contextChanged,
                                     iconChanged: iconChanged))
            }
        }
        return out
    }

    /// What is directly under each root (`project_tree`) as the tab writes it: its
    /// JSON, as `project_read` says it (`projectTree`).
    func projectTreeText() -> String {
        guard let answer = try? tool("project_read", inTree: false),
              case .object(let wrapped) = answer, case .object(let body)? = wrapped["json"],
              let tree = body["projectTree"] else { return "" }
        return tree.pretty()
    }

    /// `project_tree` as written, tried without writing: what is wrong with it, and what writing it
    /// does under each root — kept, made, moved into `_evacuation_`, gone, renamed — as lines.
    func checkProjectTree(_ tree: OJSON) -> KindsCheck {
        do {
            let answer = try projectTreeTool(tree, dryRun: true)
            guard case .object(let wrapped) = answer, case .object(let body)? = wrapped["json"] else {
                return KindsCheck(ok: false, problems: [L10n[.Common.problemCoreUnreadable]])
            }
            return KindsCheck(ok: Self.flag(body["ok"]), problems: Self.texts(body["problems"]),
                              moved: Self.projectTreeSaid(body["projectTree"]))
        } catch {
            return KindsCheck(ok: false, problems: [error.localizedDescription])
        }
    }

    /// What writing `project_tree` does, a line for each root it changes (`projectTree` of the answer).
    static func projectTreeSaid(_ raw: OJSON?) -> [String] {
        guard case .array(let trees)? = raw else { return [] }
        return trees.compactMap { tree -> String? in
            guard case .object(let t) = tree else { return nil }
            let names = { (key: String) -> [String] in
                guard case .array(let items)? = t[key] else { return [] }
                return items.compactMap { item in
                    switch item {
                    case .string(let s): return s
                    case .object(let o):
                        if case .string(let from)? = o["from"], case .string(let to)? = o["to"] { return "\(from) → \(to)" }
                        if case .string(let section)? = o["section"], case .string(let to)? = o["to"] { return "\(section) → \(to)" }
                        return nil
                    default: return nil
                    }
                }
            }
            let separator = L10n[.Common.separatorList]
            let parts = [(L10n[.Common.actionCreate], "create"), (L10n[.Common.actionRename], "rename"),
                         (L10n[.Kinds.projectTreeEvacuate], "evacuate"), (L10n[.Kinds.projectTreeDelete], "delete")]
                .compactMap { word, key in names(key).isEmpty ? nil : "\(word): \(names(key).joined(separator: separator))" }
            guard let first = parts.first else { return nil }
            let tree = if case .string("board")? = t["tree"] { L10n[.Kinds.projectTreeUnderBoard] } else { L10n[.Kinds.projectTreeUnderBranches] }
            return L10n[.Kinds.projectTreeLine(tree, parts.dropFirst().reduce(first) { L10n[.Kinds.projectTreeParts($0, $1)] })]
        }
    }

    /// `project_tree` written as `tree` (the window's: under main's lock too). Whether it was written;
    /// why not is the window's error.
    @discardableResult
    func saveProjectTree(_ tree: OJSON) -> Bool {
        write { _ = try projectTreeTool(tree, dryRun: false) }
        return lastError == nil
    }

    private func projectTreeTool(_ tree: OJSON, dryRun: Bool) throws -> OJSON {
        guard let core, let projectId = detail?.project.id else { throw CoreClient.Refusal(L10n[.Common.problemNoProject]) }
        var value = OrderedObject([("project_tree", tree)])
        if dryRun { value["dryRun"] = .bool(true) }
        let arguments = OrderedObject([("project", .string(projectId)), ("value", .object(value))])
        return try core.ordered("tool", OrderedObject([("name", .string("project_update")), ("arguments", .object(arguments))]))
    }

    /// A definition of the project's own written as `written` in place of the one called `name` (none
    /// for a new one), with the tab's `renames`, tried without writing (`dryRun`): what is wrong with
    /// it, all of it, and how many cards each rename moves.
    func checkKind(_ list: KindList, replacing name: String?, with written: OJSON, renames: [OJSON] = []) -> KindsCheck {
        do {
            return Self.checked(try kindTool(list, replacing: name, with: written, renames: renames, dryRun: true))
        } catch {
            return KindsCheck(ok: false, problems: [error.localizedDescription])
        }
    }

    /// A definition of the project's own written as `written` in place of the one called `name` (none
    /// for a new one), with `renames` (one line in main's history). Whether it was written; why not
    /// is the window's error.
    @discardableResult
    func saveKind(_ list: KindList, replacing name: String?, with written: OJSON, renames: [OJSON] = []) -> Bool {
        write { _ = try kindTool(list, replacing: name, with: written, renames: renames, dryRun: false) }
        return lastError == nil
    }

    /// The project's own kind called `name` let go of, tried without writing: refused for one in use.
    func checkKindDeleted(_ name: String) -> KindsCheck {
        do {
            return Self.checked(try kindDelete(name, dryRun: true))
        } catch {
            return KindsCheck(ok: false, problems: [error.localizedDescription])
        }
    }

    /// The project's own kind called `name` let go of. Whether it was; why not is the window's error.
    @discardableResult
    func deleteKind(_ name: String) -> Bool {
        write { _ = try kindDelete(name, dryRun: false) }
        return lastError == nil
    }

    /// A built-in kind's context written over by the project (`kind_update`'s `context`): what it is
    /// for, when to use it and not, each said only when written; none gives it back its own. Whether
    /// it was written; why not is the window's error.
    @discardableResult
    func setBuiltinContext(_ name: String, _ context: [String: String]?) -> Bool {
        write {
            let written: OJSON = context.map { said in
                .object(OrderedObject(["purpose", "use_when", "not_for"].compactMap { key in
                    said[key].flatMap { $0.isEmpty ? nil : (key, OJSON.string($0)) }
                }))
            } ?? .null
            _ = try kindsCall("kind_update", target: name, value: OrderedObject([("context", written)]))
        }
        return lastError == nil
    }

    /// A built-in kind of section's mark written over by the project (`kind_update`'s `icon`: `{name,
    /// color}`, each said only when set; none gives it back its own). Whether it was written; why not
    /// is the window's error.
    @discardableResult
    func setBuiltinIcon(_ name: String, _ mark: SectionMark?) -> Bool {
        write {
            let written: OJSON = mark.map { said in
                .object(OrderedObject([("name", said.icon), ("color", said.color)].compactMap { key, value in
                    value.map { (key, OJSON.string($0)) }
                }))
            } ?? .null
            _ = try kindsCall("kind_update", target: name, value: OrderedObject([("icon", written)]))
        }
        return lastError == nil
    }

    private static func flag(_ raw: OJSON?) -> Bool {
        raw.map { if case .bool(true) = $0 { true } else { false } } ?? false
    }

    private static func texts(_ raw: OJSON?) -> [String] {
        guard case .array(let said)? = raw else { return [] }
        return said.compactMap { if case .string(let s) = $0 { s } else { nil } }
    }

    /// What a tried write said, as the tab shows it.
    private static func checked(_ answer: OJSON) -> KindsCheck {
        guard case .object(let wrapped) = answer, case .object(let body)? = wrapped["json"] else {
            return KindsCheck(ok: false, problems: [L10n[.Common.problemCoreUnreadable]])
        }
        var moved: [String] = []
        if case .array(let renamed)? = body["renamed"] {
            for item in renamed {
                guard case .object(let o) = item else { continue }
                let text = { (key: String) -> String? in if case .string(let s)? = o[key] { s } else { nil } }
                let count = o["cards"].flatMap { if case .number(let n) = $0 { Int(n) } else { nil } } ?? 0
                let what = [text("kind"), text("row"), text("field")].compactMap { $0 }.joined(separator: ".")
                moved.append(L10n[.Kinds.renameMoved(what, text("to") ?? "", L10n[.Kinds.renameCards(count)])])
            }
        }
        return KindsCheck(ok: flag(body["ok"]), problems: texts(body["problems"]), moved: moved)
    }

    /// One of the kinds' tools on the project open, its keys in the order written: they are kept as
    /// they are sent.
    private func kindsCall(_ name: String, target: OJSON, value: OrderedObject) throws -> OJSON {
        guard let core, let projectId = detail?.project.id else { throw CoreClient.Refusal(L10n[.Common.problemNoProject]) }
        let arguments = OrderedObject([("project", .string(projectId)), ("target", target), ("value", .object(value))])
        return try core.ordered("tool", OrderedObject([("name", .string(name)), ("arguments", .object(arguments))]))
    }

    private func kindsCall(_ name: String, target: String, value: OrderedObject) throws -> OJSON {
        try kindsCall(name, target: .string(target), value: value)
    }

    /// A definition written as the kinds' tools take it: a new one made (`kind_create`, its name the
    /// target, `of` its list), one there written whole in its place (`kind_update`: each key it no
    /// longer has taken away, a name of its own its `name`, the tab's renames of its fields and rows
    /// its `renames`).
    private func kindTool(_ list: KindList, replacing name: String?, with written: OJSON, renames: [OJSON],
                          dryRun: Bool) throws -> OJSON {
        guard case .object(var value) = written, case .string(let now)? = value[list.nameKey] else {
            throw CoreClient.Refusal(L10n[.Kinds.problemNameMissing(list.nameKey)])
        }
        value[list.nameKey] = nil
        guard let name else {
            var made = OrderedObject([("of", .string(list.of))])
            for (key, item) in value.entries { made[key] = item }
            if dryRun { made["dryRun"] = .bool(true) }
            return try kindsCall("kind_create", target: now, value: made)
        }
        // What it has now that what is written has not, taken away.
        if case .object(let wrapped) = try kindsCall("kind_read", target: name, value: OrderedObject()),
           case .object(let was)? = wrapped["json"] {
            for key in was.keys where ![list.nameKey, "project", "of", "builtin", "noPlace", "contextChanged", "iconChanged"].contains(key)
                && value[key] == nil {
                value[key] = .null
            }
        }
        if now != name { value["name"] = .string(now) }
        let moved: [OJSON] = renames.compactMap { rename in
            guard case .object(var o) = rename, o["field"] != nil || o["row"] != nil else { return nil }
            o["kind"] = nil
            return .object(o)
        }
        if !moved.isEmpty { value["renames"] = .array(moved) }
        if dryRun { value["dryRun"] = .bool(true) }
        return try kindsCall("kind_update", target: name, value: value)
    }

    private func kindDelete(_ name: String, dryRun: Bool) throws -> OJSON {
        try kindsCall("kind_delete", target: name, value: dryRun ? OrderedObject([("dryRun", .bool(true))]) : OrderedObject())
    }
}

/// A definition being written in the tab for kinds, apart from the view so a test can write
/// one too: its JSON read (or why not), and renames written into it — a field, a kind of row or the kind of card itself given another name, its
/// values moving with it (`renames`).
enum KindsEditing {
    /// Why what is written is not what was asked for, as it is said under it.
    struct Problem: Error, Equatable {
        var said: String
        init(_ said: String) { self.said = said }
    }

    /// The mark a definition says: its `icon: {name, color}`, each part nil when
    /// it is not written.
    static func mark(in text: String) -> SectionMark {
        guard let parsed = OJSON.parse(text), case .object(let object) = parsed,
              case .object(let icon)? = object["icon"] else { return SectionMark() }
        let said = { (key: String) -> String? in if case .string(let s)? = icon[key] { s } else { nil } }
        return SectionMark(icon: said("name"), color: said("color"))
    }

    /// The definition written with its `icon` set to `mark` — each of `name` and `color` only when
    /// it is there, the whole taken out when neither is — the rest as it was, in its order: what the
    /// kinds tab's pickers write (a kind of section's mark). None when what is written is not an
    /// object.
    static func setting(_ mark: SectionMark, in text: String) -> String? {
        guard let parsed = OJSON.parse(text), case .object(var object) = parsed else { return nil }
        let parts: [(String, OJSON)] = [("name", mark.icon), ("color", mark.color)].compactMap { key, value in
            value.map { (key, OJSON.string($0)) }
        }
        object["icon"] = parts.isEmpty ? nil : .object(OrderedObject(parts))
        // The way before (a `color` beside the `icon`) goes with it.
        object["color"] = nil
        return OJSON.object(object).pretty()
    }

    /// What is written for `key` in the definition, when it is a string.
    static func said(_ key: String, in text: String) -> String? {
        guard let parsed = OJSON.parse(text), case .object(let object) = parsed,
              case .string(let value)? = object[key] else { return nil }
        return value
    }

    /// The marks offered for a kind of section (SF Symbols), and what each is for, in a word.
    static var sectionIcons: [(symbol: String, word: String)] {
        let icons: [(String, LocalizedStringResource)] = [
            ("folder", .Kinds.iconFolder), ("tray.full", .Kinds.iconStorage),
            ("checklist", .Kinds.iconChecklist), ("checkmark.seal", .Kinds.iconVerify),
            ("testtube.2", .Kinds.iconTest), ("exclamationmark.triangle", .Kinds.iconCaution),
            ("lightbulb", .Kinds.iconIdea), ("book", .Kinds.iconBook),
            ("doc.text", .Kinds.iconDocument), ("tablecells", .Common.wordTable),
            ("curlybraces", .Kinds.iconDefinition),
            ("cube", .Kinds.iconPart), ("shippingbox", .Kinds.iconBox),
            ("gearshape", .Common.wordSettings), ("key", .Kinds.iconKey),
            ("lock.shield", .Kinds.iconProtection), ("person.2", .Kinds.iconPeople),
            ("calendar", .Kinds.iconSchedule), ("flag", .Kinds.iconFlag),
            ("star", .Kinds.iconStar), ("tag", .Kinds.iconTag),
            ("link", .Kinds.iconLink), ("server.rack", .Kinds.iconServer),
            ("network", .Kinds.iconNetwork), ("ladybug", .Kinds.iconBug),
        ]
        return icons.map { (symbol: $0, word: L10n[$1]) }
    }

    /// The JSON written, read as one definition: or why it is not one.
    static func parse(_ text: String, as list: AppModel.KindList) -> Result<OJSON, Problem> {
        if let value = OJSON.parse(text), case .object(let object) = value {
            guard case .string(let name)? = object[list.nameKey], !name.isEmpty else {
                return .failure(Problem(L10n[.Kinds.problemNameMissing(list.nameKey)]))
            }
            return .success(value)
        }
        // Why it is not JSON, as Foundation says it.
        do {
            _ = try JSONSerialization.jsonObject(with: Data(text.utf8), options: [.fragmentsAllowed])
            return .failure(Problem(L10n[.Kinds.problemNotObject]))
        } catch {
            let said = (error as NSError).userInfo[NSDebugDescriptionErrorKey] as? String ?? error.localizedDescription
            return .failure(Problem(L10n[.Kinds.problemNotJSON(said)]))
        }
    }

    /// What can be renamed in a kind of card.
    enum Renamed: String, CaseIterable, Identifiable {
        case field, row, kind
        var id: String { rawValue }

        var title: String {
            switch self {
            case .field: L10n[.Common.wordField]
            case .row: L10n[.Kinds.renamedRow]
            case .kind: L10n[.Common.wordKind]
            }
        }
    }

    /// A rename written into a kind of card's JSON: the field (or kind of row) `from` called
    /// `to` where the definition has it (one the definition no longer has — a value left
    /// behind — stays as it is, and its values go to `to`, which the definition must have), or
    /// the kind called `to`. The JSON changed, and the rename `kind_update` is told.
    static func rename(_ text: String, what: Renamed, from: String, to: String) -> Result<(text: String, rename: OJSON), Problem> {
        guard case .success(let parsed) = parse(text, as: .cards), case .object(var object) = parsed,
              case .string(let kind)? = object["kind"] else {
            return .failure(Problem(L10n[.Kinds.problemFixJSONFirst]))
        }
        let from = from.trimmingCharacters(in: .whitespaces)
        let to = to.trimmingCharacters(in: .whitespaces)
        guard !to.isEmpty else { return .failure(Problem(L10n[.Kinds.problemNewNameMissing])) }
        func renamed(_ list: OJSON?) -> OJSON? {
            guard case .array(let items)? = list else { return list }
            return .array(items.map { item in
                guard case .object(var o) = item, case .string(let name)? = o["name"], name == from else { return item }
                o["name"] = .string(to)
                return .object(o)
            })
        }
        var rename = OrderedObject()
        switch what {
        case .kind:
            object["kind"] = .string(to)
            rename["kind"] = .string(kind)
            rename["to"] = .string(to)
        case .field:
            guard !from.isEmpty else { return .failure(Problem(L10n[.Kinds.problemPreviousFieldMissing])) }
            object["fields"] = renamed(object["fields"])
            // The header names it too.
            if case .array(let header)? = object["header"] {
                object["header"] = .array(header.map { if case .string(let s) = $0, s == from { .string(to) } else { $0 } })
            }
            rename["kind"] = .string(kind)
            rename["field"] = .string(from)
            rename["to"] = .string(to)
        case .row:
            guard !from.isEmpty else { return .failure(Problem(L10n[.Kinds.problemPreviousRowMissing])) }
            object["rows"] = renamed(object["rows"])
            rename["kind"] = .string(kind)
            rename["row"] = .string(from)
            rename["to"] = .string(to)
        }
        return .success((OJSON.object(object).pretty(), .object(rename)))
    }

    /// A kind of card's renames, as the kind is named in them: once the kind itself is renamed,
    /// a later rename of its fields names it by its new name.
    static func renaming(_ renames: [OJSON], kind from: String, to: String) -> [OJSON] {
        renames.map { item in
            guard case .object(var o) = item, case .string(let k)? = o["kind"], k == from, o["field"] != nil || o["row"] != nil
            else { return item }
            o["kind"] = .string(to)
            return .object(o)
        }
    }
}
