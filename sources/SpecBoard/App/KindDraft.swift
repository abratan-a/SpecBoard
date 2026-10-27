import Foundation

/// A definition being written in the tab for kinds (`specboard://SpecBoard/Docs/Design:CardKinds`), as the
/// form and the JSON both write it: one draft, its keys in the order written.
///
/// The form changes only the keys it writes. Whatever it has no part for — a condition, a field's
/// `when`, rows inside rows, a face's page, a key no definition has — stays where it is. A key set back
/// to what it means when it is left out goes; a key written anew goes where the definitions' table puts
/// it among those there, so a definition the form wrote reads as one written by hand. A field or a kind
/// of row the saved definition has, renamed here, is renamed with its values on saving (`renames`, as
/// 「名前を変える…」 says it). Each change can be taken back and made again until the draft is let go.
struct KindDraft {
    let list: AppModel.KindList
    /// The definition as written now.
    private(set) var object: OrderedObject
    /// The name the definition was saved under; none for a new one.
    let savedName: String?

    /// What can be taken back and made again: the definition, where each field and kind of row the saved
    /// definition has is now, and the renames of values left behind.
    private struct State {
        var object: OrderedObject
        var origins: [Place: String]
        var strays: [OJSON]
    }

    /// The saved name of each field and kind of row the saved definition has, by where it is now.
    private var origins: [Place: String] = [:]
    /// Renames of values the cards hold that the definition no longer has (「定義に無い欄」) to a field it has.
    private var strays: [OJSON] = []
    private var undone: [State] = []
    private var redone: [State] = []

    /// A place in a definition: the definition itself (no steps), one of its fields, one of its kinds of row,
    /// a field of a kind of row or of a group — each step down by its name.
    nonisolated struct Place: Hashable {
        enum Step: Hashable {
            case field(String)
            case row(String)
        }

        var steps: [Step]

        init(_ steps: [Step] = []) { self.steps = steps }

        static let kind = Place()
        static func field(_ name: String) -> Place { Place([.field(name)]) }
        static func row(_ name: String) -> Place { Place([.row(name)]) }
        static func rowField(_ row: String, _ field: String) -> Place { Place([.row(row), .field(field)]) }

        func field(_ name: String) -> Place { Place(steps + [.field(name)]) }
        func row(_ name: String) -> Place { Place(steps + [.row(name)]) }

        /// The place holding this one; the definition's is itself.
        var parent: Place { Place(Array(steps.dropLast())) }

        /// The name of what is here; none for the definition.
        var name: String? {
            switch steps.last {
            case .field(let name), .row(let name): name
            case nil: nil
            }
        }

        var isField: Bool { if case .field? = steps.last { true } else { false } }
        var isRow: Bool { if case .row? = steps.last { true } else { false } }

        /// This place with the last step's name changed.
        func named(_ name: String) -> Place {
            switch steps.last {
            case .field: Place(steps.dropLast() + [.field(name)])
            case .row: Place(steps.dropLast() + [.row(name)])
            case nil: self
            }
        }

        /// Whether this place is `other` or inside it.
        func isIn(_ other: Place) -> Bool { steps.starts(with: other.steps) }
    }

    /// The definition `text` as the form reads it; or why it cannot be. `saved`: the definition as it
    /// was saved (none for a new one), what a rename is told from.
    static func read(_ text: String, as list: AppModel.KindList, saved: String? = nil) -> Result<KindDraft, KindsEditing.Problem> {
        switch KindsEditing.parse(text, as: list) {
        case .failure(let problem): return .failure(problem)
        case .success(let parsed):
            guard case .object(let object) = parsed else { return .failure(KindsEditing.Problem(L10n[.Kinds.problemNotObject])) }
            if let problem = shapeProblem(object) { return .failure(problem) }
            let savedObject: OrderedObject? = saved.flatMap {
                if case .object(let o)? = OJSON.parse($0) { o } else { nil }
            }
            return .success(KindDraft(list: list, object: object, saved: savedObject))
        }
    }

    init(list: AppModel.KindList, object: OrderedObject, saved: OrderedObject? = nil) {
        self.list = list
        self.object = object
        if let saved, case .string(let name)? = saved[list.nameKey] {
            savedName = name
            origins = Self.places(saved, at: .kind)
        } else {
            savedName = nil
        }
    }

    /// Why a definition cannot be read by the form: a list of fields or rows that is not one of objects
    /// each named (a kind of row above taken again, `ref`, stands among the rows of a row). None when it can.
    static func shapeProblem(_ object: OrderedObject, inRow: Bool = false) -> KindsEditing.Problem? {
        for key in ["fields", "rows"] {
            guard let list = object[key] else { continue }
            guard case .array(let items) = list else { return KindsEditing.Problem(L10n[.Kinds.problemNotList(key)]) }
            for item in items {
                guard case .object(let o) = item else { return KindsEditing.Problem(L10n[.Kinds.problemItemUnnamed(key)]) }
                if inRow, key == "rows", case .string? = o["ref"] { continue }
                guard case .string? = o["name"] else { return KindsEditing.Problem(L10n[.Kinds.problemItemUnnamed(key)]) }
                if let problem = shapeProblem(o, inRow: inRow || key == "rows") { return problem }
            }
        }
        return nil
    }

    /// Every field and kind of row under `place` by where it is, its name.
    private static func places(_ object: OrderedObject, at place: Place) -> [Place: String] {
        var out: [Place: String] = [:]
        for (key, step) in [("fields", Place.Step.field), ("rows", Place.Step.row)] {
            guard case .array(let items)? = object[key] else { continue }
            for item in items {
                guard case .object(let o) = item, case .string(let name)? = o["name"] else { continue }
                let here = Place(place.steps + [step(name)])
                out[here] = name
                out.merge(places(o, at: here)) { a, _ in a }
            }
        }
        return out
    }

    // MARK: - Reading

    /// The draft as JSON, as the JSON editor shows it.
    var text: String { OJSON.object(object).pretty() }

    /// The definition's name (`kind`, `section`, `line`).
    var name: String { object[list.nameKey]?.stringOrNil ?? "" }

    /// What is at `place`: none when nothing there is so named.
    func at(_ place: Place) -> OrderedObject? {
        var here = object
        for step in place.steps {
            let (key, name) = Self.key(step)
            guard case .array(let items)? = here[key],
                  let found = items.lazy.compactMap({ if case .object(let o) = $0 { o } else { nil } })
                      .first(where: { $0["name"]?.stringOrNil == name })
            else { return nil }
            here = found
        }
        return here
    }

    /// The value of `keys` (a key, and keys inside its object: `context`, `purpose`) at `place`.
    func value(at place: Place = .kind, _ keys: String...) -> OJSON? { value(at: place, keys) }

    func value(at place: Place, _ keys: [String]) -> OJSON? {
        guard var here = at(place).map(OJSON.object) else { return nil }
        for key in keys {
            guard case .object(let o) = here, let next = o[key] else { return nil }
            here = next
        }
        return here
    }

    func string(at place: Place = .kind, _ keys: String...) -> String? { value(at: place, keys)?.stringOrNil }

    func strings(at place: Place = .kind, _ keys: String...) -> [String] {
        guard case .array(let items)? = value(at: place, keys) else { return [] }
        return items.compactMap(\.stringOrNil)
    }

    func flag(at place: Place = .kind, _ keys: String...) -> Bool {
        if case .bool(true)? = value(at: place, keys) { true } else { false }
    }

    /// The names of the fields (or of the kinds of row) right under `place`, in their order.
    func fieldNames(at place: Place = .kind) -> [String] { names("fields", at: place) }
    func rowNames(at place: Place = .kind) -> [String] { names("rows", at: place) }

    private func names(_ key: String, at place: Place) -> [String] {
        guard case .array(let items)? = at(place)?[key] else { return [] }
        return items.compactMap { if case .object(let o) = $0 { o["name"]?.stringOrNil } else { nil } }
    }

    /// A field's type (`fieldtype`).
    func fieldtype(at place: Place) -> String { string(at: place, "fieldtype") ?? "" }

    /// How many values a field holds: 0 one, 1 a list, 2 a list of lists (`many`).
    func many(at place: Place) -> Int {
        switch value(at: place, "many") {
        case .bool(true)?: 1
        case .number(let n)?: Int(n)
        default: 0
        }
    }

    /// Whether a field is required only when a condition holds (`required` written as one): the form
    /// says so and leaves it to the JSON.
    func requiredOnCondition(at place: Place) -> Bool {
        if case .object? = value(at: place, "required") { true } else { false }
    }

    /// Whether the saved definition has the field or kind of row at `place`, and by what name.
    func savedName(of place: Place) -> String? { origins[place] }

    /// The keys at `place` no definition has (the definitions' table does not name them): the form
    /// says they are there and leaves them to the JSON.
    func otherKeys(at place: Place = .kind) -> [String] {
        guard let here = at(place) else { return [] }
        let known = Set(Self.keyOrder(Self.level(of: place, list: list), []))
        return here.keys.filter { !known.contains($0) }
    }

    // MARK: - What saving renames

    /// What saving renames, as `kind_update` is told (`renames`, the kind's own rename first): the
    /// kind, each field and kind of row the saved definition has whose name is now another, and the
    /// values left behind moved to a field. Each names the kind by its name now, a field of a kind of
    /// row its kind of row by its name now.
    var renames: [OJSON] {
        var out: [OJSON] = []
        let kind = name
        if list == .cards, let savedName, savedName != kind, !kind.isEmpty {
            out.append(.object(OrderedObject([("kind", .string(savedName)), ("to", .string(kind))])))
        }
        guard list == .cards else { return out }
        let moved = origins.filter { $0.key.name != $0.value }
        func said(_ place: Place, _ was: String) -> OJSON {
            var o = OrderedObject([("kind", .string(kind))])
            if place.isRow {
                o["row"] = .string(was)
            } else {
                if case .row(let row)? = place.parent.steps.last { o["row"] = .string(row) }
                o["field"] = .string(was)
            }
            o["to"] = .string(place.name ?? "")
            return .object(o)
        }
        // The kind's fields, then its kinds of row, then their fields: a field of a kind of row is named
        // under its kind of row's name once that is renamed.
        let fields = moved.filter { $0.key.steps.count == 1 && $0.key.isField }
        let rows = moved.filter { $0.key.steps.count == 1 && $0.key.isRow }
        let rowFields = moved.filter { $0.key.steps.count == 2 && $0.key.isField && $0.key.parent.isRow }
        for group in [fields, rows, rowFields] {
            for (place, was) in group.sorted(by: { order($0.key).lexicographicallyPrecedes(order($1.key)) }) { out.append(said(place, was)) }
        }
        return out + strays
    }

    /// Where a place stands among its kind's, for saying the renames in the order the definition has them.
    private func order(_ place: Place) -> [Int] {
        var out: [Int] = []
        var walked = Place.kind
        for step in place.steps {
            let (key, name) = Self.key(step)
            let names = key == "fields" ? fieldNames(at: walked) : rowNames(at: walked)
            out.append(names.firstIndex(of: name) ?? names.count)
            walked = Place(walked.steps + [step])
        }
        return out
    }

    /// The renames as the tab says them before saving (`KindRenameSheet.said`).
    var renamesSaid: [String] { renames.map { KindRenameSheet.said($0) } }

    // MARK: - Writing

    /// `value` written for `keys` at `place`: nothing changes when it is what is there. A value that
    /// means what leaving the key out means takes the key away (and an object it leaves empty); a key
    /// written anew goes where the definitions' table puts it. Each write can be taken back.
    mutating func set(_ value: OJSON?, at place: Place = .kind, _ keys: String...) { set(value, at: place, keys) }

    mutating func set(_ value: OJSON?, at place: Place, _ keys: [String]) {
        guard !keys.isEmpty, !Self.same(self.value(at: place, keys), value) else { return }
        let level = Self.level(of: place, list: list)
        let left = value == nil || Self.isDefault(value!, key: keys.last!, level: level, parents: Array(keys.dropLast()))
        guard !(left && self.value(at: place, keys) == nil) else { return }
        remember()
        edit(place) { here in
            Self.put(left ? nil : value, keys[...], in: &here, level: level, parents: [])
        }
    }

    /// A field added under `place` (the definition, a kind of row or a group), last: named `name`, of the
    /// type `fieldtype`. Nothing is added when one there is so named.
    mutating func addField(_ name: String, fieldtype: String, at place: Place = .kind) {
        add("fields", OrderedObject([("name", .string(name)), ("fieldtype", .string(fieldtype))]), at: place)
    }

    /// A kind of row added under `place`, last, named `name`.
    mutating func addRow(_ name: String, at place: Place = .kind) {
        add("rows", OrderedObject([("name", .string(name)), ("fields", .array([]))]), at: place)
    }

    private mutating func add(_ key: String, _ item: OrderedObject, at place: Place) {
        let name = item["name"]?.stringOrNil ?? ""
        guard at(place) != nil, !names(key, at: place).contains(name) else { return }
        let level = Self.level(of: place, list: list)
        remember()
        edit(place) { here in
            var items: [OJSON] = if case .array(let list)? = here[key] { list } else { [] }
            items.append(.object(item))
            Self.put(.array(items), [key][...], in: &here, level: level, parents: [])
        }
    }

    /// The field or kind of row at `place` taken out. Saved, its values stay on the cards as values the
    /// definition no longer has.
    mutating func remove(_ place: Place) {
        guard let name = place.name, at(place) != nil else { return }
        let key = place.isField ? "fields" : "rows"
        remember()
        edit(place.parent) { here in
            guard case .array(let items)? = here[key] else { return }
            let left = items.filter { if case .object(let o) = $0 { o["name"]?.stringOrNil != name } else { true } }
            here[key] = left.isEmpty ? nil : .array(left)
        }
        origins = origins.filter { !$0.key.isIn(place) }
    }

    /// The field or kind of row at `place` moved to stand at `index` among its own (the index as they
    /// stand before the move).
    mutating func move(_ place: Place, to index: Int) {
        guard let name = place.name else { return }
        let key = place.isField ? "fields" : "rows"
        let names = self.names(key, at: place.parent)
        guard let from = names.firstIndex(of: name), index != from, index != from + 1, (0...names.count).contains(index)
        else { return }
        remember()
        edit(place.parent) { here in
            guard case .array(var items)? = here[key] else { return }
            let item = items.remove(at: from)
            items.insert(item, at: index > from ? index - 1 : index)
            here[key] = .array(items)
        }
    }

    /// The field or kind of row at `place` named `to`: one the saved definition has is renamed with its
    /// values on saving; one written since is only named anew. A field's name where the definition names
    /// it (its header, `as_row`, its face) follows. Why it cannot be, when it cannot.
    @discardableResult
    mutating func rename(_ place: Place, to: String) -> KindsEditing.Problem? {
        let to = to.trimmingCharacters(in: .whitespaces)
        guard let from = place.name, from != to else { return nil }
        guard !to.isEmpty else { return KindsEditing.Problem(L10n[.Kinds.problemNewNameMissing]) }
        let key = place.isField ? "fields" : "rows"
        guard !names(key, at: place.parent).contains(to) else {
            return KindsEditing.Problem(L10n[.Kinds.problemNameTaken(to)])
        }
        let renamed = place.named(to)
        // A field of the kind or of its kind of row, or a kind of row: renamed as 「名前を変える…」 renames it.
        let shared = place.steps.count == 1 || (place.steps.count == 2 && place.isField && place.parent.isRow)
        remember()
        if list == .cards, shared,
           case .success(let made) = KindsEditing.rename(object, what: place.isField ? .field : .row, from: from, to: to,
                                                        row: place.steps.count == 2 ? place.parent.name : nil) {
            object = made.object
        } else {
            edit(place.parent) { here in
                guard case .array(let items)? = here[key] else { return }
                here[key] = .array(items.map { item in
                    guard case .object(var o) = item, o["name"]?.stringOrNil == from else { return item }
                    o["name"] = .string(to)
                    return .object(o)
                })
            }
        }
        origins = Dictionary(origins.map { here, was in
            here.isIn(place) ? (Place(renamed.steps + here.steps.dropFirst(place.steps.count)), was) : (here, was)
        }, uniquingKeysWith: { a, _ in a })
        return nil
    }

    /// A rename as 「名前を変える…」 writes it: a field or kind of row the draft has is renamed here (as the
    /// form renames it), the kind itself named anew, or a value the cards hold that the definition no
    /// longer has (`from`, not one of its fields) moved to a field it has on saving. Why not, when not.
    @discardableResult
    mutating func rename(_ what: KindsEditing.Renamed, from: String, to: String) -> KindsEditing.Problem? {
        let from = from.trimmingCharacters(in: .whitespaces)
        let to = to.trimmingCharacters(in: .whitespaces)
        switch what {
        case .kind:
            guard !to.isEmpty else { return KindsEditing.Problem(L10n[.Kinds.problemNewNameMissing]) }
            set(.string(to), at: .kind, list.nameKey)
            return nil
        case .row:
            guard !from.isEmpty else { return KindsEditing.Problem(L10n[.Kinds.problemPreviousRowMissing]) }
            guard rowNames().contains(from) else { return KindsEditing.Problem(L10n[.Kinds.problemPreviousRowMissing]) }
            return rename(.row(from), to: to)
        case .field:
            if fieldNames().contains(from) { return rename(.field(from), to: to) }
            switch KindsEditing.rename(object, what: .field, from: from, to: to) {
            case .failure(let problem): return problem
            case .success(let made):
                remember()
                strays.append(made.rename)
                return nil
            }
        }
    }

    /// A field's type changed: what the new type cannot hold goes with the old one (taken back together).
    mutating func setFieldtype(_ fieldtype: String, at place: Place) {
        let was = self.fieldtype(at: place)
        guard was != fieldtype, at(place) != nil else { return }
        remember()
        edit(place) { here in
            here["fieldtype"] = .string(fieldtype)
            for key in here.keys where !Self.holds(fieldtype, key: key, from: was) { here[key] = nil }
        }
    }

    /// How many values a field holds (0 one, 1 a list, 2 a list of lists).
    mutating func setMany(_ many: Int, at place: Place) {
        guard many != self.many(at: place) else { return }
        set(many == 0 ? nil : many == 1 ? .bool(true) : .number(Double(many)), at: place, "many")
    }

    /// What a kind of card holds (`holds`): holding any makes it a container opened on its card map;
    /// holding none gives it back the way it opens when it says none.
    mutating func setHolds(_ kinds: [String]) {
        guard !Self.same(value(at: .kind, "holds"), kinds.isEmpty ? nil : OJSON.string(kinds)) else { return }
        let level = Self.level(of: .kind, list: list)
        let opens = list == .cards
        remember()
        edit(.kind) { here in
            Self.put(kinds.isEmpty ? nil : OJSON.string(kinds), ["holds"][...], in: &here, level: level, parents: [])
            if opens {
                Self.put(kinds.isEmpty ? nil : .string(Self.cardMap), ["opens"][...], in: &here, level: level, parents: [])
            }
        }
    }

    /// A kind of section's mark as the pickers write it (`icon: {name, color}`): each part only when it is
    /// set, the whole taken out when neither is, and the way before (an `icon` that is a text, a `color`
    /// beside it) with it. Taken back as one change.
    mutating func setMark(_ mark: SectionMark) {
        together { d in
            if case .string? = d.value(at: .kind, "icon") { d.set(nil, at: .kind, "icon") }
            d.set(mark.icon.map(OJSON.string), at: .kind, "icon", "name")
            d.set(mark.color.map(OJSON.string), at: .kind, "icon", "color")
            d.set(nil, at: .kind, "color")
        }
    }

    /// The JSON written by hand in place of the draft: or why the form cannot read it (the draft as it was).
    /// Taken back as one change.
    mutating func setText(_ text: String) -> KindsEditing.Problem? {
        switch KindDraft.read(text, as: list) {
        case .failure(let problem): return problem
        case .success(let read):
            guard !Self.same(.object(read.object), .object(object)) else { return nil }
            remember()
            object = read.object
            // What the hand renamed is no rename: only what is still where it was keeps its saved name.
            origins = origins.filter { at($0.key) != nil }
            return nil
        }
    }

    /// Several writes made as one change: taken back together.
    mutating func together(_ change: (inout KindDraft) -> Void) {
        let before = state
        let count = undone.count
        change(&self)
        guard undone.count > count else { return }
        undone.removeSubrange(count...)
        undone.append(before)
    }

    // MARK: - Taking back

    var canUndo: Bool { !undone.isEmpty }
    var canRedo: Bool { !redone.isEmpty }

    mutating func undo() {
        guard let last = undone.popLast() else { return }
        redone.append(state)
        state = last
    }

    mutating func redo() {
        guard let next = redone.popLast() else { return }
        undone.append(state)
        state = next
    }

    /// The changes let go of: what saving, reverting or picking another definition does.
    mutating func forgetChanges() {
        undone = []
        redone = []
    }

    private var state: State {
        get { State(object: object, origins: origins, strays: strays) }
        set {
            object = newValue.object
            origins = newValue.origins
            strays = newValue.strays
        }
    }

    private mutating func remember() {
        undone.append(state)
        redone = []
    }

    // MARK: - Inside

    /// `change` made to the object at `place` in the definition.
    private mutating func edit(_ place: Place, _ change: (inout OrderedObject) -> Void) {
        object = Self.edited(object, place.steps[...], change)
    }

    private static func edited(_ object: OrderedObject, _ steps: ArraySlice<Place.Step>,
                               _ change: (inout OrderedObject) -> Void) -> OrderedObject {
        var object = object
        guard let step = steps.first else {
            change(&object)
            return object
        }
        let (key, name) = Self.key(step)
        guard case .array(let items)? = object[key] else { return object }
        object[key] = .array(items.map { item in
            guard case .object(let o) = item, o["name"]?.stringOrNil == name else { return item }
            return .object(edited(o, steps.dropFirst(), change))
        })
        return object
    }

    private static func key(_ step: Place.Step) -> (String, String) {
        switch step {
        case .field(let name): ("fields", name)
        case .row(let name): ("rows", name)
        }
    }

    /// `value` put at `keys` in `object`: none takes the key away, and an object it leaves empty with it;
    /// a key new to its object goes where the table of `level` (and the keys above, `parents`) puts it.
    private static func put(_ value: OJSON?, _ keys: ArraySlice<String>, in object: inout OrderedObject, level: Level,
                            parents: [String]) {
        guard let key = keys.first else { return }
        if keys.count == 1 {
            if let value {
                if object[key] != nil { object[key] = value } else { object = inserted(key, value, in: object, order: keyOrder(level, parents)) }
            } else {
                object[key] = nil
            }
            return
        }
        var inner: OrderedObject = if case .object(let o)? = object[key] { o } else { OrderedObject() }
        put(value, keys.dropFirst(), in: &inner, level: level, parents: parents + [key])
        if inner.isEmpty {
            object[key] = nil
        } else if object[key] != nil {
            object[key] = .object(inner)
        } else {
            object = inserted(key, .object(inner), in: object, order: keyOrder(level, parents))
        }
    }

    /// `object` with `key` put where `order` puts it: after the last key there it puts before it, else
    /// before the first it puts after it, else last. Keys it does not name stay where they are.
    static func inserted(_ key: String, _ value: OJSON, in object: OrderedObject, order: [String]) -> OrderedObject {
        var entries = object.entries
        let at: Int
        if let rank = order.firstIndex(of: key) {
            let ranked = entries.enumerated().compactMap { index, entry in order.firstIndex(of: entry.0).map { (index, $0) } }
            if let before = ranked.last(where: { $0.1 < rank }) {
                at = before.0 + 1
            } else if let after = ranked.first(where: { $0.1 > rank }) {
                at = after.0
            } else {
                at = entries.count
            }
        } else {
            at = entries.count
        }
        entries.insert((key, value), at: at)
        return OrderedObject(entries)
    }

    /// Whether two values are the same, their keys in the same order.
    static func same(_ a: OJSON?, _ b: OJSON?) -> Bool {
        switch (a, b) {
        case (nil, nil): true
        case (let a?, let b?): a.compact() == b.compact()
        default: false
        }
    }

    // MARK: - The definitions' table

    /// What a place in a definition is, for the keys it has and what each means left out.
    enum Level: Hashable {
        case card, section, line, field, row
    }

    static func level(of place: Place, list: AppModel.KindList) -> Level {
        switch place.steps.last {
        case .field?: return .field
        case .row?: return .row
        case nil:
            switch list {
            case .cards: return .card
            case .sections: return .section
            case .lines: return .line
            }
        }
    }

    /// How a container of the project's own opens: on its card map.
    static let cardMap = "card-map"

    /// The keys of each place in the order the definitions' table says them (the keys of an object
    /// inside one by the key it is under).
    static func keyOrder(_ level: Level, _ parents: [String]) -> [String] {
        switch parents.last {
        case "context": return ["purpose", "use_when", "not_for"]
        case "face": return ["top", "markdown", "svg", "html", "resizable"]
        case "look": return ["color", "dash", "begin", "end"]
        case "icon": return ["name", "color"]
        case "export": return ["as", "key", "word"]
        case "to": return ["kinds", "at", "rows"]
        case .some: return []
        case nil: break
        }
        switch level {
        case .card:
            return ["kind", "label", "color", "header", "abstract", "system_only", "extends", "as_row", "holds", "opens",
                    "connectable", "on_double_click", "face", "context", "semantics", "actions", "fields", "rows"]
        case .section:
            return ["section", "label", "description", "holds", "sections", "opens", "icon", "trees", "actions", "context",
                    "export"]
        case .line:
            return ["line", "label", "description", "base", "look", "from", "to", "max", "fields", "context", "visible",
                    "export", "owner", "siblings", "source"]
        case .field:
            return ["name", "label", "description", "fieldtype", "many", "when", "required", "default", "options",
                    "optionLabels", "unique", "min", "max", "min_count", "max_count", "pattern", "forbid", "look", "store",
                    "kinds", "paths", "done", "show", "fields"]
        case .row:
            return ["name", "label", "address", "depth", "max_total", "when", "fields", "rows"]
        }
    }

    /// Whether `value` for `key` means what leaving the key out means: none, an empty text, list or
    /// object, an unset flag, or the default the definitions say for it.
    static func isDefault(_ value: OJSON, key: String, level: Level, parents: [String]) -> Bool {
        let named: Set<String> = ["kind", "section", "line", "name", "fieldtype"]
        if parents.isEmpty, named.contains(key) { return false }
        let said: [String: OJSON]
        switch (level, parents.last) {
        case (.card, nil): said = ["extends": .string("base"), "opens": .string("fields")]
        case (.row, nil): said = ["address": .string("name")]
        case (.section, nil): said = ["trees": OJSON.string(["main", "board"])]
        case (.line, nil): said = ["visible": .bool(true)]
        case (.line, "look"?): said = ["color": .string("grey"), "dash": .string("solid"), "begin": .string("none"),
                                       "end": .string("arrow")]
        default: said = [:]
        }
        // A key whose default the definitions say means only that: `trees: []` (under no root) and
        // `visible: false` (drawn only when switched on) say something of their own.
        if let standard = said[key] {
            if case .null = value { return true }
            return same(standard, value)
        }
        switch value {
        case .null: return true
        case .bool(false): return true
        case .string(let s) where s.isEmpty: return true
        case .array(let a) where a.isEmpty: return true
        case .object(let o) where o.isEmpty: return true
        default: return false
        }
    }

    /// Whether a field of the type `fieldtype` holds `key` (it held it as a field of the type `was`).
    static func holds(_ fieldtype: String, key: String, from was: String) -> Bool {
        let choices: Set<String> = ["radio", "dropdownlist"]
        let texts: Set<String> = ["text", "longtext", "markdown", "url"]
        switch key {
        case "options", "optionLabels": return choices.contains(fieldtype)
        case "done": return choices.contains(fieldtype) || fieldtype == "checkbox"
        case "min", "max": return texts.contains(fieldtype) || fieldtype == "number"
        case "pattern", "forbid", "paths": return fieldtype == "text" || fieldtype == "longtext"
        case "kinds": return fieldtype == "card"
        case "store": return fieldtype == "path"
        case "fields": return fieldtype == "group"
        case "default":
            // A default stays only where the value it is reads the same as the new type.
            let shapes: [Set<String>] = [texts.union(["date", "person", "branch"]).union(choices), ["number", "mr"],
                                         ["checkbox"], ["json"], ["map"]]
            return shapes.contains { $0.contains(fieldtype) && $0.contains(was) }
        default: return true
        }
    }
}
