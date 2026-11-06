import Foundation

/// A branch the merge window put into main and set aside, while a goal on the board still has it as its
/// work branch: told as the other notices are, with the finish offered beside it. The merge does not write
/// the board itself — whether the work is done is the person's to say.
extension AppModel {
    /// One goal told of: the first of those still naming the branch, as MCP's `branch_merge` offers it.
    struct GoalNotice: Equatable {
        /// Its entry in the bell's list (`NoticeLog.Entry.id`), which says it again in another language.
        let told: Int
        let text: String
        let project: String
        let goal: String
    }

    /// What a merge answered of the goals still naming the branch, told when there are any.
    func tellStillNamed(_ result: MergeResult, in project: String) {
        guard let goal = result.stillNamed.first else { return }
        let branch = result.preview.source
        let names = result.stillNamed.map { Self.cardName(of: $0) }
        let say: @MainActor (String) -> String = { code in
            L10n.text(.Branches.mergeGoalStillNamed(branch, names.joined(separator: L10n.text(.Common.separatorList, in: code))), in: code)
        }
        let text = say(Language.shared.code)
        // Told again (a goal of the same path made since), it is offered again.
        finishedGoals.remove(Self.finishedKey(goal, project))
        noticeLog.add(.notice, text, project: project, goal: goal, sayIn: say)
        goalNotice = GoalNotice(told: noticeLog.lastTold, text: text, project: project, goal: goal)
    }

    /// Whether a notice still offers to finish `goal`: not once it was finished from here, and only while
    /// its project is the one open, whose board the finish writes.
    func offersFinish(goal: String, in project: String) -> Bool {
        !finishedGoals.contains(Self.finishedKey(goal, project)) && detail?.project.id == project
    }

    /// The goal's last part of its path: its name.
    private static func cardName(of path: String) -> String {
        path.split(separator: ":").last.map(String.init) ?? path
    }

    static func finishedKey(_ goal: String, _ project: String) -> String { "\(project)|\(goal)" }
}
