import SwiftUI

/// What the header's bubble tells: an error, a notice, an AI's merges into a locked main — the notices
/// that only tell. Each is shown or not as this Mac's settings say (`NoticeSettings`), and goes by
/// itself after the time set. What they said stays in the bell's list (`NoticeLog`). The bars that hold
/// a state or wait for an answer are not these: they stay while what they say is so.
enum NoticeBubble {
    /// One notice waiting to be told.
    struct Item: Identifiable, Equatable {
        let kind: NoticeLog.Kind
        /// Changes when the notice does, so the stay starts again for it.
        let id: String
        let text: String
        /// The project an AI merged into, whose history the notice opens.
        let project: String?
        /// Its place in the order told (`NoticeLog.Entry.id`): the newest is shown first.
        let told: Int
    }

    /// Narrower than this the bubble says too little to be worth the header's room; the bell's badge
    /// still counts what it would have said.
    static let leastWidth: CGFloat = 200
    /// Wider than this a line of the bubble is longer than the eye follows.
    static let mostWidth: CGFloat = 440
    /// How far the bubble's point reaches toward the bell.
    static let tail: CGFloat = 7
    /// One line of words and the ways to close it, inside the header's height with a margin above and below.
    static let height: CGFloat = 32

    /// How wide the bubble is, in `free` points of the header; nil when there is too little room for it.
    static func width(free: CGFloat) -> CGFloat? {
        free >= leastWidth ? min(free, mostWidth) : nil
    }

    /// What is waiting to be told, newest first: the error, the notice and each project's AI merges,
    /// leaving out the kinds this Mac keeps in the bell's list only.
    @MainActor
    static func pending(in model: AppModel, shown: (NoticeLog.Kind) -> NoticeSettings.Shown) -> [Item] {
        let entries = model.noticeLog.entries
        var items: [Item] = []
        if let text = model.lastError, shown(.error) == .bubble {
            items.append(Item(kind: .error, id: "error|\(text)", text: text, project: nil,
                              told: entries.first { $0.kind == .error && $0.text == text }?.id ?? 0))
        }
        if let text = model.notice, shown(.notice) == .bubble {
            items.append(Item(kind: .notice, id: "notice|\(text)", text: text, project: nil,
                              told: entries.first { $0.kind == .notice && $0.text == text }?.id ?? 0))
        }
        if shown(.aiMerge) == .bubble {
            for told in model.aiMergesByProject {
                items.append(Item(kind: .aiMerge, id: "aiMerge|\(told.project)|\(told.merges.count)",
                                  text: model.aiMergeWords(told.merges), project: told.project,
                                  told: entries.first { $0.kind == .aiMerge && $0.project == told.project }?.id ?? 0))
            }
        }
        // Notices told before the log was cleared have no place in it; they keep the order they were
        // gathered in.
        return items.enumerated()
            .sorted { ($0.element.told, -$0.offset) > ($1.element.told, -$1.offset) }
            .map(\.element)
    }
}

extension AppModel {
    /// A notice told and let go, as the bubble's close and its stay do.
    func closeNotice(_ item: NoticeBubble.Item) {
        switch item.kind {
        case .error: if lastError == item.text { lastError = nil }
        case .notice: if notice == item.text { notice = nil }
        case .aiMerge: if let project = item.project { dismissAIMerges(of: project) }
        }
    }
}

/// The newest waiting notice as a bubble in the header, pointing at the bell and reaching left from it
/// over the header's free room (`free`) without covering the buttons there. Laid over the free room
/// rather than in the row, so it neither moves the header's things nor the main panel below.
///
/// The stay runs whether or not the bubble fits: a notice that is not shown for want of room still
/// goes by itself, as it would have.
struct NoticeBubbleSlot: View {
    var model: AppModel
    /// The header's free room between its left things and the bell.
    var free: CGFloat

    @AppStorage(NoticeSettings.shownKey(.error)) private var errorShown = NoticeSettings.standardShown.rawValue
    @AppStorage(NoticeSettings.shownKey(.notice)) private var noticeShown = NoticeSettings.standardShown.rawValue
    @AppStorage(NoticeSettings.shownKey(.aiMerge)) private var aiMergeShown = NoticeSettings.standardShown.rawValue
    @AppStorage(NoticeSettings.stayKey) private var stayChosen = NoticeSettings.standardStay.rawValue

    private var stay: NoticeSettings.Stay { NoticeSettings.stay(stored: stayChosen) }

    private func shown(_ kind: NoticeLog.Kind) -> NoticeSettings.Shown {
        switch kind {
        case .error: NoticeSettings.shown(stored: errorShown)
        case .notice: NoticeSettings.shown(stored: noticeShown)
        case .aiMerge: NoticeSettings.shown(stored: aiMergeShown)
        }
    }

    var body: some View {
        let items = NoticeBubble.pending(in: model, shown: shown)
        if let item = items.first {
            Group {
                if let width = NoticeBubble.width(free: free) {
                    NoticeBubbleView(model: model, item: item, more: items.count - 1)
                        .frame(maxWidth: width)
                } else {
                    Color.clear.frame(width: 0, height: 0)
                }
            }
            .modifier(ClosesItself(stay: stay, id: item.id) { model.closeNotice(item) })
        }
    }
}

/// The bubble itself: the kind's mark, the words on one line (the end cut with … when it does not fit), and at the right the ways to close
/// it (and an AI merge's way to its history) with how many more wait behind it.
struct NoticeBubbleView: View {
    var model: AppModel
    var item: NoticeBubble.Item
    var more: Int

    @Environment(\.colorScheme) private var colorScheme
    private var theme: GraphTheme { colorScheme == .dark ? .dark : .light }

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: item.kind.symbol).font(.app(11.5))
            Button { model.noticeListOpen = true } label: {
                // Validation messages are written to be read — a rename that fails has to say why, or
                // the rule looks arbitrary; what is cut off is read in the bell's list.
                Text(item.text)
                    .font(.app(11))
                    .lineLimit(1)
                    .truncationMode(.tail)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .help(L10n[.Root.noticeBubbleOpenHelp])
            VStack(alignment: .trailing, spacing: 1) {
                HStack(spacing: 8) {
                    if item.kind == .aiMerge, let project = item.project {
                        Button(L10n[.Branches.aiMergeShowHistory]) { model.showAIMergeHistory(of: project) }
                            .buttonStyle(.plain)
                            .font(.app(10.5, weight: .medium))
                            .help(L10n[.Branches.aiMergeShowHistoryHelp])
                    }
                    Button { model.closeNotice(item) } label: {
                        Image(systemName: "xmark").font(.app(9, weight: .semibold)).contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .help(L10n[.Common.actionClose])
                    .accessibilityLabel(L10n[.Common.actionClose])
                }
                if more > 0 {
                    Text(L10n[.Root.noticeBubbleMore(more)]).font(.app(9.5)).foregroundStyle(.secondary).lineLimit(1)
                }
            }
            .fixedSize()
        }
        .foregroundStyle(item.kind == .error ? Color(nsColor: theme.brassInk) : Color.primary)
        .padding(.leading, 10).padding(.trailing, 10 + NoticeBubble.tail).padding(.vertical, 3)
        .frame(height: NoticeBubble.height)
        .background {
            BubbleShape(tail: NoticeBubble.tail).fill(Color(nsColor: .windowBackgroundColor))
            BubbleShape(tail: NoticeBubble.tail).fill(tint)
            BubbleShape(tail: NoticeBubble.tail).stroke(line, lineWidth: 0.75)
        }
        .accessibilityElement(children: .contain)
    }

    /// The kind's tint, as the bars had it.
    private var tint: Color {
        switch item.kind {
        case .error: Color(nsColor: theme.brassSoft)
        case .notice: Color.green.opacity(0.10)
        case .aiMerge: Color.purple.opacity(0.12)
        }
    }

    private var line: Color {
        switch item.kind {
        case .error: Color(nsColor: theme.brassLine)
        case .notice: Color.green.opacity(0.35)
        case .aiMerge: Color.purple.opacity(0.35)
        }
    }
}

/// A rounded box with a point at the middle of its right side, `tail` wide: it points at the bell.
struct BubbleShape: Shape {
    var tail: CGFloat
    var radius: CGFloat = 9

    func path(in rect: CGRect) -> Path {
        let body = CGRect(x: rect.minX, y: rect.minY, width: max(rect.width - tail, 0), height: rect.height)
        let r = min(radius, body.height / 2, body.width / 2)
        let middle = rect.midY
        var path = Path()
        path.move(to: CGPoint(x: body.minX + r, y: body.minY))
        path.addLine(to: CGPoint(x: body.maxX - r, y: body.minY))
        path.addArc(tangent1End: CGPoint(x: body.maxX, y: body.minY),
                    tangent2End: CGPoint(x: body.maxX, y: body.minY + r), radius: r)
        path.addLine(to: CGPoint(x: body.maxX, y: middle - 5))
        path.addLine(to: CGPoint(x: rect.maxX, y: middle))
        path.addLine(to: CGPoint(x: body.maxX, y: middle + 5))
        path.addLine(to: CGPoint(x: body.maxX, y: body.maxY - r))
        path.addArc(tangent1End: CGPoint(x: body.maxX, y: body.maxY),
                    tangent2End: CGPoint(x: body.maxX - r, y: body.maxY), radius: r)
        path.addLine(to: CGPoint(x: body.minX + r, y: body.maxY))
        path.addArc(tangent1End: CGPoint(x: body.minX, y: body.maxY),
                    tangent2End: CGPoint(x: body.minX, y: body.maxY - r), radius: r)
        path.addLine(to: CGPoint(x: body.minX, y: body.minY + r))
        path.addArc(tangent1End: CGPoint(x: body.minX, y: body.minY),
                    tangent2End: CGPoint(x: body.minX + r, y: body.minY), radius: r)
        path.closeSubpath()
        return path
    }
}

/// A notice that goes by itself once `stay` has passed (`NoticeSettings.closesAfter`): not while the
/// pointer is over it, and in full again once it leaves. Something new in the same place (`id`) starts
/// the wait again.
struct ClosesItself<ID: Hashable>: ViewModifier {
    var stay: NoticeSettings.Stay
    var id: ID
    var close: () -> Void

    @State private var hovering = false

    private struct Wait: Hashable {
        var id: ID
        var stay: NoticeSettings.Stay
        var hovering: Bool
    }

    func body(content: Content) -> some View {
        content
            .onHover { hovering = $0 }
            .task(id: Wait(id: id, stay: stay, hovering: hovering)) {
                guard let wait = NoticeSettings.closesAfter(stay, hovering: hovering) else { return }
                try? await Task.sleep(for: wait)
                guard !Task.isCancelled else { return }
                close()
            }
    }
}

extension NoticeLog.Kind {
    /// The mark of a kind: the same in the bubble and in the bell's list.
    var symbol: String {
        switch self {
        case .error: "exclamationmark.triangle.fill"
        case .notice: "checkmark.circle"
        case .aiMerge: "arrow.triangle.merge"
        }
    }
}
