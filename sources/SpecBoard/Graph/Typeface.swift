import AppKit
import CoreText

/// The fonts the app's own drawing measures and draws its text in: the cards, the diagram's labels and the bars
/// that measure what they say.
///
/// AppKit declares its font calls never to give nil, and Swift takes it at its word — yet one has: the font
/// put in an attribute dictionary was nil, and measuring a card's name with it threw inside CoreText
/// (`-[NSString sizeWithAttributes:]`, a nil object inserted into a dictionary), ending the app. So each size
/// and weight is asked of AppKit once and kept, and a font AppKit gives as nil is never handed on: another that
/// is there stands in for it — the fixed-pitch or the user's font, else one CoreText always makes — and is not
/// kept, so the next drawing asks AppKit again. At worst a card is drawn a little off once.
nonisolated enum Typeface {
    private enum Design: Hashable { case system, mono, digits }
    private struct Key: Hashable {
        var design: Design
        var size: CGFloat
        var weight: CGFloat
    }

    private static let lock = NSLock()
    nonisolated(unsafe) private static var kept: [Key: NSFont] = [:]

    /// The system font, as `NSFont.systemFont(ofSize:weight:)`.
    static func system(_ size: CGFloat, _ weight: NSFont.Weight = .regular) -> NSFont {
        font(Key(design: .system, size: size, weight: weight.rawValue)) { NSFont.systemFont(ofSize: size, weight: weight) }
    }

    /// The monospaced system font, as `NSFont.monospacedSystemFont(ofSize:weight:)`.
    static func mono(_ size: CGFloat, _ weight: NSFont.Weight = .regular) -> NSFont {
        font(Key(design: .mono, size: size, weight: weight.rawValue)) { NSFont.monospacedSystemFont(ofSize: size, weight: weight) }
    }

    /// The system font with figures of one width, as `NSFont.monospacedDigitSystemFont(ofSize:weight:)`.
    static func digits(_ size: CGFloat, _ weight: NSFont.Weight = .regular) -> NSFont {
        font(Key(design: .digits, size: size, weight: weight.rawValue)) {
            NSFont.monospacedDigitSystemFont(ofSize: size, weight: weight)
        }
    }

    /// Whether a font AppKit handed back is really there: a reference Swift was told can never be nil, read as
    /// the address it is.
    static func present(_ font: NSFont) -> Bool {
        unsafeBitCast(font, to: UInt.self) != 0
    }

    /// `made` when it is there, else one that is, of the same size — set as code is when `mono`.
    static func usable(_ made: NSFont, size: CGFloat, mono: Bool) -> NSFont {
        if present(made) { return made }
        if let near = mono ? NSFont.userFixedPitchFont(ofSize: size) : NSFont.userFont(ofSize: size) { return near }
        // CoreText answers a name it does not have with a font it does: never nothing.
        return unsafeBitCast(CTFontCreateWithName((mono ? "Menlo" : "Helvetica") as CFString, size, nil), to: NSFont.self)
    }

    private static func font(_ key: Key, _ make: () -> NSFont) -> NSFont {
        lock.lock()
        defer { lock.unlock() }
        if let font = kept[key] { return font }
        let made = make()
        guard present(made) else { return usable(made, size: key.size, mono: key.design == .mono) }
        kept[key] = made
        return made
    }
}
