import SwiftUI

/// The project's settings on screen (`specboard://SpecBoard/Docs/Design:Overview` §13): what is the project's
/// alone and not a branch's — the project itself (its description and how big its cards
/// are), its rules for each place (`RulesView`), its kinds of card, section and line and its tree
/// (`KindsView`), where it is written out (`ExportView`: one setting of the project, whatever tree is
/// open), and what MCP may write in it (`MCPWritesView`). Opened from the head of the nav panel, above
/// the branches.
struct ProjectSettingsView: View {
    var model: AppModel
    var tab: AppModel.SettingsTab

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text(L10n[.ProjectSettings.settingsTitle]).font(.app(13, weight: .semibold))
                Picker("", selection: Binding(get: { tab }, set: { model.showSettings($0) })) {
                    ForEach(AppModel.SettingsTab.allCases, id: \.self) { Text($0.title).tag($0) }
                }
                .labelsHidden()
                .pickerStyle(.segmented)
                .fixedSize()
                Spacer()
            }
            .padding(.horizontal, 20)
            .padding(.vertical, 8)
            Divider()
            switch tab {
            case .project: ProjectFacts(model: model)
            case .rules: RulesView(model: model)
            case .kinds: KindsView(model: model)
            case .exports:
                ScrollView {
                    ExportView(model: model)
                        .padding(20)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            case .mcp: MCPWritesView(model: model)
            }
        }
    }
}

/// The project itself among its settings: its description, read first by a person and by
/// MCP (`project_read`), how big its cards are, and main's lock. Its name is changed in the
/// projects list.
private struct ProjectFacts: View {
    var model: AppModel

    var body: some View {
        if let project = model.detail?.project {
            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    VStack(alignment: .leading, spacing: 6) {
                        Text(project.name).font(.app(15, weight: .semibold))
                        Text(L10n[.ProjectSettings.factsNote])
                            .font(.app(11)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                            .readingWidth()
                    }
                    VStack(alignment: .leading, spacing: 6) {
                        Text(L10n[.Common.wordDescription]).font(.app(10, weight: .semibold)).tracking(1.1).foregroundStyle(.secondary)
                        CommitTextEditor(text: project.description, minHeight: 100) {
                            model.updateProjectDescription(project.id, $0)
                        }
                        .disabled(model.isArchived)
                        if !model.writingRule("project.description").isEmpty {
                            Text(model.writingRule("project.description"))
                                .font(.app(10.5)).foregroundStyle(.tertiary).fixedSize(horizontal: false, vertical: true)
                                .readingWidth()
                        }
                    }
                    CardSizeSettings(model: model)
                        .disabled(model.isArchived)
                    ExportLanguageSetting(model: model, project: project)
                        .disabled(model.isArchived)
                    MainLockSetting(model: model)
                }
                .padding(20)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
    }
}

/// The language the project's exports say their own words in, among its settings: each language by its
/// own name, no `system` (the files must be the same on every machine).
private struct ExportLanguageSetting: View {
    var model: AppModel
    var project: Project

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(L10n[.ProjectSettings.exportLanguageTitle]).font(.app(10, weight: .semibold)).tracking(1.1)
                .foregroundStyle(.secondary)
            Picker("", selection: Binding(get: { project.exportedIn },
                                          set: { model.updateExportLanguage(project.id, $0) })) {
                ForEach(Language.supported, id: \.self) { Text(Language.names[$0] ?? $0).tag($0) }
            }
            .labelsHidden()
            .pickerStyle(.segmented)
            .fixedSize()
            Text(L10n[.ProjectSettings.exportLanguageHelp])
                .font(.app(10.5)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                .readingWidth()
        }
    }
}

/// main's lock among the project's settings: a switch, with what a lock does said under it
/// (`specboard://SpecBoard/Docs/Design:DesignBranches` §8). Let go only after asking, as main's right-click in
/// the nav panel asks (`MainLock`); not while the project is archived.
private struct MainLockSetting: View {
    var model: AppModel
    @State private var unlocking = false

    static var explanation: String { L10n[.ProjectSettings.lockExplanation] }

    static var aiMergeLabel: String { L10n[.ProjectSettings.lockAiMergeLabel] }
    static var aiMergeExplanation: String { L10n[.ProjectSettings.lockAiMergeExplanation] }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(L10n[.ProjectSettings.lockTitle]).font(.app(10, weight: .semibold)).tracking(1.1).foregroundStyle(.secondary)
            Toggle(isOn: Binding(get: { model.isMainLocked }, set: { on in
                if on { model.setMainLock(true) } else { unlocking = true }
            })) {
                Text(L10n[.ProjectSettings.lockToggle]).font(.app(11.5))
            }
            .toggleStyle(.switch)
            .disabled(model.isArchived)
            .help(MainLock.lockHelp)
            Text(Self.explanation)
                .font(.app(10.5)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                .readingWidth()
            // An AI's merges let in without being approved, while main is locked.
            Toggle(isOn: Binding(get: { model.isAIMergeOn }, set: { model.setAIMerge($0) })) {
                Text(Self.aiMergeLabel).font(.app(11.5))
            }
            .toggleStyle(.switch)
            .disabled(model.isArchived || !model.isMainLocked)
            .padding(.top, 6)
            Text(Self.aiMergeExplanation)
                .font(.app(10.5)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                .readingWidth()
        }
        .mainUnlockConfirmation(model, isPresented: $unlocking)
    }
}

/// How big the cards are, for the whole project: in the graph bar's popover and in the
/// project's settings. Each change is made at once, and shows on the diagram.
struct CardSizeSettings: View {
    var model: AppModel

    var body: some View {
        let settings = model.defaultSettings
        func apply(_ change: (inout LayoutSettings) -> Void) {
            var next = settings
            change(&next)
            do {
                try model.setDefaultSettings(next)
                model.lastError = nil
            } catch {
                model.lastError = error.localizedDescription
            }
        }
        return VStack(alignment: .leading, spacing: 12) {
            Text(L10n[.ProjectSettings.sizeTitle]).font(.app(12, weight: .semibold))
            VStack(alignment: .leading, spacing: 4) {
                Stepper(value: Binding(get: { settings.size.rows }, set: { value in apply { $0.size.rows = value } }),
                        in: CardSize.rowsRange) {
                    Text(L10n[.ProjectSettings.sizeRows(settings.size.rows)]).font(.app(11.5))
                }
                .help(L10n[.ProjectSettings.sizeRowsHelp])
                HStack {
                    Picker("", selection: Binding(get: { settings.size.width == nil }, set: { auto in
                        apply { $0.size.width = auto ? nil : 352 }
                    })) {
                        Text(L10n[.ProjectSettings.sizeWidthAuto]).tag(true)
                        Text(L10n[.ProjectSettings.sizeWidthSet]).tag(false)
                    }
                    .labelsHidden().pickerStyle(.segmented).fixedSize()
                    if let width = settings.size.width {
                        Stepper(value: Binding(get: { width }, set: { value in apply { $0.size.width = value } }),
                                in: CardSize.widthRange, step: 20) {
                            Text("\(width) px").font(.app(11.5, design: .monospaced))
                        }
                    }
                }
                Text(settings.size.width == nil
                     ? L10n[.ProjectSettings.sizeAutoNote(Int(model.cardWidth))]
                     : L10n[.ProjectSettings.sizeFixedNote])
                    .font(.app(10.5)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                    .readingWidth()
            }
            Stepper(value: Binding(get: { settings.margin }, set: { value in apply { $0.margin = value } }),
                    in: LayoutSettings.marginRange) {
                Text(L10n[.ProjectSettings.sizeMargin(settings.margin)]).font(.app(11.5))
            }
            .help(L10n[.ProjectSettings.sizeMarginHelp])
            // How a formatdoc that does not say reads on the diagram.
            HStack(spacing: 8) {
                Text(L10n[.ProjectSettings.sizeDocCards]).font(.app(11.5))
                Picker("", selection: Binding(get: { settings.docView }, set: { value in apply { $0.docView = value } })) {
                    ForEach(SchemaNode.docViews, id: \.self) { Text(CardDetails.viewWord($0)).tag($0) }
                }
                .labelsHidden().pickerStyle(.segmented).fixedSize()
            }
            .help(L10n[.ProjectSettings.sizeDocHelp])
            Text(L10n[.ProjectSettings.sizeResizeNote])
                .font(.app(10.5)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                .readingWidth()
            Text(L10n[.ProjectSettings.sizeGridNote])
                .font(.app(10.5)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                .readingWidth()
        }
    }
}
