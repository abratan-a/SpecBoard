import Foundation

/// What the notice bubble said — errors, notices, an AI's merges into a locked main, new versions — kept for the
/// bell in the top bar (`NoticeBell`): a bubble goes by itself, or is not shown at all (`NoticeSettings`),
/// and what it said is still there to read. In memory, a window's, for as long as the app runs.
struct NoticeLog: Equatable {
    /// What a notice is, as the bubble tells them apart.
    enum Kind: String, CaseIterable, Identifiable, Sendable {
        case error, notice, aiMerge, update

        var id: String { rawValue }
    }

    struct Entry: Identifiable, Equatable {
        /// In the order told, from 1: a newer one's is greater.
        let id: Int
        let kind: Kind
        /// What it said when it was told, in the screen's language then (`toldIn`).
        let text: String
        let toldIn: String
        /// How it is said in another language: for what the screen words itself; none for what came
        /// worded (the core's refusals), said as it came.
        let sayIn: (@MainActor (String) -> String)?
        let at: Date
        /// The project an AI merged into, whose history the entry opens.
        let project: String?
        /// The release tag of a new version said to be there, which the entry offers to download.
        let update: String?

        /// What it says in `code`: the list follows the screen's language when it changes.
        @MainActor func text(in code: String) -> String {
            guard code != toldIn, let sayIn else { return text }
            return sayIn(code)
        }

        static func == (a: Entry, b: Entry) -> Bool {
            a.id == b.id && a.kind == b.kind && a.text == b.text && a.toldIn == b.toldIn && a.at == b.at
                && a.project == b.project && a.update == b.update
        }
    }

    /// How many are kept: enough for a long day's work, while a window that runs for weeks does not
    /// grow without end. The oldest drop off.
    static let kept = 100

    /// Newest first.
    private(set) var entries: [Entry] = []
    /// The newest one told when the list was last looked at: those after it are not seen yet.
    private(set) var seenThrough = 0
    /// The newest one told, dropped off or cleared or not.
    private(set) var lastTold = 0

    mutating func add(_ kind: Kind, _ text: String, project: String? = nil, update: String? = nil,
                      at time: Date = Date(), sayIn: (@MainActor (String) -> String)? = nil) {
        lastTold += 1
        entries.insert(Entry(id: lastTold, kind: kind, text: text, toldIn: Language.current, sayIn: sayIn, at: time,
                             project: project, update: update), at: 0)
        if entries.count > Self.kept { entries.removeLast(entries.count - Self.kept) }
    }

    /// The ones told since the list was last looked at: what the badge counts.
    var unseen: ArraySlice<Entry> { entries.prefix { $0.id > seenThrough } }

    /// Whether one not seen yet is an error: the badge says so in red, the rest can wait.
    var unseenError: Bool { unseen.contains { $0.kind == .error } }

    /// The list looked at: what is in it is seen.
    mutating func markSeen() { seenThrough = lastTold }

    /// Everything let go — and so nothing left to see.
    mutating func clear() {
        entries = []
        markSeen()
    }
}

/// How this Mac shows notices (settings' 「通知」): each kind in the bubble at the bell or only in the
/// bell's list, and how long a bubble stays. This Mac's, not a project's: how much a person wants to be
/// interrupted is theirs, whichever project is open.
enum NoticeSettings {
    /// Where a kind of notice is shown. The bell's list has every kind either way.
    enum Shown: String, CaseIterable, Identifiable {
        /// Stored as `bar`: the value a Mac that chose before the bubble replaced the bar already holds.
        case bubble = "bar"
        case list

        var id: String { rawValue }
    }

    /// How long a bubble stays, the same for every kind: one setting, since what differs between the
    /// kinds is whether a bubble is wanted at all.
    enum Stay: String, CaseIterable, Identifiable {
        case five, ten, untilClosed

        var id: String { rawValue }

        /// How long, in seconds; nil when it stays until closed.
        var seconds: Int? {
            switch self {
            case .five: 5
            case .ten: 10
            case .untilClosed: nil
            }
        }
    }

    /// Where this Mac keeps where `kind` is shown.
    static func shownKey(_ kind: NoticeLog.Kind) -> String { "notice.shown.\(kind.rawValue)" }
    /// Where this Mac keeps how long a bubble stays: named for the bar it was before, so a choice
    /// already made is kept.
    static let stayKey = "notice.barStay"

    /// A bubble, unless asked otherwise: a notice nobody sees is not a notice.
    static let standardShown = Shown.bubble
    /// Long enough to read a line of an error, short enough not to stand over the work.
    static let standardStay = Stay.five

    static func shown(_ kind: NoticeLog.Kind, in defaults: UserDefaults = .standard) -> Shown {
        shown(stored: defaults.string(forKey: shownKey(kind)))
    }

    /// What a stored value says: the standard one when there is none, or one this version does not know.
    static func shown(stored: String?) -> Shown {
        stored.flatMap(Shown.init(rawValue:)) ?? standardShown
    }

    static func stay(in defaults: UserDefaults = .standard) -> Stay {
        stay(stored: defaults.string(forKey: stayKey))
    }

    static func stay(stored: String?) -> Stay {
        stored.flatMap(Stay.init(rawValue:)) ?? standardStay
    }

    /// How long a bubble waits before it goes by itself: not while the pointer is over it — it is being
    /// read, or its button is about to be pressed — and never when it stays until closed. The wait
    /// starts again in full once the pointer leaves.
    static func closesAfter(_ stay: Stay, hovering: Bool) -> Duration? {
        guard !hovering, let seconds = stay.seconds else { return nil }
        return .seconds(seconds)
    }
}
