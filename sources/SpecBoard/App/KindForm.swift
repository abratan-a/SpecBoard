import SwiftUI

/// A definition put together from parts rather than written as JSON (`specboard://SpecBoard/Docs/Design:UI`, the
/// Kinds tab; `Docs/Features/Project:ProjectKinds` formGroups): its foldable groups, top to bottom — a kind of card's
/// basics, fields, rows, look, placement and usage; a kind of section's basics, holds, where it sits, its mark,
/// how it opens and usage; a kind of line's basics, meaning, look, from → to, how diagrams draw it, its fields,
/// export and usage. Each group folded says in a line what is in it. Every part writes the one draft the JSON
/// writes too (`KindDraft`); what no part writes is said where it is and left to the JSON.
struct KindForm: View {
    var context: KindFormContext
    var list: AppModel.KindList
    var entries: [AppModel.KindEntry]
    /// The saved definition picked; none for a new one.
    var entry: AppModel.KindEntry?
    /// Whether a section kind may be told to hold this kind: it is saved, and not changed since.
    var canHold: Bool
    /// The kinds of row a kind of card has, by its name: what a line's end may be bound to.
    var rowKinds: (String) -> [String]
    @Binding var selection: KindDraft.Place?
    /// The preview, as the first group when there is no room for it beside the form.
    var preview: AnyView?
    var previewSummary: String = ""
    /// A built-in kind's usage and mark, each written over with its own button.
    var builtinUsage: AnyView?
    var builtinMark: AnyView?
    var holdIn: (String?) -> Void
    var openProjectTree: () -> Void
    /// The kinds of section that hold any kind (`*`) and that a section of the tree open is of: they hold the
    /// project's own kinds too (`KindForm.anyHolders`).
    var anyHolders: [String] = []

    @AppStorage(KindFormMemory.openKey) private var openSaid = KindFormMemory.standardOpen

    private var draft: KindDraft { context.draft }
    private var readOnly: Bool { context.readOnly }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            if let preview {
                group("preview", shared: true, title: L10n[.Kinds.formGroupPreview], summary: previewSummary) { preview }
            }
            switch list {
            case .cards: cardGroups
            case .sections: sectionGroups
            case .lines: lineGroups
            }
        }
    }

    // MARK: - Groups

    private func id(_ name: String, shared: Bool = false) -> String { shared ? name : "\(list.of).\(name)" }

    private func group<Content: View>(_ name: String, shared: Bool = false, title: String, count: Int? = nil, summary: String,
                                      badge: String? = nil, @ViewBuilder content: @escaping () -> Content) -> some View {
        let key = id(name, shared: shared)
        let open = KindFormMemory.groups(openSaid).contains(key)
        return KindFormGroup(title: title, count: count, summary: summary, badge: badge, open: open, toggle: {
            var groups = KindFormMemory.groups(openSaid)
            if groups.contains(key) { groups.remove(key) } else { groups.insert(key) }
            openSaid = KindFormMemory.said(groups)
        }, content: content)
    }

    private func text(_ key: String, _ placeholder: String, monospaced: Bool = false, width: CGFloat? = nil) -> some View {
        CommitField(placeholder, text: draft.string(key) ?? "", monospaced: monospaced) { written in
            context.edit { $0.set(.string(written), at: .kind, key) }
        }
        .frame(width: width)
        .disabled(readOnly)
    }

    /// The definition's name: written while it is new; after, a kind of card is renamed with
    /// 「名前を変える…」 and the others keep theirs.
    private var nameLine: some View {
        KindFormLine(label: L10n[.Common.wordName]) {
            if entry == nil, !readOnly {
                text(list.nameKey, L10n[.Common.wordName], monospaced: true, width: 160)
                Text(L10n[.Kinds.formNameHelpNew]).font(.app(10.5)).foregroundStyle(.secondary)
            } else {
                Text(draft.name).font(.app(12, design: .monospaced)).textSelection(.enabled)
                if !readOnly {
                    Text(list == .cards ? L10n[.Kinds.formNameHelpSaved] : L10n[.Kinds.formNameKept])
                        .font(.app(10.5)).foregroundStyle(.secondary)
                }
            }
        }
    }

    @ViewBuilder
    private var otherKeysLine: some View {
        let others = draft.otherKeys()
        if !others.isEmpty {
            KindFormLine(label: L10n[.Kinds.formOtherKeys]) {
                KindJsonOnly(said: others.joined(separator: ", "), toJSON: context.toJSON)
                    .help(L10n[.Kinds.formOtherKeysHelp])
            }
        }
    }

    private var usageSummary: String {
        let purpose = draft.string("context", "purpose") ?? draft.string("description") ?? ""
        return purpose.isEmpty ? "" : L10n[.Kinds.formSummaryPurpose(purpose)]
    }

    @ViewBuilder
    private var usageGroup: some View {
        group("usage", title: L10n[.Kinds.formGroupUsage], summary: usageSummary) {
            if let builtinUsage {
                builtinUsage
            } else {
                VStack(alignment: .leading, spacing: 6) {
                    ForEach([("purpose", L10n[.Kinds.contextPurpose]), ("use_when", L10n[.Kinds.contextUseWhen]),
                             ("not_for", L10n[.Kinds.contextNotFor])], id: \.0) { key, word in
                        KindFormLine(label: word) {
                            CommitField(word, text: draft.string("context", key) ?? "") { written in
                                context.edit { $0.set(.string(written), at: .kind, "context", key) }
                            }
                            .disabled(readOnly)
                        }
                    }
                    Text(L10n[.Kinds.formUsageHelp]).font(.app(10.5)).foregroundStyle(.secondary).padding(.leading, 114)
                }
            }
        }
    }

    // MARK: - A kind of card

    private var ownCardKinds: [AppModel.KindEntry] { entries.filter { $0.list == .cards && !$0.builtin } }

    @ViewBuilder
    private var cardGroups: some View {
        group("basics", title: L10n[.Kinds.formGroupBasics], summary: cardBasicsSummary) {
            VStack(alignment: .leading, spacing: 7) {
                nameLine
                KindFormLine(label: L10n[.Common.wordDisplayName]) { text("label", L10n[.Common.wordDisplayName], width: 160) }
                KindFormLine(label: L10n[.Kinds.formExtends]) {
                    let parents = ["base"] + ownCardKinds.map(\.name).filter { $0 != draft.name && $0 != entry?.name }
                    Picker("", selection: Binding(get: { draft.string("extends") ?? "base" },
                                                  set: { said in context.edit { $0.set(.string(said), at: .kind, "extends") } })) {
                        ForEach(Array(Set(parents + [draft.string("extends") ?? "base"])).sorted { a, b in
                            (parents.firstIndex(of: a) ?? .max, a) < (parents.firstIndex(of: b) ?? .max, b)
                        }, id: \.self) { Text($0).tag($0) }
                    }
                    .labelsHidden().fixedSize().disabled(readOnly)
                    Text(L10n[.Kinds.formExtendsHelp]).font(.app(10.5)).foregroundStyle(.secondary)
                }
                KindFormLine(label: L10n[.Kinds.formConnectable]) {
                    Toggle(L10n[.Kinds.formConnectableHelp], isOn: Binding(
                        get: { draft.flag("connectable") },
                        set: { on in context.edit { $0.set(.bool(on), at: .kind, "connectable") } }))
                        .toggleStyle(.checkbox).font(.app(11.5)).disabled(readOnly)
                }
                otherKeysLine
            }
        }
        let fields = draft.fieldNames()
        group("fields", title: L10n[.Kinds.formGroupFields], count: fields.count, summary: fields.joined(separator: " · ")) {
            VStack(alignment: .leading, spacing: 6) {
                Text(L10n[.Kinds.formFieldsHelp]).font(.app(10.5)).foregroundStyle(.secondary)
                KindFieldsTable(context: context, parent: .kind, selection: $selection)
            }
        }
        let rows = draft.rowNames()
        group("rows", title: L10n[.Kinds.formGroupRows], count: rows.count,
              summary: rows.isEmpty ? L10n[.Common.wordNone] : rows.joined(separator: " · ")) {
            VStack(alignment: .leading, spacing: 6) {
                Text(L10n[.Kinds.formRowsHelp]).font(.app(10.5)).foregroundStyle(.secondary)
                KindRowsTable(context: context, selection: $selection)
            }
        }
        group("look", title: L10n[.Kinds.formGroupLook], summary: lookSummary) { cardLook }
        let place = placement
        group("placement", title: L10n[.Kinds.formGroupPlacement],
              summary: place.said.isEmpty ? L10n[.Kinds.formSummaryNoPlace] : L10n[.Kinds.formSummarySections(place.said.joined(separator: ", "))],
              badge: place.none ? L10n[.Kinds.rowNoPlace] : nil) { cardPlacement(place) }
        usageGroup
    }

    private var cardBasicsSummary: String {
        var parts = [draft.name]
        if let label = draft.string("label"), !label.isEmpty { parts.append(label) }
        parts.append(L10n[.Kinds.formSummaryExtends(draft.string("extends") ?? "base")])
        if draft.flag("connectable") { parts.append(L10n[.Kinds.formSummaryConnectable]) }
        return parts.joined(separator: " · ")
    }

    // MARK: Look

    /// The colours offered beside the colour well.
    static let swatches = ["#C2410C", "#DC2626", "#D97706", "#16A34A", "#0E7490", "#2563EB", "#7C3AED", "#6E7781"]

    /// The types of field a face's `top` draws as a control (the core's `FACE_TOP`).
    static let faceTop: Set<String> = ["checkbox", "radio", "dropdownlist", "date", "text", "section"]

    private func fields(where take: (String) -> Bool) -> [String] {
        draft.fieldNames().filter { take(draft.fieldtype(at: .field($0))) && draft.many(at: .field($0)) == 0 }
    }

    private var lookSummary: String {
        var parts: [String] = []
        if let color = draft.string("color") { parts.append(color) }
        let header = draft.strings("header")
        if !header.isEmpty { parts.append(L10n[.Kinds.formSummaryHeader(header.joined(separator: ", "))]) }
        parts.append(L10n[.Kinds.formSummaryOnMap(draft.value(at: .kind, "face") == nil ? L10n[.Kinds.formOnMapRows] : L10n[.Kinds.formOnMapControls])])
        return parts.joined(separator: " · ")
    }

    private func fieldLabel(_ name: String) -> String? {
        name == "label" ? L10n[.Common.wordDisplayName] : draft.string(at: .field(name), "label")
    }

    @ViewBuilder
    private var cardLook: some View {
        VStack(alignment: .leading, spacing: 8) {
            KindFormLine(label: L10n[.Kinds.markColor]) {
                HStack(spacing: 6) {
                    ForEach(Self.swatches, id: \.self) { hex in
                        Button { context.edit { $0.set(.string(hex), at: .kind, "color") } } label: {
                            Circle().fill(HexColor.color(hex: hex) ?? .gray).frame(width: 16, height: 16)
                                .overlay(Circle().stroke(Color.primary, lineWidth: draft.string("color")?.uppercased() == hex ? 2 : 0).padding(-3))
                        }
                        .buttonStyle(.plain)
                    }
                    ColorPicker("", selection: Binding(
                        get: { draft.string("color").flatMap(HexColor.color(hex:)) ?? .gray },
                        set: { color in if let hex = HexColor.hex(color) { context.edit { $0.set(.string(hex), at: .kind, "color") } } }),
                                supportsOpacity: false)
                        .labelsHidden().fixedSize()
                    text("color", "#RRGGBB", monospaced: true, width: 90)
                }
                .disabled(readOnly)
            }
            KindFormLine(label: L10n[.Kinds.formHeader]) {
                KindChips(items: draft.strings("header"), label: fieldLabel, choices: draft.fieldNames(),
                          addTitle: L10n[.Kinds.formFieldChip], readOnly: readOnly, reorders: true) { names in
                    context.edit { $0.set(OJSON.string(names), at: .kind, "header") }
                }
                Text(L10n[.Kinds.formHeaderHelp]).font(.app(10.5)).foregroundStyle(.secondary)
            }
            faceLines
            asRowLines
            opensLine
            KindFormLine(label: L10n[.Kinds.formDoubleClick]) {
                Picker("", selection: Binding(get: { draft.string("on_double_click") ?? "" },
                                              set: { said in context.edit { $0.set(.string(said), at: .kind, "on_double_click") } })) {
                    Text(L10n[.Kinds.formOpenCard]).tag("")
                    Text(L10n[.Kinds.formEditInPlace]).tag("edit_in_place")
                    if let other = draft.string("on_double_click"), other != "edit_in_place" { Text(other).tag(other) }
                }
                .labelsHidden().fixedSize().disabled(readOnly)
            }
        }
    }

    /// How the map draws the card: as rows (no `face`), or with controls (`face.top`) and Markdown below
    /// (`face.markdown`); a face that draws a page (`svg`, `html`) is the JSON's.
    @ViewBuilder
    private var faceLines: some View {
        let face = draft.value(at: .kind, "face")?.objectOrNil
        let controls = fields { Self.faceTop.contains($0) }
        if let face, face["svg"] != nil || face["html"] != nil {
            KindFormLine(label: L10n[.Kinds.formOnMap]) {
                KindJsonOnly(said: L10n[.Kinds.formOnMapPage], toJSON: context.toJSON)
            }
        } else {
            KindFormLine(label: L10n[.Kinds.formOnMap]) {
                Picker("", selection: Binding(get: { face != nil }, set: { on in
                    context.edit { draft in
                        if on {
                            if let first = controls.first { draft.set(OJSON.string([first]), at: .kind, "face", "top") }
                        } else {
                            draft.set(nil, at: .kind, "face")
                        }
                    }
                })) {
                    Text(L10n[.Kinds.formOnMapRows]).tag(false)
                    Text(L10n[.Kinds.formOnMapControls]).tag(true)
                }
                .pickerStyle(.segmented).labelsHidden().fixedSize().disabled(readOnly || (face == nil && controls.isEmpty))
                Text(L10n[.Kinds.formOnMapHelp]).font(.app(10.5)).foregroundStyle(.secondary)
            }
            if face != nil {
                KindFormLine(label: L10n[.Kinds.formControls]) {
                    VStack(alignment: .leading, spacing: 3) {
                        KindChips(items: draft.strings("face", "top"), label: fieldLabel, choices: controls,
                                  addTitle: L10n[.Kinds.formFieldChip], readOnly: readOnly, reorders: true) { names in
                            context.edit { $0.set(OJSON.string(names), at: .kind, "face", "top") }
                        }
                        Text(L10n[.Kinds.formControlsHelp]).font(.app(10.5)).foregroundStyle(.secondary)
                    }
                }
                KindFormLine(label: L10n[.Kinds.formMarkdownBelow]) {
                    let texts = fields { $0 == "longtext" || $0 == "markdown" }
                    Picker("", selection: Binding(get: { draft.string("face", "markdown") ?? "" },
                                                  set: { said in context.edit { $0.set(.string(said), at: .kind, "face", "markdown") } })) {
                        Text(L10n[.Common.wordNone]).tag("")
                        ForEach(Array(Set(texts + [draft.string("face", "markdown")].compactMap { $0 })).sorted(), id: \.self) { Text($0).tag($0) }
                    }
                    .labelsHidden().fixedSize().disabled(readOnly)
                    Text(L10n[.Kinds.formMarkdownBelowHelp]).font(.app(10.5)).foregroundStyle(.secondary)
                }
            }
        }
    }

    /// What a card of the kind says as a row of a container it is in (`as_row`): its name only (not
    /// written), the fields named, or what it holds unfolded.
    @ViewBuilder
    private var asRowLines: some View {
        let asRow = draft.value(at: .kind, "as_row")
        let mode = asRow?.stringOrNil == "unfold" ? "unfold" : asRow?.stringsOrNil != nil ? "fields" : ""
        KindFormLine(label: L10n[.Kinds.formInContainer]) {
            VStack(alignment: .leading, spacing: 4) {
                Picker("", selection: Binding(get: { mode }, set: { said in
                    context.edit { draft in
                        switch said {
                        case "unfold": draft.set(.string("unfold"), at: .kind, "as_row")
                        case "fields": draft.set(OJSON.string([draft.fieldNames().first ?? "label"]), at: .kind, "as_row")
                        default: draft.set(nil, at: .kind, "as_row")
                        }
                    }
                })) {
                    Text(L10n[.Kinds.formInContainerName]).tag("")
                    Text(L10n[.Kinds.formInContainerFields]).tag("fields")
                    Text(L10n[.Kinds.formInContainerUnfold]).tag("unfold")
                }
                .pickerStyle(.segmented).labelsHidden().fixedSize().disabled(readOnly)
                if mode == "fields" {
                    KindChips(items: asRow?.stringsOrNil ?? [], label: fieldLabel, choices: ["label"] + draft.fieldNames(),
                              addTitle: L10n[.Kinds.formFieldChip], readOnly: readOnly, reorders: true) { names in
                        context.edit { $0.set(OJSON.string(names), at: .kind, "as_row") }
                    }
                }
            }
        }
    }

    /// How a card of the kind opens (`opens`): its fields and rows, its body as Markdown, or — holding
    /// cards — its contents map; what cannot be is offered faintly, with why.
    @ViewBuilder
    private var opensLine: some View {
        let container = !draft.strings("holds").isEmpty
        let hasBody = draft.fieldNames().contains("body")
            && ["longtext", "markdown"].contains(draft.fieldtype(at: .field("body")))
        let opens = draft.string("opens") ?? (container ? KindDraft.cardMap : "fields")
        KindFormLine(label: L10n[.Kinds.formOpens]) {
            Menu {
                Button(L10n[.Board.modeFieldsAndRows]) { context.edit { $0.set(nil, at: .kind, "opens") } }.disabled(container)
                Button(L10n[.Kinds.formOpensBody]) { context.edit { $0.set(.string("markdown"), at: .kind, "opens") } }
                    .disabled(container || !hasBody)
                Button(L10n[.Board.modeCardMap]) { context.edit { $0.set(.string(KindDraft.cardMap), at: .kind, "opens") } }
                    .disabled(!container)
            } label: {
                Text(opens == "markdown" ? L10n[.Kinds.formOpensBody] : opens == KindDraft.cardMap ? L10n[.Board.modeCardMap]
                     : L10n[.Board.modeFieldsAndRows]).font(.app(11.5))
            }
            .fixedSize().disabled(readOnly)
            Text(container ? L10n[.Kinds.formOpensHoldsOnlyMap] : !hasBody ? L10n[.Kinds.formOpensBodyNeeds] : L10n[.Kinds.formOpensMapNeeds])
                .font(.app(10.5)).foregroundStyle(.secondary).lineLimit(2)
        }
    }

    // MARK: Placement

    struct Placement: Equatable {
        /// The kinds of section that name it in what they hold.
        var sections: [String]
        /// Those that hold any kind (`*`) and so hold it too, not naming it.
        var anyKind: [String] = []
        var containers: [String]
        /// No section of the project's tree can hold it.
        var none: Bool

        /// The kinds of section that hold it, as Placement says them: those that name it, then those that
        /// hold any kind, said so.
        var said: [String] { sections + anyKind.map { L10n[.Kinds.formHoldsAnyKind($0)] } }
    }

    /// The kinds of section in `entries` that hold any kind (`*`) and that one of `inTree` — the kinds of the
    /// sections of the tree open, the one sections taken out of the tree go into left out — is of.
    static func anyHolders(_ entries: [AppModel.KindEntry], inTree: Set<String>) -> [String] {
        entries.filter { $0.list == .sections && inTree.contains($0.name) && holds($0).contains("*") }.map(\.name)
    }

    /// Where a card of the kind `name` may be put, as `kind_list` says it: the kinds of section and of container
    /// that hold it by its name, and the kinds of section in the tree that hold any kind (`anyHolders`), which
    /// hold one of the project's own. A built-in kind is not counted as held by any kind: some are put only where
    /// they are named. `noPlace`: what the core said of the saved kind, which counts both.
    static func placement(of name: String, builtin: Bool, noPlace: Bool?, entries: [AppModel.KindEntry],
                          anyHolders: [String]) -> Placement {
        let sections = entries.filter { $0.list == .sections && holds($0).contains(name) }.map(\.name)
        let anyKind = builtin ? [] : anyHolders.filter { !sections.contains($0) }
        let containers = entries.filter { $0.list == .cards && holds($0).contains(name) }.map(\.name)
        return Placement(sections: sections, anyKind: anyKind, containers: containers,
                         none: noPlace ?? (sections.isEmpty && anyKind.isEmpty))
    }

    /// What a definition says it holds (`holds`).
    static func holds(_ entry: AppModel.KindEntry) -> [String] {
        guard let read = OJSON.parse(entry.text), case .object(let o) = read else { return [] }
        return o["holds"]?.stringsOrNil ?? []
    }

    /// Where a card of this kind may be put, as `kind_list` says it: the kinds of section and of
    /// container that hold it by its name.
    private var placement: Placement {
        Self.placement(of: entry?.name ?? draft.name, builtin: entry?.builtin == true, noPlace: entry?.noPlace,
                       entries: entries, anyHolders: anyHolders)
    }

    private func cardPlacement(_ place: Placement) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            if place.none {
                HStack(alignment: .firstTextBaseline, spacing: 8) {
                    KindTag(text: L10n[.Kinds.rowNoPlace], tint: .orange)
                    Text(L10n[.Kinds.formNoPlaceSaid(draft.name)]).font(.app(11)).foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                .padding(.leading, 114)
            }
            KindFormLine(label: L10n[.Kinds.formPlaceSections]) {
                if place.said.isEmpty {
                    Text(L10n[.Common.wordNone]).font(.app(11.5)).foregroundStyle(.secondary)
                } else {
                    Text(place.said.joined(separator: ", ")).font(.app(11.5, design: .monospaced))
                }
                if !readOnly { holdItMenu }
            }
            if !place.containers.isEmpty {
                KindFormLine(label: L10n[.Kinds.formPlaceContainers]) {
                    Text(place.containers.joined(separator: ", ")).font(.app(11.5, design: .monospaced))
                }
            }
            KindFormLine(label: L10n[.Kinds.formHolds]) {
                VStack(alignment: .leading, spacing: 3) {
                    KindChips(items: draft.strings("holds"), label: context.label(ofCard:),
                              choices: context.cardKinds.map(\.name), addTitle: L10n[.Kinds.formKindChip], readOnly: readOnly) { kinds in
                        context.edit { $0.setHolds(kinds) }
                    }
                    Text(L10n[.Kinds.formHoldsHelp]).font(.app(10.5)).foregroundStyle(.secondary)
                }
            }
        }
    }

    /// The way to a kind of section's definition with this kind added to what it holds: one definition is
    /// saved at a time, so the other is opened to be saved on its own — once this one is saved.
    private var holdItMenu: some View {
        let own = entries.filter { $0.list == .sections && !$0.builtin }
        return HStack(spacing: 6) {
            Menu {
                Section(L10n[.Kinds.formHoldItOwn]) {
                    ForEach(own) { section in
                        let holds = Self.holds(section)
                        Button("\(section.name)  \(section.label) · \(L10n[.Kinds.formHoldsSaid(holds.joined(separator: ", "))])") {
                            holdIn(section.name)
                        }
                        .disabled(holds.contains(draft.name))
                    }
                }
                Divider()
                Button(L10n[.Kinds.formHoldItNew]) { holdIn(nil) }
                Divider()
                Text(L10n[.Kinds.formHoldItBuiltin])
            } label: {
                Label(L10n[.Kinds.formHoldIt], systemImage: "plus").font(.app(11))
            }
            .menuStyle(.borderlessButton).fixedSize()
            .disabled(!canHold)
            if !canHold {
                Text(L10n[.Kinds.formHoldItSaveFirst]).font(.app(10.5)).foregroundStyle(.secondary)
            }
        }
    }

    // MARK: - A kind of section

    @ViewBuilder
    private var sectionGroups: some View {
        group("basics", title: L10n[.Kinds.formGroupBasics],
              summary: [draft.name, draft.string("label"), draft.string("description")].compactMap { $0 }.filter { !$0.isEmpty }
                  .joined(separator: " · ")) {
            VStack(alignment: .leading, spacing: 7) {
                nameLine
                KindFormLine(label: L10n[.Common.wordDisplayName]) { text("label", L10n[.Common.wordDisplayName], width: 160) }
                KindFormLine(label: L10n[.Common.wordDescription]) { text("description", L10n[.Common.wordDescription]) }
                otherKeysLine
            }
        }
        let holds = draft.strings("holds")
        group("holds", title: L10n[.Kinds.formGroupHolds], summary: holds.joined(separator: " · ")) {
            VStack(alignment: .leading, spacing: 7) {
                KindFormLine(label: L10n[.Kinds.formCards]) {
                    KindChips(items: holds.filter { $0 != "*" }, label: context.label(ofCard:), choices: context.cardKinds.map(\.name),
                              addTitle: L10n[.Kinds.formKindChip], readOnly: readOnly) { kinds in
                        context.edit { $0.setHolds(kinds + (holds.contains("*") ? ["*"] : [])) }
                    }
                }
                KindFormLine(label: "") {
                    Toggle(L10n[.Kinds.formAnyKind], isOn: Binding(get: { holds.contains("*") }, set: { on in
                        context.edit { $0.setHolds(holds.filter { $0 != "*" } + (on ? ["*"] : [])) }
                    }))
                    .toggleStyle(.checkbox).font(.app(11.5)).disabled(readOnly)
                    Text(L10n[.Kinds.formAnyKindHelp]).font(.app(10.5)).foregroundStyle(.secondary)
                }
                KindFormLine(label: L10n[.Kinds.formSectionsInside]) {
                    let sections = entries.filter { $0.list == .sections }
                    KindChips(items: draft.strings("sections"), label: { name in sections.first { $0.name == name }?.label },
                              choices: Array(Set(sections.map(\.name) + [draft.name])).sorted(), addTitle: L10n[.Kinds.formKindChip],
                              readOnly: readOnly) { names in
                        context.edit { $0.set(OJSON.string(names), at: .kind, "sections") }
                    }
                    Text(L10n[.Kinds.formSectionsInsideNone]).font(.app(10.5)).foregroundStyle(.secondary)
                }
            }
        }
        let trees = draft.value(at: .kind, "trees")?.stringsOrNil ?? ["main", "board"]
        group("sits", title: L10n[.Kinds.formGroupSits], summary: sitsSummary(trees)) {
            VStack(alignment: .leading, spacing: 6) {
                KindFormLine(label: L10n[.Kinds.formDirectlyUnder]) {
                    ForEach([("main", L10n[.Kinds.formUnderMain]), ("board", L10n[.Kinds.formUnderBoard])], id: \.0) { tree, word in
                        Toggle(word, isOn: Binding(get: { trees.contains(tree) }, set: { on in
                            let next = ["main", "board"].filter { $0 == tree ? on : trees.contains($0) }
                            context.edit { $0.set(OJSON.string(next), at: .kind, "trees") }
                        }))
                        .toggleStyle(.checkbox).font(.app(11.5)).disabled(readOnly)
                    }
                }
                HStack(spacing: 8) {
                    Text(L10n[.Kinds.formNeitherHelp]).font(.app(10.5)).foregroundStyle(.secondary)
                    Button(L10n[.Kinds.formProjectTreeLink], action: openProjectTree).buttonStyle(.link).font(.app(10.5))
                }
                .padding(.leading, 114)
            }
        }
        let mark = KindsEditing.mark(in: draft.text)
        group("mark", title: L10n[.Kinds.markLabel],
              summary: [mark.icon, mark.color].compactMap { $0 }.joined(separator: " · ")) {
            if let builtinMark {
                builtinMark
            } else {
                KindMarkPicker(mark: mark, editable: !readOnly) { next in context.edit { $0.setMark(next) } }
            }
        }
        group("opens", title: L10n[.Kinds.formOpens], summary: L10n[.Kinds.formOpensSectionMap]) {
            Text(L10n[.Kinds.formOpensSectionMap]).font(.app(11.5)).foregroundStyle(.secondary).padding(.leading, 114)
        }
        usageGroup
    }

    private func sitsSummary(_ trees: [String]) -> String {
        let words = trees.compactMap { $0 == "main" ? L10n[.Kinds.formUnderMain] : $0 == "board" ? L10n[.Kinds.formUnderBoard] : nil }
        return words.isEmpty ? L10n[.Kinds.formNeitherHelp] : words.joined(separator: " · ")
    }

    // MARK: - A kind of line

    @ViewBuilder
    private var lineGroups: some View {
        group("basics", title: L10n[.Kinds.formGroupBasics],
              summary: [draft.name, draft.string("label"), draft.string("description")].compactMap { $0 }.filter { !$0.isEmpty }
                  .joined(separator: " · ")) {
            VStack(alignment: .leading, spacing: 7) {
                nameLine
                KindFormLine(label: L10n[.Common.wordDisplayName]) { text("label", L10n[.Common.wordDisplayName], width: 160) }
                KindFormLine(label: L10n[.Common.wordDescription]) { text("description", L10n[.Common.wordDescription]) }
                otherKeysLine
            }
        }
        let base = draft.string("base") ?? "link"
        group("meaning", title: L10n[.Kinds.formGroupMeaning], summary: Self.baseWord(base)) {
            VStack(alignment: .leading, spacing: 4) {
                KindFormLine(label: L10n[.Kinds.formBase]) {
                    Picker("", selection: Binding(get: { base }, set: { said in context.edit { $0.set(.string(said), at: .kind, "base") } })) {
                        ForEach(["embed", "designate", "link"], id: \.self) { Text(Self.baseWord($0)).tag($0) }
                    }
                    .pickerStyle(.segmented).labelsHidden().fixedSize().disabled(readOnly)
                }
                Text(L10n[.Kinds.formBaseHelp]).font(.app(10.5)).foregroundStyle(.secondary).padding(.leading, 114)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        let look = lineLook
        group("look", title: L10n[.Kinds.formGroupLook],
              summary: "\(look.ink.rawValue) · \(Self.dashWord(look.dash)) · \(Self.capWord(look.begin)) → \(Self.capWord(look.end))") {
            lineLookLines(look)
        }
        group("ends", title: L10n[.Kinds.formGroupEnds], summary: endsSummary) { endsLines }
        let visible = draft.value(at: .kind, "visible").map { KindDraft.same($0, .bool(true)) } ?? true
        group("diagrams", title: L10n[.Kinds.formGroupDiagrams],
              summary: visible ? L10n[.Kinds.formDrawAtFirst] : L10n[.Kinds.formDrawAtFirstOff]) {
            KindFormLine(label: L10n[.Kinds.formDrawAtFirst]) {
                Toggle("", isOn: Binding(get: { visible }, set: { on in context.edit { $0.set(.bool(on), at: .kind, "visible") } }))
                    .toggleStyle(.switch).labelsHidden().controlSize(.small).disabled(readOnly)
                if !visible { Text(L10n[.Kinds.formDrawAtFirstOff]).font(.app(10.5)).foregroundStyle(.secondary) }
            }
        }
        let fields = draft.fieldNames()
        group("fields", title: L10n[.Kinds.formGroupFields], count: fields.count,
              summary: fields.isEmpty ? L10n[.Kinds.formLineFieldsHelp] : fields.joined(separator: " · ")) {
            VStack(alignment: .leading, spacing: 6) {
                Text(L10n[.Kinds.formLineFieldsHelp]).font(.app(10.5)).foregroundStyle(.secondary)
                KindFieldsTable(context: context, parent: .kind, forLine: true, selection: $selection)
            }
        }
        group("export", title: L10n[.Kinds.formGroupExport],
              summary: L10n[.Kinds.formSummaryExportAs(draft.string("export", "as") ?? "none")]) {
            VStack(alignment: .leading, spacing: 6) {
                KindFormLine(label: L10n[.Kinds.formExportAs]) {
                    Picker("", selection: Binding(get: { draft.string("export", "as") ?? "" },
                                                  set: { said in context.edit { $0.set(.string(said), at: .kind, "export", "as") } })) {
                        Text(L10n[.Common.wordNone]).tag("")
                        ForEach(["none", "comment", "annotation", "schema"], id: \.self) { Text($0).tag($0) }
                    }
                    .labelsHidden().fixedSize().disabled(readOnly)
                }
                KindFormLine(label: L10n[.Kinds.formExportKey]) {
                    CommitField("x-…", text: draft.string("export", "key") ?? "", monospaced: true) { written in
                        context.edit { $0.set(.string(written), at: .kind, "export", "key") }
                    }
                    .frame(width: 160).disabled(readOnly)
                }
                KindFormLine(label: L10n[.Kinds.formExportWord]) {
                    CommitField(draft.name, text: draft.string("export", "word") ?? "", monospaced: true) { written in
                        context.edit { $0.set(.string(written), at: .kind, "export", "word") }
                    }
                    .frame(width: 160).disabled(readOnly)
                }
            }
        }
        usageGroup
    }

    private var lineLook: LineLook {
        guard let look = draft.value(at: .kind, "look"),
              let read = try? JSONDecoder().decode(LineLook.self, from: Data(look.compact().utf8)) else { return LineLook() }
        return read
    }

    /// What a meaning is called: the label of the built-in kind of line that means it.
    static func baseWord(_ base: String) -> String {
        switch base {
        case "embed": BuiltinWords.line(Line.embed)
        case "designate": BuiltinWords.line(Line.points)
        case "link": BuiltinWords.line(Line.link)
        default: base
        }
    }

    static func dashWord(_ dash: EdgeDash) -> String {
        switch dash {
        case .solid: L10n[.Kinds.formDashSolid]
        case .dashed: L10n[.Kinds.formDashDashed]
        case .dotted: L10n[.Kinds.formDashDotted]
        }
    }

    static func capWord(_ cap: EdgeCap) -> String {
        switch cap {
        case .none: L10n[.Common.wordNone]
        case .arrow: L10n[.Kinds.formCapArrow]
        case .triangle: L10n[.Kinds.formCapTriangle]
        case .openDiamond: L10n[.Kinds.formCapOpenDiamond]
        case .filledDiamond: L10n[.Kinds.formCapFilledDiamond]
        }
    }

    private func lineLookLines(_ look: LineLook) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            KindFormLine(label: L10n[.Kinds.markColor]) {
                HStack(spacing: 6) {
                    ForEach(EdgeInk.allCases, id: \.self) { ink in
                        Button { context.edit { $0.set(.string(ink.rawValue), at: .kind, "look", "color") } } label: {
                            Circle().fill(Color(nsColor: ink.color(dark: false))).frame(width: 16, height: 16)
                                .overlay(Circle().stroke(Color.primary, lineWidth: look.ink == ink ? 2 : 0).padding(-3))
                        }
                        .buttonStyle(.plain)
                    }
                    Text(look.ink.rawValue).font(.app(11)).foregroundStyle(.secondary)
                }
                .disabled(readOnly)
            }
            KindFormLine(label: L10n[.Kinds.formDash]) {
                Picker("", selection: Binding(get: { look.dash }, set: { dash in
                    context.edit { $0.set(.string(dash.rawValue), at: .kind, "look", "dash") }
                })) {
                    ForEach(EdgeDash.allCases, id: \.self) { Text(Self.dashWord($0)).tag($0) }
                }
                .pickerStyle(.segmented).labelsHidden().fixedSize().disabled(readOnly)
            }
            KindFormLine(label: L10n[.Kinds.formEnds]) {
                capMenu(L10n[.Kinds.formEndStart], look.begin, key: "begin")
                capMenu(L10n[.Kinds.formEndEnd], look.end, key: "end")
                LineSample(look: look)
            }
        }
    }

    private func capMenu(_ word: String, _ cap: EdgeCap, key: String) -> some View {
        HStack(spacing: 4) {
            Text(word).font(.app(11)).foregroundStyle(.secondary)
            Picker("", selection: Binding(get: { cap }, set: { next in
                context.edit { $0.set(.string(next.rawValue), at: .kind, "look", key) }
            })) {
                ForEach([EdgeCap.none, .arrow, .triangle, .openDiamond, .filledDiamond], id: \.self) { Text(Self.capWord($0)).tag($0) }
            }
            .labelsHidden().fixedSize().disabled(readOnly)
        }
    }

    // MARK: From → To

    /// Where a line leaves (`from`, a list) and where it arrives (`to`, one), each as written.
    private var froms: [OrderedObject] {
        guard case .array(let items)? = draft.value(at: .kind, "from") else { return [] }
        return items.compactMap(\.objectOrNil)
    }

    private var to: OrderedObject? { draft.value(at: .kind, "to")?.objectOrNil }

    private var endsSummary: String {
        func kinds(_ end: OrderedObject?) -> String { (end?["kinds"]?.stringsOrNil ?? []).joined(separator: ", ") }
        return "\(froms.map(kinds).joined(separator: " | ")) → \(kinds(to))"
    }

    private var endsLines: some View {
        VStack(alignment: .leading, spacing: 8) {
            ForEach(Array(froms.enumerated()), id: \.offset) { index, end in
                endBox(L10n[.Kinds.formFrom], end, remove: froms.count > 1 ? {
                    var next = froms
                    next.remove(at: index)
                    context.edit { $0.set(.array(next.map(OJSON.object)), at: .kind, "from") }
                } : nil) { changed in
                    var next = froms
                    next[index] = changed
                    context.edit { $0.set(.array(next.map(OJSON.object)), at: .kind, "from") }
                }
            }
            if !readOnly {
                Button {
                    let made = OrderedObject([("kinds", OJSON.string(["*"])), ("at", .string("card"))])
                    context.edit { $0.set(.array((froms + [made]).map(OJSON.object)), at: .kind, "from") }
                } label: { Label(L10n[.Kinds.formAddStart], systemImage: "plus") }
                    .buttonStyle(.link).font(.app(11)).padding(.leading, 114)
            }
            endBox(L10n[.Kinds.formTo], to ?? OrderedObject(), remove: nil) { changed in
                context.edit { $0.set(.object(changed), at: .kind, "to") }
            }
            KindFormLine(label: L10n[.Kinds.formAtMost]) {
                KindNumberField(value: draft.value(at: .kind, "max")?.numberOrNil, placeholder: L10n[.Kinds.formAny]) { number in
                    context.edit { $0.set(number.map(OJSON.number), at: .kind, "max") }
                }
                .disabled(readOnly)
                Text(L10n[.Kinds.formMaxLines]).font(.app(10.5)).foregroundStyle(.secondary)
            }
        }
    }

    /// One end: the kinds of card (`*` any), whether at the card's header and at its rows (`at`), and —
    /// where a kind picked has two kinds of row or more — which of them (`rows`).
    private func endBox(_ title: String, _ end: OrderedObject, remove: (() -> Void)?,
                        set: @escaping (OrderedObject) -> Void) -> some View {
        let kinds = end["kinds"]?.stringsOrNil ?? []
        let atSaid = end["at"]
        let at = atSaid?.stringOrNil.map { [$0] } ?? atSaid?.stringsOrNil ?? []
        let rowChoices = Array(Set(kinds.filter { $0 != "*" }.flatMap(rowKinds))).sorted()
        let manyRows = kinds.filter { $0 != "*" }.contains { rowKinds($0).count >= 2 }
        func written(card: Bool, row: Bool) {
            let list = [card ? "card" : nil, row ? "row" : nil].compactMap { $0 }
            var next = end
            next["at"] = list.count == 1 && atSaid?.stringOrNil != nil ? .string(list[0]) : OJSON.string(list)
            if !row { next["rows"] = nil }
            set(next)
        }
        return VStack(alignment: .leading, spacing: 6) {
            KindFormLine(label: title) {
                KindChips(items: kinds, label: { $0 == "*" ? L10n[.Kinds.formAnyCard] : context.label(ofCard: $0) },
                          choices: ["*"] + context.cardKinds.map(\.name), addTitle: L10n[.Kinds.formKindChip], readOnly: readOnly) { picked in
                    var next = end
                    next["kinds"] = OJSON.string(picked)
                    set(next)
                }
                if let remove, !readOnly {
                    Button(action: remove) { Image(systemName: "minus.circle") }.buttonStyle(.borderless)
                        .help(L10n[.Kinds.formRemoveStart])
                }
            }
            KindFormLine(label: L10n[.Kinds.formAt]) {
                Toggle(L10n[.Kinds.formAtCard], isOn: Binding(get: { at.contains("card") },
                                                              set: { written(card: $0, row: at.contains("row")) }))
                    .toggleStyle(.checkbox).font(.app(11.5))
                Toggle(L10n[.Kinds.formAtRows], isOn: Binding(get: { at.contains("row") },
                                                              set: { written(card: at.contains("card"), row: $0) }))
                    .toggleStyle(.checkbox).font(.app(11.5))
                if at.contains("row"), !manyRows {
                    Text(L10n[.Kinds.formAnyRow]).font(.app(10.5)).foregroundStyle(.secondary)
                }
            }
            .disabled(readOnly)
            if at.contains("row"), manyRows || end["rows"] != nil {
                KindFormLine(label: L10n[.Kinds.renamedRow]) {
                    KindChips(items: end["rows"]?.stringsOrNil ?? [], label: { _ in nil }, choices: rowChoices,
                              addTitle: L10n[.Kinds.formKindChip], readOnly: readOnly) { rows in
                        var next = end
                        next["rows"] = rows.isEmpty ? nil : OJSON.string(rows)
                        set(next)
                    }
                    if (end["rows"]?.stringsOrNil ?? []).isEmpty {
                        Text(L10n[.Kinds.formAnyRow]).font(.app(10.5)).foregroundStyle(.secondary)
                    }
                }
            }
        }
        .padding(8)
        .overlay(RoundedRectangle(cornerRadius: 6).stroke(.quaternary))
    }
}
