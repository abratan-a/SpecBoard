import Foundation

/// Data cards written from the window
/// (`specboard://SpecBoard/Docs/Design:CardKinds`「データ（instance）と定義のセクション」): one made in the
/// section on screen with its shape, its shape changed, one value written or taken away. Each through
/// the tools as MCP writes it — a value by its path (`Data:first.status`); what a value is held to is
/// the core's to say, and a refusal is said in the window's banner.
extension AppModel {
    /// The shapes a data card may have: the project's entities and values, each with its path in full,
    /// in the order of their sections.
    var instanceShapes: [(id: String, path: String, section: String)] {
        guard let detail else { return [] }
        let directory = detail.directory
        return detail.schemas
            .filter { $0.kind.holdsFields && $0.container == nil }
            .map { (id: $0.id, path: directory.full($0), section: directory.full(section: directory.sectionId(of: $0))) }
            .sorted { ($0.section, $0.path) < ($1.section, $1.path) }
    }

    /// Whether a card of the kind `kind` (its name) is made by choosing its shape as well: a data card
    /// and a table are of a shape, which the window asks for beside the kind.
    nonisolated static func asksForShape(_ kind: String) -> Bool {
        kind == SchemaKind.instance.rawValue || kind == SchemaKind.table.rawValue
    }

    /// A data card made in the section on screen, of the shape `shape` (its id), and opened in its
    /// card editor to be filled in.
    func createInstance(name: String, label: String = "", shape: String) {
        guard let detail, let shapeCard = detail.directory.schema(id: shape) else { return }
        guard let made = createSchema(name: name, label: label, kindName: SchemaKind.instance.rawValue,
                                      more: ["of": detail.directory.full(shapeCard)], inView: false) else { return }
        open(.card(made.id))
    }

    /// A table made in the section on screen of the data of the shape `shape` (its id), and opened in
    /// its card editor to set its rules.
    func createTable(name: String, label: String = "", shape: String) {
        guard let detail, let shapeCard = detail.directory.schema(id: shape) else { return }
        guard let made = createSchema(name: name, label: label, kindName: SchemaKind.table.rawValue,
                                      more: ["of": detail.directory.full(shapeCard)], inView: false) else { return }
        open(.card(made.id))
    }

    /// One of a table's rules (`within`・`where`・`columns`・`sort`・`unnest`) written whole, or let go of
    /// (nil).
    func setTableRule(_ id: String, _ key: String, _ value: JSONValue?) {
        write { try tool("card_update", target: cardPath(id), value: [key: value?.any ?? NSNull()]) }
    }

    /// Its shape changed to the entity or value `shape` (its id) — a data card's, or a table's.
    func setInstanceShape(_ id: String, _ shape: String) {
        guard let detail, let card = detail.directory.schema(id: shape) else { return }
        write { try tool("card_update", target: cardPath(id), value: ["of": detail.directory.full(card)]) }
    }

    /// One value written at its names (`customer.name`, the names as they are), or taken away (nil).
    func setInstanceValue(_ id: String, _ names: [String], _ value: JSONValue?) {
        write {
            let path = try cardPath(id) + "." + PathText.joinNames(names)
            if let value {
                _ = try tool("card_update", target: path, value: ["value": value.any])
            } else {
                _ = try tool("card_delete", target: path)
            }
        }
    }
}
