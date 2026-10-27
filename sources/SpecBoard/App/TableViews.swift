import SwiftUI

/// A table opened — its card editor (`specboard://SpecBoard/Docs/Design:CardKinds`「表（table）」): its rules on
/// the left — the section it reads (`within`), the rules a row meets (`where`), its columns, how it is ordered
/// (`sort`) and the array a row an element of (`unnest`) — and on the right the table they build, as the core
/// builds it (`tableMarkdown`). Each rule written whole as MCP writes it; what it is held to is the core's to
/// say. Its shape is chosen in the property panel, as a data card's is.
struct TableEditor: View {
    var model: AppModel
    var card: SchemaNode
    var editable: Bool

    private var shape: SchemaNode? { card.shapeOf.flatMap { model.detail?.directory.schema(id: $0) } }
    private var lookup: SchemaLookup { { model.detail?.directory.schema(id: $0) } }
    /// The table's cells that name a card (a column's `show: "link"`), links as a body's paths are.
    private var cellRefs: MarkdownRefs? {
        model.detail.map { detail in
            var refs = NoteEditor.refs(card, detail, outside: model.isBoardOpen ? model.outsideDetail : nil) {
                model.open(path: $0, from: $1)
            }
            refs.placesOnly = true
            return refs
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            CardBar(kind: card.kind, name: card.name, label: card.label) {
                EmptyView()
            } tools: {
                if let shape {
                    Text(L10n[.Properties.editorShape(shape.name)]).font(.app(11)).foregroundStyle(.secondary)
                        .help(L10n[.Properties.tableShapeHelp])
                }
                CardTools(model: model, card: card, editable: editable, writing: nil) { model.open(.card($0)) }
            }
            .font(.app(11))
            .padding(.horizontal, 14).padding(.vertical, 8)
            Divider()
            if let shape {
                HStack(alignment: .top, spacing: 0) {
                    ScrollView {
                        rules(shape).padding(16).frame(maxWidth: .infinity, alignment: .leading)
                    }
                    .frame(width: 380)
                    Divider()
                    ScrollView {
                        MarkdownView(text: card.tableMarkdown ?? "", size: 12, refs: cellRefs)
                            .padding(16)
                    }
                    .frame(maxWidth: .infinity)
                }
            } else {
                Text(L10n[.Properties.tableNoShape])
                    .font(.app(12)).foregroundStyle(.secondary).padding(16)
                Spacer()
            }
        }
    }
}

extension TableEditor {
    /// One of its rules as written: a list of objects.
    private func list(_ key: String) -> [[String: JSONValue]] {
        card.tableRules[key]?.arrayValue?.compactMap(\.asObject) ?? []
    }

    private func text(_ key: String) -> String { card.tableRules[key]?.asText ?? "" }

    /// A list written whole (none for an empty one: let go of).
    private func write(_ key: String, _ items: [[String: JSONValue]]) {
        model.setTableRule(card.id, key, items.isEmpty ? nil : .array(items.map { .object($0) }))
    }

    @ViewBuilder
    private func rules(_ shape: SchemaNode) -> some View {
        let paths = TableEditor.paths(shape, lookup: lookup)
        VStack(alignment: .leading, spacing: 14) {
            section(L10n[.Properties.ruleWithin], help: L10n[.Properties.ruleWithinHelp]) {
                CommitField(L10n[.Properties.ruleAll], text: text("within"), monospaced: true) {
                    model.setTableRule(card.id, "within", $0.isEmpty ? nil : .string($0))
                }
                .disabled(!editable)
            }
            section(L10n[.Properties.ruleWhere], help: L10n[.Properties.ruleWhereHelp]) {
                let filters = list("where")
                ForEach(filters.indices, id: \.self) { i in
                    filterRow(filters, i, paths: paths)
                }
                add(L10n[.Properties.ruleAddCondition]) {
                    write("where", filters + [["path": .string(paths.first ?? "@name"), "op": .string("eq"), "value": .string("")]])
                }
            }
            section(L10n[.Properties.ruleColumns], help: L10n[.Properties.ruleColumnsHelp]) {
                let columns = list("columns")
                ForEach(columns.indices, id: \.self) { i in
                    columnRow(columns, i, paths: paths)
                }
                add(L10n[.Properties.ruleAddColumn]) { write("columns", columns + [["path": .string("@name")]]) }
            }
            section(L10n[.Properties.ruleSort], help: L10n[.Properties.ruleSortHelp]) {
                let sort = list("sort")
                ForEach(sort.indices, id: \.self) { i in
                    sortRow(sort, i, paths: paths)
                }
                add(L10n[.Properties.ruleAddSort]) { write("sort", sort + [["path": .string(paths.first ?? "@name")]]) }
            }
            section(L10n[.Properties.ruleUnnest], help: L10n[.Properties.ruleUnnestHelp]) {
                let arrays = TableEditor.arrays(shape, lookup: lookup)
                Picker("", selection: Binding(get: { text("unnest") },
                                              set: { model.setTableRule(card.id, "unnest", $0.isEmpty ? nil : .string($0)) })) {
                    Text(L10n[.Properties.ruleUnnestNone]).tag("")
                    ForEach(arrays, id: \.self) { Text($0).tag($0) }
                }
                .labelsHidden().pickerStyle(.menu).fixedSize().disabled(!editable)
            }
        }
    }

    private func section<Content: View>(_ title: String, help: String,
                                        @ViewBuilder content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(title).font(.app(11, weight: .semibold)).foregroundStyle(.secondary).help(help)
            content()
        }
    }

    private func add(_ title: String, action: @escaping () -> Void) -> some View {
        Button(title, action: action).controlSize(.small).disabled(!editable)
    }

    private func pathPicker(_ selection: Binding<String>, _ paths: [String]) -> some View {
        Picker("", selection: selection) {
            if !paths.contains(selection.wrappedValue) {
                Text(selection.wrappedValue).tag(selection.wrappedValue)
            }
            ForEach(paths, id: \.self) { Text($0).tag($0) }
        }
        .labelsHidden().pickerStyle(.menu).fixedSize().disabled(!editable)
    }

    private func remove(_ action: @escaping () -> Void) -> some View {
        Button { action() } label: { Image(systemName: "minus.circle") }
            .buttonStyle(.borderless).help(L10n[.Common.actionRemove]).disabled(!editable)
    }

    private func filterRow(_ filters: [[String: JSONValue]], _ i: Int, paths: [String]) -> some View {
        let filter = filters[i]
        let set = { (key: String, value: JSONValue) in
            var next = filters
            next[i][key] = value
            write("where", next)
        }
        let op = filter["op"]?.asText ?? "eq"
        return HStack(spacing: 6) {
            pathPicker(Binding(get: { filter["path"]?.asText ?? "" }, set: { set("path", .string($0)) }), paths)
            Picker("", selection: Binding(get: { op }, set: { set("op", .string($0)) })) {
                ForEach(TableEditor.ops, id: \.0) { Text($0.1).tag($0.0) }
            }
            .labelsHidden().pickerStyle(.menu).fixedSize().disabled(!editable)
            if op == "exists" {
                Toggle(L10n[.Properties.ruleExists], isOn: Binding(get: { filter["value"] != .bool(false) },
                                             set: { set("value", .bool($0)) }))
                    .toggleStyle(.checkbox).disabled(!editable)
            } else {
                CommitField(L10n[.Common.wordValue], text: filter["value"].map(InstanceRows.text) ?? "") {
                    set("value", TableEditor.typed($0))
                }
                .frame(minWidth: 80).disabled(!editable)
            }
            remove {
                var next = filters
                next.remove(at: i)
                write("where", next)
            }
        }
    }

    private func columnRow(_ columns: [[String: JSONValue]], _ i: Int, paths: [String]) -> some View {
        let column = columns[i]
        let set = { (key: String, value: JSONValue?) in
            var next = columns
            next[i][key] = value
            write("columns", next)
        }
        return HStack(spacing: 6) {
            pathPicker(Binding(get: { column["path"]?.asText ?? "" }, set: { set("path", .string($0)) }), paths)
            CommitField(L10n[.Properties.ruleHeading], text: column["label"]?.asText ?? "") { set("label", $0.isEmpty ? nil : .string($0)) }
                .frame(minWidth: 80).disabled(!editable)
            Button { move(columns, i, by: -1) } label: { Image(systemName: "chevron.up") }
                .buttonStyle(.borderless).disabled(!editable || i == 0).help(L10n[.Properties.ruleEarlier])
            Button { move(columns, i, by: 1) } label: { Image(systemName: "chevron.down") }
                .buttonStyle(.borderless).disabled(!editable || i == columns.count - 1).help(L10n[.Properties.ruleLater])
            remove {
                var next = columns
                next.remove(at: i)
                write("columns", next)
            }
        }
    }

    private func move(_ columns: [[String: JSONValue]], _ i: Int, by step: Int) {
        var next = columns
        next.swapAt(i, i + step)
        write("columns", next)
    }

    private func sortRow(_ sort: [[String: JSONValue]], _ i: Int, paths: [String]) -> some View {
        let key = sort[i]
        let set = { (name: String, value: JSONValue) in
            var next = sort
            next[i][name] = value
            write("sort", next)
        }
        return HStack(spacing: 6) {
            pathPicker(Binding(get: { key["path"]?.asText ?? "" }, set: { set("path", .string($0)) }), paths)
            Toggle(L10n[.Properties.ruleReverse], isOn: Binding(get: { key["desc"] == .bool(true) }, set: { set("desc", .bool($0)) }))
                .toggleStyle(.checkbox).disabled(!editable)
            remove {
                var next = sort
                next.remove(at: i)
                write("sort", next)
            }
        }
    }
}

extension TableEditor {
    /// The ways a rule compares, as the core takes them, each with what it says.
    static var ops: [(String, String)] {
        [
            ("eq", L10n[.Properties.opEq]), ("ne", "≠"), ("lt", "<"), ("le", L10n[.Properties.opLe]),
            ("gt", ">"), ("ge", L10n[.Properties.opGe]),
            ("contains", L10n[.Properties.opContains]), ("exists", L10n[.Properties.opExists]),
        ]
    }

    /// A value as typed: a number or a truth when it reads as one, else the text.
    static func typed(_ text: String) -> JSONValue {
        if let n = Double(text) { return .number(n) }
        if text == "true" || text == "false" { return .bool(text == "true") }
        return .string(text)
    }

    /// The paths a rule may name in a shape: the data card's name and display name, then each field
    /// with a value of its own — inside an object and what a field embeds, and an array's elements by
    /// `[]` (`ports[].number`).
    static func paths(_ shape: SchemaNode, lookup: SchemaLookup?) -> [String] {
        var out = ["@name", "@label"]
        func walk(_ fields: [FieldNode], _ prefix: String, _ depth: Int) {
            for field in fields {
                let path = prefix + PathText.quote(field.name)
                let (inner, _) = InstanceRows.element(field, lookup: lookup)
                if field.isArray {
                    if inner.isEmpty { out.append(path + "[]") } else { out.append(path) }
                    if depth < 4 { walk(inner, path + "[].", depth + 1) }
                } else if inner.isEmpty {
                    out.append(path)
                } else if depth < 4 {
                    walk(inner, path + ".", depth + 1)
                }
            }
        }
        walk(shape.fields, "", 0)
        return out
    }

    /// The arrays a table may make a row an element of: its shape's own, and those inside its objects.
    static func arrays(_ shape: SchemaNode, lookup: SchemaLookup?) -> [String] {
        var out: [String] = []
        func walk(_ fields: [FieldNode], _ prefix: String, _ depth: Int) {
            for field in fields {
                let path = prefix + PathText.quote(field.name)
                if field.isArray {
                    out.append(path)
                } else if depth < 4 {
                    let (inner, _) = InstanceRows.element(field, lookup: lookup)
                    walk(inner, path + ".", depth + 1)
                }
            }
        }
        walk(shape.fields, "", 0)
        return out
    }
}
