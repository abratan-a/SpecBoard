import SwiftUI

/// The head of a tab of the project's settings: its title, and what the tab is for a click away (the
/// (i) beside the title, as beside a property's name) — one line, so that the tab's own things begin
/// right under it however much there is to say.
struct TabHead<Trailing: View>: View {
    var title: String
    var about: String
    @ViewBuilder var trailing: Trailing

    init(_ title: String, about: String, @ViewBuilder trailing: () -> Trailing = { EmptyView() }) {
        self.title = title
        self.about = about
        self.trailing = trailing()
    }

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 6) {
            Text(title).font(.app(15, weight: .semibold))
            PropertyInfo(text: about, size: 12, width: 400, faint: false)
            Spacer()
            trailing
        }
    }
}
