import Observation
import SwiftUI

/// How large the app's own text is drawn.
///
/// Every size in this app is written as a fixed number, because the density of a
/// field list is part of what makes a 163-schema project readable. That is a fine
/// default and a bad requirement: fixed means fixed for everyone, and someone who
/// needs larger text has no way in.
///
/// macOS has no global Dynamic Type dial to inherit from, so the app carries its
/// own multiplier. The numbers stay where they are, written as the design intends
/// them, and this scales them on the way to the screen.
///
/// The diagram is deliberately outside this: its cards are rasterised at a fixed
/// size and it has its own zoom, which is the better control there.
@Observable
final class Typography {
    /// Shared rather than passed down, because `Font.app(_:)` is called from inside
    /// view bodies that have no other reason to know about settings. Observation
    /// still registers the read, so changing the scale redraws everything.
    @MainActor static let shared = Typography()

    private static let key = "app.textScale"

    /// A multiplier on every size in the app's chrome.
    var scale: CGFloat {
        didSet {
            guard scale != oldValue else { return }
            UserDefaults.standard.set(Double(scale), forKey: Self.key)
        }
    }

    /// The steps offered. Free-form values are allowed — a stored setting from a
    /// later build with more steps still loads — but a short list is easier to pick
    /// from than a slider nobody can aim.
    static let steps: [CGFloat] = [1.0, 1.1, 1.25, 1.4, 1.6]

    init(scale: CGFloat? = nil) {
        if let scale {
            self.scale = scale
        } else {
            let stored = UserDefaults.standard.double(forKey: Self.key)
            // Clamped rather than trusted: this is read before anything is drawn,
            // and a corrupt value would make the window unusable in a way that is
            // hard to undo from inside it.
            self.scale = stored > 0 ? min(max(CGFloat(stored), 1.0), 2.0) : 1.0
        }
    }

    var label: String { "\(Int((scale * 100).rounded()))%" }

    // MARK: - Stepping, for the View menu

    var canEnlarge: Bool { scale < (Self.steps.last ?? 1) }
    var canReduce: Bool { scale > (Self.steps.first ?? 1) }

    /// The next step up, or the nearest one above a stored value that is not on the
    /// list — a setting written by a later build still moves in the right direction.
    func enlarge() {
        if let next = Self.steps.first(where: { $0 > scale }) { scale = next }
    }

    func reduce() {
        if let previous = Self.steps.last(where: { $0 < scale }) { scale = previous }
    }

    func reset() { scale = Self.steps.first ?? 1 }
}

extension Font {
    /// The app's own size, scaled. Reads exactly like `.system(size:weight:design:)`
    /// so the call sites did not have to change shape when this was introduced.
    @MainActor
    static func app(_ size: CGFloat, weight: Font.Weight = .regular,
                    design: Font.Design = .default) -> Font {
        .system(size: size * Typography.shared.scale, weight: weight, design: design)
    }
}
