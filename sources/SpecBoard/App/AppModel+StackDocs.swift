import Foundation

/// A stackdoc's versions written from the window
/// (`specboard://SpecBoard/Docs/Design:CardKinds`「積み重ね文書」): the version it has open — how it looks, no
/// commit, and a locked main takes it too — one added on top, let go of, moved up or down, and one's
/// name, key, heading or body changed. Each through the tools as MCP writes it: a version by its path.
extension AppModel {
    /// The version a stackdoc has open: nil for the one on top.
    func openVersion(_ id: String, _ name: String?) {
        write { try tool("card_update", target: cardPath(id), value: ["version": name ?? NSNull()]) }
    }

    /// A version added on top, and open: its key as given, its name made from it.
    func addVersion(_ id: String, key: String) {
        guard let card = schema(id) else { return }
        let name = StackVersion.name(for: key, taken: Set(card.versions.map(\.name)))
        write {
            let path = try cardPath(id)
            _ = try tool("card_create", target: PathText.member(path, name), value: ["key": key])
            _ = try tool("card_update", target: path, value: ["version": name])
        }
    }

    /// A version let go of.
    func deleteVersion(_ id: String, _ name: String) {
        write {
            let path = try cardPath(id)
            _ = try tool("card_delete", target: PathText.member(path, name))
        }
    }

    /// A version moved up (toward the top, the newer) or down: the versions written whole in
    /// their new order, each keeping what it is.
    func moveVersion(_ id: String, _ name: String, by offset: Int) {
        guard let card = schema(id), let at = card.versions.firstIndex(where: { $0.name == name }),
              card.versions.indices.contains(at + offset) else { return }
        var versions = card.versions
        versions.swapAt(at, at + offset)
        write { try tool("card_update", target: cardPath(id), value: ["versions": versions.map(\.wire)]) }
    }

    /// A version's name, key, heading or body changed: `value` as `card_update` of the version's
    /// path takes it. A new name takes the paths that name it along, and the version open.
    func updateVersion(_ id: String, _ name: String, _ value: [String: Any]) {
        write {
            let path = try cardPath(id)
            _ = try tool("card_update", target: PathText.member(path, name), value: value)
        }
    }
}
