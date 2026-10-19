import SwiftUI

/// The bell in the top bar: what the notice bubble said in this window (`NoticeLog`), newest
/// first — a bubble goes by itself, or is not shown, and what it said is read here. Its badge counts what
/// was told since the list was last opened, red when one of those is an error. The list opens from the
/// bubble too, so whether it is open is the model's.
struct NoticeBell: View {
    @Bindable var model: AppModel

    private var log: NoticeLog { model.noticeLog }

    var body: some View {
        Button { model.noticeListOpen.toggle() } label: {
            Image(systemName: "bell")
                .font(.app(14))
                .frame(width: 18, height: 18)
                // The badge sits on the bell's top-right corner, beside the glyph rather than over it,
                // so the bell stays recognisable with a count on it.
                .overlay(alignment: .topTrailing) {
                    if !log.unseen.isEmpty {
                        NoticeBadge(count: log.unseen.count, error: log.unseenError)
                            .offset(x: 9, y: -6)
                    }
                }
                .padding(.trailing, 10)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help(L10n[.Root.topBarNoticesHelp])
        .accessibilityLabel(L10n[.Root.topBarNotices])
        .accessibilityValue(log.unseen.isEmpty ? "" : L10n[.Root.topBarNoticesUnseen(log.unseen.count)])
        .popover(isPresented: $model.noticeListOpen, arrowEdge: .bottom) {
            NoticeList(model: model) { model.noticeListOpen = false }
        }
        // Opened, the list is seen; and what is told while it is open is seen there.
        .onChange(of: model.noticeListOpen) { model.noticeLog.markSeen() }
        .onChange(of: log.lastTold) { if model.noticeListOpen { model.noticeLog.markSeen() } }
    }
}

/// How many were told since the list was last opened: red when one is an error, the accent otherwise.
struct NoticeBadge: View {
    var count: Int
    var error: Bool

    var body: some View {
        Text(count > 99 ? "99+" : "\(count)")
            .font(.app(8.5, weight: .bold))
            .monospacedDigit()
            .foregroundStyle(.white)
            .padding(.horizontal, 3.5)
            .frame(minWidth: 13, minHeight: 13)
            .background(Capsule().fill(error ? Color.red : Color.accentColor))
            .fixedSize()
    }
}

/// The bell's list: newest first, each with its kind's mark — the one its bubble has — its words and
/// when; an AI's merge with the way to its project's history, as its bubble has. 「すべて削除」 at the foot.
struct NoticeList: View {
    var model: AppModel
    /// Shuts the popover: going to a history leaves the list behind.
    var close: () -> Void

    /// The height of the rows, to be as tall as they are up to `tallest`, and scroll past it.
    @State private var rowsHeight: CGFloat = 0
    private let tallest: CGFloat = 420

    @Environment(\.colorScheme) private var colorScheme
    private var theme: GraphTheme { colorScheme == .dark ? .dark : .light }

    var body: some View {
        let entries = model.noticeLog.entries
        VStack(spacing: 0) {
            if entries.isEmpty {
                Text(L10n[.Root.noticesEmpty])
                    .font(.app(11.5)).foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 28)
            } else {
                ScrollView {
                    VStack(spacing: 0) {
                        ForEach(entries) { entry in
                            row(entry)
                            if entry.id != entries.last?.id { Divider().padding(.leading, 34) }
                        }
                    }
                    .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { rowsHeight = $0 }
                }
                .frame(height: min(max(rowsHeight, 1), tallest))
            }
            Divider()
            HStack {
                Spacer()
                Button(L10n[.Root.noticesClear]) { model.noticeLog.clear() }
                    .font(.app(11))
                    .disabled(entries.isEmpty)
            }
            .padding(.horizontal, 12).padding(.vertical, 8)
        }
        .frame(width: 380)
    }

    private func row(_ entry: NoticeLog.Entry) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Image(systemName: entry.kind.symbol)
                .font(.app(11.5))
                .foregroundStyle(mark(entry.kind))
                .frame(width: 16)
                .help(entry.kind.said)
                .accessibilityLabel(entry.kind.said)
            VStack(alignment: .leading, spacing: 4) {
                Text(entry.text)
                    .font(.app(11.5))
                    .fixedSize(horizontal: false, vertical: true)
                    .textSelection(.enabled)
                HStack(spacing: 8) {
                    Text(NoticeLog.when(entry.at, in: Language.shared.code))
                        .font(.app(10)).foregroundStyle(.secondary)
                    Spacer(minLength: 0)
                    if entry.kind == .aiMerge, let project = entry.project {
                        Button(L10n[.Branches.aiMergeShowHistory]) {
                            close()
                            model.showAIMergeHistory(of: project)
                        }
                        .font(.app(10.5))
                    }
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(.horizontal, 12).padding(.vertical, 8)
    }

    /// The colour of a kind's mark: its bubble's.
    private func mark(_ kind: NoticeLog.Kind) -> Color {
        switch kind {
        case .error: Color(nsColor: theme.brassInk)
        case .notice: .green
        case .aiMerge: .purple
        }
    }
}

extension NoticeLog {
    /// When one was told, as the list says it: the time on the day it was told, with the date
    /// after — the app may run across days.
    nonisolated static func when(_ time: Date, now: Date = Date(), in code: String, calendar: Calendar = .current) -> String {
        let locale = Locale(identifier: code)
        let style: Date.FormatStyle = calendar.isDate(time, inSameDayAs: now)
            ? .dateTime.hour().minute()
            : .dateTime.month(.abbreviated).day().hour().minute()
        return time.formatted(style.locale(locale))
    }
}

extension NoticeLog.Kind {
    /// The kind by its name, the same as the settings' 「通知」 say it.
    var said: String {
        switch self {
        case .error: L10n[.Root.noticeKindError]
        case .notice: L10n[.Root.noticeKindNotice]
        case .aiMerge: L10n[.Root.noticeKindAiMerge]
        }
    }
}
