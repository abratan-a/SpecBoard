//! How tall a card's page of Markdown is drawn — a task's body under its check: an estimate from the text alone,
//! so the core places the card as tall as the screen draws it, the screen being told the same
//! (`faceRows` in `project_detail`) rather than measuring the page it drew.
//!
//! The page is the card's (`MarkdownPage` with `card: true` in the screen): 11.5 points, lines
//! 1.55 apart; paragraphs, lists, code, tables and quotes 0.65 em apart; headings 1.35, 1.2 and
//! 1.08 times as big, 1.3 apart, 0.9 em above and 0.4 em below; code 0.92 em in a box padded
//! 0.55 em; a list indented 1.4 em a level; every line of the text a line of its own. A glyph
//! of the wide scripts (kana, kanji, hangul, full-width forms) is an em wide, any other about
//! half of one. A diagram counts as a box of its own height, whatever it draws.

/// The page's text size, in points.
const FONT: f64 = 11.5;
/// Lines of text this far apart, in ems.
const LEADING: f64 = 1.55;
/// Between blocks, in ems.
const GAP: f64 = 0.65;
/// A diagram drawn on the page (`mermaid`, or an embed `![[…]]`), in points.
const DIAGRAM: f64 = 180.0;

/// The height in points of `text` drawn as the card's page `width` points wide.
pub(crate) fn page_height(text: &str, width: f64) -> f64 {
    let width = width.max(FONT * 4.0);
    let blocks = blocks(text);
    let mut height = 0.0;
    let mut below_last: Option<f64> = None;
    for block in &blocks {
        let (above, body, below) = block.measure(width);
        // Margins between two blocks overlap: the larger is the gap; none above the first.
        if let Some(last) = below_last {
            height += last.max(above);
        }
        height += body;
        below_last = Some(below);
    }
    height.ceil()
}

/// A block of the page.
#[derive(Debug, PartialEq)]
enum Block {
    /// Lines of text, each a line of its own (hard breaks, as the page keeps them).
    Text(Vec<String>),
    /// A heading of `level` (1 to 6).
    Heading(usize, String),
    /// A list's items: how deep each is (0 for the top), and its lines.
    List(Vec<(usize, Vec<String>)>),
    /// A quote's lines.
    Quote(Vec<String>),
    /// Code: its lines.
    Code(Vec<String>),
    /// A table's rows, each its cells.
    Table(Vec<Vec<String>>),
    /// A diagram, drawn as a picture.
    Diagram,
    /// A rule across.
    Rule,
}

impl Block {
    /// Its margin above, its own height and its margin below, in points.
    fn measure(&self, width: f64) -> (f64, f64, f64) {
        let gap = GAP * FONT;
        let line = LEADING * FONT;
        match self {
            Block::Text(lines) => (0.0, wrapped(lines, width, FONT) * line, gap),
            Block::Heading(level, text) => {
                let size = FONT
                    * match level {
                        1 => 1.35,
                        2 => 1.2,
                        3 => 1.08,
                        _ => 1.0,
                    };
                let lines = wrapped(std::slice::from_ref(text), width, size);
                (0.9 * size, lines * 1.3 * size, 0.4 * size)
            }
            Block::List(items) => {
                let mut rows = 0.0;
                for (depth, lines) in items {
                    let inset = 1.4 * FONT * (*depth as f64 + 1.0);
                    rows += wrapped(lines, (width - inset).max(FONT * 2.0), FONT);
                }
                (0.0, rows * line, gap)
            }
            Block::Quote(lines) => (
                0.0,
                wrapped(lines, (width - 0.8 * FONT - 3.0).max(FONT * 2.0), FONT) * line,
                gap,
            ),
            Block::Code(lines) => {
                let size = 0.92 * FONT;
                let inner = (width - 1.5 * FONT).max(FONT * 2.0);
                let rows: f64 = lines
                    .iter()
                    .map(|l| (mono(l) * size / inner).ceil().max(1.0))
                    .sum();
                (0.0, rows.max(1.0) * LEADING * size + 1.1 * FONT, gap)
            }
            Block::Table(rows) => {
                let columns = rows.iter().map(Vec::len).max().unwrap_or(1).max(1);
                let cell = (width / columns as f64 - 1.1 * FONT - 1.0).max(FONT * 2.0);
                let height: f64 = rows
                    .iter()
                    .map(|cells| {
                        let tallest = cells
                            .iter()
                            .map(|c| wrapped(std::slice::from_ref(c), cell, FONT))
                            .fold(1.0, f64::max);
                        tallest * line + 0.4 * FONT + 1.0
                    })
                    .sum();
                (0.0, height + 1.0, gap)
            }
            Block::Diagram => (0.0, DIAGRAM, gap),
            Block::Rule => (0.5 * FONT, 1.0, 0.5 * FONT),
        }
    }
}

/// The lines `lines` take at `size` points, `width` points wide: each wrapped where it no
/// longer fits, never fewer than one.
fn wrapped(lines: &[String], width: f64, size: f64) -> f64 {
    lines
        .iter()
        .map(|l| (ems(&inline(l)) * size / width).ceil().max(1.0))
        .sum::<f64>()
        .max(1.0)
}

/// How wide a line of text is, in ems.
fn ems(text: &str) -> f64 {
    text.chars().map(em).sum()
}

/// How wide a glyph is, in ems: a wide script's a whole em, a space a quarter, a capital a
/// little over half, any other about a half.
fn em(c: char) -> f64 {
    if wide(c) {
        1.0
    } else if c == ' ' {
        0.27
    } else if c.is_ascii_uppercase() || c == 'm' || c == 'w' {
        0.66
    } else if c.is_ascii() {
        0.52
    } else {
        0.6
    }
}

/// Code's width, in ems: monospaced, 0.6 em a glyph, an em for a wide script's.
fn mono(text: &str) -> f64 {
    text.chars().map(|c| if wide(c) { 1.0 } else { 0.6 }).sum()
}

/// A glyph of the scripts drawn an em wide: hangul jamo, the CJK blocks with kana and their
/// punctuation, hangul syllables, compatibility ideographs, vertical and full-width forms, and
/// pictographs.
fn wide(c: char) -> bool {
    matches!(c as u32,
        0x1100..=0x115F
        | 0x2E80..=0xA4CF
        | 0xAC00..=0xD7A3
        | 0xF900..=0xFAFF
        | 0xFE30..=0xFE4F
        | 0xFF00..=0xFF60
        | 0xFFE0..=0xFFE6
        | 0x1F300..=0x1FAFF
        | 0x20000..=0x3FFFD)
}

/// A line as it reads once its marks are drawn: emphasis, code and strike marks gone, a link
/// its text, a path between `[[ ]]` or `{{ }}` itself.
fn inline(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        match c {
            '*' | '_' | '~' | '`' => {}
            '[' | '{' if next == Some(c) => i += 1,
            ']' | '}' if next == Some(c) => i += 1,
            '[' => {}
            ']' if next == Some('(') => {
                // The link's target is not drawn.
                while i < chars.len() && chars[i] != ')' {
                    i += 1;
                }
            }
            ']' => {}
            _ => out.push(c),
        }
        i += 1;
    }
    out
}

/// The text's blocks, in order.
fn blocks(text: &str) -> Vec<Block> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let raw = lines[i];
        let line = raw.trim();
        if line.is_empty() {
            i += 1;
            continue;
        }
        // Code fenced off, or a diagram.
        if let Some(fence) = ["```", "~~~"].into_iter().find(|f| line.starts_with(f)) {
            let diagram = line[fence.len()..].trim().starts_with("mermaid");
            let mut code = Vec::new();
            i += 1;
            while i < lines.len() && !lines[i].trim().starts_with(fence) {
                code.push(lines[i].to_string());
                i += 1;
            }
            i += 1;
            out.push(if diagram {
                Block::Diagram
            } else {
                Block::Code(code)
            });
            continue;
        }
        if line.starts_with("![[") {
            out.push(Block::Diagram);
            i += 1;
            continue;
        }
        if let Some(level) = heading(line) {
            out.push(Block::Heading(
                level,
                line[level..]
                    .trim()
                    .trim_end_matches('#')
                    .trim()
                    .to_string(),
            ));
            i += 1;
            continue;
        }
        if rule(line) {
            out.push(Block::Rule);
            i += 1;
            continue;
        }
        if line.starts_with('|') && lines.get(i + 1).is_some_and(|l| separator(l.trim())) {
            let mut rows = vec![cells(line)];
            i += 2;
            while i < lines.len() && lines[i].trim().starts_with('|') {
                rows.push(cells(lines[i].trim()));
                i += 1;
            }
            out.push(Block::Table(rows));
            continue;
        }
        if line.starts_with('>') {
            let mut quote = Vec::new();
            while i < lines.len() && lines[i].trim().starts_with('>') {
                quote.push(lines[i].trim().trim_start_matches('>').trim().to_string());
                i += 1;
            }
            out.push(Block::Quote(quote));
            continue;
        }
        if item(raw).is_some() {
            let mut items: Vec<(usize, Vec<String>)> = Vec::new();
            while i < lines.len() {
                let raw = lines[i];
                if raw.trim().is_empty() {
                    // A blank line between items keeps the list; one before anything else ends it.
                    if lines.get(i + 1).is_some_and(|l| item(l).is_some()) {
                        i += 1;
                        continue;
                    }
                    break;
                }
                if let Some((depth, text)) = item(raw) {
                    items.push((depth, vec![text]));
                } else if raw.starts_with(' ') || raw.starts_with('\t') {
                    // A line of the item above, indented under it.
                    match items.last_mut() {
                        Some(last) => last.1.push(raw.trim().to_string()),
                        None => break,
                    }
                } else {
                    break;
                }
                i += 1;
            }
            out.push(Block::List(items));
            continue;
        }
        let mut text = Vec::new();
        while i < lines.len() {
            let line = lines[i].trim();
            if line.is_empty()
                || heading(line).is_some()
                || line.starts_with("```")
                || line.starts_with("~~~")
                || line.starts_with('>')
                || item(lines[i]).is_some()
                || (line.starts_with('|') && lines.get(i + 1).is_some_and(|l| separator(l.trim())))
            {
                break;
            }
            text.push(line.to_string());
            i += 1;
        }
        out.push(Block::Text(text));
    }
    out
}

/// A heading's level: its `#`s, one to six, then a space.
fn heading(line: &str) -> Option<usize> {
    let level = line.chars().take_while(|&c| c == '#').count();
    ((1..=6).contains(&level) && line[level..].starts_with(' ')).then_some(level)
}

/// A rule: three or more of `-`, `*` or `_` alone.
fn rule(line: &str) -> bool {
    let compact: String = line.chars().filter(|c| !c.is_whitespace()).collect();
    compact.len() >= 3
        && ['-', '*', '_']
            .iter()
            .any(|&m| compact.chars().all(|c| c == m))
}

/// A table's line under its head: `|---|:--:|`.
fn separator(line: &str) -> bool {
    line.starts_with('|')
        && line.contains('-')
        && line
            .chars()
            .all(|c| matches!(c, '|' | '-' | ':' | ' ' | '\t'))
}

/// A table's line as its cells.
fn cells(line: &str) -> Vec<String> {
    line.trim()
        .trim_matches('|')
        .split('|')
        .map(|c| c.trim().to_string())
        .collect()
}

/// A list's item: how deep it is (two spaces or a tab a level) and its text, its marker and
/// a check box's taken off (the box drawn about as wide as two glyphs).
fn item(raw: &str) -> Option<(usize, String)> {
    let indent: usize = raw
        .chars()
        .take_while(|c| c.is_whitespace())
        .map(|c| if c == '\t' { 2 } else { 1 })
        .sum();
    let line = raw.trim_start();
    let rest = if let Some(r) = ["- ", "* ", "+ "].iter().find_map(|m| line.strip_prefix(m)) {
        r
    } else {
        let digits = line.chars().take_while(char::is_ascii_digit).count();
        if digits == 0 {
            return None;
        }
        line[digits..]
            .strip_prefix(". ")
            .or_else(|| line[digits..].strip_prefix(") "))?
    };
    let text = ["[ ] ", "[x] ", "[X] "]
        .iter()
        .find_map(|b| rest.strip_prefix(b))
        .map_or_else(|| rest.to_string(), |r| format!("口{r}"));
    Some((indent / 2, text))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The card page of the project's card, 352 points wide: 328 for the page.
    const WIDTH: f64 = 328.0;

    #[test]
    fn a_line_is_a_line_and_a_long_one_wraps_at_the_page() {
        let one = page_height("決めたこと", WIDTH);
        assert_eq!(one, (LEADING * FONT).ceil());
        // 60 wide glyphs: 690 points at 11.5, three lines on 328.
        let long = "あ".repeat(60);
        assert_eq!(page_height(&long, WIDTH), (3.0 * LEADING * FONT).ceil());
        // Each line of the text is a line of its own.
        assert_eq!(
            page_height("一\n二\n三", WIDTH),
            (3.0 * LEADING * FONT).ceil()
        );
    }

    #[test]
    fn blocks_are_apart_and_a_heading_is_bigger() {
        let text = "## 決めたこと\n\n- 形を決めた\n- [ ] 書く\n\n本文";
        assert_eq!(
            blocks(text),
            vec![
                Block::Heading(2, "決めたこと".into()),
                Block::List(vec![
                    (0, vec!["形を決めた".into()]),
                    (0, vec!["口書く".into()])
                ]),
                Block::Text(vec!["本文".into()]),
            ]
        );
        let heading = 1.3 * 1.2 * FONT;
        let between = (0.4 * 1.2 * FONT).max(0.0);
        let list = 2.0 * LEADING * FONT;
        let want = heading + between + list + GAP * FONT + LEADING * FONT;
        assert_eq!(page_height(text, WIDTH), want.ceil());
    }

    #[test]
    fn code_tables_quotes_and_diagrams_are_measured() {
        let code = "```\nlet a = 1\nlet b = 2\n```";
        assert_eq!(
            blocks(code),
            vec![Block::Code(vec!["let a = 1".into(), "let b = 2".into()])]
        );
        assert_eq!(
            page_height(code, WIDTH),
            (2.0 * LEADING * 0.92 * FONT + 1.1 * FONT).ceil()
        );
        let table = "| 名前 | 意味 |\n|---|---|\n| a | 一 |\n| b | 二 |";
        let Block::Table(rows) = &blocks(table)[0] else {
            panic!("表でない")
        };
        assert_eq!(rows.len(), 3);
        assert_eq!(
            blocks("```mermaid\nflowchart LR\n```\n\n![[Charts:Ship]]"),
            vec![Block::Diagram, Block::Diagram]
        );
        assert_eq!(
            blocks("> 引用\n> 続き"),
            vec![Block::Quote(vec!["引用".into(), "続き".into()])]
        );
    }

    #[test]
    fn marks_are_not_counted_and_a_link_is_its_text() {
        assert_eq!(inline("**太字** と `code`"), "太字 と code");
        assert_eq!(inline("[説明](https://example.com/very/long/path)"), "説明");
        assert_eq!(
            inline("[[Goals:Ship]] と {{Data:Order.total}}"),
            "Goals:Ship と Data:Order.total"
        );
    }
}
