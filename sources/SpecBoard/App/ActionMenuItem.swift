import AppKit

/// An `NSMenuItem` that runs a closure.
///
/// AppKit menus are target/action, and the menus here are assembled by SwiftUI code
/// that has closures over view state rather than selectors on an object. The item
/// keeps itself as its own target, so nothing else has to stay alive for it.
final class ActionMenuItem: NSMenuItem {
    private let run: () -> Void

    init(_ title: String, on: Bool = false, run: @escaping () -> Void) {
        self.run = run
        super.init(title: title, action: #selector(fire), keyEquivalent: "")
        target = self
        state = on ? .on : .off
    }

    @available(*, unavailable)
    required init(coder: NSCoder) { fatalError("not loaded from a nib") }

    @objc private func fire() { run() }
}
