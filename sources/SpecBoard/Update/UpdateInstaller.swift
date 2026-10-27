import Foundation
import Security

/// The parts of updating that touch the disk: unpacking a downloaded DMG, deciding
/// whether what came out of it may replace this app, and the swap itself.
///
/// The swap cannot happen from inside the app — a running bundle is not something to
/// pull out from under itself — so it is a short shell script that waits for this
/// process to exit, moves the new bundle into place and opens it. The version it
/// replaces is removed once the new one is in place: going back means installing the
/// earlier version's DMG from its release.
nonisolated enum UpdateInstaller {

    struct Failure: Error, CustomStringConvertible {
        let description: String
        init(_ description: String) { self.description = description }
    }

    static var workDir: URL {
        FileManager.default.homeDirectoryForCurrentUser
            .appending(path: "Library/Caches/SpecBoard-update")
    }

    /// Where earlier builds of the updater, and the install script the repository
    /// used to have, kept the version they replaced. Nothing puts anything there now;
    /// the next swap removes what is left.
    static var leftover: URL {
        FileManager.default.homeDirectoryForCurrentUser
            .appending(path: "Library/Caches/SpecBoard-install/previous")
    }

    /// What the swap script leaves for the next launch to read: `ok`, or `failed`
    /// and why.
    static var outcomeFile: URL { workDir.appending(path: "last-update") }

    // MARK: - Unpacking

    /// Mounts `dmg`, copies the app in it to `dir`, and checks the copy.
    ///
    /// The copy is what gets checked, not the image: it is the thing that will be
    /// moved into place, and nothing can change under it once the image is gone.
    static func stage(dmg: URL, into dir: URL, bundleId: String, build: Int?,
                      version: String? = nil, running: Running, team: String) throws -> URL {
        let fm = FileManager.default
        try fm.createDirectory(at: dir, withIntermediateDirectories: true)
        let mount = dir.deletingLastPathComponent()
            .appending(path: "mount-\(UUID().uuidString.prefix(8))")
        try fm.createDirectory(at: mount, withIntermediateDirectories: true)
        defer { try? fm.removeItem(at: mount) }

        let attached = run("/usr/bin/hdiutil", ["attach", "-nobrowse", "-readonly", "-noautoopen",
                                                "-mountpoint", mount.path, dmg.path])
        guard attached.status == 0 else {
            throw Failure(L10n.text(.Update.stageDmgOpen(attached.err.trimmed), in: Language.current))
        }
        defer {
            if run("/usr/bin/hdiutil", ["detach", mount.path]).status != 0 {
                _ = run("/usr/bin/hdiutil", ["detach", "-force", mount.path])
            }
        }

        let apps = (try? fm.contentsOfDirectory(at: mount, includingPropertiesForKeys: nil)) ?? []
        guard let source = apps.first(where: {
            $0.pathExtension == "app" && bundleIdentifier(of: $0) == bundleId
        }) else {
            throw Failure(L10n.text(.Update.stageNoApp(bundleId), in: Language.current))
        }

        let staged = dir.appending(path: source.lastPathComponent)
        try? fm.removeItem(at: staged)
        let copied = run("/usr/bin/ditto", [source.path, staged.path])
        guard copied.status == 0 else {
            throw Failure(L10n.text(.Update.stageExtract(copied.err.trimmed), in: Language.current))
        }
        do {
            try verify(app: staged, bundleId: bundleId, build: build, version: version, running: running,
                       team: team)
        } catch {
            try? fm.removeItem(at: staged)
            throw error
        }
        return staged
    }

    // MARK: - Checking

    /// The version and build of the app running: what a downloaded app has to be newer than.
    struct Running: Sendable, Equatable {
        let version: String
        let build: Int
    }

    /// Refuses anything that is not this app, at the version (and the build, for a tag
    /// with one) the release names, newer than the running copy, signed by the team that
    /// signed the running copy.
    ///
    /// The team is taken from the running app rather than written down here, so the
    /// check follows the certificate the app is actually built with. A release whose
    /// DMG was swapped on the server for something else fails here, not after it has
    /// been launched.
    static func verify(app: URL, bundleId: String, build: Int?, version: String? = nil,
                       running: Running, team: String) throws {
        let info = NSDictionary(contentsOf: app.appending(path: "Contents/Info.plist")) as? [String: Any]
        if let mismatch = mismatch(info ?? [:], bundleId: bundleId, build: build, version: version,
                                   running: running) {
            throw Failure(mismatch)
        }

        var code: SecStaticCode?
        guard SecStaticCodeCreateWithPath(app as CFURL, [], &code) == errSecSuccess, let code else {
            throw Failure(L10n.text(.Update.stageSignatureUnreadable, in: Language.current))
        }
        var requirement: SecRequirement?
        let text = "anchor apple generic and certificate leaf[subject.OU] = \"\(team)\""
        guard SecRequirementCreateWithString(text as CFString, [], &requirement) == errSecSuccess,
              let requirement else {
            throw Failure(L10n.text(.Update.stageRequirement(team), in: Language.current))
        }
        let flags = SecCSFlags(rawValue: kSecCSCheckAllArchitectures | kSecCSStrictValidate
                               | kSecCSCheckNestedCode)
        let status = SecStaticCodeCheckValidity(code, flags, requirement)
        guard status == errSecSuccess else {
            let reason = SecCopyErrorMessageString(status, nil) as String? ?? "\(status)"
            throw Failure(L10n.text(.Update.stageSignatureInvalid(team, reason), in: Language.current))
        }
    }

    /// What in the app's Info.plist disagrees with the release, or makes it no newer than
    /// the running app, or nil. The version is compared first and the build after it: a
    /// tag is only what the place updates come from says, and a build number alone would
    /// let it hand back an older version signed by the same team. Anything that cannot be
    /// read is refused.
    static func mismatch(_ info: [String: Any], bundleId: String, build: Int?,
                         version: String?, running: Running) -> String? {
        let language = Language.current
        guard info["CFBundleIdentifier"] as? String == bundleId else {
            return L10n.text(.Update.stageBundleIdDiffers(
                info["CFBundleIdentifier"] as? String ?? L10n.text(.Common.wordNone, in: language)),
                             in: language)
        }
        guard let version else { return L10n.text(.Update.stageVersionUnknown, in: language) }
        guard let foundVersion = info["CFBundleShortVersionString"] as? String,
              Release.isVersion(Substring(foundVersion)),
              let foundBuildText = info["CFBundleVersion"] as? String,
              let foundBuild = Int(foundBuildText) else {
            return L10n.text(.Update.stageContentsUnreadable, in: language)
        }
        guard foundVersion == version else {
            return L10n.text(.Update.stageVersionDiffers(version, foundVersion), in: language)
        }
        if let build, foundBuild != build {
            return L10n.text(.Update.stageBuildDiffers(build, foundBuildText), in: language)
        }
        let newer: Bool
        if Release.isVersion(Substring(running.version)) {
            switch Release.compareVersions(foundVersion, running.version) {
            case .orderedDescending: newer = true
            case .orderedSame: newer = foundBuild > running.build
            case .orderedAscending: newer = false
            }
        } else {
            newer = false
        }
        guard newer else {
            return L10n.text(.Update.stageNotNewer("\(foundVersion) (\(foundBuild))",
                                                   "\(running.version) (\(running.build))"), in: language)
        }
        return nil
    }

    /// The team that signed the running app, or nil when it is not signed by one.
    static func runningTeam() -> String? {
        var running: SecCode?
        guard SecCodeCopySelf([], &running) == errSecSuccess, let running else { return nil }
        var code: SecStaticCode?
        guard SecCodeCopyStaticCode(running, [], &code) == errSecSuccess, let code else { return nil }
        var info: CFDictionary?
        guard SecCodeCopySigningInformation(code, SecCSFlags(rawValue: kSecCSSigningInformation),
                                            &info) == errSecSuccess,
              let info = info as? [String: Any] else { return nil }
        return info[kSecCodeInfoTeamIdentifier as String] as? String
    }

    static func bundleIdentifier(of app: URL) -> String? {
        NSDictionary(contentsOf: app.appending(path: "Contents/Info.plist"))?["CFBundleIdentifier"]
            as? String
    }

    // MARK: - Swapping

    /// Starts the script that waits for `pid` to exit and puts `new` where `target`
    /// is. The caller quits straight after; the script outlives it.
    @discardableResult
    static func launchSwap(new: URL, target: URL, leftover: URL, outcome: URL, pid: Int32,
                           environment: [String: String] = [:]) throws -> Process {
        let fm = FileManager.default
        try fm.createDirectory(at: outcome.deletingLastPathComponent(),
                               withIntermediateDirectories: true)
        try? fm.removeItem(at: outcome)
        let script = outcome.deletingLastPathComponent().appending(path: "swap.sh")
        try swapScript.write(to: script, atomically: true, encoding: .utf8)

        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/bin/sh")
        // What a person reads of a failed swap is written in the screen's language when it starts: the
        // script only copies it into the result file.
        let language = Language.current
        process.arguments = [script.path, String(pid), new.path, target.path, leftover.path,
                             outcome.path,
                             L10n.text(.Update.swapTimeout, in: language),
                             L10n.text(.Update.swapCopyFailed(target.deletingLastPathComponent().path), in: language),
                             L10n.text(.Update.swapAsideFailed, in: language),
                             L10n.text(.Update.swapPlaceFailed, in: language)]
        process.environment = ProcessInfo.processInfo.environment
            .merging(environment) { _, new in new }
        process.standardInput = FileHandle.nullDevice
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        try process.run()
        return process
    }

    /// Every step that can fail leaves `target` a working app: the new bundle is
    /// copied beside it first, and the old one is only moved aside, never deleted,
    /// until the new one is in its place. Only then are the old one and `leftover`
    /// removed.
    static let swapScript = """
    #!/bin/sh
    # SpecBoard update: waits for the app to quit, swaps the bundles and opens the new one again.
    # Arguments: <pid to wait for> <new .app> <.app to replace> <place an earlier form left a copy>
    #            <result file> <messages for: the app not quitting, the copy, moving the old one aside,
    #            putting the new one in place>
    pid="$1"; new="$2"; target="$3"; leftover="$4"; result="$5"
    said_timeout="$6"; said_copy="$7"; said_aside="$8"; said_place="$9"

    finish() {
      printf '%s\\n' "$1" > "$result"
      if [ -z "$SPECBOARD_SWAP_NO_LAUNCH" ]; then
        /System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$target" >/dev/null 2>&1
        open "$target"
      fi
      exit 0
    }

    n=0
    while kill -0 "$pid" 2>/dev/null; do
      n=$((n + 1))
      if [ "$n" -gt 300 ]; then
        printf '%s\\n' "failed $said_timeout" > "$result"
        exit 1
      fi
      sleep 0.1
    done

    rm -rf "$target.new" "$target.old"
    if ! ditto "$new" "$target.new"; then
      rm -rf "$target.new"
      finish "failed $said_copy"
    fi
    if ! mv "$target" "$target.old"; then
      rm -rf "$target.new"
      finish "failed $said_aside"
    fi
    if ! mv "$target.new" "$target"; then
      mv "$target.old" "$target"
      finish "failed $said_place"
    fi

    rm -rf "$target.old" "$leftover" "$new"
    finish "ok"
    """

    // MARK: -

    struct Output { var status: Int32; var out: String; var err: String }

    /// Ten minutes at most: checking a disk image is the slowest of them.
    static func run(_ tool: String, _ arguments: [String]) -> Output {
        do {
            let ran = try Subprocess.run(tool, arguments, timeout: 600)
            guard !ran.timedOut else {
                return Output(status: -1, out: "", err: L10n.text(.Update.stageToolTimeout(tool), in: Language.current))
            }
            return Output(status: ran.status, out: String(decoding: ran.out, as: UTF8.self),
                          err: String(decoding: ran.err, as: UTF8.self))
        } catch {
            return Output(status: -1, out: "", err: ErrorText.said(error))
        }
    }
}

nonisolated private extension String {
    var trimmed: String { trimmingCharacters(in: .whitespacesAndNewlines) }
}
