import AppKit
import SwiftUI

/// A flow's sequence diagram drawn the way the card map draws: who takes
/// part as the relation panel's cards along the top, a lifeline down from each, what
/// passes between them as lines in the relation panel's colours — solid where it is
/// asked, dashed where it answers — and a decision, a loop, ways that part or a stop as
/// a frame in the colour of the node that makes it. A message, a note or a frame picked
/// picks its node — or its row of a process node — for the property panel; a card
/// double-clicked opens it. Zoomed and moved as the card map is.
struct SequenceCanvas: NSViewRepresentable {
    var diagram: SequenceDiagram
    var detail: ProjectDetail
    /// What is picked in the property panel, to be shown picked here.
    var selected: SequenceDiagram.Origin?
    var controller: SequenceController
    var onPick: (SequenceDiagram.Origin?) -> Void
    var onOpen: (String) -> Void

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
        view.update(diagram: diagram, kinds: Dictionary(detail.schemas.map { ($0.id, $0.kind) }, uniquingKeysWith: { a, _ in a }),
                    lines: detail.lineKinds, selected: selected)
        scroll.backgroundColor = view.theme.background
        controller.scroll = scroll
    }
}

/// Lets the editor's "全体を表示" fit the sequence diagram as it fits the card map.
final class SequenceController {
    weak var scroll: NSScrollView?

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
    struct Column {
        var center: CGFloat
        var width: CGFloat
        var rect: CGRect { CGRect(x: center - width / 2, y: SequenceLayout.margin, width: width, height: SequenceLayout.cardHeight) }
    }

    enum Shape {
        /// A message: its line at `y`, its words in `label`.
        case message(SequenceDiagram.Message, y: CGFloat, label: CGRect)
        case note(SequenceDiagram.Note, rect: CGRect)
        /// A frame: its tab, and where a section after the first begins.
        case frame(SequenceDiagram.Block, rect: CGRect, tab: CGRect, sections: [(y: CGFloat, label: String)])
    }

    struct Placed {
        var shape: Shape
        /// What a click picks it by.
        var hit: CGRect
        var origin: SequenceDiagram.Origin?
    }

    static let margin: CGFloat = 28
    static let headerHeight: CGFloat = 26
    static let cardHeight: CGFloat = 46

    static let nameFont = NSFont.monospacedSystemFont(ofSize: 13, weight: .semibold)
    static let placeFont = NSFont.systemFont(ofSize: 11)
    static let badgeFont = NSFont.systemFont(ofSize: 10)
    static let opFont = NSFont.monospacedSystemFont(ofSize: 10.5, weight: .semibold)
    static let textFont = NSFont.systemFont(ofSize: 11.5)
    static let headingFont = NSFont.systemFont(ofSize: 11.5, weight: .semibold)
    static let noteFont = NSFont.systemFont(ofSize: 11)
    static let tabFont = NSFont.monospacedSystemFont(ofSize: 10.5, weight: .semibold)

    var columns: [Column] = []
    var placed: [Placed] = []
    var size: CGSize = .zero
    var lifelineBottom: CGFloat = 0

    /// A participant's word for what it is, on its card's header.
    static func badge(_ participant: SequenceDiagram.Participant, kinds: [String: SchemaKind]) -> String {
        switch participant.role {
        case .own, .flow: return "flow"
        case .store: return "store"
        case .record: return participant.cardId.flatMap { kinds[$0] }?.rawValue ?? "record"
        case .program: return "program"
        case .caller: return L10n[.Common.wordCaller]
        }
    }

    /// The line under a participant's name: where a flow sits, what anything else is.
    static func place(_ participant: SequenceDiagram.Participant) -> String {
        switch participant.role {
        case .own: return L10n[.Diagram.sequenceOwnFlow(participant.place ?? "")]
        case .flow: return participant.place ?? ""
        case .store: return L10n[.Diagram.sequenceStorePlace]
        case .record: return L10n[.Diagram.sequenceRecordPlace]
        case .program: return L10n[.Diagram.sequenceProgramPlace]
        case .caller: return L10n[.Diagram.sequenceCallerPlace]
        }
    }

    static func width(_ text: String, _ font: NSFont) -> CGFloat {
        ceil((text as NSString).size(withAttributes: [.font: font]).width)
    }

    static func height(_ text: NSAttributedString, width: CGFloat) -> CGFloat {
        ceil(text.boundingRect(with: CGSize(width: width, height: .greatestFiniteMagnitude),
                               options: [.usesLineFragmentOrigin, .usesFontLeading]).height)
    }

    /// A message's words: what it does in its line's colour, then what it carries.
    static func words(_ message: SequenceDiagram.Message, ink: NSColor, theme: GraphTheme) -> NSAttributedString {
        let out = NSMutableAttributedString()
        let style = NSMutableParagraphStyle()
        style.lineBreakMode = .byWordWrapping
        if let op = message.op {
            out.append(NSAttributedString(string: op + " ", attributes: [.font: opFont, .foregroundColor: ink, .paragraphStyle: style]))
            out.append(NSAttributedString(string: message.text, attributes: [.font: textFont, .foregroundColor: theme.ink2, .paragraphStyle: style]))
        } else {
            out.append(NSAttributedString(string: message.text, attributes: [
                .font: message.reply ? textFont : headingFont, .foregroundColor: message.reply ? theme.ink2 : theme.ink,
                .paragraphStyle: style,
            ]))
        }
        return out
    }

    static func noteText(_ note: SequenceDiagram.Note, theme: GraphTheme) -> NSAttributedString {
        let text = note.kind == .check ? "✓ " + note.text : note.text
        return NSAttributedString(string: text, attributes: [
            .font: note.kind == .heading ? headingFont : noteFont,
            .foregroundColor: note.kind == .plain ? theme.ink2 : theme.ink,
        ])
    }

    init() {}

    init(_ diagram: SequenceDiagram, kinds: [String: SchemaKind], theme: GraphTheme, ink: (SequenceDiagram.Message) -> NSColor) {
        let margin = Self.margin
        // The cards: as wide as their name and badge, or the line under it.
        let widths = diagram.participants.map { participant -> CGFloat in
            let head = Self.width(participant.name, Self.nameFont) + 24 + Self.width(Self.badge(participant, kinds: kinds), Self.badgeFont) + 10
            let under = Self.width(Self.place(participant), Self.placeFont) + 24
            return min(320, max(150, max(head, under)))
        }
        // Between two cards: room for the words of what passes between them.
        var gaps = zip(widths, widths.dropFirst()).map { ($0 + $1) / 2 + 48 }
        func messages(_ items: [SequenceDiagram.Item]) -> [SequenceDiagram.Message] {
            items.flatMap { item -> [SequenceDiagram.Message] in
                switch item {
                case .message(let message): return [message]
                case .note: return []
                case .block(let block): return block.sections.flatMap { messages($0.items) }
                }
            }
        }
        let all = messages(diagram.items)
        let natural = all.map { message in
            min(380, Self.width((message.op.map { $0 + " " } ?? "") + message.text, Self.textFont) + 32)
        }
        for (message, need) in zip(all, natural) where message.from != message.to && !gaps.isEmpty {
            let lo = min(message.from, message.to), hi = max(message.from, message.to)
            guard hi - 1 < gaps.count else { continue }
            let span = gaps[lo..<hi].reduce(0, +)
            if span < need { gaps[hi - 1] += need - span }
        }
        var cursor = margin + (widths.first ?? 150) / 2
        for (index, width) in widths.enumerated() {
            columns.append(Column(center: cursor, width: width))
            if index < gaps.count { cursor += gaps[index] }
        }
        var right = (columns.last.map { $0.center + $0.width / 2 } ?? margin) + margin

        // Down the page, item by item.
        var y = margin + Self.cardHeight + 22
        func center(_ index: Int) -> CGFloat { index < columns.count ? columns[index].center : margin }
        func involved(_ items: [SequenceDiagram.Item]) -> Set<Int> {
            var out: Set<Int> = []
            for item in items {
                switch item {
                case .message(let message): out.insert(message.from); out.insert(message.to)
                case .note(let note): out.insert(note.over)
                case .block(let block): for section in block.sections { out.formUnion(involved(section.items)) }
                }
            }
            return out
        }
        func lay(_ items: [SequenceDiagram.Item], depth: Int) {
            for item in items {
                switch item {
                case .message(let message):
                    let from = center(message.from), to = center(message.to)
                    let alone = message.from == message.to
                    let left: CGFloat = alone ? from + 40 : min(from, to) + 8
                    let room: CGFloat = alone ? 260 : max(120, abs(to - from) - 16)
                    let words = Self.words(message, ink: ink(message), theme: theme)
                    let height = Self.height(words, width: room)
                    let label = CGRect(x: left, y: y, width: room, height: height)
                    let line: CGFloat = y + height + 5
                    let low: CGFloat = alone ? line + 18 : line
                    let spread: CGFloat = max(abs(to - from), room + 40) + 12
                    let hit = CGRect(x: min(min(from, to), left) - 6, y: y - 3, width: spread, height: low - y + 9)
                    placed.append(Placed(shape: .message(message, y: line, label: label), hit: hit, origin: message.origin))
                    right = max(right, label.maxX + margin)
                    y = low + 16
                case .note(let note):
                    let text = Self.noteText(note, theme: theme)
                    let font = note.kind == .heading ? Self.headingFont : Self.noteFont
                    let wide: CGFloat = min(280, max(80, Self.width(text.string, font) + 22))
                    let height: CGFloat = Self.height(text, width: wide - 20) + 10
                    let rect = CGRect(x: center(note.over) - wide / 2, y: y, width: wide, height: height)
                    placed.append(Placed(shape: .note(note, rect: rect), hit: rect.insetBy(dx: -3, dy: -3), origin: note.origin))
                    right = max(right, rect.maxX + margin)
                    y += height + 12
                case .block(let block):
                    let reach = involved(block.sections.flatMap(\.items)).union([0])
                    let lo = reach.min() ?? 0, hi = reach.max() ?? 0
                    let inset = CGFloat(depth) * 8
                    let left = center(lo) - min(110, (lo < widths.count ? widths[lo] : 150) / 2) - 14 + inset
                    let rightEdge = center(hi) + min(110, (hi < widths.count ? widths[hi] : 150) / 2) + 14 - inset
                    let top = y
                    let index = placed.count
                    placed.append(Placed(shape: .note(.init(over: 0, text: "", kind: .plain), rect: .zero), hit: .zero, origin: nil))
                    let word = block.kind == .break ? "break" : block.kind.rawValue
                    let tab = CGRect(x: left, y: top, width: Self.width(word, Self.tabFont) + 16, height: 20)
                    y += 28
                    var sections: [(y: CGFloat, label: String)] = []
                    for (number, section) in block.sections.enumerated() {
                        if number > 0 {
                            sections.append((y - 4, section.label))
                            y += 22
                        }
                        lay(section.items, depth: depth + 1)
                    }
                    y += 4
                    let rect = CGRect(x: left, y: top, width: max(tab.width + 40, rightEdge - left), height: y - top)
                    let first = block.sections.first?.label ?? ""
                    let labelRect = CGRect(x: tab.maxX + 6, y: top, width: Self.width(first, Self.noteFont) + 4, height: 20)
                    placed[index] = Placed(shape: .frame(block, rect: rect, tab: tab, sections: sections),
                                           hit: tab.union(labelRect), origin: block.origin)
                    right = max(right, rect.maxX + margin)
                    y += 12
                }
            }
        }
        lay(diagram.items, depth: 0)
        lifelineBottom = y + 6
        size = CGSize(width: right, height: lifelineBottom + margin)
    }
}

/// The diagram itself, top-left origin like the card map's cards.
final class SequenceView: NSView {
    var onPick: (SequenceDiagram.Origin?) -> Void = { _ in }
    var onOpen: (String) -> Void = { _ in }
    private var diagram = SequenceDiagram()
    private var kinds: [String: SchemaKind] = [:]
    private var selected: SequenceDiagram.Origin?
    private var layout = SequenceLayout()
    /// The project's kinds of line: a message is drawn in the colour of the line the relation
    /// panel draws for the same thing.
    private var lines = LineKinds()

    override var isFlipped: Bool { true }

    var theme: GraphTheme { GraphTheme.current(for: effectiveAppearance) }

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

    func update(diagram: SequenceDiagram, kinds: [String: SchemaKind], lines: LineKinds, selected: SequenceDiagram.Origin?) {
        let changed = diagram != self.diagram || kinds != self.kinds || lines != self.lines
        self.diagram = diagram
        self.kinds = kinds
        self.lines = lines
        self.selected = selected
        if changed || layout.columns.count != diagram.participants.count {
            relayout()
        }
        needsDisplay = true
    }

    private func relayout() {
        layout = SequenceLayout(diagram, kinds: kinds, theme: theme, ink: ink)
        setFrameSize(layout.size)
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        relayout()
        needsDisplay = true
    }

    /// A message's line: in the colour of the line the relation panel draws for the
    /// same thing — what it runs, reads, writes or sends — and grey where it answers.
    func ink(_ message: SequenceDiagram.Message) -> NSColor {
        let dark = theme.isDark
        if message.reply { return theme.ink3 }
        // The fetch is worded in the diagram's language, so it is compared with that.
        let fetch = L10n.text(.Charts.opFetch, in: diagram.language)
        switch message.op {
        case "read"?: return lines.look(Line.reads).ink.color(dark: dark)
        case "write"?: return lines.look(Line.writes).ink.color(dark: dark)
        case "send"?, "receive"?: return lines.look(Line.peer).ink.color(dark: dark)
        case let op? where op == fetch: return lines.look(Line.peer).ink.color(dark: dark)
        default: return lines.look(Line.runs).ink.color(dark: dark)
        }
    }

    private func picked(_ origin: SequenceDiagram.Origin?) -> Bool {
        guard let origin, let selected, origin.node == selected.node else { return false }
        return selected.row == nil || origin.row == selected.row
    }

    // MARK: Drawing

    override func draw(_ dirtyRect: NSRect) {
        guard let ctx = NSGraphicsContext.current?.cgContext else { return }
        let theme = theme
        ctx.setFillColor(theme.background.cgColor)
        ctx.fill(bounds)

        // Lifelines, under everything.
        ctx.setStrokeColor(theme.line2.cgColor)
        ctx.setLineWidth(1)
        ctx.setLineDash(phase: 0, lengths: [4, 4])
        for column in layout.columns {
            ctx.move(to: CGPoint(x: column.center, y: SequenceLayout.margin + SequenceLayout.cardHeight))
            ctx.addLine(to: CGPoint(x: column.center, y: layout.lifelineBottom))
        }
        ctx.strokePath()
        ctx.setLineDash(phase: 0, lengths: [])

        for item in layout.placed {
            let on = picked(item.origin)
            switch item.shape {
            case .frame(let block, let rect, let tab, let sections):
                drawFrame(block, rect: rect, tab: tab, sections: sections, picked: on, theme: theme, in: ctx)
            case .message(let message, let y, let label):
                if on { highlight(item.hit, theme: theme, in: ctx) }
                drawMessage(message, y: y, label: label, theme: theme, in: ctx)
            case .note(let note, let rect):
                guard rect != .zero else { continue }
                drawNote(note, rect: rect, picked: on, theme: theme, in: ctx)
            }
        }

        for (index, column) in layout.columns.enumerated() where index < diagram.participants.count {
            drawCard(diagram.participants[index], rect: column.rect, theme: theme, in: ctx)
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

    private func drawCard(_ participant: SequenceDiagram.Participant, rect: CGRect, theme: GraphTheme, in ctx: CGContext) {
        let body = rect.insetBy(dx: 0.5, dy: 0.5)
        let outline = CGPath(roundedRect: body, cornerWidth: 8, cornerHeight: 8, transform: nil)
        ctx.addPath(outline)
        ctx.setFillColor(theme.panel.cgColor)
        ctx.fillPath()
        let fill: NSColor
        switch participant.role {
        case .own, .flow: fill = theme.headerFill(for: .flow)
        case .store: fill = theme.headerFill(for: .store)
        case .record: fill = theme.headerFill(for: participant.cardId.flatMap { kinds[$0] } ?? .entity)
        case .program, .caller: fill = theme.panel3
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
        let accent: NSColor
        switch participant.role {
        case .own, .flow: accent = theme.accent(for: .flow)
        case .store: accent = theme.accent(for: .store)
        case .record: accent = theme.accent(for: participant.cardId.flatMap { kinds[$0] } ?? .entity)
        case .program, .caller: accent = theme.ink3
        }
        let inset = header.insetBy(dx: 12, dy: 0)
        draw(participant.name, in: CGRect(x: inset.minX, y: inset.minY, width: inset.width - badgeWidth - 8, height: inset.height),
             font: SequenceLayout.nameFont, color: theme.ink, centered: true)
        draw(badge, in: CGRect(x: inset.maxX - badgeWidth, y: inset.minY, width: badgeWidth, height: inset.height),
             font: SequenceLayout.badgeFont, color: accent, centered: true)
        draw(SequenceLayout.place(participant),
             in: CGRect(x: inset.minX, y: header.maxY, width: inset.width, height: body.maxY - header.maxY),
             font: SequenceLayout.placeFont, color: theme.ink3, centered: true)

        ctx.addPath(outline)
        ctx.setStrokeColor(theme.line2.cgColor)
        ctx.setLineDash(phase: 0, lengths: participant.role == .program || participant.role == .caller ? [5, 4] : [])
        ctx.setLineWidth(1)
        ctx.strokePath()
        ctx.setLineDash(phase: 0, lengths: [])
    }

    private func drawMessage(_ message: SequenceDiagram.Message, y: CGFloat, label: CGRect, theme: GraphTheme, in ctx: CGContext) {
        guard message.from < layout.columns.count, message.to < layout.columns.count else { return }
        let from = layout.columns[message.from].center, to = layout.columns[message.to].center
        let ink = ink(message)
        SequenceLayout.words(message, ink: ink, theme: theme).draw(with: label, options: [.usesLineFragmentOrigin, .usesFontLeading])
        ctx.setStrokeColor(ink.cgColor)
        ctx.setFillColor(ink.cgColor)
        ctx.setLineWidth(1.4)
        ctx.setLineDash(phase: 0, lengths: message.reply ? [5, 4] : [])
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
        case .heading: fill = theme.panel3; stroke = theme.line2
        case .key: fill = theme.panel2; stroke = lines.look(Line.key).ink.color(dark: dark)
        case .check:
            let amber = GraphTheme.nodeInk("eval").color(dark: dark)
            fill = amber.withAlphaComponent(dark ? 0.22 : 0.13); stroke = amber
        case .end:
            let red = GraphTheme.nodeInk("end").color(dark: dark)
            fill = red.withAlphaComponent(dark ? 0.22 : 0.12); stroke = red
        }
        let path = CGPath(roundedRect: rect, cornerWidth: note.kind == .check ? rect.height / 2 : 6,
                          cornerHeight: note.kind == .check ? rect.height / 2 : 6, transform: nil)
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
                           picked: Bool, theme: GraphTheme, in ctx: CGContext) {
        let dark = theme.isDark
        let ink: NSColor
        switch block.kind {
        case .alt, .loop: ink = GraphTheme.nodeInk("eval").color(dark: dark)
        case .par: ink = GraphTheme.nodeInk("join").color(dark: dark)
        case .break: ink = GraphTheme.nodeInk("end").color(dark: dark)
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
        let word = block.kind == .break ? "break" : block.kind.rawValue
        draw(word, in: tab.insetBy(dx: 8, dy: 0), font: SequenceLayout.tabFont, color: ink, centered: true)
        if let first = block.sections.first?.label, !first.isEmpty {
            draw("[\(first)]", in: CGRect(x: tab.maxX + 6, y: tab.minY, width: rect.maxX - tab.maxX - 12, height: tab.height),
                 font: SequenceLayout.noteFont, color: theme.ink2, centered: true)
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

    // MARK: Picking

    override func mouseDown(with event: NSEvent) {
        let point = convert(event.locationInWindow, from: nil)
        for (index, column) in layout.columns.enumerated() where column.rect.contains(point) {
            guard index < diagram.participants.count else { break }
            if event.clickCount >= 2, let card = diagram.participants[index].cardId { onOpen(card) }
            return
        }
        if let hit = layout.placed.last(where: { $0.hit.contains(point) && $0.origin != nil }) {
            onPick(hit.origin)
        } else {
            onPick(nil)
        }
    }

    override var acceptsFirstResponder: Bool { true }
}
