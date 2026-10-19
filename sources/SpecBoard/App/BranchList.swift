import SwiftUI

/// The top of the nav panel: main, and the project's branches under it, one level
/// (`specboard://SpecBoard/Docs/Design:DesignBranches` §2). The one picked is what the tree below and the
/// diagram show — the word for that is not said: it is just which is open.
struct BranchList: View {
    var model: AppModel
    var onCreate: () -> Void
    var onRename: (BranchInfo) -> Void
    var onDelete: (BranchInfo) -> Void

    @State private var unlocking = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 6) {
                Text("branch").font(.app(10.5)).foregroundStyle(.secondary)
                Spacer()
                Button(action: onCreate) {
                    Image(systemName: "plus").font(.app(10.5))
                }
                .buttonStyle(.borderless)
                .disabled(model.isArchived)
                .help(L10n[.Branches.listCreateHelp])
            }
            .padding(.bottom, 2)
            .padding(.horizontal, 10)
            row(nil, name: "main")
                .contextMenu {
                    if model.isMainLocked {
                        Button(L10n[.Branches.listUnlockMain]) { unlocking = true }
                            .disabled(model.isArchived)
                    } else {
                        Button(L10n[.Branches.listLockMain]) { model.setMainLock(true) }
                            .disabled(model.isArchived)
                            .help(MainLock.lockHelp)
                    }
                }
                .mainUnlockConfirmation(model, isPresented: $unlocking)
            // The merge requests out, under main: each opened as what main would
            // be once it is merged, to look at before approving.
            ForEach(model.mergeRequests.filter { $0.state == .open || $0.state == .stale }) { request in
                requestRow(request)
            }
            ForEach(model.branches) { branch in
                row(branch, name: branch.name)
                    .contextMenu {
                        Button(L10n[.Branches.listMerge]) { model.askToMerge(from: branch.id, into: nil) }
                            .disabled(model.isArchived)
                        Button(L10n[.Branches.listMergeMainIn]) { model.askToMerge(from: nil, into: branch.id) }
                            .disabled(model.isArchived)
                        Divider()
                        Button(L10n[.Common.actionRenameMenu]) { onRename(branch) }
                        Divider()
                        Button(L10n[.Branches.listDelete], role: .destructive) { onDelete(branch) }
                    }
            }
        }
        .padding(.vertical, 6)
    }

    /// The gap around a row, part of its slot (`NavRowSlot`): the point above it to the row before,
    /// and the panel's sides.
    static let rowGap = EdgeInsets(top: 1, leading: 10, bottom: 0, trailing: 10)

    /// A merge request out: its number and title, opened as what main would be once merged.
    private func requestRow(_ request: MergeRequest) -> some View {
        let open: Bool = if case .request(let number)? = model.resultView?.of { number == request.number } else { false }
        return HStack(spacing: 5) {
            Image(systemName: "arrow.triangle.merge")
                .font(.app(10))
                .foregroundStyle(open ? Color.accentColor : .secondary)
                .frame(width: 14)
            Text("#\(request.number)").font(.app(10.5, weight: .semibold, design: .monospaced)).foregroundStyle(.secondary)
            Text(request.title).font(.app(11.5)).lineLimit(1)
            if request.state == .stale {
                Text(L10n[.Branches.listRequestStale]).font(.app(9.5)).foregroundStyle(.orange)
            }
            Spacer(minLength: 4)
        }
        .padding(.leading, 14)
        .padding(.vertical, 2)
        .padding(.horizontal, 3)
        .modifier(NavRowSlot(current: open, gap: Self.rowGap))
        .onTapGesture { if !open { model.openRequestResult(request.number) } }
        .help(L10n[.Branches.listRequestHelp(request.number, request.branchName)])
    }

    private func row(_ branch: BranchInfo?, name: String) -> some View {
        // main, put right in one go under its lock, is open in a branch of the app's own.
        let open = model.openTree.id == branch?.id || (branch == nil && model.isLockedGroupOpen)
        return HStack(spacing: 5) {
            Image(systemName: branch == nil ? "circle.fill" : "arrow.triangle.branch")
                .font(.app(branch == nil ? 6 : 10))
                .foregroundStyle(open ? Color.accentColor : .secondary)
                .frame(width: 14)
            Text(name)
                .font(.app(12, weight: open ? .semibold : .regular, design: .monospaced))
                .lineLimit(1)
            if branch == nil, model.isMainLocked {
                Image(systemName: "lock.fill").font(.app(9.5)).foregroundStyle(.secondary)
                    .help(L10n[.Branches.listMainLockedHelp])
            }
            if branch == nil, model.openMergeRequests > 0 {
                Text("MR \(model.openMergeRequests)").font(.app(9.5, weight: .semibold))
                    .padding(.horizontal, 4).padding(.vertical, 0.5)
                    .background(Capsule().fill(Color.accentColor.opacity(0.18)))
                    .help(L10n[.Branches.listRequestsHelp])
            }
            Spacer(minLength: 4)
        }
        .padding(.leading, branch == nil ? 0 : 14)
        .padding(.vertical, 2)
        .padding(.horizontal, 3)
        .modifier(NavRowSlot(current: open, gap: Self.rowGap))
        .onTapGesture {
            guard !open else { return }
            // From the board, whose sections are its own: the tree's root, not where the board was.
            let fromBoard = model.isBoardOpen
            model.openBranch(branch?.id)
            if fromBoard { model.openRootOfOpenTree() }
        }
        .help(branch == nil ? L10n[.Branches.listOpenMainHelp] : L10n[.Branches.listOpenBranchHelp(name)])
    }
}

/// Locking main (`specboard://SpecBoard/Docs/Design:DesignBranches` §8): from the window only — main's right-click in
/// the nav panel (`BranchList`) and the project's settings (`ProjectSettingsView`) — and asked first when it is let
/// go, in the same words from either.
enum MainLock {
    static var lockHelp: String { L10n[.Branches.lockHelp] }
    static var unlockTitle: String { L10n[.Branches.lockUnlockTitle] }
    static var unlockMessage: String { L10n[.Branches.lockUnlockMessage] }
}

extension View {
    /// Asked before main's lock is let go, and let go on 「外す」.
    func mainUnlockConfirmation(_ model: AppModel, isPresented: Binding<Bool>) -> some View {
        confirmationDialog(MainLock.unlockTitle, isPresented: isPresented) {
            Button(L10n[.Branches.lockUnlock]) { model.setMainLock(false) }
            Button(L10n[.Common.actionCancel], role: .cancel) {}
        } message: {
            Text(MainLock.unlockMessage)
        }
    }
}
