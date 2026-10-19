//! Planning a change, then making it (`batch`; `MCPTools.batching` in the Swift app): one tool. Its steps are write
//! tools by name with their arguments. Given a project (and a tree) and steps, the steps are only
//! tried: nothing is kept and the tree is not held. With `open` a batch is opened that holds the
//! tree; given a batch's id (`target`), steps are added, one is taken out
//! or the batch is let go of — each time every step is tried again in a transaction that is let go,
//! so a mistake shows at the step that made it. `execute` runs them: one transaction, one commit,
//! the batch closed; given steps and no id, they are run at once. Each step is held to what the
//! project lets MCP write as it is run (`mcp_writes`), as a tool called on its own is. What is
//! broken once the steps are done and was not before they began is said once, of the whole batch:
//! a path one step broke and a later one mended is not. So is what a flow lacks yet, as the flow
//! stands once the steps are done: a node one step left without a way on and a later one joined
//! is not.

use super::args::Args;
use super::body_from::{Budget, Files, Holds};
use super::check::{flow_warnings, is_flow_warning};
use super::mcp_writes::{self, Asked};
use super::{Named, Seen, project_summary_in, update, write_in};
use crate::Core;
use crate::mcp::Output;
use crate::write::{Batch, Tx, Who};
use serde_json::{Map, Value};
use std::collections::HashSet;

/// What a batch may run: the writes, scoped to one project (`batchable`).
pub(crate) const BATCHABLE: [&str; 11] = [
    "project_update",
    "section_create",
    "section_update",
    "section_delete",
    "card_create",
    "card_update",
    "card_delete",
    "tag_update",
    "branch_create",
    "branch_update",
    "branch_delete",
];

/// How many steps a batch holds at the most.
const LIMIT: usize = 200;

type Step = (String, Map<String, Value>);

/// A trial of the steps a call gives: all the steps, what each would write, the project's and the
/// tree's names, and what each added step read from files.
type Trial = (Vec<Step>, Replay, String, String, Vec<Vec<Value>>);

/// What a run of the steps answered, what each wrote in the history, what each touches that a
/// person is asked about each time (when only tried), and what is broken once they are done that
/// was not before — and what the flows lack then.
struct Replay {
    results: Vec<Value>,
    changes: Vec<Vec<String>>,
    asks: Vec<Vec<Asked>>,
    broken: Vec<String>,
}

/// How a step that touches what a person is asked about each time goes: tried (it goes, and is
/// said), or run, with the items a person said yes to for this run.
enum Asking<'a> {
    Tried,
    Run(&'a [&'static str]),
}

/// What a tried batch is told of what a person is asked about when it is run.
fn asked_note(asks: &[Asked]) -> String {
    text!(
        "batch.askedNote",
        items = crate::text::list(&asks.iter().map(|a| a.item.called()).collect::<Vec<_>>())
    )
}

fn describe(index: usize, tool: &str) -> String {
    format!("step {index} ({tool})")
}

/// Whether a step's warning is of a path that does not resolve (`update::unresolved`,
/// `update::broken_warning`, a move's): said of the whole batch instead, as it stands at the end.
fn of_a_path(warning: &str) -> bool {
    update::is_unresolved(warning) || update::is_broken_warning(warning)
}

/// What a step's answer warns of, as it said it.
fn said_by(result: &Value) -> impl Iterator<Item = &str> {
    result
        .get("warnings")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
}

/// A step's own warnings — what is broken and what a flow lacks are the batch's — and what it
/// touches that a person is asked about when the batch runs.
fn step_warnings(replay: &Replay, step: usize) -> Vec<String> {
    let mut out: Vec<String> = said_by(&replay.results[step])
        .filter(|w| !of_a_path(w) && !is_flow_warning(w))
        .map(str::to_string)
        .collect();
    if let Some(asks) = replay.asks.get(step).filter(|a| !a.is_empty()) {
        out.push(asked_note(asks));
    }
    out
}

/// Every step's own warnings, each said with its step, then what is broken at the end.
fn all_warnings(replay: &Replay, steps: &[Step]) -> Vec<String> {
    let mut out: Vec<String> = steps
        .iter()
        .enumerate()
        .flat_map(|(i, (tool, _))| {
            step_warnings(replay, i)
                .into_iter()
                .map(move |w| format!("{}: {w}", describe(i, tool)))
        })
        .collect();
    out.extend(replay.broken.iter().cloned());
    out
}

fn strings(list: &[String]) -> Value {
    Value::Array(list.iter().cloned().map(Value::from).collect())
}

/// The tree a batch's steps write in, by its name now.
fn tree_of(tx: &Tx, batch: &Batch) -> Result<Named, String> {
    let name = if batch.tree.is_empty() {
        "main".to_string()
    } else if batch.tree == crate::store::BOARD {
        crate::store::BOARD_NAME.to_string()
    } else {
        super::branches(tx.conn(), &batch.project)?
            .into_iter()
            .find(|(id, _)| *id == batch.tree)
            .map(|(_, n)| n)
            .ok_or_else(|| text!("batch.tree.gone"))?
    };
    Ok(Named {
        project: batch.project.clone(),
        tree: batch.tree.clone(),
        name,
    })
}

fn project_name(tx: &Tx, project: &str) -> String {
    super::projects_in(tx.conn())
        .ok()
        .and_then(|all| all.into_iter().find(|p| p.id == project).map(|p| p.name))
        .unwrap_or_else(|| project.to_string())
}

/// A batch by its id, or why there is none. One about a project the call's access key may not see
/// is missing, like one that is gone; going on with it needs what the call's tool needs of its
/// project (`MCPTools.batching`'s `open`).
fn open(tx: &Tx, token: &str) -> Result<Batch, String> {
    let missing = || text!("batch.open.missing", token);
    let batch = tx.batch(token)?.ok_or_else(missing)?;
    if !crate::access::sees(tx.conn(), &batch.project)? {
        return Err(missing());
    }
    crate::access::admit(tx.conn(), &batch.project, &project_name(tx, &batch.project))?;
    Ok(batch)
}

/// The batch's tree as it is now in the transaction.
fn seen_now(tx: &Tx, batch: &Batch, named: &Named) -> Result<std::rc::Rc<Seen>, String> {
    let scope = super::cards::Scope {
        project: batch.project.clone(),
        project_name: project_name(tx, &batch.project),
        tree: named.tree.clone(),
        tree_name: named.name.clone(),
    };
    Seen::in_tx(tx, &scope)
}

/// What the flows in the batch's tree lack now, as a writer of each is told (`flow_warnings`).
fn unfinished_now(tx: &Tx, batch: &Batch, named: &Named) -> Result<Vec<String>, String> {
    let seen = seen_now(tx, batch, named)?;
    let project = seen.project();
    Ok(project
        .detail
        .schemas
        .iter()
        .filter(|c| c.kind == "flow")
        .flat_map(|c| flow_warnings(c, &project))
        .map(|w| w.to_string())
        .collect())
}

/// What is broken in the batch's tree now, by its key and as it is said.
fn broken_now(tx: &Tx, batch: &Batch, named: &Named) -> Result<Vec<(String, String)>, String> {
    let seen = seen_now(tx, batch, named)?;
    Ok(seen
        .broken()
        .iter()
        .cloned()
        .map(|(key, d)| {
            let said = if d.path.is_empty() {
                text!("batch.broken.targetGone", at = d.at, via = d.via)
            } else {
                text!(
                    "batch.broken.unresolved",
                    at = d.at,
                    via = d.via,
                    path = d.path,
                    hint = d.hint.unwrap_or_default()
                )
            };
            (key, said.trim_end().to_string())
        })
        .collect())
}

/// The revs the steps give (`revs`), every one held to the batch's tree as it is before they run —
/// so a step is not refused for what an earlier one wrote, and a card an earlier one makes or renames
/// to is not one that was read. Refused at the first step that gives one not as it was read, with
/// every one that is not said.
fn refuse_stale(
    tx: &Tx,
    batch: &Batch,
    named: &Named,
    steps: &[Step],
    failure: &dyn Fn(usize, &str, &str) -> String,
) -> Result<(), String> {
    if tx.who().side != crate::write::Side::Mcp {
        return Ok(());
    }
    let mut given: Vec<(usize, Vec<(String, String)>)> = Vec::new();
    for (index, (tool, arguments)) in steps.iter().enumerate() {
        match super::revs::given(tool, arguments) {
            Ok(revs) if revs.is_empty() => {}
            Ok(revs) => given.push((index, revs)),
            Err(e) => return Err(failure(index, tool, &e)),
        }
    }
    if given.is_empty() {
        return Ok(());
    }
    let seen = seen_now(tx, batch, named)?;
    let mut first: Option<usize> = None;
    let mut lines: Vec<String> = Vec::new();
    for (index, revs) in &given {
        for line in super::revs::stale(tx, &seen, revs) {
            first.get_or_insert(*index);
            if !lines.contains(&line) {
                lines.push(line);
            }
        }
    }
    match first {
        None => Ok(()),
        Some(index) => Err(failure(index, &steps[index].0, &lines.join("\n"))),
    }
}

/// The steps run in the batch's tree, as one commit, saying what each wrote; a step that
/// fails is said by `failure`.
fn replay(
    tx: &mut Tx,
    batch: &Batch,
    steps: &[Step],
    first: usize,
    asking: Asking,
    failure: &dyn Fn(usize, &str, &str) -> String,
) -> Result<Replay, String> {
    let named = tree_of(tx, batch)?;
    refuse_stale(tx, batch, &named, steps, failure)?;
    let before: HashSet<String> = broken_now(tx, batch, &named)?
        .into_iter()
        .map(|(key, _)| key)
        .collect();
    let unfinished_before: HashSet<String> =
        unfinished_now(tx, batch, &named)?.into_iter().collect();
    tx.mark(&batch.project, &batch.tree, "batch");
    let mut results = Vec::new();
    let mut marks = Vec::new();
    let mut asked = Vec::new();
    for (index, (tool, arguments)) in steps.iter().enumerate() {
        if !BATCHABLE.contains(&tool.as_str()) {
            return Err(failure(index, tool, &text!("batch.step.notBatchable")));
        }
        // What the project lets MCP do, as when the step's tool is called on its own: as the project
        // says now, so a step staged before a person stopped it is refused here.
        let args = Args::new(arguments);
        let asks = match mcp_writes::judge(tx.conn(), tx.who(), tool, &args, Some(&named)) {
            Ok(asks) => asks,
            Err(e) => return Err(failure(index, tool, &e)),
        };
        match asking {
            Asking::Tried => asked.push(asks),
            Asking::Run(granted) => {
                let unasked: Vec<Asked> = asks
                    .into_iter()
                    .filter(|a| !granted.contains(&a.item.id))
                    .collect();
                if !unasked.is_empty() {
                    return Err(failure(
                        index,
                        tool,
                        &mcp_writes::not_requested(&unasked, &text!("batch.label.execute")),
                    ));
                }
                asked.push(Vec::new());
            }
        }
        marks.push(tx.written().len());
        match write_in(tx, tool, &args, Some(&named)) {
            Ok(v) => results.push(v),
            Err(e) => {
                return Err(failure(index, tool, &made_later(steps, index, first, &e)));
            }
        }
    }
    let mut changes = vec![Vec::new(); steps.len()];
    for (at, (_, summary)) in tx.written().iter().enumerate() {
        if let Some(step) = marks.iter().rposition(|m| *m <= at) {
            changes[step].push(summary.to_string());
        }
    }
    // What the steps leave broken, as it stands once they are all done (a
    // warning a later step put right is not said).
    let named = tree_of(tx, batch)?;
    let mut broken: Vec<String> = broken_now(tx, batch, &named)?
        .into_iter()
        .filter(|(key, _)| !before.contains(key))
        .map(|(_, said)| said)
        .collect();
    // What the flows lack once the steps are all done: what a step said of them and is
    // so still, and what they did not lack before.
    let said: HashSet<&str> = results
        .iter()
        .flat_map(said_by)
        .filter(|w| is_flow_warning(w))
        .collect();
    broken.extend(
        unfinished_now(tx, batch, &named)?
            .into_iter()
            .filter(|w| !unfinished_before.contains(w) || said.contains(w.as_str())),
    );
    Ok(Replay {
        results,
        changes,
        asks: asked,
        broken,
    })
}

/// A step's refusal with the places it says to change (`fix`, said in the step's arguments) said in the
/// batch call, where the step is `steps[step]` of the call; none for a step the call does not hold.
/// A step's failure, with what a later step makes said when the step points at it: steps run in
/// order, so a card a later step makes is not there yet. Said when a path the step writes as a
/// line's end, a place or a target (`to`, `in`, `target`) is in the failure and names a card a
/// later `card_create` makes — by the same path, or by its name beside the step's own card (a
/// flow's node by its name alone). `first`: the step said `steps[0]`.
fn made_later(steps: &[Step], index: usize, first: usize, error: &str) -> String {
    fn said(value: &Value, out: &mut Vec<String>) {
        match value {
            Value::Object(map) => {
                for (key, v) in map {
                    match v {
                        Value::String(s) if ["to", "in", "target"].contains(&key.as_str()) => {
                            out.push(s.trim().to_string());
                        }
                        _ => said(v, out),
                    }
                }
            }
            Value::Array(items) => items.iter().for_each(|v| said(v, out)),
            _ => {}
        }
    }
    /// The paths a step's target names, or its value's keys when it has none (a map of paths).
    fn targets(arguments: &Map<String, Value>) -> Vec<String> {
        match arguments.get("target") {
            Some(Value::String(s)) => vec![s.trim().to_string()],
            Some(Value::Array(items)) => items
                .iter()
                .filter_map(Value::as_str)
                .map(|s| s.trim().to_string())
                .collect(),
            _ => arguments
                .get("value")
                .and_then(Value::as_object)
                .map(|m| m.keys().map(|k| k.trim().to_string()).collect())
                .unwrap_or_default(),
        }
    }
    let (_, arguments) = &steps[index];
    let mut written = Vec::new();
    said(&Value::Object(arguments.clone()), &mut written);
    written.retain(|w| !w.is_empty() && error.contains(w.as_str()));
    if written.is_empty() {
        return error.to_string();
    }
    // Where a name alone is read from: beside each card the step writes (inside its flow, in its
    // section).
    let beside: Vec<String> = targets(arguments)
        .iter()
        .flat_map(|t| {
            let mut out = Vec::new();
            if let Some((head, _)) = crate::pathtext::last_dot(t)
                && crate::pathtext::has_section(head)
            {
                out.push(format!("{head}."));
            }
            if let (Some(section), _) = crate::pathtext::split_section(t) {
                out.push(format!("{section}:"));
            }
            out
        })
        .collect();
    let names = |w: &str, made: &str| {
        crate::pathtext::same(w, made)
            || (!crate::pathtext::has_section(w)
                && beside
                    .iter()
                    .any(|b| crate::pathtext::same(&format!("{b}{w}"), made)))
    };
    for (later, (tool, args)) in steps.iter().enumerate().skip(index + 1) {
        if tool != "card_create" {
            continue;
        }
        let made = targets(args);
        if let Some(path) = written.iter().find(|w| made.iter().any(|m| names(w, m))) {
            let hint = text!(
                "batch.failed.later",
                path,
                later = later.saturating_sub(first),
                tool
            );
            let (words, beside) = crate::mcp::refusal_parts(error);
            return crate::mcp::refusal(format!("{words}{hint}"), Value::Object(beside));
        }
    }
    error.to_string()
}

fn at_step(error: &str, step: Option<usize>) -> String {
    crate::mcp::rebase_fix(
        error,
        step.map(|n| format!("/value/steps/{n}/arguments"))
            .as_deref(),
    )
}

/// What a batch tried is told first: that nothing is written, and how to run it.
fn unwritten(token: &str) -> String {
    text!("batch.unwritten", token)
}

impl Core {
    /// `batch`. `granted`: what a person said yes to for a run (`Core::admitted`).
    pub(crate) fn batch_tool(
        &self,
        who: &Who,
        args: &Args,
        granted: &[&'static str],
    ) -> Result<Output, String> {
        let given = args.value()?;
        super::check(&given, "batch", "batch")?;
        let execute = args.bool("execute", false);
        let hold = given.get("open").and_then(Value::as_bool) == Some(true);
        if hold && execute {
            return Err(text!("batch.value.openNotExecute"));
        }
        let wants_from = given.contains_key("stepsFrom");
        let wants_steps = given.contains_key("steps") || wants_from;
        let wants_remove = given.contains_key("remove");
        let wants_discard = given.get("discard").and_then(Value::as_bool) == Some(true);
        if [
            given.contains_key("steps"),
            wants_from,
            wants_remove,
            wants_discard,
        ]
        .iter()
        .filter(|w| **w)
        .count()
            > 1
        {
            return Err(text!("batch.value.oneOnly"));
        }
        // Everything one call reads from files, the steps' file and the bodies in the steps.
        let mut budget = Budget::default();
        let mut given = given;
        let mut from_said: Option<Value> = None;
        let out = match args.optional_string("target") {
            None => {
                if wants_remove || wants_discard {
                    return Err(text!("batch.value.needsTarget"));
                }
                if wants_from {
                    let Some(named) = args.optional_string("project") else {
                        return Err(text!("batch.open.needsProject"));
                    };
                    let project = self
                        .trying(who, |tx| project_summary_in(tx.conn(), named))?
                        .id;
                    from_said = Some(self.steps_from(who, &project, &mut given, &mut budget)?);
                }
                self.batch_opened(
                    who,
                    args,
                    &given,
                    execute.then_some(granted),
                    hold,
                    &mut budget,
                )?
            }
            Some(token) => {
                if execute && (wants_steps || wants_remove || wants_discard) {
                    return Err(text!("batch.value.executeOnly"));
                }
                let batch = self.trying(who, |tx| {
                    let batch = open(tx, token)?;
                    if args.raw.contains_key("branch") {
                        return Err(text!(
                            "batch.value.branchOnce",
                            tree = tree_of(tx, &batch)?.name
                        ));
                    }
                    if let Some(named) = args.optional_string("project")
                        && project_summary_in(tx.conn(), named).map(|p| p.id).ok()
                            != Some(batch.project.clone())
                    {
                        return Err(text!("batch.value.otherProject"));
                    }
                    Ok(batch)
                })?;
                if execute {
                    self.batch_run(who, &batch, granted)?
                } else if wants_discard {
                    self.plain_writing(who.clone(), |tx| tx.close_batch(&batch.token))?;
                    let mut out = Map::new();
                    out.insert("discarded".into(), batch.token.clone().into());
                    out.insert("steps".into(), batch.steps.len().into());
                    Value::Object(out)
                } else if wants_remove {
                    self.batch_remove(who, &batch, &given)?
                } else if wants_steps {
                    if wants_from {
                        from_said =
                            Some(self.steps_from(who, &batch.project, &mut given, &mut budget)?);
                    }
                    self.batch_add(who, &batch, &given, false, &mut budget)?
                } else {
                    self.batch_retry(who, &batch)?
                }
            }
        };
        let mut out = out;
        if let (Some(said), Value::Object(o)) = (from_said, &mut out) {
            o.insert("stepsFrom".into(), said);
        }
        Ok(Output::Json(out))
    }

    /// The steps in the file `stepsFrom` names, put in `given` as its `steps` (the file read here, ahead
    /// of the store's lock but for the working directory's folder), and what was read (`{directory,
    /// path, steps}`).
    fn steps_from(
        &self,
        who: &Who,
        project: &str,
        given: &mut Map<String, Value>,
        budget: &mut Budget,
    ) -> Result<Value, String> {
        let from = given.shift_remove("stepsFrom").unwrap_or_default();
        let (directory, path) = super::body_from::shape(&from, "stepsFrom")?;
        let (name, folder) = self.trying(who, |tx| {
            super::body_from::folder_of(tx.conn(), project, &directory)
        })?;
        let (items, said) = read_steps(&folder, &name, &path, budget)?;
        given.insert("steps".into(), Value::Array(items));
        Ok(said)
    }

    /// What a batch's steps touch that a person is asked about each time, for its run, with its
    /// steps as the window shows them: the batch's the call names, or those it gives to run at once.
    /// None for a batch only tried.
    pub(crate) fn batch_asks(
        &self,
        who: &Who,
        args: &Args,
    ) -> Result<(Vec<Asked>, String), String> {
        if !args.bool("execute", false) {
            return Ok((Vec::new(), String::new()));
        }
        self.trying(who, |tx| {
            let (named, steps) = match args.optional_string("target") {
                Some(token) => {
                    let Ok(batch) = open(tx, token) else {
                        // The run says what is wrong with it.
                        return Ok((Vec::new(), String::new()));
                    };
                    (tree_of(tx, &batch)?, batch.steps)
                }
                None => {
                    let given = args.value()?;
                    let Some(named) = args.optional_string("project") else {
                        return Ok((Vec::new(), String::new()));
                    };
                    let Ok(project) = project_summary_in(tx.conn(), named) else {
                        return Ok((Vec::new(), String::new()));
                    };
                    let Ok(into) =
                        super::named_in(tx.conn(), &project.id, args.optional_string("branch"))
                    else {
                        return Ok((Vec::new(), String::new()));
                    };
                    let mut given = given;
                    if let Some(from) = given.shift_remove("stepsFrom") {
                        // The file as the run will read it; a file that cannot be read is
                        // refused by the run, in its words.
                        let read = super::body_from::shape(&from, "stepsFrom").and_then(
                            |(directory, path)| {
                                let (name, folder) = super::body_from::folder_of(
                                    tx.conn(),
                                    &project.id,
                                    &directory,
                                )?;
                                read_steps(&folder, &name, &path, &mut Budget::default())
                            },
                        );
                        match read {
                            Ok((items, _)) => {
                                given.insert("steps".into(), Value::Array(items));
                            }
                            Err(_) => return Ok((Vec::new(), String::new())),
                        }
                    }
                    let steps = steps_given(&given).unwrap_or_default();
                    (into, steps)
                }
            };
            let mut asks: Vec<Asked> = Vec::new();
            let mut lines = Vec::new();
            for (index, (tool, arguments)) in steps.iter().enumerate() {
                let mut arguments = arguments.clone();
                arguments.insert("project".into(), named.project.clone().into());
                let step = Args::new(&arguments);
                // A step denied now is refused by the run, in its words.
                let found = mcp_writes::judge(tx.conn(), tx.who(), tool, &step, Some(&named))
                    .unwrap_or_default();
                for asked in found {
                    if !asks.iter().any(|a| a.item.id == asked.item.id) {
                        asks.push(asked);
                    }
                }
                if lines.len() < 30 {
                    lines.push(format!(
                        "{index}. {tool} {}",
                        super::asking::cut(&super::asking::what_of(tool, &arguments), 200)
                    ));
                }
            }
            if steps.len() > lines.len() {
                lines.push(text!("batch.asks.more", count = steps.len() - lines.len()));
            }
            Ok((asks, lines.join("\n")))
        })
    }

    /// A transaction that is let go: what it writes is only tried.
    fn trying<T>(
        &self,
        who: &Who,
        act: impl FnOnce(&mut Tx) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut store = self.lock();
        let mut tx = store.write(who.clone())?;
        act(&mut tx)
    }

    /// Steps given with a project and no batch: only tried, or run at once to `execute` them; a
    /// batch that holds the tree is opened only when asked (`open`).
    fn batch_opened(
        &self,
        who: &Who,
        args: &Args,
        given: &Map<String, Value>,
        execute: Option<&[&'static str]>,
        hold: bool,
        budget: &mut Budget,
    ) -> Result<Value, String> {
        let Some(named) = args.optional_string("project") else {
            return Err(text!("batch.open.needsProject"));
        };
        let (project, into) = self.trying(who, |tx| {
            let project = project_summary_in(tx.conn(), named)?;
            let into = super::named_in(tx.conn(), &project.id, args.optional_string("branch"))?;
            Ok((project, into))
        })?;
        if !hold && !given.contains_key("steps") {
            return Err(text!("batch.steps.needed"));
        }
        let none = Batch {
            token: String::new(),
            project: project.id.clone(),
            tree: into.tree.clone(),
            steps: Vec::new(),
            checked_at: String::new(),
        };
        if let Some(granted) = execute {
            let (steps, _) = self.trying(who, |tx| {
                checked_steps(tx, &none, &into.name, given, 0, budget)
            })?;
            let batch = Batch { steps, ..none };
            return self.plain_writing(who.clone(), |tx| {
                let result = replay(
                    tx,
                    &batch,
                    &batch.steps,
                    0,
                    Asking::Run(granted),
                    &|index, tool, error| {
                        text!(
                            "batch.failed.opened",
                            index,
                            tool,
                            error = at_step(error, Some(index))
                        )
                    },
                )?;
                ran(tx, &batch, &result)
            });
        }
        if !hold {
            let (steps, result, project, tree, reads) =
                self.batch_trial(who, &none, given, 0, budget)?;
            return Ok(tried(None, project, tree, &steps, &result, 0, &reads));
        }
        let token = self.plain_writing(who.clone(), |tx| tx.open_batch(&project.id, &into.tree))?;
        let batch = self.trying(who, |tx| open(tx, &token))?;
        if given.contains_key("steps") {
            return self.batch_add(who, &batch, given, true, budget);
        }
        let nothing = Replay {
            results: Vec::new(),
            changes: Vec::new(),
            asks: Vec::new(),
            broken: Vec::new(),
        };
        Ok(tried(
            Some(&token),
            project.name.clone(),
            into.name,
            &[],
            &nothing,
            0,
            &[],
        ))
    }

    /// Steps added to a batch and every one tried; all or none are added. A batch opened for them
    /// goes with them when they do not go through: the caller never saw its id.
    fn batch_add(
        &self,
        who: &Who,
        batch: &Batch,
        given: &Map<String, Value>,
        opened: bool,
        budget: &mut Budget,
    ) -> Result<Value, String> {
        let first = batch.steps.len();
        let (steps, result, project, tree, reads) =
            match self.batch_trial(who, batch, given, first, budget) {
                Ok(t) => t,
                Err(e) => {
                    if opened {
                        let _ = self.plain_writing(who.clone(), |tx| tx.close_batch(&batch.token));
                    }
                    return Err(e);
                }
            };
        self.plain_writing(who.clone(), |tx| tx.save_batch(&batch.token, &steps))?;
        Ok(tried(
            Some(&batch.token),
            project,
            tree,
            &steps,
            &result,
            first,
            &reads,
        ))
    }

    /// The steps a call gives, after the `first` the batch holds, every one tried in a transaction
    /// that is let go: all the steps, what each would write, the project's and the tree's names.
    fn batch_trial(
        &self,
        who: &Who,
        batch: &Batch,
        given: &Map<String, Value>,
        first: usize,
        budget: &mut Budget,
    ) -> Result<Trial, String> {
        self.trying(who, |tx| {
            let tree = tree_of(tx, batch)?.name;
            let (added, reads) = checked_steps(tx, batch, &tree, given, first, budget)?;
            let mut steps = batch.steps.clone();
            steps.extend(added);
            let result = replay(
                tx,
                batch,
                &steps,
                first,
                Asking::Tried,
                &|index, tool, error| {
                    if index >= first {
                        let step = index - first;
                        text!(
                            "batch.failed.added",
                            step,
                            tool,
                            error = at_step(error, Some(step))
                        )
                    } else {
                        text!(
                            "batch.failed.earlier",
                            step = describe(index, tool),
                            error = at_step(error, None)
                        )
                    }
                },
            )?;
            Ok((steps, result, project_name(tx, &batch.project), tree, reads))
        })
    }

    /// Every step of a batch tried again, as the project is now: what each would write.
    fn batch_retry(&self, who: &Who, batch: &Batch) -> Result<Value, String> {
        let (result, project, tree) = self.trying(who, |tx| {
            let result = replay(
                tx,
                batch,
                &batch.steps,
                0,
                Asking::Tried,
                &|index, tool, error| {
                    text!(
                        "batch.failed.retry",
                        step = describe(index, tool),
                        error = at_step(error, None)
                    )
                },
            )?;
            Ok((
                result,
                project_name(tx, &batch.project),
                tree_of(tx, batch)?.name,
            ))
        })?;
        Ok(tried(
            Some(&batch.token),
            project,
            tree,
            &batch.steps,
            &result,
            0,
            &[],
        ))
    }

    /// One step taken out, the others tried again.
    fn batch_remove(
        &self,
        who: &Who,
        batch: &Batch,
        given: &Map<String, Value>,
    ) -> Result<Value, String> {
        let index = given.get("remove").and_then(Value::as_i64).unwrap_or(-1);
        if index < 0 || index as usize >= batch.steps.len() {
            return Err(text!(
                "batch.remove.range",
                last = batch.steps.len() as i64 - 1,
                index
            ));
        }
        let index = index as usize;
        let mut steps = batch.steps.clone();
        let (removed, _) = steps.remove(index);
        let broken = if steps.is_empty() {
            Vec::new()
        } else {
            self.trying(who, |tx| {
                replay(
                    tx,
                    batch,
                    &steps,
                    0,
                    Asking::Tried,
                    &|later, tool, error| {
                        text!(
                            "batch.failed.removal",
                            step = describe(if later >= index { later + 1 } else { later }, tool),
                            error
                        )
                    },
                )
                .map(|r| all_warnings(&r, &steps))
            })?
        };
        self.plain_writing(who.clone(), |tx| tx.save_batch(&batch.token, &steps))?;
        let mut gone = Map::new();
        gone.insert("step".into(), index.into());
        gone.insert("tool".into(), removed.into());
        let mut out = Map::new();
        out.insert("said".into(), unwritten(&batch.token).into());
        if !broken.is_empty() {
            out.insert("warnings".into(), strings(&broken));
        }
        out.insert("batch".into(), batch.token.clone().into());
        out.insert("removed".into(), Value::Object(gone));
        out.insert("staged".into(), steps.len().into());
        Ok(Value::Object(out))
    }

    /// A batch's steps run, kept, and the batch closed.
    fn batch_run(
        &self,
        who: &Who,
        batch: &Batch,
        granted: &[&'static str],
    ) -> Result<Value, String> {
        if batch.steps.is_empty() {
            return Err(text!("batch.run.empty"));
        }
        self.plain_writing(who.clone(), |tx| {
            let result = replay(
                tx,
                batch,
                &batch.steps,
                0,
                Asking::Run(granted),
                &|index, tool, error| {
                    text!(
                        "batch.failed.run",
                        step = describe(index, tool),
                        error = at_step(error, None)
                    )
                },
            )?;
            let out = ran(tx, batch, &result)?;
            tx.close_batch(&batch.token)?;
            Ok(out)
        })
    }
}

/// What a batch tried answers: that nothing is written and how to run it, what is broken once its
/// steps are done, and what each step from `first` on would write and warns of. Steps only tried
/// (no batch id) are staged nowhere, so no id and no count of staged steps are said.
fn tried(
    token: Option<&str>,
    project: String,
    tree: String,
    steps: &[Step],
    result: &Replay,
    first: usize,
    reads: &[Vec<Value>],
) -> Value {
    let mut out = Map::new();
    let said = match token {
        Some(token) => unwritten(token),
        None => text!("batch.unwritten.tried"),
    };
    out.insert("said".into(), said.into());
    if !result.broken.is_empty() {
        out.insert("warnings".into(), strings(&result.broken));
    }
    if let Some(token) = token {
        out.insert("batch".into(), token.into());
    }
    out.insert("project".into(), project.into());
    out.insert("branch".into(), tree.into());
    if token.is_some() {
        out.insert("staged".into(), steps.len().into());
    }
    out.insert(
        "steps".into(),
        Value::Array(
            (first..steps.len())
                .map(|i| {
                    let mut row = Map::new();
                    row.insert("step".into(), i.into());
                    row.insert("tool".into(), steps[i].0.clone().into());
                    row.insert("changes".into(), strings(&result.changes[i]));
                    let said = step_warnings(result, i);
                    if !said.is_empty() {
                        row.insert("warnings".into(), strings(&said));
                    }
                    if let Some(read) = reads.get(i - first).filter(|r| !r.is_empty()) {
                        row.insert("bodyFrom".into(), Value::Array(read.clone()));
                    }
                    Value::Object(row)
                })
                .collect(),
        ),
    );
    Value::Object(out)
}

/// What a run answers: how many steps ran, the warnings — each step's own, and what is broken
/// once they are done — and every card the steps wrote, by its path now, with its rev (`revs`).
fn ran(tx: &Tx, batch: &Batch, replay: &Replay) -> Result<Value, String> {
    let mut out = Map::new();
    let warnings = all_warnings(replay, &batch.steps);
    if !warnings.is_empty() {
        out.insert("warnings".into(), strings(&warnings));
    }
    out.insert("project".into(), project_name(tx, &batch.project).into());
    let named = tree_of(tx, batch)?;
    out.insert("branch".into(), named.name.clone().into());
    out.insert("executed".into(), batch.steps.len().into());
    let scope = super::cards::Scope {
        project: batch.project.clone(),
        project_name: project_name(tx, &batch.project),
        tree: named.tree,
        tree_name: named.name,
    };
    let revs = super::revs::written(tx, &scope)?;
    if !revs.is_empty() {
        out.insert("revs".into(), Value::Object(revs));
    }
    Ok(Value::Object(out))
}

/// The steps a call's value gives, as written: `{tool, arguments}` each.
fn steps_given(given: &Map<String, Value>) -> Result<Vec<Step>, String> {
    let items = given
        .get("steps")
        .and_then(Value::as_array)
        .filter(|i| !i.is_empty())
        .ok_or_else(|| text!("batch.steps.needed"))?;
    items
        .iter()
        .map(|item| {
            let Some(name) = item.get("tool").and_then(Value::as_str) else {
                return Err(text!("batch.steps.elementShape"));
            };
            Ok((
                name.to_string(),
                item.get("arguments")
                    .and_then(Value::as_object)
                    .cloned()
                    .unwrap_or_default(),
            ))
        })
        .collect()
}

/// The steps in the JSON file at `path` in `folder` (a working directory called `directory`), and what
/// was read (`{directory, path, steps}`).
fn read_steps(
    folder: &str,
    directory: &str,
    path: &str,
    budget: &mut Budget,
) -> Result<(Vec<Value>, Value), String> {
    let text = super::body_from::read(
        folder,
        path,
        "stepsFrom",
        super::body_from::MOST_CALL_BYTES,
        &|bytes| {
            text!(
                "batch.stepsFrom.tooBig",
                path,
                bytes,
                most = super::body_from::MOST_CALL_BYTES
            )
        },
        budget,
    )?;
    let parsed: Value = serde_json::from_str(&text).map_err(|e| {
        let why = e.to_string();
        text!(
            "batch.stepsFrom.notJson",
            path,
            line = e.line(),
            column = e.column(),
            why = why.split(" at line ").next().unwrap_or(&why)
        )
    })?;
    let Value::Array(items) = parsed else {
        return Err(text!("batch.stepsFrom.notSteps"));
    };
    if items.is_empty() {
        return Err(text!("batch.stepsFrom.notSteps"));
    }
    let mut said = Map::new();
    said.insert("directory".into(), directory.into());
    said.insert("path".into(), path.into());
    said.insert("steps".into(), items.len().into());
    Ok((items, Value::Object(said)))
}

/// The bodies a step's arguments read from files (`bodyFrom`, in the value of `card_create` and
/// `card_update`) put in their place, as the step is received: what is said of each, and what is wrong.
/// A step staged holds the text it was tried with, so the run never reads a file.
fn read_bodies(
    files: &mut Files,
    arguments: &mut Map<String, Value>,
    reads: &mut Vec<Value>,
    errors: &mut Vec<String>,
) {
    // What the steps ahead make is not there to look at, so where a body may stand is told by the
    // value alone.
    let holds = Holds {
        body: None,
        rows: true,
        versions: true,
    };
    let targets: Vec<String> = match arguments.get("target") {
        Some(Value::String(one)) => vec![one.trim().to_string()],
        Some(Value::Array(many)) => many
            .iter()
            .filter_map(|t| t.as_str().map(|t| t.trim().to_string()))
            .collect(),
        _ => Vec::new(),
    };
    let Some(Value::Object(value)) = arguments.get_mut("value") else {
        return;
    };
    let mut said = |at: &str, taken: Result<Vec<super::body_from::Read>, Vec<String>>| match taken {
        Ok(taken) => reads.extend(taken.iter().map(|r| r.said(at))),
        Err(found) => errors.extend(found),
    };
    if targets.is_empty() {
        // Each target with a value of its own, keyed by it.
        for (key, entry) in value.iter_mut() {
            if let Value::Object(entry) = entry
                && super::body_from::wanted(entry)
            {
                let at = key.clone();
                said(&at, files.take(&at, holds, entry));
            }
        }
    } else if super::body_from::wanted(value) {
        // One value for every target: read once, said for each.
        match files.take(&targets[0], holds, value) {
            Ok(taken) => {
                for at in &targets {
                    said(at, Ok(taken.clone()));
                }
            }
            Err(found) => said(&targets[0], Err(found)),
        }
    }
}

/// The steps a call gives, each held to its tool's own arguments and put in the batch's project:
/// refused at the first that does not fit, by its place among them. What each reads from files is
/// read here, once: the steps returned hold the text, and what each step read is returned beside them.
fn checked_steps(
    tx: &Tx,
    batch: &Batch,
    tree: &str,
    given: &Map<String, Value>,
    first: usize,
    budget: &mut Budget,
) -> Result<(Vec<Step>, Vec<Vec<Value>>), String> {
    let steps = steps_given(given)?;
    if first + steps.len() > LIMIT {
        return Err(text!("batch.steps.limit", limit = LIMIT));
    }
    let mut files = Files::new(tx.conn(), &batch.project, budget);
    let mut out = Vec::new();
    let mut reads = Vec::new();
    let mut unread: Vec<String> = Vec::new();
    for (offset, (name, mut arguments)) in steps.into_iter().enumerate() {
        let label = text!("batch.step.label", offset, name);
        let Some(tool) = crate::mcp::tool(&name).filter(|_| BATCHABLE.contains(&name.as_str()))
        else {
            return Err(text!(
                "batch.step.toolOneOf",
                label,
                tools = BATCHABLE.join(" / ")
            ));
        };
        if arguments.contains_key("branch") {
            return Err(text!("batch.step.noBranch", label, tree));
        }
        if let Some(named) = arguments.get("project").and_then(Value::as_str)
            && project_summary_in(tx.conn(), named).map(|p| p.id).ok()
                != Some(batch.project.clone())
        {
            return Err(text!("batch.step.otherProject", label));
        }
        arguments.insert("project".into(), batch.project.clone().into());
        if let Some(complaint) =
            crate::mcp::complaints(&arguments, &tool["inputSchema"], "").first()
        {
            return Err(text!("batch.step.complaint", label, complaint));
        }
        let mut read = Vec::new();
        if matches!(name.as_str(), "card_create" | "card_update") {
            let mut errors = Vec::new();
            read_bodies(&mut files, &mut arguments, &mut read, &mut errors);
            unread.extend(
                errors
                    .into_iter()
                    .map(|error| text!("batch.step.bodyFrom", label = label.clone(), error)),
            );
        }
        reads.push(read);
        out.push((name, arguments));
    }
    if !unread.is_empty() {
        return Err(unread.join("\n"));
    }
    Ok((out, reads))
}
