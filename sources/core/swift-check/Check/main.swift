// Calls the core from Swift: create, open, call, hear notices, ask the app back (app_control) and close,
// end to end. Loads the sample project file (the argument, tests/fixtures/projects/sample.json) into a new
// store and reads the whole board. Exits with 1 when a check fails.
import Foundation
import SpecBoardCore

final class Heard: CoreListener, @unchecked Sendable {
    private let lock = NSLock()
    private var notices: [String] = []

    func changed(notice: String) {
        lock.lock()
        notices.append(notice)
        lock.unlock()
    }

    var all: [String] {
        lock.lock()
        defer { lock.unlock() }
        return notices
    }
}

var failures: [String] = []
func check(_ ok: Bool, _ what: String) {
    print(ok ? "passed: \(what)" : "FAILED: \(what)")
    if !ok { failures.append(what) }
}

func json(_ text: String) -> Any? { try? JSONSerialization.jsonObject(with: Data(text.utf8)) }
func object(_ text: String) -> [String: Any] { json(text) as? [String: Any] ?? [:] }

guard CommandLine.arguments.count > 1 else {
    print("usage: core-check <sample project file>")
    exit(2)
}
let sample = URL(fileURLWithPath: CommandLine.arguments[1])
guard let text = try? String(contentsOf: sample, encoding: .utf8),
      let project = ((json(text) as? [String: Any])?["project"] as? [String: Any])?["id"] as? String else {
    print("FAILED: \(sample.path) cannot be read as a project file")
    exit(1)
}

let place = FileManager.default.temporaryDirectory
    .appendingPathComponent("core-check-\(ProcessInfo.processInfo.processIdentifier).db")
try? FileManager.default.removeItem(at: place)
defer { try? FileManager.default.removeItem(at: place) }
do {
    _ = try Core.open(place: place.path)
    check(false, "a missing store does not open")
} catch {
    check("\(error)".contains("データベースがありません"), "a missing store does not open (\(error))")
}
let made = try Core.create(place: place.path)
let read = try JSONSerialization.data(withJSONObject: ["op": "project_file_read", "text": text])
check(object(made.call(request: String(decoding: read, as: UTF8.self)))["error"] == nil,
      "creates a new store and loads the sample project file")
made.close()

let core = try Core.open(place: place.path)
let heard = Heard()
core.listen(listener: heard)

let ping = object(core.call(request: #"{"op":"ping"}"#))
let version = (ping["result"] as? [String: Any])?["core"] as? String
check(version?.isEmpty == false, "ping returns the core's version (\(version ?? "none"))")
check(heard.all.count == 1 && object(heard.all.first ?? "")["op"] as? String == "ping", "the notice of ping reaches Swift")

let request = try JSONSerialization.data(withJSONObject: ["op": "project_detail", "project": project, "branch": "~board"])
let detail = object(core.call(request: String(decoding: request, as: UTF8.self)))["result"] as? [String: Any]
let cards = detail?["schemas"] as? [[String: Any]] ?? []
let goal = cards.first { $0["kind"] as? String == "goal" }
check(goal != nil, "reads the whole board, with a goal card (\(cards.count) cards)")
// What a goal holds are cards inside it (`in`), not a list on the goal.
let goalId = goal?["id"] as? String
let held = cards.filter { goalId != nil && $0["in"] as? String == goalId }
check(!held.isEmpty && ((goal?["form"] as? [String: Any])?["fields"] as? [String: Any])?["items"] == nil,
      "the cards in a goal come with in (\(held.count) cards)")

let refused = object(core.call(request: #"{"op":"card_read"}"#))
check(refused["error"] as? String == "「card_read」はまだ使えません", "the refusal of a request it does not take arrives as the core wrote it")

// app_control: the core asks the app (Swift) back, with the call and who made it, and reads its answer.
// ask: the core asks a person through the app whether MCP may do what it asked to.
final class Window: CoreApp, @unchecked Sendable {
    private let lock = NSLock()
    private var asked: [(String, String)] = []
    private var questions: [String] = []

    func appControl(arguments: String, caller: String) -> String {
        lock.lock()
        asked.append((arguments, caller))
        lock.unlock()
        return #"{"image":{"data":"iVBORw0KGgo=","mimeType":"image/png"},"note":{"target":"window"}}"#
    }

    func ask(request: String) -> String {
        lock.lock()
        questions.append(request)
        lock.unlock()
        return #"{"answer":"once","client":"check"}"#
    }

    func picture(request: String) -> String {
        #"{"error":"the check draws no pictures"}"#
    }

    var all: [(String, String)] {
        lock.lock()
        defer { lock.unlock() }
        return asked
    }

    var asks: [String] {
        lock.lock()
        defer { lock.unlock() }
        return questions
    }
}
let window = Window()
core.setApp(app: window)
let keyMade = object(core.call(request: #"{"op":"access_key_create","name":"screen helper","permits":{"app_control.capture":"allow","project_create":"ask"}}"#))["result"] as? [String: Any]
let key = keyMade?["value"] as? String ?? ""
let mcp = try JSONSerialization.data(withJSONObject: [
    "op": "mcp",
    "message": ["jsonrpc": "2.0", "id": 1, "method": "tools/call",
                "params": ["name": "app_control", "arguments": ["op": "capture"]]],
    "headers": ["authorization": "Bearer \(key)"],
])
let answered = object(core.call(request: String(decoding: mcp, as: UTF8.self)))["result"] as? [String: Any]
let content = ((answered?["body"] as? [String: Any])?["result"] as? [String: Any])?["content"] as? [[String: Any]] ?? []
check(content.first?["type"] as? String == "image" && content.first?["data"] as? String == "iVBORw0KGgo=",
      "app_control asks the app (Swift), and the picture comes back as an MCP image")
check(window.all.count == 1 && window.all.first?.1.contains(#""name":"screen helper""#) == true
      && window.all.first?.0.contains(#""op":"capture""#) == true,
      "the app gets the call's arguments and the key that called (\(window.all.first?.1 ?? "none"))")

let create = try JSONSerialization.data(withJSONObject: [
    "op": "mcp",
    "message": ["jsonrpc": "2.0", "id": 2, "method": "tools/call",
                "params": ["name": "project_create", "arguments": ["project": "Asked For", "request": true]]],
    "headers": ["authorization": "Bearer \(key)"],
])
let created = object(core.call(request: String(decoding: create, as: UTF8.self)))["result"] as? [String: Any]
let createdResult = (created?["body"] as? [String: Any])?["result"] as? [String: Any]
check(createdResult?["isError"] as? Bool == false && window.asks.count == 1
      && window.asks.first?.contains(#""tool":"project_create""#) == true,
      "an item that asks each time asks the app (Swift), is allowed once and goes through (\(window.asks.first ?? "not asked"))")

core.close()
check(object(core.call(request: #"{"op":"ping"}"#))["error"] as? String == "閉じています", "refuses after closing")

do {
    _ = try Core.open(place: "")
    check(false, "an empty store path does not open")
} catch {
    check("\(error)".contains("データの置き場が空です"), "an empty store path does not open (\(error))")
}

exit(failures.isEmpty ? 0 : 1)
