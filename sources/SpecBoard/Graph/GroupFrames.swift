import AppKit
import QuartzCore

/// The pattern a group's fill is laid over with (`MapGroup.Fill`), in the diagram's points: drawn in the group's
/// layer, so it grows and shrinks with the zoom as the cards do. One path for the whole frame — as many pieces as
/// fit across and down, not a layer each — made again only when the frame's size or its fill changes.
nonisolated struct GroupPattern {
    var path: CGPath
    /// Bands filled (the zebras), or lines stroked.
    var filled: Bool
    var lineWidth: CGFloat = 0.8
    /// Dots are a dashed line with nothing between its round ends.
    var dash: [CGFloat]? = nil
    /// How strongly it is drawn, of the frame's line colour (`opacity(_:)`).
    var opacity: CGFloat

    /// The lines of a check this far apart: finer than the grid's cells, so the two are not mistaken.
    static let checkStep: CGFloat = 6
    /// The stripes, diagonals and dots this far apart.
    static let step: CGFloat = 8

    /// How strongly `fill` is drawn, of the frame's line colour: the check stronger than the stripes, so it is not
    /// taken for the diagram's own grid; the zebras' bands a fill a little stronger than the frame's own.
    static func opacity(_ fill: MapGroup.Fill) -> CGFloat {
        switch fill {
        case .none: 0
        case .check: 0.42
        case .stripes, .diagonal: 0.32
        case .dots: 0.45
        case .zebra, .zebraDiagonal: 0.12
        }
    }

    /// `fill` over a frame `size` big on a grid of `unit` cells; none for no pattern.
    static func make(_ fill: MapGroup.Fill, size: CGSize, unit: CGSize) -> GroupPattern? {
        let path = CGMutablePath()
        let (w, h) = (size.width, size.height)
        guard w > 0, h > 0 else { return nil }
        switch fill {
        case .none:
            return nil
        case .check:
            var x = checkStep
            while x < w { path.move(to: CGPoint(x: x, y: 0)); path.addLine(to: CGPoint(x: x, y: h)); x += checkStep }
            var y = checkStep
            while y < h { path.move(to: CGPoint(x: 0, y: y)); path.addLine(to: CGPoint(x: w, y: y)); y += checkStep }
            return GroupPattern(path: path, filled: false, lineWidth: 0.6, opacity: opacity(fill))
        case .stripes:
            var y = step
            while y < h { path.move(to: CGPoint(x: 0, y: y)); path.addLine(to: CGPoint(x: w, y: y)); y += step }
            return GroupPattern(path: path, filled: false, opacity: opacity(fill))
        case .diagonal:
            // 45°, `step` apart across the lines.
            let across = step * 2.squareRoot()
            var x = -h
            while x < w { path.move(to: CGPoint(x: x, y: 0)); path.addLine(to: CGPoint(x: x + h, y: h)); x += across }
            return GroupPattern(path: path, filled: false, opacity: opacity(fill))
        case .dots:
            var y = step / 2
            while y < h { path.move(to: CGPoint(x: step / 2, y: y)); path.addLine(to: CGPoint(x: w, y: y)); y += step }
            return GroupPattern(path: path, filled: false, lineWidth: 2, dash: [0, step], opacity: opacity(fill))
        case .zebra:
            // A band every other row of the grid, the frame's first row (where its name is) left plain.
            var y = unit.height
            while y < h { path.addRect(CGRect(x: 0, y: y, width: w, height: min(unit.height, h - y))); y += 2 * unit.height }
            return GroupPattern(path: path, filled: true, opacity: opacity(fill))
        case .zebraDiagonal:
            // 45° bands a row wide across, as far apart.
            let band = unit.height * 2.squareRoot()
            var x = -h
            while x < w {
                path.move(to: CGPoint(x: x, y: 0))
                path.addLine(to: CGPoint(x: x + band, y: 0))
                path.addLine(to: CGPoint(x: x + band + h, y: h))
                path.addLine(to: CGPoint(x: x + h, y: h))
                path.closeSubpath()
                x += 2 * band
            }
            return GroupPattern(path: path, filled: true, opacity: opacity(fill))
        }
    }
}

/// The layers one group is drawn with on the canvas, behind the lines and the cards: its faint fill, its pattern
/// clipped to the rounded frame, its dashed border and its name in the corner its `headerPlace` says — and, as it is
/// picked or takes what is dragged, the picked outline with the corner to size it by, or the blue glow.
@MainActor
final class GroupFrameLayers {
    /// Placed at the frame, in the diagram's points less its origin; the rest lie inside it.
    let root = CALayer()
    private let fill = CAShapeLayer()
    private let clip = CALayer()
    private let pattern = CAShapeLayer()
    private let border = CAShapeLayer()
    private let title = CATextLayer()
    private let outline = CAShapeLayer()
    private let grip = CAShapeLayer()
    private let glow = CAShapeLayer()
    /// What the pattern was last made for: made again only when this changes.
    private var made: (fill: MapGroup.Fill, size: CGSize)?

    static let radius: CGFloat = 10
    /// How far in from the frame's side its name is written.
    static let titleInset: CGFloat = 12

    init() {
        for layer in [root, fill, clip, pattern, border, title, outline, grip, glow] as [CALayer] {
            layer.anchorPoint = .zero
            layer.actions = ["position": NSNull(), "bounds": NSNull(), "path": NSNull(), "hidden": NSNull(),
                             "contents": NSNull(), "frame": NSNull(), "strokeColor": NSNull(), "fillColor": NSNull()]
        }
        clip.masksToBounds = true
        clip.cornerRadius = Self.radius
        clip.addSublayer(pattern)
        border.fillColor = nil
        border.lineWidth = 1.2
        border.lineDashPattern = [5, 4]
        title.truncationMode = .end
        title.contentsScale = 2
        outline.fillColor = nil
        outline.lineWidth = 2
        grip.fillColor = nil
        grip.lineWidth = 1
        glow.lineWidth = 2.2
        for layer in [fill, clip, border, title, glow, outline, grip] as [CALayer] { root.addSublayer(layer) }
    }

    /// The group laid on `frame` (in the layer's parent's points), its name in a strip `strip` tall.
    func lay(_ group: MapGroup, frame: CGRect, unit: CGSize, strip: CGFloat) {
        root.frame = frame
        let bounds = CGRect(origin: .zero, size: frame.size)
        let rounded = CGPath(roundedRect: bounds.insetBy(dx: 0.6, dy: 0.6), cornerWidth: Self.radius,
                             cornerHeight: Self.radius, transform: nil)
        for layer in [fill, border, outline, glow] {
            layer.frame = bounds
            layer.path = rounded
        }
        clip.frame = bounds
        if made.map({ $0.fill != group.fill || $0.size != frame.size }) ?? true {
            made = (group.fill, frame.size)
            let drawn = GroupPattern.make(group.fill, size: frame.size, unit: unit)
            pattern.frame = bounds
            pattern.path = drawn?.path
            pattern.isHidden = drawn == nil
            pattern.lineWidth = drawn?.lineWidth ?? 0
            pattern.lineCap = drawn?.dash == nil ? .butt : .round
            pattern.lineDashPattern = drawn?.dash?.map { NSNumber(value: Double($0)) }
        }
        let place = group.headerPlace
        title.isHidden = place == .hidden
        let top = place.onTop ? 0 : frame.height - strip
        title.frame = CGRect(x: Self.titleInset, y: top + max(0, (strip - 17) / 2),
                             width: max(0, frame.width - 2 * Self.titleInset), height: 17)
        title.alignmentMode = place.onRight ? .right : .left
        let marks = CGMutablePath()
        for step in [CGFloat(4), 8, 12] {
            marks.move(to: CGPoint(x: frame.width - 4, y: frame.height - 4 - step))
            marks.addLine(to: CGPoint(x: frame.width - 4 - step, y: frame.height - 4))
        }
        grip.frame = bounds
        grip.path = marks
    }

    /// Coloured in `theme`: its own colour, or the frames' grey; marked as picked — its corner shown where it may be
    /// sized — or as taking what is dragged.
    func style(_ group: MapGroup, theme: GraphTheme, picked: Bool, glowing: Bool, sizable: Bool = true) {
        let dark = theme.isDark
        let inks = GroupInks(group.color, fill: group.fill, theme: theme)
        border.strokeColor = inks.border.cgColor
        fill.fillColor = inks.fill.cgColor
        let filled = group.fill == .zebra || group.fill == .zebraDiagonal
        pattern.fillColor = filled ? inks.pattern.cgColor : nil
        pattern.strokeColor = filled ? nil : inks.pattern.cgColor
        title.string = NSAttributedString(string: group.title, attributes: [
            .font: Typeface.mono(12, .semibold),
            .foregroundColor: inks.title,
        ])
        outline.strokeColor = theme.brass.cgColor
        outline.isHidden = !picked
        grip.strokeColor = theme.ink3.withAlphaComponent(0.9).cgColor
        grip.isHidden = !(picked && sizable)
        let blue = EdgeInk.blue.color(dark: dark)
        glow.strokeColor = blue.cgColor
        glow.fillColor = blue.withAlphaComponent(0.10).cgColor
        glow.isHidden = !glowing
    }

    /// Taken off the diagram.
    func remove() { root.removeFromSuperlayer() }
}

/// The colours a group is drawn in, of its colour (none: the frames' grey) in `theme`: its dashed border, its faint
/// fill, its pattern and its name. One place for the canvas and the fill menu's swatches, so the two look alike.
struct GroupInks {
    var border: NSColor
    var fill: NSColor
    var pattern: NSColor
    var title: NSColor

    init(_ color: MapGroup.Color?, fill pattern: MapGroup.Fill, theme: GraphTheme) {
        let dark = theme.isDark
        let ink = color?.ink.color(dark: dark) ?? theme.ink3
        border = ink.withAlphaComponent(color == nil ? 0.8 : 0.9)
        fill = color == nil ? theme.panel.withAlphaComponent(dark ? 0.16 : 0.32) : ink.withAlphaComponent(dark ? 0.10 : 0.08)
        self.pattern = ink.withAlphaComponent(GroupPattern.opacity(pattern))
        title = color == nil ? theme.ink2 : ink
    }
}

/// A small picture of a fill, as the property panel's fill menu shows it beside each word: a frame drawn by the
/// canvas's own pattern (`GroupPattern`) and colours (`GroupInks`), shrunk to a swatch, on the diagram's ground.
enum GroupSwatch {
    static let size = CGSize(width: 16, height: 12)
    /// How much of the diagram a swatch shows, in its points: a few of the pattern's lines across.
    static let shrink: CGFloat = 0.4
    /// The cells a swatch's pattern is laid on, so a zebra shows its bands.
    static let unit = CGSize(width: 10, height: 7.5)

    /// `fill` in `color` (none: the frames' grey) on a dark or a light diagram: drawn now, at the screen's two
    /// pixels to a point.
    static func image(_ fill: MapGroup.Fill, color: MapGroup.Color?, dark: Bool) -> NSImage {
        let scale: CGFloat = 2
        let image = NSImage(size: size)
        if let ctx = CGContext(data: nil, width: Int(size.width * scale), height: Int(size.height * scale), bitsPerComponent: 8,
                               bytesPerRow: 0, space: CGColorSpace(name: CGColorSpace.sRGB)!,
                               bitmapInfo: CGImageAlphaInfo.premultipliedFirst.rawValue) {
            // Down from the top, as the canvas lays its patterns.
            ctx.translateBy(x: 0, y: size.height * scale)
            ctx.scaleBy(x: scale, y: -scale)
            draw(fill, color: color, dark: dark, in: CGRect(origin: .zero, size: size), ctx)
            if let made = ctx.makeImage() {
                let rep = NSBitmapImageRep(cgImage: made)
                rep.size = size
                image.addRepresentation(rep)
            }
        }
        image.accessibilityDescription = fill.rawValue
        return image
    }

    /// The swatch drawn in `rect` of `ctx`, which runs down from the top.
    static func draw(_ fill: MapGroup.Fill, color: MapGroup.Color?, dark: Bool, in rect: CGRect, _ ctx: CGContext) {
        let theme: GraphTheme = dark ? .dark : .light
        let inks = GroupInks(color, fill: fill, theme: theme)
        let frame = rect.insetBy(dx: 0.5, dy: 0.5)
        let rounded = CGPath(roundedRect: frame, cornerWidth: 2.5, cornerHeight: 2.5, transform: nil)
        ctx.saveGState()
        ctx.addPath(rounded)
        ctx.setFillColor(theme.background.cgColor)
        ctx.fillPath()
        ctx.addPath(rounded)
        ctx.setFillColor(inks.fill.cgColor)
        ctx.fillPath()
        ctx.addPath(rounded)
        ctx.clip()
        let mapSize = CGSize(width: frame.width / shrink, height: frame.height / shrink)
        if let pattern = GroupPattern.make(fill, size: mapSize, unit: unit) {
            ctx.translateBy(x: frame.minX, y: frame.minY)
            ctx.scaleBy(x: shrink, y: shrink)
            ctx.addPath(pattern.path)
            if pattern.filled {
                ctx.setFillColor(inks.pattern.cgColor)
                ctx.fillPath()
            } else {
                ctx.setStrokeColor(inks.pattern.cgColor)
                ctx.setLineWidth(pattern.lineWidth)
                if let dash = pattern.dash {
                    ctx.setLineCap(.round)
                    ctx.setLineDash(phase: 0, lengths: dash)
                }
                ctx.strokePath()
            }
        }
        ctx.restoreGState()
        ctx.addPath(rounded)
        ctx.setStrokeColor(inks.border.cgColor)
        ctx.setLineWidth(1)
        ctx.setLineDash(phase: 0, lengths: [2, 1.5])
        ctx.strokePath()
        ctx.setLineDash(phase: 0, lengths: [])
    }
}
