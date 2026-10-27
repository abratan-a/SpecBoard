import AppKit
import SwiftUI

/// A sequence diagram the core built (`SequenceDiagram`), drawn the way the card map draws
/// (`specboard://SpecBoard/Docs/Design:UI`「シーケンス図」): who takes part as cards along the top — an actor with its
/// figure's mark on a blue head, an outside one dashed, the flow opened with a thick border, siblings
/// bracketed under their parent's name with ⌃ left of it to fold them into it, ⌄ at the left of a head's title
/// row to open it into the actors inside —, a lifeline down from each, a step as a bar on its lifeline with its
/// node's name and heading at its left, what passes as lines in the card map's colours — solid where it is asked, dashed
/// where it answers — and a decision, a loop, ways that part, a stop, a provided flow or a flow called on the
/// lifeline it is called from as a frame. An item picked picks its card, node — or row of a process node — for
/// the property panel, and says its row and what the row says of itself when pointed at; a head double-clicked
/// opens its card or its actor's section, a frame's head the flow it stands for.
/// Zoomed and moved as the card map is; scrolled down past the heads, the heads stay at the top
/// (`SequenceHeads`).
struct SequenceCanvas: NSViewRepresentable {
    var diagram: SequenceDiagram
    var detail: ProjectDetail
    /// What is picked in the property panel, to be shown picked here.
    var selected: SequenceDiagram.Origin?
    var controller: SequenceController
    /// The provided flows' frames drawn folded to one line (an actor's chart), by the flow's id.
    var folded: Set<String> = []
    var onPick: (SequenceDiagram.Origin?) -> Void
    /// A flow's or a store's card, by id.
    var onOpen: (String) -> Void
    /// An actor's section, by id.
    var onOpenSection: (String) -> Void = { _ in }
    var onPress: (SequenceLayout.Chip) -> Void = { _ in }

    func makeNSView(context: Context) -> NSScrollView {
        let scroll = NSScrollView()
        scroll.hasVerticalScroller = true
        scroll.hasHorizontalScroller = true
        scroll.autohidesScrollers = true
        scroll.allowsMagnification = true
        scroll.minMagnification = 0.25
        scroll.maxMagnification = 3
        scroll.drawsBackground = true
        let view = SequenceView()
        scroll.documentView = view
        controller.scroll = scroll
        return scroll
    }

    func updateNSView(_ scroll: NSScrollView, context: Context) {
        guard let view = scroll.documentView as? SequenceView else { return }
        view.onPick = onPick
        view.onOpen = onOpen
        view.onOpenSection = onOpenSection
        view.onPress = onPress
        view.update(diagram: diagram, kinds: SequenceView.kinds(of: detail), lines: detail.lineKinds, selected: selected,
                    folded: folded)
        scroll.backgroundColor = view.theme.background
        controller.scroll = scroll
        controller.settle()
    }
}

/// Lets the editor's "全体を表示" fit the sequence diagram as it fits the card map.
final class SequenceController {
    weak var scroll: NSScrollView?

    /// The provided flow whose frame is to be brought under the heads once the chart has drawn it.
    private var pending: String?

    /// Scrolls the head of a provided flow's frame to just under the heads that stay at the top: now, as the
    /// chart is drawn, and once more when what was just opened has been laid out.
    func reveal(flow: String) {
        pending = flow
        settle()
        DispatchQueue.main.async { [weak self] in
            self?.settle()
            self?.pending = nil
        }
    }

    /// The frame asked for brought under the heads, if the chart has it.
    func settle() {
        guard let flow = pending, let scroll, let view = scroll.documentView as? SequenceView,
              let head = view.layout.frameHeads.first(where: { $0.flow == flow }) else { return }
        let clip = scroll.contentView
        let band = view.layout.headBand
        view.scroll(CGPoint(x: clip.bounds.minX, y: max(0, head.row.minY - band.height - 8)))
        scroll.reflectScrolledClipView(clip)
    }

    func fit() {
        guard let scroll, let view = scroll.documentView else { return }
        let size = view.bounds.size, room = scroll.contentView.bounds.size
        guard size.width > 0, size.height > 0 else { return }
        let fitting = min(1, min(room.width * scroll.magnification / size.width, room.height * scroll.magnification / size.height))
        scroll.magnification = max(scroll.minMagnification, fitting)
        view.scroll(.zero)
    }
}

/// Where everything of a sequence diagram goes, measured in the fonts it is drawn in.
struct SequenceLayout {
    typealias Item = SequenceDiagram.Item

    struct Column {
        var center: CGFloat
        var width: CGFloat
        var top: CGFloat
        var rect: CGRect { CGRect(x: center - width / 2, y: top, width: width, height: SequenceLayout.cardHeight) }
    }

    /// A small button on the diagram: a lifeline's ⌃ or ⌄, a frame's ⌃, ⌄ or ↗.
    enum Chip: Equatable {
        case lifeline(SequenceLifelines.Press)
        case foldFrame(String)
        case openFrame(String)
        case openFlow(String)
        /// A called flow's ⌄ (open it, by the core's key of the call) and ⌃ (fold it).
        case openCall(String)
        case foldCall(String)
    }

    struct PlacedChip: Equatable {
        var chip: Chip
        var rect: CGRect
        var text: String
        var tip: String
    }

    /// Siblings' heads under their parent's name: the actor they fold into.
    struct Bracket: Equatable {
        var rect: CGRect
        var label: String
        var group: String
    }

    enum Shape {
        /// A message: its line at `y`, its words in `label`.
        case message(SequenceDiagram.Message, y: CGFloat, label: CGRect)
        case note(SequenceDiagram.Note, rect: CGRect)
        /// A step: its bar, and its node's name and heading left of it.
        case step(SequenceDiagram.Step, bar: CGRect, label: CGRect)
        /// A frame: its tab, where a section after the first begins, and — a provided flow's — its head.
        case frame(SequenceDiagram.Block, rect: CGRect, tab: CGRect, sections: [(y: CGFloat, label: String)], head: FrameHead?)
    }

    /// A flow called on the lifeline it is called from, drawn as a frame around what it does: its tab (how it
    /// is called), its head row — where a press picks the call and folds or opens it —, and the rule under the
    /// head of an open one.
    struct CallFrame: Equatable {
        /// The called flow's card.
        var flow: String
        /// The core's key of the call.
        var key: String
        var folded: Bool
        var op: String
        var rect: CGRect
        var tab: CGRect
        var row: CGRect
        var rule: CGRect?

        func moved(by dx: CGFloat) -> CallFrame {
            var frame = self
            frame.rect = rect.offsetBy(dx: dx, dy: 0)
            frame.tab = tab.offsetBy(dx: dx, dy: 0)
            frame.row = row.offsetBy(dx: dx, dy: 0)
            frame.rule = rule?.offsetBy(dx: dx, dy: 0)
            return frame
        }
    }

    struct Placed {
        var shape: Shape
        /// What a click picks it by.
        var hit: CGRect
        var origin: SequenceDiagram.Origin?
        /// A flow called on the lifeline it is called from: where the arrow leaves the lifeline, to come
        /// back into the step's bar.
        var call: CGFloat? = nil
        /// The frame of a flow called on the lifeline it is called from, where the core opens it.
        var frame: CallFrame? = nil
    }

    static let margin: CGFloat = 28
    static let headerHeight: CGFloat = 26
    static let cardHeight: CGFloat = 46
    /// Above the heads, where a bracket says its parent's name.
    static let bracketBand: CGFloat = 22
    static let chipHeight: CGFloat = 16
    /// A head's chip, at the left of its title row, and a bracket's, left of its parent's name.
    static let headChipWidth: CGFloat = 18
    static let barWidth: CGFloat = 10

    static let nameFont = Typeface.mono(13, .semibold)
    static let placeFont = Typeface.system(11)
    static let badgeFont = Typeface.system(10)
    static let opFont = Typeface.mono(10.5, .semibold)
    static let textFont = Typeface.system(11.5)
    static let headingFont = Typeface.system(11.5, .semibold)
    static let noteFont = Typeface.system(11)
    static let tabFont = Typeface.mono(10.5, .semibold)
    static let stepNameFont = Typeface.mono(11, .semibold)
    static let chipFont = Typeface.system(10)

    /// A provided flow's frame head: the words it says after its tab, where a press on it folds an open frame
    /// or opens a folded one (a folded frame's whole line, an open one's head row from the tab's top down to
    /// the rule), and the rule that sets an open frame's head off, across the frame at the tab's lower edge.
    struct FrameHead: Equatable {
        var flow: String
        var name: String
        var label: String
        var folded: Bool
        var said: [String]
        var row: CGRect
        var rule: CGRect?

        func moved(by dx: CGFloat) -> FrameHead {
            var head = self
            head.row = row.offsetBy(dx: dx, dy: 0)
            head.rule = rule?.offsetBy(dx: dx, dy: 0)
            return head
        }
    }

    var columns: [Column] = []
    var placed: [Placed] = []
    var chips: [PlacedChip] = []
    var brackets: [Bracket] = []
    var size: CGSize = .zero
    /// What stays at the top when scrolled down: the brackets with their parents' names, the heads and their
    /// chips, across the whole width.
    var headBand: CGRect = .zero
    var lifelineTop: CGFloat = 0
    var lifelineBottom: CGFloat = 0

    /// A participant's word for what it is, on its card's header.
    static func badge(_ participant: SequenceDiagram.Participant, kinds: [String: SchemaKind]) -> String {
        switch participant.role {
        case .own, .flow: return "flow"
        case .actor: return "actor"
        case .store: return "store"
        case .program: return "program"
        case .anyone: return "any"
        case .noPartner, .noStore: return "gap"
        }
    }

    /// The line under a participant's name: what it is, or where a flow sits.
    static func place(_ participant: SequenceDiagram.Participant, me: Bool) -> String {
        switch participant.role {
        case .own: return L10n[.Diagram.sequenceOwnFlow(participant.place ?? "")]
        case .flow: return participant.place ?? ""
        case .actor:
            if me { return L10n[.Diagram.sequenceThisFlow] }
            let figure = (participant.figure ?? .system).label
            return participant.isItselfInItsBracket ? L10n[.Diagram.sequenceActorItself(figure)] : figure
        case .store: return L10n[.Diagram.sequenceStorePlace]
        case .program: return L10n[.Diagram.sequenceProgramPlace]
        case .anyone: return L10n[.Diagram.sequenceAnyonePlace]
        // Where the gap is: the begin or the row, the shapes no store keeps.
        case .noPartner, .noStore: return participant.place ?? ""
        }
    }

    /// The chip at the left of a head's title row: an actor's own lifeline in its bracket folds the bracket into
    /// one; a folded actor that would draw actors inside it not drawn now opens into them.
    static func titleChip(_ participant: SequenceDiagram.Participant) -> (chip: Chip, text: String, tip: String)? {
        guard participant.role == .actor, let section = participant.section else { return nil }
        if participant.isItselfInItsBracket {
            return (.lifeline(.fold(into: section)), "⌃", L10n[.Diagram.sequenceFoldInto(participant.groupName ?? participant.name)])
        }
        // No count: opening shows only the actors inside that take part.
        return participant.open ? (.lifeline(.open(section)), "⌄", L10n[.Diagram.sequenceOpenInto]) : nil
    }

    /// Whether a chip belongs to the heads (and stays at the top with them), not to a frame.
    static func isHeads(_ chip: Chip) -> Bool {
        if case .lifeline = chip { return true }
        return false
    }

    static func width(_ text: String, _ font: NSFont) -> CGFloat {
        ceil((text as NSString).size(withAttributes: [.font: font]).width)
    }

    static func height(_ text: NSAttributedString, width: CGFloat) -> CGFloat {
        ceil(text.boundingRect(with: CGSize(width: width, height: .greatestFiniteMagnitude),
                               options: [.usesLineFragmentOrigin, .usesFontLeading]).height)
    }

    /// A message's words: what it does in its line's colour, what it carries, then the flow it reaches.
    static func words(_ message: SequenceDiagram.Message, ink: NSColor, theme: GraphTheme) -> NSAttributedString {
        let out = NSMutableAttributedString()
        let style = NSMutableParagraphStyle()
        style.lineBreakMode = .byWordWrapping
        if let op = message.op {
            out.append(NSAttributedString(string: op, attributes: [.font: opFont, .foregroundColor: ink, .paragraphStyle: style]))
        }
        if !message.text.isEmpty {
            out.append(NSAttributedString(string: (out.length > 0 ? " " : "") + message.text, attributes: [
                .font: textFont, .foregroundColor: message.op == nil && !message.reply ? theme.ink : theme.ink2,
                .paragraphStyle: style,
            ]))
        }
        if let flow = message.flow {
            out.append(NSAttributedString(string: (out.length > 0 ? " " : "") + flow, attributes: [
                .font: opFont, .foregroundColor: theme.accent(for: .flow), .paragraphStyle: style,
            ]))
        }
        return out
    }

    static func noteText(_ note: SequenceDiagram.Note, theme: GraphTheme) -> NSAttributedString {
        let text = note.kind == .check ? "✓ " + note.text : note.text
        return NSAttributedString(string: text, attributes: [
            .font: noteFont,
            .foregroundColor: note.kind == .plain ? theme.ink2 : theme.ink,
        ])
    }

    /// A step's words: left of its bar, the node's name over its heading, set to the right; a flow reached
    /// on another lifeline (no node), its name on the tab over its bar.
    static func stepText(_ step: SequenceDiagram.Step, theme: GraphTheme) -> NSAttributedString {
        let style = NSMutableParagraphStyle()
        style.alignment = step.node == nil ? .left : .right
        style.lineBreakMode = .byWordWrapping
        let out = NSMutableAttributedString()
        if let node = step.node {
            out.append(NSAttributedString(string: node, attributes: [
                .font: stepNameFont, .foregroundColor: theme.accent(for: .flow), .paragraphStyle: style,
            ]))
        }
        if !step.label.isEmpty {
            out.append(NSAttributedString(string: (out.length > 0 ? "\n" : "") + step.label, attributes: [
                .font: noteFont, .foregroundColor: theme.ink2, .paragraphStyle: style,
            ]))
        }
        return out
    }

    /// A flow called on the lifeline it is called from: how it is reached, its name and label, and where it
    /// is.
    static func callText(_ step: SequenceDiagram.Step, ink: NSColor, theme: GraphTheme) -> NSAttributedString {
        let out = NSMutableAttributedString()
        let style = NSMutableParagraphStyle()
        style.lineBreakMode = .byWordWrapping
        guard let reached = step.reached else {
            return NSAttributedString(string: step.label, attributes: [.font: textFont, .foregroundColor: theme.ink2, .paragraphStyle: style])
        }
        if !reached.op.isEmpty {
            out.append(NSAttributedString(string: reached.op + " ", attributes: [
                .font: opFont, .foregroundColor: ink, .paragraphStyle: style,
            ]))
        }
        out.append(NSAttributedString(string: reached.name, attributes: [
            .font: opFont, .foregroundColor: theme.accent(for: .flow), .paragraphStyle: style,
        ]))
        // The place's string carries its own spacing: none before a full-width parenthesis, a space before "(".
        let rest = (reached.label.isEmpty ? "" : " " + reached.label)
            + (reached.place.isEmpty ? "" : L10n[.Diagram.sequenceCallPlace(reached.place)])
        if !rest.isEmpty {
            out.append(NSAttributedString(string: rest, attributes: [
                .font: textFont, .foregroundColor: theme.ink2, .paragraphStyle: style,
            ]))
        }
        return out
    }

    /// The words of a called flow's frame head, after its tab (the call's op): the flow's name in bold, then its
    /// label and where it is, paler.
    static func callHead(_ step: SequenceDiagram.Step, theme: GraphTheme) -> NSAttributedString {
        let style = NSMutableParagraphStyle()
        style.lineBreakMode = .byTruncatingTail
        guard let reached = step.reached else {
            return NSAttributedString(string: step.label, attributes: [.font: headingFont, .foregroundColor: theme.ink, .paragraphStyle: style])
        }
        let out = NSMutableAttributedString(string: reached.name, attributes: [
            .font: headingFont, .foregroundColor: theme.ink, .paragraphStyle: style,
        ])
        // The place's string carries its own spacing, as in `callText`.
        let rest = (reached.label.isEmpty ? "" : " " + reached.label)
            + (reached.place.isEmpty ? "" : L10n[.Diagram.sequenceCallPlace(reached.place)])
        if !rest.isEmpty {
            out.append(NSAttributedString(string: rest, attributes: [.font: textFont, .foregroundColor: theme.ink3, .paragraphStyle: style]))
        }
        return out
    }

    /// The arrow a call on its own lifeline stands for, coloured as an arrow of its kind is.
    static func runs(_ step: SequenceDiagram.Step, over: Int) -> SequenceDiagram.Message {
        SequenceDiagram.Message(from: over, to: over, op: step.reached.map { $0.op.isEmpty ? "run" : $0.op } ?? "run",
                                text: step.label)
    }

    /// A frame's head after its tab, as a line of words: the flow's name and label, or a block's first condition.
    static func frameHead(_ block: SequenceDiagram.Block) -> String {
        if block.kind == .flow { return [block.name, block.label].compactMap { $0 }.filter { !$0.isEmpty }.joined(separator: " ") }
        return block.sections.first.map { $0.label.isEmpty ? "" : "[\($0.label)]" } ?? ""
    }

    /// The head rows of the called flows' frames, which fold and open.
    var callRows: [CGRect] {
        placed.compactMap { $0.frame }.map(\.row)
    }

    /// The provided flows' frame heads, in the order the frames stand.
    var frameHeads: [FrameHead] {
        placed.compactMap { item in
            if case .frame(_, _, _, _, let head?) = item.shape { return head }
            return nil
        }
    }

    /// A frame's head after its tab as it is drawn: a flow's name, then its label in the same size, in a
    /// regular weight and a paler grey; a block's first condition.
    static func frameHeadText(_ block: SequenceDiagram.Block, theme: GraphTheme) -> NSAttributedString {
        let style = NSMutableParagraphStyle()
        style.lineBreakMode = .byTruncatingTail
        guard block.kind == .flow else {
            return NSAttributedString(string: frameHead(block), attributes: [
                .font: noteFont, .foregroundColor: theme.ink2, .paragraphStyle: style,
            ])
        }
        let name = block.name ?? "", label = block.label ?? ""
        let out = NSMutableAttributedString(string: label.isEmpty ? name : name + " ", attributes: [
            .font: headingFont, .foregroundColor: theme.ink, .paragraphStyle: style,
        ])
        out.append(NSAttributedString(string: label, attributes: [
            .font: textFont, .foregroundColor: theme.ink3, .paragraphStyle: style,
        ]))
        return out
    }

    init() {}

    init(_ diagram: SequenceDiagram, kinds: [String: SchemaKind], theme: GraphTheme, folded: Set<String> = [],
         ink: (SequenceDiagram.Message) -> NSColor) {
        let margin = Self.margin
        let participants = diagram.participants
        let grouped = participants.contains { $0.group != nil }
        let top = margin + (grouped ? Self.bracketBand : 0)
        // The cards: as wide as their mark, name and badge, or the line under it.
        let widths = participants.enumerated().map { index, participant -> CGFloat in
            let mark: CGFloat = participant.role == .actor || participant.role == .anyone || participant.role.isGap ? 18 : 0
            let chip: CGFloat = Self.titleChip(participant) == nil ? 0 : Self.headChipWidth + 4
            let head = chip + mark + Self.width(participant.name, Self.nameFont) + 24 + Self.width(Self.badge(participant, kinds: kinds), Self.badgeFont) + 10
            let under = Self.width(Self.place(participant, me: diagram.me == index), Self.placeFont) + 24
            return min(320, max(150, max(head, under)))
        }
        // Between two cards: room for the words of what passes between them.
        var gaps = zip(widths, widths.dropFirst()).map { ($0 + $1) / 2 + 48 }
        let messages = SequenceDiagram.flattened(diagram.items).compactMap { item -> SequenceDiagram.Message? in
            if case .message(let message) = item { return message }
            return nil
        }
        for message in messages where message.from != message.to && !gaps.isEmpty {
            let said = [message.op, message.text, message.flow].compactMap { $0 }.joined(separator: " ")
            let need = min(380, Self.width(said, Self.textFont) + 32)
            let lo = min(message.from, message.to), hi = max(message.from, message.to)
            guard hi - 1 < gaps.count else { continue }
            let span = gaps[lo..<hi].reduce(0, +)
            if span < need { gaps[hi - 1] += need - span }
        }
        var cursor = margin + (widths.first ?? 150) / 2
        for (index, width) in widths.enumerated() {
            columns.append(Column(center: cursor, width: width, top: top))
            if index < gaps.count { cursor += gaps[index] }
        }
        var right = (columns.last.map { $0.center + $0.width / 2 } ?? margin) + margin
        let cardBottom = top + Self.cardHeight

        // At the left of a head's title row: ⌄ into the actors inside, or an actor's own ⌃ folding its bracket.
        for (index, participant) in participants.enumerated() where index < columns.count {
            guard let (chip, text, tip) = Self.titleChip(participant) else { continue }
            let rect = CGRect(x: columns[index].rect.minX + 6, y: top + (Self.headerHeight - Self.chipHeight) / 2,
                              width: Self.headChipWidth, height: Self.chipHeight)
            chips.append(PlacedChip(chip: chip, rect: rect, text: text, tip: tip))
        }
        // Siblings bracketed under their parent's name, ⌃ left of it folding them into it.
        var start = 0
        while start < participants.count {
            guard let group = participants[start].group else { start += 1; continue }
            var end = start
            while end + 1 < participants.count, participants[end + 1].group == group { end += 1 }
            if end < columns.count {
                let rect = CGRect(x: columns[start].rect.minX - 6, y: margin, width: columns[end].rect.maxX - columns[start].rect.minX + 12,
                                  height: cardBottom + 4 - margin)
                let label = participants[start].groupName ?? ""
                brackets.append(Bracket(rect: rect, label: label, group: group))
                chips.append(PlacedChip(chip: .lifeline(.fold(into: group)),
                                        rect: CGRect(x: rect.minX + 6, y: rect.minY + 3, width: Self.headChipWidth, height: Self.chipHeight),
                                        text: "⌃", tip: L10n[.Diagram.sequenceFoldInto(label)]))
            }
            start = end + 1
        }
        lifelineTop = cardBottom
        headBand = CGRect(x: 0, y: margin - 8, width: 0, height: cardBottom + 10 - (margin - 8))

        // Down the page, item by item.
        var y = cardBottom + 22
        func center(_ index: Int) -> CGFloat { index < columns.count ? columns[index].center : margin }
        func cardWidth(_ index: Int) -> CGFloat { index < widths.count ? widths[index] : 150 }
        /// The bars open on each lifeline: a bar inside another stands to its right.
        var open: [Int: Int] = [:]
        /// The right edge of the open frame of a called flow being laid, for a folded one inside it to reach.
        var callRight: CGFloat?
        /// The step labels beside the items being laid: words that would cross one go under it.
        var labels: [CGRect] = []
        func reach(_ items: [Item]) -> Set<Int> {
            var out: Set<Int> = []
            for item in SequenceDiagram.flattened(items) {
                switch item {
                case .message(let message): out.insert(message.from); out.insert(message.to)
                case .note(let note): out.insert(note.over)
                case .step(let step): out.insert(step.over)
                case .block: continue
                }
            }
            return out
        }
        func lay(_ items: [Item], depth: Int) {
            for item in items {
                switch item {
                case .message(let message):
                    let from = center(message.from), to = center(message.to)
                    let alone = message.from == message.to
                    // Its words start right of the bars open on the lifeline it starts nearest.
                    let near = from <= to ? message.from : message.to
                    let left: CGFloat = alone ? from + 40 : min(from, to) + 12 + CGFloat(open[near, default: 0]) * 6
                    let room: CGFloat = alone ? 260 : max(120, abs(to - from) - 20)
                    let words = Self.words(message, ink: ink(message), theme: theme)
                    let height = Self.height(words, width: room)
                    // As wide as its words, so that what stands beside them is seen not to cross them.
                    let natural = ceil(words.boundingRect(with: CGSize(width: room, height: .greatestFiniteMagnitude),
                                                          options: [.usesLineFragmentOrigin, .usesFontLeading]).width) + 2
                    var label = CGRect(x: left, y: y, width: min(room, natural), height: height)
                    // Neither its words nor its line cross a step's name: both go under the lowest one they meet.
                    let lo = alone ? from : min(from, to), hi = alone ? from + 40 : max(from, to)
                    func crossed() -> CGFloat? {
                        let line = label.minY + height + 5
                        return labels.filter { name in
                            name.intersects(label)
                                || (name.minX < hi && name.maxX > lo && line >= name.minY - 2 && line <= name.maxY + 2)
                        }.map(\.maxY).max()
                    }
                    while let below = crossed(), below + 4 > label.minY {
                        y = below + 4
                        label.origin.y = y
                    }
                    let line: CGFloat = y + height + 5
                    let low: CGFloat = alone ? line + 18 : line
                    let spread: CGFloat = max(abs(to - from), room + 40) + 12
                    let hit = CGRect(x: min(min(from, to), left) - 6, y: y - 3, width: spread, height: low - y + 9)
                    placed.append(Placed(shape: .message(message, y: line, label: label), hit: hit, origin: message.origin))
                    right = max(right, label.maxX + margin)
                    y = low + 16
                case .note(let note):
                    let text = Self.noteText(note, theme: theme)
                    let wide: CGFloat = min(note.kind == .mark ? 340 : 280, max(80, Self.width(text.string, Self.noteFont) + 22))
                    let height: CGFloat = Self.height(text, width: wide - 20) + 10
                    // A mark stands beside its lifeline, right of what goes on along it; a note inside a bar stands
                    // right of the bars, clear of them and of the steps' names left of them.
                    let bars = open[note.over, default: 0]
                    let x: CGFloat = note.kind == .mark ? center(note.over) + 14
                        : bars > 0 ? center(note.over) + Self.barWidth / 2 + CGFloat(bars - 1) * 6 + 8
                        : center(note.over) - wide / 2
                    var rect = CGRect(x: x, y: y, width: wide, height: height)
                    if let below = labels.filter({ $0.intersects(rect) }).map(\.maxY).max() {
                        y = below + 4
                        rect.origin.y = y
                    }
                    placed.append(Placed(shape: .note(note, rect: rect), hit: rect.insetBy(dx: -3, dy: -3), origin: note.origin))
                    right = max(right, rect.maxX + margin)
                    y += height + 12
                case .step(let step):
                    if let reached = step.reached, let key = reached.call {
                        // A flow called on the lifeline it is called from: a frame around what it does, as alt
                        // and loop are drawn — its tab the call's op, its head the flow —, and, where it is open, a
                        // bar on the lifeline inside it. The words of the head stand right of the bars open on the
                        // lifelines, and the whole frame under the names of the steps it is called from.
                        let nested = open[step.over, default: 0]
                        let x = center(step.over) + CGFloat(nested) * 6
                        let word = reached.op.isEmpty ? "run" : reached.op
                        let tabWidth = Self.width(word, Self.tabFont) + 16
                        let said = Self.callHead(step, theme: theme)
                        let saidWidth = ceil(said.size().width) + 4
                        let spread = reach(step.items).union([step.over])
                        let lo = spread.min() ?? step.over, hi = spread.max() ?? step.over
                        let inset = CGFloat(depth) * 8
                        let left = center(lo) - min(110, cardWidth(lo) / 2) - 14 + inset
                        let edge = center(hi) + min(110, cardWidth(hi) / 2) + 14 - inset
                        let bars = open.filter { $0.value > 0 && $0.key < columns.count }.map {
                            (center($0.key) - Self.barWidth / 2)...(center($0.key) + CGFloat($0.value - 1) * 6 + Self.barWidth / 2)
                        }.sorted { $0.lowerBound < $1.lowerBound }
                        var wordsX = left + tabWidth + 6
                        for bar in bars where wordsX < bar.upperBound + 4 && wordsX + saidWidth > bar.lowerBound - 4 {
                            wordsX = bar.upperBound + 8
                        }
                        let wordsEnd = wordsX + saidWidth
                        var at = y
                        func headBox(_ at: CGFloat) -> CGRect { CGRect(x: left, y: at, width: wordsEnd - left, height: 20) }
                        while let below = labels.filter({ $0.intersects(headBox(at)) }).map(\.maxY).max(), below + 4 > at {
                            at = below + 4
                        }
                        var width = max(edge - left, wordsEnd + 70 - left)
                        // A folded call stands as wide as the open frame it is in (its own contents are not drawn).
                        if reached.folded, let right = callRight { width = max(width, right - 8 - left) }
                        let tab = CGRect(x: left, y: at, width: tabWidth, height: 20)
                        let wordsRect = CGRect(x: wordsX, y: at, width: saidWidth, height: 20)
                        let index = placed.count
                        placed.append(Placed(shape: .note(.init(over: 0, text: "", kind: .plain), rect: .zero), hit: .zero, origin: nil))
                        let rect: CGRect, row: CGRect, rule: CGRect?, bar: CGRect
                        if reached.folded {
                            rect = CGRect(x: left, y: at, width: width, height: 24)
                            row = rect
                            rule = nil
                            bar = CGRect(x: x - Self.barWidth / 2, y: at + 24, width: Self.barWidth, height: 0)
                            y = at + 24 + 10
                        } else {
                            let barTop = at + 24
                            open[step.over, default: 0] += 1
                            y = barTop + 6
                            let enclosing = callRight
                            callRight = left + width
                            lay(step.items, depth: depth + 1)
                            callRight = enclosing
                            open[step.over, default: 1] -= 1
                            let bottom = max(y - 6, barTop + 14)
                            let frameBottom = bottom + 8
                            rect = CGRect(x: left, y: at, width: width, height: frameBottom - at)
                            row = CGRect(x: left, y: at, width: width, height: 20)
                            rule = CGRect(x: left, y: at + 20, width: width, height: 0)
                            bar = CGRect(x: x - Self.barWidth / 2, y: barTop, width: Self.barWidth, height: bottom - barTop)
                            y = frameBottom + 12
                        }
                        let frame = CallFrame(flow: reached.flow, key: key, folded: reached.folded, op: word,
                                              rect: rect, tab: tab, row: row, rule: rule)
                        placed[index] = Placed(shape: .step(step, bar: bar, label: wordsRect), hit: row, origin: step.origin, frame: frame)
                        // ↗ opens the flow; ⌄ or ⌃ opens or folds the call.
                        chips.append(PlacedChip(chip: .openFlow(reached.flow),
                                                rect: CGRect(x: rect.maxX - 28, y: at + 3, width: 22, height: Self.chipHeight),
                                                text: "↗", tip: L10n[.Diagram.sequenceOpenFlow]))
                        chips.append(PlacedChip(chip: reached.folded ? .openCall(key) : .foldCall(key),
                                                rect: CGRect(x: rect.maxX - 54, y: at + 3, width: 22, height: Self.chipHeight),
                                                text: reached.folded ? "⌄" : "⌃",
                                                tip: reached.folded ? L10n[.Diagram.sequenceOpenCall] : L10n[.Diagram.sequenceFoldCall]))
                        right = max(right, rect.maxX + margin)
                        continue
                    }
                    let nested = open[step.over, default: 0]
                    let x = center(step.over) + CGFloat(nested) * 6
                    let index = placed.count
                    placed.append(Placed(shape: .note(.init(over: 0, text: "", kind: .plain), rect: .zero), hit: .zero, origin: nil))
                    let label: CGRect
                    let barTop: CGFloat
                    var call: CGFloat?
                    if step.node == nil && nested > 0 {
                        // A flow called on the lifeline it is called from: the run goes out and comes back into
                        // a bar inside the caller's, its words right of the lifeline.
                        let words = Self.callText(step, ink: ink(Self.runs(step, over: step.over)), theme: theme)
                        let room: CGFloat = 300
                        let natural = ceil(words.boundingRect(with: CGSize(width: room, height: .greatestFiniteMagnitude),
                                                              options: [.usesLineFragmentOrigin, .usesFontLeading]).width) + 2
                        label = CGRect(x: center(step.over) + 40, y: y, width: min(room, natural), height: Self.height(words, width: room))
                        let line = label.maxY + 5
                        call = line
                        barTop = line + 12
                    } else if step.node == nil {
                        // A flow reached on another lifeline: its name on a tab over its bar, the bar under it,
                        // clear of the names of its own steps left of the lifeline.
                        let text = Self.stepText(step, theme: theme)
                        let wide = Self.width(text.string, Self.noteFont) + 12
                        let high = Self.height(text, width: wide - 10) + 4
                        label = CGRect(x: x - Self.barWidth / 2, y: y, width: wide, height: high)
                        barTop = label.maxY + 2
                    } else {
                        // Never broken inside the node's name: the gutter is as wide as it, and the diagram
                        // moves right where it reaches past the margin. Left of every bar on the lifeline.
                        let text = Self.stepText(step, theme: theme)
                        let name = step.node.map { Self.width($0, Self.stepNameFont) + 4 } ?? 0
                        let heading = Self.width(step.label, Self.noteFont) + 4
                        let wide = max(name, min(220, max(40, heading)))
                        let high = Self.height(text, width: wide)
                        label = CGRect(x: center(step.over) - Self.barWidth / 2 - 6 - wide, y: y, width: wide, height: high)
                        barTop = y
                    }
                    open[step.over, default: 0] += 1
                    labels.append(label)
                    y = barTop + 4
                    lay(step.items, depth: depth)
                    labels.removeAll { $0 == label }
                    open[step.over, default: 1] -= 1
                    let bottom = max(y - 6, step.node == nil ? barTop + 14 : label.maxY + 2)
                    let bar = CGRect(x: x - Self.barWidth / 2, y: barTop, width: Self.barWidth, height: bottom - barTop)
                    placed[index] = Placed(shape: .step(step, bar: bar, label: label), hit: label.union(bar), origin: step.origin,
                                           call: call)
                    right = max(right, label.maxX + margin)
                    y = bottom + 12
                case .block(let block):
                    let inset = CGFloat(depth) * 8
                    let index = placed.count
                    placed.append(Placed(shape: .note(.init(over: 0, text: "", kind: .plain), rect: .zero), hit: .zero, origin: nil))
                    let left: CGFloat, rightEdge: CGFloat
                    if block.kind == .flow {
                        // A provided flow's frame goes across the whole chart.
                        left = (columns.first?.rect.minX ?? margin) - 14 + inset
                        rightEdge = (columns.last?.rect.maxX ?? margin + 150) + 14 - inset
                    } else {
                        let reach = reach(block.sections.flatMap(\.items)).union([diagram.me ?? 0])
                        let lo = reach.min() ?? 0, hi = reach.max() ?? 0
                        left = center(lo) - min(110, cardWidth(lo) / 2) - 14 + inset
                        rightEdge = center(hi) + min(110, cardWidth(hi) / 2) + 14 - inset
                    }
                    let top = y
                    let word = block.kind.rawValue
                    let tab = CGRect(x: left, y: top, width: Self.width(word, Self.tabFont) + 16, height: 20)
                    let headRect = CGRect(x: tab.maxX + 6, y: top, width: ceil(Self.frameHeadText(block, theme: theme).size().width) + 4,
                                          height: 20)
                    func head(folded: Bool, rect: CGRect) -> FrameHead? {
                        guard let flow = block.flow, block.kind == .flow else { return nil }
                        let name = block.name ?? "", label = block.label ?? ""
                        return FrameHead(flow: flow, name: name, label: label, folded: folded,
                                         said: [name, label].filter { !$0.isEmpty },
                                         row: folded ? rect : CGRect(x: rect.minX, y: top, width: rect.width, height: tab.height),
                                         rule: folded ? nil : CGRect(x: rect.minX, y: tab.maxY, width: rect.width, height: 0))
                    }
                    if let flow = block.flow, block.kind == .flow, folded.contains(flow) {
                        // Folded to one line: its tab, name and label; a press anywhere on it opens it.
                        let rect = CGRect(x: left, y: top, width: max(tab.width + headRect.width + 70, rightEdge - left), height: 24)
                        placed[index] = Placed(shape: .frame(block, rect: rect, tab: tab, sections: [], head: head(folded: true, rect: rect)),
                                               hit: rect, origin: block.origin)
                        frameChips(flow, open: false, rect: rect)
                        right = max(right, rect.maxX + margin)
                        y = top + 24 + 10
                        continue
                    }
                    y += 28
                    var sections: [(y: CGFloat, label: String)] = []
                    for (number, section) in block.sections.enumerated() {
                        if number > 0 && block.kind != .flow {
                            sections.append((y - 4, section.label))
                            y += 22
                        }
                        lay(section.items, depth: depth + 1)
                    }
                    y += 4
                    let rect = CGRect(x: left, y: top, width: max(tab.width + headRect.width + 70, rightEdge - left), height: y - top)
                    let opened = head(folded: false, rect: rect)
                    placed[index] = Placed(shape: .frame(block, rect: rect, tab: tab, sections: sections, head: opened),
                                           hit: opened?.row ?? tab.union(headRect), origin: block.origin)
                    if let flow = block.flow, block.kind == .flow { frameChips(flow, open: true, rect: rect) }
                    right = max(right, rect.maxX + margin)
                    y += 12
                }
            }
        }
        /// A frame's ⌃ or ⌄ and ↗, at its top right.
        func frameChips(_ flow: String, open: Bool, rect: CGRect) {
            let go = PlacedChip(chip: .openFlow(flow), rect: CGRect(x: rect.maxX - 28, y: rect.minY + 3, width: 22, height: Self.chipHeight),
                                text: "↗", tip: L10n[.Diagram.sequenceOpenFlow])
            let fold = PlacedChip(chip: open ? .foldFrame(flow) : .openFrame(flow),
                                  rect: CGRect(x: rect.maxX - 54, y: rect.minY + 3, width: 22, height: Self.chipHeight),
                                  text: open ? "⌃" : "⌄", tip: open ? L10n[.Diagram.sequenceFoldFrame] : L10n[.Diagram.sequenceOpenFrame])
            chips += [fold, go]
        }
        let items = diagram.unshown.map { [Item.note(.init(over: 0, text: $0, kind: .plain))] } ?? diagram.items
        lay(items, depth: 0)
        // A note, a frame or a step's words reach left of the first card: everything moves right together
        // until it starts at the margin, or the view, which begins at zero, would cut it off.
        let shift = margin - min(margin, leftmost())
        if shift > 0 { move(by: shift) }
        lifelineBottom = y + 6
        size = CGSize(width: right + shift, height: lifelineBottom + margin)
        headBand.size.width = size.width
    }

    /// The left edge of what is drawn: the cards, the brackets, the notes, the messages' words, the steps'
    /// words and the frames.
    func leftmost() -> CGFloat {
        var edge = columns.map(\.rect.minX).min() ?? .greatestFiniteMagnitude
        edge = min(edge, brackets.map(\.rect.minX).min() ?? edge, chips.map(\.rect.minX).min() ?? edge)
        for item in placed {
            switch item.shape {
            case .message(_, _, let label): edge = min(edge, label.minX)
            case .note(_, let rect): if rect != .zero { edge = min(edge, rect.minX) }
            case .step(_, let bar, let label): edge = min(edge, bar.minX, label.minX)
            case .frame(_, let rect, let tab, _, _): edge = min(edge, rect.minX, tab.minX)
            }
            if let frame = item.frame { edge = min(edge, frame.rect.minX, frame.tab.minX) }
        }
        return edge
    }

    private mutating func move(by dx: CGFloat) {
        for index in columns.indices { columns[index].center += dx }
        chips = chips.map { var chip = $0; chip.rect = chip.rect.offsetBy(dx: dx, dy: 0); return chip }
        brackets = brackets.map { var bracket = $0; bracket.rect = bracket.rect.offsetBy(dx: dx, dy: 0); return bracket }
        placed = placed.map { item in
            var item = item
            switch item.shape {
            case .message(let message, let y, let label):
                item.shape = .message(message, y: y, label: label.offsetBy(dx: dx, dy: 0))
            case .note(let note, let rect):
                item.shape = .note(note, rect: rect == .zero ? rect : rect.offsetBy(dx: dx, dy: 0))
            case .step(let step, let bar, let label):
                item.shape = .step(step, bar: bar.offsetBy(dx: dx, dy: 0), label: label.offsetBy(dx: dx, dy: 0))
            case .frame(let block, let rect, let tab, let sections, let head):
                item.shape = .frame(block, rect: rect.offsetBy(dx: dx, dy: 0), tab: tab.offsetBy(dx: dx, dy: 0),
                                    sections: sections, head: head?.moved(by: dx))
            }
            if item.hit != .zero { item.hit = item.hit.offsetBy(dx: dx, dy: 0) }
            item.frame = item.frame?.moved(by: dx)
            return item
        }
    }

    /// What pointing at `point` says: a chip's use, or an item's row — its path in the flow and its words —
    /// and what the row says of itself.
    func tip(at point: CGPoint) -> String? {
        if let chip = chips.last(where: { $0.rect.contains(point) }) { return chip.tip }
        guard let item = placed.last(where: { $0.hit.contains(point) && $0.origin != nil }), let origin = item.origin else {
            return nil
        }
        let row = [origin.node, origin.row.map { "steps[\($0)]" }].compactMap { $0 }.joined(separator: ".")
        var said: [String] = []
        var described: String?
        switch item.shape {
        case .message(let message, _, _):
            said = [message.op, message.text, message.flow].compactMap { $0 }.filter { !$0.isEmpty }
            described = message.description
        case .note(let note, _):
            said = [note.text]
            described = note.description
        case .step(let step, _, _):
            said = [step.label]
            if step.reached?.cycle == true { described = L10n[.Diagram.sequenceCallCycle] }
        case .frame(let block, _, _, _, _):
            said = [Self.frameHead(block)]
        }
        let head = ([row] + said).filter { !$0.isEmpty }.joined(separator: " ")
        return ([head] + [described].compactMap { $0 }.filter { !$0.isEmpty }).joined(separator: "\n")
    }
}

/// The diagram itself, top-left origin like the card map's cards.
final class SequenceView: NSView, NSViewToolTipOwner {
    var onPick: (SequenceDiagram.Origin?) -> Void = { _ in }
    var onOpen: (String) -> Void = { _ in }
    var onOpenSection: (String) -> Void = { _ in }
    var onPress: (SequenceLayout.Chip) -> Void = { _ in }
    private(set) var diagram = SequenceDiagram()
    private var kinds: [String: SchemaKind] = [:]
    private var selected: SequenceDiagram.Origin?
    private var folded: Set<String> = []
    private(set) var layout = SequenceLayout()
    /// The project's kinds of line: a message is drawn in the colour of the line the card map draws for the
    /// same thing.
    private var lines = LineKinds()

    override var isFlipped: Bool { true }

    /// Where the pointing hand shows: what a press folds or opens.
    var pointing: [CGRect] { layout.frameHeads.map(\.row) + layout.callRows }

    override func resetCursorRects() {
        for rect in pointing { addCursorRect(rect, cursor: .pointingHand) }
    }

    var theme: GraphTheme { GraphTheme.current(for: effectiveAppearance) }

    /// The kind of each card, by id: a record's head is coloured as its card.
    static func kinds(of detail: ProjectDetail) -> [String: SchemaKind] {
        Dictionary(detail.schemas.map { ($0.id, $0.kind) }, uniquingKeysWith: { a, _ in a })
    }

    override init(frame: NSRect) {
        super.init(frame: frame)
        hearLanguage()
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        hearLanguage()
    }

    /// The cards' badges and places are measured into the layout, so a change of the screen's language
    /// lays it out again.
    private func hearLanguage() {
        NotificationCenter.default.addObserver(self, selector: #selector(languageChanged), name: Language.changed, object: nil)
    }

    @objc private func languageChanged() {
        relayout()
        needsDisplay = true
    }

    func update(diagram: SequenceDiagram, kinds: [String: SchemaKind], lines: LineKinds, selected: SequenceDiagram.Origin?,
                folded: Set<String> = []) {
        let changed = diagram != self.diagram || kinds != self.kinds || lines != self.lines || folded != self.folded
        self.diagram = diagram
        self.kinds = kinds
        self.lines = lines
        self.selected = selected
        self.folded = folded
        if changed || layout.columns.count != diagram.participants.count {
            relayout()
        }
        needsDisplay = true
    }

    private func relayout() {
        layout = SequenceLayout(diagram, kinds: kinds, theme: theme, folded: folded, ink: ink)
        setFrameSize(layout.size)
        removeAllToolTips()
        for item in layout.placed where item.origin != nil && item.hit != .zero { addToolTip(item.hit, owner: self, userData: nil) }
        for chip in layout.chips { addToolTip(chip.rect, owner: self, userData: nil) }
        if followed != nil { heads.place() }
        window?.invalidateCursorRects(for: self)
    }

    // MARK: Heads at the top

    /// The heads kept at the top of the scroll view the diagram is shown in, once it is scrolled past them.
    private(set) lazy var heads = SequenceHeads(canvas: self)
    /// The clip view whose scrolling and zooming the heads follow.
    private weak var followed: NSClipView?

    override func viewDidMoveToSuperview() {
        super.viewDidMoveToSuperview()
        let clip = enclosingScrollView?.contentView
        guard clip !== followed else { return }
        if let old = followed {
            NotificationCenter.default.removeObserver(self, name: NSView.boundsDidChangeNotification, object: old)
            NotificationCenter.default.removeObserver(self, name: NSView.frameDidChangeNotification, object: old)
        }
        heads.removeFromSuperview()
        followed = clip
        guard let scroll = enclosingScrollView, let clip else { return }
        // Over what scrolls, under the scrollers: it takes the presses on the heads it draws.
        scroll.addSubview(heads, positioned: .above, relativeTo: clip)
        clip.postsBoundsChangedNotifications = true
        clip.postsFrameChangedNotifications = true
        NotificationCenter.default.addObserver(self, selector: #selector(scrolled), name: NSView.boundsDidChangeNotification, object: clip)
        NotificationCenter.default.addObserver(self, selector: #selector(scrolled), name: NSView.frameDidChangeNotification, object: clip)
        heads.place()
    }

    /// Scrolled, zoomed or resized: the heads follow.
    @objc private func scrolled() {
        heads.place()
    }

    func view(_ view: NSView, stringForToolTip tag: NSView.ToolTipTag, point: NSPoint, userData data: UnsafeMutableRawPointer?) -> String {
        layout.tip(at: point) ?? ""
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        relayout()
        needsDisplay = true
        heads.needsDisplay = true
    }

    /// What is not written: the warning orange, so a gap is not read as one more partner.
    static let gapInk = NSColor.systemOrange

    /// Whether a message goes to or comes from a lane of what is not written.
    func touchesGap(_ message: SequenceDiagram.Message) -> Bool {
        [message.from, message.to].contains { diagram.participants.indices.contains($0) && diagram.participants[$0].role.isGap }
    }

    /// A message's line: in the colour of the line the card map draws for the same thing — what it runs,
    /// reads, writes or sends — grey where it answers, and orange to or from a gap.
    func ink(_ message: SequenceDiagram.Message) -> NSColor {
        let dark = theme.isDark
        if touchesGap(message) { return Self.gapInk }
        if message.reply { return theme.ink3 }
        // The fetch is worded in the diagram's language, so it is compared with that.
        let fetch = L10n.text(.Charts.opFetch, in: diagram.language)
        switch message.op {
        case "read"?: return lines.look(Line.reads).ink.color(dark: dark)
        case "write"?: return lines.look(Line.writes).ink.color(dark: dark)
        case "send"?, "receive"?, "return"?: return lines.look(Line.partner).ink.color(dark: dark)
        case let op? where op == fetch: return lines.look(Line.partner).ink.color(dark: dark)
        default: return lines.look(Line.runs).ink.color(dark: dark)
        }
    }

    private func picked(_ origin: SequenceDiagram.Origin?) -> Bool {
        guard let origin, let selected, let node = selected.node, origin.node == node else { return false }
        if let card = selected.card, let at = origin.card, card != at { return false }
        return selected.row == nil || origin.row == selected.row
    }

    // MARK: Drawing

    override func draw(_ dirtyRect: NSRect) {
        guard let ctx = NSGraphicsContext.current?.cgContext else { return }
        let theme = theme
        ctx.setFillColor(theme.background.cgColor)
        ctx.fill(bounds)

        for bracket in layout.brackets { drawBracket(bracket, theme: theme, in: ctx) }

        // Lifelines, under everything; a gap's in orange.
        ctx.setLineWidth(1)
        ctx.setLineDash(phase: 0, lengths: [4, 4])
        for (index, column) in layout.columns.enumerated() {
            let gap = diagram.participants.indices.contains(index) && diagram.participants[index].role.isGap
            ctx.setStrokeColor(gap ? Self.gapInk.withAlphaComponent(0.75).cgColor : theme.line2.cgColor)
            ctx.move(to: CGPoint(x: column.center, y: layout.lifelineTop))
            ctx.addLine(to: CGPoint(x: column.center, y: layout.lifelineBottom))
            ctx.strokePath()
        }
        ctx.setLineDash(phase: 0, lengths: [])

        for item in layout.placed {
            let on = picked(item.origin)
            switch item.shape {
            case .frame(let block, let rect, let tab, let sections, let head):
                drawFrame(block, rect: rect, tab: tab, sections: sections, head: head,
                          picked: block.kind == .flow ? block.flow.map(picksFrame) ?? false : on, theme: theme, in: ctx)
            case .step(let step, let bar, let label):
                if let frame = item.frame {
                    drawCall(step, bar: bar, label: label, frame: frame, picked: on, theme: theme, in: ctx)
                } else {
                    drawStep(step, bar: bar, label: label, call: item.call, picked: on, theme: theme, in: ctx)
                }
            case .message(let message, let y, let label):
                if on { highlight(item.hit, theme: theme, in: ctx) }
                drawMessage(message, y: y, label: label, theme: theme, in: ctx)
            case .note(let note, let rect):
                guard rect != .zero else { continue }
                drawNote(note, rect: rect, picked: on, theme: theme, in: ctx)
            }
        }

        for (index, column) in layout.columns.enumerated() where index < diagram.participants.count {
            drawCard(diagram.participants[index], me: diagram.me == index, rect: column.rect, theme: theme, in: ctx)
        }
        for chip in layout.chips { drawChip(chip, theme: theme, in: ctx) }
    }

    /// The heads as they stay at the top (`SequenceHeads`), in the diagram's own coordinates: the brackets with
    /// their parents' names, the heads and their chips, those that meet `visible` alone.
    func drawHeads(visible: CGRect, in ctx: CGContext) {
        let theme = theme
        for bracket in layout.brackets where bracket.rect.minX <= visible.maxX && bracket.rect.maxX >= visible.minX {
            drawBracket(bracket, theme: theme, in: ctx)
        }
        for (index, column) in layout.columns.enumerated() where index < diagram.participants.count {
            let rect = column.rect
            guard rect.minX <= visible.maxX && rect.maxX >= visible.minX else { continue }
            drawCard(diagram.participants[index], me: diagram.me == index, rect: rect, theme: theme, in: ctx)
        }
        for chip in layout.chips where SequenceLayout.isHeads(chip.chip) {
            drawChip(chip, theme: theme, in: ctx)
        }
    }

    private func highlight(_ rect: CGRect, theme: GraphTheme, in ctx: CGContext) {
        let path = CGPath(roundedRect: rect, cornerWidth: 6, cornerHeight: 6, transform: nil)
        ctx.addPath(path)
        ctx.setFillColor(theme.selection.cgColor)
        ctx.fillPath()
        ctx.addPath(path)
        ctx.setStrokeColor(theme.brass.withAlphaComponent(0.7).cgColor)
        ctx.setLineWidth(1)
        ctx.strokePath()
    }

    private func drawBracket(_ bracket: SequenceLayout.Bracket, theme: GraphTheme, in ctx: CGContext) {
        let blue = CardRenderer.actorBlue
        let path = CGPath(roundedRect: bracket.rect, cornerWidth: 8, cornerHeight: 8, transform: nil)
        ctx.addPath(path)
        ctx.setFillColor(blue.withAlphaComponent(theme.isDark ? 0.10 : 0.05).cgColor)
        ctx.fillPath()
        ctx.addPath(path)
        ctx.setStrokeColor(blue.withAlphaComponent(0.45).cgColor)
        ctx.setLineWidth(1)
        ctx.strokePath()
        // Its ⌃ stands first, left of the mark and the parent's name.
        let markRect = CGRect(x: bracket.rect.minX + 6 + SequenceLayout.headChipWidth + 4, y: bracket.rect.minY + 4, width: 12, height: 12)
        symbol(ActorFigure.system.symbol, in: markRect, tint: blue, size: 10)
        draw(bracket.label, in: CGRect(x: markRect.maxX + 4, y: bracket.rect.minY + 2,
                                       width: max(0, bracket.rect.maxX - markRect.maxX - 12), height: 16),
             font: SequenceLayout.badgeFont, color: blue, centered: true)
    }

    private func drawChip(_ chip: SequenceLayout.PlacedChip, theme: GraphTheme, in ctx: CGContext) {
        let path = CGPath(roundedRect: chip.rect, cornerWidth: chip.rect.height / 2, cornerHeight: chip.rect.height / 2, transform: nil)
        ctx.addPath(path)
        ctx.setFillColor(theme.panel2.cgColor)
        ctx.fillPath()
        ctx.addPath(path)
        ctx.setStrokeColor(theme.line2.cgColor)
        ctx.setLineWidth(1)
        ctx.strokePath()
        let style = NSMutableParagraphStyle()
        style.alignment = .center
        let text = NSAttributedString(string: chip.text, attributes: [.font: SequenceLayout.chipFont, .foregroundColor: theme.ink2,
                                                                      .paragraphStyle: style])
        let height = ceil(SequenceLayout.chipFont.ascender - SequenceLayout.chipFont.descender)
        text.draw(with: CGRect(x: chip.rect.minX, y: chip.rect.midY - height / 2, width: chip.rect.width, height: height),
                  options: [.usesLineFragmentOrigin])
    }

    private func symbol(_ name: String, in rect: CGRect, tint: NSColor, size: CGFloat) {
        let config = NSImage.SymbolConfiguration(pointSize: size, weight: .regular)
            .applying(NSImage.SymbolConfiguration(paletteColors: [tint]))
        guard let image = NSImage(systemSymbolName: name, accessibilityDescription: nil)?.withSymbolConfiguration(config) else { return }
        let fit = image.size
        let target = CGRect(x: rect.midX - fit.width / 2, y: rect.midY - fit.height / 2, width: fit.width, height: fit.height)
        image.draw(in: target, from: .zero, operation: .sourceOver, fraction: 1, respectFlipped: true, hints: nil)
    }

    private func drawCard(_ participant: SequenceDiagram.Participant, me: Bool, rect: CGRect, theme: GraphTheme, in ctx: CGContext) {
        let body = rect.insetBy(dx: 0.5, dy: 0.5)
        let outline = CGPath(roundedRect: body, cornerWidth: 8, cornerHeight: 8, transform: nil)
        ctx.addPath(outline)
        ctx.setFillColor(theme.panel.cgColor)
        ctx.fillPath()
        let blue = CardRenderer.actorBlue
        let fill: NSColor
        let accent: NSColor
        switch participant.role {
        case .own, .flow:
            fill = theme.headerFill(for: .flow); accent = theme.accent(for: .flow)
        case .actor:
            fill = theme.panel3.blended(withFraction: 0.18, of: blue) ?? theme.panel3; accent = blue
        case .store:
            fill = theme.headerFill(for: .store); accent = theme.accent(for: .store)
        case .program, .anyone:
            fill = theme.panel3; accent = theme.ink3
        case .noPartner, .noStore:
            fill = Self.gapInk.withAlphaComponent(theme.isDark ? 0.16 : 0.10); accent = Self.gapInk
        }
        let header = CGRect(x: body.minX, y: body.minY, width: body.width, height: SequenceLayout.headerHeight)
        ctx.saveGState()
        ctx.addPath(outline)
        ctx.clip()
        ctx.setFillColor(fill.cgColor)
        ctx.fill(header)
        ctx.restoreGState()
        ctx.setStrokeColor(theme.line.cgColor)
        ctx.setLineWidth(1)
        ctx.move(to: CGPoint(x: body.minX, y: header.maxY))
        ctx.addLine(to: CGPoint(x: body.maxX, y: header.maxY))
        ctx.strokePath()

        let badge = SequenceLayout.badge(participant, kinds: kinds)
        let badgeWidth = SequenceLayout.width(badge, SequenceLayout.badgeFont)
        var inset = header.insetBy(dx: 12, dy: 0)
        if SequenceLayout.titleChip(participant) != nil {
            // Its ⌄ or ⌃ stands at the left of the title row, before the mark.
            let left = header.minX + 6 + SequenceLayout.headChipWidth + 4
            inset = CGRect(x: left, y: inset.minY, width: inset.maxX - left, height: inset.height)
        }
        if participant.role.isGap || participant.role == .anyone {
            // A gap says it is one before its words; anyone, that the whole world may call.
            symbol(participant.role.isGap ? "exclamationmark.triangle.fill" : "globe",
                   in: CGRect(x: inset.minX, y: inset.minY, width: 14, height: inset.height),
                   tint: participant.role.isGap ? Self.gapInk : theme.ink2, size: 11)
            inset = CGRect(x: inset.minX + 18, y: inset.minY, width: inset.width - 18, height: inset.height)
        } else if participant.role == .actor {
            // Its figure's mark, as the nav panel draws its section.
            let mark = participant.mark
            let tint = mark?.color.flatMap { HexColor.nsColor(hex: $0) } ?? blue
            symbol(mark?.symbol ?? (participant.figure ?? .system).symbol,
                   in: CGRect(x: inset.minX, y: inset.minY, width: 14, height: inset.height), tint: tint, size: 11)
            inset = CGRect(x: inset.minX + 18, y: inset.minY, width: inset.width - 18, height: inset.height)
        }
        draw(participant.name, in: CGRect(x: inset.minX, y: inset.minY, width: inset.width - badgeWidth - 8, height: inset.height),
             font: SequenceLayout.nameFont, color: participant.role.isGap ? Self.gapInk : theme.ink, centered: true)
        draw(badge, in: CGRect(x: inset.maxX - badgeWidth, y: inset.minY, width: badgeWidth, height: inset.height),
             font: SequenceLayout.badgeFont, color: accent, centered: true)
        let under = header.insetBy(dx: 12, dy: 0)
        draw(SequenceLayout.place(participant, me: me),
             in: CGRect(x: under.minX, y: header.maxY, width: under.width, height: body.maxY - header.maxY),
             font: SequenceLayout.placeFont, color: theme.ink3, centered: true)

        ctx.addPath(outline)
        let dashed = participant.role == .program || participant.role == .anyone || participant.role.isGap
            || participant.figure == .outside
        let edge = participant.role.isGap ? Self.gapInk : theme.line2
        ctx.setStrokeColor((me ? (participant.role == .actor ? blue : accent) : edge).cgColor)
        ctx.setLineDash(phase: 0, lengths: dashed ? [5, 4] : [])
        ctx.setLineWidth(me ? 2.5 : 1)
        ctx.strokePath()
        ctx.setLineDash(phase: 0, lengths: [])
    }

    private func drawStep(_ step: SequenceDiagram.Step, bar: CGRect, label: CGRect, call: CGFloat?, picked: Bool,
                          theme: GraphTheme, in ctx: CGContext) {
        let red = theme.accent(for: .flow)
        let path = CGPath(roundedRect: bar, cornerWidth: 2, cornerHeight: 2, transform: nil)
        ctx.addPath(path)
        ctx.setFillColor(red.withAlphaComponent(theme.isDark ? 0.30 : 0.16).cgColor)
        ctx.fillPath()
        ctx.addPath(path)
        ctx.setStrokeColor((picked ? theme.brass : red.withAlphaComponent(0.8)).cgColor)
        ctx.setLineWidth(picked ? 2 : 1)
        ctx.strokePath()
        if let call, step.node == nil, step.over < layout.columns.count {
            // The run out of the lifeline and back into the bar it opens.
            let from = layout.columns[step.over].center
            let runInk = self.ink(SequenceLayout.runs(step, over: step.over))
            ctx.setStrokeColor(runInk.cgColor)
            ctx.setFillColor(runInk.cgColor)
            ctx.setLineWidth(1.4)
            ctx.move(to: CGPoint(x: from + SequenceLayout.barWidth / 2, y: call))
            ctx.addLine(to: CGPoint(x: from + 30, y: call))
            ctx.addLine(to: CGPoint(x: from + 30, y: bar.minY + 4))
            ctx.addLine(to: CGPoint(x: bar.maxX + 6, y: bar.minY + 4))
            ctx.strokePath()
            let head = CGMutablePath()
            head.move(to: CGPoint(x: bar.maxX, y: bar.minY + 4))
            head.addLine(to: CGPoint(x: bar.maxX + 9, y: bar.minY - 0.5))
            head.addLine(to: CGPoint(x: bar.maxX + 9, y: bar.minY + 8.5))
            head.closeSubpath()
            ctx.addPath(head)
            ctx.fillPath()
            SequenceLayout.callText(step, ink: runInk, theme: theme)
                .draw(with: label, options: [.usesLineFragmentOrigin, .usesFontLeading])
            return
        }
        if step.node == nil {
            // The tab a reached flow's name sits on.
            let tab = CGPath(roundedRect: label, cornerWidth: 4, cornerHeight: 4, transform: nil)
            ctx.addPath(tab)
            ctx.setFillColor(theme.panel.cgColor)
            ctx.fillPath()
            ctx.addPath(tab)
            ctx.setStrokeColor(red.withAlphaComponent(0.6).cgColor)
            ctx.setLineWidth(1)
            ctx.strokePath()
            SequenceLayout.stepText(step, theme: theme)
                .draw(with: label.insetBy(dx: 5, dy: 2), options: [.usesLineFragmentOrigin, .usesFontLeading])
            return
        }
        SequenceLayout.stepText(step, theme: theme).draw(with: label, options: [.usesLineFragmentOrigin, .usesFontLeading])
    }

    /// A flow called on the lifeline it is called from: a thin frame in the call's colour, its tab the op, its head the
    /// flow, and — open — a rule under the head and the call's bar on the lifeline.
    private func drawCall(_ step: SequenceDiagram.Step, bar: CGRect, label: CGRect, frame: SequenceLayout.CallFrame,
                          picked: Bool, theme: GraphTheme, in ctx: CGContext) {
        let dark = theme.isDark
        let ink = self.ink(SequenceLayout.runs(step, over: step.over))
        let outline = CGPath(roundedRect: frame.rect, cornerWidth: 8, cornerHeight: 8, transform: nil)
        ctx.addPath(outline)
        ctx.setFillColor(ink.withAlphaComponent(dark ? 0.07 : 0.04).cgColor)
        ctx.fillPath()
        ctx.addPath(outline)
        ctx.setStrokeColor(ink.withAlphaComponent(0.65).cgColor)
        ctx.setLineWidth(1)
        ctx.strokePath()
        let tab = CGPath(roundedRect: frame.tab, cornerWidth: 6, cornerHeight: 6, transform: nil)
        ctx.addPath(tab)
        ctx.setFillColor(ink.withAlphaComponent(dark ? 0.30 : 0.16).cgColor)
        ctx.fillPath()
        draw(frame.op, in: frame.tab.insetBy(dx: 8, dy: 0), font: SequenceLayout.tabFont, color: ink, centered: true)
        // The words stand over the lifelines: their ground hides the dashes behind them.
        let ground = CGPath(roundedRect: label.insetBy(dx: -2, dy: 1), cornerWidth: 4, cornerHeight: 4, transform: nil)
        ctx.addPath(ground)
        ctx.setFillColor((theme.background.blended(withFraction: dark ? 0.12 : 0.06, of: ink) ?? theme.background).cgColor)
        ctx.fillPath()
        draw(SequenceLayout.callHead(step, theme: theme), in: CGRect(x: label.minX, y: label.minY,
                                                                 width: min(label.width, max(0, frame.rect.maxX - label.minX - 62)),
                                                                 height: label.height),
             font: SequenceLayout.headingFont)
        if let rule = frame.rule {
            ctx.setStrokeColor(ink.withAlphaComponent(0.65).cgColor)
            ctx.setLineWidth(1)
            ctx.move(to: CGPoint(x: rule.minX, y: rule.minY))
            ctx.addLine(to: CGPoint(x: rule.maxX, y: rule.minY))
            ctx.strokePath()
        }
        guard bar.height > 0 else { return }
        let red = theme.accent(for: .flow)
        let path = CGPath(roundedRect: bar, cornerWidth: 2, cornerHeight: 2, transform: nil)
        ctx.addPath(path)
        ctx.setFillColor(red.withAlphaComponent(dark ? 0.30 : 0.16).cgColor)
        ctx.fillPath()
        ctx.addPath(path)
        ctx.setStrokeColor((picked ? theme.brass : red.withAlphaComponent(0.8)).cgColor)
        ctx.setLineWidth(picked ? 2 : 1)
        ctx.strokePath()
    }

    private func drawMessage(_ message: SequenceDiagram.Message, y: CGFloat, label: CGRect, theme: GraphTheme, in ctx: CGContext) {
        guard message.from < layout.columns.count, message.to < layout.columns.count else { return }
        let from = layout.columns[message.from].center, to = layout.columns[message.to].center
        let ink = ink(message)
        SequenceLayout.words(message, ink: ink, theme: theme).draw(with: label, options: [.usesLineFragmentOrigin, .usesFontLeading])
        ctx.setStrokeColor(ink.cgColor)
        ctx.setFillColor(ink.cgColor)
        ctx.setLineWidth(1.4)
        ctx.setLineDash(phase: 0, lengths: message.reply || touchesGap(message) ? [5, 4] : [])
        let end: CGPoint, toward: CGFloat
        if message.from == message.to {
            ctx.move(to: CGPoint(x: from, y: y))
            ctx.addLine(to: CGPoint(x: from + 30, y: y))
            ctx.addLine(to: CGPoint(x: from + 30, y: y + 18))
            ctx.addLine(to: CGPoint(x: from + 6, y: y + 18))
            end = CGPoint(x: from, y: y + 18)
            toward = -1
        } else {
            let direction: CGFloat = to > from ? 1 : -1
            ctx.move(to: CGPoint(x: from, y: y))
            ctx.addLine(to: CGPoint(x: to - direction * 6, y: y))
            end = CGPoint(x: to, y: y)
            toward = direction
        }
        ctx.strokePath()
        ctx.setLineDash(phase: 0, lengths: [])
        // The head, filled for a request and open for an answer.
        let head = CGMutablePath()
        head.move(to: end)
        head.addLine(to: CGPoint(x: end.x - toward * 9, y: end.y - 4.5))
        head.addLine(to: CGPoint(x: end.x - toward * 9, y: end.y + 4.5))
        head.closeSubpath()
        ctx.addPath(head)
        if message.reply { ctx.strokePath() } else { ctx.fillPath() }
    }

    private func drawNote(_ note: SequenceDiagram.Note, rect: CGRect, picked: Bool, theme: GraphTheme, in ctx: CGContext) {
        let dark = theme.isDark
        let fill: NSColor, stroke: NSColor
        switch note.kind {
        case .plain: fill = theme.panel2; stroke = theme.line2
        case .key: fill = theme.panel2; stroke = lines.look(Line.key).ink.color(dark: dark)
        case .check:
            let amber = GraphTheme.nodeInk("eval").color(dark: dark)
            fill = amber.withAlphaComponent(dark ? 0.22 : 0.13); stroke = amber
        case .end, .mark:
            let red = GraphTheme.nodeInk("end").color(dark: dark)
            fill = note.kind == .mark ? theme.panel : red.withAlphaComponent(dark ? 0.22 : 0.12); stroke = red
        }
        let round = note.kind == .check ? rect.height / 2 : 6
        let path = CGPath(roundedRect: rect, cornerWidth: round, cornerHeight: round, transform: nil)
        ctx.addPath(path)
        ctx.setFillColor(fill.cgColor)
        ctx.fillPath()
        ctx.addPath(path)
        ctx.setStrokeColor((picked ? theme.brass : stroke).cgColor)
        ctx.setLineWidth(picked ? 2 : 1)
        ctx.strokePath()
        SequenceLayout.noteText(note, theme: theme)
            .draw(with: rect.insetBy(dx: 10, dy: 5), options: [.usesLineFragmentOrigin, .usesFontLeading])
    }

    private func drawFrame(_ block: SequenceDiagram.Block, rect: CGRect, tab: CGRect, sections: [(y: CGFloat, label: String)],
                           head: SequenceLayout.FrameHead?, picked: Bool, theme: GraphTheme, in ctx: CGContext) {
        let dark = theme.isDark
        let ink: NSColor
        switch block.kind {
        case .alt, .loop: ink = GraphTheme.nodeInk("eval").color(dark: dark)
        case .par: ink = GraphTheme.nodeInk("join").color(dark: dark)
        case .break: ink = GraphTheme.nodeInk("end").color(dark: dark)
        case .flow: ink = CardRenderer.actorBlue
        }
        let outline = CGPath(roundedRect: rect, cornerWidth: 8, cornerHeight: 8, transform: nil)
        ctx.addPath(outline)
        ctx.setFillColor(ink.withAlphaComponent(dark ? 0.07 : 0.04).cgColor)
        ctx.fillPath()
        ctx.addPath(outline)
        ctx.setStrokeColor((picked ? theme.brass : ink.withAlphaComponent(0.65)).cgColor)
        ctx.setLineWidth(picked ? 2 : 1)
        ctx.strokePath()

        let tabPath = CGPath(roundedRect: tab, cornerWidth: 6, cornerHeight: 6, transform: nil)
        ctx.addPath(tabPath)
        ctx.setFillColor(ink.withAlphaComponent(dark ? 0.30 : 0.16).cgColor)
        ctx.fillPath()
        let tabInk = block.kind == .flow ? theme.accent(for: .flow) : ink
        draw(block.kind.rawValue, in: tab.insetBy(dx: 8, dy: 0), font: SequenceLayout.tabFont, color: tabInk, centered: true)
        let words = SequenceLayout.frameHeadText(block, theme: theme)
        if words.length > 0 {
            let width = ceil(words.size().width) + 4
            draw(words, in: CGRect(x: tab.maxX + 6, y: tab.minY, width: min(width, max(0, rect.maxX - tab.maxX - 66)), height: tab.height),
                 font: block.kind == .flow ? SequenceLayout.headingFont : SequenceLayout.noteFont)
        }
        // An open frame's head is set off from what the frame holds by a line at the tab's lower edge.
        if let rule = head?.rule {
            ctx.setStrokeColor(ink.withAlphaComponent(0.65).cgColor)
            ctx.setLineWidth(1)
            ctx.move(to: CGPoint(x: rule.minX, y: rule.minY))
            ctx.addLine(to: CGPoint(x: rule.maxX, y: rule.minY))
            ctx.strokePath()
        }
        for section in sections {
            ctx.setStrokeColor(ink.withAlphaComponent(0.5).cgColor)
            ctx.setLineWidth(1)
            ctx.setLineDash(phase: 0, lengths: [5, 4])
            ctx.move(to: CGPoint(x: rect.minX, y: section.y))
            ctx.addLine(to: CGPoint(x: rect.maxX, y: section.y))
            ctx.strokePath()
            ctx.setLineDash(phase: 0, lengths: [])
            let said = block.kind == .par ? "and" : "else"
            draw(section.label.isEmpty ? said : "\(said) [\(section.label)]",
                 in: CGRect(x: rect.minX + 10, y: section.y + 2, width: rect.width - 20, height: 18),
                 font: SequenceLayout.noteFont, color: theme.ink2, centered: true)
        }
    }

    private func draw(_ text: String, in rect: CGRect, font: NSFont, color: NSColor, centered: Bool) {
        let style = NSMutableParagraphStyle()
        style.lineBreakMode = .byTruncatingTail
        let attributed = NSAttributedString(string: text, attributes: [.font: font, .foregroundColor: color, .paragraphStyle: style])
        var target = rect
        if centered {
            let height = ceil(font.ascender - font.descender + font.leading)
            target = CGRect(x: rect.minX, y: rect.midY - height / 2, width: rect.width, height: height)
        }
        attributed.draw(with: target, options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine])
    }

    /// Words with their own fonts and colours, centred on `rect`'s height as one line of `font`'s.
    private func draw(_ words: NSAttributedString, in rect: CGRect, font: NSFont) {
        let height = ceil(font.ascender - font.descender + font.leading)
        words.draw(with: CGRect(x: rect.minX, y: rect.midY - height / 2, width: rect.width, height: height),
                   options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine])
    }

    // MARK: Picking

    override func mouseDown(with event: NSEvent) {
        pressed(at: convert(event.locationInWindow, from: nil), clicks: event.clickCount)
    }

    /// A press at `point`: a chip does what it says; a gap's head picks where the gap is; a head
    /// double-clicked opens its actor's section or its card; an item picks where it came from, and a provided
    /// flow's frame head also folds or opens the frame; elsewhere, nothing is picked.
    func pressed(at point: CGPoint, clicks: Int) {
        if pressedHeads(at: point, clicks: clicks) { return }
        if let chip = layout.chips.last(where: { $0.rect.contains(point) }) {
            onPress(chip.chip)
            return
        }
        if let hit = layout.placed.last(where: { $0.hit.contains(point) && $0.origin != nil }) {
            onPick(hit.origin)
            // A provided flow's frame head: pressed, it folds an open frame and opens a folded one.
            if case .frame(_, _, _, _, let head?) = hit.shape {
                headPressed(id: head.flow, flow: head.flow, toggle: head.folded ? .openFrame(head.flow) : .foldFrame(head.flow),
                            clicks: clicks)
            } else if let frame = hit.frame {
                // A called flow's head: it folds and opens the call too.
                headPressed(id: frame.key, flow: frame.flow,
                            toggle: frame.folded ? .openCall(frame.key) : .foldCall(frame.key), clicks: clicks)
            }
        } else {
            onPick(nil)
        }
    }

    /// What the first press of a double-click on a head folded or opened, to put it back with the second.
    private var toggled: (id: String, chip: SequenceLayout.Chip)?

    /// A press on a frame's head row: once, it folds or opens what folds and opens (`toggle`); the second press
    /// of a double-click puts that back and opens the flow, so the frame stays as it was.
    private func headPressed(id: String, flow: String, toggle: SequenceLayout.Chip?, clicks: Int) {
        guard clicks >= 2 else {
            toggled = toggle.map { (id, $0) }
            if let toggle { onPress(toggle) }
            return
        }
        if toggle != nil {
            if let toggled, toggled.id == id, let undone = Self.undone(toggled.chip) { onPress(undone) } else if let toggle { onPress(toggle) }
        }
        toggled = nil
        onPress(.openFlow(flow))
    }

    /// The press that puts back what `chip` folded or opened.
    private static func undone(_ chip: SequenceLayout.Chip) -> SequenceLayout.Chip? {
        switch chip {
        case .openFrame(let flow): return .foldFrame(flow)
        case .foldFrame(let flow): return .openFrame(flow)
        case .openCall(let key): return .foldCall(key)
        case .foldCall(let key): return .openCall(key)
        case .lifeline, .openFlow: return nil
        }
    }

    /// A press on the heads alone — a head's or a bracket's chip, a head —, as the heads that stay at the top
    /// take it: whether it met one.
    @discardableResult
    func pressedHeads(at point: CGPoint, clicks: Int) -> Bool {
        if let chip = layout.chips.last(where: { SequenceLayout.isHeads($0.chip) && $0.rect.contains(point) }) {
            onPress(chip.chip)
            return true
        }
        for (index, column) in layout.columns.enumerated() where column.rect.contains(point) {
            guard index < diagram.participants.count else { break }
            let participant = diagram.participants[index]
            if participant.role.isGap, let origin = participant.origin {
                onPick(origin)
                return true
            }
            if clicks >= 2 {
                if participant.role == .actor, let section = participant.section {
                    onOpenSection(section)
                } else if let card = participant.cardId {
                    onOpen(card)
                }
            }
            return true
        }
        return false
    }

    /// Whether the frame of the provided flow `flow` is drawn as the one picked: its card is what is picked, and no
    /// node or row in it.
    func picksFrame(_ flow: String) -> Bool {
        guard let selected, selected.card == flow, selected.node == nil else { return false }
        return layout.frameHeads.contains { $0.flow == flow }
    }

    override var acceptsFirstResponder: Bool { true }
}

/// The heads of a sequence diagram kept at the top of its scroll view once scrolled past them
/// (`specboard://SpecBoard/Docs/Design:UI`「止まる頭」): the diagram's own heads — brackets with their parents' names,
/// the heads and their chips — drawn from its layout at the scroll's zoom and following it sideways, over a light
/// band with a line under it. A press on it is a press on the heads it shows, never on what lies under them; hidden
/// while the heads themselves are in view.
final class SequenceHeads: NSView, NSViewToolTipOwner {
    private weak var canvas: SequenceView?

    init(canvas: SequenceView) {
        self.canvas = canvas
        super.init(frame: .zero)
        isHidden = true
        // A head cut by the scroll's edge starts outside the band; a view does not clip by default, and the band
        // sits over the scroll view, which does not clip either: unclipped, it would be drawn over what lies beside.
        clipsToBounds = true
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
    }

    override var isFlipped: Bool { true }

    /// The part of the diagram the scroll view shows, in the diagram's coordinates, and its zoom: the clip view's
    /// bounds less its content insets, so the band never goes under a header laid over the scroll.
    private var seen: (rect: CGRect, scale: CGFloat)? {
        guard let scroll = superview as? NSScrollView, let canvas, scroll.documentView === canvas else { return nil }
        let scale = scroll.magnification
        let clip = scroll.contentView
        let insets = clip.contentInsets
        var rect = clip.bounds
        rect.origin.x += insets.left
        rect.origin.y += insets.top
        rect.size.width = max(0, rect.width - insets.left - insets.right)
        rect.size.height = max(0, rect.height - insets.top - insets.bottom)
        return scale > 0 ? (rect, scale) : nil
    }

    /// Laid over the top of what the scroll view shows, as tall as the heads at its zoom but never taller than
    /// what it shows, once the heads' top is scrolled out of view.
    func place() {
        guard let canvas, let scroll = superview as? NSScrollView, let (seen, _) = seen,
              !canvas.layout.columns.isEmpty, seen.minY > canvas.layout.headBand.minY + 0.5 else {
            if !isHidden { isHidden = true; removeAllToolTips() }
            return
        }
        let band = canvas.layout.headBand
        let rect = scroll.contentView.convert(CGRect(x: seen.minX, y: seen.minY, width: seen.width,
                                                     height: min(band.height, seen.height)), to: scroll)
        if frame != rect { frame = rect }
        isHidden = false
        needsDisplay = true
        removeAllToolTips()
        for chip in canvas.layout.chips where SequenceLayout.isHeads(chip.chip) {
            if let at = local(chip.rect)?.intersection(bounds), !at.isEmpty { addToolTip(at, owner: self, userData: nil) }
        }
    }

    /// A point here, in the diagram's coordinates.
    func diagramPoint(_ point: CGPoint) -> CGPoint? {
        guard let canvas, let (seen, scale) = seen else { return nil }
        return CGPoint(x: seen.minX + point.x / scale, y: canvas.layout.headBand.minY + point.y / scale)
    }

    /// A rect of the diagram's heads, where it is drawn here.
    func local(_ rect: CGRect) -> CGRect? {
        guard let canvas, let (seen, scale) = seen else { return nil }
        let band = canvas.layout.headBand
        return CGRect(x: (rect.minX - seen.minX) * scale, y: (rect.minY - band.minY) * scale,
                      width: rect.width * scale, height: rect.height * scale)
    }

    override func draw(_ dirtyRect: NSRect) {
        guard let canvas, let ctx = NSGraphicsContext.current?.cgContext, let (seen, scale) = seen else { return }
        let theme = canvas.theme
        let band = canvas.layout.headBand
        ctx.setFillColor((theme.background.blended(withFraction: theme.isDark ? 0.06 : 0.035, of: theme.ink) ?? theme.background).cgColor)
        ctx.fill(bounds)
        ctx.saveGState()
        ctx.scaleBy(x: scale, y: scale)
        ctx.translateBy(x: -seen.minX, y: -band.minY)
        canvas.drawHeads(visible: CGRect(x: seen.minX, y: band.minY, width: seen.width, height: band.height), in: ctx)
        ctx.restoreGState()
        ctx.setStrokeColor(theme.line.cgColor)
        ctx.setLineWidth(1)
        ctx.move(to: CGPoint(x: bounds.minX, y: bounds.maxY - 0.5))
        ctx.addLine(to: CGPoint(x: bounds.maxX, y: bounds.maxY - 0.5))
        ctx.strokePath()
    }

    func view(_ view: NSView, stringForToolTip tag: NSView.ToolTipTag, point: NSPoint, userData data: UnsafeMutableRawPointer?) -> String {
        guard let canvas, let at = diagramPoint(point) else { return "" }
        return canvas.layout.tip(at: at) ?? ""
    }

    override func mouseDown(with event: NSEvent) {
        pressed(at: convert(event.locationInWindow, from: nil), clicks: event.clickCount)
    }

    /// A press at `point` here: on the heads it shows, as the diagram takes a press on its heads.
    func pressed(at point: CGPoint, clicks: Int) {
        guard let canvas, let at = diagramPoint(point) else { return }
        canvas.pressedHeads(at: at, clicks: clicks)
    }
}
