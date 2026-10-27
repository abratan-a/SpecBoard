import AppKit
import SwiftUI

/// A file's text as code is read: in one monospaced face, coloured by the runs the core named
/// (`FileText.spans`, `GraphTheme.codePalette`), each line's number in a gutter on its left, read only but
/// selectable, never wrapped; a function or type asked for (`FileText.focus`) scrolled to and its lines
/// tinted with the accent.
struct CodeTextView: NSViewRepresentable {
    var read: FileText

    func makeNSView(context: Context) -> NSScrollView {
        CodeText.scrollView()
    }

    func updateNSView(_ scroll: NSScrollView, context: Context) {
        guard let view = scroll.documentView as? CodeTextNSView else { return }
        view.show(read)
    }
}

/// What a file's text is drawn with, worked out apart from the view so it can be tested.
enum CodeText {
    static let font = Typeface.mono(12, .regular)
    static let keywordFont = Typeface.mono(12, .semibold)

    /// Where each line starts, in UTF-16 offsets: the first at 0, then one after each line feed.
    static func lineStarts(_ text: NSString) -> [Int] {
        var out = [0]
        var at = 0
        while at < text.length {
            let found = text.range(of: "\n", options: .literal, range: NSRange(location: at, length: text.length - at))
            guard found.location != NSNotFound else { break }
            out.append(found.location + 1)
            at = found.location + 1
        }
        return out
    }

    /// The line an offset is on, from 1.
    static func line(at offset: Int, starts: [Int]) -> Int {
        var low = 0, high = starts.count - 1
        while low < high {
            let middle = (low + high + 1) / 2
            if starts[middle] <= offset { low = middle } else { high = middle - 1 }
        }
        return low + 1
    }

    /// Lines `from` to `to` (from 1, both in) as a range of the text; nil where they are not in it.
    static func range(lines from: Int, _ to: Int, starts: [Int], length: Int) -> NSRange? {
        guard from >= 1, from <= starts.count, to >= from else { return nil }
        let start = starts[from - 1]
        let end = to < starts.count ? starts[to] : length
        return NSRange(location: start, length: max(0, end - start))
    }

    /// The text with its colours on: each run in its class's colour (none for a class drawn as the text
    /// is), a keyword a shade heavier. Runs past the text's end are left out.
    static func attributed(_ read: FileText) -> NSAttributedString {
        let out = NSMutableAttributedString(string: read.text, attributes: [
            .font: font, .foregroundColor: NSColor.textColor,
        ])
        let length = out.length
        let inks = read.classes.map { GraphTheme.codeInk($0) }
        let keyword = read.classes.firstIndex(of: "keyword")
        out.beginEditing()
        var index = 0
        while index + 2 < read.spans.count {
            let from = read.spans[index], count = read.spans[index + 1], kind = read.spans[index + 2]
            index += 3
            guard from >= 0, count > 0, from + count <= length, inks.indices.contains(kind), let ink = inks[kind] else { continue }
            let range = NSRange(location: from, length: count)
            out.addAttribute(.foregroundColor, value: ink, range: range)
            if kind == keyword { out.addAttribute(.font, value: keywordFont, range: range) }
        }
        out.endEditing()
        return out
    }

    /// A scroll view holding the text view and its gutter.
    static func scrollView() -> NSScrollView {
        let scroll = NSScrollView()
        scroll.hasVerticalScroller = true
        scroll.hasHorizontalScroller = true
        scroll.autohidesScrollers = true
        scroll.borderType = .noBorder
        scroll.drawsBackground = true
        scroll.backgroundColor = .textBackgroundColor
        let view = CodeTextNSView(usingTextLayoutManager: false)
        view.isEditable = false
        view.isSelectable = true
        view.isRichText = false
        view.drawsBackground = true
        view.backgroundColor = .textBackgroundColor
        view.textContainerInset = NSSize(width: 6, height: 8)
        view.isHorizontallyResizable = true
        view.isVerticallyResizable = true
        view.minSize = NSSize(width: 0, height: 0)
        view.maxSize = NSSize(width: CGFloat.greatestFiniteMagnitude, height: CGFloat.greatestFiniteMagnitude)
        view.autoresizingMask = [.width]
        view.textContainer?.widthTracksTextView = false
        view.textContainer?.containerSize = NSSize(width: CGFloat.greatestFiniteMagnitude, height: CGFloat.greatestFiniteMagnitude)
        scroll.documentView = view
        let ruler = LineNumberRuler(textView: view, scrollView: scroll)
        scroll.verticalRulerView = ruler
        scroll.hasVerticalRuler = true
        scroll.rulersVisible = true
        scroll.contentView.postsBoundsChangedNotifications = true
        NotificationCenter.default.addObserver(ruler, selector: #selector(LineNumberRuler.moved),
                                               name: NSView.boundsDidChangeNotification, object: scroll.contentView)
        return scroll
    }
}

/// The text view of a file: what it shows set once for each read (`show`), the lines of the function or
/// type asked for tinted across the whole width behind the text.
final class CodeTextNSView: NSTextView {
    /// The read on screen, so that drawing again does not set the text again.
    private(set) var shown: FileText?
    /// Where each line starts (`CodeText.lineStarts`), read by the gutter.
    private(set) var starts: [Int] = [0]
    /// The lines tinted, as a range of the text.
    private(set) var tinted: NSRange?

    func show(_ read: FileText) {
        guard read != shown else { return }
        let sameText = read.text == shown?.text && read.spans == shown?.spans
        shown = read
        if !sameText {
            starts = CodeText.lineStarts(read.text as NSString)
            textStorage?.setAttributedString(CodeText.attributed(read))
            sizeToFit()
        }
        tinted = read.focus.flatMap { CodeText.range(lines: $0.from, $0.to, starts: starts, length: (read.text as NSString).length) }
        (enclosingScrollView?.verticalRulerView as? LineNumberRuler)?.fit(lines: starts.count)
        needsDisplay = true
        if let focus = read.focus { scroll(toLine: focus.line) }
    }

    /// The line at the top of the view, a few lines of what comes before it above: once laid out.
    func scroll(toLine line: Int) {
        guard let range = CodeText.range(lines: line, line, starts: starts, length: (string as NSString).length),
              let layout = layoutManager, let container = textContainer else { return }
        layout.ensureLayout(for: container)
        let glyphs = layout.glyphRange(forCharacterRange: NSRange(location: range.location, length: 0), actualCharacterRange: nil)
        let rect = layout.lineFragmentRect(forGlyphAt: min(glyphs.location, max(0, layout.numberOfGlyphs - 1)), effectiveRange: nil)
        let top = max(0, rect.minY + textContainerOrigin.y - rect.height * 3)
        scroll(NSPoint(x: 0, y: top))
        enclosingScrollView?.reflectScrolledClipView(enclosingScrollView!.contentView)
    }

    /// The rectangle the tinted lines take, across the whole view.
    var tintRect: NSRect? {
        guard let tinted, let layout = layoutManager, let container = textContainer, tinted.length > 0 else { return nil }
        let glyphs = layout.glyphRange(forCharacterRange: tinted, actualCharacterRange: nil)
        let box = layout.boundingRect(forGlyphRange: glyphs, in: container)
        return NSRect(x: 0, y: box.minY + textContainerOrigin.y, width: bounds.width, height: box.height)
    }

    override func drawBackground(in rect: NSRect) {
        super.drawBackground(in: rect)
        guard let band = tintRect, band.intersects(rect) else { return }
        NSColor.controlAccentColor.withAlphaComponent(0.12).setFill()
        band.intersection(rect).fill()
    }
}

/// The gutter of a file's text: each line's number beside it, right-aligned, faint; as wide as the
/// largest number needs.
final class LineNumberRuler: NSRulerView {
    private weak var textView: CodeTextNSView?
    static let font = Typeface.digits(10.5, .regular)

    init(textView: CodeTextNSView, scrollView: NSScrollView) {
        self.textView = textView
        super.init(scrollView: scrollView, orientation: .verticalRuler)
        clientView = textView
        fit(lines: 1)
    }

    required init(coder: NSCoder) {
        fatalError("not made from a coder")
    }

    /// As wide as the number of the last line needs, and a margin.
    func fit(lines: Int) {
        let digits = max(3, String(lines).count)
        let width = (String(repeating: "8", count: digits) as NSString).size(withAttributes: [.font: Self.font]).width
        ruleThickness = ceil(width) + 16
    }

    @objc func moved(_ note: Notification) { needsDisplay = true }

    override func drawHashMarksAndLabels(in rect: NSRect) {
        NSColor.textBackgroundColor.setFill()
        bounds.fill()
        NSColor.separatorColor.setFill()
        NSRect(x: bounds.maxX - 1, y: rect.minY, width: 1, height: rect.height).fill()
        guard let view = textView, let layout = view.layoutManager, let container = view.textContainer else { return }
        let text = view.string as NSString
        let starts = view.starts
        let visible = view.visibleRect
        let glyphs = layout.glyphRange(forBoundingRect: visible, in: container)
        let chars = layout.characterRange(forGlyphRange: glyphs, actualGlyphRange: nil)
        let attributes: [NSAttributedString.Key: Any] = [.font: Self.font, .foregroundColor: NSColor.tertiaryLabelColor]
        var index = CodeText.line(at: chars.location, starts: starts) - 1
        while index < starts.count, starts[index] <= NSMaxRange(chars) {
            let start = starts[index]
            let lineRect: NSRect
            if start >= text.length {
                // The empty line after a last line feed: drawn where the text would go on.
                guard start == text.length, text.length > 0 else { break }
                lineRect = layout.extraLineFragmentRect
            } else {
                lineRect = layout.lineFragmentRect(forGlyphAt: layout.glyphIndexForCharacter(at: start), effectiveRange: nil)
            }
            let y = convert(NSPoint(x: 0, y: lineRect.minY + view.textContainerOrigin.y), from: view).y
            let number = "\(index + 1)" as NSString
            let size = number.size(withAttributes: attributes)
            number.draw(at: NSPoint(x: ruleThickness - size.width - 8, y: y + (lineRect.height - size.height) / 2),
                        withAttributes: attributes)
            index += 1
        }
    }
}
