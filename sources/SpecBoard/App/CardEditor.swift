import SwiftUI

/// A card opened to read and write — its card editor (overview §13): its rows down
/// the middle — fields for an entity or value, values for an enum, holds for a
/// store — the one picked edited under it, and the card's connections and details a
/// button away in the header.
///
/// The diagram flattens a card to at most 22 rows and cannot show depth beyond an
/// indent; this is where the tree is actually readable, and where a nested schema
/// can be built. Nesting is shown by the vertical rules rather than a disclosure
/// triangle per level, so the shape of a deep field reads at a glance.
struct CardEditor: View {
    var model: AppModel
    var schema: SchemaNode
    /// Whether rows carry their edit menu and the adder is offered. See `CardDetails`.
    var editable: Bool
    var onNavigate: (String) -> Void

    @Environment(\.colorScheme) private var colorScheme
    private var theme: GraphTheme { colorScheme == .dark ? .dark : .light }

    @State private var collapsed: Set<String> = []
    @State private var newFieldName = ""
    @State private var newValueName = ""
    @State private var newHold = ""

    /// Every name this card goes by, worked out once.
    ///
    /// `schemaPaths` walks the whole project. A row that worked out its own paths cost
    /// 0.46ms, and about thirty rows exist at a time — a frame's entire budget spent
    /// on a menu nobody had opened yet.
    @State private var prefixes: [String] = []
    /// The last field picked from a row here, so the reveal can tell it apart from a
    /// selection made somewhere else.
    @State private var pickedHere: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            header
            Divider()
            ScrollViewReader { scroll in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 0) {
                        switch schema.kind {
                        case .enum: enumRows
                        case .store: holdRows
                        default:
                            ForEach(openRows) { row in
                                FieldRow(
                                    model: model,
                                    schemaId: schema.id,
                                    field: row.field,
                                    depth: row.depth,
                                    dotted: row.dotted,
                                    open: !collapsed.contains(row.field.id),
                                    editable: editable,
                                    prefixes: prefixes,
                                    onToggleFold: { toggleFold(row.field.id) },
                                    onUnfold: { collapsed.remove(row.field.id) },
                                    onPick: { pick(row.field.id) },
                                    onNavigate: onNavigate)
                            }
                            if schema.fields.isEmpty { empty(L10n[.CardEditor.emptyNoFields]) }
                        }
                    }
                    .padding(.vertical, 4)
                }
                // A click below the rows lets go of the row, and its editor shuts.
                .background(Color.clear.contentShape(Rectangle()).onTapGesture { model.clearRow() })
                // Being sent here to look at one field — from the canvas menu, or by
                // picking a row — has to actually land on it. A deep field can be
                // both folded away and hundreds of rows down.
                // Only when the selection came from somewhere else. A row you just
                // clicked is already on screen and already unfolded, so scrolling it
                // to the middle moves the thing out from under the pointer for no
                // reason — and moves everything you were reading around it.
                .onChange(of: model.selectedFieldId) { _, id in
                    guard id != pickedHere else { pickedHere = nil; return }
                    reveal(id, scroll)
                }
                .onAppear {
                    reveal(model.selectedFieldId, scroll)
                }
            }
            // Keyed on the relation count as well as the card: a new `ref` anywhere
            // in the project adds a site, and the paths would otherwise be a card old.
            .task(id: "\(schema.id)|\(model.detail?.relations.count ?? 0)") {
                prefixes = model.detail.map { schemaPaths($0, id: schema.id) } ?? []
            }
            if editable {
                Divider()
                adder
            }
        }
    }

    // MARK: - Header

    private var header: some View {
        VStack(alignment: .leading, spacing: 8) {
            CardBar(kind: schema.kind, name: schema.name, label: schema.label) {
                if schema.kind == .enum && schema.enumOpen {
                    Text(L10n[.CardEditor.badgeOpenEnum])
                        .font(.app(10))
                        .padding(.horizontal, 6).padding(.vertical, 1)
                        .overlay(RoundedRectangle(cornerRadius: 3)
                            .stroke(Color(nsColor: theme.brassLine), lineWidth: 1))
                        .foregroundStyle(Color(nsColor: theme.brassInk))
                        .help(L10n[.CardEditor.helpOpenEnum])
                }
            } tools: {
                CardTools(model: model, card: schema, editable: editable, onNavigate: onNavigate)
            }

            if !schema.description.isEmpty {
                Text(schema.description)
                    .font(.app(11.5))
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }

            HStack(spacing: 14) {
                ForEach(facts, id: \.0) { fact in
                    HStack(spacing: 4) {
                        Text(fact.0).font(.app(10)).foregroundStyle(.tertiary)
                        Text(fact.1).font(.app(11, design: .monospaced)).bold()
                    }
                }
                Spacer()
                if !foldable.isEmpty {
                    HStack(spacing: 8) {
                        Button(L10n[.Common.actionExpandAll]) { collapsed = [] }
                            .disabled(collapsed.isEmpty)
                        Button(L10n[.Common.actionCollapseAll]) { collapsed = foldable }
                            .disabled(collapsed == foldable)
                    }
                    .font(.app(10.5))
                    .buttonStyle(.plain)
                    .foregroundStyle(.secondary)
                }
            }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
    }

    /// Every field with something under it, whether or not it is on screen. Folding
    /// only what is visible is not what "すべて" means, and a parent folded away
    /// still has children to fold.
    private var foldable: Set<String> {
        var out: Set<String> = []
        walkFields(schema.fields) { field, _, _ in
            if !field.children.isEmpty { out.insert(field.id) }
        }
        return out
    }

    private var facts: [(String, String)] {
        let references = model.references(of: schema.id)
        let incoming = references.incoming.filter { $0.fromSchemaId != schema.id }.count
        switch schema.kind {
        case .enum:
            return [(L10n[.Common.wordValue], "\(schema.values.count)"), (L10n[.CardEditor.factIncoming], "\(incoming)")]
        case .store:
            return [(L10n[.CardEditor.wordHolds], "\(model.holds(of: schema).count)")]
        default:
            var nested = 0
            walkFields(schema.fields) { field, _, _ in if field.type.isObject { nested += 1 } }
            return [
                (L10n[.CardEditor.factFields], "\(countFields(schema.fields))"),
                (L10n[.CardEditor.factNested], "\(nested)"),
                (L10n[.CardEditor.factMaxDepth], "\(maxDepth(schema.fields))"),
                (L10n[.CardEditor.factOutgoing], "\(references.outgoing.count)"),
                (L10n[.CardEditor.factIncoming], "\(incoming)"),
            ]
        }
    }

    // MARK: - Fields

    /// Unfold everything above this field, then bring it into view.
    private func reveal(_ id: String?, _ scroll: ScrollViewProxy) {
        guard let id, let trail = fieldAncestry(schema.fields, id: id) else { return }
        collapsed.subtract(trail)
        withAnimation(.easeOut(duration: 0.15)) { scroll.scrollTo(id, anchor: .center) }
    }

    private func toggleFold(_ id: String) {
        if collapsed.contains(id) { collapsed.remove(id) } else { collapsed.insert(id) }
    }

    /// A row here was clicked. Noted so the reveal that follows every other
    /// selection change does not fire for this one.
    private func pick(_ fieldId: String) {
        pickedHere = fieldId
        model.selectField(schema.id, fieldId)
    }

    private struct Row: Identifiable {
        var field: FieldNode
        var depth: Int
        /// Path within the card, `warehouse.primary.address`. Free here — the walk
        /// is already carrying the trail — and it saves every row a project-wide lookup.
        var dotted: String
        var id: String { field.id }
    }

    /// The tree flattened to what is currently shown.
    ///
    /// A view that recursed into itself could not be typed — and a flat list is what
    /// `LazyVStack` wants anyway, so a deep card costs only the rows on screen.
    private var openRows: [Row] {
        var out: [Row] = []
        func walk(_ fields: [FieldNode], _ depth: Int, _ trail: String) {
            for field in fields {
                let dotted = trail.isEmpty ? PathText.quote(field.name) : PathText.member(trail, field.name)
                out.append(Row(field: field, depth: depth, dotted: dotted))
                if !field.children.isEmpty && !collapsed.contains(field.id) {
                    walk(field.children, depth + 1, dotted)
                }
            }
        }
        walk(schema.fields, 0, "")
        return out
    }

    // MARK: - An enum's values, a store's holds

    private var values: [AppModel.ValueEntry] { AppModel.entries(schema.values) }

    @ViewBuilder
    private var enumRows: some View {
        ForEach(Array(schema.values.enumerated()), id: \.element.id) { index, value in
            SimpleRow(picked: model.selectedRow == .value(value.id), theme: theme) {
                Text(value.value).font(.app(11.5, design: .monospaced))
                if !value.label.isEmpty {
                    Text(value.label).font(.app(10.5)).foregroundStyle(.secondary)
                }
            }
            // What the value stands for, on the pointer: the row keeps to its name and label.
            .help(value.description)
            .onTapGesture { model.selectRow(schema.id, .value(value.id)) }
            .contextMenu {
                if editable {
                    Button(L10n[.Common.actionMoveUp]) { moveValue(index, by: -1) }.disabled(index == 0)
                    Button(L10n[.Common.actionMoveDown]) { moveValue(index, by: 1) }.disabled(index == schema.values.count - 1)
                    Button(L10n[.CardEditor.actionDeleteNamed(value.value)], role: .destructive) { model.deleteValue(value.id) }
                }
            }
        }
        if schema.values.isEmpty { empty(L10n[.CardEditor.emptyNoValues]) }
    }

    private func moveValue(_ index: Int, by step: Int) {
        var list = values
        let to = index + step
        guard list.indices.contains(to) else { return }
        list.swapAt(index, to)
        model.setValues(schema.id, list)
    }

    /// What the store keeps: its `holds` lines, as paths.
    private var holds: [String] { model.holds(of: schema) }

    @ViewBuilder
    private var holdRows: some View {
        let holds = holds
        ForEach(Array(holds.enumerated()), id: \.offset) { index, path in
            SimpleRow(picked: model.selectedRow == .hold(index), theme: theme) {
                Text(path).font(.app(11.5, design: .monospaced))
                if let held = model.schemaNamed(path, from: schema) {
                    Text(held.kindName).font(.app(10)).foregroundStyle(.tertiary)
                } else {
                    Text(L10n[.CardEditor.holdNotFound]).font(.app(10)).foregroundStyle(.orange)
                }
            }
            .onTapGesture { model.selectRow(schema.id, .hold(index)) }
            .contextMenu {
                if editable {
                    Button(L10n[.Common.actionMoveUp]) { moveHold(index, by: -1) }.disabled(index == 0)
                    Button(L10n[.Common.actionMoveDown]) { moveHold(index, by: 1) }.disabled(index == holds.count - 1)
                    Button(L10n[.CardEditor.actionRemoveNamed(path)], role: .destructive) {
                        var list = holds
                        list.remove(at: index)
                        model.setHolds(schema.id, list)
                    }
                }
            }
        }
        if holds.isEmpty { empty(L10n[.CardEditor.emptyNoHolds]) }
    }

    private func moveHold(_ index: Int, by step: Int) {
        var list = holds
        let to = index + step
        guard list.indices.contains(to) else { return }
        list.swapAt(index, to)
        model.setHolds(schema.id, list)
    }

    private func empty(_ text: String) -> some View {
        Text(text)
            .font(.app(11)).foregroundStyle(.tertiary)
            .padding(.horizontal, 14).padding(.vertical, 10)
    }

    // MARK: - Adding

    @ViewBuilder
    private var adder: some View {
        switch schema.kind {
        case .enum:
            adding(L10n[.CardEditor.adderNewValue], text: $newValueName, commit: commitValue)
        case .store:
            adding(L10n[.CardEditor.adderNewHold], text: $newHold, commit: commitHold)
        default:
            adding(L10n[.CardEditor.adderNewField], text: $newFieldName, commit: commitField)
        }
    }

    private func adding(_ prompt: String, text: Binding<String>, commit: @escaping () -> Void) -> some View {
        HStack(spacing: 6) {
            TextField(prompt, text: text)
                .textFieldStyle(.roundedBorder)
                .font(.app(11.5, design: .monospaced))
                .onSubmit(commit)
            Button(L10n[.Common.actionAddItem], action: commit)
                .disabled(text.wrappedValue.trimmingCharacters(in: .whitespaces).isEmpty)
        }
        .padding(10)
    }

    private func commitField() {
        let name = newFieldName.trimmingCharacters(in: .whitespaces)
        guard !name.isEmpty else { return }
        model.addField(to: schema.id, name: name)
        if model.lastError == nil { newFieldName = "" }
    }

    private func commitValue() {
        let value = newValueName.trimmingCharacters(in: .whitespaces)
        guard !value.isEmpty else { return }
        model.addValue(to: schema.id, value: value)
        if model.lastError == nil { newValueName = "" }
    }

    private func commitHold() {
        let path = newHold.trimmingCharacters(in: .whitespaces)
        guard !path.isEmpty else { return }
        model.setHolds(schema.id, holds + [path])
        if model.lastError == nil { newHold = "" }
    }
}

/// One row of an enum's values or a store's holds, lit when picked or pointed at.
private struct SimpleRow<Content: View>: View {
    var picked: Bool
    var theme: GraphTheme
    @ViewBuilder var content: Content

    @State private var hovered = false

    var body: some View {
        HStack(spacing: 8) {
            content
            Spacer(minLength: 6)
        }
        .padding(.horizontal, 14).padding(.vertical, 4)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(picked ? Color(nsColor: theme.selection)
                    : hovered ? Color(nsColor: theme.ink).opacity(0.10) : .clear)
        .contentShape(Rectangle())
        .onHover { hovered = $0 }
    }
}
