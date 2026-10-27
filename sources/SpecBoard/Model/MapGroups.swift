import Foundation

/// A group on a diagram (`specboard://SpecBoard/Docs/Design:Overview`「グループ」): a named frame put over a
/// section's map or a container's card map, on cells of its own. What is inside it is not kept with it: a box
/// whose top-left cell is inside the frame is in it, counted when it is drawn, listed or moved — the core counts
/// the same way when it carries them.
nonisolated struct MapGroup: Equatable, Hashable, Sendable {
    /// The corner its name is written in, as a ui part's header (`headerPlace`).
    enum HeaderPlace: String, CaseIterable, Sendable {
        case topLeft, topRight, bottomLeft, bottomRight, hidden

        var onTop: Bool { self != .bottomLeft && self != .bottomRight }
        var onRight: Bool { self == .topRight || self == .bottomRight }
    }

    /// Its colour, by the product's own names for colours (a line's `look.color`): drawn light or dark as the
    /// background asks.
    enum Color: String, CaseIterable, Sendable {
        case blue, green, amber, violet, red

        var ink: EdgeInk { EdgeInk(rawValue: rawValue) ?? .grey }
    }

    /// The pattern laid over its faint fill.
    enum Fill: String, CaseIterable, Sendable {
        case none, check, stripes, diagonal, dots, zebra, zebraDiagonal
    }

    var name: String
    var cells: GridRect
    /// Written instead of the name, when it has one.
    var label: String?
    var headerPlace = HeaderPlace.topLeft
    var color: Color?
    var fill = Fill.none

    /// What the frame says: its label, else its name.
    var title: String { label.flatMap { $0.isEmpty ? nil : $0 } ?? name }

    var area: Int { cells.w * cells.h }

    /// Whether `cell` is one of its cells.
    func holds(_ cell: GridPoint) -> Bool {
        cell.x >= cells.x && cell.x < cells.x + cells.w && cell.y >= cells.y && cell.y < cells.y + cells.h
    }
}

/// The rules the screen shares with the core about groups: which boxes and groups are inside one, which takes a
/// box let go, the frame round picked boxes, and the values `section_update` and `card_update` take in `groups`.
nonisolated enum MapGroups {
    /// The smallest and largest a group may be, in cells, and the size a new one has.
    static let least = (w: 6, h: 3)
    static let most = 1000
    static let made = (w: 6, h: 4)

    /// Whether two names are one group's: names are compared ignoring case, as the core compares them.
    static func same(_ a: String, _ b: String) -> Bool { a.lowercased() == b.lowercased() }

    static func named(_ name: String, in groups: [MapGroup]) -> MapGroup? { groups.first { same($0.name, name) } }

    /// The boxes in `group`, by their top-left cells: top down, then left to right.
    static func members(of group: MapGroup, boxes: [String: GridPoint]) -> [String] {
        boxes.filter { group.holds($0.value) }
            .sorted { ($0.value.y, $0.value.x, $0.key) < ($1.value.y, $1.value.x, $1.key) }
            .map(\.key)
    }

    /// The groups inside `group`: their top-left cell inside it and their frame smaller. Two of the same size hold
    /// neither, so no two groups hold each other.
    static func nested(in group: MapGroup, among groups: [MapGroup]) -> [MapGroup] {
        groups.filter { !same($0.name, group.name) && group.holds(GridPoint(x: $0.cells.x, y: $0.cells.y)) && $0.area < group.area }
    }

    /// What moving `group` carries: the boxes in it, the groups inside it and theirs, all the way down — the names
    /// with `group`'s among them.
    static func carried(_ group: MapGroup, groups: [MapGroup], boxes: [String: GridPoint]) -> (boxes: Set<String>, groups: [String]) {
        var held: Set<String> = []
        var names = [group.name]
        var todo = [group]
        while let next = todo.popLast() {
            held.formUnion(members(of: next, boxes: boxes))
            for inner in nested(in: next, among: groups) where !names.contains(where: { same($0, inner.name) }) {
                names.append(inner.name)
                todo.append(inner)
            }
        }
        return (held, names)
    }

    /// The group that takes a box or a group whose top-left cell is `cell`: the innermost of those holding the cell
    /// — the smallest, the one drawn in front among those as small — leaving out `moving` (what is being carried) and,
    /// for a group, those no larger than it (`larger`).
    static func receiver(at cell: GridPoint, among groups: [MapGroup], moving: [String] = [], larger area: Int? = nil) -> MapGroup? {
        let takers = drawingOrder(groups).filter { group in
            group.holds(cell) && !moving.contains { same($0, group.name) } && (area.map { group.area > $0 } ?? true)
        }
        guard let least = takers.map(\.area).min() else { return nil }
        return takers.last { $0.area == least }
    }

    /// Back to front: the larger behind, those as large by name.
    static func drawingOrder(_ groups: [MapGroup]) -> [MapGroup] {
        groups.sorted { $0.area != $1.area ? $0.area > $1.area : $0.name.lowercased() < $1.name.lowercased() }
    }

    /// The frame round boxes on `cells`, as `around` makes it: a cell to the left, the right and below, two above for
    /// the name's strip, grown to the smallest a group may be. A side whose room would lie over the frame of one of
    /// `beside` holding none of the boxes (not the group it goes in) has its room cut down to none, as the core cuts
    /// it: the frames do not overlap, and the new group is not taken for a group inside that one.
    static func around(_ cells: [GridRect], beside groups: [MapGroup] = []) -> GridRect? {
        guard let left = cells.map(\.x).min(), let top = cells.map(\.y).min(),
              let right = cells.map({ $0.x + $0.w }).max(), let bottom = cells.map({ $0.y + $0.h }).max() else { return nil }
        var (l, t, r, b) = (1, 2, 1, 1)
        let others = groups.filter { group in !cells.contains { group.holds(GridPoint(x: $0.x, y: $0.y)) } }
        for other in others.map(\.cells) {
            let frame = GridRect(x: left - l, y: top - t, w: right - left + l + r, h: bottom - top + t + b)
            guard frame.intersects(other) else { continue }
            if other.x + other.w <= left { l = max(0, min(l, left - (other.x + other.w))) }
            if other.x >= right { r = max(0, min(r, other.x - right)) }
            if other.y + other.h <= top { t = max(0, min(t, top - (other.y + other.h))) }
            if other.y >= bottom { b = max(0, min(b, other.y - bottom)) }
        }
        return GridRect(x: left - l, y: top - t, w: max(right - left + l + r, least.w), h: max(bottom - top + t + b, least.h))
    }

    // MARK: - What is written (`groups` of `section_update` and `card_update`)

    static func point(_ at: GridPoint) -> [String: Any] { ["x": at.x, "y": at.y] }

    /// A new group with its top-left cell at `at`, as big as a new group is. Made only (`new`): a name already on
    /// the map, but for case, is refused rather than taken for that group and moved with what is in it.
    static func adding(_ name: String, at: GridPoint) -> [String: Any] { [name: ["topLeft": point(at), "new": true]] }

    /// A new group framing the boxes `paths` names, made only as `adding` is.
    static func framing(_ name: String, around paths: [String]) -> [String: Any] { [name: ["around": paths, "new": true]] }

    /// `name` moved to `at`, what is in it carried.
    static func moving(_ name: String, to at: GridPoint) -> [String: Any] { [name: ["topLeft": point(at)]] }

    /// `name` made `w` by `h` cells: nothing in it moves.
    static func sizing(_ name: String, w: Int, h: Int) -> [String: Any] { [name: ["size": ["w": w, "h": h]]] }

    static func renaming(_ name: String, to new: String) -> [String: Any] { [name: ["name": new]] }

    /// One of its looks (`label`, `headerPlace`, `color`, `fill`): nil takes it back to none.
    static func looking(_ name: String, _ key: String, _ value: String?) -> [String: Any] {
        [name: [key: value.map { $0 as Any } ?? NSNull()]]
    }

    /// `name` let go of: the cards in it stay.
    static func removing(_ name: String) -> [String: Any] { [name: NSNull()] }
}

/// A group as the property panel names it: the diagram it is on (a section's id, or a container card's) and its name.
nonisolated struct GroupRef: Hashable, Sendable {
    var view: String
    var name: String
}
