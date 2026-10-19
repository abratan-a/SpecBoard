/// Which of a set of keys was asked for least recently: each asked for in constant time, the oldest
/// let go of in constant time.
struct LeastRecent<Key: Hashable> {
    private final class Node {
        let key: Key
        var older: Node?
        var newer: Node?
        init(_ key: Key) { self.key = key }
    }

    private var nodes: [Key: Node] = [:]
    private var oldest: Node?
    private var newest: Node?

    var count: Int { nodes.count }

    /// `key` asked for now: the most recent.
    mutating func touch(_ key: Key) {
        let node = nodes[key] ?? Node(key)
        nodes[key] = node
        guard newest !== node else { return }
        unlink(node)
        node.older = newest
        node.newer = nil
        newest?.newer = node
        newest = node
        if oldest == nil { oldest = node }
    }

    /// The key asked for least recently, no longer held.
    mutating func removeOldest() -> Key? {
        guard let node = oldest else { return nil }
        unlink(node)
        nodes[node.key] = nil
        return node.key
    }

    mutating func remove(_ key: Key) {
        guard let node = nodes.removeValue(forKey: key) else { return }
        unlink(node)
    }

    private mutating func unlink(_ node: Node) {
        if oldest === node { oldest = node.newer }
        if newest === node { newest = node.older }
        node.older?.newer = node.newer
        node.newer?.older = node.older
        node.older = nil
        node.newer = nil
    }
}
