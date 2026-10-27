import Foundation

/// What a diagram that holds groups does with them (`CardCanvasView`'s group callbacks): the diagram's id, the group
/// picked on it, and where what is done to one goes. The writing ones are nil where groups may not be written: the
/// frames are still drawn and picked there, to be read.
struct MapGroupHooks {
    /// The diagram: a section's id, or a container card's for its card map.
    var view: String
    /// The group picked on it, by its name.
    var picked: String?
    var pick: (String?) -> Void
    /// A group made at a cell, by its name: as big as a new group is.
    var add: ((String, GridPoint) -> Void)?
    /// A group made round the boxes picked (their ids on the diagram), by its name.
    var frame: ((String, [String]) -> Void)?
    /// A group dropped with its top-left cell at a cell: what is in it goes with it.
    var move: ((String, GridPoint) -> Void)?
    /// A group sized by its corner, in cells: nothing in it moves.
    var resize: ((String, Int, Int) -> Void)?
}

extension AppModel {
    /// Whether the map of section `id` holds groups: a section of the tree, but not a ui section's (the map is the
    /// mock itself, framed with frame parts) nor a working directory's (its files are not boxes the core moves).
    static func holdsGroups(section id: String, in detail: ProjectDetail?) -> Bool {
        guard let section = detail?.directory.section(id: id) else { return false }
        return section.kind != .ui && section.kindName != WorkDirIds.workdirKind && section.kindName != WorkDirIds.directoryKind
    }

    /// Whether groups may be written in what is open. They are where things sit, as a box's place is, so a locked
    /// main takes them; an archived project, a tree another hand holds and a merge looked at do not.
    var canWriteGroups: Bool {
        !isArchived && heldByMCP.isEmpty && heldElsewhere.isEmpty && resultView == nil
    }

    func groups(on view: String) -> [MapGroup] { mapGroups[view] ?? [] }

    /// The group picked, as the tree open has it.
    var pickedMapGroup: (ref: GroupRef, group: MapGroup)? {
        guard let ref = selectedGroup, let group = MapGroups.named(ref.name, in: groups(on: ref.view)) else { return nil }
        return (ref, group)
    }

    /// The hooks of the diagram `view`. `path`: what `around` names a box of it by, from its id on the diagram.
    func groupHooks(view: String, path: @escaping (String) -> String?) -> MapGroupHooks {
        let writable = canWriteGroups
        return MapGroupHooks(
            view: view,
            picked: selectedGroup?.view == view ? selectedGroup?.name : nil,
            pick: { [weak self] name in
                guard let self else { return }
                if let name { self.selectedGroup = GroupRef(view: view, name: name) } else if self.selectedGroup?.view == view {
                    self.selectedGroup = nil
                }
            },
            add: writable ? { [weak self] name, at in self?.addGroup(on: view, name, at: at) } : nil,
            frame: writable ? { [weak self] name, ids in self?.frameGroup(on: view, name, around: ids.compactMap(path)) } : nil,
            move: writable ? { [weak self] name, at in self?.moveGroup(on: view, name, to: at) } : nil,
            resize: writable ? { [weak self] name, w, h in self?.resizeGroup(on: view, name, w: w, h: h) } : nil)
    }

    /// The hooks of a section's map, or nil where its map holds none.
    func sectionGroupHooks(_ section: String) -> MapGroupHooks? {
        guard Self.holdsGroups(section: section, in: detail) else { return nil }
        return groupHooks(view: section) { [weak self] id in self?.sectionBoxPath(id) }
    }

    /// The hooks of a container's card map (a goal's, an own container kind's): its boxes named as the cards inside it.
    func cardMapGroupHooks(_ container: String) -> MapGroupHooks {
        groupHooks(view: container) { [weak self] id in self?.schema(GoalCardMap.card(of: id))?.name }
    }

    /// A box of a section's map as `around` names it: a section ghost by the section it sums up, a card ghost and a
    /// card by their paths.
    func sectionBoxPath(_ id: String) -> String? {
        guard let detail else { return nil }
        let directory = detail.directory
        if SectionGhosts.isGhost(id) { return directory.full(section: String(id.dropFirst(SectionGhosts.prefix.count))) }
        if let ghost = detail.ghosts.first(where: { $0.id == id }) { return directory.full(ghost) }
        return schema(id).map(directory.full)
    }

    // MARK: - Writing

    /// `value` written as `groups` of the diagram `view`: `section_update` of a section, `card_update` of a
    /// container. One write, read again after: what the core moved with a group is drawn where it went.
    private func writeGroups(on view: String, _ value: [String: Any], then: (() -> Void)? = nil) {
        write({
            if detail?.sections.contains(where: { $0.id == view }) == true {
                try tool("section_update", target: sectionPath(view), value: ["groups": value])
            } else {
                try tool("card_update", target: cardPath(view), value: ["groups": value])
            }
        }, then: then)
    }

    func addGroup(on view: String, _ name: String, at: GridPoint) {
        let name = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !name.isEmpty else { return }
        writeGroups(on: view, MapGroups.adding(name, at: at)) { [weak self] in self?.selectedGroup = GroupRef(view: view, name: name) }
    }

    func frameGroup(on view: String, _ name: String, around paths: [String]) {
        let name = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !name.isEmpty, !paths.isEmpty else { return }
        writeGroups(on: view, MapGroups.framing(name, around: paths)) { [weak self] in self?.selectedGroup = GroupRef(view: view, name: name) }
    }

    func moveGroup(on view: String, _ name: String, to at: GridPoint) {
        writeGroups(on: view, MapGroups.moving(name, to: at))
    }

    func resizeGroup(on view: String, _ name: String, w: Int, h: Int) {
        writeGroups(on: view, MapGroups.sizing(name, w: w, h: h))
    }

    func renameGroup(_ ref: GroupRef, to new: String) {
        let new = new.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !new.isEmpty, new != ref.name else { return }
        writeGroups(on: ref.view, MapGroups.renaming(ref.name, to: new)) { [weak self] in
            self?.selectedGroup = GroupRef(view: ref.view, name: new)
        }
    }

    /// One of its looks: `label`, `headerPlace`, `color` or `fill`; nil takes it back to none.
    func setGroupLook(_ ref: GroupRef, _ key: String, _ value: String?) {
        writeGroups(on: ref.view, MapGroups.looking(ref.name, key, value))
    }

    func deleteGroup(_ ref: GroupRef) {
        writeGroups(on: ref.view, MapGroups.removing(ref.name))
    }

    // MARK: - What is inside

    /// A box in a group, as the property panel lists it.
    struct GroupMember: Hashable {
        var id: String
        var name: String
        var kind: String
    }

    /// The top-left cell of every box placed on the diagram `view`, by its id: a section's own cards, its card
    /// ghosts, the other sections' cards and section ghosts it put somewhere — a container's cards on its card map.
    func boxCells(on view: String) -> [String: GridPoint] {
        guard let detail else { return [:] }
        var out: [String: GridPoint] = [:]
        if let container = schema(view), !detail.sections.contains(where: { $0.id == view }) {
            for (id, at) in container.itemPlaces where id != view { out[id] = at }
            return out
        }
        let directory = detail.directory
        for card in detail.schemas where card.container == nil && directory.sectionId(of: card) == view {
            guard let x = card.posX, let y = card.posY else { continue }
            out[card.id] = Grid.nearest(x: x, y: y)
        }
        for ghost in directory.ghosts(in: view) {
            guard let x = ghost.posX, let y = ghost.posY else { continue }
            out[ghost.id] = Grid.nearest(x: x, y: y)
        }
        for (id, at) in sectionPositions[view] ?? [:] { out[id] = Grid.nearest(x: at.x, y: at.y) }
        return out
    }

    /// The boxes in the group `ref`, top down: their top-left cells inside its frame.
    func groupMembers(_ ref: GroupRef) -> [GroupMember] {
        guard let group = MapGroups.named(ref.name, in: groups(on: ref.view)), let detail else { return [] }
        let directory = detail.directory
        return MapGroups.members(of: group, boxes: boxCells(on: ref.view)).compactMap { id in
            if SectionGhosts.isGhost(id) {
                let section = String(id.dropFirst(SectionGhosts.prefix.count))
                return directory.section(id: section).map {
                    GroupMember(id: id, name: directory.full(section: $0.id), kind: L10n[.Common.wordSection])
                }
            }
            if let ghost = detail.ghosts.first(where: { $0.id == id }) {
                return GroupMember(id: id, name: "\(SchemaDirectory.ghostMark) \(ghost.name)", kind: L10n[.Properties.headerCardGhost])
            }
            guard let card = schema(id) else { return nil }
            let here = directory.sectionId(of: card) == ref.view || card.container == ref.view
            return GroupMember(id: id, name: here ? card.name : directory.full(card), kind: card.kindName)
        }
    }
}
