import CryptoKit
import Foundation
import os

/// What changed of a project since a reading the window holds, as the core answers `project_detail` asked
/// with that reading's digest (`since`, `specboard://SpecBoard/Docs/Design:CoreApi`「頼み」): the entries whose
/// hash is not the one held — new ones among them — the ids of those no longer there, each list's order
/// where it changed, and the hashes that changed. Put on the reading held (`WholeDetail.applying`), it gives
/// what a whole reading gives; the digest worked out of the hashes then says whether it did. What is sent
/// is the size of what changed, not of the project.
nonisolated struct DetailDelta: Decodable {
    /// The digest of the reading it is put on.
    var from: String
    /// The digest of the reading it makes.
    var digest: String
    var changed: Changed
    var removed: Removed
    var order: Order
    var hashes: [String: String]

    struct Changed: Decodable {
        var schemas: [WholeCard]
        var inner: [InnerCard]
        var ghosts: [CardGhost]
        var sections: [WholeSection]
        /// Every line a card has, by the card they leave: a card's lines go together.
        var relations: [String: [Relation]]
        var project: Project?
        var kinds: WholeKinds?
        var filePlaces: [String: FilePlace]?
    }

    struct Removed: Decodable {
        var schemas: [String]
        var inner: [String]
        var ghosts: [String]
        var sections: [String]
        /// The cards that have no line any more.
        var relations: [String]
    }

    /// Each list's ids in their order, where it is not the order held: nothing comes or goes without it.
    struct Order: Decodable {
        var schemas: [String]?
        var inner: [String]?
        var ghosts: [String]?
        var sections: [String]?
    }

    /// How many entries it brings and takes away, for the log.
    var size: (changed: Int, removed: Int) {
        let changed = self.changed.schemas.count + self.changed.inner.count + self.changed.ghosts.count
            + self.changed.sections.count + self.changed.relations.count
            + (self.changed.project == nil ? 0 : 1) + (self.changed.kinds == nil ? 0 : 1)
            + (self.changed.filePlaces == nil ? 0 : 1)
        let removed = self.removed.schemas.count + self.removed.inner.count + self.removed.ghosts.count
            + self.removed.sections.count + self.removed.relations.count
        return (changed, removed)
    }
}

extension WholeDetail {
    /// The entries a reading has besides its lists' and its cards' lines: `project`, `kinds`, and the order of
    /// each list — and `filePlaces`, from a core that sends it.
    private static let entriesBesides = 6

    /// This reading with `delta` put on it, as the core built the reading `delta.digest` names: entries
    /// replaced, put in or taken out by id, a card's lines replaced together, each list in the order sent (or
    /// the one held), the lines in the order of the cards they leave. Nil when it does not fit: sent from
    /// another reading, an entry it names not held, or the digest of the hashes after not the core's.
    func applying(_ delta: DetailDelta) -> WholeDetail? {
        guard let digest, delta.from == digest else { return nil }
        let removed = delta.removed
        guard let cards = Self.patched(cards, \.card.id, delta.changed.schemas, removed.schemas, delta.order.schemas),
              let inner = Self.patched(inner, \.id, delta.changed.inner, removed.inner, delta.order.inner),
              let ghosts = Self.patched(ghosts, \.id, delta.changed.ghosts, removed.ghosts, delta.order.ghosts),
              let sections = Self.patched(sections, \.id, delta.changed.sections.map(\.section), removed.sections,
                                          delta.order.sections)
        else { return nil }
        var next = self
        next.cards = cards
        next.inner = inner
        next.ghosts = ghosts
        next.sections = sections
        if !delta.changed.relations.isEmpty || !removed.relations.isEmpty || delta.order.schemas != nil || delta.order.inner != nil {
            var lines = Dictionary(grouping: relations, by: \.fromSchemaId)
            for id in removed.relations { lines[id] = nil }
            for (id, group) in delta.changed.relations { lines[id] = group }
            var flat: [Relation] = []
            flat.reserveCapacity(relations.count)
            var placed = 0
            for id in cards.map(\.card.id) + inner.map(\.id) {
                guard let group = lines[id] else { continue }
                flat += group
                placed += 1
            }
            // Lines of no card held: not the reading the core built.
            guard placed == lines.count else { return nil }
            next.relations = flat
        }
        if let project = delta.changed.project { next.project = project }
        if let kinds = delta.changed.kinds { next.take(kinds) }
        if let places = delta.changed.filePlaces { next.filePlaces = places }
        var hashes = self.hashes
        for (list, ids) in [("schemas", removed.schemas), ("inner", removed.inner), ("ghosts", removed.ghosts),
                            ("sections", removed.sections), ("relations", removed.relations)] {
            for id in ids { hashes["\(list)/\(id)"] = nil }
        }
        hashes.merge(delta.hashes) { _, sent in sent }
        let cardsWithLines = Set(next.relations.lazy.map(\.fromSchemaId)).count
        let entries = cards.count + inner.count + ghosts.count + sections.count + cardsWithLines + Self.entriesBesides
            + (hashes["filePlaces"] == nil ? 0 : 1)
        guard hashes.count == entries, Self.digest(of: hashes) == delta.digest else { return nil }
        next.hashes = hashes
        next.digest = delta.digest
        return next
    }

    /// The digest of a reading's hashes, worked out as the core works it out: each key, a tab, its hash and a
    /// line's end, the keys in the order of their UTF-8 bytes, through SHA-256; its first 8 bytes in hex.
    static func digest(of hashes: [String: String]) -> String {
        let keyed = hashes.map { (key: Array($0.key.utf8), hash: $0.value) }
            .sorted { $0.key.lexicographicallyPrecedes($1.key) }
        var bytes: [UInt8] = []
        bytes.reserveCapacity(keyed.count * 64)
        for (key, hash) in keyed {
            bytes += key
            bytes.append(0x09)
            bytes += hash.utf8
            bytes.append(0x0A)
        }
        return SHA256.hash(data: bytes).prefix(8).map { String(format: "%02x", $0) }.joined()
    }

    /// A list with what changed put in: the entries sent replacing those of their ids, those removed taken
    /// out, in the order sent; nil when an id in the order is neither sent nor held.
    private static func patched<T>(_ list: [T], _ id: (T) -> String, _ changed: [T], _ removed: [String],
                                   _ order: [String]?) -> [T]? {
        if changed.isEmpty, removed.isEmpty, order == nil { return list }
        var at: [String: Int] = [:]
        at.reserveCapacity(list.count)
        for (i, entry) in list.enumerated() { at[id(entry)] = i }
        guard let order else {
            // Nothing comes or goes but with the order sent.
            guard removed.isEmpty else { return nil }
            var out = list
            for entry in changed {
                guard let i = at[id(entry)] else { return nil }
                out[i] = entry
            }
            return out
        }
        var sent: [String: T] = [:]
        for entry in changed { sent[id(entry)] = entry }
        let gone = Set(removed)
        var out: [T] = []
        out.reserveCapacity(order.count)
        for key in order {
            if let entry = sent[key] {
                out.append(entry)
            } else if let i = at[key], !gone.contains(key) {
                out.append(list[i])
            } else {
                return nil
            }
        }
        return out
    }
}

/// The readings this app holds of the trees it read from one core, the last of each, with their hashes: what
/// the next reading of a tree is asked from (`since`) and what it puts what changed on. The core keeps what it
/// sent this app the same way, by its hashes; neither holds more than the trees read last.
nonisolated final class HeldDetails: @unchecked Sendable {
    /// How many trees are held: the open one, what it reaches beside it (the board's main), and those
    /// looked at just before.
    static let trees = 6

    private let held = OSAllocatedUnfairLock<[(key: String, reading: WholeDetail)]>(uncheckedState: [])

    private static func key(_ project: String, _ tree: String) -> String { project + "\u{0}" + tree }

    /// The reading held of `tree` (`""` for main) of `project`.
    func get(_ project: String, _ tree: String) -> WholeDetail? {
        let key = Self.key(project, tree)
        return held.withLockUnchecked { list in list.last { $0.key == key }?.reading }
    }

    /// Every tree's reading let go: the next of each is read whole.
    func clear() {
        held.withLockUnchecked { $0.removeAll() }
    }

    /// `reading` held as the last of its tree; nil lets the tree's go (a reading without hashes).
    func put(_ project: String, _ tree: String, _ reading: WholeDetail?) {
        let key = Self.key(project, tree)
        held.withLockUnchecked { list in
            list.removeAll { $0.key == key }
            if let reading { list.append((key, reading)) }
            if list.count > Self.trees { list.removeFirst(list.count - Self.trees) }
        }
    }
}

/// A project read as a tree sees it (`CoreClient.readDetail`): what the window draws from, and whether what was
/// held was found not to be what the core has, and the whole read again.
nonisolated struct DetailRead: @unchecked Sendable {
    var detail: ProjectDetail
    var drifted = false
}

extension CoreClient {
    /// What a reading is, when it is the core's answer to `project_detail`: the whole, or what changed.
    nonisolated enum DetailReading {
        case whole(WholeDetail)
        case delta(DetailDelta)
    }

    /// The project as the tree reads it, asked from the reading held of it (`held`): what changed since is
    /// sent and put on it, and the project worked out from it again without reading anything twice. What was
    /// held not fitting what the core sent, the whole is read again (`drifted`). On any thread.
    nonisolated func readDetail(_ project: String, branch: String?) throws -> DetailRead {
        let tree = branch ?? ""
        let base = held.get(project, tree)
        var fields: [String: Any] = ["project": project, "branch": tree]
        if let since = base?.digest { fields["since"] = since }
        let reply = try reply("project_detail", fields)
        switch try Self.reading(reply) {
        case .whole(let whole):
            return DetailRead(detail: holding(whole, project, tree, bytes: reply.utf8.count))
        case .delta(let delta):
            if let base, let detail = putting(delta, on: base, project, tree, bytes: reply.utf8.count) {
                return DetailRead(detail: detail)
            }
            Self.log.notice("drift project_detail \(delta.from, privacy: .public) → \(delta.digest, privacy: .public)")
            let again = try self.reply("project_detail", ["project": project, "branch": tree])
            guard case .whole(let whole) = try Self.reading(again) else {
                throw Refusal.unreadable(again)
            }
            return DetailRead(detail: holding(whole, project, tree, bytes: again.utf8.count), drifted: true)
        }
    }

    /// The project read from a whole answer asked before (`detailReply`), held as the tree's reading.
    nonisolated func detailRead(reading reply: String, _ project: String, branch: String?) throws -> DetailRead {
        guard case .whole(let whole) = try Self.reading(reply) else {
            throw Refusal.unreadable(reply)
        }
        return DetailRead(detail: holding(whole, project, branch ?? "", bytes: reply.utf8.count))
    }

    /// The core's answer to `project_detail`, read in one pass: the whole or what changed, or its refusal thrown.
    nonisolated static func reading(_ reply: String) throws -> DetailReading {
        let answer = try JSONDecoder().decode(DetailAnswer.self, from: Data(reply.utf8))
        if let error = answer.error { throw Refusal(error) }
        guard let result = answer.result else { throw Refusal.unreadable(reply) }
        return result
    }

    /// A whole reading held as the tree's, and the project worked out from it.
    private nonisolated func holding(_ whole: WholeDetail, _ project: String, _ tree: String, bytes: Int) -> ProjectDetail {
        let started = ContinuousClock.now
        let detail = joined(whole, project, tree: tree)
        held.put(project, tree, whole.digest == nil ? nil : whole)
        Self.log.notice("read project_detail \(Self.seconds(since: started), format: .fixed(precision: 3), privacy: .public)s \(bytes, privacy: .public)B \(detail.schemas.count, privacy: .public) cards")
        return detail
    }

    /// What changed put on the reading held, held in its place, and the project worked out from it; nil when
    /// it does not fit.
    private nonisolated func putting(_ delta: DetailDelta, on base: WholeDetail, _ project: String, _ tree: String,
                                     bytes: Int) -> ProjectDetail? {
        let started = ContinuousClock.now
        guard let next = base.applying(delta) else { return nil }
        let applied = Self.seconds(since: started)
        let rebuilding = ContinuousClock.now
        let detail = joined(next, project, tree: tree)
        held.put(project, tree, next)
        let size = delta.size
        Self.log.notice("read project_detail delta \(applied, format: .fixed(precision: 3), privacy: .public)s \(bytes, privacy: .public)B \(size.changed, privacy: .public) changed \(size.removed, privacy: .public) removed, rebuilt \(Self.seconds(since: rebuilding), format: .fixed(precision: 3), privacy: .public)s \(detail.schemas.count, privacy: .public) cards")
        return detail
    }

    nonisolated static func seconds(since started: ContinuousClock.Instant) -> Double {
        let took = ContinuousClock.now - started
        return Double(took.components.seconds) + Double(took.components.attoseconds) / 1e18
    }
}

/// What the core answers to `project_detail`: the project whole, what changed of it, or why it could not be read.
nonisolated private struct DetailAnswer: Decodable {
    var result: CoreClient.DetailReading?
    var error: String?

    private enum Keys: String, CodingKey { case result, error }

    /// Whether a reading is what changed (`delta`) rather than the whole.
    private struct Kind: Decodable {
        var delta: Bool?
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        error = try c.decodeIfPresent(String.self, forKey: .error)
        if try c.decodeIfPresent(Kind.self, forKey: .result)?.delta == true {
            result = .delta(try c.decode(DetailDelta.self, forKey: .result))
        } else {
            result = try c.decodeIfPresent(WholeDetail.self, forKey: .result).map { .whole($0) }
        }
    }
}
