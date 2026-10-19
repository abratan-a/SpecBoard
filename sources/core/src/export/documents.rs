//! Notes and documents as Markdown files (`DocumentExport`, `NoteEmbed`, `FormatRender`
//! in the Swift app): a body as it is; a formatdoc its title, each item a heading as
//! deep as it is with its path under it as a comment, and each row what it says — a card's
//! table under the way to the card. A `![[…]]` becomes a mermaid block, a
//! `{{…}}` its value, a `[[…]]` the text it shows or the card's label or name (a section's
//! own name) — a link when it is written in the same directory — and one that does not resolve
//! is struck through.

use super::charts::{self, code, link};
use super::paths::{landing, project_value, resolves};
use super::{Project, mermaid};
use crate::model::{Card, Field, FormatRow};
use crate::notetext::{self, Kind, Ref, plain_parts, plain_path};
use serde_json::Value;
use std::collections::HashMap;

/// Text as Markdown prints it: no `[`, `]`, `*`, `_`, `` ` `` or `\` read as anything else.
pub fn text(raw: &str) -> String {
    let mut out = String::new();
    for c in raw.chars() {
        if "[]*_`\\".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// A table as Markdown: the headings, the line under them, each row as wide as the
/// headings.
pub fn table(columns: &[String], cells: &[Vec<String>], cell: &dyn Fn(&str) -> String) -> String {
    let width = columns
        .len()
        .max(cells.iter().map(Vec::len).max().unwrap_or(0));
    if width == 0 {
        return String::new();
    }
    let line = |items: &[String]| -> String {
        let mut padded: Vec<String> = items.to_vec();
        while padded.len() < width {
            padded.push(String::new());
        }
        let shown: Vec<String> = padded[..width]
            .iter()
            .map(|c| cell(c).replace('|', "\\|").replace('\n', " "))
            .collect();
        format!("| {} |", shown.join(" | "))
    };
    let mut lines = vec![line(columns), format!("|{}", "---|".repeat(width))];
    lines.extend(cells.iter().map(|r| line(r)));
    lines.join("\n")
}

/// The rows as Markdown: each item a heading as deep as it is, each other row what
/// `render` makes of it. With `named`, each item's heading has its path in the document under it
/// as a comment (`<!-- row intro.goal -->`): a body's `[[Doc.intro.goal]]` is found by it though
/// the heading says something else, and a drawn page does not show it.
pub(crate) fn rows_markdown(
    rows: &[FormatRow],
    named: bool,
    render: &mut dyn FnMut(&FormatRow) -> Option<String>,
) -> String {
    let mut parts: Vec<String> = Vec::new();
    rows_parts(rows, 0, "", named, None, &mut parts, render);
    parts.join("\n\n")
}

/// One row's part of the rows' Markdown (an item: its heading and every row inside it), the same
/// characters the whole holds there; none when no row is at `names`.
pub(crate) fn row_markdown(
    rows: &[FormatRow],
    names: &[String],
    named: bool,
    render: &mut dyn FnMut(&FormatRow) -> Option<String>,
) -> Option<String> {
    crate::export::paths::format_row(names, rows)?;
    let mut parts: Vec<String> = Vec::new();
    rows_parts(rows, 0, "", named, Some(names), &mut parts, render);
    Some(parts.join("\n\n"))
}

/// The parts of the rows' Markdown, from `depth` under the item at `trail`: all of them, or only
/// those of the row `only` names and what is inside it.
fn rows_parts(
    list: &[FormatRow],
    depth: usize,
    trail: &str,
    named: bool,
    only: Option<&[String]>,
    parts: &mut Vec<String>,
    render: &mut dyn FnMut(&FormatRow) -> Option<String>,
) {
    for row in list {
        // Below the row asked for, all of it; above it, only the way down to it.
        let (here, below) = match only {
            None => (true, None),
            Some([name]) if *name == row.name => (true, None),
            Some([name, rest @ ..]) if *name == row.name => (false, Some(rest)),
            Some(_) => continue,
        };
        let path = if trail.is_empty() {
            crate::pathtext::quote(&row.name).into_owned()
        } else {
            crate::pathtext::member(trail, &row.name)
        };
        if here {
            if row.kind == "item" {
                let title = if row.heading.is_empty() {
                    &row.name
                } else {
                    &row.heading
                };
                let heading = format!("{} {title}", "#".repeat((depth + 2).min(6)));
                parts.push(if named {
                    format!("{heading}\n{}", charts::marker(&format!("row {path}")))
                } else {
                    heading
                });
            } else if let Some(text) = render(row) {
                let text = text.trim_matches(|c: char| {
                    c == '\n'
                        || c == '\r'
                        || c == '\u{85}'
                        || c == '\u{2028}'
                        || c == '\u{2029}'
                        || c == '\u{0B}'
                        || c == '\u{0C}'
                });
                if !text.is_empty() {
                    parts.push(text.to_string());
                }
            }
        }
        if row.kind == "item" && (here || below.is_some()) {
            rows_parts(&row.rows, depth + 1, &path, named, below, parts, render);
        }
    }
}

/// A shape's contents as a table: an entity's or a value's fields, or an enum's values; a data
/// card's values; a table's rows, the paths its cells name as `links` says them.
fn card_table(
    card: &Card,
    project: &Project,
    render: &dyn Fn(&str) -> String,
    links: &dyn Fn(&str) -> String,
) -> Option<String> {
    match card.kind.as_str() {
        "entity" | "value" => {
            let mut cells: Vec<Vec<String>> = Vec::new();
            fn walk(
                list: &[Field],
                trail: &[String],
                project: &Project,
                render: &dyn Fn(&str) -> String,
                cells: &mut Vec<Vec<String>>,
            ) {
                for field in list {
                    let mut path = trail.to_vec();
                    path.push(crate::pathtext::quote(&field.name).into_owned());
                    let mut kind = if field.is_ref() {
                        field
                            .ref_schema_id
                            .as_deref()
                            .and_then(|id| project.card(id))
                            .map(|c| c.name.clone())
                            .unwrap_or_else(|| "ref".into())
                    } else {
                        field.kind.clone()
                    };
                    if field.is_array {
                        kind += "[]";
                    }
                    if field.is_nullable {
                        kind += " | null";
                    }
                    let about = if field.label.is_empty() {
                        field.description.clone()
                    } else if field.description.is_empty() {
                        field.label.clone()
                    } else {
                        format!("{} — {}", field.label, field.description)
                    };
                    cells.push(vec![
                        charts::code(&path.join(".")),
                        kind,
                        if field.is_required {
                            text!("exportDocs.cell.required")
                        } else {
                            String::new()
                        },
                        render(&about),
                    ]);
                    walk(&field.children, &path, project, render, cells);
                }
            }
            walk(&card.fields, &[], project, render, &mut cells);
            let columns: Vec<String> = vec![
                text!("exportDocs.column.name"),
                text!("exportDocs.column.type"),
                text!("exportDocs.column.required"),
                text!("exportDocs.column.description"),
            ];
            Some(table(&columns, &cells, &|c| c.to_string()))
        }
        // A table: the table its rules build.
        crate::kinds::TABLE => Some(links(&crate::tools::tables::markdown(
            &crate::tools::tables::build(project, card),
        ))),
        // A data card: its shape's fields, each with its type and its value; what the shape has
        // lost after them.
        crate::kinds::INSTANCE => {
            let shape = crate::tools::instances::shape(project, card)?;
            let mut cells: Vec<Vec<String>> = Vec::new();
            // Its objects' keys sorted, as the screen writes it.
            fn shown(value: &Value) -> String {
                format!(
                    "`{}`",
                    serde_json::to_string(&super::sorted(value)).unwrap_or_default()
                )
            }
            fn walk(
                list: &[Field],
                trail: &[String],
                values: Option<&Value>,
                project: &Project,
                cells: &mut Vec<Vec<String>>,
                depth: usize,
            ) {
                for field in list {
                    let value = values.and_then(|o| o.get(&field.name));
                    // A branch of a discriminator, when it is written.
                    if !field.variants.is_empty() && value.is_none() {
                        continue;
                    }
                    let mut path = trail.to_vec();
                    path.push(crate::pathtext::quote(&field.name).into_owned());
                    let kind = crate::tools::instances::type_label(project, field);
                    let element: &[Field] = if field.is_object() {
                        &field.children
                    } else if field.is_ref() {
                        match field
                            .ref_schema_id
                            .as_deref()
                            .and_then(|id| project.card(id))
                        {
                            Some(target) if target.kind != "enum" => &target.fields,
                            _ => &[],
                        }
                    } else {
                        &[]
                    };
                    // An array, element by element: its fields under each, a plain value on its
                    // row.
                    if field.is_array
                        && let Some(items) =
                            value.and_then(Value::as_array).filter(|i| !i.is_empty())
                        && depth < 24
                    {
                        cells.push(vec![
                            charts::code(&path.join(".")),
                            kind.clone(),
                            String::new(),
                        ]);
                        let one = kind.trim_end_matches("[]").to_string();
                        for (i, item) in items.iter().enumerate() {
                            let mut at = path.clone();
                            if let Some(last) = at.last_mut() {
                                last.push_str(&format!("[{i}]"));
                            }
                            if !element.is_empty() && item.is_object() {
                                cells.push(vec![
                                    charts::code(&at.join(".")),
                                    one.clone(),
                                    String::new(),
                                ]);
                                walk(element, &at, Some(item), project, cells, depth + 1);
                            } else {
                                cells.push(vec![
                                    charts::code(&at.join(".")),
                                    one.clone(),
                                    shown(item),
                                ]);
                            }
                        }
                        continue;
                    }
                    let inner: &[Field] = if field.is_array { &[] } else { element };
                    if !inner.is_empty() && value.is_none_or(Value::is_object) && depth < 24 {
                        cells.push(vec![charts::code(&path.join(".")), kind, String::new()]);
                        walk(inner, &path, value, project, cells, depth + 1);
                    } else {
                        cells.push(vec![
                            charts::code(&path.join(".")),
                            kind,
                            value.map(shown).unwrap_or_default(),
                        ]);
                    }
                }
            }
            walk(
                &shape.fields,
                &[],
                card.instance_data(),
                project,
                &mut cells,
                0,
            );
            for (at, value) in crate::tools::instances::strays(project, card) {
                cells.push(vec![
                    format!("`{at}`"),
                    text!("exportDocs.cell.notInShape"),
                    shown(&value),
                ]);
            }
            let columns: Vec<String> = vec![
                text!("exportDocs.column.name"),
                text!("exportDocs.column.type"),
                text!("exportDocs.column.value"),
            ];
            Some(table(&columns, &cells, &|c| c.to_string()))
        }
        "enum" => {
            // A third column only when a value says what it stands for.
            let described = card.values.iter().any(|v| !v.description.is_empty());
            let mut columns: Vec<String> = vec![
                text!("exportDocs.column.value"),
                text!("exportDocs.column.displayName"),
            ];
            if described {
                columns.push(text!("exportDocs.column.description"));
            }
            let cells: Vec<Vec<String>> = card
                .values
                .iter()
                .map(|v| {
                    let mut row = vec![format!("`{}`", v.value), v.label.clone()];
                    if described {
                        row.push(v.description.clone());
                    }
                    row
                })
                .collect();
            Some(table(&columns, &cells, &|c| c.to_string()))
        }
        _ => None,
    }
}

/// The flows of a chart section, its inner sections' after its own.
fn charts_in<'a>(target: &str, base: &str, project: &Project<'a>) -> Option<Vec<&'a Card>> {
    let name = target.strip_suffix(':').unwrap_or(target);
    let section = project.directory.section_from(name, base)?;
    if project
        .detail
        .sections
        .iter()
        .find(|s| s.id == section)?
        .kind
        .as_deref()
        != Some("chart")
    {
        return None;
    }
    let mut out = Vec::new();
    fn walk<'a>(id: &str, project: &Project<'a>, out: &mut Vec<&'a Card>) {
        for &i in project.directory.members(id) {
            let card = project.nth(i);
            if card.kind == "flow" || card.kind == "action" {
                out.push(card);
            }
        }
        for inner in project.directory.sections_in(id) {
            walk(&inner, project, out);
        }
    }
    walk(&section, project, &mut out);
    Some(out)
}

/// A table's `![[…]]`: the table, not a diagram; none for any other.
fn table_embed(r: &Ref, note: &Card, project: &Project) -> Option<String> {
    if r.kind != Kind::Embed {
        return None;
    }
    let base = project.section_of(note);
    match mermaid::find_embed(&r.path, r.option.as_deref(), &base, project) {
        Ok(mermaid::Embed::Card(card)) if card.kind == crate::kinds::TABLE => Some(format!(
            "\n{}",
            crate::tools::tables::markdown(&crate::tools::tables::build(project, card))
        )),
        _ => None,
    }
}

/// One `![[…]]`, drawn: a mermaid block, or a line saying why not; a table, the paths its cells
/// name as `links` says them.
fn embed(r: &Ref, note: &Card, project: &Project, links: &dyn Fn(&str) -> String) -> String {
    let base = project.section_of(note);
    if let Some(table) = table_embed(r, note, project) {
        return links(&table);
    }
    match mermaid::embedded(&r.path, r.option.as_deref(), &base, project) {
        Ok(diagram) => format!("\n```mermaid\n{diagram}```\n"),
        Err(why) => format!("> ⚠︎ {} — {why}", code(&r.written(&plain_path(&r.path)))),
    }
}

/// How a note's text reads written out to `here`.
pub(super) fn render(
    text_in: &str,
    note: &Card,
    project: &Project,
    here: &str,
    paths: &HashMap<String, String>,
) -> String {
    let base = project.section_of(note);
    let links = |t: &str| places(t, note, project, here, paths);
    notetext::transform(text_in, |r| {
        Some(match r.kind {
            Kind::Embed => embed(r, note, project, &links),
            Kind::Value => match project_value(project, &r.path, &base) {
                Some(found) => code(&found),
                None => format!("~~{}~~", code(&r.written(&plain_path(&r.path)))),
            },
            Kind::Place => place(r, note, project, here, paths),
        })
    })
}

/// A text's `[[…]]` written out to `here` as the body's are, the rest as it is: a table's cells,
/// which hold the data's values and are not read as a body is.
fn places(
    text_in: &str,
    note: &Card,
    project: &Project,
    here: &str,
    paths: &HashMap<String, String>,
) -> String {
    notetext::transform(text_in, |r| {
        (r.kind == Kind::Place).then(|| place(r, note, project, here, paths))
    })
}

/// One `[[…]]` written out to `here`: the text it shows when one is said, else the card's label
/// or name — a link when it is written out too — or struck through when it does not resolve. A
/// section is said by its own name, a link to its page when the export writes one (`paths` holds
/// a ui section's page by the section's id).
fn place(
    r: &Ref,
    note: &Card,
    project: &Project,
    here: &str,
    paths: &HashMap<String, String>,
) -> String {
    let base = project.section_of(note);
    let plain = plain_path(&r.path);
    let said = r.shown.as_deref().map(text);
    // Whether the file or the function is there is the check's to say: written out, the path is
    // the same whatever this machine's folder holds.
    if let Some(inside) = notetext::work_dir_path(&r.path, &project.work_dir_names()) {
        return match said {
            Some(s) => format!("{s} ({})", code(&inside)),
            None => code(&inside),
        };
    }
    if let Some(name) = crate::pathtext::section_only(&notetext::exported_path(&r.path)) {
        let Some(section) = project.directory.section_from(name, &base) else {
            return format!("~~{}~~", said.unwrap_or_else(|| code(&plain)));
        };
        let shown = said.unwrap_or_else(|| text(&plain));
        return match paths.get(&section) {
            Some(page) => format!("[{shown}]({})", super::relative(here, page)),
            None => shown,
        };
    }
    if !resolves(project, &r.path, note) {
        return format!("~~{}~~", said.unwrap_or_else(|| code(&plain)));
    }
    let target = landing(&r.path, &base, project).map(|l| l.0);
    let (_, _, field) = plain_parts(&r.path);
    let shown = match said {
        Some(s) => s,
        None if field.is_none() => text(
            &target
                .map(|t| {
                    if t.label.is_empty() {
                        t.name.clone()
                    } else {
                        t.label.clone()
                    }
                })
                .unwrap_or(plain),
        ),
        None => code(&plain),
    };
    match target.and_then(|t| paths.get(&t.id).map(|there| (t, there))) {
        Some((t, there)) => format!("[{shown}]({})", link(here, there, t, project, paths)),
        None => shown,
    }
}

/// A note's or a document's Markdown as written to `here`.
pub fn markdown(
    note: &Card,
    project: &Project,
    here: &str,
    paths: &HashMap<String, String>,
) -> String {
    composed(
        note,
        project,
        &|t: &str| render(t, note, project, here, paths),
        &|t: &str| places(t, note, project, here, paths),
    )
}

/// A text of `note` as `card_read`'s `as: markdown` gives it: its paths `[[…]]` and pictures
/// `![[…]]` as written, a table's `![[…]]` the table.
pub(super) fn as_written(text_in: &str, note: &Card, project: &Project) -> String {
    notetext::transform(text_in, |r| table_embed(r, note, project))
}

/// A note's or a document's Markdown with its texts as written — its paths `[[…]]` and
/// pictures `![[…]]` left for the reader to follow (`card_read`'s `as: markdown`) — a table's
/// `![[…]]` the table, as it is written out.
pub fn markdown_as_written(note: &Card, project: &Project) -> String {
    composed(
        note,
        project,
        &|t: &str| as_written(t, note, project),
        &|t: &str| t.to_string(),
    )
}

/// One version of a stackdoc as Markdown, written out to `here`: its heading and its body.
pub fn version_markdown(
    note: &Card,
    name: &str,
    project: &Project,
    here: &str,
    paths: &HashMap<String, String>,
) -> Option<String> {
    let version = note.versions().into_iter().find(|v| v.name == name)?;
    Some(version_text(&version, &|t: &str| {
        render(t, note, project, here, paths)
    }))
}

/// One version of a stackdoc as Markdown with its texts as written (`card_read`'s
/// `as: markdown` of a version's path), a table's `![[…]]` the table.
pub fn version_as_written(note: &Card, name: &str, project: &Project) -> Option<String> {
    let version = note.versions().into_iter().find(|v| v.name == name)?;
    Some(version_text(&version, &|t: &str| {
        notetext::transform(t, |r| table_embed(r, note, project))
    }))
}

/// A version: `## <heading>` (its key when none), then its body.
fn version_text(version: &crate::model::Version, r: &dyn Fn(&str) -> String) -> String {
    let heading = format!("## {}", version.shown_heading());
    let body = r(&version.body);
    if body.trim().is_empty() {
        heading
    } else {
        format!("{heading}\n\n{}", body.trim_end())
    }
}

/// The file of one version of a stackdoc, its mark first.
pub fn version_file(
    note: &Card,
    name: &str,
    project: &Project,
    here: &str,
    paths: &HashMap<String, String>,
    mark: &str,
) -> Option<String> {
    let body = version_markdown(note, name, project, here, paths)?;
    Some(format!("{}\n\n{body}\n", charts::marker(mark)))
}

/// A note's or a document's Markdown, each text of it as `r` gives it. A stackdoc: its title,
/// what it is, then every version in its order (`specboard://SpecBoard/Docs/Design:CardKinds`「積み重ね文書」).
fn composed(
    note: &Card,
    project: &Project,
    r: &dyn Fn(&str) -> String,
    links: &dyn Fn(&str) -> String,
) -> String {
    let title = if note.label.is_empty() {
        &note.name
    } else {
        &note.label
    };
    if note.kind == "stackdoc" {
        let mut parts = vec![format!("# {title}")];
        if !note.description.trim().is_empty() {
            parts.push(r(&note.description).trim_end().to_string());
        }
        parts.extend(note.versions().iter().map(|v| version_text(v, r)));
        return parts.join("\n\n");
    }
    if note.kind != "formatdoc" {
        return r(&note.body);
    }
    let body = rows_markdown(&note.rows, true, &mut |row| {
        row_text(row, note, project, r, links)
    });
    format!("# {title}\n\n{body}")
}

/// One item of a formatdoc (or one row) as Markdown with its texts as written (`card_read`'s
/// `as: markdown` of an item's path): its part of the document's Markdown, character for
/// character — its heading and every row inside it. None when no row is at `names`.
pub fn row_as_written(note: &Card, names: &[String], project: &Project) -> Option<String> {
    let r = |t: &str| as_written(t, note, project);
    let links = |t: &str| t.to_string();
    row_markdown(&note.rows, names, true, &mut |row| {
        row_text(row, note, project, &r, &links)
    })
}

/// What a formatdoc's row other than an item says as Markdown, each text of it as `r` gives it.
fn row_text(
    row: &FormatRow,
    note: &Card,
    project: &Project,
    r: &dyn Fn(&str) -> String,
    links: &dyn Fn(&str) -> String,
) -> Option<String> {
    let base = project.section_of(note);
    match row.kind.as_str() {
        "item" => None,
        "text" => Some(r(&row.body)),
        "table" => Some(table(&row.columns, &row.cells, r)),
        "diagram" => Some(r(&format!("![[{}]]", row.target))),
        "diagrams" => Some(match charts_in(&row.target, &base, project) {
            None => format!("~~{}~~", code(&plain_path(&row.target))),
            Some(found) => found
                .iter()
                .map(|c| {
                    format!("**{}**\n\n", text(&c.name)) + &r(&format!("![[{}]]", project.full(c)))
                })
                .collect::<Vec<_>>()
                .join("\n\n"),
        }),
        // The way to the card above what it holds as a table, so the table says which card it
        // is; a card of no table, the way to it alone.
        "card" => Some(match project.find(&row.target, &base) {
            None => format!("~~{}~~", code(&plain_path(&row.target))),
            Some(card) => {
                let at = links(&format!("[[{}]]", project.full(card)));
                match card_table(card, project, r, links) {
                    Some(t) => format!("{at}\n\n{t}"),
                    None => at,
                }
            }
        }),
        _ => None,
    }
}

/// Where a text's front matter ends — it opens with a `---` line and the next `---` line closes
/// it — just after that line; none when it opens otherwise.
fn front_matter_end(text: &str) -> Option<usize> {
    let line = |l: &str| l.trim_end_matches(['\r', ' ', '\t']) == "---";
    let first = text.split('\n').next()?;
    if !line(first) {
        return None;
    }
    let mut at = first.len() + 1;
    while at <= text.len() {
        let rest = &text[at..];
        let this = rest.split('\n').next().unwrap_or_default();
        if line(this) {
            return Some((at + this.len() + 1).min(text.len()));
        }
        if this.len() == rest.len() {
            return None;
        }
        at += this.len() + 1;
    }
    None
}

/// The file of one note or document, its mark first — or, when it opens with front matter (a
/// `SKILL.md`'s), right after it, so the front matter stays the head of the file and is read as
/// such.
pub fn file(
    note: &Card,
    project: &Project,
    here: &str,
    paths: &HashMap<String, String>,
    mark: &str,
) -> String {
    let body = markdown(note, project, here, paths);
    let end = if body.ends_with('\n') { "" } else { "\n" };
    let Some(at) = front_matter_end(&body) else {
        return format!("{}\n\n{body}{end}", charts::marker(mark));
    };
    let (head, rest) = body.split_at(at);
    let head = if head.ends_with('\n') {
        head.to_string()
    } else {
        format!("{head}\n")
    };
    let rest = rest.trim_start_matches(['\n', '\r']);
    if rest.is_empty() {
        return format!("{head}\n{}\n", charts::marker(mark));
    }
    format!("{head}\n{}\n\n{rest}{end}", charts::marker(mark))
}

#[cfg(test)]
mod tests {
    use super::front_matter_end;

    #[test]
    fn front_matter_is_a_text_s_first_lines_between_two_dashed_lines() {
        let text = "---\nname: x\n---\n\n# 本文\n";
        assert_eq!(front_matter_end(text), Some("---\nname: x\n---\n".len()));
        assert_eq!(front_matter_end("---\nname: x\n---"), Some(15));
        assert_eq!(front_matter_end("---  \r\na: 1\n---\r\nb"), Some(17));
        // Not opened by one, or never closed.
        assert_eq!(front_matter_end("# 見出し\n---\na\n---\n"), None);
        assert_eq!(front_matter_end("---\nname: x\n"), None);
        assert_eq!(front_matter_end("----\na\n---\n"), None);
        assert_eq!(front_matter_end(""), None);
    }
}
