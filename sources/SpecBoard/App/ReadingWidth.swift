import SwiftUI

extension View {
    /// Prose (an intro, a help note) read at a line length that stays readable, set on the `Text` itself:
    /// the rows, lists and trees around it fill the pane, and a paragraph should not stretch with them.
    func readingWidth() -> some View {
        frame(maxWidth: ReadingWidth.points, alignment: .leading)
    }
}

enum ReadingWidth {
    static let points: CGFloat = 720
}
