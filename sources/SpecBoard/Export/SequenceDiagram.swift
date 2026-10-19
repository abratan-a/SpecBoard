import Foundation

/// A flow — or an action — as a sequence diagram (`specboard://SpecBoard/Docs/Design:Overview` §11): who
/// takes part, then what passes between them in order, with the blocks a decision, a
/// loop or ways that part make. Built once and drawn twice: on the flow's card editor by
/// the app's own canvas (`SequenceCanvas`), and as mermaid for what is written out, a
/// document's embed and MCP (`ActionDiagram`). Each item says which node — and which
/// row of a process node — it came from, so picking it on the canvas picks that.
///
/// The flow's own part is the first participant; the flows it talks to are theirs, a
/// program outside the project is its own, and a place is the store holding it, or the
/// record itself.
struct SequenceDiagram: Equatable {
    /// What a participant is, for how it is drawn.
    enum Role: Equatable { case own, flow, store, record, program, caller }

    struct Participant: Equatable {
        /// As mermaid names it: `Behavior/Store（Write）`, `Orders（store）`, a program's name (in the
        /// diagram's language).
        var label: String
        var name: String
        /// The section a flow sits in; nil for anything else.
        var place: String?
        var role: Role
        /// The card it is, when it is one of the project's.
        var cardId: String?
    }

    /// Where an item came from.
    struct Origin: Equatable {
        var node: String
        /// The row of a process node, by its place among the node's rows.
        var row: Int?
    }

    struct Message: Equatable {
        var from: Int
        var to: Int
        /// What the row does (read, write, call, send, receive, return); nil where the
        /// node's heading says it instead.
        var op: String?
        var text: String
        /// An answer, drawn dashed.
        var reply: Bool = false
        var origin: Origin?
        /// The node's word on it, when its heading stands in for the row.
        var detail: String?
    }

    struct Note: Equatable {
        /// plain: a wait, a way that goes back, what starts it. key: a signature made
        /// or checked. check: what a decision checks before going on. end: where a way
        /// stops. heading: a process node's heading over its rows.
        enum Kind: Equatable { case plain, key, check, end, heading }
        var over: Int
        var text: String
        var kind: Kind
        var origin: Origin?
    }

    struct Block: Equatable {
        enum Kind: String, Equatable { case alt, loop, par, `break` }
        var kind: Kind
        var sections: [Section]
        var origin: Origin?
    }

    struct Section: Equatable {
        var label: String
        var items: [Item]
    }

    indirect enum Item: Equatable {
        case message(Message)
        case note(Note)
        case block(Block)
    }

    var participants: [Participant] = []
    var items: [Item] = []
    /// The language the words it makes (a participant, a way, a note) and the ones `mermaid` adds are
    /// in: the screen's for what the app draws, the project's for what is written out.
    var language = Language.fallback

    /// The other end of a receive or a return that names none: whoever called the
    /// flow — the flows whose rows call it.
    nonisolated static func caller(in language: String) -> String {
        L10n.text(.Charts.caller, in: language)
    }

    /// The diagram of `card`, in `language`. `exported`: conditions with their paths as written out
    /// rather than as the app names them.
    static func make(_ card: SchemaNode, in detail: ProjectDetail, exported: Bool = false,
                     language: String) -> SequenceDiagram {
        SequenceBuilder(card: card, detail: detail, exported: exported, language: language).build()
    }

    // MARK: - As mermaid

    /// The diagram as mermaid's lines, from `sequenceDiagram` on.
    func mermaid() -> [String] {
        var lines = participants.enumerated().map { "    participant P\($0.offset + 1) as \(Self.safe($0.element.label))" }
        func words(_ list: [String?]) -> String { list.compactMap { $0 }.filter { !$0.isEmpty }.joined(separator: " ") }
        func emit(_ items: [Item], _ indent: String) {
            for item in items {
                switch item {
                case .message(let message):
                    lines.append("\(indent)P\(message.from + 1)\(message.reply ? "-->>" : "->>")P\(message.to + 1): "
                                 + Self.safe(words([message.op, message.text])))
                case .note(let note):
                    let said = note.kind == .check ? L10n.text(.Charts.noteCheck(note.text), in: language) : note.text
                    lines.append("\(indent)Note over P\(note.over + 1): " + Self.safe(said))
                case .block(let block):
                    for (index, section) in block.sections.enumerated() {
                        switch block.kind {
                        case .par:
                            lines.append(indent + (index == 0 ? "par" : "and") + (section.label.isEmpty ? "" : " " + Self.safe(section.label)))
                        default:
                            lines.append(indent + (index == 0 ? block.kind.rawValue : "else") + " " + Self.safe(section.label))
                        }
                        emit(section.items, indent + "    ")
                    }
                    if !block.sections.isEmpty { lines.append("\(indent)end") }
                }
            }
        }
        emit(items, "    ")
        return ["sequenceDiagram"] + lines
    }

    /// A condition written with paths, as the app names them.
    static func plain(_ text: String) -> String {
        NoteText.transform(text) { $0.shown ?? SchemaDirectory.exported(path: $0.path) }
    }

    /// A condition written with paths, as they are written out.
    static func plainText(_ text: String) -> String {
        NoteText.transform(text) { $0.shown ?? SchemaDirectory.plain(path: $0.path) }
    }

    /// Text mermaid takes in a sequence diagram, which also ends a message at `;`.
    static func safe(_ text: String) -> String {
        Ident.mermaidText(text).replacingOccurrences(of: ";", with: ",")
    }
}

/// Walks a flow's nodes — or an action's rows — into a `SequenceDiagram`: the begin
/// what starts it, a process node its rows, a run a call (headed by the node's label
/// when it has one), an end's answer a return, a decision an `alt` — its ways each its
/// own until they come together again, the `else` in the alt's else where it has a way
/// of its own — or, when its one way comes back to it, a `loop`, ways that part a `par`
/// until they meet, a wait a note — and where its time runs out, a way of its own. A
/// decision, a run or a wait whose other way stops at once in failure is drawn as the
/// way going on, with a `break` for where it stops. A way to a node already on the way
/// that is no loop is said, not drawn: "→ name へ".
private final class SequenceBuilder {
    typealias Item = SequenceDiagram.Item
    typealias Origin = SequenceDiagram.Origin
    typealias Note = SequenceDiagram.Note
    typealias Message = SequenceDiagram.Message
    typealias Block = SequenceDiagram.Block
    typealias Section = SequenceDiagram.Section

    let card: SchemaNode
    let detail: ProjectDetail
    let exported: Bool
    let language: String
    let identities: PathIdentities
    var out = SequenceDiagram()
    var ids: [String: Int] = [:]
    var me = 0
    var byName: [String: FlowNode] = [:]

    init(card: SchemaNode, detail: ProjectDetail, exported: Bool, language: String) {
        self.card = card
        self.detail = detail
        self.exported = exported
        self.language = language
        identities = PathIdentities(detail)
        out.language = language
    }

    func build() -> SequenceDiagram {
        let directory = detail.directory
        let home = directory.sectionId(of: card)
        let place = directory.exported(section: home)
        me = participant(say(.Charts.participantCard(place, card.name)), name: card.name, place: place, role: .own,
                         cardId: card.id)
        if card.kind == .flow {
            byName = Dictionary(card.nodes.map { ($0.name, $0) }, uniquingKeysWith: { a, _ in a })
            if let begin = card.nodes.first(where: { $0.kind == "begin" }) {
                if let started = trigger(of: begin) { out.items += starting(started, origin: Origin(node: begin.name)) }
                // From the begin itself, so ways that part right there are drawn too.
                out.items += walk(begin.name, until: [], [])
            }
        } else {
            if let started = card.trigger { out.items += starting(started, origin: nil) }
            out.items += rows(card.steps) { _ in nil }
        }
        if out.items.isEmpty { out.items = [note(say(.Charts.noteNoRows), .plain, nil)] }
        return out
    }

    // MARK: Participants

    func participant(_ label: String, name: String, place: String? = nil, role: SequenceDiagram.Role,
                     cardId: String? = nil) -> Int {
        if let known = ids[label] { return known }
        out.participants.append(.init(label: label, name: name, place: place, role: role, cardId: cardId))
        ids[label] = out.participants.count - 1
        return out.participants.count - 1
    }

    /// A flow it names, as the participant it is drawn as: its part, or the name as written when
    /// it is not there.
    func peer(_ peer: String) -> Int {
        let directory = detail.directory
        if let target = directory.find(peer, from: directory.sectionId(of: card), seeing: true).schema {
            let place = directory.exported(section: directory.sectionId(of: target))
            return participant(say(.Charts.participantCard(place, target.name)), name: target.name, place: place,
                               role: target.id == card.id ? .own : .flow, cardId: target.id)
        }
        return participant(peer, name: peer, role: .program)
    }

    /// Each one an exchange is with, how a way to it is named and the participant it is drawn as:
    /// the flows it names, then the program; whoever called it when it names no one.
    func others(peers: [String], program: String?) -> [(said: String, at: Int)] {
        var found = peers.map { (said: short([$0]), at: peer($0)) }
        if let program { found.append((program, participant(program, name: program, role: .program))) }
        if found.isEmpty {
            let caller = SequenceDiagram.caller(in: language)
            found.append((caller, participant(caller, name: caller, role: .caller)))
        }
        return found
    }

    func others(_ step: Step) -> [(said: String, at: Int)] { others(peers: step.peers, program: step.program) }

    /// The first one a row deals with: a call has the one.
    func other(_ step: Step) -> Int { others(step)[0].at }

    /// Whether what passes with several partners comes from them or goes to them.
    enum Direction { case from, to }

    /// What passes with each of `others`: as it is with one, a way of its own with each of
    /// several (from the partner, to the partner), as only one of them is dealt with at a time.
    func withEach(_ others: [(said: String, at: Int)], _ direction: Direction, _ origin: Origin?,
                  _ items: (Int) -> [Item]) -> [Item] {
        guard others.count > 1 else { return items(others[0].at) }
        let sections = others.map { Section(label: wayLabel(direction, $0.said), items: items($0.at)) }
        return [.block(Block(kind: .alt, sections: sections, origin: origin))]
    }

    func wayLabel(_ direction: Direction, _ said: String) -> String {
        switch direction {
        case .from: return say(.Charts.wayFrom(said))
        case .to: return say(.Charts.wayTo(said))
        }
    }

    /// Where a path is kept: the store holding its record, or the record.
    func place(_ path: String) -> Int {
        let directory = detail.directory
        guard let id = identities.identify(path, from: card)?.head, let head = identities.schema(id) else {
            let name = PathText.names(splitSection(path).rest).first ?? path
            return participant(name, name: name, role: .record)
        }
        if head.kind == .store {
            return participant(say(.Charts.participantStore(head.name)), name: head.name, role: .store, cardId: head.id)
        }
        // The store whose `holds` lines reach it (through a card ghost, the card it leads to).
        let store = detail.schemas.first { store in
            store.kind == .store && store.connections.contains { held in
                held.line == Line.holds && held.from.isEmpty && held.to.flatMap { directory.card(through: $0) }?.id == head.id
            }
        }
        if let store { return participant(say(.Charts.participantStore(store.name)), name: store.name, role: .store, cardId: store.id) }
        return participant(head.name, name: head.name, role: .record, cardId: head.id)
    }

    // MARK: Words

    func short(_ paths: [String]) -> String { paths.map { splitSection($0).1 }.joined(separator: ", ") }
    func words(_ list: [String?]) -> String { list.compactMap { $0 }.filter { !$0.isEmpty }.joined(separator: " ") }
    func aside(_ comment: String?) -> String? { comment.map { say(.Charts.aside($0)) } }
    func say(_ resource: LocalizedStringResource) -> String { L10n.text(resource, in: language) }
    func plain(_ text: String) -> String { exported ? SequenceDiagram.plainText(text) : SequenceDiagram.plain(text) }

    func message(_ from: Int, _ to: Int, _ op: String?, _ text: String, reply: Bool = false, _ origin: Origin?,
                 detail: String? = nil) -> Item {
        .message(Message(from: from, to: to, op: op, text: text, reply: reply, origin: origin, detail: detail))
    }

    func note(_ text: String, _ kind: Note.Kind, _ origin: Origin?) -> Item {
        .note(Note(over: me, text: text, kind: kind, origin: origin))
    }

    // MARK: What starts it

    /// A begin as the trigger an action had.
    func trigger(of begin: FlowNode) -> Trigger? {
        var trigger: Trigger?
        switch begin.trigger {
        case "receive"?:
            var received = Trigger(on: "receive")
            received.peers = begin.peers
            received.program = begin.program
            received.carries = begin.carries
            received.via = begin.via
            received.initiator = begin.initiator
            received.verifyWith = begin.verifyWith
            trigger = received
        case "schedule"?:
            var every = Trigger(on: "schedule")
            every.every = begin.every
            trigger = every
        case let other?:
            var said = Trigger(on: other)
            if other == "event", let event = begin.event { said.description = say(.Charts.eventOf(event.flow, event.outcome)) }
            trigger = said
        case nil:
            trigger = nil
        }
        if let said = begin.description { trigger?.description = said }
        return trigger
    }

    func starting(_ trigger: Trigger, origin: Origin?) -> [Item] {
        var items: [Item] = []
        switch trigger.on {
        case "receive":
            items += withEach(others(peers: trigger.peers, program: trigger.program), .from, origin) { from in
                if trigger.initiator == "self" {
                    return [message(me, from, say(.Charts.opFetch), words([trigger.via]), origin),
                            message(from, me, nil, words([short(trigger.carries), aside(trigger.description)]), reply: true, origin)]
                }
                return [message(from, me, "receive", words([short(trigger.carries), trigger.via, aside(trigger.description)]), origin)]
            }
            if let key = trigger.verifyWith { items.append(note(words(["verify", short([key])]), .key, origin)) }
        case "schedule":
            items.append(note(words(["schedule", trigger.every, aside(trigger.description)]), .plain, origin))
        default:
            items.append(note(words([trigger.on, aside(trigger.description)]), .plain, origin))
        }
        return items
    }

    // MARK: Rows

    /// Rows as an action has them, or a process node: `origin` says where each came from.
    func rows(_ steps: [Step], origin: (Int) -> Origin?) -> [Item] {
        var items: [Item] = []
        for (index, step) in steps.enumerated() {
            let at = origin(index)
            switch step.op {
            case "receive":
                items += withEach(others(step), .from, at) { from in
                    [message(from, me, "receive", words([short(step.carries), step.via, aside(step.description)]), at)]
                }
            case "return":
                items += withEach(others(step), .to, at) { to in
                    [message(me, to, "return", words([short(step.carries), aside(step.description)]), reply: true, at)]
                }
            case "send":
                // To each it names, one after another.
                for one in others(step) {
                    items.append(message(me, one.at, "send", words([short(step.carries), step.via, aside(step.description)]), at))
                }
            case "call":
                let to = other(step)
                items.append(message(me, to, "call", words([short(step.carries), step.via, aside(step.description)]), at))
                if !step.returns.isEmpty { items.append(message(to, me, nil, short(step.returns), reply: true, at)) }
            case "read", "write":
                for path in step.paths {
                    items.append(message(me, place(path), step.op, words([short([path]), aside(step.description)]), at))
                }
            case "sign", "verify":
                items.append(note(words([step.op, step.key.map { short([$0]) }, short(step.paths), aside(step.description)]), .key, at))
            case "note":
                // A flow's: a wait, or a way that goes back where it is not a loop.
                items.append(note(words([step.description]), .plain, at))
            case "compute":
                // What the flow works out by itself, over itself: what the row says it does.
                let said = step.description.flatMap { $0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? nil : $0 }
                items.append(note(said ?? step.op, .plain, at))
            case "sequence":
                items += rows(step.steps) { _ in at }
            case "unordered":
                if !step.steps.isEmpty {
                    items.append(.block(Block(kind: .par, sections: step.steps.map { child in
                        Section(label: "", items: rows([child]) { _ in at })
                    }, origin: at)))
                }
            case "switch":
                if !step.cases.isEmpty {
                    items.append(.block(Block(kind: .alt, sections: step.cases.map { item in
                        Section(label: plain(item.when ?? ""),
                                items: item.steps.isEmpty ? [note(say(.Charts.noteNothing), .plain, at)] : rows(item.steps) { _ in at })
                    }, origin: at)))
                }
            case "loop":
                items.append(.block(Block(kind: .loop, sections: [
                    Section(label: plain(step.over ?? ""), items: rows(step.steps) { _ in at }),
                ], origin: at)))
            default:
                break
            }
        }
        return items
    }

    // MARK: A flow's nodes

    /// Whether `from` leads back to `target` without going out by `avoiding`.
    func reaches(_ from: String, _ target: String, avoiding: String?) -> Bool {
        var seen: Set<String> = []
        var queue = [from]
        while let name = queue.popLast() {
            if name == target { return true }
            guard name != avoiding, seen.insert(name).inserted, let node = byName[name] else { continue }
            queue += node.exits.map(\.to)
        }
        return false
    }

    /// Where ways that part meet: the first join every one of them reaches.
    func meeting(_ ways: [String]) -> String? {
        func joins(from start: String) -> [String] {
            var seen: Set<String> = []
            var queue = [start]
            var found: [String] = []
            while !queue.isEmpty {
                let name = queue.removeFirst()
                guard seen.insert(name).inserted, let node = byName[name] else { continue }
                if node.kind == "join" { found.append(name); continue }
                queue += node.exits.map(\.to)
            }
            return found
        }
        guard let first = ways.first else { return nil }
        let rest = ways.dropFirst().map { Set(joins(from: $0)) }
        return joins(from: first).first { join in rest.allSatisfy { $0.contains(join) } }
    }

    /// Each node `start` reaches — not going back through `from`, nor on past `stops` — and how
    /// far along it is, in the order reached.
    func reach(_ start: String, from: String, until stops: Set<String>) -> [(name: String, far: Int)] {
        var seen: Set<String> = []
        var queue: [(String, Int)] = [(start, 0)]
        var found: [(name: String, far: Int)] = []
        while !queue.isEmpty {
            let (name, far) = queue.removeFirst()
            guard name != from, seen.insert(name).inserted else { continue }
            found.append((name, far))
            guard !stops.contains(name), let node = byName[name] else { continue }
            queue += node.exits.map { ($0.to, far + 1) }
        }
        return found
    }

    /// Where ways that part at `from` come together again: of the nodes every one of them
    /// reaches — not going back through `from`, nor on past `stops` — the nearest, the one
    /// the farthest of them reaches soonest. Nil for fewer than two ways.
    func rejoin(from: String, _ ways: [String], until stops: Set<String>) -> String? {
        guard ways.count >= 2 else { return nil }
        let rest = ways.dropFirst().map { way in
            Dictionary(reach(way, from: from, until: stops).map { ($0.name, $0.far) }, uniquingKeysWith: { a, _ in a })
        }
        var best: (farthest: Int, all: Int, name: String)?
        for (name, far) in reach(ways[0], from: from, until: stops) {
            let fars = rest.compactMap { $0[name] }
            guard fars.count == rest.count else { continue }
            let farthest = (fars + [far]).max() ?? 0, all = fars.reduce(far, +)
            if best.map({ (farthest, all) < ($0.farthest, $0.all) }) ?? true { best = (farthest, all, name) }
        }
        return best?.name
    }

    /// A way out of a node that parts: what it is called, where it goes, and whether it is
    /// left out when it has nothing of its own (the way on: a decision's else, a wait's next).
    struct Way {
        var label: String
        var to: String
        var quiet: Bool
    }

    /// The end the last walk stopped at on its own way, if it did (`parting`).
    var ended: String?

    /// Ways that part at `node` (a decision's, a wait's), each its own until they come
    /// together again, as an `alt`, a quiet way with nothing of its own (the way on) left out.
    /// Where they do not all come together, those that come together with none of the others
    /// (a way to an end of its own) are left to stop where they stop, and the others still
    /// come together. A way that stops, where the ways go on after the alt, says how: what it
    /// answers, or — it answers nothing, or it went wrong — its end too. Where it goes on:
    /// where they come together, nil when they do not.
    func parting(_ node: FlowNode, _ ways: [Way], until stops: Set<String>, _ onWay: Set<String>)
        -> (block: Item?, goOn: String?) {
        let all = ways.map(\.to)
        let meet = rejoin(from: node.name, all, until: stops) ?? {
            let reached = all.map { Set(reach($0, from: node.name, until: stops).map(\.name)) }
            let on = all.indices.filter { i in
                reached.indices.contains { j in j != i && !reached[j].isDisjoint(with: reached[i]) }
            }.map { all[$0] }
            return rejoin(from: node.name, on, until: stops)
        }()
        let until = stops.union([meet].compactMap { $0 })
        let at = Origin(node: node.name)
        var sections: [Section] = []
        for way in ways {
            var inner: [Item]
            if !until.contains(way.to), ends(way.to) {
                inner = stopping(way.to)
            } else {
                inner = walk(way.to, until: until, onWay)
                // Stopped at an end of its own while the others go on: how, when it is not said.
                if meet != nil, let end = ended, unanswered(end), let said = stopNote(end) { inner.append(said) }
                ended = nil
            }
            if inner.isEmpty {
                if way.quiet { continue }
                sections.append(Section(label: way.label, items: [note(say(.Charts.noteNothing), .plain, at)]))
            } else {
                sections.append(Section(label: way.label, items: inner))
            }
        }
        return (sections.isEmpty ? nil : .block(Block(kind: .alt, sections: sections, origin: at)), meet)
    }

    /// Whether an end says nothing of how it ends by what it answers: it answers nothing, or it
    /// went wrong.
    func unanswered(_ name: String) -> Bool {
        guard let node = byName[name] else { return false }
        return node.returnRow == nil || failing(name)
    }

    /// An end's note: what it is called, or how it ends.
    func stopNote(_ name: String) -> Item? {
        guard let node = byName[name] else { return nil }
        return note(node.label ?? node.outcome ?? "end", .end, Origin(node: name))
    }

    /// A way that goes straight to the end `name`: what it answers — and how it ends, where
    /// that is not said by it (`unanswered`).
    func stopping(_ name: String) -> [Item] {
        if unanswered(name) { return stop(name) }
        guard let node = byName[name], let answer = node.returnRow else { return [] }
        let at = Origin(node: name)
        return rows([answer]) { _ in at }
    }

    /// Whether the node is an end.
    func ends(_ name: String) -> Bool { byName[name]?.kind == "end" }

    /// Whether the node is an end where things went wrong.
    func failing(_ name: String?) -> Bool {
        guard let name, let node = byName[name], node.kind == "end" else { return false }
        return node.outcome == "failure" || node.outcome == "cancelled"
    }

    /// Where a way stops: its answer, then what the end is called.
    func stop(_ name: String) -> [Item] {
        guard let node = byName[name] else { return [] }
        let at = Origin(node: name)
        var items = node.returnRow.map { answer in rows([answer]) { _ in at } } ?? []
        items.append(note(node.label ?? node.outcome ?? "end", .end, at))
        return items
    }

    /// A process node's rows, under its heading when it has one.
    func process(_ node: FlowNode) -> [Item] {
        var items: [Item] = []
        if let heading = node.label, !heading.isEmpty { items.append(note(heading, .heading, Origin(node: node.name))) }
        return items + rows(node.steps) { Origin(node: node.name, row: $0) }
    }

    /// A run's call: under its heading when it has one, its rows' words otherwise.
    func run(_ node: FlowNode) -> [Item] {
        guard let call = node.callRow else { return [] }
        let at = Origin(node: node.name)
        guard let heading = node.label, !heading.isEmpty else { return rows([call]) { _ in at } }
        let to = other(call)
        var items = [message(me, to, nil, heading, at, detail: node.description)]
        if !call.returns.isEmpty { items.append(message(to, me, nil, short(call.returns), reply: true, at)) }
        return items
    }

    func walk(_ start: String?, until stops: Set<String>, _ onWayIn: Set<String>) -> [Item] {
        var items: [Item] = []
        var current = start
        var onWay = onWayIn
        var stoppedAt: String?
        while let name = current, !stops.contains(name) {
            guard let node = byName[name] else { break }
            guard onWay.insert(name).inserted else {
                items.append(note(say(.Charts.noteBack(name)), .plain, nil))
                break
            }
            let at = Origin(node: name)
            if node.nexts.count > 1 {
                // Ways that part, each its own until they meet.
                if node.kind == "process" { items += process(node) }
                if node.kind == "run" { items += run(node) }
                let meet = meeting(node.nexts)
                let sections = node.nexts.map { way in
                    Section(label: "", items: walk(way, until: stops.union([meet].compactMap { $0 }), onWay))
                }
                items.append(.block(Block(kind: .par, sections: sections, origin: at)))
                current = meet.flatMap { byName[$0]?.next }
                if let meet { onWay.insert(meet) }
                continue
            }
            switch node.kind {
            case "process":
                items += process(node)
                current = node.next
            case "run":
                items += run(node)
                current = node.next
                // Where it goes when what it ran fails.
                if let failed = node.onError, failed != node.next {
                    if failing(node.next), let next = node.next {
                        items.append(.block(Block(kind: .break, sections: [Section(label: say(.Charts.waySuccess), items: stop(next))],
                                                  origin: at)))
                        current = failed
                    } else if failing(failed) {
                        items.append(.block(Block(kind: .break, sections: [Section(label: say(.Charts.wayFailure), items: stop(failed))],
                                                  origin: at)))
                    } else {
                        let goOn = node.next
                        items.append(.block(Block(kind: .alt, sections: [
                            Section(label: say(.Charts.wayFailure), items: walk(failed, until: stops.union([goOn].compactMap { $0 }), onWay)),
                        ], origin: at)))
                    }
                }
            case "end":
                if let answer = node.returnRow { items += rows([answer]) { _ in at } }
                stoppedAt = name
                current = nil
            case "wait":
                let what = (node.waitFor == "event" ? node.event.map { say(.Charts.eventOf($0.flow, $0.outcome)) } : node.waitFor) ?? ""
                items.append(note(node.timeout.map { say(.Charts.noteWaitUntil(what, $0)) } ?? "wait \(what)", .plain, at))
                current = node.next
                // Where it goes when the time runs out: a break where it stops at once in
                // failure, else its own way beside the one on.
                if let late = node.onTimeout, late != node.next {
                    if failing(late) {
                        items.append(.block(Block(kind: .break, sections: [Section(label: say(.Charts.wayTimeout), items: stop(late))],
                                                  origin: at)))
                    } else {
                        let ways = (node.next.map { [Way(label: say(.Charts.wayInTime), to: $0, quiet: true)] } ?? [])
                            + [Way(label: say(.Charts.wayTimeout), to: late, quiet: false)]
                        let parted = parting(node, ways, until: stops, onWay)
                        if let block = parted.block { items.append(block) }
                        current = parted.goOn
                    }
                }
            case "eval":
                let ways = node.branches.map { (label: plain($0.when), to: $0.next, otherwise: false) }
                    + (node.otherwise.map { [(label: say(.Charts.wayOtherwise), to: $0, otherwise: true)] } ?? [])
                let stopping = ways.filter { failing($0.to) }, going = ways.filter { !failing($0.to) }
                if node.branches.count == 1, let way = node.branches.first,
                   reaches(way.next, node.name, avoiding: node.otherwise) {
                    items.append(.block(Block(kind: .loop, sections: [
                        Section(label: plain(way.when), items: walk(way.next, until: stops.union([node.name]), onWay)),
                    ], origin: at)))
                    current = node.otherwise
                } else if !stopping.isEmpty, going.count == 1 {
                    // A check: the way on, and a break where it stops.
                    let on = going[0]
                    if !on.otherwise { items.append(note(on.label, .check, at)) }
                    for way in stopping {
                        items.append(.block(Block(kind: .break, sections: [Section(label: way.label, items: stop(way.to))],
                                                  origin: at)))
                    }
                    current = on.to
                } else if node.branches.isEmpty {
                    current = node.otherwise
                } else {
                    // One way or the other, the else in the alt's else where it has a way of
                    // its own, until they come together again.
                    let parted = parting(node, ways.map { Way(label: $0.label, to: $0.to, quiet: $0.otherwise) },
                                         until: stops, onWay)
                    if let block = parted.block { items.append(block) }
                    current = parted.goOn
                }
            default:
                current = node.next
            }
        }
        // Said once this walk is done, after the walks inside it.
        ended = stoppedAt
        return items
    }
}
