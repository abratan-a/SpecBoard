//! A data card's arrays an element at a time (`specboard://SpecBoard/Docs/Design:CardKinds`「データ（instance）」):
//! an element's shape with a primary key keeps its array's elements apart on every write, an element is
//! made at its own path (`card_create`) and taken out with the lines that leave it (`card_delete`), a
//! merge takes a data card's top-level fields and a keyed array's elements one by one, and a list of
//! data cards can leave their values out with the bodies.

use crate::common;

use common::{checked, ok, refused, script_store};
use serde_json::{Value, json};
use specboard_core::Core;

/// The sample project with a definition section `Data`, the shapes of a host's ports (a primary key
/// of two fields and a unique one), notes (no key) and groups (plain values), a kind of line from a
/// data card's row to a data card, and the hosts `vault` and `web`.
fn with_hosts() -> Core {
    let core = Core::open(script_store()).unwrap();
    let mut tree = ok(&core, "project_read", json!({}))["projectTree"].clone();
    tree["main_sections"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type": "definition", "name": "Data"}));
    ok(
        &core,
        "project_update",
        json!({"value": {"project_tree": tree}}),
    );
    ok(
        &core,
        "kind_create",
        json!({"target": "uses", "value": {"of": "line", "label": "使う", "base": "link",
            "from": [{"kinds": ["instance"], "at": ["card", "row"]}],
            "to": {"kinds": ["instance"], "at": ["card", "row"]}}}),
    );
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Data:Port": {"kind": "value", "fields": [
                {"name": "number", "type": "int", "primaryKey": true},
                {"name": "protocol", "type": "string", "primaryKey": true},
                {"name": "service", "type": "string", "unique": true}]},
            "Data:Note": {"kind": "value", "fields": [{"name": "text", "type": "string"}]},
            "Data:Host": {"kind": "entity", "fields": [
                {"name": "region", "type": "string"},
                {"name": "ports", "type": "string", "array": true},
                {"name": "notes", "type": "string", "array": true},
                {"name": "groups", "type": "string", "array": true}],
                "connections": [
                    {"line": "embed", "from": "ports", "to": "Data:Port"},
                    {"line": "embed", "from": "notes", "to": "Data:Note"}]}
        }}),
    );
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Data:vault": {"kind": "instance", "of": "Data:Host", "data": {"region": "tokyo",
                "ports": [{"number": 53, "protocol": "udp", "service": "dns"},
                          {"number": 22, "protocol": "tcp", "service": "ssh"}],
                "notes": [{"text": "a"}, {"text": "b"}],
                "groups": ["ops"]}},
            "Data:web": {"kind": "instance", "of": "Data:Host", "data": {"region": "osaka"}}
        }}),
    );
    core
}

fn data(core: &Core, branch: &str, card: &str) -> Value {
    ok(core, "card_read", json!({"branch": branch, "target": card}))["data"].clone()
}

fn ports(core: &Core, branch: &str) -> Vec<(i64, String)> {
    data(core, branch, "Data:vault")["ports"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["number"].as_i64().unwrap(),
                p["protocol"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

#[test]
fn an_arrays_keys_are_held_apart_on_every_write() {
    let core = with_hosts();
    // Written whole: a key twice, a key field missing, a unique field's value twice.
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Data:vault", "value": {"data": {"ports": [
            {"number": 53, "protocol": "udp"},
            {"number": 22, "protocol": "tcp", "service": "ssh"},
            {"number": 53, "protocol": "udp"},
            {"number": 80, "service": "ssh"}]}}}),
    );
    for wanted in [
        "Data:vault.ports[2]: 鍵（number=53,protocol=udp）が ports[0] と重なります",
        "Data:vault.ports[3]: 鍵の欄 protocol がありません",
        "Data:vault.ports[3].service: unique の欄 service の値 ssh が ports[1] と重なります",
    ] {
        assert!(said.contains(wanted), "wanted「{wanted}」: {said}");
    }
    // The number 53 and the text "53" are the same key: a path cannot tell them apart.
    ok(
        &core,
        "card_create",
        json!({"target": "Data:Pair", "value": {"kind": "value", "fields": [
            {"name": "id", "type": "json", "primaryKey": true}]}}),
    );
    ok(
        &core,
        "card_create",
        json!({"target": "Data:Pairs", "value": {"kind": "entity", "fields": [
            {"name": "items", "type": "string", "array": true}],
            "connections": [{"line": "embed", "from": "items", "to": "Data:Pair"}]}}),
    );
    let said = refused(
        &core,
        "card_create",
        json!({"target": "Data:twins", "value": {"kind": "instance", "of": "Data:Pairs",
            "data": {"items": [{"id": 53}, {"id": "53"}]}}}),
    );
    assert!(
        said.contains("鍵（id=\"53\"）が items[0] と重なります"),
        "{said}"
    );
    // One element written over with another's key; one value making a key the one before's.
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Data:vault.ports[number=22,protocol=tcp]",
            "value": {"value": {"number": 53, "protocol": "udp"}}}),
    );
    assert!(
        said.contains("Data:vault.ports[1]: 鍵（number=53,protocol=udp）が ports[0] と重なります"),
        "{said}"
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Data:vault.ports[number=22,protocol=tcp].protocol", "value": {"value": "udp"}}),
    );
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Data:vault.ports[number=53,protocol=udp].number", "value": {"value": 22}}),
    );
    assert!(
        said.contains("Data:vault.ports[1]: 鍵（number=22,protocol=udp）が ports[0] と重なります"),
        "{said}"
    );
    // Nothing of it was written.
    assert_eq!(
        ports(&core, "main"),
        [(53, "udp".to_string()), (22, "udp".to_string())]
    );
}

#[test]
fn an_element_is_made_at_its_path_by_its_key_its_place_or_its_value() {
    let core = with_hosts();
    // By its key: the key fields come from the path; first, as position says.
    let made = ok(
        &core,
        "card_create",
        json!({"target": "Data:vault.ports[number=443,protocol=tcp]",
            "value": {"value": {"service": "https"}, "position": 0}}),
    );
    assert_eq!(
        made["path"], "Data:vault.ports[number=443,protocol=tcp]",
        "{made}"
    );
    assert!(made["rev"].is_string(), "{made}");
    assert_eq!(
        data(&core, "main", "Data:vault")["ports"][0],
        json!({"service": "https", "number": 443, "protocol": "tcp"})
    );
    // A key there already, a key written apart from the path's, a place in a keyed array.
    let there = refused(
        &core,
        "card_create",
        json!({"target": "Data:vault.ports[number=53,protocol=udp]", "value": {}}),
    );
    assert!(
        there.contains("もうあります") && there.contains("card_update"),
        "{there}"
    );
    let apart = refused(
        &core,
        "card_create",
        json!({"target": "Data:vault.ports[number=8080,protocol=tcp]",
            "value": {"value": {"number": 8081}}}),
    );
    assert!(apart.contains("パスの鍵と違います"), "{apart}");
    let by_place = refused(
        &core,
        "card_create",
        json!({"target": "Data:vault.ports[1]", "value": {"value": {"number": 1, "protocol": "tcp"}}}),
    );
    assert!(by_place.contains("鍵の条件で作ります"), "{by_place}");
    // No key: by the place it goes in, the length last; past it, refused.
    ok(
        &core,
        "card_create",
        json!({"target": "Data:vault.notes[1]", "value": {"value": {"text": "between"}}}),
    );
    ok(
        &core,
        "card_create",
        json!({"target": "Data:vault.notes[3]", "value": {}}),
    );
    assert_eq!(
        data(&core, "main", "Data:vault")["notes"],
        json!([{"text": "a"}, {"text": "between"}, {"text": "b"}, {}])
    );
    let past = refused(
        &core,
        "card_create",
        json!({"target": "Data:vault.notes[9]", "value": {}}),
    );
    assert!(past.contains("並びの長さ（4）"), "{past}");
    let keyed = refused(
        &core,
        "card_create",
        json!({"target": "Data:vault.notes[text=x]", "value": {}}),
    );
    assert!(keyed.contains("主キーが無い"), "{keyed}");
    // Plain values: by the value; an array not written yet is made.
    ok(
        &core,
        "card_create",
        json!({"target": "Data:vault.groups[=api]", "value": {}}),
    );
    ok(
        &core,
        "card_create",
        json!({"target": "Data:web.groups[=edge]", "value": {}}),
    );
    assert_eq!(
        data(&core, "main", "Data:vault")["groups"],
        json!(["ops", "api"])
    );
    assert_eq!(data(&core, "main", "Data:web")["groups"], json!(["edge"]));
    // Any other value is not made, and the guide's form is the element's.
    let other = refused(
        &core,
        "card_create",
        json!({"target": "Data:vault.region", "value": {"value": "x"}}),
    );
    assert!(other.contains("中の値は作るものではありません"), "{other}");
}

#[test]
fn an_element_put_in_moves_the_lines_that_pick_those_after_it_by_place() {
    let core = with_hosts();
    ok(
        &core,
        "card_update",
        json!({"target": "Data:web", "value": {"connections": {"add": [
            {"line": "uses", "to": "Data:vault", "at": "notes[1]"},
            {"line": "uses", "to": "Data:vault", "at": "notes[0]"}]}}}),
    );
    let made = ok(
        &core,
        "card_create",
        json!({"target": "Data:vault.notes[1]", "value": {"value": {"text": "new"}}}),
    );
    assert!(made["warnings"].to_string().contains("notes[2]"), "{made}");
    let lines: Vec<String> = ok(&core, "card_read", json!({"target": "Data:web"}))["connections"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["at"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(lines, ["notes[2]", "notes[0]"]);
}

#[test]
fn an_element_taken_out_takes_its_own_lines_and_leaves_those_to_it_pointing_nowhere() {
    let core = with_hosts();
    ok(
        &core,
        "card_update",
        json!({"target": "Data:vault", "value": {"connections": {"add": [
            {"line": "uses", "from": "ports[number=53,protocol=udp]", "to": "Data:web"},
            {"line": "uses", "from": "ports[number=22,protocol=tcp]", "to": "Data:web"}]}}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Data:web", "value": {"connections": {"add": [
            {"line": "uses", "to": "Data:vault", "at": "ports[number=53,protocol=udp]"}]}}}),
    );
    let gone = ok(
        &core,
        "card_delete",
        json!({"target": "Data:vault.ports[number=53,protocol=udp]"}),
    );
    assert_eq!(
        gone["linesRemoved"],
        json!([{"line": "uses", "from": "ports[number=53,protocol=udp]", "to": "Data:web"}]),
        "{gone}"
    );
    assert_eq!(gone["danglingRefs"], 1, "{gone}");
    assert!(gone["warnings"].to_string().contains("Data:web"), "{gone}");
    let kept: Vec<Value> = ok(&core, "card_read", json!({"target": "Data:vault"}))["connections"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(kept.len(), 1, "{kept:?}");
    assert_eq!(kept[0]["from"], "ports[number=22,protocol=tcp]");
    // The line that landed on it is a broken path of the check.
    let broken = checked(&core, "main")["brokenPaths"].to_string();
    assert!(
        broken.contains("Data:web") && broken.contains("ports[number=53,protocol=udp]"),
        "{broken}"
    );
}

/// Two branches of main as it is: `a` and `b`.
fn branched(core: &Core) {
    ok(core, "branch_create", json!({"target": "a"}));
    ok(core, "branch_create", json!({"target": "b"}));
}

#[test]
fn branches_that_change_other_elements_of_one_data_card_merge_and_the_same_element_conflicts() {
    let core = with_hosts();
    branched(&core);
    // a: one element's field and the results a tool writes beside them (a top-level field).
    ok(
        &core,
        "card_update",
        json!({"branch": "a", "target": "Data:vault.ports[number=53,protocol=udp].service",
            "value": {"value": "resolver"}}),
    );
    ok(
        &core,
        "card_update",
        json!({"branch": "a", "target": "Data:vault.region", "value": {"value": "nagoya"}}),
    );
    // b: an element made, another's field, and a note (an array with no key: one unit).
    ok(
        &core,
        "card_create",
        json!({"branch": "b", "target": "Data:vault.ports[number=443,protocol=tcp]",
            "value": {"value": {"service": "https"}}}),
    );
    ok(
        &core,
        "card_update",
        json!({"branch": "b", "target": "Data:vault.ports[number=22,protocol=tcp].service",
            "value": {"value": "sshd"}}),
    );
    ok(
        &core,
        "branch_merge",
        json!({"target": "a", "value": {"keep": true}}),
    );
    let preview = ok(
        &core,
        "branch_read",
        json!({"target": "b", "value": {"against": "main"}}),
    );
    assert!(preview.get("conflicts").is_none(), "{preview}");
    ok(
        &core,
        "branch_merge",
        json!({"target": "b", "value": {"keep": true}}),
    );
    let now = data(&core, "main", "Data:vault");
    assert_eq!(now["region"], "nagoya", "{now}");
    assert_eq!(
        now["ports"],
        json!([
            {"number": 53, "protocol": "udp", "service": "resolver"},
            {"number": 22, "protocol": "tcp", "service": "sshd"},
            {"number": 443, "protocol": "tcp", "service": "https"}
        ]),
        "{now}"
    );
    // The same element changed on both sides is one conflict, named by its key; taken, it goes in.
    ok(&core, "branch_create", json!({"target": "c"}));
    ok(
        &core,
        "card_update",
        json!({"branch": "c", "target": "Data:vault.ports[number=443,protocol=tcp].service",
            "value": {"value": "tls"}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Data:vault.ports[number=443,protocol=tcp].service",
            "value": {"value": "h2"}}),
    );
    let preview = ok(
        &core,
        "branch_read",
        json!({"target": "c", "value": {"against": "main"}}),
    );
    let units: Vec<&str> = preview["conflicts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["unit"].as_str().unwrap())
        .collect();
    assert_eq!(units, ["data.ports[number=443,protocol=tcp]"], "{preview}");
    assert_eq!(
        preview["conflicts"][0]["source"]["service"], "tls",
        "{preview}"
    );
    let said = refused(&core, "branch_merge", json!({"target": "c"}));
    assert!(said.contains("衝突が 1 あるので"), "{said}");
    ok(
        &core,
        "branch_merge",
        json!({"target": "c", "value": {"take": {"Data:vault": {"data.ports[number=443,protocol=tcp]": "source"}}}}),
    );
    assert_eq!(
        data(&core, "main", "Data:vault")["ports"][2]["service"],
        "tls"
    );
}

#[test]
fn an_element_removed_on_one_side_and_changed_on_the_other_conflicts() {
    let core = with_hosts();
    branched(&core);
    ok(
        &core,
        "card_delete",
        json!({"branch": "a", "target": "Data:vault.ports[number=22,protocol=tcp]"}),
    );
    ok(
        &core,
        "card_update",
        json!({"branch": "b", "target": "Data:vault.ports[number=22,protocol=tcp].service",
            "value": {"value": "sshd"}}),
    );
    ok(
        &core,
        "branch_merge",
        json!({"target": "a", "value": {"keep": true}}),
    );
    assert_eq!(ports(&core, "main"), [(53, "udp".to_string())]);
    let preview = ok(
        &core,
        "branch_read",
        json!({"target": "b", "value": {"against": "main"}}),
    );
    assert_eq!(
        preview["conflicts"][0]["unit"], "data.ports[number=22,protocol=tcp]",
        "{preview}"
    );
    assert!(preview["conflicts"][0]["target"].is_null(), "{preview}");
}

#[test]
fn a_list_without_bodies_leaves_a_data_cards_values_out() {
    let core = with_hosts();
    let listed = ok(
        &core,
        "card_list",
        json!({"value": {"section": "Data", "kind": "instance", "body": false}}),
    );
    let cards = listed["cards"].as_array().unwrap();
    assert_eq!(cards.len(), 2, "{listed}");
    assert!(cards.iter().all(|c| c.get("data").is_none()), "{listed}");
    assert!(cards.iter().all(|c| c["rev"].is_string()), "{listed}");
    let whole = ok(
        &core,
        "card_list",
        json!({"value": {"section": "Data", "kind": "instance"}}),
    );
    assert!(
        whole["cards"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["data"].is_object()),
        "{whole}"
    );
}
