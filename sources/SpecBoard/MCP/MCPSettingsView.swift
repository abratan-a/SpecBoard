import SwiftUI

/// Settings' 「MCP」: what is the app's — whether the agents may read and move this window — and where
/// MCP is answered (how MCP's questions are shown is with the other things that come up over the work,
/// in 「通知」: `NoticeSettingsView`). The app listens nowhere and relays nothing: each agent's own
/// answers MCP for its projects at its host's loopback, with that agent's access keys, so this Mac's
/// projects are not reached. Whether MCP was on and its port were the
/// relay's, and are read no more.
struct MCPSettings: View {
    var model: AppModel
    /// Whether the agents this app is connected to may read and move its window (`AppControl`).
    @AppStorage(AppControl.fromAgentsKey) private var fromAgents = true
    /// Goes to the projects' home, where the agents' access keys are made.
    var onKeys: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(L10n[.MCP.settingsIntro])
                .font(.app(11)).foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            HStack(spacing: 8) {
                Text(L10n[.MCP.settingsRegisterNote]).font(.app(11))
                Button(L10n[.MCP.settingsToKeys], action: onKeys)
            }
            Divider().padding(.vertical, 4)
            // The agents' own MCP reaching this window.
            Toggle(L10n[.MCP.settingsFromAgents], isOn: Binding(
                get: { fromAgents },
                set: { fromAgents = $0; model.reportWindows() }))
                .toggleStyle(.switch).controlSize(.small).font(.app(12))
            Text(L10n[.MCP.settingsFromAgentsNote])
                .font(.app(10.5)).foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
        }
    }
}
