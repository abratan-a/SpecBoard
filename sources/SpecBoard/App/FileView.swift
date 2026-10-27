import SwiftUI

/// A working directory's file opened:
/// what it holds now, read from its folder on this Mac by the core (`file_text`) — a Markdown file
/// drawn as Markdown, any other as its text with its lines numbered, coloured by its language
/// (`CodeTextView`), the start of a long one — or why it cannot be shown. A function or type of it picked
/// (`AppModel.selectedSymbol`) is scrolled to and its lines tinted. Read only: the app writes no file.
struct FileView: View {
    var model: AppModel
    var card: SchemaNode

    /// What was read: kept on screen while the next is read (`key`).
    @State private var read: FileText?

    /// `read`: what is shown until the file is read (a picture drawn at once).
    init(model: AppModel, card: SchemaNode, read: FileText? = nil) {
        self.model = model
        self.card = card
        _read = State(initialValue: read)
    }

    /// The function or type of this file picked: the text is read with its lines.
    private var focus: String? {
        guard let picked = model.selectedSymbol, picked.file.id == card.id else { return nil }
        return picked.path
    }

    private var isMarkdown: Bool { card.label.lowercased().hasSuffix(".md") }

    private var key: String { "\(card.id)|\(focus ?? "")" }

    var body: some View {
        let read = self.read ?? FileText()
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 8) {
                Image(systemName: "doc.text").foregroundStyle(.secondary)
                Text(model.detail?.directory.full(card) ?? card.label).font(.app(13, weight: .semibold, design: .monospaced))
                    .lineLimit(1).truncationMode(.middle).textSelection(.enabled)
                if let focus {
                    Text("› \(focus)").font(.app(12, design: .monospaced)).foregroundStyle(.secondary).lineLimit(1)
                }
                if let mark = model.exportMarks[card.id] {
                    ExportMarkView(mark: mark, said: true)
                }
                Spacer()
                if let language = read.language ?? card.codeLanguage {
                    Text(CodeLanguages.name(language)).font(.app(10.5)).foregroundStyle(.secondary)
                }
                if let size = read.size {
                    Text(Int64(size).formatted(.byteCount(style: .file).locale(Language.shared.locale)))
                        .font(.app(10.5)).foregroundStyle(.tertiary)
                }
                if read.cut {
                    Text(L10n[.Export.fileStartOnly]).font(.app(10.5)).foregroundStyle(.orange)
                        .help(L10n[.Export.fileStartOnlyHelp(FileText.limitWords)])
                }
            }
            .padding(.horizontal, 14).padding(.vertical, 8)
            // What could not be done of what was asked: the symbol not found, the colours not made.
            ForEach([read.focusProblem, read.highlightProblem].compactMap { $0 }, id: \.self) { problem in
                Text(problem).font(.app(11)).foregroundStyle(.orange)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.horizontal, 14).padding(.bottom, 6)
            }
            Divider()
            if self.read == nil {
                Color.clear.frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if let problem = read.problem {
                Text(problem).font(.app(12)).foregroundStyle(.secondary)
                    .padding(16).frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            } else if isMarkdown {
                ScrollView {
                    MarkdownView(text: read.text, size: 13)
                        .padding(16).frame(maxWidth: .infinity, alignment: .leading)
                }
            } else {
                CodeTextView(read: read)
            }
        }
        .task(id: key) { load() }
    }

    /// The file read now — coloured where it has a language, with the lines of the symbol picked.
    private func load() {
        let highlight = !isMarkdown && card.codeLanguage != nil
        read = model.fileText(card.id, highlight: highlight, symbol: card.isCodeFile ? focus : nil)
    }
}

/// What the core read of a file to show it (`file_text`): its text (the start of a long one), and when
/// asked, its colours — runs of the text, in UTF-16 offsets, each with the class it is drawn as
/// (`classes`) — and the lines of a function or type of it.
struct FileText: Equatable {
    var text = ""
    var size: Int?
    var cut = false
    var problem: String?
    var language: String?
    /// The classes the runs name, by their place in this list.
    var classes: [String] = []
    /// `[from, length, class, from, length, class, …]` over `text`, in order, apart.
    var spans: [Int] = []
    var focus: Focus?
    var focusProblem: String?
    var highlightProblem: String?

    /// A function's or type's lines in the file, from 1: `from…to` with the comments above it, `line` its name's.
    struct Focus: Equatable {
        var from: Int
        var to: Int
        var line: Int
        var path: String
    }

    static let limitWords = "256 KB"

    init(text: String = "", size: Int? = nil, cut: Bool = false, problem: String? = nil) {
        self.text = text
        self.size = size
        self.cut = cut
        self.problem = problem
    }

    init(_ answer: [String: Any]) {
        text = answer["text"] as? String ?? ""
        size = answer["size"] as? Int
        cut = answer["cut"] as? Bool ?? false
        problem = answer["problem"] as? String
        language = answer["language"] as? String
        classes = answer["classes"] as? [String] ?? []
        spans = answer["spans"] as? [Int] ?? []
        if let focus = answer["focus"] as? [String: Any], let from = focus["from"] as? Int, let to = focus["to"] as? Int {
            self.focus = Focus(from: from, to: to, line: focus["line"] as? Int ?? from, path: focus["path"] as? String ?? "")
        }
        focusProblem = answer["focusProblem"] as? String
        highlightProblem = answer["highlightProblem"] as? String
    }
}

/// A file of a working directory that an export writes: a mark, the export and how the file stands, and
/// the cards written into it.
struct ExportMarkView: View {
    var mark: ExportMark
    /// With the export named beside it.
    var said = false

    var body: some View {
        HStack(spacing: 4) {
            Image(systemName: "square.and.arrow.up").font(.app(9.5))
                .foregroundStyle(mark.status.map(Self.color) ?? .secondary)
            if said {
                Text(L10n[.Export.labelExport(mark.export)]).font(.app(10.5)).foregroundStyle(.secondary)
                if let status = mark.status { ExportStatusTag(status: status) }
            }
        }
        .help(L10n[.Export.markHelp(mark.export)]
              + (mark.status.map { L10n[.Export.markHelpStatus($0.said)] } ?? "")
              + (mark.cards.isEmpty ? "" : L10n[.Export.markHelpCards(mark.cards.joined(separator: L10n[.Common.separatorList]))]))
    }

    static func color(_ status: ExportStatus) -> Color {
        switch status {
        case .current: .green
        case .changed: .orange
        case .edited, .missing: .red
        case .unwritten: .secondary
        }
    }
}
