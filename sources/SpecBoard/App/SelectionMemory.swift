import Foundation

/// What was selected when the app last ran: the project, and the schema and field in it.
///
/// Passed to `AppModel` rather than reached for, because the tests run inside the
/// app's process under its defaults. A model that remembered by itself would have a
/// test run replace the selection the person left on screen with one from a
/// throwaway database. Only the window's model is given one.
struct SelectionMemory {
    struct Remembered: Codable, Equatable {
        var project: String
        var schema: String?
        var field: String?
        /// The section on screen; nil for the project's first.
        var section: String?
        /// Where the project is kept: nil for this Mac, else the host of the agent that keeps it
        /// (`ProjectSummary.place`), whose project is waited for until it is connected.
        var place: UUID? = nil
    }

    let defaults: UserDefaults
    static let key = "selection.remembered"

    static let standard = SelectionMemory(defaults: .standard)

    func load() -> Remembered? {
        guard let data = defaults.data(forKey: Self.key) else { return nil }
        return try? JSONDecoder().decode(Remembered.self, from: data)
    }

    /// Whether the next launch opens on what was being looked at: only after 今すぐアップデート, which quits
    /// mid-work without the person choosing to leave the project. Any other launch opens on the
    /// project list, the project remembered open underneath (`AppModel.start`).
    static let reopenKey = "selection.reopenAfterUpdate"

    func reopenNextLaunch() { defaults.set(true, forKey: Self.reopenKey) }

    /// Whether this launch reopens, asked once: the mark is let go of as it is read.
    func takeReopen() -> Bool {
        defer { defaults.removeObject(forKey: Self.reopenKey) }
        return defaults.bool(forKey: Self.reopenKey)
    }

    /// A place of the tab bar, kept: what kind it is and which (`AppModel.Place`).
    struct SavedPlace: Codable, Equatable {
        var kind: String
        var id: String? = nil
        /// A dashboard's or the settings' own tab.
        var view: String? = nil
        /// How far a card's connections reach.
        var depth: Int? = nil
    }

    /// A project's tab bar as it was left: the tabs after the map tab, the one in front (nil: the map
    /// tab) and the section the map tab showed.
    struct Tabs: Codable, Equatable {
        var tabs: [SavedPlace]
        var active: Int?
        var map: String?
    }

    /// The tab bars, by project: on this Mac only, never in a project or sent anywhere.
    static let tabsKey = "tabs.byProject"

    func loadTabs(of project: String) -> Tabs? {
        allTabs()[project]
    }

    func saveTabs(_ tabs: Tabs, of project: String) {
        var all = allTabs()
        all[project] = tabs
        guard let data = try? JSONEncoder().encode(all) else { return }
        defaults.set(data, forKey: Self.tabsKey)
    }

    private func allTabs() -> [String: Tabs] {
        guard let data = defaults.data(forKey: Self.tabsKey) else { return [:] }
        return (try? JSONDecoder().decode([String: Tabs].self, from: data)) ?? [:]
    }

    func save(_ remembered: Remembered?) {
        guard let remembered, let data = try? JSONEncoder().encode(remembered) else {
            defaults.removeObject(forKey: Self.key)
            return
        }
        defaults.set(data, forKey: Self.key)
    }
}

/// Read by its key before a plan was called a project too (`plan`): what was open when 今すぐアップデート quit
/// the version before reopens after it.
extension SelectionMemory.Remembered {
    private enum CodingKeys: String, CodingKey {
        case project, plan, schema, field, section, place
    }

    init(from decoder: Decoder) throws {
        let keyed = try decoder.container(keyedBy: CodingKeys.self)
        project = try keyed.decodeIfPresent(String.self, forKey: .project)
            ?? keyed.decode(String.self, forKey: .plan)
        schema = try keyed.decodeIfPresent(String.self, forKey: .schema)
        field = try keyed.decodeIfPresent(String.self, forKey: .field)
        section = try keyed.decodeIfPresent(String.self, forKey: .section)
        place = try keyed.decodeIfPresent(UUID.self, forKey: .place)
    }

    func encode(to encoder: Encoder) throws {
        var keyed = encoder.container(keyedBy: CodingKeys.self)
        try keyed.encode(project, forKey: .project)
        try keyed.encodeIfPresent(schema, forKey: .schema)
        try keyed.encodeIfPresent(field, forKey: .field)
        try keyed.encodeIfPresent(section, forKey: .section)
        try keyed.encodeIfPresent(place, forKey: .place)
    }
}
