//! Merge requests (MR): a branch merged into a locked main by MCP
//! (`branch_merge`) is not merged but asked for, the merge tried; read back, made stale when
//! their branch moves or main moves under what they bring, approved or rejected on screen,
//! and withdrawn by whoever made them.

use crate::common;

use common::{ok, refused, script_store};
use serde_json::{Value, json};
use specboard_core::Core;

const PROJECT: &str = "見本";

/// The script's project with a branch `work` that changed a card main changed too.
fn setup() -> (Core, String) {
    let store = script_store();
    let project: String = rusqlite::Connection::open(&store)
        .unwrap()
        .query_row("SELECT id FROM projects WHERE name = ?", [PROJECT], |r| {
            r.get(0)
        })
        .unwrap();
    let core = Core::open(&store).unwrap();
    ok(&core, "branch_create", json!({"target": "work"}));
    ok(
        &core,
        "card_update",
        json!({"branch": "work", "target": "Schemas:Customer", "value": {"label": "branch の客", "description": "branch で書いた"}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"label": "main の客"}}),
    );
    (core, project)
}

fn op(core: &Core, request: Value) -> Value {
    serde_json::from_str(&core.call(&request.to_string())).unwrap()
}

/// main locked by the window: a merge into it is a merge request now.
fn lock(core: &Core, project: &str) {
    let locked = op(
        core,
        json!({"op": "main_lock", "project": project, "on": true}),
    );
    assert!(locked.get("error").is_none(), "{locked}");
}

#[test]
fn a_request_is_made_with_its_conflicts_chosen_and_read_back() {
    let (core, project) = setup();
    lock(&core, &project);
    let said = refused(
        &core,
        "branch_merge",
        json!({"target": "work", "value": {"title": "客を直す"}}),
    );
    assert!(said.contains("衝突が 1 残っているので出しません"), "{said}");
    let made = ok(
        &core,
        "branch_merge",
        json!({"target": "work", "value": {
        "title": "客を直す", "description": "表示名と説明", "take": {"Schemas:Customer": {"label": "source"}}}}),
    );
    assert_eq!(made["mr"], 1);
    assert_eq!(made["state"], "open");
    assert_eq!(made["from"], "work");
    assert_eq!(made["result"]["change"], json!(["Schemas:Customer"]));
    assert_eq!(made["result"]["conflicts"][0]["unit"], "label");
    assert_eq!(made["result"]["conflicts"][0]["took"], "source");
    assert_eq!(made["result"]["conflicts"][0]["source"], "branch の客");
    let read = ok(&core, "mr_read", json!({"target": "#1"}));
    assert_eq!(read["description"], "表示名と説明");
    let listed = ok(&core, "mr_list", json!({"value": {"state": "open"}}));
    assert_eq!(listed["mrs"].as_array().unwrap().len(), 1);
    // Made again: the one still out is withdrawn.
    ok(
        &core,
        "branch_merge",
        json!({"target": "work", "value": {
        "title": "客を直す（2）", "take": {"Schemas:Customer": {"label": "target"}}}}),
    );
    let first = ok(&core, "mr_read", json!({"target": "1"}));
    assert_eq!(
        (first["state"].as_str(), first["reason"].as_str()),
        (Some("withdrawn"), Some("出し直し"))
    );
}

#[test]
fn a_request_keeps_the_row_of_a_formatdoc_it_took_and_approved_it_goes_in() {
    let (core, project) = setup();
    for (tree, body) in [("work", "work の本文"), ("main", "main の本文")] {
        ok(
            &core,
            "card_update",
            json!({"branch": tree, "target": "Docs:Spec.intro.body", "value": {"body": body}}),
        );
    }
    ok(
        &core,
        "card_update",
        json!({"branch": "work", "target": "Docs:Spec.tables", "value": {"heading": "work の見出し"}}),
    );
    lock(&core, &project);
    let made = ok(
        &core,
        "branch_merge",
        json!({"target": "work", "value": {"title": "仕様を直す", "take": {
            "Schemas:Customer": {"label": "source"},
            "Docs:Spec": {"rows.intro.body": "source"}}}}),
    );
    let mut took: Vec<(String, String)> = made["result"]["conflicts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            (
                c["unit"].as_str().unwrap().to_string(),
                c["took"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    took.sort();
    assert_eq!(
        took,
        [
            ("label".to_string(), "source".to_string()),
            ("rows.intro.body".to_string(), "source".to_string())
        ],
        "{made}"
    );
    let done = op(
        &core,
        json!({"op": "merge_request_approve", "project": project, "number": 1}),
    );
    assert!(done.get("error").is_none(), "{done}");
    let read = |path: &str| ok(&core, "card_read", json!({"target": path}));
    assert_eq!(read("Docs:Spec.intro.body")["body"], "work の本文");
    assert_eq!(read("Docs:Spec.tables")["heading"], "work の見出し");
}

#[test]
fn approved_it_goes_in_as_it_was_tried() {
    let (core, project) = setup();
    lock(&core, &project);
    ok(
        &core,
        "branch_merge",
        json!({"target": "work", "value": {
        "title": "客を直す", "take": {"Schemas:Customer": {"label": "source"}}}}),
    );
    let done = op(
        &core,
        json!({"op": "merge_request_approve", "project": project, "number": 1}),
    );
    assert!(done.get("error").is_none(), "{done}");
    let customer = ok(&core, "card_read", json!({"target": "Schemas:Customer"}));
    assert_eq!(customer["label"], "branch の客");
    assert_eq!(customer["description"], "branch で書いた");
    assert_eq!(
        ok(&core, "mr_read", json!({"target": "1"}))["state"],
        "merged"
    );
    let history = ok(
        &core,
        "branch_read",
        json!({"target": "main", "value": {"as": "history", "limit": 1}}),
    );
    let lines: Vec<&str> = history["commits"][0]["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l.as_str().unwrap())
        .collect();
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("MR 1「客を直す」を承認して merge")),
        "{lines:?}"
    );
    let again = op(
        &core,
        json!({"op": "merge_request_approve", "project": project, "number": 1}),
    );
    assert_eq!(again["error"], "MR 1 はもうmerge したです");
}

#[test]
fn a_request_tried_on_what_has_moved_since_is_stale_and_not_approved() {
    let (core, project) = setup();
    lock(&core, &project);
    ok(
        &core,
        "branch_merge",
        json!({"target": "work", "value": {
        "title": "客を直す", "take": {"Schemas:Customer": {"label": "source"}}}}),
    );
    // The branch moves on (main is locked: it moves only by a merge).
    ok(
        &core,
        "card_update",
        json!({"branch": "work", "target": "Schemas/Ordering:Order", "value": {"description": "branch が動いた"}}),
    );
    assert_eq!(
        ok(&core, "mr_read", json!({"target": "1"}))["state"],
        "stale"
    );
    let refused = op(
        &core,
        json!({"op": "merge_request_approve", "project": project, "number": 1}),
    );
    assert!(
        refused["error"]
            .as_str()
            .unwrap()
            .contains("古くなりました"),
        "{refused}"
    );
}

/// Main moving apart from what a request brings leaves it open; moving under it — a conflict now
/// — leaves it stale.
#[test]
fn a_request_stays_open_while_main_moves_apart_from_what_it_brings() {
    let (core, project) = setup();
    // Two more branches cut from main as it is now.
    ok(&core, "branch_create", json!({"target": "orders"}));
    ok(
        &core,
        "card_update",
        json!({"branch": "orders", "target": "Schemas/Ordering:Order", "value": {"description": "orders で書いた"}}),
    );
    ok(&core, "branch_create", json!({"target": "rival"}));
    ok(
        &core,
        "card_update",
        json!({"branch": "rival", "target": "Schemas:Customer", "value": {"description": "rival で書いた"}}),
    );
    lock(&core, &project);
    ok(
        &core,
        "branch_merge",
        json!({"target": "work", "value": {
        "title": "客を直す", "take": {"Schemas:Customer": {"label": "source"}}}}),
    );
    ok(
        &core,
        "branch_merge",
        json!({"target": "orders", "value": {"title": "注文を直す"}}),
    );
    ok(
        &core,
        "branch_merge",
        json!({"target": "rival", "value": {"title": "客の説明"}}),
    );
    let state = |n: &str| ok(&core, "mr_read", json!({"target": n}))["state"].clone();
    assert_eq!(
        [state("1"), state("2"), state("3")],
        [json!("open"), json!("open"), json!("open")]
    );
    // MR 1 in: main moved. MR 2 brings what it brought; MR 3's description of Customer conflicts now.
    let approved = op(
        &core,
        json!({"op": "merge_request_approve", "project": project, "number": 1}),
    );
    assert!(approved.get("error").is_none(), "{approved}");
    assert_eq!(state("2"), "open");
    assert_eq!(state("3"), "stale");
    let listed = ok(&core, "mr_list", json!({"value": {"state": "open"}}));
    assert_eq!(listed["mrs"].as_array().unwrap().len(), 1, "{listed}");
    // MR 2 goes in onto main as it is now: both in.
    let approved = op(
        &core,
        json!({"op": "merge_request_approve", "project": project, "number": 2}),
    );
    assert!(approved.get("error").is_none(), "{approved}");
    assert_eq!(
        ok(&core, "card_read", json!({"target": "Schemas:Customer"}))["description"],
        "branch で書いた"
    );
    assert_eq!(
        ok(
            &core,
            "card_read",
            json!({"target": "Schemas/Ordering:Order"})
        )["description"],
        "orders で書いた"
    );
    let refused = op(
        &core,
        json!({"op": "merge_request_approve", "project": project, "number": 3}),
    );
    assert!(
        refused["error"]
            .as_str()
            .unwrap()
            .contains("古くなりました"),
        "{refused}"
    );
}

#[test]
fn rejected_with_a_reason_and_withdrawn_by_whoever_made_it() {
    let (core, project) = setup();
    lock(&core, &project);
    ok(
        &core,
        "branch_merge",
        json!({"target": "work", "value": {
        "title": "客を直す", "take": {"Schemas:Customer": {"label": "source"}}}}),
    );
    let no_reason = op(
        &core,
        json!({"op": "merge_request_reject", "project": project, "number": 1, "reason": " "}),
    );
    assert_eq!(no_reason["error"], "拒否の理由 を入力してください");
    let done = op(
        &core,
        json!({"op": "merge_request_reject", "project": project, "number": 1, "reason": "まだ早い"}),
    );
    assert!(done.get("error").is_none(), "{done}");
    let read = ok(&core, "mr_read", json!({"target": "1"}));
    assert_eq!(
        (read["state"].as_str(), read["reason"].as_str()),
        (Some("rejected"), Some("まだ早い"))
    );
    let said = refused(&core, "mr_delete", json!({"target": "1"}));
    assert!(said.contains("MR 1 はもう拒否したです"), "{said}");

    ok(
        &core,
        "branch_merge",
        json!({"target": "work", "value": {
        "title": "出し直し", "take": {"Schemas:Customer": {"label": "source"}}}}),
    );
    let withdrawn = ok(&core, "mr_delete", json!({"target": "2"}));
    assert_eq!(withdrawn["state"], "withdrawn");
    let requests = op(&core, json!({"op": "merge_requests", "project": project}));
    assert_eq!(requests["result"]["requests"][0]["state"], "withdrawn");
}

#[test]
fn a_branch_with_nothing_for_main_makes_no_request() {
    let (core, project) = setup();
    lock(&core, &project);
    ok(&core, "branch_create", json!({"target": "empty"}));
    let said = refused(
        &core,
        "branch_merge",
        json!({"target": "empty", "value": {"title": "空"}}),
    );
    assert!(
        said.contains("branch empty には main へ入るものがありません"),
        "{said}"
    );
    let listed = ok(&core, "mr_list", json!({}));
    assert_eq!(listed["mrs"], json!([]), "{listed}");
    // A target that is no number is refused too.
    let said = refused(&core, "mr_read", json!({"target": "x"}));
    assert!(said.contains("target は MR の番号です（例: 3）"), "{said}");
}

#[test]
fn what_the_merge_would_break_is_told_in_the_request() {
    let (core, project) = setup();
    ok(&core, "branch_create", json!({"target": "rename"}));
    ok(
        &core,
        "card_update",
        json!({"branch": "rename", "target": "Schemas/Ordering:Order.total", "value": {"name": "amount"}}),
    );
    ok(
        &core,
        "card_create",
        json!({"target": "Free:Audit", "value": {"kind": "note", "body": "[[Schemas/Ordering:Order.total]] を見る"}}),
    );
    lock(&core, &project);
    let made = ok(
        &core,
        "branch_merge",
        json!({"target": "rename", "value": {"title": "改名"}}),
    );
    let warnings = made["result"]["warnings"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        warnings
            .iter()
            .any(|w| w.as_str().unwrap().contains("Schemas/Ordering:Order.total")),
        "{made}"
    );
    // Only tried: main is as it was.
    ok(
        &core,
        "card_read",
        json!({"target": "Schemas/Ordering:Order.total"}),
    );
}

/// Where main places Customer.
fn customer_at(core: &Core) -> Value {
    ok(core, "card_read", json!({"target": "Schemas:Customer"}))["topLeft"].clone()
}

#[test]
fn a_request_asking_for_the_boxes_puts_them_into_a_locked_main_as_the_branch_has_them_then() {
    let (core, project) = setup();
    ok(
        &core,
        "card_update",
        json!({"branch": "work", "target": "Schemas:Customer", "value": {"topLeft": {"x": 500, "y": 60}}}),
    );
    lock(&core, &project);
    let made = ok(
        &core,
        "branch_merge",
        json!({"target": "work", "value": {
        "title": "客を直して置き直す", "take": {"Schemas:Customer": {"label": "source"}}, "places": true}}),
    );
    assert_eq!(made["places"], true, "{made}");
    assert_eq!(made["result"]["places"], 1, "{made}");
    let read = ok(&core, "mr_read", json!({"target": "1"}));
    assert_eq!(read["places"], true, "{read}");
    assert_eq!(read["result"]["places"], 1, "{read}");
    let listed = op(&core, json!({"op": "merge_requests", "project": project}));
    assert_eq!(listed["result"]["requests"][0]["places"], true, "{listed}");

    // Moved again after it was made: how it looks, so the request is not stale, and the
    // box goes in as the branch has it when it is approved.
    ok(
        &core,
        "card_update",
        json!({"branch": "work", "target": "Schemas:Customer", "value": {"topLeft": {"x": 520, "y": 70}}}),
    );
    assert_eq!(
        ok(&core, "mr_read", json!({"target": "1"}))["state"],
        "open"
    );
    let done = op(
        &core,
        json!({"op": "merge_request_approve", "project": project, "number": 1}),
    );
    assert!(done.get("error").is_none(), "{done}");
    assert_eq!(done["result"]["places"], 1, "{done}");
    assert_eq!(customer_at(&core), json!({"x": 520, "y": 70}));
    let customer = ok(&core, "card_read", json!({"target": "Schemas:Customer"}));
    assert_eq!(customer["label"], "branch の客");
}

#[test]
fn a_request_not_asking_for_the_boxes_leaves_them_unless_the_person_takes_them() {
    let (core, project) = setup();
    let before = customer_at(&core);
    ok(
        &core,
        "card_update",
        json!({"branch": "work", "target": "Schemas:Customer", "value": {"topLeft": {"x": 500, "y": 60}}}),
    );
    lock(&core, &project);
    let made = ok(
        &core,
        "branch_merge",
        json!({"target": "work", "value": {
        "title": "客を直す", "take": {"Schemas:Customer": {"label": "source"}}, "keep": true}}),
    );
    // Not asked for, but told: the person may take them along.
    assert!(made.get("places").is_none(), "{made}");
    assert!(made["result"].get("places").is_none(), "{made}");
    assert_eq!(made["result"]["placesLeft"], 1, "{made}");
    assert!(
        made["result"]["hint"]
            .as_str()
            .unwrap()
            .contains("places: true"),
        "{made}"
    );
    let read = ok(&core, "mr_read", json!({"target": "1"}));
    assert_eq!(read["result"]["placesLeft"], 1, "{read}");
    let done = op(
        &core,
        json!({"op": "merge_request_approve", "project": project, "number": 1}),
    );
    assert!(done.get("error").is_none(), "{done}");
    assert_eq!(done["result"]["placesLeft"], 1, "{done}");
    assert_eq!(customer_at(&core), before);

    // Only the boxes are left: a request for them alone is made, and one without them is not.
    let said = refused(
        &core,
        "branch_merge",
        json!({"target": "work", "value": {"title": "置き直す"}}),
    );
    assert!(
        said.contains("branch work には main へ入るものがありません"),
        "{said}"
    );
    ok(
        &core,
        "branch_merge",
        json!({"target": "work", "value": {"title": "置き直す", "places": true, "keep": true}}),
    );
    // The person says no to them: they stay out.
    let done = op(
        &core,
        json!({"op": "merge_request_approve", "project": project, "number": 2, "places": false}),
    );
    assert!(done.get("error").is_none(), "{done}");
    assert_eq!(customer_at(&core), before);
    // And taken along from the window, asked for or not.
    ok(
        &core,
        "card_update",
        json!({"branch": "work", "target": "Schemas:Customer", "value": {"description": "もう一度"}}),
    );
    ok(
        &core,
        "branch_merge",
        json!({"target": "work", "value": {"title": "説明を直す"}}),
    );
    let done = op(
        &core,
        json!({"op": "merge_request_approve", "project": project, "number": 3, "places": true}),
    );
    assert!(done.get("error").is_none(), "{done}");
    assert_eq!(customer_at(&core), json!({"x": 500, "y": 60}));
}

/// What a merge into main would leave, opened as a branch to look at: a merge request's as it was
/// asked for, or the merge window's; the app's own, listed nowhere and named by no one, let go when
/// the window closes it. main is as it was.
#[test]
fn a_merge_result_is_opened_as_a_branch_of_its_own_and_let_go() {
    let (core, project) = setup();
    lock(&core, &project);
    ok(
        &core,
        "branch_merge",
        json!({"target": "work", "value": {
        "title": "客を直す", "take": {"Schemas:Customer": {"label": "source"}}}}),
    );
    let label_in = |tree: &str| -> Value {
        let detail = op(
            &core,
            json!({"op": "project_detail", "project": project, "branch": tree}),
        );
        detail["result"]["schemas"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["name"] == "Customer")
            .map(|s| s["label"].clone())
            .unwrap_or(Value::Null)
    };
    let opened = op(
        &core,
        json!({"op": "merge_request_open", "project": project, "number": 1}),
    );
    assert!(opened.get("error").is_none(), "{opened}");
    let result = opened["result"]["branch"].as_str().unwrap().to_string();
    assert_eq!(label_in(&result), "branch の客");
    assert_eq!(label_in(""), "main の客", "main に入った");
    let changed: Vec<&str> = opened["result"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["path"].as_str().unwrap())
        .collect();
    assert_eq!(changed, ["Schemas:Customer"]);
    // Listed nowhere, named by no one.
    let listed = op(&core, json!({"op": "branches", "project": project}));
    assert!(!listed.to_string().contains(&result), "{listed}");
    let mcp = ok(&core, "branch_list", json!({}));
    assert!(!mcp.to_string().contains("~result"), "{mcp}");
    let said = refused(
        &core,
        "card_read",
        json!({"branch": result, "target": "Schemas:Customer"}),
    );
    assert!(
        said.contains(&format!("branch「{result}」はありません")),
        "{said}"
    );
    // The merge window's, from the branch: one at a time — the one before goes.
    let branches = op(&core, json!({"op": "branches", "project": project}));
    let work = branches["result"]["branches"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["name"] == "work")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let again = op(
        &core,
        json!({"op": "merge_result_open", "project": project, "source": work, "take": {"Schemas:Customer": {"label": "target"}}}),
    );
    assert!(again.get("error").is_none(), "{again}");
    let second = again["result"]["branch"].as_str().unwrap().to_string();
    assert_ne!(second, result);
    assert_eq!(label_in(&second), "main の客");
    let left: i64 = rusqlite::Connection::open(core.place())
        .unwrap()
        .query_row(
            "SELECT count(*) FROM branches WHERE name LIKE '~result-%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(left, 1);
    // Into another branch, not shown so.
    let refused_into = op(
        &core,
        json!({"op": "merge_result_open", "project": project, "source": work, "target": work}),
    );
    assert_eq!(
        refused_into["error"],
        "結果を開いて見られるのは、main への merge だけです"
    );
    // Closed: gone, with nothing said in main's history.
    let before = ok(
        &core,
        "branch_read",
        json!({"target": "main", "value": {"as": "history", "limit": 1}}),
    );
    let closed = op(
        &core,
        json!({"op": "merge_result_close", "project": project}),
    );
    assert!(closed.get("error").is_none(), "{closed}");
    let left: i64 = rusqlite::Connection::open(core.place())
        .unwrap()
        .query_row(
            "SELECT count(*) FROM branches WHERE name LIKE '~result-%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(left, 0);
    let after = ok(
        &core,
        "branch_read",
        json!({"target": "main", "value": {"as": "history", "limit": 1}}),
    );
    assert_eq!(before["commits"], after["commits"]);
    // The request is still out, as it was.
    assert_eq!(
        ok(&core, "mr_read", json!({"target": "1"}))["state"],
        "open"
    );
}

/// What the core tells, heard.
struct Heard(std::sync::Mutex<Vec<Value>>);

impl specboard_core::Listener for Heard {
    fn changed(&self, notice: &str) {
        self.0
            .lock()
            .unwrap()
            .push(serde_json::from_str(notice).unwrap());
    }
}

/// A request made, made again or withdrawn tells the windows of main, though nothing in main was
/// written.
#[test]
fn a_request_made_or_withdrawn_is_told_of() {
    let (core, project) = setup();
    lock(&core, &project);
    let heard = std::sync::Arc::new(Heard(Default::default()));
    core.listen(heard.clone());
    let told = |heard: &Heard| -> usize {
        let mut said = heard.0.lock().unwrap();
        let n = said
            .iter()
            .filter(|n| {
                n["op"] == "changed" && n["project"] == project.as_str() && n["branch"] == ""
            })
            .count();
        said.clear();
        n
    };
    let take = json!({"Schemas:Customer": {"label": "source"}});
    ok(
        &core,
        "branch_merge",
        json!({"target": "work", "value": {"take": take}}),
    );
    assert_eq!(told(&heard), 1);
    ok(
        &core,
        "branch_merge",
        json!({"target": "work", "value": {"take": take}}),
    );
    assert_eq!(told(&heard), 1);
    ok(&core, "mr_delete", json!({"target": 2}));
    assert_eq!(told(&heard), 1);
}
