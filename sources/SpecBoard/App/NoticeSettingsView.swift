import SwiftUI

/// Settings' 「通知」: how this Mac tells things (`NoticeSettings`) — each kind of the notices that only
/// tell in the bubble at the bell or only in the bell's list, and how long a bubble stays.
struct NoticeSettingsView: View {
    @AppStorage(NoticeSettings.shownKey(.error)) private var errorShown = NoticeSettings.standardShown.rawValue
    @AppStorage(NoticeSettings.shownKey(.notice)) private var noticeShown = NoticeSettings.standardShown.rawValue
    @AppStorage(NoticeSettings.shownKey(.aiMerge)) private var aiMergeShown = NoticeSettings.standardShown.rawValue
    @AppStorage(NoticeSettings.shownKey(.update)) private var updateShown = NoticeSettings.standardShown.rawValue
    @AppStorage(NoticeSettings.stayKey) private var stay = NoticeSettings.standardStay.rawValue

    var body: some View {
        VStack(alignment: .leading, spacing: 22) {
            settingsSection(L10n[.Settings.noticesShownTitle]) {
                SwiftUI.Grid(alignment: .leading, horizontalSpacing: 12, verticalSpacing: 8) {
                    shownRow(.error, $errorShown)
                    shownRow(.notice, $noticeShown)
                    shownRow(.aiMerge, $aiMergeShown)
                    shownRow(.update, $updateShown)
                }
                note(L10n[.Settings.noticesShownNote])
            }
            settingsSection(L10n[.Settings.noticesStayTitle]) {
                Picker("", selection: $stay) {
                    ForEach(NoticeSettings.Stay.allCases) { choice in Text(choice.said).tag(choice.rawValue) }
                }
                .labelsHidden().fixedSize()
                .font(.app(12))
                note(L10n[.Settings.noticesStayNote])
            }
        }
    }

    private func shownRow(_ kind: NoticeLog.Kind, _ chosen: Binding<String>) -> some View {
        SwiftUI.GridRow {
            Text(kind.said).font(.app(12))
            Picker("", selection: chosen) {
                ForEach(NoticeSettings.Shown.allCases) { choice in Text(choice.said).tag(choice.rawValue) }
            }
            .labelsHidden().fixedSize()
            .font(.app(12))
        }
    }

    private func note(_ text: String) -> some View {
        Text(text)
            .font(.app(10.5)).foregroundStyle(.secondary)
            .fixedSize(horizontal: false, vertical: true)
    }
}

extension NoticeSettings.Shown {
    var said: String {
        switch self {
        case .bubble: L10n[.Settings.noticesShownBubble]
        case .list: L10n[.Settings.noticesShownList]
        }
    }
}

extension NoticeSettings.Stay {
    var said: String {
        switch self {
        case .five: L10n[.Settings.noticesStayFive]
        case .ten: L10n[.Settings.noticesStayTen]
        case .untilClosed: L10n[.Settings.noticesStayUntilClosed]
        }
    }
}
