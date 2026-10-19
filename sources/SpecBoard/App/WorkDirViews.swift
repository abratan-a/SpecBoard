import SwiftUI

/// 「アクション ▸ 読み込む」 on a working directory's section or a folder in it (the nav panel's right-click):
/// its folder read again, or why it cannot be now.
struct WorkDirActionsMenu: View {
    var model: AppModel
    var section: String

    var body: some View {
        Menu(L10n[.Board.actionsMenu]) {
            let why = model.whyNotRead(section)
            Button(L10n[.Properties.workDirRead]) { model.readWorkDir(section) }
                .disabled(why != nil || !model.canEditWorkDirs)
                .help(why ?? L10n[.Properties.workDirReadHelp])
        }
    }
}

/// A working directory's section open in the property panel: the folder it is bound to on the machine its
/// project is kept on (bound, changed, let go — that machine's alone), how it is read (`readType`,
/// `exclude`: the project's, the same in every tree), what reading it came to, and 「読み込む」. Each
/// written through its section (`section_update`, `section_action`); its name and description are the
/// section's own.
struct WorkDirProperties: View {
    var model: AppModel
    var section: ProjectSection

    /// A folder being typed (an agent's host path).
    @State private var naming: NameAsk?

    var body: some View {
        if let dir = model.workDir(of: section.id) {
            let editable = model.canEditWorkDirs
            PropertyCategory(title: L10n[.Properties.workDirTitle]) {
                PropertyRow("folder", help: L10n[.Properties.workDirFolderHelp(model.openPlaceName)]) {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(dir.folder ?? L10n[.Export.dirsNoFolder(model.openPlaceName)])
                            .font(.app(11, design: dir.folder == nil ? .default : .monospaced))
                            .foregroundStyle(dir.folder == nil ? AnyShapeStyle(.orange) : AnyShapeStyle(.primary))
                            .lineLimit(2).truncationMode(.middle).textSelection(.enabled)
                        HStack(spacing: 6) {
                            Button(dir.folder == nil ? L10n[.Export.dirsLinkFolder] : L10n[.Export.dirsChangeFolder]) {
                                chooseFolder(dir)
                            }
                            if dir.folder != nil {
                                Button(L10n[.Export.dirsUnlinkFolder]) { model.bindWorkDirFolder(section.id, to: nil) }
                            }
                        }
                        .buttonStyle(.borderless).font(.app(11))
                        .disabled(!editable)
                    }
                }
                PropertyRow("readType", help: L10n[.Export.dirsReadHelp]) {
                    Picker("", selection: Binding(get: { dir.readType },
                                                  set: { model.setWorkDir(section.id, ["readType": $0]) })) {
                        ForEach(WorkDir.readTypes, id: \.id) { Text($0.label).tag($0.id) }
                    }
                    .labelsHidden().pickerStyle(.menu).fixedSize()
                    .disabled(!editable)
                }
                PropertyRow("exclude", help: L10n[.Properties.workDirExcludeHelp]) {
                    CommitTextEditor(text: dir.exclude.joined(separator: "\n"), minHeight: 40,
                                     placeholder: L10n[.Export.dirsExcludePlaceholder]) { raw in
                        let lines = raw.split(whereSeparator: \.isNewline)
                            .map { $0.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty }
                        if lines != dir.exclude { model.setWorkDir(section.id, ["exclude": lines]) }
                    }
                    .id("\(dir.id)|exclude|\(dir.exclude.joined(separator: ","))")
                    .disabled(!editable)
                }
                PropertyRow("files", help: L10n[.Properties.workDirFilesHelp]) {
                    VStack(alignment: .leading, spacing: 4) {
                        if let problem = dir.problem {
                            Text(problem).font(.app(11)).foregroundStyle(.orange)
                                .fixedSize(horizontal: false, vertical: true)
                        } else if let read = dir.entriesRead {
                            Text(L10n[.Export.dirsFilesRead(read.formatted())]).font(.app(11)).foregroundStyle(.secondary)
                        }
                        if let warning = dir.warning {
                            Text(warning).font(.app(10.5)).foregroundStyle(.orange)
                                .fixedSize(horizontal: false, vertical: true)
                        }
                        let why = model.whyNotRead(section.id)
                        Button(L10n[.Properties.workDirRead]) { model.readWorkDir(section.id) }
                            .controlSize(.small)
                            .disabled(why != nil || !editable)
                            .help(why ?? L10n[.Properties.workDirReadHelp])
                    }
                }
            }
            .nameSheet($naming)
        }
    }

    /// The folder chosen on this Mac, or typed for an agent's host (this Mac's panel shows this Mac's folders).
    private func chooseFolder(_ dir: WorkDir) {
        if model.core?.isAgent ?? false {
            naming = NameAsk(title: L10n[.Export.dirsAskTitle(model.openPlaceName, dir.name)],
                             placeholder: "~/repo", start: dir.folder ?? "", confirm: L10n[.Export.dirsLink],
                             cancel: L10n[.Common.actionCancel]) {
                model.bindWorkDirFolder(section.id, to: $0)
            }
        } else if let chosen = FolderPanel.choose(message: L10n[.Export.dirsPanelMessage(dir.name)]) {
            model.bindWorkDirFolder(section.id, to: chosen)
        }
    }
}

/// A folder of a working directory open in the property panel: read from the folder, where it is there,
/// and — for one reading no longer finds — that it is gone and what to do with the notes in it.
struct WorkDirFolderProperties: View {
    var model: AppModel
    var section: ProjectSection

    var body: some View {
        PropertyCategory(title: L10n[.Properties.workDirTitle]) {
            PropertyRow("filePath", help: L10n[.Properties.workDirFolderPathHelp]) {
                Text(section.description).font(.app(11, design: .monospaced)).textSelection(.enabled)
                    .foregroundStyle(section.missing ? AnyShapeStyle(.orange) : AnyShapeStyle(.primary))
                    .strikethrough(section.missing, color: .orange)
            }
            if section.missing {
                PropertyRow("missing", help: L10n[.Properties.workDirMissingHelp]) {
                    Text(L10n[.Properties.workDirMissing]).font(.app(11)).foregroundStyle(.orange)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
        }
    }
}
