import AppKit

/// A whiteboard's sketch as it is drawn (`specboard://SpecBoard/Docs/Design:CardKinds`「ホワイトボード（whiteboard）」): its
/// shape, its text and its colours, the kind's defaults where nothing is written (a blue rounded box).
nonisolated struct Sketch: Equatable, Sendable {
    var shape: String
    var text: String
    /// None for a sketch filled with nothing.
    var fill: EdgeInk?
    var stroke: EdgeInk

    init(shape: String, text: String, fill: EdgeInk?, stroke: EdgeInk) {
        self.shape = shape
        self.text = text
        self.fill = fill
        self.stroke = stroke
    }

    init(_ card: SchemaNode) {
        let said = { (name: String) in card.customFields[name].flatMap { $0.isEmpty ? nil : $0 } }
        shape = said("shape").flatMap { SketchShape.all.contains($0) ? $0 : nil } ?? "rounded"
        text = said("text") ?? ""
        let fill = said("fill") ?? "blue"
        self.fill = fill == "none" ? nil : EdgeInk(rawValue: fill) ?? .blue
        stroke = said("stroke").flatMap(EdgeInk.init(rawValue:)) ?? .blue
    }
}

/// The outlines a sketch is drawn as, each fitted to its box.
nonisolated enum SketchShape {
    static let all = ["rect", "rounded", "ellipse", "diamond", "cylinder", "text"]

    /// The outline of `shape` in `rect`; a box with nothing round it (`text`) its box, which is filled with
    /// nothing and stroked with nothing.
    static func path(for shape: String, in rect: CGRect) -> CGPath {
        switch shape {
        case "rect", "text":
            return CGPath(rect: rect, transform: nil)
        case "ellipse":
            return CGPath(ellipseIn: rect, transform: nil)
        case "diamond":
            let path = CGMutablePath()
            path.move(to: CGPoint(x: rect.midX, y: rect.minY))
            path.addLine(to: CGPoint(x: rect.maxX, y: rect.midY))
            path.addLine(to: CGPoint(x: rect.midX, y: rect.maxY))
            path.addLine(to: CGPoint(x: rect.minX, y: rect.midY))
            path.closeSubpath()
            return path
        case "cylinder":
            // Its body, and its top as an ellipse a fifth of it at the most (`top`).
            let lid = min(rect.height * 0.2, 14)
            let path = CGMutablePath()
            path.move(to: CGPoint(x: rect.minX, y: rect.minY + lid / 2))
            path.addLine(to: CGPoint(x: rect.minX, y: rect.maxY - lid / 2))
            path.addCurve(to: CGPoint(x: rect.maxX, y: rect.maxY - lid / 2),
                          control1: CGPoint(x: rect.minX, y: rect.maxY + lid / 6),
                          control2: CGPoint(x: rect.maxX, y: rect.maxY + lid / 6))
            path.addLine(to: CGPoint(x: rect.maxX, y: rect.minY + lid / 2))
            path.addEllipse(in: CGRect(x: rect.minX, y: rect.minY, width: rect.width, height: lid))
            return path
        default:
            let radius = min(12, rect.height / 3)
            return CGPath(roundedRect: rect, cornerWidth: radius, cornerHeight: radius, transform: nil)
        }
    }

    /// Where its text goes inside it: the middle a diamond and an ellipse leave, below a cylinder's top.
    static func textArea(for shape: String, in rect: CGRect) -> CGRect {
        switch shape {
        case "diamond": rect.insetBy(dx: rect.width * 0.2, dy: rect.height * 0.2)
        case "ellipse": rect.insetBy(dx: rect.width * 0.12, dy: rect.height * 0.12)
        case "cylinder": CGRect(x: rect.minX + 8, y: rect.minY + min(rect.height * 0.2, 14),
                                width: rect.width - 16, height: rect.height - min(rect.height * 0.2, 14))
        default: rect.insetBy(dx: 10, dy: 6)
        }
    }
}

/// A whiteboard's arrow as its own fields draw it: its label, dashed or not, its ends and its colour — each the
/// kind's default where none is written.
nonisolated struct ArrowLook: Equatable, Sendable {
    var label: String
    var dashed: Bool
    /// `none`, `end` or `both`.
    var ends: String
    var ink: EdgeInk

    init(label: String, dashed: Bool, ends: String, ink: EdgeInk) {
        self.label = label
        self.dashed = dashed
        self.ends = ends
        self.ink = ink
    }

    init(_ relation: Relation) {
        let fields = relation.fields
        if case .string(let label)? = fields["label"] { self.label = label } else { label = "" }
        if case .bool(let dashed)? = fields["dashed"] { self.dashed = dashed } else { dashed = false }
        if case .string(let ends)? = fields["ends"], ["none", "end", "both"].contains(ends) { self.ends = ends } else { ends = "end" }
        if case .string(let ink)? = fields["color"], let read = EdgeInk(rawValue: ink) { self.ink = read } else { self.ink = .grey }
    }

    /// The kind's look with this arrow's own: its colour and dash, its head the kind's at the ends it has.
    func applied(to base: LineLook) -> LineLook {
        LineLook(ink: ink, dash: dashed ? .dashed : .solid,
                 begin: ends == "both" ? base.end : .none, end: ends == "none" ? .none : base.end)
    }
}

/// A whiteboard's sketch, sticky and image drawn into a picture of their box: the shape filled and outlined in
/// its colours, its text in the middle — a `**bold**` run bold, a `[[link]]` in the link's colour by its name —,
/// an image fitted in its box. Over each, what a branch did to it (`ChangeMark`) on a chip at the top right, as a
/// card's header says it, and the corner it is sized by while it is picked.
@MainActor
enum SketchPicture {
    static func image(_ sketch: Sketch, size: CGSize, dark: Bool, scale: CGFloat, selected: Bool,
                      mark: ChangeMark? = nil, grip: Bool = false) -> CGImage? {
        draw(size: size, scale: scale, dark: dark, mark: mark, grip: grip) { ctx, rect in
            let body = rect.insetBy(dx: 1.5, dy: 1.5)
            let path = SketchShape.path(for: sketch.shape, in: body)
            if sketch.shape != "text" {
                if let fill = sketch.fill {
                    ctx.setFillColor(fill.color(dark: dark).withAlphaComponent(dark ? 0.28 : 0.14).cgColor)
                    ctx.addPath(path)
                    ctx.fillPath()
                }
                ctx.setStrokeColor(sketch.stroke.color(dark: dark).cgColor)
                ctx.setLineWidth(selected ? 3 : 1.5)
                ctx.addPath(path)
                ctx.strokePath()
            } else if selected {
                ctx.setStrokeColor(NSColor.controlAccentColor.cgColor)
                ctx.setLineDash(phase: 0, lengths: [4, 3])
                ctx.stroke(body)
            }
            centred(text(sketch.text, dark: dark), in: SketchShape.textArea(for: sketch.shape, in: body))
        }
    }

    /// A note on a whiteboard: a yellow sticky with its body.
    static func sticky(_ body: String, size: CGSize, dark: Bool, scale: CGFloat, selected: Bool,
                       mark: ChangeMark? = nil, grip: Bool = false) -> CGImage? {
        draw(size: size, scale: scale, dark: dark, mark: mark, grip: grip) { ctx, rect in
            let paper = rect.insetBy(dx: 2, dy: 2)
            ctx.setShadow(offset: CGSize(width: 0, height: 1.5), blur: 3, color: NSColor.black.withAlphaComponent(0.25).cgColor)
            ctx.setFillColor((dark ? NSColor.hex(0x6B5E1E) : NSColor.hex(0xFFF1A8)).cgColor)
            ctx.fill(paper)
            ctx.setShadow(offset: .zero, blur: 0)
            if selected {
                ctx.setStrokeColor(NSColor.controlAccentColor.cgColor)
                ctx.setLineWidth(2)
                ctx.stroke(paper)
            }
            text(body, dark: dark).draw(with: paper.insetBy(dx: 8, dy: 6),
                                        options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine])
        }
    }

    /// An image card: its picture fitted in its box, its sides kept, with nothing round it; while the picture is
    /// read (none), a thin dashed outline. Picked, ringed in the accent colour.
    static func picture(_ picture: CGImage?, size: CGSize, dark: Bool, scale: CGFloat, selected: Bool,
                        mark: ChangeMark? = nil, grip: Bool = false) -> CGImage? {
        draw(size: size, scale: scale, dark: dark, mark: mark, grip: grip) { ctx, rect in
            if let picture, picture.width > 0, picture.height > 0 {
                let fit = min(rect.width / CGFloat(picture.width), rect.height / CGFloat(picture.height))
                let drawn = CGSize(width: CGFloat(picture.width) * fit, height: CGFloat(picture.height) * fit)
                let at = CGRect(x: rect.midX - drawn.width / 2, y: rect.midY - drawn.height / 2,
                                width: drawn.width, height: drawn.height)
                // The context draws top-left first; a picture is drawn the other way up within it.
                ctx.saveGState()
                ctx.translateBy(x: 0, y: at.maxY + at.minY)
                ctx.scaleBy(x: 1, y: -1)
                ctx.interpolationQuality = .high
                ctx.draw(picture, in: at)
                ctx.restoreGState()
            } else {
                ctx.setStrokeColor(NSColor.secondaryLabelColor.cgColor)
                ctx.setLineDash(phase: 0, lengths: [4, 3])
                ctx.stroke(rect.insetBy(dx: 0.5, dy: 0.5))
                ctx.setLineDash(phase: 0, lengths: [])
            }
            if selected {
                ctx.setStrokeColor(NSColor.controlAccentColor.cgColor)
                ctx.setLineWidth(2)
                ctx.stroke(rect.insetBy(dx: 1, dy: 1))
            }
        }
    }

    /// `text` as drawn: each line its own, `**…**` bold, `[[path]]` the last part of its path in the link colour.
    static func text(_ text: String, dark: Bool) -> NSAttributedString {
        let font = NSFont.systemFont(ofSize: 12), bold = NSFont.boldSystemFont(ofSize: 12)
        let ink = dark ? NSColor.hex(0xE6E8EB) : NSColor.hex(0x1D2430)
        let style = NSMutableParagraphStyle()
        style.alignment = .center
        style.lineBreakMode = .byWordWrapping
        let out = NSMutableAttributedString()
        var strong = false
        var rest = Substring(text)
        while !rest.isEmpty {
            if rest.hasPrefix("**") {
                strong.toggle()
                rest = rest.dropFirst(2)
            } else if rest.hasPrefix("[["), let close = rest.range(of: "]]") {
                let path = rest[rest.index(rest.startIndex, offsetBy: 2)..<close.lowerBound]
                // By what follows its section, as the core measures it (`plain_note`).
                let name = path.split(separator: ":").last.map(String.init) ?? String(path)
                out.append(NSAttributedString(string: name, attributes: [
                    .font: strong ? bold : font, .foregroundColor: NSColor.linkColor,
                    .underlineStyle: NSUnderlineStyle.single.rawValue, .paragraphStyle: style]))
                rest = rest[close.upperBound...]
            } else {
                let next = [rest.range(of: "**")?.lowerBound, rest.range(of: "[[")?.lowerBound]
                    .compactMap { $0 }.min() ?? rest.endIndex
                let run = next == rest.startIndex ? rest.prefix(1) : rest[..<next]
                out.append(NSAttributedString(string: String(run), attributes: [
                    .font: strong ? bold : font, .foregroundColor: ink, .paragraphStyle: style]))
                rest = rest[run.endIndex...]
            }
        }
        return out
    }

    private static func centred(_ text: NSAttributedString, in area: CGRect) {
        let fits = text.boundingRect(with: CGSize(width: max(0, area.width), height: .greatestFiniteMagnitude),
                                     options: [.usesLineFragmentOrigin])
        let height = min(area.height, ceil(fits.height))
        text.draw(with: CGRect(x: area.minX, y: area.midY - height / 2, width: area.width, height: height),
                  options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine])
    }

    /// A picture of `size` drawn top-left first, AppKit's text drawn the same way up; over it what a branch did to
    /// it and the corner it is sized by (`decor`).
    private static func draw(size: CGSize, scale: CGFloat, dark: Bool, mark: ChangeMark?, grip: Bool,
                             _ body: (CGContext, CGRect) -> Void) -> CGImage? {
        let width = Int((size.width * scale).rounded(.up)), height = Int((size.height * scale).rounded(.up))
        guard width > 0, height > 0,
              let ctx = CGContext(data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0,
                                  space: CGColorSpace(name: CGColorSpace.sRGB)!,
                                  bitmapInfo: CGImageAlphaInfo.premultipliedFirst.rawValue) else { return nil }
        ctx.scaleBy(x: scale, y: scale)
        ctx.translateBy(x: 0, y: size.height)
        ctx.scaleBy(x: 1, y: -1)
        let previous = NSGraphicsContext.current
        NSGraphicsContext.current = NSGraphicsContext(cgContext: ctx, flipped: true)
        defer { NSGraphicsContext.current = previous }
        body(ctx, CGRect(origin: .zero, size: size))
        decor(ctx, size: size, dark: dark, mark: mark, grip: grip)
        return ctx.makeImage()
    }

    /// What a branch did to it, as a card's header says it — its letters in their tints — on a chip at the top
    /// right; the three short strokes of the corner it is sized by, at the bottom right.
    private static func decor(_ ctx: CGContext, size: CGSize, dark: Bool, mark: ChangeMark?, grip: Bool) {
        if let mark, let first = mark.letters.first {
            let tint = first.tint.nsColor(dark: dark)
            let letters = NSMutableAttributedString()
            for letter in mark.letters {
                letters.append(NSAttributedString(string: letter.text, attributes: [
                    .font: NSFont.monospacedSystemFont(ofSize: 10, weight: .bold),
                    .foregroundColor: letter.tint.nsColor(dark: dark)]))
            }
            let width = ceil(letters.size().width) + 8
            let chip = CGRect(x: size.width - width - 4, y: 4, width: width, height: 15)
            ctx.setFillColor((dark ? NSColor.hex(0x0A0F16) : NSColor.white).cgColor)
            ctx.addPath(CGPath(roundedRect: chip, cornerWidth: 3, cornerHeight: 3, transform: nil))
            ctx.fillPath()
            ctx.setStrokeColor(tint.cgColor)
            ctx.setLineWidth(1)
            ctx.addPath(CGPath(roundedRect: chip.insetBy(dx: 0.5, dy: 0.5), cornerWidth: 3, cornerHeight: 3, transform: nil))
            ctx.strokePath()
            letters.draw(at: CGPoint(x: chip.minX + 4, y: chip.minY + 1))
        }
        if grip {
            ctx.setStrokeColor(NSColor.controlAccentColor.cgColor)
            ctx.setLineWidth(1)
            for step in [CGFloat(4), 8, 12] {
                ctx.move(to: CGPoint(x: size.width - 3, y: size.height - 3 - step))
                ctx.addLine(to: CGPoint(x: size.width - 3 - step, y: size.height - 3))
            }
            ctx.strokePath()
        }
    }
}
