//! A section's own mark (`icon`, `specboard://SpecBoard/Docs/Design:Overview` §9「セクションの印」): any
//! section but a working directory's may carry one, written and checked as a kind's mark is, each part
//! it leaves out taken from its kind's — an actor's by what it is.

use crate::actor_sections::shop_core;
use crate::common;

use common::{ok, op, refused, script_store};
use serde_json::json;
use specboard_core::Core;

#[test]
fn a_section_keeps_its_own_mark_and_takes_its_kinds_for_what_it_leaves_out() {
    let core = shop_core();
    let marked = [
        ("Charts/シナリオ", json!({"name": "testtube.2"})),
        ("Charts/利用者", json!({"color": "#FF9F0A"})),
        (
            "Charts/注文サーバ/受付",
            json!({"name": "key", "color": "#FF9F0A"}),
        ),
    ];
    for (target, icon) in &marked {
        ok(
            &core,
            "section_update",
            json!({"target": target, "value": {"icon": icon}}),
        );
    }
    let read = |target: &str| ok(&core, "section_read", json!({"target": target}));
    for (target, icon) in &marked {
        assert_eq!(read(target)["icon"], *icon, "{target}");
    }
    assert_eq!(
        read("Charts/シナリオ")["shownMark"],
        json!({"name": "testtube.2", "color": "#FF3B30"})
    );
    assert_eq!(
        read("Charts/利用者")["shownMark"],
        json!({"name": "person", "color": "#FF9F0A"})
    );
    assert_eq!(
        read("Charts/注文サーバ/受付")["shownMark"],
        json!({"name": "key", "color": "#FF9F0A"})
    );

    // Taken away, the kind's mark again: a system's.
    ok(
        &core,
        "section_update",
        json!({"target": "Charts/注文サーバ/受付", "value": {"icon": null}}),
    );
    let desk = read("Charts/注文サーバ/受付");
    assert!(desk.get("icon").is_none(), "{desk}");
    assert_eq!(
        desk["shownMark"],
        json!({"name": "cube", "color": "#0A84FF"})
    );
    let history = ok(
        &core,
        "branch_read",
        json!({"target": "main", "value": {"as": "history", "limit": 1}}),
    );
    assert_eq!(
        history["commits"][0]["lines"][0],
        "セクション Charts/注文サーバ/受付 を更新（印を変更）"
    );
}

#[test]
fn a_section_mark_is_checked_as_a_kinds_mark_is() {
    let core = shop_core();
    let at = "Charts/シナリオ";
    let wrong = |icon: serde_json::Value| {
        refused(
            &core,
            "section_update",
            json!({"target": at, "value": {"icon": icon}}),
        )
    };
    assert_eq!(
        wrong(json!("key")),
        format!(
            "{at}: icon は {{name, color}} の形です（前の書き方の icon: \"<名前>\" と color は、icon: {{\"name\": \"<名前>\", \"color\": \"#RRGGBB\"}} に書き直します）"
        )
    );
    assert_eq!(
        wrong(json!({"name": "Key"})),
        format!(
            "{at}: icon の name は SF Symbols の名前です（小文字・数字・. の 80 字まで。例 folder・checklist）（\"Key\"）"
        )
    );
    assert_eq!(
        wrong(json!({"name": "key", "color": "red"})),
        format!("{at}: icon の color は #RRGGBB です（\"red\"）")
    );
    assert_eq!(
        wrong(json!({"name": "key", "size": 2})),
        format!("{at}: icon に「size」は書けません（書けるのは name・color）")
    );
}

#[test]
fn a_working_directory_section_takes_no_mark() {
    let core = Core::open(script_store()).unwrap();
    // The working directory `repo`, bound to a folder holding `src`.
    let folder = common::place("repo");
    std::fs::create_dir_all(folder.join("src")).unwrap();
    std::fs::write(folder.join("src/a.rs"), "fn a() {}").unwrap();
    // Read as git reads a repository.
    std::fs::create_dir_all(folder.join(".git")).unwrap();
    std::fs::write(folder.join(".git/HEAD"), "ref: refs/heads/main").unwrap();
    let mut tree = ok(&core, "project_read", json!({}))["projectTree"].clone();
    tree["main_sections"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type": "workdir", "name": "repo"}));
    ok(
        &core,
        "project_update",
        json!({"value": {"project_tree": tree}}),
    );
    let bound = op(
        &core,
        json!({"op": "tool", "name": "section_update", "arguments": {
            "project": common::PROJECT, "target": "repo",
            "value": {"folder": folder.to_string_lossy()}}}),
    );
    assert!(bound.get("error").is_none(), "{bound}");
    for target in ["repo", "repo/src"] {
        let said = refused(
            &core,
            "section_update",
            json!({"target": target, "value": {"icon": {"name": "key"}}}),
        );
        assert!(
            said.starts_with("作業ディレクトリのセクションの印は種類の定義で変えます"),
            "{target}: {said}"
        );
    }
}
