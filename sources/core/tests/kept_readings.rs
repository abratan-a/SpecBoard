//! What the core reads of a tree is kept between calls while the store stands as it was read
//! (`specboard://SpecBoard/Docs/Design:CoreApi`「読みを覚える」): a tree is worked out of its documents
//! once for the reads that follow one another, and again as soon as anything is written — by this
//! core, or by another holder of the same store — so that no read is answered from what a write has
//! since changed.

use crate::common;

use common::{ok, op, place, project_id, script_store};
use serde_json::{Value, json};
use specboard_core::Core;

/// How many readings of `kind` the core has worked out so far.
fn made(core: &Core, kind: &str) -> u64 {
    core.readings_made(kind)
}

fn label(core: &Core, branch: Option<&str>) -> Value {
    let mut arguments = json!({"target": "Schemas:Customer"});
    if let Some(branch) = branch {
        arguments["branch"] = branch.into();
    }
    ok(core, "card_read", arguments)["label"].clone()
}

fn detail(core: &Core, connection: &str, since: Option<&str>) -> Value {
    let mut request = json!({
        "op": "project_detail",
        "project": project_id(core),
        "connection": connection,
    });
    if let Some(since) = since {
        request["since"] = since.into();
    }
    let answer = op(core, request);
    assert!(answer.get("error").is_none(), "{answer}");
    answer["result"].clone()
}

/// The working directories' folders read for the first time, as the first call that lists the
/// sections does: what is kept of a tree is kept by the folders' reading too, so the counts below
/// start from one that stands.
fn folders_read(core: &Core) {
    ok(core, "section_list", json!({}));
}

#[test]
fn a_tree_is_worked_out_once_while_nothing_is_written() {
    let core = Core::open(script_store()).unwrap();
    folders_read(&core);
    let before = made(&core, "tree");
    // Reads that follow one another, of whatever tools: main is worked out for the first, and the
    // board (a card is read with what the board's tasks say of it) for the first that reads it.
    ok(&core, "card_read", json!({"target": "Schemas:Customer"}));
    ok(
        &core,
        "card_read",
        json!({"target": "Schemas/Ordering:Order"}),
    );
    ok(&core, "section_list", json!({}));
    ok(&core, "card_list", json!({"value": {"section": "Schemas"}}));
    ok(&core, "project_read", json!({}));
    assert_eq!(made(&core, "tree") - before, 2);

    // Written: the next read works each out again, and says what was written.
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"label": "お客さま"}}),
    );
    assert_eq!(label(&core, None), "お客さま");
    assert_eq!(label(&core, None), "お客さま");
    assert_eq!(made(&core, "tree") - before, 4);

    // A branch is a tree of its own: read beside main and the board, each is worked out once.
    ok(&core, "branch_create", json!({"target": "plan"}));
    ok(
        &core,
        "card_update",
        json!({"branch": "plan", "target": "Schemas:Customer", "value": {"label": "顧客（案）"}}),
    );
    let before = made(&core, "tree");
    for _ in 0..3 {
        assert_eq!(label(&core, Some("plan")), "顧客（案）");
        assert_eq!(label(&core, None), "お客さま");
    }
    assert_eq!(made(&core, "tree") - before, 3);
}

#[test]
fn what_another_holder_of_the_store_writes_is_read() {
    let store = script_store();
    let here = Core::open(&store).unwrap();
    let there = Core::open(&store).unwrap();
    assert_eq!(label(&here, None), label(&there, None));
    // Written through the other: this one's next read is of the store as it stands now.
    for n in 0..3 {
        let name = format!("顧客 {n}");
        ok(
            &there,
            "card_update",
            json!({"target": "Schemas:Customer", "value": {"label": name}}),
        );
        assert_eq!(label(&here, None), name.as_str());
        assert_eq!(label(&there, None), name.as_str());
    }
}

#[test]
fn reads_beside_writes_never_go_back() {
    const WRITES: u64 = 40;
    let core = Core::open(script_store()).unwrap();
    let said = |core: &Core| -> u64 {
        label(core, None)
            .as_str()
            .and_then(|l| l.strip_prefix("n"))
            .and_then(|n| n.parse().ok())
            .unwrap_or(0)
    };
    std::thread::scope(|threads| {
        let writer = threads.spawn(|| {
            for n in 1..=WRITES {
                ok(
                    &core,
                    "card_update",
                    json!({"target": "Schemas:Customer", "value": {"label": format!("n{n}")}}),
                );
            }
        });
        let readers: Vec<_> = (0..3)
            .map(|_| {
                threads.spawn(|| {
                    // What one reader sees only moves on: never a label older than one it has seen.
                    let mut last = 0;
                    let mut reads = 0;
                    while last < WRITES {
                        let now = said(&core);
                        assert!(now >= last, "read {now} after {last}");
                        last = now;
                        reads += 1;
                        assert!(reads < 200_000, "the last write was never read");
                    }
                })
            })
            .collect();
        writer.join().unwrap();
        for reader in readers {
            reader.join().unwrap();
        }
    });
    assert_eq!(said(&core), WRITES);
}

#[test]
fn the_windows_reading_is_answered_from_the_one_made_while_nothing_is_written() {
    let core = Core::open(script_store()).unwrap();
    folders_read(&core);
    let before = made(&core, "detail");
    let whole = detail(&core, "a", None);
    let digest = whole["digest"].as_str().unwrap().to_string();
    assert_eq!(made(&core, "detail") - before, 1);
    // Asked again with what it holds, by the same reader and by others: nothing is worked out.
    for _ in 0..3 {
        let same = detail(&core, "a", Some(&digest));
        assert_eq!(same["delta"], true, "{same}");
        assert_eq!(same["digest"], digest.as_str());
    }
    let other = detail(&core, "b", None);
    assert_eq!(other["digest"], digest.as_str());
    assert_eq!(other["schemas"], whole["schemas"]);
    assert_eq!(made(&core, "detail") - before, 1);
    // An MCP read of the same tree takes the tree the window's reading was made from: the board,
    // which a card is read beside, is all that is worked out for it.
    let trees = made(&core, "tree");
    ok(&core, "card_read", json!({"target": "Schemas:Customer"}));
    ok(&core, "card_list", json!({"value": {"section": "Schemas"}}));
    assert_eq!(made(&core, "tree") - trees, 1);

    // Written: worked out once for all the readers, each told what changed since its own.
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"label": "お客さま"}}),
    );
    let changed = detail(&core, "a", Some(&digest));
    assert_eq!(changed["delta"], true, "{changed}");
    assert_ne!(changed["digest"], digest.as_str());
    let customer = changed["changed"]["schemas"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "Customer")
        .unwrap_or_else(|| panic!("{changed}"));
    assert_eq!(customer["label"], "お客さま");
    let also = detail(&core, "b", Some(&digest));
    assert_eq!(also["digest"], changed["digest"]);
    assert_eq!(made(&core, "detail") - before, 2);
}

#[test]
fn the_branches_are_listed_from_trees_worked_out_once() {
    let core = Core::open(script_store()).unwrap();
    folders_read(&core);
    for name in ["one", "two", "three"] {
        ok(&core, "branch_create", json!({"target": name}));
        ok(
            &core,
            "card_update",
            json!({"branch": name, "target": "Schemas:Customer", "value": {"label": name}}),
        );
    }
    let before = made(&core, "tree");
    let listed = ok(&core, "branch_list", json!({}));
    let names: Vec<&str> = listed["branches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["branch"].as_str().unwrap())
        .collect();
    for name in ["one", "two", "three"] {
        assert!(names.contains(&name), "{listed}");
    }
    // Main once for them all, and each branch once — however many there are to hold against it.
    let branches = names.len() as u64;
    assert_eq!(made(&core, "tree") - before, branches + 1, "{listed}");
    // Listed again: nothing more, nor is the list itself made again.
    let lists = made(&core, "branches");
    assert_eq!(ok(&core, "branch_list", json!({})), listed);
    assert_eq!(made(&core, "tree") - before, branches + 1);
    assert_eq!(made(&core, "branches"), lists);
    // Each branch was read in passing — kept, they would take the room of the trees read again:
    // one then read is worked out for itself (and the board beside it), once.
    assert_eq!(label(&core, Some("two")), "two");
    assert_eq!(label(&core, Some("two")), "two");
    assert_eq!(made(&core, "tree") - before, branches + 3);
}

#[test]
fn what_a_folder_holds_is_read_anew_though_the_store_is_not_written() {
    let core = Core::open(script_store()).unwrap();
    let said = op(
        &core,
        json!({"op": "mcp_writes_set", "project": project_id(&core), "group": "section", "state": "allow"}),
    );
    assert!(said.get("error").is_none(), "{said}");
    let folder = place("kept-folder");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("a.md"), "# a\n").unwrap();
    let bound = op(
        &core,
        json!({"op": "tool", "name": "section_update", "arguments": {
            "project": common::PROJECT, "target": "docs_2", "value": {"folder": folder.to_str().unwrap()}}}),
    );
    assert!(bound.get("error").is_none(), "{bound}");
    let files = |core: &Core| -> Vec<String> {
        ok(core, "card_list", json!({"value": {"section": "docs_2"}}))["cards"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["path"].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(files(&core), ["docs_2:'a.md'"]);
    // A file put there by something else and the folder read again: none of the store's documents
    // is written, and the reads that follow have the file all the same — what is kept of a tree
    // is of one reading of the folders, and not taken for the next.
    ok(&core, "card_read", json!({"target": "Schemas:Customer"}));
    std::fs::write(folder.join("b.md"), "# b\n").unwrap();
    ok(
        &core,
        "section_action",
        json!({"target": "docs_2", "op": "read"}),
    );
    assert_eq!(files(&core), ["docs_2:'a.md'", "docs_2:'b.md'"]);
    let section = ok(&core, "section_read", json!({"target": "docs_2"}));
    assert_eq!(section["files"], 2, "{section}");
}

#[test]
fn a_projects_size_is_counted_once_while_nothing_is_written() {
    let core = Core::open(script_store()).unwrap();
    folders_read(&core);
    let before = made(&core, "sizes");
    let first = ok(&core, "project_read", json!({}))["size"].clone();
    assert!(first["history"].as_i64().unwrap() >= 0, "{first}");
    for _ in 0..3 {
        assert_eq!(ok(&core, "project_read", json!({}))["size"], first);
    }
    assert_eq!(made(&core, "sizes") - before, 1);

    // Written: counted again, and what the write added to the history is in it.
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"label": "お客さま"}}),
    );
    let second = ok(&core, "project_read", json!({}))["size"].clone();
    assert!(
        second["history"].as_i64().unwrap() > first["history"].as_i64().unwrap(),
        "{first} → {second}"
    );
    assert_eq!(ok(&core, "project_read", json!({}))["size"], second);
    assert_eq!(made(&core, "sizes") - before, 2);
}
