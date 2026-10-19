import SwiftUI

/// Chips that wrap.
///
/// `HStack` would run a project's eleven tags off the edge of the popover, and a
/// `LazyVGrid` would put them in columns wide enough for the longest one — which
/// for Japanese tag names means most of the row is empty. This packs each line and
/// moves on.
struct FlowRow: Layout {
    var spacing: CGFloat = 6
    var lineSpacing: CGFloat = 6

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let width = proposal.width ?? .infinity
        let lines = wrap(subviews, in: width)
        let height = lines.reduce(0) { $0 + $1.height } +
            CGFloat(max(0, lines.count - 1)) * lineSpacing
        // At least the widest line: an item wider than the room is painted past the frame it is
        // given, over what stands beside it.
        let widest = lines.map(\.width).max() ?? 0
        return CGSize(width: max(proposal.width ?? 0, widest), height: height)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize,
                       subviews: Subviews, cache: inout ()) {
        var y = bounds.minY
        for line in wrap(subviews, in: bounds.width) {
            var x = bounds.minX
            for item in line.items {
                subviews[item.index].place(
                    at: CGPoint(x: x, y: y + (line.height - item.size.height) / 2),
                    proposal: ProposedViewSize(item.size))
                x += item.size.width + spacing
            }
            y += line.height + lineSpacing
        }
    }

    private struct Item { var index: Int; var size: CGSize }
    private struct Line {
        var items: [Item] = []
        var width: CGFloat = 0
        var height: CGFloat = 0
    }

    private func wrap(_ subviews: Subviews, in width: CGFloat) -> [Line] {
        var lines: [Line] = []
        var line = Line()
        for index in subviews.indices {
            let size = subviews[index].sizeThatFits(.unspecified)
            let advance = line.items.isEmpty ? size.width : line.width + spacing + size.width
            if !line.items.isEmpty && advance > width {
                lines.append(line)
                line = Line()
            }
            line.width = line.items.isEmpty ? size.width : line.width + spacing + size.width
            line.height = max(line.height, size.height)
            line.items.append(Item(index: index, size: size))
        }
        if !line.items.isEmpty { lines.append(line) }
        return lines
    }
}
