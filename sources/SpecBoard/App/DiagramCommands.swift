import AppKit
import SwiftUI

/// A diagram the menu bar's zoom and fit act on: a card map or a sequence.
protocol DiagramZoom: AnyObject {
    func zoomIn()
    func zoomOut()
    func actualSize()
    func fit()
}

extension CardCanvasController: DiagramZoom {}

/// What the menu bar can do to the diagram in the front window.
struct DiagramActions {
    /// The side panels, also opened and shut on the window: the nav panel at its button by
    /// the window's own (`NavPanelToggle`), the property panel at its strips.
    var toggleNavPanel: () -> Void
    /// Nil where the property panel does not go: a root's dashboard, the project's settings.
    var toggleProperties: (() -> Void)?
    var zoomIn: () -> Void
    var zoomOut: () -> Void
    var actualSize: () -> Void
    var fit: () -> Void
    /// 「整える」 and 「線に沿って並べ直す」: nil where they cannot run (`AppModel.arrangeActions`).
    var tidy: (() -> Void)?
    var arrangeByLines: (() -> Void)?
    /// Whether ⌘W is the tab bar's: not while the app's settings cover the window.
    var closesTabs = true
    /// The tab in front closed (⌘W); nil on the map tab, which does not close.
    var closeTab: (() -> Void)?
    /// The app's settings closed, back to what was under them (⌘W, as their 「閉じる」 and Esc); nil while they are
    /// not on screen.
    var closeSettings: (() -> Void)? = nil
}

/// What ⌘W does (`DiagramCommands.closeKey`).
enum CloseKey: Equatable {
    case closeTab, closeSettings, closeWindow
}

/// The menus' keys: ⌘ for what is in front of the reader — the diagram's zoom, as a document's — and ⌥⌘ for the
/// app's text size, which is set once and seldom.
enum MenuKeys {
    static let zoomIn = KeyboardShortcut("+", modifiers: .command)
    static let zoomOut = KeyboardShortcut("-", modifiers: .command)
    static let actualSize = KeyboardShortcut("0", modifiers: .command)
    static let fit = KeyboardShortcut("9", modifiers: .command)
    static let textLarger = KeyboardShortcut("+", modifiers: [.command, .option])
    static let textSmaller = KeyboardShortcut("-", modifiers: [.command, .option])
    static let textActual = KeyboardShortcut("0", modifiers: [.command, .option])
}

/// The two ways the map on screen is put in order, each nil where it cannot run now: what the band's 「整列」 and
/// the Edit menu offer.
struct ArrangeActions {
    var tidy: (() -> Void)?
    var arrangeByLines: (() -> Void)?
}

struct DiagramActionsKey: FocusedValueKey {
    typealias Value = DiagramActions
}

extension FocusedValues {
    var diagram: DiagramActions? {
        get { self[DiagramActionsKey.self] }
        set { self[DiagramActionsKey.self] = newValue }
    }
}

/// Zoom and fit in 表示, 整える and 線に沿って並べ直す in 編集 — where a Mac keeps them — rather than
/// in the diagram's bar, which ran out of room for the things used every minute.
///
/// ⌘+ / ⌘- / ⌘0 already belong to the text size in the same menu, so the diagram
/// takes the same keys with ⌥.
struct DiagramCommands: Commands {
    @FocusedValue(\.diagram) private var diagram

    /// ⌘W: the app's settings closed while they cover the window, the tab in front while a project is on screen,
    /// else the window, as everywhere.
    static func closeKey(_ diagram: DiagramActions?) -> CloseKey {
        guard let diagram else { return .closeWindow }
        if diagram.closeSettings != nil { return .closeSettings }
        return diagram.closesTabs ? .closeTab : .closeWindow
    }

    var body: some Commands {
        CommandGroup(after: .toolbar) {
            Button(L10n[.Diagram.commandToggleNavigator]) { diagram?.toggleNavPanel() }
                .keyboardShortcut("s", modifiers: [.command, .control])
                .disabled(diagram == nil)
            Button(L10n[.Diagram.commandToggleProperties]) { diagram?.toggleProperties?() }
                .keyboardShortcut("p", modifiers: [.command, .control])
                .disabled(diagram?.toggleProperties == nil)
            Divider()
            Button(L10n[.Diagram.commandZoomIn]) { diagram?.zoomIn() }
                .keyboardShortcut(MenuKeys.zoomIn)
                .disabled(diagram == nil)
            Button(L10n[.Diagram.commandZoomOut]) { diagram?.zoomOut() }
                .keyboardShortcut(MenuKeys.zoomOut)
                .disabled(diagram == nil)
            Button(L10n[.Diagram.commandActualSize]) { diagram?.actualSize() }
                .keyboardShortcut(MenuKeys.actualSize)
                .disabled(diagram == nil)
            Button(L10n[.Diagram.commandFit]) { diagram?.fit() }
                .keyboardShortcut(MenuKeys.fit)
                .disabled(diagram == nil)
            Divider()
        }
        // ⌘W closes the tab in front while a project is on screen, and never the window then (the red
        // button and ⌘Q do); the app's settings, back to what they cover; over the projects, the window.
        CommandGroup(replacing: .saveItem) {
            switch Self.closeKey(diagram) {
            case .closeSettings:
                Button(L10n[.Diagram.commandCloseSettings]) { diagram?.closeSettings?() }
                    .keyboardShortcut("w", modifiers: .command)
            case .closeTab:
                Button(L10n[.Diagram.commandCloseTab]) { diagram?.closeTab?() }
                    .keyboardShortcut("w", modifiers: .command)
                    .disabled(diagram?.closeTab == nil)
            case .closeWindow:
                Button(L10n[.Diagram.commandCloseWindow]) { NSApp.keyWindow?.performClose(nil) }
                    .keyboardShortcut("w", modifiers: .command)
            }
        }
        CommandGroup(after: .pasteboard) {
            Divider()
            Button(L10n[.Root.arrangeTidy]) { diagram?.tidy?() }
                .disabled(diagram?.tidy == nil)
                .help(L10n[.Root.arrangeTidyHelp])
            Button(L10n[.Root.arrangeByLines]) { diagram?.arrangeByLines?() }
                .disabled(diagram?.arrangeByLines == nil)
                .help(L10n[.Root.arrangeByLinesHelp])
        }
    }
}

/// What the map band says beside 「整列」.
enum MapBand {
    /// 「重なり N か所」 while cards overlap; nothing while none do.
    static func overlapWord(_ count: Int) -> String? {
        count > 0 ? L10n[.Root.overlaps(count)] : nil
    }

    /// A value object or an enum of the section drawn inside the cards that hold it (`withoutValueCards`),
    /// with the fields that hold it, as paths.
    struct Folded: Identifiable, Equatable {
        let id: String
        let name: String
        let holders: [String]
    }

    /// The section's value objects and enums the map draws in no card of their own, by name: those the
    /// kinds switched on let in, not shown on their own (kept or for now). Said on the band, or nothing on
    /// the map would tell they are there but a row's type.
    @MainActor
    static func folded(_ model: AppModel, filters: GraphFilters = GraphFilters()) -> [Folded] {
        guard let detail = model.sectionDetail else { return [] }
        let directory = detail.directory
        let section = model.shownSectionKey
        let drawn = Set(visibleSchemas(detail, filters).map(\.id)).union(model.surroundings.here)
        let byId = Dictionary(detail.schemas.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        var holders: [String: [String]] = [:]
        for line in detail.relations where line.line == Line.embed && !line.from.isEmpty {
            if let holder = byId[line.fromSchemaId] {
                holders[line.toSchemaId, default: []].append("\(directory.full(holder)).\(line.from)")
            }
        }
        return detail.schemas
            .filter { card in
                (card.kind == .value || card.kind == .enum) && card.container == nil && filters.kinds.contains(card.kind)
                    && directory.sectionId(of: card) == section && !drawn.contains(card.id) && !model.isForNow(card.id)
            }
            .map { Folded(id: $0.id, name: $0.name, holders: holders[$0.id] ?? []) }
            .sorted { $0.name.localizedStandardCompare($1.name) == .orderedAscending }
    }

    /// 「N 枚は中に畳んでいます」 while some are; nothing while none are.
    static func foldedWord(_ count: Int) -> String? {
        count > 0 ? L10n[.Root.folded(count)] : nil
    }
}

/// The band's list of the cards folded inside others: each with the fields holding it, a press going there.
struct FoldedList: View {
    var model: AppModel
    var folded: [MapBand.Folded]
    var done: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            ForEach(folded) { card in
                ForEach(card.holders.isEmpty ? [""] : card.holders, id: \.self) { holder in
                    Button {
                        if !holder.isEmpty, model.show(path: holder) { done() }
                    } label: {
                        HStack(spacing: 6) {
                            Text(card.name).font(.app(11.5))
                            if !holder.isEmpty {
                                Text("→ \(holder.split(separator: ":").last.map(String.init) ?? holder)")
                                    .font(.app(11, design: .monospaced)).foregroundStyle(.secondary)
                            }
                            Spacer(minLength: 0)
                        }
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .disabled(holder.isEmpty)
                    .help(L10n[.Root.foldedRowHelp])
                    .padding(.horizontal, 12).padding(.vertical, 3)
                }
            }
            Divider().padding(.vertical, 4)
            Text(L10n[.Root.foldedNote(L10n[.Nav.menuShowStandalone])])
                .font(.app(10.5)).foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
                .padding(.horizontal, 12)
        }
        .padding(.vertical, 8)
        .frame(width: 320, alignment: .leading)
    }
}
