//! What MCP may do (`specboard://SpecBoard/Docs/Design:DesignAccess` §6): an item for each tool
//! that writes, named by the tool, and a few parts of
//! a tool that weigh more than the rest of it (`project_update.rules`). Each item is allowed, asked of
//! a person each time, or denied (`State`).
//!
//! The project's items hold for every access key that may write the project: a person sets them on
//! the project's settings (its 「MCP」 tab, `mcp_writes_set`), and MCP only reads them (`project_read`'s
//! `mcpAsk` and `mcpDenied`). The key's items (`KEY_ITEMS`: `app_control` and its pictures,
//! `project_create`) are the key's, set on the app's settings (`access_key_permit`). Both stand apart
//! from whether the key may write the project at all (`access`) and from main's lock (`write`): a call
//! goes through only when each of them lets it.
//!
//! A call's tool and its arguments say which items it touches (`touched`). Before anything of it runs
//! (`judge`), a call from MCP is refused when any of them is denied, naming every one and where a
//! person changes it; an item asked each time is asked of a person through the window when the call
//! says `request: true` (`Core::ask_person`), and refused, saying so, when it does not. A batch's steps
//! are judged one by one as they are staged, tried and run, and what they ask is asked once, for the
//! run (`batch` with `execute: true`). The window writes whatever it writes.
//!
//! Kept in the project's row (`projects.mcp_writes`), and so in its file: the items set apart from
//! their default, `{"<item>": "allow" | "ask" | "deny"}`; none when every one is as it was. The form
//! before: an item a person had switched off — `{"card.update": false}`, by the
//! items of before — is asked each time, so nothing is let through that was not.

use super::Named;
use super::args::Args;
use crate::access::{self, Caller};
use crate::store::{BOARD, sql};
use crate::write::{Side, Tx, Who, db};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Map, Value, json};
use std::collections::HashMap;

/// How MCP stands with an item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Allow,
    /// Asked of a person each time (`request: true`).
    Ask,
    Deny,
}

impl State {
    pub fn word(self) -> &'static str {
        match self {
            State::Allow => "allow",
            State::Ask => "ask",
            State::Deny => "deny",
        }
    }

    pub fn of(word: &str) -> Option<State> {
        match word {
            "allow" => Some(State::Allow),
            "ask" => Some(State::Ask),
            "deny" => Some(State::Deny),
            _ => None,
        }
    }

    /// As the window says it, kept to be said in the language of whoever reads it.
    pub fn keyed(self) -> crate::text::Keyed {
        match self {
            State::Allow => keyed!("mcpWrites.state.allow"),
            State::Ask => keyed!("mcpWrites.state.ask"),
            State::Deny => keyed!("mcpWrites.state.deny"),
        }
    }

    /// As the window says it, in the language of the call.
    pub fn said(self) -> String {
        self.keyed().render()
    }

    /// As a key's column keeps it: 1, 0 and -1 (`access_keys.can_*`; 0 was "not allowed" before
    /// there was asking, and is asked now).
    pub fn of_column(n: i64) -> State {
        match n {
            1.. => State::Allow,
            0 => State::Ask,
            _ => State::Deny,
        }
    }

    pub fn column(self) -> i64 {
        match self {
            State::Allow => 1,
            State::Ask => 0,
            State::Deny => -1,
        }
    }
}

/// One thing MCP may do, or not.
pub(crate) struct Item {
    /// The tool's name, or `<tool>.<part>` for a part of it that weighs more than the rest.
    pub id: &'static str,
    /// The key of what it does, as the window shows it beside the id, and a refusal says it.
    pub name: &'static str,
    /// The key of what it covers, where its name does not say it all; none when empty.
    pub about: &'static str,
    pub default: State,
}

/// The text of a key in the table of items, in the language of the call; none for no key.
fn render_key(key: &str) -> String {
    if key.is_empty() {
        String::new()
    } else {
        crate::text::render(key, &[])
    }
}

impl Item {
    /// What it does, in the language of the call.
    pub(crate) fn name_said(&self) -> String {
        render_key(self.name)
    }

    /// What it covers, in the language of the call.
    pub(crate) fn about_said(&self) -> String {
        render_key(self.about)
    }

    /// As a refusal and the window's question name it: `card_update（カードを変える）`.
    pub(crate) fn called(&self) -> String {
        text!(
            "mcpWrites.item.called",
            id = self.id,
            name = self.name_said()
        )
    }
}

/// Items shown together, and switched together.
pub(crate) struct Group {
    pub id: &'static str,
    /// The key of its name.
    pub name: &'static str,
    /// The key of what it covers; none when empty.
    pub about: &'static str,
    pub items: &'static [Item],
}

impl Group {
    pub(crate) fn name_said(&self) -> String {
        render_key(self.name)
    }

    pub(crate) fn about_said(&self) -> String {
        render_key(self.about)
    }
}

const fn allowed(id: &'static str, name: &'static str, about: &'static str) -> Item {
    Item {
        id,
        name,
        about,
        default: State::Allow,
    }
}

/// The project's items, in the order the window shows them and a refusal names them.
pub(crate) const GROUPS: [Group; 9] = [
    Group {
        id: "project",
        name: "mcpWrites.group.project.name",
        about: "",
        items: &[
            allowed(
                "project_update",
                "mcpWrites.item.projectUpdate.name",
                "mcpWrites.item.projectUpdate.about",
            ),
            Item {
                id: "project_update.rules",
                name: "mcpWrites.item.projectUpdateRules.name",
                about: "mcpWrites.item.projectUpdateRules.about",
                default: State::Ask,
            },
            // The folders of what was written move, and what names the project by its slug from
            // outside is cut.
            Item {
                id: "project_update.slug",
                name: "mcpWrites.item.projectUpdateSlug.name",
                about: "mcpWrites.item.projectUpdateSlug.about",
                default: State::Ask,
            },
            // What is directly under main's root can be changed from MCP under main's lock only through
            // this item: a merge cannot carry
            // it — it is the project's setting, for main, every branch and the board at once.
            Item {
                id: "project_update.main_tree",
                name: "mcpWrites.item.projectUpdateMainTree.name",
                about: "mcpWrites.item.projectUpdateMainTree.about",
                default: State::Ask,
            },
            allowed(
                "inspection_update",
                "mcpWrites.item.inspectionUpdate.name",
                "mcpWrites.item.inspectionUpdate.about",
            ),
        ],
    },
    Group {
        id: "kind",
        name: "mcpWrites.group.kind.name",
        about: "",
        items: &[
            allowed(
                "kind_create",
                "mcpWrites.item.kindCreate.name",
                "mcpWrites.item.kindCreate.about",
            ),
            allowed(
                "kind_update",
                "mcpWrites.item.kindUpdate.name",
                "mcpWrites.item.kindUpdate.about",
            ),
            allowed(
                "kind_delete",
                "mcpWrites.item.kindDelete.name",
                "mcpWrites.item.kindDelete.about",
            ),
        ],
    },
    Group {
        id: "section",
        name: "mcpWrites.group.section.name",
        about: "",
        items: &[
            allowed("section_create", "mcpWrites.item.sectionCreate.name", ""),
            allowed(
                "section_update",
                "mcpWrites.item.sectionUpdate.name",
                "mcpWrites.item.sectionUpdate.about",
            ),
            allowed("section_delete", "mcpWrites.item.sectionDelete.name", ""),
            // The folder a working directory is read from on this machine: what the core reads on
            // behalf of the call, apart from the project.
            Item {
                id: "section_update.folder",
                name: "mcpWrites.item.sectionUpdateFolder.name",
                about: "mcpWrites.item.sectionUpdateFolder.about",
                default: State::Ask,
            },
        ],
    },
    Group {
        id: "card",
        name: "mcpWrites.group.card.name",
        about: "",
        items: &[
            allowed("card_create", "mcpWrites.item.cardCreate.name", ""),
            allowed(
                "card_update",
                "mcpWrites.item.cardUpdate.name",
                "mcpWrites.item.cardUpdate.about",
            ),
            allowed("card_delete", "mcpWrites.item.cardDelete.name", ""),
        ],
    },
    Group {
        id: "tag",
        name: "mcpWrites.group.tag.name",
        about: "",
        items: &[allowed(
            "tag_update",
            "mcpWrites.item.tagUpdate.name",
            "mcpWrites.item.tagUpdate.about",
        )],
    },
    Group {
        id: "export",
        name: "mcpWrites.group.export.name",
        about: "",
        items: &[
            Item {
                id: "export_create",
                name: "mcpWrites.item.exportCreate.name",
                about: "",
                default: State::Ask,
            },
            Item {
                id: "export_update",
                name: "mcpWrites.item.exportUpdate.name",
                about: "mcpWrites.item.exportUpdate.about",
                default: State::Ask,
            },
            Item {
                id: "export_delete",
                name: "mcpWrites.item.exportDelete.name",
                about: "mcpWrites.item.exportDelete.about",
                default: State::Ask,
            },
            allowed(
                "export_execute",
                "mcpWrites.item.exportExecute.name",
                "mcpWrites.item.exportExecute.about",
            ),
            Item {
                id: "export.dir",
                name: "mcpWrites.item.exportDir.name",
                about: "mcpWrites.item.exportDir.about",
                default: State::Ask,
            },
            // A write into the folder each export has by default here: what a branch's work tree
            // is written to instead.
            Item {
                id: "export.default",
                name: "mcpWrites.item.exportDefault.name",
                about: "mcpWrites.item.exportDefault.about",
                default: State::Ask,
            },
        ],
    },
    Group {
        id: "branch",
        name: "mcpWrites.group.branch.name",
        about: "",
        items: &[
            allowed("branch_create", "mcpWrites.item.branchCreate.name", ""),
            allowed("branch_update", "mcpWrites.item.branchUpdate.name", ""),
            allowed("branch_delete", "mcpWrites.item.branchDelete.name", ""),
            allowed(
                "branch_revert",
                "mcpWrites.item.branchRevert.name",
                "mcpWrites.item.branchRevert.about",
            ),
            // Undoing a person's commit, or another key's: one at a time (`undo`), each named — never
            // a range, which could take in what was written between the asking and the writing.
            Item {
                id: "branch_revert.others",
                name: "mcpWrites.item.branchRevertOthers.name",
                about: "mcpWrites.item.branchRevertOthers.about",
                default: State::Ask,
            },
            allowed(
                "branch_merge",
                "mcpWrites.item.branchMerge.name",
                "mcpWrites.item.branchMerge.about",
            ),
        ],
    },
    Group {
        id: "mr",
        name: "mcpWrites.group.mr.name",
        about: "",
        items: &[allowed("mr_delete", "mcpWrites.item.mrDelete.name", "")],
    },
    Group {
        id: "board",
        name: "mcpWrites.group.board.name",
        about: "",
        items: &[allowed(
            "board",
            "mcpWrites.item.board.name",
            "mcpWrites.item.board.about",
        )],
    },
];

/// The key's items (`access_keys.can_control`, `can_capture`, `can_create`): the window shows any
/// project, so what a key may do with it is the key's, apart from the projects; a project it makes it
/// then writes. `app_control`'s pictures are their own item, apart from the rest of it.
pub(crate) const KEY_ITEMS: [Item; 3] = [
    Item {
        id: "app_control",
        name: "mcpWrites.item.appControl.name",
        about: "mcpWrites.item.appControl.about",
        default: State::Ask,
    },
    Item {
        id: "app_control.capture",
        name: "mcpWrites.item.appControlCapture.name",
        about: "mcpWrites.item.appControlCapture.about",
        default: State::Ask,
    },
    allowed(
        "project_create",
        "mcpWrites.item.projectCreate.name",
        "mcpWrites.item.projectCreate.about",
    ),
];

fn items() -> impl Iterator<Item = &'static Item> {
    GROUPS.iter().flat_map(|g| g.items.iter())
}

pub(crate) fn find(id: &str) -> Option<&'static Item> {
    items().find(|i| i.id == id)
}

pub(crate) fn key_item(id: &str) -> &'static Item {
    KEY_ITEMS
        .iter()
        .find(|i| i.id == id)
        .unwrap_or(&KEY_ITEMS[0])
}

/// The items of before (by what is written, not by the tool), each as the items
/// of now it went into.
fn before(id: &str) -> &'static [&'static str] {
    match id {
        "project.name"
        | "project.description"
        | "project.layout"
        | "plan.name"
        | "plan.description"
        | "plan.layout" => &["project_update"],
        // The kinds were written by project_update.
        "project.kinds" | "plan.kinds" => &[
            "project_update",
            "kind_create",
            "kind_update",
            "kind_delete",
        ],
        // The working directories were written by project_update: its project tree now.
        "project.workDirs" | "project.exportDirs" | "plan.workDirs" | "plan.exportDirs" => {
            &["project_update"]
        }
        "project.rules" | "plan.rules" => &["project_update.rules"],
        "section.create" => &["section_create"],
        "section.update" => &["section_update"],
        "section.delete" => &["section_delete"],
        "card.create" => &["card_create"],
        "card.update" | "card.move" => &["card_update"],
        "card.delete" => &["card_delete"],
        "layout" => &["card_update", "section_update"],
        "tag.cards" | "tag.groups" => &["tag_update"],
        "branch.create" => &["branch_create"],
        "branch.update" => &["branch_update"],
        "branch.delete" => &["branch_delete"],
        "branch.revert" => &["branch_revert"],
        "branch.merge" | "mr.create" | "pr.create" => &["branch_merge"],
        "mr.delete" | "pr.delete" => &["mr_delete"],
        "export.write" => &["export_execute"],
        _ => &[],
    }
}

/// An item of now by the id it had before: the folder a working directory is bound to, set by
/// `section_update` since the working directories became sections.
pub(crate) fn renamed(id: &str) -> Option<&'static str> {
    match id {
        "directory.path" => Some("section_update.folder"),
        _ => None,
    }
}

// MARK: - What the project says

/// What the project keeps over the defaults. What is not an item, or not a state, is passed over (a
/// file may say anything); an item of before is read as the items it went into, one switched off asked
/// each time — and asked, when two of before went into one and either was.
fn kept(conn: &Connection, project: &str) -> Result<HashMap<&'static str, State>, String> {
    let text: Option<String> = conn
        .query_row(
            "SELECT mcp_writes FROM projects WHERE id = ?",
            [project],
            |r| r.get(0),
        )
        .optional()
        .map_err(sql)?
        .flatten();
    let Some(Value::Object(said)) = text.and_then(|t| serde_json::from_str::<Value>(&t).ok())
    else {
        return Ok(HashMap::new());
    };
    let mut out: HashMap<&'static str, State> = HashMap::new();
    for (id, value) in &said {
        if let Value::Bool(on) = value {
            for now in before(id) {
                let state = if *on { State::Allow } else { State::Ask };
                let held = out.entry(*now).or_insert(state);
                if state == State::Ask {
                    *held = State::Ask;
                }
            }
        }
    }
    // The form of now over what came before it; an item of now named as before (`renamed`) as the
    // one it is, unless that is said too.
    for (id, value) in &said {
        let Some(state) = value.as_str().and_then(State::of) else {
            continue;
        };
        if let Some(item) = find(id) {
            out.insert(item.id, state);
        } else if let Some(now) = renamed(id)
            && !said.contains_key(now)
        {
            out.insert(now, state);
        }
    }
    Ok(out)
}

/// How MCP stands with each of the project's items.
pub(crate) fn states(
    conn: &Connection,
    project: &str,
) -> Result<HashMap<&'static str, State>, String> {
    let kept = kept(conn, project)?;
    Ok(items()
        .map(|i| (i.id, kept.get(i.id).copied().unwrap_or(i.default)))
        .collect())
}

/// The items of the project in `state`, in their order: what `project_read` says (`mcpAsk`,
/// `mcpDenied`).
pub(crate) fn in_state(
    conn: &Connection,
    project: &str,
    state: State,
) -> Result<Vec<&'static str>, String> {
    let states = states(conn, project)?;
    Ok(items()
        .filter(|i| states[i.id] == state)
        .map(|i| i.id)
        .collect())
}

/// The window's view of it (`mcp_writes`): every group, and every item of it with how MCP stands
/// with it and with it by default.
pub(crate) fn view(conn: &Connection, project: &str) -> Result<Value, String> {
    let states = states(conn, project)?;
    Ok(json!({ "groups": GROUPS.iter().map(|g| json!({
        "id": g.id,
        "name": g.name_said(),
        "about": g.about_said(),
        "items": g.items.iter().map(|i| json!({
            "id": i.id,
            "name": i.name_said(),
            "about": i.about_said(),
            "state": states[i.id].word(),
            "byDefault": i.default.word(),
        })).collect::<Vec<_>>(),
    })).collect::<Vec<_>>() }))
}

/// The state a request names (`state`).
fn state_named(request: &Value, op: &str) -> Result<State, String> {
    request
        .get("state")
        .and_then(Value::as_str)
        .and_then(State::of)
        .ok_or_else(|| text!("mcpWrites.stateNeeded", op))
}

/// Items set to a state, in the project's row, with a line in main's history; under main's lock too,
/// and while MCP holds a tree: a person may stop it.
fn put(tx: &mut Tx, project: &str, chosen: &[&'static Item], state: State) -> Result<(), String> {
    let archived: Option<String> = tx
        .conn()
        .query_row(
            "SELECT archived_at FROM projects WHERE id = ?",
            [project],
            |r| r.get(0),
        )
        .map_err(db)?;
    if archived.is_some() {
        return Err(crate::write::archived());
    }
    let mut kept = kept(tx.conn(), project)?;
    for item in chosen {
        if state == item.default {
            kept.remove(item.id);
        } else {
            kept.insert(item.id, state);
        }
    }
    let stored = (!kept.is_empty()).then(|| {
        let ordered: Map<String, Value> = items()
            .filter_map(|i| Some((i.id.to_string(), kept.get(i.id)?.word().into())))
            .collect();
        crate::ojson::compact(&Value::Object(ordered))
    });
    tx.conn()
        .execute(
            "UPDATE projects SET mcp_writes = ? WHERE id = ?",
            params![stored, project],
        )
        .map_err(db)?;
    let named = crate::text::List::of(chosen.iter().map(|i| i.id));
    tx.line(
        project,
        "",
        "project.update",
        "project",
        Some(project),
        keyed!("mcpWrites.line.set", named, state = state.keyed()),
    )
}

/// An item, or every item of a group, set to a state (`mcp_writes_set`, the window's alone). Returns
/// the view as it is now.
pub(crate) fn set(tx: &mut Tx, project: &str, request: &Value) -> Result<Value, String> {
    let op = "mcp_writes_set";
    let state = state_named(request, op)?;
    let item = request.get("item").and_then(Value::as_str);
    let group = request.get("group").and_then(Value::as_str);
    let chosen: Vec<&'static Item> = match (item, group) {
        (Some(id), None) => vec![find(id).ok_or_else(|| {
            text!(
                "mcpWrites.itemUnknown",
                id,
                known = crate::text::List::of(items().map(|i| i.id))
            )
        })?],
        (None, Some(id)) => GROUPS
            .iter()
            .find(|g| g.id == id)
            .ok_or_else(|| {
                text!(
                    "mcpWrites.groupUnknown",
                    id,
                    known = crate::text::List::of(GROUPS.iter().map(|g| g.id))
                )
            })?
            .items
            .iter()
            .collect(),
        _ => {
            return Err(text!("mcpWrites.itemOrGroup", op));
        }
    };
    put(tx, project, &chosen, state)?;
    view(tx.conn(), project)
}

// MARK: - What a call touches

/// The tools that write in a tree: main, a branch, or the board.
const TREE_TOOLS: [&str; 7] = [
    "section_create",
    "section_update",
    "section_delete",
    "card_create",
    "card_update",
    "card_delete",
    "tag_update",
];

/// The project's items a call of `tool` touches, by its arguments, in their order; none for a tool
/// that touches none of them. A write in the board's tree is the board's alone.
pub(crate) fn touched(tool: &str, args: &Args, board: bool) -> Vec<&'static str> {
    if board && TREE_TOOLS.contains(&tool) {
        return vec!["board"];
    }
    // The board's history gone back in: a write in the board's tree, which `target` names.
    if tool == "branch_revert"
        && args
            .optional_string("target")
            .is_some_and(|t| t.trim().eq_ignore_ascii_case(crate::store::BOARD_NAME))
    {
        return vec!["board"];
    }
    // A card's action: what it writes — the card where it is (the board's, or a card of main's or a
    // branch's), and the branch it makes or lets go of.
    if tool == "card_action" {
        let mut out = vec![if board { "board" } else { "card_update" }];
        match args.optional_string("op").map(str::trim) {
            Some("start") => out.push("branch_create"),
            Some("finish" | "cancel") => out.push("branch_delete"),
            _ => {}
        }
        return out;
    }
    let mut out: Vec<&'static str> = Vec::new();
    if let Some(item) = find(tool) {
        out.push(item.id);
    }
    // The writing rules and the slug weigh more than the rest of the project's settings; the folder a working
    // directory is on this machine, more than the rest of it; a place to write to — a path given
    // to `export_execute`, or an export's default one — more than the rest of an export; and so does a
    // write into the folder each export has by default.
    if tool == "project_update" && args.value().is_ok_and(|v| v.contains_key("rules")) {
        out.push("project_update.rules");
    }
    if tool == "project_update" && args.value().is_ok_and(|v| v.contains_key("slug")) {
        out.push("project_update.slug");
    }
    if tool == "section_update"
        && args
            .pairs()
            .is_ok_and(|pairs| pairs.iter().any(|(_, v)| v.contains_key("folder")))
    {
        out.push("section_update.folder");
    }
    if (["export_create", "export_update"].contains(&tool)
        && args.value().is_ok_and(|v| v.contains_key("path")))
        || (tool == "export_execute"
            && args
                .optional_string("targetdir")
                .is_some_and(|t| t.trim() != super::files::DEFAULT))
    {
        out.push("export.dir");
    }
    // A branch is not written into the default folders (`export_execute` refuses it): nobody is asked
    // for what is refused anyway.
    if tool == "export_execute"
        && args
            .optional_string("targetdir")
            .is_some_and(|t| t.trim() == super::files::DEFAULT)
        && args
            .optional_string("branch")
            .is_none_or(|b| b.trim().is_empty() || b.trim().eq_ignore_ascii_case("main"))
    {
        out.push("export.default");
    }
    out
}

/// Whether a call of `project_update` changes main's sections directly under its root while main is
/// locked: its `project_tree`'s `main_sections` are not as they are, or it renames one. Not when it
/// only checks (`dryRun`), when main is not locked, or when only the board's are changed.
fn changes_locked_main_tree(conn: &Connection, project: &str, args: &Args) -> bool {
    let Ok(value) = args.value() else {
        return false;
    };
    if value.get("dryRun").and_then(Value::as_bool) == Some(true) {
        return false;
    }
    let locked = conn
        .query_row(
            "SELECT locked_at IS NOT NULL FROM projects WHERE id = ?",
            [project],
            |r| r.get::<_, bool>(0),
        )
        .unwrap_or(false);
    locked && super::own_kinds::changes_main_tree(conn, project, &value)
}

/// Whether the commit `seq` of the tree is another writer's than the caller's — the window's, or
/// another key's — that wrote a card or a section: what MCP undoes only through `branch_revert.others`.
/// The window's calls are not held to it.
pub(crate) fn undoes_others(
    conn: &Connection,
    who: &Who,
    project: &str,
    tree: &str,
    seq: i64,
) -> Result<bool, String> {
    if who.side != Side::Mcp {
        return Ok(false);
    }
    let Some(commit) = crate::write::history_commit(conn, project, tree, seq)? else {
        return Ok(false);
    };
    let by = crate::write::Author {
        kind: commit.author_kind,
        name: commit.author,
    };
    if by == crate::write::Author::of(who) {
        return Ok(false);
    }
    let wrote = conn
        .query_row(
            "SELECT 1 FROM commit_rows WHERE commit_id = ? LIMIT 1",
            [&commit.id],
            |_| Ok(()),
        )
        .optional()
        .map_err(sql)?
        .is_some();
    Ok(wrote)
}

/// `undoes_others` for a call of `branch_revert` by its arguments: its `undo` in the tree its `target`
/// names. A call that names none, or what is not there, touches nothing more (the tool refuses it).
fn undoes_others_by_args(conn: &Connection, who: &Who, project: &str, args: &Args) -> bool {
    let Some(seq) = args
        .value()
        .ok()
        .and_then(|v| v.get("undo").and_then(Value::as_f64))
        .filter(|f| f.fract() == 0.0 && *f >= 0.0)
    else {
        return false;
    };
    let Some(named) = args
        .optional_string("target")
        .and_then(|t| super::named_in(conn, project, Some(t)).ok())
    else {
        return false;
    };
    undoes_others(conn, who, project, &named.tree, seq as i64).unwrap_or(false)
}

/// The key's items a call of `tool` touches.
fn key_touched(tool: &str, args: &Args) -> Option<&'static Item> {
    match tool {
        "app_control" if args.optional_string("op") == Some("capture") => {
            Some(key_item("app_control.capture"))
        }
        "app_control" => Some(key_item("app_control")),
        "project_create" => Some(key_item("project_create")),
        _ => None,
    }
}

/// Whose an item asked of a person is: what "always" changes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Whose {
    /// The project's setting (by its id, name and slug — the slug shown beside the name, so two of one
    /// name made before names were held apart are told apart), for every key.
    Project {
        id: String,
        name: String,
        slug: String,
    },
    /// The key's (by its id and name).
    Key { id: String, name: String },
}

/// An item a call touches that a person is asked about each time.
#[derive(Clone)]
pub(crate) struct Asked {
    pub item: &'static Item,
    pub whose: Whose,
}

/// What a call is told when an item it touches is denied: every such item at once, and where a person
/// changes it.
fn denied(project: &[&Item], key: &[&Item]) -> String {
    let named =
        |items: &[&Item]| crate::text::list(&items.iter().map(|i| i.called()).collect::<Vec<_>>());
    let project_said =
        (!project.is_empty()).then(|| text!("mcpWrites.denied.project", items = named(project)));
    let key_said = (!key.is_empty()).then(|| text!("mcpWrites.denied.key", items = named(key)));
    match (project_said, key_said) {
        (Some(project), Some(key)) => text!("mcpWrites.denied.both", project, key),
        (Some(said), None) | (None, Some(said)) => said,
        (None, None) => String::new(),
    }
}

/// What a call is told when it touches what is asked each time and did not say `request: true`.
pub(crate) fn not_requested(asks: &[Asked], tool: &str) -> String {
    text!(
        "mcpWrites.notRequested",
        items = crate::text::List::of(asks.iter().map(|a| a.item.called())),
        tool
    )
}

/// A call from MCP judged before it runs: refused when an item it touches is denied — the project's
/// and the key's, every one at once; else the items a person is to be asked about. `batch` is the tree
/// a batch's step writes in; none for a tool called on its own, whose `project` and `branch` say
/// where. A project the call's key may not see or write is left to the tool to refuse, in its own
/// words.
pub(crate) fn judge(
    conn: &Connection,
    who: &Who,
    tool: &str,
    args: &Args,
    batch: Option<&Named>,
) -> Result<Vec<Asked>, String> {
    if who.side != Side::Mcp {
        return Ok(Vec::new());
    }
    let mut asks = Vec::new();
    let mut key_denied = Vec::new();
    if let Some(item) = key_touched(tool, args)
        && let Some(caller) = access::caller()
    {
        match caller_state(&caller, item) {
            State::Allow => {}
            State::Ask => asks.push(Asked {
                item,
                whose: Whose::Key {
                    id: caller.key.clone(),
                    name: caller.name.clone(),
                },
            }),
            State::Deny => key_denied.push(item),
        }
    }
    let board = match batch {
        Some(named) => named.tree == BOARD,
        None => args
            .optional_string("branch")
            .is_some_and(|b| b.trim().eq_ignore_ascii_case(crate::store::BOARD_NAME)),
    };
    let mut touched = touched(tool, args, board);
    let mut project_denied = Vec::new();
    if !touched.is_empty() {
        let project = match batch {
            Some(named) => Some(named.project.clone()),
            None => args
                .optional_string("project")
                .and_then(|name| super::project_summary_in(conn, name).ok())
                .map(|found| found.id),
        };
        if let Some(project) = project {
            // main's sections directly under its root, under main's lock: a project's setting no merge
            // carries, so it is an item (and the lock lets it through once the item has).
            if tool == "project_update" && changes_locked_main_tree(conn, &project, args) {
                touched.push("project_update.main_tree");
            }
            // Another writer's commit undone: what it is is read from the history.
            if tool == "branch_revert" && undoes_others_by_args(conn, who, &project, args) {
                touched.push("branch_revert.others");
            }
            let states = states(conn, &project)?;
            let (name, slug): (String, String) = conn
                .query_row(
                    "SELECT name, slug FROM projects WHERE id = ?",
                    [&project],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .map_err(sql)?;
            for id in touched {
                let item = find(id).expect("an item");
                match states[id] {
                    State::Allow => {}
                    State::Ask => asks.push(Asked {
                        item,
                        whose: Whose::Project {
                            id: project.clone(),
                            name: name.clone(),
                            slug: slug.clone(),
                        },
                    }),
                    State::Deny => project_denied.push(item),
                }
            }
        }
    }
    if project_denied.is_empty() && key_denied.is_empty() {
        Ok(asks)
    } else {
        Err(denied(&project_denied, &key_denied))
    }
}

/// How the caller's key stands with one of the key's items.
fn caller_state(caller: &Caller, item: &Item) -> State {
    match item.id {
        "app_control" => caller.permits.control,
        "app_control.capture" => caller.permits.capture,
        _ => caller.permits.create,
    }
}

/// What a person said to "always": the asked items set to `state` where they are kept — the project's
/// in its row (a line in main's history, as the window of `client` writes it), the key's in its row.
pub(crate) fn always(tx: &mut Tx, asks: &[Asked], state: State) -> Result<(), String> {
    let mut by_project: Vec<(String, Vec<&'static Item>)> = Vec::new();
    for asked in asks {
        match &asked.whose {
            Whose::Project { id, .. } => match by_project.iter_mut().find(|(p, _)| p == id) {
                Some((_, items)) => items.push(asked.item),
                None => by_project.push((id.clone(), vec![asked.item])),
            },
            Whose::Key { id, .. } => access::set_key_item(tx.conn(), id, asked.item.id, state)?,
        }
    }
    for (project, items) in by_project {
        put(tx, &project, &items, state)?;
    }
    Ok(())
}

/// The items asked about, by whose they are, as the window shows them (`App::ask`).
pub(crate) fn asked_view(asks: &[Asked]) -> Value {
    Value::Array(
        asks.iter()
            .map(|a| match &a.whose {
                Whose::Project { id, name, slug } => json!({
                    "id": a.item.id,
                    "name": a.item.name_said(),
                    "whose": "project",
                    "whoseId": id,
                    "whoseName": name,
                    "whoseSlug": slug,
                }),
                Whose::Key { id, name } => json!({
                    "id": a.item.id,
                    "name": a.item.name_said(),
                    "whose": "key",
                    "whoseId": id,
                    "whoseName": name,
                }),
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn of(tool: &str, arguments: Value, board: bool) -> Vec<&'static str> {
        let arguments = arguments.as_object().cloned().unwrap_or_default();
        touched(tool, &Args::new(&arguments), board)
    }

    #[test]
    fn every_item_has_one_id_and_only_the_rules_are_asked_by_default() {
        let ids: Vec<&str> = items().chain(KEY_ITEMS.iter()).map(|i| i.id).collect();
        let mut unique = ids.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), ids.len());
        // A tool that writes, an item.
        assert_eq!(items().count(), 30);
        let asked: Vec<&str> = items()
            .filter(|i| i.default != State::Allow)
            .map(|i| i.id)
            .collect();
        assert_eq!(
            asked,
            [
                "project_update.rules",
                "project_update.slug",
                "project_update.main_tree",
                "section_update.folder",
                "export_create",
                "export_update",
                "export_delete",
                "export.dir",
                "export.default",
                "branch_revert.others"
            ]
        );
        // The window, asked each time; making a project, as every key could.
        let keys: Vec<(&str, State)> = KEY_ITEMS.iter().map(|i| (i.id, i.default)).collect();
        assert_eq!(
            keys,
            [
                ("app_control", State::Ask),
                ("app_control.capture", State::Ask),
                ("project_create", State::Allow)
            ]
        );
        for state in [State::Allow, State::Ask, State::Deny] {
            assert_eq!(State::of(state.word()), Some(state));
            assert_eq!(State::of_column(state.column()), state);
        }
    }

    #[test]
    fn a_call_touches_its_tools_item() {
        assert_eq!(
            of(
                "card_update",
                json!({"project": "p", "target": "Core:User", "value": {"section": "S", "tags": [], "topLeft": {}}}),
                false
            ),
            ["card_update"]
        );
        assert_eq!(
            of(
                "project_update",
                json!({"project": "p", "value": {"rules": {}, "name": "x"}}),
                false
            ),
            ["project_update", "project_update.rules"]
        );
        assert_eq!(
            of(
                "project_update",
                json!({"project": "p", "value": {"slug": "x", "rules": {}}}),
                false
            ),
            [
                "project_update",
                "project_update.rules",
                "project_update.slug"
            ]
        );
        assert_eq!(
            of(
                "project_update",
                json!({"project": "p", "value": {"description": ""}}),
                false
            ),
            ["project_update"]
        );
        assert_eq!(
            of(
                "section_update",
                json!({"project": "p", "target": "repo", "value": {"folder": "~/repo"}}),
                false
            ),
            ["section_update", "section_update.folder"]
        );
        // A place to write to: an export's default one, or a path given to the writing; `[default]`
        // is the exports' own, and is the writing's item alone.
        for tool in ["export_create", "export_update"] {
            assert_eq!(
                of(
                    tool,
                    json!({"project": "p", "target": "docs", "value": {"path": "~/repo"}}),
                    false
                ),
                [tool, "export.dir"]
            );
        }
        assert_eq!(
            of(
                "export_execute",
                json!({"project": "p", "targetdir": "[default]"}),
                false
            ),
            ["export_execute", "export.default"]
        );
        assert_eq!(
            of(
                "export_execute",
                json!({"project": "p", "targetdir": "[default]", "branch": "main"}),
                false
            ),
            ["export_execute", "export.default"]
        );
        // A branch into the default folders is refused by the tool: nobody is asked for it.
        assert_eq!(
            of(
                "export_execute",
                json!({"project": "p", "targetdir": "[default]", "branch": "goal"}),
                false
            ),
            ["export_execute"]
        );
        assert_eq!(
            of(
                "export_execute",
                json!({"project": "p", "targetdir": "/work/tree"}),
                false
            ),
            ["export_execute", "export.dir"]
        );
        assert_eq!(
            of(
                "export_execute",
                json!({"project": "p", "targetdir": "/work/tree", "branch": "goal"}),
                false
            ),
            ["export_execute", "export.dir"]
        );
        for tool in [
            "section_create",
            "section_update",
            "section_delete",
            "card_create",
            "card_delete",
            "tag_update",
            "export_create",
            "export_update",
            "export_delete",
            "export_execute",
            "branch_create",
            "branch_update",
            "branch_delete",
            "branch_revert",
            "branch_merge",
            "mr_delete",
            "inspection_update",
        ] {
            assert_eq!(
                of(tool, json!({"project": "p", "target": "x"}), false),
                [tool]
            );
        }
        // What reads, the key's own, and the batch's (its steps are judged one by one), none.
        for tool in [
            "project_read",
            "card_read",
            "card_list",
            "guide",
            "project_list",
            "project_create",
            "batch",
            "app_control",
            "section_action",
            "inspection_list",
            "inspection_read",
        ] {
            assert!(
                of(tool, json!({"project": "p"}), false).is_empty(),
                "{tool}"
            );
        }
    }

    #[test]
    fn the_boards_tree_is_the_boards_alone() {
        for tool in TREE_TOOLS {
            assert_eq!(
                of(
                    tool,
                    json!({"project": "p", "target": "Goals:G", "value": {"tags": []}}),
                    true
                ),
                ["board"],
                "{tool}"
            );
        }
        // The project's own settings and the branches are not in the board's tree.
        assert_eq!(
            of(
                "project_update",
                json!({"project": "p", "value": {"description": ""}}),
                true
            ),
            ["project_update"]
        );
        assert_eq!(
            of(
                "branch_create",
                json!({"project": "p", "target": "b"}),
                true
            ),
            ["branch_create"]
        );
    }

    fn kept_of(said: Value) -> HashMap<&'static str, State> {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE projects (id TEXT, mcp_writes TEXT)")
            .unwrap();
        conn.execute("INSERT INTO projects VALUES ('p', ?)", [said.to_string()])
            .unwrap();
        states(&conn, "p").unwrap()
    }

    #[test]
    fn the_items_of_before_are_read_as_those_they_went_into() {
        let states = kept_of(json!({
            "card.move": false,
            "layout": true,
            "project.rules": true,
            "mr.create": false,
            "branch.merge": true,
            "project.workDirs": false,
        }));
        // Switched off: asked each time; two that went into one, asked when either was.
        assert_eq!(states["card_update"], State::Ask);
        assert_eq!(states["section_update"], State::Allow);
        assert_eq!(states["branch_merge"], State::Ask);
        assert_eq!(states["project_update"], State::Ask);
        // Allowed, though asked by default.
        assert_eq!(states["project_update.rules"], State::Allow);
        assert_eq!(states["card_create"], State::Allow);
        // The form of now over the one before; what is neither, passed over.
        let states = kept_of(
            json!({"card.update": false, "card_update": "deny", "card_create": "maybe", "what": true}),
        );
        assert_eq!(states["card_update"], State::Deny);
        assert_eq!(states["card_create"], State::Allow);
        // The folder's item by the id it had: the one it is now, unless that is said too.
        let states = kept_of(json!({"directory.path": "allow"}));
        assert_eq!(states["section_update.folder"], State::Allow);
        let states = kept_of(json!({"directory.path": "allow", "section_update.folder": "deny"}));
        assert_eq!(states["section_update.folder"], State::Deny);
    }

    #[test]
    fn a_refusal_names_every_item_and_where_a_person_changes_it() {
        let rules = find("project_update.rules").unwrap();
        let cards = find("card_update").unwrap();
        assert_eq!(
            denied(&[rules, cards], &[]),
            "このプロジェクトでは、project_update.rules（書き方の決まり）・card_update（カードを変える） は MCP から使えません（拒否）。変えられるのは人だけで、アプリのプロジェクト設定の「MCP」のタブで許可か都度承認にします"
        );
        assert_eq!(
            denied(&[], &[key_item("app_control.capture")]),
            "このアクセスキーでは、app_control.capture（画面を撮る） は使えません（拒否）。変えられるのは人だけで、プロジェクト一覧の場所の設定の「アクセスキー」でこのキーの項目を許可か都度承認にします"
        );
        let asked = [Asked {
            item: rules,
            whose: Whose::Project {
                id: "p".into(),
                name: "店".into(),
                slug: "mise".into(),
            },
        }];
        assert_eq!(
            not_requested(&asked, "project_update"),
            "project_update.rules（書き方の決まり） は都度承認です。project_update に request: true を付けて呼び直すと、画面に許可を求めて答えを待ちます（2 分まで）"
        );
    }
}
