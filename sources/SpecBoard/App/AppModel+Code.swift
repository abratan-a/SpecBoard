import Foundation

extension CoreClient {
    /// The functions and types of the files `ids` (`file_outline`): each read from the file now, the ones
    /// whose stamp is the one in `since` answered without them (`unchanged`). On any thread.
    nonisolated func fileOutlines(_ project: String, ids: [String], since: [String: String]) throws
        -> [(outline: FileOutline, unchanged: Bool)] {
        var fields: [String: Any] = ["project": project, "ids": ids]
        if !since.isEmpty { fields["since"] = since }
        let reply = try reply("file_outline", fields)
        guard let answer = try JSONSerialization.jsonObject(with: Data(reply.utf8)) as? [String: Any] else {
            throw Refusal.unreadable(reply)
        }
        if let error = answer["error"] as? String { throw Refusal(error) }
        let files = (answer["result"] as? [String: Any])?["files"] as? [[String: Any]] ?? []
        return files.compactMap { file in
            FileOutline(answer: file).map { ($0, file["unchanged"] as? Bool ?? false) }
        }
    }
}

extension AppModel {
    /// The files' functions and types read so far, by the file's id. Read in a view's body, the view is
    /// drawn again as more come in.
    var outlines: [String: FileOutline] {
        _ = outlineReads
        return outlineStore
    }

    /// A file's functions and types: as read so far, read now when they are not — on this turn, or off
    /// the main thread when the window reads so (`readsOffMain`), drawn once they are in.
    func outline(_ id: String) -> FileOutline? {
        askOutlines([id])
        _ = outlineReads
        return outlineStore[id]
    }

    /// The functions and types of the files `ids` that are not read yet, or were read before the
    /// project was read again: asked in one request, those read before with their stamps (`since`).
    func askOutlines(_ ids: [String]) {
        let wanted = Array(Set(ids.filter { (outlineStore[$0] == nil || outlineStale.contains($0)) && !outlineAsked.contains($0) }))
            .sorted()
        guard !wanted.isEmpty, let core, let projectId = detail?.project.id else { return }
        var since: [String: String] = [:]
        for id in wanted { if let stamp = outlineStore[id]?.stamp { since[id] = stamp } }
        guard readsOffMain else {
            putOutlines(Result { try core.fileOutlines(projectId, ids: wanted, since: since) }, asked: wanted)
            return
        }
        outlineAsked.formUnion(wanted)
        let back = WeakModel(self)
        let asked = Handed(value: (wanted, since))
        DispatchQueue.global(qos: .utility).async {
            let read = Handed(value: Result { try core.fileOutlines(projectId, ids: asked.value.0, since: asked.value.1) })
            DispatchQueue.main.async {
                MainActor.assumeIsolated {
                    guard let self = back.model, self.detail?.project.id == projectId else { return }
                    self.outlineAsked.subtract(asked.value.0)
                    self.putOutlines(read.value, asked: asked.value.0)
                    self.outlineReads += 1
                }
            }
        }
    }

    /// What the core said of the files asked, kept: a file it answered `unchanged` keeps what was read
    /// before; one it could not read says why, so that it is not asked again until the project is.
    private func putOutlines(_ read: Result<[(outline: FileOutline, unchanged: Bool)], Error>, asked: [String]) {
        switch read {
        case .success(let files):
            for (outline, unchanged) in files where !(unchanged && outlineStore[outline.id] != nil) {
                outlineStore[outline.id] = outline
            }
            for id in asked where outlineStore[id] == nil { outlineStore[id] = FileOutline(id: id) }
        case .failure(let error):
            for id in asked where outlineStore[id] == nil {
                outlineStore[id] = FileOutline(id: id, problem: error.localizedDescription)
            }
        }
        outlineStale.subtract(asked)
        outlineVersion += 1
    }

    /// What was read is asked again when next it is wanted: the files may have changed since.
    func staleOutlines() {
        outlineStale = Set(outlineStore.keys)
    }

    /// Another project: nothing read of this one's files is kept.
    func forgetOutlines() {
        outlineStore = [:]
        outlineStale = []
        outlineAsked = []
        outlineVersion += 1
    }

    /// A working directory's file as it is now, to show it (`file_text`): read each time it is asked
    /// for — the file is the folder's, not the project's — coloured by its language (`highlight`), and with
    /// the lines of its function or type `symbol` when one is asked.
    func fileText(_ id: String, highlight: Bool = false, symbol: String? = nil) -> FileText {
        guard let core, let projectId = detail?.project.id else { return FileText(problem: L10n[.Common.problemNoProject]) }
        var fields: [String: Any] = ["project": projectId, "id": id]
        if highlight { fields["highlight"] = true }
        if let symbol, !symbol.isEmpty { fields["symbol"] = symbol }
        do {
            let answer = try core.request("file_text", fields)
            return FileText(answer as? [String: Any] ?? [:])
        } catch {
            return FileText(problem: error.localizedDescription)
        }
    }

    /// The function or type picked, with the file it is in: a row of a file's card, the nav panel's row
    /// of it, one picked in the property panel.
    var selectedSymbol: (file: SchemaNode, path: String)? {
        guard case .symbol(let path)? = selectedRow, let file = selectedSchema, WorkDirIds.isFile(file.id) else { return nil }
        return (file, path)
    }

    /// A function or type opened: its file's text, scrolled to it and its lines tinted (`FileView`), the
    /// symbol picked in the property panel.
    func openSymbol(_ file: String, _ path: String) {
        open(.card(file))
        selectRow(file, .symbol(path))
    }

    /// A path to a function or type inside a working directory's file, in `detail`: the file's id and the
    /// symbol's path in it. Nil for any other path.
    func fileSymbol(_ detail: ProjectDetail, _ path: String, base: String?) -> (String, String)? {
        guard let written = WrittenPath(PathText.withoutTree(path)), !written.names.isEmpty,
              let file = detail.directory.find(written.card, from: base).schema, WorkDirIds.isFile(file.id)
        else { return nil }
        return (file.id, written.names.map(PathText.canonicalStep).joined(separator: "."))
    }

    /// The section of the working directory a path goes into when this machine does not read it
    /// (`intoUnreadWorkDir`), in `detail`. Nil for any other path.
    func unreadMount(_ detail: ProjectDetail, _ path: String) -> String? {
        let directory = detail.directory
        guard intoUnreadWorkDir(detail, path, directory: directory),
              let sections = PathText.splitSection(PathText.withoutTree(path.trimmingCharacters(in: .whitespaces))).section,
              let head = PathText.slashes(sections).first
        else { return nil }
        return directory.section(named: head)?.id
    }

    /// A function or type picked where its file is drawn: on its file's card, its row marked.
    func showSymbol(_ file: String, _ path: String) {
        guard let detail, let card = detail.directory.schema(id: file) else { return }
        perform(.show(card: file, section: detail.directory.sectionId(of: card), focus: file, row: path))
    }

    /// The lines that land on a function or type — on a type, on what is in it too —, as the project
    /// has them: one for each line, the card it leaves first.
    func linesArriving(atSymbol path: String, in file: String) -> [Relation] {
        guard let detail else { return [] }
        var seen: Set<String> = []
        return detail.relations.filter { relation in
            guard relation.toSchemaId == file, let at = relation.at, at == path || at.hasPrefix(path + "."),
                  let id = relation.connection else { return false }
            return seen.insert(relation.fromSchemaId + "|" + id).inserted
        }
    }

    /// What the core reads of a function or type for the property panel (`card_read` of its path): its
    /// declaration's line and the names it calls. Read once until the project is read again.
    func symbolRead(_ file: SchemaNode, _ path: String) -> SymbolRead {
        guard let detail else { return SymbolRead() }
        let target = SymbolPath.full(detail.directory.full(file), path)
        return aside("symbol|\(detail.project.id)|\(target)", "card_read", target: target, value: nil) { json in
            SymbolRead(json)
        }
    }
}

/// A function or type as the core reads it for the property panel (`card_read` of its path).
struct SymbolRead: Equatable {
    var declaration: String?
    var calls: [String] = []
    var problem: String?
    /// Read, or said why not: nil while it is being read.
    var read = false

    init() {}

    init(_ json: [String: Any]?) {
        guard let json else { return }
        read = true
        declaration = json["declaration"] as? String
        calls = json["calls"] as? [String] ?? []
        problem = json["problem"] as? String
    }
}

extension ProjectDetail {
    /// This tree with the functions and types read of its files put on their cards (`SchemaNode.symbols`),
    /// for the cards to draw them as rows.
    nonisolated func withSymbols(_ outlines: [String: FileOutline]) -> ProjectDetail {
        guard schemas.contains(where: { $0.isCodeFile && outlines[$0.id] != nil }) else { return self }
        var out = self
        out.schemas = schemas.map { card in
            guard card.isCodeFile, let outline = outlines[card.id] else { return card }
            var card = card
            card.symbols = outline.symbols
            return card
        }
        return out
    }
}
