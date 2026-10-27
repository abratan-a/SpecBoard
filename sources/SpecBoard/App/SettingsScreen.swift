import SwiftUI

/// The items of the settings screen, in the order its sidebar lists them
/// (`specboard://SpecBoard/Docs/Design:DesignAccess` §4). What is a place's — its connection, access keys
/// and what is public — is in the place's settings, on the projects' home.
enum SettingsPage: String, CaseIterable, Identifiable {
    case general, notifications, diagram, update, mcp, licenses

    var id: String { rawValue }

    var title: String {
        switch self {
        case .general: L10n[.Settings.pageGeneral]
        case .notifications: L10n[.Settings.pageNotifications]
        case .diagram: L10n[.Settings.pageDiagram]
        case .update: L10n[.Settings.pageUpdate]
        case .mcp: "MCP"
        case .licenses: L10n[.Settings.pageLicenses]
        }
    }

    var symbol: String {
        switch self {
        case .general: "gearshape"
        case .notifications: "bell"
        case .diagram: "point.3.connected.trianglepath.dotted"
        case .update: "arrow.down.circle"
        case .mcp: "network"
        case .licenses: "doc.text"
        }
    }
}

/// Settings, over the whole window: the items on the left, the chosen one's settings
/// on the right. 閉じる or Esc goes back to what was there, which stays as it was
/// underneath. The list of items reaches the top of the window as the nav panel does, the
/// window's buttons over its first row; the title row on the right is the top bar here
/// (overview §13 「窓の形」).
struct SettingsScreen: View {
    @Binding var page: SettingsPage?
    var model: AppModel
    var hosts: RemoteHosts
    var updater: Updater

    private var typography: Typography { Typography.shared }

    var body: some View {
        let shown = page ?? .general
        HStack(spacing: 0) {
            List(selection: Binding(get: { page }, set: { if let chosen = $0 { page = chosen } })) {
                ForEach(SettingsPage.allCases) { item in
                    Label(item.title, systemImage: item.symbol)
                        .font(.app(12.5))
                        .tag(item)
                }
            }
            .listStyle(.sidebar)
            // Its first row: under the window's buttons, and where the window is taken hold of.
            // An inset rather than a row above, so the list's own background reaches the top.
            .safeAreaInset(edge: .top, spacing: 0) {
                TopRow(room: 0) { Spacer(minLength: 0) }
            }
            .frame(width: 200)

            Divider()

            VStack(alignment: .leading, spacing: 0) {
                // No room: the list to its left carries the window's buttons (`TopBarLayout.room`).
                TopRow(room: 0, margin: 20) {
                    Text(shown.title).font(.app(15, weight: .semibold))
                    Spacer(minLength: 12)
                    Button(L10n[.Common.actionClose]) { page = nil }
                        .keyboardShortcut(.cancelAction)
                        .help(L10n[.Settings.closeHelp])
                }
                Divider()
                content(shown)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            }
        }
        .background(Color(nsColor: .windowBackgroundColor))
    }

    @ViewBuilder
    private func content(_ shown: SettingsPage) -> some View {
        switch shown {
        case .general: scrolling { general }
        case .notifications: scrolling { NoticeSettingsView() }
        case .diagram:
            scrolling {
                // How a line looks is its kind's definition's, not a setting of this Mac's; the
                // nav panel's marks and colours are the kinds of section's: what is left here is
                // the way a line goes.
                VStack(alignment: .leading, spacing: 22) {
                    settingsSection(L10n[.Settings.lineRoutingTitle]) { LineRoutingSetting() }
                }
            }
        case .update: scrolling { UpdateSettingsView(updater: updater) }
        case .mcp:
            scrolling {
                // The access keys are an agent's, on the projects' home: the first agent's (this Mac's
                // settings, saying it has none, when there is no agent).
                MCPSettings(model: model) {
                    page = nil
                    model.openPlaceSettings(hosts.hosts.first?.id, .keys)
                }
            }
        case .licenses: ThirdPartyNoticesView().padding(20)
        }
    }

    private func scrolling<Content: View>(@ViewBuilder _ content: () -> Content) -> some View {
        ScrollView {
            content()
                .frame(maxWidth: 720, alignment: .leading)
                .padding(20)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private var general: some View {
        VStack(alignment: .leading, spacing: 22) {
            settingsSection(L10n[.Settings.textSizeTitle]) {
                HStack(spacing: 8) {
                    Button(L10n[.Settings.textSizeSmaller]) { typography.reduce() }
                        .disabled(!typography.canReduce)
                    Text(typography.label)
                        .font(.app(11, design: .monospaced))
                        .frame(width: 52)
                    Button(L10n[.Settings.textSizeLarger]) { typography.enlarge() }
                        .disabled(!typography.canEnlarge)
                    Button(L10n[.Settings.textSizeDefault]) { typography.reset() }
                        .font(.app(10.5)).buttonStyle(.plain)
                        .foregroundStyle(.secondary)
                }
            }
            settingsSection(L10n[.Settings.languageTitle]) {
                LanguageSetting()
            }
        }
    }
}

/// The language the screen says things in (`Language`): this Mac's, changed on the spot.
struct LanguageSetting: View {
    private var language: Language { Language.shared }

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            LanguagePicker(choice: Binding(get: { language.choice }, set: { language.choice = $0 }))
            Text(L10n[.Settings.languageNote])
                .font(.app(10.5)).foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            Text(L10n[.Settings.languageMenusNote])
                .font(.app(10.5)).foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
        }
    }
}

/// A choice of `Language.choices`: the screen's.
struct LanguagePicker: View {
    @Binding var choice: String

    var body: some View {
        Picker("", selection: $choice) {
            ForEach(Language.choices, id: \.self) { choice in
                Text(Language.said(choice)).tag(choice)
            }
        }
        .labelsHidden().fixedSize()
        .font(.app(12))
    }
}

/// A titled group of settings, the same on every page.
func settingsSection<Content: View>(_ title: String, @ViewBuilder content: () -> Content) -> some View {
    VStack(alignment: .leading, spacing: 8) {
        Text(title)
            .font(.app(10.5, weight: .semibold))
            .tracking(1.2)
            .foregroundStyle(.secondary)
        content()
    }
}

/// How this Mac draws the lines between cards (`LineRouting`): round the cards, or one
/// curve each. The same switch in the settings' 「図」 and the relation panel's filters.
struct LineRoutingSetting: View {
    @AppStorage(LineRouting.key) private var avoids = LineRouting.standard

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Toggle(L10n[.Settings.lineRoutingAvoid], isOn: $avoids)
                .toggleStyle(.switch).controlSize(.small)
                .font(.app(12))
            Text(avoids ? L10n[.Settings.lineRoutingAvoidNote] : L10n[.Settings.lineRoutingCurveNote])
                .font(.app(10.5)).foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
        }
        .help(L10n[.Settings.lineRoutingHelp])
    }
}

/// What the settings screen lies over, while it is open: nothing there is
/// pressed or typed into. The screen is drawn over the project's, and a field the cursor had been
/// left in kept the keys typed over the settings — and wrote them to its card when they closed.
/// As the settings open, the keys are taken from whatever had them below.
struct CoveredBySettings: ViewModifier {
    var covered: Bool

    func body(content: Content) -> some View {
        content
            .disabled(covered)
            .accessibilityHidden(covered)
            .background(KeysLetGo(covered: covered))
    }

    /// Takes the keys from what had them in the window, once, as it is covered — the settings'
    /// own fields take them after.
    private struct KeysLetGo: NSViewRepresentable {
        var covered: Bool

        final class Coordinator { var covered = false }

        func makeCoordinator() -> Coordinator { Coordinator() }
        func makeNSView(context: Context) -> NSView { NSView() }

        func updateNSView(_ view: NSView, context: Context) {
            defer { context.coordinator.covered = covered }
            guard covered, !context.coordinator.covered, let window = view.window else { return }
            window.makeFirstResponder(nil)
        }
    }
}
