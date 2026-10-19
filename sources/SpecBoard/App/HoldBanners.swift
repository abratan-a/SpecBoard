import SwiftUI

/// Who is putting the open branch right in one go (`specboard://SpecBoard/Docs/Design:DesignBranches` §5), said
/// above everything: the window's "まとめて直す" with its way to close or give up, or an
/// MCP batch with the way to cancel it — a person's hand comes first.
struct HoldBanners: View {
    var model: AppModel

    /// The tree as the window calls it: the board by its title.
    private var branch: String {
        model.isBoardOpen ? Board.title : model.openTree.name
    }

    var body: some View {
        VStack(spacing: 0) {
            if let group = model.group {
                HStack(spacing: 8) {
                    Image(systemName: "square.stack.3d.up")
                    Text(model.isLockedGroupOpen
                         ? L10n[.Branches.holdGroupLocked(branch, group.sinceLabel)]
                         : L10n[.Branches.holdGroupOpen(branch, group.sinceLabel)])
                        .font(.app(11.5)).fixedSize(horizontal: false, vertical: true)
                    Spacer()
                    Button(L10n[.Common.actionCancel]) { model.abandonGroup() }.font(.app(11))
                        .help(model.isLockedGroupOpen
                              ? L10n[.Branches.holdAbandonLockedHelp]
                              : L10n[.Branches.holdAbandonHelp])
                    Button(L10n[.Common.actionClose]) { model.endGroup() }.font(.app(11))
                }
                .padding(.horizontal, 14).padding(.vertical, 7)
                .background(Color.accentColor.opacity(0.12))
            }
            if let other = model.heldElsewhere.first {
                HStack(spacing: 8) {
                    Image(systemName: "desktopcomputer")
                    Text(L10n[.Branches.holdElsewhere(other.client, branch, other.sinceLabel)])
                        .font(.app(11.5)).fixedSize(horizontal: false, vertical: true)
                    Spacer()
                }
                .padding(.horizontal, 14).padding(.vertical, 7)
                .background(Color.orange.opacity(0.14))
            }
            if let batch = model.heldByMCP.first {
                HStack(spacing: 8) {
                    Image(systemName: "hourglass")
                    Text(L10n[.Branches.holdMcpBatch(branch, batch.sinceLabel)])
                        .font(.app(11.5)).fixedSize(horizontal: false, vertical: true)
                    Spacer()
                    Button(L10n[.Branches.holdCancelBatch]) { model.cancelBatches() }.font(.app(11))
                        .help(L10n[.Branches.holdCancelBatchHelp])
                }
                .padding(.horizontal, 14).padding(.vertical, 7)
                .background(Color.orange.opacity(0.14))
            }
        }
    }
}
