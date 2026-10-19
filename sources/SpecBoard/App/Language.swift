import Foundation
import Observation
import os

/// The language the screen says things in (`specboard://SpecBoard/Docs/Design:Localization` §1・§2): this
/// Mac's choice (`app.language`), `system` by default — the Mac's own language when there are texts in it,
/// else English.
///
/// Shared rather than passed down, for the reason `Typography` is: `L10n` is called from inside view
/// bodies that have no other reason to know about settings, and Observation still registers the read, so
/// a change draws every text again on the spot. What is not drawn by SwiftUI hears `changed`: the card
/// canvas draws its cards again, and the window reads again what the core said (the core makes its texts
/// in the language each request carries, `CoreClient`).
@Observable
final class Language {
    /// The languages the screen and the core have texts in, in the order they are offered: the same as
    /// the core's (`text::LANGUAGES`). A language is added here, to the core's list, and to every table.
    nonisolated static let supported = ["ja", "en"]

    /// Each language's name, in that language: whoever looks for their own finds it, whatever the
    /// screen is in.
    nonisolated static let names: [String: String] = ["ja": "日本語", "en": "English"]

    /// The choice that follows the machine's language.
    nonisolated static let systemChoice = "system"

    /// What can be chosen, in the order offered.
    nonisolated static let choices = [systemChoice] + supported

    /// The language when the machine's are none of `supported`: the one most who read neither of the
    /// others can read.
    nonisolated static let fallback = "en"

    /// Where this Mac keeps the choice.
    nonisolated static let key = "app.language"

    /// Posted on the main thread when the screen's language has changed.
    nonisolated static let changed = Notification.Name("SpecBoard.Language.changed")

    @MainActor static let shared = Language()

    /// What was chosen: `systemChoice` or one of `supported`.
    var choice: String {
        didSet {
            guard choice != oldValue else { return }
            UserDefaults.standard.set(choice, forKey: Self.key)
            code = Self.resolve(choice)
        }
    }

    /// The language the screen says things in now: one of `supported`.
    private(set) var code: String {
        didSet {
            guard code != oldValue else { return }
            Self.now.withLock { $0 = code }
            NotificationCenter.default.post(name: Self.changed, object: nil)
        }
    }

    private init() {
        let chosen = Self.storedChoice()
        let resolved = Self.resolve(chosen)
        choice = chosen
        code = resolved
        Self.now.withLock { $0 = resolved }
    }

    // MARK: - Off the main thread

    /// The screen's language as any thread reads it: what each request to the core carries (`lang`).
    nonisolated static var current: String { now.withLock { $0 } }

    /// `code`, kept where any thread can read it — from the first request on, whether or not the
    /// screen has looked at `shared` yet.
    private nonisolated static let now = OSAllocatedUnfairLock(initialState: resolve(storedChoice()))

    /// The choice this Mac keeps. Under tests none is read: they run in Japanese whatever was left
    /// behind (`machine`).
    private nonisolated static func storedChoice() -> String {
        guard !underTests, let stored = UserDefaults.standard.string(forKey: key),
              choices.contains(stored) else { return systemChoice }
        return stored
    }

    // MARK: - The machine's language

    /// Whether the app runs as the host of the tests: they read the screen's words, and run on
    /// machines in any language.
    nonisolated static let underTests: Bool = {
        let environment = ProcessInfo.processInfo.environment
        return ["XCTestConfigurationFilePath", "XCTestBundlePath", "XCTestSessionIdentifier", "XCInjectBundleInto"]
            .contains { environment[$0] != nil }
    }()

    /// The machine's languages, in the order its user prefers them; Japanese alone under tests.
    nonisolated static var machine: [String] {
        underTests ? ["ja"] : Locale.preferredLanguages
    }

    /// The language of `choice`: itself, or for `systemChoice` the first of `preferred` there are texts
    /// in, else `fallback`.
    nonisolated static func resolve(_ choice: String, preferred: [String] = machine) -> String {
        if supported.contains(choice) { return choice }
        return preferred.lazy.compactMap(supported(_:)).first ?? fallback
    }

    /// The language of a tag (`ja`, `ja-JP`, `en_US`), if there are texts in it.
    nonisolated static func supported(_ tag: String) -> String? {
        let base = tag.split(whereSeparator: { "-_.@".contains($0) }).first.map { $0.lowercased() } ?? ""
        return supported.contains(base) ? base : nil
    }

    /// How a choice is offered: `systemChoice` in the screen's language, a language by its own name.
    @MainActor
    static func said(_ choice: String) -> String {
        choice == systemChoice ? L10n[.Settings.languageSystem] : names[choice] ?? choice
    }
}

/// The one way the screen's own texts are looked up: in the String Catalogs (`Strings/`, a table for
/// each part of the screen), by the symbols Xcode makes of their keys — a key no table has does not
/// build — in the screen's language now, not the bundle's, so that a change shows at once.
enum L10n {
    /// `resource` in the screen's language. Read in a view's body, the view is drawn again when the
    /// language changes.
    static subscript(_ resource: LocalizedStringResource) -> String {
        text(resource, in: Language.shared.code)
    }

    /// `resource` in `code`, else in `Language.fallback`: a table without the text in that language
    /// says the English rather than its key.
    ///
    /// A text that takes nothing in is said as the table has it: lists draw many of them on every
    /// pass, and there is nothing in it to format.
    nonisolated static func text(_ resource: LocalizedStringResource, in code: String) -> String {
        let found = template(resource, in: code)
        let language = found == nil ? Language.fallback : code
        if let plain = found ?? template(resource, in: Language.fallback), !plain.contains("%") {
            return plain
        }
        var resource = resource
        resource.locale = Locale(identifier: language)
        return String(localized: resource)
    }

    /// The text of `resource` as `code`'s table has it, before anything is put into it.
    private nonisolated static func template(_ resource: LocalizedStringResource, in code: String) -> String? {
        guard let table = table(code) else { return nil }
        let missing = "\u{0}"
        let found = table.localizedString(forKey: resource.key, value: missing, table: resource.table)
        return found == missing ? nil : found
    }

    /// The tables of `code`, as built into the app.
    private nonisolated static func table(_ code: String) -> Bundle? {
        tables.withLockUnchecked { tables in
            if let found = tables[code] { return found }
            let found = Bundle(for: Token.self).path(forResource: code, ofType: "lproj").flatMap(Bundle.init(path:))
            tables[code] = found
            return found
        }
    }

    private nonisolated static let tables = OSAllocatedUnfairLock<[String: Bundle?]>(uncheckedState: [:])

    /// What finds the app's bundle, as the symbols Xcode makes do.
    private nonisolated final class Token {}
}
