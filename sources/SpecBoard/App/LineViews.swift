import AppKit
import SwiftUI

// What the window shows of lines drawn by hand and picked (overview §13「つなぐ」「線を選ぶ」):
// the one choice every way of drawing comes to (`ConnectView`), where it is shown on a
// diagram (`ConnectAnchor`), the lines leaving what is picked in the property panel
// (`LineList`), and a line picked (`LineProperties`).

/// つなぐ: from a place, a line to another — the other end picked here (`PathSelector`) unless
/// it was dropped on, then the kinds of line the place may draw there, by their definitions'
/// words (`label`, `description`) and how they are drawn. Which kinds those are is the
/// core's to say (`AppModel.drawable`). Every way in — a row's handle dragged, 「つなぐ…」 on a
/// row's or a header's right-click, the property panel's 「＋ つなぐ…」 — comes to this one.
/// A kind whose line needs a field (a branch's condition, a key's use) asks it here.
struct ConnectView: View {
    var model: AppModel
    var source: LineSource
    var done: () -> Void

    @State private var target: String?
    @State private var answer: Result<DrawableLines, Problem>?
    /// What is written for the fields a kind's line needs, by `<kind>.<field>`.
    @State private var needed: [String: String] = [:]
    /// Why the line chosen was not written.
    @State private var failed: String?

    /// What the core said instead of the kinds of line: why it cannot say them.
    struct Problem: Error, Equatable { var said: String }

    init(model: AppModel, source: LineSource, target: String?, done: @escaping () -> Void) {
        self.model = model
        self.source = source
        self.done = done
        _target = State(initialValue: target)
        _answer = State(initialValue: target.map { Self.read(model, source, $0) })
    }

    /// What `source` may draw to `target`, or why that cannot be said.
    static func read(_ model: AppModel, _ source: LineSource, _ target: String) -> Result<DrawableLines, Problem> {
        do {
            return .success(try model.drawable(from: source, to: target))
        } catch {
            return .failure(Problem(said: error.localizedDescription))
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 5) {
                Image(systemName: "arrow.right.circle").foregroundStyle(.secondary)
                Text(L10n[.Lines.actionConnect]).font(.app(12, weight: .semibold))
                Text(source.said)
                    .font(.app(11, design: .monospaced)).foregroundStyle(.secondary)
                    .lineLimit(1).truncationMode(.middle)
            }
            if let target, let answer {
                choices(target, answer)
            } else {
                Text(L10n[.Lines.connectPickTarget]).font(.app(11)).foregroundStyle(.secondary)
                // From the board, main's cards too: only a `link` reaches them,
                // as the core says when one is picked.
                PathSelector(detail: model.detail, owner: nil, wanted: .link, outside: model.outsideDetail,
                             words: model.outsideWords, outlines: model.outlines,
                             askOutline: { model.askOutlines([$0]) }) { picked in
                    target = picked
                    answer = Self.read(model, source, picked)
                    failed = nil
                }
            }
        }
        .padding(12)
        .frame(width: 420, alignment: .leading)
    }

    /// The kinds of line it may draw to `target`, one to press each; those it has as many of
    /// as its kind lets it, said and not pressed.
    @ViewBuilder
    private func choices(_ target: String, _ answer: Result<DrawableLines, Problem>) -> some View {
        switch answer {
        case .failure(let problem):
            Text(problem.said).font(.app(11)).foregroundStyle(.orange)
                .fixedSize(horizontal: false, vertical: true)
        case .success(let lines):
            Text("→ \(Self.reached(lines, target))")
                .font(.app(11.5, design: .monospaced)).lineLimit(1).truncationMode(.middle)
            Text(L10n[.Lines.connectPickKind]).font(.app(11, weight: .semibold)).foregroundStyle(.secondary)
            if lines.lines.isEmpty {
                Text(L10n[.Lines.connectNoKinds])
                    .font(.app(11)).foregroundStyle(.tertiary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            ScrollView {
                VStack(alignment: .leading, spacing: 6) {
                    ForEach(lines.lines) { kind in kindRow(kind, lines, target) }
                    ForEach(lines.full) { kind in fullRow(kind) }
                }
            }
            .frame(maxHeight: 360)
            .fixedSize(horizontal: false, vertical: true)
        }
        if let failed {
            Text(failed).font(.app(11)).foregroundStyle(.orange).fixedSize(horizontal: false, vertical: true)
        }
        Button(L10n[.Lines.connectPickAgain]) {
            self.target = nil
            self.answer = nil
            failed = nil
        }
        .buttonStyle(.link)
        .font(.app(11))
    }

    /// Where the line would land, as a person reads it.
    static func reached(_ lines: DrawableLines, _ target: String) -> String {
        guard let to = lines.to else { return target }
        guard let at = lines.at, !at.isEmpty else { return to }
        return "\(to).\(at)"
    }

    /// One kind to press: how it is drawn, its name and what it means — and the fields its
    /// line cannot be written without.
    private func kindRow(_ kind: DrawableLines.Kind, _ lines: DrawableLines, _ target: String) -> some View {
        let definition = model.detail?.lineKinds[kind.line]
        let asked = (definition?.fields ?? []).filter { kind.required?.contains($0.name) == true }
        let filled = asked.allSatisfy { !(needed["\(kind.line).\($0.name)"] ?? "").isEmpty }
        return VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 6) {
                LineSample(look: definition?.look ?? LineLook())
                Text(kind.label).font(.app(12, weight: .semibold))
                Text(kind.line).font(.app(10, design: .monospaced)).foregroundStyle(.tertiary)
                if let count = kind.count, count > 0 {
                    Text(L10n[.Lines.connectCurrentCount(count)]).font(.app(10)).foregroundStyle(.tertiary)
                }
                Spacer(minLength: 4)
                Button(L10n[.Lines.actionConnect]) { pick(kind, lines, target, values(kind, asked)) }
                    .controlSize(.small)
                    .disabled(!filled)
            }
            if let description = kind.description, !description.isEmpty {
                Text(description).font(.app(10.5)).foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            ForEach(asked) { field in
                HStack(spacing: 6) {
                    Text(field.said).font(.app(10.5)).foregroundStyle(.secondary)
                    let binding = Binding(get: { needed["\(kind.line).\(field.name)"] ?? "" },
                                          set: { needed["\(kind.line).\(field.name)"] = $0 })
                    if field.options.isEmpty {
                        TextField(field.name, text: binding).textFieldStyle(.roundedBorder).font(.app(11))
                    } else {
                        // Side by side while they fit the window, a menu when they do not.
                        FittingChoice(options: .words(field.options, none: true), selection: binding, segmented: true)
                    }
                }
            }
        }
        .padding(7)
        .background(RoundedRectangle(cornerRadius: 6).fill(Color.secondary.opacity(0.07)))
        .contentShape(Rectangle())
        .onTapGesture(count: 2) { if asked.isEmpty { pick(kind, lines, target, [:]) } }
    }

    /// A kind it has as many lines of as its kind lets it: said, not pressed.
    private func fullRow(_ kind: DrawableLines.Kind) -> some View {
        HStack(spacing: 6) {
            LineSample(look: model.detail?.lineKinds.look(kind.line) ?? LineLook())
            Text(kind.label).font(.app(12)).foregroundStyle(.secondary)
            Text(Self.fullWords(kind)).font(.app(10.5)).foregroundStyle(.tertiary)
        }
        .padding(.horizontal, 7)
    }

    /// Why a kind is not offered: as many as it may have.
    static func fullWords(_ kind: DrawableLines.Kind) -> String {
        L10n[.Lines.connectFullKind(kind.max ?? 0, kind.count ?? 0)]
    }

    /// The fields asked, as the line writes them.
    private func values(_ kind: DrawableLines.Kind, _ asked: [LineField]) -> [String: JSONValue] {
        var out: [String: JSONValue] = [:]
        for field in asked {
            let text = (needed["\(kind.line).\(field.name)"] ?? "").trimmingCharacters(in: .whitespaces)
            if !text.isEmpty { out[field.name] = .string(text) }
        }
        return out
    }

    private func pick(_ kind: DrawableLines.Kind, _ lines: DrawableLines, _ target: String, _ fields: [String: JSONValue]) {
        model.connect(source, line: kind.line, to: lines.to ?? target, at: lines.at, fields: fields)
        if let error = model.lastError {
            failed = error
        } else {
            done()
        }
    }
}

/// Where つなぐ asked on a diagram is shown (`AppModel.connecting` with its `anchor`): beside
/// the point it was asked from, in the canvas's own points.
struct ConnectAnchor: ViewModifier {
    var model: AppModel

    func body(content: Content) -> some View {
        content.overlay(alignment: .topLeading) {
            if let connecting = model.connecting, let anchor = connecting.anchor {
                Color.clear
                    .frame(width: 1, height: 1)
                    .popover(isPresented: Binding(
                        get: { model.connecting?.id == connecting.id },
                        set: { shown in if !shown, model.connecting?.id == connecting.id { model.connecting = nil } }),
                             arrowEdge: .trailing) {
                        ConnectView(model: model, source: connecting.source, target: connecting.target) {
                            model.connecting = nil
                        }
                        .id(connecting.id)
                    }
                    .position(x: anchor.x, y: anchor.y)
            }
        }
    }
}

/// The lines leaving what is picked — a card's head, a field, a row, a node; a card picked,
/// its fields' and rows' under its head's — in the property panel's 出ていく線, and those reaching
/// it from elsewhere in 入ってくる線: each by its kind and where it reaches, picked with a press (the
/// line's own properties then), taken off with its button; and 「＋ つなぐ…」 for one more.
/// Only read where nothing may be written (a locked main).
struct LineList: View {
    var model: AppModel
    var source: LineSource
    var editable: Bool

    /// 「＋ つなぐ…」 pressed: つなぐ from `source`, shown beside the button (no anchor on a
    /// diagram), the other end to pick first.
    static func begin(_ model: AppModel, _ source: LineSource) {
        guard !model.isReadOnly else { return }
        model.connecting = Connecting(source: source)
    }

    var body: some View {
        let refs = model.lines(leaving: source)
        // A card picked: what leaves its fields and rows too, by where each leaves, so the
        // count is the card's every line, as its card editor's head counts them.
        let inside = source.from.isEmpty ? model.linesInside(source.card) : []
        // And what reaches it from elsewhere: the card's, or the field's.
        let arriving = model.linesArriving(at: source.card, field: source.from.isEmpty ? nil : source.from)
        outgoing(refs, inside)
        incoming(arriving)
        // main's (or a branch's) card: the board's cards still to do that reach it. Not on the
        // board, whose cards reach main and not the other way.
        if !model.isBoardOpen, !WorkDirIds.isFile(source.card) {
            work(model.boardReach(of: source))
        }
    }

    /// 関わるやること: the board's cards not done yet that reach here by a line or a path in their
    /// text — each by its title, its kind, how far it has come and where it is on the board, shown
    /// there with a press. What is done is not listed (a long-running project's
    /// would crowd it).
    private func work(_ reach: [BoardReach]) -> some View {
        PropertyCategory(title: L10n[.Lines.workTitle], count: reach.count) {
            if reach.isEmpty {
                Text(L10n[.Lines.workEmpty]).font(.app(11)).foregroundStyle(.tertiary)
                    .padding(.horizontal, 18).padding(.vertical, 4)
            }
            ForEach(reach) { one in
                Button { model.showOnBoard(one.path) } label: {
                    VStack(alignment: .leading, spacing: 1) {
                        HStack(spacing: 6) {
                            Text(one.label.isEmpty ? PathText.lastName(one.path) : one.label)
                                .font(.app(11.5)).lineLimit(1)
                            Text(one.kind).font(.app(9.5, design: .monospaced)).foregroundStyle(.tertiary)
                            if let state = one.state, !state.isEmpty {
                                Text(state).font(.app(10)).foregroundStyle(.secondary)
                            }
                        }
                        Text(one.at.map { "\(one.path) → \($0)" } ?? one.path)
                            .font(.app(10.5, design: .monospaced)).foregroundStyle(.secondary)
                            .lineLimit(1).truncationMode(.head)
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .help(L10n[.Lines.workHelpShow])
                .padding(.horizontal, 18).padding(.vertical, 3)
            }
        }
    }

    /// The lines leaving here, and つなぐ for one more.
    private func outgoing(_ refs: [LineRef], _ inside: [(from: String, refs: [LineRef])]) -> some View {
        PropertyCategory(title: L10n[.Lines.outgoingTitle], count: refs.count + inside.reduce(0) { $0 + $1.refs.count }) {
            if refs.isEmpty && inside.isEmpty {
                Text(L10n[.Lines.outgoingEmpty]).font(.app(11)).foregroundStyle(.tertiary)
                    .padding(.horizontal, 18).padding(.vertical, 4)
            }
            ForEach(refs) { ref in lineRow(ref) }
            ForEach(inside, id: \.from) { group in
                Text(Self.leaving(group.from))
                    .font(.app(10.5, design: .monospaced)).foregroundStyle(.secondary)
                    .lineLimit(1).truncationMode(.head)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 18).padding(.top, 5)
                ForEach(group.refs) { ref in lineRow(ref) }
            }
            if editable {
                Button { Self.begin(model, source) } label: {
                    Label(L10n[.Lines.actionConnectMenu], systemImage: "plus")
                }
                .buttonStyle(.borderless)
                .font(.app(11.5))
                .padding(.horizontal, 18).padding(.vertical, 5)
                .help(L10n[.Lines.outgoingHelpConnect])
                .popover(isPresented: Binding(
                    get: { model.connecting?.anchor == nil && model.connecting?.source == source },
                    set: { shown in if !shown, model.connecting?.anchor == nil { model.connecting = nil } }),
                         arrowEdge: .leading) {
                    if let connecting = model.connecting {
                        ConnectView(model: model, source: connecting.source, target: connecting.target) {
                            model.connecting = nil
                        }
                        .id(connecting.id)
                    }
                }
            }
        }
    }

    /// The lines reaching here from elsewhere: where each leaves and its kind, picked with a press
    /// (the line's own properties then), its card shown with the arrow. Taken off where it leaves
    /// — the card it leaves holds it — not here.
    private func incoming(_ arriving: [Relation]) -> some View {
        let kinds = model.detail?.lineKinds ?? LineKinds()
        return PropertyCategory(title: L10n[.Lines.incomingTitle], count: arriving.count) {
            if arriving.isEmpty {
                Text(L10n[.Lines.incomingEmpty]).font(.app(11)).foregroundStyle(.tertiary)
                    .padding(.horizontal, 18).padding(.vertical, 4)
            }
            ForEach(arriving, id: \.self) { relation in
                HStack(spacing: 6) {
                    LineSample(look: kinds.look(relation.line))
                    Button {
                        if let id = relation.connection { model.selectLine(LineRef(card: relation.fromSchemaId, id: id)) }
                    } label: {
                        VStack(alignment: .leading, spacing: 1) {
                            Text(kinds.label(relation.line)).font(.app(11.5))
                            Text(model.leaving(relation))
                                .font(.app(10.5, design: .monospaced)).foregroundStyle(.secondary)
                                .lineLimit(1).truncationMode(.head)
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .help(L10n[.Lines.incomingHelpSelect])
                    Button { Self.showSource(model, relation) } label: { Image(systemName: "arrow.up.forward.square") }
                        .buttonStyle(.borderless)
                        .help(L10n[.Lines.incomingHelpShowSource])
                }
                .padding(.horizontal, 18).padding(.vertical, 3)
            }
        }
    }

    /// The card a line arriving here leaves, shown on its section's diagram — a flow's node on
    /// its flow's row, a card inside a goal on the goal's — as the nav panel shows it.
    static func showSource(_ model: AppModel, _ relation: Relation) {
        guard let detail = model.detail else { return }
        let node: NavPanelTree.Node = detail.inner.contains { $0.id == relation.fromSchemaId }
            ? .node(relation.fromSchemaId) : .card(relation.fromSchemaId)
        model.perform(NavPanelTree.press(node, twice: false, in: detail))
    }

    /// Where lines leave inside a card, as a person reads it: a field or a row by its path, a
    /// process node's row by its number.
    static func leaving(_ from: String) -> String {
        if let index = GraphEdges.stepIndex(from) { return L10n[.Lines.leavingRow(index + 1)] }
        return L10n[.Lines.leavingFrom(from)]
    }

    /// One line: its kind and where it reaches, picked with a press, taken off with its button.
    @ViewBuilder
    private func lineRow(_ ref: LineRef) -> some View {
        let kinds = model.detail?.lineKinds ?? LineKinds()
        if let (_, line) = model.line(ref) {
            HStack(spacing: 6) {
                LineSample(look: kinds.look(line.line))
                Button { model.selectLine(ref) } label: {
                    VStack(alignment: .leading, spacing: 1) {
                        Text(kinds.label(line.line)).font(.app(11.5))
                        Text(model.landing(of: line) ?? L10n[.Lines.lineTargetGone])
                            .font(.app(10.5, design: .monospaced)).foregroundStyle(.secondary)
                            .lineLimit(1).truncationMode(.head)
                        if let code = model.codeReview(of: ref) { CodeStateNote(code: code) }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .help(L10n[.Lines.lineHelpSelect])
                if model.codeReview(of: ref)?.isChanged == true { CodeStateDot() }
                if editable {
                    Button { LineRemoval.remove(model, ref) } label: { Image(systemName: "minus.circle") }
                        .buttonStyle(.borderless)
                        .help(L10n[.Lines.lineHelpRemove])
                }
            }
            .padding(.horizontal, 18).padding(.vertical, 3)
        }
    }
}

/// What an implementation line says under its target when its code is to be looked at: in orange where the code
/// changed since it was recorded (its dot, `CodeStateDot`, is at the row's end), in grey where it was never recorded.
struct CodeStateNote: View {
    var code: CodeReviewLine

    var body: some View {
        switch code.state {
        case .changed:
            Text(code.linkedAt.map { L10n[.Lines.lineCodeChanged($0)] } ?? L10n[.Lines.lineCodeChangedNoTime])
                .font(.app(10.5)).foregroundStyle(Color(nsColor: CardRenderer.reviewOrange))
                .lineLimit(1).truncationMode(.tail)
        case .unrecorded:
            Text(L10n[.Lines.lineCodeUnrecorded]).font(.app(10.5)).foregroundStyle(.secondary)
        }
    }
}

/// The map's review dot, at the end of an implementation line whose code changed.
struct CodeStateDot: View {
    var body: some View {
        Circle().fill(Color(nsColor: CardRenderer.reviewOrange)).frame(width: 8, height: 8)
    }
}

/// A line taken off, as the window takes it: an embedding is asked first, since its field's
/// type goes back to `string` (the core does that, `specboard://SpecBoard/Docs/Design:CardKinds`「カードの線」).
enum LineRemoval {
    /// What is said before a line is taken off, when it is an embedding.
    static func warning(_ model: AppModel, _ ref: LineRef) -> String? {
        guard let (holder, line) = model.line(ref), model.detail?.lineKinds[line.line]?.base == "embed" else { return nil }
        let field = line.from.isEmpty ? holder.path : "\(holder.path).\(line.from)"
        let to = line.to.flatMap(model.linePath(to:)) ?? L10n[.Lines.removalPartner]
        return L10n[.Lines.removalEmbedWarning(field, to)]
    }

    @MainActor
    static func remove(_ model: AppModel, _ ref: LineRef) {
        if let warning = warning(model, ref) {
            let alert = NSAlert()
            alert.messageText = L10n[.Lines.removalEmbedTitle]
            alert.informativeText = warning
            alert.addButton(withTitle: L10n[.Common.actionRemove])
            alert.addButton(withTitle: L10n[.Common.actionCancel])
            guard alert.runModal() == .alertFirstButtonReturn else { return }
        }
        model.removeLine(ref)
    }
}

/// A line picked, in the property panel: its kind, where it leaves and what it reaches (each
/// a press away), where it lands (`at`, a press away — outside what it reaches, in full),
/// its own fields as its kind defines them, and taking it off. Its description is the
/// panel's foot (`PropertyPanel.describable`).
struct LineProperties: View {
    var model: AppModel
    var ref: LineRef
    var editable: Bool

    var body: some View {
        if let (holder, line) = model.line(ref) {
            let kind = model.detail?.lineKinds[line.line]
            PropertyCategory(title: L10n[.Common.wordLine]) {
                PropertyRow("line", help: kind?.description ?? "") {
                    HStack(spacing: 6) {
                        LineSample(look: kind?.look ?? LineLook())
                        Text(kind?.label ?? line.line)
                        Text(line.line).font(.app(10.5, design: .monospaced)).foregroundStyle(.tertiary)
                    }
                }
                PropertyRow("from", help: L10n[.Lines.propsHelpFrom]) {
                    Button(LineSource(card: holder.id, path: holder.path, from: line.from).said) { model.showSource(of: ref) }
                        .buttonStyle(.link).lineLimit(1).truncationMode(.middle)
                }
                PropertyRow("to", help: L10n[.Lines.propsHelpTo]) {
                    if let to = line.to.flatMap(model.linePath(to:)) {
                        Button(to) { model.show(path: to) }.buttonStyle(.link).lineLimit(1).truncationMode(.middle)
                    } else {
                        Text(L10n[.Lines.propsTargetGone]).foregroundStyle(.orange)
                    }
                }
                if let at = line.at {
                    PropertyRow("at", help: L10n[.Lines.propsHelpAt]) {
                        let path = model.landing(of: line) ?? at
                        Button(at) { model.show(path: path) }.buttonStyle(.link).lineLimit(1).truncationMode(.middle)
                    }
                }
            }
            if let kind, !kind.fields.isEmpty {
                PropertyCategory(title: L10n[.Common.wordField]) {
                    ForEach(kind.fields) { field in
                        PropertyRow(field.name, help: [field.label, field.description].filter { !$0.isEmpty }.joined(separator: L10n[.Lines.propsSentenceSeparator])) {
                            LineFieldControl(model: model, ref: ref, field: field, value: line.fields[field.name],
                                             editable: editable)
                        }
                    }
                }
            }
            if editable {
                Button(role: .destructive) { LineRemoval.remove(model, ref) } label: {
                    Label(L10n[.Lines.propsRemoveLine], systemImage: "minus.circle")
                }
                .padding(.horizontal, 18).padding(.vertical, 10)
            }
        } else {
            Text(L10n[.Lines.propsGone]).font(.app(11)).foregroundStyle(.tertiary).padding(12)
        }
    }
}

/// One of a line's fields in the property panel, as a card's field is shown: a choice as a
/// pull-down, a check as a check, words in one line, a list a line each in a larger box, and a
/// group (a pointer's `when`) a press away in a small window of its own fields.
struct LineFieldControl: View {
    var model: AppModel
    var ref: LineRef
    var field: LineField
    var value: JSONValue?
    var editable: Bool

    @State private var open = false

    var body: some View {
        switch field.fieldtype {
        case "dropdownlist", "radio":
            if editable {
                // As wide as the value chosen, within the panel's column (`FittingChoice`).
                FittingChoice(options: .words(field.options, none: !field.required || value == nil),
                              selection: Binding(
                                get: { value?.stringValue ?? "" },
                                set: { model.setLineField(ref, field.name, $0.isEmpty ? nil : .string($0)) }))
            } else {
                Text(value?.display ?? "—").foregroundStyle(value == nil ? .tertiary : .primary)
            }
        case "checkbox":
            if editable {
                Toggle("", isOn: Binding(get: { value == .bool(true) },
                                         set: { model.setLineField(ref, field.name, $0 ? .bool(true) : nil) }))
                    .toggleStyle(.checkbox).labelsHidden()
            } else {
                Text(value == .bool(true) ? L10n[.Lines.valueYes] : L10n[.Lines.valueNo])
            }
        case "group":
            group
        default:
            if field.many > 0 {
                let items: [String] = {
                    if case .array(let list)? = value { return list.map { $0.stringValue ?? $0.display } }
                    return value.map { [$0.stringValue ?? $0.display] } ?? []
                }()
                PropertyLongText(text: items.joined(separator: "\n"), placeholder: L10n[.Lines.fieldOneOnEachLine], editable: editable) { text in
                    let lines = text.components(separatedBy: "\n").map { $0.trimmingCharacters(in: .whitespaces) }
                        .filter { !$0.isEmpty }
                    model.setLineField(ref, field.name, lines.isEmpty ? nil : .array(lines.map(JSONValue.string)))
                }
            } else if editable {
                CommitField(field.said, text: value.map { $0.stringValue ?? $0.display } ?? "", monospaced: true) { text in
                    let said = text.trimmingCharacters(in: .whitespaces)
                    let written: JSONValue? = said.isEmpty ? nil
                        : field.fieldtype == "number" ? Double(said).map(JSONValue.number) ?? .string(said) : .string(said)
                    model.setLineField(ref, field.name, written)
                }
                .id("line-field-\(ref.card)-\(ref.id)-\(field.name)-\(value?.display ?? "")")
            } else {
                Text(value.map { $0.stringValue ?? $0.display } ?? "—").foregroundStyle(value == nil ? .tertiary : .primary)
                    .textSelection(.enabled)
            }
        }
    }

    /// A group's fields said in a line, and a small window to write them.
    private var group: some View {
        let object = value?.objectValue ?? [:]
        let said = field.fields.compactMap { inner in object[inner.name].map { "\(inner.name) = \($0.stringValue ?? $0.display)" } }
            .joined(separator: L10n[.Lines.groupSeparator])
        return HStack(spacing: 4) {
            Text(said.isEmpty ? L10n[.Common.wordNone] : said)
                .foregroundStyle(said.isEmpty ? .tertiary : .primary)
                .lineLimit(1).truncationMode(.tail)
                .frame(maxWidth: .infinity, alignment: .leading)
            if editable {
                Button("…") { open = true }.buttonStyle(.borderless).help(L10n[.Lines.groupHelpWrite(field.said)])
            }
        }
        .popover(isPresented: $open, arrowEdge: .leading) {
            LineGroupEditor(field: field, value: object) { written in
                model.setLineField(ref, field.name, written.map(JSONValue.object))
                open = false
            } cancel: {
                open = false
            }
        }
    }
}

/// A group of a line's fields written together (a pointer's `when`: the field the other
/// end's kind is told by, and the value that picks it): kept, taken off, or let be.
struct LineGroupEditor: View {
    var field: LineField
    var keep: ([String: JSONValue]?) -> Void
    var cancel: () -> Void

    @State private var draft: [String: String]

    init(field: LineField, value: [String: JSONValue], keep: @escaping ([String: JSONValue]?) -> Void,
         cancel: @escaping () -> Void) {
        self.field = field
        self.keep = keep
        self.cancel = cancel
        _draft = State(initialValue: value.mapValues { $0.stringValue ?? $0.display })
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(field.said).font(.app(12, weight: .semibold))
            if !field.description.isEmpty {
                Text(field.description).font(.app(10.5)).foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            ForEach(field.fields) { inner in
                VStack(alignment: .leading, spacing: 2) {
                    Text(L10n[.Lines.groupFieldLabel(inner.name, inner.said)]).font(.app(10.5)).foregroundStyle(.secondary)
                    let binding = Binding(get: { draft[inner.name] ?? "" }, set: { draft[inner.name] = $0 })
                    if inner.options.isEmpty {
                        TextField(inner.name, text: binding).textFieldStyle(.roundedBorder).font(.app(11.5, design: .monospaced))
                    } else {
                        FittingChoice(options: .words(inner.options, none: true), selection: binding)
                    }
                }
            }
            HStack {
                Button(L10n[.Common.actionRemove], role: .destructive) { keep(nil) }
                Spacer()
                Button(L10n[.Common.actionCancel], action: cancel)
                Button(L10n[.Lines.groupDecide]) {
                    let kept = draft.compactMapValues { text -> JSONValue? in
                        let said = text.trimmingCharacters(in: .whitespaces)
                        return said.isEmpty ? nil : .string(said)
                    }
                    keep(kept.isEmpty ? nil : kept)
                }
                .keyboardShortcut(.defaultAction)
            }
        }
        .padding(12)
        .frame(width: 300)
    }
}

/// The kinds of line a diagram draws, each a press away from being hidden or shown (under the
/// diagram, as a card map says its lines, rather than in the filter window
/// that is seldom opened): its look and its label, faint while hidden — a kind hidden from the
/// start by its definition (`visible: false`) too, until it is shown. As many rows as they
/// take. A diagram with no lines says so, rather than offering switches that change nothing.
struct LineSwitches: View {
    /// The kinds drawn there (`CardCanvasController.drawnKinds`).
    var drawn: [String]
    var kinds: LineKinds
    var filters: GraphFilters
    var toggle: (String) -> Void

    var body: some View {
        if drawn.isEmpty {
            Text(L10n[.Lines.switchesEmpty]).font(.app(10.5)).foregroundStyle(.tertiary)
                .frame(maxWidth: .infinity, alignment: .leading)
        } else {
            FlowRow(spacing: 12, lineSpacing: 4) {
                ForEach(drawn, id: \.self) { kind in
                    let on = filters.shows(kind, kinds: kinds)
                    Button { toggle(kind) } label: {
                        HStack(spacing: 5) {
                            LineSample(look: kinds.look(kind))
                            Text(kinds.label(kind)).font(.app(10.5)).lineLimit(1)
                        }
                        .fixedSize()
                        .contentShape(Rectangle())
                        .opacity(on ? 1 : 0.32)
                    }
                    .buttonStyle(.plain)
                    .help(on ? L10n[.Lines.switchesHelpHide(kinds.label(kind), kind, kinds.meaning(kind))]
                          : L10n[.Lines.switchesHelpShow(kinds.label(kind), kind, kinds.meaning(kind))])
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}
