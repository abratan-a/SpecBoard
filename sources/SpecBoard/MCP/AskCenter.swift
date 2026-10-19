import AppKit
import SwiftUI

/// A person asked whether MCP may do what it asked to (`specboard_core::App::ask`).
///
/// A call that touches an item set to 都度承認 and says `request: true` waits — on the thread the MCP
/// request is answered on, two minutes at the most — while the question is shown in a window over the
/// other apps that does not take the keyboard: the one place a question can be answered from whichever
/// app is in front. Four answers:
/// 今回だけ許可 (that one call; a batch's run), 常に許可, 拒否 (that one call), 常に拒否. The core sets
/// what "always" changes: the project's setting for every key, or the key's own. Questions come from this
/// Mac's core and from the agents' (each Mac a person may be at is asked; the first answer is taken and
/// the rest taken away: `withdraw`).
@MainActor @Observable
final class AskCenter {
    static let shared = AskCenter()

    /// What a person says.
    enum Answer: String, CaseIterable {
        case once, always, deny, never

        var said: String {
            switch self {
            case .once: L10n[.MCP.askAnswerOnce]
            case .always: L10n[.MCP.askAnswerAlways]
            case .deny: L10n[.MCP.stateDeny]
            case .never: L10n[.MCP.askAnswerNever]
            }
        }
    }

    /// One question, as the core asks it.
    nonisolated struct Question: Identifiable, Sendable {
        let id: UUID
        /// The tool called.
        let tool: String
        /// The access key's name.
        let key: String
        /// The items as the call said them, in the language of the call.
        let items: [Item]
        /// The items in each language the core said them in (`languages`): the window shows them in the
        /// screen's, whatever the call's is.
        var itemsByLanguage: [String: [Item]] = [:]
        /// What it is to do: the call's arguments, or a batch's steps.
        let what: String
        let seconds: Int
        /// An agent's question, by its host and number there (`withdraw`); none for this Mac's core.
        let tag: String?
        fileprivate let reply: @Sendable (String) -> Void

        nonisolated struct Item: Identifiable, Equatable, Sendable {
            /// The item: the tool, or a part of it (`project_update.rules`).
            let id: String
            let name: String
            /// Whose setting "always" changes: the project's (for every key) or the key's.
            let isKey: Bool
            let whoseName: String
            /// The project's slug, shown beside its name (two projects of one name made before names were
            /// held apart are told apart by it); none for a key.
            var whoseSlug: String = ""

            /// Whose it is, as the window says it: 「キー「…」」 or 「プロジェクト「…」（slug …）」.
            var whose: String {
                let language = Language.current
                if isKey { return L10n.text(.MCP.askWhoseKey(whoseName), in: language) }
                return whoseSlug.isEmpty
                    ? L10n.text(.MCP.askWhoseProject(whoseName), in: language)
                    : L10n.text(.MCP.askWhoseProjectSlug(whoseName, whoseSlug), in: language)
            }
        }

        /// The items as `code`'s screen says them: in that language when the core said them so, else as
        /// the call did.
        func items(in code: String) -> [Item] { itemsByLanguage[code] ?? items }

        /// What 常に許可 and 常に拒否 change, in a line.
        var always: String {
            let language = Language.current
            var said: [String] = []
            for item in items(in: language) where !said.contains(where: { $0.contains(item.whose) }) {
                said.append(item.isKey
                            ? L10n.text(.MCP.askAlwaysKey(item.whoseName), in: language)
                            : L10n.text(.MCP.askAlwaysProject(item.whose), in: language))
            }
            return L10n.text(.MCP.askAlways(said.joined(separator: L10n.text(.MCP.askAlwaysJoin, in: language))), in: language)
        }
    }

    /// The questions not answered yet, the oldest first.
    private(set) var questions: [Question] = []
    /// Whether it shows anything: not in a test.
    private let presents: Bool
    /// The window over the other apps, once a question has been shown in it.
    @ObservationIgnored private(set) var panel: NSPanel?
    /// The question the window was laid out and placed for: laid out again only for another.
    @ObservationIgnored private var placedFor: UUID?

    init(presents: Bool = true) {
        self.presents = presents
    }

    // MARK: - Asked

    /// A question read from the core's request (`{"tool", "key", "items", "what", "seconds"}`); nil when
    /// it cannot be read.
    nonisolated static func reading(_ request: String, tag: String?,
                                    reply: @escaping @Sendable (String) -> Void) -> Question? {
        guard let said = try? JSONSerialization.jsonObject(with: Data(request.utf8)) as? [String: Any],
              let tool = said["tool"] as? String,
              let items = said["items"] as? [[String: Any]] else { return nil }
        func read(_ items: [[String: Any]]) -> [Question.Item] {
            items.map {
                Question.Item(id: $0["id"] as? String ?? "", name: $0["name"] as? String ?? "",
                              isKey: $0["whose"] as? String == "key", whoseName: $0["whoseName"] as? String ?? "",
                              whoseSlug: $0["whoseSlug"] as? String ?? "")
            }
        }
        var byLanguage: [String: [Question.Item]] = [:]
        for (code, said) in said["languages"] as? [String: [String: Any]] ?? [:] {
            if let items = said["items"] as? [[String: Any]] { byLanguage[code] = read(items) }
        }
        var question = Question(
            id: UUID(), tool: tool, key: said["key"] as? String ?? "",
            items: read(items),
            what: said["what"] as? String ?? "", seconds: said["seconds"] as? Int ?? 120,
            tag: tag, reply: reply)
        question.itemsByLanguage = byLanguage
        return question
    }

    /// Asked on the thread the MCP request is answered on: the question shown, and this thread waits for
    /// the answer (`{"answer", "client"}`), or for its time to run out (`{"timeout": true}`, the question
    /// taken away). Never on main, which shows it.
    nonisolated static func ask(_ request: String, tag: String? = nil) -> String {
        // An agent's question (a tag) is an MCP call's: its failure is said in the language MCP is
        // answered in.
        let language = tag == nil ? Language.current : AppControl.mcpLanguage
        if Thread.isMainThread {
            return failure(.MCP.askErrorMainThread, in: language)
        }
        final class Box: @unchecked Sendable {
            var said: String?
        }
        let box = Box()
        let done = DispatchSemaphore(value: 0)
        guard let question = reading(request, tag: tag, reply: { said in
            box.said = said
            done.signal()
        }) else {
            return failure(.MCP.askErrorUnreadable, in: language)
        }
        Task { @MainActor in shared.add(question) }
        if done.wait(timeout: .now() + .seconds(question.seconds)) == .timedOut {
            Task { @MainActor in shared.remove(question.id) }
            return #"{"timeout":true}"#
        }
        return box.said ?? #"{"timeout":true}"#
    }

    /// The reply that tells the core a question could not be asked, in `language`.
    nonisolated private static func failure(_ message: LocalizedStringResource, in language: String) -> String {
        let text = L10n.text(message, in: language)
        let reply = try? JSONSerialization.data(withJSONObject: ["error": text], options: [.withoutEscapingSlashes])
        return reply.map { String(decoding: $0, as: UTF8.self) } ?? #"{"error":"?"}"#
    }

    func add(_ question: Question) {
        questions.append(question)
        show()
    }

    /// A person's answer: the call goes on with it.
    func answer(_ question: Question, _ said: Answer) {
        guard questions.contains(where: { $0.id == question.id }) else { return }
        let reply = ["answer": said.rawValue, "client": RemoteHosts.macName]
        let text = (try? JSONSerialization.data(withJSONObject: reply)).map { String(decoding: $0, as: UTF8.self) }
        question.reply(text ?? #"{"answer":"deny"}"#)
        remove(question.id)
    }

    /// An agent's question taken away: answered on another Mac, or no answer in time.
    func withdraw(tag: String) {
        for question in questions where question.tag == tag {
            question.reply(#"{"timeout":true}"#)
            remove(question.id)
        }
    }

    func remove(_ id: UUID) {
        questions.removeAll { $0.id == id }
        show()
    }

    // MARK: - Shown

    /// The window shown while a question waits, and put away when none does.
    private func show() {
        guard presents else { return }
        if let first = questions.first {
            let panel = self.panel ?? makePanel()
            self.panel = panel
            // The window is kept, and the screen's language can change meanwhile.
            panel.title = L10n[.MCP.askPanelTitle]
            if placedFor != first.id || !panel.isVisible {
                lay(panel)
                placedFor = first.id
            }
            panel.orderFrontRegardless()
        } else {
            panel?.orderOut(nil)
            placedFor = nil
        }
    }

    private func makePanel() -> NSPanel {
        let panel = NSPanel(contentRect: NSRect(x: 0, y: 0, width: 440, height: 260),
                            styleMask: [.titled, .nonactivatingPanel, .fullSizeContentView],
                            backing: .buffered, defer: false)
        panel.titleVisibility = .hidden
        panel.titlebarAppearsTransparent = true
        // Over the other apps, on every desktop, and not taking the keyboard from what is typed into.
        panel.level = .floating
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
        panel.hidesOnDeactivate = false
        panel.becomesKeyOnlyIfNeeded = true
        panel.isReleasedWhenClosed = false
        return panel
    }

    /// The question first in line laid out in the window, and the window at the top right of the screen
    /// with the mouse on it, at the size the question needs.
    ///
    /// Laid out afresh for each: the view the window kept still showed nothing — the question before
    /// answered — when it was measured for the next, its contents not yet read again. Measured as
    /// nothing wide, the window went to the right edge (16 points of it on the screen, x = 1904 of 1920)
    /// before it grew to its width, and at the least height its view would take: what is asked to be
    /// done, its box shut to a grey line.
    private func lay(_ panel: NSPanel) {
        let view = NSHostingView(rootView: AskView(center: self))
        panel.contentView = view
        let size = view.fittingSize
        let mouse = NSEvent.mouseLocation
        let screen = NSScreen.screens.first { $0.frame.contains(mouse) } ?? NSScreen.main
        guard let area = screen?.visibleFrame else { return }
        panel.setFrame(Self.frame(size, in: area), display: true)
    }

    /// Where a window of `size` goes on a screen whose free part is `area`: at its top right, 16 points
    /// in, and never past its edges.
    nonisolated static func frame(_ size: NSSize, in area: NSRect) -> NSRect {
        let width = min(size.width, max(area.width - 32, 0))
        let height = min(size.height, max(area.height - 32, 0))
        return NSRect(x: area.maxX - width - 16, y: area.maxY - height - 16, width: width, height: height)
    }
}

/// The question first in line, with its four answers, in the window over the other apps.
struct AskView: View {
    var center: AskCenter

    var body: some View {
        if let question = center.questions.first {
            VStack(alignment: .leading, spacing: 8) {
                HStack(spacing: 6) {
                    Image(systemName: "hand.raised").foregroundStyle(.orange)
                    Text(L10n[.MCP.askTitle]).font(.app(12.5, weight: .semibold))
                    Spacer()
                    if center.questions.count > 1 {
                        Text(L10n[.MCP.askMore(center.questions.count - 1)]).font(.app(10.5)).foregroundStyle(.secondary)
                    }
                }
                Text(L10n[.MCP.askCalling(question.key, question.tool)])
                    .font(.app(11.5)).fixedSize(horizontal: false, vertical: true)
                VStack(alignment: .leading, spacing: 2) {
                    ForEach(question.items(in: Language.shared.code)) { item in
                        HStack(alignment: .firstTextBaseline, spacing: 6) {
                            Text(item.id).font(.app(11, design: .monospaced))
                            Text(item.name).font(.app(11))
                            Text(item.whose)
                                .font(.app(10.5)).foregroundStyle(.secondary)
                        }
                    }
                }
                if !question.what.isEmpty {
                    ScrollView {
                        Text(question.what)
                            .font(.app(10.5, design: .monospaced))
                            .textSelection(.enabled)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    .frame(maxHeight: 96)
                    .padding(6)
                    .background(RoundedRectangle(cornerRadius: 5).fill(Color.secondary.opacity(0.08)))
                }
                Text(L10n[.MCP.askAlwaysTimeout(question.always, question.seconds / 60)])
                    .font(.app(10)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                HStack(spacing: 8) {
                    Button(AskCenter.Answer.never.said) { center.answer(question, .never) }
                    Button(AskCenter.Answer.deny.said) { center.answer(question, .deny) }
                    Spacer()
                    Button(AskCenter.Answer.always.said) { center.answer(question, .always) }
                    Button(AskCenter.Answer.once.said) { center.answer(question, .once) }
                        .buttonStyle(.borderedProminent)
                }
                .font(.app(11.5))
            }
            .padding(EdgeInsets(top: 28, leading: 16, bottom: 14, trailing: 16))
            .frame(width: 440, alignment: .leading)
        }
    }
}
