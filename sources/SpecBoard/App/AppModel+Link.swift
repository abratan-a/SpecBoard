import Foundation

/// A place in a project as a link names it: `specboard://<plan>/<section>:<card>` — a field after the
/// card's name as a path goes on into it, a section alone or the project alone; `?branch=<name>` the
/// tree it opens in; what follows `#` (a heading's number or words) kept, not yet read. The app is
/// the scheme's (`Info.plist`), so a comment in the code that names a document of the project opens
/// it where an editor takes the link.
nonisolated struct AppLink: Equatable {
    static let scheme = "specboard"

    var project: String
    /// The section or the card, as the tools name it (percent-decoded: a name quoted in it as a path
    /// quotes it); empty for the project alone.
    var path: String
    var branch: String?
    var anchor: String?

    /// None for a URL of another scheme, or one naming no project.
    init?(_ url: URL) {
        guard url.scheme?.lowercased() == Self.scheme,
              let parts = URLComponents(url: url, resolvingAgainstBaseURL: false),
              let project = (parts.percentEncodedHost?.removingPercentEncoding ?? parts.host), !project.isEmpty else { return nil }
        self.project = project
        path = String(parts.path.drop { $0 == "/" })
        branch = parts.queryItems?.first { $0.name == "branch" }?.value.flatMap { $0.isEmpty ? nil : $0 }
        anchor = parts.fragment
    }
}

extension AppModel {
    /// A link opened (`AppLink`): the project, off the project list, in the tree it names, and there the
    /// card in its editor or the section's diagram — shown, nothing written, as a link may come from
    /// anywhere. A link that came while the app was starting is opened once it has. False, with why
    /// said, where it leads nowhere.
    @discardableResult
    func openLink(_ url: URL) -> Bool {
        guard let link = AppLink(url) else {
            lastError = L10n[.Lines.linkCantOpen(url.absoluteString)]
            return false
        }
        guard case .ready = phase else {
            pendingLink = url
            return true
        }
        let named = link.project
        guard let project = projects.first(where: { $0.id == named }) ?? projects.first(where: { $0.name == named })
                ?? projects.first(where: { $0.slug == named })
                ?? projects.first(where: { $0.name.lowercased() == named.lowercased() }) else {
            lastError = L10n[.Lines.linkNoProject(named)]
            return false
        }
        if detail?.project.id != project.id { select(project.id) }
        if let branch = link.branch {
            switch branch.lowercased() {
            case SchemaDirectory.mainWord: openBranch(nil)
            case Board.word: openBoard()
            default:
                guard let found = branches.first(where: { $0.name.lowercased() == branch.lowercased() }) else {
                    lastError = L10n[.Lines.linkNoBranch(branch)]
                    return false
                }
                openBranch(found.id)
            }
        }
        atHome = false
        lastError = nil
        guard !link.path.isEmpty else { return true }
        if !PathText.hasSection(link.path), let detail, let section = detail.directory.section(named: link.path) {
            selectSection(section.id)
            showCards()
            return true
        }
        guard open(path: link.path) else {
            lastError = L10n[.Lines.linkNoPlace(link.path)]
            return false
        }
        return true
    }
}
