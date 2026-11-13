//! Projects shared between agents over a hub (`specboard://SpecBoard/Docs/Design:DesignHub`). The
//! side that uses a shared project lists it with its owner, takes the names it is called by, hands
//! the call to its owner and answers as the owner said — or says why it is not handed. The side
//! that owns it runs a member's call as that member, by the share and the level sent, and never
//! touches its own folders for it. The way between the two (`specboard_core::Hub`) is the test's
//! own: it keeps what it was handed and answers what the test set.

use crate::common;

use common::op;
use serde_json::{Value, json};
use specboard_core::{App, Core, Hub};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// The build both sides run, unless a test says another.
const BUILD: &str = "0.3.57-1";

/// The members' ids, as the hub names them.
const MAC: &str = "mac-1";
const SERVER: &str = "srv-1";
const LAPTOP: &str = "lap-1";

/// What the way answers when the test set nothing.
fn from_owner() -> Value {
    json!({"from": "owner"})
}

/// One call handed to the way.
#[derive(Clone, Debug)]
struct Handed {
    owner: String,
    request: Value,
    wait: bool,
}

/// The way to the owners: keeps what it was handed, and answers what the test set, in order.
#[derive(Default)]
struct Way {
    handed: Mutex<Vec<Handed>>,
    answers: Mutex<VecDeque<Value>>,
}

impl Way {
    fn handed(&self) -> Vec<Handed> {
        self.handed.lock().unwrap().clone()
    }

    fn answer(&self, answers: &[Value]) {
        self.answers.lock().unwrap().extend(answers.iter().cloned());
    }
}

impl Hub for Way {
    fn call(&self, owner: &str, request: &str, wait: bool) -> String {
        self.handed.lock().unwrap().push(Handed {
            owner: owner.to_string(),
            request: serde_json::from_str(request).unwrap(),
            wait,
        });
        self.answers
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| json!({"output": {"json": from_owner()}}))
            .to_string()
    }
}

/// The hub's members, all linked and of one build.
fn members() -> Value {
    json!([
        {"id": MAC, "name": "Mac", "online": true, "build": BUILD},
        {"id": SERVER, "name": "Server", "online": true, "build": BUILD},
        {"id": LAPTOP, "name": "Laptop", "online": true, "build": BUILD},
    ])
}

/// What the agent tells its core of the hub: who it is there, whether it is linked, the members.
fn state(core: &Core, me: Value, up: bool, members: Value) {
    let said = op(
        core,
        json!({"op": "hub_state", "self": me, "build": BUILD, "up": up, "members": members}),
    );
    assert_eq!(said, json!({"result": {}}));
}

/// A request of the window's that must go through: its result.
fn done(core: &Core, request: Value) -> Value {
    let said = op(core, request.clone());
    assert!(said.get("error").is_none(), "{request}: {said}");
    said["result"].clone()
}

/// A request of the window's that must be refused: why.
fn turned_down(core: &Core, request: Value) -> String {
    let said = op(core, request.clone());
    said["error"]
        .as_str()
        .unwrap_or_else(|| panic!("{request} が通った: {said}"))
        .to_string()
}

/// A tool called through MCP with the key `value`: the call's result, as MCP answers it.
fn raw(core: &Core, value: &str, name: &str, arguments: Value) -> Value {
    let message = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": name, "arguments": arguments}});
    let reply = op(
        core,
        json!({"op": "mcp", "message": message, "headers": {"authorization": format!("Bearer {value}")}}),
    )["result"]
        .clone();
    assert_eq!(reply["status"], 200, "{reply}");
    reply["body"]["result"].clone()
}

/// A tool called through MCP with the key `value`: whether it was refused, and what it said (the
/// refusal's words alone, when refused).
fn tool(core: &Core, value: &str, name: &str, arguments: Value) -> (bool, Value) {
    let result = raw(core, value, name, arguments);
    let text = result["content"][0]["text"].as_str().unwrap_or_default();
    let said: Value = serde_json::from_str(text).unwrap_or_else(|_| text.into());
    if result["isError"] == true {
        return (true, said["error"]["message"].clone());
    }
    (false, said)
}

fn ok(core: &Core, value: &str, name: &str, arguments: Value) -> Value {
    let (refused, said) = tool(core, value, name, arguments);
    assert!(!refused, "{name}: {said}");
    said
}

fn refused(core: &Core, value: &str, name: &str, arguments: Value) -> String {
    let (refused, said) = tool(core, value, name, arguments);
    assert!(refused, "{name} が通った: {said}");
    said.as_str().unwrap().to_string()
}

// MARK: - The side that uses a shared project

/// The agent Mac, linked to the hub, with a key (K) that was given nothing yet.
struct User {
    core: Core,
    way: Arc<Way>,
    /// K's id and value.
    key: (String, String),
}

fn user() -> User {
    let core = Core::open(common::script_store()).unwrap();
    let way = Arc::new(Way::default());
    core.set_hub(way.clone());
    state(&core, MAC.into(), true, members());
    let made = done(&core, json!({"op": "access_key_create", "name": "K"}));
    let key = (
        made["key"]["id"].as_str().unwrap().to_string(),
        made["value"].as_str().unwrap().to_string(),
    );
    User { core, way, key }
}

impl User {
    /// What `owner` told this agent it shares with it.
    fn told(&self, owner: &str, projects: Value) {
        done(
            &self.core,
            json!({"op": "hub_incoming", "owner": owner, "projects": projects}),
        );
    }

    /// Shop, shared by `owner` to read and write, under the id `id`.
    fn told_shop(&self, owner: &str, id: &str) {
        self.told(
            owner,
            json!([{"id": id, "name": "Shop", "slug": "shop", "level": "write"}]),
        );
    }

    /// K given a shared project (`level` null takes it away).
    fn grant(&self, owner: &str, project: &str, level: Value) {
        done(
            &self.core,
            json!({"op": "access_grant", "key": self.key.0, "owner": owner, "project": project, "level": level}),
        );
    }

    /// K given the sample project, this agent's own, to write.
    fn grant_own(&self) {
        done(
            &self.core,
            json!({"op": "access_grant", "key": self.key.0, "project": common::project_id(&self.core), "level": "write"}),
        );
    }

    fn ok(&self, name: &str, arguments: Value) -> Value {
        ok(&self.core, &self.key.1, name, arguments)
    }

    fn refused(&self, name: &str, arguments: Value) -> String {
        refused(&self.core, &self.key.1, name, arguments)
    }

    /// K's row, as the window lists the keys.
    fn key_row(&self) -> Value {
        done(&self.core, json!({"op": "access_keys"}))["keys"]
            .as_array()
            .unwrap()
            .iter()
            .find(|k| k["id"] == self.key.0.as_str())
            .unwrap()
            .clone()
    }
}

#[test]
fn a_shared_project_is_listed_with_its_owner_and_the_lower_level() {
    let u = user();
    u.told(
        SERVER,
        json!([
            {"id": "shop-srv", "name": "Shop", "slug": "shop", "level": "write"},
            {"id": "notes-srv", "name": "Notes", "slug": "notes", "level": "read"},
        ]),
    );
    u.grant(SERVER, "shop-srv", json!("read"));
    u.grant(SERVER, "notes-srv", json!("write"));
    u.grant_own();
    let listed = u.ok("project_list", json!({}));
    let rows = listed["projects"].as_array().unwrap();
    assert_eq!(rows.len(), 3, "{listed}");
    // By name among its own, as the list is: the lower of the owner's share and the key's grant.
    assert_eq!(
        rows[0],
        json!({"project": "Notes", "owner": "Server", "access": "read"})
    );
    assert_eq!(
        rows[1],
        json!({"project": "Shop", "owner": "Server", "access": "read"})
    );
    // Its own, as before.
    assert_eq!(rows[2]["project"], common::PROJECT);
    assert_eq!(rows[2]["access"], "write");
    assert!(rows[2].get("owner").is_none(), "{listed}");
    // A write is refused here, in the words of whichever of the two holds it back.
    let thing = |project: &str| json!({"project": project, "target": "Schemas:Thing", "value": {"kind": "entity"}});
    assert_eq!(
        u.refused("card_create", thing("Shop")),
        "プロジェクト「Shop」は、このアクセスキーでは読むだけです（書けるようにするのは、プロジェクト一覧の場所の設定の「アクセスキー」で）"
    );
    assert_eq!(
        u.refused("card_create", thing("Notes")),
        "プロジェクト「Notes」は、ほかのエージェントからは読むだけです（持ち主の共有か、使う側のアクセスキーの許可が読むだけ）"
    );
    assert!(u.way.handed().is_empty());
}

#[test]
fn a_key_given_nothing_does_not_see_a_shared_project() {
    let u = user();
    u.told_shop(SERVER, "shop-srv");
    let listed = u.ok("project_list", json!({}));
    assert_eq!(listed["projects"], json!([]), "{listed}");
    // Missing in the words of one that is nowhere.
    let shared = u.refused("card_list", json!({"project": "Shop"}));
    let nowhere = u.refused("card_list", json!({"project": "Nowhere"}));
    assert_eq!(shared, nowhere.replace("Nowhere", "Shop"));
    let by_id = u.refused("card_list", json!({"project": "shop-srv"}));
    assert_eq!(by_id, nowhere.replace("Nowhere", "shop-srv"));
    assert!(u.way.handed().is_empty());
    // The key's row shows no grant for it.
    assert_eq!(u.key_row()["hubGrants"], json!([]));
}

#[test]
fn a_call_naming_a_shared_project_is_handed_to_its_owner_and_answered_as_it_says() {
    let u = user();
    u.told_shop(SERVER, "shop-srv");
    u.grant(SERVER, "shop-srv", json!("write"));
    assert_eq!(
        u.key_row()["hubGrants"],
        json!([{"owner": SERVER, "project": "shop-srv", "level": "write"}])
    );
    let note = json!({"target": "window", "width": 640});
    u.way.answer(&[
        json!({"output": {"json": {"card": "Schemas:Order", "name": "注文"}}}),
        json!({"output": {"blocks": ["一つ目", "二つ目"]}}),
        json!({"output": {"image": {"data": "iVBORw0KGgo=", "mimeType": "image/png", "note": note}}}),
        json!({"error": format!(
            "カード「Schemas:Nope」はありません\u{1e}{}",
            json!({"guide": "cards", "fix": [{"at": "/target", "to": "Schemas:Order"}]})
        )}),
    ]);
    let arguments = json!({"project": "Shop", "target": "Schemas:Order"});

    // A structure, as the owner made it.
    let read = raw(&u.core, &u.key.1, "card_read", arguments.clone());
    assert_eq!(read["isError"], false, "{read}");
    assert_eq!(
        serde_json::from_str::<Value>(read["content"][0]["text"].as_str().unwrap()).unwrap(),
        json!({"card": "Schemas:Order", "name": "注文"})
    );
    let handed = u.way.handed();
    assert_eq!(handed.len(), 1);
    assert_eq!(handed[0].owner, SERVER);
    assert!(!handed[0].wait);
    // The project by its id; the key by its name, with what it may do there; the answer's language.
    assert_eq!(
        handed[0].request,
        json!({
            "k": "call",
            "tool": "card_read",
            "arguments": {"project": "shop-srv", "target": "Schemas:Order"},
            "key": "K",
            "level": "write",
            "build": BUILD,
            "lang": "ja",
        })
    );

    // Blocks of text.
    let read = raw(&u.core, &u.key.1, "card_read", arguments.clone());
    assert_eq!(
        read,
        json!({"content": [{"type": "text", "text": "一つ目"}, {"type": "text", "text": "二つ目"}], "isError": false})
    );

    // A picture, then what it is.
    let read = raw(&u.core, &u.key.1, "card_read", arguments);
    assert_eq!(read["isError"], false, "{read}");
    assert_eq!(
        read["content"][0],
        json!({"type": "image", "data": "iVBORw0KGgo=", "mimeType": "image/png"})
    );
    assert_eq!(
        serde_json::from_str::<Value>(read["content"][1]["text"].as_str().unwrap()).unwrap(),
        note
    );

    // A refusal: its words, and what it says beside them — the call to make, written with the
    // project as this call named it.
    let read = raw(
        &u.core,
        &u.key.1,
        "card_read",
        json!({"project": "Shop", "target": "Schemas:Nope"}),
    );
    assert_eq!(read["isError"], true, "{read}");
    assert_eq!(
        serde_json::from_str::<Value>(read["content"][0]["text"].as_str().unwrap()).unwrap(),
        json!({"error": {
            "message": "カード「Schemas:Nope」はありません",
            "guide": "cards",
            "use": {"tool": "card_read", "arguments": {"project": "Shop", "target": "Schemas:Order"}},
        }})
    );
    assert_eq!(u.way.handed().len(), 4);
}

#[test]
fn ones_own_project_comes_first_and_name_at_owner_names_the_shared_one() {
    let u = user();
    // Its own Shop, made by K, which writes it then.
    u.ok("project_create", json!({"project": "Shop"}));
    u.told_shop(SERVER, "shop-srv");
    u.grant(SERVER, "shop-srv", json!("read"));

    let own = u.ok("card_list", json!({"project": "Shop"}));
    assert_ne!(own, from_owner());
    // Listed side by side, its own before the shared one called the same.
    let listed = u.ok("project_list", json!({}));
    let rows = listed["projects"].as_array().unwrap();
    assert_eq!(rows.len(), 2, "{listed}");
    assert!(rows[0].get("owner").is_none(), "{listed}");
    assert_eq!(
        rows[1],
        json!({"project": "Shop", "owner": "Server", "access": "read"})
    );
    assert!(u.way.handed().is_empty());

    for name in ["Shop@Server", "shop@server", "Shop@srv-1"] {
        assert_eq!(
            u.ok("card_list", json!({"project": name})),
            from_owner(),
            "{name}"
        );
    }
    let handed = u.way.handed();
    assert_eq!(handed.len(), 3);
    for one in handed {
        assert_eq!(one.owner, SERVER);
        assert_eq!(one.request["arguments"]["project"], "shop-srv");
        assert_eq!(one.request["level"], "read");
    }
    // An owner that shares no such project is not asked.
    let nowhere = u.refused("card_list", json!({"project": "Shop@Laptop"}));
    assert!(nowhere.contains("Shop@Laptop"), "{nowhere}");
    assert_eq!(u.way.handed().len(), 3);
}

#[test]
fn shared_projects_of_one_name_are_refused_with_the_ways_to_name_each() {
    let u = user();
    u.told_shop(SERVER, "shop-srv");
    u.told_shop(LAPTOP, "shop-lap");
    u.grant(SERVER, "shop-srv", json!("read"));
    u.grant(LAPTOP, "shop-lap", json!("read"));
    assert_eq!(
        u.refused("card_list", json!({"project": "Shop"})),
        "共有されたプロジェクト「Shop」が 2 つあります。project には Shop@Laptop / Shop@Server のどれかを書きます"
    );
    assert!(u.way.handed().is_empty());
    // Each by its owner.
    u.ok("card_list", json!({"project": "Shop@Laptop"}));
    assert_eq!(u.way.handed()[0].owner, LAPTOP);
    // One the key was not given is not among them.
    u.grant(LAPTOP, "shop-lap", Value::Null);
    u.ok("card_list", json!({"project": "Shop"}));
    assert_eq!(u.way.handed()[1].owner, SERVER);
}

#[test]
fn an_owner_away_or_of_another_build_is_refused_without_being_asked() {
    let u = user();
    u.told_shop(SERVER, "shop-srv");
    u.grant(SERVER, "shop-srv", json!("write"));
    let server = |online: bool, build: &str| {
        json!([
            {"id": MAC, "name": "Mac", "online": true, "build": BUILD},
            {"id": SERVER, "name": "Server", "online": online, "build": build},
        ])
    };
    let listed = |why: &str| json!([{"project": "Shop", "owner": "Server", "access": "write", "reachable": false, "why": why}]);

    // Not linked to the hub now.
    let away = "Server は今ハブに繋がっていません";
    state(&u.core, MAC.into(), true, server(false, BUILD));
    assert_eq!(u.refused("card_list", json!({"project": "Shop"})), away);
    assert_eq!(u.ok("project_list", json!({}))["projects"], listed(away));

    // Of another build: its answers are not the ones this side's tools describe.
    let other = "Server は別の版で動いています（アプリを両方のエージェントに繋ぐと揃います）";
    state(&u.core, MAC.into(), true, server(true, "0.3.58-9"));
    assert_eq!(u.refused("card_list", json!({"project": "Shop"})), other);
    assert_eq!(u.ok("project_list", json!({}))["projects"], listed(other));

    // This side not linked: the shared projects stay listed, and their grants stay.
    let down = "ハブに繋がっていません";
    state(&u.core, MAC.into(), false, json!([]));
    assert_eq!(u.refused("card_list", json!({"project": "Shop"})), down);
    assert_eq!(u.ok("project_list", json!({}))["projects"], listed(down));

    assert!(u.way.handed().is_empty());

    // Linked again, it is handed on.
    state(&u.core, MAC.into(), true, server(true, BUILD));
    assert_eq!(
        u.ok("project_list", json!({}))["projects"],
        json!([{"project": "Shop", "owner": "Server", "access": "write"}])
    );
    u.ok("card_list", json!({"project": "Shop"}));
    assert_eq!(u.way.handed().len(), 1);
}

#[test]
fn a_write_cut_on_the_way_is_said_unknown_and_a_read_is_asked_once_more() {
    let u = user();
    u.told_shop(SERVER, "shop-srv");
    u.grant(SERVER, "shop-srv", json!("write"));
    let cut = json!({"failed": "cut"});

    // A write: whether it reached the owner is not known, so it is not sent again.
    u.way.answer(std::slice::from_ref(&cut));
    assert_eq!(
        u.refused(
            "card_update",
            json!({"project": "Shop", "target": "Schemas:Order", "value": {"description": "x"}})
        ),
        "届いたか分かりません。読んで確かめてからやり直してください"
    );
    let handed = u.way.handed();
    assert_eq!(handed.len(), 1);
    assert!(!handed[0].wait);

    // A read: once more, waiting for the way to be there again.
    u.way
        .answer(&[cut.clone(), json!({"output": {"json": {"card": "second"}}})]);
    assert_eq!(
        u.ok(
            "card_read",
            json!({"project": "Shop", "target": "Schemas:Order"})
        ),
        json!({"card": "second"})
    );
    let handed = u.way.handed();
    assert_eq!(handed.len(), 3);
    assert!(!handed[1].wait);
    assert!(handed[2].wait);
    assert_eq!(handed[1].request, handed[2].request);

    // Cut again, it is said — the way was there, so not that this agent is not linked — and not
    // asked a third time.
    u.way.answer(&[cut.clone(), cut.clone()]);
    assert_eq!(
        u.refused(
            "card_read",
            json!({"project": "Shop", "target": "Schemas:Order"})
        ),
        "持ち主までの繋ぎが、読んでいる途中で切れました。もう一度呼んでください"
    );
    assert_eq!(u.way.handed().len(), 5);
    // The way not back in time is this agent not linked.
    u.way.answer(&[cut, json!({"failed": "down"})]);
    assert_eq!(
        u.refused(
            "card_read",
            json!({"project": "Shop", "target": "Schemas:Order"})
        ),
        "ハブに繋がっていません"
    );
}

#[test]
fn a_call_its_owner_does_not_answer_in_time_is_said_unknown_and_not_asked_again() {
    let u = user();
    u.told_shop(SERVER, "shop-srv");
    u.grant(SERVER, "shop-srv", json!("write"));
    let slow = json!({"failed": "slow"});
    let said = "Server が時間内に答えませんでした。届いたか分かりません。読んで確かめてからやり直してください";

    // A read is not asked once more: its owner is there, and still has the first to answer.
    u.way.answer(std::slice::from_ref(&slow));
    assert_eq!(
        u.refused(
            "card_read",
            json!({"project": "Shop", "target": "Schemas:Order"})
        ),
        said
    );
    assert_eq!(u.way.handed().len(), 1);

    // A write: whether it was done is not known, as when the way was cut.
    u.way.answer(std::slice::from_ref(&slow));
    assert_eq!(
        u.refused(
            "card_update",
            json!({"project": "Shop", "target": "Schemas:Order", "value": {"description": "x"}})
        ),
        said
    );
    assert_eq!(u.way.handed().len(), 2);
}

#[test]
fn a_project_its_owner_no_longer_shares_is_said_in_this_sides_names() {
    let u = user();
    u.told_shop(SERVER, "shop-srv");
    u.grant(SERVER, "shop-srv", json!("write"));
    // The owner let go of the share, and what it told of it has not come yet: it says so, and
    // names nothing of its own.
    let unshared = json!({"failed": "unshared"});
    let said = "プロジェクト「Shop」は、Server から共有されていません（project_list で今の一覧を読めます）";
    u.way.answer(&[unshared.clone(), unshared]);
    assert_eq!(u.refused("card_list", json!({"project": "Shop"})), said);
    assert_eq!(
        u.refused(
            "card_update",
            json!({"project": "Shop", "target": "Schemas:Order", "value": {"description": "x"}})
        ),
        said
    );
    // Neither is asked again.
    assert_eq!(u.way.handed().len(), 2);
}

#[test]
fn a_batchs_steps_are_handed_with_the_owners_id_however_they_name_the_project() {
    let u = user();
    u.grant_own();
    u.told_shop(SERVER, "shop-srv");
    u.grant(SERVER, "shop-srv", json!("write"));
    let step = |project: Value| {
        let mut arguments = json!({"target": "Schemas:Thing", "value": {"kind": "entity"}});
        if !project.is_null() {
            arguments["project"] = project;
        }
        json!({"tool": "card_create", "arguments": arguments})
    };

    // The owner knows the project by its id alone: a step that names it as this side does is
    // handed on naming it so.
    let steps = json!([
        step(json!("Shop@Server")),
        step(json!("shop")),
        step(Value::Null)
    ]);
    u.ok(
        "batch",
        json!({"project": "Shop@Server", "value": {"steps": steps}}),
    );
    let handed = u.way.handed();
    assert_eq!(handed.len(), 1);
    let arguments = &handed[0].request["arguments"];
    assert_eq!(arguments["project"], "shop-srv");
    let said = &arguments["value"]["steps"];
    assert_eq!(said[0]["arguments"]["project"], "shop-srv", "{said}");
    assert_eq!(said[1]["arguments"]["project"], "shop-srv", "{said}");
    assert!(said[2]["arguments"].get("project").is_none(), "{said}");

    // A step that names another project — one of this agent's own, or none there is — is refused
    // here, as a batch of this agent's own refuses it.
    for other in [common::project_id(&u.core), "Elsewhere".to_string()] {
        let steps = json!([step(json!("Shop")), step(json!(other))]);
        assert_eq!(
            u.refused(
                "batch",
                json!({"project": "Shop", "value": {"steps": steps}})
            ),
            "steps[1]（card_create）: project がこのバッチのプロジェクトと違います"
        );
    }
    assert_eq!(u.way.handed().len(), 1);
}

#[test]
fn a_name_at_an_owner_is_read_wherever_its_at_signs_are() {
    let u = user();
    // A member is called what its place is called, which may be `user@host`.
    state(
        &u.core,
        MAC.into(),
        true,
        json!([
            {"id": MAC, "name": "Mac", "online": true, "build": BUILD},
            {"id": SERVER, "name": "ops@server", "online": true, "build": BUILD},
            {"id": LAPTOP, "name": "c", "online": true, "build": BUILD},
        ]),
    );
    u.told(
        SERVER,
        json!([
            {"id": "shop-srv", "name": "Shop", "slug": "shop", "level": "read"},
            {"id": "a-srv", "name": "a", "slug": "a", "level": "read"},
        ]),
    );
    u.told(
        LAPTOP,
        json!([{"id": "ab-lap", "name": "a@ops@server", "slug": "a-ops-server", "level": "read"}]),
    );
    for project in ["shop-srv", "a-srv"] {
        u.grant(SERVER, project, json!("read"));
    }
    u.grant(LAPTOP, "ab-lap", json!("read"));

    u.ok("card_list", json!({"project": "Shop@ops@server"}));
    u.ok("card_list", json!({"project": "a@ops@server@c"}));
    let handed = u.way.handed();
    assert_eq!(handed.len(), 2);
    assert_eq!(handed[0].owner, SERVER);
    assert_eq!(handed[0].request["arguments"]["project"], "shop-srv");
    assert_eq!(handed[1].owner, LAPTOP);
    assert_eq!(handed[1].request["arguments"]["project"], "ab-lap");

    // What reads two ways names neither: each is named by its id at its owner's.
    assert_eq!(
        u.refused("card_list", json!({"project": "a@ops@server"})),
        "共有されたプロジェクト「a@ops@server」が 2 つあります。project には a-srv@srv-1 / ab-lap@lap-1 のどれかを書きます"
    );
    u.ok("card_list", json!({"project": "a-srv@srv-1"}));
    assert_eq!(u.way.handed()[2].request["arguments"]["project"], "a-srv");
}

/// The app of the side that uses a shared project: keeps what it was asked.
#[derive(Default)]
struct Window {
    asked: Mutex<Vec<Value>>,
}

impl App for Window {
    fn app_control(&self, arguments: &str, _caller: &str) -> String {
        self.asked
            .lock()
            .unwrap()
            .push(serde_json::from_str(arguments).unwrap());
        json!({"json": {"windows": []}}).to_string()
    }
}

#[test]
fn app_control_on_a_shared_project_is_judged_by_the_users_key_and_handed_on() {
    let u = user();
    let window = Arc::new(Window::default());
    u.core.set_app(window.clone());
    u.told_shop(SERVER, "shop-srv");
    u.grant(SERVER, "shop-srv", json!("read"));
    let permit = |state: &str| {
        done(
            &u.core,
            json!({"op": "access_key_permit", "key": u.key.0, "permits": {"app_control": state}}),
        );
    };
    let select = json!({"project": "Shop", "op": "select", "target": "Schemas:Order"});

    // The key's own item, judged here: the owner does not know this side's keys.
    permit("deny");
    let denied = u.refused("app_control", select.clone());
    assert!(denied.contains("拒否"), "{denied}");
    assert!(u.way.handed().is_empty());

    // Asked each time, and the call did not say to ask.
    permit("ask");
    let asked = u.refused("app_control", select.clone());
    assert!(asked.contains("request: true"), "{asked}");
    assert!(u.way.handed().is_empty());

    // Allowed: the window that shows the project is its owner's.
    permit("allow");
    assert_eq!(u.ok("app_control", select), from_owner());
    let handed = u.way.handed();
    assert_eq!(handed.len(), 1);
    assert_eq!(handed[0].request["tool"], "app_control");
    assert_eq!(
        handed[0].request["arguments"],
        json!({"project": "shop-srv", "op": "select", "target": "Schemas:Order"})
    );
    assert_eq!(handed[0].request["level"], "read");
    assert!(window.asked.lock().unwrap().is_empty());

    // The windows are this side's own.
    assert_eq!(
        u.ok("app_control", json!({"op": "windows"})),
        json!({"windows": []})
    );
    assert_eq!(window.asked.lock().unwrap().len(), 1);
    assert_eq!(u.way.handed().len(), 1);
}

// MARK: - The side that owns a shared project

/// The agent Server, linked to the hub, with Shop (shared with Mac to write), Notes (shared with
/// Mac to read) and Private (shared with nobody).
struct Owner {
    core: Core,
    shop: String,
    notes: String,
}

fn project_id(core: &Core, name: &str) -> String {
    rusqlite::Connection::open(core.place())
        .unwrap()
        .query_row("SELECT id FROM projects WHERE name = ?", [name], |r| {
            r.get(0)
        })
        .unwrap()
}

fn share(core: &Core, project: &str, member: &str, level: Value) -> Value {
    op(
        core,
        json!({"op": "hub_share_set", "project": project, "member": member, "level": level}),
    )
}

fn owner() -> Owner {
    let core = Core::open(common::script_store()).unwrap();
    for name in ["Shop", "Notes", "Private"] {
        let (refused, said) = common::call(&core, "project_create", json!({"project": name}));
        assert!(!refused, "{said}");
    }
    state(&core, SERVER.into(), true, members());
    let (shop, notes) = (project_id(&core, "Shop"), project_id(&core, "Notes"));
    assert_eq!(
        share(&core, &shop, MAC, json!("write")),
        json!({"result": {}})
    );
    assert_eq!(
        share(&core, &notes, MAC, json!("read")),
        json!({"result": {}})
    );
    Owner { core, shop, notes }
}

impl Owner {
    /// A call handed over the hub by `member`, for its key K that may do `level` there: what the
    /// tool answered, or its refusal as the tool said it.
    fn handed(
        &self,
        member: &str,
        tool: &str,
        arguments: Value,
        level: &str,
    ) -> Result<Value, String> {
        let request = json!({"k": "call", "tool": tool, "arguments": arguments, "key": "K", "level": level, "build": BUILD, "lang": "ja"});
        let result = done(
            &self.core,
            json!({"op": "hub_call", "member": member, "request": request}),
        );
        if let Some(why) = result.get("failed") {
            return Err(format!("failed: {}", why.as_str().unwrap()));
        }
        match result.get("error") {
            Some(error) => Err(error.as_str().unwrap().to_string()),
            None => {
                assert!(result.get("output").is_some(), "{result}");
                Ok(result["output"].clone())
            }
        }
    }

    fn turned_down(&self, tool: &str, arguments: Value) -> String {
        let shown = arguments.to_string();
        match self.handed(MAC, tool, arguments, "write") {
            Ok(said) => panic!("{tool} {shown} が通った: {said}"),
            Err(why) => why,
        }
    }

    /// An item of what MCP may write in Shop, allowed.
    fn allow(&self, which: &str, name: &str) {
        done(
            &self.core,
            json!({"op": "mcp_writes_set", "project": self.shop, which: name, "state": "allow"}),
        );
    }
}

#[test]
fn a_members_call_is_judged_by_the_share_and_the_level_sent_and_written_under_key_at_agent() {
    let o = owner();
    let thing = |project: &str| json!({"project": project, "target": "Schemas:Thing", "value": {"kind": "entity"}});

    // Shared to write, and the key may write: written, under the key at its agent.
    let made = o.handed(MAC, "card_create", thing(&o.shop), "write");
    assert!(made.is_ok(), "{made:?}");
    let commits = done(&o.core, json!({"op": "commits", "project": o.shop}));
    assert_eq!(commits["commits"][0]["authorKind"], "mcp", "{commits}");
    assert_eq!(commits["commits"][0]["author"], "K@Mac");

    // The lower of the two: a key that may only read, a share to read.
    let read_only = |name: &str| {
        format!(
            "プロジェクト「{name}」は、ほかのエージェントからは読むだけです（持ち主の共有か、使う側のアクセスキーの許可が読むだけ）"
        )
    };
    assert_eq!(
        o.handed(
            MAC,
            "card_create",
            json!({"project": o.shop, "target": "Schemas:Other", "value": {"kind": "entity"}}),
            "read"
        ),
        Err(read_only("Shop"))
    );
    assert_eq!(
        o.handed(MAC, "card_create", thing(&o.notes), "write"),
        Err(read_only("Notes"))
    );
    // Read, both go.
    assert!(
        o.handed(MAC, "card_list", json!({"project": o.shop}), "read")
            .is_ok()
    );
    assert!(
        o.handed(MAC, "card_list", json!({"project": o.notes}), "write")
            .is_ok()
    );

    // Not shared with it: not run, and nothing here named — neither the project nor the others
    // shared with that agent, which the key behind the call may not have been given. The side
    // that asked says it in its own names.
    let unshared = Err("failed: unshared".to_string());
    assert_eq!(
        o.handed(
            MAC,
            "card_list",
            json!({"project": project_id(&o.core, "Private")}),
            "write",
        ),
        unshared
    );
    // Shared with Mac is not shared with Laptop.
    assert_eq!(
        o.handed(LAPTOP, "card_list", json!({"project": o.shop}), "write"),
        unshared
    );
    // A project is named by its id, as the side that uses it hands it over: its names are that
    // side's to read.
    for name in ["Shop", "Private"] {
        assert_eq!(
            o.handed(MAC, "card_list", json!({"project": name}), "write"),
            unshared,
            "{name}"
        );
    }

    // The key behind the call is named as a key is: its name is written with what it writes.
    for key in ["", "   ", &"k".repeat(81)] {
        assert_eq!(
            turned_down(
                &o.core,
                json!({"op": "hub_call", "member": MAC, "request": {"k": "call", "tool": "card_list", "arguments": {"project": o.shop}, "key": key, "level": "read", "build": BUILD, "lang": "ja"}})
            ),
            "hub_call の request（{k: \"call\", tool, arguments, key, level, build, lang}）が読めません",
            "{key:?}"
        );
    }

    // Only a member of the hub; and what is made is made at one's own agent.
    assert_eq!(
        turned_down(
            &o.core,
            json!({"op": "hub_call", "member": "ghost", "request": {"k": "call", "tool": "card_list", "arguments": {"project": o.shop}, "key": "K", "level": "read", "build": BUILD, "lang": "ja"}})
        ),
        "エージェント「ghost」はハブの名簿にありません"
    );
    for tool in ["project_create", "project_list"] {
        let said = o.turned_down(tool, json!({"project": "New"}));
        assert!(said.contains("ハブ越しにはできません"), "{tool}: {said}");
    }
    // Nor is the owner's window asked about with no project named, or its windows listed: they
    // show whatever is open there.
    for (tool, arguments) in [
        ("app_control", json!({"op": "state"})),
        ("app_control", json!({"op": "windows"})),
        ("app_control", json!({"project": o.shop, "op": "windows"})),
        ("guide", json!({"about": "entity"})),
    ] {
        let shown = arguments.to_string();
        let said = o.turned_down(tool, arguments);
        assert!(
            said.contains("ハブ越しにはできません"),
            "{tool} {shown}: {said}"
        );
    }
}

#[test]
fn a_members_call_does_not_touch_the_owners_folders_alone_or_in_a_batch() {
    let o = owner();
    o.allow("group", "export");
    o.allow("item", "section_update.folder");
    let folder = common::place("hub-folder");
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.to_str().unwrap();
    let over_hub = |what: &str| {
        format!(
            "{what} は、ハブ越しにはできません（持ち主のホストのフォルダに触ります。持ち主のエージェントに繋いだセッションでします）"
        )
    };
    let entries = json!([{"from": "Schemas", "to": "docs/schemas.md"}]);

    // Alone.
    assert_eq!(
        o.turned_down(
            "export_execute",
            json!({"project": o.shop, "targetdir": path})
        ),
        over_hub("export_execute")
    );
    assert_eq!(
        o.turned_down(
            "export_execute",
            json!({"project": o.shop, "targetdir": "[default]"})
        ),
        over_hub("export_execute")
    );
    assert_eq!(
        o.turned_down(
            "section_update",
            json!({"project": o.shop, "target": "Schemas", "value": {"folder": path}})
        ),
        over_hub("section_update の folder")
    );
    assert_eq!(
        o.turned_down(
            "export_create",
            json!({"project": o.shop, "target": "docs", "value": {"path": path, "entries": entries}})
        ),
        over_hub("export_create の path")
    );
    assert_eq!(
        o.turned_down(
            "export_update",
            json!({"project": o.shop, "target": "docs", "value": {"path": path}})
        ),
        over_hub("export_update の path")
    );
    assert_eq!(
        o.turned_down("export_read", json!({"project": o.shop, "targetdir": path})),
        over_hub("export_read の targetdir")
    );

    // In a batch's steps, tried and run.
    for execute in [false, true] {
        let said = o.turned_down(
            "batch",
            json!({"project": o.shop, "execute": execute, "value": {"steps": [
                {"tool": "card_create", "arguments": {"target": "Schemas:Thing", "value": {"kind": "entity"}}},
                {"tool": "section_update", "arguments": {"target": "Schemas", "value": {"folder": path}}},
            ]}}),
        );
        assert!(
            said.contains(&over_hub("section_update の folder")),
            "execute {execute}: {said}"
        );
    }
    let cards = o
        .handed(MAC, "card_list", json!({"project": o.shop}), "read")
        .unwrap();
    assert!(!cards.to_string().contains("Thing"), "{cards}");

    // Nothing was written or made there.
    assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 0);

    // An export with no folder of its own, and its files compared in the folders it has by default,
    // touch no folder the member chose.
    let made = o.handed(
        MAC,
        "export_create",
        json!({"project": o.shop, "target": "docs", "value": {"entries": entries}}),
        "write",
    );
    assert!(made.is_ok(), "{made:?}");
    let read = o.handed(
        MAC,
        "export_read",
        json!({"project": o.shop, "targetdir": "[default]"}),
        "read",
    );
    assert!(read.is_ok(), "{read:?}");
}

#[test]
fn what_is_public_at_the_owner_is_not_a_members() {
    let o = owner();
    let (refused, said) = common::call(&o.core, "project_create", json!({"project": "Open"}));
    assert!(!refused, "{said}");
    let open = project_id(&o.core, "Open");
    done(
        &o.core,
        json!({"op": "project_public_set", "project": open, "on": true}),
    );
    // Public is for the keys of the agent that holds it: to a member it is not shared.
    assert_eq!(
        o.handed(MAC, "card_list", json!({"project": open}), "read"),
        Err("failed: unshared".to_string())
    );
}

// MARK: - Both sides' lists

/// How many rows each of the hub's tables holds: members, shares, shared projects, grants.
fn rows(core: &Core) -> [i64; 4] {
    let conn = rusqlite::Connection::open(core.place()).unwrap();
    ["hub_members", "hub_shares", "hub_projects", "hub_grants"].map(|table| {
        conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    })
}

#[test]
fn a_member_gone_from_the_hub_loses_its_shares_and_the_grants_to_its_projects() {
    let u = user();
    let own = common::project_id(&u.core);
    let set = || {
        state(&u.core, MAC.into(), true, members());
        assert_eq!(
            share(&u.core, &own, LAPTOP, json!("write")),
            json!({"result": {}})
        );
        u.told_shop(SERVER, "shop-srv");
        u.grant(SERVER, "shop-srv", json!("read"));
    };
    set();
    assert_eq!(rows(&u.core), [3, 1, 1, 1]);

    // Not linked: nothing is known of who is a member now, so nothing goes.
    state(&u.core, MAC.into(), false, json!([]));
    assert_eq!(rows(&u.core), [3, 1, 1, 1]);
    let view = done(&u.core, json!({"op": "hub_shares"}));
    assert_eq!(
        view["shares"],
        json!([{"project": own, "name": common::PROJECT, "to": [{"member": LAPTOP, "name": "Laptop", "level": "write"}]}])
    );
    assert_eq!(
        view["shared"],
        json!([{"owner": SERVER, "ownerName": "Server", "project": "shop-srv", "name": "Shop", "level": "write", "reachable": false, "why": "ハブに繋がっていません"}])
    );
    assert_eq!(
        u.key_row()["hubGrants"],
        json!([{"owner": SERVER, "project": "shop-srv", "level": "read"}])
    );

    // Linked, and the two are members no more: the share to the one, the projects of the other and
    // the grants to them go.
    state(
        &u.core,
        MAC.into(),
        true,
        json!([{"id": MAC, "name": "Mac", "online": true, "build": BUILD}]),
    );
    assert_eq!(rows(&u.core), [1, 0, 0, 0]);
    let view = done(&u.core, json!({"op": "hub_shares"}));
    assert_eq!(view["shares"], json!([]));
    assert_eq!(view["shared"], json!([]));
    assert_eq!(u.key_row()["hubGrants"], json!([]));

    // A project the owner shares no more goes with its grants; the ones it still shares stay.
    set();
    u.told(
        SERVER,
        json!([
            {"id": "shop-srv", "name": "Shop", "slug": "shop", "level": "write"},
            {"id": "notes-srv", "name": "Notes", "slug": "notes", "level": "read"},
        ]),
    );
    u.grant(SERVER, "notes-srv", json!("read"));
    assert_eq!(rows(&u.core), [3, 1, 2, 2]);
    u.told(
        SERVER,
        json!([{"id": "notes-srv", "name": "Notes renamed", "slug": "notes", "level": "read"}]),
    );
    assert_eq!(rows(&u.core), [3, 1, 1, 1]);
    assert_eq!(
        u.key_row()["hubGrants"],
        json!([{"owner": SERVER, "project": "notes-srv", "level": "read"}])
    );
    // A revoked key's grants go with it.
    done(&u.core, json!({"op": "access_key_revoke", "key": u.key.0}));
    assert_eq!(rows(&u.core), [3, 1, 1, 0]);

    // Left the hub: nothing of it is kept.
    state(&u.core, Value::Null, false, json!([]));
    assert_eq!(rows(&u.core), [0, 0, 0, 0]);
}

#[test]
fn shares_are_set_and_listed_for_the_screen_and_no_tool_writes_them() {
    let o = owner();
    assert_eq!(
        share(&o.core, &o.shop, LAPTOP, json!("read")),
        json!({"result": {}})
    );
    assert_eq!(
        share(&o.core, &o.shop, LAPTOP, Value::Null),
        json!({"result": {}})
    );
    let view = done(&o.core, json!({"op": "hub_shares"}));
    // Who a project can be shared with: the other members.
    assert_eq!(
        view["members"],
        json!([
            {"id": MAC, "name": "Mac", "online": true, "build": BUILD},
            {"id": LAPTOP, "name": "Laptop", "online": true, "build": BUILD},
        ])
    );
    assert_eq!(
        view["shares"],
        json!([
            {"project": o.notes, "name": "Notes", "to": [{"member": MAC, "name": "Mac", "level": "read"}]},
            {"project": o.shop, "name": "Shop", "to": [{"member": MAC, "name": "Mac", "level": "write"}]},
        ])
    );
    assert_eq!(view["shared"], json!([]));

    // What each member is told: every other member, with nothing for one shared nothing.
    assert_eq!(
        done(&o.core, json!({"op": "hub_outgoing"})),
        json!({
            MAC: [
                {"id": o.notes, "name": "Notes", "slug": "notes", "level": "read"},
                {"id": o.shop, "name": "Shop", "slug": "shop", "level": "write"},
            ],
            LAPTOP: [],
        })
    );

    // Not to one that is no member, not to oneself, not a project that is not there, not a level
    // that is none.
    assert_eq!(
        share(&o.core, &o.shop, "ghost", json!("read"))["error"],
        "エージェント「ghost」はハブの名簿にありません"
    );
    assert_eq!(
        share(&o.core, &o.shop, SERVER, json!("read"))["error"],
        "自分のエージェントには共有しません"
    );
    assert_eq!(
        share(&o.core, "nowhere", MAC, json!("read"))["error"],
        "プロジェクトが見つかりません"
    );
    assert_eq!(
        share(&o.core, &o.shop, MAC, json!("all"))["error"],
        "level は read か write です（\"all\" は読めません）"
    );

    // No tool of MCP's writes a share, or a grant.
    let listed = op(
        &o.core,
        json!({"op": "mcp", "message": {"jsonrpc": "2.0", "id": 1, "method": "tools/list"}, "headers": common::headers(&o.core)}),
    );
    let tools = listed["result"]["body"]["result"]["tools"]
        .as_array()
        .unwrap();
    assert!(!tools.is_empty());
    for tool in tools {
        let name = tool["name"].as_str().unwrap();
        assert!(!name.contains("hub") && !name.contains("share"), "{name}");
    }
    let unknown = op(
        &o.core,
        json!({"op": "mcp", "message": {"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "hub_share_set", "arguments": {"project": o.shop, "member": LAPTOP, "level": "write"}}}, "headers": common::headers(&o.core)}),
    );
    assert_eq!(
        unknown["result"]["body"]["error"]["code"], -32602,
        "{unknown}"
    );

    // A project read in from its file in its place keeps its shares; one deleted takes them along.
    let text = done(&o.core, json!({"op": "project_file", "project": o.shop}))["text"].clone();
    done(&o.core, json!({"op": "project_file_read", "text": text}));
    assert_eq!(rows(&o.core)[1], 2);
    done(&o.core, json!({"op": "project_delete", "project": o.shop}));
    let view = done(&o.core, json!({"op": "hub_shares"}));
    assert_eq!(
        view["shares"],
        json!([{"project": o.notes, "name": "Notes", "to": [{"member": MAC, "name": "Mac", "level": "read"}]}])
    );
}
