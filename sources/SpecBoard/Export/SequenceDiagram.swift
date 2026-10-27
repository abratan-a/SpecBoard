import Foundation

/// A flow's sequence diagram — or an actor's chart — as the core builds it (`sequence_chart`,
/// `specboard://SpecBoard/Docs/Design:Overview` §11「シーケンス図の生命線と帯」), read to be drawn on the app's own
/// canvas (`SequenceCanvas`). The screen builds none of it: what it draws, what is written out and what MCP
/// reads are one picture with one rev.
///
/// Who takes part (actors as the flow sees them, stores, flows, programs, anyone where a begin takes any
/// caller, and a lane for each kind of what is not written: the partner, the store), then what passes
/// between them in order: arrows, notes, a step's bar with its rows'
/// arrows inside, and the blocks a decision, a loop, ways that part or an actor's provided flow make. Each
/// item says which card, node — and row of a process node — it came from, so picking it picks that.
struct SequenceDiagram: Equatable {
    /// What a participant is, for how it is drawn.
    enum Role: String, Equatable {
        case own, flow, actor, store, program
        /// Whoever calls a begin that takes any caller.
        case anyone
        /// The lane of the partners not written: a gap, never filled in from who calls.
        case noPartner
        /// The lane of the shapes no store keeps: a gap too.
        case noStore

        /// A lane of what is not written.
        var isGap: Bool { self == .noPartner || self == .noStore }
    }

    struct Participant: Equatable {
        /// As the core names it: the section path beside the name where two share one.
        var label: String
        var name: String
        /// An actor's own path (opened from its head), a flow's section.
        var place: String?
        var role: Role
        /// The card it is: a flow, a store.
        var cardId: String?
        /// The actor's section.
        var section: String?
        var figure: ActorFigure?
        /// The actor it is bracketed in with its siblings — ⌃ folds it into that one — and that one's name.
        var group: String?
        var groupName: String?
        /// Whether opening it would draw an actor inside it that is not drawn now: ⌄ opens it into them.
        var open: Bool
        /// The mark its section is drawn with.
        var mark: SectionMark?
        /// A gap's lane: where its first gap is, what pressing its head picks.
        var origin: Origin? = nil

        /// An actor's own lifeline, drawn first in its own bracket beside the actors inside it.
        var isItselfInItsBracket: Bool { role == .actor && group != nil && group == section }
    }

    /// Where an item came from: picking it picks this card, node and row.
    struct Origin: Hashable {
        var card: String?
        var node: String?
        /// The row of a process node, by its place among the node's rows.
        var row: Int?

        init(card: String? = nil, node: String? = nil, row: Int? = nil) {
            self.card = card
            self.node = node
            self.row = row
        }
    }

    struct Message: Equatable {
        var from: Int
        var to: Int
        /// What the row does (`receive`, `send`, `run`, `read`, `write`…); nil on an answer to a run or a call.
        var op: String?
        /// What it carries.
        var text: String
        /// The flow it reaches on another actor's lifeline: said after the text.
        var flow: String?
        /// An answer, drawn dashed.
        var reply: Bool = false
        var origin: Origin?
        /// What the row says of itself: the tip and the property panel, not the arrow.
        var description: String?
    }

    struct Note: Equatable {
        /// plain: a wait, what starts it. key: a signature made or checked. check: what a decision checks
        /// before going on. end: where a way stops. mark: a way that only fails (`✕`) or the end of a flow
        /// nobody waits for (`■`), beside the lifeline.
        enum Kind: String, Equatable { case plain, key, check, end, mark }
        var over: Int
        var text: String
        var kind: Kind
        var origin: Origin?
        var description: String?
    }

    /// A process or run node's bar on its lifeline, its rows' arrows inside.
    struct Step: Equatable {
        var over: Int
        /// The node's name; none on a call into a flow of the same lifeline.
        var node: String?
        /// The node's heading, or the flow called.
        var label: String
        var items: [Item]
        var origin: Origin?
        /// The flow it stands for, where it has no node (the core's `flow`, `op`, `place`): one called on this
        /// lifeline, or reached on another.
        var reached: Reached?
    }

    /// A flow a bar with no node stands for, as the core says it.
    struct Reached: Equatable {
        var flow: String
        /// How it is reached (`run`, `send`…); empty for an answer.
        var op: String
        /// Its name and label, as the project has them.
        var name: String
        var label: String
        /// Its section, under the tree's top (`注文サーバ/在庫`).
        var place: String
        /// A call into a flow of the same lifeline: the core's key of the call.
        var call: String? = nil
        /// Not opened: the call drawn as one line.
        var folded = false
        /// A call into a flow drawn above already, never opened.
        var cycle = false
    }

    struct Block: Equatable {
        enum Kind: String, Equatable { case alt, loop, par, `break`, flow }
        var kind: Kind
        var sections: [Section]
        var origin: Origin?
        /// A provided flow's frame (an actor's chart): the flow's id, its label and name.
        var flow: String?
        var label: String?
        var name: String?
    }

    struct Section: Equatable {
        var label: String
        var items: [Item]
    }

    indirect enum Item: Equatable {
        case message(Message)
        case note(Note)
        case step(Step)
        case block(Block)
    }

    var participants: [Participant] = []
    var items: [Item] = []
    /// The lifeline of the flow opened (drawn with a thick border); none in an actor's chart.
    var me: Int?
    /// Why nothing is drawn: too large, or refused.
    var unshown: String?
    /// The language its words are in.
    var language = Language.fallback

    init() {}

    /// The core's answer (`result` of `sequence_chart`) read; the names it gives by id only — a group's
    /// actor, a framed flow — and the marks of the actors read in `detail`.
    init(chart: Any, detail: ProjectDetail?, language: String) {
        self.language = language
        guard let chart = chart as? [String: Any] else { return }
        if let why = chart["tooLarge"] as? String {
            unshown = why
            return
        }
        let sections = Dictionary((detail?.sections ?? []).map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        func origin(_ value: Any?) -> Origin? {
            guard let value = value as? [String: Any] else { return nil }
            return Origin(card: value["card"] as? String, node: value["node"] as? String, row: value["row"] as? Int)
        }
        participants = (chart["participants"] as? [[String: Any]] ?? []).map { one in
            let section = one["section"] as? String
            let group = one["group"] as? String
            return Participant(label: one["label"] as? String ?? "", name: one["name"] as? String ?? "",
                               place: one["place"] as? String,
                               role: (one["role"] as? String).flatMap(Role.init(rawValue:)) ?? .program,
                               cardId: one["card"] as? String, section: section,
                               figure: (one["figure"] as? String).flatMap(ActorFigure.init(rawValue:)),
                               group: group, groupName: group.flatMap { sections[$0]?.name },
                               open: one["open"] as? Bool ?? false,
                               mark: section.flatMap { sections[$0] }.flatMap { detail?.mark(of: $0) },
                               origin: origin(one["origin"]))
        }
        me = chart["me"] as? Int
        func items(_ value: Any?) -> [Item] {
            (value as? [[String: Any]] ?? []).compactMap { wrapped -> Item? in
                if let m = wrapped["message"] as? [String: Any] {
                    return .message(Message(from: m["from"] as? Int ?? 0, to: m["to"] as? Int ?? 0, op: m["op"] as? String,
                                            text: m["text"] as? String ?? "", flow: m["flow"] as? String,
                                            reply: m["reply"] as? Bool ?? false, origin: origin(m["origin"]),
                                            description: m["description"] as? String))
                }
                if let n = wrapped["note"] as? [String: Any] {
                    return .note(Note(over: n["over"] as? Int ?? 0, text: n["text"] as? String ?? "",
                                      kind: (n["kind"] as? String).flatMap(Note.Kind.init(rawValue:)) ?? .plain,
                                      origin: origin(n["origin"]), description: n["description"] as? String))
                }
                if let s = wrapped["step"] as? [String: Any] {
                    var step = Step(over: s["over"] as? Int ?? 0, node: s["node"] as? String,
                                    label: s["label"] as? String ?? "", items: items(s["items"]), origin: origin(s["origin"]))
                    if let flow = s["flow"] as? String {
                        let card = detail?.schemas.first { $0.id == flow }
                        let place = (s["place"] as? String ?? "").split(separator: "/").dropFirst().joined(separator: "/")
                        step.reached = Reached(flow: flow, op: s["op"] as? String ?? "", name: card?.name ?? "",
                                               label: card?.label ?? "", place: place, call: s["call"] as? String,
                                               folded: s["folded"] as? Bool ?? false, cycle: s["cycle"] as? Bool ?? false)
                    }
                    return .step(step)
                }
                if let b = wrapped["block"] as? [String: Any] {
                    let flow = b["flow"] as? String
                    let card = flow.flatMap { id in detail?.schemas.first { $0.id == id } }
                    return .block(Block(kind: (b["kind"] as? String).flatMap(Block.Kind.init(rawValue:)) ?? .alt,
                                        sections: (b["sections"] as? [[String: Any]] ?? []).map {
                                            Section(label: $0["label"] as? String ?? "", items: items($0["items"]))
                                        },
                                        origin: origin(b["origin"]), flow: flow, label: b["label"] as? String,
                                        name: card?.name))
                }
                return nil
            }
        }
        self.items = items(chart["items"])
    }

    /// Why nothing is drawn, as the diagram it is drawn as: the reason over no lifeline.
    init(refused why: String, language: String) {
        self.language = language
        unshown = why
    }

    /// The request `sequence_chart` takes: a flow's chart (`card`) or an actor's (`section`), the actors
    /// chosen to draw where they hold a partner (`lifelines`; nil for the default depth), the calls into
    /// flows of one lifeline to open, at any depth (`opened`: their keys; none for all folded), in `language`.
    static func request(project: String, tree: String, card: String? = nil, section: String? = nil,
                        lifelines: [String]?, opened: [String]? = nil, language: String) -> [String: Any] {
        var fields: [String: Any] = ["project": project, "branch": tree, "lang": language]
        if let card { fields["card"] = card }
        if let section { fields["section"] = section }
        if let lifelines { fields["lifelines"] = lifelines }
        if let opened, !opened.isEmpty { fields["open"] = opened }
        return fields
    }

    /// Whether a row or a node of the card `card` is drawn in it: the diagram's own flow, or a flow whose
    /// call is opened in it.
    func draws(_ card: String) -> Bool {
        Self.flattened(items).contains { item in
            switch item {
            case .message(let message): return message.origin?.card == card
            case .note(let note): return note.origin?.card == card
            case .step(let step): return step.origin?.card == card
            case .block(let block): return block.origin?.card == card
            }
        }
    }

    /// Every item, the ones inside bars and blocks too, in order.
    static func flattened(_ items: [Item]) -> [Item] {
        items.flatMap { item -> [Item] in
            switch item {
            case .message, .note: return [item]
            case .step(let step): return [item] + flattened(step.items)
            case .block(let block): return [item] + block.sections.flatMap { flattened($0.items) }
            }
        }
    }
}

/// The actors a sequence diagram draws where they hold a partner, as chosen on this Mac with the
/// lifelines' ⌃ and ⌄ (`specboard://SpecBoard/Docs/Design:UI`「シーケンス図」): kept for each flow — and each
/// actor's chart — of each project, as a view is, never written to the project. None kept: the core's
/// default depth, as seen from the flow.
enum SequenceLifelines {
    /// A lifeline's chip pressed: fold it into the actor it is bracketed in, or open an actor into the
    /// actors inside it.
    enum Press: Equatable {
        case fold(into: String)
        case open(String)
    }

    static func key(project: String, of: String) -> String { "sequence.lifelines.\(project).\(of)" }

    /// The choice kept for `of` (a flow's or an actor section's id); nil for the default.
    static func kept(project: String, of: String) -> [String]? {
        UserDefaults.standard.stringArray(forKey: key(project: project, of: of))
    }

    static func keep(_ chosen: [String]?, project: String, of: String) {
        if let chosen {
            UserDefaults.standard.set(chosen, forKey: key(project: project, of: of))
        } else {
            UserDefaults.standard.removeObject(forKey: key(project: project, of: of))
        }
    }

    /// The choice after `press`, from `current`: folding into G adds G and drops what is inside it; opening S
    /// drops S and adds the actors right inside it. Opening never goes back to the default: the default
    /// draws S folded when S does not hold the flow, so it would open into nothing.
    static func pressed(_ press: Press, current: [String]?, in detail: ProjectDetail) -> [String]? {
        var chosen = current ?? []
        switch press {
        case .fold(let group):
            let inside = Set(descendants(of: group, in: detail))
            chosen.removeAll { inside.contains($0) || $0 == group }
            chosen.append(group)
            return chosen
        case .open(let section):
            chosen.removeAll { $0 == section }
            for inner in detail.sections where inner.parentId == section && inner.isActor && !chosen.contains(inner.id) {
                chosen.append(inner.id)
            }
            return chosen
        }
    }

    /// The choice after `press` kept for `of`, and returned.
    @discardableResult
    static func press(_ press: Press, project: String, of: String, in detail: ProjectDetail) -> [String]? {
        let chosen = pressed(press, current: kept(project: project, of: of), in: detail)
        keep(chosen, project: project, of: of)
        return chosen
    }

    private static func descendants(of id: String, in detail: ProjectDetail) -> [String] {
        var out: [String] = []
        var queue = [id]
        while let one = queue.popLast() {
            for inner in detail.sections where inner.parentId == one {
                out.append(inner.id)
                queue.append(inner.id)
            }
        }
        return out
    }
}

/// The calls into flows of one lifeline opened on a sequence diagram on this Mac, kept for each flow — and each
/// actor's chart — of each project, as the lifelines chosen are, never written to the project.
enum SequenceCalls {
    static func key(project: String, of: String) -> String { "sequence.calls.\(project).\(of)" }

    /// The keys of the calls opened on `of`'s chart (a flow's or an actor section's id), at any depth; nil for
    /// none: every call is a folded line.
    static func kept(project: String, of: String) -> [String]? {
        UserDefaults.standard.stringArray(forKey: key(project: project, of: of)).flatMap { $0.isEmpty ? nil : $0 }
    }

    static func keep(_ opened: [String]?, project: String, of: String) {
        if let opened, !opened.isEmpty {
            UserDefaults.standard.set(opened, forKey: key(project: project, of: of))
        } else {
            UserDefaults.standard.removeObject(forKey: key(project: project, of: of))
        }
    }

    /// One call opened or folded: its key kept, or dropped.
    static func setOpen(_ open: Bool, call: String, project: String, of: String) {
        var keys = kept(project: project, of: of) ?? []
        keys.removeAll { $0 == call }
        if open { keys.append(call) }
        keep(keys, project: project, of: of)
    }
}

/// How an actor's section is seen on this Mac: its map or its sequence (`Map | Sequence`), and which of its
/// provided flows' frames are folded — every frame is open until one is folded. View state, never written to the
/// project.
enum ActorView {
    static func sequenceKey(project: String, actor: String) -> String { "actor.sequence.\(project).\(actor)" }
    static func framesKey(project: String, actor: String) -> String { "actor.frames.\(project).\(actor)" }

    /// Whether the actor shows its sequence rather than its map.
    static func showsSequence(project: String, actor: String) -> Bool {
        UserDefaults.standard.bool(forKey: sequenceKey(project: project, actor: actor))
    }

    static func setShowsSequence(_ on: Bool, project: String, actor: String) {
        UserDefaults.standard.set(on, forKey: sequenceKey(project: project, actor: actor))
    }

    /// The frames kept folded (false) by the flow's id; a frame with no entry is open (so is one kept as true).
    static func pressed(project: String, actor: String) -> [String: Bool] {
        UserDefaults.standard.dictionary(forKey: framesKey(project: project, actor: actor)) as? [String: Bool] ?? [:]
    }

    /// The frames of `flows` (ids) folded to one line: the ones kept as folded.
    static func folded(_ flows: [String], project: String, actor: String) -> Set<String> {
        let pressed = pressed(project: project, actor: actor)
        return Set(flows.filter { pressed[$0] == false })
    }

    /// A frame opened or folded: only a folded one is kept, so that a frame added later opens as the others do.
    static func setOpen(_ open: Bool, flow: String, project: String, actor: String) {
        var pressed = pressed(project: project, actor: actor)
        if open { pressed.removeValue(forKey: flow) } else { pressed[flow] = false }
        if pressed.isEmpty {
            UserDefaults.standard.removeObject(forKey: framesKey(project: project, actor: actor))
        } else {
            UserDefaults.standard.set(pressed, forKey: framesKey(project: project, actor: actor))
        }
    }

    /// What this Mac keeps of `actor` forgotten.
    static func forget(project: String, actor: String) {
        UserDefaults.standard.removeObject(forKey: sequenceKey(project: project, actor: actor))
        UserDefaults.standard.removeObject(forKey: framesKey(project: project, actor: actor))
        SequenceLifelines.keep(nil, project: project, of: actor)
        SequenceCalls.keep(nil, project: project, of: actor)
    }
}
