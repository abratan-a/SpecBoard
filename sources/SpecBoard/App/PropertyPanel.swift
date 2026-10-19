import SwiftUI

/// The panel on the right (`specboard://SpecBoard/Docs/Design:Overview` §13): the properties of what is
/// picked — a row of a card, a card, or the section open — read and changed in place,
/// the way Visual Studio's form designer shows a control's. At the top, what is picked,
/// and a way to another row of the same card; in the middle, its properties as a table
/// of name — the key MCP writes it by — and value, in groups that fold, each name with
/// what it means a click away; at the foot, its description, written there with the
/// project's rule for what to write as the faint text.
struct PropertyPanel: View {
    var model: AppModel
    var editable: Bool

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            ScrollView { middle }
            Divider()
            descriptionPane
        }
        .background(Color(nsColor: .controlBackgroundColor).opacity(0.35))
    }

    /// What the panel is about, in the order it is looked for: a row picked in a
    /// card, the card picked, the section open.
    private enum Subject: Equatable {
        /// A line picked (overview §13「線を選ぶ」).
        case line(LineRef)
        case row(SchemaNode)
        case card(SchemaNode)
        /// A card ghost picked (`specboard://SpecBoard/Docs/Design:DesignSections` §2).
        case ghost(CardGhost)
        case section(ProjectSection)
        /// What the open branch removed, picked in the nav panel: shown, never changed.
        case gone(AppModel.GonePicked)
        case nothing
    }

    private var subject: Subject {
        if let gone = model.goneShown { return .gone(gone) }
        if let line = model.selectedLine { return .line(line) }
        if let ghost = model.selectedGhost { return .ghost(ghost) }
        if let schema = model.selectedSchema {
            if model.selectedField != nil || model.selectedRow != nil { return .row(schema) }
            return .card(schema)
        }
        if let section = model.shownSection, section.parentId != nil { return .section(section) }
        return .nothing
    }

    // MARK: - Top: what is picked

    private var header: some View {
        HStack(spacing: 6) {
            switch subject {
            case .row(let card), .card(let card):
                Menu {
                    Button(L10n[.Properties.headerCardOf(card.name)]) { model.clearRow() }
                    let rows = PropertyPanel.rows(of: card, holds: model.holds(of: card))
                    if !rows.isEmpty { Divider() }
                    ForEach(rows, id: \.title) { row in
                        Button(row.title) { row.pick(model, card) }
                    }
                } label: {
                    HStack(spacing: 5) {
                        Text(pickedTitle(card)).font(.app(12, weight: .semibold, design: .monospaced)).lineLimit(1)
                        Text(pickedKind(card)).font(.app(10.5)).foregroundStyle(.secondary).lineLimit(1)
                    }
                }
                .menuStyle(.borderlessButton)
                .help(L10n[.Properties.headerMenuHelp])
            case .line:
                let (title, kind) = Self.heading(model)
                Text(title).font(.app(12, weight: .semibold)).lineLimit(1).truncationMode(.middle)
                Text(kind).font(.app(10.5)).foregroundStyle(.secondary)
            case .ghost(let ghost):
                Text(Self.ghostHeading).font(.app(12, weight: .semibold)).foregroundStyle(.secondary)
                Text(ghost.name).font(.app(12, weight: .semibold, design: .monospaced)).lineLimit(1)
                Text(L10n[.Properties.headerCardGhost]).font(.app(10.5)).foregroundStyle(.secondary)
            case .section(let section):
                Text(section.name).font(.app(12, weight: .semibold, design: .monospaced)).lineLimit(1)
                Text(Self.sectionKindName(section)).font(.app(10.5)).foregroundStyle(.secondary)
            case .gone(let gone):
                let (name, kind) = Self.goneHeading(gone)
                Text(name).font(.app(12, weight: .semibold, design: .monospaced)).lineLimit(1)
                Text(kind).font(.app(10.5)).foregroundStyle(.secondary).lineLimit(1)
            case .nothing:
                Text(L10n[.Properties.headerProperties]).font(.app(12, weight: .semibold))
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 10)
        .frame(height: 30)
    }

    /// What the top of the panel says of what the open branch removed: its name, and what it is.
    static func goneHeading(_ gone: AppModel.GonePicked) -> (title: String, kind: String) {
        switch gone {
        case .card(let card): (card.name, card.kindName)
        case .ghost(let ghost): ("\(ghostHeading) \(ghost.name)", L10n[.Properties.headerCardGhost])
        case .node(let node, _): (node.name, L10n[.Properties.kindNode])
        case .section(let section): (section.name, sectionKindName(section))
        }
    }

    /// What an open section is, by its kind: "Schema section".
    private static func sectionKindName(_ section: ProjectSection) -> String {
        section.kind?.named ?? L10n[.Common.wordSection]
    }

    /// The shortcut's mark, before a card ghost's name at the top.
    static let ghostHeading = SchemaDirectory.ghostMark

    private func pickedTitle(_ card: SchemaNode) -> String { Self.pickedTitle(card, model) }
    private func pickedKind(_ card: SchemaNode) -> String { Self.pickedKind(card, model) }

    /// What the top of the panel says is picked, and what it is: a row of a card, a card,
    /// the section open — as the panel shows it, for the tests too.
    static func heading(_ model: AppModel) -> (title: String, kind: String) {
        if let gone = model.goneShown { return goneHeading(gone) }
        if let ref = model.selectedLine, let (_, line) = model.line(ref) {
            return (model.detail?.lineKinds.label(line.line) ?? line.line, L10n[.Common.wordLine])
        }
        if let ghost = model.selectedGhost { return ("\(ghostHeading) \(ghost.name)", L10n[.Properties.headerCardGhost]) }
        if let card = model.selectedSchema { return (pickedTitle(card, model), pickedKind(card, model)) }
        if let section = model.shownSection, section.parentId != nil {
            return (section.name, sectionKindName(section))
        }
        return (L10n[.Properties.headerProperties], "")
    }

    private static func pickedTitle(_ card: SchemaNode, _ model: AppModel) -> String {
        if let field = model.selectedField {
            let head = PathText.quote(card.name)
            return field.dotted.hasPrefix(head + ".") ? field.dotted : "\(head).\(field.dotted)"
        }
        if let row = model.selectedRow {
            return "\(card.name) · \(PropertyPanel.title(of: row, in: card, holds: model.holds(of: card)))"
        }
        return card.name
    }

    private static func pickedKind(_ card: SchemaNode, _ model: AppModel) -> String {
        if let field = model.selectedField { return field.field.type.rawValue }
        switch model.selectedRow {
        case .value?: return L10n[.Properties.kindEnumValue]
        case .hold?: return L10n[.Properties.rowHolds]
        case .formatRow?: return L10n[.Properties.kindFormatdocRow]
        case .node?: return L10n[.Properties.kindNode]
        case .nodeStep?: return L10n[.Properties.kindProcessRow]
        case .customRow? where card.kind == .custom: return L10n[.Properties.kindRowOf(card.kindDef?.label ?? card.kindName)]
        case .customRow?: return L10n[.Properties.kindRowOf(CustomKind.of(card.kind)?.name ?? card.kind.rawValue)]
        case .symbol(let path)?:
            return (["symbol"] + [model.outline(card.id)?.symbol(path)?.word].compactMap { $0 }).joined(separator: " · ")
        case nil: return card.kindName
        }
    }

    /// A row of a card as the top's menu offers it.
    struct Pick {
        var title: String
        var pick: (AppModel, SchemaNode) -> Void
    }

    /// The card's rows a level down, to go to from the top: its fields, values,
    /// holds, an action's trigger and rows, a flow's nodes, a formatdoc's rows.
    /// `holds`: a store's, as paths (`AppModel.holds`).
    static func rows(of card: SchemaNode, holds: [String] = []) -> [Pick] {
        switch card.kind {
        case .entity, .value:
            return card.fields.map { field in Pick(title: field.name) { $0.selectField($1.id, field.id, follow: false) } }
        case .enum:
            return card.values.map { value in Pick(title: value.value) { $0.selectRow($1.id, .value(value.id)) } }
        case .store:
            return holds.indices.map { index in Pick(title: holds[index]) { $0.selectRow($1.id, .hold(index)) } }
        case .flow:
            return card.nodes.map { node in Pick(title: node.name) { $0.selectRow($1.id, .node(node.name)) } }
        case .formatdoc:
            return card.rows.map { row in Pick(title: row.name) { $0.selectRow($1.id, .formatRow(row.name)) } }
        case .stackdoc:
            // Its versions: one picked is the one it has open (how it looks).
            return card.versions.map { version in Pick(title: version.key) { $0.openVersion($1.id, version.name) } }
        case .goal:
            var out: [Pick] = []
            CustomRows.walk(card.customRows) { row, path in
                out.append(Pick(title: row.text.isEmpty ? path : row.text) { $0.selectRow($1.id, .customRow(path)) })
            }
            return out
        case .custom:
            // A kind the project defines: its rows, as its definition names them (`steps.a`).
            guard let def = card.kindDef else { return [] }
            return OwnRows.walk(def, card.ownRows).map { row in
                Pick(title: String(repeating: "  ", count: row.depth) + row.title) { $0.selectRow($1.id, .customRow(row.path)) }
            }
        case .note, .freedoc, .action, .task, .instance, .table, .svg, .html, .frame:
            return []
        }
    }

    static func title(of row: AppModel.CardRow, in card: SchemaNode, holds: [String] = []) -> String {
        switch row {
        case .value(let id): card.values.first { $0.id == id }?.value ?? L10n[.Common.wordValue]
        case .hold(let index): holds.indices.contains(index) ? holds[index] : L10n[.Properties.rowHolds]
        case .formatRow(let path): path
        case .node(let name): name
        case .nodeStep(let name, let path):
            "\(name).\(StepEditing.parts(path)?.map { part in if case .step(let i) = part { "\(i + 1)" } else { "" } }.joined(separator: ".") ?? path)"
        case .customRow(let path):
            CustomRows.row(at: path, in: card.customRows).map { $0.text.isEmpty ? path : $0.text } ?? path
        case .symbol(let path): path
        }
    }

    // MARK: - Middle: the properties

    /// The properties as the panel's scroll view holds them. Laid out no wider than the panel:
    /// wider, the scroll view centres it, over the diagram on the left and off the screen on the
    /// right (the tests lay it out at the panel's width).
    var middle: some View {
        content
            .padding(.vertical, 6)
            .frame(maxWidth: .infinity, alignment: .leading)
    }

    @ViewBuilder
    private var content: some View {
        switch subject {
        case .line(let ref):
            LineProperties(model: model, ref: ref, editable: editable)
        case .row(let card):
            CardDetails(model: model, part: .row, card: card, editable: editable) { model.showCard($0) }
        case .card(let card):
            CardDetails(model: model, part: .card, card: card, editable: editable) { model.showCard($0) }
            if card.isCodeFile { FileSymbolsProperties(model: model, file: card) }
        case .ghost(let ghost):
            GhostProperties(model: model, ghost: ghost, editable: editable)
        case .section(let section):
            SectionProperties(model: model, section: section, editable: editable)
        case .gone(let gone):
            goneContent(gone)
        case .nothing:
            Text(L10n[.Properties.emptyNothing])
                .font(.app(11)).foregroundStyle(.tertiary)
                .padding(12)
        }
    }

    /// What the open branch removed, as it was, read only.
    @ViewBuilder
    private func goneContent(_ gone: AppModel.GonePicked) -> some View {
        switch gone {
        case .card(let card):
            CardDetails(model: model, part: .card, card: card, editable: false) { model.showCard($0) }
        case .ghost(let ghost):
            GhostProperties(model: model, ghost: ghost, editable: false)
        case .node(let inner, let flow):
            if let flow, let detail = model.detail {
                // Among the flow's nodes as it had them.
                let node = FlowNode(card: inner, siblings: [:], landing: { _ in nil })
                let shown = { var shown = flow; shown.nodes.append(node); return shown }()
                NodeInspector(model: model, flow: shown, detail: detail, node: node, editable: false)
                    .disabled(true)
            }
        case .section(let section):
            SectionProperties(model: model, section: section, editable: false)
        }
    }

    // MARK: - Foot: the description

    /// The description of what is picked, as it is read and written: its text, the place
    /// whose rule is the faint text, and how a new text is kept. Nil for what has none.
    private struct Describable {
        var key: String
        var text: String
        /// Where the project's rule for it is (`WritingRules`); none for what has no rule (a line).
        var place: String?
        /// The faint text where there is no rule.
        var placeholder = ""
        var set: (String) -> Void
    }

    private var describable: Describable? {
        func optional(_ text: String) -> String? { text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? nil : text }
        switch subject {
        case .line(let ref):
            // One line's own words: what the written-out notes say of it (`description`).
            guard let (_, line) = model.line(ref) else { return nil }
            return Describable(key: "line-\(ref.card)-\(ref.id)", text: line.description ?? "", place: nil,
                               placeholder: L10n[.Properties.descriptionLinePlaceholder]) {
                model.setLineDescription(ref, $0)
            }
        case .card(let card):
            return Describable(key: "card-\(card.id)", text: card.description, place: WritingRules.cardPlace(card.kind)) {
                model.updateSchema(card.id, .init(description: $0))
            }
        case .row(let card):
            if let field = model.selectedField?.field {
                return Describable(key: "field-\(field.id)", text: field.description, place: WritingRules.fieldPlace(card.kind)) {
                    model.updateField(field.id, .init(description: $0))
                }
            }
            switch model.selectedRow {
            case .nodeStep(let name, let path)?:
                guard let step = StepEditing.step(at: path, in: model.rows(of: card, node: name)) else { return nil }
                return Describable(key: "nodestep-\(card.id)-\(name)-\(path)", text: step.description ?? "",
                                   place: "step.description") { text in
                    model.editRows(card.id, node: name) { steps in
                        guard var row = StepEditing.step(at: path, in: steps) else { return }
                        row.description = optional(text)
                        steps = StepEditing.replacing(path, with: row, in: steps)
                    }
                }
            case .node(let name)?:
                guard let node = card.nodes.first(where: { $0.name == name }) else { return nil }
                return Describable(key: "node-\(card.id)-\(name)", text: node.description ?? "", place: "node.description") { text in
                    model.setFlowNodes(card.id, card.nodes.map { other in
                        var next = other
                        if other.name == name { next.description = optional(text) }
                        return next
                    })
                }
            default:
                return nil
            }
        case .section(let section):
            return Describable(key: "section-\(section.id)", text: section.description, place: "section.description") {
                model.describeSection(section.id, $0)
            }
        case .gone(.card(let card)):
            return Describable(key: "gone-\(card.id)", text: card.description, place: WritingRules.cardPlace(card.kind)) { _ in }
        case .gone(.node(let node, _)):
            return Describable(key: "gone-\(node.id)", text: node.description, place: "node.description") { _ in }
        case .gone(.section(let section)):
            return Describable(key: "gone-\(section.id)", text: section.description, place: "section.description") { _ in }
        case .ghost, .gone(.ghost), .nothing:
            return nil
        }
    }

    private var descriptionPane: some View {
        VStack(alignment: .leading, spacing: 4) {
            let found = describable
            HStack(spacing: 3) {
                Text("description").font(.app(11, weight: .semibold, design: .monospaced))
                if let found, let place = found.place {
                    PropertyInfo(text: WritingRules.helpText(place, project: model.writingRules))
                }
                Spacer()
            }
            if let found {
                if editable {
                    CommitTextEditor(text: found.text, minHeight: 72,
                                     placeholder: found.place.map(model.writingRule) ?? found.placeholder,
                                     onCommit: found.set)
                        .id(found.key)
                } else if found.text.isEmpty {
                    Text(L10n[.Properties.descriptionEmpty]).font(.app(11)).foregroundStyle(.tertiary)
                } else {
                    ScrollView { MarkdownView(text: found.text, size: 11.5).frame(maxWidth: .infinity, alignment: .leading) }
                }
            } else {
                Text(L10n[.Properties.descriptionNone]).font(.app(11)).foregroundStyle(.tertiary)
            }
        }
        .padding(.horizontal, 10).padding(.vertical, 8)
        .frame(maxWidth: .infinity, minHeight: 110, maxHeight: 180, alignment: .topLeading)
    }
}

// MARK: - The table's parts

/// What a property means, a click away beside its name — a small window, like the (i)
/// beside a setting.
struct PropertyInfo: View {
    var text: String

    @State private var open = false

    var body: some View {
        if !text.isEmpty {
            Button { open.toggle() } label: {
                Image(systemName: "info.circle").font(.app(9.5)).foregroundStyle(.tertiary)
            }
            .buttonStyle(.borderless)
            .help(text)
            .popover(isPresented: $open, arrowEdge: .trailing) {
                Text(text)
                    .font(.app(11))
                    .fixedSize(horizontal: false, vertical: true)
                    .textSelection(.enabled)
                    .frame(width: 280, alignment: .leading)
                    .padding(10)
            }
        }
    }
}

/// A group of properties, folded with a click — VS's category.
struct PropertyCategory<Content: View>: View {
    var title: String
    var count: Int?
    var startsFolded = false
    @ViewBuilder var content: Content

    @State private var folded: Bool?

    var body: some View {
        let isFolded = folded ?? startsFolded
        VStack(alignment: .leading, spacing: 0) {
            Button {
                folded = !isFolded
            } label: {
                HStack(spacing: 5) {
                    Image(systemName: isFolded ? "chevron.right" : "chevron.down")
                        .font(.app(9, weight: .semibold)).foregroundStyle(.secondary).frame(width: 10)
                    Text(title).font(.app(11, weight: .semibold))
                    if let count { Text("\(count)").font(.app(10, design: .monospaced)).foregroundStyle(.tertiary) }
                    Spacer()
                }
                .padding(.horizontal, 8).padding(.vertical, 4)
                .contentShape(Rectangle())
                .background(Color.secondary.opacity(0.08))
            }
            .buttonStyle(.plain)
            if !isFolded {
                VStack(alignment: .leading, spacing: 0) { content }
            }
        }
    }
}

enum PropertyLayout {
    /// Where the name column ends: the line between name and value is drawn here, over
    /// the row rather than in it, so a name of two lines makes the row taller.
    static let nameWidth: CGFloat = 122
}

/// One property: its name — the key MCP writes it by — on the left, with what it means
/// a click away; its value — to read, or to change — on the right.
struct PropertyRow<Value: View>: View {
    var name: String
    var help: String
    @ViewBuilder var value: Value

    init(_ name: String, help: String = "", @ViewBuilder value: () -> Value) {
        self.name = name
        self.help = help
        self.value = value()
    }

    var body: some View {
        HStack(alignment: .top, spacing: 0) {
            HStack(spacing: 3) {
                Text(name)
                    .font(.app(11, design: .monospaced))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.tail)
                PropertyInfo(text: help)
                Spacer(minLength: 0)
            }
            .frame(width: PropertyLayout.nameWidth - 24, alignment: .leading)
            .padding(.leading, 18)
            .padding(.trailing, 6)
            .padding(.top, 4)
            value
                .font(.app(11.5))
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 6)
                .padding(.vertical, 1)
        }
        .padding(.vertical, 3)
        .overlay(alignment: .leading) {
            Rectangle().fill(Color.secondary.opacity(0.15)).frame(width: 1).padding(.leading, PropertyLayout.nameWidth)
        }
        .overlay(alignment: .bottom) { Rectangle().fill(Color.secondary.opacity(0.08)).frame(height: 1) }
    }
}

/// A long text in one row of the table — its first line — and the whole of it in a
/// larger box a click away (VS's "…").
struct PropertyLongText: View {
    var text: String
    var placeholder: String
    var editable: Bool
    var commit: (String) -> Void

    @State private var open = false

    var body: some View {
        HStack(spacing: 4) {
            Text(text.isEmpty ? (editable ? placeholder : "") : text.components(separatedBy: "\n").first ?? "")
                .foregroundStyle(text.isEmpty ? .tertiary : .primary)
                .lineLimit(1)
                .truncationMode(.tail)
                .frame(maxWidth: .infinity, alignment: .leading)
                .contentShape(Rectangle())
                .onTapGesture(count: 2) { open = true }
            Button("…") { open = true }
                .buttonStyle(.borderless)
                .help(editable ? L10n[.Properties.longTextWrite] : L10n[.Properties.longTextRead])
        }
        .popover(isPresented: $open, arrowEdge: .leading) {
            VStack(alignment: .leading, spacing: 8) {
                if editable {
                    CommitTextEditor(text: text, minHeight: 180, placeholder: placeholder, onCommit: commit)
                } else {
                    ScrollView { Text(text).textSelection(.enabled).frame(maxWidth: .infinity, alignment: .leading) }
                }
            }
            .padding(12)
            .frame(width: 440, height: 260)
        }
    }
}

/// The section open, with nothing picked in it: its name, what it is, what it holds
/// written out by default, its card ghosts.
struct SectionProperties: View {
    var model: AppModel
    var section: ProjectSection
    var editable: Bool


    var body: some View {
        if let detail = model.detail {
            let directory = detail.directory
            PropertyCategory(title: L10n[.Common.wordName]) {
                PropertyRow("name", help: Glossary.term("section")?.meaning ?? "") {
                    if editable, !model.isReadFromFolder(section.id) {
                        CommitField(L10n[.Sections.listNamePlaceholder], text: section.name, monospaced: true) { model.renameSection(section.id, to: $0) }
                            .id("section-name-\(section.id)")
                    } else {
                        Text(section.name).font(.app(11.5, design: .monospaced)).textSelection(.enabled)
                    }
                }
                PropertyRow("path", help: L10n[.Properties.sectionPathHelp]) {
                    let path = directory.full(section: section.id)
                    Text(path).font(.app(11, design: .monospaced)).textSelection(.enabled)
                        .lineLimit(1).truncationMode(.head).help(path)
                }
                PropertyRow("type", help: section.kind?.help ?? "") {
                    Text(section.kind.map { L10n[.Properties.sectionTypeValue($0.label, $0.rawValue)] } ?? section.kindName ?? "")
                        .foregroundStyle(.secondary)
                }
                PropertyRow("cardCount", help: L10n[.Properties.sectionCardCountHelp]) {
                    Text(L10n[.Properties.sectionCardCount(directory.members(section.id).count)]).foregroundStyle(.secondary)
                }
            }
            // A working directory: its folder on this Mac and how it is read; a folder in one: where it is.
            if section.kindName == WorkDirIds.workdirKind {
                WorkDirProperties(model: model, section: section)
            } else if section.kindName == WorkDirIds.directoryKind {
                WorkDirFolderProperties(model: model, section: section)
            }
            // Its card ghosts: what each points at, pointed elsewhere and let go here — made from
            // a card's right-click or a drag with ⌥⌘ (`specboard://SpecBoard/Docs/Design:DesignSections` §2).
            let ghosts = directory.ghosts(in: section.id)
            PropertyCategory(title: "ghosts", count: ghosts.count) {
                if ghosts.isEmpty {
                    PropertyRow("ghosts", help: Glossary.term("ghost")?.meaning ?? "") {
                        Text(L10n[.Common.wordNone]).foregroundStyle(.tertiary)
                    }
                }
                ForEach(ghosts) { ghost in
                    PropertyRow(ghost.name, help: Glossary.term("ghost")?.meaning ?? "") {
                        HStack(spacing: 4) {
                            Text("\(SchemaDirectory.ghostMark) \(directory.shownTarget(of: ghost))")
                                .font(.app(11, design: .monospaced)).lineLimit(1).truncationMode(.middle)
                                .foregroundStyle(directory.reached(ghost) == nil ? .secondary : .primary)
                                .frame(maxWidth: .infinity, alignment: .leading)
                            if editable {
                                PathPickButton(title: L10n[.Properties.ghostRetarget], detail: detail, owner: nil,
                                               wanted: .retarget(ghost, in: directory)) { model.retargetGhost(ghost.id, to: $0) }
                                    .buttonStyle(.borderless)
                                Button(L10n[.Common.actionDelete]) { model.deleteGhost(ghost.id) }.buttonStyle(.borderless)
                            }
                        }
                    }
                }
            }
        }
    }
}
