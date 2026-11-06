import Foundation

/// What the sheet that adds an export's row has typed, and what was said of the last try: a refusal, or what
/// the row went in leaving out. Any of it changed, what was said is of a row no longer there — it goes, and
/// the row can be added again.
struct ExportEntryDraft: Equatable {
    var from = "" { didSet { if from != oldValue { edited() } } }
    var to = "" { didSet { if to != oldValue { edited() } } }
    var format = "" { didSet { if format != oldValue { edited() } } }
    var kinds = "" { didSet { if kinds != oldValue { edited() } } }
    var problem: String?
    /// The row went in, with what it leaves out said under the fields: the sheet stays open to be read.
    var added = false

    private mutating func edited() {
        problem = nil
        added = false
    }
}
