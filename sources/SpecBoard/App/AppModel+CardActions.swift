import Foundation

/// One of what a goal, a task, a UI part or a card with implementation lines can be done with, as the core says of the card now (`card_read`'s
/// `actions`, `specboard://SpecBoard/Docs/Design:DesignBoard` §9). Whether it can be done is the core's
/// answer, never worked out here: `card_action` decides by the same rule.
struct CardAction: Equatable, Identifiable, Sendable {
    /// What `card_action` is called with: `start`, `finish`, `cancel` (a goal's), `done` (a task's) or `reviewed` (a part's, or a card's whose implementation lines' code is to be looked at).
    var op: String
    /// What a button or a menu item says, in the screen's language.
    var label: String
    /// What it does, for a tooltip.
    var description: String
    /// Whether `card_action` takes it now.
    var available: Bool
    /// Why it is not taken now, when it is not.
    var why: String?
    /// `start`'s: the branch's name it makes when none is given, which the name asked for starts as.
    var suggestedBranch: String?
    /// `cancel`'s, when the goal's branch has changes main does not: how many — the call is refused
    /// unless they are let go of with it.
    var discardChanges: Int?

    var id: String { op }

    init(op: String, label: String, description: String = "", available: Bool, why: String? = nil,
         suggestedBranch: String? = nil, discardChanges: Int? = nil) {
        self.op = op
        self.label = label
        self.description = description
        self.available = available
        self.why = why
        self.suggestedBranch = suggestedBranch
        self.discardChanges = discardChanges
    }

    init?(_ json: [String: Any]) {
        guard let op = json["op"] as? String else { return nil }
        self.init(op: op, label: json["label"] as? String ?? op, description: json["description"] as? String ?? "",
                  available: json["available"] as? Bool ?? false, why: json["why"] as? String,
                  suggestedBranch: json["suggestedBranch"] as? String,
                  discardChanges: (json["discardChanges"] as? NSNumber)?.intValue)
    }

    /// What a button or menu item for it says when it cannot be pressed, or what it does when it can.
    var help: String { available ? description : why ?? description }
}

/// A goal's cancel asked about first: its branch has changes main does not, and cancelling throws them
/// away with the branch.
struct CardDiscardAsk: Identifiable {
    let id = UUID()
    /// The goal, by its id.
    var card: String
    var branch: String
    var changes: Int
}

extension AppModel {
    /// What is kept of the cards' actions starts with this (`aside`'s keys).
    static let cardActionsKey = "actions|"

    /// What a goal or a task (on the board), a UI part (on main or a branch) or a card whose implementation
    /// lines' code is to be looked at can be done with, as the core says now: none for any other card, nor where
    /// a card of its kind is not. The whole tree's are read in one go (`treeCardActions`), not a card's at a
    /// time: a menu may be built along with each row, and each read builds the project.
    func cardActions(_ id: String) -> [CardAction] {
        guard let card = schema(id), carriesActions(card), let path = try? cardPath(id) else { return [] }
        // A card with lines to look at is read apart from the parts and the board's cards: its kind says
        // none of this, the core adds `reviewed` for its lines (`codeActions`).
        if !isBoardOpen, !card.kind.isUiPart { return codeActions()[path] ?? [] }
        return treeCardActions()[path] ?? []
    }

    /// Whether `card` has actions in the tree open: a goal or a task on the board, a UI part anywhere but
    /// there (not in what a merge would leave, which no tool names), and in a tree of cards a card with
    /// implementation lines to look at again (`codeReview`), whatever its kind.
    private func carriesActions(_ card: SchemaNode) -> Bool {
        if isBoardOpen { return card.kind == .goal || card.kind == .task }
        guard resultView == nil else { return false }
        return card.kind.isUiPart || !card.codeReview.isEmpty
    }

    /// The actions of every card of the open tree that has some by their paths (`card_list`'s
    /// `include: ["actions"]`, every page), read once until the next write (`letGoOfCardActions`): the
    /// board's goals and tasks, or a tree's parts. Narrowed to the kinds that carry actions, so the read
    /// does not grow with the cards that have none.
    private func treeCardActions() -> [String: [CardAction]] {
        guard let projectId = detail?.project.id else { return [:] }
        let tree = openTrees[projectId] ?? ""
        let kinds = isBoardOpen ? ["goal", "task"] : ["html", "svg", "frame"]
        return actionsRead(key: "\(Self.cardActionsKey)\(projectId)|\(tree)", ["kind": kinds])
    }

    /// The actions of the cards of the open tree whose implementation lines' code changed or was never
    /// recorded, by their paths: read once like the others.
    private func codeActions() -> [String: [CardAction]] {
        guard let projectId = detail?.project.id else { return [:] }
        let tree = openTrees[projectId] ?? ""
        return actionsRead(key: "\(Self.cardActionsKey)\(projectId)|\(tree)|code",
                           ["implementation": ["changed", "unrecorded"]])
    }

    /// The actions of the cards `filter` picks (`card_list`'s value), by their paths, read every page.
    private func actionsRead(key: String, _ filter: [String: Any]) -> [String: [CardAction]] {
        var value = filter
        value["include"] = ["actions"]
        value["limit"] = 200
        return aside(key, "card_list", value: value, paged: true) { json in
            var found: [String: [CardAction]] = [:]
            for card in json?["cards"] as? [[String: Any]] ?? [] {
                guard let path = card["path"] as? String, let actions = card["actions"] as? [[String: Any]] else { continue }
                found[path] = actions.compactMap(CardAction.init)
            }
            return found
        }
    }

    /// The actions a button or a menu offers: those the core takes now, where the cards can be written (and,
    /// on a locked main, a card's review of its implementation lines).
    func availableCardActions(_ id: String) -> [CardAction] {
        if !isReadOnly { return cardActions(id).filter(\.available) }
        // A locked main still takes the code of a card's implementation lines recorded again: it is kept
        // beside the tree, not written in it.
        guard isLockedMainOpen, !isArchived, heldByMCP.isEmpty, heldElsewhere.isEmpty,
              let card = schema(id), !card.kind.isUiPart, !card.codeReview.isEmpty else { return [] }
        return cardActions(id).filter { $0.available && $0.op == "reviewed" }
    }

    /// The actions as read before a write: read again after it, as a write may change any of them
    /// (a card's status, a branch made or deleted). What was read shows until then; a read still out
    /// is not taken in, as it answers for before the write.
    func letGoOfCardActions() {
        for key in asideKept.keys where key.hasPrefix(Self.cardActionsKey) {
            asideBefore[key] = asideKept.removeValue(forKey: key)
        }
        asideAsked = asideAsked.filter { !$0.hasPrefix(Self.cardActionsKey) }
    }

    /// An action pressed, on a button or in a menu: start asks for the branch's name first, starting
    /// from the one the core would make; cancel, where the goal's branch has changes main does not,
    /// asks whether to throw them away; the rest are done at once.
    func pressCardAction(_ id: String, _ action: CardAction) {
        if action.op == "start" {
            nameAsk = NameAsk(title: L10n[.Board.actionStartTitle(schema(id)?.name ?? "")], message: L10n[.Board.actionStartMessage],
                              placeholder: L10n[.Board.actionStartPlaceholder], start: action.suggestedBranch ?? "",
                              confirm: L10n[.Board.actionStartConfirm], cancel: L10n[.Common.actionCancel]) { [weak self] name in
                self?.runCardAction(id, "start", ["branch": name])
            }
        } else if action.op == "cancel", let changes = action.discardChanges, changes > 0 {
            let branch = schema(id)?.customFields["workBranch"] ?? ""
            discardAsk = CardDiscardAsk(card: id, branch: branch, changes: changes)
        } else {
            runCardAction(id, action.op)
        }
    }

    /// `card_action` called on a card: one write. Once it is read again, a card it moved (to the board's
    /// Complete or Cancelled) is shown where it went, the branches are read again when it made or
    /// deleted one, and what it warns of (a finished goal's tasks not done) is said. Refused, the
    /// core's words are said as any write's are.
    func runCardAction(_ id: String, _ op: String, _ value: [String: Any] = [:]) {
        guard let path = try? cardPath(id) else { return }
        var answer: [String: Any] = [:]
        write {
            let said = try tool("card_action", target: path, value: value, ["op": op])
            if case .object(let wrapped) = said, let json = wrapped["json"],
               let read = try? JSONSerialization.jsonObject(with: Data(json.compact().utf8)) as? [String: Any] {
                answer = read
            }
        } then: { [weak self] in
            self?.cardActed(answer)
        }
    }

    private func cardActed(_ answer: [String: Any]) {
        if answer["branchCreated"] != nil || answer["branchDeleted"] != nil { readBranchesAgain() }
        if let warnings = answer["warnings"] as? [String], !warnings.isEmpty {
            notice = warnings.joined(separator: "\n")
        }
        if answer["movedTo"] != nil, let path = answer["path"] as? String { show(path: path) }
    }
}
