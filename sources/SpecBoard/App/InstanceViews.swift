import SwiftUI

/// A data card opened — its card editor
/// (`specboard://SpecBoard/Docs/Design:CardKinds`「データ（instance）と定義のセクション」): its shape's fields, each
/// its name, its type's badge and an input as its type is (`InstanceValueControl`), written a value at a
/// time. Its shape is changed, and what its shape no longer has let go of, in the property panel.
struct InstanceEditor: View {
    var model: AppModel
    var card: SchemaNode
    var editable: Bool

    private var shape: SchemaNode? { card.instanceOf.flatMap { model.detail?.directory.schema(id: $0) } }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            CardBar(kind: card.kind, badge: L10n[.Properties.instanceBadge], name: card.name, label: card.label) {
                EmptyView()
            } tools: {
                if let shape {
                    Text(L10n[.Properties.editorShape(shape.name)]).font(.app(11)).foregroundStyle(.secondary)
                        .help(L10n[.Properties.instanceShapeHelp])
                }
                CardTools(model: model, card: card, editable: editable, writing: nil) { model.open(.card($0)) }
            }
            .font(.app(11))
            .padding(.horizontal, 14).padding(.vertical, 8)
            Divider()
            if let shape {
                ScrollView {
                    SwiftUI.Grid(alignment: .leadingFirstTextBaseline, horizontalSpacing: 10, verticalSpacing: 8) {
                        ForEach(shown(shape), id: \.id) { field in row(field) }
                    }
                    .padding(16)
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
            } else {
                Text(L10n[.Properties.instanceNoShape])
                    .font(.app(12)).foregroundStyle(.secondary).padding(16)
                Spacer()
            }
        }
    }
}

extension InstanceEditor {
    /// Its shape's fields shown: a discriminator's branch only when it is written.
    private func shown(_ shape: SchemaNode) -> [FieldNode] {
        shape.fields.filter { $0.variants.isEmpty || InstanceRows.value(card.data, at: [PathText.quote($0.name)]) != nil }
    }

    /// One field: its name (marked when it is required and has no value), its type's badge, its input.
    private func row(_ field: FieldNode) -> some View {
        let value = InstanceRows.value(card.data, at: [PathText.quote(field.name)])
        let badge = InstanceRows.badge(field, lookup: { model.detail?.directory.schema(id: $0) })
        return SwiftUI.GridRow {
            HStack(spacing: 4) {
                if field.isRequired, value == nil {
                    Circle().fill(Color.orange).frame(width: 6, height: 6).help(L10n[.Properties.instanceRequiredMissing])
                }
                Text(field.name).font(.app(12, design: .monospaced))
            }
            .help([field.label, field.description].filter { !$0.isEmpty }.joined(separator: L10n[.Properties.separatorSentence]))
            InstanceBadge(text: badge)
            InstanceValueControl(model: model, field: field, value: value,
                                 key: "\(card.id)|\(field.name)", editable: editable) { next in
                model.setInstanceValue(card.id, [field.name], next)
            }
            .gridColumnAlignment(.leading)
        }
    }
}

/// A type's badge, as the nav panel's branch badge is drawn: set apart from the
/// value beside it.
struct InstanceBadge: View {
    var text: String

    var body: some View {
        Text(text)
            .font(.app(9.5, design: .monospaced))
            .foregroundStyle(.secondary)
            .padding(.horizontal, 4).padding(.vertical, 1)
            .overlay(RoundedRectangle(cornerRadius: 3).stroke(.tertiary))
            .fixedSize()
    }
}

/// One value of a data card written as its field's type is: a line of text for a string, a URL, an
/// email, a uuid, a date or a time; lines for `text`; a number for `int`, `float` and `decimal`; a
/// check for `bool`; a choice of an enum's values (any text for an open one); JSON for an array, an
/// object, an embedded shape and `json`. Emptied, the value is taken away.
struct InstanceValueControl: View {
    var model: AppModel
    var field: FieldNode
    var value: JSONValue?
    /// What tells this control apart from another (a card's field).
    var key: String
    var editable: Bool
    var write: (JSONValue?) -> Void

    @State private var problem: String?

    private var target: SchemaNode? { field.embeds.flatMap { model.detail?.directory.schema(id: $0) } }

    private var asJSON: Bool {
        field.isArray || field.type.isObject || field.type.scalar == .json
            || (field.type.isRef && target?.kind != .enum)
    }

    private var text: String {
        switch value {
        case .string(let s)?: s
        case .number?, .bool?: value.map(InstanceRows.text) ?? ""
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
        if asJSON {
            CommitTextEditor(text: value.map(\.jsonText) ?? "", minHeight: 44,
                             placeholder: field.isArray ? L10n[.Properties.inputJsonList] : L10n[.Properties.inputJsonObject]) { raw in
                commitJSON(raw)
            }
            .font(.app(11, design: .monospaced))
            .id(key)
        } else if field.type.isRef, let target, !target.enumOpen {
            FittingChoice(options: .words(target.values.map(\.value), none: true),
                          selection: Binding(get: { value?.asText ?? "" }, set: { set($0.isEmpty ? nil : .string($0)) }))
        } else {
            switch field.type.scalar {
            case .bool?:
                Toggle("", isOn: Binding(get: { value == .bool(true) }, set: { set(.bool($0)) }))
                    .labelsHidden().toggleStyle(.checkbox)
            case .int?, .float?:
                CommitField(L10n[.Properties.inputNumber], text: text, monospaced: true) { raw in
                    guard !raw.isEmpty else { return set(nil) }
                    guard let number = Double(raw), field.type.scalar != .int || number == number.rounded() else {
                        problem = field.type.scalar == .int ? L10n[.Properties.inputIntegerProblem(raw)] : L10n[.Properties.inputNumberProblem(raw)]
                        return
                    }
                    set(.number(number))
                }
                .id(key).frame(maxWidth: 160)
            case .text?:
                CommitTextEditor(text: text, minHeight: 52) { set($0.isEmpty ? nil : .string($0)) }
                    .id(key)
            default:
                CommitField(Self.placeholder(field.type.scalar), text: text,
                            monospaced: field.type.scalar != .string) { raw in
                    set(raw.isEmpty ? nil : .string(raw))
                }
                .id(key).frame(maxWidth: 360)
            }
        }
    }

    /// What a line of text is to hold, said faintly.
    static func placeholder(_ type: ScalarType?) -> String {
        switch type {
        case .decimal?: L10n[.Properties.inputDecimal]
        case .uuid?: "uuid"
        case .url?: "https://…"
        case .email?: "name@example.com"
        case .date?: "YYYY-MM-DD"
        case .datetime?: "2026-10-10T09:30:00Z"
        default: L10n[.Properties.inputText]
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
            problem = L10n[.Properties.inputJsonProblem]
            return
        }
        set(JSONValue.of(parsed))
    }
}

/// What the property panel says of a data card: its shape (`of`), chosen among the project's entities
/// and values, and the values whose fields its shape no longer has, each let go of by hand — or all.
struct InstanceProperties: View {
    var model: AppModel
    var card: SchemaNode
    var editable: Bool

    static var strayHelp: String { L10n[.Properties.instanceStrayHelp] }

    var body: some View {
        PropertyCategory(title: L10n[.Properties.instanceShapeTitle]) {
            PropertyRow("of", help: card.kind == .table
                        ? L10n[.Properties.instanceOfHelpTable]
                        : L10n[.Properties.instanceOfHelp]) {
                let shapes = model.instanceShapes
                if editable {
                    FittingChoice(options: (card.shapeOf.map { of in shapes.contains { $0.id == of } } == true
                                            ? [] : [FittingChoice.Option.none])
                                  + shapes.map { FittingChoice.Option($0.id, $0.path) },
                                  selection: Binding(get: { card.shapeOf ?? "" },
                                                     set: { if !$0.isEmpty { model.setInstanceShape(card.id, $0) } }))
                } else {
                    Text(shapes.first { $0.id == card.shapeOf }?.path ?? L10n[.Properties.instanceNoShapeValue])
                        .font(.app(11.5, design: .monospaced)).textSelection(.enabled)
                }
            }
        }
        if !card.strayFields.isEmpty {
            PropertyCategory(title: L10n[.Properties.instanceStrays], count: card.strayFields.count) {
                ForEach(card.strayFields.keys.sorted(), id: \.self) { at in
                    PropertyRow(at, help: Self.strayHelp) {
                        HStack(alignment: .firstTextBaseline, spacing: 6) {
                            Text(InstanceRows.text(card.strayFields[at] ?? .null))
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
                        Button(L10n[.Properties.straysDeleteAll]) { model.dropStrays(card.id) }.controlSize(.small)
                    }
                    .padding(.horizontal, 10).padding(.vertical, 4)
                }
            }
        }
    }
}
