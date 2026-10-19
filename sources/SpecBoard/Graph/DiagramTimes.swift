import Foundation
import os

/// Time marks on what showing a section takes — the switch from the press to the diagram settled,
/// and on the way the canvas's show, rebuild, card layers and lines' ways, a document's text made and
/// its picture taken — as signposts (Instruments' Points of Interest), and kept for a test to read
/// while it asks for them (`record`).
nonisolated enum DiagramTimes {
    static let signposter = OSSignposter(subsystem: "com.m-osone.SpecBoard", category: .pointsOfInterest)

    /// One step and how long it took, in milliseconds.
    struct Step: Sendable {
        var name: String
        var milliseconds: Double
    }

    /// The steps since `record`, while a test reads them; nil otherwise, and nothing is kept.
    private static let kept = OSAllocatedUnfairLock<[Step]?>(initialState: nil)

    /// From now on, each step is kept until it is taken.
    static func record() { kept.withLock { $0 = [] } }

    /// The steps kept since they were last taken, and nothing kept from now on.
    static func stop() -> [Step] { kept.withLock { held in defer { held = nil }; return held ?? [] } }

    /// The steps kept since they were last taken.
    static func take() -> [Step] { kept.withLock { held in defer { if held != nil { held = [] } }; return held ?? [] } }

    /// `body`, marked as the step `name`.
    static func measure<T>(_ name: StaticString, _ body: () throws -> T) rethrows -> T {
        let state = signposter.beginInterval(name)
        let started = ContinuousClock.now
        defer {
            signposter.endInterval(name, state)
            note(name, since: started)
        }
        return try body()
    }

    /// A step that began at `started` and is over now, kept while a test reads them.
    static func note(_ name: StaticString, since started: ContinuousClock.Instant) {
        let took = ContinuousClock.now - started
        let milliseconds = Double(took.components.seconds) * 1000 + Double(took.components.attoseconds) / 1e15
        kept.withLock { $0?.append(Step(name: "\(name)", milliseconds: milliseconds)) }
    }

    /// A step over more than one turn of the main thread: begun here, ended by `end`.
    struct Interval: Sendable {
        fileprivate let name: StaticString
        fileprivate let state: OSSignpostIntervalState
        fileprivate let started: ContinuousClock.Instant
    }

    static func begin(_ name: StaticString) -> Interval {
        Interval(name: name, state: signposter.beginInterval(name), started: .now)
    }

    static func end(_ interval: Interval) {
        signposter.endInterval(interval.name, interval.state)
        note(interval.name, since: interval.started)
    }

    /// The section switch on its way: from the press that chose the section until the diagram has
    /// settled (`CardCanvasView.settle`).
    private static let switching = OSAllocatedUnfairLock<Interval?>(initialState: nil)

    static func beginSwitch() {
        let interval = begin("section switch")
        let before = switching.withLock { held in defer { held = interval }; return held }
        // One chosen before the last was settled: over as the next begins.
        if let before { end(before) }
    }

    static func endSwitch() {
        if let interval = switching.withLock({ held in defer { held = nil }; return held }) { end(interval) }
    }
}
