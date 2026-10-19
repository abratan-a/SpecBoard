import AppKit
import SwiftUI

/// A side panel's edge as a strip (overview §13): pressed, it opens or shuts the
/// panel; dragged — when the panel is open — it sets the panel's width. Shut, the
/// property panel's strip stays at the window's edge, so a panel that is away still shows
/// where it is; the nav panel, shut, opens at its button by the window's own
/// (`NavPanelToggle`).
struct PanelStrip: View {
    /// Which way the arrow points: where pressing it moves the panel's edge.
    enum Arrow { case left, right }

    var arrow: Arrow
    var help: String
    var onToggle: () -> Void
    /// The horizontal distance dragged so far, while the panel is open; nil when shut.
    var onDrag: ((CGFloat) -> Void)? = nil
    var onDragEnd: () -> Void = {}

    static let width: CGFloat = 12
    /// How far a press may move and still be a press, not a drag.
    private static let slack: CGFloat = 3

    @State private var hovering = false
    @State private var dragging = false

    var body: some View {
        ZStack {
            Rectangle()
                .fill(Color.secondary.opacity(hovering || dragging ? 0.16 : 0.06))
            Image(systemName: arrow == .left ? "chevron.compact.left" : "chevron.compact.right")
                .font(.app(12, weight: .semibold))
                .foregroundStyle(hovering ? .primary : .secondary)
        }
        .frame(width: Self.width)
        .frame(maxHeight: .infinity)
        .contentShape(Rectangle())
        .onHover { hovering = $0 }
        .modifier(ResizePointer(on: onDrag != nil))
        .gesture(
            DragGesture(minimumDistance: 0, coordinateSpace: .global)
                .onChanged { value in
                    guard let onDrag, dragging || abs(value.translation.width) > Self.slack else { return }
                    dragging = true
                    onDrag(value.translation.width)
                }
                .onEnded { _ in
                    if dragging {
                        onDragEnd()
                    } else {
                        onToggle()
                        // The strip that set the column-resize pointer goes with the panel it
                        // opened or shut, and nothing under the pointer sets another: the arrow,
                        // until the pointer moves to what sets its own (seen with
                        // the nav panel shut).
                        Self.arrowAgain()
                    }
                    dragging = false
                })
        .onDisappear { if hovering { Self.arrowAgain() } }
        .help(help)
    }

    /// The arrow pointer, once what is on screen now has been laid out.
    static func arrowAgain() {
        DispatchQueue.main.async { NSCursor.arrow.set() }
    }
}

/// The column-resize pointer, where the strip can be dragged.
private struct ResizePointer: ViewModifier {
    var on: Bool

    func body(content: Content) -> some View {
        if on { content.pointerStyle(.columnResize) } else { content }
    }
}
