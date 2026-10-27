import AppKit
import SwiftUI

/// The line at the top of every card editor (`NoteEditor`, `CardEditor`, `FormatDocEditor`,
/// and a section card's `CardMapBar`): the card's kind, its name and label, what its kind
/// says beside them (`leading`), and on the right the card's tools (`tools`).
///
/// One line however narrow the editor is, with the name always readable (in a 1024pt window with
/// both side panels open, a name stood one letter to a line; and then, kept to a line, the badge
/// and the tools took the whole width and the name went to nothing). Where the room runs out, the
/// other parts give way first, in this order:
/// 1. the label is cut short — and left out when not even a few letters of it fit (never an
///    ellipsis alone);
/// 2. the badge says the kind's name alone (`freedoc（自由文書）` → `freedoc`);
/// 3. the tools go compact (`cardBarCompact`): marks without words, a switch between ways to
///    look as marks (`BarLabel`), and what goes together in one menu (`BarMenu`, `CardTools`);
/// 4. the name is cut short, down to its first few letters and an ellipsis — no further.
/// At 384pt across (`PaneSplit.middleLeast`: the least window, `WindowLeast`, less both side panels
/// as they start and their strips) every card editor's bar fits;
/// narrower still, it runs off on the right and the name stays in sight.
///
/// A stack hands its width out from the highest layout priority down, keeping back what the
/// lower ones need at the least; so each part's priority is the reverse of the order above,
/// and the badge, the label and the tools each take the fullest form that fits what they are
/// handed (`ViewThatFits`). The gaps go with the parts, so a part left out leaves none.
struct CardBar<Leading: View, Tools: View>: View {
    var kind: SchemaKind
    /// The kind as the badge says it while there is room; the kind's name when none. Left out, a
    /// built-in kind's as its definition says it (`BuiltinWords.badge`).
    var badge: String? = nil
    var name: String
    var label: String
    /// Under another card editor's bar, which already says what the card is: the tools alone.
    var titled = true
    @ViewBuilder var leading: Leading
    @ViewBuilder var tools: Tools

    private static var gap: CGFloat { 8 }

    var body: some View {
        HStack(spacing: 0) {
            if titled {
                let said = badge ?? BuiltinWords.badge(kind.rawValue)
                ViewThatFits(in: .horizontal) {
                    KindBadge(kind: kind, text: said)
                    if said != kind.rawValue { KindBadge(kind: kind, text: nil) }
                }
                .layoutPriority(2)
                Text(name)
                    .font(CardBarFit.nameFont)
                    .lineLimit(1).truncationMode(.tail)
                    .measured(.name)
                    .frame(minWidth: CardBarFit.nameMinWidth(name), alignment: .leading)
                    .help(name)
                    .padding(.leading, Self.gap)
                    .layoutPriority(4)
                if !label.isEmpty {
                    // Its ideal width is the least it is shown at: what fits that, or nothing.
                    ViewThatFits(in: .horizontal) {
                        let least = CardBarFit.labelMinWidth(label)
                        Text(label).font(CardBarFit.labelFont).foregroundStyle(.secondary)
                            .lineLimit(1).truncationMode(.tail)
                            .measured(.label)
                            .frame(minWidth: least, idealWidth: least, alignment: .leading)
                            .help(label)
                            .padding(.leading, Self.gap)
                        Color.clear.frame(width: 0, height: 0)
                    }
                    .layoutPriority(1)
                }
            }
            if Leading.self != EmptyView.self {
                leading.fixedSize().padding(.leading, titled ? Self.gap : 0)
            }
            Spacer(minLength: Self.gap * 2)
            ViewThatFits(in: .horizontal) {
                HStack(spacing: Self.gap) { tools }
                HStack(spacing: Self.gap) { tools }
                    .labelStyle(.iconOnly)
                    .environment(\.cardBarCompact, true)
            }
            .layoutPriority(3)
        }
        .measured(.bar)
        // Run off on the right when even the least of it does not fit: the name stays in sight.
        .frame(maxWidth: .infinity, alignment: .leading)
        .clipShape(SidesOnly())
    }
}

/// The line below a card editor (a flow's legend, a goal's card map, a formatdoc's adding):
/// what it holds on the left — the legend, buttons — and on the right how it is used.
///
/// Never a column a letter or two wide (as the bar above was): the items
/// keep their words on one line each. Where the room runs out: the hint goes under an (i), its
/// words on the pointer; then the items go compact, as the bar's tools do (`cardBarCompact`);
/// then they wrap a whole item at a time onto the next line (`FlowRow`).
struct CardFoot<Items: View>: View {
    var hint: String
    @ViewBuilder var items: Items

    var body: some View {
        ViewThatFits(in: .horizontal) {
            HStack(spacing: 14) {
                HStack(spacing: 14) { items }.fixedSize()
                Spacer(minLength: 14)
                Text(hint).font(.app(10.5)).foregroundStyle(.tertiary).fixedSize()
            }
            HStack(spacing: 14) {
                HStack(spacing: 14) { items }.fixedSize()
                Spacer(minLength: 14)
                info
            }
            HStack(spacing: 14) {
                HStack(spacing: 14) { items }.fixedSize()
                    .labelStyle(.iconOnly)
                    .environment(\.cardBarCompact, true)
                Spacer(minLength: 14)
                info
            }
            HStack(alignment: .top, spacing: 14) {
                FlowRow(spacing: 14, lineSpacing: 6) { items }
                    .labelStyle(.iconOnly)
                    .environment(\.cardBarCompact, true)
                    .frame(maxWidth: .infinity, alignment: .leading)
                info
            }
        }
        .measured(.foot)
    }

    /// The hint, where there is no room for its words.
    private var info: some View {
        Image(systemName: "info.circle")
            .font(.app(11))
            .foregroundStyle(.tertiary)
            .help(hint)
            .accessibilityLabel(hint)
    }
}

/// The least a card editor's bar shows of a name and of a label (`CardBar`).
enum CardBarFit {
    static var nameFont: Font { .app(14, weight: .semibold, design: .monospaced) }
    static var labelFont: Font { .app(12) }

    /// The least a name is shown at: its first four letters and an ellipsis, or all of it when
    /// it is no longer than that.
    static func nameMinWidth(_ name: String) -> CGFloat {
        let size = 14 * Typography.shared.scale
        return width(least(name, letters: 4), Typeface.mono(size, .semibold))
    }

    /// The least a label is shown at: its first three letters and an ellipsis, or all of it.
    static func labelMinWidth(_ label: String) -> CGFloat {
        width(least(label, letters: 3), Typeface.system(12 * Typography.shared.scale))
    }

    /// The first letters of a text and an ellipsis — the text itself when that is no shorter.
    static func least(_ text: String, letters: Int) -> String {
        text.count <= letters + 1 ? text : String(text.prefix(letters)) + "…"
    }

    private static func width(_ text: String, _ font: NSFont) -> CGFloat {
        // A little over what the text measures: a frame a hair too narrow cuts one more letter.
        ceil((text as NSString).size(withAttributes: [.font: font]).width) + 2
    }
}

extension EnvironmentValues {
    /// Inside a card editor's bar (or the line below it) too narrow for its tools' words: the
    /// tools go compact (`CardBar`, `CardFoot`, `BarLabel`, `BarMenu`, `CardTools`).
    @Entry var cardBarCompact = false
}

/// A tool's name in a card editor's bar (`CardBar`) — a button's, or one way to look of a
/// switch: its words — with its mark beside them, or not — while there is room, and its mark
/// alone when the bar is narrow.
struct BarLabel: View {
    var title: String
    var systemImage: String
    /// Whether the mark stands beside the words while there is room.
    var markWhenWide = false

    @Environment(\.cardBarCompact) private var compact

    var body: some View {
        if compact {
            Label(title, systemImage: systemImage).labelStyle(.iconOnly)
        } else if markWhenWide {
            Label(title, systemImage: systemImage)
        } else {
            Text(title)
        }
    }
}

/// Tools that go together, in a card editor's bar (`CardBar`): side by side while there is
/// room (`wide`), and one menu under a mark when the bar is narrow (`items`).
struct BarMenu<Wide: View, Items: View>: View {
    /// What the menu is, on the pointer.
    var title: String
    var systemImage: String
    @ViewBuilder var wide: Wide
    @ViewBuilder var items: Items

    @Environment(\.cardBarCompact) private var compact

    var body: some View {
        if compact {
            Menu { items } label: { Image(systemName: systemImage) }
                .menuStyle(.borderlessButton)
                .fixedSize()
                .help(title)
        } else {
            HStack(spacing: 8) { wide }
        }
    }
}

/// The parts of a card editor's bars, as they are measured (`CardBarSizes`): the bar above,
/// the name and the label in it, and the line below (`CardFoot`).
nonisolated enum CardBarPart: Hashable, Sendable { case bar, name, label, foot }

/// The sizes a card editor's bars were laid out at, by part: for a test to read with
/// `onPreferenceChange`, as nothing in the window needs them.
struct CardBarSizes: PreferenceKey {
    static let defaultValue: [CardBarPart: CGSize] = [:]

    static func reduce(value: inout [CardBarPart: CGSize], nextValue: () -> [CardBarPart: CGSize]) {
        value.merge(nextValue()) { $1 }
    }
}

private extension View {
    /// Its size, said as `part` of a card editor's bars (`CardBarSizes`).
    func measured(_ part: CardBarPart) -> some View {
        background(GeometryReader { proxy in
            Color.clear.preference(key: CardBarSizes.self, value: [part: proxy.size])
        })
    }
}

/// Cuts off what runs past the left and right edges only: above and below, the controls' focus
/// rings stay whole.
private struct SidesOnly: Shape {
    func path(in rect: CGRect) -> Path { Path(rect.insetBy(dx: 0, dy: -12)) }
}
