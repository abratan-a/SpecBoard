import AppKit
import Foundation
import Observation

/// Keeps the installed app up to date with the latest release of the chosen place.
///
/// Checking happens on its own, as often as the place in use is set to (`UpdateSettings`); downloading
/// happens on its own only when 設定 says so, and otherwise when somebody asks; replacing the app never
/// does. A new version is fetched, unpacked and checked in the background, and then waits for somebody to
/// press the button — quitting the app takes the canvas they were looking at and every MCP session
/// connected to it along with it, and that is not a decision to make for them.
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
        /// Newer than this app and not fetched: downloading waits to be asked.
        case available(Release)
        case downloading(Release)
        case ready(Release)
        /// 今すぐアップデート was pressed, and the place is asked once more before the swap.
        case rechecking(Release)
        case installing(Release)
        case failed(String)
    }

    /// Why a check runs: one asked for from the app menu says what it found even when it was said before.
    enum Asked {
        case scheduled, byHand, menu
    }

    private(set) var phase: Phase = .idle
    private(set) var lastChecked: Date?
    /// Why the last check did not answer, while what was found before it still stands (a new version, or one
    /// fetched): the failure does not take that away, it is only said beside it.
    private(set) var lastProblem: String?
    /// How the previous update went, read once at launch.
    var notice: Notice?

    struct Notice: Equatable {
        var ok: Bool
        var message: String
    }

    /// A new version worth telling about in the notices: found and waiting to be fetched, or fetched and
    /// waiting to be swapped in. `serial` changes each time it is to be told.
    struct Announcement: Equatable {
        enum What: Equatable { case available, ready }
        let serial: Int
        let what: What
        let release: Release
        let feedName: String
    }

    private(set) var announcement: Announcement?
    /// What was told already, so the same version found by every check is told once.
    @ObservationIgnored private var told: Set<String> = []

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
    /// Whether a new version is fetched as soon as it is found.
    private(set) var autoDownload: Bool
    /// Each place's periodic check, as 設定 shows it.
    private(set) var periodics: [String: UpdateSettings.Periodic] = [:]
    /// When the place in use is next asked on its own; nil when it is not.
    private(set) var nextDue: Date?

    var selectedFeed: UpdateFeed? { UpdateFeeds.selected(in: feeds, id: selectedFeedId) }

    var label: String { "\(version) (\(build))" }

    func periodic(for id: String) -> UpdateSettings.Periodic { periodics[id] ?? .standard }

    @ObservationIgnored private var timer: Task<Void, Never>?
    @ObservationIgnored private var started = false
    @ObservationIgnored private var staged: URL?
    /// Not before this: launch and waking leave a moment for the window and the network to come up.
    @ObservationIgnored private var holdUntil: Date = .distantPast
    @ObservationIgnored private var wakeObserver: NSObjectProtocol?

    @ObservationIgnored private let network: UpdateNetwork
    @ObservationIgnored private let now: () -> Date
    @ObservationIgnored private let swapApp: @MainActor (URL) throws -> Void

    @ObservationIgnored private let outcomeFile: URL

    /// `network`, `now`, `installed`, `swap` and `outcome` are for trying what it decides without a server, a
    /// clock, an app to replace or the result of a real swap.
    init(bundle: Bundle = .main, defaults: UserDefaults = .standard, network: UpdateNetwork? = nil,
         now: @escaping () -> Date = Date.init, installed: Bool? = nil,
         swap: @escaping @MainActor (URL) throws -> Void = Updater.swapAndQuit,
         outcome: URL = UpdateInstaller.outcomeFile) {
        version = bundle.infoDictionary?["CFBundleShortVersionString"] as? String ?? "?"
        build = Int(bundle.infoDictionary?["CFBundleVersion"] as? String ?? "") ?? 0
        self.installed = installed ?? (bundle.bundleURL.deletingLastPathComponent().lastPathComponent == "Applications")
        feedsBundle = bundle
        feedsDefaults = defaults
        self.network = network ?? LiveUpdateNetwork()
        self.now = now
        swapApp = swap
        outcomeFile = outcome
        autoDownload = UpdateSettings.autoDownload(defaults)
        reloadFeeds()
        selectedFeedId = defaults.string(forKey: UpdateFeeds.selectedKey)
    }

    @ObservationIgnored private let feedsBundle: Bundle
    /// Passed in so a test keeps its places and its choice out of the app's own preferences.
    @ObservationIgnored private let feedsDefaults: UserDefaults

    private func reloadFeeds() {
        feeds = UpdateFeeds.all(feedsBundle, feedsDefaults)
        tokenIds = Set(feeds.filter { !$0.isBuiltin && UpdateTokens.has($0.id) }.map(\.id))
        periodics = Dictionary(uniqueKeysWithValues: feeds.map { ($0.id, UpdateSettings.periodic(for: $0.id, feedsDefaults)) })
    }

    // MARK: - Where from

    /// Drops anything already fetched: it was checked against the other place's
    /// release, and leaving it waiting would offer the wrong version.
    func selectFeed(_ id: String) {
        guard id != selectedFeed?.id, feeds.contains(where: { $0.id == id }), !busy else { return }
        selectedFeedId = id
        feedsDefaults.set(id, forKey: UpdateFeeds.selectedKey)
        dropStaged()
        phase = .idle
        lastProblem = nil
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
        var added = UpdateFeeds.added(feedsDefaults)
        if let index = added.firstIndex(where: { $0.id == feed.id }) {
            added[index] = feed
        } else {
            // Without a choice of its own, the place in use follows the list, and a new place
            // would change it: keep the one in use as the choice before the list grows.
            if !feeds.contains(where: { $0.id == selectedFeedId }), let inUse = selectedFeed {
                selectedFeedId = inUse.id
                feedsDefaults.set(inUse.id, forKey: UpdateFeeds.selectedKey)
            }
            added.append(feed)
        }
        UpdateFeeds.save(added, feedsDefaults)
        reloadFeeds()
        // What was fetched from the old address of this place no longer stands.
        if feed.id == selectedFeed?.id, !busy { dropStaged(); phase = .idle; lastProblem = nil }
        plan()
    }

    func removeFeed(_ id: String) {
        guard id != UpdateFeed.builtinId, !busy else { return }
        UpdateTokens.remove(id)
        UpdateSettings.forget(id, feedsDefaults)
        UpdateFeeds.save(UpdateFeeds.added(feedsDefaults).filter { $0.id != id }, feedsDefaults)
        let wasSelected = selectedFeed?.id == id
        reloadFeeds()
        if wasSelected {
            selectedFeedId = nil
            feedsDefaults.removeObject(forKey: UpdateFeeds.selectedKey)
            dropStaged()
            phase = .idle
            lastProblem = nil
        }
        plan()
    }

    // MARK: - Settings

    func setPeriodic(_ periodic: UpdateSettings.Periodic, for id: String) {
        UpdateSettings.setPeriodic(periodic, for: id, feedsDefaults)
        periodics[id] = UpdateSettings.periodic(for: id, feedsDefaults)
        plan()
    }

    /// Turned on while a new version waits to be asked for, it is fetched now: that is what on means.
    func setAutoDownload(_ on: Bool) {
        UpdateSettings.setAutoDownload(on, feedsDefaults)
        autoDownload = on
        if on, case .available = phase { Task { await download() } }
    }

    // MARK: - When

    func start() {
        guard installed, !started else { return }
        started = true
        readOutcome()
        holdUntil = now().addingTimeInterval(UpdateSchedule.settle)
        wakeObserver = NSWorkspace.shared.notificationCenter.addObserver(
            forName: NSWorkspace.didWakeNotification, object: nil, queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.woke() }
        }
        plan()
    }

    /// Asleep, the timer may have come due with no network to ask; awake, a check that is due waits a
    /// moment for it rather than failing at once.
    func woke() {
        holdUntil = now().addingTimeInterval(UpdateSchedule.settle)
        plan()
    }

    /// Sets the next check of the place in use from its setting and the last look, replacing the one set
    /// before: any change of place, setting or last look moves it.
    func plan() {
        timer?.cancel()
        timer = nil
        guard let feed = selectedFeed, installed else { nextDue = nil; return }
        let current = now()
        nextDue = UpdateSchedule.due(periodic: periodic(for: feed.id), lastCheck: lastChecked,
                                     notBefore: max(holdUntil, current))
        guard started, let due = nextDue else { return }
        let wait = max(due.timeIntervalSince(current), 0)
        timer = Task { [weak self] in
            try? await Task.sleep(for: .seconds(wait))
            guard !Task.isCancelled, let self else { return }
            // Whatever is under way plans again when it ends; asking now would only be skipped.
            guard !self.busy else { return }
            // A task of its own, not the timer's: planning again cancels the timer, and that must not
            // cancel a request already on its way.
            Task { await self.check(.scheduled) }
        }
    }

    var busy: Bool {
        switch phase {
        case .checking, .downloading, .rechecking, .installing: return true
        default: return false
        }
    }

    // MARK: - Checking

    func check(_ asked: Asked = .byHand) async {
        guard !busy else { return }
        defer { plan() }
        guard installed else {
            phase = .failed(L10n[.Update.errorDevBuild])
            return
        }
        guard let feed = selectedFeed else {
            phase = .failed(L10n[.Update.errorNoSource])
            return
        }
        let source = source(of: feed)
        let before = phase
        phase = .checking
        do {
            let release = try await network.latest(from: source)
            lastChecked = now()
            lastProblem = nil
            if !release.isNewer(than: version, build: build) {
                dropStaged()
                phase = .current
            } else if case .ready(let waiting) = before, waiting.tag == release.tag, staged != nil {
                phase = .ready(waiting)
                announce(.ready, waiting, again: asked == .menu)
            } else if autoDownload {
                await fetch(release, from: source, asked: asked)
            } else {
                // The swap always takes the latest, so an older copy would never be swapped in.
                dropStaged()
                phase = .available(release)
                announce(.available, release, again: asked == .menu)
            }
        } catch {
            lastChecked = now()
            switch before {
            case .available, .ready:
                phase = before
                lastProblem = ErrorText.said(error)
            default:
                phase = .failed(ErrorText.said(error))
            }
        }
    }

    /// Fetches the version found and waiting to be asked for.
    func download() async {
        guard case .available(let release) = phase, let feed = selectedFeed else { return }
        await fetch(release, from: source(of: feed), asked: .byHand)
        plan()
    }

    private func fetch(_ release: Release, from source: UpdateSource, asked: Asked) async {
        phase = .downloading(release)
        lastProblem = nil
        do {
            staged = try await network.stage(release, from: source, running: running)
            phase = .ready(release)
            announce(.ready, release, again: asked == .menu)
        } catch {
            staged = nil
            phase = .failed(ErrorText.said(error))
        }
    }

    private func source(of feed: UpdateFeed) -> UpdateSource {
        UpdateFeeds.source(feed, token: feed.isBuiltin ? nil : UpdateTokens.token(for: feed.id))
    }

    private var running: UpdateInstaller.Running { UpdateInstaller.Running(version: version, build: build) }

    private func dropStaged() {
        if let staged { try? FileManager.default.removeItem(at: staged) }
        staged = nil
    }

    private func announce(_ what: Announcement.What, _ release: Release, again: Bool) {
        let key = "\(what)|\(release.tag)"
        guard again || !told.contains(key) else { return }
        told.insert(key)
        announcement = Announcement(serial: (announcement?.serial ?? 0) + 1, what: what, release: release,
                                    feedName: selectedFeed?.displayName ?? L10n[.Update.feedFallbackName])
    }

    /// Whether the notice of `tag` may still offer to download it: only while that version waits to be asked for.
    func offersDownload(_ tag: String) -> Bool {
        if case .available(let release) = phase { return release.tag == tag }
        return false
    }

    // MARK: - Replacing

    /// Asks the place once more and swaps in the latest: a release made after the one fetched would
    /// otherwise wait for the next check, and pressing the button means the latest. A newer one is fetched
    /// and checked first, and swapped in without another press. When the place does not answer, the copy
    /// fetched is swapped in: it was checked, and a server out of reach must not stand between somebody
    /// and the update they asked for.
    func updateNow() async {
        guard case .ready(let fetched) = phase, staged != nil, let feed = selectedFeed else { return }
        // A swap that does not happen leaves the next check to be planned from here.
        defer { plan() }
        phase = .rechecking(fetched)
        let source = source(of: feed)
        var release = fetched
        if let latest = try? await network.latest(from: source) {
            lastChecked = now()
            lastProblem = nil
            if latest.tag != fetched.tag, latest.isNewer(than: fetched.version, build: fetched.build ?? 0) {
                phase = .downloading(latest)
                do {
                    staged = try await network.stage(latest, from: source, running: running)
                    release = latest
                } catch {
                    staged = nil
                    phase = .failed(ErrorText.said(error))
                    return
                }
            }
        }
        guard let staged else { return }
        phase = .installing(release)
        do {
            try swapApp(staged)
        } catch {
            phase = .failed(L10n[.Update.errorSwapNotStarted(ErrorText.said(error))])
        }
    }

    /// Quits, and leaves the swap to a script that opens the new version.
    ///
    /// Selection, window placement and full screen are all written as the app quits, and the
    /// next launch is told to open on what was being looked at rather than on the project list
    /// (`SelectionMemory.reopenNextLaunch`): the new version opens where this one was.
    static func swapAndQuit(_ staged: URL) throws {
        try UpdateInstaller.launchSwap(
            new: staged, target: Bundle.main.bundleURL, leftover: UpdateInstaller.leftover,
            outcome: UpdateInstaller.outcomeFile, pid: ProcessInfo.processInfo.processIdentifier)
        SelectionMemory.standard.reopenNextLaunch()
        NSApp.terminate(nil)
    }

    private func readOutcome() {
        let file = outcomeFile
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

    /// For アップデートを確認… in the app menu, where nothing else on screen would show that there was
    /// nothing new or that asking failed. A new version, found or fetched, goes to the notices instead,
    /// as one found on its own does: a window would stop whatever was being done for news that can wait.
    func checkAndReport() async {
        await check(.menu)
        let alert = NSAlert()
        switch phase {
        case .current:
            alert.messageText = L10n[.Update.statusUpToDate]
            alert.informativeText = L10n[.Update.alertUpToDate(label)]
        case .failed(let message):
            alert.alertStyle = .warning
            alert.messageText = L10n[.Update.alertFailedTitle]
            alert.informativeText = message
        case .available, .ready:
            guard let lastProblem else { return }
            alert.alertStyle = .warning
            alert.messageText = L10n[.Update.alertFailedTitle]
            alert.informativeText = lastProblem
        default:
            return
        }
        alert.runModal()
    }
}
