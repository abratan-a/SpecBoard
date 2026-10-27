import SwiftUI

/// The definition being written, drawn as it would be once saved (`Docs/Features/Project:ProjectKinds` preview):
/// a kind of card's card — one of the tree's cards of the kind, or one of values made up for it — as the section
/// map draws it, the field or kind of row picked in the form outlined, and with the look open, a card of it as
/// one row of a container; a kind of section as the navigator shows it and what making a card or a section in
/// it offers; a kind of line between two made-up cards as the map draws it, and how the map's line switches say
/// it. The core reads the draft (`kind_preview`); nothing is written. While the draft cannot be read, the last
/// drawing stays, faint.
struct KindPreviewView: View {
    var list: AppModel.KindList
    /// What the core last read of the draft, and whether the draft now is unreadable.
    var preview: AppModel.KindPreview?
    var faded: Bool
    var draft: KindDraft?
    var selection: KindDraft.Place?
    /// Whether a card of the kind is drawn as a container's row too (the look is open).
    var containerRow: Bool
    /// The tree's cards of the kind, by id, each with its path.
    var samples: [(id: String, path: String)]
    @Binding var sample: String?
    /// The project's kinds of card, to read a container's row by.
    var kinds: CardKinds
    /// Whether the column's own title and sample menu are drawn (not when it is a group of the form).
    var titled = true
    /// The page the map makes of a card's Markdown (`AppModel.documentPage`): what a real card's body is drawn as.
    var makePage: ((DocumentParts.Source) async -> DocumentPage?)? = nil

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            if titled {
                HStack {
                    Text(L10n[.Kinds.formGroupPreview]).font(.app(11, weight: .semibold)).foregroundStyle(.secondary)
                    Spacer()
                    if list == .cards { sampleMenu }
                }
            } else if list == .cards {
                sampleMenu
            }
            Group {
                switch list {
                case .cards: cardPreview
                case .sections: sectionPreview
                case .lines: linePreview
                }
            }
            .opacity(faded ? 0.4 : 1)
            if faded {
                Text(L10n[.Kinds.previewStale]).font(.app(10.5)).foregroundStyle(.orange)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }

    /// The name of the sample drawn, as the preview's folded group says it.
    var sampleSaid: String {
        guard list == .cards else { return draft?.name ?? "" }
        return samples.first { $0.id == sample }?.path ?? L10n[.Kinds.previewSampleValues]
    }

    private var sampleMenu: some View {
        Menu {
            Button(L10n[.Kinds.previewSampleValues]) { sample = nil }
            if !samples.isEmpty {
                Divider()
                ForEach(samples, id: \.id) { item in Button(item.path) { sample = item.id } }
            }
        } label: {
            Text(sampleSaid).font(.app(11)).lineLimit(1).truncationMode(.middle)
        }
        .fixedSize(horizontal: false, vertical: true)
        .frame(maxWidth: 200)
        .disabled(samples.isEmpty)
    }

    private var kindName: String { draft?.name ?? preview?.cardKind?.kind ?? "" }

    // MARK: - A kind of card

    @ViewBuilder
    private var cardPreview: some View {
        if let card = preview?.card {
            KindCardPicture(schema: card, outlined: outlined(card), header: outlinesHeader,
                            makePage: preview?.made == false ? makePage : nil)
            Text(preview?.made == false ? L10n[.Kinds.previewCaptionReal(kindName)] : L10n[.Kinds.previewCaptionMade(kindName, kindName)])
                .font(.app(10.5)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
            if containerRow, let container = preview?.inContainer(kinds: kinds, named: L10n[.Kinds.previewContainerName]) {
                KindCardPicture(schema: container, outlined: [], header: false).padding(.top, 6)
                Text(L10n[.Kinds.previewContainer]).font(.app(10.5)).foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
        } else {
            Text(L10n[.Kinds.previewNothing]).font(.app(11)).foregroundStyle(.secondary)
        }
    }

    /// Whether the field picked is one the card says on its header (`header`).
    private var outlinesHeader: Bool {
        guard let selection, selection.steps.count == 1, selection.isField, let name = selection.name else { return false }
        return preview?.cardKind?.header.contains(name) == true
    }

    /// The rows of the card to outline: the field picked, or every row of the kind of row picked (a field of
    /// one picked outlines its rows).
    private func outlined(_ card: SchemaNode) -> Set<String> {
        guard let selection, let first = selection.steps.first else { return [] }
        switch first {
        case .field(let name):
            return [OwnRows.fieldRow + name]
        case .row(let name):
            guard let def = card.kindDef else { return [] }
            return Set(OwnRows.walk(def, card.ownRows).filter { $0.places.first?.kind == name }.map(\.path))
        }
    }

    // MARK: - A kind of section

    @ViewBuilder
    private var sectionPreview: some View {
        let name = draft?.name ?? ""
        let label = draft?.string(at: .kind, "label") ?? name
        let mark = KindsEditing.mark(in: draft?.text ?? "")
        let tint = mark.color.flatMap(HexColor.color(hex:)) ?? .gray
        let holds = draft?.strings(at: .kind, "holds") ?? []
        let inside = draft.map { d in d.strings(at: .kind, "sections").isEmpty ? [d.name] : d.strings(at: .kind, "sections") } ?? []
        VStack(alignment: .leading, spacing: 6) {
            Text(L10n[.Kinds.previewInNavigator]).font(.app(10.5)).foregroundStyle(.secondary)
            HStack(spacing: 6) {
                Image(systemName: "chevron.down").font(.app(8)).foregroundStyle(.secondary)
                Image(systemName: mark.symbol).foregroundStyle(tint)
                Text(label.isEmpty ? name : label).font(.app(12))
                Spacer()
            }
            .padding(6)
            .background(Color.secondary.opacity(0.08), in: RoundedRectangle(cornerRadius: 5))
            Text(L10n[.Kinds.previewNewCardOffers]).font(.app(10.5)).foregroundStyle(.secondary).padding(.top, 4)
            VStack(alignment: .leading, spacing: 3) {
                ForEach(holds, id: \.self) { kind in
                    HStack(spacing: 8) {
                        Text(kind == "*" ? L10n[.Kinds.formAnyKind] : kind).font(.app(11.5, design: .monospaced))
                        if kind != "*", let said = kinds[kind]?.label, said != kind {
                            Text(said).font(.app(11)).foregroundStyle(.secondary)
                        }
                    }
                }
                if holds.isEmpty { Text(L10n[.Common.wordNone]).font(.app(11)).foregroundStyle(.secondary) }
            }
            .padding(8)
            .frame(maxWidth: .infinity, alignment: .leading)
            .overlay(RoundedRectangle(cornerRadius: 5).stroke(.quaternary))
            Text(L10n[.Kinds.previewNewSectionMakes]).font(.app(10.5)).foregroundStyle(.secondary).padding(.top, 4)
            FlowRow(spacing: 6, lineSpacing: 4) {
                ForEach(inside, id: \.self) { KindTag(text: $0) }
            }
        }
    }

    // MARK: - A kind of line

    private var lineKind: LineKind? {
        if let kind = preview?.lineKind { return kind }
        guard let draft, let read = try? JSONDecoder().decode(LineKind.self, from: Data(OJSON.object(draft.object).compact().utf8))
        else { return nil }
        return read
    }

    /// The kinds a made-up line runs between: the first a start names, and the first its end names.
    private func ends(_ draft: KindDraft?) -> (from: String, fromRow: Bool, to: String, toRow: Bool) {
        func end(_ value: OJSON?) -> (String, Bool) {
            let o = value?.objectOrNil
            let kind = o?["kinds"]?.stringsOrNil?.first ?? "*"
            let at = o?["at"]?.stringOrNil.map { [$0] } ?? o?["at"]?.stringsOrNil ?? []
            return (kind == "*" ? L10n[.Kinds.formAnyCard] : kind, at.contains("row") && !at.contains("card"))
        }
        let from: OJSON? = if case .array(let items)? = draft?.value(at: .kind, "from") { items.first } else { nil }
        let (a, aRow) = end(from)
        let (b, bRow) = end(draft?.value(at: .kind, "to"))
        return (a, aRow, b, bRow)
    }

    @ViewBuilder
    private var linePreview: some View {
        if let kind = lineKind {
            let between = ends(draft)
            KindLinePicture(look: kind.look, from: between.from, fromRow: between.fromRow, to: between.to, toRow: between.toRow)
            Text(L10n[.Kinds.previewLineCaption(between.from, between.to)]).font(.app(10.5)).foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            HStack(spacing: 8) {
                Text(L10n[.Kinds.previewLineSwitches]).font(.app(10.5)).foregroundStyle(.secondary)
                LineSample(look: kind.look)
                Text(kind.label).font(.app(11))
                if !kind.visible { Text(L10n[.Kinds.previewOff]).font(.app(10.5)).foregroundStyle(.secondary) }
                Spacer()
            }
            .padding(6)
            .overlay(RoundedRectangle(cornerRadius: 5).stroke(.quaternary))
            if !kind.visible {
                Text(L10n[.Kinds.previewOffCaption]).font(.app(10.5)).foregroundStyle(.secondary)
            }
        } else {
            Text(L10n[.Kinds.previewNothing]).font(.app(11)).foregroundStyle(.secondary)
        }
    }
}

/// A card drawn as the section map draws it (`CardRenderer`), fitted to the room, the rows named outlined; under
/// the fields, its face's Markdown as the map draws it — the page made of it as the map makes one (`makePage`),
/// then a picture of that taken out of sight (`DocSnapshots`), the card drawn again once it is — up to `bodyMost`
/// of it. Neither is waited for inside what makes it: each only tells the view to draw again.
struct KindCardPicture: View {
    var schema: SchemaNode
    var outlined: Set<String>
    /// Whether the header is outlined (a field it says is picked).
    var header: Bool

    @Environment(\.colorScheme) private var colorScheme
    /// Pictures of the Markdown taken since: read so the card is drawn again when one is.
    @State private var taken = 0

    /// The most of the Markdown the preview shows, past which the card says there is more, as the map's does.
    static let bodyMost: CGFloat = 240
    /// The room the Markdown has while its picture is being taken: the word that says so.
    static let bodyWhileDrawing: CGFloat = 28

    /// What the card is drawn as: its rows, its height, and under the rows the Markdown's picture (`body`; nil
    /// while it is not taken) when its face draws Markdown that says anything.
    struct Drawn {
        var content: CardContent
        var height: CGFloat
        var document: CardRenderer.DocumentBody?
    }

    /// How wide the Markdown is drawn: the area under a card's fields.
    static var bodyWidth: CGFloat { CardRenderer.documentArea(CGSize(width: CardGeometry.cardWidth, height: 0)).width }

    static func drawn(_ schema: SchemaNode, body: DocSnapshots.Picture?) -> Drawn {
        guard schema.faceDrawsMarkdown else {
            let content = cardRows(schema, inlineReuse: true)
            return Drawn(content: content, height: cardHeight(content), document: nil)
        }
        let content = cardRows(schema, inlineReuse: true, form: CardForm(document: true))
        let width = CardGeometry.cardWidth
        let top = CardRenderer.documentArea(CGSize(width: width, height: 0), rows: content.rows.count).minY
        let height = top + (body.map { min($0.height, bodyMost) } ?? bodyWhileDrawing) + 14
        let area = CardRenderer.documentArea(CGSize(width: width, height: height), rows: content.rows.count)
        let document = CardRenderer.DocumentBody(image: body?.image,
                                                 size: CGSize(width: area.width, height: body?.height ?? area.height),
                                                 cut: body?.cut ?? false, picture: body?.number ?? 0)
        return Drawn(content: content, height: height, document: document)
    }

    /// The page the map makes of the Markdown, its `[[…]]`, `{{…}}` and `![[…]]` read by the core from the card
    /// (`AppModel.documentPage`); none for made-up values, which are no card of the tree: drawn as written.
    var makePage: ((DocumentParts.Source) async -> DocumentPage?)? = nil
    /// The page made of the Markdown it was made of.
    @State private var made: (text: String, page: DocumentPage)?

    /// The page the Markdown is drawn as: as written when no page is made of it, the made one once it is.
    private func page(_ markdown: String) -> DocumentPage? {
        guard makePage != nil else { return DocumentPage(markdown) }
        return made?.text == markdown ? made?.page : nil
    }

    var body: some View {
        let theme: GraphTheme = colorScheme == .dark ? .dark : .light
        let _ = taken
        let markdown = schema.faceDrawsMarkdown ? schema.faceMarkdown : nil
        let picture = markdown.flatMap(page).flatMap {
            DocSnapshots.shared.picture($0, width: Self.bodyWidth, dark: theme.isDark, scale: 2) { taken &+= 1 }
        }
        let drawn = Self.drawn(schema, body: picture)
        let content = drawn.content
        let width = CardGeometry.cardWidth
        Group {
            if let image = CardRenderer.image(for: schema, content: content, theme: theme, scale: 2, selected: false,
                                              width: width, height: drawn.height, document: drawn.document) {
                cardImage(image, content: content, theme: theme)
            }
        }
        .task(id: markdown) {
            guard let markdown, let makePage else { return }
            let source = DocumentParts.Source(card: schema.id, text: markdown)
            made = (markdown, await makePage(source) ?? DocumentPage(markdown))
        }
    }

    private func cardImage(_ image: CGImage, content: CardContent, theme: GraphTheme) -> some View {
        let width = CardGeometry.cardWidth
        return Image(decorative: image, scale: 2)
            .resizable()
            .aspectRatio(contentMode: .fit)
            .frame(maxWidth: width)
            .overlay {
                GeometryReader { geo in
                    let scale = geo.size.width / width
                    ForEach(Array(frames(content).enumerated()), id: \.offset) { _, frame in
                        RoundedRectangle(cornerRadius: 4)
                            .stroke(Color(nsColor: theme.brass), lineWidth: 2)
                            .frame(width: frame.width * scale, height: frame.height * scale)
                            .offset(x: frame.minX * scale, y: frame.minY * scale)
                    }
                }
            }
    }

    /// Where the outlines go, in the card's own points.
    private func frames(_ content: CardContent) -> [CGRect] {
        var out: [CGRect] = []
        let width = CardGeometry.cardWidth
        if header { out.append(CGRect(x: 3, y: 3, width: width - 6, height: CardGeometry.headerHeight - 4)) }
        for (index, row) in content.rows.enumerated() where outlined.contains(row.path) {
            let middle = rowOffset(index, in: content)
            out.append(CGRect(x: 4, y: middle - CardGeometry.rowHeight / 2, width: width - 8, height: CardGeometry.rowHeight))
        }
        return out
    }
}

/// A line of a kind drawn between two made-up cards, from a row or the header of one to the other's, in
/// its colour and dash with the marks at its ends.
struct KindLinePicture: View {
    var look: LineLook
    var from: String
    var fromRow: Bool
    var to: String
    var toRow: Bool

    @Environment(\.colorScheme) private var colorScheme

    var body: some View {
        let theme: GraphTheme = colorScheme == .dark ? .dark : .light
        let ink = Color(nsColor: look.ink.color(dark: theme.isDark))
        GeometryReader { geo in
            let boxWidth = min(120, (geo.size.width - 50) / 2)
            let left = CGRect(x: 0, y: 6, width: boxWidth, height: 76)
            let right = CGRect(x: geo.size.width - boxWidth, y: 30, width: boxWidth, height: 76)
            let start = CGPoint(x: left.maxX, y: fromRow ? left.minY + 50 : left.minY + 12)
            let end = CGPoint(x: right.minX, y: toRow ? right.minY + 50 : right.minY + 12)
            ZStack(alignment: .topLeading) {
                box(from, row: fromRow, theme: theme).frame(width: left.width, height: left.height).offset(x: left.minX, y: left.minY)
                box(to, row: toRow, theme: theme).frame(width: right.width, height: right.height).offset(x: right.minX, y: right.minY)
                Path { p in
                    p.move(to: start)
                    let middle = (start.x + end.x) / 2
                    p.addCurve(to: end, control1: CGPoint(x: middle, y: start.y), control2: CGPoint(x: middle, y: end.y))
                }
                .stroke(ink, style: StrokeStyle(lineWidth: 1.6, dash: look.dash.pattern ?? []))
                cap(look.begin, at: start, pointing: CGPoint(x: -1, y: 0), ink: ink, theme: theme)
                cap(look.end, at: end, pointing: CGPoint(x: 1, y: 0), ink: ink, theme: theme)
            }
        }
        .frame(height: 112)
    }

    private func box(_ kind: String, row: Bool, theme: GraphTheme) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            Text(kind).font(.app(11, weight: .semibold, design: .monospaced)).lineLimit(1)
                .padding(.horizontal, 6).frame(height: 24, alignment: .leading)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(Color(nsColor: theme.panel2))
            Rectangle().fill(Color(nsColor: theme.line)).frame(height: 1)
            VStack(alignment: .leading, spacing: 0) {
                Text(verbatim: "1").font(.app(10.5)).foregroundStyle(.secondary).frame(height: 24)
                Text(verbatim: "2").font(.app(10.5)).foregroundStyle(.secondary).frame(height: 24)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(row ? Color(nsColor: theme.brassSoft) : Color.clear)
            }
            .padding(.horizontal, 6)
        }
        .background(Color(nsColor: theme.panel), in: RoundedRectangle(cornerRadius: 6))
        .overlay(RoundedRectangle(cornerRadius: 6).stroke(Color(nsColor: theme.line2)))
    }

    /// The mark at one end, drawn pointing out of the line (`pointing` +x at the end, −x at the start).
    @ViewBuilder
    private func cap(_ cap: EdgeCap, at point: CGPoint, pointing: CGPoint, ink: Color, theme: GraphTheme) -> some View {
        if let path = cap.path(size: 9) {
            // The cap is drawn from its tip at the origin along +x: turned so it points back along the line.
            let turn = pointing.x > 0 ? CGAffineTransform(scaleX: -1, y: 1) : .identity
            let placed = Path(path).applying(turn).applying(CGAffineTransform(translationX: point.x, y: point.y))
            if cap.isClosed {
                placed.fill(cap.isFilled ? ink : Color(nsColor: theme.background)).overlay(placed.stroke(ink, lineWidth: 1.2))
            } else {
                placed.stroke(ink, lineWidth: 1.4)
            }
        }
    }
}

extension AppModel.KindPreview {
    /// The card drawn as the one row of a made-up container (a goal) holding it, as its kind's `as_row`
    /// says it: read by the project's kinds of card `kinds` with the draft in place of its name's.
    func inContainer(kinds: CardKinds, named: String) -> SchemaNode? {
        guard case .object(var child)? = raw, let own = cardKind else { return nil }
        let id = "kind_preview:container"
        let goal = OrderedObject([("id", .string(id)), ("projectId", child["projectId"] ?? .string("")),
                                  ("name", .string(named)), ("kind", .string("goal")), ("label", .string(""))])
        child["in"] = .string(id)
        func decoded(_ object: OrderedObject) -> WholeCard? {
            try? JSONDecoder().decode(WholeCard.self, from: Data(OJSON.object(object).compact().utf8))
        }
        guard let container = decoded(goal), let held = decoded(child) else { return nil }
        var all = kinds.list.filter { $0.kind != own.kind }
        all.append(own)
        return WholeCard.shown([container, held], kinds: CardKinds(all)).first { $0.id == id }
    }
}
