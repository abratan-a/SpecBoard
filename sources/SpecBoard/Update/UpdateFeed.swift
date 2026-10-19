import Foundation

/// Where updates come from: the latest release of a repository on GitHub, or on a Forgejo
/// or Gitea instance.
///
/// A build knows at most one place: 公開版, the public distribution repository, given
/// to it as build settings (`SPECBOARD_UPDATE_FEED_*`) because the source is published and
/// names no instance — and none while nothing is published (`specboard://SpecBoard/Docs/Design:Release`). Anywhere else — a private closed-beta feed, say — is added in 設定 by
/// whoever runs the app, as an `UpdateFeed`, and chosen there. See `UpdateFeeds`.
nonisolated struct UpdateSource: Equatable {
    var base: URL
    var repo: String
    /// Sent to where the releases are read and nowhere else: `base` for Forgejo,
    /// `api.github.com` for GitHub.
    var token: String?

    /// Which API answers. Read from the host, so a place needs no kind of its own and one
    /// added in 設定 as `https://github.com` is read as GitHub too.
    enum API: Equatable { case forgejo, github }

    static let githubHost = "github.com"
    static let githubAPI = URL(string: "https://api.github.com")!

    var api: API { base.host()?.lowercased() == Self.githubHost ? .github : .forgejo }

    var latestURL: URL {
        switch api {
        case .github: Self.githubAPI.appending(path: "repos/\(repo)/releases/latest")
        case .forgejo: base.appending(path: "api/v1/repos/\(repo)/releases/latest")
        }
    }

    func request(_ url: URL) -> URLRequest {
        var request = URLRequest(url: url, timeoutInterval: 60)
        switch api {
        case .github:
            request.setValue("application/vnd.github+json", forHTTPHeaderField: "Accept")
            request.setValue("2022-11-28", forHTTPHeaderField: "X-GitHub-Api-Version")
            if let token, url.scheme == "https", url.host()?.lowercased() == Self.githubAPI.host() {
                request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
            }
        case .forgejo:
            if let token { request.setValue("token \(token)", forHTTPHeaderField: "Authorization") }
        }
        return request
    }

    /// For the asset link the release names. The token goes only to the instance it
    /// belongs to: the link is whatever the server wrote, and could point anywhere.
    /// GitHub's never gets it: its link is on github.com and redirects to a storage
    /// host, and the token was given for the API alone.
    func download(_ url: URL) -> URLRequest {
        guard api == .forgejo, url.scheme == base.scheme, url.host() == base.host(), url.port == base.port else {
            return URLRequest(url: url, timeoutInterval: 60)
        }
        return request(url)
    }

    /// Why asking for the latest release did not answer 200.
    enum Failure: Error, Equatable, CustomStringConvertible {
        /// GitHub allows a few requests an hour without a token; the next check asks again.
        case rateLimited(Int)
        case refused(Int)
        case githubRefused(Int)
        case noRelease(String)
        case status(Int)

        var description: String {
            let language = Language.current
            switch self {
            case .rateLimited(let status): return L10n.text(.Update.errorRateLimited(status), in: language)
            case .refused(let status): return L10n.text(.Update.errorRefused(status), in: language)
            case .githubRefused(let status): return L10n.text(.Update.errorGithubRefused(status), in: language)
            case .noRelease(let repo): return L10n.text(.Update.errorNoRelease(repo), in: language)
            case .status(let status): return L10n.text(.Update.errorServerStatus(status), in: language)
            }
        }
    }

    /// GitHub says it is out of requests with 429, or with 403 and none remaining; any
    /// other 401 or 403 is a token or a repository it will not show.
    func failure(status: Int, rateLimitRemaining: String?) -> Failure {
        switch (api, status) {
        case (.github, 429): return .rateLimited(status)
        case (.github, 403) where rateLimitRemaining?.trimmingCharacters(in: .whitespaces) == "0":
            return .rateLimited(status)
        case (.github, 401), (.github, 403): return .githubRefused(status)
        case (.forgejo, 401), (.forgejo, 403): return .refused(status)
        case (_, 404): return .noRelease(repo)
        default: return .status(status)
        }
    }
}

/// A published release, as much of it as updating needs.
///
/// The distribution repository tags a release with its version alone — `v0.1.0` —
/// and a version is what says one release comes after another. A private closed-beta feed
/// marks its tag — `v0.1.8-internal` — and tags each version once, so it too names a
/// version alone. Earlier closed-beta tags carry the build — `v0.1.0-202609241052`
/// — and then two builds of one version are told apart by it.
nonisolated struct Release: Equatable {
    var tag: String
    var name: String
    var notes: String
    var version: String
    var build: Int?
    var asset: Asset

    struct Asset: Equatable {
        var name: String
        var url: URL
        var size: Int
    }

    enum Problem: Error, Equatable, CustomStringConvertible {
        case unreadable
        case tag(String)
        case noDiskImage(String)

        var description: String {
            switch self {
            case .unreadable:
                return L10n.text(.Update.problemUnreadable, in: Language.current)
            case .tag(let tag):
                return L10n.text(.Update.problemTag(tag), in: Language.current)
            case .noDiskImage(let tag):
                return L10n.text(.Update.problemNoDiskImage(tag), in: Language.current)
            }
        }
    }

    static func tag(version: String, build: Int) -> String { "v\(version)-\(build)" }

    /// What the closed beta puts after the version in its tags.
    static let internalSuffix = "-internal"

    /// `v0.1.0` → ("0.1.0", nil); `v0.1.8-internal` → ("0.1.8", nil);
    /// `v1.0-202609241052` → ("1.0", 202609241052).
    /// The version has to be numbers and dots: anything else is not a release tag.
    static func parseTag(_ tag: String) -> (version: String, build: Int?)? {
        let body = tag.hasPrefix("v") ? tag.dropFirst() : Substring(tag)
        var version = body
        var build: Int?
        if body.hasSuffix(internalSuffix) {
            version = body.dropLast(internalSuffix.count)
        } else if let dash = body.lastIndex(of: "-") {
            guard let number = Int(body[body.index(after: dash)...]), number > 0 else { return nil }
            version = body[..<dash]
            build = number
        }
        guard isVersion(version) else { return nil }
        return (String(version), build)
    }

    static func isVersion(_ text: Substring) -> Bool {
        let parts = text.split(separator: ".", omittingEmptySubsequences: false)
        return !parts.isEmpty && parts.allSatisfy { !$0.isEmpty && $0.allSatisfy(\.isASCII) && Int($0) != nil }
    }

    /// Numerically, part by part: `0.10.0` comes after `0.9.9`, and `1.0` is `1.0.0`.
    static func compareVersions(_ a: String, _ b: String) -> ComparisonResult {
        let x = a.split(separator: ".").map { Int($0) ?? 0 }
        let y = b.split(separator: ".").map { Int($0) ?? 0 }
        for i in 0..<max(x.count, y.count) {
            let l = i < x.count ? x[i] : 0, r = i < y.count ? y[i] : 0
            if l != r { return l < r ? .orderedAscending : .orderedDescending }
        }
        return .orderedSame
    }

    /// The answer to `latestURL`: GitHub, Forgejo and Gitea name a release's fields alike.
    static func parse(_ data: Data) throws(Problem) -> Release {
        guard let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let tag = object["tag_name"] as? String else { throw .unreadable }
        guard let (version, build) = parseTag(tag) else { throw .tag(tag) }
        let assets = object["assets"] as? [[String: Any]] ?? []
        guard let dmg = assets.first(where: {
                  ($0["name"] as? String)?.lowercased().hasSuffix(".dmg") == true }),
              let name = dmg["name"] as? String,
              let link = dmg["browser_download_url"] as? String,
              let url = URL(string: link) else { throw .noDiskImage(tag) }
        return Release(
            tag: tag,
            name: nonEmpty(object["name"] as? String) ?? tag,
            notes: object["body"] as? String ?? "",
            version: version, build: build,
            asset: Asset(name: name, url: url, size: dmg["size"] as? Int ?? 0))
    }

    var label: String { build.map { "\(version) (\($0))" } ?? version }

    /// A later version is newer. The same version is newer only when the tag names a
    /// later build — a tag without one never replaces the version it names.
    func isNewer(than version: String, build: Int) -> Bool {
        switch Release.compareVersions(self.version, version) {
        case .orderedDescending: return true
        case .orderedAscending: return false
        case .orderedSame: return (self.build ?? 0) > build
        }
    }
}

nonisolated private func nonEmpty(_ text: String?) -> String? {
    guard let text, !text.isEmpty else { return nil }
    return text
}
