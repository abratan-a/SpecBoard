import SwiftUI

// A card of a kind the project defines, on screen by its definition (`CardKindDef`):
// one field written the way its type is (`OwnFieldControl`), the card's fields, rows and what its
// definition no longer has in the property panel (`OwnCardProperties`), one of its rows there
// (`OwnRowPane`), and the card opened (`OwnCardEditor`: its fields and rows to write, or, for a
// container, its card map).

/// One field of a card of a kind the project defines, written the way its type is: a choice, a
/// check, a branch or a merge request of the project, a number, a date, text in a line or in a box, and a
/// list, a group, any JSON or a map as JSON written by hand. `write` takes the value as its kind
/// holds it; nil takes it off.
struct OwnFieldControl: View {
    var model: AppModel
    var field: KindField
    var value: JSONValue?
    /// What tells this control apart from another of the same field (a card's, a row's).
    var key: String
    var editable: Bool
    var write: (JSONValue?) -> Void

    @State private var problem: String?

    private var text: String {
        switch value {
        case .string(let s)?: s
        case .number?, .bool?: value.map(OwnFieldText.short) ?? ""
        default: ""
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            control.disabled(!editable)
            if let problem {
                Text(problem).font(.app(10.5)).foregroundStyle(.red).fixedSize(horizontal: false, vertical: true)
            }
        }
        .help(field.description)
    }

    @ViewBuilder
    private var control: some View {
        if field.writtenAsJSON {
            CommitTextEditor(text: value.map(\.jsonText) ?? "", minHeight: 44,
                             placeholder: field.many ? L10n[.Board.fieldJsonListPlaceholder] : "JSON") { raw in
                commitJSON(raw)
            }
            .font(.app(11, design: .monospaced))
            .id(key)
        } else {
            // A choice is as wide as the value chosen, within the panel's column (`FittingChoice`).
            switch field.fieldtype {
            case "radio", "dropdownlist":
                FittingChoice(options: .words(field.options, labels: field.optionLabels, none: !field.required || value == nil),
                              selection: Binding(get: { value?.asText ?? "" }, set: { set($0.isEmpty ? nil : .string($0)) }),
                              segmented: field.fieldtype == "radio" && field.options.count <= 4)
            case "checkbox":
                Toggle("", isOn: Binding(get: { value == .bool(true) }, set: { set(.bool($0)) }))
                    .labelsHidden().toggleStyle(.checkbox)
            case "branch":
                FittingChoice(options: .branches(model.branches.map(\.name), keeping: value?.asText ?? ""),
                              selection: Binding(get: { value?.asText ?? "" }, set: { set($0.isEmpty ? nil : .string($0)) }))
            case "mr":
                FittingChoice(options: .mergeRequests(model.mergeRequests, keeping: text),
                              selection: Binding(get: { text }, set: { raw in set(Double(raw).map(JSONValue.number)) }))
            case "number":
                CommitField(L10n[.Board.fieldNumber], text: text, monospaced: true) { raw in
                    guard !raw.isEmpty else { return set(nil) }
                    guard let number = Double(raw) else { problem = L10n[.Board.fieldNumberProblem(raw)]; return }
                    set(.number(number))
                }
                .id(key).frame(maxWidth: 160)
            case "longtext", "markdown":
                CommitTextEditor(text: text, minHeight: 52) { set($0.isEmpty ? nil : .string($0)) }
                    .id(key)
            default:
                CommitField(field.fieldtype == "date" ? "YYYY-MM-DD" : field.label, text: text,
                            monospaced: ["path", "card", "url"].contains(field.fieldtype)) { raw in
                    set(raw.isEmpty ? nil : .string(raw))
                }
                .id(key).frame(maxWidth: 320)
            }
        }
    }

    private func set(_ next: JSONValue?) {
        problem = nil
        guard next != value else { return }
        write(next)
    }

    private func commitJSON(_ raw: String) {
        let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return set(nil) }
        guard let data = trimmed.data(using: .utf8),
              let parsed = try? JSONSerialization.jsonObject(with: data, options: [.fragmentsAllowed]) else {
            problem = L10n[.Board.fieldJsonProblem]
            return
        }
        set(JSONValue.of(parsed))
    }
}

/// What the property panel says of a card of a kind the project defines, after its names: its
/// fields, each written the way its type is; its rows, a count each (written in its card
/// editor, a row picked here); and what its definition no longer has, faint, each let go of
/// by hand (「定義に無い欄」).
struct OwnCardProperties: View {
    var model: AppModel
    var card: SchemaNode
    var editable: Bool

    var body: some View {
        if let def = card.kindDef {
            // Opened as a note (`opens: markdown`): its body written in its card editor, not here.
            let shown = def.ownFields.filter { !(def.opensAsNote && $0.name == "body") }
            if !shown.isEmpty {
                PropertyCategory(title: L10n[.Common.wordField]) {
                    ForEach(shown, id: \.name) { field in
                        PropertyRow(field.name, help: [field.label == field.name ? "" : field.label, field.description]
                            .filter { !$0.isEmpty }.joined(separator: L10n[.Board.separatorSentence])) {
                            OwnFieldControl(model: model, field: field, value: card.ownFields[field.name],
                                            key: "\(card.id)|\(field.name)", editable: editable) { value in
                                model.setOwnField(card.id, field.name, value)
                            }
                        }
                    }
                }
            }
            if !def.rows.isEmpty {
                let placed = OwnRows.walk(def, card.ownRows)
                PropertyCategory(title: L10n[.Common.wordRow], count: placed.count) {
                    ForEach(def.rows, id: \.name) { kind in
                        PropertyRow(kind.name, help: kind.label == kind.name ? "" : kind.label) {
                            Text(L10n[.Board.rowsCount(card.ownRows[kind.name]?.arrayValue?.count ?? 0)])
                                .font(.app(11)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                        }
                    }
                }
            }
        } else {
            PropertyCategory(title: L10n[.Common.wordField]) {
                Text(L10n[.Board.kindNoDefinitionTab(card.kindName)])
                    .font(.app(11)).foregroundStyle(.secondary).padding(8)
            }
        }
        OwnStrays(model: model, card: card, editable: editable)
    }
}

/// What a card of a kind the project defines holds that its definition no longer has (「定義に無い
/// 欄」: a field taken out of the definition, or renamed without its values): faint, by where it
/// is, each let go of by hand — or all at once.
struct OwnStrays: View {
    var model: AppModel
    var card: SchemaNode
    var editable: Bool

    static var help: String { L10n[.Board.straysHelp] }

    var body: some View {
        if !card.strayFields.isEmpty {
            PropertyCategory(title: L10n[.Board.straysTitle], count: card.strayFields.count) {
                ForEach(card.strayFields.keys.sorted(), id: \.self) { at in
                    PropertyRow(at, help: Self.help) {
                        HStack(alignment: .firstTextBaseline, spacing: 6) {
                            Text(OwnFieldText.short(card.strayFields[at] ?? .null))
                                .font(.app(11)).foregroundStyle(.tertiary).lineLimit(2).textSelection(.enabled)
                            Spacer(minLength: 0)
                            if editable {
                                Button(L10n[.Common.actionDelete]) { model.dropStrays(card.id, at: at) }.controlSize(.small)
                            }
                        }
                    }
                }
                if editable, card.strayFields.count > 1 {
                    HStack {
                        Spacer()
                        Button(L10n[.Board.straysDeleteAll]) { model.dropStrays(card.id) }.controlSize(.small)
                    }
                    .padding(.horizontal, 10).padding(.vertical, 4)
                }
            }
        }
    }
}

/// One row of a card of a kind the project defines, picked: its fields, each written the way its
/// type is (the row's kind's whole list written back), the lines leaving it, and the way to take
/// it out.
struct OwnRowPane: View {
    var model: AppModel
    var card: SchemaNode
    var path: String
    var editable: Bool

    var body: some View {
        if let def = card.kindDef, let row = OwnRows.walk(def, card.ownRows).first(where: { $0.path == path }) {
            PropertyCategory(title: row.kind.label) {
                PropertyRow("path", help: L10n[.Board.rowPathHelp(path)]) {
                    Text(path).font(.app(11, design: .monospaced)).foregroundStyle(.secondary).textSelection(.enabled)
                }
                ForEach(row.kind.fields, id: \.name) { field in
                    PropertyRow(field.name, help: field.description) {
                        OwnFieldControl(model: model, field: field, value: row.fields[field.name],
                                        key: "\(card.id)|\(path)|\(field.name)", editable: editable) { value in
                            change(row) { fields in
                                var fields = fields
                                fields[field.name] = value
                                return fields
                            }
                        }
                    }
                }
                if editable {
                    HStack {
                        Spacer()
                        Button(L10n[.Board.rowDelete], role: .destructive) {
                            model.clearRow()
                            change(row) { _ in nil }
                        }
                        .controlSize(.small)
                    }
                    .padding(.horizontal, 10).padding(.vertical, 4)
                }
            }
            if let source = model.lineSource(of: card, row: .customRow(path)) {
                LineList(model: model, source: source, editable: editable)
            }
        }
    }

    private func change(_ row: OwnRows.Placed, _ body: ([String: JSONValue]) -> [String: JSONValue]?) {
        guard let changed = OwnRows.changing(card.ownRows, at: row.places, body) else { return }
        model.setOwnRows(card.id, changed.kind, changed.rows)
    }
}

/// A card of a kind the project defines, opened — its card editor: its fields, each written the
/// way its type is, its rows, nested as written, each with its fields in a line and its own
/// way to be taken out, and what its definition no longer has. A container (`holds`) opens on
/// its card map, as a goal does: the cards it holds, each where its map keeps it, and 「カードを
/// 足す」 of a kind it holds.
struct OwnCardEditor: View {
    var model: AppModel
    var card: SchemaNode
    var detail: ProjectDetail
    var editable: Bool

    enum Mode: CaseIterable {
        case map, fields

        var title: String {
            switch self {
            case .map: L10n[.Board.modeCardMap]
            case .fields: L10n[.Board.modeFieldsAndRows]
            }
        }
    }

    @State private var mode = Mode.map
    @State private var controller = CardCanvasController()
    @State private var adding = false
    @State private var newName = ""
    @State private var newLabel = ""
    @State private var newKind = ""
    /// A row being added, by its kind of row: what it is to be called.
    @State private var newRows: [String: String] = [:]

    private var writable: Bool { editable && !model.isReadOnly }
    private var def: CardKindDef? { card.kindDef }
    private var isMap: Bool { (def?.isContainer ?? false) && mode == .map }

    var body: some View {
        VStack(spacing: 0) {
            CardMapBar(model: model, card: card, badge: def?.badge ?? card.kindName, editable: editable,
                       fit: { controller.fit() }) {
                EmptyView()
            } tools: {
                if def?.isContainer ?? false {
                    Button { adding = true } label: { Label(L10n[.Board.cardAdd], systemImage: "plus") }
                        .fixedSize()
                        .disabled(!writable)
                        .help(L10n[.Board.cardAddHelp])
                        .popover(isPresented: $adding, arrowEdge: .bottom) { addingCard }
                }
            } modes: {
                if def?.isContainer ?? false {
                    Picker("", selection: $mode) {
                        ForEach(Mode.allCases, id: \.self) {
                            BarLabel(title: $0.title, systemImage: $0 == .map ? "rectangle.3.group" : "list.bullet").tag($0)
                        }
                    }
                    .labelsHidden().pickerStyle(.segmented).fixedSize()
                    .help(L10n[.Board.cardModesHelp])
                }
            }
            Divider()
            if isMap {
                cardMap
            } else {
                ScrollView {
                    fieldsAndRows
                        .padding(16)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
        }
    }

    // MARK: - A container's card map

    @ViewBuilder
    private var cardMap: some View {
        let map = GoalCardMap.detail(card, in: detail)
        if map.schemas.isEmpty {
            VStack(spacing: 8) {
                Text(L10n[.Board.cardEmpty]).foregroundStyle(.secondary)
                Button(L10n[.Board.cardAdd]) { adding = true }.disabled(!writable)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else {
            DiagramFrame(model: model, definition: DiagramDefinition(
                detail: map,
                key: "own:\(card.id)",
                controller: controller,
                dataGeneration: model.dataGeneration,
                card: GoalCardMap.card(of:),
                presses: DiagramPresses.goalMap(model, menu: { _ in nil }),
                onMove: { id, at in model.moveOnGoalMap(card.id, GoalCardMap.card(of: id), to: at) },
                // A card its kind's face says is sized by hand.
                onResize: { id, size in model.resizeByHand(GoalCardMap.card(of: id), to: size) },
                onFormPress: { id, press in model.pressForm(GoalCardMap.card(of: id), press) },
                onClickEmpty: { model.selectSchema(card.id, follow: false) },
                mermaid: { [path = detail.directory.full(card)] kinds in
                    model.diagramMermaid("card_read", target: path, kinds: kinds)
                }),
                hint: L10n[.Board.cardMapHint])
        }
    }

    /// 「カードを足す」: its kind (one the container holds), its name and the name people read.
    private var addingCard: some View {
        let kinds = def?.holds ?? []
        return VStack(alignment: .leading, spacing: 10) {
            Text(L10n[.Board.cardAdd]).font(.app(13, weight: .semibold))
            Text(L10n[.Board.addWhereTo(detail.directory.full(card))]).font(.app(11)).foregroundStyle(.secondary)
            Picker("", selection: $newKind) {
                ForEach(kinds, id: \.self) { name in Text(detail.cardKinds[name]?.badge ?? name).tag(name) }
            }
            .labelsHidden().pickerStyle(.menu).fixedSize()
            TextField(L10n[.Board.cardNamePlaceholder], text: $newName)
                .textFieldStyle(.roundedBorder).font(.app(12, design: .monospaced)).frame(width: 280)
            TextField(L10n[.Board.fieldDisplayName], text: $newLabel)
                .textFieldStyle(.roundedBorder).frame(width: 280)
            HStack {
                Spacer()
                Button(L10n[.Common.actionCancel]) { adding = false }.keyboardShortcut(.cancelAction)
                Button(L10n[.Common.actionAdd]) {
                    // Brought into view on the card map once drawn, when it is the map that is seen.
                    model.addInside(card.id, kind: newKind.isEmpty ? (kinds.first ?? "") : newKind, name: newName, label: newLabel,
                                    showingOn: isMap ? controller : nil)
                    guard model.lastError == nil else { return }
                    newName = ""
                    newLabel = ""
                    adding = false
                }
                .keyboardShortcut(.defaultAction)
                .disabled(PathText.nameProblem(PathText.trimmed(newName)) != nil)
            }
        }
        .padding(16)
        .onAppear { if newKind.isEmpty { newKind = kinds.first ?? "" } }
    }

    // MARK: - Its fields and rows

    @ViewBuilder
    private var fieldsAndRows: some View {
        VStack(alignment: .leading, spacing: 18) {
            if let def {
                if !def.meaning.isEmpty {
                    Text(def.meaning).font(.app(11)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                        .readingWidth()
                }
                if !def.ownFields.isEmpty {
                    block(L10n[.Common.wordField]) {
                        ForEach(def.ownFields, id: \.name) { field in
                            HStack(alignment: .firstTextBaseline, spacing: 10) {
                                VStack(alignment: .leading, spacing: 1) {
                                    Text(field.label).font(.app(11.5))
                                    if field.label != field.name {
                                        Text(field.name).font(.app(10, design: .monospaced)).foregroundStyle(.tertiary)
                                    }
                                }
                                .frame(width: 140, alignment: .leading)
                                OwnFieldControl(model: model, field: field, value: card.ownFields[field.name],
                                                key: "editor|\(card.id)|\(field.name)", editable: writable) { value in
                                    model.setOwnField(card.id, field.name, value)
                                }
                                Spacer(minLength: 0)
                            }
                        }
                    }
                }
                ForEach(def.rows, id: \.name) { kind in
                    block(kind.label == kind.name ? kind.name : L10n[.Board.blockLabelAndName(kind.label, kind.name)]) {
                        rowList(kind)
                    }
                }
            } else {
                Text(L10n[.Board.kindNoDefinitionDefine(card.kindName)])
                    .foregroundStyle(.secondary)
            }
            if !card.strayFields.isEmpty {
                block(L10n[.Board.straysTitle]) {
                    Text(OwnStrays.help).font(.app(10.5)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                        .readingWidth()
                    ForEach(card.strayFields.keys.sorted(), id: \.self) { at in
                        HStack(alignment: .firstTextBaseline, spacing: 10) {
                            Text(at).font(.app(11, design: .monospaced)).foregroundStyle(.tertiary)
                                .frame(width: 140, alignment: .leading)
                            Text(OwnFieldText.short(card.strayFields[at] ?? .null)).font(.app(11.5)).foregroundStyle(.tertiary)
                                .lineLimit(3).textSelection(.enabled)
                            Spacer(minLength: 0)
                            if writable { Button(L10n[.Common.actionDelete]) { model.dropStrays(card.id, at: at) }.controlSize(.small) }
                        }
                    }
                }
            }
        }
    }

    /// One kind of row's rows, at the top of the card (the rows under them in a line, each
    /// picked to write in the property panel): each row's fields in a line, and a row added at
    /// the end by what it is to be called.
    @ViewBuilder
    private func rowList(_ kind: KindRow) -> some View {
        let placed = def.map { OwnRows.walk($0, card.ownRows) } ?? []
        let mine = placed.filter { $0.places.first?.kind == kind.name }
        if mine.isEmpty {
            Text(L10n[.Board.rowsEmpty]).font(.app(11)).foregroundStyle(.tertiary)
        }
        ForEach(mine, id: \.path) { row in
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                if let checked = row.checked {
                    Toggle("", isOn: Binding(get: { checked }, set: { _ in model.toggleOwnRow(card, at: row.path) }))
                        .labelsHidden().toggleStyle(.checkbox).disabled(!writable)
                }
                Button {
                    model.selectRow(card.id, .customRow(row.path))
                } label: {
                    HStack(spacing: 6) {
                        Text(row.title).font(.app(12))
                        Text(row.kind.fields
                            .filter { $0.name != row.kind.titleField && $0.name != row.kind.check }
                            .compactMap { field in row.fields[field.name].map { "\(field.name): \(OwnFieldText.short($0))" } }
                            .joined(separator: " · "))
                            .font(.app(10.5)).foregroundStyle(.secondary).lineLimit(1)
                    }
                }
                .buttonStyle(.plain)
                .padding(.leading, CGFloat(row.depth) * 16)
                Spacer(minLength: 0)
                if writable {
                    Button {
                        guard let changed = OwnRows.changing(card.ownRows, at: row.places, { _ in nil }) else { return }
                        model.setOwnRows(card.id, changed.kind, changed.rows)
                    } label: { Image(systemName: "minus.circle") }
                        .buttonStyle(.borderless)
                        .help(L10n[.Board.rowDeleteWithChildren])
                }
            }
        }
        if writable {
            HStack(spacing: 6) {
                TextField(kind.titleField.map { L10n[.Board.rowsNewTitleField($0)] } ?? L10n[.Board.rowsNewPlaceholder], text: Binding(
                    get: { newRows[kind.name] ?? "" }, set: { newRows[kind.name] = $0 }))
                    .textFieldStyle(.roundedBorder).font(.app(11.5)).frame(maxWidth: 260)
                Button(L10n[.Board.rowsAdd]) {
                    model.setOwnRows(card.id, kind.name, OwnRows.adding(card.ownRows, kind: kind, title: newRows[kind.name] ?? ""))
                    if model.lastError == nil { newRows[kind.name] = "" }
                }
                .controlSize(.small)
            }
        }
    }

    private func block<Content: View>(_ title: String, @ViewBuilder content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(title).font(.app(10.5, weight: .semibold)).tracking(1.1).foregroundStyle(.secondary)
            content()
        }
    }
}
