import Foundation

extension AppModel {
    /// The project's rules and exports read again.
    func readExports(_ projectId: String) {
        guard let core = core(of: projectId) else { return }
        writingRules = (try? core.writingRules(projectId)) ?? [:]
        readExportSettings()
    }

    /// What to write at a place, as the project's Rules or the app say it.
    func writingRule(_ place: String) -> String {
        WritingRules.rule(place, project: writingRules)?.text ?? ""
    }

    /// One place's rule in the project's settings; empty lets the app's rule stand again.
    /// A person's to change under main's lock too.
    func setWritingRule(_ place: String, _ text: String) {
        guard let core, let projectId = detail?.project.id else { return }
        do {
            let written = text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? "" : text
            _ = try core.request("writing_rules_set", ["project": projectId, "place": place, "text": written])
            lastError = nil
        } catch {
            lastError = ErrorText.said(error)
        }
        readExports(projectId)
    }

    /// A person may change the project's settings under main's lock (`specboard://SpecBoard/Docs/Design:Overview`
    /// §10) — not in an archived project, nor while MCP holds it.
    var canEditExports: Bool { !isArchived && heldByMCP.isEmpty }

    // MARK: - The working directories

    /// The working directory whose section is `id`, or that a folder or file of id `id` is in.
    func workDir(of id: String) -> WorkDir? {
        let mount = WorkDirIds.mount(of: id) ?? id
        return workDirs.first { $0.section == mount }
    }

    /// Whether the section `id` is a working directory's or a folder in one: its folders and files are read
    /// from the folder, not written; only notes are put there.
    func isInWorkDir(section id: String) -> Bool {
        guard let section = detail?.directory.section(id: id) else { return false }
        return section.kindName == WorkDirIds.workdirKind || section.kindName == WorkDirIds.directoryKind
    }

    /// Whether `id` is what reading a working directory made — a file, or a folder of one — and so not
    /// written here.
    func isReadFromFolder(_ id: String) -> Bool {
        if WorkDirIds.isFile(id) { return true }
        return detail?.directory.section(id: id)?.kindName == WorkDirIds.directoryKind
    }

    /// A working directory's settings may be changed: the project's, not held by main's lock — not in an
    /// archived project, nor while MCP holds it.
    var canEditWorkDirs: Bool { !isArchived && heldByMCP.isEmpty }

    /// The working directory of the section `id` read again from its folder (「アクション ▸ 読み込む」,
    /// `section_action`'s `read`): what was made, changed or let go since shows, in every tree.
    func readWorkDir(_ id: String) {
        guard let path = detail?.directory.full(section: id) else { return }
        write { _ = try tool("section_action", target: path, ["op": "read"]) }
    }

    /// Why the working directory of the section `id` cannot be read now, or nil when it can: it reads a folder
    /// bound on the machine its project is kept on, and is not one that is not read (`none`).
    func whyNotRead(_ id: String) -> String? {
        guard let dir = workDir(of: id) else { return L10n[.Export.dirsNoFolder(openPlaceName)] }
        if dir.folder == nil { return L10n[.Export.dirsNoFolder(openPlaceName)] }
        if dir.readType == "none" { return L10n[.Export.dirsReadNone] }
        return nil
    }

    /// A working directory's settings written (`section_update` of its section): how it is read
    /// (`readType`, `exclude`) and its description — the project's, the same in every tree.
    func setWorkDir(_ id: String, _ value: [String: Any]) {
        guard !value.isEmpty, let path = detail?.directory.full(section: id) else { return }
        write { _ = try tool("section_update", target: path, value: value) }
    }

    /// The folder the working directory of the section `id` is bound to on the machine its project is kept
    /// on; nil lets it go. Read again from it.
    func bindWorkDirFolder(_ id: String, to folder: String?) {
        setWorkDir(id, ["folder": folder ?? NSNull()])
    }

    // MARK: - The exports

    /// An export tool run as the window on the project on screen — main's, whatever tree is open: its
    /// answer.
    @discardableResult
    func exportTool(_ name: String, target: Any? = nil, value: [String: Any]? = nil,
                    _ more: [String: Any] = [:]) throws -> OJSON {
        guard case .object(let wrapped) = try tool(name, target: target, value: value, more, inTree: false) else {
            throw CoreClient.Refusal(L10n[.Common.problemCoreUnreadable])
        }
        return wrapped["json"] ?? .null
    }

    /// The exports read again, each in full, and the files they write marked by their cards in the
    /// working directories' sections.
    func readExportSettings() {
        do {
            // The list says each export by its counts; only its name is taken from it.
            // Against each export's own folder on the machine the project is kept on (`[default]`).
            let listed = try JSONDecoder().decode(
                ExportsListed.self,
                from: Data(try exportTool("export_read", Self.defaultFolders).compact().utf8))
            let names = listed.exports.map(\.name)
            exports = names.isEmpty
                ? ExportsRead(exports: [], problems: listed.problems ?? [])
                : try JSONDecoder().decode(
                    ExportsRead.self,
                    from: Data(try exportTool("export_read", target: names, Self.defaultFolders).compact().utf8))
        } catch {
            exports = ExportsRead(exports: [], problems: [ErrorText.said(error)])
        }
        var marks: [String: ExportMark] = [:]
        for export in exports.exports {
            for file in export.files {
                // A file written into a working directory's folder is one of its section's.
                guard let place = file.directory,
                      let dir = workDirs.first(where: { $0.name == place.name }) else { continue }
                marks["file:\(dir.id)/\(place.path)"] = ExportMark(export: export.name, status: file.status, cards: file.cards)
            }
        }
        exportMarks = marks
    }

    /// A change to the exports, as the window makes it: the refusal said back, or nil; read again after.
    @discardableResult
    func changeExports(_ name: String, target: Any?, value: [String: Any]? = nil) -> String? {
        defer { readExportSettings() }
        do {
            try exportTool(name, target: target, value: value)
            lastError = nil
            return nil
        } catch {
            return ErrorText.said(error)
        }
    }

    /// What `targetdir` says for each export's own default folder (`export_read`, `export_execute`).
    static let defaultFolders: [String: Any] = ["targetdir": "[default]"]

    /// An export made with the folder it is written to by default on the machine the project is kept on,
    /// when one is given (what to write is added after, with its rows).
    func createExport(_ name: String, path: String? = nil) -> String? {
        var value: [String: Any] = [:]
        if let path, !path.isEmpty { value["path"] = path }
        return changeExports("export_create", target: name, value: value)
    }

    /// The folder an export is written to by default on the machine the project is kept on; nil lets it go
    /// (`export_update`'s `path`: a window's, not asked about).
    func setExportFolder(_ name: String, to folder: String?) -> String? {
        changeExports("export_update", target: name, value: ["path": folder ?? NSNull()])
    }

    func updateExport(_ name: String, _ value: [String: Any]) -> String? {
        changeExports("export_update", target: name, value: value)
    }

    func deleteExport(_ name: String) -> String? {
        changeExports("export_delete", target: [name])
    }

    /// Where a card is written out, as the core works it out (`card_read`'s `include: exports`): each file,
    /// the export that writes it, in its format.
    func writtenTo(_ id: String) -> [WrittenTo] {
        guard let path = try? cardPath(id),
              case .object(let wrapped)? = try? tool("card_read", target: path, value: ["include": ["exports"]]),
              case .object(let read)? = wrapped["json"], let files = read["writtenTo"] else { return [] }
        return (try? JSONDecoder().decode([WrittenTo].self, from: Data(files.compact().utf8))) ?? []
    }

    /// The exports `names` names written (every one, for none), and said: into each one's own folder, or into
    /// `folder` when it is given (a worktree, say; the default is not changed) — from main, or from `branch`
    /// into a folder given.
    func executeExportsAndSay(_ projectId: String, names: [String]? = nil, to folder: String? = nil,
                              branch: String? = nil) {
        Task {
            do {
                let summary = try await executeExports(projectId, names: names, to: folder, branch: branch)
                let written = summary.written.count, removed = summary.removed.count
                tell(notice: { code in
                    let from = branch.map { L10n.text(.Export.noticeFromBranch($0), in: code) } ?? ""
                    let gone = removed == 0 ? "" : L10n.text(.Export.noticeRemovedCount(removed), in: code)
                    return L10n.text(.Export.noticeExported, in: code) + from
                        + L10n.text(.Export.noticeFileCount(written), in: code) + gone
                })
            } catch {
                lastError = ErrorText.said(error)
            }
            readExportSettings()
        }
    }

    /// main written where the exports say, each into its own default folder on the machine the project is
    /// kept on, or into `folder` — main whatever branch is open, or `branch` when it is named, into a folder
    /// given only: the default folders are main's. Everything is checked before anything is written. The
    /// writing runs off the main thread.
    func executeExports(_ projectId: String, names: [String]? = nil, to folder: String? = nil,
                        branch: String? = nil) async throws -> ExportWriter.Summary {
        guard let core = core(of: projectId) else { throw ExportWriter.Failure(L10n[.Export.errorNotConnected]) }
        // A second write over one still going would race it for the same files and what each keeps.
        guard !exportsUnderWay.contains(projectId) else {
            throw ExportWriter.Failure(L10n[.Export.errorStillRunning])
        }
        exportsUnderWay.insert(projectId)
        defer { exportsUnderWay.remove(projectId) }
        var arguments: [String: Any] = ["project": projectId, "targetdir": folder ?? "[default]"]
        if let names { arguments["target"] = names }
        if let branch { arguments["branch"] = branch }
        let call = arguments
        // The core checks everything before it writes anything, writes, and leaves a mark in the history
        // of the tree written from (`export_execute`). Off the main thread: it writes files.
        let answer = try await Task.detached(priority: .utility) {
            try core.request("tool", ["name": "export_execute", "arguments": call])
        }.value
        reloadHistory()
        let json = ((answer as? [String: Any])?["json"] as? [String: Any]) ?? [:]
        let done = json["exports"] as? [[String: Any]] ?? []
        return ExportWriter.Summary(written: done.flatMap { $0["written"] as? [String] ?? [] },
                                    removed: done.flatMap { $0["removed"] as? [String] ?? [] })
    }
}

/// Writing main out into this Mac's folders (`export_execute`): what was written and removed, and why
/// it could not be.
enum ExportWriter {
    struct Summary: Equatable, Sendable {
        var written: [String]
        var removed: [String]
    }

    struct Failure: LocalizedError {
        var errorDescription: String?
        init(_ message: String) { errorDescription = message }
    }
}
