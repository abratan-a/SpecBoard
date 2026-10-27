import Foundation

/// Colours read from texts written as raw strings: each one holds what looks like its end.
struct Palette {
    static let light = #"""
    { "accent": "#3366FF", "muted": "#8899AA" }
    """#

    func accent() -> String {
        Self.light
    }

    static let dark = ##"""
    { "accent": "##1F2937", "line": "#E5E7EB", "quote": """# }
    """##

    func background() -> String {
        Self.dark
    }

    func swatch(name: String) -> String {
        #"""
        { "name": "\#(name)", "hex": "#FFAA00" }
        """#
    }

    func label(of name: String) -> String {
        let quoted = #"say "hi" to #ff"#
        return #"\#(name): \#(quoted)"#
    }

    // The last one: still inside the struct.
    func border() -> Int {
        1
    }
}
