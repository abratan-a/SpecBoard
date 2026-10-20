import Foundation
import Security

/// One place updates can come from, as 設定 lists it.
nonisolated struct UpdateFeed: Codable, Equatable, Identifiable, Sendable {
    var id: String
    var name: String
    var url: URL
    var repo: String

    /// The build's own, 公開版. Cannot be edited or removed.
    static let builtinId = "builtin"
    var isBuiltin: Bool { id == Self.builtinId }

    /// What the lists call it: the build's own has no name of its own kept, so it follows the
    /// screen's language.
    var displayName: String {
        isBuiltin ? L10n.text(.Update.feedPublic, in: Language.current) : name
    }

    /// `git.example.org / owner/name`.
    var place: String { "\(url.host() ?? url.absoluteString) / \(repo)" }
}

/// The places updates can come from, and which one is followed.
///
/// The build's own comes first, when it has one. The rest are added by whoever
/// runs the app and kept in its preferences — no instance is written in the source,
/// which is published — and a token for one goes to the Keychain, never beside it.
/// A token is sent only to the host of the place it was given for (for GitHub, its API host).
nonisolated enum UpdateFeeds {

    static let listKey = "update.feeds"
    static let selectedKey = "update.feed"

    /// 公開版, from the build. Nil for a build told nothing: a development build, or any
    /// build while nothing is published.
    static func builtin(_ bundle: Bundle = .main) -> UpdateFeed? {
        let info = bundle.infoDictionary ?? [:]
        guard let text = info["SpecBoardUpdateFeedURL"] as? String, !text.isEmpty,
              let url = URL(string: text),
              let repo = info["SpecBoardUpdateFeedRepo"] as? String, !repo.isEmpty else { return nil }
        return UpdateFeed(id: UpdateFeed.builtinId, name: L10n.text(.Update.feedPublic, in: Language.current),
                          url: url, repo: repo)
    }

    static func added(_ defaults: UserDefaults = .standard) -> [UpdateFeed] {
        guard let data = defaults.data(forKey: listKey),
              let feeds = try? JSONDecoder().decode([UpdateFeed].self, from: data) else { return [] }
        return feeds.filter { !$0.isBuiltin }
    }

    static func save(_ feeds: [UpdateFeed], _ defaults: UserDefaults = .standard) {
        defaults.set(try? JSONEncoder().encode(feeds.filter { !$0.isBuiltin }), forKey: listKey)
    }

    static func all(_ bundle: Bundle = .main, _ defaults: UserDefaults = .standard) -> [UpdateFeed] {
        (builtin(bundle).map { [$0] } ?? []) + added(defaults)
    }

    /// The chosen one; when the choice is gone (removed, or never made), the first one the person
    /// added, and only then the build's own. Somebody who has only ever used a place they added
    /// never made a choice, and a later build that gains a place of its own at the head of the
    /// list must not move them off it: they would stop seeing what that place does not publish.
    static func selected(in feeds: [UpdateFeed], id: String?) -> UpdateFeed? {
        feeds.first { $0.id == id } ?? feeds.first { !$0.isBuiltin } ?? feeds.first
    }

    /// Checks what somebody typed. The URL must be https: a token for it would
    /// otherwise travel in the clear.
    static func make(id: String = UUID().uuidString, name: String, url: String,
                     repo: String) throws -> UpdateFeed {
        let name = name.trimmingCharacters(in: .whitespaces)
        guard !name.isEmpty else { throw UpdateInstaller.Failure(L10n.text(.Update.feedFormNameEmpty, in: Language.current)) }
        var text = url.trimmingCharacters(in: .whitespaces)
        while text.hasSuffix("/") { text.removeLast() }
        guard let parsed = URL(string: text), parsed.scheme?.lowercased() == "https",
              let host = parsed.host(), !host.isEmpty,
              parsed.query == nil, parsed.fragment == nil else {
            throw UpdateInstaller.Failure(L10n.text(.Update.feedFormUrlShape, in: Language.current))
        }
        let repo = repo.trimmingCharacters(in: .whitespaces)
        let parts = repo.split(separator: "/", omittingEmptySubsequences: false)
        let allowed = CharacterSet.alphanumerics.union(CharacterSet(charactersIn: "-_."))
        guard parts.count == 2, parts.allSatisfy({ part in
            !part.isEmpty && part != "." && part != ".."
                && part.unicodeScalars.allSatisfy { allowed.contains($0) }
        }) else {
            throw UpdateInstaller.Failure(L10n.text(.Update.feedFormRepoShape, in: Language.current))
        }
        guard id != UpdateFeed.builtinId else { throw UpdateInstaller.Failure(L10n.text(.Update.feedFormPublicLocked, in: Language.current)) }
        return UpdateFeed(id: id, name: name, url: parsed, repo: repo)
    }

    /// Where to ask, with the token that goes there. The build's own place never gets one:
    /// it is public, and a token kept for another place has no business there.
    static func source(_ feed: UpdateFeed, token: String?) -> UpdateSource {
        UpdateSource(base: feed.url, repo: feed.repo,
                     token: feed.isBuiltin ? nil : token.flatMap { $0.isEmpty ? nil : $0 })
    }
}

/// Tokens for added places, in the login Keychain.
///
/// Not in the preferences with the rest of the place: a preferences file is read by
/// anything that reads the user's files, and a token is a credential.
nonisolated enum UpdateTokens {

    private static let service = "com.m-osone.SpecBoard.update-feed"

    static func token(for id: String) -> String? {
        var query = base(id)
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var item: CFTypeRef?
        guard SecItemCopyMatching(query as CFDictionary, &item) == errSecSuccess,
              let data = item as? Data else { return nil }
        return String(data: data, encoding: .utf8)
    }

    static func has(_ id: String) -> Bool { token(for: id) != nil }

    static func set(_ token: String, for id: String) throws {
        remove(id)
        var item = base(id)
        item[kSecValueData as String] = Data(token.utf8)
        item[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlock
        let status = SecItemAdd(item as CFDictionary, nil)
        guard status == errSecSuccess else {
            throw UpdateInstaller.Failure(L10n.text(.Update.feedFormTokenNotSaved(Int(status)), in: Language.current))
        }
    }

    static func remove(_ id: String) {
        SecItemDelete(base(id) as CFDictionary)
    }

    private static func base(_ id: String) -> [String: Any] {
        [kSecClass as String: kSecClassGenericPassword,
         kSecAttrService as String: service,
         kSecAttrAccount as String: id]
    }
}
