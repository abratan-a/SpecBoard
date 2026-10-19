//! A file's functions and types found in its syntax tree by its language's query (`symbols.scm`):
//! where each is (from its doc comments to its end, and the line its name is on), what is in it, and
//! the names it calls.
//!
//! The query marks a definition `@definition.<of>.<word>` with its name `@name`; a block whose
//! members belong to a type named elsewhere `@impl.<word>` with the type's name `@owner`; a
//! definition that belongs to such a type by itself (a Go method's receiver) `@owner.<word>`; a
//! name called `@call`; and, where a function's locals may still be its own, an object whose members
//! are the function's (`@members`, what a function returns) and the functions an object may be in
//! (`@scope`).

use streaming_iterator::StreamingIterator;
use tree_sitter::{Node, Query, QueryCursor, Tree};

/// What a function or type is, in every language alike: what the screen's badge and `card_list`'s
/// `of` go by. The language's own word for it is beside it (`word`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Of {
    Function,
    Method,
    Type,
    Module,
    Constant,
    Macro,
}

impl Of {
    pub const ALL: [Of; 6] = [
        Of::Function,
        Of::Method,
        Of::Type,
        Of::Module,
        Of::Constant,
        Of::Macro,
    ];

    pub fn word(self) -> &'static str {
        match self {
            Of::Function => "function",
            Of::Method => "method",
            Of::Type => "type",
            Of::Module => "module",
            Of::Constant => "constant",
            Of::Macro => "macro",
        }
    }

    pub fn of_word(word: &str) -> Option<Of> {
        Of::ALL.into_iter().find(|o| o.word() == word)
    }

    /// Whether what is inside one is its own and not the file's (a function's locals).
    fn is_callable(self) -> bool {
        matches!(self, Of::Function | Of::Method)
    }

    /// Whether others are listed inside one.
    pub fn holds(self) -> bool {
        matches!(self, Of::Type | Of::Module)
    }
}

/// A part of a file: its bytes and its lines (from 1, both ends in it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub from: usize,
    pub to: usize,
}

/// One function or type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Symbol {
    /// Its name among those beside it: the identifier, with ` (2)`… after one written before.
    pub name: String,
    /// Its names from the file's top, each quoted as a path quotes it, joined by `.`.
    pub path: String,
    pub of: Of,
    pub word: String,
    /// The symbol it is in, by its place in the outline.
    pub parent: Option<usize>,
    /// Where it is, from its doc comments and attributes on; for one made of blocks alone (`parts`),
    /// from the first to the last.
    pub span: Span,
    /// The line its name is on.
    pub line: usize,
    /// The blocks elsewhere in the file whose members are its own (Rust's `impl`, Swift's
    /// `extension`), in the file's order.
    pub parts: Vec<Span>,
    /// Made of those blocks alone: its type is not in the file.
    pub made: bool,
    /// The names it calls, each once, in the order first called.
    pub calls: Vec<String>,
    /// Its own text's fingerprint (`digest_of`): its span, and the blocks elsewhere that are its
    /// own (only those when it is made of them). What an implementation's line records of it.
    pub digest: String,
}

/// What the file's functions and types came to.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Found {
    /// In the file's order, each after the one it is in.
    pub symbols: Vec<Symbol>,
    /// How many places the parser could not read (`ERROR` nodes).
    pub parse_errors: usize,
    /// More were found than are kept (`MOST_SYMBOLS`).
    pub cut: bool,
}

/// The most functions and types kept of one file.
pub const MOST_SYMBOLS: usize = 5_000;
/// The most names kept of what one calls.
pub const MOST_CALLS: usize = 200;

/// A definition or a block, as the query found it.
#[derive(Clone, Debug)]
struct Raw {
    /// The node's own bytes, taken out to what wraps it (`Rules::wrappers`): what nests by.
    start: usize,
    end: usize,
    /// From its doc comments and attributes on (`Rules::leading`).
    lead: usize,
    of: Option<Of>,
    word: String,
    name: String,
    name_row: usize,
    /// The type it belongs to, by name, with the word for a type made of such alone.
    owner: Option<(String, String)>,
    /// A block of members (`@impl`), not a definition.
    block: bool,
    /// The object it is a member of, by its bytes: what `@members` is said of.
    in_object: Option<(usize, usize)>,
}

/// What the query says of the objects whose members are a function's own: the objects (`@members`)
/// and the functions they may be in (`@scope`), by their bytes.
#[derive(Default)]
struct Owned {
    members: std::collections::HashSet<(usize, usize)>,
    scopes: Vec<(usize, usize)>,
}

impl Owned {
    /// Whether a definition inside the function `parent` (its bytes) is the function's: a member of
    /// an object it returns itself — the function nearest around the object is the outermost in
    /// `parent`, no callback between.
    fn owns(&self, parent: (usize, usize), object: Option<(usize, usize)>) -> bool {
        let Some(object) = object.filter(|o| self.members.contains(o)) else {
            return false;
        };
        let nearest = self
            .scopes
            .iter()
            .filter(|s| s.0 <= object.0 && object.1 <= s.1)
            .max_by_key(|s| (s.0, std::cmp::Reverse(s.1)));
        let outermost = self
            .scopes
            .iter()
            .filter(|s| parent.0 <= s.0 && s.1 <= parent.1)
            .min_by_key(|s| (s.0, std::cmp::Reverse(s.1)));
        nearest.is_some() && nearest == outermost
    }
}

/// The line (from 1) a byte is on.
pub fn line_at(starts: &[usize], byte: usize) -> usize {
    match starts.binary_search(&byte) {
        Ok(i) => i + 1,
        Err(i) => i,
    }
}

/// The byte each line starts at.
pub fn line_starts(text: &[u8]) -> Vec<usize> {
    let mut out = vec![0];
    out.extend(
        text.iter()
            .enumerate()
            .filter(|(_, b)| **b == b'\n')
            .map(|(i, _)| i + 1),
    );
    out
}

fn span(starts: &[usize], start: usize, end: usize) -> Span {
    // A node ending with its line's newline ends on that line, not the next.
    let last = if end > start { end - 1 } else { end };
    Span {
        start,
        end,
        from: line_at(starts, start),
        to: line_at(starts, last),
    }
}

fn count_errors(tree: &Tree) -> usize {
    let mut count = 0;
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        if node.is_error() || node.is_missing() {
            count += 1;
        } else if node.has_error() {
            let mut cursor = node.walk();
            stack.extend(node.children(&mut cursor));
        }
    }
    count
}

/// Where a definition starts once what wraps it alone and the siblings before it that belong to it
/// are taken in: a comment or an attribute right above it, with no blank line between.
fn framed<'t>(node: Node<'t>, rules: &super::registry::Rules, text: &[u8]) -> (Node<'t>, usize) {
    let mut outer = node;
    while let Some(parent) = outer.parent() {
        // Only what wraps it alone: Go's `type ( … )` holds many, and each is its own.
        let mut cursor = parent.walk();
        let alone = parent
            .named_children(&mut cursor)
            .filter(|c| c.kind() == outer.kind())
            .count()
            == 1;
        if alone && rules.wrappers.iter().any(|w| w == parent.kind()) {
            outer = parent;
        } else {
            break;
        }
    }
    let mut lead = outer.start_byte();
    let mut row = outer.start_position().row;
    let mut before = outer.prev_sibling();
    while let Some(p) = before {
        if !rules.leading.iter().any(|k| k == p.kind()) {
            break;
        }
        let own = &text[p.start_byte()..p.end_byte()];
        // A module's own doc (`//!`, `/*!`) is not the next item's.
        if own.starts_with(b"//!") || own.starts_with(b"/*!") {
            break;
        }
        let end = p.end_position();
        let last_row = if end.column == 0 && end.row > p.start_position().row {
            end.row - 1
        } else {
            end.row
        };
        if last_row + 1 < row {
            break;
        }
        lead = p.start_byte();
        row = p.start_position().row;
        before = p.prev_sibling();
    }
    (outer, lead)
}

/// The functions and types of a file whose tree is `tree`, found by `query`.
pub fn find(
    tree: &Tree,
    text: &[u8],
    query: &Query,
    rules: &super::registry::Rules,
    starts: &[usize],
) -> Found {
    let names = query.capture_names();
    let mut raws: Vec<Raw> = Vec::new();
    let mut calls: Vec<(usize, String)> = Vec::new();
    let mut owned = Owned::default();
    let mut seen = std::collections::HashSet::new();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(query, tree.root_node(), text);
    while let Some(m) = matches.next() {
        let mut def: Option<(Node, Option<Of>, String, bool)> = None;
        let mut name: Option<Node> = None;
        let mut owner: Option<(String, String)> = None;
        for c in m.captures() {
            let capture = names[c.index as usize];
            let parts: Vec<&str> = capture.split('.').collect();
            match parts.as_slice() {
                ["definition", of, word @ ..] => {
                    def = Some((c.node, Of::of_word(of), word.join("."), false));
                }
                ["impl", word @ ..] => {
                    def = Some((c.node, None, word.join("."), true));
                }
                ["name"] => name = Some(c.node),
                ["owner", word @ ..] => {
                    let said = c.node.utf8_text(text).unwrap_or_default().to_string();
                    owner = Some((said, word.join(".")));
                }
                ["call"] => {
                    if let Ok(said) = c.node.utf8_text(text) {
                        calls.push((c.node.start_byte(), said.to_string()));
                    }
                }
                ["members"] => {
                    owned
                        .members
                        .insert((c.node.start_byte(), c.node.end_byte()));
                }
                ["scope"] => owned.scopes.push((c.node.start_byte(), c.node.end_byte())),
                _ => {}
            }
        }
        let Some((node, of, word, block)) = def else {
            continue;
        };
        if !block && of.is_none() {
            continue;
        }
        let (outer, lead) = framed(node, rules, text);
        if !seen.insert((outer.start_byte(), outer.end_byte(), block)) {
            continue;
        }
        let (name, name_row) = match (name, &owner) {
            (Some(n), _) => (
                n.utf8_text(text).unwrap_or_default().trim().to_string(),
                n.start_position().row,
            ),
            (None, Some((o, _))) if block => (o.clone(), node.start_position().row),
            _ => continue,
        };
        if name.is_empty() {
            continue;
        }
        // A name taken from a text (a test's) may be long: cut as a file's name is.
        let name = if name.chars().count() > crate::pathtext::NAME_MAX {
            name.chars().take(crate::pathtext::NAME_MAX).collect()
        } else {
            name
        };
        let in_object = node
            .parent()
            .filter(|p| p.kind() == "object")
            .map(|p| (p.start_byte(), p.end_byte()));
        raws.push(Raw {
            start: outer.start_byte(),
            end: outer.end_byte(),
            lead,
            of,
            word,
            name,
            name_row,
            owner,
            block,
            in_object,
        });
    }
    raws.sort_by(|a, b| a.start.cmp(&b.start).then(b.end.cmp(&a.end)));
    let found = arrange(raws, starts, &owned);
    let mut found = Found {
        parse_errors: count_errors(tree),
        ..found
    };
    put_calls(&mut found.symbols, calls);
    for symbol in &mut found.symbols {
        let own = (!symbol.made).then_some(symbol.span);
        symbol.digest = digest_of(
            text,
            own.iter()
                .chain(symbol.parts.iter())
                .map(|s| (s.start, s.end)),
        );
    }
    found
}

/// What tells one form of a code's text from another (`w1`): the text of `spans`, each with every
/// run of spaces, tabs and line ends made one space and trimmed, joined by one space, SHA-256's
/// first 8 bytes in hex. So re-indenting it, or moving it in its file, does not change it, and a
/// change to any of its words does. The tag says how it was made: a fingerprint made another way
/// is never taken for this one.
pub fn digest_of(text: &[u8], spans: impl IntoIterator<Item = (usize, usize)>) -> String {
    use sha2::{Digest, Sha256};
    let mut said: Vec<u8> = Vec::new();
    for (start, end) in spans {
        let own = &text[start.min(text.len())..end.min(text.len())];
        if !said.is_empty() {
            said.push(b' ');
        }
        let mut gap = false;
        let began = said.len();
        for &b in own {
            if matches!(b, b' ' | b'\t' | b'\r' | b'\n') {
                gap = said.len() > began;
                continue;
            }
            if gap {
                said.push(b' ');
                gap = false;
            }
            said.push(b);
        }
    }
    let digest = Sha256::digest(&said);
    let hex: String = digest[..8].iter().map(|b| format!("{b:02x}")).collect();
    format!("{DIGEST_TAG}{hex}")
}

/// How the fingerprints `digest_of` makes are made, written before each.
pub const DIGEST_TAG: &str = "w1:";

/// A symbol as it is being arranged: by its place among the raw ones.
struct Node2 {
    raw: Raw,
    parent: Option<usize>,
    kept: bool,
    parts: Vec<Span>,
    made: bool,
}

/// The raw definitions nested by their bytes, the locals of a function let go, the blocks put in the
/// types they are of, named and cut to the most kept.
fn arrange(raws: Vec<Raw>, starts: &[usize], owned: &Owned) -> Found {
    let mut nodes: Vec<Node2> = Vec::with_capacity(raws.len());
    let mut stack: Vec<usize> = Vec::new();
    for raw in raws {
        while let Some(&top) = stack.last() {
            if nodes[top].raw.end <= raw.start {
                stack.pop();
            } else {
                break;
            }
        }
        let parent = stack.last().copied();
        // What is inside a function, or inside what was let go, is not the file's — but a member
        // of an object the function returns is the function's.
        let kept = parent.is_none_or(|p| {
            let held = &nodes[p].raw;
            nodes[p].kept
                && (!held.of.is_some_and(Of::is_callable)
                    || owned.owns((held.start, held.end), raw.in_object))
        });
        nodes.push(Node2 {
            raw,
            parent,
            kept,
            parts: Vec::new(),
            made: false,
        });
        stack.push(nodes.len() - 1);
    }
    // The blocks and the members that name their type: into the type of that name beside them, or
    // one made for them where there is none.
    let count = nodes.len();
    for i in 0..count {
        if !nodes[i].kept || nodes[i].raw.owner.is_none() {
            continue;
        }
        let parent = nodes[i].parent;
        let (owner, owner_word) = nodes[i].raw.owner.clone().unwrap();
        let target = (0..nodes.len()).find(|&j| {
            j != i
                && nodes[j].kept
                && nodes[j].parent == parent
                && nodes[j].raw.name == owner
                && (nodes[j].made || (!nodes[j].raw.block && nodes[j].raw.of == Some(Of::Type)))
        });
        let block = nodes[i].raw.block;
        let own = span(starts, nodes[i].raw.lead, nodes[i].raw.end);
        let target = match target {
            Some(t) => t,
            None => {
                let word = if block {
                    nodes[i].raw.word.clone()
                } else {
                    owner_word
                };
                nodes.push(Node2 {
                    raw: Raw {
                        start: nodes[i].raw.start,
                        end: nodes[i].raw.end,
                        lead: nodes[i].raw.lead,
                        of: Some(Of::Type),
                        word,
                        name: owner.clone(),
                        name_row: nodes[i].raw.name_row,
                        owner: None,
                        block: false,
                        in_object: None,
                    },
                    parent,
                    kept: true,
                    parts: Vec::new(),
                    made: true,
                });
                nodes.len() - 1
            }
        };
        if block || nodes[target].made {
            nodes[target].parts.push(own);
        }
        if block {
            for n in nodes.iter_mut().take(count) {
                if n.parent == Some(i) {
                    n.parent = Some(target);
                }
            }
            nodes[i].kept = false;
        } else {
            nodes[i].parent = Some(target);
        }
        if nodes[target].made {
            let first = nodes[target].parts.first().map_or(own.start, |p| p.start);
            let last = nodes[target]
                .parts
                .iter()
                .map(|p| p.end)
                .max()
                .unwrap_or(own.end);
            nodes[target].raw.lead = first;
            nodes[target].raw.start = first;
            nodes[target].raw.end = last;
        }
    }
    // In the file's order, each after the one it is in.
    let mut children: std::collections::HashMap<Option<usize>, Vec<usize>> =
        std::collections::HashMap::new();
    for (i, n) in nodes.iter().enumerate() {
        if n.kept {
            children.entry(n.parent).or_default().push(i);
        }
    }
    for list in children.values_mut() {
        list.sort_by_key(|&i| (nodes[i].raw.lead, nodes[i].made));
    }
    let mut emit = Emit {
        nodes: &nodes,
        children: &children,
        starts,
        out: Vec::new(),
        cut: false,
    };
    emit.under(None, None, "", None);
    Found {
        symbols: emit.out,
        parse_errors: 0,
        cut: emit.cut,
    }
}

/// The symbols arranged, put in the outline's order: each named among those beside it and said as
/// its path.
struct Emit<'a> {
    nodes: &'a [Node2],
    children: &'a std::collections::HashMap<Option<usize>, Vec<usize>>,
    starts: &'a [usize],
    out: Vec<Symbol>,
    cut: bool,
}

impl Emit<'_> {
    /// Those in `at` (the file's top for none), each followed by what is in it.
    fn under(
        &mut self,
        at: Option<usize>,
        parent_out: Option<usize>,
        prefix: &str,
        parent_of: Option<Of>,
    ) {
        let Some(list) = self.children.get(&at) else {
            return;
        };
        let mut taken: std::collections::HashSet<String> = std::collections::HashSet::new();
        for &i in list {
            if self.out.len() >= MOST_SYMBOLS {
                self.cut = true;
                return;
            }
            let n = &self.nodes[i];
            let mut name = n.raw.name.clone();
            let mut k = 2;
            while !taken.insert(name.clone()) {
                name = format!("{} ({k})", n.raw.name);
                k += 1;
            }
            let quoted = crate::pathtext::quote(&name).into_owned();
            let path = if prefix.is_empty() {
                quoted
            } else {
                format!("{prefix}.{quoted}")
            };
            let mut of = n.raw.of.unwrap_or(Of::Type);
            if of == Of::Function && parent_of == Some(Of::Type) {
                of = Of::Method;
            }
            let mut parts = n.parts.clone();
            parts.sort_by_key(|p| p.start);
            self.out.push(Symbol {
                name,
                path: path.clone(),
                of,
                word: n.raw.word.clone(),
                parent: parent_out,
                span: span(self.starts, n.raw.lead, n.raw.end),
                line: n.raw.name_row + 1,
                parts,
                made: n.made,
                calls: Vec::new(),
                digest: String::new(),
            });
            let me = self.out.len() - 1;
            self.under(Some(i), Some(me), &path, Some(of));
        }
    }
}

/// Each name called given to the innermost function or type it is called in.
fn put_calls(symbols: &mut [Symbol], mut calls: Vec<(usize, String)>) {
    calls.sort_by_key(|c| c.0);
    // The spans a call can be in, the innermost last among those around it.
    let mut spans: Vec<(usize, usize, usize)> = Vec::new();
    for (i, s) in symbols.iter().enumerate() {
        if s.made {
            continue;
        }
        spans.push((s.span.start, s.span.end, i));
    }
    spans.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));
    let mut stack: Vec<(usize, usize, usize)> = Vec::new();
    let mut next = 0;
    let mut seen: Vec<std::collections::HashSet<String>> =
        vec![std::collections::HashSet::new(); symbols.len()];
    for (at, name) in calls {
        while next < spans.len() && spans[next].0 <= at {
            while stack.last().is_some_and(|top| top.1 <= spans[next].0) {
                stack.pop();
            }
            stack.push(spans[next]);
            next += 1;
        }
        while stack.last().is_some_and(|top| top.1 <= at) {
            stack.pop();
        }
        let Some(&(_, _, i)) = stack.last() else {
            continue;
        };
        if symbols[i].calls.len() < MOST_CALLS && seen[i].insert(name.clone()) {
            symbols[i].calls.push(name);
        }
    }
}
