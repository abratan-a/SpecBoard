import SwiftUI

/// What a merge into main would leave, on screen (`AppModel.ResultView`): said
/// above everything — what it is, that it is only read, what the merge takes away (a section's
/// cards also on its diagram, faded and struck through) — with what may be done: a merge request approved or rejected, the merge window's
/// merge done, or closed, back to where the window was.
struct ResultBanner: View {
    var model: AppModel
    var shown: AppModel.ResultView
    /// Why a merge request is rejected, being asked for (`NameSheet`).
    @State private var naming: NameAsk?

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 8) {
                Image(systemName: "arrow.triangle.merge")
                Text(L10n[.Branches.resultViewing(model.resultTitle(shown))])
                    .font(.app(11.5)).fixedSize(horizontal: false, vertical: true)
                Spacer()
                actions
            }
            if shown.stale {
                Text(L10n[.Branches.resultStale])
                    .font(.app(11)).foregroundStyle(.orange)
            }
            if !shown.removed.isEmpty {
                Text(L10n[.Branches.resultRemoved(shown.removed.joined(separator: L10n[.Common.separatorList]))])
                    .font(.app(11)).foregroundStyle(.secondary).textSelection(.enabled)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(.horizontal, 14).padding(.vertical, 7)
        .background(Color.accentColor.opacity(0.12))
        .nameSheet($naming)
    }

    @ViewBuilder
    private var actions: some View {
        HStack(spacing: 8) {
            switch shown.of {
            case .request(let number):
                Button(L10n[.Branches.resultReject]) {
                    naming = NameAsk(title: L10n[.Branches.resultRejectTitle], message: L10n[.Branches.resultRejectMessage],
                                     placeholder: L10n[.Branches.resultRejectPlaceholder],
                                     confirm: L10n[.Branches.resultRejectConfirm], cancel: L10n[.Common.actionCancel]) { typed in
                        model.closeResult()
                        model.rejectMergeRequest(number, reason: typed)
                    }
                }
                .disabled(model.isArchived)
                Button(L10n[.Branches.resultApprove]) { model.approveShownRequest(places: nil) }
                    .disabled(model.isArchived || shown.stale)
            case .merge:
                Button(L10n[.Branches.resultMerge]) { model.mergeShownResult() }
                    .disabled(model.isArchived)
            }
            Button(L10n[.Common.actionClose]) { model.closeResult() }
                .help(L10n[.Branches.resultCloseHelp])
        }
        .controlSize(.small)
        .fixedSize()
    }
}
