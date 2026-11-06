import SwiftUI

@main
struct SpecBoardApp: App {
    init() {
        WindowRestore.register()
        // The nav panel's colours are the projects' now, not this Mac's.
        HexColor.forgetOldTints()
    }

    var body: some Scene {
        WindowGroup {
            // SwiftUI's own controls (a date picker's month and day) in the screen's language, not the Mac's.
            ContentView()
                .environment(\.locale, Language.shared.locale)
        }
        // A `specboard://` link goes to the window there is (`ContentView`'s `onOpenURL`); a new
        // one is made only when there is none.
        .handlesExternalEvents(matching: ["*"])
        // No title bar: the nav panel reaches the top of the window, the
        // window's buttons over its first row, and the rows there are where the window is taken
        // hold of (overview §13 「窓の形」). The title is only hidden: the Window menu and
        // Mission Control still name the window.
        .windowStyle(.hiddenTitleBar)
        // Only those rows' empty parts move the window (`WindowGrip`), not any empty part of it.
        .windowBackgroundDragBehavior(.disabled)
        .defaultSize(width: 1440, height: 900)
        .commands {
            UpdateCommands()
            TextSizeCommands()
            DiagramCommands()
        }
    }
}

/// Where the window comes back when the app is opened again: the app puts back the window's frame
/// (`WindowMemory`), but macOS opens it on the desktop the app is opened on — even when it
/// restores the windows (System Settings' "Close windows when quitting an application" off),
/// unless AppKit is told to bring each back to the desktop it was on. That is a default AppKit
/// reads and does not document; it is registered here, so a value the person has set themselves
/// still wins. When macOS does not restore the windows (that setting on, as it comes), this
/// changes nothing.
///
/// The window's frame is the app's to keep (`WindowMemory`): the frame
/// AppKit kept for SwiftUI's window (`NSWindow Frame SwiftUI.…`) is let go of before the window is
/// made, so the window is put back in one place alone.
enum WindowRestore {
    static let key = "NSWindowRestoresWorkspaceAtLaunch"
    /// What AppKit keeps a window's frame under, for the windows SwiftUI makes.
    static let swiftUIFrames = "NSWindow Frame SwiftUI."

    static func register(in defaults: UserDefaults = .standard) {
        defaults.register(defaults: [key: true])
        for name in defaults.dictionaryRepresentation().keys where name.hasPrefix(swiftUIFrames) {
            defaults.removeObject(forKey: name)
        }
    }
}

/// Text size, where macOS keeps it: the View menu. ⌥⌘＋ and ⌥⌘−, as ⌘＋ and ⌘− zoom the diagram in front
/// (`MenuKeys`): text size is set for one's eyes and kept, the diagram's zoom changes all the time.
struct TextSizeCommands: Commands {
    private var typography: Typography { Typography.shared }

    var body: some Commands {
        CommandGroup(after: .toolbar) {
            Button(L10n[.Root.menuTextBigger]) { typography.enlarge() }
                .keyboardShortcut(MenuKeys.textLarger)
                .disabled(!typography.canEnlarge)
            Button(L10n[.Root.menuTextSmaller]) { typography.reduce() }
                .keyboardShortcut(MenuKeys.textSmaller)
                .disabled(!typography.canReduce)
            Button(L10n[.Root.menuTextActual(typography.label)]) { typography.reset() }
                .keyboardShortcut(MenuKeys.textActual)
                .disabled(typography.scale == 1)
            Divider()
        }
    }
}
