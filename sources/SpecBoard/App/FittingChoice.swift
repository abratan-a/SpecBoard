import SwiftUI

/// A choice among words the project holds — a merge request's title, a branch's name, the options a kind's or a
/// line's field defines — as the property panel offers it:
/// its button says the value chosen and no more (a merge request as `MR 8`), on one line cut short with …
/// where it is long, and the menu it opens says each option in full, the one chosen checked. Its
/// width is the value's, and it narrows to the column it is in. A menu picker is as wide as its
/// longest option, and one held at that width (`.fixedSize()`) pushed the whole of the panel's
/// content wider than the panel — over the diagram on the left, off the screen on the right.
///
/// `segmented`: a radio's options side by side while they fit the column, the menu when they do not.
struct FittingChoice: View {
    /// One option: the value it writes, what the menu says of it, and what the button says
    /// when it is the one chosen (its text when nil).
    struct Option {
        var tag: String
        var text: String
        var short: String?

        init(_ tag: String, _ text: String, short: String? = nil) {
            self.tag = tag
            self.text = text
            self.short = short
        }

        /// The empty choice, said `—`.
        static let none = Option("", "—")
    }

    var options: [Option]
    @Binding var selection: String
    var segmented = false

    var body: some View {
        if segmented {
            ViewThatFits(in: .horizontal) {
                Picker("", selection: $selection) {
                    ForEach(options, id: \.tag) { Text($0.text).tag($0.tag) }
                }
                .labelsHidden().pickerStyle(.segmented).fixedSize()
                menu
            }
        } else {
            menu
        }
    }

    private var menu: some View {
        Menu {
            Picker("", selection: $selection) {
                ForEach(options, id: \.tag) { Text($0.text).tag($0.tag) }
            }
            .labelsHidden().pickerStyle(.inline)
        } label: {
            Text(Self.shown(selection, in: options)).lineLimit(1).truncationMode(.tail)
        }
    }

    /// What the button says: the chosen option's short words, or the value itself where no
    /// option has it (`—` for none).
    static func shown(_ selection: String, in options: [Option]) -> String {
        guard let chosen = options.first(where: { $0.tag == selection }) else { return selection.isEmpty ? "—" : selection }
        return chosen.short ?? chosen.text
    }
}

/// The options of the choices the panel offers, as each kind of field has them.
extension Array where Element == FittingChoice.Option {
    /// Words to choose among, each shown as its kind says it (`labels`) else as it is written; `—`
    /// first when none may be chosen.
    static func words(_ options: [String], labels: [String: String] = [:], none: Bool) -> [Element] {
        (none ? [Element.none] : []) + options.map { Element($0, labels[$0] ?? $0) }
    }

    /// The project's branches, `—` first, and the one written where the project no longer has it.
    static func branches(_ names: [String], keeping value: String) -> [Element] {
        var out = [Element.none] + names.map { Element($0, $0) }
        if !value.isEmpty, !names.contains(value) { out.append(Element(value, value)) }
        return out
    }

    /// The project's PRs, `—` first: each by its number and title in the menu, by its number on the
    /// button; and the one written where the project no longer has it.
    static func mergeRequests(_ requests: [MergeRequest], keeping value: String) -> [Element] {
        var out = [Element.none] + requests.map { request in
            Element(String(request.number), "MR \(request.number) \(request.title)", short: "MR \(request.number)")
        }
        if !value.isEmpty, !requests.contains(where: { String($0.number) == value }) {
            out.append(Element(value, "MR \(value)"))
        }
        return out
    }
}
