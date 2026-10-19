//! A person asked, through the window, whether MCP may do what it asked to: a call that touches an item asked each time
//! (`mcp_writes::judge`) and says `request: true` waits, two minutes at the most, for a person to
//! answer on the window (`App::ask`) — once, always, no, or never. "Always" and "never" set the items
//! where they are kept, as the window would: the project's for every key, the key's for itself
//! (`mcp_writes::always`). A call that does not say `request: true` is refused, saying it may.

use super::args::Args;
use super::mcp_writes::{self, Asked, State};
use crate::Core;
use crate::access;
use crate::write::Who;
use serde_json::{Map, Value, json};

/// How long a person has to answer.
pub const ASK_SECONDS: u64 = 120;

/// What a call is told when there is no window to ask on.
pub fn no_screen() -> String {
    text!("asking.noScreen")
}

/// How much of a call's arguments the window is shown.
const WHAT: usize = 1500;

/// A call's arguments as the window shows what it is to do: all but `request` and the project (said
/// apart; but the one `project_create` makes), cut to a length a person reads.
pub(crate) fn what_of(tool: &str, arguments: &Map<String, Value>) -> String {
    let shown: Map<String, Value> = arguments
        .iter()
        .filter(|(k, _)| *k != "request" && (*k != "project" || tool == "project_create"))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    cut(&crate::ojson::compact(&Value::Object(shown)), WHAT)
}

pub(crate) fn cut(text: &str, at: usize) -> String {
    if text.chars().count() <= at {
        text.to_string()
    } else {
        let head: String = text.chars().take(at).collect();
        format!("{head}…")
    }
}

/// The items, named as a refusal names them.
fn named(asks: &[Asked]) -> String {
    crate::text::list(&asks.iter().map(|a| a.item.called()).collect::<Vec<_>>())
}

/// Where a person changes what was said "never" to.
fn where_set(asks: &[Asked]) -> String {
    let key = asks
        .iter()
        .any(|a| matches!(a.whose, mcp_writes::Whose::Key { .. }));
    let project = asks
        .iter()
        .any(|a| matches!(a.whose, mcp_writes::Whose::Project { .. }));
    match (project, key) {
        (true, true) => text!("asking.whereSet.both"),
        (false, true) => text!("asking.whereSet.key"),
        _ => text!("asking.whereSet.project"),
    }
}

impl Core {
    /// What `asks` touch, asked of a person when the call said `request: true` (`requested`): through
    /// when they say yes, refused when they say no, take too long, or there is no window. `tool` and
    /// `what` are what the window shows the call as.
    pub(crate) fn ask_person(
        &self,
        tool: &str,
        what: String,
        asks: &[Asked],
        requested: bool,
    ) -> Result<(), String> {
        if asks.is_empty() {
            return Ok(());
        }
        if !requested {
            return Err(mcp_writes::not_requested(asks, tool));
        }
        let app = self.app().ok_or_else(no_screen)?;
        // The window's language can differ from the call's: the items are said in each language
        // too, for the window to show the ones of its own.
        let mut languages = Map::new();
        for code in crate::text::LANGUAGES.iter().copied() {
            let items = {
                let _bound = crate::text::bind(Some(code));
                mcp_writes::asked_view(asks)
            };
            languages.insert(code.to_string(), json!({ "items": items }));
        }
        let request = json!({
            "tool": tool,
            "key": access::caller().map(|c| c.name).unwrap_or_default(),
            "items": mcp_writes::asked_view(asks),
            "languages": languages,
            "what": what,
            "seconds": ASK_SECONDS,
        });
        let said: Value =
            serde_json::from_str(&app.ask(&request.to_string())).unwrap_or(Value::Null);
        if let Some(error) = said.get("error") {
            return Err(error
                .as_str()
                .map_or_else(|| error.to_string(), str::to_string));
        }
        if said.get("timeout") == Some(&Value::Bool(true)) {
            return Err(text!("asking.timeout", minutes = ASK_SECONDS / 60));
        }
        let client = said.get("client").and_then(Value::as_str).unwrap_or("");
        match said.get("answer").and_then(Value::as_str) {
            Some("once") => Ok(()),
            Some("always") => {
                self.plain_writing(Who::screen_of(client), |tx| {
                    mcp_writes::always(tx, asks, State::Allow)
                })?;
                Ok(())
            }
            Some("deny") => Err(text!("asking.denied", items = named(asks))),
            Some("never") => {
                self.plain_writing(Who::screen_of(client), |tx| {
                    mcp_writes::always(tx, asks, State::Deny)
                })?;
                Err(text!(
                    "asking.never",
                    items = named(asks),
                    place = where_set(asks)
                ))
            }
            _ => Err(text!(
                "asking.unreadable",
                said = cut(&said.to_string(), 200)
            )),
        }
    }

    /// The commit `branch_revert` names to undo (`undo`, in the tree its `target` names): its number,
    /// who made it and when, and its first lines.
    fn commit_said(&self, args: &Args) -> Option<String> {
        const SHOWN: usize = 10;
        let store = self.reading().ok()?;
        let project = super::project_summary(&store, args.string("project").ok()?).ok()?;
        let named = super::named(&store, &project.id, args.optional_string("target")).ok()?;
        let seq = args.value().ok()?.get("undo")?.as_i64()?;
        let commit =
            crate::write::history_commit(&store.conn, &project.id, &named.tree, seq).ok()??;
        let mut lines: Vec<String> = commit
            .lines
            .iter()
            .take(SHOWN)
            .map(|l| l.summary.clone())
            .collect();
        if commit.lines.len() > SHOWN {
            lines.push(text!(
                "asking.what.moreLines",
                count = commit.lines.len() - SHOWN
            ));
        }
        Some(text!(
            "asking.what.commit",
            seq,
            who = crate::write::author_said(&commit.author_kind, commit.author.as_deref()),
            at = commit.at,
            lines = lines.join(" / ")
        ))
    }

    /// A tool called on its own, judged and asked about before it runs: what a person said yes to, for
    /// `batch` to run its steps with (`execute: true`).
    pub(crate) fn admitted(
        &self,
        who: &Who,
        name: &str,
        args: &Args,
    ) -> Result<Vec<&'static str>, String> {
        let (asks, what) = if name == "batch" {
            self.batch_asks(who, args)?
        } else {
            let asks = mcp_writes::judge(&self.reading()?.conn, who, name, args, None)?;
            let mut what = what_of(name, args.raw);
            // Another writer's commit to undo: which one, as the history says it, for the person to
            // see what they allow.
            if asks.iter().any(|a| a.item.id == "branch_revert.others")
                && let Some(commit) = self.commit_said(args)
            {
                what = format!("{what}\n{commit}");
            }
            (asks, what)
        };
        self.ask_person(name, what, &asks, args.bool("request", false))?;
        Ok(asks.iter().map(|a| a.item.id).collect())
    }
}
