import Foundation
import Observation

/// One package the app or its core is built from, as `tools/notices.py` wrote it into
/// `ThirdPartyNotices.json`: where it comes from, its license, and who holds the copyright.
struct LicensePackage: Decodable, Identifiable, Hashable {
    enum Source: String, Decodable, CaseIterable {
        case rust, npm, swift

        /// The tag a list row carries: a name, said the same in every language.
        var tag: String {
            switch self {
            case .rust: "Rust"
            case .npm: "npm"
            case .swift: "Swift"
            }
        }

        var chip: String {
            switch self {
            case .rust: L10n[.Docs.licensesChipRust]
            case .npm: L10n[.Docs.licensesChipNpm]
            case .swift: L10n[.Docs.licensesChipSwift]
            }
        }

        var provenance: String {
            switch self {
            case .rust: L10n[.Docs.licensesFromRust]
            case .npm: L10n[.Docs.licensesFromNpm]
            case .swift: L10n[.Docs.licensesFromSwift]
            }
        }

        /// "71 npm packages", counted as the language says it.
        func count(_ n: Int) -> String {
            switch self {
            case .rust: L10n[.Docs.licensesCountRust(n)]
            case .npm: L10n[.Docs.licensesCountNpm(n)]
            case .swift: L10n[.Docs.licensesCountSwift(n)]
            }
        }
    }

    let name: String
    let version: String
    let source: Source
    let license: String
    let homepage: String
    let copyright: [String]
    /// The ids of its license texts in `LicenseCatalog.texts`: one per license it names.
    let texts: [String]
    let copyrightFrom: String?

    /// Name and version are one package; the same name in two versions is two (`syn` 2 and 3).
    var id: String { "\(source.rawValue)/\(name)@\(version)" }

    /// The copyright line was made from the package's authors: its license file has none.
    var copyrightIsFromAuthors: Bool { copyrightFrom == "authors" }

    /// How many licenses it offers to choose from (`MIT OR Apache-2.0` is 2), 1 when it is one license or
    /// they all apply (`AND`).
    var choices: Int {
        license.contains("(") || license.contains(" AND ") ? 1 : license.components(separatedBy: " OR ").count
    }

    var homepageURL: URL? {
        guard let url = URL(string: homepage), ["http", "https"].contains(url.scheme?.lowercased() ?? "") else { return nil }
        return url
    }
}

/// A license's text, once, under the license it is.
struct LicenseText: Decodable, Hashable {
    let license: String
    let body: String
}

/// The packages that share one license (one spelling of its expression), by name.
struct LicenseGroup: Identifiable, Hashable {
    let license: String
    let packages: [LicensePackage]

    var id: String { license }

    /// How many of its packages come from each source, the most first.
    var sources: [(source: LicensePackage.Source, count: Int)] {
        let counts = Dictionary(grouping: packages, by: \.source).mapValues(\.count)
        return LicensePackage.Source.allCases.compactMap { source in counts[source].map { (source, $0) } }
            .sorted { $0.count > $1.count }
    }
}

/// Everything the Licenses page shows, read from `ThirdPartyNotices.json`. Each distinct license text is
/// held once (`texts`); a package names the ones it is under.
struct LicenseCatalog: Decodable {
    let packages: [LicensePackage]
    let texts: [String: LicenseText]

    static func decode(_ data: Data) throws -> LicenseCatalog {
        try JSONDecoder().decode(LicenseCatalog.self, from: data)
    }

    /// The catalog the app ships; nil when the file is missing or unreadable.
    static let bundled: LicenseCatalog? = {
        Bundle.main.url(forResource: "ThirdPartyNotices", withExtension: "json")
            .flatMap { try? Data(contentsOf: $0) }
            .flatMap { try? decode($0) }
    }()

    /// The packages whose name has `query` in it (case does not matter), grouped by license: the
    /// licenses with the most packages first, then by name; the packages of a group by name. No query is
    /// every package.
    func groups(matching query: String = "") -> [LicenseGroup] {
        let query = query.trimmingCharacters(in: .whitespaces)
        let found = query.isEmpty ? packages : packages.filter { $0.name.range(of: query, options: .caseInsensitive) != nil }
        return Dictionary(grouping: found, by: \.license)
            .map { LicenseGroup(license: $0.key, packages: $0.value) }
            .sorted { a, b in
                a.packages.count != b.packages.count ? a.packages.count > b.packages.count : a.license < b.license
            }
    }

    /// The texts a package is under, in the order it names them.
    func texts(of package: LicensePackage) -> [LicenseText] {
        package.texts.compactMap { texts[$0] }
    }

    /// One text for each license a group's packages are under: the one most of them use. Their
    /// copyright lines differ and are not in it; the packages are listed beside it.
    func texts(of group: LicenseGroup) -> [LicenseText] {
        var order: [String] = []
        var used: [String: [String: Int]] = [:]
        for package in group.packages {
            for id in package.texts {
                guard let text = texts[id] else { continue }
                if !order.contains(text.license) { order.append(text.license) }
                used[text.license, default: [:]][id, default: 0] += 1
            }
        }
        return order.compactMap { license in
            used[license]?.sorted { a, b in a.value != b.value ? a.value > b.value : a.key < b.key }.first
                .flatMap { texts[$0.key] }
        }
    }
}

/// What the Licenses page has chosen and typed: the search, the package or the license (its header)
/// selected, which groups are folded, and which of a package's texts is shown. The page draws this.
@Observable
final class LicensePage {
    enum Selection: Hashable {
        case package(String)
        case group(String)
    }

    /// A line of the list as the keys walk it.
    enum Item: Hashable {
        case header(String)
        case row(String)

        var selection: Selection {
            switch self {
            case .header(let license): .group(license)
            case .row(let id): .package(id)
            }
        }
    }

    let catalog: LicenseCatalog
    /// What is typed in the search field; the list follows it.
    var query = "" {
        didSet { if query != oldValue { searched() } }
    }
    private(set) var groups: [LicenseGroup]
    private(set) var selection: Selection?
    private(set) var folded: Set<String> = []
    /// Which of the shown texts is read, for a package or a license that offers a choice.
    var textIndex = 0

    init(_ catalog: LicenseCatalog) {
        self.catalog = catalog
        groups = catalog.groups()
        selection = groups.first.map { .group($0.license) }
    }

    var isSearching: Bool { !query.trimmingCharacters(in: .whitespaces).isEmpty }

    var packageCount: Int { groups.reduce(0) { $0 + $1.packages.count } }

    /// The lines as they are drawn: a header, its packages unless the group is folded (a search shows
    /// every match).
    var items: [Item] {
        groups.flatMap { group in
            [Item.header(group.license)]
                + (folded.contains(group.license) && !isSearching ? [] : group.packages.map { Item.row($0.id) })
        }
    }

    func isFolded(_ license: String) -> Bool { folded.contains(license) && !isSearching }

    var selectedPackage: LicensePackage? {
        guard case .package(let id) = selection else { return nil }
        return groups.lazy.flatMap(\.packages).first { $0.id == id }
    }

    var selectedGroup: LicenseGroup? {
        guard case .group(let license) = selection else { return nil }
        return groups.first { $0.license == license }
    }

    /// The texts the selection is read under: a package's own, a license's once.
    var shownTexts: [LicenseText] {
        if let package = selectedPackage { return catalog.texts(of: package) }
        if let group = selectedGroup { return catalog.texts(of: group) }
        return []
    }

    var shownText: LicenseText? {
        let texts = shownTexts
        return texts.isEmpty ? nil : texts[min(textIndex, texts.count - 1)]
    }

    func select(_ selection: Selection) {
        guard self.selection != selection else { return }
        self.selection = selection
        textIndex = 0
    }

    /// The package or license `delta` lines on (the arrow keys), stopping at the ends.
    func move(_ delta: Int) {
        let items = items
        guard !items.isEmpty else { return }
        let now = items.firstIndex { $0.selection == selection } ?? (delta > 0 ? -1 : items.count)
        select(items[min(max(now + delta, 0), items.count - 1)].selection)
    }

    /// Folds a license's packages away, or shows them again. A package selected in it gives the
    /// selection to the license.
    func toggle(_ license: String) {
        if folded.contains(license) {
            folded.remove(license)
        } else {
            folded.insert(license)
            if let package = selectedPackage, package.license == license { select(.group(license)) }
        }
    }

    /// Folds the license of the selection (left arrow), or shows it (right arrow).
    func fold(_ fold: Bool) {
        let license = selectedPackage?.license ?? selectedGroup?.license
        guard let license, folded.contains(license) != fold else { return }
        toggle(license)
        if fold { select(.group(license)) }
    }

    /// The same selection if it is still in the list, else the first line of what is left.
    private func searched() {
        groups = catalog.groups(matching: query)
        let lines = items
        if !lines.contains(where: { $0.selection == selection }) {
            if let first = lines.first { select(first.selection) } else { selection = nil }
        }
    }
}
