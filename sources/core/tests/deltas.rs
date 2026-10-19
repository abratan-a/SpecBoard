//! The screen's reading of a project told as what changed (`project_detail`'s `since`,
//! `specboard://SpecBoard/Docs/Design:CoreApi`「頼み」): a write's entries alone, put on what the
//! reader holds as the app puts them, give what a whole reading gives, down to the digest; what the
//! reader was not sent is answered whole; and each reader is answered against its own.

use crate::common;

use common::{ok, op, project_id, script_store};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use specboard_core::Core;
use std::collections::HashMap;

const LISTS: [&str; 4] = ["schemas", "inner", "ghosts", "sections"];

fn read(core: &Core, connection: &str, branch: &str, since: Option<&str>) -> Value {
    let mut request = json!({
        "op": "project_detail",
        "project": project_id(core),
        "branch": branch,
        "connection": connection,
    });
    if let Some(since) = since {
        request["since"] = since.into();
    }
    let answer = op(core, request);
    assert!(answer.get("error").is_none(), "{answer}");
    answer["result"].clone()
}

fn digest(answer: &Value) -> String {
    answer["digest"].as_str().unwrap().to_string()
}

/// The digest of the hashes a reader holds, worked out as the app works it out.
fn digest_of(hashes: &Map<String, Value>) -> String {
    let mut keys: Vec<&String> = hashes.keys().collect();
    keys.sort();
    let mut hasher = Sha256::new();
    for key in keys {
        hasher.update(key.as_bytes());
        hasher.update(b"\t");
        hasher.update(hashes[key].as_str().unwrap().as_bytes());
        hasher.update(b"\n");
    }
    hasher.finalize()[..8]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// A delta put on the whole reading a reader holds, as the app puts it: entries replaced, put in or
/// taken out by id, a card's lines replaced together, each list in the order sent (or the one held),
/// the lines in the order of the cards they leave; and the hashes with them.
fn patch(held: &mut Value, delta: &Value) {
    assert_eq!(delta["delta"], true, "{delta}");
    assert_eq!(delta["from"], held["digest"]);
    for list in LISTS {
        let mut by_id: HashMap<String, Value> = HashMap::new();
        let mut order: Vec<String> = Vec::new();
        for entry in held[list].as_array().unwrap() {
            let id = entry["id"].as_str().unwrap().to_string();
            order.push(id.clone());
            by_id.insert(id, entry.clone());
        }
        for id in delta["removed"][list].as_array().unwrap() {
            by_id.remove(id.as_str().unwrap());
        }
        for entry in delta["changed"][list].as_array().unwrap() {
            by_id.insert(entry["id"].as_str().unwrap().to_string(), entry.clone());
        }
        if let Some(sent) = delta["order"][list].as_array() {
            order = sent
                .iter()
                .map(|i| i.as_str().unwrap().to_string())
                .collect();
        }
        assert_eq!(order.len(), by_id.len(), "{list}: {delta}");
        held[list] = Value::Array(order.iter().map(|id| by_id[id].clone()).collect());
    }
    let mut lines: HashMap<String, Vec<Value>> = HashMap::new();
    for line in held["relations"].as_array().unwrap() {
        lines
            .entry(line["fromSchemaId"].as_str().unwrap().to_string())
            .or_default()
            .push(line.clone());
    }
    for id in delta["removed"]["relations"].as_array().unwrap() {
        lines.remove(id.as_str().unwrap());
    }
    for (id, group) in delta["changed"]["relations"].as_object().unwrap() {
        lines.insert(id.clone(), group.as_array().unwrap().clone());
    }
    let mut flat = Vec::new();
    for list in ["schemas", "inner"] {
        for entry in held[list].as_array().unwrap() {
            flat.extend(
                lines
                    .remove(entry["id"].as_str().unwrap())
                    .unwrap_or_default(),
            );
        }
    }
    assert!(lines.is_empty(), "lines of no card: {lines:?}");
    held["relations"] = Value::Array(flat);
    for one in ["project", "kinds"] {
        if let Some(entry) = delta["changed"].get(one) {
            held[one] = entry.clone();
        }
    }
    let hashes = held["hashes"].as_object_mut().unwrap();
    for list in LISTS.into_iter().chain(["relations"]) {
        for id in delta["removed"][list].as_array().unwrap() {
            hashes.remove(&format!("{list}/{}", id.as_str().unwrap()));
        }
    }
    for (key, hash) in delta["hashes"].as_object().unwrap() {
        hashes.insert(key.clone(), hash.clone());
    }
    let worked_out = digest_of(hashes);
    assert_eq!(
        worked_out,
        digest(delta),
        "the digest worked out of the hashes held"
    );
    held["digest"] = delta["digest"].clone();
}

/// Reads again with what `held` was sent, puts the delta on it and holds it to a whole reading
/// on another reader's way: the same entries, the same digest. The delta, for what else is asked.
fn read_again(core: &Core, branch: &str, held: &mut Value) -> Value {
    let delta = read(core, "app", branch, Some(&digest(held)));
    patch(held, &delta);
    let whole = read(core, "fresh", branch, None);
    assert_eq!(*held, whole);
    delta
}

fn ids(list: &Value) -> Vec<&str> {
    let mut out: Vec<&str> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_str().unwrap())
        .collect();
    out.sort();
    out
}

fn id_of(whole: &Value, list: &str, name: &str) -> String {
    whole[list]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == name)
        .unwrap_or_else(|| panic!("{list} に {name} がありません"))["id"]
        .as_str()
        .unwrap()
        .to_string()
}

fn nothing_else(delta: &Value, but: &[&str]) {
    for list in LISTS {
        if !but.contains(&list) {
            assert_eq!(delta["changed"][list], json!([]), "{list}: {delta}");
        }
        assert_eq!(delta["removed"][list], json!([]), "{list}: {delta}");
    }
    assert_eq!(delta["removed"]["relations"], json!([]), "{delta}");
}

#[test]
fn one_card_written_is_sent_alone_with_the_digest_a_whole_reading_has() {
    let core = Core::open(script_store()).unwrap();
    let mut held = read(&core, "app", "", None);
    assert!(held["hashes"].as_object().unwrap().len() > held["schemas"].as_array().unwrap().len());
    assert_eq!(
        digest_of(held["hashes"].as_object().unwrap()),
        digest(&held)
    );

    // Nothing written: nothing sent, the digest as it was.
    let delta = read_again(&core, "", &mut held);
    nothing_else(&delta, &[]);
    assert_eq!(delta["changed"]["relations"], json!({}));
    assert_eq!(delta["hashes"], json!({}));
    assert_eq!(delta["order"], json!({}));
    assert_eq!(delta["from"], delta["digest"]);

    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"description": "書き直した"}}),
    );
    let delta = read_again(&core, "", &mut held);
    let customer = id_of(&held, "schemas", "Customer");
    assert_eq!(ids(&delta["changed"]["schemas"]), vec![customer.as_str()]);
    nothing_else(&delta, &["schemas"]);
    assert_eq!(delta["changed"]["relations"], json!({}));
    // The project, for when it was written last.
    assert!(
        delta["changed"]["project"]["updatedAt"].is_string(),
        "{delta}"
    );
    assert!(delta["changed"].get("kinds").is_none(), "{delta}");
    assert_eq!(delta["order"], json!({}));
    assert_eq!(
        delta["hashes"]
            .as_object()
            .unwrap()
            .keys()
            .collect::<Vec<_>>(),
        vec!["project", &format!("schemas/{customer}")]
    );
}

#[test]
fn writes_of_every_kind_put_on_what_is_held_give_a_whole_reading() {
    let core = Core::open(script_store()).unwrap();
    let mut held = read(&core, "app", "", None);

    // A card made: new, and the order with it.
    ok(
        &core,
        "card_create",
        json!({"target": "Schemas:Fresh", "value": {"kind": "entity", "fields": [{"name": "code", "type": "string"}]}}),
    );
    let delta = read_again(&core, "", &mut held);
    let fresh = id_of(&held, "schemas", "Fresh");
    assert!(ids(&delta["changed"]["schemas"]).contains(&fresh.as_str()));
    assert!(delta["order"]["schemas"].is_array(), "{delta}");

    // A line drawn from it: its lines sent.
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Fresh", "value": {"connections": {"add": [{"line": "points", "from": "code", "to": "Schemas:Customer"}]}}}),
    );
    let delta = read_again(&core, "", &mut held);
    assert!(
        delta["changed"]["relations"].get(&fresh).is_some(),
        "{delta}"
    );

    // A flow's node written: in `inner`.
    ok(
        &core,
        "card_update",
        json!({"target": "Charts/Shop:Checkout.save", "value": {"description": "保存する"}}),
    );
    let delta = read_again(&core, "", &mut held);
    assert_eq!(
        delta["changed"]["inner"].as_array().unwrap().len(),
        1,
        "{delta}"
    );

    // A section made.
    ok(
        &core,
        "section_create",
        json!({"target": "Schemas/Billing"}),
    );
    let delta = read_again(&core, "", &mut held);
    assert_eq!(
        delta["changed"]["sections"].as_array().unwrap().len(),
        1,
        "{delta}"
    );
    assert!(delta["order"]["sections"].is_array(), "{delta}");

    // A card moved to it.
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Fresh", "value": {"section": "Schemas/Billing"}}),
    );
    read_again(&core, "", &mut held);

    // The card let go of: gone with its lines.
    ok(
        &core,
        "card_delete",
        json!({"target": "Schemas/Billing:Fresh"}),
    );
    let delta = read_again(&core, "", &mut held);
    assert_eq!(delta["removed"]["schemas"], json!([fresh]));
    assert_eq!(delta["removed"]["relations"], json!([fresh]));
    assert!(
        !held["hashes"]
            .as_object()
            .unwrap()
            .contains_key(&format!("schemas/{fresh}"))
    );
}

#[test]
fn a_card_let_go_of_is_in_removed() {
    let core = Core::open(script_store()).unwrap();
    let mut held = read(&core, "app", "", None);
    let memo = id_of(&held, "schemas", "Memo");
    ok(&core, "card_delete", json!({"target": "Free:Memo"}));
    let delta = read_again(&core, "", &mut held);
    assert_eq!(delta["removed"]["schemas"], json!([memo]));
    assert!(delta["order"]["schemas"].is_array(), "{delta}");
}

#[test]
fn renaming_a_card_others_mention_sends_their_lines_again() {
    let core = Core::open(script_store()).unwrap();
    let mut held = read(&core, "app", "", None);
    let order = id_of(&held, "schemas", "Order");
    // The cards whose text names it, as it is read now.
    let mut naming: Vec<String> = held["relations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["line"] == "mentions" && r["toSchemaId"] == order.as_str())
        .map(|r| r["fromSchemaId"].as_str().unwrap().to_string())
        .collect();
    naming.sort();
    naming.dedup();
    assert!(naming.len() >= 2, "{naming:?}");

    ok(
        &core,
        "card_update",
        json!({"target": "Schemas/Ordering:Order", "value": {"name": "Purchase"}}),
    );
    let delta = read_again(&core, "", &mut held);
    let sent = delta["changed"]["relations"].as_object().unwrap();
    for id in &naming {
        assert!(sent.contains_key(id), "{id}: {delta}");
    }
}

#[test]
fn a_box_moved_is_sent() {
    let core = Core::open(script_store()).unwrap();
    let mut held = read(&core, "app", "", None);
    let customer = id_of(&held, "schemas", "Customer");
    let answer = op(
        &core,
        json!({"op": "places", "project": project_id(&core), "boxes": [{"item": customer, "x": 41, "y": 3}]}),
    );
    assert!(answer.get("error").is_none(), "{answer}");
    let delta = read_again(&core, "", &mut held);
    assert_eq!(ids(&delta["changed"]["schemas"]), vec![customer.as_str()]);
    nothing_else(&delta, &["schemas"]);
}

#[test]
fn a_digest_not_sent_is_answered_whole() {
    let core = Core::open(script_store()).unwrap();
    read(&core, "app", "", None);
    let answer = read(&core, "app", "", Some("0000000000000000"));
    assert!(answer.get("delta").is_none());
    assert!(answer["schemas"].is_array());
    assert_eq!(
        digest_of(answer["hashes"].as_object().unwrap()),
        digest(&answer)
    );
}

#[test]
fn each_reader_and_tree_is_answered_against_its_own() {
    let core = Core::open(script_store()).unwrap();
    let main = read(&core, "a", "", None);
    let board = read(&core, "a", "~board", None);

    // Another reader was not sent it.
    assert!(
        read(&core, "b", "", Some(&digest(&main)))
            .get("delta")
            .is_none()
    );
    // Nor another tree.
    assert!(
        read(&core, "a", "~board", Some(&digest(&main)))
            .get("delta")
            .is_none()
    );
    assert_eq!(read(&core, "a", "", Some(&digest(&main)))["delta"], true);
    assert_eq!(
        read(&core, "a", "~board", Some(&digest(&board)))["delta"],
        true
    );
    // The app that holds the core is a reader of its own.
    assert!(
        op(
            &core,
            json!({"op": "project_detail", "project": project_id(&core), "since": digest(&main)})
        )["result"]
            .get("delta")
            .is_none()
    );

    // A reader that went is not answered against what it was sent.
    core.forget_reader("a");
    assert!(
        read(&core, "a", "", Some(&digest(&main)))
            .get("delta")
            .is_none()
    );
}
