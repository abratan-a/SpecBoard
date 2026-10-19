import AppKit
import SwiftUI

/// The tab bar (`specboard://SpecBoard/Docs/Design:UI` §4): under the top bar and the banners, across the
/// middle and the property panel. First the map tab — the section open, drawn as its map, never closed
/// (the nav panel changes what it shows) — then a tab for each thing opened (`AppModel.tabs`): a card's
/// editor, a card's connections map, the project's settings, the inspection, a tree's dashboard. Pressed,
/// a tab comes forward; its × (⌘W for the one in front) closes it.
///
/// Where the tabs do not fit, they shrink from what they say to a least width — a card's badge goes
/// first, then the name is cut at its end — the map tab and the one in front keeping theirs; what does
/// not fit even so is listed under 「» N」 at the right end, with 「ほかのタブを閉じる」.
struct TabBar: View {
    var model: AppModel

    @State private var width: CGFloat = 0
    @State private var moreOpen = false

    var body: some View {
        let items = TabItem.all(model)
        let front = model.activeTab.map { $0 + 1 } ?? 0
        let natural = items.map(\.natural)
        let laid = TabBarLayout.lay(natural: natural, front: front, room: width,
                                    least: TabBarLayout.least, more: TabBarLayout.more)
        HStack(spacing: 0) {
            ForEach(items.indices.filter { !laid.hidden.contains($0) }, id: \.self) { at in
                tab(items[at], width: laid.widths[at], front: at == front)
            }
            Spacer(minLength: 0)
            if !laid.hidden.isEmpty {
                more(laid.hidden.map { items[$0] })
            }
        }
        .frame(maxWidth: .infinity, minHeight: TabBarLayout.height, maxHeight: TabBarLayout.height, alignment: .leading)
        .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { width = $0 }
    }

    /// One tab, `width` wide: pressed it comes forward, its × closes it.
    private func tab(_ item: TabItem, width: CGFloat, front: Bool) -> some View {
        ZStack(alignment: .trailing) {
            Button { model.open(item.place) } label: {
                HStack(spacing: 0) {
                    if let symbol = item.symbol {
                        Image(systemName: symbol).font(.app(11))
                            .foregroundStyle(item.tint ?? .secondary)
                            .frame(width: TabItem.symbolWidth)
                            .padding(.trailing, TabItem.symbolGap)
                    }
                    // Shrunk below what it says, a tab lets its badge go first.
                    if let badge = item.badge, width >= item.natural - 0.5 {
                        KindBadge(kind: item.kind ?? .custom, text: badge)
                            .padding(.trailing, TabItem.badgeGap)
                    }
                    Text(item.name)
                        .font(item.font(front: front))
                        .foregroundStyle(front ? .primary : .secondary)
                        .lineLimit(1).truncationMode(.tail)
                    Spacer(minLength: 0)
                }
                .padding(.leading, TabItem.padding)
                .padding(.trailing, TabItem.padding + (item.closes ? TabItem.closeGap + TabItem.closeWidth : 0))
                .frame(width: width, height: TabBarLayout.height)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .help(item.help)
            if let index = item.index {
                Button { model.closeTab(index) } label: {
                    Image(systemName: "xmark").font(.app(8.5, weight: .semibold))
                        .frame(width: TabItem.closeWidth, height: TabItem.closeWidth)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .foregroundStyle(.secondary)
                .help(L10n[.Sections.tabClose])
                .padding(.trailing, TabItem.padding)
            }
        }
        .frame(width: width, height: TabBarLayout.height)
        .background(front ? Color.primary.opacity(0.07) : .clear)
        .overlay(alignment: .top) {
            if front { Rectangle().fill(Color.accentColor).frame(height: 2) }
        }
        .overlay(alignment: .trailing) {
            Rectangle().fill(Color(nsColor: .separatorColor)).frame(width: 1, height: 14)
        }
    }

    /// 「» N」: the tabs that do not fit, to bring one forward, and the way to close all but the one in front.
    private func more(_ hidden: [TabItem]) -> some View {
        Button { moreOpen.toggle() } label: {
            Text("» \(hidden.count)").font(.app(11.5))
                .frame(width: TabBarLayout.more, height: TabBarLayout.height)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help(L10n[.Sections.tabMore(hidden.count)])
        .popover(isPresented: $moreOpen, arrowEdge: .bottom) {
            TabMoreList(model: model, hidden: hidden) { moreOpen = false }
        }
    }
}

/// What 「» N」 lists: the tabs that do not fit, each by its mark or badge and its name — pressed, it comes
/// forward — and under them 「ほかのタブを閉じる」.
struct TabMoreList: View {
    var model: AppModel
    var hidden: [TabItem]
    /// Something was chosen: the list goes.
    var done: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(hidden) { item in
                TabMenuRow { done(); model.open(item.place) } label: {
                    HStack(spacing: 6) {
                        if let symbol = item.symbol {
                            Image(systemName: symbol).font(.app(11)).foregroundStyle(item.tint ?? .secondary)
                                .frame(width: TabItem.symbolWidth)
                        }
                        if let badge = item.badge { KindBadge(kind: item.kind ?? .custom, text: badge) }
                        Text(item.name).font(item.font(front: false)).lineLimit(1).truncationMode(.tail)
                    }
                }
                .help(item.help)
            }
            Divider().padding(.vertical, 4)
            TabMenuRow { done(); model.closeOtherTabs() } label: {
                Text(L10n[.Sections.tabCloseOthers]).font(.app(12))
            }
        }
        .padding(6)
        .frame(minWidth: 220, maxWidth: 420, alignment: .leading)
    }
}

/// A row of the 「» N」 list: lit under the pointer, as a menu's rows are.
private struct TabMenuRow<Label: View>: View {
    var action: () -> Void
    @ViewBuilder var label: Label
    @State private var lit = false

    var body: some View {
        Button(action: action) {
            label
                .padding(.horizontal, 8).padding(.vertical, 4)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(RoundedRectangle(cornerRadius: 4).fill(lit ? Color.primary.opacity(0.1) : .clear))
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { lit = $0 }
    }
}

/// One tab as it is drawn and measured: what it says, how wide that is, and the place it opens.
struct TabItem: Identifiable {
    /// Its index in `AppModel.tabs`; nil for the map tab.
    var index: Int?
    var place: AppModel.Place
    /// A mark before the name (the map tab's section kind, the settings' gear …); a card has its badge instead.
    var symbol: String?
    var tint: Color?
    /// A card's kind, as its badge says it.
    var badge: String?
    var kind: SchemaKind?
    var name: String
    /// A card's name, set as code is.
    var mono = false
    /// On the pointer: the whole of what it is (a section's or a card's path).
    var help: String

    var id: String { index.map(String.init) ?? "map" }
    var closes: Bool { index != nil }

    static let padding: CGFloat = 10
    static let symbolWidth: CGFloat = 14
    static let symbolGap: CGFloat = 5
    static let badgeGap: CGFloat = 6
    static let closeGap: CGFloat = 6
    static let closeWidth: CGFloat = 14
    /// The most of the map tab's name shown: past it the name is cut, the path on the pointer.
    static let mapNameMost: CGFloat = 220

    @MainActor
    func font(front: Bool) -> Font {
        .app(12, weight: front ? .semibold : .regular, design: mono ? .monospaced : .default)
    }

    /// As wide as it is with all of it said (the name as it is in front, the wider way).
    @MainActor
    var natural: CGFloat {
        let scale = Typography.shared.scale
        let size = 12 * scale
        let nameFont = mono ? NSFont.monospacedSystemFont(ofSize: size, weight: .semibold) : NSFont.systemFont(ofSize: size, weight: .semibold)
        var name = Self.measure(self.name, nameFont)
        if index == nil { name = min(name, Self.mapNameMost * scale) }
        var lead: CGFloat = 0
        if symbol != nil { lead = Self.symbolWidth + Self.symbolGap }
        if let badge {
            lead = Self.measure(badge, NSFont.monospacedSystemFont(ofSize: 10 * scale, weight: .regular)) + 10 + Self.badgeGap
        }
        let close = closes ? Self.closeGap + Self.closeWidth : 0
        return ceil(2 * Self.padding + lead + name + close)
    }

    private static func measure(_ text: String, _ font: NSFont) -> CGFloat {
        ceil((text as NSString).size(withAttributes: [.font: font]).width) + 2
    }

    /// The map tab and a tab for each of `model.tabs`, in the bar's order.
    @MainActor
    static func all(_ model: AppModel) -> [TabItem] {
        guard let detail = model.detail else { return [] }
        let directory = detail.directory
        let key = model.mapSectionKey
        let section = directory.section(id: key)
        let mark = (section?.kind?.rawValue ?? section?.kindName).flatMap { detail.sectionMarks[$0] } ?? SectionMark()
        let map = TabItem(index: nil, place: .section(key), symbol: mark.symbol,
                          tint: mark.color.flatMap(HexColor.color(hex:)), name: section?.name ?? "",
                          help: directory.full(section: key))
        return [map] + model.tabs.enumerated().map { at, place in
            item(at, place, model, directory)
        }
    }

    @MainActor
    private static func item(_ at: Int, _ place: AppModel.Place, _ model: AppModel,
                             _ directory: SchemaDirectory) -> TabItem {
        switch place {
        case .card(let id):
            let card = model.schema(id)
            let path = card.map { directory.full($0) } ?? ""
            let label = card?.label ?? ""
            return TabItem(index: at, place: place, badge: card?.kindName ?? "", kind: card?.kind,
                           name: card?.name ?? "", mono: true, help: label.isEmpty ? path : "\(path)\n\(label)")
        case .connections(let id, _):
            let card = model.schema(id)
            return TabItem(index: at, place: place, symbol: "point.3.filled.connected.trianglepath.dotted",
                           name: L10n[.Sections.tabConnections(card?.name ?? "")],
                           help: "\(DiagramMenus.connectionsWord) · \(card.map { directory.full($0) } ?? "")")
        case .settings:
            return TabItem(index: at, place: place, symbol: "gearshape", name: L10n[.Nav.rowProjectSettings],
                           help: L10n[.Nav.rowProjectSettings])
        case .inspection:
            return TabItem(index: at, place: place, symbol: "stethoscope", name: L10n[.Nav.rowInspection],
                           help: L10n[.Nav.rowInspection])
        case .root:
            // The board's tree has an internal name; people know it by the nav panel's word.
            let tree = model.isBoardOpen ? L10n[.Common.wordProjectBoard] : model.openTree.name
            return TabItem(index: at, place: place, symbol: "square.grid.2x2", name: tree, help: tree)
        case .section(let id):
            return TabItem(index: at, place: place, name: directory.section(id: id)?.name ?? "",
                           help: directory.full(section: id))
        }
    }
}

/// How the tab bar lays its tabs out where they do not all fit (`TabBar`).
enum TabBarLayout {
    static let height: CGFloat = 30
    /// The least a tab shrinks to before it goes under 「» N」.
    @MainActor static var least: CGFloat { 104 * Typography.shared.scale }
    /// The room 「» N」 takes at the right end.
    @MainActor static var more: CGFloat { 44 * Typography.shared.scale }

    struct Laid: Equatable {
        /// By tab, the map tab first: how wide it is drawn (0 for one under 「» N」).
        var widths: [CGFloat]
        /// The tabs under 「» N」, in the bar's order.
        var hidden: [Int]
    }

    /// The tabs of `natural` widths (the map tab first, `front` the one in front) in a bar `room` wide:
    /// all as wide as they say while they fit; else the map tab and the one in front keep their width and
    /// the others, from the left, each take what they can down to `least` (a tab narrower than that keeps
    /// its own), and those that do not fit even so go under 「» N」, which takes `more`.
    static func lay(natural: [CGFloat], front: Int, room: CGFloat, least: CGFloat, more: CGFloat) -> Laid {
        guard natural.reduce(0, +) > room, room > 0 else { return Laid(widths: natural, hidden: []) }
        let kept: Set<Int> = [0, front]
        let others = natural.indices.filter { !kept.contains($0) }
        let fixed = kept.reduce(0) { $0 + natural[$1] }
        let floor = { (at: Int) in min(natural[at], least) }
        var shown: [Int] = []
        var used: CGFloat = 0
        for (count, at) in others.enumerated() {
            let left = others.count - count - 1
            guard fixed + used + floor(at) + (left > 0 ? more : 0) <= room else { break }
            shown.append(at)
            used += floor(at)
        }
        let hidden = others.filter { !shown.contains($0) }
        let room = room - fixed - (hidden.isEmpty ? 0 : more)
        // The most a shrunk tab takes: as much as leaves them all inside the room.
        let width = { (most: CGFloat, at: Int) in min(natural[at], max(most, floor(at))) }
        var low = least, high = max(least, shown.map { natural[$0] }.max() ?? least)
        for _ in 0..<40 {
            let mid = (low + high) / 2
            if shown.reduce(0, { $0 + width(mid, $1) }) <= room { low = mid } else { high = mid }
        }
        var widths = natural
        for at in hidden { widths[at] = 0 }
        for at in shown { widths[at] = width(low, at).rounded(.down) }
        return Laid(widths: widths, hidden: hidden)
    }
}
