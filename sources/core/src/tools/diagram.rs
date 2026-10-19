//! A diagram read through the tools: `branch_read`, `section_read` and `card_read` with
//! `as: diagram`, handed on to one operation (`get_diagram`) — the mermaid of the whole
//! tree, of a section with what it touches, or of the cards around some, cut as the value
//! says; a flow drawn as its chart; or, with `revisionOnly`, only the rev and what it was
//! asked. The answer is before `Seen::headed` names the branch in its first line.
//!
//! Through MCP a class diagram is given in the characters a list is (`paging::MOST_CHARS`): one that
//! does not fit is drawn smaller — the cards around some a hop less at a time down to one, else every
//! diagram a section to a box (`mermaid::render_sections`) — headed with what it was in place of
//! (`mermaid::Cut`). A flow's chart, a card map, `revisionOnly` and the window's requests are as they
//! were.

use super::args::Args;
use super::walk::{Target, locate};
use super::{Seen, capped};
use crate::export::Project;
use crate::export::charts;
use crate::export::datachart::{self, Scope};
use crate::export::mermaid::{self, Direction, Spec, Subgraph};
use crate::mcp::Output;
use serde_json::{Map, Value, json};

/// What a diagram is of, as the tool that asks says it.
#[derive(Clone, Copy, Debug)]
pub(crate) enum At<'a> {
    /// The whole tree read (`branch_read`'s `as: diagram`).
    Project,
    /// A section, by its path (`section_read`'s target).
    Section(&'a str),
    /// The cards around these (`card_read`'s targets); one flow is drawn as its chart.
    Cards(&'a [String]),
}

impl At<'_> {
    fn tool(self) -> &'static str {
        match self {
            At::Project => "branch_read",
            At::Section(_) => "section_read",
            At::Cards(_) => "card_read",
        }
    }

    /// The label of the tool's form for `as: diagram`.
    fn form(self) -> &'static str {
        match self {
            At::Project => "branch（as: diagram）",
            At::Section(_) => "section（as: diagram）",
            At::Cards(_) => "as: diagram",
        }
    }
}

/// What a diagram is given in through MCP: `most` characters of its mermaid (`mermaid`, the head
/// said apart as keys).
#[derive(Clone, Copy, Debug)]
struct Room {
    most: usize,
}

impl Room {
    /// The characters `text` takes as the answer's mermaid: under its head.
    fn size(self, text: &str) -> usize {
        let mut rest = text;
        while rest.starts_with("%% ") {
            rest = rest.split_once('\n').map_or("", |(_, r)| r);
        }
        rest.chars().count()
    }

    fn fits(self, text: &str) -> bool {
        self.size(text) <= self.most
    }
}

/// The diagram `given` (the call's value, `as` and all) asks for, of `at` — a class diagram in `most`
/// characters when it is said (through MCP), drawn smaller when it does not fit — as JSON: what its
/// head says of it, each by its key, and the mermaid (`mermaid`); with `revisionOnly`, the rev and
/// what it was asked alone.
pub(crate) fn diagram(
    seen: &Seen,
    given: &Map<String, Value>,
    at: At,
    most: Option<usize>,
) -> Result<Output, String> {
    Ok(match drawn(seen, given, at, most)? {
        Output::Blocks(texts) => Output::Json(said(&seen.summary.name, &texts.join("\n"))),
        other => other,
    })
}

/// The keys a diagram's head says, in the order they are said; a value runs on to the next of them.
const HEAD_KEYS: [&str; 18] = [
    "project",
    "roots",
    "depth",
    "kinds",
    "tags",
    "section",
    "fields",
    "direction",
    "truncated",
    "chars",
    "cards",
    "rev",
    "map",
    "flow",
    "action",
    "shape",
    "data",
    "field",
];

/// A diagram drawn (`text`, under its head) as JSON: the project by its name, what the head says
/// (`depth=2` a number, `fields=false` a truth, `kinds=a,b` a list, `hint` on a line of its own), and
/// the mermaid under it.
fn said(project: &str, text: &str) -> Value {
    let mut out = Map::new();
    out.insert("project".into(), project.into());
    let mut body = text;
    loop {
        let (line, rest) = body.split_once('\n').unwrap_or((body, ""));
        if let Some(hint) = line.strip_prefix("%% hint=") {
            out.insert("hint".into(), hint.into());
        } else if let Some(head) = line.strip_prefix(super::DIAGRAM_MARK) {
            let mut pairs: Vec<(String, String)> = Vec::new();
            for word in head.split(' ') {
                match word.split_once('=') {
                    Some((key, value)) if HEAD_KEYS.contains(&key) => {
                        pairs.push((key.to_string(), value.to_string()))
                    }
                    _ => {
                        if let Some((_, value)) = pairs.last_mut() {
                            value.push(' ');
                            value.push_str(word);
                        }
                    }
                }
            }
            for (key, value) in pairs {
                let typed = match key.as_str() {
                    // The project said by its name above, as every answer says it.
                    "project" => continue,
                    "depth" | "chars" | "cards" => value
                        .parse::<u64>()
                        .map_or_else(|_| value.clone().into(), Value::from),
                    "fields" | "truncated" => Value::Bool(value == "true"),
                    "kinds" | "tags" => Value::Array(
                        value
                            .split(',')
                            .filter(|k| !k.is_empty())
                            .map(Value::from)
                            .collect(),
                    ),
                    _ => value.into(),
                };
                out.insert(key, typed);
            }
        } else {
            break;
        }
        body = rest;
    }
    out.insert("mermaid".into(), body.into());
    Value::Object(out)
}

/// The diagram `diagram` gives, as it is drawn: the mermaid under its head.
fn drawn(
    seen: &Seen,
    given: &Map<String, Value>,
    at: At,
    most: Option<usize>,
) -> Result<Output, String> {
    super::check(given, at.tool(), at.form())?;
    let room = most.map(|most| Room { most });
    let project = seen.project();
    let mut args = given.clone();
    args.remove("as");
    match at {
        At::Project => {}
        At::Section(path) => {
            if let Ok(id) = section(&project, path)
                && datachart::holds_data(&project, &id)
            {
                return data(&project, &args, Scope::Section(id));
            }
            args.insert("section".into(), path.into());
        }
        At::Cards(list) => {
            let base = base(&project, &args)?;
            if let [one] = list
                && let Target::Card(card) = locate(&project, one, base.as_deref())?
            {
                if card.kind == "flow" || card.kind == "action" {
                    return chart(&project, &args, one);
                }
                if card.kind == crate::kinds::INSTANCE {
                    return data(&project, &args, Scope::Card(card));
                }
                // A container (a goal, a container kind the project defines): its card map.
                if project
                    .defs
                    .cards()
                    .get(&card.kind)
                    .is_some_and(|k| !k.holds.is_empty())
                {
                    return card_map(&project, &args, card);
                }
            }
            if args.contains_key("shape") {
                return Err(text!("diagram.shape.flowOnly"));
            }
            args.insert("roots".into(), json!(list));
        }
    }
    if args.contains_key("field") {
        return Err(text!("diagram.field.dataOnly"));
    }
    relations(&project, &args, at, room)
}

/// Data drawn as the flowchart a document's `![[…]]` puts in: its boxes with `field`'s values
/// beside their names when it is said.
fn data(project: &Project, args: &Map<String, Value>, scope: Scope) -> Result<Output, String> {
    let others: Vec<&str> = [
        "roots",
        "depth",
        "kinds",
        "tags",
        "fields",
        "direction",
        "shape",
    ]
    .into_iter()
    .filter(|k| args.contains_key(*k))
    .collect();
    if !others.is_empty() {
        return Err(text!(
            "diagram.data.unused",
            keys = crate::text::List::of(&others)
        ));
    }
    let field = match args.get("field") {
        None => None,
        Some(Value::String(f)) => Some(f.as_str()),
        Some(_) => return Err(text!("diagram.data.fieldString")),
    };
    if Args::new(args).bool("revisionOnly", false) {
        let mut spec = json!({ "data": datachart::named(project, &scope) });
        if let Some(f) = field {
            spec["field"] = f.into();
        }
        return Ok(Output::Json(json!({
            "rev": datachart::revision(project, &scope, field)?,
            "spec": spec,
        })));
    }
    Ok(Output::Blocks(vec![
        project
            .detail
            .project
            .in_export_language(|| datachart::render(project, &scope, field, false))?,
    ]))
}

/// The section a call reads bare names from: none when it names none (`base`).
fn base(project: &Project, args: &Map<String, Value>) -> Result<Option<String>, String> {
    match args.get("section").and_then(Value::as_str) {
        None => Ok(None),
        Some(name) => section(project, name).map(Some),
    }
}

/// A section by its path, as a call names it (`MCPTools.section`).
fn section(project: &Project, name: &str) -> Result<String, String> {
    let trimmed = name.trim_matches(|c: char| c == ' ' || c == '\t');
    if let Some(id) = project.directory.section_named(trimmed, None) {
        return Ok(id);
    }
    if project.detail.sections.is_empty() {
        if crate::pathtext::same(name.trim(), "main") {
            return Ok(project.directory.home.clone());
        }
        return Err(text!("diagram.section.missing", name));
    }
    Err(project
        .directory
        .section_problem(name, &project.directory.home))
}

/// A flow drawn as its chart: in its own shape unless `shape` asks for the other;
/// an action as its sequence diagram.
fn chart(project: &Project, args: &Map<String, Value>, name: &str) -> Result<Output, String> {
    let others: Vec<&str> = [
        "roots",
        "depth",
        "kinds",
        "tags",
        "fields",
        "direction",
        "field",
    ]
    .into_iter()
    .filter(|k| args.contains_key(*k))
    .collect();
    if !others.is_empty() {
        return Err(text!(
            "diagram.chart.unused",
            keys = crate::text::List::of(&others)
        ));
    }
    let base = base(project, args)?;
    let Some(found) = project.directory.find(name, base.as_deref()) else {
        let names: Vec<String> = project
            .detail
            .schemas
            .iter()
            .map(|c| c.name.clone())
            .collect();
        return Err(text!(
            "diagram.chart.at",
            name,
            why = project
                .directory
                .find_refusal(name, base.as_deref(), &names)
        ));
    };
    let card = project.nth(found);
    let asked = args.get("shape").and_then(Value::as_str);
    let shape = match card.kind.as_str() {
        "flow" => {
            let shape = asked.unwrap_or(card.diagram_shape());
            if shape != "flowchart" && shape != "sequence" {
                return Err(text!("diagram.chart.shapeOf", shape));
            }
            shape
        }
        "action" => {
            if let Some(shape) = asked.filter(|s| *s != "sequence") {
                return Err(text!("diagram.chart.actionShape", shape));
            }
            "sequence"
        }
        other => {
            return Err(text!(
                "diagram.chart.notFlow",
                card = project.full(card),
                kind = other
            ));
        }
    };
    if Args::new(args).bool("revisionOnly", false) {
        return Ok(Output::Json(json!({
            "rev": charts::revision_in(card, project, shape),
            "spec": { "chart": project.exported(card), "shape": shape },
        })));
    }
    Ok(Output::Blocks(vec![
        project
            .detail
            .project
            .in_export_language(|| charts::render_drawn(card, project, shape)),
    ]))
}

/// A container drawn as its card map: the cards it holds and the lines
/// between them, of the kinds `kinds` names (every kind when none).
fn card_map(
    project: &Project,
    args: &Map<String, Value>,
    card: &crate::model::Card,
) -> Result<Output, String> {
    let others: Vec<&str> = [
        "roots",
        "depth",
        "tags",
        "fields",
        "direction",
        "shape",
        "revisionOnly",
        "field",
    ]
    .into_iter()
    .filter(|k| args.contains_key(*k))
    .collect();
    if !others.is_empty() {
        return Err(text!(
            "diagram.map.unused",
            kind = card.kind,
            keys = crate::text::List::of(&others)
        ));
    }
    let kinds = Args::new(args).strings("kinds").unwrap_or_default();
    let head = format!(
        "%% specboard project={} map={}",
        project.detail.project.slug,
        project.exported(card)
    );
    Ok(Output::Blocks(vec![
        project
            .detail
            .project
            .in_export_language(|| mermaid::card_map(project, card, &kinds, &head)),
    ]))
}

/// The class diagram the value cuts, checked to draw what it was asked (`get_diagram`); in `room`,
/// when it is said, else drawn smaller (`smaller`).
fn relations(
    project: &Project,
    args: &Map<String, Value>,
    at: At,
    room: Option<Room>,
) -> Result<Output, String> {
    let spec = spec(project, args)?;
    let graph = mermaid::select(project, &spec);
    refuse_roots(project, &spec, &graph)?;
    if Args::new(args).bool("revisionOnly", false) {
        return Ok(Output::Json(revision_view(project, &spec, &graph)));
    }
    let text = project
        .detail
        .project
        .in_export_language(|| mermaid::render_drawn(project, &spec, &graph, None));
    Ok(Output::Blocks(vec![match room {
        Some(room) if !room.fits(&text) => smaller(project, &spec, &graph, at, room, &text),
        _ => text,
    }]))
}

/// A class diagram too big for its `room` (`text` as it was drawn) drawn smaller: the cards around
/// some (its roots: `card_read`'s targets, `section_read`'s `roots`) a hop less at a time, down to
/// one, in the first that fits; else, and every other diagram, a section to a box. Headed with the
/// rev, the characters and the cards of the one asked, and what to ask to draw less.
fn smaller(
    project: &Project,
    spec: &Spec,
    graph: &Subgraph,
    at: At,
    room: Room,
    text: &str,
) -> String {
    let rev = mermaid::revision(project, spec, graph);
    let cut = |hint: String| mermaid::Cut {
        rev: rev.clone(),
        chars: room.size(text),
        cards: graph.cards(),
        hint,
    };
    if !spec.roots.is_empty() {
        for depth in (1..spec.depth).rev() {
            let lower = Spec {
                depth,
                ..spec.clone()
            };
            let drawn = mermaid::select(project, &lower);
            let hint = lowered(at, spec, depth);
            let text = project.detail.project.in_export_language(|| {
                mermaid::render_drawn(project, &lower, &drawn, Some(&cut(hint)))
            });
            if room.fits(&text) {
                return text;
            }
        }
    }
    let hint = cut(boxed(at, spec));
    project
        .detail
        .project
        .in_export_language(|| mermaid::render_sections(project, spec, graph, &hint, room.most))
}

// What a diagram drawn smaller says it was drawn so for, and how to ask for less: said to the caller,
// in the call's language, though the diagram is in the project's export language.

/// The characters a diagram is given in, in words (`4 万字`).
fn limit_said() -> String {
    text!(
        "diagram.room.limit",
        count = super::paging::MOST_CHARS / 10_000
    )
}

/// Where a call of `at` names the cards a diagram starts from.
fn roots_said(at: At) -> String {
    match at {
        At::Section(_) => text!("diagram.room.rootsOfValue"),
        _ => "target".to_string(),
    }
}

/// The ways to draw less every diagram drawn smaller says first.
fn narrower(spec: &Spec) -> Vec<String> {
    let mut ways = vec![text!("diagram.room.narrowByTags")];
    if spec.fields {
        ways.push(text!("diagram.room.noFields"));
    }
    ways
}

/// What a diagram of the cards around some, drawn `depth` hops out in place of more, says.
fn lowered(at: At, spec: &Spec, depth: usize) -> String {
    text!(
        "diagram.room.lowered",
        asked = spec.depth,
        limit = limit_said(),
        depth,
        ways = narrower(spec).concat(),
        roots = roots_said(at)
    )
}

/// What a diagram of a section to a box says, drawn in place of the one of `at`.
fn boxed(at: At, spec: &Spec) -> String {
    let mut ways = narrower(spec);
    let roots = !spec.roots.is_empty();
    let last = match at {
        At::Project => text!("diagram.room.readSectionOrCards"),
        At::Section(_) if !roots => text!("diagram.room.readCards"),
        At::Section(_) => text!("diagram.room.fewerRoots", roots = roots_said(at)),
        At::Cards(_) => {
            ways.push(text!("diagram.room.fewerTargets"));
            text!("diagram.room.readSection")
        }
    };
    let even = if roots && spec.depth >= 1 {
        text!("diagram.room.evenAtDepthOne")
    } else {
        String::new()
    };
    text!(
        "diagram.room.boxed",
        even,
        limit = limit_said(),
        ways = ways.concat(),
        last
    )
}

/// What a diagram is cut to, as the value says it — refused where it is out of range, not
/// rounded into one (`diagramSpec`).
fn spec(project: &Project, args: &Map<String, Value>) -> Result<Spec, String> {
    let a = Args::new(args);
    let mut spec = Spec::default();
    if let Some(roots) = a.strings("roots") {
        spec.roots = roots;
        spec.roots.sort();
        spec.roots.dedup();
    }
    let depth = a.int("depth", 1);
    if !(0..=10).contains(&depth) {
        return Err(text!("diagram.spec.depth", depth));
    }
    spec.depth = depth as usize;
    if let Some(kinds) = a.strings("kinds") {
        spec.kinds = kinds_asked(project, &kinds)?;
    } else {
        // Not asked: every kind but those whose definition hides them from the start
        // (`visible: false`), as the window draws them.
        let available = mermaid::available_kinds(project);
        let hidden = |k: &String| project.defs.line(k).is_some_and(|d| !d.visible);
        if available.iter().any(hidden) {
            spec.kinds = available.into_iter().filter(|k| !hidden(k)).collect();
        }
    }
    if let Some(tags) = a.strings("tags") {
        spec.tags = tags;
        spec.tags.sort();
        spec.tags.dedup();
    }
    spec.section = base(project, args)?;
    // A section's own cards drawn whether or not a line reaches them, as the window draws it.
    spec.whole_section = true;
    spec.fields = a.bool("fields", false);
    if let Some(direction) = a.optional_string("direction") {
        spec.direction =
            Direction::named(direction).ok_or_else(|| text!("diagram.spec.direction"))?;
    }
    Ok(spec)
}

/// The kinds of line a value's `kinds` names, as the drawing keeps them, in the order the project
/// has them — refused when one is not a kind of line there. An empty list is no filter (none), not
/// "draw nothing".
pub(crate) fn kinds_asked(project: &Project, kinds: &[String]) -> Result<Vec<String>, String> {
    let available = mermaid::available_kinds(project);
    let kinds: Vec<String> = kinds
        .iter()
        .map(|k| mermaid::kept_kind(k).to_string())
        .collect();
    let unknown: Vec<&str> = kinds
        .iter()
        .filter(|k| !available.contains(k))
        .map(|k| mermaid::said_kind(k))
        .collect();
    if !unknown.is_empty() {
        return Err(text!(
            "diagram.spec.kinds",
            unknown = unknown.join(", "),
            available = available
                .iter()
                .map(|k| mermaid::said_kind(k))
                .collect::<Vec<_>>()
                .join(" / ")
        ));
    }
    if kinds.is_empty() {
        return Ok(Vec::new());
    }
    Ok(available
        .into_iter()
        .filter(|k| kinds.contains(k))
        .collect())
}

/// Roots that are not there, or that the tags took out: refused, not drawn as an empty
/// diagram (`checkSelection`).
fn refuse_roots(project: &Project, spec: &Spec, graph: &mermaid::Subgraph) -> Result<(), String> {
    if let Some(first) = graph.missing_roots.first() {
        // With sections the usual cause is a missing section, which the lookup can say.
        if project.directory.is_sectioned() {
            let names: Vec<String> = project
                .detail
                .schemas
                .iter()
                .map(|c| c.name.clone())
                .collect();
            return Err(text!(
                "diagram.roots.at",
                first,
                why = project
                    .directory
                    .find_refusal(first, spec.section.as_deref(), &names)
            ));
        }
        return Err(text!(
            "diagram.roots.missing",
            names = capped(&graph.missing_roots, 12)
        ));
    }
    if !graph.excluded_roots.is_empty() {
        return Err(text!(
            "diagram.roots.excluded",
            names = capped(&graph.excluded_roots, 12)
        ));
    }
    Ok(())
}

/// What `revisionOnly` answers with: the rev, and the spec it was of, so two revs can be
/// told to be of the same question (`revisionView`).
fn revision_view(project: &Project, spec: &Spec, graph: &mermaid::Subgraph) -> Value {
    let mut asked = Map::new();
    if let Some(section) = &spec.section
        && project.detail.sections.iter().any(|s| &s.id == section)
    {
        asked.insert(
            "section".into(),
            project.directory.full_section(section).into(),
        );
    }
    asked.insert("roots".into(), json!(spec.roots));
    asked.insert("depth".into(), json!(spec.depth));
    let kinds: Vec<&str> = spec.kinds.iter().map(|k| mermaid::said_kind(k)).collect();
    asked.insert("kinds".into(), json!(kinds));
    asked.insert("tags".into(), json!(spec.tags));
    asked.insert("fields".into(), json!(spec.fields));
    asked.insert("direction".into(), json!(spec.direction.word()));
    json!({
        "rev": mermaid::revision(project, spec, graph),
        "spec": Value::Object(asked),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;
    use crate::tools::paging::MOST_CHARS as MOST;

    /// The diagrams kept (`tests/fixtures/kept/diagrams/`), by their names: the project's, the
    /// section Schemas', a card's and a flow's.
    const KEPT: [&str; 4] = ["project", "schemas", "order", "refund"];

    /// The sample project, in a store made once in a run (these only read it).
    fn store() -> Store {
        Store::open(crate::fixtures::read_only_sample()).unwrap()
    }

    /// A kept diagram's rev, as its answer says it.
    fn kept_rev(name: &str) -> String {
        let text = std::fs::read_to_string(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join(format!("tests/fixtures/kept/diagrams/{name}.json")),
        )
        .unwrap();
        let said: Value = serde_json::from_str(&text).unwrap();
        said["rev"].as_str().unwrap().to_string()
    }

    fn value(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    /// The answer as the surface gives it: named with its branch.
    fn read(seen: &Seen, given: Value, at: At) -> Result<String, String> {
        diagram(seen, &value(given), at, None).map(|out| seen.headed(out).text())
    }

    #[test]
    fn the_diagrams_are_as_kept() {
        let store = store();
        let seen = Seen::read(&store, "見本", None).unwrap();
        let diagram = json!({ "as": "diagram" });
        let order = ["Schemas/Ordering:Order".to_string()];
        let refund = ["Charts/Shop:Refund".to_string()];
        for (name, at) in KEPT.iter().zip([
            At::Project,
            At::Section("Schemas"),
            At::Cards(&order),
            At::Cards(&refund),
        ]) {
            // The mermaid apart from what is said of it, each read as it is written.
            let mut said: Value =
                serde_json::from_str(&read(&seen, diagram.clone(), at).unwrap()).unwrap();
            let mermaid = said
                .as_object_mut()
                .unwrap()
                .shift_remove("mermaid")
                .unwrap();
            crate::fixtures::kept(
                &format!("diagrams/{name}.json"),
                &crate::ojson::pretty(&said),
            );
            crate::fixtures::kept(&format!("diagrams/{name}.mmd"), mermaid.as_str().unwrap());
        }
    }

    #[test]
    fn a_value_out_of_its_form_is_refused() {
        let store = store();
        let seen = Seen::read(&store, "見本", None).unwrap();
        let order = ["Schemas/Ordering:Order".to_string()];
        let refund = ["Charts/Shop:Refund".to_string()];
        let refused = |given: Value, at: At| read(&seen, given, at).unwrap_err();
        assert_eq!(
            refused(json!({ "as": "diagram", "depth": 11 }), At::Cards(&order)),
            "depth は 0〜10 です（11）"
        );
        assert_eq!(
            refused(json!({ "as": "diagram", "roots": ["x"] }), At::Project),
            "value に使えないキーです: roots。使えるのは: as / kinds / tags / fields / direction / revisionOnly（形は guide の about: branch_read）"
        );
        assert_eq!(
            refused(
                json!({ "as": "diagram", "direction": "up" }),
                At::Section("Schemas")
            ),
            "value の direction は out / in / both のいずれかです（渡されたのは up）（value の形は guide の about: section_read）"
        );
        assert_eq!(
            refused(json!({ "as": "diagram", "kinds": ["nope"] }), At::Project),
            "kinds に使えない値です: nope。このプロジェクトにあるのは embed / points / signedBy / reads / writes / next / mentions / run"
        );
        assert_eq!(
            refused(
                json!({ "as": "diagram", "shape": "sequence" }),
                At::Cards(&order)
            ),
            "shape は target が flow 1 つのときだけです"
        );
        assert_eq!(
            refused(json!({ "as": "diagram", "depth": 2 }), At::Cards(&refund)),
            "flow の図では depth は使いません"
        );
        assert!(
            refused(
                json!({ "as": "diagram" }),
                At::Cards(&["Schemas:Nothing".into(), "Schemas:Customer".into()])
            )
            .starts_with("図の起点の「Schemas:Nothing」: ")
        );
        assert!(
            refused(
                json!({ "as": "diagram", "tags": ["無いタグ"] }),
                At::Cards(&order)
            )
            .starts_with(
                "図の起点のカードが tags の絞り込みから外れています: Schemas/Ordering:Order。"
            )
        );
    }

    #[test]
    fn sections_too_many_for_the_room_are_left_out_said_so() {
        let store = store();
        let seen = Seen::read(&store, "見本", None).unwrap();
        let project = seen.project();
        let spec = spec(&project, &Map::new()).unwrap();
        let graph = mermaid::select(&project, &spec);
        let cut = mermaid::Cut {
            rev: "abc".into(),
            chars: 50_000,
            cards: graph.cards(),
            hint: "ヒント".into(),
        };
        let all = mermaid::render_sections(&project, &spec, &graph, &cut, MOST);
        let boxes = |text: &str| text.lines().filter(|l| l.contains("    class s")).count();
        assert_eq!(boxes(&all), 4, "{all}");
        assert!(!all.contains("ほか"), "{all}");
        assert!(all.contains("    class s1[\"Docs（2 枚）\"]\n"), "{all}");
        assert!(all.contains("    s1 ..> s2 : mentions ×1\n"), "{all}");
        // Room for the first box alone: the rest said, no line to a box left out.
        let head: usize = all.lines().take(3).map(|l| l.chars().count() + 1).sum();
        let room = head + 40 + "    class s1[\"Docs（2 枚）\"]\n".chars().count();
        let one = mermaid::render_sections(&project, &spec, &graph, &cut, room);
        assert!(one.chars().count() <= room, "{one}");
        assert_eq!(boxes(&one), 1, "{one}");
        assert!(one.ends_with("    note \"…ほか 3 セクション\"\n"), "{one}");
        assert!(!one.contains(" : "), "{one}");
        assert!(one.starts_with(
            "%% specboard project=見本 roots=* depth=1 kinds=embed,points,signedBy,reads,writes,next,mentions,run fields=false truncated=true chars=50000 cards=8 rev=abc\n%% hint=ヒント\nclassDiagram\n"
        ), "{one}");
    }

    #[test]
    fn revision_only_answers_the_rev_of_the_diagram() {
        let store = store();
        let seen = Seen::read(&store, "見本", None).unwrap();
        let order = ["Schemas/Ordering:Order".to_string()];
        let only = |at: At| {
            let Output::Json(v) = diagram(
                &seen,
                &value(json!({ "as": "diagram", "revisionOnly": true })),
                at,
                None,
            )
            .unwrap() else {
                panic!("not JSON")
            };
            v
        };
        let read = only(At::Cards(&order));
        assert_eq!(read["rev"], kept_rev("order").as_str());
        assert_eq!(read["spec"]["roots"], json!(["Schemas/Ordering:Order"]));
        let section = only(At::Section("Schemas"));
        assert_eq!(section["rev"], kept_rev("schemas").as_str());
        assert_eq!(section["spec"]["section"], "Schemas");
        let refund = ["Charts/Shop:Refund".to_string()];
        let chart = only(At::Cards(&refund));
        assert_eq!(chart["rev"], kept_rev("refund").as_str());
        assert_eq!(
            chart["spec"],
            json!({ "chart": "Charts/Shop:Refund", "shape": "flowchart" })
        );
    }
}
