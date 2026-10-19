import AppKit
import WebKit

/// A ui section's mock drawn as a picture for MCP's `as: picture` (`picture: mock`): the page the core built
/// (the same as `section_read` `as: mock`) loaded in a web view no one sees — its scripts off — at the page's
/// size, its images decoded, and taken as a PNG made smaller so its longest side keeps to `maxSize` (never
/// larger than the page). On the browser's white, as the page is drawn in a window of its own.
enum MockPicture {

    /// What is asked: the page, how big it is, and how big the picture may be.
    nonisolated struct Request: Sendable {
        var html: String
        var width: Int
        var height: Int
        var maxSize = WindowCapture.defaultMaxSize
        var lang = AppControl.mcpLanguage

        init?(_ asked: [String: Any]) {
            guard asked["picture"] as? String == "mock", let html = asked["html"] as? String else { return nil }
            self.html = html
            width = (asked["width"] as? NSNumber)?.intValue ?? 0
            height = (asked["height"] as? NSNumber)?.intValue ?? 0
            maxSize = (asked["maxSize"] as? NSNumber)?.intValue ?? maxSize
            lang = asked["lang"] as? String ?? lang
        }

        /// The picture's size against the page's: the longest side within `maxSize`, never past 1.
        var scale: CGFloat { min(1, CGFloat(maxSize) / CGFloat(max(1, max(width, height)))) }
    }

    /// A picture taken: the page's pixels at `scale`, and how many parts and frames it drew.
    struct Shot {
        var image: CGImage
        var scale: CGFloat
        var parts: Int
        var frames: Int
    }

    /// How long the page is waited for to load.
    nonisolated static let patience: Duration = .seconds(10)

    /// The answer to the core's request (`specboard_core::App::picture`), as `MapPicture.answer` gives the
    /// others'. From any thread but main: main cannot wait for the page it would itself draw.
    nonisolated static func answer(_ asked: [String: Any]) -> String {
        guard let request = Request(asked) else {
            return refusal(L10n.text(.AppControl.pictureUnreadableRequest, in: AppControl.mcpLanguage))
        }
        guard request.width > 0, request.height > 0 else {
            return refusal(L10n.text(.AppControl.pictureNothingDrawn, in: request.lang))
        }
        guard !Thread.isMainThread else { return refusal(L10n.text(.AppControl.pictureNotDrawn, in: request.lang)) }
        // While the app is in the background, App Nap would hold back the page WebKit draws.
        let activity = ProcessInfo.processInfo.beginActivity(options: .userInitiated, reason: "Drawing a mock for MCP")
        defer { ProcessInfo.processInfo.endActivity(activity) }
        final class Box: @unchecked Sendable {
            var said = ""
        }
        let box = Box()
        let done = DispatchSemaphore(value: 0)
        Task { @MainActor in
            do {
                let shot = try await take(request)
                let picture = try WindowCapture.encode(shot.image)
                box.said = json(["image": ["data": picture.png.base64EncodedString(), "mimeType": "image/png"],
                                 "note": ["picture": "mock", "width": picture.width, "height": picture.height,
                                          "scale": (Double(shot.scale) * 1000).rounded() / 1000,
                                          "parts": shot.parts, "frames": shot.frames]])
            } catch {
                box.said = refusal((error as? MapPicture.Refused)?.why ?? error.localizedDescription)
            }
            done.signal()
        }
        let seconds = Double(MapPicture.budget.components.seconds)
        guard done.wait(timeout: .now() + seconds) == .success else {
            return refusal(L10n.text(.AppControl.pictureLate, in: request.lang))
        }
        return box.said
    }

    nonisolated private static func refusal(_ why: String) -> String {
        json(["error": why])
    }

    nonisolated private static func json(_ object: [String: Any]) -> String {
        let data = (try? JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])) ?? Data()
        return String(decoding: data, as: UTF8.self)
    }

    /// The page drawn and taken.
    @MainActor
    static func take(_ request: Request) async throws -> Shot {
        let drawer = Drawer(width: request.width, height: request.height)
        defer { drawer.close() }
        guard await drawer.load(request.html) else {
            throw MapPicture.Refused(why: L10n.text(.AppControl.pictureNotDrawn, in: request.lang))
        }
        let counted = (try? await drawer.view.callAsyncJavaScript(Self.settle, arguments: [:], in: nil,
                                                                   contentWorld: .defaultClient)) as? [NSNumber]
        let scale = request.scale
        guard let taken = await drawer.snapshot(scale: scale),
              let image = resized(taken, width: max(1, Int((CGFloat(request.width) * scale).rounded())),
                                  height: max(1, Int((CGFloat(request.height) * scale).rounded()))) else {
            throw MapPicture.Refused(why: L10n.text(.AppControl.pictureNotDrawn, in: request.lang))
        }
        return Shot(image: image, scale: scale, parts: counted?.first?.intValue ?? 0,
                    frames: counted.flatMap { $0.count > 1 ? $0[1].intValue : nil } ?? 0)
    }

    /// The page's images decoded (a second at the most), then what it draws counted: its html and svg parts and
    /// its frames, inside frames too.
    private static let settle = """
        await Promise.race([
            Promise.all(Array.from(document.images, image => image.decode().catch(() => {}))),
            new Promise(done => setTimeout(done, 1000))
        ]);
        return [document.querySelectorAll('.sb-part[data-kind=html], .sb-part[data-kind=svg]').length,
                document.querySelectorAll('.sb-frame').length];
        """

    /// `image` drawn at `width` × `height` pixels, smoothly, on white.
    private static func resized(_ image: CGImage, width: Int, height: Int) -> CGImage? {
        guard let ctx = CGContext(data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0,
                                  space: CGColorSpace(name: CGColorSpace.sRGB)!,
                                  bitmapInfo: CGImageAlphaInfo.premultipliedFirst.rawValue) else { return nil }
        ctx.setFillColor(CGColor(red: 1, green: 1, blue: 1, alpha: 1))
        ctx.fill(CGRect(x: 0, y: 0, width: width, height: height))
        ctx.interpolationQuality = .high
        ctx.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
        return ctx.makeImage()
    }

    /// A web view in a window that is never shown, the page's size, the page's scripts off.
    @MainActor
    private final class Drawer: NSObject, WKNavigationDelegate {
        let view: WKWebView
        private let window: NSWindow
        private let width: Int
        private let height: Int
        private var navigation: WKNavigation?
        private var loaded: CheckedContinuation<Bool, Never>?

        init(width: Int, height: Int) {
            self.width = width
            self.height = height
            let config = WKWebViewConfiguration()
            config.websiteDataStore = .nonPersistent()
            config.defaultWebpagePreferences.allowsContentJavaScript = false
            let frame = CGRect(x: 0, y: 0, width: width, height: height)
            view = WKWebView(frame: frame, configuration: config)
            window = NSWindow(contentRect: CGRect(x: -20000, y: -20000, width: width, height: height),
                              styleMask: .borderless, backing: .buffered, defer: false)
            super.init()
            view.navigationDelegate = self
            window.isReleasedWhenClosed = false
            window.appearance = NSAppearance(named: .aqua)
            view.appearance = NSAppearance(named: .aqua)
            window.contentView = view
        }

        private static let address = URL(string: "\(MarkdownPage.scheme)://mock/picture")

        /// The page loaded: false when it could not be, or did not come in time.
        func load(_ html: String) async -> Bool {
            await withCheckedContinuation { going in
                loaded = going
                navigation = view.loadHTMLString(html, baseURL: Self.address)
                Task { @MainActor [weak self] in
                    try? await Task.sleep(for: MockPicture.patience)
                    self?.finish(false)
                }
            }
        }

        private func finish(_ ok: Bool) {
            guard let going = loaded else { return }
            loaded = nil
            going.resume(returning: ok)
        }

        func snapshot(scale: CGFloat) async -> CGImage? {
            let config = WKSnapshotConfiguration()
            config.rect = CGRect(x: 0, y: 0, width: width, height: height)
            config.afterScreenUpdates = true
            config.snapshotWidth = NSNumber(value: Double(CGFloat(width) * max(scale, 0.01)))
            let image = try? await view.takeSnapshot(configuration: config)
            return image?.cgImage(forProposedRect: nil, context: nil, hints: nil)
        }

        func close() {
            finish(false)
            view.stopLoading()
            window.contentView = nil
            window.close()
        }

        func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
            guard navigation === self.navigation else { return }
            finish(true)
        }

        func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: any Error) {
            guard navigation === self.navigation else { return }
            finish(false)
        }

        func webView(_ webView: WKWebView, didFailProvisionalNavigation navigation: WKNavigation!,
                     withError error: any Error) {
            guard navigation === self.navigation else { return }
            finish(false)
        }

        /// The page loading itself, and nothing else.
        func webView(_ webView: WKWebView, decidePolicyFor action: WKNavigationAction, preferences: WKWebpagePreferences,
                     decisionHandler: @escaping @MainActor (WKNavigationActionPolicy, WKWebpagePreferences) -> Void) {
            preferences.allowsContentJavaScript = false
            let own = action.navigationType == .other && action.targetFrame?.isMainFrame == true
                && action.request.url == Self.address
            decisionHandler(own ? .allow : .cancel, preferences)
        }
    }
}
