import SwiftUI

/// The one place a path is picked from (`specboard://SpecBoard/Docs/Design:DesignSections` §8): found by
/// name, or walked to — tree → section → card → what is inside it. What is picked
/// comes in full.
struct PathSelector: View {
    var detail: ProjectDetail?
    /// The card the path is written in: only what it sees is offered.
    var owner: SchemaNode?
    var wanted: PathWanted
    /// The tree beside the one open (`AppModel.outsideDetail`): offered after it, each called so.
    var outside: ProjectDetail? = nil
    var words: (own: String, outside: String) = (Board.title, SchemaDirectory.mainWord)
    /// The files' functions and types read so far, offered inside their files, and how a file's are asked
    /// for as its line opens (`AppModel.outlines`, `askOutlines`); none where only cards are picked.
    var outlines: [String: FileOutline]? = nil
    var askOutline: ((String) -> Void)? = nil
    var pick: (String) -> Void

    @State private var query = ""
    /// Trees and sections start open, cards closed: what was turned the other way.
    @State private var turned: Set<String> = []

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 4) {
                Image(systemName: "magnifyingglass").font(.app(10)).foregroundStyle(.secondary)
                TextField(L10n[.Nav.selectorSearch], text: $query).textFieldStyle(.roundedBorder)
            }
            if let detail {
                if query.trimmingCharacters(in: .whitespaces).isEmpty {
                    let tree = PathChoices.tree(detail, from: owner, wanted: wanted, outside: outside, words: words,
                                                outlines: outlines)
                    // Opaque, as the window around it.
                    List {
                        ForEach(tree) { branch($0) }
                    }
                    .listStyle(.inset)
                    .overlay { if tree.isEmpty { empty } }
                } else {
                    let found = PathChoices.found(detail, from: owner, wanted: wanted, query: query, outside: outside,
                                                  words: words, outlines: outlines)
                    List(found) { row($0, flat: true) }
                        .listStyle(.inset)
                        .overlay { if found.isEmpty { empty } }
                }
            } else {
                empty
            }
        }
        .frame(width: 380, height: 340)
        .padding(10)
    }

    /// A line and, open, what is under it.
    private func branch(_ choice: PathChoice) -> AnyView {
        guard let children = choice.children else { return AnyView(row(choice, flat: false)) }
        let startsOpen: Bool
        switch choice.kind {
        case .tree, .section: startsOpen = true
        default: startsOpen = false
        }
        let open = Binding(
            get: { startsOpen != turned.contains(choice.id) },
            set: { value in
                if value == startsOpen { turned.remove(choice.id) } else { turned.insert(choice.id) }
            })
        return AnyView(DisclosureGroup(isExpanded: open) {
            ForEach(children) { branch($0) }
        } label: {
            row(choice, flat: false)
        })
    }

    private var empty: some View {
        Text(L10n[.Nav.selectorNoMatches]).font(.app(11)).foregroundStyle(.tertiary)
    }

    /// A line: what it is, its name — its whole path when found by name — and,
    /// when it can be picked, picked by a click.
    private func row(_ choice: PathChoice, flat: Bool) -> some View {
        HStack(spacing: 6) {
            Text(Self.mark(choice.kind))
                .font(.app(9.5, design: .monospaced)).foregroundStyle(.secondary)
                .frame(width: 44, alignment: .leading)
            Text(flat ? (choice.path ?? choice.title) : choice.title)
                .font(.app(11.5, design: flat ? .monospaced : .default))
                .foregroundStyle(choice.path == nil ? .secondary : .primary)
                .lineLimit(1).truncationMode(.head)
            Spacer(minLength: 0)
        }
        .contentShape(Rectangle())
        .onTapGesture { if let path = choice.path { pick(path) } }
        .help(choice.path ?? "")
        .onAppear { if let file = choice.asks { askOutline?(file) } }
    }

    static func mark(_ kind: PathChoice.Kind) -> String {
        switch kind {
        case .tree: "tree"
        case .section: L10n[.Common.wordSection]
        case .card(let kind): kind.rawValue
        // A card ghost: the shortcut's mark, and the kind of the card it leads to.
        case .ghost(let kind): "\(SchemaDirectory.ghostMark)\(kind.rawValue)"
        case .field: "field"
        case .formatRow: L10n[.Common.wordRow]
        case .node: "node"
        case .symbol(let of): SymbolBadge.of(of)
        }
    }
}

/// A button that opens the selector beside it (`PathSelector`).
struct PathPickButton: View {
    var title: String
    var systemImage: String? = nil
    /// The mark it shows instead of its words in a narrow card editor's bar (`CardBar`).
    var compactImage: String? = nil
    var detail: ProjectDetail?
    var owner: SchemaNode?
    var wanted: PathWanted
    /// main's detail, from the board (`PathSelector.outside`).
    var outside: ProjectDetail? = nil
    var pick: (String) -> Void

    @State private var picking = false
    @Environment(\.cardBarCompact) private var compact

    var body: some View {
        Button { picking = true } label: {
            if let image = systemImage ?? (compact ? compactImage : nil) { Image(systemName: image) } else { Text(title) }
        }
        .help(title)
        .popover(isPresented: $picking, arrowEdge: .leading) {
            PathSelector(detail: detail, owner: owner, wanted: wanted, outside: outside) { path in
                picking = false
                pick(path)
            }
        }
    }
}
