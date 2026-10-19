import AppKit
import SwiftUI

/// A colour as `#RRGGBB`, and back: how a kind of section's mark says its colour in the definition
/// (`icon: {name, color}`). The colours of the nav panel's marks are the project's — the built-in
/// kinds' in `builtin.json`, written over by the project — and not a setting of this Mac's: what was
/// kept here (`navigator.tint.*`) is let go of (`forgetOldTints`).
enum HexColor {
    static func hex(_ color: Color) -> String? {
        guard let rgb = NSColor(color).usingColorSpace(.sRGB) else { return nil }
        let parts = [rgb.redComponent, rgb.greenComponent, rgb.blueComponent].map { Int(($0 * 255).rounded()) }
        return "#" + parts.map { String(format: "%02X", max(0, min(255, $0))) }.joined()
    }

    static func color(hex: String) -> Color? {
        let digits = hex.hasPrefix("#") ? String(hex.dropFirst()) : hex
        guard digits.count == 6, let value = Int(digits, radix: 16) else { return nil }
        return Color(.sRGB, red: Double((value >> 16) & 0xFF) / 255, green: Double((value >> 8) & 0xFF) / 255,
                     blue: Double(value & 0xFF) / 255)
    }

    /// The colours this Mac kept for the nav panel's marks before they were the project's
    /// (`navigator.tint.<kind>`), let go of: they are not carried into the projects.
    static func forgetOldTints(in defaults: UserDefaults = .standard) {
        for name in defaults.dictionaryRepresentation().keys where name.hasPrefix("navigator.tint.") {
            defaults.removeObject(forKey: name)
        }
    }
}
