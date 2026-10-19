//! Card ghosts as shortcuts: a card of the kind `ghost` a section holds, with a name of
//! its own and what it points at by id — a card of another section, or another ghost. A
//! path whose head is a ghost goes on in the card its chain leads to, and is kept as it
//! was written (the ghost's path, the ghost's id); a rename or a move of the card, of what
//! is in it or of the ghost is followed; a card removed leaves the ghost pointing nowhere.
//! The checks that need a kind look at the card it leads to, and what is written out names
//! the card. Over the sample project (`見本`): Schemas holds Customer (entity), Status
//! (enum), Orders (store) and Rule (note); Schemas/Ordering holds Order (entity) and Line
//! (value); Docs holds Overview (freedoc) and Spec (formatdoc).

use crate::common;

use common::{call, checked, line_edits, ok, op, project_id, refused, script_store};
use serde_json::{Value, json};
use specboard_core::Core;

const PROJECT: &str = "見本";

fn read(core: &Core, target: &str) -> Value {
    ok(core, "card_read", json!({"target": target}))
}

/// A card's lines (`connections`) as it writes them, each changed by `change` (and those it
/// gives back after): what it holds now.
fn lines(core: &Core, card: &str, change: impl Fn(&mut Vec<Value>)) -> Value {
    let before = read(core, card)["connections"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mut lines = before.clone();
    change(&mut lines);
    ok(
        core,
        "card_update",
        json!({"target": card, "value": {"connections": line_edits(&before, &lines)}}),
    )
}

/// A field made in `card` that embeds `to` (an embedding is a line of its card).
fn embedding(core: &Core, card: &str, field: &str, to: &str) {
    ok(
        core,
        "card_create",
        json!({"target": format!("{card}.{field}"), "value": {"type": "string"}}),
    );
    let (field, to) = (field.to_string(), to.to_string());
    lines(core, card, move |l| {
        l.push(json!({"line": "embed", "from": field, "to": to}))
    });
}

/// A ghost made: `at` its path, `to` what it points at.
fn ghost(core: &Core, at: &str, to: &str) -> Value {
    ok(
        core,
        "card_create",
        json!({"target": at, "value": {"kind": "ghost", "target": to}}),
    )
}

/// The project's broken paths, each as `at via path: hint`, less those the script's project has
/// from the start (its Docs:Overview names what is not there, on purpose).
fn broken(core: &Core) -> Vec<String> {
    all_broken(core)
        .into_iter()
        .filter(|b| !b.contains("Nope:") && !b.contains("Schemas:Nothing"))
        .collect()
}

fn all_broken(core: &Core) -> Vec<String> {
    checked(core, "main")["brokenPaths"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|b| {
            format!(
                "{} {} {}: {}",
                b["at"].as_str().unwrap(),
                b["via"].as_str().unwrap(),
                b["path"].as_str().unwrap(),
                b["hint"].as_str().unwrap_or("")
            )
        })
        .collect()
}

fn read_project(core: &Core) -> Value {
    ok(core, "project_read", json!({}))
}

/// The whole project as the window reads it.
fn detail(core: &Core, branch: &str) -> Value {
    op(
        core,
        json!({"op": "project_detail", "project": project_id(core), "branch": branch}),
    )["result"]
        .clone()
}

/// The last commit's lines in a tree (`""` for main).
fn last_lines(core: &Core, branch: &str) -> Vec<String> {
    let read = op(
        core,
        json!({"op": "commits", "project": project_id(core), "branch": branch}),
    );
    read["result"]["commits"][0]["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["summary"].as_str().unwrap().to_string())
        .collect()
}

fn commit_count(core: &Core, branch: &str) -> usize {
    op(
        core,
        json!({"op": "commits", "project": project_id(core), "branch": branch}),
    )["result"]["commits"]
        .as_array()
        .unwrap()
        .len()
}

// MARK: - Made, named, read

#[test]
fn a_ghost_is_a_card_of_its_section_named_as_it_says_and_read_as_one() {
    let core = Core::open(script_store()).unwrap();
    let project_list = || -> Value {
        let (_, text) = call(&core, "project_list", json!({}));
        serde_json::from_str::<Value>(&text).unwrap()["projects"][0]["cardCount"].clone()
    };
    let cards_before = project_list();
    assert!(cards_before.is_number(), "{cards_before}");
    let made = ghost(&core, "Schemas/Ordering:Customer", "Schemas:Customer");
    assert_eq!(made["created"][0]["path"], "Schemas/Ordering:Customer");
    assert_eq!(made["created"][0]["kind"], "ghost");
    assert_eq!(
        last_lines(&core, ""),
        ["カードゴースト Schemas/Ordering:Customer を作成（Schemas:Customer を指す）"]
    );

    let read = read(&core, "Schemas/Ordering:Customer");
    assert_eq!(read["path"], "Schemas/Ordering:Customer");
    assert_eq!(read["kind"], "ghost");
    assert_eq!(read["target"], "Schemas:Customer");
    assert_eq!(read["resolvesTo"], "Schemas:Customer");
    assert_eq!(read["targetKind"], "entity");
    // It has a place on its section's diagram, as a card has.
    assert!(read["topLeft"].is_object(), "{read}");

    // Named among its section's cards, after them; counted apart.
    let section = ok(&core, "section_read", json!({"target": "Schemas/Ordering"}));
    assert_eq!(
        section["cards"],
        json!([
            "Schemas/Ordering:Order",
            "Schemas/Ordering:Line",
            "Schemas/Ordering:Customer"
        ])
    );
    assert_eq!(section["cardCount"], 2);
    assert_eq!(section["ghostCount"], 1);
    assert!(section.get("cardGhosts").is_none());
    let listed = ok(
        &core,
        "card_list",
        json!({"value": {"section": "Schemas/Ordering"}}),
    );
    let kinds: Vec<(&str, &str)> = listed["cards"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| (c["path"].as_str().unwrap(), c["kind"].as_str().unwrap()))
        .collect();
    assert_eq!(
        kinds,
        [
            ("Schemas/Ordering:Order", "entity"),
            ("Schemas/Ordering:Line", "value"),
            ("Schemas/Ordering:Customer", "ghost"),
        ]
    );
    let only = ok(&core, "card_list", json!({"value": {"kind": "ghost"}}));
    assert_eq!(only["matched"], 1);
    // A box of its section's diagram, as a card of it is: put, and read back.
    ok(
        &core,
        "section_update",
        json!({"target": "Schemas/Ordering", "value": {"places": {
            "Schemas/Ordering:Customer": {"topLeft": {"x": 200, "y": 30}},
        }}}),
    );
    let boxes = ok(
        &core,
        "section_read",
        json!({"target": "Schemas/Ordering", "value": {"include": ["places"]}}),
    )["places"]
        .clone();
    let own = boxes
        .get("Schemas/Ordering:Customer")
        .unwrap_or_else(|| panic!("{boxes}"));
    assert_eq!(own["topLeft"], json!({"x": 200, "y": 30}));
    // A section holding one is not empty.
    ok(
        &core,
        "section_create",
        json!({"target": "Schemas/Ordering/Spare"}),
    );
    ghost(&core, "Schemas/Ordering/Spare:Who", "Schemas:Customer");
    let said = refused(
        &core,
        "section_delete",
        json!({"target": "Schemas/Ordering/Spare"}),
    );
    assert!(said.contains("カードゴーストが 1 件あります"), "{said}");
    ok(
        &core,
        "card_delete",
        json!({"target": "Schemas/Ordering/Spare:Who"}),
    );
    ok(
        &core,
        "section_delete",
        json!({"target": "Schemas/Ordering/Spare"}),
    );
    // Not among the project's cards, nor what is counted of them.
    assert_eq!(project_list(), cards_before);
    assert_eq!(read_project(&core)["counts"]["ghost"], 1);

    // The window reads it apart from the cards; no section lists card ghosts.
    let whole = detail(&core, "");
    assert!(
        whole["schemas"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["kind"] != "ghost")
    );
    assert!(
        whole["sections"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s.get("ghosts").is_none())
    );
    let ghosts = whole["ghosts"].as_array().unwrap();
    assert_eq!(ghosts.len(), 1);
    let customer = whole["schemas"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "Customer")
        .unwrap();
    assert_eq!(ghosts[0]["name"], "Customer");
    assert_eq!(ghosts[0]["kind"], "ghost");
    assert_eq!(ghosts[0]["target"], customer["id"]);
    assert_eq!(ghosts[0]["resolvesTo"], customer["id"]);
    assert!(ghosts[0]["posX"].is_number());
}

#[test]
fn a_ghost_points_out_of_its_section_and_never_round() {
    let core = Core::open(script_store()).unwrap();
    // Not at what is in its own section (named there by its name already), nor itself.
    let said = refused(
        &core,
        "card_create",
        json!({"target": "Schemas/Ordering:Again", "value": {"kind": "ghost", "target": "Order"}}),
    );
    assert!(
        said.contains("同じセクションにあるので指せません"),
        "{said}"
    );
    // Nor in the root.
    let said = refused(
        &core,
        "card_create",
        json!({"target": "Main:Root", "value": {"kind": "ghost", "target": "Schemas:Customer"}}),
    );
    assert!(
        said.contains("Main は根なので、カードゴーストは置けません"),
        "{said}"
    );
    // Nor at what is not there.
    let said = refused(
        &core,
        "card_create",
        json!({"target": "Docs:Nobody", "value": {"kind": "ghost", "target": "Schemas:Nobody"}}),
    );
    assert!(
        said.contains("「Schemas:Nobody」というカードはありません"),
        "{said}"
    );

    // A chain: Free:Cust → Schemas/Ordering:Customer → Schemas:Customer.
    ghost(&core, "Schemas/Ordering:Customer", "Schemas:Customer");
    ghost(&core, "Free:Cust", "Schemas/Ordering:Customer");
    assert_eq!(read(&core, "Free:Cust")["resolvesTo"], "Schemas:Customer");
    // Turned back on itself, it would go round.
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Schemas/Ordering:Customer", "value": {"target": "Free:Cust"}}),
    );
    assert!(said.contains("自分に戻る"), "{said}");
    assert!(
        said.contains("Schemas/Ordering:Customer → Free:Cust → Schemas/Ordering:Customer"),
        "{said}"
    );
    // Two made in one call, pointing at each other.
    let said = refused(
        &core,
        "card_create",
        json!({"value": {
            "Docs:A": {"kind": "ghost", "target": "Free:B"},
            "Free:B": {"kind": "ghost", "target": "Docs:A"},
        }}),
    );
    assert!(said.contains("自分に戻る"), "{said}");
    assert!(
        read_project(&core)["counts"]["ghost"] == 2,
        "nothing of the refused call is kept"
    );
}

#[test]
fn names_do_not_clash_in_a_section_ghosts_and_cards_alike() {
    let core = Core::open(script_store()).unwrap();
    // A ghost named as a card there.
    let said = refused(
        &core,
        "card_create",
        json!({"target": "Schemas/Ordering:Line", "value": {"kind": "ghost", "target": "Schemas:Customer"}}),
    );
    assert!(
        said.starts_with("Schemas/Ordering:Line: カード「Line」は既にあります"),
        "{said}"
    );
    ghost(&core, "Schemas/Ordering:Customer", "Schemas:Customer");
    // A card made, renamed or moved where a ghost has the name.
    let said = refused(
        &core,
        "card_create",
        json!({"target": "Schemas/Ordering:Customer", "value": {"kind": "value"}}),
    );
    assert!(
        said.starts_with("Schemas/Ordering:Customer: カード「Customer」は既にあります"),
        "{said}"
    );
    assert_eq!(
        refused(
            &core,
            "card_update",
            json!({"target": "Schemas/Ordering:Line", "value": {"name": "Customer"}})
        ),
        "カード「Customer」は既にあります"
    );
    ok(
        &core,
        "card_create",
        json!({"target": "Schemas:Billing", "value": {"kind": "entity"}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Billing", "value": {"name": "Customer2"}}),
    );
    ghost(&core, "Schemas/Ordering:Customer2", "Schemas:Status");
    assert_eq!(
        refused(
            &core,
            "card_update",
            json!({"target": "Schemas:Customer2", "value": {"section": "Schemas/Ordering"}})
        ),
        "セクション Schemas/Ordering にはもう「Customer2」があります"
    );
    // A ghost renamed to a card's name, or moved where one has its name.
    assert_eq!(
        refused(
            &core,
            "card_update",
            json!({"target": "Schemas/Ordering:Customer", "value": {"name": "Order"}})
        ),
        "カード「Order」は既にあります"
    );
    ghost(&core, "Docs:Customer", "Schemas:Customer");
    assert_eq!(
        refused(
            &core,
            "card_update",
            json!({"target": "Docs:Customer", "value": {"section": "Schemas/Ordering"}})
        ),
        "セクション Schemas/Ordering にはもう「Customer」があります"
    );
    // Moved where what it points at is: refused, it would point into its own section.
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Docs:Customer", "value": {"section": "Schemas"}}),
    );
    assert!(
        said.contains("同じセクションにあるので指せません"),
        "{said}"
    );
}

#[test]
fn a_ghost_is_renamed_and_turned_to_a_card_of_the_same_kind() {
    let core = Core::open(script_store()).unwrap();
    ghost(&core, "Docs:Who", "Schemas:Customer");
    ok(
        &core,
        "card_create",
        json!({"target": "Schemas:Person", "value": {"kind": "entity", "fields": [{"name": "name", "type": "string"}]}}),
    );
    // Turned: the name stays.
    let turned = ok(
        &core,
        "card_update",
        json!({"target": "Docs:Who", "value": {"target": "Schemas:Person"}}),
    );
    assert_eq!(turned["path"], "Docs:Who");
    assert_eq!(turned["target"], "Schemas:Person");
    assert_eq!(
        last_lines(&core, ""),
        ["カードゴースト Docs:Who を更新（指す先を Schemas:Person に）"]
    );
    // Only to what leads to a card of the kind it leads to now.
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Docs:Who", "value": {"target": "Schemas/Ordering:Line"}}),
    );
    assert_eq!(
        said,
        "付け替えられるのは、今と同じ種類（entity）のカードに着く先だけです（Schemas/Ordering:Line は value）"
    );
    // Renamed.
    let renamed = ok(
        &core,
        "card_update",
        json!({"target": "Docs:Who", "value": {"name": "Client"}}),
    );
    assert_eq!(renamed["path"], "Docs:Client");
    assert_eq!(
        last_lines(&core, ""),
        ["カードゴースト Docs:Who を更新（名前を Client に）"]
    );
    // What a card has of its own, a ghost has not.
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Docs:Client", "value": {"description": "x"}}),
    );
    assert!(
        said.contains("カードゴーストで、description は持ちません"),
        "{said}"
    );
    assert!(said.contains("Schemas:Person"), "{said}");
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Docs:Client", "value": {"kind": "entity"}}),
    );
    assert!(said.contains("ほかの種類にはできません"), "{said}");
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Schemas:Person", "value": {"kind": "ghost"}}),
    );
    assert!(said.contains("カードゴーストにはできません"), "{said}");
    // Where it sits, and its place among the section's ghosts.
    ok(
        &core,
        "card_update",
        json!({"target": "Docs:Client", "value": {"topLeft": {"x": 100, "y": 50}}}),
    );
    let placed = ok(
        &core,
        "card_read",
        json!({"target": "Docs:Client", "value": {"include": ["at"]}}),
    );
    assert_eq!(placed["topLeft"], json!({"x": 100, "y": 50}));
    ghost(&core, "Docs:First", "Schemas:Status");
    ok(
        &core,
        "card_update",
        json!({"target": "Docs:First", "value": {"position": 0}}),
    );
    let cards = ok(&core, "section_read", json!({"target": "Docs"}))["cards"].clone();
    assert_eq!(
        cards,
        json!(["Docs:Spec", "Docs:Overview", "Docs:First", "Docs:Client"])
    );
}

// MARK: - Paths through a ghost

#[test]
fn a_path_through_a_ghost_goes_on_in_its_card_and_is_kept_as_written() {
    let core = Core::open(script_store()).unwrap();
    ghost(&core, "Schemas/Ordering:Customer", "Schemas:Customer");
    // In its section by its name alone; elsewhere with its section; past it into the card.
    ok(
        &core,
        "card_create",
        json!({"target": "Schemas/Ordering:Note", "value": {"kind": "note", "body": "[[Customer]] の [[Customer.name]]"}}),
    );
    ok(
        &core,
        "card_create",
        json!({"target": "Free:Note", "value": {"kind": "note", "body": "[[Schemas/Ordering:Customer.id]]"}}),
    );
    assert_eq!(
        read(&core, "Schemas/Ordering:Note")["body"],
        "[[Schemas/Ordering:Customer]] の [[Schemas/Ordering:Customer.name]]"
    );
    assert_eq!(
        read(&core, "Free:Note")["body"],
        "[[Schemas/Ordering:Customer.id]]"
    );
    assert!(broken(&core).is_empty(), "{:?}", broken(&core));

    // A line holds the ghost by id; the checks look at the card it leads to.
    embedding(&core, "Schemas/Ordering:Order", "buyer", "Customer");
    let field = read(&core, "Schemas/Ordering:Order.buyer");
    assert_eq!(field["type"], "ref");
    lines(&core, "Schemas/Ordering:Order", |l| {
        for line in l.iter_mut().filter(|c| c["from"] == "customer_id") {
            line["to"] = "Customer".into();
        }
    });
    let order = read(&core, "Schemas/Ordering:Order");
    let to = |from: &str| {
        order["connections"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["from"] == from)
            .unwrap()["to"]
            .clone()
    };
    assert_eq!(to("buyer"), "Schemas/Ordering:Customer");
    assert_eq!(to("customer_id"), "Schemas/Ordering:Customer");
    // What points at the card through it is counted on the card.
    let referred = read(&core, "Schemas:Customer")["referencedBy"].clone();
    assert!(
        referred
            .as_array()
            .unwrap()
            .contains(&json!("Schemas/Ordering:Order.buyer")),
        "{referred}"
    );
    // The window's relations land on the card, saying the ghost.
    let whole = detail(&core, "");
    let ghost_id = whole["ghosts"][0]["id"].clone();
    let customer = whole["schemas"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "Customer")
        .unwrap()["id"]
        .clone();
    let through: Vec<&Value> = whole["relations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["viaGhostId"] == ghost_id)
        .collect();
    let (said, lines): (Vec<&Value>, Vec<&Value>) =
        through.iter().partition(|r| r["line"] == "mentions");
    assert_eq!(lines.len(), 2, "{through:?}");
    assert!(through.iter().all(|r| r["toSchemaId"] == customer));
    // What a note's text names through it is a line too, saying the path as written.
    assert!(!said.is_empty(), "{through:?}");
    assert!(
        said.iter().all(|r| r["written"]
            .as_str()
            .unwrap()
            .starts_with("Schemas/Ordering:Customer")),
        "{said:?}"
    );

    // Read and written through it, the answer says it as it was written.
    let name = read(&core, "Schemas/Ordering:Customer.name");
    assert_eq!(name["path"], "Schemas/Ordering:Customer.name");
    assert_eq!(name["form"], "field");
    assert_eq!(name["name"], "name");
    let changed = ok(
        &core,
        "card_update",
        json!({"target": "Schemas/Ordering:Customer.name", "value": {"label": "氏名"}}),
    );
    assert_eq!(changed["path"], "Schemas/Ordering:Customer.name");
    assert_eq!(read(&core, "Schemas:Customer.name")["label"], "氏名");
    let made = ok(
        &core,
        "card_create",
        json!({"target": "Schemas/Ordering:Customer.email", "value": {"type": "email"}}),
    );
    assert_eq!(made["path"], "Schemas/Ordering:Customer.email");
    assert!(read(&core, "Schemas:Customer.email")["type"] == "email");
    let gone = ok(
        &core,
        "card_delete",
        json!({"target": "Schemas/Ordering:Customer.email"}),
    );
    assert_eq!(gone["deleted"], "Schemas/Ordering:Customer.email");

    // A ghost of a ghost follows when the one it points at is turned.
    ghost(&core, "Free:Cust", "Schemas/Ordering:Customer");
    ok(
        &core,
        "card_create",
        json!({"target": "Schemas:Person", "value": {"kind": "entity"}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas/Ordering:Customer", "value": {"target": "Schemas:Person"}}),
    );
    let chained = read(&core, "Free:Cust");
    assert_eq!(chained["target"], "Schemas/Ordering:Customer");
    assert_eq!(chained["resolvesTo"], "Schemas:Person");
    // What went through it now lands where it leads, as it was written; a path into what
    // the card it led to had and the new one has not is broken, and said so.
    assert_eq!(
        read(&core, "Schemas/Ordering:Note")["body"],
        "[[Schemas/Ordering:Customer]] の [[Schemas/Ordering:Customer.name]]"
    );
    let now = broken(&core);
    assert!(
        now.iter()
            .any(|b| b.starts_with("Schemas/Ordering:Note body Schemas/Ordering:Customer.name")),
        "{now:?}"
    );
}

#[test]
fn renames_and_moves_are_followed_through_a_ghost() {
    let core = Core::open(script_store()).unwrap();
    ghost(&core, "Schemas/Ordering:Customer", "Schemas:Customer");
    ok(
        &core,
        "card_create",
        json!({"target": "Free:Note", "value": {"kind": "note", "body": "[[Schemas/Ordering:Customer.name]] と [[Schemas:Customer.name]]"}}),
    );
    // The card's field renamed: through the ghost too.
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer.name", "value": {"name": "full_name"}}),
    );
    assert_eq!(
        read(&core, "Free:Note")["body"],
        "[[Schemas/Ordering:Customer.full_name]] と [[Schemas:Customer.full_name]]"
    );
    // The card renamed: what names it does; what goes through the ghost keeps the ghost's.
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"name": "Person"}}),
    );
    assert_eq!(
        read(&core, "Free:Note")["body"],
        "[[Schemas/Ordering:Customer.full_name]] と [[Schemas:Person.full_name]]"
    );
    assert_eq!(
        read(&core, "Schemas/Ordering:Customer")["target"],
        "Schemas:Person"
    );
    // The ghost renamed, then moved: what goes through it follows.
    let renamed = ok(
        &core,
        "card_update",
        json!({"target": "Schemas/Ordering:Customer", "value": {"name": "Client"}}),
    );
    assert_eq!(
        renamed["warnings"],
        json!(["名前に合わせて、プロジェクトに書かれたパス 1 か所を Client に書き換えました"])
    );
    assert_eq!(
        read(&core, "Free:Note")["body"],
        "[[Schemas/Ordering:Client.full_name]] と [[Schemas:Person.full_name]]"
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas/Ordering:Client", "value": {"section": "Docs"}}),
    );
    assert_eq!(
        read(&core, "Free:Note")["body"],
        "[[Docs:Client.full_name]] と [[Schemas:Person.full_name]]"
    );
    // A section renamed: the ghost's path with it.
    ok(
        &core,
        "section_update",
        json!({"target": "Docs", "value": {"name": "Papers"}}),
    );
    assert_eq!(
        read(&core, "Free:Note")["body"],
        "[[Papers:Client.full_name]] と [[Schemas:Person.full_name]]"
    );
    assert!(broken(&core).is_empty(), "{:?}", broken(&core));
}

#[test]
fn what_a_ghost_points_at_is_not_moved_into_the_ghosts_section() {
    let core = Core::open(script_store()).unwrap();
    ghost(&core, "Free:Cust", "Schemas:Customer");
    // The card moved where a ghost that points at it is: it would point into its section.
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"section": "Free"}}),
    );
    assert_eq!(
        said,
        "Schemas:Customer をセクション Free へ移すと、それを指すカードゴースト Free:Cust が自分のセクションのものを指すことになります。カードゴーストを消すか付け替えてから移してください"
    );
    // A ghost moved there that another ghost points at, the same.
    ghost(&core, "Docs:Via", "Schemas:Status");
    ghost(&core, "Free:Chain", "Docs:Via");
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Docs:Via", "value": {"section": "Free"}}),
    );
    assert!(
        said.contains(
            "Docs:Via をセクション Free へ移すと、それを指すカードゴースト Free:Chain が"
        ),
        "{said}"
    );
    // A ghost and what it points at, moved together to one section.
    ghost(&core, "Docs:Who", "Schemas:Customer");
    ok(&core, "section_create", json!({"target": "Schemas/Other"}));
    let said = refused(
        &core,
        "card_update",
        json!({"value": {
            "Docs:Who": {"section": "Schemas/Other"},
            "Schemas:Customer": {"section": "Schemas/Other"},
        }}),
    );
    assert!(said.contains("カードゴースト Docs:Who"), "{said}");
    assert!(
        said.contains("カードゴーストを消すか付け替えてから移してください"),
        "{said}"
    );
    // Nothing of them moved; elsewhere, the card moves and its ghosts follow it.
    assert_eq!(read(&core, "Docs:Who")["resolvesTo"], "Schemas:Customer");
    ok(&core, "section_create", json!({"target": "Schemas/People"}));
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"section": "Schemas/People"}}),
    );
    assert_eq!(
        read(&core, "Free:Cust")["resolvesTo"],
        "Schemas/People:Customer"
    );
    assert_eq!(read(&core, "Docs:Who")["target"], "Schemas/People:Customer");
    // Once the ghost is let go, the card may go there.
    ok(&core, "card_delete", json!({"target": "Free:Cust"}));
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas/People:Customer", "value": {"section": "Free"}}),
    );
}

// MARK: - Gone

#[test]
fn a_card_removed_leaves_its_ghosts_pointing_nowhere() {
    let core = Core::open(script_store()).unwrap();
    ghost(&core, "Schemas/Ordering:Customer", "Schemas:Customer");
    ghost(&core, "Free:Cust", "Schemas/Ordering:Customer");
    ok(
        &core,
        "card_create",
        json!({"target": "Free:Note", "value": {"kind": "note", "body": "[[Schemas/Ordering:Customer.name]]"}}),
    );
    embedding(&core, "Schemas/Ordering:Order", "buyer", "Customer");
    // The card's danglingRefs count what pointed at it through the ghost too.
    let gone = ok(&core, "card_delete", json!({"target": "Schemas:Customer"}));
    // Every reference left pointing nowhere, as the warnings say them one by one:
    // Order.customer_id points at it, Order's key signs with it, Order.buyer through the
    // ghost, the bodies of Docs:Overview and Free:Note, Refund's read, and the two ghosts.
    assert_eq!(gone["danglingRefs"], 8, "{gone}");
    assert_eq!(gone["warnings"].as_array().map(Vec::len), Some(8), "{gone}");
    let warned = gone["warnings"].to_string();
    assert!(warned.contains("Free:Note"), "{warned}");
    assert!(
        warned.contains("Schemas/Ordering:Customer の target の指す先が無くなりました"),
        "{warned}"
    );

    let left = read(&core, "Schemas/Ordering:Customer");
    assert_eq!(left["kind"], "ghost");
    assert_eq!(left["target"], Value::Null);
    assert_eq!(left["resolvesTo"], Value::Null);
    assert!(
        left["problem"]
            .as_str()
            .unwrap()
            .contains("指す先がありません"),
        "{left}"
    );
    assert_eq!(read(&core, "Free:Cust")["resolvesTo"], Value::Null);
    let now = broken(&core);
    assert!(
        now.iter()
            .any(|b| b.starts_with("Schemas/Ordering:Customer target : 指す先がありません")),
        "{now:?}"
    );
    assert!(
        now.iter().any(|b| b.starts_with(
            "Free:Cust target Schemas/Ordering:Customer: 指す先の Schemas/Ordering:Customer がカードに着きません"
        )),
        "{now:?}"
    );
    assert!(
        now.iter()
            .any(|b| b.starts_with("Free:Note body Schemas/Ordering:Customer.name")),
        "{now:?}"
    );
    // Past a ghost that leads nowhere, a target is refused, saying why.
    let said = refused(
        &core,
        "card_read",
        json!({"target": "Schemas/Ordering:Customer.name"}),
    );
    assert!(said.contains("指す先がありません"), "{said}");

    // A ghost removed as a card is: what went through it dangles.
    ghost(&core, "Docs:Status", "Schemas:Status");
    ok(
        &core,
        "card_create",
        json!({"target": "Docs:Spec.state", "value": {"kind": "card", "target": "Status"}}),
    );
    embedding(&core, "Schemas/Ordering:Order", "state", "Docs:Status");
    let gone = ok(&core, "card_delete", json!({"target": "Docs:Status"}));
    assert_eq!(gone["deleted"], "Docs:Status");
    assert_eq!(gone["danglingRefs"], 1);
    assert!(gone["warnings"].to_string().contains("Docs:Spec"), "{gone}");
    assert_eq!(last_lines(&core, ""), ["カードゴースト Docs:Status を削除"]);
}

// MARK: - The lock, a merge

#[test]
fn a_locked_main_keeps_its_ghosts_as_cards_and_lets_them_be_arranged() {
    let core = Core::open(script_store()).unwrap();
    ghost(&core, "Docs:Who", "Schemas:Customer");
    ghost(&core, "Docs:Kept", "Schemas:Status");
    let project = project_id(&core);
    let locked = op(
        &core,
        json!({"op": "main_lock", "project": project, "on": true}),
    );
    assert!(locked.get("error").is_none(), "{locked}");
    let lock = &specboard_core::write::locked_main();
    for (tool, arguments) in [
        (
            "card_create",
            json!({"target": "Docs:New", "value": {"kind": "ghost", "target": "Schemas:Customer"}}),
        ),
        (
            "card_update",
            json!({"target": "Docs:Who", "value": {"name": "Client"}}),
        ),
        (
            "card_update",
            json!({"target": "Docs:Who", "value": {"target": "Schemas/Ordering:Order"}}),
        ),
        ("card_delete", json!({"target": "Docs:Who"})),
    ] {
        let said = refused(&core, tool, arguments.clone());
        assert!(said.contains(lock), "{tool} {arguments}: {said}");
    }
    // Where it sits, and its place among the section's ghosts: as a card's.
    let before = commit_count(&core, "");
    ok(
        &core,
        "card_update",
        json!({"target": "Docs:Who", "value": {"topLeft": {"x": 70, "y": 3}}}),
    );
    assert_eq!(commit_count(&core, ""), before);
    ok(
        &core,
        "card_update",
        json!({"target": "Docs:Kept", "value": {"position": 0}}),
    );
    assert_eq!(commit_count(&core, ""), before + 1);
    let unlocked = op(
        &core,
        json!({"op": "main_lock", "project": project, "on": false}),
    );
    assert!(unlocked.get("error").is_none(), "{unlocked}");
}

#[test]
fn a_ghost_merges_as_a_card_its_target_one_unit() {
    let core = Core::open(script_store()).unwrap();
    ghost(&core, "Docs:Who", "Schemas:Customer");
    ok(&core, "branch_create", json!({"target": "work"}));
    ok(
        &core,
        "card_create",
        json!({"target": "Schemas:Person", "value": {"kind": "entity"}}),
    );
    ok(
        &core,
        "card_create",
        json!({"branch": "work", "target": "Schemas:Buyer", "value": {"kind": "entity"}}),
    );
    // Made in a branch, it comes in with the merge.
    ok(
        &core,
        "card_create",
        json!({"branch": "work", "target": "Free:Mine", "value": {"kind": "ghost", "target": "Schemas:Status"}}),
    );
    // Turned on both sides, apart: its target is one unit, and they conflict.
    ok(
        &core,
        "card_update",
        json!({"branch": "work", "target": "Docs:Who", "value": {"target": "Schemas:Buyer"}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Docs:Who", "value": {"target": "Schemas:Person"}}),
    );
    let preview = ok(
        &core,
        "branch_read",
        json!({"target": "work", "value": {"against": "main"}}),
    );
    let conflicts: Vec<(&str, &str)> = preview["conflicts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| (c["path"].as_str().unwrap(), c["unit"].as_str().unwrap()))
        .collect();
    assert_eq!(conflicts, [("Docs:Who", "target")]);
    // The branch's diff says what it turned it to.
    let diff = ok(&core, "branch_read", json!({"target": "work"}));
    assert!(
        diff.to_string()
            .contains("指す先 Schemas:Customer → Schemas:Buyer"),
        "{diff}"
    );
    ok(
        &core,
        "branch_merge",
        json!({"target": "work", "value": {"take": {"Docs:Who": {"target": "source"}}}}),
    );
    assert_eq!(read(&core, "Docs:Who")["resolvesTo"], "Schemas:Buyer");
    assert_eq!(read(&core, "Free:Mine")["resolvesTo"], "Schemas:Status");
}

// MARK: - Written out, drawn

#[test]
fn what_is_written_out_names_the_card_a_ghost_leads_to() {
    let core = Core::open(script_store()).unwrap();
    let before = ok(
        &core,
        "export_read",
        json!({"targetdir": "[default]", "target": "ts", "value": {"files": ["types.ts"]}}),
    );
    ghost(&core, "Schemas/Ordering:Client", "Schemas:Customer");
    embedding(&core, "Schemas/Ordering:Order", "buyer", "Client");
    let after = ok(
        &core,
        "export_read",
        json!({"targetdir": "[default]", "target": "ts", "value": {"files": ["types.ts"]}}),
    );
    let (before, after) = (
        before["files"][0]["text"].as_str().unwrap(),
        after["files"][0]["text"].as_str().unwrap(),
    );
    // No type of the ghost's own; the reference names the card's.
    assert!(!after.contains("Client"), "{after}");
    assert!(after.contains("buyer?: Customer"), "{after}");
    assert_eq!(
        after.matches("export interface").count(),
        before.matches("export interface").count()
    );
    // A note through the ghost names the card where it is written out.
    ok(
        &core,
        "card_create",
        json!({"target": "Docs:Guide", "value": {"kind": "freedoc", "body": "[[Schemas/Ordering:Client]] を見る"}}),
    );
    assert!(broken(&core).is_empty(), "{:?}", broken(&core));

    // The section's diagram draws the card whole, and says the ghost's name.
    let drawn = ok(
        &core,
        "section_read",
        json!({"target": "Schemas/Ordering", "value": {"as": "diagram"}}),
    );
    let drawn = drawn["mermaid"].as_str().unwrap();
    assert!(drawn.contains("Schemas:Customer"), "{drawn}");
    assert!(
        drawn.contains("note for") && drawn.contains("カードゴースト Schemas/Ordering:Client"),
        "{drawn}"
    );
}

// MARK: - standalone

#[test]
fn a_value_or_an_enum_is_drawn_on_its_own_as_it_looks() {
    let core = Core::open(script_store()).unwrap();
    let before = commit_count(&core, "");
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas/Ordering:Line", "value": {"standalone": true}}),
    );
    assert_eq!(commit_count(&core, ""), before, "how it looks: no commit");
    let read = ok(
        &core,
        "card_read",
        json!({"target": "Schemas/Ordering:Line", "value": {"include": ["at"]}}),
    );
    assert_eq!(read["standalone"], true);
    let whole = detail(&core, "");
    let line = whole["schemas"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "Line")
        .unwrap();
    assert_eq!(line["standalone"], true);
    // The section's diagram draws it as a class of its own, where it was folded into Order.
    let drawn = |core: &Core| {
        ok(
            core,
            "section_read",
            json!({"target": "Schemas/Ordering", "value": {"as": "diagram"}}),
        )["mermaid"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let class = "class Schemas_Ordering__Line[\"Schemas/Ordering:Line\"]";
    assert!(drawn(&core).contains(class), "{}", drawn(&core));
    // Off again; only a value or an enum takes it.
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas/Ordering:Line", "value": {"standalone": false}}),
    );
    assert!(
        detail(&core, "")["schemas"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c.get("standalone").is_none())
    );
    assert!(!drawn(&core).contains(class), "{}", drawn(&core));
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"standalone": true}}),
    );
    assert!(said.contains("value と enum のカードだけ"), "{said}");
    // Made with it.
    ok(
        &core,
        "card_create",
        json!({"target": "Schemas:Money", "value": {"kind": "value", "standalone": true}}),
    );
    assert_eq!(
        ok(
            &core,
            "card_read",
            json!({"target": "Schemas:Money", "value": {"include": ["at"]}})
        )["standalone"],
        true
    );
    // Under the lock too; what the project lets MCP write counts it as the layout.
    let project = project_id(&core);
    op(
        &core,
        json!({"op": "main_lock", "project": project, "on": true}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Status", "value": {"standalone": true}}),
    );
    op(
        &core,
        json!({"op": "main_lock", "project": project, "on": false}),
    );
    let denied = op(
        &core,
        json!({"op": "mcp_writes_set", "project": project, "item": "card_update", "state": "deny"}),
    );
    assert!(denied.get("error").is_none(), "{denied}");
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Schemas:Status", "value": {"standalone": false}}),
    );
    assert!(said.contains("card_update（カードを変える）"), "{said}");
}

// MARK: - F2: a name found elsewhere

#[test]
fn a_name_found_in_another_section_says_where_and_that_a_ghost_would_name_it() {
    let core = Core::open(script_store()).unwrap();
    ok(
        &core,
        "card_create",
        json!({"target": "Docs:Talk", "value": {"kind": "note", "body": "[[Customer]] を見る"}}),
    );
    let now = broken(&core);
    let talk = now
        .iter()
        .find(|b| b.starts_with("Docs:Talk body Customer"))
        .unwrap_or_else(|| panic!("{now:?}"));
    // What the directory says, not what Docs:Talk itself holds.
    assert!(
        talk.contains("ほかのセクションには Schemas:Customer があります"),
        "{talk}"
    );
    assert!(
        talk.contains("セクション Docs にカードゴーストを置けば名前だけで書けます"),
        "{talk}"
    );
    assert!(!talk.contains("直下にあるのは"), "{talk}");
    // A ghost put there, and the name is found.
    ghost(&core, "Docs:Customer", "Schemas:Customer");
    assert!(broken(&core).is_empty(), "{:?}", broken(&core));
    // A short path inside its own card still reads there (a rule's parameter).
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas/Ordering:Order", "value": {"rules": [{"kind": "lteRef", "target": "lines.qty"}]}}),
    );
    assert!(broken(&core).is_empty(), "{:?}", broken(&core));
}

// MARK: - Moved from the sections' lists

/// A section's id by its name, in a tree.
fn section_of(conn: &rusqlite::Connection, name: &str) -> String {
    conn.query_row(
        "SELECT id FROM section_docs WHERE branch_id = '' AND json_extract(doc, '$.name') = ?",
        [name],
        |r| r.get(0),
    )
    .unwrap()
}

/// A card's id by its name, in main.
fn card_of(conn: &rusqlite::Connection, name: &str) -> String {
    conn.query_row(
        "SELECT id FROM card_docs WHERE branch_id = '' AND name = ?",
        [name],
        |r| r.get(0),
    )
    .unwrap()
}

/// A section's document with `ghosts` listing the cards, in their order.
fn listing(doc: &str, cards: &[&str]) -> String {
    let mut doc: Value = serde_json::from_str(doc).unwrap();
    doc["ghosts"] = cards
        .iter()
        .enumerate()
        .map(|(i, c)| json!({"schema_id": c, "sort_order": i}))
        .collect();
    doc.to_string()
}

#[test]
fn a_store_made_before_has_its_sections_card_ghosts_moved_into_cards_once() {
    let place = script_store();
    // A name there to clash with: a note in Docs named as the card Docs' ghost points at.
    {
        let core = Core::open(&place).unwrap();
        ok(
            &core,
            "card_create",
            json!({"target": "Docs:Customer", "value": {"kind": "note", "body": "客のメモ"}}),
        );
        core.close();
    }
    {
        let conn = rusqlite::Connection::open(&place).unwrap();
        let project: String = conn
            .query_row("SELECT id FROM projects WHERE name = ?", [PROJECT], |r| {
                r.get(0)
            })
            .unwrap();
        let branch: String = conn
            .query_row("SELECT id FROM branches WHERE name = 'refunds'", [], |r| {
                r.get(0)
            })
            .unwrap();
        let ordering = section_of(&conn, "Ordering");
        let docs = section_of(&conn, "Docs");
        let (customer, line, order, status) = (
            card_of(&conn, "Customer"),
            card_of(&conn, "Line"),
            card_of(&conn, "Order"),
            card_of(&conn, "Status"),
        );
        let doc_of = |id: &str| -> String {
            conn.query_row(
                "SELECT doc FROM section_docs WHERE branch_id = '' AND id = ?",
                [id],
                |r| r.get(0),
            )
            .unwrap()
        };
        // main: another section's card (put on the diagram), its own value, its own entity
        // (drawn already), and a card that is gone.
        let ordering_doc = doc_of(&ordering);
        let main_list = listing(&ordering_doc, &[&customer, &line, &order, "gone-card"]);
        conn.execute(
            "UPDATE section_docs SET doc = ? WHERE branch_id = '' AND id = ?",
            rusqlite::params![main_list, ordering],
        )
        .unwrap();
        conn.execute(
            "UPDATE section_docs SET doc = ? WHERE branch_id = '' AND id = ?",
            rusqlite::params![listing(&doc_of(&docs), &[&customer]), docs],
        )
        .unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO places (project_id, branch_id, view, item_kind, item, x, y, pinned) VALUES (?, '', ?, 'card', ?, 50, 5, 1)",
            rusqlite::params![project, ordering, customer],
        )
        .unwrap();
        // The branch's own copy: main's ghost kept, its own value let go, a ghost of its own.
        conn.execute(
            "INSERT INTO section_docs (project_id, branch_id, id, parent_id, name, removed, doc, base, based_at) SELECT project_id, ?, id, parent_id, name, 0, ?, ?, 'then' FROM section_docs WHERE branch_id = '' AND id = ?",
            rusqlite::params![
                branch,
                listing(&ordering_doc, &[&customer, &status]),
                main_list,
                ordering
            ],
        )
        .unwrap();
        conn.execute(
            "DELETE FROM _migrations WHERE name = 'core_0003_card_ghosts'",
            [],
        )
        .unwrap();
    }

    let core = Core::open(&place).unwrap();
    // main: a ghost card where the list had another section's card, in the box it had.
    let customer = ok(
        &core,
        "card_read",
        json!({"target": "Schemas/Ordering:Customer", "value": {"include": ["at"]}}),
    );
    assert_eq!(customer["kind"], "ghost");
    assert_eq!(customer["resolvesTo"], "Schemas:Customer");
    assert_eq!(customer["topLeft"], json!({"x": 50, "y": 5}));
    // Its own value drawn on its own; its own entity and what is gone, let go.
    let line = ok(
        &core,
        "card_read",
        json!({"target": "Schemas/Ordering:Line", "value": {"include": ["at"]}}),
    );
    assert_eq!(line["standalone"], true);
    assert_eq!(
        ok(&core, "section_read", json!({"target": "Schemas/Ordering"}))["ghostCount"],
        1
    );
    // A name taken: another, said in the record.
    let renamed = read(&core, "Docs:Customer_ghost");
    assert_eq!(renamed["resolvesTo"], "Schemas:Customer");
    let record = last_lines(&core, "");
    assert_eq!(
        record[0],
        "セクションのカードゴースト 2 件を、セクションに置くカード（種類 ghost）にした"
    );
    for line in [
        "Schemas/Ordering:Line を図に単独で出す（standalone）にした",
        "セクション Schemas/Ordering のカードゴーストの 1 つは、指すカードが無いので移さなかった",
        "カードゴースト Docs:Customer は、名前がぶつかるので Customer_ghost にした（Schemas:Customer を指す）",
    ] {
        assert!(record.contains(&line.to_string()), "{record:?}");
    }
    // No section keeps a list any more.
    {
        let conn = rusqlite::Connection::open(&place).unwrap();
        let listed: i64 = conn
            .query_row(
                "SELECT count(*) FROM section_docs WHERE json_extract(doc, '$.ghosts') IS NOT NULL OR json_extract(base, '$.ghosts') IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(listed, 0);
    }

    // The branch: main's ghost, one of its own, and its own value folded again.
    let in_branch = |target: &str| {
        ok(
            &core,
            "card_read",
            json!({"branch": "refunds", "target": target, "value": {"include": ["at"]}}),
        )
    };
    assert_eq!(
        in_branch("Schemas/Ordering:Customer")["resolvesTo"],
        "Schemas:Customer"
    );
    assert_eq!(
        in_branch("Schemas/Ordering:Status")["resolvesTo"],
        "Schemas:Status"
    );
    assert!(
        in_branch("Schemas/Ordering:Line")
            .get("standalone")
            .is_none()
    );
    // main's ghosts were there when the branch was made, as far as it remembers.
    let diff = ok(&core, "branch_read", json!({"target": "refunds"}));
    assert!(
        !diff
            .get("mainMade")
            .is_some_and(|m| m.to_string().contains("Schemas/Ordering:Customer")),
        "{diff}"
    );
    assert!(
        diff["made"].to_string().contains("Schemas/Ordering:Status"),
        "{diff}"
    );
    let count = commit_count(&core, "");
    core.close();
    drop(core);
    // Opened again: nothing more.
    let core = Core::open(&place).unwrap();
    assert_eq!(commit_count(&core, ""), count);
    assert_eq!(read_project(&core)["counts"]["ghost"], 2);
}

// MARK: - The board

#[test]
fn a_card_ghost_goes_in_no_container_and_points_into_one() {
    let core = Core::open(script_store()).unwrap();
    let board = |tool: &str, mut arguments: Value| {
        arguments["branch"] = "_project_board_".into();
        ok(&core, tool, arguments)
    };
    board("section_create", json!({"target": "Goals/Later"}));
    // A card inside a goal, by its path on from the goal's.
    board(
        "card_create",
        json!({"target": "Goals/Later:Alias", "value": {"kind": "ghost", "target": "Goals:Ship.Design"}}),
    );
    let alias = board("card_read", json!({"target": "Goals/Later:Alias"}));
    assert_eq!(alias["resolvesTo"], "Goals:Ship.Design", "{alias}");
    assert_eq!(alias["targetKind"], "task");
    // A ghost is put in a section, never inside a container.
    let mut arguments = json!({"target": "Goals:Ship.Alias", "value": {"kind": "ghost", "target": "Goals/Later:Alias"}});
    arguments["branch"] = "_project_board_".into();
    let said = refused(&core, "card_create", arguments);
    assert!(
        said.contains("カードゴーストは入れ物の中に置けません（Goals:Ship の中）"),
        "{said}"
    );
}
