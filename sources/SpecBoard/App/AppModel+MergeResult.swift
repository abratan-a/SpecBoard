import Foundation

/// What a merge into main would leave, looked at as the project it would be: the core makes a
/// branch of its own from main with the merge in it (`merge_result_open`, `merge_request_open`),
/// the window opens it as it opens a branch — its marks say which cards the merge changes and
/// makes, the property panel what of each — and lets it go when it is closed
/// (`merge_result_close`). Only read there; the banner above says what it is, what goes, and what
/// may be done (`ResultBanner`).
extension AppModel {
    struct ResultView: Equatable {
        enum Of: Equatable {
            /// A merge request, by its number.
            case request(Int)
            /// The merge window's merge of a branch into main, as it was set there.
            case merge(source: String, take: CoreClient.MergeTake, places: Bool, keep: Bool)
        }

        /// The core's branch holding it.
        var branch: String
        var of: Of
        /// What it is, as it was said when it was opened; the banner says it again in the language now
        /// (`AppModel.resultTitle`).
        var title: String
        /// What the merge takes away, by path (sections too): said on the banner.
        var removed: [String]
        /// The tree the window had open before: back there when it is closed (nil, main).
        var back: String?
        /// A merge request tried on what has moved since: what is shown is the merge as it is now.
        var stale: Bool
        /// The cards it takes away, as main has them now: drawn on their
        /// sections' diagrams where main puts them, faded and struck through (`Surroundings.gone`).
        /// Not the cards inside a container, nor card ghosts: those are said on the banner only.
        var gone: [SchemaNode] = []

        /// The open tree's name while one is shown (`AppModel.openTree`).
        static var word: String { L10n[.Branches.resultTreeName] }
    }

    /// A merge request's result opened, a card of it picked when `card` names one.
    func openRequestResult(_ number: Int, places: Bool? = nil, card: String? = nil) {
        guard let core, let projectId = detail?.project.id,
              let request = mergeRequests.first(where: { $0.number == number }) else { return }
        do {
            let back = resultView?.back ?? openTrees[projectId]
            let opened = try core.openMergeRequestResult(projectId, number: number, places: places)
            show(ResultView(branch: opened.branch, of: .request(number), title: Self.requestTitle(request),
                            removed: Self.removed(opened.preview), back: back, stale: request.state == .stale,
                            gone: gone(projectId, opened.preview)),
                 in: projectId, card: card)
        } catch {
            lastError = ErrorText.said(error)
        }
    }

    /// The merge window's merge into main, opened as it would leave main.
    func openMergeResult(source: String, take: CoreClient.MergeTake, places: Bool, keep: Bool = false) {
        guard let core, let projectId = detail?.project.id else { return }
        do {
            let back = resultView?.back ?? openTrees[projectId]
            let opened = try core.openMergeResult(projectId, source: source, take: take, places: places)
            show(ResultView(branch: opened.branch, of: .merge(source: source, take: take, places: places, keep: keep),
                            title: mergeTitle(source: source), removed: Self.removed(opened.preview),
                            back: back, stale: false, gone: gone(projectId, opened.preview)),
                 in: projectId, card: nil)
        } catch {
            lastError = ErrorText.said(error)
        }
    }

    /// What the result shown is, in the language now: what the banner says after "as it would be after".
    func resultTitle(_ view: ResultView) -> String {
        switch view.of {
        case .request(let number):
            guard let request = mergeRequests.first(where: { $0.number == number }) else { return view.title }
            return Self.requestTitle(request)
        case .merge(let source, _, _, _):
            return mergeTitle(source: source)
        }
    }

    private static func requestTitle(_ request: MergeRequest) -> String {
        L10n[.Branches.resultTitleRequest(request.number, request.title, request.branchName)]
    }

    private func mergeTitle(source: String) -> String {
        L10n[.Branches.resultTitleMerge(branches.first { $0.id == source }?.name ?? source)]
    }

    /// Back to the tree the window had open, the result let go.
    func closeResult() {
        guard let projectId = detail?.project.id, let shown = resultView else { return }
        resultView = nil
        openTrees[projectId] = shown.back
        try? core?.closeMergeResult(projectId)
        lastError = nil
        select(projectId)
    }

    /// The merge request shown, approved: into main as it was tried, then back.
    func approveShownRequest(places: Bool?) {
        guard case .request(let number)? = resultView?.of else { return }
        let asked = mergeRequests.first { $0.number == number }
        closeResult()
        approveMergeRequest(number, places: places ?? asked?.places ?? false)
    }

    /// The merge window's merge shown, done: into main as it was set, then back.
    func mergeShownResult() {
        guard case .merge(let source, let take, let places, let keep)? = resultView?.of else { return }
        closeResult()
        _ = merge(source: source, target: nil, take: take, places: places, keep: keep)
    }

    private func show(_ view: ResultView, in projectId: String, card: String?) {
        resultView = view
        openTrees[projectId] = view.branch
        lastError = nil
        select(projectId)
        if let card { open(path: card) }
    }

    private static func removed(_ preview: MergePreview) -> [String] {
        preview.items.filter { $0.change == .remove }.map(\.path)
    }

    /// The cards the merge takes away, read from main as it is now: a section's own cards, each
    /// in its section, where main puts it.
    private func gone(_ projectId: String, _ preview: MergePreview) -> [SchemaNode] {
        let paths = Set(preview.items.filter { $0.change == .remove && !$0.section }.map(\.path))
        guard !paths.isEmpty, let main = try? core?.detail(projectId, branch: nil) else { return [] }
        let directory = main.directory
        return main.schemas.filter { $0.container == nil && paths.contains(directory.full($0)) }
    }
}
