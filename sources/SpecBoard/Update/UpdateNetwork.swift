import Foundation

/// What updating asks of the network: the latest release of a place, and a release's DMG fetched,
/// unpacked and checked. Apart from `Updater` so that what it decides can be tried without a server.
@MainActor
protocol UpdateNetwork {
    func latest(from source: UpdateSource) async throws -> Release
    /// The checked copy of the app in `release`, ready to swap in; it replaces any copy staged before.
    func stage(_ release: Release, from source: UpdateSource, running: UpdateInstaller.Running) async throws -> URL
}

/// The real one: the place's API over https, and the DMG unpacked and checked on the disk.
struct LiveUpdateNetwork: UpdateNetwork {

    func latest(from source: UpdateSource) async throws -> Release {
        let (data, response) = try await URLSession.shared.data(for: source.request(source.latestURL))
        let http = response as? HTTPURLResponse
        let status = http?.statusCode ?? 0
        guard status == 200 else {
            throw source.failure(status: status,
                                 rateLimitRemaining: http?.value(forHTTPHeaderField: "x-ratelimit-remaining"))
        }
        return try Release.parse(data)
    }

    func stage(_ release: Release, from source: UpdateSource, running: UpdateInstaller.Running) async throws -> URL {
        let (download, response) = try await URLSession.shared.download(
            for: source.download(release.asset.url))
        let http = response as? HTTPURLResponse
        guard http?.statusCode == 200,
              http?.value(forHTTPHeaderField: "Content-Type")?.hasPrefix("text/html") != true else {
            throw UpdateInstaller.Failure(L10n[.Update.errorDownloadFailed(release.asset.name, http?.statusCode ?? 0)])
        }

        let fm = FileManager.default
        let dir = UpdateInstaller.workDir
        try fm.createDirectory(at: dir, withIntermediateDirectories: true)
        // Not the asset's own name: that comes from the server, and a name with
        // `../` in it would put the file — and the removal before it — anywhere.
        let dmg = dir.appending(path: "update.dmg")
        try? fm.removeItem(at: dmg)
        try fm.moveItem(at: download, to: dmg)

        guard let team = UpdateInstaller.runningTeam() else {
            throw UpdateInstaller.Failure(L10n[.Update.errorNoTeam])
        }
        let bundleId = Bundle.main.bundleIdentifier ?? ""
        let build = release.build
        let version = release.version
        return try await Task.detached {
            defer { try? FileManager.default.removeItem(at: dmg) }
            return try UpdateInstaller.stage(dmg: dmg, into: dir.appending(path: "staged"),
                                             bundleId: bundleId, build: build, version: version,
                                             running: running, team: team)
        }.value
    }
}
