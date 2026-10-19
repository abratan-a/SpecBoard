import AppKit
import SwiftUI

/// Where the window was when the app last ran, and whether it was full screen.
///
/// The size SwiftUI keeps by itself, but not the display: a window left on a display beside the
/// main one came back on the display the app was opened from — the one a Dock was pressed on, or
/// the main one after 今すぐアップデート (three displays, the main one in the middle). Full screen it does
/// not keep either: macOS restores it only when the system keeps windows across a quit, which the
/// default setting ("close windows when quitting an application") turns off — so every relaunch
/// came back as a plain window, to be put back into full screen by hand.
///
/// Injected, like `SelectionMemory`, so a test never writes the app's own defaults.
struct WindowMemory {
    let defaults: UserDefaults
    static let key = "window.fullScreen"
    static let frameKey = "window.frame"
    static let standard = WindowMemory(defaults: .standard)

    var fullScreen: Bool {
        get { defaults.bool(forKey: Self.key) }
        nonmutating set { defaults.set(newValue, forKey: Self.key) }
    }

    /// Where the window last was out of full screen, in the coordinates every display shares (the
    /// main one's bottom left at the origin); none before it was ever kept.
    var frame: NSRect? {
        get {
            guard let said = defaults.string(forKey: Self.frameKey) else { return nil }
            let frame = NSRectFromString(said)
            return frame.width > 0 && frame.height > 0 ? frame : nil
        }
        nonmutating set {
            if let newValue { defaults.set(NSStringFromRect(newValue), forKey: Self.frameKey) }
            else { defaults.removeObject(forKey: Self.frameKey) }
        }
    }

    /// A display as the window is put back on it: all of it, and the part a window may take (no
    /// menu bar, no Dock).
    struct Display: Equatable {
        var frame: NSRect
        var visible: NSRect
    }

    /// Where a window kept at `frame` goes back among the displays connected now: on the one most
    /// of it was on, as it was — moved and cut to that display's room, should the room have
    /// changed. None when no part of it is on a display connected (that display taken away): the
    /// window opens where it would have.
    static func placement(of frame: NSRect, on displays: [Display]) -> NSRect? {
        func area(_ r: NSRect) -> CGFloat { r.isNull ? 0 : r.width * r.height }
        guard let display = displays.max(by: { area($0.frame.intersection(frame)) < area($1.frame.intersection(frame)) }),
              area(display.frame.intersection(frame)) > 0 else { return nil }
        let room = display.visible
        let width = min(frame.width, room.width)
        let height = min(frame.height, room.height)
        return NSRect(x: min(max(frame.minX, room.minX), room.maxX - width),
                      y: min(max(frame.minY, room.minY), room.maxY - height),
                      width: width, height: height)
    }

    /// The displays connected now.
    static var displays: [Display] { NSScreen.screens.map { Display(frame: $0.frame, visible: $0.visibleFrame) } }
}

/// Whether a window has been put back yet this run: the first window opened is; one opened after
/// it (ファイル › 新規ウインドウ) opens where macOS puts it, rather than on top of the first.
final class WindowPutBack {
    static let app = WindowPutBack()
    private var done = false

    /// True the first time it is asked.
    func first() -> Bool {
        defer { done = true }
        return !done
    }
}

/// Watches the window it ends up in: keeps where it is, and at launch puts it back there — on the
/// display it was on — and into full screen if it was left that way.
///
/// Remembered from the window's own notifications rather than from a menu command, so the green
/// button, the keyboard shortcut, Mission Control, a drag and the Window menu's Fill all count. A
/// quit while in full screen sends no "exit" — which is exactly what leaves it remembered.
struct WindowStateKeeper: NSViewRepresentable {
    var memory: WindowMemory = .standard
    var putBack: WindowPutBack = .app

    func makeNSView(context: Context) -> Tracker { Tracker(memory: memory, putBack: putBack) }
    func updateNSView(_ view: Tracker, context: Context) {}

    final class Tracker: NSView {
        private let memory: WindowMemory
        private let putBack: WindowPutBack
        private var observers: [NSObjectProtocol] = []
        /// Until the window is where it was put back: what SwiftUI opens it at is not kept.
        private var settled = false
        /// Going into full screen or out of it: the frames on the way are not kept.
        private var turning = false
        /// The frame kept is on a display not connected at launch: kept
        /// as it is — not overwritten by where the window opened instead — until the window is
        /// moved or sized by hand.
        private var holding = false

        init(memory: WindowMemory, putBack: WindowPutBack) {
            self.memory = memory
            self.putBack = putBack
            super.init(frame: .zero)
        }

        required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }

        deinit { observers.forEach(NotificationCenter.default.removeObserver) }

        /// Where the window is now, kept — out of full screen, not minimized, not on its way. `moved`:
        /// the window was moved or sized, which lets a frame held for a display away go.
        private func keep(_ window: NSWindow, moved: Bool) {
            guard settled, !turning, !window.styleMask.contains(.fullScreen), !window.isMiniaturized,
                  !window.inLiveResize else { return }
            if moved { holding = false } else if holding { return }
            memory.frame = window.frame
        }

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            observers.forEach(NotificationCenter.default.removeObserver)
            observers = []
            guard let window else { return }

            let center = NotificationCenter.default
            let memory = self.memory
            func on(_ name: Notification.Name, _ act: @escaping (NSWindow) -> Void) {
                observers.append(center.addObserver(forName: name, object: window, queue: .main) { [weak window] _ in
                    MainActor.assumeIsolated { if let window { act(window) } }
                })
            }
            on(NSWindow.willEnterFullScreenNotification) { [weak self] _ in self?.turning = true }
            on(NSWindow.willExitFullScreenNotification) { [weak self] _ in self?.turning = true }
            on(NSWindow.didEnterFullScreenNotification) { [weak self] _ in
                self?.turning = false
                memory.fullScreen = true
            }
            on(NSWindow.didExitFullScreenNotification) { [weak self] window in
                self?.turning = false
                memory.fullScreen = false
                self?.keep(window, moved: false)
            }
            for name in [NSWindow.didMoveNotification, NSWindow.didResizeNotification,
                         NSWindow.didEndLiveResizeNotification] {
                on(name) { [weak self] window in self?.keep(window, moved: true) }
            }
            on(NSWindow.willCloseNotification) { [weak self] window in self?.keep(window, moved: false) }
            observers.append(center.addObserver(forName: NSApplication.willTerminateNotification, object: nil,
                                                queue: .main) { [weak self, weak window] _ in
                MainActor.assumeIsolated { if let window { self?.keep(window, moved: false) } }
            })

            // Once, at launch, the first window: where it was, on the display it was on, before it
            // is drawn there — and again a turn later, should SwiftUI have put it where it opens
            // after this. Full screen after that, on that display (asking a window that is not yet
            // shown to go full screen does nothing).
            guard putBack.first() else {
                settled = true
                return
            }
            // Put back here alone: AppKit keeps no frame of its own for
            // the window — SwiftUI had it keep one, put back at launch and again after this, onto
            // another display when they differed in scale (`WindowRestore`).
            window.setFrameAutosaveName("")
            let kept = memory.frame
            let placed = kept.flatMap { WindowMemory.placement(of: $0, on: WindowMemory.displays) }
            holding = kept != nil && placed == nil
            if let placed { window.setFrame(placed, display: false) }
            DispatchQueue.main.async { [weak self, weak window] in
                guard let window else { return }
                if let placed, window.frame != placed { window.setFrame(placed, display: true) }
                self?.settled = true
                if memory.fullScreen && !window.styleMask.contains(.fullScreen) { window.toggleFullScreen(nil) }
            }
        }
    }
}
