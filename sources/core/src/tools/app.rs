//! `app_control`: the one tool the core does not answer itself. It reads and moves the
//! window and takes pictures of it, so the app answers it (`crate::App`, handed over with
//! `Core::set_app`; the Swift app's `AppControl`). What the core does is what it does for
//! every tool: the key may do what the op does with the window — choose and change how it
//! looks, or take a picture of it, as a person allowed it or says each time
//! (`mcp_writes::judge`) — the arguments fit the tool's schema (`mcp::complaints`), the value
//! fits its op's form (`check`) — and then the app's answer read back as the tool's. A `select`
//! is looked up too (`select_checked`): the window moves only for one it can do.

use super::{Args, Seen};
use crate::Core;
use crate::access;
use crate::mcp::Output;
use serde_json::{Map, Value, json};

/// What `app_control` is told while no app is there to answer it: a core opened without the
/// window (the agent, a test), or the window gone.
pub(crate) fn no_app() -> String {
    text!("app.noApp")
}

impl Core {
    /// `app_control`, checked and handed to the app.
    pub(crate) fn app_control(&self, arguments: &Map<String, Value>) -> Result<Output, String> {
        let args = Args::new(arguments);
        let value = args.value()?;
        let op = args.string("op")?;
        super::check(&value, "app_control", &format!("op: {op}"))?;
        in_range(&value, op)?;
        let mut arguments = arguments.clone();
        if op == "select"
            && let Some(project) = args.optional_string("project")
        {
            let tree = self.select_checked(project, &args)?;
            arguments.insert("branch".into(), Value::String(tree));
        }
        let Some(app) = self.app() else {
            return Err(no_app());
        };
        let caller = match access::caller() {
            Some(c) => json!({
                "id": c.key,
                "name": c.name,
            }),
            None => Value::Null,
        };
        let said = app.app_control(&Value::Object(arguments).to_string(), &caller.to_string());
        read(&said)
    }

    /// The tree a `select` opens — the one its `branch` names, main when it names none, as every
    /// tool reads it — and its target looked for in that tree alone: a path the window would not
    /// find there is refused here, before the app is asked, so the window does not move for it.
    /// The tree goes to the app by the name it is called by.
    fn select_checked(&self, project: &str, args: &Args) -> Result<String, String> {
        let store = self.reading()?;
        let seen = Seen::read(&store, project, args.optional_string("branch"))?;
        let Some(target) = args.targets()?.and_then(|t| t.into_iter().next()) else {
            return Ok(seen.named.name);
        };
        let target = super::check::trim_spaces(&target).to_string();
        let tree = seen.project();
        // A name without a section is the section it names, when there is one; the window reads it so.
        let section =
            (!crate::pathtext::has_section(&target)).then(|| super::section_id(&tree, &target));
        if let Some(Ok(_)) = section {
            return Ok(seen.named.name.clone());
        }
        // A card ghost's own path is the ghost — the window picks it, as a press on its header
        // does — even where it leads nowhere.
        let ghost = crate::export::paths::WrittenPath::new(&target).is_some_and(|w| {
            w.names.is_empty() && tree.directory.find_entry(&w.card, None).is_some()
        });
        let hit = super::check::resolve_here(&tree, &target);
        if ghost || hit.ok {
            return Ok(seen.named.name.clone());
        }
        let why = match section {
            Some(Err(why)) => why,
            _ => hit.hint.unwrap_or_else(|| text!("check.path.notFound")),
        };
        Err(text!(
            "app.select.notThere",
            path = target,
            tree = seen.named.name,
            why = why
        ))
    }
}

/// How far from 0 a point or an area handed to the window may be (the diagram's coordinates): past
/// it, the window would only draw nothing.
const COORDINATE_MOST: f64 = 10_000_000.0;

/// The numbers an op hands the window, each in the range the window takes it in — a depth of the
/// connections map, a margin, a zoom, a picture's size, a point and an area: one past it is refused
/// here, so the window is never handed a number it cannot hold (`1e300` as a whole number).
fn in_range(value: &Map<String, Value>, op: &str) -> Result<(), String> {
    let whole = |key: &str, from: i64, to: i64| -> Result<(), String> {
        let Some(given) = value.get(key) else {
            return Ok(());
        };
        let fits = given
            .as_f64()
            .is_some_and(|n| n.fract() == 0.0 && n >= from as f64 && n <= to as f64);
        if fits {
            Ok(())
        } else {
            Err(text!("app.range.whole", key, from, to, given))
        }
    };
    let number = |key: String, given: Option<&Value>, from: f64, to: f64| {
        let Some(given) = given else {
            return Ok(());
        };
        if given.as_f64().is_some_and(|n| n >= from && n <= to) {
            Ok(())
        } else {
            Err(text!("app.range.number", key, from, to, given))
        }
    };
    match op {
        "select" => whole("depth", 0, 2),
        "view" => {
            whole("margin", 0, 400)?;
            number("zoom".into(), value.get("zoom"), 0.02, 2.5)?;
            for (at, keys) in [
                ("area", &["x", "y", "width", "height"][..]),
                ("point", &["x", "y"][..]),
            ] {
                let Some(Value::Object(inside)) = value.get(at) else {
                    continue;
                };
                for key in keys {
                    number(
                        format!("{at}.{key}"),
                        inside.get(*key),
                        -COORDINATE_MOST,
                        COORDINATE_MOST,
                    )?;
                }
            }
            Ok(())
        }
        "capture" => whole("maxSize", 400, 3000),
        _ => Ok(()),
    }
}

/// The app's answer as the tool's: `{"json": …}`, `{"blocks": […]}`,
/// `{"image": {"data", "mimeType"}, "note": …}`, or `{"error": …}`.
fn read(said: &str) -> Result<Output, String> {
    let unreadable = || {
        let head: String = said.chars().take(200).collect();
        text!("app.unreadable", head)
    };
    let Ok(Value::Object(mut answer)) = serde_json::from_str::<Value>(said) else {
        return Err(unreadable());
    };
    if let Some(error) = answer.remove("error") {
        return Err(match error {
            Value::String(text) => text,
            other => other.to_string(),
        });
    }
    if let Some(value) = answer.remove("json") {
        return Ok(Output::Json(value));
    }
    if let Some(Value::Array(blocks)) = answer.get("blocks") {
        let texts: Option<Vec<String>> = blocks
            .iter()
            .map(|b| b.as_str().map(str::to_string))
            .collect();
        return texts.map(Output::Blocks).ok_or_else(unreadable);
    }
    if let Some(Value::Object(image)) = answer.get("image")
        && let (Some(Value::String(data)), Some(Value::String(mime_type))) =
            (image.get("data"), image.get("mimeType"))
    {
        return Ok(Output::Image {
            data: data.clone(),
            mime_type: mime_type.clone(),
            note: answer.get("note").cloned().unwrap_or(Value::Null),
        });
    }
    Err(unreadable())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_apps_answers_are_read_as_the_tools() {
        let Ok(Output::Json(v)) = read(r#"{"json": {"project": "店", "zoom": 1}}"#) else {
            panic!("not JSON")
        };
        assert_eq!(v, json!({"project": "店", "zoom": 1}));
        let Ok(Output::Blocks(b)) = read(r#"{"blocks": ["a", "b"]}"#) else {
            panic!("not blocks")
        };
        assert_eq!(b, ["a", "b"]);
        let Ok(Output::Image {
            data,
            mime_type,
            note,
        }) = read(
            r#"{"image": {"data": "iVBORw==", "mimeType": "image/png"}, "note": {"width": 2}}"#,
        )
        else {
            panic!("not an image")
        };
        assert_eq!(
            (data.as_str(), mime_type.as_str(), note),
            ("iVBORw==", "image/png", json!({"width": 2}))
        );
        assert_eq!(
            read(r#"{"error": "セクションマップが表示されていません"}"#)
                .err()
                .unwrap(),
            "セクションマップが表示されていません"
        );
        for broken in [
            "",
            "[]",
            r#"{"what": 1}"#,
            r#"{"blocks": [1]}"#,
            r#"{"image": {"data": "x"}}"#,
        ] {
            assert!(
                read(broken)
                    .err()
                    .unwrap()
                    .starts_with("アプリの答えが読めません"),
                "{broken}"
            );
        }
    }

    #[test]
    fn a_picture_says_what_it_is_and_how_big() {
        let picture = Output::Image {
            data: "iVBORw0KGgo=".into(),
            mime_type: "image/png".into(),
            note: json!({"target": "window"}),
        };
        assert_eq!(
            picture.text(),
            "{\n  \"target\": \"window\"\n}\n（PNG 8 バイト）"
        );
    }
}
