import AppKit
import SwiftUI
import UniformTypeIdentifiers
import WebKit

/// An SVG or HTML card opened (`specboard://SpecBoard/Docs/Design:CardKinds`「SVG と HTML のカード」): its body
/// written on the left in a fixed-width face, and on the right the page it draws as it is written
/// (`PlainPage`), drawn again once the typing pauses. A file of this Mac read in replaces the body as
/// any edit does — written as the body, so it is kept in the history and undone as one.
struct PageCardEditor: View {
    var model: AppModel
    var card: SchemaNode
    var editable: Bool

    /// The body being written, and the card it was taken from: a draft is only ever saved back to it.
    @State private var draft = ""
    @State private var draftOf: String?
    /// The body as it was last taken or saved: a newer one written elsewhere replaces a draft that has
    /// not moved from it, and never one being written.
    @State private var taken = ""
    /// What the preview shows: the draft as it was when the typing paused.
    @State private var shown = ""
    @State private var pause: Task<Void, Never>?

    /// How long the typing pauses before the preview is drawn again.
    static let settle: Duration = .milliseconds(400)

    private var form: DocumentPage.Form { card.facePage?.form ?? (card.kind == .svg ? .svg : .html) }
    private var field: String { card.facePage?.field ?? "body" }
    private var text: String { card.facePage == nil ? card.body : card.facePageText }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            header
            Divider()
            HStack(spacing: 0) {
                TextEditor(text: editable ? $draft : .constant(text))
                    .font(.app(13, design: .monospaced))
                    .autocorrectionDisabled()
                    .scrollContentBackground(.hidden)
                    .padding(12)
                    .frame(maxWidth: .infinity)
                Divider()
                PagePreview(page: DocumentPage(editable ? shown : text, form: form)) { path in
                    _ = model.open(path: path, from: model.detail?.directory.sectionId(of: card))
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .onAppear { take() }
        .onChange(of: card.id) { _, _ in
            save()
            take()
        }
        .onChange(of: text) { _, newer in
            // Written elsewhere (an AI's write, an undo): taken when nothing is being written here.
            guard draftOf == card.id, draft == taken else { return }
            draft = newer
            taken = newer
            shown = newer
        }
        .onChange(of: draft) { _, _ in redrawLater() }
        .onDisappear {
            pause?.cancel()
            save()
        }
    }

    private var header: some View {
        CardBar(kind: card.kind, badge: card.kind == .custom ? card.kindDef?.badge : nil, name: card.name,
                label: card.label) {
            EmptyView()
        } tools: {
            Button { readFile() } label: {
                BarLabel(title: L10n[.CardEditor.pageReadFile], systemImage: "doc.badge.arrow.up", markWhenWide: true)
            }
            .disabled(!editable)
            .help(L10n[.CardEditor.pageReadFileHelp])
            Button { save() } label: {
                BarLabel(title: L10n[.CardEditor.pageSave], systemImage: "square.and.arrow.down")
            }
            .keyboardShortcut("s", modifiers: .command)
            .disabled(!editable || draft == text)
            .help(L10n[.CardEditor.pageSaveHelp])
            CardTools(model: model, card: card, editable: editable) { model.open(.card($0)) }
        }
        .font(.app(11))
        .padding(.horizontal, 14).padding(.vertical, 8)
    }

    private func take() {
        draft = text
        taken = text
        shown = text
        draftOf = card.id
    }

    private func redrawLater() {
        pause?.cancel()
        pause = Task { @MainActor in
            try? await Task.sleep(for: Self.settle)
            guard !Task.isCancelled else { return }
            shown = draft
        }
    }

    /// The draft written as the body; refused, it stays as written here and the core's word is shown
    /// as any refusal is.
    private func save() {
        guard editable, let id = draftOf, let current = model.schema(id) else { return }
        let before = current.facePage == nil ? current.body : current.facePageText
        guard before != draft else { return }
        if model.writePage(id, field: field, text: draft) == nil { taken = draft }
    }

    /// A file of this Mac chosen, read as UTF-8 and written as the body.
    private func readFile() {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = Self.fileTypes(form)
        panel.allowsMultipleSelection = false
        panel.message = L10n[.CardEditor.pageReadFileHelp]
        guard panel.runModal() == .OK, let url = panel.url else { return }
        guard let read = Self.read(url) else {
            model.tell(error: { L10n.text(.CardEditor.pageNotText(url.lastPathComponent), in: $0) })
            return
        }
        pause?.cancel()
        draft = read
        shown = read
        save()
    }

    /// What the panel offers: the page's own files first, and any text.
    static func fileTypes(_ form: DocumentPage.Form) -> [UTType] {
        form == .svg ? [.svg, .xml, .text] : [.html, .text]
    }

    /// A file's text, read as UTF-8; nil when it is not that.
    static func read(_ url: URL) -> String? {
        guard let data = try? Data(contentsOf: url) else { return nil }
        return String(data: data, encoding: .utf8)
    }
}

extension AppModel {
    /// An SVG or HTML card's page written (its field `field`, the body for the built-in kinds) as the
    /// window's own edit. The core's refusal, when it refuses — an SVG it cannot read, a body too long —
    /// left in `lastError` as any is, and handed back.
    @discardableResult
    func writePage(_ id: String, field: String, text: String) -> String? {
        lastError = nil
        write { try tool("card_update", target: cardPath(id), value: [field: text]) }
        return lastError
    }
}

/// The page an SVG or HTML card draws, as its editor previews it: scrolled when it is longer than the
/// pane, none of its scripts run, and a link pressed in it followed as a document's are — `sb-nav:` in
/// the app, a web address in the browser, nothing else.
struct PagePreview: NSViewRepresentable {
    var page: DocumentPage
    var open: (String) -> Void

    func makeCoordinator() -> Coordinator { Coordinator() }

    func makeNSView(context: Context) -> WKWebView {
        let config = WKWebViewConfiguration()
        config.websiteDataStore = .nonPersistent()
        config.defaultWebpagePreferences.allowsContentJavaScript = false
        let view = WKWebView(frame: .zero, configuration: config)
        view.navigationDelegate = context.coordinator
        PassiveWebView.transparent(view)
        view.setAccessibilityLabel(L10n[.CardEditor.previewTitle])
        context.coordinator.open = open
        load(into: view, context.coordinator)
        return view
    }

    func updateNSView(_ view: WKWebView, context: Context) {
        context.coordinator.open = open
        guard context.coordinator.digest != page.digest else { return }
        load(into: view, context.coordinator)
    }

    private func load(into view: WKWebView, _ coordinator: Coordinator) {
        coordinator.digest = page.digest
        view.loadHTMLString(PlainPage.html(page, clipped: false), baseURL: URL(string: "\(MarkdownPage.scheme)://page/"))
    }

    final class Coordinator: NSObject, WKNavigationDelegate {
        var digest: DocumentPage.Digest?
        var open: ((String) -> Void)?

        func webView(_ webView: WKWebView, decidePolicyFor action: WKNavigationAction, preferences: WKWebpagePreferences,
                     decisionHandler: @escaping @MainActor (WKNavigationActionPolicy, WKWebpagePreferences) -> Void) {
            preferences.allowsContentJavaScript = false
            switch PlainPage.visit(action.navigationType, mainFrame: action.targetFrame?.isMainFrame ?? false,
                                   url: action.request.url) {
            case .allow:
                decisionHandler(.allow, preferences)
                return
            case .browser(let url): NSWorkspace.shared.open(url)
            case .inApp(let path): open?(path)
            case .cancel: break
            }
            decisionHandler(.cancel, preferences)
        }
    }
}
