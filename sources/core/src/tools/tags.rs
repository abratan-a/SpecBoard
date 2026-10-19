//! The tool that puts tags on cards and takes them off, and makes, changes and lets go
//! of the project's tag groups (`tag_update`; the Swift app's `MCPSurface.tags`,
//! `MCPTools`' `set_tags` / `edit_tag_groups` and `Repository.applyTags`,
//! `editTagGroup`, `setTagGroups`). A card's tags are its document's; the groups are the
//! project's, the same in every tree.

use super::Seen;
use super::args::Args;
use super::cards::{Scope, text};
use super::sections::each_pair;
use crate::write::{Tx, db};
use rusqlite::params;
use serde_json::{Map, Value};
use std::collections::HashSet;

/// How long a tag may be (`tagMaxLength`).
const TAG_MAX_LENGTH: usize = 40;
/// How many tags a card may carry (`tagMaxCount`).
const TAG_MAX_COUNT: usize = 32;
/// How many groups a project may have (`tagGroupMaxCount`).
const TAG_GROUP_MAX_COUNT: usize = 12;

/// Tags as they are kept: trimmed, the empty left out, each once, held to a length and
/// a count (`Validate.tags`).
fn valid_tags(raw: &[String], field: &str) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::new();
    for tag in raw {
        let value = tag.trim();
        if value.is_empty() {
            continue;
        }
        if value.chars().count() > TAG_MAX_LENGTH {
            return Err(text!(
                "tags.tooLong",
                field = field,
                max = TAG_MAX_LENGTH,
                value = value
            ));
        }
        // Tag lists travel through the diagram spec comma-separated.
        if value.contains(',') {
            return Err(text!("tags.comma", value = value));
        }
        if !out.iter().any(|t| t == value) {
            out.push(value.to_string());
        }
    }
    if out.len() > TAG_MAX_COUNT {
        return Err(text!(
            "tags.tooMany",
            field = field,
            max = TAG_MAX_COUNT,
            count = out.len()
        ));
    }
    Ok(out)
}

/// A list of strings the value holds under `key`; none when it holds none.
fn strings(given: &Map<String, Value>, key: &str) -> Vec<String> {
    given
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// The project's tags with how often each is used, as the tree has them now (`projectTags`).
fn project_tags(tx: &Tx, scope: &Scope) -> Result<Value, String> {
    let seen = Seen::in_tx(tx, scope)?;
    Ok(Value::Array(
        super::check::project_tags(&seen.detail.schemas)
            .into_iter()
            .map(|(tag, count)| {
                let mut row = Map::new();
                row.insert("tag".into(), tag.into());
                row.insert("count".into(), count.into());
                Value::Object(row)
            })
            .collect(),
    ))
}

// MARK: - tag_update

pub(crate) fn tag_update(tx: &mut Tx, scope: &Scope, args: &Args) -> Result<Value, String> {
    let pairs = args.pairs()?;
    each_pair(tx, scope, &pairs, |tx, scope, name, given| {
        if let Some(group) = name.strip_prefix("group:") {
            // A tag group is the project's, not a branch's.
            if !scope.tree.is_empty() {
                return Err(text!(
                    "tags.group.inBranch",
                    name = name,
                    branch = scope.tree_name
                ));
            }
            super::check(given, "tag_update", "group:")?;
            return edit_tag_group(tx, scope, group.trim_matches([' ', '\t']), given);
        }
        super::check(given, "tag_update", "tag")?;
        let add = strings(given, "add");
        let remove = strings(given, "remove");
        if add.is_empty() && remove.is_empty() {
            return Err(text!("tags.cardsNeeded"));
        }
        let mut out = Map::new();
        out.insert("tag".into(), name.into());
        if !add.is_empty() {
            let changed = apply_tags(tx, scope, &add, &[name.to_string()], &[])?;
            out.insert("added".into(), changed.into());
        }
        if !remove.is_empty() {
            let changed = apply_tags(tx, scope, &remove, &[], &[name.to_string()])?;
            out.insert("removed".into(), changed.into());
        }
        out.insert("projectTags".into(), project_tags(tx, scope)?);
        Ok(Value::Object(out))
    })
}

/// Tags put on and taken off the cards named, the others each carries kept, in one
/// line of the history (`set_tags`, `Repository.applyTags`). Returns the cards whose
/// tags changed, in full.
fn apply_tags(
    tx: &mut Tx,
    scope: &Scope,
    cards: &[String],
    add: &[String],
    remove: &[String],
) -> Result<Vec<String>, String> {
    let added = valid_tags(add, &text!("tags.field.tags"))?;
    let seen = Seen::in_tx(tx, scope)?;
    let project = seen.project();
    let names: Vec<String> = seen.detail.schemas.iter().map(|c| c.name.clone()).collect();
    let mut resolved: Vec<(String, String)> = Vec::new();
    let mut missing: Vec<(String, String)> = Vec::new();
    for name in cards {
        match project.directory.find(name, None) {
            Some(i) => {
                let card = project.nth(i);
                resolved.push((project.full(card), card.id.clone()));
            }
            None => missing.push((
                name.clone(),
                project.directory.find_refusal(name, None, &names),
            )),
        }
    }
    if let [(_, why)] = missing.as_slice() {
        return Err(why.clone());
    }
    if !missing.is_empty() {
        // Several: then the caller's own list is what is wrong, not one word in it.
        let listed: Vec<String> = missing.iter().map(|m| m.0.clone()).collect();
        return Err(text!(
            "tags.cardsMissing",
            listed = super::capped(&listed, 12),
            count = names.len()
        ));
    }
    drop(seen);
    // Read once, as they were: a card named twice is written from the same tags.
    let docs = scope.docs(tx)?;
    let mut changed = Vec::new();
    for (full, id) in &resolved {
        let Some(doc) = docs.get(id) else {
            continue;
        };
        let current: Vec<String> = doc
            .get("fields")
            .and_then(|f| f.get("tags"))
            .and_then(Value::as_array)
            .map(|t| {
                t.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let mut merged: Vec<String> = current
            .iter()
            .filter(|t| !remove.contains(t))
            .cloned()
            .collect();
        for tag in &added {
            if !merged.contains(tag) {
                merged.push(tag.clone());
            }
        }
        // Checked against the result, not against what was added: small additions do
        // not walk a card past the ceiling one tag at a time.
        let next = valid_tags(&merged, &text!("tags.field.tagsOf", full = full))?;
        if next == current {
            continue;
        }
        let mut doc = doc.clone();
        let fields = doc
            .entry("fields")
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .ok_or_else(|| text!("tools.doc.fieldsNotObject"))?;
        if next.is_empty() {
            fields.shift_remove("tags");
        } else {
            fields.insert("tags".into(), next.clone().into());
        }
        if fields.is_empty() {
            doc.shift_remove("fields");
        }
        tx.put_card(&scope.project, &scope.tree, &doc)?;
        changed.push(full.clone());
    }
    if !changed.is_empty() {
        let mut parts = Vec::new();
        if !added.is_empty() {
            parts.push(format!("+{}", added.join(" ")));
        }
        if !remove.is_empty() {
            let mut gone: Vec<&String> =
                remove.iter().collect::<HashSet<_>>().into_iter().collect();
            gone.sort();
            parts.push(format!(
                "-{}",
                gone.iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            ));
        }
        tx.line(
            &scope.project,
            &scope.tree,
            "schema.tags",
            "project",
            None,
            keyed!(
                "tags.line.changed",
                count = changed.len(),
                tags = parts.join(" ")
            ),
        )?;
    }
    Ok(changed)
}

// MARK: - The tag groups

/// A group as the project keeps it (`TagGroup`).
#[derive(Clone)]
struct Group {
    name: String,
    tags: Vec<String>,
    exclusive: bool,
}

/// The project's groups; none when they cannot be read (`tagGroups`).
fn groups(tx: &Tx, project: &str) -> Result<Vec<Group>, String> {
    let text: Option<String> = tx
        .conn()
        .query_row(
            "SELECT tag_groups FROM projects WHERE id = ?",
            [project],
            |r| r.get(0),
        )
        .map_err(db)?;
    let Some(Value::Array(items)) = text.and_then(|t| serde_json::from_str::<Value>(&t).ok())
    else {
        return Ok(Vec::new());
    };
    let read: Option<Vec<Group>> = items
        .iter()
        .map(|item| {
            Some(Group {
                name: item.get("name")?.as_str()?.to_string(),
                tags: item
                    .get("tags")?
                    .as_array()?
                    .iter()
                    .map(|t| t.as_str().map(str::to_string))
                    .collect::<Option<_>>()?,
                exclusive: match item.get("exclusive") {
                    None | Some(Value::Null) => false,
                    Some(v) => v.as_bool()?,
                },
            })
        })
        .collect();
    Ok(read.unwrap_or_default())
}

/// One group: tags added and removed, its rule set, or the group itself gone; a name
/// that is not there yet makes a new group (`edit_tag_groups`,
/// `Repository.editTagGroup`).
fn edit_tag_group(
    tx: &mut Tx,
    scope: &Scope,
    name: &str,
    given: &Map<String, Value>,
) -> Result<Value, String> {
    let add = strings(given, "add");
    let remove = strings(given, "remove");
    let exclusive = given.get("exclusive").and_then(Value::as_bool);
    let delete = given
        .get("delete")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if add.is_empty() && remove.is_empty() && exclusive.is_none() && !delete {
        return Err(text!("tags.group.keyNeeded"));
    }
    if delete && (!add.is_empty() || !remove.is_empty() || exclusive.is_some()) {
        return Err(text!("tags.group.deleteAlone"));
    }
    let mut all = groups(tx, &scope.project)?;
    if delete {
        if !all.iter().any(|g| g.name == name) {
            return Err(text!("tags.group.gone", name = name));
        }
        all.retain(|g| g.name != name);
    } else if let Some(group) = all.iter_mut().find(|g| g.name == name) {
        group.tags.retain(|t| !remove.contains(t));
        for tag in &add {
            if !group.tags.contains(tag) {
                group.tags.push(tag.clone());
            }
        }
        if let Some(exclusive) = exclusive {
            group.exclusive = exclusive;
        }
    } else {
        all.push(Group {
            name: name.to_string(),
            tags: add
                .iter()
                .filter(|t| !remove.contains(t))
                .cloned()
                .collect(),
            exclusive: exclusive.unwrap_or(false),
        });
    }
    set_tag_groups(tx, scope, &all)?;
    let mut out = Map::new();
    out.insert(
        "groups".into(),
        Value::Array(
            all.iter()
                .map(|g| {
                    let mut row = Map::new();
                    row.insert("name".into(), g.name.clone().into());
                    row.insert("tags".into(), g.tags.clone().into());
                    if g.exclusive {
                        row.insert("exclusive".into(), true.into());
                    }
                    Value::Object(row)
                })
                .collect(),
        ),
    );
    // Straight back: an exclusive group is usually declared over tags already on
    // things, and whether they were all filed right is wanted now.
    let seen = Seen::in_tx(tx, scope)?;
    let broken = super::check::tag_group_violations(&seen.project());
    if !broken.is_empty() {
        out.insert(
            "tagGroupViolations".into(),
            Value::Array(
                broken
                    .into_iter()
                    .map(|(group, at, count)| {
                        let mut row = Map::new();
                        row.insert("group".into(), group.into());
                        row.insert("at".into(), at.into());
                        row.insert("count".into(), count.into());
                        Value::Object(row)
                    })
                    .collect(),
            ),
        );
    }
    Ok(Value::Object(out))
}

/// No group naming itself, through others or directly (`Validate.acyclic`).
fn acyclic(groups: &[Group]) -> Result<(), String> {
    fn walk(group: &Group, chain: &[String], groups: &[Group]) -> Result<(), String> {
        for member in &group.tags {
            let Some(nested) = groups.iter().find(|g| &g.name == member) else {
                continue;
            };
            let mut deeper = chain.to_vec();
            deeper.push(member.clone());
            if chain.contains(member) {
                return Err(text!("tags.group.cycle", chain = deeper.join(" → ")));
            }
            walk(nested, &deeper, groups)?;
        }
        Ok(())
    }
    for group in groups {
        walk(group, std::slice::from_ref(&group.name), groups)?;
    }
    Ok(())
}

/// The project's groups, replacing what it had (`Repository.setTagGroups`). Group names and
/// tag names share one namespace: a group may name another as a member.
fn set_tag_groups(tx: &mut Tx, scope: &Scope, all: &[Group]) -> Result<(), String> {
    if all.len() > TAG_GROUP_MAX_COUNT {
        return Err(text!("tags.group.tooMany", max = TAG_GROUP_MAX_COUNT));
    }
    let mut seen_names = HashSet::new();
    let mut cleaned = Vec::new();
    for group in all {
        let name = text(&group.name, &text!("tags.field.groupName"), TAG_MAX_LENGTH)?;
        if name.is_empty() {
            return Err(text!("tags.group.nameNeeded"));
        }
        if name.contains(',') {
            return Err(text!("tags.group.nameComma"));
        }
        if !seen_names.insert(name.clone()) {
            return Err(text!("tags.group.nameTwice", name = name));
        }
        cleaned.push(Group {
            name,
            tags: valid_tags(&group.tags, &text!("tags.field.tags"))?,
            exclusive: group.exclusive,
        });
    }
    acyclic(&cleaned)?;
    let in_use: HashSet<String> = Seen::in_tx(tx, scope)?
        .detail
        .schemas
        .iter()
        .flat_map(|c| c.tags.iter().cloned())
        .collect();
    if let Some(clash) = cleaned.iter().find(|g| in_use.contains(&g.name)) {
        return Err(text!("tags.group.nameIsTag", name = clash.name));
    }
    let stored = Value::Array(
        cleaned
            .iter()
            .map(|g| {
                let mut row = Map::new();
                row.insert("name".into(), g.name.clone().into());
                row.insert("tags".into(), g.tags.clone().into());
                row.insert("exclusive".into(), g.exclusive.into());
                Value::Object(row)
            })
            .collect(),
    );
    tx.conn()
        .execute(
            "UPDATE projects SET tag_groups = ? WHERE id = ?",
            params![stored.to_string(), scope.project],
        )
        .map_err(db)?;
    let said = if cleaned.is_empty() {
        keyed!("tags.line.groupsCleared")
    } else {
        keyed!(
            "tags.line.groupsSet",
            names = cleaned
                .iter()
                .map(|g| g.name.as_str())
                .collect::<Vec<_>>()
                .join(" / ")
        )
    };
    tx.line(
        &scope.project,
        &scope.tree,
        "project.tagGroups",
        "project",
        Some(&scope.project),
        said,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::sections::tests::{call, script_core};
    use serde_json::json;

    #[test]
    fn a_tag_group_is_the_projects() {
        let core = script_core();
        call(
            &core,
            "branch_create",
            json!({"project": "見本", "target": "work"}),
        )
        .unwrap();
        assert_eq!(
            call(
                &core,
                "tag_update",
                json!({"project": "見本", "branch": "work", "target": "group:領域", "value": {"add": ["中核"]}}),
            )
            .unwrap_err(),
            "タググループ（group:領域）はプロジェクトの設定で、branch ごとには持ちません。branch を付けずに呼びます（branch work で付け外しできるのはカードのタグです）"
        );
        let group = |value: Value| {
            call(
                &core,
                "tag_update",
                json!({"project": "見本", "target": "group:領域", "value": value}),
            )
        };
        assert_eq!(
            group(json!({"delete": true})).unwrap_err(),
            "グループ「領域」はありません"
        );
        assert_eq!(
            group(json!({"delete": true, "exclusive": true})).unwrap_err(),
            "delete はほかの指定と一緒には使えません"
        );
        assert_eq!(
            call(
                &core,
                "tag_update",
                json!({"project": "見本", "target": "group:中核", "value": {"add": ["x"]}}),
            )
            .unwrap_err(),
            "「中核」は既にタグとして使われています。グループ名とタグ名は同じ名前空間です"
        );
        let made = group(json!({"add": ["中核", "外"]})).unwrap();
        assert_eq!(
            made["groups"],
            json!([{"name": "領域", "tags": ["中核", "外"]}])
        );
        assert_eq!(group(json!({"delete": true})).unwrap()["groups"], json!([]));
    }

    #[test]
    fn a_cards_tags_are_put_on_and_taken_off() {
        let core = script_core();
        assert_eq!(
            call(
                &core,
                "tag_update",
                json!({"project": "見本", "target": "請求", "value": {"add": ["Schemas:Nope", "Schemas:Gone"]}}),
            )
            .unwrap_err(),
            "スキーマがありません: Schemas:Nope / Schemas:Gone。このプロジェクトには 11 件あります。find で探せます"
        );
        let tag = |value: Value| {
            call(
                &core,
                "tag_update",
                json!({"project": "見本", "branch": "_project_board_", "target": "やる", "value": value}),
            )
            .unwrap()
        };
        let added = tag(json!({"add": ["Goals:Ship"]}));
        assert_eq!(added["branch"], "_project_board_");
        assert_eq!(added["added"], json!(["Goals:Ship"]));
        // Already there: nothing changes, and nothing is said changed.
        assert_eq!(tag(json!({"add": ["Goals:Ship"]}))["added"], json!([]));
        let removed = tag(json!({"remove": ["Goals:Ship"]}));
        assert_eq!(removed["removed"], json!(["Goals:Ship"]));
        assert_eq!(removed["projectTags"], json!([]));
    }

    #[test]
    fn tags_are_trimmed_once_each_and_held_to_their_limits() {
        let given = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            valid_tags(&given(&[" a ", "a", "", "b"]), "タグ").unwrap(),
            vec!["a", "b"]
        );
        assert_eq!(
            valid_tags(&given(&["a,b"]), "タグ").unwrap_err(),
            "タグにカンマは使えません: a,b"
        );
        let many: Vec<String> = (0..33).map(|n| format!("t{n}")).collect();
        assert_eq!(
            valid_tags(&many, "Core:User のタグ").unwrap_err(),
            "Core:User のタグは 32 個までです（33 個になります）"
        );
    }

    #[test]
    fn a_group_that_names_itself_is_refused() {
        let group = |name: &str, tags: &[&str]| Group {
            name: name.to_string(),
            tags: tags.iter().map(|s| s.to_string()).collect(),
            exclusive: false,
        };
        assert!(acyclic(&[group("a", &["b"]), group("b", &["c"])]).is_ok());
        assert_eq!(
            acyclic(&[group("a", &["b"]), group("b", &["a"])]).unwrap_err(),
            "グループが循環しています: a → b → a。グループは自分自身を含められません"
        );
    }
}
