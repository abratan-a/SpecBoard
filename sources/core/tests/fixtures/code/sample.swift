import Foundation

/// The most an order holds.
let most = 10

/// An order.
@MainActor
final class Order: Codable {
    var lines: [Int] = []

    /// A new one.
    init() {
        func local() {}
        local()
    }

    init(lines: [Int]) {
        self.lines = lines
    }

    func total() -> Int {
        lines.reduce(0, +) + helper(1)
    }

    struct Line {
        let count: Int
    }
}

extension Order {
    func cheap() -> Bool { total() < 10 }
}

extension Order: CustomStringConvertible {
    var description: String { "\(total())" }
    static func == (a: Order, b: Order) -> Bool { a.total() == b.total() }
}

extension Ghost {
    func appear() {}
}

protocol Priced {
    func price() -> Int
}

enum State {
    case open, shut

    func flipped() -> State { self == .open ? .shut : .open }
}

struct Point {
    var x: Double
}

typealias Lines = [Int]

actor Counter {
    func bump() {}
}

// A plain comment, then a blank line: not the function's.

/// Helps.
func helper(_ n: Int) -> Int {
    let f = { (x: Int) in x + 1 }
    return f(n)
}
