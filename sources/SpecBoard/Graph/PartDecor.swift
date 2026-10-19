import AppKit

/// What the map lays over a ui part's face (`specboard://SpecBoard/Docs/Design:CardKinds`「UI のセクションとはめ込み」):
/// its header shrunk to a one-line tag at the corner `headerPlace` names, its note as the mock's sticky note
/// at the corner `notePlace` names, and its review dot at the top right. The face is the page the part
/// makes and nothing more, so what tells the parts apart on the map is drawn here, over it, and switched
/// on and off without the face being taken again.
enum PartDecor {

    /// A corner of a part's box.
    enum Corner: String, CaseIterable {
        case topLeft, topRight, bottomLeft, bottomRight

        var top: Bool { self == .topLeft || self == .topRight }
        var left: Bool { self == .topLeft || self == .bottomLeft }
    }

    /// Where a field's place puts its layer: its corner, `otherwise` when it says none or one not known,
    /// nil when it says `hidden`.
    static func corner(_ value: String?, otherwise: Corner) -> Corner? {
        guard let value, !value.isEmpty else { return otherwise }
        if value == "hidden" { return nil }
        return Corner(rawValue: value) ?? otherwise
    }

    /// Where each layer lies, in the part's box (its top left at 0, 0): the tag and the note may reach past
    /// the box, which `bounds` takes in.
    struct Layout: Equatable {
        var tag: CGRect?
        var note: CGRect?
        var dot: CGRect?
        var bounds: CGRect
    }

    /// The tag's size: about the size of a row's small print, and the room round it.
    static let tagFont: CGFloat = 11
    static let tagPadding = CGSize(width: 6, height: 2)
    /// The sticky note as the mock draws it (`.sb-note`): the widest it grows, the room inside it, the
    /// height of a line of it.
    static let noteWidest: CGFloat = 240
    static let notePadding = CGSize(width: 6, height: 4)
    static let noteFont: CGFloat = 11
    static let noteLine: CGFloat = 11 * 1.35
    /// The room between the tag and a note at the same corner.
    static let gap: CGFloat = 2

    /// The tag's words, `name | label | kind`: drawn on the dark map's colours whatever the appearance,
    /// so it reads the same over any face.
    static func tagText(_ part: SchemaNode) -> NSAttributedString {
        let theme = GraphTheme.dark
        let style = NSMutableParagraphStyle()
        style.lineBreakMode = .byTruncatingTail
        let bar = NSAttributedString(string: " | ", attributes: [
            .font: NSFont.systemFont(ofSize: tagFont), .foregroundColor: theme.ink3, .paragraphStyle: style,
        ])
        let out = NSMutableAttributedString(string: part.name, attributes: [
            .font: NSFont.monospacedSystemFont(ofSize: tagFont, weight: .bold), .foregroundColor: theme.ink,
            .paragraphStyle: style,
        ])
        if !part.label.isEmpty {
            out.append(bar)
            out.append(NSAttributedString(string: part.label, attributes: [
                .font: NSFont.systemFont(ofSize: tagFont), .foregroundColor: theme.ink2, .paragraphStyle: style,
            ]))
        }
        out.append(bar)
        out.append(NSAttributedString(string: CardRenderer.customKindWord(part) ?? part.kind.rawValue, attributes: [
            .font: NSFont.systemFont(ofSize: tagFont, weight: .medium), .foregroundColor: theme.accent(for: part),
            .paragraphStyle: style,
        ]))
        return out
    }

    /// The note's words as the mock writes them, wrapped within the note.
    static func noteText(_ note: String) -> NSAttributedString {
        let style = NSMutableParagraphStyle()
        style.minimumLineHeight = noteLine
        style.maximumLineHeight = noteLine
        style.lineBreakMode = .byWordWrapping
        return NSAttributedString(string: note, attributes: [
            .font: NSFont.systemFont(ofSize: noteFont),
            .foregroundColor: NSColor(srgbRed: 0x3D / 255, green: 0x3A / 255, blue: 0x1E / 255, alpha: 1),
            .paragraphStyle: style,
        ])
    }

    /// The part's note, when it has one to show.
    static func note(of part: SchemaNode) -> String? {
        guard let note = part.customFields["note"], !note.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            return nil
        }
        return note
    }

    /// Where the tag, the note and the dot of `part` lie in a box `size` big; `tags` and `notes`: whether
    /// they are shown at all (the bar's switches).
    static func layout(_ part: SchemaNode, size: CGSize, tags: Bool, notes: Bool) -> Layout {
        let box = CGRect(origin: .zero, size: size)
        var out = Layout(bounds: box)
        let tagAt = tags ? corner(part.customFields["headerPlace"], otherwise: .topLeft) : nil
        if let tagAt {
            let text = tagText(part).size()
            let width = min(ceil(text.width) + 2 * tagPadding.width, size.width)
            let height = min(ceil(text.height) + 2 * tagPadding.height, size.height)
            let rect = CGRect(x: tagAt.left ? 0 : size.width - width, y: tagAt.top ? 0 : size.height - height,
                              width: width, height: height)
            if width > 0, height > 0 { out.tag = rect }
        }
        if notes, let note = note(of: part), let noteAt = corner(part.customFields["notePlace"], otherwise: .topRight) {
            let inside = noteText(note).boundingRect(
                with: CGSize(width: noteWidest - 2 * notePadding.width - 2, height: .greatestFiniteMagnitude),
                options: [.usesLineFragmentOrigin, .usesFontLeading])
            let width = min(noteWidest, ceil(inside.width) + 2 * notePadding.width + 2)
            let height = ceil(inside.height) + 2 * notePadding.height + 2
            var y = noteAt.top ? 0 : size.height - height
            // The tag at the same corner keeps the corner; the note goes next to it, inward.
            if tagAt == noteAt, let tag = out.tag {
                y = noteAt.top ? tag.maxY + gap : tag.minY - gap - height
            }
            out.note = CGRect(x: noteAt.left ? 0 : size.width - width, y: y, width: width, height: height)
        }
        if part.showsReviewDot { out.dot = CardRenderer.reviewDotRect(in: size) }
        for rect in [out.tag, out.note, out.dot].compactMap({ $0 }) { out.bounds = out.bounds.union(rect) }
        return out
    }

    /// The layers drawn, `layout.bounds` big at `scale`, its top left at the bounds' own; nil when there is
    /// nothing to draw.
    static func image(_ part: SchemaNode, _ layout: Layout, scale: CGFloat) -> CGImage? {
        guard layout.tag != nil || layout.note != nil || layout.dot != nil else { return nil }
        let bounds = layout.bounds
        let pixelWidth = Int((bounds.width * scale).rounded(.up)), pixelHeight = Int((bounds.height * scale).rounded(.up))
        guard pixelWidth > 0, pixelHeight > 0,
              let ctx = CGContext(data: nil, width: pixelWidth, height: pixelHeight, bitsPerComponent: 8, bytesPerRow: 0,
                                  space: CGColorSpace(name: CGColorSpace.sRGB)!,
                                  bitmapInfo: CGImageAlphaInfo.premultipliedFirst.rawValue) else { return nil }
        ctx.scaleBy(x: scale, y: scale)
        ctx.translateBy(x: 0, y: bounds.height)
        ctx.scaleBy(x: 1, y: -1)
        ctx.translateBy(x: -bounds.minX, y: -bounds.minY)
        let previous = NSGraphicsContext.current
        NSGraphicsContext.current = NSGraphicsContext(cgContext: ctx, flipped: true)
        defer { NSGraphicsContext.current = previous }

        if let tag = layout.tag {
            ctx.addPath(CGPath(roundedRect: tag, cornerWidth: 3, cornerHeight: 3, transform: nil))
            ctx.setFillColor(GraphTheme.dark.background.withAlphaComponent(0.8).cgColor)
            ctx.fillPath()
            let text = tagText(part)
            let height = ceil(text.size().height)
            text.draw(with: CGRect(x: tag.minX + tagPadding.width, y: tag.midY - height / 2,
                                   width: max(0, tag.width - 2 * tagPadding.width), height: height),
                      options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine])
        }
        if let rect = layout.note, let note = note(of: part) {
            let body = rect.insetBy(dx: 0.5, dy: 0.5)
            let path = CGPath(roundedRect: body, cornerWidth: 2, cornerHeight: 2, transform: nil)
            ctx.saveGState()
            // The shadow falls below the note, as the page's does: the context is flipped, its shadow not.
            ctx.setShadow(offset: CGSize(width: 0, height: -1), blur: 3, color: NSColor(white: 0, alpha: 0.25).cgColor)
            ctx.addPath(path)
            ctx.setFillColor(NSColor(srgbRed: 1, green: 0xF4 / 255, blue: 0xB0 / 255, alpha: 1).cgColor)
            ctx.fillPath()
            ctx.restoreGState()
            ctx.addPath(path)
            ctx.setStrokeColor(NSColor(srgbRed: 0xE3 / 255, green: 0xC9 / 255, blue: 0x4F / 255, alpha: 1).cgColor)
            ctx.setLineWidth(1)
            ctx.strokePath()
            noteText(note).draw(with: rect.insetBy(dx: notePadding.width + 1, dy: notePadding.height + 1),
                                options: [.usesLineFragmentOrigin, .usesFontLeading])
        }
        // Last: over everything else of the part's.
        if let dot = layout.dot { CardRenderer.drawReviewDot(dot, in: ctx) }
        return ctx.makeImage()
    }
}
