import AppKit

/// Draws one card into a bitmap, which then becomes a layer's `contents`.
///
/// Rasterising once and letting Core Animation move the result is the whole reason
/// this app exists: the web build had to keep 163 cards and their rows as live DOM,
/// and paid for that on every pan and zoom. Here a card is a texture until its
/// content actually changes.
///
/// **What is drawn never depends on the zoom.** A card looks the same whether the
/// whole project is on screen or one card fills it — the point of the diagram is to be
/// read while overviewing it, and a card that turns into a grey block when you pull
/// back is a card you cannot read when you most need to. `scale` below is raster
/// resolution only: the same drawing, more pixels.
enum CardRenderer {

    /// Every card is kept at this resolution, always. 163 cards of the real project
    /// cost 47 MB here, which buys "the entire diagram is drawn at all times".
    static let baseScale: CGFloat = 1.0
    /// On-screen cards are re-cut at this resolution once zoomed in far enough that
    /// a 1x texture would look soft on a retina display.
    static let sharpScale: CGFloat = 2.0
    /// Below this zoom the base texture is already denser than the screen.
    static let sharpFromZoom: CGFloat = 0.6

    static func height(of content: CardContent) -> CGFloat { cardHeight(content) }

    /// `drawn` names the schemas that have a card right now.
    ///
    /// A reference row shows `→` because a line is about to carry the answer. When
    /// the target has no card — an enum with reuse unfolded, or anything the tag
    /// filter removed — there is no line, and the arrow promises one that never
    /// arrives. Naming the target instead is the same move the self-reference row
    /// already makes with `↺Name`.
    ///
    /// `outside` is another section's card: drawn the same, then faded under a veil
    /// of the canvas colour and outlined with a dash, so it reads as context rather
    /// than as part of the section. `temporary` is one put there for a look, not
    /// kept: a brass dash and 一時 on the header. `mark` is what the open branch did to
    /// it (`BranchMarks.mark`): its letters in their colours beside the kind's name on the header.
    /// `ghost`: a card ghost's box — the card it leads to, under the ghost's header and a dashed
    /// outline (`GhostLook`). `gone`: one the open branch removed, or a merge looked at takes away —
    /// faded under the veil, its name struck through, outlined with a red dash; its `mark` D. `missing`:
    /// the box of a working directory's folder that reading the folder no longer finds, kept for the notes
    /// in it — its name struck through, outlined with a dash in the warning colour.
    static func image(
        for schema: SchemaNode, content: CardContent, theme: GraphTheme,
        scale: CGFloat, selected: Bool, drawn: Set<String> = [], outside: Bool = false,
        temporary: Bool = false, ghost: GhostLook? = nil, gone: Bool = false, missing: Bool = false,
        mark: ChangeMark? = nil,
        width: CGFloat = CardGeometry.cardWidth,
        height given: CGFloat? = nil, document: DocumentBody? = nil, grip: Bool = false
    ) -> CGImage? {
        let height = given ?? height(of: content)

        let pixelWidth = Int((width * scale).rounded(.up))
        let pixelHeight = Int((height * scale).rounded(.up))
        guard pixelWidth > 0, pixelHeight > 0 else { return nil }

        guard let ctx = CGContext(
            data: nil, width: pixelWidth, height: pixelHeight,
            bitsPerComponent: 8, bytesPerRow: 0,
            space: CGColorSpace(name: CGColorSpace.sRGB)!,
            bitmapInfo: CGImageAlphaInfo.premultipliedFirst.rawValue)
        else { return nil }

        ctx.scaleBy(x: scale, y: scale)
        // Card geometry is top-left origin, like the layout that produced it.
        ctx.translateBy(x: 0, y: height)
        ctx.scaleBy(x: 1, y: -1)

        let previous = NSGraphicsContext.current
        NSGraphicsContext.current = NSGraphicsContext(cgContext: ctx, flipped: true)
        defer { NSGraphicsContext.current = previous }

        draw(schema: schema, content: content, theme: theme,
             size: CGSize(width: width, height: height), selected: selected,
             drawn: drawn, temporary: temporary, ghost: ghost, mark: mark, document: document, grip: grip, in: ctx)
        if missing {
            // Gone from the folder: its name struck through, a dash in the warning colour round it.
            let body = CGRect(x: 0, y: 0, width: width, height: height).insetBy(dx: 1, dy: 1)
            let outline = CGPath(roundedRect: body, cornerWidth: 8, cornerHeight: 8, transform: nil)
            let ink = NSColor.systemOrange
            let nameFont = NSFont.monospacedSystemFont(ofSize: 13, weight: .semibold)
            let nameWidth = (schema.name as NSString).size(withAttributes: [.font: nameFont]).width
            let midY = CardGeometry.headerHeight / 2
            let start = 12 + compactMarkWidth
            ctx.setStrokeColor(ink.cgColor)
            ctx.setLineWidth(1.4)
            ctx.move(to: CGPoint(x: start, y: midY))
            ctx.addLine(to: CGPoint(x: min(width - 10, start + nameWidth + 2), y: midY))
            ctx.strokePath()
            ctx.addPath(outline)
            ctx.setStrokeColor((selected ? theme.brass : ink).cgColor)
            ctx.setLineWidth(selected ? 2.4 : 1.6)
            ctx.setLineDash(phase: 0, lengths: [6, 4])
            ctx.strokePath()
        } else if gone {
            // Removed: faded, its name struck through, a red dash round it.
            let body = CGRect(x: 0, y: 0, width: width, height: height).insetBy(dx: 0.5, dy: 0.5)
            let outline = CGPath(roundedRect: body, cornerWidth: 8, cornerHeight: 8, transform: nil)
            ctx.addPath(outline)
            ctx.setFillColor(theme.background.withAlphaComponent(0.55).cgColor)
            ctx.fillPath()
            let ink = GraphTheme.nodeInk("end").color(dark: theme.isDark)
            let nameFont = NSFont.monospacedSystemFont(ofSize: 13, weight: .semibold)
            let nameWidth = (schema.name as NSString).size(withAttributes: [.font: nameFont]).width
            let midY = CardGeometry.headerHeight / 2
            ctx.setStrokeColor(ink.cgColor)
            ctx.setLineWidth(1.6)
            ctx.move(to: CGPoint(x: 10, y: midY))
            ctx.addLine(to: CGPoint(x: min(width - 10, 14 + nameWidth), y: midY))
            ctx.strokePath()
            ctx.addPath(outline)
            ctx.setStrokeColor((selected ? theme.brass : ink).cgColor)
            ctx.setLineWidth(selected ? 2.4 : 1.4)
            ctx.setLineDash(phase: 0, lengths: [6, 4])
            ctx.strokePath()
        } else if let ghost {
            // A shortcut, not the card: outlined with a dash in the card's colour — red where
            // it leads nowhere.
            let body = CGRect(x: 0, y: 0, width: width, height: height).insetBy(dx: 1, dy: 1)
            ctx.addPath(CGPath(roundedRect: body, cornerWidth: 8, cornerHeight: 8, transform: nil))
            let ink = ghost.broken ? GraphTheme.nodeInk("end").color(dark: theme.isDark)
                : selected ? theme.brass : theme.accent(for: schema.kind)
            ctx.setStrokeColor(ink.cgColor)
            ctx.setLineWidth(selected ? 2.4 : 1.6)
            ctx.setLineDash(phase: 0, lengths: [7, 4])
            ctx.strokePath()
        } else if temporary {
            let body = CGRect(x: 0, y: 0, width: width, height: height).insetBy(dx: 1, dy: 1)
            ctx.addPath(CGPath(roundedRect: body, cornerWidth: 8, cornerHeight: 8, transform: nil))
            ctx.setStrokeColor(theme.brass.cgColor)
            ctx.setLineWidth(selected ? 2.4 : 1.6)
            ctx.setLineDash(phase: 0, lengths: [6, 4])
            ctx.strokePath()
        } else if outside {
            let body = CGRect(x: 0, y: 0, width: width, height: height).insetBy(dx: 0.5, dy: 0.5)
            let outline = CGPath(roundedRect: body, cornerWidth: 8, cornerHeight: 8, transform: nil)
            ctx.addPath(outline)
            ctx.setFillColor(theme.background.withAlphaComponent(0.5).cgColor)
            ctx.fillPath()
            ctx.addPath(outline)
            ctx.setStrokeColor((selected ? theme.brass : theme.ink3).cgColor)
            ctx.setLineWidth(selected ? 2 : 1.2)
            ctx.setLineDash(phase: 0, lengths: [5, 4])
            ctx.strokePath()
        }

        return ctx.makeImage()
    }

    // MARK: - Drawing

    /// How a card ghost's box is drawn (`specboard://SpecBoard/Docs/Design:DesignSections` §2): its header says it
    /// is a shortcut — the mark, the ghost's name and what it points at
    /// (`SchemaDirectory.ghostTitle`) — and one that leads nowhere is drawn small and broken.
    struct GhostLook: Hashable {
        var title: String
        var broken: Bool
    }

    /// What a card ghost's header badge says where it leads nowhere.
    static var brokenGhostWord: String { L10n[.Diagram.cardBrokenGhost] }

    /// A card drawn as its document: the picture of its whole text, taken once
    /// (`DocSnapshots`), scrolled `offset` points into it. Nil picture: not taken yet.
    struct DocumentBody {
        var image: CGImage?
        /// The page in its own points: as wide as it was drawn, the whole document's height.
        var size: CGSize
        /// How far the card is zoomed into: 1 is the page's width across the card.
        var zoom: CGFloat = 1
        /// How far it is moved, in the card's points once zoomed.
        var offset: CGPoint = .zero
        /// The picture stops short of the document's end (`DocSnapshots.tallest`).
        var cut = false
        /// Which picture it is (`DocSnapshots.Picture.number`): 0 for none.
        var picture = 0
    }

    /// Where a document's picture shows inside a card of `size`: under its header, or, for a card
    /// its kind's face draws, under the `rows` of its fields above it.
    static func documentArea(_ size: CGSize, rows: Int = 0) -> CGRect {
        let top = rows == 0 ? CardGeometry.headerHeight + 8
            : CardGeometry.cardBorder + CardGeometry.headerHeight + CardGeometry.listPadding / 2
                + CGFloat(rows) * CardGeometry.rowHeight + 8
        return CGRect(x: 12, y: top, width: max(0, size.width - 24), height: max(0, size.height - top - 14))
    }

    /// The corner a writing card is dragged by to size it.
    static let gripSize: CGFloat = 14

    private static func draw(
        schema: SchemaNode, content: CardContent, theme: GraphTheme,
        size: CGSize, selected: Bool, drawn: Set<String>, temporary: Bool = false, ghost: GhostLook? = nil,
        mark: ChangeMark? = nil, document: DocumentBody? = nil, grip: Bool = false,
        in ctx: CGContext
    ) {
        let body = CGRect(origin: .zero, size: size).insetBy(dx: 0.5, dy: 0.5)
        let radius: CGFloat = 8
        let outline = CGPath(roundedRect: body, cornerWidth: radius, cornerHeight: radius,
                             transform: nil)

        ctx.addPath(outline)
        ctx.setFillColor(theme.panel.cgColor)
        ctx.fillPath()

        let header = CGRect(x: body.minX, y: body.minY,
                            width: body.width, height: CardGeometry.headerHeight)
        ctx.saveGState()
        ctx.addPath(outline)
        ctx.clip()
        let headerFill = SectionGhosts.isGhost(schema.id) || ghost?.broken == true ? theme.panel3
            : FlowCardMap.isCard(schema.id) ? theme.headerFill(forNode: schema.nodes.first?.kind ?? "")
            : theme.headerFill(for: schema)
        ctx.setFillColor(headerFill.cgColor)
        ctx.fill(header)
        ctx.restoreGState()

        ctx.setStrokeColor(theme.line.cgColor)
        ctx.setLineWidth(1)
        ctx.move(to: CGPoint(x: body.minX, y: header.maxY))
        ctx.addLine(to: CGPoint(x: body.maxX, y: header.maxY))
        ctx.strokePath()

        drawHeaderText(schema, in: header, theme: theme, temporary: temporary, ghost: ghost, mark: mark)

        // What is out of sight above, at the top where it went. The strip is
        // reserved whether or not it has anything to say, so the card keeps its
        // height as it scrolls.
        if content.hasAboveStrip {
            let rect = CGRect(x: body.minX, y: aboveStripOffset(content)
                                - CardGeometry.moreHeight / 2,
                              width: body.width, height: CardGeometry.moreHeight)
            text("↑\(content.above)", in: rect.insetBy(dx: 12, dy: 2),
                 font: .monospacedSystemFont(ofSize: 10.5, weight: .regular),
                 color: theme.ink3)
        }

        var y = listTop(content)
        for row in content.rows {
            let rect = CGRect(x: body.minX + 1, y: y,
                              width: body.width - 2, height: CardGeometry.rowHeight)
            drawRow(row, schema: schema, rect: rect, theme: theme,
                    drawn: drawn, in: ctx)
            y += CardGeometry.rowHeight
        }
        // The rows an implementation line whose code changed leaves: a dot at the left of each.
        for drawnRow in codeRows(schema, content).keys.sorted() {
            drawReviewDot(codeDotRect(row: drawnRow, in: content), in: ctx)
        }

        if content.hasBelowStrip {
            let rect = CGRect(x: body.minX, y: y,
                              width: body.width, height: CardGeometry.moreHeight)
            // "+N" while the card is at its top, because that is what it has always
            // said and nothing has moved yet. Once scrolled it is one of two counts,
            // and an arrow is what tells them apart.
            text(content.above == 0 ? "+\(content.below)" : "↓\(content.below)",
                 in: rect.insetBy(dx: 12, dy: 2),
                 font: .monospacedSystemFont(ofSize: 10.5, weight: .regular),
                 color: theme.ink3)
        }

        if content.document, let document {
            let area = documentArea(size, rows: content.rows.count)
            if !content.rows.isEmpty {
                // The fields above, the Markdown below: a hairline between them.
                ctx.setStrokeColor(theme.line.cgColor)
                ctx.setLineWidth(0.75)
                ctx.move(to: CGPoint(x: body.minX + 12, y: area.minY - 4))
                ctx.addLine(to: CGPoint(x: body.maxX - 12, y: area.minY - 4))
                ctx.strokePath()
            }
            drawDocument(document, in: area, theme: theme, ctx: ctx)
        }
        if grip {
            // Three short strokes in the corner, as a window's has.
            ctx.setStrokeColor(theme.ink3.withAlphaComponent(0.8).cgColor)
            ctx.setLineWidth(1)
            for step in [CGFloat(4), 8, 12] {
                ctx.move(to: CGPoint(x: size.width - 3, y: size.height - 3 - step))
                ctx.addLine(to: CGPoint(x: size.width - 3 - step, y: size.height - 3))
            }
            ctx.strokePath()
        }

        ctx.addPath(outline)
        ctx.setStrokeColor((selected ? theme.brass : theme.line2).cgColor)
        ctx.setLineWidth(selected ? 2 : 1)
        ctx.strokePath()

        if schema.showsReviewDot { drawReviewDot(reviewDotRect(in: size), in: ctx) }
    }

    /// Where the dot of a drawn row (`row`, an index into `content.rows`) sits in the card's box (top-left origin):
    /// the same 8 points as the header's, at the left of the row, left of the row's own marks.
    static func codeDotRect(row: Int, in content: CardContent) -> CGRect {
        CGRect(x: 3.5, y: listTop(content) + CGFloat(row) * CardGeometry.rowHeight + (CardGeometry.rowHeight - 8) / 2,
               width: 8, height: 8)
    }

    /// The implementation lines whose code changed, by the drawn row each leaves (an index into
    /// `content.rows`): the row the line names, or the nearest one the card draws above it. None for a card
    /// drawn as its document, whose lines leave from its header, for a line from the head, and for a row out
    /// of the window.
    static func codeRows(_ schema: SchemaNode, _ content: CardContent) -> [Int: [CodeReviewLine]] {
        guard !content.document, !content.rows.isEmpty else { return [:] }
        let changed = schema.codeReview.filter { $0.isChanged && !$0.from.isEmpty }
        guard !changed.isEmpty else { return [:] }
        let index = CardIndex(content)
        var out: [Int: [CodeReviewLine]] = [:]
        for line in changed {
            guard let at = GraphEdges.nearestRow(index, line.from) else { continue }
            let drawn = at - content.first
            if drawn >= 0, drawn < content.rows.count { out[drawn, default: []].append(line) }
        }
        return out
    }

    /// Where a part's review dot sits in its box (top-left origin), as the mock draws it: 8 points across,
    /// 4 in from the top right corner. A card with an implementation line whose code changed wears it there too.
    static func reviewDotRect(in size: CGSize) -> CGRect {
        CGRect(x: size.width - 12, y: 4, width: 8, height: 8)
    }

    /// What a dot says of the implementation lines whose code changed: a line saying so, then each line by
    /// where it leaves and what it reaches, with when its code was recorded.
    static func codeTip(_ lines: [CodeReviewLine]) -> [String] {
        [L10n[.Diagram.cardCodeTipTitle]] + lines.map { line in
            line.linkedAt.map { L10n[.Diagram.cardCodeTipRecorded(line.reached, $0)] } ?? line.reached
        }
    }

    /// The review dot's colour, also the panel's for the line it marks.
    static let reviewOrange = NSColor(srgbRed: 0xF9 / 255, green: 0x73 / 255, blue: 0x16 / 255, alpha: 1)

    /// The orange dot of a part with cards to look at again — the mock's colour and white ring.
    static func drawReviewDot(_ dot: CGRect, in ctx: CGContext) {
        ctx.setFillColor(reviewOrange.cgColor)
        ctx.fillEllipse(in: dot)
        ctx.setStrokeColor(NSColor.white.cgColor)
        ctx.setLineWidth(1.5)
        ctx.strokeEllipse(in: dot.insetBy(dx: 0.75, dy: 0.75))
    }

    /// The document's picture in `area`, scrolled; a word while it is being taken; and
    /// at either end that has more, a strip that says so.
    private static func drawDocument(_ document: DocumentBody, in area: CGRect, theme: GraphTheme, ctx: CGContext) {
        guard let image = document.image else {
            text(L10n[.Diagram.cardDrawing], in: CGRect(x: area.minX, y: area.minY, width: area.width, height: 20),
                 font: .systemFont(ofSize: 11), color: theme.ink3)
            return
        }
        let shown = CGSize(width: document.size.width * document.zoom, height: document.size.height * document.zoom)
        let offset = document.offset
        ctx.saveGState()
        ctx.clip(to: area)
        let picture = NSImage(cgImage: image, size: document.size)
        picture.draw(in: CGRect(x: area.minX - offset.x, y: area.minY - offset.y, width: shown.width, height: shown.height),
                     from: .zero, operation: .sourceOver, fraction: 1, respectFlipped: true,
                     hints: [.interpolation: NSNumber(value: NSImageInterpolation.high.rawValue)])
        ctx.restoreGState()
        let font = NSFont.monospacedSystemFont(ofSize: 10.5, weight: .regular)
        func strip(_ word: String, at y: CGFloat) {
            let band = CGRect(x: area.minX - 11, y: y, width: area.width + 22, height: CardGeometry.moreHeight)
            ctx.setFillColor(theme.panel.withAlphaComponent(0.92).cgColor)
            ctx.fill(band)
            text(word, in: band.insetBy(dx: 12, dy: 2), font: font, color: theme.ink3)
        }
        /// A small label on the panel's colour, for what is to the side and how far it is zoomed.
        func tag(_ word: String, _ rect: CGRect) {
            ctx.setFillColor(theme.panel.withAlphaComponent(0.92).cgColor)
            ctx.addPath(CGPath(roundedRect: rect, cornerWidth: 4, cornerHeight: 4, transform: nil))
            ctx.fillPath()
            text(word, in: rect.insetBy(dx: 3, dy: 2), font: font, color: theme.ink3, centeredVertically: true)
        }
        let moved = offset.x > 0.5 || offset.y > 0.5 || document.zoom > 1.01
        if offset.y > 0.5 { strip("↑", at: area.minY - 2) }
        if offset.y + area.height < shown.height - 1 {
            strip(moved ? "↓" : L10n[.Diagram.cardDocumentMore], at: area.maxY - CardGeometry.moreHeight + 2)
        } else if document.cut {
            strip(L10n[.Diagram.cardDocumentCut], at: area.maxY - CardGeometry.moreHeight + 2)
        }
        if offset.x > 0.5 { tag("‹", CGRect(x: area.minX - 8, y: area.midY - 10, width: 14, height: 20)) }
        if offset.x + area.width < shown.width - 1 { tag("›", CGRect(x: area.maxX - 6, y: area.midY - 10, width: 14, height: 20)) }
        if document.zoom > 1.01 {
            tag(String(format: "×%.1f", document.zoom), CGRect(x: area.maxX - 40, y: area.minY + 2, width: 40, height: 18))
        }
    }

    /// As wide as the card's longest line (§12: width `auto`): its header — name,
    /// label and kind — and each of its rows, measured in the fonts they are drawn in.
    static func naturalWidth(_ schema: SchemaNode, content: CardContent) -> CGFloat {
        func width(_ text: String, _ font: NSFont) -> CGFloat { (text as NSString).size(withAttributes: [.font: font]).width }
        let name = NSFont.monospacedSystemFont(ofSize: 13, weight: .semibold)
        var widest = 24 + width(schema.name, name) + 10
            + (schema.mapNumber.map { width("\($0)", numberFont) + 14 } ?? 0)
            + (schema.label.isEmpty ? 0 : width(schema.label, .systemFont(ofSize: 11.5)) + 8)
            + width(schema.kind == .custom ? ownKindWord(schema) : schema.kind == .instance || schema.kind == .table ? instanceWord(schema)
                    : schema.kind.rawValue, .systemFont(ofSize: 10)) + 8
        let field = NSFont.monospacedSystemFont(ofSize: 11.5, weight: .regular)
        let type = NSFont.monospacedSystemFont(ofSize: 10, weight: .regular)
        for row in content.all {
            let indent = CGFloat(row.depth) * 14 + 22
            if let datum = row.datum {
                widest = max(widest, indent + width(datum.name, field) + width(datum.badge, badgeFont) + 16
                             + width(datum.text ?? "", field) + 36)
            } else if let symbol = row.symbol {
                widest = max(widest, indent + 20 + width(symbol.name, field) + width("\(symbol.line)", type) + 36)
            } else if let step = row.step {
                widest = max(widest, indent + max(46, width(step.label, .monospacedSystemFont(ofSize: 10.5, weight: .semibold)))
                             + width(step.text, .monospacedSystemFont(ofSize: 11, weight: .regular))
                             + (step.detail.map { width($0, .systemFont(ofSize: 10)) + 8 } ?? 0) + 30)
            } else if let item = row.field {
                let trailing = item.type.isRef ? "→ " + (item.embeds ?? "") : item.type.rawValue
                widest = max(widest, indent + width(item.name + (item.isArray ? "[]" : ""), field) + width(trailing, type) + 36)
            } else if let value = row.value {
                widest = max(widest, indent + width(value.value, field) + width(value.label, .systemFont(ofSize: 11)) + 32)
            }
        }
        return min(640, max(220, widest.rounded(.up)))
    }

    /// A card of a kind the project defines, as its header says it: the kind's label and the
    /// fields its definition puts in the header, when written.
    static func ownKindWord(_ schema: SchemaNode) -> String {
        let def = schema.kindDef
        let said = (def?.header ?? []).compactMap { name -> String? in
            guard let value = schema.ownFields[name] else { return nil }
            let text = OwnFieldText.short(value)
            return text.isEmpty ? nil : text
        }
        return ([def?.label ?? schema.kindName] + said).joined(separator: " · ")
    }

    /// A goal's header word: its kind by name, and the fields its kind puts in the header, each
    /// value as its definition shows it (a goal's 進行中 is In progress on an English screen); none
    /// for a card of another kind.
    static func customKindWord(_ schema: SchemaNode) -> String? {
        CustomKind.of(schema.kind).map { custom in
            let said = custom.header.compactMap { name -> String? in
                guard let value = schema.customFields[name], !value.isEmpty else { return nil }
                return schema.definition?.field(name)?.shown(value) ?? value
            }
            return ([custom.name] + said).joined(separator: " · ")
        }
    }

    /// A data card's header word: its shape's name, where another card's kind is.
    static func instanceWord(_ schema: SchemaNode) -> String {
        let shape = schema.shapeName ?? L10n[.Diagram.cardNoShape]
        // A table's header says whose data it gathers.
        return schema.kind == .table ? L10n[.Diagram.cardTableOf(shape)] : shape
    }

    /// A data card's badge: a type, as the nav panel's branch badge says a tree.
    static let badgeFont = NSFont.monospacedSystemFont(ofSize: 9.5, weight: .regular)

    /// A data card's row (`InstanceDatum`): its name, its type's badge after it and its value after
    /// that, cut at its end where it does not fit — the badge always shown; a required field with no
    /// value marked before its name.
    private static func drawDatum(_ datum: InstanceDatum, rect: CGRect, indent: CGFloat, theme: GraphTheme,
                                  ctx: CGContext) {
        let nameFont = NSFont.monospacedSystemFont(ofSize: 11.5, weight: .regular)
        var x = rect.minX + indent
        if datum.missing {
            ctx.setFillColor(theme.brass.cgColor)
            ctx.fillEllipse(in: CGRect(x: x, y: rect.midY - 3, width: 6, height: 6))
            x += 10
        }
        let badgeWidth = (datum.badge as NSString).size(withAttributes: [.font: badgeFont]).width + 8
        let nameWidth = min((datum.name as NSString).size(withAttributes: [.font: nameFont]).width,
                            max(10, rect.maxX - 12 - x - badgeWidth - 8))
        text(datum.name, in: CGRect(x: x, y: rect.minY, width: nameWidth, height: rect.height),
             font: nameFont, color: theme.ink, centeredVertically: true)
        x += nameWidth + 6
        let badge = CGRect(x: x, y: rect.midY - 7.5, width: badgeWidth, height: 15)
        ctx.addPath(CGPath(roundedRect: badge.insetBy(dx: 0.5, dy: 0.5), cornerWidth: 3, cornerHeight: 3, transform: nil))
        ctx.setStrokeColor(theme.ink3.withAlphaComponent(0.7).cgColor)
        ctx.setLineWidth(1)
        ctx.strokePath()
        text(datum.badge, in: badge.insetBy(dx: 4, dy: 0), font: badgeFont, color: theme.ink2, centeredVertically: true)
        x = badge.maxX + 8
        if let value = datum.text, rect.maxX - 12 - x > 10 {
            text(value, in: CGRect(x: x, y: rect.minY, width: rect.maxX - 12 - x, height: rect.height),
                 font: nameFont, color: theme.ink, centeredVertically: true)
        }
    }

    /// A file's row (`CodeSymbol`): its mark in a small box in its kind's colour, its name, and on the
    /// right, faint, the line its name is on — what the file's text is read at.
    private static func drawSymbol(_ symbol: CodeSymbol, rect: CGRect, indent: CGFloat, theme: GraphTheme,
                                   ctx: CGContext) {
        let ink = GraphTheme.symbolInk(symbol.of, dark: theme.isDark)
        let box = CGRect(x: rect.minX + indent, y: rect.midY - 7, width: 14, height: 14)
        ctx.addPath(CGPath(roundedRect: box, cornerWidth: 3, cornerHeight: 3, transform: nil))
        ctx.setFillColor(ink.withAlphaComponent(theme.isDark ? 0.22 : 0.14).cgColor)
        ctx.fillPath()
        let badge = symbol.badge
        let size = (badge as NSString).size(withAttributes: [.font: symbolBadgeFont])
        text(badge, in: CGRect(x: box.midX - size.width / 2, y: box.minY, width: size.width + 1, height: box.height),
             font: symbolBadgeFont, color: ink, centeredVertically: true)
        let number = "\(symbol.line)"
        let numberFont = NSFont.monospacedDigitSystemFont(ofSize: 10, weight: .regular)
        let numberWidth = (number as NSString).size(withAttributes: [.font: numberFont]).width
        text(number, in: CGRect(x: rect.maxX - numberWidth - 12, y: rect.minY, width: numberWidth + 1, height: rect.height),
             font: numberFont, color: theme.ink3, centeredVertically: true)
        let nameX = box.maxX + 6
        text(symbol.name, in: CGRect(x: nameX, y: rect.minY, width: max(10, rect.maxX - numberWidth - 20 - nameX), height: rect.height),
             font: .monospacedSystemFont(ofSize: 11.5, weight: .regular), color: theme.ink, centeredVertically: true)
    }

    static let symbolBadgeFont = NSFont.monospacedSystemFont(ofSize: 9.5, weight: .bold)

    /// The number a row or a card on a card map is said by, its order.
    static let numberFont = NSFont.monospacedDigitSystemFont(ofSize: 10.5, weight: .semibold)

    /// A number in a small capsule at `x`, centred on `midY`: how far it reaches.
    @discardableResult
    private static func drawNumber(_ number: Int, x: CGFloat, midY: CGFloat, theme: GraphTheme,
                                   accent: NSColor, ctx: CGContext) -> CGFloat {
        let word = "\(number)"
        let width = max(CardGeometry.numberWidth, (word as NSString).size(withAttributes: [.font: numberFont]).width + 8)
        let capsule = CGRect(x: x, y: midY - 8, width: width, height: 16)
        ctx.addPath(CGPath(roundedRect: capsule, cornerWidth: 8, cornerHeight: 8, transform: nil))
        ctx.setFillColor(accent.withAlphaComponent(0.16).cgColor)
        ctx.fillPath()
        let size = (word as NSString).size(withAttributes: [.font: numberFont])
        text(word, in: CGRect(x: capsule.midX - size.width / 2, y: capsule.minY, width: size.width + 1, height: capsule.height),
             font: numberFont, color: accent, centeredVertically: true)
        return capsule.maxX
    }

    /// How far a compact box's name is set in for its mark (`CompactBox`).
    static let compactMarkWidth: CGFloat = 20

    /// A working directory's file or a folder's box: its mark — a document or a folder — and its name,
    /// cut at its end where the box is too narrow, and what a branch did to it.
    private static func drawCompactHeader(_ schema: SchemaNode, in header: CGRect, theme: GraphTheme,
                                          mark: ChangeMark?) {
        let inset = header.insetBy(dx: 12, dy: 0)
        let symbol = schema.ownKind == WorkDirIds.fileKind ? "doc.text" : "folder"
        let tint = schema.ownKind == WorkDirIds.fileKind ? theme.accent(for: schema) : theme.ink3
        let config = NSImage.SymbolConfiguration(pointSize: 12, weight: .regular)
            .applying(NSImage.SymbolConfiguration(paletteColors: [tint]))
        if let image = NSImage(systemSymbolName: symbol, accessibilityDescription: nil)?.withSymbolConfiguration(config) {
            let size = image.size
            let rect = CGRect(x: inset.minX, y: inset.midY - size.height / 2, width: size.width, height: size.height)
            image.draw(in: rect, from: .zero, operation: .sourceOver, fraction: 1, respectFlipped: true, hints: nil)
        }
        let marked = mark.map { Self.markText($0, dark: theme.isDark) }
        let markWidth = marked.map { ceil($0.size().width) + 5 } ?? 0
        let nameX = inset.minX + compactMarkWidth
        text(schema.name, in: CGRect(x: nameX, y: inset.minY, width: max(0, inset.maxX - nameX - markWidth), height: inset.height),
             font: .monospacedSystemFont(ofSize: 12, weight: .semibold), color: theme.ink, centeredVertically: true)
        if let marked {
            let size = marked.size()
            marked.draw(in: CGRect(x: inset.maxX - ceil(size.width), y: inset.midY - size.height / 2,
                                     width: ceil(size.width), height: size.height))
        }
    }

    private static func drawHeaderText(
        _ schema: SchemaNode, in header: CGRect, theme: GraphTheme, temporary: Bool = false,
        ghost: GhostLook? = nil, mark: ChangeMark? = nil
    ) {
        if CompactBox.isCompact(schema) {
            drawCompactHeader(schema, in: header, theme: theme, mark: mark)
            return
        }
        var inset = header.insetBy(dx: 12, dy: 0)
        // Its order among the cards of the container whose map draws it (a task in its goal).
        if let number = schema.mapNumber, let ctx = NSGraphicsContext.current?.cgContext {
            let reach = drawNumber(number, x: inset.minX, midY: inset.midY, theme: theme,
                                   accent: theme.accent(for: schema), ctx: ctx)
            inset = CGRect(x: reach + 6, y: inset.minY, width: max(0, inset.maxX - reach - 6), height: inset.height)
        }
        let nameFont = NSFont.monospacedSystemFont(ofSize: 13, weight: .semibold)
        // A card ghost's header is the ghost's: the mark, its name and what it points at.
        let name = ghost?.title ?? schema.name
        let nameWidth = (name as NSString).size(withAttributes: [.font: nameFont]).width

        // The kind badge is drawn from the right, so the Japanese label gets whatever
        // is left rather than pushing the badge off the card.
        let kindFont = NSFont.systemFont(ofSize: 10)
        let temporaryWord = temporary ? " · " + L10n[.Diagram.cardTemporary] : ""
        let kindWord = (ghost?.broken == true ? brokenGhostWord
            : SectionGhosts.isGhost(schema.id) ? SectionGhosts.kindWord
            : FlowCardMap.isCard(schema.id) ? FlowNode.Vocabulary.label(schema.nodes.first?.kind ?? "")
            : schema.kind == .custom ? ownKindWord(schema)
            : schema.kind == .instance || schema.kind == .table ? instanceWord(schema)
            : customKindWord(schema) ?? schema.kind.rawValue)
            + temporaryWord
        // What the open branch did to it: its letters in their colours, after the kind's name.
        let marked = mark.map { Self.markText($0, dark: theme.isDark) }
        let markWidth = marked.map { ceil($0.size().width) + 5 } ?? 0
        let kindWidth = (kindWord as NSString)
            .size(withAttributes: [.font: kindFont]).width + markWidth
        // A ghost's title keeps to the room the badge leaves, cut at its end.
        let nameRoom = ghost == nil ? nameWidth + 2 : min(nameWidth + 2, max(0, inset.width - kindWidth - 8))
        text(name,
             in: CGRect(x: inset.minX, y: inset.minY, width: nameRoom, height: inset.height),
             font: nameFont, color: theme.ink, centeredVertically: true)
        if let marked {
            let size = marked.size()
            marked.draw(in: CGRect(x: inset.maxX - ceil(size.width), y: inset.midY - size.height / 2,
                                     width: ceil(size.width), height: size.height))
        }
        text(kindWord,
             in: CGRect(x: inset.maxX - kindWidth, y: inset.minY,
                        width: kindWidth - markWidth, height: inset.height),
             font: kindFont,
             color: SectionGhosts.isGhost(schema.id) ? theme.ink3
                : FlowCardMap.isCard(schema.id) ? theme.accent(forNode: schema.nodes.first?.kind ?? "")
                : theme.accent(for: schema),
             centeredVertically: true)

        let labelX = inset.minX + nameWidth + 10
        let labelWidth = inset.maxX - kindWidth - 8 - labelX
        if ghost == nil, !schema.label.isEmpty, labelWidth > 20 {
            text(schema.label,
                 in: CGRect(x: labelX, y: inset.minY, width: labelWidth, height: inset.height),
                 font: .systemFont(ofSize: 11.5), color: theme.ink3, centeredVertically: true)
        }
    }

    /// What the open branch did to a card, as its header says it after the kind's name: its letters,
    /// each in its colour on a light or a dark diagram.
    static func markText(_ mark: ChangeMark, dark: Bool) -> NSAttributedString {
        let font = NSFont.monospacedSystemFont(ofSize: 10.5, weight: .bold)
        let out = NSMutableAttributedString()
        for letter in mark.letters {
            out.append(NSAttributedString(string: letter.text, attributes: [
                .font: font, .foregroundColor: letter.tint.nsColor(dark: dark),
            ]))
        }
        return out
    }

    private static func drawRow(
        _ row: CardRow, schema: SchemaNode, rect: CGRect, theme: GraphTheme,
        drawn: Set<String>, in ctx: CGContext
    ) {
        let indent = 12 + CGFloat(row.depth) * 13

        // The disclosure mark, in the gap the indent already leaves to its left. A
        // row with nothing under it gets none, so the mark means "there is more here"
        // rather than decorating every line.
        if row.expandable {
            chevron(pointingRight: row.folded,
                    in: CGRect(x: row.discloseLeft(from: rect.minX),
                               y: rect.minY, width: CardGeometry.discloseWidth,
                               height: rect.height),
                    color: theme.ink2, in: ctx)
        }

        // Depth rails — the vertical rules that show nesting without spending a
        // disclosure triangle per level.
        if row.depth > 0 {
            ctx.setStrokeColor(theme.teal.withAlphaComponent(0.22).cgColor)
            ctx.setLineWidth(1)
            for d in 1...row.depth {
                let x = 12 + CGFloat(d - 1) * 13 + 4.5
                ctx.move(to: CGPoint(x: x, y: rect.minY))
                ctx.addLine(to: CGPoint(x: x, y: rect.maxY))
            }
            ctx.strokePath()
        }

        if let datum = row.datum {
            drawDatum(datum, rect: rect, indent: indent, theme: theme, ctx: ctx)
            return
        }

        if let symbol = row.symbol {
            drawSymbol(symbol, rect: rect, indent: indent, theme: theme, ctx: ctx)
            return
        }

        if let value = row.value {
            text(value.value,
                 in: CGRect(x: rect.minX + indent, y: rect.minY,
                            width: rect.width * 0.5, height: rect.height),
                 font: .monospacedSystemFont(ofSize: 11.5, weight: .regular),
                 color: theme.ink, centeredVertically: true)
            if !value.label.isEmpty {
                text(value.label,
                     in: CGRect(x: rect.minX + rect.width * 0.5 + 8, y: rect.minY,
                                width: rect.width * 0.5 - 20, height: rect.height),
                     font: .systemFont(ofSize: 11), color: theme.ink3, centeredVertically: true)
            }
            return
        }

        if let form = row.form, let step = row.step {
            var indent = indent
            // Its order (a goal's row), before what it says.
            if let number = row.number {
                indent = drawNumber(number, x: rect.minX + indent, midY: rect.midY, theme: theme,
                                    accent: theme.accent(for: schema.kind), ctx: ctx) + 6 - rect.minX
            }
            drawForm(form, step: step, schema: schema, rect: rect, indent: indent, theme: theme, ctx: ctx)
            return
        }

        if let step = row.step {
            // The word first, in the card's colour, then what it is done to; how it
            // travels on the right. The word is the part to scan down the card for.
            let wordFont = NSFont.monospacedSystemFont(ofSize: 10.5, weight: .semibold)
            let word = step.label
            let wordWidth = max(46, (word as NSString).size(withAttributes: [.font: wordFont]).width)
            let detail = step.detail ?? ""
            let detailFont = NSFont.systemFont(ofSize: 10)
            // At most half the row, so what the row does is never written over.
            let detailWidth = detail.isEmpty ? 0
                : min((detail as NSString).size(withAttributes: [.font: detailFont]).width, rect.width * 0.45)
            text(word,
                 in: CGRect(x: rect.minX + indent, y: rect.minY, width: wordWidth, height: rect.height),
                 font: wordFont, color: theme.accent(for: schema.kind), centeredVertically: true)
            text(step.text,
                 in: CGRect(x: rect.minX + indent + wordWidth + 8, y: rect.minY,
                            width: max(10, rect.width - indent - wordWidth - detailWidth - 36),
                            height: rect.height),
                 font: .monospacedSystemFont(ofSize: 11, weight: .regular),
                 color: theme.ink, centeredVertically: true)
            if !detail.isEmpty {
                text(detail,
                     in: CGRect(x: rect.maxX - detailWidth - 12, y: rect.minY,
                                width: detailWidth, height: rect.height),
                     font: detailFont, color: theme.ink3, centeredVertically: true)
            }
            return
        }

        guard let field = row.field else { return }

        let isRef = field.type.isRef
        let isNested = field.type.isObject
        // Nesting is teal, references are brass. Choosing between the two *is* the
        // design decision, so they must never be mistaken for one another.
        let nameColor: NSColor = row.fromType != nil ? theme.ink3
            : isRef ? theme.brassInk : isNested ? theme.teal : theme.ink

        let nameFont = NSFont.monospacedSystemFont(ofSize: 11.5, weight: isRef ? .medium : .regular)
        let name = field.name + (field.isArray ? "[]" : "")
        // The trailing text first, because it is what is left over that the name gets.
        // A reference names its target, so with a fixed width reserved for it a long
        // schema name and a long field name would overlap.
        let trailing = trailingText(row, schema: schema, drawn: drawn)
        let typeFont = NSFont.monospacedSystemFont(ofSize: 10, weight: .regular)
        let typeWidth = (trailing as NSString).size(withAttributes: [.font: typeFont]).width

        // A primary key says so before its name, with a key: an ID pointing
        // at the card lands here.
        let keyRoom: CGFloat = field.isPk ? 14 : 0
        if field.isPk {
            keyMark(in: CGRect(x: rect.minX + indent, y: rect.midY - 5, width: 10, height: 10), color: theme.brass)
        }
        let nameWidth = min((name as NSString).size(withAttributes: [.font: nameFont]).width,
                            rect.width - indent - keyRoom - typeWidth - 24)

        text(name,
             in: CGRect(x: rect.minX + indent + keyRoom, y: rect.minY,
                        width: max(10, nameWidth), height: rect.height),
             font: nameFont, color: nameColor, centeredVertically: true)

        // A self-reference shows ↺Name here rather than getting a line that loops
        // back to this very card — the loop costs a lot of space to say one thing.
        text(trailing,
             in: CGRect(x: rect.maxX - typeWidth - 12, y: rect.minY,
                        width: typeWidth, height: rect.height),
             font: typeFont,
             color: isRef ? theme.brass : isNested ? theme.teal : theme.ink3,
             centeredVertically: true)
    }

    /// What a row says on its right-hand side.
    ///
    /// A reference names its target, whether or not there is a line to follow. The
    /// arrow alone left the reader to trace a curve across the diagram to find out
    /// what `signing` was — which is the answer the card had in hand.
    static func trailingText(_ row: CardRow, schema: SchemaNode,
                             drawn: Set<String>) -> String {
        guard let field = row.field else { return "" }
        guard field.type.isRef else {
            return field.type.isObject ? "object" : field.type.rawValue
        }
        // A loop back to this very card gets no line at all, and says so.
        if field.embeds == schema.id { return "↺\(schema.name)" }

        let name = row.refTargetName ?? L10n[.Diagram.cardNoRefTarget]
        // No card, no line: the arrow would point at nothing, so it is left off.
        let linked = field.embeds.map { drawn.contains($0) } ?? false
        return linked ? "→\(name)" : name
    }

    /// The same mark the structure pane uses, drawn rather than set in a glyph.
    ///
    /// A `▸` at this size renders as a dot — the two panes are looking at the same
    /// tree and should say "there is more here" the same way. Stroked as a path so
    /// the weight holds at every zoom the card is re-cut at.
    private static func chevron(pointingRight: Bool, in rect: CGRect,
                                color: NSColor, in ctx: CGContext) {
        let arm: CGFloat = 2.6
        let centre = CGPoint(x: rect.midX, y: rect.midY)
        let path = CGMutablePath()
        if pointingRight {
            path.move(to: CGPoint(x: centre.x - arm / 2, y: centre.y - arm))
            path.addLine(to: CGPoint(x: centre.x + arm / 2, y: centre.y))
            path.addLine(to: CGPoint(x: centre.x - arm / 2, y: centre.y + arm))
        } else {
            path.move(to: CGPoint(x: centre.x - arm, y: centre.y - arm / 2))
            path.addLine(to: CGPoint(x: centre.x, y: centre.y + arm / 2))
            path.addLine(to: CGPoint(x: centre.x + arm, y: centre.y - arm / 2))
        }
        ctx.saveGState()
        ctx.addPath(path)
        ctx.setStrokeColor(color.cgColor)
        ctx.setLineWidth(1.3)
        ctx.setLineCap(.round)
        ctx.setLineJoin(.round)
        ctx.strokePath()
        ctx.restoreGState()
    }

    // MARK: - A custom card's form

    /// Where a row's switch is, in the row: at its right end.
    static func toggleRect(in row: CGRect) -> CGRect {
        CGRect(x: row.maxX - 12 - 30, y: row.midY - 8.5, width: 30, height: 17)
    }

    private static let formLabelFont = NSFont.systemFont(ofSize: 10.5, weight: .semibold)

    /// Whether a radio field's options fit side by side in the row, each its word and a little
    /// room (7 points each side of the word, so a word never touches its segment's edge): else it
    /// is drawn, and pressed, as a menu — as `FittingChoice` does in the panel.
    static func segmentsFit(in row: CGRect, label: String, options: [String]) -> Bool {
        let boxes = segmentRects(in: row, label: label, count: options.count)
        let font = NSFont.systemFont(ofSize: 10, weight: .semibold)
        return zip(options, boxes).allSatisfy { option, box in
            (option as NSString).size(withAttributes: [.font: font]).width + 14 <= box.width
        }
    }

    /// Where a menu's button is, in the row (a dropdown list, or a radio whose options do not
    /// fit): after its name, to the row's end.
    static func menuRect(in row: CGRect, label: String) -> CGRect {
        let labelWidth = max(46, (label as NSString).size(withAttributes: [.font: formLabelFont]).width)
        let x = row.minX + 12 + labelWidth + 8
        return CGRect(x: x, y: row.midY - 9.5, width: max(0, row.maxX - 12 - x), height: 19)
    }

    /// Where a radio field's options are, in the row: after its name, side by side.
    static func segmentRects(in row: CGRect, label: String, count: Int) -> [CGRect] {
        guard count > 0 else { return [] }
        let labelWidth = max(46, (label as NSString).size(withAttributes: [.font: formLabelFont]).width)
        let area = CGRect(x: row.minX + 12 + labelWidth + 8, y: row.midY - 9.5,
                          width: max(0, row.maxX - 12 - (row.minX + 12 + labelWidth + 8)), height: 19)
        let each = area.width / CGFloat(count)
        return (0..<count).map { CGRect(x: area.minX + CGFloat($0) * each, y: area.minY, width: each, height: area.height) }
    }

    private static func drawForm(_ form: CardForm.Control, step: StepRow, schema: SchemaNode, rect: CGRect,
                                 indent: CGFloat, theme: GraphTheme, ctx: CGContext) {
        let accent = theme.accent(for: schema.kind)
        switch form {
        case .toggle(let on):
            let toggle = toggleRect(in: rect)
            let detailFont = NSFont.systemFont(ofSize: 10)
            let detail = step.detail ?? ""
            let detailWidth = detail.isEmpty ? 0 : (detail as NSString).size(withAttributes: [.font: detailFont]).width
            text(step.text,
                 in: CGRect(x: rect.minX + indent, y: rect.minY,
                            width: max(0, toggle.minX - 8 - detailWidth - (detail.isEmpty ? 0 : 6) - (rect.minX + indent)),
                            height: rect.height),
                 font: .systemFont(ofSize: 11.5), color: on ? theme.ink3 : theme.ink, centeredVertically: true)
            if !detail.isEmpty {
                text(detail, in: CGRect(x: toggle.minX - 8 - detailWidth, y: rect.minY, width: detailWidth, height: rect.height),
                     font: detailFont, color: theme.ink3, centeredVertically: true)
            }
            // A switch as the Mac draws one: a track, filled when on, and a knob at that end.
            let track = CGPath(roundedRect: toggle, cornerWidth: toggle.height / 2, cornerHeight: toggle.height / 2, transform: nil)
            ctx.addPath(track)
            ctx.setFillColor((on ? accent : theme.line2).cgColor)
            ctx.fillPath()
            let knob = CGRect(x: on ? toggle.maxX - toggle.height + 1.5 : toggle.minX + 1.5, y: toggle.minY + 1.5,
                              width: toggle.height - 3, height: toggle.height - 3)
            ctx.setFillColor(NSColor.white.cgColor)
            ctx.fillEllipse(in: knob)
            ctx.setStrokeColor(NSColor.black.withAlphaComponent(0.12).cgColor)
            ctx.setLineWidth(0.5)
            ctx.strokeEllipse(in: knob)
        case .segments(let field, let options, let chosen, let labels):
            // Said by its label (状態), written by its name (status); each value as it is shown.
            let label = step.label.isEmpty ? field : step.label
            guard segmentsFit(in: rect, label: label, options: options.map { CardForm.Control.shown($0, labels) }) else {
                drawMenu(label: label, chosen: CardForm.Control.shown(chosen, labels), rect: rect, theme: theme,
                         accent: accent, ctx: ctx)
                return
            }
            text(label, in: CGRect(x: rect.minX + 12, y: rect.minY, width: 60, height: rect.height),
                 font: formLabelFont, color: accent, centeredVertically: true)
            let segments = segmentRects(in: rect, label: label, count: options.count)
            guard let first = segments.first, let last = segments.last else { return }
            let whole = CGRect(x: first.minX, y: first.minY, width: last.maxX - first.minX, height: first.height)
            ctx.addPath(CGPath(roundedRect: whole, cornerWidth: 5, cornerHeight: 5, transform: nil))
            ctx.setFillColor(theme.panel2.cgColor)
            ctx.fillPath()
            for (option, box) in zip(options, segments) {
                let picked = option == chosen
                if picked {
                    ctx.addPath(CGPath(roundedRect: box.insetBy(dx: 1.5, dy: 1.5), cornerWidth: 4, cornerHeight: 4, transform: nil))
                    ctx.setFillColor(accent.cgColor)
                    ctx.fillPath()
                }
                let font = NSFont.systemFont(ofSize: 10, weight: picked ? .semibold : .regular)
                let shown = CardForm.Control.shown(option, labels)
                let width = (shown as NSString).size(withAttributes: [.font: font]).width
                text(shown, in: CGRect(x: box.midX - min(width, box.width - 4) / 2, y: box.minY,
                                        width: min(width, box.width - 4), height: box.height),
                     font: font, color: picked ? .white : theme.ink2, centeredVertically: true)
            }
            ctx.addPath(CGPath(roundedRect: whole, cornerWidth: 5, cornerHeight: 5, transform: nil))
            ctx.setStrokeColor(theme.line2.cgColor)
            ctx.setLineWidth(0.75)
            ctx.strokePath()
        case .menu(let field, _, let chosen, let labels):
            drawMenu(label: step.label.isEmpty ? field : step.label, chosen: CardForm.Control.shown(chosen, labels),
                     rect: rect, theme: theme, accent: accent, ctx: ctx)
        }
    }

    /// A menu's button in a row: its name, then the one chosen (`—` for none, as `FittingChoice`
    /// says it) cut short where it is long, and the mark that it opens.
    private static func drawMenu(label: String, chosen: String, rect: CGRect, theme: GraphTheme, accent: NSColor,
                                 ctx: CGContext) {
        text(label, in: CGRect(x: rect.minX + 12, y: rect.minY, width: 60, height: rect.height),
             font: formLabelFont, color: accent, centeredVertically: true)
        let button = menuRect(in: rect, label: label)
        ctx.addPath(CGPath(roundedRect: button, cornerWidth: 5, cornerHeight: 5, transform: nil))
        ctx.setFillColor(theme.panel2.cgColor)
        ctx.fillPath()
        ctx.addPath(CGPath(roundedRect: button, cornerWidth: 5, cornerHeight: 5, transform: nil))
        ctx.setStrokeColor(theme.line2.cgColor)
        ctx.setLineWidth(0.75)
        ctx.strokePath()
        let word = chosen.isEmpty ? FittingChoice.Option.none.text : chosen
        text(word, in: CGRect(x: button.minX + 8, y: button.minY, width: max(0, button.width - 26), height: button.height),
             font: .systemFont(ofSize: 10.5), color: chosen.isEmpty ? theme.ink3 : theme.ink, centeredVertically: true)
        // The mark that it opens: a small chevron down, at its end.
        let arm: CGFloat = 3
        let centre = CGPoint(x: button.maxX - 10, y: button.midY)
        let path = CGMutablePath()
        path.move(to: CGPoint(x: centre.x - arm, y: centre.y - arm / 2))
        path.addLine(to: CGPoint(x: centre.x, y: centre.y + arm / 2))
        path.addLine(to: CGPoint(x: centre.x + arm, y: centre.y - arm / 2))
        ctx.saveGState()
        ctx.addPath(path)
        ctx.setStrokeColor(theme.ink2.cgColor)
        ctx.setLineWidth(1.2)
        ctx.setLineCap(.round)
        ctx.strokePath()
        ctx.restoreGState()
    }

    // MARK: - Text

    /// The key a primary key's row carries before its name: the system's key, in `color`.
    static func keyMark(in rect: CGRect, color: NSColor) {
        let config = NSImage.SymbolConfiguration(pointSize: rect.height, weight: .semibold)
            .applying(NSImage.SymbolConfiguration(paletteColors: [color]))
        guard let image = NSImage(systemSymbolName: "key.fill", accessibilityDescription: L10n[.Diagram.cardPrimaryKey])?
            .withSymbolConfiguration(config) else { return }
        image.draw(in: rect, from: .zero, operation: .sourceOver, fraction: 1, respectFlipped: true, hints: nil)
    }

    private static func text(
        _ string: String, in rect: CGRect, font: NSFont, color: NSColor,
        centeredVertically: Bool = false, struckThrough: Bool = false
    ) {
        let style = NSMutableParagraphStyle()
        style.lineBreakMode = .byTruncatingTail
        var attributes: [NSAttributedString.Key: Any] = [
            .font: font, .foregroundColor: color, .paragraphStyle: style,
        ]
        if struckThrough {
            attributes[.strikethroughStyle] = NSUnderlineStyle.single.rawValue
            attributes[.strikethroughColor] = color
        }
        var box = rect
        if centeredVertically {
            let height = (string as NSString).size(withAttributes: attributes).height
            box = CGRect(x: rect.minX, y: rect.midY - height / 2,
                         width: rect.width, height: height)
        }
        (string as NSString).draw(in: box, withAttributes: attributes)
    }
}
