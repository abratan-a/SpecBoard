//! Projects shared between agents over a hub (`specboard://SpecBoard/Docs/Design:DesignHub`).
//!
//! An agent that joined a hub keeps, in its store: the hub's members as it was last told them
//! (`hub_members`), which of its own projects it shares with which member (`hub_shares`), the
//! projects the members share with it (`hub_projects`) and which of those each access key may use
//! (`hub_grants`). Whether it is linked now, and which members are, is kept only while the core is
//! open (`Now`): the agent tells it each time it changes (`hub_state`).
//!
//! The side that uses a shared project: a tool call through MCP whose `project` names one the
//! call's key was given is not run here. What the key may do, the link and the owner's build are
//! checked, the call is handed to the owner by the way the agent gave (`Hub`), and what the owner
//! answers is answered as it is (`Core::hub_named`, `Core::hub_relay`). No copy of the project is
//! kept, so nothing read is older than what its owner holds.
//!
//! The side that owns it: a call handed over is run as the member it came from (`Core::hub_call`).
//! It sees the projects shared with that member alone — what is public here is for this agent's
//! own keys —, writes them only where both the share and the level sent let it, and never touches
//! a folder of this host (`refuse_folders`): the session behind it is another user's of another
//! host, and would write files as this agent's user.
//!
//! Shares are set by the window's requests alone (`hub_share_set`; a key's with `access_grant`):
//! no tool of MCP's changes them, so an agent cannot share more to itself.

use crate::Core;
use crate::access::{self, Caller, Level, Member, Need, Permits};
use crate::mcp::Output;
use crate::store::sql;
use crate::tools::Args;
use crate::write::{Who, db};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Map, Value, json};
use std::collections::HashMap;

/// The way to the other agents of the hub: the agent's, handed over with `Core::set_hub`.
///
/// `call` hands `request` (JSON, a call's body: `{"k": "call", "tool", "arguments", "key", "level",
/// "build", "lang"}`) to the agent `owner` (its id in the hub) and waits for what it answers:
/// `{"output": {"json": …} | {"blocks": […]} | {"image": {"data", "mimeType", "note"}}}`,
/// `{"error": <the tool's refusal as it said it>}`, `{"failed": "unshared"}` when the owner does not
/// share the project with this agent, or `{"failed": "cut" | "offline" | "down" | "slow"}` when it
/// was not answered — the way was cut after the request left, the owner is not linked to the hub,
/// this agent is not, the owner is linked and did not answer in the time it is given. `wait` waits
/// first for this agent's link to be there again (a read asked once more after a cut).
///
/// Called on the thread that took the MCP request, with no lock of the store held.
pub trait Hub: Send + Sync {
    fn call(&self, owner: &str, request: &str, wait: bool) -> String;
}

/// What the agent told of the hub last (`hub_state`).
#[derive(Clone, Debug, Default)]
pub(crate) struct Now {
    /// This agent's id in the hub; none while it has joined none.
    me: Option<String>,
    /// This agent's build.
    build: String,
    /// Whether this agent is linked to the hub now.
    up: bool,
    /// The members linked to the hub now, each with its build (empty for one that said none: a
    /// build of the agent's own, made for no app's).
    linked: HashMap<String, String>,
}

impl Now {
    /// Why a call cannot be handed to the member `owner` now; none when it can. A member of
    /// another build is not handed one: the tools are described by this side and answered by the
    /// owner's, and two builds may not mean the same by them.
    fn unreachable(&self, owner: &str, owner_name: &str) -> Option<String> {
        if !self.up {
            return Some(text!("hub.down"));
        }
        match self.linked.get(owner) {
            None => Some(text!("hub.ownerAway", owner = owner_name)),
            Some(build) if *build != self.build => {
                Some(text!("hub.otherBuild", owner = owner_name))
            }
            Some(_) => None,
        }
    }
}

/// What a member's write to a project it may only read is told: the share's, or the level its key
/// was given on its own side.
pub(crate) fn read_only(project: &str) -> String {
    text!("hub.readOnly", project)
}

fn no_member(member: &str) -> String {
    text!("hub.noMember", member)
}

/// A request's text at `key`, or what it lacks.
fn text_of<'a>(request: &'a Value, key: &str, op: &str, what: &str) -> Result<&'a str, String> {
    request
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| text!("access.needs", op, key, what))
}

/// A level as a request says it: none for null or nothing said (taken away).
fn level_of(said: Option<&Value>) -> Result<Option<Level>, String> {
    match said {
        None | Some(Value::Null) => Ok(None),
        Some(other) => other
            .as_str()
            .and_then(Level::of)
            .map(Some)
            .ok_or_else(|| text!("access.levelWord", value = other)),
    }
}

/// A member's name, as the hub's members were last told; none for one that is not among them.
fn member_name(conn: &Connection, member: &str) -> Result<Option<String>, String> {
    conn.query_row("SELECT name FROM hub_members WHERE id = ?", [member], |r| {
        r.get(0)
    })
    .optional()
    .map_err(sql)
}

// MARK: - The side that owns a shared project

/// The projects shared with `member`, each with what a call of its may do there: the lower of the
/// share and the level the call was sent with (`access::Sight`).
pub(crate) fn shared_with(
    conn: &Connection,
    member: &Member,
) -> Result<HashMap<String, Level>, String> {
    let mut stmt = conn
        .prepare("SELECT project_id, level FROM hub_shares WHERE member = ?")
        .map_err(sql)?;
    let rows = stmt
        .query_map([&member.id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    Ok(rows
        .into_iter()
        .map(|(project, level)| {
            let share = Level::of(&level).unwrap_or(Level::Read);
            (project, share.min(member.level))
        })
        .collect())
}

/// A project's shares, kept over a project read in from a file in its place (`access::kept`): each
/// member and its level.
pub(crate) fn shares_of(conn: &Connection, project: &str) -> Result<Vec<(String, Level)>, String> {
    let mut stmt = conn
        .prepare("SELECT member, level FROM hub_shares WHERE project_id = ? ORDER BY member")
        .map_err(sql)?;
    let rows = stmt
        .query_map([project], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    Ok(rows
        .into_iter()
        .map(|(member, level)| (member, Level::of(&level).unwrap_or(Level::Read)))
        .collect())
}

/// A project shared with a member at a level; none takes the share away.
pub(crate) fn set_share(
    conn: &Connection,
    project: &str,
    member: &str,
    level: Option<Level>,
) -> Result<(), String> {
    match level {
        None => conn
            .execute(
                "DELETE FROM hub_shares WHERE project_id = ? AND member = ?",
                [project, member],
            )
            .map(|_| ())
            .map_err(db),
        Some(level) => conn
            .execute(
                "INSERT INTO hub_shares (project_id, member, level) VALUES (?1, ?2, ?3) \
                 ON CONFLICT (project_id, member) DO UPDATE SET level = excluded.level",
                params![project, member, level.word()],
            )
            .map(|_| ())
            .map_err(db),
    }
}

/// What of a tool call touches a folder of this host, as a refusal names it: writing an export out,
/// choosing the folder a working directory is read from or an export is written to, or naming a
/// folder to compare an export with. None for a call that touches none — reading what is in a
/// folder bound already is not one.
fn folder_touched(tool: &str, args: &Args) -> Option<String> {
    let of = |key: &str| text!("hub.call.folderOf", tool, key);
    match tool {
        "export_execute" => Some(tool.to_string()),
        "section_update"
            if args
                .pairs()
                .is_ok_and(|pairs| pairs.iter().any(|(_, v)| v.contains_key("folder"))) =>
        {
            Some(of("folder"))
        }
        "export_create" | "export_update" if args.value().is_ok_and(|v| v.contains_key("path")) => {
            Some(of("path"))
        }
        "export_read"
            if args
                .optional_string("targetdir")
                .is_some_and(|t| t.trim() != crate::tools::files::DEFAULT) =>
        {
            Some(of("targetdir"))
        }
        _ => None,
    }
}

/// Refuses a call handed over the hub that touches a folder of this host, whatever the project's
/// items let MCP do: for a tool called on its own, and for each step of a batch.
pub(crate) fn refuse_folders(tool: &str, args: &Args) -> Result<(), String> {
    if access::caller().is_none_or(|c| c.member.is_none()) {
        return Ok(());
    }
    match folder_touched(tool, args) {
        Some(what) => Err(text!("hub.call.folder", what)),
        None => Ok(()),
    }
}

/// A tool's answer as a call's answer carries it over the hub.
fn output_said(output: Output) -> Value {
    match output {
        Output::Json(value) => json!({ "json": value }),
        Output::Blocks(blocks) => json!({ "blocks": blocks }),
        Output::Image {
            data,
            mime_type,
            note,
        } => json!({ "image": { "data": data, "mimeType": mime_type, "note": note } }),
    }
}

// MARK: - The side that uses a shared project

/// A project another agent shares with this one, that an access key was given.
#[derive(Clone, Debug)]
pub(crate) struct Shared {
    /// The owner's id in the hub, and its name there.
    owner: String,
    owner_name: String,
    /// The project's id in its owner's store.
    project: String,
    name: String,
    slug: String,
    /// What the owner shares it for, and what the key was given.
    share: Level,
    grant: Level,
}

impl Shared {
    /// How a call names it whatever else is called the same: `<name>@<owner>`.
    fn at_owner(&self) -> String {
        format!("{}@{}", self.name, self.owner_name)
    }
}

/// The shared projects the key `key` was given, by name, then by owner.
fn granted(conn: &Connection, key: &str) -> Result<Vec<Shared>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT p.owner, COALESCE(m.name, p.owner), p.project_id, p.name, p.slug, p.level, g.level \
             FROM hub_grants g \
             JOIN hub_projects p ON p.owner = g.owner AND p.project_id = g.project_id \
             LEFT JOIN hub_members m ON m.id = p.owner \
             WHERE g.key_id = ? \
             ORDER BY p.name, COALESCE(m.name, p.owner), p.owner, p.project_id",
        )
        .map_err(sql)?;
    let level = |word: String| Level::of(&word).unwrap_or(Level::Read);
    stmt.query_map([key], |r| {
        Ok(Shared {
            owner: r.get(0)?,
            owner_name: r.get(1)?,
            project: r.get(2)?,
            name: r.get(3)?,
            slug: r.get(4)?,
            share: level(r.get(5)?),
            grant: level(r.get(6)?),
        })
    })
    .map_err(sql)?
    .collect::<Result<Vec<_>, _>>()
    .map_err(sql)
}

/// The shared projects the call's key was given, each as a call names it whatever else is called
/// the same (`<name>@<owner>`): what a refusal lists beside this store's own. None for the window
/// and for a call handed over the hub.
pub(crate) fn names_for_call(conn: &Connection) -> Result<Vec<String>, String> {
    match access::caller() {
        Some(caller) if caller.member.is_none() => Ok(granted(conn, &caller.key)?
            .iter()
            .map(Shared::at_owner)
            .collect()),
        _ => Ok(Vec::new()),
    }
}

/// Whether `named` names a project of this store the call's key may see: by id, name or slug, or
/// by its name in any case, as a project is looked up (`tools::project_summary_in`). One's own
/// comes before a shared one of the same name.
fn names_own(conn: &Connection, named: &str) -> Result<bool, String> {
    let sight = access::Sight::of_call(conn)?;
    let mut stmt = conn
        .prepare("SELECT id, name, slug FROM projects")
        .map_err(sql)?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    Ok(rows.into_iter().any(|(id, name, slug)| {
        sight.level(&id).is_some()
            && (id == named
                || name == named
                || slug == named
                || crate::pathtext::same(&name, named))
    }))
}

/// The one of `among` that `named` names — by id, by name, by slug, then by name in any case, as
/// this store's own are looked up; refused, with how to name each, when it names several (two
/// owners share a project of one name, or copies of one project).
fn by_name<'a>(among: &[&'a Shared], named: &str) -> Result<Option<&'a Shared>, String> {
    type Is = fn(&Shared, &str) -> bool;
    let ways: [Is; 4] = [
        |s, named| s.project == named,
        |s, named| s.name == named,
        |s, named| s.slug == named,
        |s, named| crate::pathtext::same(&s.name, named),
    ];
    for is in ways {
        let hits: Vec<&Shared> = among.iter().copied().filter(|s| is(s, named)).collect();
        match hits.as_slice() {
            [] => continue,
            [one] => return Ok(Some(one)),
            many => {
                // Two of one owner's by one name are told apart by their ids.
                let way = |s: &Shared| {
                    if many.iter().filter(|o| o.owner == s.owner).count() > 1 {
                        format!("{}@{}", s.project, s.owner_name)
                    } else {
                        s.at_owner()
                    }
                };
                return Err(text!(
                    "hub.twice",
                    name = named,
                    count = many.len(),
                    ways = many.iter().map(|s| way(s)).collect::<Vec<_>>().join(" / ")
                ));
            }
        }
    }
    Ok(None)
}

/// The shared project `named` names among those a key was given: as it is written, or as
/// `<name>@<owner>` (the owner by its name in the hub, in any case, or by its id there) — read at
/// each `@` of it, for a project's name and a member's may hold one too. What reads as two
/// projects names neither: each is then named by its id at its owner's id, which hold none.
fn found<'a>(granted: &'a [Shared], named: &str) -> Result<Option<&'a Shared>, String> {
    let all: Vec<&Shared> = granted.iter().collect();
    let mut hits: Vec<&Shared> = by_name(&all, named)?.into_iter().collect();
    for (at, _) in named.match_indices('@') {
        let (name, owner) = (&named[..at], &named[at + 1..]);
        let of_owner: Vec<&Shared> = granted
            .iter()
            .filter(|s| s.owner == owner || crate::pathtext::same(&s.owner_name, owner))
            .collect();
        if let Some(one) = by_name(&of_owner, name)?
            && !hits.iter().any(|hit| std::ptr::eq(*hit, one))
        {
            hits.push(one);
        }
    }
    match hits.as_slice() {
        [] => Ok(None),
        [one] => Ok(Some(one)),
        many => Err(text!(
            "hub.twice",
            name = named,
            count = many.len(),
            ways = granted
                .iter()
                .filter(|s| many.iter().any(|hit| std::ptr::eq(*hit, *s)))
                .map(|s| format!("{}@{}", s.project, s.owner))
                .collect::<Vec<_>>()
                .join(" / ")
        )),
    }
}

/// What the owner said to a call handed to it, or why it said nothing.
enum Answer {
    Output(Output),
    /// The tool's refusal, as the owner's core said it.
    Refused(String),
    /// Not answered: `cut`, `offline`, `down` or `slow` (`Hub::call`).
    Failed(String),
    Unreadable,
}

fn answer_of(said: &str) -> Answer {
    let Ok(Value::Object(mut said)) = serde_json::from_str::<Value>(said) else {
        return Answer::Unreadable;
    };
    if let Some(Value::String(why)) = said.get("failed") {
        return Answer::Failed(why.clone());
    }
    if let Some(Value::String(refusal)) = said.get("error") {
        return Answer::Refused(refusal.clone());
    }
    let Some(Value::Object(mut output)) = said.remove("output") else {
        return Answer::Unreadable;
    };
    if let Some(value) = output.remove("json") {
        return Answer::Output(Output::Json(value));
    }
    if let Some(Value::Array(blocks)) = output.get("blocks") {
        let texts: Option<Vec<String>> = blocks
            .iter()
            .map(|b| b.as_str().map(str::to_string))
            .collect();
        return texts.map_or(Answer::Unreadable, |t| Answer::Output(Output::Blocks(t)));
    }
    if let Some(Value::Object(image)) = output.get("image")
        && let (Some(Value::String(data)), Some(Value::String(mime_type))) =
            (image.get("data"), image.get("mimeType"))
    {
        return Answer::Output(Output::Image {
            data: data.clone(),
            mime_type: mime_type.clone(),
            note: image.get("note").cloned().unwrap_or(Value::Null),
        });
    }
    Answer::Unreadable
}

impl Core {
    /// The way to the other agents now, if the agent handed one over: taken out, so it is not held
    /// while an owner answers.
    fn hub(&self) -> Option<std::sync::Arc<dyn Hub>> {
        self.hub.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// What the agent told of the hub last.
    fn hub_now(&self) -> Now {
        self.hub_now
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// The shared project a tool call through MCP names, when it names one the call's key was
    /// given: the call is its owner's to answer (`hub_relay`). None for a call that names a project
    /// of this store the key may see, or nothing the key was given — run here, and refused here in
    /// the words of a project that is not there. A project is made and the projects are listed
    /// here, and the windows asked about are this agent's own.
    pub(crate) fn hub_named(
        &self,
        tool: &str,
        arguments: &Map<String, Value>,
    ) -> Result<Option<Shared>, String> {
        if matches!(tool, "project_create" | "project_list") {
            return Ok(None);
        }
        let Some(named) = arguments.get("project").and_then(Value::as_str) else {
            return Ok(None);
        };
        if tool == "app_control" && arguments.get("op").and_then(Value::as_str) == Some("windows") {
            return Ok(None);
        }
        // A call handed over the hub is not handed on: only its owner shares a project.
        let Some(caller) = access::caller().filter(|c| c.member.is_none()) else {
            return Ok(None);
        };
        let store = self.reading()?;
        let granted = granted(&store.conn, &caller.key)?;
        if granted.is_empty() || names_own(&store.conn, named)? {
            return Ok(None);
        }
        Ok(found(&granted, named)?.cloned())
    }

    /// A tool call handed to the owner of the shared project it names, answered as the owner
    /// answers it. Judged here first, by what this side knows: what the key was given and the
    /// key's own items (the owner does not know this side's keys), the share as the owner last
    /// said it, then whether the owner can be reached. A call the way lost is asked once more when
    /// it only reads; a write is not, for the owner may have written it.
    pub(crate) fn hub_relay(
        &self,
        shared: &Shared,
        tool: &str,
        arguments: &Map<String, Value>,
    ) -> Result<Output, String> {
        let need = access::need(tool);
        // A write where the key or the share lets it only read, in the words of the one that
        // holds it back: the key's is set here, the share at its owner.
        if need == Need::Write && shared.grant == Level::Read {
            return Err(access::read_only(&shared.name));
        }
        if need == Need::Write && shared.share == Level::Read {
            return Err(read_only(&shared.name));
        }
        let args = Args::new(arguments);
        let asks = crate::tools::mcp_writes::judge_key(tool, &args)?;
        let now = self.hub_now();
        if let Some(why) = now.unreachable(&shared.owner, &shared.owner_name) {
            return Err(why);
        }
        let Some(way) = self.hub() else {
            return Err(text!("hub.down"));
        };
        self.ask_person(
            tool,
            crate::tools::asking::what_of(tool, arguments),
            &asks,
            args.bool("request", false),
        )?;
        let handed = self.hub_handed(shared, tool, arguments)?;
        let request = json!({
            "k": "call",
            "tool": tool,
            "arguments": handed,
            "key": access::caller().map(|c| c.name).unwrap_or_default(),
            "level": shared.grant.word(),
            "build": now.build,
            "lang": crate::text::current(),
        })
        .to_string();
        let reads = matches!(need, Need::Read | Need::None);
        let mut answer = answer_of(&way.call(&shared.owner, &request, false));
        let mut asked_again = false;
        if reads && matches!(&answer, Answer::Failed(why) if why == "cut") {
            answer = answer_of(&way.call(&shared.owner, &request, true));
            asked_again = true;
        }
        match answer {
            Answer::Output(output) => Ok(output),
            Answer::Refused(refusal) => Err(refusal),
            Answer::Failed(why) => Err(match why.as_str() {
                "cut" if asked_again => text!("hub.cutRead"),
                "cut" => text!("hub.cut"),
                "offline" => text!("hub.ownerAway", owner = shared.owner_name),
                // Not asked once more, a read either: its owner is there, with the first to answer.
                "slow" => text!("hub.slow", owner = shared.owner_name),
                "unshared" => text!(
                    "hub.unshared",
                    project = shared.name,
                    owner = shared.owner_name
                ),
                _ => text!("hub.down"),
            }),
            Answer::Unreadable => Err(text!("hub.unreadable")),
        }
    }

    /// A call's arguments as they are handed to the owner, which knows the project by its id
    /// alone: `project` is the id, in the call and — in a batch — in each step that names the
    /// project. A step that names another is refused here, as a batch of this store's own refuses
    /// it: its owner could not read this side's names.
    fn hub_handed(
        &self,
        shared: &Shared,
        tool: &str,
        arguments: &Map<String, Value>,
    ) -> Result<Map<String, Value>, String> {
        let mut handed = arguments.clone();
        handed.insert("project".into(), shared.project.clone().into());
        if tool != "batch" {
            return Ok(handed);
        }
        let steps = handed
            .get_mut("value")
            .and_then(|value| value.get_mut("steps"))
            .and_then(Value::as_array_mut);
        for (offset, step) in steps.into_iter().flatten().enumerate() {
            let name = step["tool"].as_str().unwrap_or_default().to_string();
            let Some(named) = step
                .get("arguments")
                .and_then(|a| a.get("project"))
                .and_then(Value::as_str)
            else {
                continue;
            };
            let naming = Map::from_iter([("project".to_string(), Value::from(named))]);
            let same = self
                .hub_named(tool, &naming)?
                .is_some_and(|s| s.owner == shared.owner && s.project == shared.project);
            if !same {
                let label = text!("batch.step.label", offset, name);
                return Err(text!("batch.step.otherProject", label));
            }
            step["arguments"]["project"] = shared.project.clone().into();
        }
        Ok(handed)
    }

    /// The shared projects the call's key was given, as `project_list` lists them beside this
    /// store's own: each with its owner and the lower of the share and the key's grant, and why it
    /// cannot be reached now when it cannot. Read from what this store keeps: listing asks no owner.
    pub(crate) fn hub_listed(&self, conn: &Connection) -> Result<Vec<Value>, String> {
        let Some(caller) = access::caller().filter(|c| c.member.is_none()) else {
            return Ok(Vec::new());
        };
        let granted = granted(conn, &caller.key)?;
        if granted.is_empty() {
            return Ok(Vec::new());
        }
        let now = self.hub_now();
        Ok(granted
            .into_iter()
            .map(|shared| {
                let mut row = Map::new();
                row.insert("project".into(), shared.name.clone().into());
                row.insert("owner".into(), shared.owner_name.clone().into());
                row.insert(
                    "access".into(),
                    shared.share.min(shared.grant).word().into(),
                );
                if let Some(why) = now.unreachable(&shared.owner, &shared.owner_name) {
                    row.insert("reachable".into(), false.into());
                    row.insert("why".into(), why.into());
                }
                Value::Object(row)
            })
            .collect())
    }
}

// MARK: - The agent's and the window's requests

/// A key given a shared project (`access_grant` with `owner`); none takes it away. Only one its
/// owner shares now.
pub(crate) fn set_grant(
    conn: &Connection,
    key: &str,
    owner: &str,
    project: &str,
    level: Option<Level>,
) -> Result<(), String> {
    let shared = conn
        .query_row(
            "SELECT 1 FROM hub_projects WHERE owner = ? AND project_id = ?",
            [owner, project],
            |_| Ok(()),
        )
        .optional()
        .map_err(sql)?
        .is_some();
    if !shared {
        return Err(text!("hub.grant.noProject"));
    }
    match level {
        None => conn
            .execute(
                "DELETE FROM hub_grants WHERE key_id = ? AND owner = ? AND project_id = ?",
                [key, owner, project],
            )
            .map(|_| ())
            .map_err(db),
        Some(level) => conn
            .execute(
                "INSERT INTO hub_grants (key_id, owner, project_id, level) VALUES (?1, ?2, ?3, ?4) \
                 ON CONFLICT (key_id, owner, project_id) DO UPDATE SET level = excluded.level",
                params![key, owner, project, level.word()],
            )
            .map(|_| ())
            .map_err(db),
    }
}

/// The shared projects a key was given, as its row lists them (`access_keys`): `[{owner, project,
/// level}]`, by owner, then by project.
pub(crate) fn grants_said(conn: &Connection, key: &str) -> Result<Value, String> {
    let mut stmt = conn
        .prepare(
            "SELECT owner, project_id, level FROM hub_grants WHERE key_id = ? ORDER BY owner, project_id",
        )
        .map_err(sql)?;
    let rows = stmt
        .query_map([key], |r| {
            Ok(json!({
                "owner": r.get::<_, String>(0)?,
                "project": r.get::<_, String>(1)?,
                "level": r.get::<_, String>(2)?,
            }))
        })
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    Ok(Value::Array(rows))
}

/// The members a request tells (`hub_state`): each one's id and name, whether it is linked, and
/// its build.
type Told = Vec<(String, String, bool, Option<String>)>;

fn told_members(request: &Value) -> Result<Told, String> {
    let unreadable = || text!("hub.state.members");
    let Some(list) = request.get("members").and_then(Value::as_array) else {
        return Err(unreadable());
    };
    list.iter()
        .map(|member| {
            let id = member.get("id").and_then(Value::as_str);
            let name = member.get("name").and_then(Value::as_str);
            match (id, name) {
                (Some(id), Some(name)) => Ok((
                    id.to_string(),
                    name.to_string(),
                    member.get("online").and_then(Value::as_bool) == Some(true),
                    member
                        .get("build")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                )),
                _ => Err(unreadable()),
            }
        })
        .collect()
}

impl Core {
    /// The requests about the hub — the agent's, as its link and the hub's members change and as
    /// calls and shares come in, and the window's, to read and set the shares; none when `op` is
    /// not one of them.
    pub(crate) fn hub_op(&self, op: &str, request: &Value) -> Option<Result<Value, String>> {
        let known = [
            "hub_state",
            "hub_shares",
            "hub_share_set",
            "hub_outgoing",
            "hub_incoming",
            "hub_call",
        ];
        known.contains(&op).then(|| match op {
            "hub_state" => self.hub_state(request),
            "hub_shares" => self.hub_shares(),
            "hub_share_set" => self.hub_share_set(request),
            "hub_outgoing" => self.hub_outgoing(),
            "hub_incoming" => self.hub_incoming(request),
            _ => self.hub_call(request),
        })
    }

    /// Something of the hub changed that the agent and the windows show or pass on.
    fn hub_told(&self) {
        self.tell(&json!({ "op": "hub" }));
    }

    /// What the agent says of the hub (`{self, build, up, members}`): who this agent is there, its
    /// build, whether it is linked, and the members as the hub said them. Linked, the members are
    /// kept as told, and what a member no longer among them left — the shares to it, the projects
    /// it shared and the keys' grants to them — is let go of. Not linked, nothing is known of who
    /// is a member now, so nothing is let go of. Joined to no hub (`self` null), nothing is kept.
    fn hub_state(&self, request: &Value) -> Result<Value, String> {
        let me = request
            .get("self")
            .and_then(Value::as_str)
            .map(str::to_string);
        let up = me.is_some() && request.get("up").and_then(Value::as_bool) == Some(true);
        let members = if up {
            told_members(request)?
        } else {
            Vec::new()
        };
        self.plain_writing(Who::screen_for(request), |tx| {
            let conn = tx.conn();
            if me.is_none() {
                return conn
                    .execute_batch(
                        "DELETE FROM hub_grants; DELETE FROM hub_projects; \
                         DELETE FROM hub_shares; DELETE FROM hub_members;",
                    )
                    .map_err(db);
            }
            if !up {
                return Ok(());
            }
            // Written anew, in the hub's order: the windows list them so.
            conn.execute("DELETE FROM hub_members", []).map_err(db)?;
            for (id, name, _, _) in &members {
                conn.execute(
                    "INSERT INTO hub_members (id, name) VALUES (?1, ?2) \
                     ON CONFLICT (id) DO UPDATE SET name = excluded.name",
                    params![id, name],
                )
                .map_err(db)?;
            }
            conn.execute_batch(
                "DELETE FROM hub_shares WHERE member NOT IN (SELECT id FROM hub_members); \
                 DELETE FROM hub_grants WHERE owner NOT IN (SELECT id FROM hub_members); \
                 DELETE FROM hub_projects WHERE owner NOT IN (SELECT id FROM hub_members);",
            )
            .map_err(db)
        })?;
        *self.hub_now.lock().unwrap_or_else(|e| e.into_inner()) = Now {
            build: request
                .get("build")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            up,
            linked: members
                .into_iter()
                .filter(|(id, _, online, _)| *online && Some(id) != me.as_ref())
                .map(|(id, _, _, build)| (id, build.unwrap_or_default()))
                .collect(),
            me,
        };
        self.hub_told();
        Ok(json!({}))
    }

    /// What the window shows of the hub: the other members (`members`: who a project can be shared
    /// with), this agent's shares (`shares`, by project) and the projects shared with it (`shared`).
    fn hub_shares(&self) -> Result<Value, String> {
        let now = self.hub_now();
        let store = self.reading()?;
        let conn = &store.conn;
        let members: Vec<(String, String)> = {
            let mut stmt = conn
                .prepare("SELECT id, name FROM hub_members ORDER BY rowid")
                .map_err(sql)?;
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .map_err(sql)?
                .collect::<Result<_, _>>()
                .map_err(sql)?
        };
        let others: Vec<Value> = members
            .iter()
            .filter(|(id, _)| Some(id) != now.me.as_ref())
            .map(|(id, name)| {
                let linked = now.linked.get(id).filter(|_| now.up);
                let mut said = Map::new();
                said.insert("id".into(), id.clone().into());
                said.insert("name".into(), name.clone().into());
                said.insert("online".into(), linked.is_some().into());
                if let Some(build) = linked.filter(|build| !build.is_empty()) {
                    said.insert("build".into(), build.clone().into());
                }
                Value::Object(said)
            })
            .collect();
        let mut shares: Vec<Value> = Vec::new();
        {
            let mut stmt = conn
                .prepare(
                    "SELECT s.project_id, p.name, s.member, COALESCE(m.name, s.member), s.level \
                     FROM hub_shares s JOIN projects p ON p.id = s.project_id \
                     LEFT JOIN hub_members m ON m.id = s.member \
                     ORDER BY p.name, p.id, m.rowid, s.member",
                )
                .map_err(sql)?;
            let rows = stmt
                .query_map([], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, String>(4)?,
                    ))
                })
                .map_err(sql)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(sql)?;
            for (project, name, member, member_name, level) in rows {
                let to = json!({ "member": member, "name": member_name, "level": level });
                match shares
                    .last_mut()
                    .filter(|s| s["project"] == project.as_str())
                {
                    Some(share) => {
                        if let Some(list) = share["to"].as_array_mut() {
                            list.push(to);
                        }
                    }
                    None => shares.push(json!({ "project": project, "name": name, "to": [to] })),
                }
            }
        }
        let shared: Vec<Value> = {
            let mut stmt = conn
                .prepare(
                    "SELECT p.owner, COALESCE(m.name, p.owner), p.project_id, p.name, p.level \
                     FROM hub_projects p LEFT JOIN hub_members m ON m.id = p.owner \
                     ORDER BY p.name, COALESCE(m.name, p.owner), p.owner, p.project_id",
                )
                .map_err(sql)?;
            let rows = stmt
                .query_map([], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, String>(4)?,
                    ))
                })
                .map_err(sql)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(sql)?;
            rows.into_iter()
                .map(|(owner, owner_name, project, name, level)| {
                    let why = now.unreachable(&owner, &owner_name);
                    let mut said = json!({
                        "owner": owner,
                        "ownerName": owner_name,
                        "project": project,
                        "name": name,
                        "level": level,
                        "reachable": why.is_none(),
                    });
                    if let Some(why) = why {
                        said["why"] = why.into();
                    }
                    said
                })
                .collect()
        };
        Ok(json!({ "members": others, "shares": shares, "shared": shared }))
    }

    /// A project of this store shared with a member to read or to write, or no more (`level`
    /// null): `{project, member, level}`.
    fn hub_share_set(&self, request: &Value) -> Result<Value, String> {
        let op = "hub_share_set";
        let project = text_of(request, "project", op, &text!("access.what.projectId"))?;
        let member = text_of(request, "member", op, &text!("hub.what.member"))?;
        let level = level_of(request.get("level"))?;
        if Some(member) == self.hub_now().me.as_deref() {
            return Err(text!("hub.share.self"));
        }
        self.plain_writing(Who::screen_for(request), |tx| {
            let conn = tx.conn();
            if member_name(conn, member)?.is_none() {
                return Err(no_member(member));
            }
            access::project_there(conn, project)?;
            set_share(conn, project, member, level)
        })?;
        self.hub_told();
        Ok(json!({}))
    }

    /// What each other member is to be told this agent shares with it (`{<member>: [{id, name,
    /// slug, level}]}`): nothing, for one shared nothing — it is told so too, to let go of what it
    /// was told before.
    fn hub_outgoing(&self) -> Result<Value, String> {
        let now = self.hub_now();
        let store = self.reading()?;
        let conn = &store.conn;
        let mut out = Map::new();
        {
            let mut stmt = conn
                .prepare("SELECT id FROM hub_members ORDER BY rowid")
                .map_err(sql)?;
            let members = stmt
                .query_map([], |r| r.get::<_, String>(0))
                .map_err(sql)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(sql)?;
            for member in members {
                if Some(&member) != now.me.as_ref() {
                    out.insert(member, Value::Array(Vec::new()));
                }
            }
        }
        let mut stmt = conn
            .prepare(
                "SELECT s.member, p.id, p.name, p.slug, s.level \
                 FROM hub_shares s JOIN projects p ON p.id = s.project_id ORDER BY p.name, p.id",
            )
            .map_err(sql)?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    json!({
                        "id": r.get::<_, String>(1)?,
                        "name": r.get::<_, String>(2)?,
                        "slug": r.get::<_, String>(3)?,
                        "level": r.get::<_, String>(4)?,
                    }),
                ))
            })
            .map_err(sql)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql)?;
        for (member, project) in rows {
            if let Some(Value::Array(list)) = out.get_mut(&member) {
                list.push(project);
            }
        }
        Ok(Value::Object(out))
    }

    /// What a member said it shares with this agent (`{owner, projects: [{id, name, slug,
    /// level}]}`), kept in place of what it said before: a project it shares no more goes, with the
    /// keys' grants to it; one it still shares keeps them.
    fn hub_incoming(&self, request: &Value) -> Result<Value, String> {
        let op = "hub_incoming";
        let owner = text_of(request, "owner", op, &text!("hub.what.member"))?;
        let unreadable = || text!("hub.incoming.projects");
        let Some(list) = request.get("projects").and_then(Value::as_array) else {
            return Err(unreadable());
        };
        let mut projects = Vec::new();
        for project in list {
            let word = |key: &str| project.get(key).and_then(Value::as_str);
            let level = word("level").and_then(Level::of);
            match (word("id"), word("name"), word("slug"), level) {
                (Some(id), Some(name), Some(slug), Some(level)) => {
                    projects.push((id, name, slug, level))
                }
                _ => return Err(unreadable()),
            }
        }
        if Some(owner) == self.hub_now().me.as_deref() {
            return Err(text!("hub.share.self"));
        }
        self.plain_writing(Who::screen_for(request), |tx| {
            let conn = tx.conn();
            if member_name(conn, owner)?.is_none() {
                return Err(no_member(owner));
            }
            let had: Vec<String> = {
                let mut stmt = conn
                    .prepare("SELECT project_id FROM hub_projects WHERE owner = ?")
                    .map_err(sql)?;
                stmt.query_map([owner], |r| r.get(0))
                    .map_err(sql)?
                    .collect::<Result<_, _>>()
                    .map_err(sql)?
            };
            for gone in had
                .iter()
                .filter(|id| !projects.iter().any(|(still, ..)| still == id))
            {
                conn.execute(
                    "DELETE FROM hub_grants WHERE owner = ? AND project_id = ?",
                    [owner, gone.as_str()],
                )
                .map_err(db)?;
                conn.execute(
                    "DELETE FROM hub_projects WHERE owner = ? AND project_id = ?",
                    [owner, gone.as_str()],
                )
                .map_err(db)?;
            }
            for (id, name, slug, level) in &projects {
                conn.execute(
                    "INSERT INTO hub_projects (owner, project_id, name, slug, level) VALUES (?1, ?2, ?3, ?4, ?5) \
                     ON CONFLICT (owner, project_id) DO UPDATE SET \
                     name = excluded.name, slug = excluded.slug, level = excluded.level",
                    params![owner, id, name, slug, level.word()],
                )
                .map_err(db)?;
            }
            Ok(())
        })?;
        self.hub_told();
        Ok(json!({}))
    }

    /// A call another member handed over the hub (`{member, request}`; `request` the call's body,
    /// as `Hub::call` hands it), run as that member: `{output}` as the tool answered, `{error}`
    /// its refusal as it said it, or `{failed: "unshared"}` for a project not shared with that
    /// member. Only a member the hub named is run for, and one of this agent's build.
    fn hub_call(&self, request: &Value) -> Result<Value, String> {
        let op = "hub_call";
        let member = text_of(request, "member", op, &text!("hub.what.member"))?;
        let unreadable = || text!("hub.call.unreadable");
        let Some(body) = request.get("request").and_then(Value::as_object) else {
            return Err(unreadable());
        };
        // Everything said to the call is said in the language it was asked in.
        let _lang = crate::text::bind(body.get("lang").and_then(Value::as_str));
        let name = {
            let store = self.reading()?;
            member_name(&store.conn, member)?.ok_or_else(|| no_member(member))?
        };
        let (Some("call"), Some(tool), Some(arguments), Some(key), Some(level)) = (
            body.get("k").and_then(Value::as_str),
            body.get("tool").and_then(Value::as_str),
            body.get("arguments").and_then(Value::as_object),
            body.get("key").and_then(Value::as_str),
            body.get("level")
                .and_then(Value::as_str)
                .and_then(Level::of),
        ) else {
            return Err(unreadable());
        };
        // Written with what the call writes: held to what a key of this store's may be named.
        let key = access::key_name(key).map_err(|_| unreadable())?;
        let now = self.hub_now();
        if body.get("build").and_then(Value::as_str) != Some(now.build.as_str()) {
            let own = match &now.me {
                Some(me) => {
                    let store = self.reading()?;
                    member_name(&store.conn, me)?.unwrap_or_else(|| me.clone())
                }
                None => String::new(),
            };
            return Ok(json!({ "error": text!("hub.otherBuild", owner = own) }));
        }
        let caller = Caller {
            key: format!("hub:{member}"),
            name: format!("{key}@{name}"),
            permits: Permits::ALL,
            member: Some(Member {
                id: member.to_string(),
                level,
            }),
        };
        Ok(match self.hub_run(&caller, tool, arguments) {
            Ok(Some(output)) => json!({ "output": output_said(output) }),
            Ok(None) => json!({ "failed": "unshared" }),
            Err(refusal) => json!({ "error": refusal }),
        })
    }

    /// A member's call run: the tool it names, its arguments held to the tool's schema as a call
    /// through MCP is, bound to the member (`access::as_caller`). Only a call that names a project:
    /// a project is made and the projects are listed at one's own agent, and a window asked about
    /// with no project named, or the windows listed, would be this agent's, showing whatever
    /// project is open there.
    ///
    /// The project is named by its id, as the side that uses it hands it over (its names are that
    /// side's to read). None for one not shared with the member, or no longer: it is not run and
    /// nothing of this store is named — not the project, nor the others shared with that agent,
    /// which the key behind the call may not have been given. That side says so in its own names.
    fn hub_run(
        &self,
        caller: &Caller,
        tool: &str,
        arguments: &Map<String, Value>,
    ) -> Result<Option<Output>, String> {
        let windows =
            tool == "app_control" && arguments.get("op").and_then(Value::as_str) == Some("windows");
        let project = arguments
            .get("project")
            .and_then(Value::as_str)
            .filter(|_| !matches!(tool, "project_create" | "project_list") && !windows);
        let Some(project) = project else {
            return Err(text!("hub.call.local", tool));
        };
        if let Some(member) = &caller.member {
            let store = self.reading()?;
            if !shared_with(&store.conn, member)?.contains_key(project) {
                return Ok(None);
            }
        }
        crate::mcp::checked(tool, arguments)?;
        let who = Who::mcp(Some(&caller.name));
        access::as_caller(caller, tool, || self.tool(&who, tool, arguments)).map(Some)
    }
}
