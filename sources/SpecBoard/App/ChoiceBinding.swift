import SwiftUI

extension Binding where Value: Equatable {
    /// A picker's choice that does something when chosen: a segmented control sets the side chosen
    /// already again when it is pressed again, and that is let go of — what was being done is not
    /// started over (a draft read again from what is kept, a value set back to its default).
    static func choice(_ get: @escaping () -> Value, _ set: @escaping (Value) -> Void) -> Binding<Value> {
        Binding(get: get, set: { chosen in
            guard chosen != get() else { return }
            set(chosen)
        })
    }
}
