import AppKit
import SwiftUI

// How the marks of what a branch changed look (`ChangeMark`, `specboard://SpecBoard/Docs/Design:UI` §2
// 「変更の印」): the system's green, yellow, red and blue, light and dark — the yellow dark enough to
// read as text on a light window.

extension ChangeMark.Tint {
    var nsColor: NSColor {
        switch self {
        case .green: .systemGreen
        case .yellow: Self.readableYellow
        case .red: .systemRed
        case .blue: .systemBlue
        }
    }

    var color: Color { Color(nsColor: nsColor) }

    /// The colour as a light or a dark diagram draws it, whatever the window's appearance.
    func nsColor(dark: Bool) -> NSColor {
        let tint = nsColor
        var out = tint
        NSAppearance(named: dark ? .darkAqua : .aqua)?.performAsCurrentDrawingAppearance {
            out = tint.usingColorSpace(.sRGB) ?? tint
        }
        return out
    }

    /// The system's yellow on a dark window; on a light one, darkened until its letters read.
    private static let readableYellow = NSColor(name: nil) { appearance in
        appearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
            ? .systemYellow
            : NSColor(srgbRed: 0.70, green: 0.53, blue: 0.0, alpha: 1)
    }
}

/// A mark's letters, each in its colour: what the nav panel's right-most column and the lists of
/// what a branch changed show.
struct ChangeMarkText: View {
    var mark: ChangeMark
    var size: CGFloat = 10.5

    var body: some View {
        HStack(spacing: 0) {
            ForEach(Array(mark.letters.enumerated()), id: \.offset) { _, letter in
                Text(letter.text).foregroundStyle(letter.tint.color)
            }
        }
        .font(.app(size, weight: .bold, design: .monospaced))
        .fixedSize()
        .accessibilityLabel(mark.meaning)
    }
}

extension View {
    /// A tooltip, where there is anything to say.
    @ViewBuilder
    func help(ifAny text: String?) -> some View {
        if let text, !text.isEmpty { help(text) } else { self }
    }
}
