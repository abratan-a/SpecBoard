import AppKit

/// The right-click menu of a row of a formatdoc read as a document
/// (`specboard://SpecBoard/Docs/Design:UI` §5「formatdoc の文書の見え方」).
enum FormatDocMenus {

    /// What WebKit's own menu keeps beside the row's: the selection copied and the word looked up. The rest
    /// of it (translate, search the web, share, speech) is about the words, not the document.
    static let kept = ["WKMenuItemIdentifierCopy", "WKMenuItemIdentifierLookUp"]

    /// What a link to a path is written as in a body.
    static func link(_ path: String) -> String { "[[\(path)]]" }

    /// `menu`, the one WebKit is about to open, made into the row's at `names` in `doc`: the row's path as its
    /// head, its path and its link to copy and the row to edit (not when read only); then the item holding the
    /// row — its path in the document and its heading as a head, its path and its link to copy (not for a row at
    /// the top); then `kept`. A row no longer in the document leaves the menu as it is (false): the page shown
    /// is older than the rows.
    @discardableResult
    static func rowMenu(_ menu: NSMenu, row names: [String], in doc: SchemaNode, directory: SchemaDirectory,
                        editable: Bool, copy: @escaping (String) -> Void = copyToPasteboard,
                        edit: @escaping ([String]) -> Void) -> Bool {
        guard !names.isEmpty, FormatRows.row(at: names, in: doc.rows) != nil else { return false }
        let webKit = kept.compactMap { id in menu.items.first { $0.identifier?.rawValue == id } }
        menu.removeAllItems()

        let path = directory.full(doc, names)
        menu.addItem(.sectionHeader(title: path))
        menu.addItem(ActionMenuItem(L10n[.Common.actionCopyPath]) { copy(path) })
        menu.addItem(linkItem(L10n[.Docs.menuCopyAsLink]) { copy(link(path)) })
        if editable { menu.addItem(ActionMenuItem(L10n[.Docs.menuEditRow]) { edit(names) }) }

        let holder = Array(names.dropLast())
        if !holder.isEmpty, let item = FormatRows.row(at: holder, in: doc.rows) {
            let itemPath = directory.full(doc, holder)
            menu.addItem(.separator())
            menu.addItem(.sectionHeader(title: L10n[.Docs.menuItemHeader(PathText.joinNames(holder), item.title)]))
            menu.addItem(ActionMenuItem(L10n[.Docs.menuCopyItemPath]) { copy(itemPath) })
            menu.addItem(linkItem(L10n[.Docs.menuCopyItemAsLink]) { copy(link(itemPath)) })
        }

        if !webKit.isEmpty {
            menu.addItem(.separator())
            webKit.forEach(menu.addItem)
        }
        return true
    }

    /// A copy as a link, with what it copies shown at its right.
    private static func linkItem(_ title: String, run: @escaping () -> Void) -> NSMenuItem {
        let item = ActionMenuItem(title, run: run)
        item.badge = NSMenuItemBadge(string: link("…"))
        return item
    }
}
