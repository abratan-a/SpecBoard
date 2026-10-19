import SwiftUI

/// The top bar's own things (overview §13): above a project, 書き出す and まとめて直す — after the
/// project's name and the branch open, to the projects, only while the nav panel is shut (open, the nav
/// panel says them: never twice); above the projects, the way back to the project open, if one is. On
/// the right, the app's own things, the same on both: the bell (`NoticeBell`), 更新を確認, 設定 (whether
/// the agents answer MCP is in the status bar, `StatusBar`, always there). Between the two, the room
/// left over holds the notice bubble (`NoticeBubbleSlot`), reaching left from the bell. Laid in the top
/// row of the window (`WindowFrame`), whose empty parts take hold of the window.
struct TopBar: View {
    enum Over {
        case project(ProjectDetail)
        case projects
    }

    var over: Over
    var model: AppModel
    var updater: Updater
    /// The settings over the whole window, at this item; nil when shut.
    @Binding var settingsPage: SettingsPage?

    /// The widths of the row's three parts, measured: the room the bubble may take is what the left
    /// things and the app's own things (the bell and after) leave of the row.
    @State private var rowWidth: CGFloat = 0
    @State private var leftWidth: CGFloat = 0
    @State private var rightWidth: CGFloat = 0

    private static let gap: CGFloat = 14

    /// The room between the left things and the bell: the row less both, less the two gaps around the
    /// spacer between them.
    private var freeWidth: CGFloat { max(rowWidth - leftWidth - rightWidth - 2 * Self.gap, 0) }

    @Environment(\.navPanelShut) private var navPanelShut
    @Environment(\.colorScheme) private var colorScheme
    private var theme: GraphTheme { colorScheme == .dark ? .dark : .light }

    var body: some View {
        HStack(spacing: Self.gap) {
            HStack(spacing: Self.gap) {
                switch over {
                case .project(let detail):
                    if navPanelShut { projectName(detail) }
                    exportButton(detail.project.id)
                    groupButton
                case .projects:
                    backToProject
                }
            }
            .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { leftWidth = $0 }
            // The last thing in the bar to give way: it says which project this is.
            .layoutPriority(1)

            Spacer(minLength: 12)

            HStack(spacing: Self.gap) {
                NoticeBell(model: model)
                UpdatePill(updater: updater, theme: theme)
                settingsButton
            }
            .fixedSize()
            .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { rightWidth = $0 }
        }
        .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { rowWidth = $0 }
        // Over the row, not in it: the bubble moves nothing and covers nothing. It ends where the bell
        // begins and reaches left over the room between.
        .overlay(alignment: .trailing) {
            NoticeBubbleSlot(model: model, free: freeWidth)
                .padding(.trailing, rightWidth + Self.gap)
        }
    }

    /// Above the projects: the way back to the project open, if one is.
    @ViewBuilder
    private var backToProject: some View {
        if let project = model.detail?.project {
            Button { model.atHome = false } label: {
                HStack(spacing: 6) {
                    Image(systemName: "chevron.left").font(.app(11, weight: .semibold))
                    Text(project.name).font(.app(13, weight: .semibold)).lineLimit(1)
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .help(L10n[.Root.topBarBackHelp])
        } else {
            Text("SpecBoard").font(.app(13, weight: .semibold))
        }
    }

    /// The project's name, while the nav panel is shut: which project and which branch this is, and what
    /// somebody reaches for to change projects — it goes to the projects over the whole window, where
    /// choosing, making, renaming and deleting live (the nav panel's 「‹ プロジェクト一覧」 while it is open).
    private func projectName(_ detail: ProjectDetail) -> some View {
        Button {
            model.atHome = true
        } label: {
            HStack(spacing: 10) {
                Text(detail.project.name)
                    .font(.app(13, weight: .semibold))
                    .lineLimit(1)
                // Which branch is open: everything below is what it sees.
                if let branch = model.openBranch {
                    Label(branch.name, systemImage: "arrow.triangle.branch")
                        .font(.app(11.5, design: .monospaced))
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
            }
            // As wide as the name, so what follows sits beside it.
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        // The last thing in the bar to give way: it says which project this is.
        .layoutPriority(1)
        .help(L10n[.Root.topBarProjectsHelp])
    }

    // MARK: - Exports (§10)

    /// The sections written where their settings say, under the project's export root.
    private func exportButton(_ projectId: String) -> some View {
        Button {
            model.executeExportsAndSay(projectId)
        } label: {
            HStack(spacing: 5) {
                Image(systemName: "square.and.arrow.up")
                Text(L10n[.Root.topBarExport]).lineLimit(1)
            }
            .font(.app(11.5))
        }
        .buttonStyle(.borderless)
        .fixedSize()
        .disabled(model.isArchived)
        .help(L10n[.Root.topBarExportHelp])
    }

    /// Putting the open branch right in one go (`specboard://SpecBoard/Docs/Design:DesignBranches` §5): what is
    /// written until it is closed is one commit, and MCP may not write there. On a locked main
    /// too: closed, it goes into main as one merge (§8).
    private var groupButton: some View {
        Button {
            model.beginGroup()
        } label: {
            HStack(spacing: 5) {
                Image(systemName: "square.stack.3d.up")
                Text(L10n[.Root.topBarGroup]).lineLimit(1)
            }
            .font(.app(11.5))
        }
        .buttonStyle(.borderless)
        .fixedSize()
        .disabled(!model.canBeginGroup)
        .help(model.isLockedMainOpen ? L10n[.Root.topBarGroupLockedHelp] : L10n[.Root.topBarGroupHelp])
    }

    // MARK: - Settings

    /// Things that are true of the app rather than of what it is showing right now.
    /// The filter popover is a view being composed and gets put back; these are
    /// preferences, and they are written to disk.
    private var settingsButton: some View {
        Button {
            settingsPage = .general
        } label: {
            HStack(spacing: 5) {
                Image(systemName: "gearshape")
                Text(L10n[.Common.wordSettings]).font(.app(11.5))
            }
        }
        .buttonStyle(.plain)
        .keyboardShortcut(",", modifiers: .command)
        .help(L10n[.Root.topBarSettingsHelp])
    }
}
