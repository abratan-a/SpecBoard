import AppKit
import WebKit

/// Pictures of what the window is showing, for an agent to look at.
///
/// Drawn by the app itself rather than taken from the screen, so nothing here needs
/// the Screen Recording permission, and a window behind others is pictured all the
/// same. What is not part of the window — popovers, menus, sheets, other apps — is
/// not in the picture. That includes the MCP popover and the tunnel code it shows.
///
/// Pages drawn by WebKit — a chart, a note, a diagram in the panel — are not in what
/// the window draws for itself either. They are pictured one by one once they say
/// they are drawn (`pages(in:)`), which is why a picture waits for them (`AppControl`).
@MainActor
enum WindowCapture {

    /// Registered by the canvas when it is made. Weak, so a closed window lets go.
    static weak var canvas: CardCanvasView?

    enum Target: String, CaseIterable {
        /// The whole window: navigation pane, bars, and what is open.
        case window
        /// The diagram panel, as much of it as is on screen.
        case graph
    }

    struct Picture {
        var png: Data
        var width: Int
        var height: Int
    }

    nonisolated static let defaultMaxSize = 1600
    static let sizeRange = 400...3000

    /// A page and where it sits, in the window's content view.
    struct Page {
        var image: CGImage
        var frame: CGRect
    }

    /// `pages`: pictures of the WebKit pages to lay over the window (`pages(in:)`).
    static func capture(_ target: Target, maxSize: Int = defaultMaxSize, pages: [Page] = []) throws -> Picture {
        let image: CGImage
        switch target {
        case .graph:
            guard let canvas, canvas.window != nil, !canvas.isHiddenOrHasHiddenAncestor,
                  let shot = canvas.snapshot() else {
                throw MCPError(.AppControl.errorGraphNotShown)
            }
            image = shot
        case .window:
            guard let window = mainWindow, let view = window.contentView,
                  let shot = render(view, pages: pages) else {
                throw MCPError(.AppControl.errorNoAppWindow)
            }
            image = shot
        }
        return try encode(scaled(image, maxSize: maxSize))
    }

    /// The window's content view, for `pages(in:)`.
    static var contentView: NSView? { mainWindow?.contentView }

    /// Each WebKit page on screen, pictured once it has drawn — its diagrams too —
    /// waiting `patience` at most, and only the part of it that is in view.
    static func pages(in root: NSView, patience: Duration = .seconds(3)) async -> [Page] {
        func onScreen() -> [WKWebView] {
            var out: [WKWebView] = []
            func walk(_ view: NSView) {
                if let page = view as? WKWebView {
                    if !page.isHiddenOrHasHiddenAncestor, !page.visibleRect.intersection(page.bounds).isEmpty {
                        out.append(page)
                    }
                    return
                }
                view.subviews.forEach(walk)
            }
            walk(root)
            return out
        }
        let clock = ContinuousClock()
        let deadline = clock.now + patience
        while onScreen().contains(where: { ($0 as? PassiveWebView)?.ready == false }), clock.now < deadline {
            try? await Task.sleep(for: .milliseconds(50))
        }
        // The heights they reported, laid out.
        try? await Task.sleep(for: .milliseconds(150))
        root.layoutSubtreeIfNeeded()

        var out: [Page] = []
        for page in onScreen() {
            // Inside SwiftUI, `visibleRect` can reach past the view itself; WebKit
            // paints what is past it white, over whatever is really there.
            let visible = page.visibleRect.intersection(page.bounds)
            guard !visible.isNull, !visible.isEmpty else { continue }
            let config = WKSnapshotConfiguration()
            config.rect = visible
            guard let shot = try? await page.takeSnapshot(configuration: config),
                  let image = shot.cgImage(forProposedRect: nil, context: nil, hints: nil) else { continue }
            out.append(Page(image: image, frame: page.convert(visible, to: root)))
        }
        return out
    }

    private static var mainWindow: NSWindow? {
        canvas?.window ?? NSApp.windows.first {
            $0.isVisible && !($0 is NSPanel) && $0.contentView != nil && $0.level == .normal
        }
    }

    /// The window's views over the window's own background, with the canvas drawn
    /// over its frame afterwards.
    ///
    /// Both are things `cacheDisplay` leaves out. The background belongs to the window,
    /// not to any view, so without it the picture is transparent wherever nothing
    /// draws — white once flattened, and in dark mode the bars' white text vanished
    /// into it. The canvas is layers and nothing else.
    static func render(_ view: NSView, pages: [Page] = []) -> CGImage? {
        let bounds = view.bounds
        guard bounds.width > 0, bounds.height > 0,
              let rep = view.bitmapImageRepForCachingDisplay(in: bounds) else { return nil }
        view.cacheDisplay(in: bounds, to: rep)
        guard let base = rep.cgImage,
              let ctx = context(width: base.width, height: base.height) else { return nil }

        var background = NSColor.windowBackgroundColor.cgColor
        view.effectiveAppearance.performAsCurrentDrawingAppearance {
            background = (view.window?.backgroundColor ?? .windowBackgroundColor).cgColor
        }
        ctx.setFillColor(background)
        ctx.fill(CGRect(x: 0, y: 0, width: base.width, height: base.height))
        ctx.draw(base, in: CGRect(x: 0, y: 0, width: base.width, height: base.height))

        let scale = CGFloat(base.width) / bounds.width
        func place(_ image: CGImage, at frame: CGRect) {
            var frame = frame
            // A bitmap context counts up from the bottom; a flipped view counts down.
            if view.isFlipped { frame.origin.y = bounds.height - frame.maxY }
            ctx.draw(image, in: CGRect(x: frame.minX * scale, y: frame.minY * scale,
                                       width: frame.width * scale, height: frame.height * scale))
        }
        if let canvas, canvas.window === view.window, !canvas.isHiddenOrHasHiddenAncestor,
           let shot = canvas.snapshot() {
            place(shot, at: canvas.convert(canvas.bounds, to: view))
        }
        for page in pages { place(page.image, at: page.frame) }
        return ctx.makeImage() ?? base
    }

    /// Longest side no more than `maxSize`: a picture is read as tokens, and past a
    /// point more pixels cost more than they show.
    static func scaled(_ image: CGImage, maxSize: Int) -> CGImage {
        let longest = max(image.width, image.height)
        guard longest > maxSize else { return image }
        let factor = Double(maxSize) / Double(longest)
        let width = max(1, Int((Double(image.width) * factor).rounded()))
        let height = max(1, Int((Double(image.height) * factor).rounded()))
        guard let ctx = context(width: width, height: height) else { return image }
        ctx.interpolationQuality = .high
        ctx.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
        return ctx.makeImage() ?? image
    }

    static func encode(_ image: CGImage) throws -> Picture {
        guard let png = NSBitmapImageRep(cgImage: image).representation(using: .png, properties: [:])
        else { throw MCPError(.AppControl.errorNoImage) }
        return Picture(png: png, width: image.width, height: image.height)
    }

    private static func context(width: Int, height: Int) -> CGContext? {
        CGContext(data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0,
                  space: CGColorSpace(name: CGColorSpace.sRGB)!,
                  bitmapInfo: CGImageAlphaInfo.premultipliedFirst.rawValue)
    }
}
