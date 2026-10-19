import SwiftUI

/// The licenses of what the app and its core are built from, as their terms ask: each package's license
/// and copyright, and the license's text, where a person using the app can read them. A page of the
/// settings (ライセンス), the list on the left and what is chosen in it on the right
/// (`specboard://SpecBoard/Docs/Design:UI` §6).
struct ThirdPartyNoticesView: View {
    @State private var page: LicensePage?

    /// `page`: what the page starts from, the bundled list when none is given.
    init(page: LicensePage? = LicenseCatalog.bundled.map(LicensePage.init)) {
        _page = State(initialValue: page)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(L10n[.Docs.licensesNote])
                .font(.app(11)).foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            if let page {
                HStack(alignment: .top, spacing: 16) {
                    LicenseList(page: page).frame(width: 290)
                    LicenseDetail(page: page).frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
                }
            } else {
                Text(L10n[.Docs.licensesNotFound]).font(.app(12)).foregroundStyle(.secondary)
            }
        }
    }
}

// MARK: - The list

/// The search field, how many it found, and the packages by license: a header for each license that stays
/// at the top while its packages go by, and its packages. Only what is in view is built.
private struct LicenseList: View {
    @Bindable var page: LicensePage
    @FocusState private var focused: Bool

    var body: some View {
        VStack(spacing: 0) {
            VStack(alignment: .leading, spacing: 6) {
                searchField
                Text(summary).font(.app(10.5)).foregroundStyle(.secondary).padding(.leading, 4)
            }
            .padding(10)
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(spacing: 0, pinnedViews: [.sectionHeaders]) {
                        ForEach(page.groups) { group in
                            Section {
                                if !page.isFolded(group.license) {
                                    ForEach(group.packages) { package in row(package) }
                                }
                            } header: {
                                header(group)
                            }
                        }
                    }
                    .padding(.horizontal, 4).padding(.bottom, 6)
                }
                .onChange(of: page.selection) { _, selection in
                    guard let selection else { return }
                    proxy.scrollTo(Self.anchor(selection))
                }
            }
            // The arrow keys walk the list wherever the focus is in it, the field included.
            .focusable().focused($focused).focusEffectDisabled()
        }
        .onKeyPress(.upArrow) { page.move(-1); return .handled }
        .onKeyPress(.downArrow) { page.move(1); return .handled }
        .onKeyPress(.leftArrow) { page.fold(true); return .handled }
        .onKeyPress(.rightArrow) { page.fold(false); return .handled }
        .background(Color(nsColor: .controlBackgroundColor).opacity(0.6))
        .clipShape(RoundedRectangle(cornerRadius: 8))
        .overlay(RoundedRectangle(cornerRadius: 8).stroke(Color.secondary.opacity(0.25)))
    }

    private static func anchor(_ selection: LicensePage.Selection) -> String {
        switch selection {
        case .package(let id): id
        case .group(let license): "license/" + license
        }
    }

    private var summary: String {
        if page.isSearching { return L10n[.Docs.licensesMatches(page.packageCount)] }
        return L10n[.Docs.licensesSummary(L10n[.Docs.licensesPackages(page.packageCount)],
                                          L10n[.Docs.licensesLicenses(page.groups.count)])]
    }

    private var searchField: some View {
        HStack(spacing: 6) {
            Image(systemName: "magnifyingglass").foregroundStyle(.secondary).font(.app(11))
            TextField(L10n[.Docs.licensesSearch], text: $page.query)
                .textFieldStyle(.plain).font(.app(12))
                .onKeyPress(.upArrow) { page.move(-1); return .handled }
                .onKeyPress(.downArrow) { page.move(1); return .handled }
            if page.isSearching {
                Button { page.query = "" } label: {
                    Image(systemName: "xmark.circle.fill").foregroundStyle(.secondary).font(.app(11))
                }
                .buttonStyle(.plain)
            }
        }
        .padding(.horizontal, 8).padding(.vertical, 5)
        .background(RoundedRectangle(cornerRadius: 6).fill(Color(nsColor: .textBackgroundColor)))
        .overlay(RoundedRectangle(cornerRadius: 6).stroke(Color.secondary.opacity(0.35)))
    }

    private func header(_ group: LicenseGroup) -> some View {
        let chosen = page.selection == .group(group.license)
        return HStack(spacing: 6) {
            Button { page.toggle(group.license) } label: {
                Image(systemName: "chevron.down").font(.app(9, weight: .semibold))
                    .rotationEffect(.degrees(page.isFolded(group.license) ? -90 : 0))
                    .frame(width: 16, height: 18)
            }
            .buttonStyle(.plain)
            Text(group.license).font(.app(12, weight: .semibold)).lineLimit(1)
            Spacer(minLength: 6)
            Text("\(group.packages.count)").font(.app(11)).monospacedDigit()
        }
        .foregroundStyle(chosen ? Color.white : Color.primary)
        .padding(.horizontal, 6).padding(.vertical, 5)
        .background(RoundedRectangle(cornerRadius: 5).fill(chosen ? Color(nsColor: .selectedContentBackgroundColor) : Color.clear))
        .background(Color(nsColor: .controlBackgroundColor))
        .contentShape(Rectangle())
        .onTapGesture { page.select(.group(group.license)); focused = true }
        .id("license/" + group.license)
    }

    private func row(_ package: LicensePackage) -> some View {
        let chosen = page.selection == .package(package.id)
        return HStack(spacing: 5) {
            Text(Self.named(package.name, matching: page.query, plain: chosen)).font(.app(12)).lineLimit(1)
            Text(package.version).font(.app(10.5)).opacity(0.6).lineLimit(1)
            Spacer(minLength: 6)
            Text(package.source.tag).font(.app(10)).opacity(0.6)
        }
        .foregroundStyle(chosen ? Color.white : Color.primary)
        .padding(.leading, 28).padding(.trailing, 8).padding(.vertical, 3)
        .background(RoundedRectangle(cornerRadius: 5).fill(chosen ? Color(nsColor: .selectedContentBackgroundColor) : Color.clear))
        .contentShape(Rectangle())
        .onTapGesture { page.select(.package(package.id)); focused = true }
        .id(package.id)
    }

    /// The name with what the search matched in the accent colour.
    static func named(_ name: String, matching query: String, plain: Bool) -> AttributedString {
        var text = AttributedString(name)
        let query = query.trimmingCharacters(in: .whitespaces)
        if !plain, !query.isEmpty, let found = text.range(of: query, options: .caseInsensitive) {
            text[found].foregroundColor = .accentColor
        }
        return text
    }
}

// MARK: - What is chosen

/// A package's license and copyright and its text, or a license's packages and its text.
private struct LicenseDetail: View {
    var page: LicensePage

    var body: some View {
        if let package = page.selectedPackage {
            PackageDetail(page: page, package: package)
        } else if let group = page.selectedGroup {
            GroupDetail(page: page, group: group)
        } else {
            Text(L10n[.Docs.licensesEmpty]).font(.app(12)).foregroundStyle(.secondary).padding(.top, 8)
        }
    }
}

private struct Chip: View {
    var text: String
    var accent = false

    var body: some View {
        Text(text)
            .font(.app(11, weight: .medium))
            .foregroundStyle(accent ? Color.accentColor : Color.primary)
            .padding(.horizontal, 9).padding(.vertical, 3)
            .background(Capsule().fill(accent ? Color.accentColor.opacity(0.18) : Color.secondary.opacity(0.18)))
    }
}

private struct PackageDetail: View {
    var page: LicensePage
    var package: LicensePackage

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Text(package.name).font(.app(20, weight: .semibold)).textSelection(.enabled)
                Text(package.version).font(.app(13)).foregroundStyle(.secondary).textSelection(.enabled)
            }
            HStack(spacing: 6) {
                Chip(text: package.source.chip)
                Chip(text: package.license, accent: true)
            }
            SwiftUI.Grid(alignment: .topLeading, horizontalSpacing: 16, verticalSpacing: 7) {
                infoRow(L10n[.Docs.licensesRowSource]) { Text(package.source.provenance) }
                infoRow(L10n[.Docs.licensesRowHomepage]) {
                    if let url = package.homepageURL {
                        Link(package.homepage, destination: url)
                    } else {
                        Text(package.homepage).textSelection(.enabled)
                    }
                }
                infoRow(L10n[.Docs.licensesRowCopyright]) { copyright }
                infoRow(L10n[.Docs.licensesRowLicense]) {
                    (Text(package.license).fontWeight(.medium) + choice).textSelection(.enabled)
                }
            }
            .font(.app(12))
            LicenseTextBox(page: page)
        }
    }

    private var choice: Text {
        switch package.choices {
        case 1: Text("")
        case 2: Text(" — " + L10n[.Docs.licensesEither]).foregroundStyle(.secondary).font(.app(11))
        default: Text(" — " + L10n[.Docs.licensesAnyOf]).foregroundStyle(.secondary).font(.app(11))
        }
    }

    @ViewBuilder
    private var copyright: some View {
        VStack(alignment: .leading, spacing: 2) {
            if package.copyright.isEmpty {
                Text(L10n[.Docs.licensesCopyrightNone]).foregroundStyle(.secondary)
            } else {
                ForEach(package.copyright, id: \.self) { Text($0).textSelection(.enabled) }
                if package.copyrightIsFromAuthors {
                    Text(L10n[.Docs.licensesCopyrightFromAuthors]).font(.app(10.5)).foregroundStyle(.secondary)
                }
            }
        }
    }

    private func infoRow<Value: View>(_ label: String, @ViewBuilder _ value: () -> Value) -> some View {
        SwiftUI.GridRow(alignment: .firstTextBaseline) {
            Text(label).foregroundStyle(.secondary).gridColumnAlignment(.leading)
            value().frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}

private struct GroupDetail: View {
    var page: LicensePage
    var group: LicenseGroup

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text(group.license).font(.app(20, weight: .semibold)).textSelection(.enabled)
            HStack(spacing: 6) {
                Chip(text: L10n[.Docs.licensesUsedBy(group.packages.count)], accent: true)
                Chip(text: group.sources.map { $0.source.count($0.count) }.joined(separator: ", "))
            }
            sectionTitle(L10n[.Docs.licensesTableTitle])
            table
            LicenseTextBox(page: page, note: L10n[.Docs.licensesDiffers])
        }
    }

    /// Each package with where it comes from and its own copyright line: the text below has none.
    private var table: some View {
        VStack(spacing: 0) {
            HStack(spacing: 10) {
                Text(L10n[.Docs.licensesTablePackage]).frame(width: 230, alignment: .leading)
                Text(L10n[.Docs.licensesRowSource]).frame(width: 56, alignment: .leading)
                Text(L10n[.Docs.licensesRowCopyright]).frame(maxWidth: .infinity, alignment: .leading)
            }
            .font(.app(10, weight: .semibold)).textCase(.uppercase).foregroundStyle(.secondary)
            .padding(.horizontal, 10).padding(.vertical, 5)
            Divider()
            ScrollView {
                LazyVStack(spacing: 0) {
                    ForEach(group.packages) { package in
                        HStack(spacing: 10) {
                            HStack(spacing: 5) {
                                Text(package.name).font(.app(12, weight: .medium)).lineLimit(1)
                                Text(package.version).font(.app(10.5)).foregroundStyle(.secondary).lineLimit(1)
                            }
                            .frame(width: 230, alignment: .leading)
                            Text(package.source.tag).foregroundStyle(.secondary).frame(width: 56, alignment: .leading)
                            Text(package.copyright.first ?? L10n[.Docs.licensesCopyrightNone])
                                .lineLimit(1).frame(maxWidth: .infinity, alignment: .leading)
                        }
                        .font(.app(11.5))
                        .padding(.horizontal, 10).padding(.vertical, 3)
                        .textSelection(.enabled)
                        Divider().opacity(0.5)
                    }
                }
            }
        }
        .frame(height: min(CGFloat(group.packages.count) * 25 + 30, 270))
        .background(Color(nsColor: .textBackgroundColor))
        .clipShape(RoundedRectangle(cornerRadius: 6))
        .overlay(RoundedRectangle(cornerRadius: 6).stroke(Color.secondary.opacity(0.25)))
    }
}

private func sectionTitle(_ text: String) -> some View {
    Text(text).font(.app(10.5, weight: .semibold)).tracking(1.2).textCase(.uppercase).foregroundStyle(.secondary)
}

/// The license text in a box of its own, scrolling, with a switch between the texts when there is more than
/// one to read (`MIT OR Apache-2.0`: either). `note` stands beside the title.
private struct LicenseTextBox: View {
    var page: LicensePage
    var note: String?

    var body: some View {
        let texts = page.shownTexts
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .firstTextBaseline, spacing: 12) {
                sectionTitle(L10n[.Docs.licensesTextTitle])
                if let note {
                    Text(note).font(.app(10.5)).foregroundStyle(.secondary).lineLimit(2).frame(maxWidth: .infinity, alignment: .trailing)
                } else {
                    Spacer(minLength: 0)
                }
                if texts.count > 1 {
                    Picker("", selection: Binding(get: { min(page.textIndex, texts.count - 1) }, set: { page.textIndex = $0 })) {
                        ForEach(texts.indices, id: \.self) { Text(texts[$0].license).tag($0) }
                    }
                    .pickerStyle(.segmented).labelsHidden().fixedSize()
                }
            }
            ScrollView {
                Text(page.shownText?.body ?? "")
                    .font(.system(size: 10.5, design: .monospaced))
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(10)
            }
            .frame(minHeight: 120, maxHeight: .infinity)
            .background(Color(nsColor: .textBackgroundColor))
            .clipShape(RoundedRectangle(cornerRadius: 6))
            .overlay(RoundedRectangle(cornerRadius: 6).stroke(Color.secondary.opacity(0.25)))
            .id(page.selection)
        }
    }
}
