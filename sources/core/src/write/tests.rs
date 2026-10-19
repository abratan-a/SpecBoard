use super::*;
use crate::store::Store;
use serde_json::json;

const PROJECT: &str = "p1";

/// A database of the core's form, made as a new one is (`Store::create`), with one project and
/// nothing in it.
fn fresh() -> Store {
    let path = crate::fixtures::place("write");
    Store::create(&path).unwrap();
    let conn = Connection::open(&path).unwrap();
    conn.execute(
        "INSERT INTO projects (id, name, slug) VALUES (?, 'Project', 'project')",
        [PROJECT],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO branches (id, project_id, name) VALUES ('b1', ?, 'work')",
        [PROJECT],
    )
    .unwrap();
    drop(conn);
    Store::open(&path).unwrap()
}

fn card(id: &str, name: &str, label: &str) -> Map<String, Value> {
    json!({ "id": id, "kind": "entity", "name": name, "section": "s1", "v": 1, "fields": { "label": label } })
        .as_object()
        .unwrap()
        .clone()
}

fn put(store: &mut Store, who: Who, tree: &str, doc: &Map<String, Value>) -> Result<(), String> {
    let mut tx = store.write(who)?;
    tx.put_card(PROJECT, tree, doc)?;
    tx.line(
        PROJECT,
        tree,
        "schema.update",
        "schema",
        doc["id"].as_str(),
        keyed!("create.line.created", full = "Docs:A"),
    )?;
    tx.commit().map(|_| ())
}

/// A row as kept: removed, its document, its base, when the base was taken, its name.
type Kept = (i64, Option<String>, Option<String>, Option<String>, String);

fn row(store: &Store, tree: &str, id: &str) -> Option<Kept> {
    store
        .conn
        .query_row(
            "SELECT removed, doc, base, based_at, name FROM card_docs WHERE project_id = ? AND branch_id = ? AND id = ?",
            params![PROJECT, tree, id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .optional()
        .unwrap()
}

fn label(text: Option<String>) -> Value {
    serde_json::from_str::<Value>(&text.unwrap()).unwrap()["fields"]["label"].clone()
}

/// The branch `b1` as made now: it sees main as it is (what it kept of main as it was let go of;
/// a branch sees main as it was made).
fn made_now(store: &Store) {
    store
        .conn
        .execute("DELETE FROM branch_frozen WHERE branch_id = 'b1'", [])
        .unwrap();
}

fn count(store: &Store, query: &str) -> i64 {
    store.conn.query_row(query, [], |r| r.get(0)).unwrap()
}

#[test]
fn a_write_to_main_is_a_commit_with_what_it_was() {
    let mut store = fresh();
    put(&mut store, Who::screen(), "", &card("c1", "Order", "注文")).unwrap();
    let (removed, doc, base, _, name) = row(&store, "", "c1").unwrap();
    assert_eq!((removed, name, base), (0, "Order".to_string(), None));
    assert_eq!(label(doc), "注文");
    let commits = history::commits(&store.conn, PROJECT, "", 10).unwrap();
    assert_eq!(commits.len(), 1);
    assert_eq!(
        (
            commits[0].seq,
            commits[0].kind.as_str(),
            commits[0].author_kind.as_str()
        ),
        (1, "edit", "screen")
    );
    assert_eq!(commits[0].lines[0].summary, "カード Docs:A を作成");
    let before: Option<String> = store
        .conn
        .query_row("SELECT before FROM commit_rows", [], |r| r.get(0))
        .unwrap();
    assert_eq!(before, None, "it was not there before");

    // The same document again changes nothing: no row, no commit.
    let mut tx = store.write(Who::mcp(Some("key"))).unwrap();
    tx.put_card(PROJECT, "", &card("c1", "Order", "注文"))
        .unwrap();
    tx.commit().unwrap();
    assert_eq!(count(&store, "SELECT count(*) FROM commits"), 1);
}

#[test]
fn a_branch_writes_a_copy_with_mains_as_its_base() {
    let mut store = fresh();
    put(&mut store, Who::screen(), "", &card("c1", "Order", "注文")).unwrap();
    made_now(&store);
    put(
        &mut store,
        Who::screen(),
        "b1",
        &card("c1", "Order", "受注"),
    )
    .unwrap();
    let (_, doc, base, based_at, _) = row(&store, "b1", "c1").unwrap();
    assert_eq!(label(doc), "受注");
    assert_eq!(label(base.clone()), "注文", "main's as it was");
    let first = based_at.unwrap();
    // Written again, it keeps its base and when it was taken.
    put(
        &mut store,
        Who::screen(),
        "b1",
        &card("c1", "Order", "受注2"),
    )
    .unwrap();
    let (_, _, again, at, _) = row(&store, "b1", "c1").unwrap();
    assert_eq!((again, at), (base, Some(first)));
    assert_eq!(
        label(row(&store, "", "c1").unwrap().1),
        "注文",
        "main stays"
    );
    assert_eq!(
        count(
            &store,
            "SELECT count(*) FROM commits WHERE branch_id = 'b1'"
        ),
        2
    );
}

#[test]
fn a_branch_marks_what_it_removed_and_lets_go_of_what_it_made() {
    let mut store = fresh();
    put(&mut store, Who::screen(), "", &card("c1", "Order", "注文")).unwrap();
    made_now(&store);
    put(&mut store, Who::screen(), "b1", &card("c2", "Item", "明細")).unwrap();
    let mut tx = store.write(Who::screen()).unwrap();
    tx.place(
        PROJECT,
        "b1",
        "",
        "card",
        "c1",
        Some(&Spot {
            x: Some(3),
            y: Some(4),
            ..Spot::default()
        }),
    )
    .unwrap();
    assert!(tx.remove_card(PROJECT, "b1", "c1").unwrap());
    assert!(tx.remove_card(PROJECT, "b1", "c2").unwrap());
    assert!(!tx.remove_card(PROJECT, "b1", "c3").unwrap(), "never there");
    tx.commit().unwrap();
    let (removed, doc, base, _, name) = row(&store, "b1", "c1").unwrap();
    assert_eq!((removed, doc, name), (1, None, "Order".to_string()));
    assert_eq!(label(base), "注文");
    assert!(
        row(&store, "b1", "c2").is_none(),
        "what the branch made goes"
    );
    assert!(row(&store, "", "c1").is_some(), "main stays");
    assert_eq!(
        count(&store, "SELECT count(*) FROM places"),
        0,
        "where it sat goes too"
    );
}

#[test]
fn going_back_puts_the_rows_back_as_a_commit_of_its_own() {
    let mut store = fresh();
    put(&mut store, Who::screen(), "", &card("c1", "Order", "一")).unwrap();
    put(
        &mut store,
        Who::mcp(Some("key")),
        "",
        &card("c1", "Order", "二"),
    )
    .unwrap();
    put(
        &mut store,
        Who::mcp(Some("key")),
        "",
        &card("c2", "Item", "三"),
    )
    .unwrap();

    // MCP may undo only its own: commit 1 is the window's.
    let mut tx = store.write(Who::mcp(Some("key"))).unwrap();
    let refused = tx
        .revert(
            PROJECT,
            "",
            0,
            Some(&history::Author::of(&Who::mcp(Some("key")))),
        )
        .unwrap_err();
    assert!(refused.contains("コミット 0 はありません"), "{refused}");
    let refused = tx
        .revert(
            PROJECT,
            "",
            1,
            Some(&history::Author::of(&Who::mcp(Some("other")))),
        )
        .unwrap_err();
    assert_eq!(
        refused,
        "ほかの書き手のコミットを含むので戻しません: 2（MCP・key）、3（MCP・key）。ほかの書き手のコミットは、branch_revert の undo で 1 つずつ、request: true を付けて取り消せます"
    );
    assert_eq!(tx.revert(PROJECT, "", 1, None).unwrap(), 2);
    tx.commit().unwrap();
    assert_eq!(label(row(&store, "", "c1").unwrap().1), "一");
    assert!(row(&store, "", "c2").is_none());
    let commits = history::commits(&store.conn, PROJECT, "", 10).unwrap();
    assert_eq!(commits[0].kind, "revert");
    assert_eq!(
        commits[0].lines[0].summary,
        "戻す（2 件。コミット 1 のすぐあとへ）"
    );
    assert_eq!(
        commits.iter().map(|c| c.reverted).collect::<Vec<_>>(),
        [false, true, true, false]
    );

    // The going back is undone in turn.
    let mut tx = store.write(Who::screen()).unwrap();
    tx.revert(PROJECT, "", 3, None).unwrap();
    tx.commit().unwrap();
    assert_eq!(label(row(&store, "", "c1").unwrap().1), "二");
    assert!(row(&store, "", "c2").is_some());

    // Cut before 3: the first two go.
    let mut tx = store.write(Who::screen()).unwrap();
    assert_eq!(tx.cut(PROJECT, "", 3).unwrap(), 2);
    tx.commit().unwrap();
    assert_eq!(count(&store, "SELECT min(seq) FROM commits"), 3);
}

#[test]
fn a_locked_main_changes_only_by_a_merge_but_where_things_sit() {
    let mut store = fresh();
    put(&mut store, Who::screen(), "", &card("c1", "Order", "注文")).unwrap();
    let root = json!({ "id": "r", "name": "Main" })
        .as_object()
        .unwrap()
        .clone();
    let mut tx = store.write(Who::screen()).unwrap();
    tx.put_section(PROJECT, "", &root).unwrap();
    tx.set_main_lock(PROJECT, true).unwrap();
    tx.commit().unwrap();

    assert_eq!(
        put(&mut store, Who::screen(), "", &card("c1", "Order", "受注")).unwrap_err(),
        locked_main()
    );
    let mut tx = store.write(Who::screen()).unwrap();
    assert_eq!(
        tx.remove_card(PROJECT, "", "c1").unwrap_err(),
        locked_main()
    );
    assert!(
        tx.revert(PROJECT, "", 1, None)
            .unwrap_err()
            .starts_with("main はロック中なので戻せません")
    );
    // Put right in one go in a branch of the app's own (`~group`), and given up.
    let group = tx
        .begin_group(PROJECT, "")
        .unwrap()
        .expect("a branch of its own");
    assert_eq!(
        tx.group_branch(PROJECT).unwrap().as_deref(),
        Some(group.as_str())
    );
    assert_eq!(tx.abandon_group(PROJECT, "").unwrap(), 0);
    assert_eq!(tx.group_branch(PROJECT).unwrap(), None);
    // Where it sits and how it looks pass.
    tx.place(
        PROJECT,
        "",
        "",
        "card",
        "c1",
        Some(&Spot {
            x: Some(1),
            y: Some(2),
            ..Spot::default()
        }),
    )
    .unwrap();
    tx.look(PROJECT, "", "c1", "", "view", Some(&json!("document")))
        .unwrap();
    // The root's name does not, nor anything else of a section.
    let renamed = json!({ "id": "r", "name": "Root" })
        .as_object()
        .unwrap()
        .clone();
    assert_eq!(
        tx.put_section(PROJECT, "", &renamed).unwrap_err(),
        locked_main()
    );
    let described = json!({ "id": "r", "name": "Main", "description": "x" })
        .as_object()
        .unwrap()
        .clone();
    assert_eq!(
        tx.put_section(PROJECT, "", &described).unwrap_err(),
        locked_main()
    );
    // A branch is free, and a merge writes main.
    tx.put_card(PROJECT, "b1", &card("c1", "Order", "受注"))
        .unwrap();
    tx.merging();
    tx.put_card(PROJECT, "", &card("c1", "Order", "受注"))
        .unwrap();
    tx.commit().unwrap();
    assert_eq!(label(row(&store, "", "c1").unwrap().1), "受注");

    let mut tx = store.write(Who::screen()).unwrap();
    tx.set_main_lock(PROJECT, false).unwrap();
    tx.commit().unwrap();
    put(&mut store, Who::screen(), "", &card("c1", "Order", "注文")).unwrap();
    let lines: Vec<String> = history::commits(&store.conn, PROJECT, "", 10)
        .unwrap()
        .into_iter()
        .flat_map(|c| c.lines.into_iter().map(|l| l.summary))
        .collect();
    assert!(lines.contains(&"main をロックした".to_string()));
    assert!(lines.contains(&"main のロックを外した".to_string()));
}

/// `card("c1", …)` with its place among the cards and its tags.
fn arranged(label: &str, order: Option<i64>, tags: &[&str]) -> Map<String, Value> {
    let mut doc = card("c1", "Order", label);
    if let Some(n) = order {
        doc.insert("order".into(), n.into());
    }
    if !tags.is_empty() {
        doc["fields"]["tags"] = json!(tags);
    }
    doc
}

#[test]
fn a_locked_main_lets_a_card_be_reordered_and_tagged_and_nothing_else_of_it() {
    let mut store = fresh();
    put(&mut store, Who::screen(), "", &arranged("注文", None, &[])).unwrap();
    let mut tx = store.write(Who::screen()).unwrap();
    tx.set_main_lock(PROJECT, true).unwrap();
    tx.commit().unwrap();

    for who in [Who::screen(), Who::mcp(Some("key"))] {
        let before = count(&store, "SELECT count(*) FROM commits WHERE branch_id = ''");
        // Its place among the cards, then its tags, then both: each a commit of main's.
        put(&mut store, who.clone(), "", &arranged("注文", Some(3), &[])).unwrap();
        put(
            &mut store,
            who.clone(),
            "",
            &arranged("注文", Some(3), &["a"]),
        )
        .unwrap();
        put(&mut store, who.clone(), "", &arranged("注文", None, &[])).unwrap();
        assert_eq!(
            count(&store, "SELECT count(*) FROM commits WHERE branch_id = ''"),
            before + 3
        );
        // Anything else of it, along with them or alone, is refused.
        assert_eq!(
            put(
                &mut store,
                who.clone(),
                "",
                &arranged("受注", Some(4), &["a"])
            )
            .unwrap_err(),
            locked_main()
        );
        let mut moved = arranged("注文", None, &["a"]);
        moved["section"] = "s2".into();
        assert_eq!(
            put(&mut store, who.clone(), "", &moved).unwrap_err(),
            locked_main()
        );
        // A card made, or let go, is refused too.
        assert_eq!(
            put(&mut store, who.clone(), "", &card("c2", "Item", "明細")).unwrap_err(),
            locked_main()
        );
    }
    let kept: Value = serde_json::from_str(&row(&store, "", "c1").unwrap().1.unwrap()).unwrap();
    assert_eq!(kept, Value::Object(arranged("注文", None, &[])));
}

#[test]
fn a_locked_main_lets_what_sits_on_a_sections_diagram_for_another_move_too() {
    let mut store = fresh();
    put(&mut store, Who::screen(), "", &card("c1", "Order", "注文")).unwrap();
    let at = |x: i64| Spot {
        x: Some(x),
        y: Some(0),
        ..Spot::default()
    };
    let mut tx = store.write(Who::screen()).unwrap();
    tx.place(PROJECT, "", "s2", "card", "c1", Some(&at(1)))
        .unwrap();
    tx.place(PROJECT, "", "s2", "section", "s1", Some(&at(1)))
        .unwrap();
    tx.set_main_lock(PROJECT, true).unwrap();
    tx.commit().unwrap();
    // Where a box sits, as the store has it: (x, y, w, h).
    type Box4 = (Option<i64>, Option<i64>, Option<i64>, Option<i64>);
    let spot = |conn: &Connection,
                tree: &str,
                view: &str,
                kind: &str,
                item: &str|
     -> Option<Box4> {
        conn.query_row(
            "SELECT x, y, w, h FROM places WHERE project_id = ? AND branch_id = ? AND view = ? AND item_kind = ? AND item = ?",
            params![PROJECT, tree, view, kind, item],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()
        .unwrap()
    };
    let main_commits = "SELECT count(*) FROM commits WHERE branch_id = ''";
    let commits_before = count(&store, main_commits);

    for who in [Who::screen(), Who::mcp(None)] {
        let mut tx = store.write(who).unwrap();
        // A card's own box, its size, and a flow's nodes move.
        tx.place(PROJECT, "", "", "card", "c1", Some(&at(5)))
            .unwrap();
        let sized = Spot {
            w: Some(12),
            h: Some(6),
            ..at(5)
        };
        tx.place(PROJECT, "", "", "card", "c1", Some(&sized))
            .unwrap();
        tx.place(PROJECT, "", "c9", "node", "start", Some(&at(2)))
            .unwrap();
        tx.place(PROJECT, "", "c9", "node", "start", None).unwrap();
        // Another section's card on this one's diagram, and the card that sums one up, move
        // too: moved, sized, let go or put there new.
        for (kind, item) in [("card", "c1"), ("section", "s1")] {
            tx.place(PROJECT, "", "s2", kind, item, Some(&at(7)))
                .unwrap();
            let sized = Spot {
                w: Some(12),
                h: Some(6),
                ..at(1)
            };
            tx.place(PROJECT, "", "s2", kind, item, Some(&sized))
                .unwrap();
            tx.place(PROJECT, "", "s2", kind, item, None).unwrap();
            tx.place(PROJECT, "", "s3", kind, item, Some(&at(1)))
                .unwrap();
            assert_eq!(spot(&tx.tx, "", "s2", kind, item), None, "{kind}");
            assert_eq!(
                spot(&tx.tx, "", "s3", kind, item),
                Some((Some(1), Some(0), None, None)),
                "{kind}"
            );
        }
        assert_eq!(
            spot(&tx.tx, "", "", "card", "c1"),
            Some((Some(5), Some(0), Some(12), Some(6)))
        );
        assert_eq!(spot(&tx.tx, "", "c9", "node", "start"), None);
        // A branch is free, and so is a merge.
        tx.place(PROJECT, "b1", "s2", "card", "c1", Some(&at(8)))
            .unwrap();
        tx.merging();
        tx.place(PROJECT, "", "s2", "section", "s1", Some(&at(9)))
            .unwrap();
        assert_eq!(
            spot(&tx.tx, "b1", "s2", "card", "c1"),
            Some((Some(8), Some(0), None, None))
        );
        assert_eq!(
            spot(&tx.tx, "", "s2", "section", "s1"),
            Some((Some(9), Some(0), None, None))
        );
        // Undone: the next writer finds them where they were.
        drop(tx);
        for (kind, item) in [("card", "c1"), ("section", "s1")] {
            assert_eq!(
                spot(&store.conn, "", "s2", kind, item),
                Some((Some(1), Some(0), None, None)),
                "{kind}"
            );
            assert_eq!(spot(&store.conn, "", "s3", kind, item), None, "{kind}");
        }
        assert_eq!(spot(&store.conn, "", "", "card", "c1"), None);
    }
    // Boxes moved make no commit in main.
    assert_eq!(count(&store, main_commits), commits_before);
}

#[test]
fn the_window_putting_a_tree_right_holds_it_and_makes_one_commit() {
    let mut store = fresh();
    put(&mut store, Who::screen(), "", &card("c1", "Order", "一")).unwrap();
    let mut tx = store.write(Who::screen()).unwrap();
    tx.begin_group(PROJECT, "").unwrap();
    tx.commit().unwrap();
    put(&mut store, Who::screen(), "", &card("c1", "Order", "二")).unwrap();
    put(&mut store, Who::screen(), "", &card("c2", "Item", "三")).unwrap();
    let refused = put(&mut store, Who::mcp(None), "", &card("c1", "Order", "四")).unwrap_err();
    assert!(
        refused.starts_with("アプリバッチ処理中です（main、"),
        "{refused}"
    );
    // Elsewhere MCP writes on.
    put(&mut store, Who::mcp(None), "b1", &card("c1", "Order", "四")).unwrap();
    let commits = history::commits(&store.conn, PROJECT, "", 10).unwrap();
    assert_eq!(commits.len(), 2, "the two writes are one commit");
    assert_eq!(
        (commits[0].kind.as_str(), commits[0].lines.len()),
        ("batch", 2)
    );

    // Given up: back to where it began.
    let mut tx = store.write(Who::screen()).unwrap();
    assert_eq!(tx.abandon_group(PROJECT, "").unwrap(), 1);
    tx.commit().unwrap();
    assert_eq!(label(row(&store, "", "c1").unwrap().1), "一");
    assert!(row(&store, "", "c2").is_none());
    assert_eq!(count(&store, "SELECT count(*) FROM holds"), 0);
    put(&mut store, Who::mcp(None), "", &card("c1", "Order", "五")).unwrap();
}

#[test]
fn an_mcp_batch_holds_its_tree_until_it_goes() {
    let mut store = fresh();
    let mut tx = store.write(Who::mcp(Some("key"))).unwrap();
    let token = tx.open_batch(PROJECT, "").unwrap();
    let steps = vec![(
        "card_update".to_string(),
        json!({ "project": "Project", "target": "Core:Order" })
            .as_object()
            .unwrap()
            .clone(),
    )];
    tx.save_batch(&token, &steps).unwrap();
    tx.commit().unwrap();
    assert!(token.starts_with("batch-"));
    let batch = holds::batch(&store.conn, &token).unwrap().unwrap();
    assert_eq!(
        (batch.project.as_str(), batch.tree.as_str(), batch.steps),
        (PROJECT, "", steps)
    );

    let refused = put(&mut store, Who::screen(), "", &card("c1", "Order", "一")).unwrap_err();
    assert!(
        refused.starts_with("MCP バッチ処理中です（main、"),
        "{refused}"
    );
    let mut tx = store.write(Who::screen()).unwrap();
    assert!(
        tx.begin_group(PROJECT, "")
            .unwrap_err()
            .starts_with("MCP バッチ処理中です")
    );
    drop(tx);
    put(
        &mut store,
        Who::mcp(Some("key")),
        "",
        &card("c1", "Order", "一"),
    )
    .unwrap();

    let mut tx = store.write(Who::mcp(Some("key"))).unwrap();
    tx.close_batch(&token).unwrap();
    tx.commit().unwrap();
    put(&mut store, Who::screen(), "", &card("c1", "Order", "二")).unwrap();
    assert!(holds::batch(&store.conn, &token).unwrap().is_none());
}

#[test]
fn an_archived_project_refuses_every_write() {
    let mut store = fresh();
    put(&mut store, Who::screen(), "", &card("c1", "Order", "一")).unwrap();
    store
        .conn
        .execute(
            "UPDATE projects SET archived_at = 'x' WHERE id = ?",
            [PROJECT],
        )
        .unwrap();
    assert_eq!(
        put(&mut store, Who::screen(), "b1", &card("c1", "Order", "二")).unwrap_err(),
        archived()
    );
    let mut tx = store.write(Who::screen()).unwrap();
    assert_eq!(
        tx.place(PROJECT, "", "", "card", "c1", Some(&Spot::default()))
            .unwrap_err(),
        archived()
    );
    assert!(
        tx.set_main_lock(PROJECT, true)
            .unwrap_err()
            .ends_with("はアーカイブです（読むだけ）")
    );
    drop(tx);
    // Out of the archive, it is written again.
    store
        .conn
        .execute(
            "UPDATE projects SET archived_at = NULL WHERE id = ?",
            [PROJECT],
        )
        .unwrap();
    put(&mut store, Who::screen(), "", &card("c1", "Order", "二")).unwrap();
}

#[test]
fn a_new_id_is_a_random_uuid() {
    let (a, b) = (new_id(), new_id());
    assert_ne!(a, b);
    assert_eq!(a.len(), 36);
    assert_eq!(&a[14..15], "4");
    assert!(
        a.chars()
            .all(|c| c == '-' || c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
    );
}

/// The sample project in a store of its own.
fn built() -> Store {
    Store::open(&crate::fixtures::sample()).unwrap()
}

fn built_project() -> String {
    crate::fixtures::project_id("sample")
}

#[test]
fn a_project_file_reads_back_as_it_was_written() {
    let built_project = built_project();
    let built_project = built_project.as_str();
    let mut store = built();
    let written = crate::projectfile::write(&store.conn, built_project).unwrap();
    // A file of before carries the change notes (`changelog`, a table nothing wrote, gone):
    // read, they are let go.
    let mut noted: serde_json::Value = serde_json::from_str(&written).unwrap();
    noted["changelog"] = serde_json::json!([{"id": "n1", "branch_id": "", "target_kind": "branch",
        "text": "返金を足した", "at": "now", "author_kind": "screen"}]);
    let mut tx = store.write(Who::screen()).unwrap();
    assert_eq!(
        tx.read_project_file(&noted.to_string(), &Landing::Replacing, false)
            .unwrap(),
        built_project
    );
    tx.commit().unwrap();
    assert_eq!(
        crate::projectfile::write(&store.conn, built_project).unwrap(),
        written,
        "the same file again"
    );
    assert!(!written.contains("changelog"));
    // The form named for SpecBoard, at 14: a card's lines are its
    // connections, the project's own kinds in its row, a flow's nodes cards inside it, a goal's
    // fields by their names in English, at 15 its merge request by `MR`, at 16 what is directly
    // under each root in its kinds (`project_tree`), at 17 a goal's working branch by
    // `workBranch`, at 18 the form named by the project, at 19 the project's exports in its row,
    // at 20 those exports holding no working directory, at 21 the check's marks.
    assert!(written.contains(r#""format": "specboard/project""#));
    assert!(written.contains(&format!(r#""version": {}"#, crate::projectfile::VERSION)));
    assert!(written.contains(r#""project_tree""#));
    assert!(written.contains(r#""in": "#));
    assert!(written.contains(r#""connections": ["#));
    assert!(written.contains(r#""kinds": {"#));
    // Each tree's history starts again, with the read.
    let commits = history::commits(&store.conn, built_project, "", 10).unwrap();
    assert_eq!(commits.len(), 1);
    assert_eq!(
        commits[0].lines[0].summary,
        format!(
            "プロジェクトのファイル（版 {}）から読み込んだ",
            crate::projectfile::VERSION
        )
    );
}

#[test]
fn a_copy_is_a_project_of_its_own_and_an_archive_copy_is_archived() {
    let built_project = built_project();
    let built_project = built_project.as_str();
    let mut store = built();
    let mut tx = store.write(Who::screen()).unwrap();
    let copy = tx.archive_copy(built_project).unwrap();
    tx.commit().unwrap();
    assert_ne!(copy, built_project);
    let (name, slug, archived): (String, String, Option<String>) = store
        .conn
        .query_row(
            "SELECT name, slug, archived_at FROM projects WHERE id = ?",
            [&copy],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert!(name.contains("（アーカイブ "), "{name}");
    assert!(archived.is_some());
    let original_slug: String = store
        .conn
        .query_row(
            "SELECT slug FROM projects WHERE id = ?",
            [built_project],
            |r| r.get(0),
        )
        .unwrap();
    assert_ne!(slug, original_slug);
    // Every id made anew: no card of the copy shares one with the original.
    let shared: i64 = store
        .conn
        .query_row(
            "SELECT count(*) FROM card_docs a JOIN card_docs b ON a.id = b.id WHERE a.project_id = ? AND b.project_id = ?",
            [built_project, &copy],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(shared, 0);
    let cards = |store: &Store, project: &str| -> i64 {
        store
            .conn
            .query_row(
                "SELECT count(*) FROM card_docs WHERE project_id = ? AND removed = 0",
                [project],
                |r| r.get(0),
            )
            .unwrap()
    };
    assert_eq!(cards(&store, &copy), cards(&store, built_project));
    // An archived project refuses; out of the archive, it is written; deleted, it is gone.
    let mut tx = store.write(Who::screen()).unwrap();
    assert_eq!(
        tx.set_main_lock(&copy, true).unwrap_err(),
        format!("プロジェクト「{name}」はアーカイブです（読むだけ）")
    );
    tx.restore_project(&copy).unwrap();
    tx.set_main_lock(&copy, true).unwrap();
    tx.delete_project(&copy).unwrap();
    tx.commit().unwrap();
    assert_eq!(cards(&store, &copy), 0);
}

#[test]
fn files_that_cannot_be_read_are_refused() {
    let mut store = fresh();
    let mut tx = store.write(Who::screen()).unwrap();
    let refuse = |tx: &mut Tx, text: &str| {
        tx.read_project_file(text, &Landing::Replacing, false)
            .unwrap_err()
    };
    assert_eq!(
        refuse(&mut tx, "x"),
        "プロジェクトのファイルとして読めません（JSON ではありません）"
    );
    assert_eq!(
        refuse(&mut tx, r#"{"format": "other"}"#),
        "プロジェクトのファイルではありません（format が specboard/project ではない）"
    );
    // The form at a version past this one's.
    assert!(
        refuse(
            &mut tx,
            &json!({"format": "specboard/project", "version": crate::projectfile::VERSION + 1})
                .to_string()
        )
        .starts_with("新しい版のアプリ")
    );
    // The name before a plan was called a project is read still (versions 11 to 17).
    assert!(
        refuse(
            &mut tx,
            &json!({"format": "specboard/plan", "version": 17}).to_string()
        )
        .starts_with("ファイルに project がありません")
    );
    // A format name the app does not know, and the versions before 11, are not read.
    assert!(
        refuse(&mut tx, r#"{"format": "other-tool/plan", "version": 10}"#)
            .starts_with("プロジェクトのファイルではありません")
    );
    assert!(
        refuse(&mut tx, r#"{"format": "specboard/project", "version": 10}"#)
            .starts_with("版 10 のファイルは読めません")
    );
    let wrong = json!({
        "format": "specboard/project", "version": 12,
        "project": { "id": "p9", "name": "P", "slug": "p" },
        "cards": [{ "id": "c", "kind": "entity", "name": "X", "section": "s", "fields": { "nope": 1 } }]
    });
    assert!(
        refuse(&mut tx, &wrong.to_string())
            .starts_with("ファイルのカードが種類の定義に合いません:\nX: ")
    );
    drop(tx);
}

#[test]
fn a_slug_is_the_name_lowercased_and_dashed() {
    assert_eq!(slugify("  Online Shop v2 "), "online-shop-v2");
    assert_eq!(slugify("注文 / 管理"), "注文-管理");
    assert_eq!(slugify("!!"), "project");
}

/// What a transaction worked out from its rows is not given back once the rows it saw are undone:
/// a step of a call that wrote, was
/// worked out again, then was refused, leaves its rows as they were before it — and the count of
/// rows changed (what `remembered_or_brought_up` goes by) does not go back with them.
#[test]
fn what_was_worked_out_from_a_step_undone_is_not_given_back() {
    let mut store = fresh();
    put(&mut store, Who::screen(), "", &card("c1", "Order", "前")).unwrap();
    let mut tx = store.write(Who::screen()).unwrap();
    let label_now = |tx: &Tx| -> String {
        let doc = tx.seen(Table::Cards, PROJECT, "", "c1").unwrap().unwrap();
        doc.doc().unwrap()["fields"]["label"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let worked = std::cell::Cell::new(0);
    let read = |tx: &Tx| -> String {
        tx.remembered_or_brought_up(
            "label",
            || {
                worked.set(worked.get() + 1);
                Ok(label_now(tx))
            },
            |_, _| false,
        )
        .unwrap()
        .as_ref()
        .clone()
    };
    assert_eq!(read(&tx), "前");
    let refused = tx
        .alone(|tx| -> Result<(), String> {
            tx.put_card(PROJECT, "", &card("c1", "Order", "後"))?;
            // Worked out again after the write, inside the step.
            assert_eq!(read(tx), "後");
            Err("断る".to_string())
        })
        .unwrap();
    assert!(refused.is_err());
    assert_eq!(label_now(&tx), "前");
    assert_eq!(read(&tx), "前", "undone rows were answered from before");
    assert_eq!(worked.get(), 3);
    drop(tx);
}

#[test]
fn a_name_held_twice_is_refused_in_words_not_the_databases() {
    let store = fresh();
    let conn = &store.conn;
    let twice = |sql: &str| {
        conn.execute(sql, []).unwrap();
        conn.execute(&sql.replace("'x1'", "'x2'"), []).unwrap_err()
    };
    for e in [
        twice(
            "INSERT INTO card_docs (project_id, id, section_id, name, kind) VALUES ('p1', 'x1', 's', 'A', 'entity')",
        ),
        twice(
            "INSERT INTO section_docs (project_id, id, parent_id, name) VALUES ('p1', 'x1', NULL, 'A')",
        ),
    ] {
        assert!(name_taken(&e), "{e}");
        let said = db(e);
        assert!(!said.contains("UNIQUE"), "{said}");
    }
}

/// The lines to files a write made or pointed elsewhere are what is recorded after it commits: a
/// line written again as it was is not, nor one a merge carries.
#[test]
fn the_lines_to_files_made_or_pointed_elsewhere_are_taken_once() {
    let mut store = fresh();
    let with = |to: &str, at: &str| {
        let mut doc = card("c1", "A", "a");
        doc.insert(
            "connections".into(),
            json!([
                {"id": "l1", "line": "implementedBy", "to": to, "at": at},
                {"id": "l2", "line": "link", "to": "c2"}
            ]),
        );
        doc
    };
    let taken = |store: &mut Store, doc: &Map<String, Value>, merging: bool| {
        let mut tx = store.write(Who::screen()).unwrap();
        if merging {
            tx.merging();
        }
        tx.put_card(PROJECT, "", doc).unwrap();
        let out = tx.take_code_lines();
        tx.commit().unwrap();
        out
    };
    let made = taken(&mut store, &with("file:d1/a.rs", "f"), false);
    assert_eq!(made.len(), 1);
    assert_eq!(made[0].line_id, "l1");
    assert_eq!(made[0].file, "file:d1/a.rs");
    assert_eq!(made[0].at.as_deref(), Some("f"));
    assert!(taken(&mut store, &with("file:d1/a.rs", "f"), false).is_empty());
    let mut relabelled = with("file:d1/a.rs", "f");
    relabelled["fields"]["label"] = "b".into();
    assert!(taken(&mut store, &relabelled, false).is_empty());
    let moved = taken(&mut store, &with("file:d1/a.rs", "g"), false);
    assert_eq!(moved[0].at.as_deref(), Some("g"));
    assert!(taken(&mut store, &with("file:d1/b.rs", "g"), true).is_empty());
}
