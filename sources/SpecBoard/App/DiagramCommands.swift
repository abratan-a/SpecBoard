import AppKit
import SwiftUI

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
    /// Nil while the cards are on cells: 自動整列 places them anywhere.
    var autoLayout: (() -> Void)?
    /// Whether ⌘W is the tab bar's: not while the app's settings cover the window.
    var closesTabs = true
    /// The tab in front closed (⌘W); nil on the map tab, which does not close.
    var closeTab: (() -> Void)?
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

/// Zoom and fit in 表示, 自動整列 in 編集 — where a Mac keeps them — rather than
/// in the diagram's bar, which ran out of room for the things used every minute.
///
/// ⌘+ / ⌘- / ⌘0 already belong to the text size in the same menu, so the diagram
/// takes the same keys with ⌥.
struct DiagramCommands: Commands {
    @FocusedValue(\.diagram) private var diagram

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
                .keyboardShortcut("=", modifiers: [.command, .option])
                .disabled(diagram == nil)
            Button(L10n[.Diagram.commandZoomOut]) { diagram?.zoomOut() }
                .keyboardShortcut("-", modifiers: [.command, .option])
                .disabled(diagram == nil)
            Button(L10n[.Diagram.commandActualSize]) { diagram?.actualSize() }
                .keyboardShortcut("0", modifiers: [.command, .option])
                .disabled(diagram == nil)
            Button(L10n[.Diagram.commandFit]) { diagram?.fit() }
                .keyboardShortcut("9", modifiers: [.command, .option])
                .disabled(diagram == nil)
            Divider()
        }
        // ⌘W closes the tab in front while a project is on screen, and never the window then (the red
        // button and ⌘Q do); over the projects and the app's settings, it closes the window as it always has.
        CommandGroup(replacing: .saveItem) {
            if let diagram, diagram.closesTabs {
                Button(L10n[.Diagram.commandCloseTab]) { diagram.closeTab?() }
                    .keyboardShortcut("w", modifiers: .command)
                    .disabled(diagram.closeTab == nil)
            } else {
                Button(L10n[.Diagram.commandCloseWindow]) { NSApp.keyWindow?.performClose(nil) }
                    .keyboardShortcut("w", modifiers: .command)
            }
        }
        CommandGroup(after: .pasteboard) {
            Divider()
            Button(L10n[.Diagram.commandAutoLayout]) { diagram?.autoLayout?() }
                .disabled(diagram?.autoLayout == nil)
        }
    }
}
