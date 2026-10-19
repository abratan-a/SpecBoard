import Foundation

/// A working directory (`specboard://SpecBoard/Docs/Design:Overview` §10) — a repository of code, for
/// one, its root — as the core says it (`workdir_detail`'s `dirs`): one of the project's settings, read
/// by its way of being read and what it leaves out besides, and the top section of main and every branch
/// it shows as (`section`, `dir:<id>`). Where it is on the machine its project is kept on (`folder`) is
/// that machine's alone, never the project's.
nonisolated struct WorkDir: Decodable, Hashable, Identifiable, Sendable {
    var id: String
    /// The id of its section, the same in every tree.
    var section: String
    var name: String
    var description: String = ""
    /// How it is read (`readType`): `none` (not read: its root alone), `basic` (all but `.git/` and
    /// `.DS_Store`) or `git` (those and what `.gitignore` says).
    var readType: String = WorkDir.defaultReadType
    /// What it leaves out besides: a directory (`target/`, `sources/core/target/`), a file, an
    /// extension (`.png`).
    var exclude: [String] = []
    /// The folder it is bound to on the machine its project is kept on; nil while none is.
    var folder: String?
    /// What reading it came to there: how many files and how many directories, or why none (`problem`),
    /// and what it read otherwise than asked (`warning`: no git repository for `git`). Read, not kept.
    var files: Int?
    var directories: Int?
    var problem: String?
    var warning: String?

    static var readTypes: [(id: String, label: String)] {
        let language = Language.current
        return [
            ("none", L10n.text(.Export.dirsReadNone, in: language)),
            ("basic", L10n.text(.Export.dirsReadBasic, in: language)),
            ("git", L10n.text(.Export.dirsReadGit, in: language)),
        ]
    }
    static let defaultReadType = "git"

    init(id: String, section: String? = nil, name: String, description: String = "",
         readType: String = WorkDir.defaultReadType, exclude: [String] = []) {
        self.id = id
        self.section = section ?? WorkDirIds.section(of: id)
        self.name = name
        self.description = description
        self.readType = readType
        self.exclude = exclude
    }

    private enum CodingKeys: String, CodingKey {
        case id, section, name, description, readType, exclude, folder, files, directories, problem, warning
    }

    /// The directories and files read, together: what the property panel says it read.
    var entriesRead: Int? { files.map { $0 + (directories ?? 0) } }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        section = try c.decodeIfPresent(String.self, forKey: .section) ?? WorkDirIds.section(of: id)
        name = try c.decode(String.self, forKey: .name)
        description = try c.decodeIfPresent(String.self, forKey: .description) ?? ""
        readType = try c.decodeIfPresent(String.self, forKey: .readType) ?? Self.defaultReadType
        exclude = try c.decodeIfPresent([String].self, forKey: .exclude) ?? []
        folder = try c.decodeIfPresent(String.self, forKey: .folder)
        files = try c.decodeIfPresent(Int.self, forKey: .files)
        directories = try c.decodeIfPresent(Int.self, forKey: .directories)
        problem = try c.decodeIfPresent(String.self, forKey: .problem)
        warning = try c.decodeIfPresent(String.self, forKey: .warning)
    }
}

/// How the core names what a working directory shows as (`specboard://SpecBoard/Docs/Design:CoreApi`): its section
/// `dir:<id>`, a folder in it `dir:<id>/<path>`, a file `file:<id>/<path>` — the id of the working directory
/// first, so what belongs to which is read off the id.
nonisolated enum WorkDirIds {
    /// What the section kinds and the file's card kind are called.
    static let workdirKind = "workdir"
    static let directoryKind = "directory"
    static let fileKind = "file"

    /// The section of the working directory `id`.
    static func section(of id: String) -> String { "dir:" + id }

    /// The working directory's section a folder's or a file's id is in (`dir:<id>`); nil for any other id.
    static func mount(of id: String) -> String? {
        let rest: Substring
        if id.hasPrefix("dir:") { rest = id.dropFirst(4) } else if id.hasPrefix("file:") { rest = id.dropFirst(5) } else { return nil }
        let dir = rest.split(separator: "/", maxSplits: 1, omittingEmptySubsequences: false).first ?? ""
        return dir.isEmpty ? nil : "dir:" + dir
    }

    /// Whether `id` is a working directory's file.
    static func isFile(_ id: String) -> Bool { id.hasPrefix("file:") }
}

/// The exports (書き出し) as the core reads them (`export_read`): each by its name, what it writes from where
/// (`entries`) and leaves out (`exclude`), by path; where it is written to (`path`: its default folder on
/// the machine the project is kept on, for the `[default]` the window asks with); each file it
/// writes, the cards in it and how it stands against what was written last; what writing again
/// takes away; and what would stop the write.
nonisolated struct ExportsRead: Decodable, Equatable, Sendable {
    var exports: [ExportRead]
    var problems: [String]

    init(exports: [ExportRead] = [], problems: [String] = []) {
        self.exports = exports
        self.problems = problems
    }

    enum CodingKeys: String, CodingKey { case exports, problems }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        exports = try c.decodeIfPresent([ExportRead].self, forKey: .exports) ?? []
        problems = try c.decodeIfPresent([String].self, forKey: .problems) ?? []
    }
}

/// `export_read` without a target: each export by its name (and how many of its files stand each way).
nonisolated struct ExportsListed: Decodable, Sendable {
    nonisolated struct Listed: Decodable, Sendable { var name: String }
    var exports: [Listed]
    var problems: [String]?
}

/// One export as `export_read` says it, by its name.
nonisolated struct ExportRead: Decodable, Equatable, Sendable, Identifiable {
    var name: String
    var entries: [Entry]
    var exclude: [String]
    /// The folder it is written to by default on the machine the project is kept on; nil when none is set.
    var path: String?
    var files: [File]
    var removing: [String]
    var writtenAt: String?
    /// Why what was written was not read (no folder bound there).
    var note: String?

    var id: String { name }

    /// What goes where: a card or a section's cards (`from`, a path) into a file or a folder (`to`).
    nonisolated struct Entry: Decodable, Equatable, Hashable, Sendable {
        var from: String
        var to: String
        var format: String?
        var kinds: [String]?
    }

    /// One file it writes: what it holds (a format, `chart` or `document`), its cards by path, and how
    /// it stands.
    nonisolated struct File: Decodable, Equatable, Hashable, Sendable, Identifiable {
        var path: String
        var content: String
        var cards: [String]
        var status: ExportStatus?
        var changedCards: [String]?
        var outputChanged: Bool?
        /// Where in a working directory's tree it is, when it is written into that directory's folder.
        var directory: InDirectory?

        var id: String { path }
    }

    /// A working directory by its name, and a path in its tree.
    nonisolated struct InDirectory: Decodable, Equatable, Hashable, Sendable {
        var name: String
        var path: String
    }

    enum CodingKeys: String, CodingKey { case name, entries, exclude, path, files, removing, writtenAt, note }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        name = try c.decode(String.self, forKey: .name)
        entries = try c.decodeIfPresent([Entry].self, forKey: .entries) ?? []
        exclude = try c.decodeIfPresent([String].self, forKey: .exclude) ?? []
        path = try c.decodeIfPresent(String.self, forKey: .path)
        files = try c.decodeIfPresent([File].self, forKey: .files) ?? []
        removing = try c.decodeIfPresent([String].self, forKey: .removing) ?? []
        writtenAt = try c.decodeIfPresent(String.self, forKey: .writtenAt)
        note = try c.decodeIfPresent(String.self, forKey: .note)
    }
}

/// How a file an export writes stands against what it wrote last (`export_read`'s `status`).
nonisolated enum ExportStatus: String, Decodable, Sendable, CaseIterable {
    case current, changed, edited, missing, unwritten

    /// For a view: the text is read in the screen's language where SwiftUI sees it, so it is drawn again
    /// when the language changes.
    @MainActor var said: String {
        switch self {
        case .current: L10n[.Export.statusCurrent]
        case .changed: L10n[.Export.statusChanged]
        case .edited: L10n[.Export.statusEdited]
        case .missing: L10n[.Export.statusMissing]
        case .unwritten: L10n[.Export.statusUnwritten]
        }
    }
}

/// Where a file of a working directory is written from (`exportMarks`): the export, and how it stands. A
/// file is the one of a tree when its export is written into the directory's folder.
nonisolated struct ExportMark: Equatable, Sendable {
    var export: String
    var status: ExportStatus?
    var cards: [String]
}

/// One file a card is written to (`card_read`'s `writtenTo`): the export, the path from where it is
/// written to, and its format for a shape.
nonisolated struct WrittenTo: Decodable, Hashable, Sendable {
    var export: String?
    var file: String
    var format: String?
}

/// What a file holds, as `export_read` says it: a format, `chart`, `document` or `mock` (a ui section's page).
enum ExportWords {
    static let codeFormats = ["typescript", "zod", "json-schema"]

    static func label(_ content: String) -> String {
        switch content {
        case "chart": L10n[.Export.contentChart]
        case "document": L10n[.Export.contentDocument]
        case "mock": L10n[.Export.contentMock]
        default: formatLabel(content)
        }
    }

    static func formatLabel(_ format: String) -> String {
        switch format {
        case "typescript": "TypeScript"
        case "zod": "Zod"
        case "json-schema": "JSON Schema"
        default: format
        }
    }
}
