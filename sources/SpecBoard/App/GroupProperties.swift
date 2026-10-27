import SwiftUI

/// A group picked on a diagram, in the property panel: its name and the name it shows instead, the boxes in it (read
/// only, counted from where they are), its frame — size, the corner its name is in, its colour and fill — and the
/// way to let go of it, its cards staying where they are. Written where groups may be written: a locked main too.
struct GroupProperties: View {
    var model: AppModel
    var ref: GroupRef
    var group: MapGroup
    var editable: Bool

    @Environment(\.colorScheme) private var colorScheme

    var body: some View {
        PropertyCategory(title: L10n[.Common.wordName]) {
            PropertyRow("name", help: L10n[.Properties.groupNameHelp]) {
                if editable {
                    CommitField(L10n[.Diagram.groupNamePlaceholder], text: group.name, monospaced: true) { model.renameGroup(ref, to: $0) }
                        .id("group-name-\(ref.view)-\(group.name)")
                } else {
                    Text(group.name).font(.app(11.5, design: .monospaced)).textSelection(.enabled)
                }
            }
            PropertyRow("label", help: L10n[.Properties.groupLabelHelp]) {
                if editable {
                    CommitField(L10n[.Properties.groupLabelPlaceholder], text: group.label ?? "") { written in
                        let trimmed = written.trimmingCharacters(in: .whitespacesAndNewlines)
                        guard trimmed != (group.label ?? "") else { return }
                        model.setGroupLook(ref, "label", trimmed.isEmpty ? nil : trimmed)
                    }
                    .id("group-label-\(ref.view)-\(group.name)")
                } else {
                    Text(group.label ?? "").font(.app(11.5))
                }
            }
        }
        let members = model.groupMembers(ref)
        PropertyCategory(title: L10n[.Properties.groupInside], count: members.count) {
            ForEach(members, id: \.id) { member in
                HStack(spacing: 6) {
                    Text(member.name).font(.app(11.5, design: .monospaced)).lineLimit(1).truncationMode(.middle)
                    Spacer(minLength: 4)
                    Text(member.kind).font(.app(10.5)).foregroundStyle(.secondary).lineLimit(1)
                }
                .padding(.horizontal, 18).padding(.vertical, 3)
            }
            Text(L10n[.Properties.groupInsideNote])
                .font(.app(10.5)).foregroundStyle(.tertiary)
                .fixedSize(horizontal: false, vertical: true)
                .padding(.horizontal, 18).padding(.vertical, 4)
        }
        PropertyCategory(title: L10n[.Properties.groupBox]) {
            PropertyRow("size", help: L10n[.Properties.groupSizeHelp(MapGroups.least.w, MapGroups.least.h, MapGroups.most)]) {
                HStack(spacing: 4) {
                    Text("w").foregroundStyle(.secondary)
                    side(group.cells.w, "w") { model.resizeGroup(on: ref.view, ref.name, w: $0, h: group.cells.h) }
                    Text("×").foregroundStyle(.secondary)
                    Text("h").foregroundStyle(.secondary)
                    side(group.cells.h, "h") { model.resizeGroup(on: ref.view, ref.name, w: group.cells.w, h: $0) }
                    Text(L10n[.Properties.groupCells]).foregroundStyle(.secondary)
                }
            }
            PropertyRow("headerPlace", help: L10n[.Properties.groupHeaderPlaceHelp]) {
                if editable {
                    Picker("", selection: Binding(get: { group.headerPlace }, set: { place in
                        model.setGroupLook(ref, "headerPlace", place == .topLeft ? nil : place.rawValue)
                    })) {
                        ForEach(MapGroup.HeaderPlace.allCases, id: \.self) { Text(Self.word($0)).tag($0) }
                    }
                    .labelsHidden().pickerStyle(.menu).fixedSize()
                } else {
                    Text(Self.word(group.headerPlace))
                }
            }
            PropertyRow("color", help: L10n[.Properties.groupColorHelp]) {
                HStack(spacing: 5) {
                    swatch(nil)
                    ForEach(MapGroup.Color.allCases, id: \.self) { swatch($0) }
                    Text(Self.word(group.color)).font(.app(10.5)).foregroundStyle(.secondary)
                }
            }
            PropertyRow("fill", help: L10n[.Properties.groupFillHelp]) {
                if editable {
                    Picker("", selection: Binding(get: { group.fill }, set: { fill in
                        model.setGroupLook(ref, "fill", fill == .none ? nil : fill.rawValue)
                    })) {
                        ForEach(MapGroup.Fill.allCases, id: \.self) { fill in
                            Label {
                                Text(Self.word(fill))
                            } icon: {
                                Image(nsImage: GroupSwatch.image(fill, color: group.color, dark: colorScheme == .dark))
                            }
                            .tag(fill)
                        }
                    }
                    .labelsHidden().pickerStyle(.menu).fixedSize()
                } else {
                    Label {
                        Text(Self.word(group.fill))
                    } icon: {
                        Image(nsImage: GroupSwatch.image(group.fill, color: group.color, dark: colorScheme == .dark))
                    }
                }
            }
        }
        if editable {
            HStack(spacing: 8) {
                Button(L10n[.Properties.groupDelete]) { model.deleteGroup(ref) }
                Text(L10n[.Properties.groupDeleteNote]).font(.app(10.5)).foregroundStyle(.secondary)
            }
            .padding(.horizontal, 18).padding(.vertical, 8)
        }
    }

    /// One side of the size, in cells: written when it is a whole number, else let be.
    @ViewBuilder
    private func side(_ cells: Int, _ key: String, _ set: @escaping (Int) -> Void) -> some View {
        if editable {
            CommitField("", text: "\(cells)", monospaced: true) { written in
                guard let value = Int(written.trimmingCharacters(in: .whitespaces)), value != cells else { return }
                set(value)
            }
            .frame(width: 52)
            .id("group-\(key)-\(ref.view)-\(group.name)-\(cells)")
        } else {
            Text("\(cells)").font(.app(11.5, design: .monospaced))
        }
    }

    /// A colour to choose (nil: none), the one chosen ringed.
    private func swatch(_ color: MapGroup.Color?) -> some View {
        let chosen = group.color == color
        let dark = colorScheme == .dark
        return Button {
            model.setGroupLook(ref, "color", color?.rawValue)
        } label: {
            ZStack {
                RoundedRectangle(cornerRadius: 3)
                    .fill(color.map { Color(nsColor: $0.ink.color(dark: dark)) } ?? Color.clear)
                    .frame(width: 14, height: 14)
                if color == nil {
                    RoundedRectangle(cornerRadius: 3).stroke(Color.secondary, lineWidth: 1).frame(width: 14, height: 14)
                    Path { path in
                        path.move(to: CGPoint(x: 2, y: 12))
                        path.addLine(to: CGPoint(x: 12, y: 2))
                    }
                    .stroke(Color.secondary, lineWidth: 1)
                    .frame(width: 14, height: 14)
                }
                if chosen {
                    RoundedRectangle(cornerRadius: 4).stroke(Color.accentColor, lineWidth: 2).frame(width: 19, height: 19)
                }
            }
            .frame(width: 20, height: 20)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(!editable)
        .help(Self.word(color))
    }

    // MARK: - Words

    /// The corner its name is in, as a ui part's header place is said.
    static func word(_ place: MapGroup.HeaderPlace) -> String {
        switch place {
        case .topLeft: L10n[.Properties.groupPlaceTopLeft]
        case .topRight: L10n[.Properties.groupPlaceTopRight]
        case .bottomLeft: L10n[.Properties.groupPlaceBottomLeft]
        case .bottomRight: L10n[.Properties.groupPlaceBottomRight]
        case .hidden: L10n[.Properties.groupPlaceHidden]
        }
    }

    static func word(_ color: MapGroup.Color?) -> String {
        switch color {
        case nil: L10n[.Properties.groupColorNone]
        case .blue?: L10n[.Properties.groupColorBlue]
        case .green?: L10n[.Properties.groupColorGreen]
        case .amber?: L10n[.Properties.groupColorAmber]
        case .violet?: L10n[.Properties.groupColorViolet]
        case .red?: L10n[.Properties.groupColorRed]
        }
    }

    static func word(_ fill: MapGroup.Fill) -> String {
        switch fill {
        case .none: L10n[.Properties.groupFillNone]
        case .check: L10n[.Properties.groupFillCheck]
        case .stripes: L10n[.Properties.groupFillStripes]
        case .diagonal: L10n[.Properties.groupFillDiagonal]
        case .dots: L10n[.Properties.groupFillDots]
        case .zebra: L10n[.Properties.groupFillZebra]
        case .zebraDiagonal: L10n[.Properties.groupFillZebraDiagonal]
        }
    }
}
