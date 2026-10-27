import SwiftUI

/// What the tab for kinds keeps on this Mac (`specboard://SpecBoard/Docs/Design:UI`, the Kinds tab): whether a
/// definition opens as the form or as its JSON, and which groups of the form are open — the same groups stay
/// open whichever definition is picked.
enum KindFormMemory {
    static let modeKey = "kinds.editor"
    static let openKey = "kinds.form.open"
    static let form = "form"
    static let json = "json"
    /// The groups open the first time: what a definition is, and what it holds.
    static let standardOpen = "card.basics,card.fields,section.basics,section.holds,line.basics,line.look,preview"

    static func groups(_ said: String) -> Set<String> { Set(said.split(separator: ",").map(String.init)) }
    static func said(_ groups: Set<String>) -> String { groups.sorted().joined(separator: ",") }
}

/// One foldable group of the form: its title, a count where it lists things, and folded, a line saying
/// what is in it.
struct KindFormGroup<Content: View>: View {
    var title: String
    var count: Int? = nil
    var summary: String
    /// A word put beside the title whether it is open or not (「置き場なし」).
    var badge: String? = nil
    var open: Bool
    var toggle: () -> Void
    @ViewBuilder var content: () -> Content

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Button(action: toggle) {
                HStack(spacing: 6) {
                    Image(systemName: open ? "chevron.down" : "chevron.right")
                        .font(.app(9, weight: .semibold)).foregroundStyle(.secondary).frame(width: 10)
                    Text(title).font(.app(12.5, weight: .semibold))
                    if let count { Text("\(count)").font(.app(11)).foregroundStyle(.secondary) }
                    if let badge { KindTag(text: badge, tint: .orange) }
                    if !open, !summary.isEmpty {
                        Text(summary).font(.app(11)).foregroundStyle(.secondary).lineLimit(1).truncationMode(.tail)
                    }
                    Spacer(minLength: 0)
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            if open {
                content().padding(.leading, 16).frame(maxWidth: .infinity, alignment: .leading)
            }
            Divider()
        }
    }
}

/// A small word in a box: a setting a field has, or what a definition lacks.
struct KindTag: View {
    var text: String
    var tint: Color = .secondary

    var body: some View {
        Text(text).font(.app(9.5)).foregroundStyle(tint)
            .padding(.horizontal, 4).padding(.vertical, 1)
            .overlay(RoundedRectangle(cornerRadius: 3).stroke(tint.opacity(0.6)))
            .fixedSize()
    }
}

/// One line of a group: its word on the left, right-aligned with the others, and its control.
struct KindFormLine<Content: View>: View {
    var label: String
    var width: CGFloat = 104
    @ViewBuilder var content: () -> Content

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 10) {
            Text(label).font(.app(11)).foregroundStyle(.secondary)
                .frame(width: width, alignment: .trailing)
            content()
            Spacer(minLength: 0)
        }
    }
}

/// What the form has no part for, said where it is: a short summary, and the way to the JSON to write it.
struct KindJsonOnly: View {
    var said: String
    var toJSON: () -> Void

    var body: some View {
        HStack(spacing: 6) {
            Text(said).font(.app(11)).foregroundStyle(.secondary).lineLimit(2).truncationMode(.tail)
            Button(L10n[.Kinds.formEditInJSON], action: toJSON)
                .buttonStyle(.link).font(.app(11))
        }
    }
}

/// Names picked from a list, each a chip taken out with its ×, more added from a menu; dragged to
/// another place among them when their order says something.
struct KindChips: View {
    /// The names, each with what people call it.
    var items: [String]
    var label: (String) -> String?
    /// What can be added, in the order offered.
    var choices: [String]
    var addTitle: String
    var readOnly: Bool
    var reorders = false
    var set: ([String]) -> Void

    @State private var target: String?

    var body: some View {
        FlowRow(spacing: 6, lineSpacing: 6) {
            ForEach(items, id: \.self) { name in chip(name) }
            if !readOnly {
                let left = choices.filter { !items.contains($0) }
                Menu {
                    ForEach(left, id: \.self) { name in
                        Button(label(name).map { "\(name)  \($0)" } ?? name) { set(items + [name]) }
                    }
                } label: {
                    Label(addTitle, systemImage: "plus").font(.app(11))
                }
                .menuStyle(.borderlessButton).fixedSize()
                .padding(.horizontal, 6).padding(.vertical, 2)
                .overlay(RoundedRectangle(cornerRadius: 4).strokeBorder(style: StrokeStyle(lineWidth: 1, dash: [3, 2]))
                    .foregroundStyle(.tertiary))
                .disabled(left.isEmpty)
            }
        }
    }

    private func chip(_ name: String) -> some View {
        HStack(spacing: 4) {
            Text(name).font(.app(11, design: .monospaced))
            if let said = label(name), said != name { Text(said).font(.app(10.5)).foregroundStyle(.secondary) }
            if !readOnly {
                Button { set(items.filter { $0 != name }) } label: { Image(systemName: "xmark").font(.app(8, weight: .bold)) }
                    .buttonStyle(.plain).foregroundStyle(.secondary)
            }
        }
        .padding(.horizontal, 6).padding(.vertical, 2)
        .background(Color.secondary.opacity(0.12), in: RoundedRectangle(cornerRadius: 4))
        .overlay(alignment: .leading) {
            if target == name { Rectangle().fill(Color.accentColor).frame(width: 2).offset(x: -4) }
        }
        .modifier(ChipDrag(name: name, on: reorders && !readOnly, target: $target) { moved in
            guard let from = items.firstIndex(of: moved), let to = items.firstIndex(of: name), from != to else { return }
            var next = items
            next.remove(at: from)
            next.insert(moved, at: to)
            set(next)
        })
    }
}

/// A chip that can be dragged before another of the same list.
private struct ChipDrag: ViewModifier {
    var name: String
    var on: Bool
    @Binding var target: String?
    var dropped: (String) -> Void

    func body(content: Content) -> some View {
        if on {
            content
                .draggable(KindDragItem.chip + name)
                .dropDestination(for: String.self) { items, _ in
                    guard let item = items.first, item.hasPrefix(KindDragItem.chip) else { return false }
                    dropped(String(item.dropFirst(KindDragItem.chip.count)))
                    return true
                } isTargeted: { target = $0 ? name : (target == name ? nil : target) }
        } else {
            content
        }
    }
}

/// What is dragged in the form, as text: what kind of thing, where it is, and its name.
enum KindDragItem {
    static let chip = "specboard.kind.chip:"
    static let item = "specboard.kind.item:"

    /// A field or kind of row dragged among those beside it: they are told apart from those elsewhere.
    static func item(_ place: KindDraft.Place) -> String { item + where_(place.parent) + "|" + (place.isRow ? "r" : "f") + "|" + (place.name ?? "") }

    /// Where a dragged field or kind of row came from, when it was among those of `parent` (`row` for kinds of row).
    static func name(_ dragged: String, under parent: KindDraft.Place, row: Bool) -> String? {
        let head = item + where_(parent) + "|" + (row ? "r" : "f") + "|"
        return dragged.hasPrefix(head) ? String(dragged.dropFirst(head.count)) : nil
    }

    private static func where_(_ place: KindDraft.Place) -> String {
        place.steps.map { step -> String in
            switch step {
            case .field(let name): "f." + name
            case .row(let name): "r." + name
            }
        }.joined(separator: "/")
    }
}

/// The 18 types of field, as the menu that picks one groups them (`specboard://SpecBoard/Docs/Design:CardKinds`
/// 「欄の定義」): each its word, its key and what a value of it is.
enum KindFieldTypes {
    struct Item: Identifiable {
        var key: String
        var symbol: String
        var word: String
        var what: String
        var id: String { key }
    }

    struct Group: Identifiable {
        var title: String
        var items: [Item]
        var id: String { title }
    }

    static var groups: [Group] {
        [
            Group(title: L10n[.Kinds.typeGroupText], items: [
                Item(key: "text", symbol: "textformat", word: L10n[.Kinds.typeText], what: L10n[.Kinds.typeTextWhat]),
                Item(key: "longtext", symbol: "text.alignleft", word: L10n[.Kinds.typeLongtext], what: L10n[.Kinds.typeLongtextWhat]),
                Item(key: "markdown", symbol: "doc.richtext", word: L10n[.Kinds.typeMarkdown], what: L10n[.Kinds.typeMarkdownWhat]),
                Item(key: "url", symbol: "link", word: "URL", what: L10n[.Kinds.typeUrlWhat]),
            ]),
            Group(title: L10n[.Kinds.typeGroupNumber], items: [
                Item(key: "number", symbol: "number", word: L10n[.Kinds.typeNumber], what: ""),
                Item(key: "date", symbol: "calendar", word: L10n[.Kinds.typeDate], what: "YYYY-MM-DD"),
            ]),
            Group(title: L10n[.Kinds.typeGroupChoice], items: [
                Item(key: "checkbox", symbol: "checkmark.square", word: L10n[.Kinds.typeCheckbox], what: L10n[.Kinds.typeCheckboxWhat]),
                Item(key: "radio", symbol: "circle.inset.filled", word: L10n[.Kinds.typeRadio], what: L10n[.Kinds.typeRadioWhat]),
                Item(key: "dropdownlist", symbol: "chevron.up.chevron.down", word: L10n[.Kinds.typeDropdownlist],
                     what: L10n[.Kinds.typeDropdownlistWhat]),
            ]),
            Group(title: L10n[.Kinds.typeGroupWork], items: [
                Item(key: "person", symbol: "person", word: L10n[.Kinds.typePerson], what: ""),
                Item(key: "branch", symbol: "arrow.triangle.branch", word: L10n[.Kinds.typeBranch], what: L10n[.Kinds.typeBranchWhat]),
                Item(key: "mr", symbol: "arrow.triangle.merge", word: "MR", what: L10n[.Kinds.typeMrWhat]),
            ]),
            Group(title: L10n[.Kinds.typeGroupPoints], items: [
                Item(key: "card", symbol: "rectangle", word: L10n[.Kinds.typeCard], what: L10n[.Kinds.typeCardWhat]),
                Item(key: "path", symbol: "folder", word: L10n[.Kinds.typePath], what: L10n[.Kinds.typePathWhat]),
                Item(key: "section", symbol: "square.stack", word: L10n[.Kinds.typeSection], what: L10n[.Kinds.typeSectionWhat]),
            ]),
            Group(title: L10n[.Kinds.typeGroupJSON], items: [
                Item(key: "group", symbol: "curlybraces", word: L10n[.Kinds.typeGroup], what: L10n[.Kinds.typeGroupWhat]),
                Item(key: "json", symbol: "curlybraces.square", word: "JSON", what: L10n[.Kinds.typeJsonWhat]),
                Item(key: "map", symbol: "list.bullet.rectangle", word: L10n[.Kinds.typeMap], what: L10n[.Kinds.typeMapWhat]),
            ]),
        ]
    }

    static func item(_ key: String) -> Item? { groups.lazy.flatMap(\.items).first { $0.key == key } }

    /// The types a line's field may have: a card and a look are not written on a line.
    static let notOnLines: Set<String> = ["card"]
}

/// A field's type, shown with its mark and word, picked from the grouped menu.
struct KindTypeMenu: View {
    var fieldtype: String
    var forLine = false
    var pick: (String) -> Void

    var body: some View {
        let shown = KindFieldTypes.item(fieldtype)
        Menu {
            ForEach(KindFieldTypes.groups) { group in
                Section(group.title) {
                    ForEach(group.items.filter { !forLine || !KindFieldTypes.notOnLines.contains($0.key) }) { item in
                        Button { pick(item.key) } label: {
                            Label(item.what.isEmpty ? "\(item.word)  \(item.key)" : "\(item.word)  \(item.key) — \(item.what)",
                                  systemImage: item.key == fieldtype ? "checkmark" : item.symbol)
                        }
                    }
                }
            }
        } label: {
            Label(shown.map { "\($0.word)  \($0.key)" } ?? fieldtype, systemImage: shown?.symbol ?? "questionmark")
                .font(.app(11.5))
        }
        .fixedSize()
    }
}

/// The marks and colours a kind of section's mark is picked from (`icon: {name, color}`): the SF Symbol from
/// the candidates, the colour from a colour well, each given back (`defaultMark`, when there is one, is what
/// is shown for a part not said).
///
/// A section's own mark (`kindsMarkWord` given) is picked the same way, from a list that starts with its
/// kind's mark — the whole mark taken off, the kind's shown — then the kinds' marks a section is told apart by
/// (an actor's three figures, a chart's) and the candidates.
struct KindMarkPicker: View {
    var mark: SectionMark
    var defaultMark: SectionMark? = nil
    var clears = true
    /// What is said before it; nothing when empty.
    var label: String = L10n[.Kinds.markLabel]
    /// A section's: what its kind's mark is called ("actor · system").
    var kindsMarkWord: String? = nil
    var editable: Bool
    var set: (SectionMark) -> Void

    var body: some View {
        let shown = SectionMark(icon: mark.icon ?? defaultMark?.icon, color: mark.color ?? defaultMark?.color)
        HStack(spacing: 12) {
            if !label.isEmpty {
                Text(label).font(.app(11)).foregroundStyle(.secondary)
            }
            Menu {
                if let kindsMarkWord {
                    Button { set(SectionMark()) } label: {
                        Label(L10n[.Kinds.markKinds(kindsMarkWord)], systemImage: (defaultMark ?? SectionMark()).symbol)
                    }
                    Divider()
                    ForEach(KindsEditing.sectionKindIcons, id: \.symbol) { item in
                        Button {
                            var next = mark
                            next.icon = item.symbol
                            set(next)
                        } label: { Label(item.word, systemImage: item.symbol) }
                    }
                    Divider()
                }
                ForEach(KindsEditing.sectionIcons, id: \.symbol) { item in
                    Button {
                        var next = mark
                        next.icon = item.symbol
                        set(next)
                    } label: { Label(item.word, systemImage: item.symbol) }
                }
                if clears, kindsMarkWord == nil {
                    Divider()
                    Button(defaultMark == nil ? L10n[.Kinds.markUnsetFolder] : L10n[.Kinds.markRestoreBuiltin]) {
                        var next = mark
                        next.icon = nil
                        set(next)
                    }
                }
            } label: {
                Label(menuTitle(shown), systemImage: shown.symbol)
            }
            .fixedSize()
            .disabled(!editable)
            ColorPicker(L10n[.Kinds.markColor], selection: Binding(
                get: { shown.color.flatMap(HexColor.color(hex:)) ?? .gray },
                set: { color in
                    var next = mark
                    next.color = HexColor.hex(color)
                    set(next)
                }), supportsOpacity: false)
                .labelsHidden(kindsMarkWord != nil)
                .fixedSize()
                .disabled(!editable)
            if clears, kindsMarkWord == nil, mark.color != nil {
                Button(defaultMark == nil ? L10n[.Kinds.markClearColor] : L10n[.Kinds.markRestoreColor]) {
                    var next = mark
                    next.color = nil
                    set(next)
                }
                .buttonStyle(.borderless).font(.app(11))
                .disabled(!editable)
            }
            Spacer()
        }
    }

    /// What the menu shows: a section's mark by its word ("Kind's mark" while it has none of its own), a
    /// kind's by the symbol's name.
    private func menuTitle(_ shown: SectionMark) -> String {
        guard kindsMarkWord != nil else { return shown.icon ?? L10n[.Kinds.markUnset] }
        guard let icon = mark.icon else { return L10n[.Kinds.markKindsShort] }
        return KindsEditing.word(for: icon) ?? icon
    }
}

private extension View {
    /// The label of a control taken off where `hidden`.
    @ViewBuilder
    func labelsHidden(_ hidden: Bool) -> some View {
        if hidden { labelsHidden() } else { self }
    }
}

/// A number written in a field: none when it is left empty.
struct KindNumberField: View {
    var value: Double?
    var placeholder: String
    var width: CGFloat = 70
    var set: (Double?) -> Void

    var body: some View {
        CommitField(placeholder, text: value.map(Self.said) ?? "") { text in
            if text.isEmpty { set(nil) } else if let number = Double(text) { set(number) }
        }
        .frame(width: width)
    }

    static func said(_ value: Double) -> String {
        value == value.rounded() && abs(value) < 1e15 ? String(Int(value)) : String(value)
    }
}

extension OJSON {
    /// The number this is, if it is one.
    var numberOrNil: Double? { if case .number(let n) = self { n } else { nil } }

    /// The texts this list holds, if it is one.
    var stringsOrNil: [String]? {
        guard case .array(let items) = self else { return nil }
        return items.compactMap(\.stringOrNil)
    }

    var objectOrNil: OrderedObject? { if case .object(let o) = self { o } else { nil } }
}
