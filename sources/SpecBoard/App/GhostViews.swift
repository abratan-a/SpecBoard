import SwiftUI

/// A card ghost pointed at something else, from a sheet (the nav panel's right-click,
/// `specboard://SpecBoard/Docs/Design:DesignSections` §2): the selector, offering what it may point at — a card or a
/// card ghost of the kind it leads to now, not in its own section (`PathWanted.retarget`).
struct GhostRetargetSheet: View {
    var model: AppModel
    var ghost: CardGhost
    var close: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(L10n[.CardEditor.retargetTitle(ghost.name)]).font(.app(13, weight: .semibold))
            Text(L10n[.CardEditor.retargetMessage])
                .font(.app(11)).foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            if let detail = model.detail {
                PathSelector(detail: detail, owner: nil, wanted: .retarget(ghost, in: detail.directory)) { path in
                    model.retargetGhost(ghost.id, to: path)
                    close()
                }
            }
            HStack {
                Spacer()
                Button(L10n[.Common.actionCancelDialog], action: close).keyboardShortcut(.cancelAction)
            }
        }
        .padding(14)
        .frame(width: 420)
    }
}

/// A card ghost picked (`specboard://SpecBoard/Docs/Design:DesignSections` §2), in the property panel: its name, what
/// it points at — pointed elsewhere from the selector — and the card it leads to, opened
/// from here. Read only where the project is (a locked main too).
struct GhostProperties: View {
    var model: AppModel
    var ghost: CardGhost
    var editable: Bool

    var body: some View {
        if let detail = model.detail {
            let directory = detail.directory
            let card = directory.reached(ghost)
            PropertyCategory(title: L10n[.CardEditor.categoryCardGhost]) {
                PropertyRow("name", help: L10n[.CardEditor.helpGhostName]) {
                    if editable {
                        CommitField(L10n[.Common.wordName], text: ghost.name, monospaced: true) { model.renameGhost(ghost.id, to: $0) }
                            .id("ghost-name-\(ghost.id)")
                    } else {
                        Text(ghost.name).font(.app(11.5, design: .monospaced)).textSelection(.enabled)
                    }
                }
                PropertyRow("path", help: L10n[.CardEditor.helpGhostPath]) {
                    Text(directory.full(ghost)).font(.app(11, design: .monospaced)).textSelection(.enabled)
                }
                PropertyRow("target", help: Glossary.term("ghost")?.meaning ?? "") {
                    HStack(spacing: 4) {
                        Text(directory.targetPath(of: ghost) ?? L10n[.CardEditor.ghostNoTarget])
                            .font(.app(11, design: .monospaced)).lineLimit(1).truncationMode(.middle)
                            .foregroundStyle(ghost.target == nil ? .secondary : .primary)
                            .frame(maxWidth: .infinity, alignment: .leading)
                        if editable {
                            PathPickButton(title: "…", detail: detail, owner: nil,
                                           wanted: .retarget(ghost, in: directory)) { model.retargetGhost(ghost.id, to: $0) }
                                .buttonStyle(.borderless)
                                .help(L10n[.CardEditor.helpRetargetButton])
                        }
                    }
                }
                PropertyRow("resolvesTo", help: L10n[.CardEditor.helpResolvesTo]) {
                    if let card {
                        HStack(spacing: 4) {
                            Text(directory.full(card)).font(.app(11, design: .monospaced)).lineLimit(1)
                                .truncationMode(.middle)
                                .frame(maxWidth: .infinity, alignment: .leading)
                            Button(L10n[.CardEditor.buttonOpenReal]) { model.open(.card(card.id)) }.buttonStyle(.borderless)
                        }
                    } else {
                        Text(directory.ghostProblem(ghost) ?? L10n[.CardEditor.ghostUnreachable])
                            .font(.app(11)).foregroundStyle(.red)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
                PropertyRow("kind", help: L10n[.CardEditor.helpGhostKind]) {
                    Text(card.map { L10n[.CardEditor.ghostKindOf($0.kindName)] } ?? "ghost").foregroundStyle(.secondary)
                }
            }
        }
    }
}
