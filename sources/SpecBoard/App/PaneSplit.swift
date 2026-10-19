import CoreGraphics

/// How wide the navigation pane on the left may be.
///
/// Kept apart from the view because the interesting part is the clamping, and the
/// clamping is what a stored width can violate: it may have been set before these
/// bounds, or by hand.
enum PaneSplit {
    /// The nav panel on the left, dragged to taste within these, and how wide it starts.
    static let navPanelRange: ClosedRange<CGFloat> = 220...640
    static let navPanelStart: Double = 300

    static func navPanelWidth(_ stored: Double) -> CGFloat {
        min(max(CGFloat(stored), navPanelRange.lowerBound), navPanelRange.upperBound)
    }

    /// The property panel on the right, dragged to taste within these, and how wide it starts.
    static let propertiesRange: ClosedRange<CGFloat> = 260...620
    static let propertiesStart: Double = 340

    /// What the middle needs at the least: every card editor's bar fits (`CardBar`).
    static let middleLeast: CGFloat = 384

    static func propertiesWidth(_ stored: Double) -> CGFloat {
        min(max(CGFloat(stored), propertiesRange.lowerBound), propertiesRange.upperBound)
    }

    /// Where the property panel is: not beside what is open (a root's dashboard, the settings), folded to
    /// its strip, or open.
    nonisolated enum PropertiesSeat { case absent, folded, open }

    /// What the two panels take of a window as it is now.
    nonisolated struct Widths: Equatable {
        var nav: CGFloat
        var properties: CGFloat
    }

    /// How much of the window both panels together may take, their strips apart: the rest is the middle's
    /// least.
    static func room(window: CGFloat, navShown: Bool, properties: PropertiesSeat) -> CGFloat {
        let strips = (navShown ? PanelStrip.width : 0) + (properties == .absent ? 0 : PanelStrip.width)
        return window - strips - middleLeast
    }

    /// The panels' widths laid out in a window `window` wide: the stored ones (dragged, kept) where the
    /// middle keeps its least, else both given up toward their own least in step with how far each is
    /// over it — the stored widths stay, so they come back when the window grows. A window narrower than
    /// that leaves the panels their least.
    static func widths(window: CGFloat, nav: Double, navShown: Bool,
                       properties: Double, seat: PropertiesSeat) -> Widths {
        let wantedNav = navPanelWidth(nav), wantedProperties = propertiesWidth(properties)
        let opens = seat == .open
        let over = (navShown ? wantedNav : 0) + (opens ? wantedProperties : 0)
            - room(window: window, navShown: navShown, properties: seat)
        let spareNav = navShown ? wantedNav - navPanelRange.lowerBound : 0
        let spareProperties = opens ? wantedProperties - propertiesRange.lowerBound : 0
        let spare = spareNav + spareProperties
        guard over > 0, spare > 0 else { return Widths(nav: wantedNav, properties: wantedProperties) }
        let cut = min(over, spare) / spare
        return Widths(nav: (wantedNav - spareNav * cut).rounded(.down),
                      properties: (wantedProperties - spareProperties * cut).rounded(.down))
    }
}
