import Foundation

/// How updates are looked for and fetched, as 設定 sets them: whether a new version is downloaded as soon
/// as it is found, and, for each place updates come from, whether and how often it is asked on its own.
///
/// Kept beside the places (`UpdateFeeds`) in the preferences, not in them: the build's own place is not
/// stored there, and `UpdateFeeds.added` rebuilds each place from its four fields, so a field added there
/// would be dropped — as it would by an earlier version that writes the list again.
/// `specboard://SpecBoard/Docs/Design:Release`
nonisolated enum UpdateSettings {

    static let autoDownloadKey = "update.autoDownload"
    static let periodicKey = "update.checkEvery"

    /// Six hours: often enough that a release made in the morning is waiting by the afternoon, rarely
    /// enough that a laptop away from the release server does not spend the day failing to reach it.
    static let standardMinutes = 360

    /// What an added place may be asked every. Not shorter than an hour: releases come a few times a day
    /// at most, and a place reached only on a VPN would otherwise fail every few minutes; a GitHub place
    /// without a token shares 60 requests an hour with everything else on the network.
    static let choices = [60, 360, 1440]

    /// Whether, and how often, one place is asked without being told to.
    struct Periodic: Equatable, Sendable {
        var on: Bool
        var minutes: Int

        static let standard = Periodic(on: true, minutes: UpdateSettings.standardMinutes)

        var interval: TimeInterval { TimeInterval(minutes * 60) }
    }

    /// Off unless turned on: fetching a version nobody asked for spends the network and the disk, and
    /// everyone — those who had it on before as well — decides for themselves.
    static func autoDownload(_ defaults: UserDefaults = .standard) -> Bool {
        defaults.bool(forKey: autoDownloadKey)
    }

    static func setAutoDownload(_ on: Bool, _ defaults: UserDefaults = .standard) {
        defaults.set(on, forKey: autoDownloadKey)
    }

    /// `id`'s setting; on, every six hours, when nothing (or nothing that reads) is kept — what every place
    /// did before there was a setting. The build's own place is asked every six hours or not at all, and
    /// an added one only at one of `choices`: the preferences are a file anything run as the person can
    /// write, and an interval of a second would have the app ask without pause.
    static func periodic(for id: String, _ defaults: UserDefaults = .standard) -> Periodic {
        guard let kept = defaults.dictionary(forKey: periodicKey)?[id] as? [String: Any],
              let on = kept["on"] as? Bool else { return .standard }
        if id == UpdateFeed.builtinId { return Periodic(on: on, minutes: standardMinutes) }
        guard let minutes = kept["minutes"] as? Int, choices.contains(minutes) else { return .standard }
        return Periodic(on: on, minutes: minutes)
    }

    static func setPeriodic(_ periodic: Periodic, for id: String, _ defaults: UserDefaults = .standard) {
        var all = defaults.dictionary(forKey: periodicKey) ?? [:]
        let minutes = id == UpdateFeed.builtinId || !choices.contains(periodic.minutes) ? standardMinutes : periodic.minutes
        all[id] = ["on": periodic.on, "minutes": minutes]
        defaults.set(all, forKey: periodicKey)
    }

    /// A place removed takes its setting with it.
    static func forget(_ id: String, _ defaults: UserDefaults = .standard) {
        guard var all = defaults.dictionary(forKey: periodicKey), all[id] != nil else { return }
        all[id] = nil
        defaults.set(all, forKey: periodicKey)
    }
}

/// When the place in use is next asked on its own.
nonisolated enum UpdateSchedule {

    /// Not before this long after launch or waking: the window and the MCP endpoint come up first, and a
    /// Mac just awake is often not on the network yet.
    static let settle: TimeInterval = 10

    /// The interval after the last look, and not before `notBefore`; nil when the place is not asked on
    /// its own. Never looked at yet, it is due at `notBefore`.
    static func due(periodic: UpdateSettings.Periodic, lastCheck: Date?, notBefore: Date) -> Date? {
        guard periodic.on else { return nil }
        guard let lastCheck else { return notBefore }
        return max(lastCheck.addingTimeInterval(periodic.interval), notBefore)
    }
}
