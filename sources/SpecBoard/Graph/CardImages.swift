import CoreGraphics

/// The pictures of the cards as they were drawn, kept for every diagram of the window: a section shown
/// again, a card picked and let go, draw what is kept rather than drawing the card again. A picture
/// is kept by everything its card's face shows (`look`), its size and how fine it is, so one that
/// differs in anything is drawn anew; what was asked for least recently goes first past `budget`.
@MainActor
final class CardImages {
    static let shared = CardImages()

    struct Key: Hashable {
        var card: String
        /// What the face shows, all of it hashed: the card, its rows, its document's picture, how
        /// it is marked and picked, the appearance and the language.
        var look: Int
        var width: CGFloat
        var height: CGFloat
        var scale: CGFloat
    }

    /// What the pictures kept may take, in bytes.
    static let budget = 128 << 20

    private var kept: [Key: CGImage] = [:]
    private var used = LeastRecent<Key>()
    private var held = 0

    /// How many pictures are kept now.
    var count: Int { kept.count }

    func image(_ key: Key) -> CGImage? {
        guard let image = kept[key] else { return nil }
        used.touch(key)
        return image
    }

    func keep(_ image: CGImage, for key: Key) {
        if let before = kept[key] { held -= Self.bytes(before) }
        kept[key] = image
        held += Self.bytes(image)
        used.touch(key)
        while held > Self.budget, used.count > 1, let oldest = used.removeOldest() {
            if let gone = kept.removeValue(forKey: oldest) { held -= Self.bytes(gone) }
        }
    }

    private static func bytes(_ image: CGImage) -> Int { image.bytesPerRow * image.height }
}
