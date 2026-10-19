import AppKit
import Foundation
import Observation

/// Keeps the installed app up to date with the latest release of the chosen place.
///
/// Checking and downloading happen on their own; replacing the app does not. A new
/// version is fetched, unpacked and checked in the background, and then waits for
/// somebody to press the button — quitting the app takes the canvas they were
/// looking at and every MCP session connected to it along with it, and that is not
/// a decision to make for them.
///
/// Only a copy in an Applications folder updates itself. A Debug build run from
/// Xcode is somebody working on the app, and replacing it with a release would be
/// replacing their work.
@Observable
@MainActor
final class Updater {

    static let shared = Updater()

    enum Phase: Equatable {
        case idle
        case checking
        case current
        case downloading(Release)
        case ready(Release)
        case installing(Release)
        case failed(String)
    }

    private(set) var phase: Phase = .idle
    private(set) var lastChecked: Date?
    /// How the previous update went, read once at launch.
    var notice: Notice?

    struct Notice: Equatable {
        var ok: Bool
        var message: String
    }

    let version: String
    let build: Int
    let installed: Bool
    /// Where updates can come from: the build's own 公開版 first (when it has one), then any added in
    /// 設定. Kept in the preferences, which an update keeps, so they survive it.
    private(set) var feeds: [UpdateFeed] = []
    private(set) var selectedFeedId: String?
    /// Which added places have a token, for the list. Read from the Keychain when the
    /// list changes, not on every draw.
    private(set) var tokenIds: Set<String> = []

    var selectedFeed: UpdateFeed? { UpdateFeeds.selected(in: feeds, id: selectedFeedId) }

    var label: String { "\(version) (\(build))" }

    @ObservationIgnored private var loop: Task<Void, Never>?
    @ObservationIgnored private var staged: URL?

    /// Six hours: often enough that a release made in the morning is waiting by the
    /// afternoon, rarely enough that a laptop away from the release server does not
    /// spend the day failing to reach it.
    static let interval: Duration = .seconds(6 * 60 * 60)

    init(bundle: Bundle = .main) {
        version = bundle.infoDictionary?["CFBundleShortVersionString"] as? String ?? "?"
        build = Int(bundle.infoDictionary?["CFBundleVersion"] as? String ?? "") ?? 0
        installed = bundle.bundleURL.deletingLastPathComponent().lastPathComponent == "Applications"
        feedsBundle = bundle
        reloadFeeds()
        selectedFeedId = UserDefaults.standard.string(forKey: UpdateFeeds.selectedKey)
    }

    @ObservationIgnored private let feedsBundle: Bundle

    private func reloadFeeds() {
        feeds = UpdateFeeds.all(feedsBundle)
        tokenIds = Set(feeds.filter { !$0.isBuiltin && UpdateTokens.has($0.id) }.map(\.id))
    }

    // MARK: - Where from

    /// Drops anything already fetched: it was checked against the other place's
    /// release, and leaving it waiting would offer the wrong version.
    func selectFeed(_ id: String) {
        guard id != selectedFeed?.id, feeds.contains(where: { $0.id == id }), !busy else { return }
        selectedFeedId = id
        UserDefaults.standard.set(id, forKey: UpdateFeeds.selectedKey)
        staged = nil
        phase = .idle
        Task { await check() }
    }

    /// Adds a place, or replaces the one with the same id. A token given replaces the
    /// stored one; nil leaves it as it was; `removeToken` takes it away.
    func saveFeed(_ feed: UpdateFeed, token: String?, removeToken: Bool = false) throws {
        guard !feed.isBuiltin else { throw UpdateInstaller.Failure(L10n[.Update.feedFormPublicLocked]) }
        if let token, !token.isEmpty {
            try UpdateTokens.set(token, for: feed.id)
        } else if removeToken {
            UpdateTokens.remove(feed.id)
        }
        var added = UpdateFeeds.added()
        if let index = added.firstIndex(where: { $0.id == feed.id }) {
            added[index] = feed
        } else {
            added.append(feed)
        }
        UpdateFeeds.save(added)
        reloadFeeds()
        // What was fetched from the old address of this place no longer stands.
        if feed.id == selectedFeed?.id, !busy { staged = nil; phase = .idle }
    }

    func removeFeed(_ id: String) {
        guard id != UpdateFeed.builtinId, !busy else { return }
        UpdateTokens.remove(id)
        UpdateFeeds.save(UpdateFeeds.added().filter { $0.id != id })
        let wasSelected = selectedFeed?.id == id
        reloadFeeds()
        if wasSelected {
            selectedFeedId = nil
            UserDefaults.standard.removeObject(forKey: UpdateFeeds.selectedKey)
            staged = nil
            phase = .idle
        }
    }

    func start() {
        guard installed, loop == nil else { return }
        readOutcome()
        loop = Task { [weak self] in
            // Let the window and the MCP endpoint come up first.
            try? await Task.sleep(for: .seconds(10))
            while !Task.isCancelled {
                await self?.check()
                try? await Task.sleep(for: Self.interval)
            }
        }
    }

    var busy: Bool {
        switch phase {
        case .checking, .downloading, .installing: return true
        default: return false
        }
    }

    // MARK: - Checking

    func check() async {
        guard !busy else { return }
        guard installed else {
            phase = .failed(L10n[.Update.errorDevBuild])
            return
        }
        guard let feed = selectedFeed else {
            phase = .failed(L10n[.Update.errorNoSource])
            return
        }
        let source = UpdateFeeds.source(feed, token: feed.isBuiltin ? nil : UpdateTokens.token(for: feed.id))
        let waiting: Release? = if case .ready(let release) = phase { release } else { nil }
        phase = .checking
        do {
            let release = try await latest(from: source)
            lastChecked = Date()
            if !release.isNewer(than: version, build: build) {
                phase = .current
            } else if let waiting, waiting.tag == release.tag, staged != nil {
                phase = .ready(waiting)
            } else {
                phase = .downloading(release)
                staged = try await fetch(release, from: source)
                phase = .ready(release)
            }
        } catch {
            lastChecked = Date()
            phase = .failed(String(describing: error))
        }
    }

    private func latest(from source: UpdateSource) async throws -> Release {
        let (data, response) = try await URLSession.shared.data(for: source.request(source.latestURL))
        let http = response as? HTTPURLResponse
        let status = http?.statusCode ?? 0
        guard status == 200 else {
            throw source.failure(status: status,
                                 rateLimitRemaining: http?.value(forHTTPHeaderField: "x-ratelimit-remaining"))
        }
        return try Release.parse(data)
    }

    private func fetch(_ release: Release, from source: UpdateSource) async throws -> URL {
        let (download, response) = try await URLSession.shared.download(
            for: source.download(release.asset.url))
        let http = response as? HTTPURLResponse
        guard http?.statusCode == 200,
              http?.value(forHTTPHeaderField: "Content-Type")?.hasPrefix("text/html") != true else {
            throw UpdateInstaller.Failure(L10n[.Update.errorDownloadFailed(release.asset.name, http?.statusCode ?? 0)])
        }

        let fm = FileManager.default
        let dir = UpdateInstaller.workDir
        try fm.createDirectory(at: dir, withIntermediateDirectories: true)
        // Not the asset's own name: that comes from the server, and a name with
        // `../` in it would put the file — and the removal before it — anywhere.
        let dmg = dir.appending(path: "update.dmg")
        try? fm.removeItem(at: dmg)
        try fm.moveItem(at: download, to: dmg)

        guard let team = UpdateInstaller.runningTeam() else {
            throw UpdateInstaller.Failure(L10n[.Update.errorNoTeam])
        }
        let bundleId = Bundle.main.bundleIdentifier ?? ""
        let build = release.build
        let version = release.version
        let running = UpdateInstaller.Running(version: self.version, build: self.build)
        let staged = try await Task.detached {
            defer { try? FileManager.default.removeItem(at: dmg) }
            return try UpdateInstaller.stage(dmg: dmg, into: dir.appending(path: "staged"),
                                             bundleId: bundleId, build: build, version: version,
                                             running: running, team: team)
        }.value
        return staged
    }

    // MARK: - Replacing

    /// Quits, and leaves the swap to a script that opens the new version.
    ///
    /// Selection, window placement and full screen are all written as the app quits, and the
    /// next launch is told to open on what was being looked at rather than on the project list
    /// (`SelectionMemory.reopenNextLaunch`): the new version opens where this one was.
    func installAndRelaunch() {
        guard case .ready(let release) = phase, let staged else { return }
        phase = .installing(release)
        do {
            try UpdateInstaller.launchSwap(
                new: staged, target: Bundle.main.bundleURL, leftover: UpdateInstaller.leftover,
                outcome: UpdateInstaller.outcomeFile, pid: ProcessInfo.processInfo.processIdentifier)
            SelectionMemory.standard.reopenNextLaunch()
            NSApp.terminate(nil)
        } catch {
            phase = .failed(L10n[.Update.errorSwapNotStarted(error.localizedDescription)])
        }
    }

    private func readOutcome() {
        let file = UpdateInstaller.outcomeFile
        guard let text = try? String(contentsOf: file, encoding: .utf8) else { return }
        try? FileManager.default.removeItem(at: file)
        let line = text.split(whereSeparator: \.isNewline).first.map(String.init) ?? ""
        // A replacement that went through needs no word; only one that failed does.
        if line.hasPrefix("failed") {
            let reason = line.dropFirst("failed".count).trimmingCharacters(in: .whitespaces)
            notice = Notice(ok: false, message: L10n[.Update.noticeFailed(reason)])
        }
    }

    // MARK: - Menu

    /// For アップデートを確認… in the app menu, where nothing else on screen would
    /// show what came of it.
    func checkAndReport() async {
        await check()
        let alert = NSAlert()
        switch phase {
        case .current:
            alert.messageText = L10n[.Update.statusUpToDate]
            alert.informativeText = L10n[.Update.alertUpToDate(label)]
        case .ready(let release):
            alert.messageText = L10n[.Update.alertReadyTitle(release.label)]
            alert.informativeText = L10n[.Update.alertReadyMessage(label)]
            alert.addButton(withTitle: L10n[.Update.alertRelaunch])
            alert.addButton(withTitle: L10n[.Update.alertLater])
            if alert.runModal() == .alertFirstButtonReturn { installAndRelaunch() }
            return
        case .failed(let message):
            alert.alertStyle = .warning
            alert.messageText = L10n[.Update.alertFailedTitle]
            alert.informativeText = message
        default:
            return
        }
        alert.runModal()
    }
}
