import AppKit
import Foundation

/// A whiteboard on screen (`specboard://SpecBoard/Docs/Design:CardKinds`「ホワイトボード（whiteboard）」): its bar's tools,
/// and a sketch or a sticky placed with one where the map is pressed.
extension AppModel {
    /// The open section, when it is a whiteboard: what the bar's tools are for.
    var shownWhiteboard: ProjectSection? {
        guard let id = mapShownSection, let section = detail?.directory.section(id: id), section.kind == .whiteboard else {
            return nil
        }
        return section
    }

    /// The bar's tool that draws an arrow from the part pressed rather than placing one (`sketchTool`).
    static let arrowTool = "arrow"
    /// The bar's tool that places a picture chosen from a file where the map is pressed (`placeImage`).
    static let imageTool = "image"

    /// Whether a line from `from` to `to` joins two parts of the whiteboard open: each a sketch, a note or an SVG
    /// of it.
    func joinsWhiteboardParts(_ from: String, _ to: String) -> Bool {
        guard from != to, let board = shownWhiteboard, let detail else { return false }
        return [from, to].allSatisfy { id in
            guard let card = schema(id) else { return false }
            return [.sketch, .note, .svg, .image].contains(card.kind) && detail.directory.sectionId(of: card) == board.id
        }
    }

    /// A sketch of the tool chosen made at `cell` of the whiteboard open — a sticky for `note` —, named the first
    /// of its kind's names free there, picked, and the tool put back to 「選ぶ」. Returns its name; none when
    /// nothing is chosen or it was refused (said in `lastError`).
    @discardableResult
    func placeSketch(at cell: GridPoint) -> String? {
        guard let tool = sketchTool, tool != Self.arrowTool, tool != Self.imageTool, let detail else { return nil }
        sketchTool = nil
        // A tool chosen before the map went read only (a hold, a lock) places nothing.
        guard shownWhiteboard != nil, !isReadOnly else { return nil }
        let kind: SchemaKind = tool == "note" ? .note : .sketch
        let base = kind == .note ? "Note" : "Sketch"
        let taken = Set(detail.schemas.filter { detail.directory.sectionId(of: $0) == shownSectionKey }.map(\.name))
        guard let name = (1...).lazy.map({ "\(base)\($0)" }).first(where: { !taken.contains($0) }) else { return nil }
        var more: [String: Any] = ["topLeft": ["x": max(0, cell.x), "y": max(0, cell.y)]]
        if kind == .sketch { more["shape"] = tool }
        return createSchema(name: name, kindName: kind.rawValue, more: more, inView: false)?.name
    }

    // MARK: - Images

    /// An image card's picture, read from the core by the name the card gives it; none while it is read —
    /// `ready` is called once it is in.
    func imagePicture(_ card: SchemaNode, ready: (() -> Void)?) -> CGImage? {
        guard let detail, let hash = card.customFields["image"], !hash.isEmpty else { return nil }
        return whiteboardPictures.picture(project: detail.project.id, hash: hash, core: core(of: detail.project.id),
                                          ready: ready)
    }

    /// A picture placed at `cell` of the whiteboard open, named the first of the names `Image1`, `Image2`… free
    /// there, and picked. Returns its name; none where the cards are read only, or when the core refused it
    /// (said in `lastError`).
    @discardableResult
    func placeImage(_ data: Data, at cell: GridPoint) -> String? {
        guard let detail, shownWhiteboard != nil, !isReadOnly else { return nil }
        guard let portable = Self.portablePicture(data) else {
            lastError = L10n[.Root.whiteboardImageUnreadable]
            return nil
        }
        let taken = Set(detail.schemas.filter { detail.directory.sectionId(of: $0) == shownSectionKey }.map(\.name))
        guard let name = (1...).lazy.map({ "Image\($0)" }).first(where: { !taken.contains($0) }) else { return nil }
        let more: [String: Any] = ["imageData": portable.base64EncodedString(),
                                   "topLeft": ["x": max(0, cell.x), "y": max(0, cell.y)]]
        return createSchema(name: name, kindName: SchemaKind.image.rawValue, more: more, inView: false)?.name
    }

    /// An image card's picture put in place of the one it has.
    func replaceImage(_ id: String, with data: Data) {
        guard let portable = Self.portablePicture(data) else {
            lastError = L10n[.Root.whiteboardImageUnreadable]
            return
        }
        write { try tool("card_update", target: cardPath(id), value: ["imageData": portable.base64EncodedString()]) }
    }

    /// The most a picture may be for the core to take it.
    nonisolated static let mostPictureBytes = 5 * 1024 * 1024

    /// A picture as the core takes one: a PNG, a JPEG or a GIF as it is; any other the system reads (TIFF from
    /// the pasteboard, HEIC) as a PNG — as a JPEG when a PNG of it is past what the core takes, as a photo's
    /// is —; none when it is no picture.
    nonisolated static func portablePicture(_ data: Data) -> Data? {
        let head = [UInt8](data.prefix(6))
        if head.starts(with: [0x89, 0x50, 0x4E, 0x47]) || head.starts(with: [0xFF, 0xD8]) || head.starts(with: Array("GIF8".utf8)) {
            return data
        }
        guard let read = NSBitmapImageRep(data: data) else { return nil }
        if let png = read.representation(using: .png, properties: [:]), png.count <= mostPictureBytes { return png }
        return read.representation(using: .jpeg, properties: [.compressionFactor: 0.85])
    }

    // MARK: - Writing in place

    /// What is written where a card is, on the diagram: a sketch's text, any other card's body.
    func inPlaceText(_ id: String) -> String {
        guard let card = schema(id) else { return "" }
        return card.kind == .sketch ? card.customFields["text"] ?? "" : card.body
    }

    /// What was written where a card is, kept: a sketch's as its text, any other's as its body.
    func writeInPlace(_ id: String, _ text: String) {
        guard let card = schema(id) else { return }
        if card.kind == .sketch {
            setCustomField(id, "text", text)
        } else {
            updateSchema(id, .init(body: text))
        }
    }
}
