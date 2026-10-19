//! Cards written (`buildSchemas`, `createSchema` and the checks around them in the Swift
//! app), in the kinds' form: what a call writes is read from MCP's shape into a card's
//! document (`kinds::convert::from_wire`), checked by the kind's definition, and put
//! where the tree keeps it. What the history says and how a refusal is worded are the
//! Swift app's.

use super::args::Reason;
use crate::export::Project;
use crate::kinds::Names;
use crate::model::Detail;
use crate::write::Tx;
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};

/// The tree a call writes in.
#[derive(Clone, Debug)]
pub(crate) struct Scope {
    pub project: String,
    pub project_name: String,
    /// `""` for main.
    pub tree: String,
    pub tree_name: String,
}

impl Scope {
    fn branch(&self) -> Option<&str> {
        (!self.tree.is_empty()).then_some(self.tree.as_str())
    }

    /// The project as the transaction sees it now, in this tree.
    pub fn detail(&self, tx: &Tx) -> Result<Detail, String> {
        crate::model::typed(&crate::detail::read(
            &tx.reader(),
            &self.project,
            self.branch(),
        )?)
    }

    /// main, as the board's lines reach it (`Project::outside`): read for the board alone.
    pub fn outside(&self, tx: &Tx) -> Result<Option<Detail>, String> {
        if self.tree != crate::store::BOARD {
            return Ok(None);
        }
        Ok(Some(crate::model::typed(&crate::detail::read(
            &tx.reader(),
            &self.project,
            None,
        )?)?))
    }

    /// One card's document as this tree sees it, as `docs` has it: from the project the
    /// transaction keeps worked out (`Seen::in_tx`), not every card's read again.
    pub fn doc(&self, tx: &Tx, id: &str) -> Result<Option<Map<String, Value>>, String> {
        Ok(super::Seen::in_tx(tx, self)?.docs.get(id).cloned())
    }

    /// Every card's document as this tree sees it, by id.
    pub fn docs(&self, tx: &Tx) -> Result<HashMap<String, Map<String, Value>>, String> {
        Ok(tx
            .reader()
            .compose(&self.project, self.branch())?
            .cards
            .into_iter()
            .map(|i| (i.id, i.doc))
            .collect())
    }
}

/// A section kind as people call it (the definition's `label`).
pub(crate) fn section_label(defs: &crate::kinds::Defs, kind: &str) -> String {
    defs.section_label(kind)
}

/// The kinds a card may be made of (`SchemaKind.made`); a card ghost too, and a flow's nodes
/// (cards inside it).
pub(crate) const MADE_KINDS: [&str; 24] = [
    "entity",
    "value",
    "enum",
    "note",
    "flow",
    "store",
    "freedoc",
    "formatdoc",
    "stackdoc",
    "instance",
    "table",
    "svg",
    "html",
    "frame",
    "goal",
    "task",
    "ghost",
    "begin",
    "process",
    "run",
    "eval",
    "wait",
    "join",
    "end",
];

/// A kind named by a call (`madeKind`): a built-in one, or one the project defines
/// (`projects.kinds.cards`).
pub(crate) fn made_kind(raw: &str, project: &Project) -> Result<String, String> {
    if raw == "action" {
        return Err(super::action_gone());
    }
    let raw = if raw == "custom_goal" { "goal" } else { raw };
    if MADE_KINDS.contains(&raw) || project.is_own_kind(raw) {
        return Ok(raw.to_string());
    }
    let own: Vec<String> = project
        .defs
        .cards()
        .placeable()
        .filter(|k| !k.builtin)
        .map(|k| k.name.clone())
        .collect();
    Err(text!(
        "cards.kind.oneOf",
        kinds = MADE_KINDS.join(" / "),
        own = if own.is_empty() {
            String::new()
        } else {
            text!("cards.kind.ownKinds", kinds = own.join(" / "))
        }
    ))
}

/// A name a card, a card ghost or a field can have: the rule of the names a path is made of
/// (`pathtext::name_problem`), its surrounding whitespace dropped.
pub(crate) fn identifier(raw: &str, field: &str) -> Result<String, String> {
    let value = raw.trim().to_string();
    match crate::pathtext::name_problem(&value) {
        None => Ok(value),
        Some(crate::pathtext::NameProblem::Empty) => Err(text!("cards.field.required", field)),
        Some(crate::pathtext::NameProblem::TooLong) => Err(text!(
            "cards.field.tooLong",
            field,
            max = crate::pathtext::NAME_MAX
        )),
        Some(_) => Err(text!("cards.field.nameControl", field)),
    }
}

/// Text held to a length (`Validate.text`).
pub(crate) fn text(raw: &str, field: &str, max: usize) -> Result<String, String> {
    let value = raw.trim().to_string();
    if value.chars().count() > max {
        return Err(text!("cards.field.tooLong", field, max));
    }
    Ok(value)
}

/// Why no card of `kind` is made in this project, or none: the project has a section for it (`project_tree`
/// and what is made inside its sections). One the app alone draws is never
/// made. A card of a kind with no place left keeps being read and written where it is.
pub(crate) fn require_home(project: &Project, kind: &str) -> Result<(), String> {
    if crate::kinds::system_only(kind) {
        let label = project
            .defs
            .cards()
            .get(kind)
            .map_or_else(|| kind.to_string(), |k| k.label.clone());
        return Err(text!("cards.home.appDraws", kind, label));
    }
    if kind == crate::detail::GHOST || crate::kinds::inner_only(kind) {
        return Ok(());
    }
    if project.defs.home_of(kind).is_none() {
        return Err(no_home(kind));
    }
    Ok(())
}

/// What a card of a kind no section of the project holds is told.
fn no_home(kind: &str) -> String {
    text!("cards.home.none", kind)
}

/// Whether a card may be made in `section`, or moved there: not in the root's
/// `_evacuation_` (what `project_tree` let go of waits there to be moved out).
pub(crate) fn require_not_evacuation(project: &Project, section: &str) -> Result<(), String> {
    let full = project.directory.full_section(section);
    let evacuation = crate::kinds::EVACUATION;
    if full == evacuation || full.starts_with(&format!("{evacuation}/")) {
        return Err(text!("cards.evacuation.refused", full, evacuation));
    }
    Ok(())
}

/// Whether a card of `kind` may go in `section`, said as the Swift app says it
/// (`requireFits`): the section's kind holds it (`holds` of its definition).
pub(crate) fn require_fits(project: &Project, kind: &str, section: &str) -> Result<(), String> {
    // A kind no section holds goes only inside a container (a flow's nodes).
    if crate::kinds::inner_only(kind) {
        return Err(super::containers::only_inside(kind));
    }
    let defs = &project.defs;
    let home = defs.home_of(kind);
    let candidates: Vec<String> = project
        .detail
        .sections
        .iter()
        .filter(|s| home.is_some() && s.kind == home)
        .take(3)
        .map(|s| project.directory.full_section(&s.id))
        .collect();
    let go_to = |example: &str| match &home {
        Some(home) => text!(
            "cards.fits.putIn",
            kind,
            label = section_label(defs, home),
            example
        ),
        None => no_home(kind),
    };
    let example = if candidates.is_empty() {
        String::new()
    } else {
        text!("cards.fits.example", paths = candidates.join(" / "))
    };
    let Some(s) = project.detail.sections.iter().find(|s| s.id == section) else {
        return Err(text!("cards.fits.noSection"));
    };
    // A card ghost goes in any section but the root.
    if kind == crate::detail::GHOST {
        if s.parent_id.is_none() {
            return Err(text!("cards.fits.rootGhost", name = s.name));
        }
        return Ok(());
    }
    if s.parent_id.is_none() {
        return Err(text!(
            "cards.fits.rootCard",
            name = s.name,
            hint = go_to(&example)
        ));
    }
    let Some(held) = s.kind.as_deref() else {
        return Ok(());
    };
    if defs.holds(held, kind) {
        return Ok(());
    }
    Err(text!(
        "cards.fits.notHeld",
        section = project.directory.full_section(section),
        label = section_label(defs, held),
        kind,
        hint = go_to(&example)
    ))
}

/// Names in a write, read as the card they are written in reads them: from its section,
/// and among the cards the same call makes.
pub(crate) struct Resolver<'p, 'a> {
    pub project: &'p Project<'a>,
    /// The section names are read from.
    pub base: String,
    /// The cards the call makes: their full paths, by id.
    pub made: &'p HashMap<String, String>,
    /// The container the card written is inside, by its full path: a name alone is read
    /// first among the cards beside it there (a flow's node names its next by name).
    pub inside: Option<String>,
    /// The cards the call was to make and could not, by their full paths: a path to one of them is
    /// refused as that card's refusal (`Names::unmade`).
    pub unmade: Option<&'p HashSet<String>>,
}

impl Resolver<'_, '_> {
    /// A name alone as a card beside the one written, inside the same container.
    fn sibling(&self, path: &str) -> Option<String> {
        let container = self.inside.as_deref()?;
        if !crate::pathtext::is_one_name(path) {
            return None;
        }
        let full = crate::pathtext::member(container, &crate::pathtext::unquote(path));
        self.made
            .iter()
            .find(|(_, p)| **p == full)
            .map(|(id, _)| id.clone())
            .or_else(|| self.project.written_id(&full, &self.base))
    }

    /// A name as the full path of a card made in this call, if it is one.
    fn made_id(&self, path: &str) -> Option<String> {
        let full = self.full(path)?;
        self.made
            .iter()
            .find(|(_, p)| **p == full)
            .map(|(id, _)| id.clone())
    }

    /// A path in full as the card written reads it: its section's full path (this one's when it
    /// has none), then the names in it.
    fn full(&self, path: &str) -> Option<String> {
        Some(match crate::pathtext::split_section(path) {
            (Some(section), name) => {
                let s = self
                    .project
                    .directory
                    .section_named(section.trim(), Some(&self.base))?;
                format!(
                    "{}:{}",
                    self.project.directory.full_section(&s),
                    crate::pathtext::canonical_names(name)
                )
            }
            (None, name) => format!(
                "{}:{}",
                self.project.directory.full_section(&self.base),
                crate::pathtext::canonical_names(name)
            ),
        })
    }
}

impl Names for Resolver<'_, '_> {
    /// As written: a card ghost by its own path.
    fn path(&self, id: &str) -> Option<String> {
        self.made
            .get(id)
            .cloned()
            .or_else(|| self.project.named(id))
    }

    /// As written: a name that is a card ghost is held by the ghost's id (when it leads to
    /// a card).
    fn id(&self, path: &str) -> Option<String> {
        self.sibling(path)
            .or_else(|| self.made_id(path))
            .or_else(|| self.project.written_id(path, &self.base))
    }

    /// A name alone beside the card written in its container, or a path read from its section.
    fn unmade(&self, path: &str) -> Option<String> {
        let unmade = self.unmade?;
        if let Some(container) = &self.inside
            && crate::pathtext::is_one_name(path)
        {
            let beside = crate::pathtext::member(container, &crate::pathtext::unquote(path));
            if unmade.contains(&beside) {
                return Some(beside);
            }
        }
        self.full(path).filter(|full| unmade.contains(full))
    }

    fn section_path(&self, id: &str) -> Option<String> {
        let directory = &self.project.directory;
        directory
            .has_section(id)
            .then(|| directory.full_section(id))
    }

    fn section_id(&self, path: &str) -> Option<String> {
        self.project.directory.section_named(path, Some(&self.base))
    }
}

/// A record's fields as MCP lists them — one level, a nested one by its dotted name — put
/// into rows under their parents (`buildFields`), shallowest first. Refused, with where,
/// when a parent is missing or is not an object.
pub(crate) fn nest_fields(label: &str, items: &[Value]) -> Result<Vec<Value>, Vec<Reason>> {
    let mut specs: Vec<(usize, Map<String, Value>)> = Vec::new();
    for (i, item) in items.iter().enumerate() {
        match item.as_object() {
            Some(o) => specs.push((i, o.clone())),
            None => {
                return Err(vec![Reason {
                    at: Some(label.to_string()),
                    text: text!("cards.fields.notObject", i),
                }]);
            }
        }
    }
    let depth = |o: &Map<String, Value>| {
        crate::pathtext::dots(o.get("name").and_then(Value::as_str).unwrap_or("")).len()
    };
    specs.sort_by_key(|(i, o)| (depth(o), *i));
    // Each field as a row, kept by its dotted name, under its parent.
    let mut rows: Vec<(String, Map<String, Value>, Vec<usize>)> = Vec::new();
    let mut top: Vec<usize> = Vec::new();
    for (_, mut entry) in specs {
        let Some(relative) = entry
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_string)
            .filter(|n| !n.is_empty())
        else {
            return Err(vec![Reason {
                at: Some(label.to_string()),
                text: text!("cards.fields.nameNeeded"),
            }]);
        };
        let path = format!("{label}.{relative}");
        // A nested field by its dotted name, a name holding a dot quoted (`'e.mail'`).
        let segments: Vec<String> = crate::pathtext::names(&relative);
        let relative = crate::pathtext::join_names(segments.iter().map(String::as_str));
        if segments.iter().any(|s| s.is_empty()) {
            return Err(vec![Reason {
                at: Some(path),
                text: text!("cards.fields.emptySegment"),
            }]);
        }
        let mut parent_at = None;
        if segments.len() > 1 {
            let parent = crate::pathtext::join_names(
                segments[..segments.len() - 1].iter().map(String::as_str),
            );
            let Some(at) = rows.iter().position(|r| r.0 == parent) else {
                return Err(vec![Reason {
                    at: Some(path),
                    text: text!("cards.fields.parentMissing", label, parent),
                }]);
            };
            let t = rows[at]
                .1
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("string")
                .to_string();
            if t != "object" {
                return Err(vec![Reason {
                    at: Some(path),
                    text: text!("cards.fields.parentNotObject", label, parent, t),
                }]);
            }
            parent_at = Some(at);
        }
        // An embedding is a line of the card's (`connections`), not a type.
        let kind = entry
            .get("type")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| "string".to_string());
        if kind == "ref" || entry.contains_key("ref") || entry.contains_key("pointsAt") {
            return Err(vec![Reason {
                at: Some(path),
                text: super::lines::refuse_field(&entry)
                    .err()
                    .unwrap_or_else(super::lines::ref_type),
            }]);
        }
        entry.insert("type".into(), kind.into());
        entry.insert("name".into(), segments[segments.len() - 1].clone().into());
        let at = rows.len();
        rows.push((relative, entry, Vec::new()));
        match parent_at {
            Some(p) => rows[p].2.push(at),
            None => top.push(at),
        }
    }
    fn built(rows: &[(String, Map<String, Value>, Vec<usize>)], at: usize) -> Value {
        let (_, entry, children) = &rows[at];
        let mut out = entry.clone();
        if !children.is_empty() {
            out.insert(
                "fields".into(),
                Value::Array(children.iter().map(|c| built(rows, *c)).collect()),
            );
        }
        Value::Object(out)
    }
    Ok(top.iter().map(|t| built(&rows, *t)).collect())
}

/// The next place among the project's cards (`nextSchemaOrder`).
pub(crate) fn next_order(docs: &HashMap<String, Map<String, Value>>) -> i64 {
    // A document keeps no order of 0: it is one all the same.
    docs.values()
        .map(|d| d.get("order").and_then(Value::as_i64).unwrap_or(0))
        .max()
        .map_or(0, |n| n + 1)
}

/// What an extra of a card is called in its history line (`updateSchema`'s `changed`).
pub(crate) fn changed_label(key: &str, value: &Value) -> Option<crate::text::Keyed> {
    Some(match key {
        "description" => keyed!("common.label.description"),
        "tags" => keyed!("cards.changed.tags"),
        "metadata" => keyed!("cards.changed.metadata"),
        "rules" => keyed!("cards.changed.rules"),
        "absentFields" => keyed!("cards.changed.absentFields"),
        "connections" => keyed!("cards.changed.connections"),
        "discriminator" => keyed!("cards.changed.discriminator"),
        "open" => {
            if value.as_bool() == Some(true) {
                keyed!("cards.changed.openEnum")
            } else {
                keyed!("cards.changed.closedEnum")
            }
        }
        "fields" => keyed!("cards.changed.fields"),
        "values" => keyed!("cards.changed.values"),
        "versions" => keyed!("cards.changed.versions"),
        "body" => keyed!("cards.changed.body"),
        "rows" => keyed!("cards.changed.rows"),
        "nodes" => keyed!("cards.changed.nodes"),
        "diagram" => keyed!("cards.changed.diagram"),
        "label" => keyed!("common.label.displayName"),
        "of" => keyed!("cards.changed.of"),
        "data" => keyed!("cards.changed.data"),
        _ => return None,
    })
}

#[cfg(test)]
mod order_tests {
    use super::next_order;
    use serde_json::{Map, Value, json};
    use std::collections::HashMap;

    fn doc(order: Option<i64>) -> Map<String, Value> {
        let mut out = Map::new();
        if let Some(n) = order {
            out.insert("order".into(), json!(n));
        }
        out
    }

    #[test]
    fn a_card_made_comes_after_the_last_one_made_the_first_at_none() {
        let mut docs = HashMap::new();
        assert_eq!(next_order(&docs), 0);
        // The first keeps no order (0); the next still comes after it.
        docs.insert("a".to_string(), doc(None));
        assert_eq!(next_order(&docs), 1);
        docs.insert("b".to_string(), doc(Some(1)));
        assert_eq!(next_order(&docs), 2);
    }
}
