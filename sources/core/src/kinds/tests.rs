use super::*;
use serde_json::{Value, json};
use std::collections::HashMap;

fn card(kind: &str, fields: Value, rows: Value) -> Value {
    json!({ "id": "c1", "kind": kind, "name": "Card", "section": "s1", "v": 1, "fields": fields, "rows": rows })
}

/// The problems with a card, as text.
fn problems(doc: &Value) -> Vec<String> {
    check_card(builtin(), doc)
        .iter()
        .map(ToString::to_string)
        .collect()
}

fn says(doc: &Value, wanted: &str) {
    let found = problems(doc);
    assert!(
        found.iter().any(|p| p == wanted),
        "wanted「{wanted}」, found {found:#?}"
    );
}

struct Project(HashMap<String, String>);

impl Project {
    fn new(pairs: &[(&str, &str)]) -> Project {
        Project(
            pairs
                .iter()
                .map(|(id, path)| (id.to_string(), path.to_string()))
                .collect(),
        )
    }
}

impl Names for Project {
    fn path(&self, id: &str) -> Option<String> {
        self.0.get(id).cloned()
    }
    fn id(&self, path: &str) -> Option<String> {
        self.0
            .iter()
            .find(|(_, p)| p.as_str() == path)
            .map(|(id, _)| id.clone())
    }
}

fn counter() -> impl FnMut() -> String {
    let mut n = 0;
    move || {
        n += 1;
        format!("new{n}")
    }
}

// MARK: - The definitions

#[test]
fn the_builtin_kinds_read() {
    let kinds = builtin();
    // 3 since a container holds cards inside it; 4 since a kind says how
    // its card is drawn (`face`) and the board's fields are named in English;
    // 5 since a goal's `PR` is its `MR`, of the kind of field `mr`; 8 since
    // the stacked document `stackdoc`; 9 since the data card `instance` and its
    // section; and up by one with each
    // change since (builtin.json's history says which).
    assert_eq!(kinds.version, 23);
    let placeable: Vec<&str> = kinds.placeable().map(|k| k.name.as_str()).collect();
    assert_eq!(
        placeable,
        [
            "entity",
            "value",
            "enum",
            "store",
            "instance",
            "table",
            "flow",
            "begin",
            "process",
            "run",
            "eval",
            "wait",
            "join",
            "end",
            "formatdoc",
            "freedoc",
            "stackdoc",
            "note",
            "svg",
            "html",
            "frame",
            "goal",
            "task",
            "ghost"
        ]
    );
    let abstracts: Vec<&str> = kinds
        .all()
        .iter()
        .filter(|k| k.is_abstract)
        .map(|k| k.name.as_str())
        .collect();
    assert_eq!(abstracts, ["base", "shape", "record", "node"]);
    // The containers: what each holds, opened as a card map.
    let flow = kinds.get("flow").unwrap();
    assert_eq!(
        flow.holds,
        ["begin", "process", "run", "eval", "wait", "join", "end"]
    );
    assert_eq!(flow.opens.as_deref(), Some("card-map"));
    assert_eq!(kinds.get("goal").unwrap().holds, ["goal", "task"]);
    assert!(is_container("goal") && !is_container("task"));
    // A node is only inside a flow; a task goes in a section too.
    assert!(inner_only("process") && !inner_only("task") && !inner_only("flow"));
    assert_eq!(holders_of("end"), ["flow"]);
    assert_eq!(holders_of("task"), ["goal"]);
}

/// A definition promised to the outside is kept as its shapes and their examples side by side:
/// the section holds both, and its sections inside are of its kind.
#[test]
fn the_builtin_definition_section_holds_shapes_and_their_data() {
    {
        let _en = crate::text::bind(Some("en"));
        assert_eq!(builtin_defs().section_label("definition"), "Definition");
    }
    let _ja = crate::text::bind(Some("ja"));
    let defs = builtin_defs();
    let def = defs.section("definition").unwrap();
    assert!(def.builtin);
    assert_eq!(def.label, "定義");
    assert_eq!(
        def.holds,
        [
            "entity", "value", "enum", "store", "instance", "table", "note"
        ]
    );
    assert_eq!(def.sections, ["definition"]);
    assert_eq!(def.trees, [Tree::Main]);
    assert_eq!(def.icon.as_deref(), Some("curlybraces"));
    assert_eq!(def.color.as_deref(), Some("#30B0C7"));
    for kind in [
        "entity", "value", "enum", "store", "instance", "table", "note", "ghost",
    ] {
        assert!(defs.holds("definition", kind), "{kind}");
    }
    for kind in [
        "flow",
        "freedoc",
        "formatdoc",
        "stackdoc",
        "goal",
        "task",
        "file",
    ] {
        assert!(!defs.holds("definition", kind), "{kind}");
    }
    // The kind it replaced is no kind of now.
    assert!(defs.section("instances").is_none());
}

#[test]
fn the_builtin_sections_and_lines_read() {
    let defs = builtin_defs();
    let sections: Vec<&str> = defs.sections.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(
        sections,
        [
            "document",
            "ui",
            "schema",
            "definition",
            "chart",
            "actor",
            "free",
            "goals",
            "complete",
            "cancelled",
            "workdir",
            "directory"
        ]
    );
    // A working directory's section, under main's root alone; the directories in it, under no
    // root, and only what the core reads there.
    assert_eq!(defs.section("workdir").unwrap().trees, [Tree::Main]);
    assert!(defs.section("directory").unwrap().trees.is_empty());
    assert_eq!(defs.section("workdir").unwrap().actions, ["read"]);
    assert!(defs.section("directory").unwrap().actions.is_empty());
    assert_eq!(defs.section("workdir").unwrap().holds, ["file", "note"]);
    // The trees whose root each may be put directly under (`project_tree`).
    let trees = |kind: &str| -> Vec<Tree> { defs.section(kind).unwrap().trees.clone() };
    assert_eq!(trees("document"), [Tree::Main]);
    assert_eq!(trees("chart"), [Tree::Main]);
    // An actor sits only inside a chart or another actor, which list each other.
    assert_eq!(trees("actor"), []);
    assert_eq!(defs.section("chart").unwrap().sections, ["chart", "actor"]);
    assert_eq!(defs.section("actor").unwrap().sections, ["actor", "chart"]);
    assert_eq!(trees("free"), [Tree::Main, Tree::Board]);
    assert_eq!(trees("goals"), [Tree::Board]);
    // Where the card actions move cards: only inside a goals section, never directly under a root.
    assert_eq!(trees("complete"), []);
    assert_eq!(trees("cancelled"), []);
    assert_eq!(
        defs.section("goals").unwrap().sections,
        ["goals", "complete", "cancelled"]
    );
    assert_eq!(trees("definition"), [Tree::Main]);
    // A new project's: Docs・Schemas・Definitions・Charts・Free under main, Goals under the board.
    let kinds: Vec<(&str, &str)> = defs
        .tree
        .main
        .iter()
        .map(|t| (t.kind.as_str(), t.name.as_str()))
        .collect();
    assert_eq!(
        kinds,
        [
            ("document", "Docs"),
            ("schema", "Schemas"),
            ("definition", "Definitions"),
            ("chart", "Charts"),
            ("free", "Free")
        ]
    );
    assert_eq!(defs.tree.board, [TopSection::new("goals", "Goals")]);
    // Where a card belongs, and where it may go: what the sections hold.
    assert_eq!(defs.home_of("entity").as_deref(), Some("schema"));
    assert_eq!(defs.home_of("note").as_deref(), Some("document"));
    assert_eq!(defs.home_of("flow").as_deref(), Some("chart"));
    assert_eq!(defs.home_of("goal").as_deref(), Some("goals"));
    // A data card's home is the definition section a new project has; a shape's stays schema,
    // which comes first.
    assert_eq!(defs.home_of("instance").as_deref(), Some("definition"));
    assert_eq!(defs.home_of("table").as_deref(), Some("document"));
    assert!(!defs.holds("schema", "instance"));
    // A section ghost the app alone draws: no section holds it, not even free.
    assert_eq!(defs.home_of("section"), None);
    assert!(!defs.holds("free", "section") && system_only("section"));
    assert!(defs.holds("free", "flow"));
    assert!(defs.holds("schema", "note"));
    assert!(!defs.holds("schema", "flow"));
    assert!(defs.holds("chart", "ghost"));
    // A flow's node is in no section, not even one that holds anything.
    assert!(!defs.holds("free", "process"));
    assert!(defs.holds("free", "task"));
    // A working directory's file the core alone makes: made nowhere — a
    // directory section is no tree's root's — and in no section that holds anything.
    assert_eq!(defs.home_of("file"), None);
    assert!(!defs.holds("free", "file") && system_only("file"));
    let lines: Vec<&str> = defs.lines.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(
        lines,
        [
            "embed",
            "points",
            "link",
            "holds",
            "shapeOf",
            "rules",
            "implementedBy",
            "mentions",
            "next",
            "condition",
            "else",
            "onError",
            "onTimeout",
            "runs",
            "awaits",
            "reads",
            "writes",
            "partner",
            "key"
        ]
    );
    // The flow's lines: between its nodes, beside each other in it; from a row, by its place.
    let next = defs.line("next").unwrap();
    let head = LinePlace::HEAD;
    let step = LinePlace::row(Some("steps"));
    assert!(next.siblings && next.leaves("begin", &head) && !next.leaves("end", &head));
    assert_eq!(next.owner, ["flow"]);
    assert_eq!(defs.line("else").unwrap().max, Some(1));
    assert!(defs.line("condition").unwrap().field("when").is_some());
    let reads = defs.line("reads").unwrap();
    assert!(reads.leaves("process", &step) && !reads.leaves("process", &head));
    assert!(!reads.siblings);
    let key = defs.line("key").unwrap();
    assert!(key.leaves("run", &head) && key.leaves("process", &step));
    assert_eq!(key.max, Some(2));
    let points = defs.line("points").unwrap();
    assert_eq!(points.base, Base::Designate);
    assert_eq!(points.annotation(), Some("x-pointsAt"));
    assert_eq!(points.word(), "id");
    // A record's field is a row, of its kind of row `fields`.
    let field = LinePlace::row(Some(FIELD_ROWS));
    assert!(points.leaves("entity", &field));
    assert!(!points.leaves("entity", &head));
    assert!(!points.leaves("formatdoc", &LinePlace::row(Some("rows"))));
    assert_eq!(points.from_said(), "entity・valueの欄");
    assert_eq!(points.to.said(), "entity・valueの頭・欄");
    let link = defs.line("link").unwrap();
    assert!(
        link.leaves("formatdoc", &LinePlace::row(Some("rows"))) && link.leaves("process", &step)
    );
    assert_eq!(link.to.said(), "どのカードの頭・欄・行");
    assert_eq!(
        defs.line("rules").unwrap().to.said(),
        "note・freedoc・formatdocの頭・行"
    );
    assert_eq!(defs.line("embed").unwrap().max, Some(1));
    assert!(defs.line("mentions").unwrap().from_text);
    assert_eq!(defs.line("holds").unwrap().owner, ["store"]);
}

#[test]
fn a_projects_own_kinds_are_checked_beside_the_builtin_ones() {
    let own = json!({
        "sections": [
            { "section": "keys", "label": "鍵", "holds": ["entity", "note"], "export": true },
            { "section": "bad name", "holds": [] },
            { "section": "schema", "holds": ["*"] },
            { "section": "pinned", "holds": ["*"], "fixed": "main", "trees": ["main", "roots"] },
            // A section opens on its map alone: a ui section's parts are drawn there too.
            { "section": "shown", "holds": ["note"], "opens": "mock" }
        ],
        "lines": [
            { "line": "確かめる", "base": "link", "from": [{ "kinds": ["entity"], "at": "card" }],
              "to": { "kinds": ["*"], "at": ["card", "row"] },
              "fields": [{ "name": "payload", "fieldtype": "text", "required": true }],
              "export": { "as": "annotation", "key": "x-payloads" } },
            { "line": "points", "base": "designate", "from": [{ "kinds": ["entity"], "at": "row" }], "to": { "kinds": ["*"] } },
            { "line": "odd", "base": "sideways", "from": [{ "kinds": ["moon"], "at": "edge" }], "to": { "kinds": ["*"] },
              "export": { "as": "annotation" }, "owner": "store" },
            // A record's field was `field` before; a row now, and which kinds of row
            // only where a kind has them.
            { "line": "前の形", "base": "link", "from": [{ "kinds": ["entity"], "at": "field" }],
              "to": { "kinds": ["formatdoc"], "at": ["card"], "rows": ["rows"] } },
            { "line": "行の種類", "base": "link", "from": [{ "kinds": ["entity", "note"], "at": "row", "rows": ["fields", "steps"] }],
              "to": { "kinds": ["*"], "at": ["row"], "rows": ["nope"] } },
            { "line": "型", "base": "embed", "from": [{ "kinds": ["formatdoc"], "at": "row" }],
              "to": { "kinds": ["entity"], "at": ["card"] } }
        ]
    });
    let problems = Defs::checked(&own).unwrap_err();
    for wanted in [
        "sections の bad name: 名前は、半角英字か _ で始まる英数字と _ だけで、80 字までです",
        "sections の bad name: holds（入れられるカードの種類。* でどれでも）がありません",
        "sections の schema: 同じ名前のセクションの種類がもうあります（組み込み）",
        "sections の pinned: 「fixed」は書けないキーです（書けるのは section・label・description・context・holds・sections・opens・trees・export・icon・color・actions）",
        "sections の pinned: trees は main・board の並びです（roots）",
        "sections の shown: opens は relations です（mock）",
        "lines の points: 同じ名前の線の種類がもうあります（組み込み）",
        "lines の odd: base は embed（型が相手になる）・designate（型は変わらず相手を指す）・link（型に関係しない）のどれかです",
        "lines の odd の from[0]: 「moon」というカードの種類はありません",
        "lines の odd の from[0]: at は card（頭）・row（中の行）・symbol（ファイルの中の関数と型）のどれかです（edge）",
        "lines の 前の形 の from[0]: at は card（頭）・row（中の行）・symbol（ファイルの中の関数と型）のどれかです（field）。entity・value の欄も row です",
        "lines の 前の形 の to: rows（行の種類）は at に row があるときだけ書けます",
        "lines の 行の種類 の from[0]: rows の「steps」という行の種類は entity・note にありません（あるのは: fields）",
        "lines の 行の種類 の to: rows の「nope」という行の種類はどのカードの種類にもありません",
        "lines の 型: base が embed の線は、entity・value の欄（from の kinds が entity・value、at が row）からだけ出せます",
        "lines の odd: export の as が annotation のときは、key に JSON Schema の注記の名前（x- で始まる）を書きます",
        "lines の odd: owner（カードの種類が自分で持つ線）は組み込みの線だけです",
    ] {
        assert!(
            problems.iter().any(|p| p == wanted),
            "wanted「{wanted}」, found {problems:#?}"
        );
    }
    let fine = json!({
        "sections": [{ "section": "keys", "holds": ["entity"], "export": true }],
        "lines": [{ "line": "確かめる", "base": "link", "from": [{ "kinds": ["entity"], "at": "card" }],
                    "to": { "kinds": ["*"], "at": ["card", "row"] },
                    "export": { "as": "annotation", "key": "x-payloads" } }]
    });
    let defs = Defs::checked(&fine).unwrap();
    let keys = defs.section("keys").unwrap();
    assert!(!keys.builtin);
    assert_eq!(keys.sections, ["keys"]);
    assert_eq!(
        defs.line("確かめる").unwrap().annotation(),
        Some("x-payloads")
    );
    // As a project keeps them: read beside the built-in ones.
    let read = Defs::of_project(Some(&fine.to_string()));
    assert!(read.section("keys").is_some() && read.section("schema").is_some());
    assert!(read.line("確かめる").is_some() && read.line("embed").is_some());
}

/// A line's end is a card's head or a row, and for a kind of card with
/// more than one kind of row, which ones (`rows`): from and to alike. A field of a project's own
/// kind's card is a row of no kind.
#[test]
fn a_line_names_the_kinds_of_row_of_a_kind_with_more_than_one() {
    let own = json!({
        "cards": [{ "kind": "risk",
            "fields": [{ "name": "severity", "fieldtype": "text" }],
            "rows": [
                { "name": "steps", "fields": [{ "name": "name", "fieldtype": "text" }],
                  "rows": [{ "name": "checks", "fields": [{ "name": "name", "fieldtype": "text" }] }] },
                { "name": "causes", "fields": [{ "name": "name", "fieldtype": "text" }] }
            ] }],
        "lines": [
            { "line": "原因", "base": "link", "from": [{ "kinds": ["risk"], "at": "row", "rows": ["causes"] }],
              "to": { "kinds": ["risk", "entity"], "at": ["card", "row"], "rows": ["steps", "checks"] } },
            { "line": "どの行", "base": "link", "from": [{ "kinds": ["risk"], "at": "row" }], "to": { "kinds": ["*"] } }
        ]
    });
    let defs = Defs::checked(&own).unwrap();
    let cause = defs.line("原因").unwrap();
    assert!(cause.leaves("risk", &LinePlace::row(Some("causes"))));
    assert!(!cause.leaves("risk", &LinePlace::row(Some("steps"))));
    assert!(!cause.leaves("risk", &LinePlace::row(None)));
    assert!(cause.to.takes("risk", &LinePlace::row(Some("checks"))));
    assert!(cause.to.takes("entity", &LinePlace::HEAD));
    // The kinds of row named are every kind's: an entity's fields are not among them.
    assert!(!cause.to.takes("entity", &LinePlace::row(Some(FIELD_ROWS))));
    assert_eq!(cause.from_said(), "riskの行（causes）");
    assert_eq!(cause.to.said(), "risk・entityの頭・行（steps・checks）");
    // None named: any row, a field of the card too.
    let any = defs.line("どの行").unwrap();
    assert!(any.leaves("risk", &LinePlace::row(Some("steps"))));
    assert!(any.leaves("risk", &LinePlace::row(None)));
    assert_eq!(any.from_said(), "riskの行");
}

#[test]
fn a_projects_own_kinds_of_card_are_read_after_the_builtin_ones_and_held_to_them() {
    let own = json!({
        "cards": [
            { "kind": "risk", "color": "#C2410C", "header": ["severity"],
              "fields": [{ "name": "severity", "fieldtype": "radio", "options": ["高", "低"] }],
              "rows": [{ "name": "steps", "fields": [{ "name": "name", "fieldtype": "text" }] }] },
            { "kind": "epic", "holds": ["risk", "process", "ghost"], "opens": "card-map" },
            { "kind": "odd", "extends": "record", "semantics": {}, "color": "red", "as_row": ["nope"],
              "on_click": "toggle_done", "on_double_click": "edit_in_place",
              "fields": [{ "name": "a.b", "fieldtype": "text", "look": true }, { "name": "connections", "fieldtype": "text" }],
              "context": { "why": "x" } },
            { "kind": "task" },
            { "kind": "1bad" },
            { "kind": "page", "opens": "markdown", "fields": [{ "name": "body", "fieldtype": "text" }] },
            { "kind": "alone", "opens": "card-map" },
            { "kind": "box", "holds": ["risk"], "opens": "fields" }
        ],
        "sections": [{ "section": "risks", "holds": ["risk", "epic"] }],
        "lines": [{ "line": "対策", "base": "link", "from": [{ "kinds": ["risk"], "at": ["card", "row"] }], "to": { "kinds": ["*"] } }]
    });
    let problems = Defs::checked(&own).unwrap_err();
    for wanted in [
        "cards の epic: holds の「process」は入れ物（flow）の中にだけ置ける種類です",
        "cards の epic: holds に ghost は書けません（カードゴーストは入れ物の中に置けません）",
        "cards の odd: semantics（検査・線・エクスポート・編集画面のコードの名前）は組み込みの種類だけです",
        "cards の odd: extends は base（組み込みの基の定義）かプロジェクトの種類です（record は組み込みの種類）",
        "cards の odd: color は #RRGGBB です（\"red\"）",
        "cards の odd: as_row の \"nope\" は欄にありません",
        "cards の odd: on_double_click の edit_in_place には、longtext か markdown の欄 body が要ります",
        "cards の odd.a.b: look（見かけの欄）は組み込みの種類だけです",
        "cards の odd.a.b: 欄の名前は空でなく、前後に空白が無く、40 字までで、. : / [ ] , \" と改行を含みません",
        "cards の odd.connections: 「connections」はカードの value のキーなので、欄の名前に使えません",
        "cards の odd: context に「why」は書けません（書けるのは purpose・use_when・not_for）",
        "cards の task: 同じ名前の種類がもうあります（組み込み）",
        "cards の 1bad: 名前は、半角英字か _ で始まる英数字と _ だけで、80 字までです",
        "cards の page: opens の markdown には、longtext か markdown の欄 body が要ります",
        "cards の alone: opens の card-map は、holds のある種類（入れ物）だけです",
        "cards の box: holds のある種類（入れ物）の opens は card-map です（fields）",
    ] {
        assert!(
            problems.iter().any(|p| p == wanted),
            "wanted「{wanted}」, found {problems:#?}"
        );
    }
    let fine = json!({
        "cards": [
            { "kind": "risk", "header": ["severity"],
              "fields": [{ "name": "severity", "fieldtype": "radio", "options": ["高", "低"] }],
              "rows": [{ "name": "steps", "fields": [{ "name": "name", "fieldtype": "text" }] }] },
            { "kind": "epic", "holds": ["risk"], "opens": "card-map" },
            // Opened as a note's body is; `on_click`, written before, read and let go.
            { "kind": "page", "opens": "markdown", "on_click": "toggle_done",
              "fields": [{ "name": "body", "fieldtype": "markdown" }] }
        ],
        "sections": [{ "section": "risks", "holds": ["risk", "epic"] }],
        "lines": [{ "line": "対策", "base": "link", "from": [{ "kinds": ["risk"], "at": ["card", "row"] }], "to": { "kinds": ["*"] } }]
    });
    let defs = Defs::checked(&fine).unwrap();
    let cards = defs.cards();
    let risk = cards.get("risk").unwrap();
    // Taken from base when it names no parent; the project's own, after the built-in ones.
    assert!(!risk.builtin && cards.is_own("risk") && !cards.is_own("goal"));
    let names: Vec<&str> = risk.fields.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(
        names,
        ["label", "description", "tags", "metadata", "severity"]
    );
    assert!(cards.holds_cards("epic") && !cards.holds_cards("risk"));
    assert_eq!(
        cards.get("page").unwrap().opens.as_deref(),
        Some("markdown")
    );
    assert!(defs.section("risks").unwrap().holds_kind("risk"));
    // A section that holds anything holds one of the project's own too.
    assert!(defs.holds("free", "risk"));
    assert_eq!(defs.home_of("risk").as_deref(), Some("free"));
    // As a project keeps them.
    let read = Defs::of_project(Some(&fine.to_string()));
    assert!(read.cards().is_own("epic") && read.cards().get("entity").is_some());
    // Whole, as the screen reads them: the parents' fields first, no code names.
    let whole = written_whole(fine.get("cards"), read.cards());
    let risk = whole.iter().find(|k| k["kind"] == "risk").unwrap();
    assert!(risk.get("builtin").is_none());
    assert_eq!(risk["fields"][0]["name"], "label");
    assert_eq!(risk["fields"][4]["name"], "severity");
    let flow = whole.iter().find(|k| k["kind"] == "flow").unwrap();
    assert_eq!(flow["builtin"], true);
    assert!(flow.get("semantics").is_none() && flow.get("extends").is_some());
    assert!(whole.iter().all(|k| k["kind"] != "base"));
}

#[test]
fn a_face_names_fields_of_its_kind_it_can_draw() {
    // The built-in task's: its check above, its body below.
    let task = builtin().get("task").unwrap();
    assert_eq!(
        task.face,
        Some(Face {
            top: vec!["done".to_string()],
            markdown: Some("body".to_string()),
            page: None,
        })
    );
    assert!(builtin().get("goal").unwrap().face.is_none());
    // A project's own: what it names is checked with its fields, its parents' too.
    let own = json!({ "cards": [
        { "kind": "risk",
          "fields": [
            { "name": "done", "fieldtype": "checkbox" },
            { "name": "severity", "fieldtype": "radio", "options": ["高", "低"] },
            { "name": "area", "fieldtype": "dropdownlist", "options": ["画面", "コア"] },
            { "name": "due", "fieldtype": "date" },
            { "name": "count", "fieldtype": "number" },
            { "name": "owners", "fieldtype": "text", "many": true },
            { "name": "memo", "fieldtype": "markdown" },
            { "name": "note", "fieldtype": "longtext" },
            { "name": "summary", "fieldtype": "text" } ],
          "rows": [{ "name": "steps", "fields": [{ "name": "name", "fieldtype": "text" }] }],
          "face": { "top": ["done", "severity", "area", "due", "label", "count", "owners", "steps", "nowhere", "done"],
                    "markdown": "summary", "resizable": "yes", "size": 3 } },
        { "kind": "short", "fields": [{ "name": "memo", "fieldtype": "markdown", "many": true }],
          "face": { "top": "memo", "markdown": "memo" } },
        { "kind": "odd", "fields": [{ "name": "memo", "fieldtype": "markdown" }],
          "face": { "markdown": "missing" } },
        { "kind": "flat", "face": ["label"] }
    ] });
    let problems = Defs::checked(&own).unwrap_err();
    for wanted in [
        "cards の risk: face に「size」は書けません（書けるのは top・markdown・svg・html）",
        "cards の risk: face の top に書けるのは checkbox・radio・dropdownlist・date・text・section の欄です（count は number）",
        "cards の risk: face の top に並びの欄は書けません（owners は many）",
        "cards の risk: face の top の「steps」は欄にありません（行の種類は top に書けません）",
        "cards の risk: face の top の「nowhere」は欄にありません",
        "cards の risk: face の top の「done」が重なっています",
        "cards の risk: face の markdown に書けるのは markdown・longtext の欄です（summary は text）",
        "cards の short: face の top は欄の名前の並びです",
        "cards の short: face の markdown に書けるのは markdown・longtext の欄です（memo は markdown の並び）",
        "cards の odd: face の markdown の「missing」は欄にありません",
        r#"cards の flat: face は {"top": [欄の名前…], "markdown"（か "svg"・"html"）: "<欄の名前>"} のオブジェクトです"#,
    ] {
        assert!(
            problems.iter().any(|p| p == wanted),
            "wanted「{wanted}」, found {problems:#?}"
        );
    }
    // A long text below — a parent's too (`description`, base's) — and every kind of field above
    // a press changes or a value is written in.
    let fine = json!({ "cards": [
        { "kind": "risk",
          "fields": [
            { "name": "done", "fieldtype": "checkbox" },
            { "name": "severity", "fieldtype": "radio", "options": ["高", "低"] },
            { "name": "area", "fieldtype": "dropdownlist", "options": ["画面", "コア"] },
            { "name": "due", "fieldtype": "date" } ],
          "face": { "top": ["done", "severity", "area", "due", "label"], "markdown": "description", "resizable": true } },
        { "kind": "plain", "face": {} }
    ] });
    let defs = Defs::checked(&fine).unwrap();
    let risk = defs.cards().get("risk").unwrap();
    assert_eq!(risk.face.as_ref().map(|f| f.top.len()), Some(5));
    // `resizable`, of an earlier form, is read and let go, whatever its value.
    assert!(
        problems.iter().all(|p| !p.contains("resizable")),
        "{problems:#?}"
    );
    // As the screen reads it: as written.
    let whole = written_whole(fine.get("cards"), defs.cards());
    let risk = whole.iter().find(|k| k["kind"] == "risk").unwrap();
    assert_eq!(risk["face"]["markdown"], "description");
    let task = whole.iter().find(|k| k["kind"] == "task").unwrap();
    assert_eq!(task["face"], json!({ "top": ["done"], "markdown": "body" }));
    // A face is a kind's own, not taken from its parent.
    let child = json!({ "cards": [
        { "kind": "risk", "fields": [{ "name": "done", "fieldtype": "checkbox" }], "face": { "top": ["done"] } },
        { "kind": "subrisk", "extends": "risk" }
    ] });
    let defs = Defs::checked(&child).unwrap();
    assert!(defs.cards().get("subrisk").unwrap().face.is_none());
}

#[test]
fn a_face_draws_one_long_text_below_as_markdown_or_a_page() {
    // The built-in svg and html: their body drawn as a page.
    for (name, drawn) in [("svg", PageKind::Svg), ("html", PageKind::Html)] {
        let kind = builtin().get(name).unwrap();
        assert_eq!(
            kind.face,
            Some(Face {
                top: Vec::new(),
                markdown: None,
                page: Some(Page {
                    field: "body".to_string(),
                    drawn,
                }),
            })
        );
    }
    let own = json!({ "cards": [
        { "kind": "both", "fields": [{ "name": "memo", "fieldtype": "longtext" }],
          "face": { "markdown": "memo", "svg": "memo" } },
        { "kind": "pages", "fields": [{ "name": "memo", "fieldtype": "longtext" }],
          "face": { "svg": "memo", "html": "memo" } },
        { "kind": "typed", "fields": [
            { "name": "memo", "fieldtype": "markdown" },
            { "name": "many", "fieldtype": "longtext", "many": true },
            { "name": "done", "fieldtype": "checkbox" } ],
          "face": { "top": ["done"], "html": "memo" } },
        { "kind": "listed", "fields": [{ "name": "many", "fieldtype": "longtext", "many": true }],
          "face": { "svg": "many" } },
        { "kind": "named", "face": { "svg": 3 } },
        { "kind": "missing", "face": { "html": "nowhere" } },
        { "kind": "twice", "fields": [{ "name": "pic", "fieldtype": "longtext" }],
          "face": { "top": ["pic"], "svg": "pic" } }
    ] });
    let problems = Defs::checked(&own).unwrap_err();
    for wanted in [
        "cards の both: face の下に描く欄（markdown・svg・html）は 1 つだけです",
        "cards の pages: face の下に描く欄（markdown・svg・html）は 1 つだけです",
        "cards の typed: face の html に書けるのは longtext の欄です（memo は markdown。並びの欄は書けません）",
        "cards の listed: face の svg に書けるのは longtext の欄です（many は longtext。並びの欄は書けません）",
        "cards の named: face の svg は欄の名前（文字列）です",
        "cards の missing: face の html の「nowhere」は欄にありません",
        "cards の twice: face の「pic」は top と、下に描く欄（markdown・svg・html）の両方にあります",
    ] {
        assert!(
            problems.iter().any(|p| p == wanted),
            "wanted「{wanted}」, found {problems:#?}"
        );
    }
    // A project's own kind may draw a page too; a picture's field is held to SVG as any is.
    let fine = json!({ "cards": [
        { "kind": "mock", "fields": [{ "name": "picture", "fieldtype": "longtext" }],
          "face": { "svg": "picture", "resizable": true } }
    ] });
    let defs = Defs::checked(&fine).unwrap();
    let card = |text: &str| {
        json!({ "id": "c1", "kind": "mock", "name": "M", "section": "s", "v": 1,
                "fields": { "picture": text } })
    };
    assert!(crate::kinds::check_card(defs.cards(), &card("<svg/>")).is_empty());
    let problems = crate::kinds::check_card(defs.cards(), &card("<html/>"));
    assert_eq!(
        problems.iter().map(ToString::to_string).collect::<Vec<_>>(),
        ["picture: SVG ではありません: 根の要素が <html> です（svg の要素で書きます）"]
    );
}

#[test]
fn what_a_definition_no_longer_has_is_kept_found_and_let_go_of() {
    let own = json!({ "cards": [{ "kind": "risk",
        "fields": [{ "name": "severity", "fieldtype": "text" }],
        "rows": [{ "name": "steps", "fields": [{ "name": "name", "fieldtype": "text" }] }] }] });
    let defs = Defs::checked(&own).unwrap();
    let kind = defs.cards().get("risk").unwrap();
    let mut doc = json!({ "id": "c1", "kind": "risk", "name": "A", "section": "s1", "v": 3,
        "fields": { "severity": "高", "sev": "低" },
        "rows": { "steps": [{ "id": "r1", "fields": { "name": "a", "memo": "x" } }],
                  "old": [{ "id": "r2", "fields": { "name": "b" } }] } });
    // Said as problems marked stray, and only those.
    let problems = check_card(defs.cards(), &doc);
    assert!(problems.iter().all(|p| p.stray), "{problems:#?}");
    assert_eq!(problems.len(), 3, "{problems:#?}");
    let found = strays(kind, &doc);
    assert_eq!(
        found,
        vec![
            ("sev".to_string(), json!("低")),
            ("steps.a.memo".to_string(), json!("x")),
            ("old".to_string(), json!([{ "name": "b" }])),
        ]
    );
    let which: std::collections::HashSet<String> = ["steps.a.memo".to_string()].into();
    let doc_map = doc.as_object_mut().unwrap();
    assert_eq!(drop_strays(kind, doc_map, Some(&which)), ["steps.a.memo"]);
    assert_eq!(
        doc_map["rows"]["steps"][0]["fields"],
        json!({ "name": "a" })
    );
    assert_eq!(drop_strays(kind, doc_map, None), ["sev", "old"]);
    assert!(check_card(defs.cards(), &doc).is_empty());
}

#[test]
fn a_kind_takes_its_parents_fields_and_rows_first() {
    let kinds = builtin();
    let names = |kind: &str| -> Vec<String> {
        kinds
            .get(kind)
            .unwrap()
            .fields
            .iter()
            .map(|f| f.name.clone())
            .collect()
    };
    let base = ["label", "description", "tags", "metadata"];
    // A signature's keys are lines now (`connections`), not a shape's field.
    let shape = ["rules", "absentFields", "discriminator"];
    // How data of a record's shape is drawn as a list.
    let record = ["face"];
    assert_eq!(
        names("entity"),
        [&base[..], &shape[..], &record[..]].concat()
    );
    // How a value or an enum is drawn on its own section's diagram is how it looks.
    assert_eq!(
        names("value"),
        [&base[..], &shape[..], &record[..], &["standalone"][..]].concat()
    );
    assert!(
        kinds
            .get("value")
            .unwrap()
            .field("standalone")
            .unwrap()
            .look
    );
    // An enum is a shape's in name only: rules, absent fields and a discriminator are not its.
    assert_eq!(
        names("enum"),
        [&base[..], &["open", "standalone"][..]].concat()
    );
    // A card ghost holds what it points at, by id, and nothing of a card's own.
    assert_eq!(names("ghost"), ["target"]);
    assert!(kinds.get("ghost").unwrap().field("target").unwrap().by_id);
    // What a store holds is a line of its own (`holds`).
    assert_eq!(names("store"), base);
    assert!(kinds.get("entity").unwrap().row("fields").is_some());
    assert!(kinds.get("enum").unwrap().row("fields").is_none());
    assert!(kinds.get("enum").unwrap().row("values").is_some());
    // What a kind means comes down too, its own words over its parent's.
    assert_eq!(kinds.get("entity").unwrap().semantics["editor"], "schema");
    assert_eq!(
        kinds.get("entity").unwrap().context["purpose"],
        "単位で保存・送信される記録（ID を持つ）"
    );
    // What sets value apart from entity: how it sits inside another card.
    assert_eq!(kinds.get("entity").unwrap().as_row, None);
    assert_eq!(
        kinds.get("value").unwrap().as_row,
        Some(serde_json::json!("unfold"))
    );
}

#[test]
fn a_definition_with_problems_is_refused_whole() {
    let text = json!({
        "version": 1,
        "kinds": [
            { "kind": "orphan", "extends": "nowhere", "places": ["free"] },
            {
                "kind": "broken", "places": ["moon"],
                "fields": [
                    { "name": "a", "fieldtype": "colour" },
                    { "name": "b", "fieldtype": "radio" },
                    { "name": "c", "fieldtype": "text", "when": { "z": ["x"] } },
                    { "name": "c", "fieldtype": "text" }
                ],
                "rows": [
                    { "name": "items", "fields": [{ "name": "name", "fieldtype": "text" }], "rows": [{ "ref": "elsewhere" }] },
                    { "name": "a", "address": "value", "fields": [{ "name": "x", "fieldtype": "text" }] }
                ]
            }
        ]
    })
    .to_string();
    let problems = Kinds::parse(&text).unwrap_err();
    for wanted in [
        "orphan: 受け継ぐ「nowhere」が、この種類より前にありません",
        // Where a card goes is the sections' to say (`holds`), not the card kind's.
        "broken: 「places」は定義に書けないキーです",
        "broken.fields[0]（a）: 「colour」は値の種類ではありません",
        "broken.b: radio には options が要ります",
        "broken.c: 条件の「z」は同じ所の欄にありません",
        "broken.c: 名前が重なっています",
        "broken.items: ref の「elsewhere」は上の行の種類ではありません",
        "broken: 「a」は欄と行の種類で名前が重なっています",
        "broken.a: address: value には欄 value が要ります",
    ] {
        assert!(
            problems.iter().any(|p| p == wanted),
            "wanted「{wanted}」, found {problems:#?}"
        );
    }
}

// MARK: - Documents that hold

fn entity() -> Value {
    let mut doc = card(
        "entity",
        json!({ "label": "注文", "tags": ["台帳"], "rules": [{ "kind": "uniqueBy", "fields": ["id"] }], "metadata": { "source": "web" } }),
        json!({ "fields": [
            { "id": "r1", "fields": { "name": "id", "type": "uuid", "required": true } },
            { "id": "r2", "fields": { "name": "address" } },
            { "id": "r3", "fields": { "name": "profile", "type": "object" }, "rows": { "fields": [
                { "id": "r4", "fields": { "name": "nick" } }
            ] } },
            { "id": "r5", "fields": { "name": "owner" } }
        ] }),
    );
    // What a field embeds and what its value names are the card's lines.
    doc["connections"] = json!([
        { "id": "l1", "line": "embed", "from": "address", "to": "c-address" },
        { "id": "l2", "line": "points", "from": "owner", "to": "c-user", "fields": { "cardinality": "N:1" } }
    ]);
    doc
}

fn flow() -> Value {
    card("flow", json!({ "diagram": "sequence" }), json!({}))
}

/// A node card inside the flow `c1`: its fields, its rows, its lines.
fn node(id: &str, kind: &str, name: &str, fields: Value, rows: Value, lines: Value) -> Value {
    let mut doc = json!({ "id": id, "kind": kind, "name": name, "section": "s1", "in": "c1", "v": 3,
                          "fields": fields, "rows": rows });
    if lines.as_array().is_some_and(|l| !l.is_empty()) {
        doc["connections"] = lines;
    }
    doc
}

/// The flow's nodes as cards: what they were as its rows, their ways and paths its lines.
fn nodes() -> Vec<Value> {
    vec![
        node(
            "n1",
            "begin",
            "start",
            json!({ "trigger": "receive", "carries": ["Data:Order"] }),
            json!({}),
            json!([
                { "id": "l1", "line": "peer", "to": "c-client" },
                { "id": "l2", "line": "next", "to": "n2" }
            ]),
        ),
        node(
            "n2",
            "process",
            "work",
            json!({}),
            json!({ "steps": [
                { "id": "s1", "fields": { "do": "read" } },
                { "id": "s2", "fields": { "do": "sign" } },
                { "id": "s3", "fields": { "do": "call", "program": "git", "returns": ["Data:Commit"] } }
            ] }),
            json!([
                { "id": "l3", "line": "reads", "from": "steps[0]", "to": "c-order" },
                { "id": "l4", "line": "key", "from": "steps[1]", "to": "c-keys", "at": "private", "fields": { "use": "sign" } },
                { "id": "l5", "line": "next", "to": "n3" }
            ]),
        ),
        node(
            "n3",
            "eval",
            "check",
            json!({ "value": "Data:Order.state" }),
            json!({}),
            json!([
                { "id": "l6", "line": "branch", "to": "n5", "fields": { "when": "ok" } },
                { "id": "l7", "line": "else", "to": "n5" }
            ]),
        ),
        node(
            "n4",
            "wait",
            "wait",
            json!({ "for": "event" }),
            json!({}),
            json!([
                { "id": "l8", "line": "event", "to": "c-pay", "fields": { "outcome": "success" } },
                { "id": "l9", "line": "next", "to": "n5" }
            ]),
        ),
        node(
            "n5",
            "end",
            "done",
            json!({ "outcome": "success", "return": { "carries": ["Data:Order"] } }),
            json!({}),
            json!([]),
        ),
    ]
}

fn formatdoc() -> Value {
    card(
        "formatdoc",
        json!({ "label": "仕様" }),
        json!({ "rows": [
            { "id": "f1", "fields": { "name": "auth", "heading": "認証" }, "rows": { "rows": [
                { "id": "f2", "fields": { "name": "login", "kind": "text", "body": "[[Behavior:Login]] で入る" } },
                { "id": "f3", "fields": { "name": "table", "kind": "table", "columns": ["名前", "型"], "cells": [["id", "uuid"]] } },
                { "id": "f4", "fields": { "name": "shape", "kind": "card", "target": "Data:Order" } }
            ] } }
        ] }),
    )
}

#[test]
fn a_card_of_every_kind_holds() {
    let docs = [
        entity(),
        card(
            "value",
            json!({}),
            json!({ "fields": [{ "id": "a", "fields": { "name": "city" } }] }),
        ),
        card(
            "enum",
            json!({ "open": true }),
            json!({ "values": [{ "id": "v1", "fields": { "name": "a", "label": "A" } }, { "id": "v2", "fields": { "name": "b" } }] }),
        ),
        card("store", json!({}), json!({})),
        flow(),
        nodes()[0].clone(),
        nodes()[1].clone(),
        nodes()[2].clone(),
        nodes()[3].clone(),
        nodes()[4].clone(),
        formatdoc(),
        card("freedoc", json!({ "body": "# 見出し" }), json!({})),
        card("note", json!({ "body": "メモ [[Data:Order]]" }), json!({})),
        card(
            "goal",
            json!({ "status": "inProgress", "priority": "high", "due": "2026-10-31", "workBranch": "rust", "MR": 3, "body": "完成系" }),
            json!({}),
        ),
        card(
            "task",
            json!({ "done": true, "body": "- [ ] 確かめる" }),
            json!({}),
        ),
    ];
    for doc in docs {
        assert_eq!(problems(&doc), Vec::<String>::new(), "{doc}");
    }
}

// MARK: - Documents that do not

#[test]
fn the_card_itself_is_checked() {
    says(
        &card("shape", json!({}), json!({})),
        "「shape」はカードにできない種類です（受け継ぐためだけの定義）",
    );
    says(
        &card("widget", json!({}), json!({})),
        "「widget」という種類はありません",
    );
    let mut doc = card("note", json!({}), json!({}));
    doc["name"] = json!("a\nb");
    doc["colour"] = json!("red");
    says(
        &doc,
        "名前「a\nb」は 1〜255 字で、改行・タブなどの制御文字と前後の空白を含みません",
    );
    says(&doc, "「colour」はカードの文書に書けないキーです");
    let mut doc = card("task", json!({}), json!({}));
    doc["order"] = json!("first");
    says(&doc, "order（セクションの中の並び）は整数です");
    // Where a card is written out is the project's exports', not the card's.
    doc["exportTo"] = json!([{ "dir": "d1" }]);
    says(&doc, "「exportTo」はカードの文書に書けないキーです");
    let mut doc = card("entity", json!({}), json!({}));
    doc["order"] = json!(3);
    assert_eq!(problems(&doc), Vec::<String>::new());
}

#[test]
fn fields_hold_what_they_say() {
    let doc = card(
        "goal",
        json!({ "status": "途中", "due": "2026-02-30", "MR": 0, "metadata": { "n": 1 }, "tags": ["a,b"], "colour": "red" }),
        json!({}),
    );
    says(
        &doc,
        "status: \"途中\" は選べません（notStarted・inProgress・done・cancelled）",
    );
    says(&doc, "due: 日付（YYYY-MM-DD）です");
    says(&doc, "MR: MR の番号（1 以上の整数）です");
    says(&doc, "metadata: 「n」の値は文字列です");
    says(&doc, "tags[0]: 「,」は書けません");
    says(&doc, "colour: 持てない欄です");
    let many: Vec<String> = (0..33).map(|i| format!("t{i}")).collect();
    says(
        &card("note", json!({ "tags": many }), json!({})),
        "tags: 32 個までです",
    );
    says(
        &card("note", json!({ "tags": ["a", "a"] }), json!({})),
        "tags: \"a\" が重なっています",
    );
}

#[test]
fn a_look_is_not_written_in_the_document() {
    says(
        &card("formatdoc", json!({ "view": "rows" }), json!({})),
        "view: 見かけの欄なので、カードの文書には書けません",
    );
}

#[test]
fn fields_apply_where_their_condition_holds() {
    let mut doc = nodes()[0].clone();
    doc["fields"]["trigger"] = json!("manual");
    says(&doc, "carries: trigger が receive のときだけ書けます");
    let mut doc = nodes()[1].clone();
    doc["rows"]["steps"][0]["fields"]["paths"] = json!(["Data:Order"]);
    says(
        &doc,
        "steps[0].paths: do が sign・verify のときだけ書けます",
    );
    let mut doc = formatdoc();
    doc["rows"]["rows"][0]["rows"]["rows"][0]["fields"]["heading"] = json!("見出し");
    says(
        &doc,
        "rows.auth.rows.login.heading: kind が item のときだけ書けます",
    );
}

#[test]
fn required_fields_are_required_where_they_apply() {
    says(
        &node("a", "end", "done", json!({}), json!({}), json!([])),
        "outcome: 要ります",
    );
    says(
        &node("b", "wait", "w", json!({}), json!({}), json!([])),
        "for: 要ります",
    );
    let doc = node(
        "c",
        "process",
        "work",
        json!({}),
        json!({ "steps": [
            { "id": "d", "fields": { "do": "call" } },
            { "id": "e", "fields": { "do": "verify" } }
        ] }),
        json!([]),
    );
    says(&doc, "steps[0].program: 要ります");
    // verify's paths are optional, and its key is a line.
    assert!(!problems(&doc).iter().any(|p| p.starts_with("steps[1]")));
    // A node's ways and paths are its lines, not fields of its own.
    says(
        &node(
            "f",
            "run",
            "go",
            json!({ "target": "Behavior:Pay", "next": ["x"] }),
            json!({}),
            json!([]),
        ),
        "target: 持てない欄です",
    );
    // An embedding is a line (`connections`), not a type a field is written in.
    let doc = card(
        "entity",
        json!({}),
        json!({ "fields": [{ "id": "a", "fields": { "name": "to", "type": "ref" } }] }),
    );
    says(
        &doc,
        "fields.to.type: \"ref\" は選べません（string・text・int・float・decimal・bool・uuid・url・email・date・datetime・json・object）",
    );
}

#[test]
fn rows_sit_where_they_may_and_no_deeper() {
    let doc = card(
        "entity",
        json!({}),
        json!({ "fields": [{ "id": "a", "fields": { "name": "flat" }, "rows": { "fields": [{ "id": "b", "fields": { "name": "x" } }] } }] }),
    );
    says(
        &doc,
        "fields.flat.fields: type が object のときだけ置けます",
    );
    let mut deep = json!({ "id": "f6", "fields": { "name": "f6", "type": "object" } });
    for level in (1..=5).rev() {
        deep = json!({ "id": format!("f{level}"), "fields": { "name": format!("f{level}"), "type": "object" }, "rows": { "fields": [deep] } });
    }
    says(
        &card("entity", json!({}), json!({ "fields": [deep] })),
        "fields.f1.fields.f2.fields.f3.fields.f4.fields.f5.fields: 5 段までです",
    );
    says(
        &node(
            "a",
            "end",
            "end",
            json!({ "outcome": "success" }),
            json!({ "steps": [] }),
            json!([]),
        ),
        "「steps」は end が持てる行の種類ではありません",
    );
    says(
        &card("flow", json!({}), json!({ "nodes": [] })),
        "「nodes」は flow が持てる行の種類ではありません",
    );
    says(
        &card("note", json!({}), json!({ "fields": [] })),
        "「fields」は note が持てる行の種類ではありません",
    );
}

#[test]
fn rows_beside_one_another_differ() {
    let doc = card(
        "enum",
        json!({}),
        json!({ "values": [{ "id": "a", "fields": { "name": "x" } }, { "id": "a", "fields": { "name": "x" } }] }),
    );
    says(&doc, "values: 値「x」が重なっています");
    says(&doc, "values.x: 行の id「a」が重なっています");
}

// MARK: - The wire

fn names() -> Project {
    Project::new(&[
        ("c-address", "Data:Address"),
        ("c-user", "Core:User"),
        ("t1", "Goals/Rust:Read"),
    ])
}

#[test]
fn a_document_reads_as_mcp_has_always_written_it() {
    let wire = to_wire(builtin().get("entity").unwrap(), &entity(), &names());
    assert_eq!(
        wire,
        json!({
            "kind": "entity",
            "label": "注文",
            "tags": ["台帳"],
            "metadata": { "source": "web" },
            "rules": [{ "kind": "uniqueBy", "fields": ["id"] }],
            "fields": [
                { "name": "id", "type": "uuid", "required": true },
                { "name": "address" },
                { "name": "profile", "type": "object", "fields": [{ "name": "nick" }] },
                { "name": "owner" }
            ]
        })
    );
    // Its lines apart, the card each reaches by its path, no ids.
    assert_eq!(
        connections_to_wire(&entity(), &names()),
        Some(json!([
            { "line": "embed", "from": "address", "to": "Data:Address" },
            { "line": "points", "from": "owner", "to": "Core:User", "fields": { "cardinality": "N:1" } }
        ]))
    );
}

fn back(kind: &str, wire: &Value, prior: Option<&Value>) -> Result<Parts, Vec<String>> {
    let wire = wire.as_object().unwrap();
    from_wire(
        builtin().get(kind).unwrap(),
        wire,
        prior,
        &names(),
        &mut counter(),
    )
    .map_err(|problems| problems.iter().map(ToString::to_string).collect())
}

#[test]
fn the_wire_reads_back_into_the_same_document() {
    for doc in [
        entity(),
        flow(),
        nodes()[1].clone(),
        nodes()[4].clone(),
        formatdoc(),
    ] {
        let kind = builtin().get(doc["kind"].as_str().unwrap()).unwrap();
        let wire = to_wire(kind, &doc, &names());
        let parts = back(&kind.name, &wire, Some(&doc)).unwrap();
        assert_eq!(Value::Object(parts.fields), doc["fields"]);
        assert_eq!(Value::Object(parts.rows), doc["rows"]);
    }
}

#[test]
fn rows_keep_their_ids_by_name_and_new_ones_are_given_one() {
    let wire = json!({ "fields": [
        { "name": "total", "type": "int" },
        { "name": "id", "type": "uuid", "required": true },
        { "name": "profile", "type": "object", "fields": [{ "name": "nick" }, { "name": "age", "type": "int" }] }
    ] });
    let parts = back("entity", &wire, Some(&entity())).unwrap();
    let ids: Vec<&str> = parts.rows["fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["new1", "r1", "r3"]);
    let inner: Vec<&str> = parts.rows["fields"][2]["rows"]["fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap())
        .collect();
    assert_eq!(inner, ["r4", "new2"]);
}

#[test]
fn steps_keep_their_ids_by_place() {
    let wire = json!({ "steps": [
        { "do": "write" },
        { "do": "read" }
    ] });
    let parts = back("process", &wire, Some(&nodes()[1])).unwrap();
    let steps: Vec<&str> = parts.rows["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap())
        .collect();
    assert_eq!(steps, ["s1", "s2"]);
    assert_eq!(parts.rows["steps"][0]["fields"]["do"], "write");
}

#[test]
fn the_wire_sets_aside_how_a_card_looks_and_refuses_what_it_cannot_hold() {
    let parts = back(
        "formatdoc",
        &json!({ "kind": "formatdoc", "label": "仕様", "view": "rows" }),
        None,
    )
    .unwrap();
    assert_eq!(Value::Object(parts.looks), json!({ "view": "rows" }));
    assert_eq!(Value::Object(parts.fields), json!({ "label": "仕様" }));
    let refused = back("entity", &json!({ "kind": "flow", "body": "x", "fields": [{ "name": "a", "type": "ref", "ref": "Data:Nowhere" }] }), None)
        .unwrap_err();
    for wanted in [
        "kind: \"flow\" ではなく entity です",
        "body: entity が持てないキーです",
        // An embedding is a line of the card's now.
        "fields[0]: 「ref」はフィールドの行に書けないキーです",
    ] {
        assert!(
            refused.iter().any(|p| p == wanted),
            "wanted「{wanted}」, found {refused:#?}"
        );
    }
}

/// The board's sections are told apart in the nav panel: the ones finished and cancelled are marked
/// unlike Goals and unlike each other.
#[test]
fn the_boards_sections_are_marked_apart() {
    let icon = |name: &str| {
        builtin_written("sections")
            .iter()
            .find(|s| s["section"] == name)
            .map(|s| s["icon"].clone())
            .unwrap_or_else(|| panic!("{name}"))
    };
    let (goals, complete, cancelled) = (icon("goals"), icon("complete"), icon("cancelled"));
    assert_ne!(complete["name"], goals["name"]);
    assert_ne!(cancelled["name"], goals["name"]);
    assert_ne!(cancelled["name"], complete["name"]);
    assert_ne!(cancelled["color"], complete["color"]);
}

// MARK: - Lines

/// Lines that may meet on one map are told apart by how they are drawn: no two built-in kinds share
/// a colour, a dash and both ends.
#[test]
fn lines_that_meet_on_a_map_look_apart() {
    let defs = builtin_defs();
    let lines: Vec<&LineDef> = defs.lines.iter().collect();
    for (i, a) in lines.iter().enumerate() {
        for b in &lines[i + 1..] {
            assert_ne!(a.look, b.look, "{} と {} が同じ見た目", a.name, b.name);
        }
    }
}

#[test]
fn a_cards_lines_hold_what_their_kind_says() {
    let defs = builtin_defs();
    assert_eq!(check_connections(defs, &entity()), Vec::<Problem>::new());
    let mut doc = entity();
    doc["connections"] = json!([
        { "id": "a", "line": "points", "from": "owner", "to": "c-user", "fields": { "cardinality": "many" } },
        { "id": "a", "line": "sideways", "to": "c-user" },
        { "line": "embed", "from": "address" },
        { "id": "b", "line": "mentions", "to": "c-user" },
        { "id": "c", "line": "link", "to": "c-user", "colour": "red" },
        { "id": "d", "line": "points", "from": "owner", "fields": { "discriminatedBy": "artifact_type" } }
    ]);
    let found: Vec<String> = check_connections(defs, &doc)
        .iter()
        .map(ToString::to_string)
        .collect();
    for wanted in [
        "connections[0].fields.cardinality: \"many\" は選べません（1:1・1:N・N:1・N:N）",
        "connections[1]: 線の id「a」が重なっています",
        "connections[1]: 「sideways」という線の種類はありません（あるのは embed・points・link・holds・rules・implementedBy・next・condition・else・onError・onTimeout・runs・awaits・reads・writes・partner・key）",
        "connections[2]: 線の id がありません",
        "connections[2]: to（相手のカード）がありません",
        "connections[3]: mentions（言及）は本文の [[ ]] から拾う線なので、connections には書きません",
        "connections[4]: 「colour」は線に書けないキーです",
    ] {
        assert!(
            found.iter().any(|p| p == wanted),
            "wanted「{wanted}」, found {found:#?}"
        );
    }
    // A pointer told apart by its discriminator names no card.
    assert!(!found.iter().any(|p| p.starts_with("connections[5]")));
    // A card ghost holds none.
    let mut ghost = card("ghost", json!({ "target": "c-user" }), json!({}));
    ghost.as_object_mut().unwrap().remove("rows");
    ghost["connections"] = json!([]);
    says(
        &ghost,
        "カードゴーストは線（connections）を持ちません（線は指す先の本物に書きます）",
    );
}

#[test]
fn lines_keep_their_ids_by_what_they_are_and_name_cards_by_path_on_the_wire() {
    let wire = json!([
        { "line": "points", "from": "owner[]", "to": "Core:User", "at": "Core:User.id", "fields": { "cardinality": "1:1" } },
        { "line": "embed", "from": "address", "to": "Data:Address", "description": "  届け先 " },
        { "line": "link", "to": "Core:User" }
    ]);
    let read = connections_from_wire(&wire, Some(&entity()), &names(), &mut counter()).unwrap();
    assert_eq!(
        Value::Array(read),
        json!([
            // `[]` is notation; a landing in full in the card reached is its names there.
            { "id": "new1", "line": "points", "from": "owner", "to": "c-user", "at": "id", "fields": { "cardinality": "1:1" } },
            { "id": "l1", "line": "embed", "from": "address", "to": "c-address", "description": "届け先" },
            { "id": "new2", "line": "link", "to": "c-user" }
        ])
    );
    let refused = connections_from_wire(
        &json!([{ "line": "points", "to": "Core:Nobody", "colour": 1 }]),
        None,
        &names(),
        &mut counter(),
    )
    .unwrap_err();
    let refused: Vec<String> = refused.iter().map(ToString::to_string).collect();
    assert!(
        refused.contains(&"connections[0].to: 「Core:Nobody」というカードはありません".to_string())
    );
    assert!(refused.contains(
        &"connections[0]: 「colour」は線に書けないキーです（書けるのは line・from・to・at・description・fields）".to_string()
    ));
}

#[test]
fn a_card_inside_a_container_keeps_it_by_id() {
    // What a goal holds is cards inside it (`in`), not a list of its own.
    says(
        &card(
            "goal",
            json!({ "status": "進行中", "items": ["t1"] }),
            json!({}),
        ),
        "items: 持てない欄です",
    );
    let mut task = card("task", json!({}), json!({}));
    task["in"] = json!("g1");
    assert_eq!(problems(&task), Vec::<String>::new());
    task["in"] = json!("");
    says(&task, "in（入れ物のカードの id）は空でない文字列です");
}

#[test]
fn the_rules_a_write_brings_are_held_to_their_kinds() {
    // A card's own rule and a field's, at depth: each by where it is.
    let doc = card(
        "entity",
        json!({ "rules": [{ "kind": "uniqueBy", "fields": ["id"] }, { "kind": "mine", "x": 1 }] }),
        json!({ "fields": [{ "id": "r1", "fields": { "name": "profile", "type": "object", "rules": [{ "kind": "pattern" }] },
            "rows": { "fields": [{ "id": "r2", "fields": { "name": "nick", "rules": [{ "kind": "sortedBy", "keys": "a", "order": "up" }] } }] } }] }),
    );
    let said: Vec<String> = rule_problems(&doc, &|_| true)
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(
        said,
        [
            "rules[0]: rule「uniqueBy」に fields は書けません（書けるのは by・keys・field・note。形は guide の about: rules）",
            "profile.rules[0]: rule「pattern」には regex が要ります",
            "profile.nick.rules[0]: rule「sortedBy」の keys は欄の名前の並びです",
            "profile.nick.rules[0]: rule「sortedBy」の order は asc / desc のいずれかです",
        ]
    );
    // What the card held before as it was is not asked again: a project written before the
    // rules were checked keeps being written.
    let before = rules_held(&doc);
    assert_eq!(before.len(), 4);
    assert!(rule_problems(&doc, &|rule| !before.contains(rule)).is_empty());
    // Nor are they the card's problems (project_read's cardProblems, the move of a store).
    assert_eq!(problems(&doc), Vec::<String>::new());
}

#[test]
fn a_flows_via_takes_the_same_values_wherever_a_node_says_it() {
    let via = |kind: &str| -> Vec<String> {
        let kind = builtin().get(kind).unwrap();
        kind.fields
            .iter()
            .chain(kind.rows.iter().flat_map(|r| r.fields.iter()))
            .find(|f| f.name == "via")
            .map(|f| f.options.clone())
            .unwrap_or_else(|| panic!("{} に via が無い", kind.name))
    };
    for kind in VIA_KINDS {
        assert_eq!(via(kind), builtin_vias(), "{kind}");
    }
    // A project's own after them, in each.
    let defs = Defs::of_project(Some(r#"{"vias": ["kafka"]}"#));
    let mut all = builtin_vias().to_vec();
    all.push("kafka".to_string());
    assert_eq!(defs.vias, all);
    for kind in VIA_KINDS {
        let kind = defs.cards().get(kind).unwrap();
        let options: Vec<&Vec<String>> = kind
            .fields
            .iter()
            .chain(kind.rows.iter().flat_map(|r| r.fields.iter()))
            .filter(|f| f.name == "via")
            .map(|f| &f.options)
            .collect();
        assert_eq!(options, [&all], "{}", kind.name);
    }
}

// MARK: - The built-in definitions in each language

/// The values cards keep that a text in another language may quote as they are: a goal's status
/// and priority.
const KEPT_VALUES: [&str; 7] = ["未着手", "進行中", "完了", "打ち切り", "高", "中", "低"];

fn written_root() -> Value {
    serde_json::from_str(BUILTIN).unwrap()
}

#[test]
fn every_language_but_japanese_has_words_for_the_built_in_kinds() {
    let with_words: Vec<&str> = overlay::languages().collect();
    for code in crate::text::LANGUAGES {
        assert_eq!(
            with_words.contains(code),
            *code != "ja",
            "kinds/text/{code}.json: every language but Japanese (builtin.json's) has one"
        );
    }
    for code in with_words {
        assert!(
            crate::text::LANGUAGES.contains(&code),
            "kinds/text/{code}.json is not a language of LANGUAGES"
        );
    }
}

#[test]
fn every_japanese_text_of_the_built_in_kinds_has_words_in_every_language() {
    let root = written_root();
    for code in overlay::languages() {
        let overs = overlay::overs(code).unwrap();
        let missing: Vec<String> = overlay::places(&root)
            .into_iter()
            .filter(|p| crate::text::has_japanese(&p.japanese))
            .filter(|p| crate::text::over(overs, &p.name, &p.japanese).is_none())
            .map(|p| format!("{}: {}", p.name, p.japanese))
            .collect();
        assert!(
            missing.is_empty(),
            "kinds/text/{code}.json has no words for these (or words made from what they said before): \
             {missing:#?}"
        );
    }
}

#[test]
fn no_words_of_the_built_in_kinds_are_left_for_a_text_no_more_written() {
    let places = overlay::places(&written_root());
    for code in overlay::languages() {
        let mut stale: Vec<String> = overlay::overs(code)
            .unwrap()
            .iter()
            .filter(|(name, over)| {
                !places
                    .iter()
                    .any(|p| &p.name == *name && p.japanese == over.from)
            })
            .map(|(name, over)| format!("{name}: made from「{}」", over.from))
            .collect();
        stale.sort();
        assert!(
            stale.is_empty(),
            "kinds/text/{code}.json has words for texts builtin.json no longer has: {stale:#?}"
        );
    }
}

#[test]
fn the_words_of_the_built_in_kinds_in_english_have_no_japanese_but_values_cards_keep() {
    let mut found: Vec<String> = overlay::overs("en")
        .unwrap()
        .iter()
        .filter(|(_, over)| {
            let mut text = over.text.clone();
            for value in KEPT_VALUES {
                text = text.replace(value, "");
            }
            crate::text::has_japanese(&text)
        })
        .map(|(name, over)| format!("{name}: {}", over.text))
        .collect();
    found.sort();
    assert!(found.is_empty(), "{found:#?}");
}

#[test]
fn the_built_in_kinds_are_said_in_the_language_of_the_call() {
    let said = |lang: &str| {
        let _lang = crate::text::bind(Some(lang));
        let entity = builtin().get("entity").unwrap();
        let field = builtin().get("record").unwrap().rows[0].fields[0].clone();
        let section = builtin_defs().section_label("schema");
        let line = defs::builtin_written("lines")
            .iter()
            .find(|l| l["line"] == "partner")
            .unwrap()["label"]
            .clone();
        let purpose = builtin_definition("flow").unwrap()["context"]["purpose"].clone();
        (
            entity.label.clone(),
            field.label,
            section,
            line,
            purpose.as_str().unwrap().to_string(),
        )
    };
    let (entity, field, section, line, purpose) = said("ja");
    assert_eq!((entity.as_str(), field.as_str()), ("記録", "名前"));
    assert_eq!(
        (section.as_str(), line.as_str()),
        ("スキーマ", Some("相手"))
    );
    assert!(crate::text::has_japanese(&purpose), "{purpose}");
    let (entity, field, section, line, purpose) = said("en");
    assert_eq!((entity.as_str(), field.as_str()), ("Record", "Name"));
    assert_eq!(
        (section.as_str(), line.as_str()),
        ("Schema", Some("Partner"))
    );
    assert!(!crate::text::has_japanese(&purpose), "{purpose}");
}

#[test]
fn a_choice_is_shown_in_the_language_and_keeps_the_values_cards_keep() {
    let status = |lang: &str| {
        let _lang = crate::text::bind(Some(lang));
        let field = builtin().get("goal").unwrap().field("status").unwrap();
        let written = builtin_definition("goal").unwrap()["fields"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["name"] == "status")
            .unwrap()
            .clone();
        (field.options.clone(), field.default.clone(), written)
    };
    let (options, default, written) = status("ja");
    assert_eq!(options, ["notStarted", "inProgress", "done", "cancelled"]);
    assert_eq!(default, Some(json!("notStarted")));
    assert_eq!(
        written["optionLabels"],
        json!({"notStarted": "未着手", "inProgress": "進行中", "done": "完了", "cancelled": "打ち切り"})
    );
    let (options, default, written) = status("en");
    assert_eq!(options, ["notStarted", "inProgress", "done", "cancelled"]);
    assert_eq!(default, Some(json!("notStarted")));
    assert_eq!(written["done"], json!(["done"]));
    assert_eq!(
        written["optionLabels"],
        json!({"notStarted": "Not started", "inProgress": "In progress", "done": "Done", "cancelled": "Cancelled"})
    );
}

#[test]
fn a_choice_kept_in_english_is_shown_by_labels_said_in_the_language() {
    let place = |lang: &str| {
        let _lang = crate::text::bind(Some(lang));
        builtin_definition("html").unwrap()["fields"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["name"] == "notePlace")
            .unwrap()
            .clone()
    };
    let ja = place("ja");
    assert_eq!(ja["default"], "topRight");
    assert_eq!(ja["optionLabels"]["topLeft"], "左上");
    assert_eq!(ja["optionLabels"]["hidden"], "隠す");
    let en = place("en");
    assert_eq!(en["options"], ja["options"]);
    assert_eq!(en["optionLabels"]["topLeft"], "Top left");
    assert_eq!(en["optionLabels"]["hidden"], "Hidden");
    assert_eq!(en["label"], "Note place");
}

#[test]
fn a_parts_header_place_is_a_corner_or_hidden_and_top_left_unless_written() {
    for part in ["html", "svg", "frame"] {
        let field = |lang: &str| {
            let _lang = crate::text::bind(Some(lang));
            let fields = builtin_definition(part).unwrap()["fields"]
                .as_array()
                .unwrap()
                .clone();
            let at = fields
                .iter()
                .position(|f| f["name"] == "headerPlace")
                .unwrap();
            // Said beside the note's fields, before them.
            assert_eq!(fields[at + 1]["name"], "note", "{part}");
            fields[at].clone()
        };
        let ja = field("ja");
        assert_eq!(ja["fieldtype"], "radio", "{part}");
        assert_eq!(
            ja["options"],
            json!(["topLeft", "topRight", "bottomLeft", "bottomRight", "hidden"]),
            "{part}"
        );
        assert_eq!(ja["default"], "topLeft", "{part}");
        assert_eq!(ja["label"], "見出しの位置", "{part}");
        assert_eq!(
            ja["optionLabels"],
            json!({"topLeft": "左上", "topRight": "右上", "bottomLeft": "左下", "bottomRight": "右下", "hidden": "隠す"}),
            "{part}"
        );
        let en = field("en");
        assert_eq!(en["options"], ja["options"], "{part}");
        assert_eq!(en["label"], "Header place", "{part}");
        assert_eq!(
            en["optionLabels"],
            json!({"topLeft": "Top left", "topRight": "Top right", "bottomLeft": "Bottom left", "bottomRight": "Bottom right", "hidden": "Hidden"}),
            "{part}"
        );
    }
}

#[test]
fn a_frames_face_is_its_picture_alone() {
    let frame = builtin().get("frame").unwrap();
    let face = frame.face.as_ref().unwrap();
    assert!(face.top.is_empty());
    assert!(face.markdown.is_none() && face.page.is_none());
}

#[test]
fn a_projects_own_kind_may_say_how_its_choices_are_shown() {
    let own = json!([{
        "kind": "risk",
        "fields": [{"name": "severity", "fieldtype": "radio", "options": ["high", "low"],
            "optionLabels": {"high": "High", "low": "Low"}}]
    }]);
    let mut problems = Vec::new();
    let kinds = builtin().with_own(Some(&own), &mut problems);
    assert!(problems.is_empty(), "{problems:?}");
    assert!(kinds.get("risk").is_some());
    let wrong = json!([{
        "kind": "risk",
        "fields": [{"name": "severity", "fieldtype": "radio", "options": ["high"],
            "optionLabels": {"medium": "Medium"}}]
    }]);
    let mut problems = Vec::new();
    builtin().with_own(Some(&wrong), &mut problems);
    assert!(
        problems.iter().any(|p| p.contains("optionLabels")),
        "{problems:?}"
    );
}

#[test]
fn the_actions_of_a_kind_are_the_cores_and_only_the_built_in_kinds_name_them() {
    assert_eq!(
        builtin().get("goal").unwrap().actions,
        ["start", "finish", "cancel"]
    );
    assert_eq!(builtin().get("task").unwrap().actions, ["done"]);
    for part in ["html", "svg", "frame"] {
        assert_eq!(builtin().get(part).unwrap().actions, ["reviewed"], "{part}");
    }
    assert!(builtin().get("note").unwrap().actions.is_empty());
    // What an action does is the core's: a project's own kind names none.
    let own = json!([{ "kind": "chore", "actions": ["done"] }]);
    let mut problems = Vec::new();
    builtin().with_own(Some(&own), &mut problems);
    assert!(
        problems.iter().any(|p| p.contains("actions")),
        "{problems:?}"
    );
    // Every action a built-in kind names has its words.
    let written = builtin_root()["actions"].as_array().unwrap();
    for op in crate::kinds::ACTIONS {
        let one = written.iter().find(|a| a["action"] == op).unwrap();
        assert!(one["label"].is_string() && one["description"].is_string());
    }
}

/// A line lands at a function or a type only where its end reaches files: they are inside one.
#[test]
fn a_line_lands_at_a_function_only_where_it_reaches_files() {
    let own = |to: Value| {
        json!({"lines": [{
            "line": "codedBy", "label": "コード", "base": "link",
            "from": [{"kinds": ["*"], "at": ["card"]}], "to": to
        }]})
    };
    let problems = Defs::checked(&own(json!({"kinds": ["entity"], "at": ["symbol"]}))).unwrap_err();
    assert!(
        problems
            .iter()
            .any(|p| p.contains("symbol") && p.contains("file")),
        "{problems:#?}"
    );
    Defs::checked(&own(json!({"kinds": ["file"], "at": ["card", "symbol"]}))).unwrap();
}

#[test]
fn every_boxed_card_may_carry_a_size() {
    let kinds = builtin();
    for name in [
        "entity",
        "value",
        "enum",
        "store",
        "instance",
        "table",
        "flow",
        "note",
        "freedoc",
        "formatdoc",
        "stackdoc",
        "svg",
        "html",
        "frame",
        "goal",
        "task",
    ] {
        assert!(kinds.sizable(name), "{name}");
    }
    // A flow's node is drawn by its node's shape, a card ghost at its card's size, and what the
    // app alone draws is laid out by it.
    for name in [
        "begin", "process", "run", "eval", "wait", "join", "end", "node", "ghost", "section",
        "file", "symbol", "base",
    ] {
        assert!(!kinds.sizable(name), "{name}");
    }
    let own = json!({ "cards": [
        { "kind": "risk", "fields": [{ "name": "memo", "fieldtype": "markdown" }],
          "face": { "markdown": "memo", "resizable": false } },
        { "kind": "plain" }
    ] });
    let defs = Defs::checked(&own).unwrap();
    assert!(defs.cards().sizable("risk") && defs.cards().sizable("plain"));
}
