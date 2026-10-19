import AppKit
import SwiftUI

/// The top of the window, now that it has no title bar (overview §13 「窓の形」).
///
/// The nav panel reaches the top of the window; its first row carries the window's own
/// buttons (close, minimise, zoom) and, right of them, the button that shuts it. The top bar
/// starts where the nav panel ends. Where nothing is to the left of the top bar — the nav
/// panel shut, the projects — the window's buttons sit over the top bar, and it leaves them
/// room. In full screen they are gone, and so is the room.
///
/// Kept apart from the views as numbers: the part a test can hold to.
enum TopBarLayout {
    /// The top bar, and the first row of the column beside it (the nav panel, the settings' list).
    static let height: CGFloat = 44
    /// What a top row leaves at its left when no room is taken there.
    static let margin: CGFloat = 14
    /// Between the window's last button and what follows it.
    static let lightsGap: CGFloat = 9
    /// How far the window's buttons reach from the window's left edge, with the gap after them:
    /// on macOS 26, 14 pt buttons at 9, 32 and 55. Measured from the window when it is up
    /// (`WindowChrome.lights`); this is what is assumed until then.
    static let lights: CGFloat = 69 + lightsGap
    /// Where the middle of the window's buttons goes, from the window's top: the middle of the top row.
    static let lightsMiddle: CGFloat = height / 2

    /// What is at the top of the window.
    enum Screen: Equatable {
        /// A project: the nav panel left of the top bar, or shut.
        case project(navPanelOpen: Bool)
        /// The projects over the whole window: nothing left of the top bar.
        case home
        /// The settings over the whole window: their list of items reaches the top, as the nav panel does.
        case settings
    }

    /// The room the top bar leaves at its left for the window's buttons: theirs wherever nothing
    /// is to its left, none where the column to its left carries them, none in full screen.
    static func room(_ screen: Screen, fullScreen: Bool, lights: CGFloat = lights) -> CGFloat {
        guard !fullScreen else { return 0 }
        switch screen {
        case .project(let navPanelOpen): return navPanelOpen ? 0 : lights
        case .home: return lights
        case .settings: return 0
        }
    }

    /// The room the column at the window's left — the nav panel, the settings' list — leaves in
    /// its first row: the window's buttons always sit over it, but in full screen.
    static func columnRoom(fullScreen: Bool, lights: CGFloat = lights) -> CGFloat {
        fullScreen ? 0 : lights
    }

    /// Where a top row's first thing starts, from the row's left edge: past the room, or at
    /// the row's own margin when it leaves none.
    static func leading(room: CGFloat, margin: CGFloat = margin) -> CGFloat {
        room > 0 ? room : margin
    }
}

/// The window's top as it is now: whether it is in full screen, and how far its buttons
/// reach. Kept up to date by `WindowChromeKeeper`; the rows read it for their room.
@Observable
final class WindowChrome {
    var fullScreen = false
    var lights = TopBarLayout.lights

    init(fullScreen: Bool = false) {
        self.fullScreen = fullScreen
    }

    func room(_ screen: TopBarLayout.Screen) -> CGFloat {
        TopBarLayout.room(screen, fullScreen: fullScreen, lights: lights)
    }

    var columnRoom: CGFloat { TopBarLayout.columnRoom(fullScreen: fullScreen, lights: lights) }
}

/// What a double click on the window's top does: what the system's setting says (System
/// Settings › Desktop & Dock › "Double-click a window's title bar to"), as AppKit reads it.
/// The top rows stand where the title bar was, so they answer as it would.
enum TitleBarDoubleClick: Equatable {
    case zoom, minimize, nothing

    /// `action`: `AppleActionOnDoubleClick`. `minimizeLegacy`: the older switch, read only
    /// where the newer setting is missing.
    static func of(_ action: String?, minimizeLegacy: Bool = false) -> Self {
        switch action {
        case "Minimize": .minimize
        case "None": .nothing
        case nil: minimizeLegacy ? .minimize : .zoom
        // "Maximize", and "Fill" (macOS 15 and later): AppKit has no public way to fill, and
        // zooming a window that has no largest size makes it as large as the screen lets it.
        default: .zoom
        }
    }

    static var current: Self {
        let defaults = UserDefaults.standard
        return of(defaults.string(forKey: "AppleActionOnDoubleClick"),
                  minimizeLegacy: defaults.bool(forKey: "AppleMiniaturizeOnDoubleClick"))
    }

    func perform(on window: NSWindow) {
        switch self {
        case .zoom: window.zoom(nil)
        case .minimize: window.miniaturize(nil)
        case .nothing: break
        }
    }
}

/// Where the window is taken hold of: laid behind a top row, so only its empty parts are
/// it — a button or a field on the row is pressed as ever (overview §13). Dragged, it moves
/// the window, even one in the background; double-clicked, it does what the title bar did.
struct WindowGrip: NSViewRepresentable {
    func makeNSView(context: Context) -> Grip { Grip() }
    func updateNSView(_ view: Grip, context: Context) {}

    final class Grip: NSView {
        /// Answered here rather than left to the window, so a double click is ours to answer.
        override var mouseDownCanMoveWindow: Bool { false }
        /// The first click on a window in the background moves it, as a title bar does.
        override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

        override func mouseDown(with event: NSEvent) {
            guard let window else { return super.mouseDown(with: event) }
            if event.clickCount == 2 {
                TitleBarDoubleClick.current.perform(on: window)
            } else {
                window.performDrag(with: event)
            }
        }
    }
}

/// Keeps `WindowChrome` up to date, and the window's buttons in the middle of the top row.
///
/// Left to AppKit, the buttons sit in the middle of the title bar that is no longer drawn
/// (32 pt), above the middle of the 44 pt row they now share with the nav panel's button
/// and the top bar. They are moved down within it, and moved again whenever AppKit puts
/// them back — a resize does. Only within what holds them, where they can still be
/// pressed; where they cannot be moved there, they stay where AppKit put them.
struct WindowChromeKeeper: NSViewRepresentable {
    var chrome: WindowChrome

    func makeNSView(context: Context) -> Keeper { Keeper(chrome: chrome) }
    func updateNSView(_ view: Keeper, context: Context) {}

    final class Keeper: NSView {
        private let chrome: WindowChrome
        /// Moving the buttons tells of their frames again; not answered while moving them.
        private var placing = false

        init(chrome: WindowChrome) {
            self.chrome = chrome
            super.init(frame: .zero)
        }

        required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }

        private static let windowChanges: [Notification.Name] = [
            NSWindow.willEnterFullScreenNotification, NSWindow.didEnterFullScreenNotification,
            NSWindow.willExitFullScreenNotification, NSWindow.didExitFullScreenNotification,
            NSWindow.didResizeNotification,
            NSWindow.didBecomeKeyNotification, NSWindow.didResignKeyNotification,
            NSWindow.didChangeBackingPropertiesNotification,
        ]

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            let center = NotificationCenter.default
            center.removeObserver(self)
            guard let window else { return }
            // Only the rows' grips move it (`WindowGrip`), not any empty part of the window.
            window.isMovableByWindowBackground = false
            for name in Self.windowChanges {
                center.addObserver(self, selector: #selector(windowChanged(_:)), name: name, object: window)
            }
            for button in Self.buttons(of: window) {
                button.postsFrameChangedNotifications = true
                center.addObserver(self, selector: #selector(buttonMoved(_:)),
                                   name: NSView.frameDidChangeNotification, object: button)
            }
            // A turn later: this may be in the middle of drawing the view it belongs to. Only
            // ever to say it is in full screen: going into it at launch (`WindowStateKeeper`)
            // may have said so already, before the window's style does.
            DispatchQueue.main.async { [weak self] in
                guard let self, let window = self.window else { return }
                if window.styleMask.contains(.fullScreen) { self.chrome.fullScreen = true }
                self.place(window)
            }
        }

        @objc private func windowChanged(_ note: Notification) {
            guard let window else { return }
            switch note.name {
            case NSWindow.willEnterFullScreenNotification:
                chrome.fullScreen = true
                // Back to the middle of their own bar, for the one full screen shows at the top.
                for button in Self.buttons(of: window) {
                    guard let bar = button.superview else { continue }
                    button.setFrameOrigin(NSPoint(x: button.frame.minX,
                                                  y: ((bar.bounds.height - button.frame.height) / 2).rounded()))
                }
            case NSWindow.didEnterFullScreenNotification:
                chrome.fullScreen = true
            case NSWindow.willExitFullScreenNotification:
                chrome.fullScreen = false
            default:
                if note.name == NSWindow.didExitFullScreenNotification { chrome.fullScreen = false }
                place(window)
            }
        }

        @objc private func buttonMoved(_ note: Notification) {
            guard let window else { return }
            place(window)
        }

        private static func buttons(of window: NSWindow) -> [NSButton] {
            [.closeButton, .miniaturizeButton, .zoomButton].compactMap { window.standardWindowButton($0) }
        }

        /// The buttons in the middle of the top row, and how far they reach.
        private func place(_ window: NSWindow) {
            guard !placing, !chrome.fullScreen, !window.styleMask.contains(.fullScreen) else { return }
            placing = true
            defer { placing = false }
            var reach: CGFloat = 0
            for button in Self.buttons(of: window) {
                guard let bar = button.superview else { continue }
                // The window's coordinates count up from its bottom edge.
                let middle = bar.convert(NSPoint(x: 0, y: window.frame.height - TopBarLayout.lightsMiddle), from: nil).y
                var frame = button.frame
                frame.origin.y = (middle - frame.height / 2).rounded()
                if frame.origin.y != button.frame.origin.y, bar.bounds.contains(frame) {
                    button.setFrameOrigin(frame.origin)
                }
                if !button.isHidden {
                    reach = max(reach, button.convert(button.bounds, to: nil).maxX)
                }
            }
            let lights = reach > 0 ? (reach + TopBarLayout.lightsGap).rounded(.up) : TopBarLayout.lights
            if chrome.lights != lights { chrome.lights = lights }
        }
    }
}

/// A row at the top of the window: as tall as the top bar, the room for the window's buttons
/// at its left where they sit over it (`TopBarLayout`), and the window taken hold of by its
/// empty parts (`WindowGrip`).
struct TopRow<Content: View>: View {
    var room: CGFloat
    /// At its left when it leaves no room, and at its right.
    var margin: CGFloat = TopBarLayout.margin
    @ViewBuilder var content: () -> Content

    var body: some View {
        HStack(spacing: 0) { content() }
            .padding(.leading, TopBarLayout.leading(room: room, margin: margin))
            .padding(.trailing, margin)
            .frame(maxWidth: .infinity, minHeight: TopBarLayout.height, maxHeight: TopBarLayout.height,
                   alignment: .leading)
            .background(WindowGrip())
    }
}

/// The nav panel's button, right of the window's buttons: in the nav panel's first row while
/// it is open, at the left of the top bar while it is shut — the same place either way; back and
/// forward follow it (`HistoryButtons`).
struct NavPanelToggle: View {
    var open: Bool
    var action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: "sidebar.left")
                .font(.app(14))
                .foregroundStyle(.secondary)
                .frame(width: 28, height: 24)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help(open ? L10n[.Root.navPanelCloseHelp] : L10n[.Root.navPanelOpenHelp])
        .accessibilityLabel(open ? L10n[.Root.navPanelClose] : L10n[.Root.navPanelOpen])
    }
}

/// Back and forward (⌘[ ⌘]) over what was opened (`AppModel.goBack`), right of the nav panel's button:
/// in the nav panel's first row while it is open, in the top bar while it is shut — the same place
/// either way, and on screen once, so the keys are always theirs.
struct HistoryButtons: View {
    var model: AppModel

    var body: some View {
        HStack(spacing: 2) {
            Button { model.goBack() } label: {
                Image(systemName: "chevron.left").frame(width: 18, height: 18)
            }
            .disabled(!model.canGoBack)
            .keyboardShortcut("[", modifiers: .command)
            .help(L10n[.Sections.barBack])
            Button { model.goForward() } label: {
                Image(systemName: "chevron.right").frame(width: 18, height: 18)
            }
            .disabled(!model.canGoForward)
            .keyboardShortcut("]", modifiers: .command)
            .help(L10n[.Sections.barForward])
        }
        .buttonStyle(.borderless)
        .fixedSize()
    }
}

extension EnvironmentValues {
    /// Under a window whose nav panel is shut (`WindowFrame`): the top bar says the project and the
    /// branch, which the nav panel says while it is open (`TopBar`).
    @Entry var navPanelShut = false
}

/// The nav panel in the window: open at a width, shut, or not on the screen (the projects).
enum NavPanelPane: Equatable {
    case open(width: CGFloat)
    case shut
    case absent
}

/// The window without a title bar (overview §13 「窓の形」): the nav panel from the top of the
/// window to the bottom, its first row the window's buttons and the button that shuts it; to
/// its right the top bar, and under the top bar everything else. Shut, the nav panel leaves the
/// window's buttons to the top bar, which makes room for them and begins with the button that
/// opens it. Over the projects there is no nav panel, and the top bar always makes room.
struct WindowFrame<Nav: View, Bar: View, Main: View>: View {
    var pane: NavPanelPane
    var chrome: WindowChrome
    var onToggleNavPanel: () -> Void = {}
    /// The open nav panel's edge dragged: how far so far.
    var onDragNavPanel: (CGFloat) -> Void = { _ in }
    var onDragNavPanelEnd: () -> Void = {}
    /// Whose back and forward stand right of the nav panel's button (`HistoryButtons`); none over the
    /// projects.
    var history: AppModel? = nil
    /// The nav panel's contents, under its first row.
    @ViewBuilder var nav: () -> Nav
    /// The top bar's own things (`TopBar`).
    @ViewBuilder var bar: () -> Bar
    /// Under the top bar.
    @ViewBuilder var main: () -> Main

    private var screen: TopBarLayout.Screen {
        switch pane {
        case .open: .project(navPanelOpen: true)
        case .shut: .project(navPanelOpen: false)
        case .absent: .home
        }
    }

    var body: some View {
        HStack(spacing: 0) {
            if case .open(let width) = pane {
                VStack(spacing: 0) {
                    TopRow(room: chrome.columnRoom) {
                        NavPanelToggle(open: true, action: onToggleNavPanel)
                        if let history { HistoryButtons(model: history).padding(.leading, 4) }
                        Spacer(minLength: 0)
                    }
                    nav()
                        .frame(minHeight: 0, maxHeight: .infinity, alignment: .top)
                }
                .frame(width: width)
                // Its edge, from the top of the window down: pressed it shuts, dragged it sets the width.
                PanelStrip(arrow: .left, help: L10n[.Root.navPanelDragHelp],
                           onToggle: onToggleNavPanel, onDrag: onDragNavPanel, onDragEnd: onDragNavPanelEnd)
            }
            VStack(spacing: 0) {
                TopRow(room: chrome.room(screen)) {
                    if pane == .shut {
                        NavPanelToggle(open: false, action: onToggleNavPanel)
                        if let history { HistoryButtons(model: history).padding(.leading, 4) }
                        Spacer().frame(width: 10)
                    }
                    bar()
                        .environment(\.navPanelShut, pane == .shut)
                }
                main()
                    .frame(minHeight: 0, maxHeight: .infinity, alignment: .top)
            }
        }
    }
}

/// How small the window may be: said here, not left to what it holds. The window is never smaller
/// than its content (`WindowGroup`), and what is under the top row — a banner's words, the
/// switches for the kinds of line, branches and rows in the nav panel — said, asked for its size
/// with no width, the height of all of it stacked a word or a switch a line: over a locked main the
/// window was taller than a display, could not go full screen (the green button a 「＋」) and ran
/// past the bottom when filled. Under the top row, then, nothing says it needs height
/// (`WindowFrame`); the window's least is this.
///
/// As wide as both side panels as they start and their strips, with the middle's least between
/// (`PaneSplit`; 1048): at 720 the middle had 56 points and the property panel lay over it. A panel
/// dragged wider than it starts can still crowd the middle of a window this small; it is not folded
/// away by itself.
enum WindowLeast {
    static let width: CGFloat = CGFloat(PaneSplit.navPanelStart + PaneSplit.propertiesStart)
        + 2 * PanelStrip.width + PaneSplit.middleLeast
    static let height: CGFloat = 480
}
