import SwiftUI

/// A line as its kind draws it (`LineLook`), read left to right as where it leaves →
/// where it arrives: its colour and dash, and the marks at both ends — what the legend
/// shows beside each kind of line, so it cannot drift from the canvas.
struct LineSample: View {
    var look: LineLook

    @Environment(\.colorScheme) private var colorScheme
    private var theme: GraphTheme { colorScheme == .dark ? .dark : .light }

    var body: some View {
        let colour = Color(nsColor: look.ink.color(dark: theme.isDark))
        ZStack(alignment: .leading) {
            Path { p in
                p.move(to: CGPoint(x: 0, y: 5))
                p.addLine(to: CGPoint(x: 32, y: 5))
            }
            .stroke(colour, style: StrokeStyle(lineWidth: 1.5, dash: look.dash.pattern ?? []))

            mark(look.begin, colour, pointing: .left)
                .frame(width: 11, height: 10)
            mark(look.end, colour, pointing: .right)
                .frame(width: 11, height: 10)
                .offset(x: 21)
        }
        .frame(width: 32, height: 10)
    }

    private enum Pointing { case left, right }

    @ViewBuilder
    private func mark(_ cap: EdgeCap, _ colour: Color, pointing: Pointing) -> some View {
        let shape = Self.shape(cap, pointing: pointing)
        if cap.isClosed {
            shape.fill(cap.isFilled ? colour : Color(nsColor: theme.background))
                .overlay(shape.stroke(colour, lineWidth: 1.2))
        } else if cap != .none {
            shape.stroke(colour, lineWidth: 1.2)
        }
    }

    /// The mark in an 11×10 box: a diamond across it, an arrowhead or a triangle pointing
    /// out of the line at its end.
    private static func shape(_ cap: EdgeCap, pointing: Pointing) -> Path {
        Path { p in
            let right = pointing == .right
            switch cap {
            case .filledDiamond, .openDiamond:
                p.move(to: CGPoint(x: 0, y: 5))
                p.addLine(to: CGPoint(x: 5, y: 2))
                p.addLine(to: CGPoint(x: 10, y: 5))
                p.addLine(to: CGPoint(x: 5, y: 8))
                p.closeSubpath()
            case .arrow:
                p.move(to: CGPoint(x: right ? 3 : 8, y: 1))
                p.addLine(to: CGPoint(x: right ? 11 : 0, y: 5))
                p.addLine(to: CGPoint(x: right ? 3 : 8, y: 9))
            case .triangle:
                p.move(to: CGPoint(x: right ? 3 : 8, y: 1))
                p.addLine(to: CGPoint(x: right ? 11 : 0, y: 5))
                p.addLine(to: CGPoint(x: right ? 3 : 8, y: 9))
                p.closeSubpath()
            case .none:
                break
            }
        }
    }
}
