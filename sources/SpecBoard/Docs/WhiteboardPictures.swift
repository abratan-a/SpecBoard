import CoreGraphics
import Foundation
import ImageIO

/// The pictures of a whiteboard's image cards, kept for the window by the project and the name the card
/// gives them (the sha256 of their contents): a picture never changes under its name, so one read is kept
/// for good — a card shown again, picked, moved or zoomed draws what is kept and reads nothing.
@MainActor
final class WhiteboardPictures {
    private var kept: [String: CGImage] = [:]
    /// The pictures the core holds none of by their names, or that do not decode: read once, and drawn as their
    /// outline from then on rather than read again on every drawing.
    private var unreadable: Set<String> = []
    /// The pictures being read, until they are in.
    private var reading: Set<String> = []
    private var waiting: [String: [() -> Void]] = [:]
    /// Read one after another, off the main thread.
    private let queue = DispatchQueue(label: "SpecBoard.whiteboardPictures", qos: .userInitiated)
    /// The most pictures kept; past it, all are let go and read again as they are drawn.
    static let mostKept = 128

    /// The picture named `hash` in the project `project`; none while it is read — `ready` is called once it
    /// is in — or when the core holds none by that name.
    func picture(project: String, hash: String, core: CoreClient?, ready: (() -> Void)? = nil) -> CGImage? {
        let key = "\(project)|\(hash)"
        if let image = kept[key] { return image }
        guard let core, !hash.isEmpty, !unreadable.contains(key) else { return nil }
        if let ready { waiting[key, default: []].append(ready) }
        guard reading.insert(key).inserted else { return nil }
        let asking = Handed(value: (core, ["project": project, "hash": hash] as [String: Any]))
        let back = Handed(value: { [weak self] (image: CGImage?) in self?.take(image, key: key) })
        queue.async {
            let image = try? Self.read(asking.value.0, asking.value.1)
            let done = Handed(value: image ?? nil)
            DispatchQueue.main.async { MainActor.assumeIsolated { back.value(done.value) } }
        }
        return nil
    }

    /// Whether a picture is being read now.
    var busy: Bool { !reading.isEmpty }

    /// The longest side a picture is decoded at: as fine as a whiteboard's box shows it, whatever its own size.
    static let mostSide = 2048

    /// A picture's contents, read on the thread it is called on, decoded no bigger than `mostSide`; none when the
    /// core holds none or they do not decode.
    nonisolated static func read(_ core: CoreClient, _ arguments: [String: Any]) throws -> CGImage? {
        let said = try core.request("image_bytes", arguments) as? [String: Any]
        guard let data = (said?["data"] as? String).flatMap({ Data(base64Encoded: $0) }),
              let source = CGImageSourceCreateWithData(data as CFData, nil) else { return nil }
        let options: [CFString: Any] = [kCGImageSourceCreateThumbnailFromImageAlways: true,
                                        kCGImageSourceCreateThumbnailWithTransform: true,
                                        kCGImageSourceThumbnailMaxPixelSize: mostSide]
        return CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary)
    }

    private func take(_ image: CGImage?, key: String) {
        reading.remove(key)
        if let image {
            if kept.count >= Self.mostKept { kept.removeAll() }
            kept[key] = image
        } else {
            unreadable.insert(key)
        }
        for ready in waiting.removeValue(forKey: key) ?? [] { ready() }
    }
}
