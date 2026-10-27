import SwiftUI

/// The toolbar's word on updates, always in the same place.
///
/// 設定 is not somewhere people go, so asking and acting both live in the bar: the
/// button says 更新を確認 and asks now; a new version found and not fetched says
/// ダウンロード and where from, outlined, since pressing it leaves the app running; once
/// fetched and checked it says 今すぐアップデート, filled, and replaces the app. A replacement
/// that failed takes the spot until it is dismissed; one that went through says
/// nothing. A build outside Applications never updates itself, so it shows nothing
/// here.
struct UpdatePill: View {
    let updater: Updater
    let theme: GraphTheme
    /// A few seconds of 最新です after asking by hand, so the click visibly did something.
    @State private var flash: String?

    var body: some View {
        if let notice = updater.notice {
            pill(notice.ok ? "checkmark.circle" : "exclamationmark.triangle",
                 notice.message) { updater.notice = nil }
        } else if updater.installed {
            button
        }
    }

    @ViewBuilder
    private var button: some View {
        switch updater.phase {
        case .available(let release):
            Button {
                Task { await updater.download() }
            } label: {
                label("arrow.down.circle", L10n[.Update.pillDownload(release.version, feedName)], filled: false)
                    .fixedSize()
            }
            .buttonStyle(.plain)
            .help(downloadHelp(release))
        case .ready(let release):
            Button {
                Task { await updater.updateNow() }
            } label: {
                label("arrow.down.circle", readyLabel(release)).fixedSize()
            }
            .buttonStyle(.plain)
            .help(readyHelp(release))
        case .installing(let release):
            label("arrow.triangle.2.circlepath", L10n[.Update.statusInstalling(release.label)])
        case .checking:
            working(L10n[.Update.statusChecking])
        case .rechecking:
            working(L10n[.Update.statusRechecking])
        case .downloading(let release):
            working(L10n[.Update.statusDownloading(release.label)])
        case .failed(let message):
            ask(symbol: "exclamationmark.triangle", help: message)
        case .idle, .current:
            ask(symbol: "arrow.clockwise", help: lastChecked)
        }
    }

    private func ask(symbol: String, help: String) -> some View {
        Button {
            Task { await check() }
        } label: {
            HStack(spacing: 5) {
                Image(systemName: flash == nil ? symbol : "checkmark")
                Text(flash ?? L10n[.Update.pillCheck]).font(.app(11.5))
            }
        }
        .buttonStyle(.plain)
        .help(help)
    }

    private func working(_ text: String) -> some View {
        HStack(spacing: 6) {
            ProgressView().controlSize(.small)
            Text(text).font(.app(11.5)).foregroundStyle(.secondary)
        }
    }

    private func check() async {
        await updater.check()
        guard case .current = updater.phase else { return }
        flash = L10n[.Update.pillUpToDate]
        try? await Task.sleep(for: .seconds(3))
        flash = nil
    }

    private var feedName: String { updater.selectedFeed?.displayName ?? L10n[.Update.feedFallbackName] }

    /// What pressing asks, and how often the place in use is asked without pressing.
    private var lastChecked: String {
        let when = updater.lastChecked.map {
            L10n[.Update.statusCheckedAt($0.formatted(Date.FormatStyle(date: .omitted, time: .shortened, locale: Language.shared.locale)))]
        } ?? ""
        let periodic = updater.selectedFeed.map { updater.periodic(for: $0.id) } ?? .standard
        let help = periodic.on
            ? L10n[.Update.pillCheckHelp(feedName, UpdateIntervals.inSentence(periodic.minutes))]
            : L10n[.Update.pillCheckHelpOff(feedName)]
        return help + when
    }

    /// Where it comes from and which version — the build is in the help.
    private func readyLabel(_ release: Release) -> String {
        let from = [updater.selectedFeed?.displayName, release.version].compactMap { $0 }.joined(separator: " ")
        return L10n[.Update.pillUpdateNow(from)]
    }

    private func readyHelp(_ release: Release) -> String {
        withNotes(L10n[.Update.pillReadyHelp(updater.label, release.label)], release)
    }

    private func downloadHelp(_ release: Release) -> String {
        withNotes(L10n[.Update.pillDownloadHelp(updater.label, release.label)], release)
    }

    /// The release's notes, and why the last check failed when what was found before still stands.
    private func withNotes(_ text: String, _ release: Release) -> String {
        var text = text
        let notes = release.notes.trimmingCharacters(in: .whitespacesAndNewlines)
        if !notes.isEmpty { text += "\n\n" + notes }
        if let problem = updater.lastProblem { text += "\n\n" + L10n[.Update.statusLastProblem(problem)] }
        return text
    }

    /// Filled for the button that quits the app, outlined for one that leaves it running: the two sit
    /// in the same place and must not be taken for each other.
    private func label(_ symbol: String, _ text: String, filled: Bool = true) -> some View {
        HStack(spacing: 5) {
            Image(systemName: symbol).font(.app(10.5))
            Text(text).font(.app(11.5, weight: .medium))
        }
        .padding(.horizontal, 8).padding(.vertical, 3)
        .background(RoundedRectangle(cornerRadius: 5).fill(filled ? Color(nsColor: theme.brassSoft) : Color.clear))
        .overlay(RoundedRectangle(cornerRadius: 5)
            .stroke(Color(nsColor: theme.brassLine), lineWidth: 1))
        .foregroundStyle(Color(nsColor: theme.brassInk))
    }

    private func pill(_ symbol: String, _ text: String,
                      dismiss: @escaping () -> Void) -> some View {
        HStack(spacing: 6) {
            label(symbol, text)
            Button(action: dismiss) {
                Image(systemName: "xmark.circle.fill").font(.app(10))
            }
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)
        }
    }
}

/// How often a place is asked, as the screen says it.
enum UpdateIntervals {
    /// As a choice of its own.
    static func choice(_ minutes: Int) -> String {
        switch minutes {
        case 60: L10n[.Update.intervalHour]
        case 1440: L10n[.Update.intervalDay]
        default: L10n[.Update.intervalSix]
        }
    }

    /// Inside a sentence.
    static func inSentence(_ minutes: Int) -> String {
        switch minutes {
        case 60: L10n[.Update.everyHour]
        case 1440: L10n[.Update.everyDay]
        default: L10n[.Update.everySix]
        }
    }
}

/// The アップデート section of 設定: where the app stands, where updates come from and how
/// often each is asked, whether a new version is fetched as soon as it is found, and a way to
/// ask now.
struct UpdateSettingsView: View {
    let updater: Updater

    /// The place being added or edited; nil when the form is shut.
    @State private var editing: Draft?
    @State private var error: String?

    private struct Draft: Equatable {
        var id: String?
        var name = ""
        var url = ""
        var repo = ""
        var token = ""
        var removeToken = false
        var hadToken = false
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                Text(L10n[.Update.settingsVersion(updater.label)]).font(.app(11.5, design: .monospaced))
                Spacer()
                if updater.busy {
                    ProgressView().controlSize(.small)
                }
                topButton
            }

            feeds

            Divider()
            autoDownload
            Divider()

            if case .failed = updater.phase {
                // What went wrong is what to act on, and was missed in the faint type: it says
                // so, with a mark.
                warning(status)
            } else {
                Text(status)
                    .font(.app(10.5))
                    .foregroundStyle(.tertiary)
                    .textSelection(.enabled)
                    .fixedSize(horizontal: false, vertical: true)
                if let problem = updater.lastProblem {
                    warning(L10n[.Update.statusLastProblem(problem)])
                }
            }
        }
    }

    /// What can be done now: fetch the version found, swap in the one fetched, or ask.
    @ViewBuilder
    private var topButton: some View {
        switch updater.phase {
        case .available(let release):
            Button(L10n[.Update.settingsDownload(release.version)]) { Task { await updater.download() } }
        case .ready:
            Button(L10n[.Update.settingsUpdateNow]) { Task { await updater.updateNow() } }
        default:
            Button(L10n[.Update.settingsCheckNow]) { Task { await updater.check() } }
                .disabled(updater.busy || !updater.installed)
        }
    }

    private func warning(_ text: String) -> some View {
        Label {
            Text(text)
                .font(.app(11))
                .foregroundStyle(.primary)
                .textSelection(.enabled)
                .fixedSize(horizontal: false, vertical: true)
        } icon: {
            Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(.orange)
        }
        .font(.app(11))
    }

    /// One switch for the whole app, not one per place: whether to spend the network and the disk on a
    /// version nobody asked for is the person's, wherever it comes from.
    private var autoDownload: some View {
        VStack(alignment: .leading, spacing: 3) {
            HStack(spacing: 8) {
                Toggle(isOn: Binding(get: { updater.autoDownload }, set: { updater.setAutoDownload($0) })) { EmptyView() }
                    .toggleStyle(.switch).controlSize(.mini).labelsHidden()
                Text(L10n[.Update.settingsAutoDownload]).font(.app(11.5))
            }
            Text(L10n[.Update.settingsAutoDownloadNote])
                .font(.app(10.5)).foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
        }
    }

    // MARK: - Where from

    private var feeds: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(L10n[.Update.settingsSources]).font(.app(11)).foregroundStyle(.secondary)
            ForEach(updater.feeds) { feed in
                feedRow(feed)
            }
            if let draft = editing {
                form(draft)
            } else {
                Button {
                    error = nil
                    editing = Draft()
                } label: {
                    Label(L10n[.Update.settingsAddSource], systemImage: "plus")
                }
                .buttonStyle(.borderless)
                .font(.app(11))
                .disabled(updater.busy)
            }
        }
    }

    private func feedRow(_ feed: UpdateFeed) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            feedLine(feed)
            periodicLine(feed)
        }
    }

    /// Under every place, not only the one in use: each keeps its own, which takes effect once it is
    /// chosen. The build's own is asked every six hours or not at all; an added one at a choice of
    /// intervals, kept while its switch is off.
    private func periodicLine(_ feed: UpdateFeed) -> some View {
        let periodic = updater.periodic(for: feed.id)
        return HStack(spacing: 8) {
            Toggle(isOn: Binding(get: { periodic.on },
                                 set: { updater.setPeriodic(.init(on: $0, minutes: periodic.minutes), for: feed.id) })) {
                EmptyView()
            }
            .toggleStyle(.switch).controlSize(.mini).labelsHidden()
            Text(feed.isBuiltin ? L10n[.Update.settingsPeriodicBuiltin] : L10n[.Update.settingsPeriodic])
                .font(.app(10.5))
            if !feed.isBuiltin {
                Picker("", selection: Binding(get: { periodic.minutes },
                                              set: { updater.setPeriodic(.init(on: periodic.on, minutes: $0), for: feed.id) })) {
                    ForEach(UpdateSettings.choices, id: \.self) { minutes in
                        Text(UpdateIntervals.choice(minutes)).tag(minutes)
                    }
                }
                .labelsHidden().fixedSize()
                .controlSize(.small).font(.app(10.5))
                .disabled(!periodic.on)
            }
        }
        // Under the place's name, past its radio button.
        .padding(.leading, 22)
    }

    private func feedLine(_ feed: UpdateFeed) -> some View {
        let chosen = feed.id == updater.selectedFeed?.id
        return HStack(spacing: 6) {
            Button {
                updater.selectFeed(feed.id)
            } label: {
                Image(systemName: chosen ? "largecircle.fill.circle" : "circle")
                    .foregroundStyle(chosen ? Color.accentColor : Color.secondary)
            }
            .buttonStyle(.plain)
            .disabled(updater.busy)
            .help(chosen ? L10n[.Update.settingsSourceChosenHelp] : L10n[.Update.settingsSourceChooseHelp])
            VStack(alignment: .leading, spacing: 1) {
                HStack(spacing: 4) {
                    Text(feed.displayName).font(.app(11.5, weight: chosen ? .semibold : .regular))
                    if updater.tokenIds.contains(feed.id) {
                        Image(systemName: "key.fill").font(.app(9)).foregroundStyle(.secondary)
                            .help(L10n[.Update.settingsTokenSavedHelp])
                    }
                }
                Text(feed.place)
                    .font(.app(10, design: .monospaced))
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
            }
            Spacer()
            if !feed.isBuiltin {
                Button(L10n[.Common.actionEdit]) {
                    error = nil
                    editing = Draft(id: feed.id, name: feed.name, url: feed.url.absoluteString,
                                    repo: feed.repo, hadToken: updater.tokenIds.contains(feed.id))
                }
                .buttonStyle(.borderless).font(.app(10.5))
                .disabled(updater.busy)
                Button(L10n[.Common.actionDeleteItem]) { updater.removeFeed(feed.id) }
                    .buttonStyle(.borderless).font(.app(10.5))
                    .disabled(updater.busy)
            }
        }
    }

    private func form(_ draft: Draft) -> some View {
        let binding = Binding<Draft>(get: { editing ?? draft }, set: { editing = $0 })
        return VStack(alignment: .leading, spacing: 6) {
            TextField(L10n[.Update.formName], text: binding.name)
            TextField(L10n[.Update.formUrl], text: binding.url)
            TextField(L10n[.Update.formRepo], text: binding.repo)
            SecureField(draft.hadToken ? L10n[.Update.formTokenKeep] : L10n[.Update.formTokenNew],
                        text: binding.token)
            if draft.hadToken {
                Toggle(L10n[.Update.formRemoveToken], isOn: binding.removeToken)
                    .font(.app(10.5))
            }
            if let error {
                Text(error).font(.app(10.5)).foregroundStyle(.red)
                    .fixedSize(horizontal: false, vertical: true)
            }
            HStack {
                Spacer()
                Button(L10n[.Common.actionCancel]) { editing = nil; error = nil }
                Button(draft.id == nil ? L10n[.Common.actionAddItem] : L10n[.Update.formSave]) { save() }
                    .keyboardShortcut(.defaultAction)
            }
        }
        .textFieldStyle(.roundedBorder)
        .font(.app(11))
        .padding(8)
        .background(RoundedRectangle(cornerRadius: 6).fill(Color.secondary.opacity(0.08)))
    }

    private func save() {
        guard let draft = editing else { return }
        do {
            let feed = try UpdateFeeds.make(id: draft.id ?? UUID().uuidString, name: draft.name,
                                            url: draft.url, repo: draft.repo)
            try updater.saveFeed(feed, token: draft.token.isEmpty ? nil : draft.token,
                                 removeToken: draft.removeToken)
            editing = nil
            error = nil
        } catch {
            self.error = ErrorText.said(error)
        }
    }

    private var status: String {
        let checked = updater.lastChecked.map {
            L10n[.Update.statusCheckedAt($0.formatted(Date.FormatStyle(date: .omitted, time: .shortened, locale: Language.shared.locale)))]
        } ?? ""
        let from = updater.selectedFeed?.displayName ?? L10n[.Update.feedFallbackName]
        switch updater.phase {
        case .idle:
            return updater.installed ? schedule : L10n[.Update.statusDevBuild]
        case .checking: return L10n[.Update.statusChecking]
        case .current: return L10n[.Update.statusJoined(L10n[.Update.statusUpToDate] + checked, schedule)]
        case .available(let release): return L10n[.Update.statusAvailable(from, release.version)] + checked
        case .downloading(let release): return L10n[.Update.statusDownloading(release.label)]
        case .ready(let release): return L10n[.Update.statusReady(release.label)] + checked
        case .rechecking: return L10n[.Update.statusRechecking]
        case .installing(let release): return L10n[.Update.statusInstalling(release.label)]
        case .failed(let message): return message + checked
        }
    }

    /// How often the place in use is asked without being told to.
    private var schedule: String {
        guard let feed = updater.selectedFeed else { return L10n[.Update.errorNoSource] }
        let periodic = updater.periodic(for: feed.id)
        return periodic.on
            ? L10n[.Update.statusEvery(feed.displayName, UpdateIntervals.inSentence(periodic.minutes))]
            : L10n[.Update.statusOff(feed.displayName)]
    }
}

/// アップデートを確認… in the app menu, where macOS apps keep it.
struct UpdateCommands: Commands {
    var body: some Commands {
        CommandGroup(after: .appInfo) {
            Button(L10n[.Update.menuCheckForUpdates]) {
                Task { await Updater.shared.checkAndReport() }
            }
        }
    }
}
