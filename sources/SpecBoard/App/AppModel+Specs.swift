import Foundation

/// A spec written from the window (`specboard://SpecBoard/Docs/Design:CardKinds`「仕様（spec）」): its story's fields, and
/// its scenarios — one added last, its title and steps written, moved up or down, let go of — each by its path, through
/// the tools as MCP writes it.
extension AppModel {
    /// One of its story's fields (`as`, `iWant`, `soThat`, `about`).
    func setSpecField(_ id: String, _ key: String, _ text: String) {
        write { _ = try tool("card_update", target: cardPath(id), value: [key: text]) }
    }

    /// A scenario added last, named from the names there are: its name, none where the core refused it.
    @discardableResult
    func addScenario(_ id: String) -> String? {
        guard let card = schema(id) else { return nil }
        let name = SpecScenario.name(taken: Set(card.scenarios.map(\.name)))
        write {
            let path = try cardPath(id)
            _ = try tool("card_create", target: PathText.member(path, name), value: ["steps": [] as [Any]])
        }
        return lastError == nil ? name : nil
    }

    /// A scenario's title, its steps (written whole), or both.
    func updateScenario(_ id: String, _ name: String, heading: String?, steps: [SpecStep]?) {
        var value: [String: Any] = [:]
        if let heading { value["heading"] = heading }
        if let steps { value["steps"] = steps.map(\.wire) }
        guard !value.isEmpty else { return }
        write {
            let path = try cardPath(id)
            _ = try tool("card_update", target: PathText.member(path, name), value: value)
        }
    }

    /// A scenario moved up or down: put before the one above it or after the one below, by that one's name, so a
    /// scenario added meanwhile stays.
    func moveScenario(_ id: String, _ name: String, by offset: Int) {
        guard let card = schema(id), let at = card.scenarios.firstIndex(where: { $0.name == name }),
              card.scenarios.indices.contains(at + offset) else { return }
        let beside = card.scenarios[at + offset].name
        write {
            let path = try cardPath(id)
            _ = try tool("card_update", target: PathText.member(path, name),
                         value: [offset < 0 ? "before" : "after": beside])
        }
    }

    /// A scenario let go of.
    func deleteScenario(_ id: String, _ name: String) {
        write {
            let path = try cardPath(id)
            _ = try tool("card_delete", target: PathText.member(path, name))
        }
    }
}
