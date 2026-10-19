import Foundation

/// What a host looks like from here: is the connection up, what it is called, and is there a
/// Claude Code to use its agent's MCP.
///
/// Read-only, deliberately. Writing the registration into the host's config file as
/// well — find `~/.claude.json`, back it up, replace it atomically, show the diff
/// first — is a lot of machinery aimed at somebody else's file, and it only fits
/// one shape of host. The app starts the agent, and the agent says where it listens;
/// registering on the far side is `claude mcp add`, run there.
enum HostProbe {

    // MARK: - What a probe found

    struct Probe {
        var reachable = false
        var message = ""
        var host = ""
        var home = ""
        /// What was found, not whether a binary is on PATH: on some hosts there
        /// is no `claude` in PATH at all, and Claude Code runs
        /// out of `~/.claude/remote/ccd-cli/<version>`. Reporting "not installed"
        /// there was wrong in the way that matters — it said the connection could
        /// not be used on the host where it demonstrably works.
        var claudeRuntime: String?
        var at = Date()
    }

    // MARK: - Probing

    /// The cheap tier: is the shared connection alive. A local socket question, so
    /// it can run every second or two without touching the network.
    static func liveness(_ ssh: SSH) -> Bool { ssh.check().ok }

    /// The full tier. Rides the existing master, so there is no handshake.
    static func probe(_ ssh: SSH) -> Probe {
        var probe = Probe()

        let connected = ssh.connect()
        guard connected.ok || ssh.check().ok else {
            probe.message = connected.error.isEmpty
                ? L10n.text(.Places.errorCantConnect, in: Language.current) : connected.error
            return probe
        }
        probe.reachable = true

        let result = ssh.python(script)
        guard result.ok, let parsed = OJSON.parse(result.out),
              case .object(let root) = parsed else {
            probe.message = result.error.isEmpty
                ? L10n.text(.Places.errorProbeFailed, in: Language.current) : result.error
            return probe
        }

        if case .string(let host)? = root["host"] { probe.host = host }
        if case .string(let home)? = root["home"] { probe.home = home }
        probe.claudeRuntime = root["claude"]?.stringOrNil

        return probe
    }

    /// Read-only. Reports what is there; changes nothing.
    private static let script = """
        import json, os, glob, shutil

        def claude_runtime():
            # PATH first, then the place the remote runtime actually unpacks to.
            found = shutil.which("claude")
            if found:
                return "claude (%s)" % found
            versions = sorted(glob.glob(
                os.path.join(os.path.expanduser("~"), ".claude", "remote",
                             "ccd-cli", "*")))
            if versions:
                return "ccd-cli %s" % os.path.basename(versions[-1])
            return None

        home = os.path.expanduser("~")

        print(json.dumps({
            "host": os.uname().nodename,
            "home": home,
            "claude": claude_runtime(),
        }, ensure_ascii=False))
        """

}
