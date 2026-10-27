//! Where a bundled grammar reads a text wrong in a way its own error recovery cannot undo, found in
//! the tree it made: the bytes to blank in a copy of the text, so that the copy parses as the text
//! means (`super::parse`). Each is the grammar's fault, kept here until its crate reads the text
//! right. Design: `specboard://SpecBoard/Docs/Design:Overview` (the code in the working directories).

use std::ops::Range;
use tree_sitter::{Node, Tree};

/// A grammar's mis-read: the bytes without which a text cannot have it (so a text without them is
/// parsed once), and how the first one in a tree is found (the bytes to blank, or None).
#[derive(Clone, Copy, Debug)]
pub struct Mend {
    pub marker: &'static [u8],
    pub find: fn(&Tree, &[u8]) -> Option<Range<usize>>,
}

impl Mend {
    /// Whether `text` may hold the mis-read.
    pub fn may(&self, text: &[u8]) -> bool {
        text.windows(self.marker.len()).any(|w| w == self.marker)
    }
}

pub const SWIFT: Mend = Mend {
    marker: b"#\"\"\"",
    find: swift_raw_strings,
};

/// Swift: the grammar's scanner reads a multi-line raw string (`#"""` … `"""#`) as a one-line one and
/// ends it at the first `"` followed by as many `#` as it opened with (the `"#` of `"#3366FF"`), which
/// only ends a one-line raw string. What follows is read as code until the file breaks. The bytes
/// returned are those `#`s: blanked, the scanner reads on to the real end.
fn swift_raw_strings(tree: &Tree, text: &[u8]) -> Option<Range<usize>> {
    // The hashes the literal being read opened with, and whether it is a multi-line one.
    let mut open: Option<(usize, bool)> = None;
    let mut found = None;
    leaves(tree.root_node(), &mut |leaf| {
        let kind = leaf.kind();
        if kind != "raw_str_part" && kind != "raw_str_end_part" {
            return true;
        }
        let own = &text[leaf.start_byte()..leaf.end_byte()];
        let (hashes, multi) = match open {
            Some(o) => o,
            None => opening(own),
        };
        if kind == "raw_str_part" {
            open = Some((hashes, multi));
            return true;
        }
        open = None;
        if multi && !closes(own, hashes) && hashes > 0 && own.len() >= hashes {
            found = Some(leaf.end_byte() - hashes..leaf.end_byte());
            return false;
        }
        true
    });
    found
}

/// The hashes a raw string opens with, and whether `"""` and a line end follow them.
fn opening(own: &[u8]) -> (usize, bool) {
    let hashes = own.iter().take_while(|&&b| b == b'#').count();
    let rest = &own[hashes..];
    let multi = rest.starts_with(b"\"\"\"") && matches!(rest.get(3), Some(b'\n') | Some(b'\r'));
    (hashes, multi)
}

/// Whether a multi-line raw string's last part ends as one: `"""` and its hashes.
fn closes(own: &[u8], hashes: usize) -> bool {
    own.len() >= hashes + 3
        && own[own.len() - hashes..].iter().all(|&b| b == b'#')
        && own[..own.len() - hashes].ends_with(b"\"\"\"")
}

/// The leaves under `node` in the text's order, until `each` says stop.
fn leaves(node: Node, each: &mut impl FnMut(Node) -> bool) {
    let mut cursor = node.walk();
    loop {
        let here = cursor.node();
        if here.child_count() == 0 {
            if !each(here) {
                return;
            }
        } else if cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return;
            }
        }
    }
}
