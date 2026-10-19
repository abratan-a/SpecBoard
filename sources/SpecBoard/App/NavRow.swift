import AppKit
import SwiftUI

/// How a row of the nav panel — the tree's, the four above the branches, the branches' — answers the
/// pointer. The whole of its slot takes a press, the gap to the next row and the panel's sides as well:
/// a press that fell between two rows did nothing, and nothing said whether it had missed or the window
/// was busy. The pointer over a row lights it grey, as a project's row on the project list is lit. A
/// press on a tree row lights it as the row on screen before what it opens is drawn, which on a big
/// section takes a while.
enum NavRow {

    /// What a row is filled with.
    enum Fill: Equatable {
        /// The accent: the row on screen, or the row pressed while what it opens is on its way.
        case current
        /// The pointer's grey.
        case hovered
        case none
    }

    /// The fill of the row `key`. While a press waits for what it does (`pressed`), the accent is that
    /// row's alone — the window is on its way there, and two rows lit would read as two picked. The
    /// pointer's grey never covers the accent, and a row a drag would be dropped on shows the drag's
    /// mark alone.
    static func fill(key: String, current: Bool, pressed: String?, hovered: Bool, dropping: Bool = false) -> Fill {
        if pressed.map({ $0 == key }) ?? current { return .current }
        return hovered && !dropping ? .hovered : .none
    }

    /// The pointer's grey: the project list's.
    static let hoverColor = Color.secondary.opacity(0.08)

    /// Whether what a press does waits for the row to be drawn lit first: what moves the window does —
    /// a section opened, a card shown or opened, what a branch removed picked. Opening and shutting in
    /// the tree is drawn as soon as it is done, and a press that does nothing lights nothing.
    static func paintsFirst(_ press: NavPanelTree.Press) -> Bool {
        switch press {
        case .toggle, .nothing: false
        case .openSection, .show, .openCard, .openNode, .openSymbol, .pickGone: true
        }
    }

    /// A press on a tree row. What it does is read from the press's event at once — its click count
    /// tells a double-click, and by the time the work runs the window's event is another — then, where
    /// it waits on the drawing (`paintsFirst`), the row is lit (`light`) and the work handed to `later`,
    /// to run once the light is on screen; otherwise it is done now.
    static func pressing(_ node: NavPanelTree.Node, event: NSEvent?, in detail: ProjectDetail,
                         light: () -> Void, later: (@escaping () -> Void) -> Void,
                         run: @escaping (NavPanelTree.Press) -> Void) {
        let press = NavPanelTree.press(node, twice: NavPanelTree.twice(event), in: detail)
        guard paintsFirst(press) else {
            run(press)
            return
        }
        light()
        later { run(press) }
    }

    /// Once the window has drawn what was lit now: a moment on, as presses held for a reading wait
    /// for the window to draw what was read (`InputHold`). The light is handed to the screen before
    /// the main thread is taken by what the press opens.
    static func afterDrawn(_ work: @escaping () -> Void) {
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.02) { work() }
    }
}

/// A row's slot in the nav panel: its fill drawn behind the row as it stands, then the gap around it —
/// the space to the next row, the panel's sides — made part of it, so a press or the pointer anywhere
/// on the slot is the row's while the row looks as it did. Whether the pointer is over it is kept here,
/// so moving the pointer redraws this row alone, not the tree.
struct NavRowSlot: ViewModifier {
    /// Its key, for which row a press waits on.
    var key = ""
    /// The row on screen.
    var current: Bool
    /// The row a press waits on, if any (`NavRow.fill`).
    var pressed: String?
    /// A drag would be dropped on it: its mark is drawn, no grey under it.
    var dropping = false
    /// How strong the accent is.
    var accent = 0.16
    /// The corners of the fill, as the row's accent has them.
    var radius: CGFloat = 5
    /// The gap around the row, part of its slot.
    var gap = EdgeInsets()

    @State private var hovered = false

    func body(content: Content) -> some View {
        content
            .background(RoundedRectangle(cornerRadius: radius).fill(color))
            .padding(gap)
            .contentShape(Rectangle())
            .onHover { hovered = $0 }
    }

    private var color: Color {
        switch NavRow.fill(key: key, current: current, pressed: pressed, hovered: hovered, dropping: dropping) {
        case .current: Color.accentColor.opacity(accent)
        case .hovered: NavRow.hoverColor
        case .none: .clear
        }
    }
}
