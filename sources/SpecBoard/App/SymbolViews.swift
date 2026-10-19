import SwiftUI

/// A function's or type's mark (`SymbolBadge`): one letter in a small box, coloured by what it is.
struct SymbolBadgeView: View {
    var symbol: CodeSymbol

    @Environment(\.colorScheme) private var colorScheme

    var body: some View {
        Text(symbol.badge)
            .font(.app(9, weight: .bold, design: .monospaced))
            .foregroundStyle(Color(nsColor: GraphTheme.symbolInk(symbol.of, dark: colorScheme == .dark)))
            .frame(width: 14, height: 14)
            .background(RoundedRectangle(cornerRadius: 3)
                .fill(Color(nsColor: GraphTheme.symbolInk(symbol.of, dark: colorScheme == .dark)).opacity(0.14)))
            .help(symbol.word)
    }

    /// What a pause over it says: its language's word for it, the language, and its lines.
    static func help(_ symbol: CodeSymbol, language: String?) -> String {
        L10n[.Nav.symbolHelp(symbol.word, language.map(CodeLanguages.name) ?? "—", symbol.from, symbol.to)]
    }
}

/// A function or type picked, in the property panel: what it is in its language, its lines, the line that
/// declares it, the lines that land on it (the cards it carries out) and what it calls — the ones in
/// the same file a press away. Read only: it is made from the file.
struct SymbolProperties: View {
    var model: AppModel
    var file: SchemaNode
    var path: String

    var body: some View {
        let outline = model.outline(file.id)
        let symbol = outline?.symbol(path)
        let read = model.symbolRead(file, path)
        let directory = model.detail?.directory
        PropertyCategory(title: L10n[.Properties.symbolTitle]) {
            PropertyRow("name", help: Glossary.term("symbol")?.meaning ?? "") {
                Text(symbol?.name ?? PathText.lastName(path)).font(.app(11.5, design: .monospaced)).textSelection(.enabled)
            }
            PropertyRow("path", help: L10n[.Properties.symbolPathHelp]) {
                Text(directory.map { SymbolPath.full($0.full(file), path) } ?? path)
                    .font(.app(11, design: .monospaced)).textSelection(.enabled)
                    .lineLimit(2).truncationMode(.middle)
            }
            PropertyRow("of", help: L10n[.Properties.symbolOfHelp]) {
                HStack(spacing: 6) {
                    if let symbol { SymbolBadgeView(symbol: symbol) }
                    Text(Self.kindSaid(symbol, language: outline?.language)).foregroundStyle(.secondary)
                }
            }
            PropertyRow("lines", help: L10n[.Properties.symbolLinesHelp]) {
                if let symbol {
                    Text(L10n[.Properties.symbolLines(symbol.from, symbol.to)]).font(.app(11, design: .monospaced))
                } else {
                    Text(L10n[.Properties.symbolGone]).font(.app(11)).foregroundStyle(.orange)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            PropertyRow("declaration", help: L10n[.Properties.symbolDeclarationHelp]) {
                Group {
                    if let declaration = read.declaration {
                        Text(declaration).font(.app(11, design: .monospaced)).textSelection(.enabled)
                            .fixedSize(horizontal: false, vertical: true)
                    } else if let problem = read.problem {
                        Text(problem).font(.app(11)).foregroundStyle(.orange)
                    } else {
                        Text(read.read ? "—" : L10n[.Properties.symbolReading]).font(.app(11)).foregroundStyle(.tertiary)
                    }
                }
            }
            HStack {
                Button(L10n[.Properties.symbolOpen]) { model.openSymbol(file.id, path) }
                    .controlSize(.small)
                    .help(L10n[.Properties.symbolOpenHelp])
                Spacer()
            }
            .padding(.horizontal, 18).padding(.vertical, 5)
        }
        incoming
        calls(read.calls, outline)
    }

    /// What it is, as its language says it, and the language: `fn · Rust`.
    static func kindSaid(_ symbol: CodeSymbol?, language: String?) -> String {
        [symbol?.word, language.map(CodeLanguages.name)].compactMap { $0 }.joined(separator: " · ")
    }

    /// The lines landing here — on a type, on its members too: where each leaves, its card shown with the
    /// arrow, the line picked with a press.
    private var incoming: some View {
        let arriving = model.linesArriving(atSymbol: path, in: file.id)
        let kinds = model.detail?.lineKinds ?? LineKinds()
        return PropertyCategory(title: L10n[.Lines.incomingTitle], count: arriving.count) {
            if arriving.isEmpty {
                Text(L10n[.Lines.incomingEmpty]).font(.app(11)).foregroundStyle(.tertiary)
                    .padding(.horizontal, 18).padding(.vertical, 4)
            }
            ForEach(arriving, id: \.self) { relation in
                HStack(spacing: 6) {
                    LineSample(look: kinds.look(relation.line))
                    Button {
                        if let id = relation.connection { model.selectLine(LineRef(card: relation.fromSchemaId, id: id)) }
                    } label: {
                        VStack(alignment: .leading, spacing: 1) {
                            HStack(spacing: 4) {
                                Text(kinds.label(relation.line)).font(.app(11.5))
                                if let at = relation.at, at != path {
                                    Text("→ \(at)").font(.app(10, design: .monospaced)).foregroundStyle(.tertiary)
                                }
                            }
                            Text(model.leaving(relation))
                                .font(.app(10.5, design: .monospaced)).foregroundStyle(.secondary)
                                .lineLimit(1).truncationMode(.head)
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .help(L10n[.Lines.incomingHelpSelect])
                    Button { LineList.showSource(model, relation) } label: { Image(systemName: "arrow.up.forward.square") }
                        .buttonStyle(.borderless)
                        .help(L10n[.Lines.incomingHelpShowSource])
                }
                .padding(.horizontal, 18).padding(.vertical, 3)
            }
        }
    }

    /// The names it calls, as the core read them by name alone: a name that is a function or type of this
    /// file is a button to it.
    private func calls(_ names: [String], _ outline: FileOutline?) -> some View {
        PropertyCategory(title: L10n[.Properties.symbolCallsTitle], count: names.count) {
            Text(L10n[.Properties.symbolCallsApprox]).font(.app(10.5)).foregroundStyle(.tertiary)
                .fixedSize(horizontal: false, vertical: true)
                .padding(.horizontal, 18).padding(.vertical, 3)
            if names.isEmpty {
                Text(L10n[.Common.wordNone]).font(.app(11)).foregroundStyle(.tertiary)
                    .padding(.horizontal, 18).padding(.vertical, 4)
            }
            FlowRow(spacing: 6, lineSpacing: 4) {
                ForEach(names, id: \.self) { name in
                    if let target = Self.call(name, in: outline) {
                        Button(name) { go(to: target.path) }
                            .buttonStyle(.link)
                            .font(.app(11, design: .monospaced))
                            .help(L10n[.Properties.symbolCallHelp(target.path)])
                    } else {
                        Text(name).font(.app(11, design: .monospaced)).foregroundStyle(.secondary)
                    }
                }
            }
            .padding(.horizontal, 18).padding(.vertical, 4)
        }
    }

    /// The function or type of the same file a called name is: the first by that name, wherever it sits.
    static func call(_ name: String, in outline: FileOutline?) -> CodeSymbol? {
        outline?.symbols.first { $0.name == name }
    }

    /// Where a press on a call goes: on the file's text while it is open, else to its row where its file is
    /// drawn.
    private func go(to target: String) {
        if model.openCard?.id == file.id {
            model.selectRow(file.id, .symbol(target))
        } else {
            model.showSymbol(file.id, target)
        }
    }
}

/// A file whose functions and types are listed, picked in the property panel: its language, how many it
/// has at the top, and why some may be missing.
struct FileSymbolsProperties: View {
    var model: AppModel
    var file: SchemaNode

    var body: some View {
        let outline = model.outline(file.id)
        PropertyCategory(title: L10n[.Properties.symbolsTitle], count: outline?.symbols.count) {
            PropertyRow("language", help: L10n[.Properties.fileLanguageHelp]) {
                Text(file.codeLanguage.map(CodeLanguages.name) ?? "—").foregroundStyle(.secondary)
            }
            if let problem = outline?.problem {
                PropertyRow("problem", help: L10n[.Properties.fileSymbolsProblemHelp]) {
                    Text(problem).font(.app(11)).foregroundStyle(.orange).fixedSize(horizontal: false, vertical: true)
                }
            }
            if let errors = outline?.parseErrors, errors > 0 {
                PropertyRow("parseErrors", help: L10n[.Properties.fileParseErrorsHelp]) {
                    Text("\(errors)").font(.app(11, design: .monospaced)).foregroundStyle(.orange)
                }
            }
            ForEach(outline?.children(of: nil) ?? [], id: \.path) { symbol in
                Button { model.selectRow(file.id, .symbol(symbol.path)) } label: {
                    HStack(spacing: 6) {
                        SymbolBadgeView(symbol: symbol)
                        Text(symbol.name).font(.app(11.5, design: .monospaced)).lineLimit(1)
                        Spacer(minLength: 4)
                        Text("\(symbol.line)").font(.app(10, design: .monospaced)).foregroundStyle(.tertiary)
                    }
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .help(SymbolBadgeView.help(symbol, language: outline?.language))
                .padding(.horizontal, 18).padding(.vertical, 2)
            }
        }
    }
}
