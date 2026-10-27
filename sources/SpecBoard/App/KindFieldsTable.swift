import SwiftUI

/// What the parts of the form are given: the draft as written, whether it is only to read, the kinds the
/// project has to pick from, and the way to change the draft (one change, taken back as one) and to go to
/// its JSON.
struct KindFormContext {
    var draft: KindDraft
    var readOnly: Bool
    /// Every kind of card the project has, its name and what people call it: what a field of the type
    /// card, a container and a kind of section hold.
    var cardKinds: [(name: String, label: String)]
    var edit: ((inout KindDraft) -> Void) -> Void
    var toJSON: () -> Void

    func label(ofCard name: String) -> String? { cardKinds.first { $0.name == name }?.label }
}

/// The fields of a place in a definition (the kind, a kind of row, a group, a kind of line) as a table:
/// their name, display name, type and settings, in the order the card shows them — dragged by ⋮⋮ to
/// another place, one added below. The field picked opens its settings under its row.
struct KindFieldsTable: View {
    var context: KindFormContext
    var parent: KindDraft.Place
    var forLine = false
    @Binding var selection: KindDraft.Place?

    @State private var target: Int?

    private var draft: KindDraft { context.draft }

    var body: some View {
        let names = draft.fieldNames(at: parent)
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 8) {
                Spacer().frame(width: 14)
                Text(L10n[.Common.wordName]).frame(width: 110, alignment: .leading)
                Text(L10n[.Common.wordDisplayName]).frame(width: 96, alignment: .leading)
                Text(L10n[.Kinds.formColType]).frame(width: 120, alignment: .leading)
                Text(L10n[.Kinds.formColSettings])
                Spacer(minLength: 0)
            }
            .font(.app(10.5)).foregroundStyle(.secondary)
            .padding(.vertical, 3)
            ForEach(Array(names.enumerated()), id: \.element) { index, name in
                let place = parent.field(name)
                row(place, index: index)
                if selection == place {
                    KindFieldSettings(context: context, place: place, forLine: forLine, selection: $selection)
                        .padding(.leading, 18).padding(.vertical, 4)
                }
            }
            if !context.readOnly {
                Button {
                    let name = Self.free("field", among: names)
                    context.edit { $0.addField(name, fieldtype: "text", at: parent) }
                    selection = parent.field(name)
                } label: { Label(L10n[.Kinds.formAddField], systemImage: "plus") }
                    .buttonStyle(.link).font(.app(11.5))
                    .padding(.top, 5)
                    .overlay(alignment: .top) { if target == names.count { dropLine } }
                    .dropDestination(for: String.self) { items, _ in
                        drop(items, at: names.count)
                    } isTargeted: { target = $0 ? names.count : (target == names.count ? nil : target) }
            }
        }
    }

    private var dropLine: some View { Rectangle().fill(Color.accentColor).frame(height: 2) }

    private func row(_ place: KindDraft.Place, index: Int) -> some View {
        let name = place.name ?? ""
        let type = KindFieldTypes.item(draft.fieldtype(at: place))
        let picked = selection == place
        return HStack(spacing: 8) {
            Image(systemName: "line.3.horizontal").font(.app(9)).foregroundStyle(.tertiary).frame(width: 14)
                .draggable(KindDragItem.item(place))
            Text(name).font(.app(11.5, design: .monospaced)).lineLimit(1).frame(width: 110, alignment: .leading)
            Text(draft.string(at: place, "label") ?? "").font(.app(11.5)).lineLimit(1).frame(width: 96, alignment: .leading)
            Label(type?.word ?? draft.fieldtype(at: place), systemImage: type?.symbol ?? "questionmark")
                .font(.app(11.5)).lineLimit(1).frame(width: 120, alignment: .leading)
            FlowRow(spacing: 4, lineSpacing: 3) {
                ForEach(Self.badges(draft, place), id: \.self) { KindTag(text: $0) }
            }
            Spacer(minLength: 0)
        }
        .padding(.vertical, 4).padding(.horizontal, 2)
        .background(picked ? Color.accentColor.opacity(0.14) : Color.clear, in: RoundedRectangle(cornerRadius: 4))
        .overlay(alignment: .top) { if target == index { dropLine } }
        .contentShape(Rectangle())
        .onTapGesture { selection = picked ? nil : place }
        .dropDestination(for: String.self) { items, _ in drop(items, at: index) } isTargeted: {
            target = $0 ? index : (target == index ? nil : target)
        }
    }

    private func drop(_ items: [String], at index: Int) -> Bool {
        guard !context.readOnly, let item = items.first,
              let name = KindDragItem.name(item, under: parent, row: false) else { return false }
        context.edit { $0.move(parent.field(name), to: index) }
        return true
    }

    /// A name not yet among `names`: `base`, else `base_2`, `base_3`…
    static func free(_ base: String, among names: [String]) -> String {
        if !names.contains(base) { return base }
        return (2...).lazy.map { "\(base)_\($0)" }.first { !names.contains($0) }!
    }

    /// What the table's settings column says of a field: whether it is required, how many options and
    /// values it has, what bounds it.
    static func badges(_ draft: KindDraft, _ place: KindDraft.Place) -> [String] {
        var out: [String] = []
        if draft.requiredOnCondition(at: place) { out.append(L10n[.Kinds.formBadgeConditional]) }
        else if draft.flag(at: place, "required") { out.append(L10n[.Common.wordRequired]) }
        switch draft.many(at: place) {
        case 1: out.append(L10n[.Kinds.formValuesList])
        case 2: out.append(L10n[.Kinds.formValuesListOfLists])
        default: break
        }
        let options = draft.strings(at: place, "options")
        if !options.isEmpty { out.append(L10n[.Kinds.formBadgeOptions(options.count)]) }
        if draft.flag(at: place, "unique") { out.append(L10n[.Kinds.formUnique]) }
        if let max = draft.value(at: place, "max")?.numberOrNil { out.append("≤ \(KindNumberField.said(max))") }
        if draft.string(at: place, "show") == "editor" { out.append(L10n[.Kinds.formEditorOnly]) }
        if draft.value(at: place, "when") != nil { out.append(L10n[.Kinds.formBadgeWhen]) }
        let inside = draft.fieldNames(at: place)
        if !inside.isEmpty { out.append(L10n[.Kinds.formBadgeInside(inside.count)]) }
        return out
    }
}

/// The settings of the field picked: name, display name, type, how many values, description, options,
/// whether it is required, and the rest under 「その他」; a group's own fields in the same table.
struct KindFieldSettings: View {
    var context: KindFormContext
    var place: KindDraft.Place
    var forLine = false
    @Binding var selection: KindDraft.Place?

    @State private var problem: String?
    @State private var more = false

    private var draft: KindDraft { context.draft }
    private var fieldtype: String { draft.fieldtype(at: place) }
    private static let choices: Set<String> = ["radio", "dropdownlist"]
    private static let texts: Set<String> = ["text", "longtext", "markdown", "url"]

    var body: some View {
        VStack(alignment: .leading, spacing: 7) {
            VStack(alignment: .leading, spacing: 7) {
                HStack(spacing: 12) {
                    KindFormLine(label: L10n[.Common.wordName], width: 80) {
                        CommitField.refusable(L10n[.Common.wordName], text: place.name ?? "", monospaced: true, accept: rename)
                            .frame(width: 130)
                    }
                    .fixedSize()
                    KindFormLine(label: L10n[.Common.wordDisplayName], width: 80) {
                        CommitField(L10n[.Common.wordDisplayName], text: draft.string(at: place, "label") ?? "") { text in
                            context.edit { $0.set(.string(text), at: place, "label") }
                        }
                        .frame(width: 130)
                    }
                    .fixedSize()
                }
                if let problem { Text(problem).font(.app(10.5)).foregroundStyle(.red).padding(.leading, 90) }
                HStack(spacing: 12) {
                    KindFormLine(label: L10n[.Kinds.formColType], width: 80) {
                        KindTypeMenu(fieldtype: fieldtype, forLine: forLine) { type in
                            context.edit { $0.setFieldtype(type, at: place) }
                        }
                    }
                    .fixedSize()
                    KindFormLine(label: L10n[.Kinds.formValues], width: 80) {
                        Picker("", selection: Binding(get: { draft.many(at: place) },
                                                      set: { many in context.edit { $0.setMany(many, at: place) } })) {
                            Text(L10n[.Kinds.formValuesOne]).tag(0)
                            Text(L10n[.Kinds.formValuesList]).tag(1)
                            Text(L10n[.Kinds.formValuesListOfLists]).tag(2)
                        }
                        .labelsHidden().fixedSize()
                    }
                    .fixedSize()
                }
                KindFormLine(label: L10n[.Common.wordDescription], width: 80) {
                    CommitField(L10n[.Common.wordDescription], text: draft.string(at: place, "description") ?? "") { text in
                        context.edit { $0.set(.string(text), at: place, "description") }
                    }
                }
                if Self.choices.contains(fieldtype) {
                    KindFormLine(label: L10n[.Kinds.formOptions], width: 80) {
                        KindOptionsEditor(context: context, place: place)
                    }
                }
                KindFormLine(label: L10n[.Common.wordRequired], width: 80) {
                    if draft.requiredOnCondition(at: place) {
                        KindJsonOnly(said: L10n[.Kinds.formRequiredOnlyWhen], toJSON: context.toJSON)
                    } else {
                        Toggle(L10n[.Kinds.formRequiredAlways], isOn: Binding(
                            get: { draft.flag(at: place, "required") },
                            set: { on in context.edit { $0.set(.bool(on), at: place, "required") } }))
                            .toggleStyle(.checkbox).font(.app(11.5))
                        Button(L10n[.Kinds.formRequiredOnlyWhen], action: context.toJSON).buttonStyle(.link).font(.app(11))
                    }
                }
            }
            .disabled(context.readOnly)
            moreSettings
            if fieldtype == "group" {
                KindFormLine(label: L10n[.Kinds.formFieldsInside], width: 80) {
                    KindFieldsTable(context: context, parent: place, forLine: forLine, selection: $selection)
                }
            }
            if !context.readOnly {
                HStack {
                    Spacer()
                    Button(L10n[.Kinds.formDeleteField], role: .destructive) {
                        context.edit { $0.remove(place) }
                        selection = nil
                    }
                    .buttonStyle(.borderless).font(.app(11))
                }
            }
        }
        .padding(10)
        .background(Color.secondary.opacity(0.06), in: RoundedRectangle(cornerRadius: 6))
        .overlay(RoundedRectangle(cornerRadius: 6).stroke(.quaternary))
    }

    /// Whether the field took the name: one refused (another field beside it has it) leaves the draft and the
    /// Name box on the name it had, the reason under it.
    private func rename(_ to: String) -> Bool {
        var why: KindsEditing.Problem?
        context.edit { why = $0.rename(place, to: to) }
        problem = why?.said
        if why == nil, to != place.name { selection = place.named(to) }
        return why == nil
    }

    // MARK: - その他

    /// What the field's type lets it say beyond the usual, folded under 「その他」 with a line of what is set.
    @ViewBuilder
    private var moreSettings: some View {
        VStack(alignment: .leading, spacing: 6) {
            Button { more.toggle() } label: {
                HStack(spacing: 6) {
                    Image(systemName: more ? "chevron.down" : "chevron.right").font(.app(9, weight: .semibold)).frame(width: 10)
                    Text(L10n[.Kinds.formMore]).font(.app(11.5, weight: .medium))
                    if !more { Text(moreSummary).font(.app(10.5)).foregroundStyle(.secondary).lineLimit(1) }
                    Spacer(minLength: 0)
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .padding(.leading, 60)
            if more { moreLines.padding(.leading, 20).disabled(context.readOnly) }
        }
    }

    /// The names of what 「その他」 holds, as its folded line says them.
    private var moreSummary: String {
        var out = [L10n[.Kinds.formUnique], L10n[.Kinds.formMinMax]]
        if draft.many(at: place) > 0 { out.append(L10n[.Kinds.formCountRange]) }
        if KindDraft.holds(fieldtype, key: "done", from: fieldtype) { out.append(L10n[.Kinds.formDoneValues]) }
        out += [L10n[.Kinds.formRequiredOnlyWhen], L10n[.Kinds.formEditorOnly]]
        return out.joined(separator: " · ")
    }

    private func flag(_ key: String, _ word: String, on: OJSON = .bool(true)) -> some View {
        Toggle(word, isOn: Binding(
            get: { draft.value(at: place, key).map { KindDraft.same($0, on) } ?? false },
            set: { now in context.edit { $0.set(now ? on : nil, at: place, key) } }))
            .toggleStyle(.checkbox).font(.app(11.5))
    }

    @ViewBuilder
    private var moreLines: some View {
        let type = fieldtype
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 14) {
                flag("unique", L10n[.Kinds.formUnique])
                flag("show", L10n[.Kinds.formEditorOnly], on: .string("editor"))
                if !forLine { flag("look", L10n[.Kinds.formLookOnly]) }
            }
            if KindDraft.holds(type, key: "min", from: type) {
                KindFormLine(label: L10n[.Kinds.formMinMax], width: 110) {
                    bound("min")
                    Text(verbatim: "–").foregroundStyle(.secondary)
                    bound("max")
                }
            }
            if draft.many(at: place) > 0 {
                KindFormLine(label: L10n[.Kinds.formCountRange], width: 110) {
                    bound("min_count")
                    Text(verbatim: "–").foregroundStyle(.secondary)
                    bound("max_count")
                }
            }
            if KindDraft.holds(type, key: "done", from: type) {
                KindFormLine(label: L10n[.Kinds.formDoneValues], width: 110) {
                    if type == "checkbox" {
                        flag("done", L10n[.Kinds.formDoneChecked], on: .array([.bool(true)]))
                    } else {
                        KindChips(items: draft.strings(at: place, "done"), label: { _ in nil },
                                  choices: draft.strings(at: place, "options"), addTitle: L10n[.Kinds.formOptionChip],
                                  readOnly: context.readOnly) { values in
                            context.edit { $0.set(OJSON.string(values), at: place, "done") }
                        }
                    }
                }
            }
            if KindDraft.holds(type, key: "pattern", from: type) {
                KindFormLine(label: L10n[.Kinds.formPattern], width: 110) {
                    Picker("", selection: Binding(get: { draft.string(at: place, "pattern") ?? "" },
                                                  set: { said in context.edit { $0.set(.string(said), at: place, "pattern") } })) {
                        Text(L10n[.Common.wordNone]).tag("")
                        Text(L10n[.Kinds.formPatternIdentifier]).tag("identifier")
                    }
                    .labelsHidden().fixedSize()
                }
                KindFormLine(label: L10n[.Kinds.formForbid], width: 110) {
                    CommitField(L10n[.Kinds.formForbid], text: draft.string(at: place, "forbid") ?? "", monospaced: true) { text in
                        context.edit { $0.set(.string(text), at: place, "forbid") }
                    }
                    .frame(width: 120)
                }
                flag("paths", L10n[.Kinds.formPaths]).padding(.leading, 120)
            }
            if type == "card" {
                KindFormLine(label: L10n[.Kinds.formCardKinds], width: 110) {
                    KindChips(items: draft.strings(at: place, "kinds"), label: context.label(ofCard:),
                              choices: context.cardKinds.map(\.name), addTitle: L10n[.Kinds.formKindChip],
                              readOnly: context.readOnly) { kinds in
                        context.edit { $0.set(OJSON.string(kinds), at: place, "kinds") }
                    }
                }
            }
            if type == "path" { flag("store", L10n[.Kinds.formStoreId], on: .string("id")).padding(.leading, 120) }
            defaultLine(type)
            KindFormLine(label: L10n[.Kinds.formCondition], width: 110) {
                if let when = draft.value(at: place, "when") {
                    KindJsonOnly(said: when.compact(), toJSON: context.toJSON)
                } else {
                    Button(L10n[.Kinds.formRequiredOnlyWhen], action: context.toJSON).buttonStyle(.link).font(.app(11))
                }
            }
        }
    }

    /// A number bound (`min`, `max`, `min_count`, `max_count`).
    private func bound(_ key: String) -> some View {
        KindNumberField(value: draft.value(at: place, key)?.numberOrNil, placeholder: key) { number in
            context.edit { $0.set(number.map(OJSON.number), at: place, key) }
        }
    }

    /// The default of a field whose type is not a choice (a choice's is its options' dot): written for a
    /// text or a number; a json's and a map's is the JSON's to write.
    @ViewBuilder
    private func defaultLine(_ type: String) -> some View {
        let value = draft.value(at: place, "default")
        if type == "json" || type == "map" || type == "group" {
            if let value {
                KindFormLine(label: L10n[.Kinds.formOptionDefault], width: 110) {
                    KindJsonOnly(said: value.compact(), toJSON: context.toJSON)
                }
            }
        } else if type == "checkbox" {
            flag("default", L10n[.Kinds.formDefaultOn]).padding(.leading, 120)
        } else if !Self.choices.contains(type), draft.many(at: place) == 0 {
            KindFormLine(label: L10n[.Kinds.formOptionDefault], width: 110) {
                CommitField(L10n[.Kinds.formOptionDefault], text: value?.stringOrNil ?? value?.numberOrNil.map(KindNumberField.said) ?? "") { text in
                    let written: OJSON? = text.isEmpty ? nil
                        : (type == "number" || type == "mr") ? Double(text).map(OJSON.number) : .string(text)
                    context.edit { $0.set(written, at: place, "default") }
                }
                .frame(width: 160)
            }
        }
    }
}

/// A choice's options: which is the default (the dot), the value written, and how it is shown
/// (`optionLabels`) — dragged by ⋮⋮ to another place, one added below.
struct KindOptionsEditor: View {
    var context: KindFormContext
    var place: KindDraft.Place

    @State private var target: Int?

    private var draft: KindDraft { context.draft }

    var body: some View {
        let options = draft.strings(at: place, "options")
        let labels = draft.value(at: place, "optionLabels")?.objectOrNil ?? OrderedObject()
        let chosen = draft.value(at: place, "default")?.stringOrNil
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 8) {
                Spacer().frame(width: 14)
                Text(L10n[.Kinds.formOptionDefault]).frame(width: 40)
                Text(L10n[.Kinds.formOptionValue]).frame(width: 110, alignment: .leading)
                Text(L10n[.Kinds.formOptionShownAs]).frame(width: 110, alignment: .leading)
            }
            .font(.app(10)).foregroundStyle(.secondary)
            ForEach(Array(options.enumerated()), id: \.offset) { index, option in
                HStack(spacing: 8) {
                    Image(systemName: "line.3.horizontal").font(.app(9)).foregroundStyle(.tertiary).frame(width: 14)
                        .draggable(KindDragItem.chip + "option:\(index)")
                    Button {
                        write(options, labels, default: chosen == option ? nil : option)
                    } label: {
                        Image(systemName: chosen == option ? "largecircle.fill.circle" : "circle")
                            .foregroundStyle(chosen == option ? Color.accentColor : .secondary)
                    }
                    .buttonStyle(.plain).frame(width: 40)
                    CommitField(L10n[.Kinds.formOptionValue], text: option) { renamed(index, $0, options, labels, chosen) }
                        .frame(width: 110)
                    CommitField(L10n[.Kinds.formOptionShownPlaceholder], text: labels[option]?.stringOrNil ?? "") { text in
                        var next = labels
                        next[option] = text.isEmpty ? nil : .string(text)
                        write(options, next, default: chosen)
                    }
                    .frame(width: 110)
                    Button {
                        var next = options
                        next.remove(at: index)
                        var nextLabels = labels
                        nextLabels[option] = nil
                        write(next, nextLabels, default: chosen == option ? nil : chosen,
                              done: draft.strings(at: place, "done").filter { $0 != option })
                    } label: { Image(systemName: "minus") }
                        .buttonStyle(.borderless)
                }
                .overlay(alignment: .top) { if target == index { Rectangle().fill(Color.accentColor).frame(height: 2) } }
                .dropDestination(for: String.self) { items, _ in
                    guard let item = items.first, item.hasPrefix(KindDragItem.chip + "option:"),
                          let from = Int(item.dropFirst((KindDragItem.chip + "option:").count)), from != index,
                          options.indices.contains(from) else { return false }
                    var next = options
                    let moved = next.remove(at: from)
                    next.insert(moved, at: from < index ? index - 1 : index)
                    write(next, labels, default: chosen)
                    return true
                } isTargeted: { target = $0 ? index : (target == index ? nil : target) }
            }
            Button {
                let made = ((options.count + 1)...).lazy.map { L10n[.Kinds.formOptionNew($0)] }.first { !options.contains($0) }!
                write(options + [made], labels, default: chosen)
            } label: { Label(L10n[.Kinds.formAddOption], systemImage: "plus") }
                .buttonStyle(.link).font(.app(11)).padding(.leading, 22)
        }
    }

    /// An option written anew: its default, how it is shown and whether it counts as done follow it.
    private func renamed(_ index: Int, _ to: String, _ options: [String], _ labels: OrderedObject, _ chosen: String?) {
        let from = options[index]
        guard !to.isEmpty, to != from, !options.contains(to) else { return }
        var next = options
        next[index] = to
        var nextLabels = OrderedObject()
        for (key, value) in labels.entries { nextLabels[key == from ? to : key] = value }
        write(next, nextLabels, default: chosen == from ? to : chosen,
              done: draft.strings(at: place, "done").map { $0 == from ? to : $0 })
    }

    private func write(_ options: [String], _ labels: OrderedObject, default chosen: String?, done: [String]? = nil) {
        context.edit { draft in
            draft.together { d in
                d.set(OJSON.string(options), at: place, "options")
                d.set(.object(labels), at: place, "optionLabels")
                d.set(chosen.map(OJSON.string), at: place, "default")
                if let done { d.set(OJSON.string(done), at: place, "done") }
            }
        }
    }
}

/// The kinds of row of a place (the kind, or a kind of row above) as a table: name, display name, how a
/// row is named in a path, how many fields — dragged to another place, one added below. The one picked
/// opens its settings and its fields' table under its row.
struct KindRowsTable: View {
    var context: KindFormContext
    var parent: KindDraft.Place = .kind
    @Binding var selection: KindDraft.Place?

    @State private var target: Int?
    @State private var problem: String?

    private var draft: KindDraft { context.draft }

    var body: some View {
        let names = draft.rowNames(at: parent)
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 8) {
                Spacer().frame(width: 14)
                Text(L10n[.Kinds.renamedRow]).frame(width: 110, alignment: .leading)
                Text(L10n[.Common.wordDisplayName]).frame(width: 96, alignment: .leading)
                Text(L10n[.Kinds.formNamedBy]).frame(width: 100, alignment: .leading)
                Text(L10n[.Kinds.formColFields])
                Spacer(minLength: 0)
            }
            .font(.app(10.5)).foregroundStyle(.secondary)
            .padding(.vertical, 3)
            ForEach(Array(names.enumerated()), id: \.element) { index, name in
                let place = parent.row(name)
                row(place, index: index)
                if selection.map({ $0.isIn(place) }) == true {
                    settings(place).padding(.leading, 18).padding(.vertical, 4)
                }
            }
            if !context.readOnly {
                Button {
                    let name = KindFieldsTable.free("rows", among: names)
                    context.edit { $0.addRow(name, at: parent) }
                    selection = parent.row(name)
                } label: { Label(L10n[.Kinds.formAddRow], systemImage: "plus") }
                    .buttonStyle(.link).font(.app(11.5)).padding(.top, 5)
                    .overlay(alignment: .top) { if target == names.count { Rectangle().fill(Color.accentColor).frame(height: 2) } }
                    .dropDestination(for: String.self) { items, _ in drop(items, at: names.count) } isTargeted: {
                        target = $0 ? names.count : (target == names.count ? nil : target)
                    }
            }
        }
    }

    private func row(_ place: KindDraft.Place, index: Int) -> some View {
        let picked = selection == place
        return HStack(spacing: 8) {
            Image(systemName: "line.3.horizontal").font(.app(9)).foregroundStyle(.tertiary).frame(width: 14)
                .draggable(KindDragItem.item(place))
            Text(place.name ?? "").font(.app(11.5, design: .monospaced)).lineLimit(1).frame(width: 110, alignment: .leading)
            Text(draft.string(at: place, "label") ?? "").font(.app(11.5)).lineLimit(1).frame(width: 96, alignment: .leading)
            Text(Self.address(draft.string(at: place, "address") ?? "name")).font(.app(11.5)).lineLimit(1)
                .frame(width: 100, alignment: .leading)
            Text("\(draft.fieldNames(at: place).count)").font(.app(11.5))
            Spacer(minLength: 0)
        }
        .padding(.vertical, 4).padding(.horizontal, 2)
        .background(picked ? Color.accentColor.opacity(0.14) : Color.clear, in: RoundedRectangle(cornerRadius: 4))
        .overlay(alignment: .top) { if target == index { Rectangle().fill(Color.accentColor).frame(height: 2) } }
        .contentShape(Rectangle())
        .onTapGesture { selection = selection.map { $0.isIn(place) } == true ? nil : place }
        .dropDestination(for: String.self) { items, _ in drop(items, at: index) } isTargeted: {
            target = $0 ? index : (target == index ? nil : target)
        }
    }

    private func drop(_ items: [String], at index: Int) -> Bool {
        guard !context.readOnly, let item = items.first,
              let name = KindDragItem.name(item, under: parent, row: true) else { return false }
        context.edit { $0.move(parent.row(name), to: index) }
        return true
    }

    /// How a row is named in a path, said.
    static func address(_ address: String) -> String {
        switch address {
        case "value": L10n[.Kinds.formAddressValue]
        case "index": L10n[.Kinds.formAddressIndex]
        default: L10n[.Kinds.formAddressName]
        }
    }

    /// A path to a row of this kind, as an example of how it is named.
    private func example(_ place: KindDraft.Place) -> String {
        let name = PathText.quote(place.name ?? "")
        switch draft.string(at: place, "address") ?? "name" {
        case "index": return "\(name)[0]"
        case "value": return "\(name).value"
        default: return "\(name).name"
        }
    }

    private func settings(_ place: KindDraft.Place) -> some View {
        VStack(alignment: .leading, spacing: 7) {
            VStack(alignment: .leading, spacing: 7) {
                HStack(spacing: 12) {
                    KindFormLine(label: L10n[.Common.wordName], width: 80) {
                        CommitField.refusable(L10n[.Common.wordName], text: place.name ?? "", monospaced: true, accept: { to in
                            var why: KindsEditing.Problem?
                            context.edit { why = $0.rename(place, to: to) }
                            problem = why?.said
                            if why == nil, to != place.name { selection = place.named(to) }
                            return why == nil
                        })
                        .frame(width: 130)
                    }
                    .fixedSize()
                    KindFormLine(label: L10n[.Common.wordDisplayName], width: 80) {
                        CommitField(L10n[.Common.wordDisplayName], text: draft.string(at: place, "label") ?? "") { text in
                            context.edit { $0.set(.string(text), at: place, "label") }
                        }
                        .frame(width: 130)
                    }
                    .fixedSize()
                }
                if let problem { Text(problem).font(.app(10.5)).foregroundStyle(.red).padding(.leading, 90) }
                KindFormLine(label: L10n[.Kinds.formNamedBy], width: 80) {
                    Picker("", selection: Binding(get: { draft.string(at: place, "address") ?? "name" },
                                                  set: { said in context.edit { $0.set(.string(said), at: place, "address") } })) {
                        ForEach(["name", "value", "index"], id: \.self) { Text(Self.address($0)).tag($0) }
                    }
                    .labelsHidden().fixedSize()
                    Text(L10n[.Kinds.formPathExample(example(place))]).font(.app(10.5, design: .monospaced)).foregroundStyle(.secondary)
                }
                KindFormLine(label: L10n[.Kinds.formAtMost], width: 80) {
                    KindNumberField(value: draft.value(at: place, "max_total")?.numberOrNil, placeholder: L10n[.Kinds.formAny]) { number in
                        context.edit { $0.set(number.map(OJSON.number), at: place, "max_total") }
                    }
                    Text(L10n[.Kinds.formAtMostRows]).font(.app(10.5)).foregroundStyle(.secondary)
                }
                KindFormLine(label: L10n[.Kinds.formRowsInside], width: 80) {
                    KindJsonOnly(said: rowsInside(place), toJSON: context.toJSON)
                }
            }
            .disabled(context.readOnly)
            KindFieldsTable(context: context, parent: place, selection: $selection)
            if !context.readOnly {
                HStack {
                    Spacer()
                    Button(L10n[.Kinds.formDeleteRow], role: .destructive) {
                        context.edit { $0.remove(place) }
                        selection = nil
                    }
                    .buttonStyle(.borderless).font(.app(11))
                }
            }
        }
        .padding(10)
        .background(Color.secondary.opacity(0.06), in: RoundedRectangle(cornerRadius: 6))
        .overlay(RoundedRectangle(cornerRadius: 6).stroke(.quaternary))
    }

    /// The rows under a row of this kind (`rows`, `ref`), and how deep it nests and when (`depth`, `when`),
    /// in short: the JSON writes them.
    private func rowsInside(_ place: KindDraft.Place) -> String {
        var parts: [String] = []
        if case .array(let items)? = draft.value(at: place, "rows") {
            parts += items.compactMap { item in
                guard case .object(let o) = item else { return nil }
                return o["name"]?.stringOrNil ?? o["ref"]?.stringOrNil.map { "↺\($0)" }
            }
        }
        if let depth = draft.value(at: place, "depth")?.numberOrNil { parts.append("depth \(KindNumberField.said(depth))") }
        if draft.value(at: place, "when") != nil { parts.append("when") }
        return parts.isEmpty ? L10n[.Common.wordNone] : parts.joined(separator: " · ")
    }
}
