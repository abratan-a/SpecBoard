//! The check of a tree read item by item (`inspection_list`, `inspection_read`) and its items marked
//! as designed (`inspection_update`): worked out on every read, each item known by an id that is the
//! same in every tree and on every read, a mark kept for the project apart from any tree.

use crate::common;

use common::{PROJECT, call, headers, mcp, ok, op, project_id, refused, script_store, store_with};
use serde_json::{Value, json};
use specboard_core::Core;
use std::collections::HashSet;

const GROUPS: [&str; 17] = [
    "brokenPaths",
    "ambiguousPointers",
    "unresolvedPointers",
    "lineProblems",
    "sectionProblems",
    "cardProblems",
    "strayFields",
    "flowProblems",
    "peerMismatches",
    "missingPartners",
    "missingStores",
    "unlinkedPartners",
    "tagGroupViolations",
    "freeCards",
    "unreferenced",
    "uncheckedPatterns",
    "overlaps",
];

fn core() -> Core {
    Core::open(script_store()).unwrap()
}

fn list(core: &Core, value: Value) -> Value {
    ok(core, "inspection_list", json!({ "value": value }))
}

fn listed_in(core: &Core, tree: &str, value: Value) -> Value {
    ok(
        core,
        "inspection_list",
        json!({"branch": tree, "value": value}),
    )
}

fn ids(answer: &Value) -> Vec<String> {
    answer["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_str().unwrap().to_string())
        .collect()
}

/// The id of the first item of `group` the whole of main's check has.
fn first_of(core: &Core, group: &str) -> String {
    ids(&list(core, json!({"show": "all", "group": group})))[0].clone()
}

fn accept(core: &Core, target: Value, reason: &str) -> Value {
    ok(
        core,
        "inspection_update",
        json!({"target": target, "value": {"accept": reason}}),
    )
}

fn accepted_ids(core: &Core, tree: &str) -> Vec<String> {
    ids(&listed_in(core, tree, json!({"show": "accepted"})))
}

/// A project's commits on main, as many as there are.
fn commits(core: &Core) -> usize {
    ok(
        core,
        "branch_read",
        json!({"target": "main", "value": {"as": "history", "limit": 1000}}),
    )["commits"]
        .as_array()
        .unwrap()
        .len()
}

/// A tool called through MCP on the project `project` of the store, apart from the sample's.
fn on(core: &Core, project: &str, tool: &str, mut arguments: Value) -> Value {
    arguments["project"] = project.into();
    let (refused, text) = call(core, tool, arguments);
    assert!(!refused, "{tool}: {text}");
    serde_json::from_str(&text).unwrap()
}

/// Every item of a project's tree, read on through the pages.
fn every_id(core: &Core, project: &str, tree: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut after: Option<Value> = None;
    loop {
        let mut value = json!({"show": "all", "limit": 50});
        if let Some(next) = &after {
            value["after"] = next.clone();
        }
        let page = on(
            core,
            project,
            "inspection_list",
            json!({"branch": tree, "value": value}),
        );
        out.extend(ids(&page));
        match page.get("next") {
            Some(next) => after = Some(next.clone()),
            None => {
                let counts = &page["counts"];
                let whole = ["fix", "info", "accepted"]
                    .iter()
                    .map(|k| counts[*k].as_u64().unwrap() as usize)
                    .sum::<usize>();
                assert_eq!(out.len(), whole, "{project} {tree}");
                return out;
            }
        }
    }
}

fn hex_id(id: &str) -> bool {
    let Some((group, hex)) = id.split_once('-') else {
        return false;
    };
    GROUPS.contains(&group)
        && hex.len() == 10
        && hex
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
}

#[test]
fn the_list_gives_what_to_fix_by_default_and_counts_the_whole_tree() {
    let core = core();
    let fix = list(&core, json!({}));
    let keys: Vec<&str> = fix
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        ["project", "branch", "counts", "groups", "items"],
        "{fix}"
    );
    assert_eq!(fix["branch"], "main");
    assert_eq!(fix["counts"], json!({"fix": 6, "info": 1, "accepted": 0}));
    assert_eq!(
        fix["groups"],
        json!({
            "brokenPaths": {"kind": "fix", "label": "壊れたパス", "count": 3, "accepted": 0},
            "peerMismatches": {"kind": "fix", "label": "flow のやり取りの食い違い", "count": 2, "accepted": 0},
            "missingStores": {"kind": "fix", "label": "置き場が書かれていない", "count": 1, "accepted": 0},
            "freeCards": {"kind": "info", "label": "フリーセクションのカード", "count": 1, "accepted": 0}
        })
    );
    let items = fix["items"].as_array().unwrap();
    assert_eq!(items.len(), 6);
    let first = &items[0];
    assert!(hex_id(first["id"].as_str().unwrap()), "{first}");
    assert_eq!(
        first,
        &json!({
            "id": first["id"],
            "group": "brokenPaths",
            "at": "Docs:Overview",
            "problem": "body の Nope:Missing が解決できません",
            "hint": "セクション「Nope」がありません。あるのは: Docs / Schemas / Schemas/Ordering / Charts / Charts/Shop / Free / ts / zod / schema / charts_2 / docs_2"
        })
    );
    let groups: Vec<&str> = items.iter().map(|i| i["group"].as_str().unwrap()).collect();
    assert_eq!(
        groups,
        [
            "brokenPaths",
            "brokenPaths",
            "brokenPaths",
            "peerMismatches",
            "peerMismatches",
            "missingStores"
        ]
    );
    // The information alone, then all: in the groups' order.
    let info = list(&core, json!({"show": "info"}));
    assert_eq!(
        info["items"],
        json!([{
            "id": info["items"][0]["id"],
            "group": "freeCards",
            "at": "Free:Memo",
            "problem": "フリーセクション Free に置いたままです"
        }])
    );
    assert_eq!(info["counts"], fix["counts"]);
    let all = list(&core, json!({"show": "all"}));
    assert_eq!(all["items"].as_array().unwrap().len(), 7);
    assert_eq!(all["items"][6]["group"], "freeCards");
    assert_eq!(list(&core, json!({"show": "accepted"}))["items"], json!([]));
    // What is said is the check's groups alone: a document written from two sections is not one.
    for group in all["groups"].as_object().unwrap().keys() {
        assert!(GROUPS.contains(&group.as_str()), "{group}");
    }
    let said = refused(&core, "inspection_list", json!({"value": {"show": "some"}}));
    assert!(said.contains("show"), "{said}");
}

#[test]
fn the_list_is_kept_to_groups_and_to_a_section_or_a_card() {
    let core = core();
    let peers = list(&core, json!({"group": "peerMismatches"}));
    assert_eq!(peers["items"].as_array().unwrap().len(), 2);
    // The counts and the groups are the whole tree's still.
    assert_eq!(peers["counts"]["fix"], 6);
    assert_eq!(peers["groups"].as_object().unwrap().len(), 4);
    let two = list(
        &core,
        json!({"show": "all", "group": ["brokenPaths", "freeCards"]}),
    );
    assert_eq!(two["items"].as_array().unwrap().len(), 4);
    let said = refused(
        &core,
        "inspection_list",
        json!({"value": {"group": "nope"}}),
    );
    assert!(
        said.starts_with("組 nope はありません。あるのは: brokenPaths"),
        "{said}"
    );
    // A section holds what is in the sections under it.
    let charts = list(&core, json!({"show": "all", "in": "Charts"}));
    let at: Vec<&str> = charts["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["at"].as_str().unwrap())
        .collect();
    assert_eq!(
        at,
        [
            "Charts/Shop:Refund.start",
            "Charts/Shop:Refund.ask.steps[1]",
            "Charts/Shop:Refund.a.steps[0]"
        ]
    );
    assert_eq!(
        list(&core, json!({"show": "all", "in": "Charts/Shop:Refund"}))["items"],
        charts["items"]
    );
    assert_eq!(
        list(&core, json!({"in": "Docs"}))["items"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(
        list(&core, json!({"in": "Free"}))["items"],
        json!([]),
        "the free card is information"
    );
    assert_eq!(
        list(&core, json!({"show": "info", "in": "Free"}))["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let said = refused(&core, "inspection_list", json!({"value": {"in": "Nope"}}));
    assert!(said.contains("Nope"), "{said}");
    refused(
        &core,
        "inspection_list",
        json!({"value": {"in": "Docs:Nothing"}}),
    );
}

#[test]
fn a_long_list_is_read_on_from_the_last_id_given() {
    let core = core();
    let whole = ids(&list(&core, json!({"show": "all"})));
    let first = list(&core, json!({"show": "all", "limit": 2}));
    assert_eq!(first["truncated"], true);
    assert_eq!(first["matched"], 7);
    assert_eq!(ids(&first), whole[..2]);
    assert_eq!(first["next"], whole[1].as_str());
    // What describes the tree is never cut.
    assert_eq!(first["groups"].as_object().unwrap().len(), 4);
    assert_eq!(first["counts"]["fix"], 6);
    let mut read = ids(&first);
    let mut after = first["next"].clone();
    loop {
        let page = list(&core, json!({"show": "all", "limit": 2, "after": after}));
        read.extend(ids(&page));
        match page.get("next") {
            Some(next) => after = next.clone(),
            None => break,
        }
    }
    assert_eq!(read, whole);
}

#[test]
fn an_item_is_known_by_the_same_id_on_every_read_and_in_every_tree() {
    let core = core();
    let main = every_id(&core, PROJECT, "main");
    assert_eq!(every_id(&core, PROJECT, "main"), main);
    // A branch sees main: the same items, by the same ids.
    ok(&core, "branch_create", json!({"target": "work"}));
    assert_eq!(every_id(&core, PROJECT, "work"), main);
    for tree in ["main", "work", "_project_board_"] {
        let all = every_id(&core, PROJECT, tree);
        let unique: HashSet<&String> = all.iter().collect();
        assert_eq!(unique.len(), all.len(), "{tree}");
        assert!(all.iter().all(|id| hex_id(id)), "{tree}: {all:?}");
    }
}

#[test]
fn no_two_items_of_a_tree_share_an_id_in_a_project_with_much_to_say() {
    let core = core();
    ok(&core, "section_create", json!({"target": "Schemas/Many"}));
    for n in 0..12 {
        ok(
            &core,
            "card_create",
            json!({"target": format!("Schemas/Many:Value{n}"), "value": {"kind": "value"}}),
        );
        ok(
            &core,
            "card_create",
            json!({"target": format!("Free:Note{n}"), "value": {"kind": "note"}}),
        );
    }
    // The same broken path written twice in one text, and two of them in another.
    ok(
        &core,
        "card_create",
        json!({"target": "Schemas/Many:Twice", "value": {"kind": "entity", "description": "[[Nope:A]] と [[Nope:A]]"}}),
    );
    ok(
        &core,
        "card_create",
        json!({"target": "Schemas/Many:Two", "value": {"kind": "entity", "description": "[[Nope:A]] と [[Nope:B]]"}}),
    );
    ok(
        &core,
        "tag_update",
        json!({"target": "group:stage", "value": {"add": ["draft", "done"], "exclusive": true}}),
    );
    for tag in ["draft", "done"] {
        ok(
            &core,
            "tag_update",
            json!({"target": tag, "value": {"add": ["Schemas/Many:Twice", "Schemas/Many:Two"]}}),
        );
    }
    ok(&core, "branch_create", json!({"target": "more"}));
    ok(
        &core,
        "card_create",
        json!({"branch": "more", "target": "Schemas/Many:Three", "value": {"kind": "entity", "description": "[[Nope:A]] [[Nope:C]]"}}),
    );
    for tree in ["main", "more"] {
        let all = every_id(&core, PROJECT, tree);
        let unique: HashSet<&String> = all.iter().collect();
        assert_eq!(unique.len(), all.len(), "{tree}");
        assert!(all.len() > 30, "{tree}: {}", all.len());
    }
    let groups: HashSet<String> = every_id(&core, PROJECT, "more")
        .iter()
        .map(|id| id.split_once('-').unwrap().0.to_string())
        .collect();
    for group in [
        "brokenPaths",
        "peerMismatches",
        "tagGroupViolations",
        "freeCards",
        "unreferenced",
    ] {
        assert!(groups.contains(group), "{group}: {groups:?}");
    }
}

#[test]
fn no_two_items_of_a_tree_share_an_id_in_any_fixture() {
    let core = Core::open(store_with(&["sample", "large", "datamodel", "small"])).unwrap();
    let mut found = 0;
    for project in [
        "見本",
        "Harbor City Library Network",
        "データモデルの見本",
        "最小の見本",
    ] {
        let all = every_id(&core, project, "main");
        let unique: HashSet<&String> = all.iter().collect();
        assert_eq!(unique.len(), all.len(), "{project}");
        found += all.len();
    }
    assert!(found > 10, "{found}");
}

#[test]
fn an_item_is_read_whole_with_how_to_fix_it() {
    let core = core();
    let broken = first_of(&core, "brokenPaths");
    let free = first_of(&core, "freeCards");
    let read = ok(
        &core,
        "inspection_read",
        json!({"target": [broken, free, "brokenPaths-0000000000"]}),
    );
    assert_eq!(read["branch"], "main");
    assert_eq!(
        read["items"],
        json!([
            {
                "id": broken,
                "group": "brokenPaths",
                "kind": "fix",
                "label": "壊れたパス",
                "at": "Docs:Overview",
                "problem": "body の Nope:Missing が解決できません",
                "via": "body",
                "path": "Nope:Missing",
                "hint": "セクション「Nope」がありません。あるのは: Docs / Schemas / Schemas/Ordering / Charts / Charts/Shop / Free / ts / zod / schema / charts_2 / docs_2",
                "fix": "パスを今ある所に書き直すか、指す先を作ります（改名・移動したものは新しいパスに）"
            },
            {
                "id": free,
                "group": "freeCards",
                "kind": "info",
                "label": "フリーセクションのカード",
                "at": "Free:Memo",
                "problem": "フリーセクション Free に置いたままです",
                "section": "Free",
                "fix": "置き場が決まったら、種類の合うセクションへ移します"
            }
        ])
    );
    assert_eq!(read["notFound"], json!(["brokenPaths-0000000000"]));
    assert_eq!(
        read["hint"],
        "見つからない id は、直ったか、ほかの木の件です（inspection_list で読み直します）"
    );
    assert_eq!(
        refused(
            &core,
            "inspection_read",
            json!({"target": "brokenPaths-0000000000"})
        ),
        "brokenPaths-0000000000 は見つかりません（直ったか、ほかの木の件です。inspection_list で読み直します）"
    );
    refused(
        &core,
        "inspection_read",
        json!({"target": broken, "value": {"show": "all"}}),
    );
}

#[test]
fn an_item_marked_as_designed_leaves_what_to_fix_and_comes_back_when_unmarked() {
    let core = core();
    let peer = first_of(&core, "peerMismatches");
    let marked = accept(&core, json!(peer), "相手は外で受ける");
    assert_eq!(marked["project"], PROJECT);
    assert_eq!(marked["branch"], "main");
    let mark = &marked["updated"][0]["accepted"];
    assert_eq!(marked["updated"][0]["id"], peer.as_str());
    assert_eq!(mark["reason"], "相手は外で受ける");
    assert_eq!(mark["by"], common::KEY);
    assert!(mark["at"].is_string());
    let fix = list(&core, json!({}));
    assert!(!ids(&fix).contains(&peer));
    assert_eq!(fix["counts"], json!({"fix": 5, "info": 1, "accepted": 1}));
    assert_eq!(
        fix["groups"]["peerMismatches"],
        json!({"kind": "fix", "label": "flow のやり取りの食い違い", "count": 1, "accepted": 1})
    );
    let accepted = list(&core, json!({"show": "accepted"}));
    assert_eq!(ids(&accepted), [peer.as_str()]);
    assert_eq!(accepted["items"][0]["accepted"], *mark);
    assert_eq!(
        ok(&core, "inspection_read", json!({"target": peer}))["items"][0]["accepted"],
        *mark
    );
    // Marked again: the reason of now.
    let again = accept(&core, json!(peer), "受け口は次の版で");
    assert_eq!(
        again["updated"][0]["accepted"]["reason"],
        "受け口は次の版で"
    );
    assert_eq!(accepted_ids(&core, "main"), [peer.as_str()]);
    let unmarked = ok(
        &core,
        "inspection_update",
        json!({"target": peer, "value": {"accept": false}}),
    );
    assert_eq!(unmarked["updated"], json!([{"id": peer, "accepted": null}]));
    let fix = list(&core, json!({}));
    assert!(ids(&fix).contains(&peer));
    assert_eq!(fix["counts"]["accepted"], 0);
}

#[test]
fn a_mark_is_refused_whole_when_any_of_it_is_wrong() {
    let core = core();
    let broken = first_of(&core, "brokenPaths");
    let said = refused(
        &core,
        "inspection_update",
        json!({"target": [broken, "brokenPaths-0000000000"], "value": {"accept": "理由"}}),
    );
    assert_eq!(
        said,
        "brokenPaths-0000000000 は見つかりません（直ったか、ほかの木の件です。inspection_list で読み直します）"
    );
    assert!(accepted_ids(&core, "main").is_empty());
    for value in [
        json!({"accept": ""}),
        json!({"accept": "  "}),
        json!({"accept": true}),
        json!({"accept": 3}),
        json!({}),
        json!({"accept": "理由", "show": "all"}),
    ] {
        refused(
            &core,
            "inspection_update",
            json!({"target": broken, "value": value}),
        );
    }
    let said = refused(
        &core,
        "inspection_update",
        json!({"target": broken, "value": {"accept": true}}),
    );
    assert!(said.starts_with("accept: true は書けません"), "{said}");
    assert!(accepted_ids(&core, "main").is_empty());
    // Taking a mark off one that has none.
    let said = refused(
        &core,
        "inspection_update",
        json!({"target": broken, "value": {"accept": false}}),
    );
    assert!(
        said.starts_with(&format!("{broken} には印がありません")),
        "{said}"
    );
    accept(&core, json!(broken), "見本の壊れたパス");
    let other = ids(&list(&core, json!({"group": "brokenPaths"})))[0].clone();
    refused(
        &core,
        "inspection_update",
        json!({"target": [broken, other], "value": {"accept": false}}),
    );
    assert_eq!(accepted_ids(&core, "main"), [broken.as_str()]);
    // An item no longer in the tree has its mark taken off still.
    ok(
        &core,
        "inspection_update",
        json!({"target": broken, "value": {"accept": false}}),
    );
    assert!(accepted_ids(&core, "main").is_empty());
}

#[test]
fn a_mark_made_in_a_branch_holds_in_every_tree_and_on_main_once_merged() {
    let core = core();
    ok(&core, "branch_create", json!({"target": "work"}));
    // An item main has too, marked while the branch is read.
    let peer = ids(&listed_in(
        &core,
        "work",
        json!({"group": "peerMismatches"}),
    ))[0]
        .clone();
    ok(
        &core,
        "inspection_update",
        json!({"branch": "work", "target": peer, "value": {"accept": "外で受ける"}}),
    );
    assert_eq!(accepted_ids(&core, "main"), [peer.as_str()]);
    assert_eq!(accepted_ids(&core, "work"), [peer.as_str()]);
    // An item the branch alone has: not on main, until the branch is merged.
    ok(
        &core,
        "card_create",
        json!({"branch": "work", "target": "Schemas:Pointer", "value": {"kind": "entity", "description": "見る: [[Nope:There]]"}}),
    );
    let pointer: Vec<Value> = listed_in(&core, "work", json!({"in": "Schemas:Pointer"}))["items"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(pointer.len(), 1, "{pointer:?}");
    let pointer = pointer[0]["id"].as_str().unwrap().to_string();
    let said = refused(
        &core,
        "inspection_update",
        json!({"target": pointer, "value": {"accept": "main には無い"}}),
    );
    assert!(said.contains("見つかりません"), "{said}");
    ok(
        &core,
        "inspection_update",
        json!({"branch": "work", "target": pointer, "value": {"accept": "あとで作る"}}),
    );
    assert_eq!(accepted_ids(&core, "main"), [peer.as_str()]);
    ok(&core, "branch_merge", json!({"target": "work"}));
    let main = list(&core, json!({"show": "accepted"}));
    assert_eq!(ids(&main), [pointer.clone(), peer.clone()]);
    assert_eq!(main["items"][0]["accepted"]["reason"], "あとで作る");
}

#[test]
fn an_item_that_says_something_else_comes_back_with_the_mark_of_before() {
    let core = core();
    ok(
        &core,
        "tag_update",
        json!({"target": "group:stage", "value": {"add": ["draft", "done", "old"], "exclusive": true}}),
    );
    for tag in ["draft", "done"] {
        ok(
            &core,
            "tag_update",
            json!({"target": tag, "value": {"add": ["Schemas:Customer"]}}),
        );
    }
    let violation = |core: &Core, show: &str| {
        list(
            core,
            json!({"show": show, "group": "tagGroupViolations", "in": "Schemas:Customer"}),
        )["items"]
            .as_array()
            .unwrap()
            .clone()
    };
    let found = violation(&core, "fix");
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0]["at"], "Schemas:Customer");
    assert_eq!(
        found[0]["problem"],
        "排他のタググループ stage のタグが 2 つ付いています"
    );
    let id = found[0]["id"].as_str().unwrap().to_string();
    accept(&core, json!(id), "移る途中は 2 つ");
    assert!(violation(&core, "fix").is_empty());
    // A third tag of the group: what the item says is not what was marked.
    ok(
        &core,
        "tag_update",
        json!({"target": "old", "value": {"add": ["Schemas:Customer"]}}),
    );
    let back = violation(&core, "fix");
    assert_eq!(back.len(), 1, "{back:?}");
    assert_eq!(back[0]["id"], id.as_str());
    assert_eq!(
        back[0]["problem"],
        "排他のタググループ stage のタグが 3 つ付いています"
    );
    assert_eq!(back[0]["acceptedBefore"]["reason"], "移る途中は 2 つ");
    assert!(back[0].get("accepted").is_none());
    assert!(violation(&core, "accepted").is_empty());
    let read = ok(&core, "inspection_read", json!({"target": id}))["items"][0].clone();
    assert_eq!(read["tagGroup"], "stage");
    assert_eq!(read["count"], 3);
    assert_eq!(read["acceptedBefore"]["reason"], "移る途中は 2 つ");
    // Marked again, it is as designed as it is now.
    accept(&core, json!(id), "3 つでよい");
    assert_eq!(violation(&core, "accepted").len(), 1);
}

#[test]
fn a_hint_that_changes_leaves_the_mark_on() {
    let core = core();
    let broken = first_of(&core, "brokenPaths");
    accept(&core, json!(broken), "まだ書いていない");
    // A section made changes what the hint names, not what the item is.
    ok(&core, "section_create", json!({"target": "Schemas/Extra"}));
    let read = ok(&core, "inspection_read", json!({"target": broken}))["items"][0].clone();
    assert!(
        read["hint"].as_str().unwrap().contains("Schemas/Extra"),
        "{read}"
    );
    assert_eq!(read["accepted"]["reason"], "まだ書いていない");
    assert_eq!(accepted_ids(&core, "main"), [broken]);
}

#[test]
fn marks_go_on_a_locked_main_and_into_no_history() {
    let core = core();
    let project = project_id(&core);
    let locked = op(
        &core,
        json!({"op": "main_lock", "project": project, "on": true}),
    );
    assert!(locked.get("error").is_none(), "{locked}");
    let before = commits(&core);
    let broken = first_of(&core, "brokenPaths");
    accept(&core, json!(broken), "ロック中でも");
    assert_eq!(accepted_ids(&core, "main"), [broken.as_str()]);
    ok(
        &core,
        "inspection_update",
        json!({"target": broken, "value": {"accept": false}}),
    );
    assert_eq!(commits(&core), before);
    // Main itself is not written while it is locked.
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Free:Memo", "value": {"description": "x"}}),
    );
    assert!(said.contains("ロック中"), "{said}");
}

#[test]
fn an_archived_project_takes_no_mark() {
    let core = core();
    let broken = first_of(&core, "brokenPaths");
    accept(&core, json!(broken), "アーカイブの前に");
    let archived = op(
        &core,
        json!({"op": "project_archive", "project": project_id(&core)}),
    );
    assert!(archived.get("error").is_none(), "{archived}");
    let peer = first_of(&core, "peerMismatches");
    for (target, value) in [
        (peer.clone(), json!({"accept": "理由"})),
        (broken.clone(), json!({"accept": false})),
    ] {
        let said = mcp(
            &core,
            "inspection_update",
            json!({"target": target, "value": value}),
        )
        .unwrap_err();
        assert!(said.contains("アーカイブ"), "{said}");
    }
    assert_eq!(accepted_ids(&core, "main"), [broken]);
}

#[test]
fn the_marks_go_with_the_project_file_and_a_file_of_before_has_none() {
    let core = core();
    let project = project_id(&core);
    let file = |core: &Core| -> Value {
        let text = op(core, json!({"op": "project_file", "project": project}))["result"]["text"]
            .as_str()
            .unwrap()
            .to_string();
        serde_json::from_str(&text).unwrap()
    };
    assert!(file(&core).get("inspectionMarks").is_none());
    let broken = first_of(&core, "brokenPaths");
    let peer = first_of(&core, "peerMismatches");
    accept(&core, json!([peer, broken]), "見本のまま");
    let written = file(&core);
    assert_eq!(written["version"], 33);
    let marks = written["inspectionMarks"].as_array().unwrap();
    // In the order of their ids.
    let mut order = vec![broken.clone(), peer.clone()];
    order.sort();
    let said: Vec<&str> = marks.iter().map(|m| m["id"].as_str().unwrap()).collect();
    assert_eq!(said, order);
    let keys: Vec<&str> = marks[0]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys, ["id", "group", "print", "reason", "by", "at"]);
    assert_eq!(marks[0]["reason"], "見本のまま");
    // Read in its place: the marks come back with it.
    let text = written.to_string();
    let read = op(
        &core,
        json!({"op": "project_file_read", "text": text, "landing": "replace"}),
    );
    assert!(read.get("error").is_none(), "{read}");
    let mut back = accepted_ids(&core, "main");
    back.sort();
    assert_eq!(back, order);
    // A copy has them as they are: an item is known by what it is, not by an id of the file.
    let copy = op(
        &core,
        json!({"op": "project_file_read", "text": text, "landing": "copy", "name": "写し"}),
    );
    assert!(copy.get("error").is_none(), "{copy}");
    // The key the tests call with was given the projects before the copy: the window reads it.
    let copied = op(
        &core,
        json!({"op": "tool", "name": "inspection_list", "arguments": {"project": "写し", "value": {"show": "accepted"}}}),
    );
    let mut copied = ids(&copied["result"]["json"]);
    copied.sort();
    assert_eq!(copied, order);
    // A file of before has none.
    let mut before = written.clone();
    before["version"] = 20.into();
    before
        .as_object_mut()
        .unwrap()
        .shift_remove("inspectionMarks");
    let read = op(
        &core,
        json!({"op": "project_file_read", "text": before.to_string(), "landing": "replace"}),
    );
    assert!(read.get("error").is_none(), "{read}");
    assert!(accepted_ids(&core, "main").is_empty());
    assert_eq!(list(&core, json!({}))["counts"]["accepted"], 0);
}

#[test]
fn the_marks_move_with_the_project() {
    let here = core();
    let broken = first_of(&here, "brokenPaths");
    accept(&here, json!(broken), "動かしても");
    let bundle = op(
        &here,
        json!({"op": "project_bundle", "project": project_id(&here)}),
    )["result"]
        .clone();
    let there = Core::create(common::place("inspection-there.sqlite")).unwrap();
    let read = op(
        &there,
        json!({"op": "project_bundle_read", "bundle": bundle}),
    );
    assert!(read.get("error").is_none(), "{read}");
    assert_eq!(accepted_ids(&there, "main"), [broken]);
}

#[test]
fn the_window_reads_the_whole_list_and_marks_as_the_window() {
    let core = core();
    let window = |name: &str, arguments: Value| -> Value {
        let said = op(
            &core,
            json!({"op": "tool", "name": name, "arguments": arguments}),
        );
        assert!(said.get("error").is_none(), "{said}");
        said["result"]["json"].clone()
    };
    // Not cut: the window reads whole.
    let all = window(
        "inspection_list",
        json!({"project": PROJECT, "value": {"show": "all", "limit": 1}}),
    );
    assert_eq!(all["items"].as_array().unwrap().len(), 7);
    assert!(all.get("truncated").is_none());
    let id = all["items"][0]["id"].clone();
    let marked = window(
        "inspection_update",
        json!({"project": PROJECT, "target": id, "value": {"accept": "画面で"}}),
    );
    assert_eq!(marked["updated"][0]["accepted"]["by"], "画面");
}

#[test]
fn a_project_that_denies_the_marks_to_mcp_reads_them_still() {
    let core = core();
    let set = op(
        &core,
        json!({"op": "mcp_writes_set", "project": project_id(&core), "item": "inspection_update", "state": "deny"}),
    );
    assert!(set.get("error").is_none(), "{set}");
    let broken = first_of(&core, "brokenPaths");
    let said = refused(
        &core,
        "inspection_update",
        json!({"target": broken, "value": {"accept": "理由"}}),
    );
    assert!(said.contains("点検の件に印を付ける"), "{said}");
    assert_eq!(list(&core, json!({}))["counts"]["fix"], 6);
}

/// The items of `group` in main, whole.
fn items_of(core: &Core, group: &str) -> Vec<Value> {
    list(core, json!({"show": "all", "group": group}))["items"]
        .as_array()
        .unwrap()
        .clone()
}

#[test]
fn a_begin_naming_no_one_is_a_missing_partner() {
    let core = crate::actor_sections::shop_core();
    let items = items_of(&core, "missingPartners");
    let at: HashSet<&str> = items.iter().map(|i| i["at"].as_str().unwrap()).collect();
    assert_eq!(
        at,
        HashSet::from([
            "Charts/決済サービス:Charge.start",
            "Charts/注文サーバ/在庫:Reserve.start"
        ]),
        "{items:?}"
    );
    let reserve = items
        .iter()
        .find(|i| i["at"] == "Charts/注文サーバ/在庫:Reserve.start")
        .unwrap();
    let read = ok(&core, "inspection_read", json!({"target": reserve["id"]}))["items"][0].clone();
    assert_eq!(read["kind"], "fix", "{read}");
    assert_eq!(read["label"], "相手が書かれていない");
    assert_eq!(
        read["problem"],
        "受ける相手が書かれていません（今これを呼ぶ flow: Charts/注文サーバ/受付:TakeOrder）"
    );
    assert_eq!(read["callers"], json!(["Charts/注文サーバ/受付:TakeOrder"]));
    assert!(
        read["fix"].as_str().unwrap().contains("anyCaller: true"),
        "{read}"
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Charts/決済サービス:Charge.start", "value": {"anyCaller": true}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Charts/注文サーバ/在庫:Reserve.start", "value": {"connections": {
            "add": [{"line": "partner", "to": "Charts/注文サーバ/受付:TakeOrder"}]}}}),
    );
    assert_eq!(items_of(&core, "missingPartners"), Vec::<Value>::new());
    // Whoever writes one is told at once.
    let made = ok(
        &core,
        "card_create",
        json!({"value": {
            "Charts/シナリオ:Ask": {"kind": "flow"},
            "Charts/シナリオ:Ask.start": {"kind": "begin", "trigger": "receive",
                "connections": [{"line": "next", "to": "done"}]},
            "Charts/シナリオ:Ask.done": {"kind": "end", "outcome": "success"}
        }}),
    );
    assert!(
        made["warnings"].as_array().into_iter().flatten().any(|w| w
            .as_str()
            .unwrap_or("")
            .contains("受ける相手が書かれていません")),
        "{made}"
    );
}

#[test]
fn a_row_keeping_a_shape_no_store_holds_is_a_missing_store() {
    let core = crate::actor_sections::shop_core();
    // The shop's own: the store the project it is made in already lacks is not its.
    let take_order = |core: &Core| -> Vec<Value> {
        list(
            core,
            json!({"show": "all", "group": "missingStores", "in": "Charts/注文サーバ/受付:TakeOrder"}),
        )["items"]
            .as_array()
            .unwrap()
            .clone()
    };
    let items = take_order(&core);
    assert_eq!(items.len(), 1, "{items:?}");
    assert_eq!(
        items[0]["at"],
        "Charts/注文サーバ/受付:TakeOrder.save.steps[2]"
    );
    let read = ok(&core, "inspection_read", json!({"target": items[0]["id"]}))["items"][0].clone();
    assert_eq!(read["kind"], "fix", "{read}");
    assert_eq!(read["label"], "置き場が書かれていない");
    assert_eq!(
        read["problem"],
        "Receipt を置く store がありません（write Receipt.number）"
    );
    assert_eq!(read["shapes"], json!(["Schemas/Shop:Receipt"]));
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas/Shop:ShopDB", "value": {"connections": {
            "add": [{"line": "holds", "to": "Schemas/Shop:Receipt"}]}}}),
    );
    assert_eq!(take_order(&core), Vec::<Value>::new());
}

#[test]
fn a_shape_reaching_two_stores_is_a_missing_store_naming_them() {
    let core = crate::actor_sections::shop_core();
    crate::actor_sections::receipt_in_two_stores(&core);
    let take_order = |core: &Core| -> Vec<Value> {
        list(
            core,
            json!({"show": "all", "group": "missingStores", "in": "Charts/注文サーバ/受付:TakeOrder"}),
        )["items"]
            .as_array()
            .unwrap()
            .clone()
    };
    let items = take_order(&core);
    assert_eq!(items.len(), 1, "{items:?}");
    assert_eq!(
        items[0]["at"],
        "Charts/注文サーバ/受付:TakeOrder.save.steps[2]"
    );
    let read = ok(&core, "inspection_read", json!({"target": items[0]["id"]}))["items"][0].clone();
    assert_eq!(
        read["problem"],
        "Receipt を置く store が決まりません（ShopDB・Archive のどれか。write Receipt.number）",
        "{read}"
    );
    assert_eq!(read["shapes"], json!(["Schemas/Shop:Receipt"]));
    assert_eq!(
        read["candidates"],
        json!({"Schemas/Shop:Receipt": ["Schemas/Shop:ShopDB", "Schemas/Shop:Archive"]})
    );
    // The row names the store by a path inside the shape it holds.
    ok(
        &core,
        "card_update",
        json!({"target": "Charts/注文サーバ/受付:TakeOrder.save", "value": {"connections": {
            "remove": [{"line": "writes", "from": "steps[2]", "to": "Schemas/Shop:Receipt", "at": "number"}],
            "add": [{"line": "writes", "from": "steps[2]", "to": "Schemas/Shop:Order", "at": "receipt.number"}]}}}),
    );
    assert_eq!(take_order(&core), Vec::<Value>::new());
}

#[test]
fn a_shape_held_by_two_stores_is_a_missing_store_naming_them() {
    let core = crate::actor_sections::shop_core();
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas/Shop:ShopDB", "value": {"connections": {
            "add": [{"line": "holds", "to": "Schemas/Shop:Receipt"}]}}}),
    );
    ok(
        &core,
        "card_create",
        json!({"target": "Schemas/Shop:Archive", "value": {"kind": "store", "connections": [
            {"line": "holds", "to": "Schemas/Shop:Receipt"}]}}),
    );
    let take_order = |core: &Core| -> Vec<Value> {
        list(
            core,
            json!({"show": "all", "group": "missingStores", "in": "Charts/注文サーバ/受付:TakeOrder"}),
        )["items"]
            .as_array()
            .unwrap()
            .clone()
    };
    let items = take_order(&core);
    assert_eq!(items.len(), 1, "{items:?}");
    assert_eq!(
        items[0]["at"],
        "Charts/注文サーバ/受付:TakeOrder.save.steps[2]"
    );
    let read = ok(&core, "inspection_read", json!({"target": items[0]["id"]}))["items"][0].clone();
    assert_eq!(
        read["problem"],
        "Receipt を置く store が決まりません（ShopDB・Archive のどれか。write Receipt.number）",
        "{read}"
    );
    assert_eq!(
        read["candidates"],
        json!({"Schemas/Shop:Receipt": ["Schemas/Shop:ShopDB", "Schemas/Shop:Archive"]})
    );
    // One store left holding it keeps it.
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas/Shop:Archive", "value": {"connections": {
            "remove": [{"line": "holds", "to": "Schemas/Shop:Receipt"}]}}}),
    );
    assert_eq!(take_order(&core), Vec::<Value>::new());
}

#[test]
fn a_program_named_like_a_flow_or_an_actor_is_an_unlinked_partner() {
    let core = crate::actor_sections::shop_core();
    let made = ok(
        &core,
        "card_create",
        json!({"value": {
            "Charts/ブラウザ:Track": {"kind": "flow"},
            "Charts/ブラウザ:Track.start": {"kind": "begin", "trigger": "receive", "program": "注文サーバ",
                "connections": [{"line": "next", "to": "ask"}]},
            "Charts/ブラウザ:Track.ask": {"kind": "process",
                "steps": [
                    {"do": "send", "program": "Charge"},
                    {"do": "call", "program": "Charts/決済サービス:Charge"},
                    {"do": "call", "program": "git"},
                    {"do": "send", "program": "ブラウザ"},
                    {"do": "send", "program": "Track"},
                    {"do": "send", "program": "charge"},
                    {"do": "send", "program": "DB"}],
                "connections": [{"line": "next", "to": "done"}]},
            "Charts/ブラウザ:Track.done": {"kind": "end", "outcome": "success",
                "return": {"program": "支払う"}}
        }}),
    );
    // The rows of a name no flow or actor of the project has, of the actor around the flow, of the
    // flow itself, of another case and of an actor with no flow are not told.
    let items = items_of(&core, "unlinkedPartners");
    let at: Vec<&str> = items.iter().map(|i| i["at"].as_str().unwrap()).collect();
    assert_eq!(
        at,
        [
            "Charts/ブラウザ:Track.start",
            "Charts/ブラウザ:Track.ask.steps[0]",
            "Charts/ブラウザ:Track.ask.steps[1]",
            "Charts/ブラウザ:Track.done"
        ],
        "{items:?}"
    );
    for item in &items {
        assert!(hex_id(item["id"].as_str().unwrap()), "{item}");
    }
    let before = ids(&json!({"items": items}));
    let read = ok(&core, "inspection_read", json!({"target": before}));
    let read = read["items"].as_array().unwrap();
    for item in read {
        assert_eq!(item["kind"], "fix", "{item}");
        assert_eq!(item["label"], "相手が線になっていない", "{item}");
    }
    // A name the flows of the project have: the flows are told, and no actor.
    let sent = &read[1];
    assert_eq!(
        sent["problem"],
        "send の相手 Charge は文字ですが、flow Charts/決済サービス:Charge と同じ名前です"
    );
    assert_eq!(sent["op"], "send");
    assert_eq!(sent["program"], "Charge");
    assert_eq!(sent["flows"], json!(["Charts/決済サービス:Charge"]));
    assert!(sent.get("actor").is_none(), "{sent}");
    let fix = sent["fix"].as_str().unwrap();
    for said in ["partner の線", "実行ノード", "send と receive の行"] {
        assert!(fix.contains(said), "{said}: {fix}");
    }
    // A name an actor with flows in it has: the actor is told, and no flow.
    let start = &read[0];
    assert_eq!(
        start["problem"],
        "receive の相手 注文サーバ は文字ですが、アクター Charts/注文サーバ と同じ名前です"
    );
    assert_eq!(start["op"], "receive");
    assert_eq!(start["program"], "注文サーバ");
    assert_eq!(start["actor"], "Charts/注文サーバ");
    assert!(start.get("flows").is_none(), "{start}");
    // The path of a flow, and the heading of one, are its name as well.
    assert_eq!(read[2]["op"], "call");
    assert_eq!(read[2]["flows"], json!(["Charts/決済サービス:Charge"]));
    assert_eq!(read[3]["op"], "return");
    assert_eq!(read[3]["program"], "支払う");
    assert_eq!(read[3]["flows"], json!(["Charts/決済サービス:Charge"]));
    // Whoever writes it is told at once, and the flow's own group does not say it twice.
    let warnings: Vec<&str> = made["warnings"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|w| w.contains("は文字ですが"))
        .collect();
    assert!(
        warnings.iter().any(|w| w.contains("Charge は文字ですが")),
        "{made}"
    );
    assert_eq!(warnings.len(), 4, "{made}");
    assert!(
        !items_of(&core, "flowProblems")
            .iter()
            .any(|i| i["problem"].as_str().unwrap_or("").contains("は文字ですが"))
    );
    // A peer line in place of the program takes the row out; the others stay as they were.
    ok(
        &core,
        "card_update",
        json!({"target": "Charts/ブラウザ:Track.ask", "value": {
            "steps": [
                {"do": "send"},
                {"do": "call", "program": "Charts/決済サービス:Charge"},
                {"do": "call", "program": "git"},
                {"do": "send", "program": "ブラウザ"},
                {"do": "send", "program": "Track"},
                {"do": "send", "program": "charge"},
                {"do": "send", "program": "DB"}],
            "connections": {"add": [
                {"line": "partner", "from": "steps[0]", "to": "Charts/決済サービス:Charge"}]}}}),
    );
    let after = ids(&json!({"items": items_of(&core, "unlinkedPartners")}));
    assert_eq!(
        after,
        [before[0].clone(), before[2].clone(), before[3].clone()]
    );
}

#[test]
fn free_cards_are_said_one_by_one() {
    let core = core();
    ok(
        &core,
        "card_create",
        json!({"target": "Free:Later", "value": {"kind": "note"}}),
    );
    let free = list(&core, json!({"show": "info", "group": "freeCards"}));
    let at: Vec<&str> = free["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["at"].as_str().unwrap())
        .collect();
    assert_eq!(at, ["Free:Memo", "Free:Later"]);
    assert_eq!(free["groups"]["freeCards"]["count"], 2);
    let read = ok(&core, "inspection_read", json!({"target": ids(&free)}));
    for item in read["items"].as_array().unwrap() {
        assert_eq!(item["section"], "Free");
    }
}

#[test]
fn the_group_labels_are_said_in_the_language_the_request_asks_for() {
    let core = core();
    let window = |lang: Option<&str>| {
        let mut request = json!({
            "op": "tool",
            "name": "inspection_list",
            "arguments": {"project": PROJECT, "value": {"show": "all"}}
        });
        if let Some(lang) = lang {
            request["lang"] = lang.into();
        }
        op(&core, request)["result"]["json"]["groups"]["brokenPaths"]["label"].clone()
    };
    assert_eq!(window(Some("en")), "Broken paths");
    assert_eq!(window(Some("ja")), "壊れたパス");
    assert_eq!(window(None), "壊れたパス");

    // MCP: the agent sends the language its place answers MCP in.
    let asked = |tool: &str, arguments: Value, lang: &str| -> Value {
        let message = json!({
            "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": {"name": tool, "arguments": arguments}
        });
        let reply = op(
            &core,
            json!({"op": "mcp", "message": message, "headers": headers(&core), "lang": lang}),
        );
        let text = reply["result"]["body"]["result"]["content"][0]["text"]
            .as_str()
            .unwrap_or_else(|| panic!("{reply}"));
        serde_json::from_str(text).unwrap()
    };
    let listed = asked(
        "inspection_list",
        json!({"project": PROJECT, "value": {"show": "all"}}),
        "en",
    );
    assert_eq!(
        listed["groups"]["freeCards"]["label"],
        "Cards in free sections"
    );
    let free = listed["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["group"] == "freeCards")
        .unwrap()["id"]
        .clone();
    let read = asked(
        "inspection_read",
        json!({"project": PROJECT, "target": [free.clone()]}),
        "en",
    );
    assert_eq!(read["items"][0]["label"], "Cards in free sections");
    assert_eq!(
        read["items"][0]["fix"],
        "Once their place is decided, move them to a section that fits their kind"
    );
    // The item is the same one whatever the language: its id does not hang on the words.
    let japanese = asked(
        "inspection_list",
        json!({"project": PROJECT, "value": {"show": "all"}}),
        "ja",
    );
    let ids = |listed: &Value| -> Vec<Value> {
        listed["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["id"].clone())
            .collect()
    };
    assert_eq!(ids(&listed), ids(&japanese));
    assert_eq!(
        japanese["groups"]["freeCards"]["label"],
        "フリーセクションのカード"
    );
}

/// What is found in a card done — a goal finished or cancelled, a task done, a card inside one — is
/// kept as a record and not worked on again: it is said as information, in the group it is of, and the
/// counts follow. What is found in a card still to do is to fix as before.
#[test]
fn what_a_card_done_holds_is_information() {
    let core = core();
    let board = |value: Value| json!({"branch": "_project_board_", "value": value});
    ok(
        &core,
        "card_create",
        board(json!({
            "Goals:Open": {"kind": "goal", "status": "inProgress", "body": "[[Nope:Open]]"},
            "Goals:Open.Doing": {"kind": "task", "body": "[[Nope:Doing]]"},
            "Goals:Open.Did": {"kind": "task", "done": true, "body": "[[Nope:Did]]"},
            "Goals:Finished": {"kind": "goal", "status": "done", "body": "[[Nope:Finished]]"},
            "Goals:Finished.Left": {"kind": "task", "body": "[[Nope:Left]]"},
            "Goals:Dropped": {"kind": "goal", "status": "cancelled", "body": "[[Nope:Dropped]]"},
            "Goals:FinishedToo": {"kind": "goal", "body": "[[Nope:Too]]"}
        })),
    );
    let tree = "_project_board_";
    let ours = |answer: &Value| -> Vec<(String, String)> {
        answer["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|i| i["group"] == "brokenPaths")
            .map(|i| {
                let kind = i.get("kind").and_then(Value::as_str).unwrap_or("fix");
                (i["at"].as_str().unwrap().to_string(), kind.to_string())
            })
            .filter(|(at, _)| at.starts_with("Goals:") && at != "Goals:Ship")
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect()
    };
    let all = listed_in(&core, tree, json!({"show": "all"}));
    assert_eq!(
        ours(&all),
        [
            ("Goals:Dropped", "info"),
            ("Goals:Finished", "info"),
            ("Goals:Finished.Left", "info"),
            ("Goals:FinishedToo", "fix"),
            ("Goals:Open", "fix"),
            ("Goals:Open.Did", "info"),
            ("Goals:Open.Doing", "fix"),
        ]
        .map(|(at, kind)| (at.to_string(), kind.to_string())),
        "{all}"
    );
    // Shown as the kind each is: what to fix by default, the records with the information.
    let fix = listed_in(&core, tree, json!({}));
    assert!(ours(&fix).iter().all(|(_, kind)| kind == "fix"), "{fix}");
    assert_eq!(ours(&fix).len(), 3, "{fix}");
    let info = listed_in(&core, tree, json!({"show": "info"}));
    assert!(ours(&info).iter().all(|(_, kind)| kind == "info"), "{info}");
    assert_eq!(ours(&info).len(), 4, "{info}");
    // The group stays the one it is of, and says how many of its items are information.
    let group = &all["groups"]["brokenPaths"];
    assert_eq!(group["kind"], "fix", "{group}");
    assert_eq!(group["info"], 4, "{group}");
    let counted = |answer: &Value, kind: &str| {
        answer["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|i| i.get("accepted").is_none())
            .filter(|i| {
                let group = all["groups"][i["group"].as_str().unwrap()]["kind"]
                    .as_str()
                    .unwrap();
                i.get("kind").and_then(Value::as_str).unwrap_or(group) == kind
            })
            .count()
    };
    assert_eq!(all["counts"]["fix"], counted(&all, "fix"), "{all}");
    assert_eq!(all["counts"]["info"], counted(&all, "info"), "{all}");
    let total = all["groups"]
        .as_object()
        .unwrap()
        .values()
        .map(|g| g["count"].as_u64().unwrap())
        .sum::<u64>();
    assert_eq!(
        total,
        all["counts"]["fix"].as_u64().unwrap() + all["counts"]["info"].as_u64().unwrap()
    );
    // Read whole, an item says its own kind.
    let id_at = |at: &str| {
        all["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["group"] == "brokenPaths" && i["at"] == at)
            .unwrap()["id"]
            .clone()
    };
    let read = ok(
        &core,
        "inspection_read",
        json!({"branch": tree, "target": [id_at("Goals:Finished.Left"), id_at("Goals:Open.Doing")]}),
    );
    assert_eq!(read["items"][0]["kind"], "info", "{read}");
    assert_eq!(read["items"][1]["kind"], "fix", "{read}");
    // A record marked as designed is so as any item is, and leaves the information.
    let before = all["counts"]["info"].as_u64().unwrap();
    ok(
        &core,
        "inspection_update",
        json!({"branch": tree, "target": id_at("Goals:Dropped"), "value": {"accept": "打ち切りの記録"}}),
    );
    let after = listed_in(&core, tree, json!({"show": "all"}));
    assert_eq!(after["counts"]["info"], before - 1, "{after}");
    assert_eq!(after["groups"]["brokenPaths"]["info"], 3, "{after}");
    assert_eq!(after["groups"]["brokenPaths"]["accepted"], 1, "{after}");
    // Opened again, the goal's items are to fix once more.
    ok(
        &core,
        "card_update",
        json!({"branch": tree, "target": "Goals:Finished", "value": {"status": "inProgress"}}),
    );
    let reopened = listed_in(&core, tree, json!({"show": "all"}));
    let kinds: Vec<(String, String)> = ours(&reopened)
        .into_iter()
        .filter(|(at, _)| at.starts_with("Goals:Finished."))
        .collect();
    assert_eq!(
        kinds,
        [("Goals:Finished.Left".to_string(), "fix".to_string())],
        "{reopened}"
    );
}
