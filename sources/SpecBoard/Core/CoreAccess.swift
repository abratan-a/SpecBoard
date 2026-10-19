import Foundation

/// The access keys MCP asks for, the public projects, and what MCP may write in each project
/// (`specboard://SpecBoard/Docs/Design:DesignAccess`), read and set through the core's window requests
/// (`specboard://SpecBoard/Docs/Design:CoreApi`「アクセスキー」). No tool changes any of it, so an agent cannot give
/// itself more. The core keeps a key's digest only: its value is said once, when it is made.
extension CoreClient {

    /// A key as the window lists it, with what it may do and the projects it was given
    /// (project → level).
    struct AccessKeyRow: Decodable {
        var id: String
        var name: String
        var permits: [String: MCPState]
        var createdAt: String
        var usedAt: String?
        var grants: [String: String]

        var key: AccessKey {
            AccessKey(id: id, name: name, permits: permits, createdAt: createdAt, usedAt: usedAt)
        }

        var levels: [String: AccessLevel] { grants.compactMapValues(AccessLevel.init(rawValue:)) }
    }

    /// What a key may do besides the projects it was given: its items, by their ids.
    enum Permit: String, CaseIterable, Sendable {
        /// Choose and change how the window looks (`app_control`'s state, select, view, windows).
        case control = "app_control"
        /// Take pictures of the window (`app_control`'s capture).
        case capture = "app_control.capture"
        /// Make a project (`project_create`).
        case create = "project_create"

        /// As a key is made with nothing said.
        var byDefault: MCPState { self == .create ? .allow : .ask }
    }

    /// Every key, in the order they were made.
    func accessKeys() throws -> [AccessKeyRow] {
        try request(AccessKeyList.self, "access_keys").keys
    }

    /// A new key: its record, and its value — to be copied once, never kept. It may make
    /// projects unless told otherwise, and do nothing with the window.
    func createAccessKey(name: String, permits: [Permit: MCPState] = [:]) throws -> (key: AccessKey, value: String) {
        var fields: [String: Any] = ["name": name]
        if !permits.isEmpty {
            fields["permits"] = Dictionary(uniqueKeysWithValues: permits.map { ($0.key.rawValue, $0.value.rawValue) })
        }
        let made = try request(MadeKey.self, "access_key_create", fields)
        return (made.key.key, made.value)
    }

    func renameAccessKey(_ id: String, to name: String) throws {
        _ = try request("access_key_rename", ["key": id, "name": name])
    }

    /// One of what a key may do besides its projects, set to a state.
    func setPermit(_ id: String, _ permit: Permit, _ state: MCPState) throws {
        _ = try request("access_key_permit", ["key": id, "permits": [permit.rawValue: state.rawValue]])
    }

    /// What MCP may do in the project (`mcp_writes`), by group.
    func mcpWrites(_ project: String) throws -> [MCPWriteGroup] {
        try request(MCPWrites.self, "mcp_writes", ["project": project]).groups
    }

    /// An item of the project set to a state: the groups as they are now. A line in main's history.
    func setMCPWrite(_ project: String, item: String, _ state: MCPState) throws -> [MCPWriteGroup] {
        try request(MCPWrites.self, "mcp_writes_set", ["project": project, "item": item, "state": state.rawValue]).groups
    }

    /// Every item of a group set to a state.
    func setMCPWrite(_ project: String, group: String, _ state: MCPState) throws -> [MCPWriteGroup] {
        try request(MCPWrites.self, "mcp_writes_set", ["project": project, "group": group, "state": state.rawValue]).groups
    }

    /// Gone at once, with what it was given: the next request with it is refused.
    func revokeAccessKey(_ id: String) throws {
        _ = try request("access_key_revoke", ["key": id])
    }

    /// The same key — its name, what it may do and the projects it was given — with a new value,
    /// to be copied once, never kept: the value before is refused from the next request.
    func regenerateAccessKey(_ id: String) throws -> (key: AccessKey, value: String) {
        let made = try request(MadeKey.self, "access_key_regenerate", ["key": id])
        return (made.key.key, made.value)
    }

    /// Nil takes the grant away.
    func setGrant(keyId: String, projectId: String, _ level: AccessLevel?) throws {
        _ = try request("access_grant", ["key": keyId, "project": projectId, "level": level?.rawValue ?? NSNull()])
    }

    /// A project made public — every key may read it — or not.
    func setProjectPublic(_ project: String, _ isPublic: Bool) throws {
        _ = try request("project_public_set", ["project": project, "on": isPublic])
    }
}

private struct AccessKeyList: Decodable { var keys: [CoreClient.AccessKeyRow] }

private struct MadeKey: Decodable {
    var key: CoreClient.AccessKeyRow
    var value: String
}

private struct MCPWrites: Decodable { var groups: [MCPWriteGroup] }
