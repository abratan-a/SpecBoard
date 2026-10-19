import SwiftUI

/// The project's rules on screen, in its settings (`specboard://SpecBoard/Docs/Design:Overview` §5「書く場所」):
/// for each place free text goes, what to write there — the project's own rule, or,
/// faintly, the one it falls back to (a broader place's, or the app's). A person's to
/// change, under main's lock too; MCP reads it.
struct RulesView: View {
    var model: AppModel

    private static var groups: [(title: String, places: [String])] {
        [
            (L10n[.ProjectSettings.rulesGroupProject], ["project.description", "section.description"]),
            (L10n[.ProjectSettings.rulesGroupCards], ["card.description"] + SchemaKind.made.map { WritingRules.cardPlace($0) }),
            (L10n[.ProjectSettings.rulesGroupFields], ["field.description", "field.entity.description", "field.value.description"]),
            (L10n[.ProjectSettings.rulesGroupRows], ["step.description", "node.description"]),
            (L10n[.ProjectSettings.rulesGroupNotes], ["note.body", "document.body"]),
        ]
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                VStack(alignment: .leading, spacing: 6) {
                    Text(L10n[.ProjectSettings.tabRules]).font(.app(15, weight: .semibold))
                    Text(L10n[.ProjectSettings.rulesIntro])
                        .font(.app(11)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                        .readingWidth()
                }
                ForEach(Self.groups, id: \.title) { group in
                    VStack(alignment: .leading, spacing: 10) {
                        Text(group.title).font(.app(10, weight: .semibold)).tracking(1.1).foregroundStyle(.secondary)
                        ForEach(group.places, id: \.self) { place in placeRow(place) }
                    }
                }
            }
            .padding(20)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    @ViewBuilder
    private func placeRow(_ place: String) -> some View {
        let own = model.writingRules[place] ?? ""
        var others = model.writingRules
        let _ = others.removeValue(forKey: place)
        let fallback = WritingRules.rule(place, project: others)
        VStack(alignment: .leading, spacing: 4) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Text(WritingRules.byName[place]?.label ?? place).font(.app(12, weight: .medium))
                Text(place).font(.app(10, design: .monospaced)).foregroundStyle(.tertiary)
                Spacer()
                if own.isEmpty, let fallback {
                    Text(Self.source(fallback.source)).font(.app(10)).foregroundStyle(.tertiary)
                }
            }
            if model.canEditExports {
                CommitTextEditor(text: own, minHeight: 40, placeholder: own.isEmpty ? fallback?.text ?? "" : "") { text in
                    model.setWritingRule(place, text)
                }
                .id("rule-\(place)-\(model.detail?.project.id ?? "")")
            } else {
                Text(own.isEmpty ? fallback?.text ?? "" : own)
                    .font(.app(11.5)).foregroundStyle(own.isEmpty ? .tertiary : .primary)
                    .fixedSize(horizontal: false, vertical: true)
                    .readingWidth()
            }
        }
    }

    private static func source(_ source: WritingRules.Source) -> String {
        switch source {
        case .project(let at): L10n[.ProjectSettings.rulesSourceProject(at)]
        case .app: L10n[.ProjectSettings.rulesSourceApp]
        }
    }
}
