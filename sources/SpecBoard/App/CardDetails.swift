import SwiftUI

/// What is shown of a card beside its rows (overview §13): in the property panel,
/// the row picked or the card itself; and, a button away in the card editor's header,
/// what it is connected to (つながり) and what happened to it (履歴).
///
/// Everything the model can carry but the diagram cannot draw — rules, const values,
/// deliberate absences, what a value designates — is read and written here.
struct CardDetails: View {
    enum Part {
        /// The field or row picked, in the property panel.
        case row
        /// The card's names, description, tags, export settings, rules, in the property panel.
        case card
        /// What it refers to and what refers to it, the actions that use it, the
        /// notes that mention it; an action's keys and rules, a note's mentions.
        case connections
        /// What happened to it: its history, and in a branch what the branch changed.
        case history
    }

    var model: AppModel
    var part: Part
    /// The card open, for `card` and `connections`.
    var card: SchemaNode?
    /// Whether it offers the form or just the facts. Where nothing can be changed — a
    /// locked main, an archived project — the same content
    /// is shown as text rather than as greyed-out controls — a disabled text field
    /// is harder to read than the string it holds, which defeats the purpose.
    var editable: Bool
    /// For `connections`: the card's body as it is being written, not yet saved
    /// (`NoteEditor`); nil when it is not being written.
    var writing: String? = nil
    /// Jump to another schema. References are the main way to move around a project.
    var onNavigate: (String) -> Void

    /// Tags move in a locked main too (`specboard://SpecBoard/Docs/Design:DesignBranches` §8): they stay
    /// editable where only the lock holds the rest.
    private var tagsEditable: Bool {
        editable || (model.isLockedMainOpen && !model.isArchived && model.heldByMCP.isEmpty)
    }

    @Environment(\.colorScheme) private var colorScheme
    private var theme: GraphTheme { colorScheme == .dark ? .dark : .light }

    @State private var newTag = ""
    /// Sections that start folded. Rules and metadata can run to a dozen entries
    /// each, and burying the rest under them hides the thing most often looked for.
    @State private var folded: Set<String> = ["rules", "payloads", "absent", "metadata", "history"]

    /// In the property panel the parts are its table; in a card editor's small
    /// windows, a column of headed blocks.
    private var inPanel: Bool { part == .row || part == .card }

    var body: some View {
        VStack(alignment: .leading, spacing: inPanel ? 0 : 16) {
            switch part {
            case .row:
                if let field = model.selectedField {
                    fieldPane(field)
                } else if let schema = model.selectedSchema, let row = model.selectedRow {
                    rowPane(schema, row)
                }
            case .card:
                if let card { schemaPane(card) }
            case .connections:
                if let card { connectionsPane(card) }
            case .history:
                if let card { historyPane(card) }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    // MARK: - Schema

    /// What is around a card: what it refers to and what refers to it, and who reads,
    /// writes or mentions it.
    @ViewBuilder
    private func connectionsPane(_ schema: SchemaNode) -> some View {
        if schema.kind == .action {
            // Rows say what it reads and writes; the old fields only while it has no
            // rows, or while it still holds one that its rows leave unread.
            if !schema.speaksInRows || !schema.unreadOldFields.isEmpty { usesSection(schema) }
            linksSection(schema)
        } else if schema.kind.isWriting {
            linksSection(schema)
        } else if schema.kind != .flow {
            // A task's body names places as a note's does (it is written in the same editor).
            if schema.kind == .task { linksSection(schema) }
            usedBySection(PathText.quote(schema.name), in: schema)
        }
        let references = model.references(of: schema.id)
        referenceSection(L10n[.CardEditor.sectionRefersTo], references.outgoing, side: \.toSchemaId, arrow: "→")
        referenceSection(L10n[.CardEditor.sectionReferencedBy], references.incoming, side: \.fromSchemaId, arrow: "←")
    }

    @ViewBuilder
    private func schemaPane(_ schema: SchemaNode) -> some View {
        PropertyCategory(title: L10n[.Common.wordName]) {
            PropertyRow("name", help: L10n[.CardEditor.helpCardName]) {
                if editable {
                    CommitField(L10n[.CardEditor.placeholderCardName], text: schema.name, monospaced: true) { name in
                        model.updateSchema(schema.id, .init(name: name))
                    }
                    .id("name-\(schema.id)")
                } else {
                    Text(schema.name).font(.app(11.5, design: .monospaced)).textSelection(.enabled)
                }
            }
            PropertyRow("label", help: L10n[.CardEditor.helpCardLabel]) {
                if editable {
                    CommitField(L10n[.CardEditor.placeholderDisplayNameAnyLanguage], text: schema.label) { label in
                        model.updateSchema(schema.id, .init(label: label))
                    }
                    .id("label-\(schema.id)")
                } else {
                    Text(schema.label).textSelection(.enabled)
                }
            }
            PropertyRow("kind", help: schema.kind == .custom
                        ? L10n[.CardEditor.helpCustomKind(schema.kindDef?.meaning ?? "")]
                        : Glossary.term(schema.kind.rawValue).map { "\(schema.kind.rawValue): \($0.meaning)" } ?? "") {
                if schema.kind == .custom {
                    Text(schema.kindDef?.badge ?? schema.kindName).font(.app(11.5, design: .monospaced))
                } else if editable {
                    Picker("", selection: Binding(
                        get: { schema.kind },
                        set: { model.updateSchema(schema.id, .init(kind: $0)) })) {
                        ForEach(SchemaKind.made, id: \.self) { Text($0.rawValue).tag($0) }
                    }
                    .labelsHidden()
                    .pickerStyle(.menu)
                    .fixedSize()
                } else {
                    Text(schema.kind.rawValue).font(.app(11.5, design: .monospaced))
                }
            }
            PropertyRow("path", help: L10n[.CardEditor.helpCardPath]) {
                let path = model.detail?.directory.full(schema) ?? schema.name
                Text(path)
                    .font(.app(11, design: .monospaced)).foregroundStyle(.secondary).textSelection(.enabled)
                    .lineLimit(1).truncationMode(.head).help(path)
            }
            if schema.kind == .flow {
                PropertyRow("diagram", help: L10n[.CardEditor.helpFlowDiagram]) {
                    if editable {
                        Picker("", selection: Binding(get: { schema.diagramShape }, set: { model.setDiagram(schema.id, $0) })) {
                            ForEach(SchemaNode.diagramShapes, id: \.self) { Text(FlowEditor.shapeWord($0)).tag($0) }
                        }
                        .labelsHidden().pickerStyle(.menu).fixedSize()
                    } else {
                        Text(FlowEditor.shapeWord(schema.diagramShape)).font(.app(11.5))
                    }
                }
            }
            // How big a card is drawn on the diagram. How it looks, not what it says: no history,
            // and a locked branch takes it too.
            if schema.sizable {
                PropertyRow("size", help: L10n[.CardEditor.helpCardSize]) {
                    HStack(spacing: 8) {
                        Text(Self.sizeWord(schema.size)).font(.app(11.5))
                        if schema.size != nil, !model.isArchived {
                            Button(L10n[.CardEditor.buttonResetSize]) { model.setCardSize(schema.id, nil) }
                                .controlSize(.small)
                        }
                    }
                }
            }
            if schema.kind == .formatdoc {
                PropertyRow("view", help: L10n[.CardEditor.helpDocView]) {
                    if !model.isArchived {
                        // The default's words are long ("Project default (View as document)"): the button
                        // cuts them short with … where the panel is narrow, the menu says them in full.
                        let options: [FittingChoice.Option] = [.init("", L10n[.CardEditor.docViewProjectDefault(Self.viewWord(model.defaultSettings.docView))])]
                            + SchemaNode.docViews.map { .init($0, Self.viewWord($0)) }
                        FittingChoice(options: options,
                                      selection: Binding(get: { schema.docView ?? "" },
                                                         set: { model.setDocView(schema.id, $0.isEmpty ? nil : $0) }))
                    } else {
                        Text(Self.viewWord(schema.docView ?? model.defaultSettings.docView)).font(.app(11.5))
                    }
                }
            }
            // A card with lines to files whose code is to be looked at again, which has no category of fields to hold
            // its actions: they are here, under its name.
            if CustomKind.fields(of: schema.kind, in: model.detail?.cardKinds).isEmpty, !schema.codeReview.isEmpty {
                CardActionButtons(model: model, card: schema)
            }
        }
        // A stackdoc's version open: its name, key and heading — its body is
        // written in its card editor.
        if schema.kind == .stackdoc, let version = schema.openVersion {
            PropertyCategory(title: L10n[.CardEditor.categoryOpenVersion]) {
                PropertyRow("name", help: L10n[.CardEditor.helpVersionName]) {
                    if editable {
                        CommitField(L10n[.CardEditor.placeholderVersionName], text: version.name, monospaced: true) { name in
                            model.updateVersion(schema.id, version.name, ["name": name])
                        }
                        .id("version-name-\(schema.id)-\(version.name)")
                    } else {
                        Text(version.name).font(.app(11.5, design: .monospaced)).textSelection(.enabled)
                    }
                }
                PropertyRow("key", help: L10n[.CardEditor.helpVersionKey]) {
                    if editable {
                        CommitField(L10n[.CardEditor.placeholderVersionKey], text: version.key) { key in
                            model.updateVersion(schema.id, version.name, ["key": key])
                        }
                        .id("version-key-\(schema.id)-\(version.name)")
                    } else {
                        Text(version.key).textSelection(.enabled)
                    }
                }
                PropertyRow("heading", help: L10n[.CardEditor.helpVersionHeading]) {
                    if editable {
                        CommitField(L10n[.CardEditor.placeholderVersionHeading], text: version.heading) { heading in
                            model.updateVersion(schema.id, version.name, ["heading": heading])
                        }
                        .id("version-heading-\(schema.id)-\(version.name)")
                    } else {
                        Text(version.shownHeading).textSelection(.enabled)
                    }
                }
            }
        }
        // A data card: its shape, and what its shape no longer has.
        if schema.kind == .instance || schema.kind == .table {
            InstanceProperties(model: model, card: schema, editable: editable)
        }
        // A card of a kind the project defines: its fields by its definition, its rows, and what the
        // definition no longer has.
        if schema.kind == .custom {
            OwnCardProperties(model: model, card: schema, editable: editable)
        }
        // A goal's fields (status・priority・due・workBranch・MR) and a task's (done): written here, as
        // the goal's card editor is its card map (`GoalEditor`) and a task's its body — read from
        // the kinds' definitions the project has (`kinds.cards`), the built-in ones until it is read.
        // Each by its name, as MCP writes it, its label (状態) said in its (i).
        let ownFields = CustomKind.fields(of: schema.kind, in: model.detail?.cardKinds)
        if !ownFields.isEmpty {
            PropertyCategory(title: L10n[.Common.wordField]) {
                ForEach(ownFields, id: \.name) { field in
                    PropertyRow(field.name, help: field.shown == field.name ? field.description
                                : L10n[.CardEditor.helpOwnFieldShown(field.shown, field.description)]) {
                        CustomFieldControl(model: model, card: schema, field: field, editable: editable)
                    }
                }
                if editable { CardActionButtons(model: model, card: schema) }
            }
        }
        // What a goal or a task holds in a field taken out of its definition (担当 and the like):
        // faint, and let go of by hand.
        if schema.kind == .goal || schema.kind == .task {
            OwnStrays(model: model, card: schema, editable: editable)
        }
        // The lines leaving its head — a link, a flow's rules, what a store holds, a line of the
        // project's own (つながり, overview §13).
        if let source = model.lineSource(of: schema, row: nil) {
            LineList(model: model, source: source, editable: editable)
        }
        // The rows themselves — fields, values, holds, an action's rows, a flow's
        // nodes, a note's text — are the card editor's; what is here is the rest.
        tagsSection(schema)

        if CardExports.exportable(schema.kind) {
            section(L10n[.Common.wordExport]) { CardExports(model: model, card: schema) }
        }

        if let discriminator = schema.discriminator {
            section(L10n[.CardEditor.sectionDiscriminator]) {
                HStack(spacing: 6) {
                    Text(discriminator).font(.app(11.5, design: .monospaced))
                    Text(schema.variantValues.joined(separator: " / "))
                        .font(.app(11)).foregroundStyle(.secondary)
                }
            }
        }

        if !schema.rules.isEmpty { rulesSection(schema.rules) }

        if !schema.absentFields.isEmpty {
            foldable("absent", L10n[.CardEditor.sectionAbsentFields], count: schema.absentFields.count) {
                ForEach(schema.absentFields, id: \.name) { absent in
                    VStack(alignment: .leading, spacing: 2) {
                        Text(absent.name).font(.app(11.5, design: .monospaced))
                        Text(absent.reason)
                            .font(.app(11)).foregroundStyle(.secondary)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
            }
        }

        if !schema.metadata.isEmpty {
            foldable("metadata", L10n[.CardEditor.sectionMetadata], count: schema.metadata.count) {
                ForEach(schema.metadata.sorted(by: { $0.key < $1.key }), id: \.key) { pair in
                    HStack(alignment: .firstTextBaseline, spacing: 8) {
                        Text(pair.key)
                            .font(.app(11, design: .monospaced))
                            .foregroundStyle(.secondary)
                        Text(pair.value).font(.app(11.5)).textSelection(.enabled)
                    }
                }
            }
        }

    }

    /// What happened to the card: in a branch, what the branch changed and what main
    /// changed since; and its history.
    @ViewBuilder
    private func historyPane(_ schema: SchemaNode) -> some View {
        if model.openBranch != nil { branchSection(schema) }
        let mine = model.events.filter { $0.targetId == schema.id }.prefix(40)
        section(L10n[.CardEditor.sectionChangeHistory]) {
            if mine.isEmpty {
                Text(L10n[.CardEditor.emptyNothingYet]).font(.app(11)).foregroundStyle(.tertiary)
            }
            ForEach(Array(mine), id: \.id) { event in
                HStack(alignment: .firstTextBaseline, spacing: 8) {
                    Text(event.at.prefix(16).replacingOccurrences(of: "T", with: " "))
                        .font(.app(10, design: .monospaced))
                        .foregroundStyle(.tertiary)
                    Text(event.summary)
                        .font(.app(11))
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
        }
    }

    // MARK: - An enum's value, a store's hold

    /// The row picked in a card editor, to change — or to read, where nothing can be changed.
    @ViewBuilder
    private func rowPane(_ schema: SchemaNode, _ row: AppModel.CardRow) -> some View {
        switch row {
        case .value(let id):
            if let index = schema.values.firstIndex(where: { $0.id == id }) {
                valuePane(schema, index)
            }
        case .hold(let index):
            if model.holds(of: schema).indices.contains(index) {
                holdPane(schema, index)
            }
        case .formatRow(let path):
            FormatRowEditor(model: model, note: schema, path: path, editable: editable, part: .properties)
                .id("\(schema.id)|\(path)")
            lines(schema, row)
        case .nodeStep(let name, let path):
            StepEditor(model: model, flow: schema, node: name, path: path, editable: editable)
                .id("\(schema.id)|\(name)|\(path)")
            lines(schema, row)
        case .node(let name):
            if let node = schema.nodes.first(where: { $0.name == name }), let detail = model.detail {
                NodeInspector(model: model, flow: schema, detail: detail, node: node, editable: editable)
                    .id("\(schema.id)|\(name)")
                    .disabled(!editable)
            }
            lines(schema, row)
        case .customRow(let path) where schema.kind == .custom:
            OwnRowPane(model: model, card: schema, path: path, editable: editable)
                .id("\(schema.id)|\(path)")
        case .customRow(let path):
            CustomRowPane(model: model, card: schema, path: path, editable: editable)
                .id("\(schema.id)|\(path)")
        case .symbol(let path):
            SymbolProperties(model: model, file: schema, path: path)
                .id("\(schema.id)|\(path)")
        }
    }

    /// The lines leaving a row picked (つながり, overview §13): the row's place in its card,
    /// a node's own card.
    @ViewBuilder
    private func lines(_ schema: SchemaNode, _ row: AppModel.CardRow) -> some View {
        if let source = model.lineSource(of: schema, row: row) {
            LineList(model: model, source: source, editable: editable)
        }
    }

    @ViewBuilder
    private func valuePane(_ schema: SchemaNode, _ index: Int) -> some View {
        let value = schema.values[index]
        let list = AppModel.entries(schema.values)
        PropertyCategory(title: L10n[.Common.wordValue]) {
            PropertyRow("value", help: L10n[.CardEditor.helpEnumValue]) {
                if editable {
                    CommitField(L10n[.Common.wordValue], text: value.value, monospaced: true) { text in
                        var next = list
                        next[index].value = text
                        model.setValues(schema.id, next)
                    }
                    .id("value-\(value.id)")
                } else {
                    Text(value.value).font(.app(11.5, design: .monospaced)).textSelection(.enabled)
                }
            }
            .help(value.description)
            PropertyRow("label", help: L10n[.CardEditor.helpEnumLabel]) {
                if editable {
                    CommitField(L10n[.CardEditor.placeholderValueLabel], text: value.label) { text in
                        var next = list
                        next[index].label = text
                        model.setValues(schema.id, next)
                    }
                    .id("label-\(value.id)")
                } else {
                    Text(value.label).textSelection(.enabled)
                }
            }
            PropertyRow("description", help: L10n[.CardEditor.helpEnumDescription]) {
                if editable {
                    CommitField(L10n[.CardEditor.placeholderValueDescription], text: value.description) { text in
                        var next = list
                        next[index].description = text
                        model.setValues(schema.id, next)
                    }
                    .id("description-\(value.id)")
                } else {
                    Text(value.description).textSelection(.enabled)
                }
            }
            PropertyRow("index", help: L10n[.CardEditor.helpValueIndex]) {
                Text("\(index + 1) / \(schema.values.count)").foregroundStyle(.secondary)
            }
        }
    }

    @ViewBuilder
    private func holdPane(_ schema: SchemaNode, _ index: Int) -> some View {
        let holds = model.holds(of: schema)
        let path = holds[index]
        let held = model.schemaNamed(path, from: schema)
        PropertyCategory(title: L10n[.CardEditor.wordHolds]) {
            PropertyRow("path", help: L10n[.CardEditor.helpHoldPath]) {
                HStack(spacing: 4) {
                    if editable {
                        CommitField(L10n[.CardEditor.placeholderCardPath], text: path, monospaced: true) { text in
                            var next = holds
                            next[index] = text
                            model.setHolds(schema.id, next)
                        }
                        .id("hold-\(schema.id)-\(index)-\(path)")
                        // Picked from the project's own cards, the ones the store sees, in full.
                        PathPickButton(title: "…", detail: model.detail, owner: schema,
                                       wanted: PathWanted(cards: [.entity, .value])) { written in
                            var next = holds
                            next[index] = written
                            model.setHolds(schema.id, next)
                        }
                        .fixedSize()
                    } else {
                        Text(path).font(.app(11.5, design: .monospaced)).textSelection(.enabled)
                    }
                }
            }
            PropertyRow("card", help: L10n[.CardEditor.helpHoldCard]) {
                if let held {
                    Button(held.name) { onNavigate(held.id) }.buttonStyle(.link)
                } else {
                    Text(L10n[.CardEditor.holdCardNotFound]).foregroundStyle(.orange)
                }
            }
        }
    }

    // MARK: - Field

    @ViewBuilder
    private func fieldPane(_ location: FieldLocation) -> some View {
        let field = location.field

        PropertyCategory(title: L10n[.Common.wordName]) {
            PropertyRow("name", help: L10n[.CardEditor.helpFieldName]) {
                if editable {
                    CommitField(L10n[.CardEditor.placeholderFieldName], text: field.name, monospaced: true) { name in
                        model.updateField(field.id, .init(name: name))
                    }
                    .id("fname-\(field.id)")
                } else {
                    Text(field.name).font(.app(11.5, design: .monospaced)).textSelection(.enabled)
                }
            }
            PropertyRow("label", help: L10n[.CardEditor.helpFieldLabel]) {
                if editable {
                    CommitField(L10n[.Common.wordDisplayName], text: field.label) { label in
                        model.updateField(field.id, .init(label: label))
                    }
                    .id("flabel-\(field.id)")
                } else {
                    Text(field.label).textSelection(.enabled)
                }
            }
            PropertyRow("path", help: L10n[.CardEditor.helpFieldPath]) {
                Text(location.dotted).font(.app(11, design: .monospaced)).foregroundStyle(.secondary).textSelection(.enabled)
            }
        }

        // required and nullable are different questions — "is the key there" and
        // "is null a legal value" — so both are always offered, never merged.
        PropertyCategory(title: L10n[.Common.wordType]) {
            PropertyRow("type", help: L10n[.CardEditor.helpFieldType]) {
                // An embedded field's type is the card its embed line reaches: said, not
                // picked — the line is what makes it.
                if field.type.isRef {
                    Text(field.embeds.flatMap { model.card(through: $0) }.map { L10n[.CardEditor.fieldEmbeds($0.name)] } ?? L10n[.CardEditor.fieldEmbedsNothing])
                        .font(.app(11.5, design: .monospaced))
                } else if editable {
                    Picker("", selection: Binding(
                        get: { field.type.rawValue },
                        set: { model.updateField(field.id, .init(type: FieldType(rawValue: $0))) })) {
                        ForEach(ScalarType.allCases, id: \.rawValue) { Text($0.rawValue).tag($0.rawValue) }
                        Divider()
                        Text(L10n[.CardEditor.fieldObjectNested]).tag("object")
                    }
                    .labelsHidden()
                    .fixedSize()
                } else {
                    Text(field.type.rawValue).font(.app(11.5, design: .monospaced))
                }
            }
            flagRow("required", L10n[.CardEditor.helpRequired], field.isRequired) { model.updateField(field.id, .init(isRequired: $0)) }
            flagRow("nullable", L10n[.CardEditor.helpNullable], field.isNullable) { model.updateField(field.id, .init(isNullable: $0)) }
            flagRow("array", L10n[.CardEditor.helpArray], field.isArray) { model.updateField(field.id, .init(isArray: $0)) }
            PropertyRow("also", help: L10n[.CardEditor.helpAlso]) {
                let types = field.also ?? []
                if editable {
                    CommitField(L10n[.Common.wordNone], text: types.joined(separator: ", "), monospaced: true) { text in
                        let names = text.split(whereSeparator: { $0 == "," || $0 == " " || $0 == "、" }).map(String.init)
                        model.updateField(field.id, .init(also: names))
                    }
                    .id("falso-\(field.id)-\(types.joined(separator: ","))")
                } else {
                    Text(types.isEmpty ? L10n[.Common.wordNone] : types.joined(separator: ", "))
                        .font(.app(11.5, design: .monospaced)).foregroundStyle(types.isEmpty ? .tertiary : .primary)
                }
            }
            flagRow("unique", L10n[.CardEditor.helpUnique], field.isUnique) { model.updateField(field.id, .init(isUnique: $0)) }
            flagRow("primaryKey", L10n[.CardEditor.helpPrimaryKey], field.isPk) { model.updateField(field.id, .init(isPk: $0)) }
            PropertyRow("written", help: L10n[.CardEditor.helpWritten]) {
                Text(typeSummary(field)).font(.app(11, design: .monospaced)).foregroundStyle(.secondary)
            }
        }

        if let constValue = field.constValue, !constValue.isNull {
            section(L10n[.CardEditor.sectionConstValue]) {
                Text(constValue.display)
                    .font(.app(12, design: .monospaced))
                    .textSelection(.enabled)
                Text(L10n[.CardEditor.noteLiteralType])
                    .font(.app(10.5)).foregroundStyle(.tertiary)
            }
        }

        if field.type.isRef, let target = field.embeds, let schema = model.card(through: target) {
            section(L10n[.CardEditor.sectionEmbeddedCard]) {
                navigate(to: schema, note: L10n[.CardEditor.noteJsonShape(schema.name)])
            }
        }

        // Its lines — an embedding, a pointer, a line of the project's own: each picked or taken off
        // here, one more drawn (つなぐ, overview §13).
        if let source = model.lineSource(of: location) {
            LineList(model: model, source: source, editable: editable)
        }

        if !field.variants.isEmpty {
            section(L10n[.CardEditor.sectionVariantOnly]) {
                FlowRow(spacing: 5) {
                    ForEach(field.variants, id: \.self) { plain($0) }
                }
            }
        }

        if !field.rules.isEmpty { rulesSection(field.rules) }

        usedBySection(location.dotted, in: location.schema)
    }

    /// A field's flag as a row of the table: a check to change, or yes / no to read.
    private func flagRow(_ name: String, _ help: String, _ on: Bool, _ set: @escaping (Bool) -> Void) -> some View {
        PropertyRow(name, help: help) {
            if editable {
                Toggle("", isOn: Binding(get: { on }, set: set)).toggleStyle(.checkbox).labelsHidden()
            } else {
                Text(on ? L10n[.CardEditor.wordYes] : L10n[.CardEditor.wordNo]).foregroundStyle(on ? .primary : .tertiary)
            }
        }
    }

    /// How the three flags actually combine when something is generated from this.
    private func typeSummary(_ field: FieldNode) -> String {
        let base = field.type.isRef ? "T" : field.type.rawValue
        var element = field.isArray ? "\(base)[]" : base
        // The other types it also takes: a list of its own type as `T[]`.
        for other in field.also ?? [] {
            element += " | " + (other == "array" ? "\(base)[]" : other)
        }
        if field.isRequired { return field.isNullable ? "\(element) | null" : element }
        return field.isNullable ? L10n[.CardEditor.typeOptionalNullable(element)] : L10n[.CardEditor.typeOptional(element)]
    }

    // MARK: - Shared pieces

    @ViewBuilder
    private func rulesSection(_ rules: [Rule]) -> some View {
        foldable("rules", L10n[.CardEditor.sectionRules], count: rules.count) {
            ForEach(Array(rules.enumerated()), id: \.offset) { _, rule in
                VStack(alignment: .leading, spacing: 2) {
                    HStack(spacing: 6) {
                        Text(rule.kind).font(.app(11.5, design: .monospaced))
                        if knownRules[rule.kind] == nil {
                            // Unknown kinds are stored untouched and degrade to a
                            // comment on export rather than being refused.
                            Text(L10n[.CardEditor.ruleUnknown]).font(.app(10)).foregroundStyle(.tertiary)
                        }
                    }
                    if !rule.params.isEmpty {
                        Text(rule.params.sorted(by: { $0.key < $1.key })
                            .map { "\($0.key): \($0.value.display)" }
                            .joined(separator: "  "))
                            .font(.app(11, design: .monospaced))
                            .foregroundStyle(.secondary)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                    if let note = knownRules[rule.kind]?.note {
                        Text(note)
                            .font(.app(10.5)).foregroundStyle(.tertiary)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
            }
        }
    }

    @ViewBuilder
    private func referenceSection(
        _ title: String, _ relations: [Relation],
        side: KeyPath<Relation, String>, arrow: String
    ) -> some View {
        if relations.isEmpty {
            section(title) {
                Text(L10n[.Common.wordNone]).font(.app(11)).foregroundStyle(.tertiary)
            }
        } else {
            foldable("ref-\(arrow)", title, count: relations.count) {
                ForEach(Array(relations.enumerated()), id: \.offset) { _, relation in
                    if let other = model.schema(relation[keyPath: side]) {
                        Button {
                            onNavigate(other.id)
                        } label: {
                            HStack(alignment: .firstTextBaseline, spacing: 6) {
                                Text(arrow).foregroundStyle(.tertiary)
                                Text(other.name)
                                    .font(.app(11.5, design: .monospaced))
                                    .foregroundStyle(Color(nsColor: theme.brassInk))
                                Text(relation.from)
                                    .font(.app(10.5, design: .monospaced))
                                    .foregroundStyle(.tertiary)
                                Spacer()
                                if let cardinality = relation.cardinality {
                                    Text(cardinality.rawValue)
                                        .font(.app(10.5, design: .monospaced))
                                        .foregroundStyle(.secondary)
                                }
                                Text(model.detail?.lineKinds.label(relation.line) ?? relation.line)
                                    .font(.app(10))
                                    .foregroundStyle(.tertiary)
                                    .help("\(relation.line): \(model.detail?.lineKinds.meaning(relation.line) ?? "")")
                            }
                        }
                        .buttonStyle(.plain)
                    }
                }
            }
        }
    }

    @ViewBuilder
    private func navigate(to schema: SchemaNode, note: String?) -> some View {
        Button {
            onNavigate(schema.id)
        } label: {
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 6) {
                    Text(schema.name)
                        .font(.app(11.5, design: .monospaced))
                        .foregroundStyle(Color(nsColor: theme.brassInk))
                    if !schema.label.isEmpty {
                        Text(schema.label).font(.app(11)).foregroundStyle(.tertiary)
                    }
                }
                if let note {
                    Text(note).font(.app(10.5)).foregroundStyle(.tertiary)
                }
            }
        }
        .buttonStyle(.plain)
    }

    /// A note's, a document's or a task's mentions: each a way to what it names.
    @ViewBuilder
    private func linksSection(_ holder: SchemaNode) -> some View {
        let links = Self.links(of: holder, writing: writing)
        if !links.isEmpty {
            section(L10n[.CardEditor.sectionMentions]) {
                ForEach(Array(links.enumerated()), id: \.offset) { _, path in
                    let target = model.detail.flatMap { detail in
                        let directory = detail.directory
                        return landing(path, from: directory.sectionId(of: holder), in: directory)?.schema
                    }
                    Button {
                        if let target { onNavigate(target.id) }
                    } label: {
                        HStack(spacing: 8) {
                            Text(L10n[.CardEditor.wordMention])
                                .font(.app(10.5))
                                .foregroundStyle(Color(nsColor: theme.accent(for: .action)))
                                .frame(width: 34, alignment: .leading)
                            Text(path)
                                .font(.app(11.5, design: .monospaced))
                                .foregroundStyle(target == nil ? Color.red : Color.primary)
                        }
                    }
                    .buttonStyle(.plain)
                    .disabled(target == nil)
                    .help(target == nil ? L10n[.CardEditor.helpPathNotFound] : "")
                }
            }
        }
    }

    /// In a branch: what the branch changed in the card, from what main held when it
    /// first wrote it, and what main changed since (`specboard://SpecBoard/Docs/Design:DesignBranches` §2).
    @ViewBuilder
    private func branchSection(_ schema: SchemaNode) -> some View {
        let path = model.detail.map { SchemaDirectory.exported(path: $0.directory.full(schema)) }
        let diff = model.branchDiff()
        let made = path.map { diff?.made.contains($0) ?? false } ?? false
        let changed = diff?.changed.first { $0.path == path }
        let moved = diff?.mainChanged.first { $0.path == path }
        if made || changed != nil || moved != nil {
            section(L10n[.CardEditor.sectionBranchChanges]) {
                VStack(alignment: .leading, spacing: 4) {
                    if made {
                        Text(L10n[.CardEditor.branchMadeHere]).font(.app(11)).foregroundStyle(.secondary)
                    }
                    if let changed {
                        ForEach(changed.changes, id: \.self) { line in
                            Text(line).font(.app(11, design: .monospaced)).textSelection(.enabled)
                        }
                    }
                    if let moved {
                        Text(L10n[.CardEditor.branchMainChanged]).font(.app(10.5, weight: .semibold))
                            .foregroundStyle(.orange)
                        if moved.changes.isEmpty {
                            Text(L10n[.CardEditor.branchMainContentChanged]).font(.app(11)).foregroundStyle(.secondary)
                        }
                        ForEach(moved.changes, id: \.self) { line in
                            Text(line).font(.app(11, design: .monospaced)).textSelection(.enabled)
                        }
                    }
                }
            }
        }
    }

    /// An action's side: where it reads and writes, each a way to the document.
    @ViewBuilder
    private func usesSection(_ module: SchemaNode) -> some View {
        section(module.speaksInRows ? L10n[.CardEditor.sectionUsesOld] : L10n[.CardEditor.sectionUses]) {
            if module.speaksInRows {
                Text(L10n[.CardEditor.noteUsesUnused])
                    .font(.app(10.5)).foregroundStyle(.orange)
            } else if module.uses.isEmpty {
                Text(L10n[.CardEditor.noteUsesEmpty])
                    .font(.app(10.5)).foregroundStyle(.tertiary)
            }
            ForEach(Array(module.uses.enumerated()), id: \.offset) { _, use in
                // Read from the module's own section, like every path written in it.
                let target = model.detail.flatMap { detail in
                    let directory = detail.directory
                    return landing(use.path, from: directory.sectionId(of: module), in: directory)?.schema
                }
                Button {
                    if let target { onNavigate(target.id) }
                } label: {
                    VStack(alignment: .leading, spacing: 2) {
                        HStack(spacing: 8) {
                            Text(use.role.label)
                                .font(.app(10.5, weight: use.role == .write ? .semibold : .regular))
                                .foregroundStyle(Color(nsColor: theme.accent(for: .action)))
                                .frame(minWidth: 26, alignment: .leading).fixedSize()
                            Text(use.path)
                                .font(.app(11.5, design: .monospaced))
                                .foregroundStyle(target == nil ? Color.red : Color.primary)
                        }
                        if let note = use.note {
                            Text(note).font(.app(10.5)).foregroundStyle(.tertiary)
                                .padding(.leading, 34)
                        }
                    }
                }
                .buttonStyle(.plain)
                .disabled(target == nil)
                .help(target == nil ? L10n[.CardEditor.helpPathNotFound] : "")
            }
        }
    }

    /// The other side: which actions a change here reaches and how, and which notes
    /// talk about it.
    @ViewBuilder
    private func usedBySection(_ path: String, in holder: SchemaNode) -> some View {
        if let detail = model.detail, detail.schemas.contains(where: { !$0.kind.isShape }),
           let all = Impact.of(path, in: detail, base: detail.directory.sectionId(of: holder)) {
            hitsSection(L10n[.CardEditor.sectionUsedByActions], L10n[.CardEditor.emptyNoActionUses],
                        all.filter { $0.action.kind == .action }, detail, holder)
            let notes = all.filter { $0.action.kind.isWriting }
            if !notes.isEmpty {
                hitsSection(L10n[.CardEditor.sectionMentionedBy], "", notes, detail, holder)
            }
        }
    }

    @ViewBuilder
    private func hitsSection(_ title: String, _ none: String, _ hits: [Impact.Hit],
                             _ detail: ProjectDetail, _ holder: SchemaNode) -> some View {
            section(title) {
                if hits.isEmpty {
                    Text(none)
                        .font(.app(10.5)).foregroundStyle(.tertiary)
                }
                ForEach(Array(hits.enumerated()), id: \.offset) { _, hit in
                    Button {
                        onNavigate(hit.action.id)
                    } label: {
                        VStack(alignment: .leading, spacing: 2) {
                            HStack(spacing: 8) {
                                Text(hit.action.kind.isWriting ? L10n[.CardEditor.wordMention] : hit.use.role.label)
                                    .font(.app(10.5, weight: hit.use.role == .write ? .semibold : .regular))
                                    .foregroundStyle(Color(nsColor: theme.accent(for: hit.action.kind)))
                                    .frame(minWidth: 26, alignment: .leading).fixedSize()
                                Text(detail.directory.shown(hit.action,
                                                                from: detail.directory.sectionId(of: holder)))
                                    .font(.app(11.5, design: .monospaced))
                                    .foregroundStyle(Color(nsColor: theme.brassInk))
                            }
                            Text(hit.encloses ? L10n[.CardEditor.hitEncloses(hit.use.path)] : hit.via)
                                .font(.app(10.5, design: .monospaced)).foregroundStyle(.tertiary)
                                .padding(.leading, 34)
                        }
                    }
                    .buttonStyle(.plain)
                }
            }
    }

    private func facts(_ items: [(String, String)]) -> some View {
        HStack(spacing: 16) {
            ForEach(items, id: \.0) { item in
                VStack(alignment: .leading, spacing: 1) {
                    Text(item.1).font(.app(13, design: .monospaced)).bold()
                    Text(item.0).font(.app(10)).foregroundStyle(.secondary)
                }
            }
        }
    }

    @ViewBuilder
    private func foldable<Content: View>(
        _ key: String, _ title: String, count: Int? = nil,
        @ViewBuilder content: () -> Content
    ) -> some View {
        if inPanel {
            PropertyCategory(title: title, count: count, startsFolded: folded.contains(key)) {
                VStack(alignment: .leading, spacing: 6) { content() }
                    .padding(.horizontal, 18).padding(.vertical, 6)
            }
        } else {
            foldableBlock(key, title, count: count, content: content)
        }
    }

    @ViewBuilder
    private func foldableBlock<Content: View>(
        _ key: String, _ title: String, count: Int? = nil,
        @ViewBuilder content: () -> Content
    ) -> some View {
        let open = !folded.contains(key)
        VStack(alignment: .leading, spacing: 6) {
            Button {
                if open { folded.insert(key) } else { folded.remove(key) }
            } label: {
                HStack(spacing: 5) {
                    Image(systemName: open ? "chevron.down" : "chevron.right")
                        .font(.app(8, weight: .semibold))
                        .frame(width: 8)
                    Text(title)
                        .font(.app(10, weight: .semibold))
                        .tracking(1.1)
                    if let count {
                        Text("\(count)").font(.app(10, design: .monospaced))
                    }
                    Spacer()
                }
                .foregroundStyle(.secondary)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            if open { content() }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    /// Tags are labels for people, so they are edited here as chips rather than as
    /// a comma-separated string somebody has to get the punctuation right in.
    @ViewBuilder
    private func tagsSection(_ schema: SchemaNode) -> some View {
        let inUse = projectTags(model.detail?.schemas ?? [])
        let unused = inUse.map(\.tag).filter { !schema.tags.contains($0) }

        foldable("tags", L10n[.Common.wordTags], count: schema.tags.count) {
            if !tagsEditable {
                if schema.tags.isEmpty {
                    Text(L10n[.Common.wordNone]).font(.app(11)).foregroundStyle(.tertiary)
                } else {
                    FlowRow(spacing: 5) {
                        ForEach(schema.tags, id: \.self) { plain($0) }
                    }
                }
            } else if !schema.tags.isEmpty {
                FlowRow(spacing: 5) {
                    ForEach(schema.tags, id: \.self) { tag in
                        Button {
                            model.updateSchema(schema.id,
                                               .init(tags: schema.tags.filter { $0 != tag }))
                        } label: {
                            HStack(spacing: 4) {
                                Text(tag).font(.app(10.5))
                                Image(systemName: "xmark").font(.app(7))
                            }
                            .padding(.horizontal, 7).padding(.vertical, 2)
                            .background(RoundedRectangle(cornerRadius: 4)
                                .fill(Color(nsColor: theme.brassSoft)))
                            .overlay(RoundedRectangle(cornerRadius: 4)
                                .stroke(Color(nsColor: theme.brassLine), lineWidth: 1))
                            .foregroundStyle(Color(nsColor: theme.brassInk))
                        }
                        .buttonStyle(.plain)
                        .help(L10n[.CardEditor.actionRemoveNamed(tag)])
                    }
                }
            }

            if tagsEditable {
                HStack(spacing: 6) {
                    TextField(L10n[.CardEditor.placeholderAddTag], text: $newTag)
                        .textFieldStyle(.roundedBorder)
                        .font(.app(11))
                        .onSubmit { addTag(schema, newTag) }

                    if !unused.isEmpty {
                        Menu {
                            ForEach(unused, id: \.self) { tag in
                                Button(tag) { addTag(schema, tag) }
                            }
                        } label: {
                            Image(systemName: "tag")
                        }
                        .menuStyle(.borderlessButton)
                        .frame(width: 26)
                        .help(L10n[.CardEditor.helpPickTag])
                    }
                }
            }
        }
    }

    private func addTag(_ schema: SchemaNode, _ raw: String) {
        let tag = raw.trimmingCharacters(in: .whitespaces)
        guard !tag.isEmpty, !schema.tags.contains(tag) else { newTag = ""; return }
        model.updateSchema(schema.id, .init(tags: schema.tags + [tag]))
        if model.lastError == nil { newTag = "" }
    }

    @ViewBuilder
    private func section<Content: View>(
        _ title: String, @ViewBuilder content: () -> Content
    ) -> some View {
        if inPanel {
            PropertyCategory(title: title) {
                VStack(alignment: .leading, spacing: 6) { content() }
                    .padding(.horizontal, 18).padding(.vertical, 6)
            }
        } else {
            sectionBlock(title, content: content)
        }
    }

    @ViewBuilder
    private func sectionBlock<Content: View>(
        _ title: String, @ViewBuilder content: () -> Content
    ) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(title)
                .font(.app(10, weight: .semibold))
                .tracking(1.1)
                .foregroundStyle(.secondary)
            content()
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    /// The head of a pane where nothing can be changed: the same three facts the form carries — what
    /// it is called, what it is called in Japanese, and what kind of thing it is —
    /// at a size worth reading.
    private func heading(_ name: String, _ label: String, kind: String) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(name)
                .font(.app(15, weight: .semibold, design: .monospaced))
                .textSelection(.enabled)
                .fixedSize(horizontal: false, vertical: true)
            if !label.isEmpty {
                Text(label)
                    .font(.app(12)).foregroundStyle(.secondary)
                    .textSelection(.enabled)
                    .fixedSize(horizontal: false, vertical: true)
            }
            plain(kind)
        }
    }

    /// Free text, read as Markdown: descriptions are where the design reasoning is
    /// written, and it is written with lists, code and tables. Line breaks stay
    /// where they were typed.
    private func prose(_ text: String) -> some View {
        MarkdownView(text: text, size: 12)
    }

    private func plain(_ text: String) -> some View {
        Text(text)
            .font(.app(10.5, design: .monospaced))
            .padding(.horizontal, 7).padding(.vertical, 2)
            .overlay(RoundedRectangle(cornerRadius: 4)
                .stroke(Color(nsColor: theme.line2), lineWidth: 1))
    }

    private func toggle(_ label: String, _ on: Bool,
                        _ set: @escaping (Bool) -> Void) -> some View {
        Toggle(label, isOn: Binding(get: { on }, set: set))
            .toggleStyle(.checkbox)
            .font(.app(11.5))
    }

    private func flag(_ label: String, _ on: Bool) -> some View {
        Text(on ? label : L10n[.CardEditor.flagNo(label)])
            .font(.app(10.5))
            .padding(.horizontal, 7).padding(.vertical, 2)
            .background(RoundedRectangle(cornerRadius: 4)
                .fill(on ? Color(nsColor: theme.brassSoft) : Color.clear))
            .overlay(RoundedRectangle(cornerRadius: 4)
                .stroke(Color(nsColor: on ? theme.brassLine : theme.line2), lineWidth: 1))
            .foregroundStyle(on ? Color(nsColor: theme.brassInk) : .secondary)
    }
}

/// The two buttons a card editor's header carries (overview §13): what the card is
/// connected to (つながり), and what happened to it (履歴) — each in a small window. What
/// it is — its properties — is in the property panel.
extension CardDetails {
    /// A formatdoc's view on the diagram, as the panel and the project's settings say it.
    static func viewWord(_ view: String) -> String { view == "rows" ? L10n[.CardEditor.docViewRows] : L10n[.CardEditor.docViewDocument] }

    /// A card's own size as the panel's `size` row says it: none, the default; a width alone, with
    /// the height from its contents; or width × height.
    static func sizeWord(_ size: GridSize?) -> String {
        guard let size else { return L10n[.CardEditor.sizeDefault] }
        guard let h = size.h else { return L10n[.CardEditor.sizeWidth(size.w)] }
        return L10n[.CardEditor.sizeCells(size.w, h)]
    }

    /// What つながり lists as the places a card's text names, once each (`Impact.mentions`) —
    /// and for a card whose body is being written (`writing`: a note, a freedoc, a task in
    /// `NoteEditor`), what the body names as written so far, not as last saved. A task's body names
    /// places as a note's does.
    static func links(of card: SchemaNode, writing: String?) -> [String] {
        var card = card
        if let writing { card.body = writing }
        if card.kind == .task { card.kind = .note }
        return Impact.mentions(card).map(\.path)
    }
}

struct CardTools: View {
    var model: AppModel
    var card: SchemaNode
    var editable: Bool
    /// The body as it is being written (`NoteEditor`), for the places つながり lists; nil
    /// when it is not.
    var writing: String? = nil
    var onNavigate: (String) -> Void

    @State private var showsConnections = false
    @State private var showsDetails = false
    /// In a narrow card editor's bar: both under one menu (`CardBar`).
    @Environment(\.cardBarCompact) private var compact

    var body: some View {
        if compact {
            Menu {
                Button(L10n[.CardEditor.wordConnections]) { showsConnections = true }
                Button(DiagramMenus.connectionsWord) { model.openConnections(card.id) }
                Button(L10n[.Common.wordHistory]) { showsDetails = true }
            } label: {
                Image(systemName: "ellipsis.circle")
            }
            .menuStyle(.borderlessButton)
            .menuIndicator(.hidden)
            .fixedSize()
            .help(L10n[.CardEditor.helpConnectionsAndHistory])
            .popover(isPresented: Binding(get: { showsConnections || showsDetails },
                                          set: { if !$0 { showsConnections = false; showsDetails = false } }),
                     arrowEdge: .bottom) {
                if showsConnections { connections } else { history }
            }
        } else {
            HStack(spacing: 6) {
                Button { showsConnections.toggle() } label: {
                    Label(L10n[.CardEditor.wordConnections], systemImage: "point.3.connected.trianglepath.dotted")
                }
                .help(L10n[.CardEditor.helpConnectionsButton])
                .popover(isPresented: $showsConnections, arrowEdge: .bottom) { connections }
                Button { showsDetails.toggle() } label: {
                    Label(L10n[.Common.wordHistory], systemImage: "clock.arrow.circlepath")
                }
                .help(L10n[.CardEditor.helpHistoryButton])
                .popover(isPresented: $showsDetails, arrowEdge: .bottom) { history }
            }
            .controlSize(.small)
            .fixedSize()
        }
    }

    private var connections: some View {
        window {
            VStack(alignment: .leading, spacing: 12) {
                // The same, drawn: every card it is joined to, wherever it is.
                Button {
                    showsConnections = false
                    model.openConnections(card.id)
                } label: {
                    Label(L10n[.CardEditor.buttonViewIn(DiagramMenus.connectionsWord)], systemImage: "point.3.filled.connected.trianglepath.dotted")
                }
                .help(L10n[.CardEditor.helpViewConnectionsMap])
                CardDetails(model: model, part: .connections, card: card, editable: editable,
                            writing: writing) { id in
                    showsConnections = false
                    onNavigate(id)
                }
            }
        }
    }

    private var history: some View {
        window {
            CardDetails(model: model, part: .history, card: card, editable: editable) { id in
                showsDetails = false
                onNavigate(id)
            }
        }
    }

    private func window<Content: View>(@ViewBuilder _ content: () -> Content) -> some View {
        ScrollView { content().padding(16) }
            .frame(width: 400)
            .frame(minHeight: 160, idealHeight: 460, maxHeight: 600)
    }
}


/// Removing a card, asked first wherever it is offered — its right-click, in the
/// nav panel and on the diagram.
enum CardDeletion {
    static var message: String { L10n[.CardEditor.deletionMessage] }

    /// Asked in a window of its own, for the diagram's menu.
    @MainActor static func confirm(_ name: String) -> Bool {
        let alert = NSAlert()
        alert.messageText = L10n[.CardEditor.deletionTitle(name)]
        alert.informativeText = message
        alert.alertStyle = .warning
        alert.addButton(withTitle: L10n[.Common.actionDeleteItem])
        alert.addButton(withTitle: L10n[.Common.actionCancel])
        alert.buttons.first?.hasDestructiveAction = true
        return alert.runModal() == .alertFirstButtonReturn
    }
}
