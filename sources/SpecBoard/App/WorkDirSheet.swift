import AppKit
import SwiftUI

/// The sheet a working directory is added from (the navigator's ＋): its name and the folder on this Mac it is
/// read from, read as a git repository where the folder holds one. Refused (a name there already), said under
/// the name, the sheet left open.
struct WorkDirSheet: View {
    var model: AppModel
    var close: () -> Void

    @State private var name = ""
    @State private var folder: URL?
    @State private var refused: String?
    @FocusState private var focused: Bool

    /// Whether `folder` holds a git repository: read so, else as the files in it.
    static func readsAsGit(_ folder: URL) -> Bool {
        FileManager.default.fileExists(atPath: folder.appendingPathComponent(".git").path)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(L10n[.Nav.workDirSheetTitle]).font(.app(13, weight: .semibold))
            SwiftUI.Grid(alignment: .leading, horizontalSpacing: 10, verticalSpacing: 8) {
                GridRow {
                    Text(L10n[.Common.wordName]).font(.app(11)).foregroundStyle(.secondary)
                    VStack(alignment: .leading, spacing: 3) {
                        TextField(L10n[.Common.wordName], text: $name)
                            .textFieldStyle(.roundedBorder)
                            .font(.app(12))
                            .focused($focused)
                            .onChange(of: name) { refused = nil }
                        if let refused {
                            Text(refused).font(.app(10.5)).foregroundStyle(.red)
                                .fixedSize(horizontal: false, vertical: true)
                        }
                    }
                }
                GridRow {
                    Text(L10n[.Nav.workDirSheetFolder]).font(.app(11)).foregroundStyle(.secondary)
                    HStack(spacing: 6) {
                        Text(folder.map { ($0.path as NSString).abbreviatingWithTildeInPath } ?? L10n[.Nav.workDirSheetNoFolder])
                            .font(.app(11.5, design: .monospaced))
                            .foregroundStyle(folder == nil ? .secondary : .primary)
                            .lineLimit(1).truncationMode(.middle)
                            .frame(maxWidth: .infinity, alignment: .leading)
                        Button(L10n[.Nav.workDirSheetChoose], action: choose)
                    }
                }
                if let folder {
                    GridRow {
                        Color.clear.frame(width: 1, height: 1)
                        Text(Self.readsAsGit(folder) ? L10n[.Nav.workDirSheetReadGit] : L10n[.Nav.workDirSheetReadBasic])
                            .font(.app(10.5)).foregroundStyle(.secondary)
                    }
                }
            }
            HStack {
                Spacer()
                Button(L10n[.Common.actionCancelDialog], action: close)
                    .keyboardShortcut(.cancelAction)
                Button(L10n[.Nav.workDirSheetAdd], action: add)
                    .keyboardShortcut(.defaultAction)
                    .disabled(folder == nil || PathText.nameProblem(PathText.trimmed(name)) != nil)
            }
        }
        .padding(18)
        .frame(width: 440, alignment: .leading)
        .defaultFocus($focused, true)
        .onAppear { DispatchQueue.main.async { focused = true } }
    }

    private func choose() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = false
        guard panel.runModal() == .OK, let url = panel.url else { return }
        folder = url
        if name.isEmpty { name = url.lastPathComponent }
    }

    private func add() {
        guard let folder else { return }
        if let said = model.addWorkDir(name: PathText.trimmed(name), folder: folder) {
            refused = said
        } else {
            close()
        }
    }
}
