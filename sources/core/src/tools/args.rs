//! A call's arguments as the tools read them (the surface's `value`・`targets`・`pairs`), and what several targets found wrong said together
//! (`MCPRefusals`).

use serde_json::{Map, Value};

/// A write's targets, each with its value.
pub(crate) type Pairs = Vec<(String, Map<String, Value>)>;

pub(crate) struct Args<'a> {
    pub raw: &'a Map<String, Value>,
    /// Read through MCP: a list and a body cut to their default size (`paging`). The window's requests read whole.
    pub paged: bool,
}

impl<'a> Args<'a> {
    pub fn new(raw: &'a Map<String, Value>) -> Args<'a> {
        Args { raw, paged: false }
    }

    /// The same arguments, read as MCP reads them (`paged`) or not.
    pub fn paged(mut self, paged: bool) -> Args<'a> {
        self.paged = paged;
        self
    }

    pub fn string(&self, name: &str) -> Result<&'a str, String> {
        self.raw
            .get(name)
            .and_then(Value::as_str)
            .ok_or_else(|| text!("args.required", name = name))
    }

    pub fn optional_string(&self, name: &str) -> Option<&'a str> {
        self.raw.get(name).and_then(Value::as_str)
    }

    pub fn optional_bool(&self, name: &str) -> Option<bool> {
        self.raw.get(name).and_then(Value::as_bool)
    }

    pub fn bool(&self, name: &str, fallback: bool) -> bool {
        self.optional_bool(name).unwrap_or(fallback)
    }

    /// A number as a whole one, toward zero.
    pub fn int(&self, name: &str, fallback: i64) -> i64 {
        match self.raw.get(name).and_then(Value::as_f64) {
            Some(n) => n.trunc() as i64,
            None => fallback,
        }
    }

    pub fn strings(&self, name: &str) -> Option<Vec<String>> {
        self.raw.get(name).and_then(Value::as_array).map(|items| {
            items
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
    }

    pub fn object(&self, name: &str) -> Option<&'a Map<String, Value>> {
        self.raw.get(name).and_then(Value::as_object)
    }

    /// The value, an object; empty when there is none.
    pub fn value(&self) -> Result<Map<String, Value>, String> {
        match self.raw.get("value") {
            None => Ok(Map::new()),
            Some(Value::Object(o)) => Ok(o.clone()),
            Some(_) => Err(text!("args.valueObject")),
        }
    }

    /// The target as a list; none when none was given.
    pub fn targets(&self) -> Result<Option<Vec<String>>, String> {
        match self.raw.get("target") {
            None => Ok(None),
            Some(Value::String(one)) => {
                let trimmed = one.trim();
                if trimmed.is_empty() {
                    return Err(text!("args.targetEmpty"));
                }
                Ok(Some(vec![trimmed.to_string()]))
            }
            Some(Value::Array(many)) => {
                let list: Vec<String> = many
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.trim().to_string()))
                    .collect();
                if list.is_empty() || list.len() != many.len() || list.iter().any(String::is_empty)
                {
                    return Err(text!("args.targetListShape"));
                }
                Ok(Some(list))
            }
            // A number, for what has one (an MR's): as its digits.
            Some(Value::Number(n)) if n.is_u64() => Ok(Some(vec![n.to_string()])),
            Some(_) => Err(text!("args.targetType")),
        }
    }

    pub fn target(&self) -> Result<String, String> {
        let list = self.targets()?.ok_or_else(|| text!("args.targetNeeded"))?;
        if list.len() != 1 {
            return Err(text!("args.targetOne"));
        }
        Ok(list.into_iter().next().unwrap())
    }

    /// What a write is about: a value for each target. With no target, the value is the
    /// targets themselves, each with its own value.
    pub fn pairs(&self) -> Result<Pairs, String> {
        let given = self.value()?;
        if let Some(list) = self.targets()? {
            return Ok(list.into_iter().map(|t| (t, given.clone())).collect());
        }
        if given.is_empty() {
            return Err(text!("args.targetNeededForPairs"));
        }
        given
            .into_iter()
            .map(|(key, entry)| match entry {
                Value::Object(o) => Ok((key, o)),
                _ => Err(text!("args.pairsValueShape", key = key)),
            })
            .collect()
    }
}

/// One reason a write was refused, with where it is when it said so (`MCPError.reasons`).
#[derive(Clone, Debug)]
pub(crate) struct Reason {
    pub at: Option<String>,
    pub text: String,
}

/// A refusal of several reasons, said one a line (`MCPError(reasons:)`).
pub(crate) fn said(reasons: &[Reason]) -> String {
    reasons
        .iter()
        .map(|r| match &r.at {
            Some(at) => format!("{at}: {}", r.text),
            None => r.text.clone(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// What a call writing several targets found wrong with them: every target tried, all
/// that was wrong said at once, nothing written while anything is.
pub(crate) struct Refusals {
    targets: usize,
    lines: Vec<String>,
    /// Each target written with a value of its own (no `target`): the value is keyed by the target.
    keyed: bool,
}

impl Refusals {
    /// Said one a line up to this many; the rest are counted.
    const SHOWN: usize = 20;

    pub fn new(targets: usize) -> Refusals {
        Refusals {
            targets,
            lines: Vec::new(),
            keyed: false,
        }
    }

    /// For a call that writes `value` over its targets: the places a refusal says to change are said
    /// in the call's own arguments, under `value` or, with no `target`, under the target's key.
    pub fn in_call(mut self, args: &Args) -> Refusals {
        self.keyed = !args.raw.contains_key("target");
        self
    }

    /// A refusal with the places it says to change (`fix`, said from the value of one target) moved to
    /// where the call has that value.
    fn placed(&self, target: Option<&str>, line: &str) -> String {
        let base = match (self.keyed, target) {
            (false, _) => Some("/value".to_string()),
            (true, Some(target)) => Some(format!("/value/{}", crate::mcp::pointer_key(target))),
            (true, None) => None,
        };
        crate::mcp::rebase_fix(line, base.as_deref())
    }

    pub fn note(&mut self, target: Option<&str>, error: &str) {
        for line in error.split('\n') {
            let at = if self.targets > 1 { target } else { None };
            let line = self.placed(target, line);
            self.lines.push(match at {
                Some(at) => format!("{at}: {line}"),
                None => line,
            });
        }
    }

    /// A refusal of several reasons: each said where it says it is, else at `target`
    /// when the call writes several.
    pub fn note_reasons(&mut self, target: Option<&str>, reasons: &[Reason]) {
        for reason in reasons {
            let at = reason
                .at
                .as_deref()
                .or(if self.targets > 1 { target } else { None });
            let text = self.placed(reason.at.as_deref().or(target), &reason.text);
            self.lines.push(match at {
                Some(at) => format!("{at}: {text}"),
                None => text,
            });
        }
    }

    /// `body`'s answer for `target`, or none when it was refused (and the refusal noted).
    pub fn trying<T>(
        &mut self,
        target: &str,
        body: impl FnOnce() -> Result<T, String>,
    ) -> Option<T> {
        match body() {
            Ok(v) => Some(v),
            Err(e) => {
                self.note(Some(target), &e);
                None
            }
        }
    }

    /// What was noted, if anything was.
    pub fn check(&self) -> Result<(), String> {
        if self.lines.is_empty() {
            return Ok(());
        }
        if self.lines.len() == 1 && self.targets == 1 {
            return Err(self.lines[0].clone());
        }
        let mut out = vec![text!("args.refusals.count", count = self.lines.len())];
        out.extend(
            self.lines
                .iter()
                .take(Self::SHOWN)
                .enumerate()
                .map(|(i, l)| format!("{}. {l}", i + 1)),
        );
        if self.lines.len() > Self::SHOWN {
            out.push(text!(
                "args.refusals.more",
                count = self.lines.len() - Self::SHOWN
            ));
        }
        Err(out.join("\n"))
    }
}

// MARK: - The value's shape

/// What a note's clauses became.
pub(crate) fn old_clauses() -> String {
    text!("args.oldClauses")
}

/// Why no action is made, what is written instead, and an example (`MCPTools.actionGone`):
/// a flow and the node cards inside it, made in one call.
pub(crate) fn action_gone() -> String {
    text!("args.actionGone")
}

/// A tool's form of its value, as `guide` shows it under `label` (the catalog's `forms`).
pub(crate) fn form(tool: &str, label: &str) -> &'static Value {
    static NONE: Value = Value::Null;
    crate::mcp::catalog()["tools"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|t| t["name"] == tool)
        .and_then(|t| t["forms"].as_array()?.iter().find(|f| f["label"] == label))
        .map_or(&NONE, |f| &f["spec"])
}

/// The value against its form, every complaint at once (`Surface.check`): a note's
/// clauses and an action — said how they are written now — before the form's own words.
pub(crate) fn check(
    value: &Map<String, Value>,
    form: &Value,
    tool: &str,
    label: &str,
) -> Result<(), String> {
    if value.contains_key("clauses") {
        return Err(old_clauses());
    }
    if let Some(kind) = value.get("kind").and_then(Value::as_str)
        && (kind == "action" || kind == "module")
    {
        return Err(action_gone());
    }
    // A card's form (the one with a flow's diagram) given an action's keys: a node's keys
    // are read by its kind's definition, not by this form.
    if form["properties"].get("diagram").is_some()
        && let Some(key) = ["trigger", "steps", "uses", "signedBy", "verifiedBy", "next"]
            .into_iter()
            .find(|k| value.contains_key(*k))
    {
        return Err(text!("args.cardKeyGone", key = key, gone = action_gone()));
    }
    let mut complaints: Vec<String> =
        crate::mcp::complaints_of_form(value, form, "value", tool, label)
            .into_iter()
            .map(|c| {
                c.replace(
                    &text!("args.guideAbout", tool = "value"),
                    &text!("args.guideAbout", tool = tool),
                )
            })
            .collect();
    if complaints.is_empty() {
        return Ok(());
    }
    if !complaints.iter().any(|c| c.contains("guide")) {
        complaints
            .last_mut()
            .unwrap()
            .push_str(&text!("args.valueShape", tool = tool));
    }
    Err(complaints.join("\n"))
}
