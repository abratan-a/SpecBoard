import AppKit
import SwiftUI

/// A checkbox that can say "some of them".
///
/// SwiftUI's toggle is on or off. The 入れ子 switch speaks for every card of a
/// layout, and once a person has switched some of them it is neither — showing it
/// as either would claim something untrue, and clicking it would do something
/// nobody asked for. AppKit's checkbox has had a mixed state all along.
///
/// The click is the owner's to interpret: this never changes its own state, it
/// reports the click and waits to be told what it shows.
struct TriStateCheckbox: NSViewRepresentable {
    var title: String
    var state: Nesting.State
    var help: String
    var onClick: () -> Void

    func makeNSView(context: Context) -> NSButton {
        let button = NSButton(checkboxWithTitle: title, target: context.coordinator,
                              action: #selector(Coordinator.clicked(_:)))
        button.allowsMixedState = true
        button.setContentHuggingPriority(.required, for: .horizontal)
        return button
    }

    func updateNSView(_ button: NSButton, context: Context) {
        context.coordinator.parent = self
        button.title = title
        button.font = .systemFont(ofSize: 12 * Typography.shared.scale)
        button.toolTip = help
        button.state = Self.value(state)
    }

    func makeCoordinator() -> Coordinator { Coordinator(parent: self) }

    static func value(_ state: Nesting.State) -> NSControl.StateValue {
        switch state {
        case .on: return .on
        case .off: return .off
        case .mixed: return .mixed
        }
    }

    final class Coordinator: NSObject {
        var parent: TriStateCheckbox
        init(parent: TriStateCheckbox) { self.parent = parent }

        @objc func clicked(_ sender: NSButton) {
            // AppKit has already stepped the state; put it back until the owner says.
            sender.state = TriStateCheckbox.value(parent.state)
            parent.onClick()
        }
    }
}
