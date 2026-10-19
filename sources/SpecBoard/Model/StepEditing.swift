import Foundation

/// An action's rows, changed one row at a time from its card editor (overview §13).
///
/// A row is addressed the way its card writes it (`CardRow.path`): `#p.0` for the
/// first row, `#p.0.1` for the second row inside it, `#p.0.c1` for the second case
/// of a `switch` and `#p.0.c1.0` for the first row inside that case. `#t` is the
/// trigger, which is not in the list. Nothing here checks what a row says: the
/// whole list goes through `Validate.steps` when it is saved.
enum StepEditing {
    enum Part: Hashable {
        case step(Int)
        case `case`(Int)
    }

    static let trigger = "#t"

    /// `#p.0.c1.0` as the way down to it; nil for what is not a row.
    static func parts(_ path: String) -> [Part]? {
        let pieces = path.split(separator: ".")
        guard pieces.first == "#p", pieces.count >= 2 else { return nil }
        var out: [Part] = []
        for piece in pieces.dropFirst() {
            if piece.hasPrefix("c"), let k = Int(piece.dropFirst()) {
                // A case only ever follows the switch it belongs to.
                guard case .step? = out.last else { return nil }
                out.append(.case(k))
            } else if let i = Int(piece) {
                out.append(.step(i))
            } else {
                return nil
            }
        }
        guard case .step? = out.first else { return nil }
        return out
    }

    static func path(_ parts: [Part]) -> String {
        "#p" + parts.map { part in
            switch part {
            case .step(let i): return ".\(i)"
            case .case(let k): return ".c\(k)"
            }
        }.joined()
    }

    /// Whether the row is a case of a switch rather than a row of its own.
    static func isCase(_ path: String) -> Bool {
        if case .case? = parts(path)?.last { return true }
        return false
    }

    /// The row the path names.
    static func step(at path: String, in steps: [Step]) -> Step? {
        var copy = steps
        var found: Step?
        within(&copy, path) { list, index in
            found = list[index]
            return false
        }
        return found
    }

    /// The list the row is in, and where in it, handed to `body` to change. What
    /// `body` returns says whether it did; nothing happens for a path that names
    /// no row.
    @discardableResult
    static func within(_ steps: inout [Step], _ path: String,
                       _ body: (inout [Step], Int) -> Bool) -> Bool {
        guard let parts = parts(path) else { return false }
        return within(&steps, parts[...], body)
    }

    private static func within(_ steps: inout [Step], _ parts: ArraySlice<Part>,
                               _ body: (inout [Step], Int) -> Bool) -> Bool {
        guard case .step(let i)? = parts.first, steps.indices.contains(i) else { return false }
        let rest = parts.dropFirst()
        guard let next = rest.first else { return body(&steps, i) }
        switch next {
        case .step:
            return within(&steps[i].steps, rest, body)
        case .case(let k):
            guard steps[i].cases.indices.contains(k) else { return false }
            let inside = rest.dropFirst()
            if inside.isEmpty { return body(&steps[i].cases, k) }
            return within(&steps[i].cases[k].steps, inside, body)
        }
    }

    // MARK: - Changes

    static func replacing(_ path: String, with row: Step, in steps: [Step]) -> [Step] {
        var out = steps
        within(&out, path) { list, index in list[index] = row; return true }
        return out
    }

    static func removing(_ path: String, in steps: [Step]) -> [Step] {
        var out = steps
        within(&out, path) { list, index in list.remove(at: index); return true }
        return out
    }

    /// A new row just after this one, in the same list. After the trigger it is the
    /// first row; after a case, a new case.
    static func inserting(_ row: Step, after path: String, in steps: [Step]) -> [Step] {
        if path == trigger { return [row] + steps }
        var out = steps
        within(&out, path) { list, index in list.insert(row, at: index + 1); return true }
        return out
    }

    /// A new row at the end of a group's rows or a case's; for a switch, a new case.
    static func appending(_ row: Step, inside path: String, in steps: [Step]) -> [Step] {
        var out = steps
        within(&out, path) { list, index in
            if list[index].op == "switch" {
                list[index].cases.append(row)
            } else {
                list[index].steps.append(row)
            }
            return true
        }
        return out
    }

    /// Up (-1) or down (+1) among its neighbours; nothing at either end.
    static func moving(_ path: String, by offset: Int, in steps: [Step]) -> [Step] {
        var out = steps
        within(&out, path) { list, index in
            let to = index + offset
            guard list.indices.contains(to) else { return false }
            list.swapAt(index, to)
            return true
        }
        return out
    }

    /// The row put inside a new group of this kind — the group standing where it was.
    /// A switch gets it as its one case; a loop and a case start with nothing to
    /// say, which the check on saving asks for.
    static func wrapping(_ path: String, in op: String, in steps: [Step]) -> [Step] {
        var out = steps
        within(&out, path) { list, index in
            let row = list[index]
            var group = Step(op: op)
            if op == "switch" {
                var way = Step(op: "")
                way.when = ""
                way.steps = [row]
                group.cases = [way]
            } else {
                group.steps = [row]
                if op == "loop" { group.over = "" }
            }
            list[index] = group
            return true
        }
        return out
    }

    /// A group taken apart: its rows stand where it stood. Not a switch, whose
    /// cases have nowhere to go.
    static func unwrapping(_ path: String, in steps: [Step]) -> [Step] {
        var out = steps
        within(&out, path) { list, index in
            let group = list[index]
            guard group.cases.isEmpty, Step.Vocabulary.groups.contains(group.op) else { return false }
            list.replaceSubrange(index...index, with: group.steps)
            return true
        }
        return out
    }

    /// Where a row just added lands: after `path`, or at the end inside it.
    static func added(after path: String, in steps: [Step]) -> String {
        if path == trigger { return "#p.0" }
        guard var parts = parts(path), let last = parts.popLast() else { return path }
        switch last {
        case .step(let i): return self.path(parts + [.step(i + 1)])
        case .case(let k): return self.path(parts + [.case(k + 1)])
        }
    }

    static func added(inside path: String, in steps: [Step]) -> String {
        guard let row = step(at: path, in: steps) else { return path }
        if row.op == "switch" { return path + ".c\(row.cases.count - 1)" }
        return path + ".\(row.steps.count - 1)"
    }

    /// A row of this kind with nothing in it yet — a case when the list is a switch's.
    static func blank(_ op: String) -> Step {
        var row = Step(op: op)
        if op == "loop" { row.over = "" }
        return row
    }

    static func blankCase() -> Step {
        var way = Step(op: "")
        way.when = ""
        return way
    }
}
