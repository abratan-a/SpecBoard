import Foundation

/// What the updater finds, told as the other notices are: in the bubble at the bell and in its list,
/// never in a window — a new version can wait for a moment that suits, and a window would stop whatever
/// was being done for it. `specboard://SpecBoard/Docs/Design:Release`
extension AppModel {
    /// One new version told of.
    struct UpdateNotice: Equatable {
        /// Its entry in the bell's list (`NoticeLog.Entry.id`), which says it again in another language.
        let told: Int
        let text: String
        /// The release tag of a version found and not fetched, which the notice offers to download; nil
        /// for one fetched, whose next step is the swap, and that is not a button in a passing bubble.
        let download: String?
    }

    /// An announcement of the updater, told once: the words in the screen's language now, and said
    /// again so in another.
    func tell(update told: Updater.Announcement) {
        let say: @MainActor (String) -> String = { code in Self.updateWords(told, in: code) }
        let text = say(Language.shared.code)
        let download = told.what == .available ? told.release.tag : nil
        noticeLog.add(.update, text, update: download, sayIn: say)
        updateNotice = UpdateNotice(told: noticeLog.lastTold, text: text, download: download)
    }

    static func updateWords(_ told: Updater.Announcement, in code: String) -> String {
        switch told.what {
        case .available: L10n.text(.Update.noticeAvailable(told.release.version, told.feedName), in: code)
        case .ready: L10n.text(.Update.noticeReady(told.release.version, told.feedName), in: code)
        }
    }
}
