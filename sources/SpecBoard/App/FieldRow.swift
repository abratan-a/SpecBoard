import SwiftUI

/// One field in a card editor: its name, type and flags, and under them what
/// it is for.
///
/// A view of its own rather than a method on `CardEditor`, and that is the whole
/// point. With the hover flag and the selection read in the pane's body, moving the
/// pointer down a 164-field schema re-ran that body on every frame: the tree was
/// flattened again, every visible row was rebuilt, and each of those rebuilt rows
/// worked out its own copy-path list — a project-wide walk, 0.46ms each. Thirty rows of
/// that is more than a frame, which is exactly what scrolling looked like.
///
/// Here the hover flag is the row's own state and the paths arrive precomputed, so
/// pointing at a row repaints that row.
struct FieldRow: View {
    var model: AppModel
    var schemaId: String
    var field: FieldNode
    var depth: Int
    /// Path within the schema, `warehouse.primary.address`.
    var dotted: String
    var open: Bool
    var editable: Bool
    /// The names the owning card goes by, worked out once by `CardEditor`.
    var prefixes: [String]
    var onToggleFold: () -> Void
    var onUnfold: () -> Void
    /// Selecting goes through the pane rather than straight to the model, so the pane
    /// knows the selection came from a row it is already showing.
    var onPick: () -> Void
    var onNavigate: (String) -> Void

    @Environment(\.colorScheme) private var colorScheme
    private var theme: GraphTheme { colorScheme == .dark ? .dark : .light }

    @State private var hovered = false

    var body: some View {
        HStack(alignment: .top, spacing: 6) {
            rails

            disclosure

            VStack(alignment: .leading, spacing: 1) {
                HStack(spacing: 6) {
                    Text(field.name)
                        .font(.app(11.5, design: .monospaced))
                        .foregroundStyle(nameColour)

                    if !field.label.isEmpty {
                        Text(field.label)
                            .font(.app(10.5)).foregroundStyle(.tertiary)
                            .lineLimit(1)
                    }

                    Spacer(minLength: 6)

                    badges
                    typeChip
                }
                // What the field is for, read with it rather than a click away.
                if !field.description.isEmpty {
                    Text(field.description)
                        .font(.app(10.5)).foregroundStyle(.secondary)
                        .lineLimit(2)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 3)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(fill)
        .contentShape(Rectangle())
        // Same weight as the canvas's row hover, so a row lights up the same way in
        // both places.
        .onHover { hovered = $0 }
        .onTapGesture(perform: onPick)
        .contextMenu {
            if editable { editItems; Divider() }
            pathMenu
        }
    }

    /// The one handle for the subtree.
    ///
    /// Folding is on this handle and not on the whole row, so a parent can be selected
    /// without its children disappearing — selecting and hiding are different
    /// intentions and take different clicks. A glyph about eleven points wide on its
    /// own is not something a pointer can be expected to find.
    ///
    /// A chevron that turns, the way every outline does it: pointing right when shut,
    /// down when open. The box around it is the target, and it is the full height of
    /// the row so there is no vertical aiming to do either.
    private var disclosure: some View {
        Group {
            if field.children.isEmpty {
                Color.clear
            } else {
                Button(action: onToggleFold) {
                    Image(systemName: "chevron.right")
                        .font(.app(9, weight: .semibold))
                        .foregroundStyle(.secondary)
                        .rotationEffect(.degrees(open ? 90 : 0))
                        .animation(.easeOut(duration: 0.12), value: open)
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .help(open ? L10n[.Properties.foldCollapse] : L10n[.Properties.foldExpand])
            }
        }
        .frame(width: 18, height: 20)
    }

    private var fill: Color {
        if model.selectedFieldId == field.id { return Color(nsColor: theme.selection) }
        return hovered ? Color(nsColor: theme.ink).opacity(0.10) : .clear
    }

    /// Taking the name of the thing away with you.
    @ViewBuilder
    private var pathMenu: some View {
        if prefixes.count == 1 {
            Button(L10n[.Common.actionCopyPath]) { copyToPasteboard(prefixes[0] + "." + dotted) }
        } else if prefixes.count > 1 {
            Menu(L10n[.Common.actionCopyPath]) {
                ForEach(prefixes, id: \.self) { prefix in
                    let path = prefix + "." + dotted
                    Button(path) { copyToPasteboard(path) }
                }
                if prefixes.count >= pathCeiling {
                    Divider()
                    // Say so rather than letting a truncated list read as everything.
                    Text(L10n[.Properties.pathCutOff(pathCeiling)])
                }
            }
        }
    }

    /// Adding under a row and taking one out, from the row itself.
    @ViewBuilder
    private var editItems: some View {
        if field.type.isObject {
            Button(L10n[.Properties.menuAddFieldInside]) {
                onUnfold()
                model.addField(to: schemaId, parentFieldId: field.id, name: "field")
            }
        }
        Button(L10n[.Properties.menuDeleteField(field.name)], role: .destructive) {
            model.deleteField(field.id)
        }
    }

    /// Depth shown as vertical rules, one per level, rather than a triangle each.
    private var rails: some View {
        HStack(spacing: 0) {
            ForEach(0..<max(depth, 0), id: \.self) { _ in
                Rectangle()
                    .fill(Color(nsColor: theme.teal).opacity(0.28))
                    .frame(width: 1)
                    .padding(.leading, 5)
                    .padding(.trailing, 7)
            }
        }
        .frame(minHeight: 15)
    }

    private var nameColour: Color {
        if field.type.isRef { return Color(nsColor: theme.brassInk) }
        if field.type.isObject { return Color(nsColor: theme.teal) }
        return .primary
    }

    @ViewBuilder
    private var typeChip: some View {
        // Written through a card ghost, it holds the ghost's id: the card it leads to.
        if field.type.isRef, let target = field.embeds, let other = model.card(through: target) {
            Button { onNavigate(other.id) } label: {
                Text((other.id == schemaId ? "↺" : "→") + other.name
                     + (field.isArray ? "[]" : ""))
                    .font(.app(10, design: .monospaced))
                    .foregroundStyle(Color(nsColor: theme.brass))
            }
            .buttonStyle(.plain)
        } else {
            Text(field.type.rawValue + (field.isArray ? "[]" : ""))
                .font(.app(10, design: .monospaced))
                .foregroundStyle(field.type.isObject
                                 ? Color(nsColor: theme.teal) : .secondary)
        }
    }

    @ViewBuilder
    private var badges: some View {
        HStack(spacing: 3) {
            if field.isPk { badge(BuiltinWords.recordField("primaryKey")) }
            if field.isUnique { badge(BuiltinWords.recordField("unique")) }
            if field.isRequired { badge(BuiltinWords.recordField("required")) }
            // Shown separately from 必須 on purpose: presence and nullability are
            // different questions and a strict wire format needs both.
            if field.isNullable { badge(BuiltinWords.recordField("nullable")) }
            if let constValue = field.constValue, !constValue.isNull {
                badge("= \(constValue.display)")
            }
            if !field.rules.isEmpty { badge("\(BuiltinWords.recordField("rules")) \(field.rules.count)") }
            // What its lines point at besides what it embeds (the core's `relations`).
            let pointing = model.detail?.relations.filter { $0.fieldId == field.id && $0.line != Line.embed }.count ?? 0
            if pointing > 0 { badge("→ \(pointing)") }
            if !field.variants.isEmpty { badge(field.variants.joined(separator: ",")) }
            if let also = field.also, !also.isEmpty { badge("\(BuiltinWords.recordField("also")): \(also.joined(separator: ","))") }
        }
    }

    private func badge(_ text: String) -> some View {
        Text(text)
            .font(.app(9))
            .padding(.horizontal, 4).padding(.vertical, 1)
            .background(RoundedRectangle(cornerRadius: 3)
                .fill(Color.secondary.opacity(0.14)))
            .foregroundStyle(.secondary)
            .lineLimit(1)
    }
}
