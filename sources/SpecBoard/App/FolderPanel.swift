import AppKit

/// A folder on this Mac chosen in the system's panel: where an export is written, where a working directory
/// is read from.
enum FolderPanel {
    /// The folder chosen, written the way the core takes it (`~` for the home folder), or nil when none was.
    static func choose(message: String) -> String? {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.canCreateDirectories = true
        panel.message = message
        guard panel.runModal() == .OK, let url = panel.url else { return nil }
        return written(url.path)
    }

    /// A path as the core takes it: under the home folder, from `~`.
    static func written(_ path: String) -> String {
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        return path.hasPrefix(home + "/") ? "~" + path.dropFirst(home.count) : path
    }
}
