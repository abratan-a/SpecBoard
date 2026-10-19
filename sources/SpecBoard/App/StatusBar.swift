import AppKit
import SwiftUI

/// The window's content over the status bar across the bottom, under the nav panel too: put together the
/// same for the window and for its pictures, so that a picture shows the window as it is.
struct WindowBody<Content: View>: View {
    var model: AppModel
    var hosts: RemoteHosts
    var updater: Updater
    @ViewBuilder var content: Content

    var body: some View {
        VStack(spacing: 0) {
            content.frame(minHeight: 0, maxHeight: .infinity)
            StatusBar(model: model, hosts: hosts, updater: updater)
        }
    }
}

/// The band across the bottom of the window, one line. On the left, what the window does: a spinning mark
/// and 「処理中...」 while it writes or reads, for a few seconds what it did by itself and a person may want
/// to know of (`AppModel.activity`, `AppModel.say`), else 「-」. On the right, how many agents the app is
/// connected to, with whether they answer MCP (`MCPLight`), and the app's name and version: each in a box
/// as wide as its widest saying, so the dividers stay put as the count and the version change.
struct StatusBar: View {
    var model: AppModel
    var hosts: RemoteHosts
    var updater: Updater

    var body: some View {
        StatusBand(activity: model.activity, said: model.said,
                   light: MCPLight.of(hosts: hosts.hosts, cores: hosts.coreState), version: updater.version) {
            model.statusLine = $0
        }
    }
}

/// The status bar as it is drawn, from what it says.
struct StatusBand: View {
    var activity: AppModel.Activity?
    var said: String?
    /// Whether the agents answer MCP.
    var light: MCPLight
    /// The app's version.
    var version: String
    /// Handed the line once it is made: the window draws it at once when its main thread is about to be
    /// busy (`StatusLineView.drawNow`).
    var hold: ((StatusLineView) -> Void)? = nil

    var body: some View {
        let scale = Typography.shared.scale
        VStack(spacing: 0) {
            Divider()
            HStack(spacing: 0) {
                StatusLine(activity: activity, said: said, scale: scale, hold: hold)
                    .frame(maxWidth: .infinity)
                divider
                agents.padding(.horizontal, 10)
                divider
                Self.widest(["SpecBoard 0.99.99"]) {
                    Text("SpecBoard \(version)").font(.app(StatusLineView.textSize)).foregroundStyle(.secondary)
                }
                .padding(.horizontal, 10)
            }
            .frame(maxWidth: .infinity)
            .frame(height: Self.height(scale))
        }
    }

    /// How many agents are connected, after a dot red while an agent's own cannot answer MCP, green while
    /// one does, grey while none does; what each says on the pointer. Only said: nothing to press.
    private var agents: some View {
        Self.widest(["No Agents Connected", "99 Agents Connected"]) {
            Text(light.agents)
        }
        .padding(.leading, 12)
        .overlay(alignment: .leading) {
            Circle().fill(light.color).frame(width: 7, height: 7)
        }
        .contentShape(Rectangle())
        .help(light.help)
    }

    private var divider: some View {
        Divider().padding(.vertical, 4)
    }

    /// `content` in a box as wide as the widest of `sayings` would be, from its leading edge.
    private static func widest<Content: View>(_ sayings: [String], @ViewBuilder _ content: () -> Content) -> some View {
        ZStack(alignment: .leading) {
            ForEach(sayings, id: \.self) { Text($0).hidden() }
            content()
        }
        .font(.app(StatusLineView.textSize))
        .lineLimit(1)
        .fixedSize()
    }

    /// As high as a line of its text and a little more.
    static func height(_ scale: CGFloat) -> CGFloat { (StatusLineView.textSize * scale).rounded() + 9 }
}

/// The status bar's line, AppKit's (`StatusLineView`) inside the band.
private struct StatusLine: NSViewRepresentable {
    var activity: AppModel.Activity?
    var said: String?
    var scale: CGFloat
    var hold: ((StatusLineView) -> Void)?

    func makeNSView(context: Context) -> StatusLineView {
        let view = StatusLineView()
        hold?(view)
        return view
    }

    func updateNSView(_ view: StatusLineView, context: Context) {
        view.scale = scale
        view.show(activity: activity, said: said)
    }
}

/// The status bar's line: a spinning mark and what the window waits for, or what it says for a while.
/// AppKit's so that it can be drawn at once (`drawNow`) before the main thread is busy with a write or a
/// reading: the window draws nothing more until the main thread is free, and the mark goes on spinning
/// by itself meanwhile.
final class StatusLineView: NSView {
    static let textSize: CGFloat = 11
    /// What the line says while nothing is going on: the band keeps its line rather than looking empty.
    static let idle = "-"

    private let spinner = NSProgressIndicator()
    private let label = NSTextField(labelWithString: "")
    private var spinning = false

    var scale: CGFloat = 1 {
        didSet {
            guard scale != oldValue else { return }
            label.font = .systemFont(ofSize: Self.textSize * scale)
            needsLayout = true
        }
    }

    override init(frame: NSRect) {
        super.init(frame: frame)
        spinner.style = .spinning
        spinner.controlSize = .small
        spinner.isDisplayedWhenStopped = false
        spinner.usesThreadedAnimation = true
        label.font = .systemFont(ofSize: Self.textSize)
        label.textColor = .secondaryLabelColor
        label.lineBreakMode = .byTruncatingTail
        label.maximumNumberOfLines = 1
        addSubview(spinner)
        addSubview(label)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not used") }

    override var isFlipped: Bool { true }

    override func layout() {
        super.layout()
        let side = min(16, max(8, bounds.height - 6))
        spinner.frame = NSRect(x: 10, y: (bounds.height - side) / 2, width: side, height: side)
        let left: CGFloat = spinning ? spinner.frame.maxX + 6 : 10
        let height = label.intrinsicContentSize.height
        label.frame = NSRect(x: left, y: (bounds.height - height) / 2, width: max(0, bounds.width - left - 10), height: height)
    }

    /// What the window waits for, else what it says for a while, else that nothing is going on.
    func show(activity: AppModel.Activity?, said: String?) {
        let text = activity?.said ?? said ?? Self.idle
        if label.stringValue != text { label.stringValue = text }
        let spin = activity != nil
        if spin != spinning {
            spinning = spin
            if spin { spinner.startAnimation(nil) } else { spinner.stopAnimation(nil) }
            needsLayout = true
        }
    }

    /// What the line says now.
    var said: String { label.stringValue }

    /// Drawn now, and handed to the screen: what it says shows before the main thread is busy.
    func drawNow() {
        guard window != nil else { return }
        layoutSubtreeIfNeeded()
        display()
        CATransaction.flush()
    }
}

/// What a person does — presses and keys — while the window waits for a reading in the background, held
/// and done once it is in, in the order it was done: on what was read, as it would have been had the
/// window waited on the main thread. Moving the pointer and scrolling go on as they come. Held a few
/// seconds at most: a reading that does not come in does not keep the window from being used.
final class InputHold {
    private var monitor: Any?
    private var held: [NSEvent] = []
    private var round = 0
    private static let longest: Double = 5
    private static let kinds: NSEvent.EventTypeMask = [
        .leftMouseDown, .leftMouseUp, .leftMouseDragged, .rightMouseDown, .rightMouseUp, .rightMouseDragged,
        .otherMouseDown, .otherMouseUp, .otherMouseDragged, .keyDown, .keyUp,
    ]

    var holding = false {
        didSet {
            guard holding != oldValue else { return }
            if holding { start() } else { release() }
        }
    }

    private func start() {
        round += 1
        let started = round
        monitor = NSEvent.addLocalMonitorForEvents(matching: Self.kinds) { [weak self] event in
            guard let self else { return event }
            self.held.append(event)
            return nil
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + Self.longest) { [weak self] in
            guard let self, self.round == started, self.monitor != nil else { return }
            self.release()
        }
    }

    private func release() {
        if let monitor { NSEvent.removeMonitor(monitor) }
        monitor = nil
        let events = held
        held = []
        guard !events.isEmpty else { return }
        // After the window has drawn what was read, so that a press lands on what is shown.
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.02) {
            for event in events { NSApp.postEvent(event, atStart: false) }
        }
    }
}

/// A reading of the project on its way in the background (`AppModel.begin`): what it reads, how (`make`,
/// also run at once on the main thread when a write cannot wait for it), and what was read once it is in.
nonisolated final class PendingReading: @unchecked Sendable {
    enum Read {
        case whole(AppModel.WholeRead)
        case written(AppModel.WrittenRead)
    }

    let core: CoreClient
    let project: String
    let tree: String?
    let make: @Sendable () throws -> Read
    /// Written off the main thread, then read on it once the main thread is told it is in.
    var read: Result<Read, Error>?

    init(core: CoreClient, project: String, tree: String?, make: @escaping @Sendable () throws -> Read) {
        self.core = core
        self.project = project
        self.tree = tree
        self.make = make
    }
}

/// The window's model, held weakly, to be handed back to the main thread from a queue that reads for it.
nonisolated final class WeakModel: @unchecked Sendable {
    weak var model: AppModel?
    init(_ model: AppModel) { self.model = model }
}

/// Something handed to a queue that works off the main thread, and back, read on one thread at a time.
nonisolated struct Handed<T>: @unchecked Sendable {
    let value: T
}
