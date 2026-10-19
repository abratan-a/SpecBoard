//! `app_control` on the agent's projects: the windows are the apps' that it holds, so the call is
//! handed back to the app whose window it names, which answers it as it does for the core it holds
//! itself. Each app says which of its windows show this agent's projects (`{"t":"windows"}`), named
//! here by its machine (`MacBook:1`): what a key sees of them is the windows of the projects it may read
//! (`op: windows`), and a call goes to the one `value.window` names, or the nearest that shows its
//! `project` (the window in front, then the one said last), or the nearest of all it may see. `select`
//! chooses in a window that shows the project already: what a person is looking at is not swapped for
//! another project (a window of its own for it, when the app opens more than one).
//!
//! A person asked whether MCP may do what it asked to (`App::ask`) is asked on every app it holds: a person is at one Mac or another, and the
//! first answer is taken.
//!
//! A map drawn as a picture (`App::picture`) goes to one app, not to a window: the app draws it on a
//! canvas no window holds, so no window needs to show the project, and nothing a person looks at moves
//! (the app's switch for taking `app_control` from agents is not asked). The nearest app draws it: the
//! one with the window in front, then the one that said its windows last, then the one attached last.

use crate::daemon::Clients;
use serde_json::{Value, json};
use specboard_core::{App, Core};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Weak};
use std::time::Duration;

/// How long an app has to answer: a picture waits on what the window draws.
const WAIT: Duration = Duration::from_secs(30);

pub struct Screens {
    clients: Arc<Clients>,
    core: Weak<Core>,
}

/// A window of an app it holds that shows one of its projects.
struct Shown {
    app: u64,
    /// `<machine>:<number>`.
    name: String,
    number: Value,
    project: String,
    said: Value,
    front: bool,
    said_at: u64,
}

impl Screens {
    pub fn new(clients: Arc<Clients>, core: Weak<Core>) -> Screens {
        Screens { clients, core }
    }

    /// The windows, the nearest first.
    fn windows(&self) -> Vec<Shown> {
        let mut out = Vec::new();
        for (app, client) in self.clients.all() {
            let seen = client.seen.lock().unwrap_or_else(|e| e.into_inner());
            for window in &seen.windows {
                let number = window["window"].clone();
                let shown = match &number {
                    Value::String(n) => n.clone(),
                    other => other.to_string(),
                };
                out.push(Shown {
                    app,
                    name: format!("{}:{shown}", seen.name),
                    number,
                    project: window["project"].as_str().unwrap_or("").to_string(),
                    said: window.clone(),
                    front: window["front"] == true,
                    said_at: seen.said_at,
                });
            }
        }
        out.sort_by(|a, b| b.front.cmp(&a.front).then(b.said_at.cmp(&a.said_at)));
        out
    }

    fn answer(&self, arguments: &str, caller: &str) -> Result<Value, String> {
        let core = self
            .core
            .upgrade()
            .ok_or_else(|| specboard_core::text!("agent.screens.coreClosed"))?;
        let args: Value = serde_json::from_str(arguments)
            .map_err(|_| specboard_core::text!("agent.screens.argumentsUnreadable"))?;
        let caller_said: Value = serde_json::from_str(caller).unwrap_or(Value::Null);
        let op = args["op"].as_str().unwrap_or("");
        let (projects, readable) = asked_of(&core, &caller_said);
        let sees = |project: &str| readable.as_ref().is_none_or(|r| r.contains(project));
        let shown: Vec<Shown> = self
            .windows()
            .into_iter()
            .filter(|w| projects.contains_key(&w.project) && sees(&w.project))
            .collect();
        if op == "windows" {
            let listed: Vec<Value> = shown
                .iter()
                .map(|w| {
                    json!({
                        "window": w.name,
                        "project": projects[&w.project].0,
                        "branch": w.said["branch"],
                        "section": w.said["section"],
                        "card": w.said["card"],
                        "front": w.front,
                    })
                })
                .collect();
            return Ok(json!({"json": {"windows": listed}}));
        }
        let named = args["project"]
            .as_str()
            .map(str::trim)
            .filter(|p| !p.is_empty());
        let project = named.map(|name| {
            projects
                .iter()
                .find(|(id, (n, slug))| *id == name || n == name || slug == name)
                .or_else(|| {
                    projects
                        .iter()
                        .find(|(_, (n, _))| n.to_lowercase() == name.to_lowercase())
                })
                .map(|(id, _)| id.clone())
                .filter(|id| sees(id))
        });
        if let Some(None) = project {
            return Err(specboard_core::text!(
                "agent.screens.noProject",
                name = named.unwrap_or("")
            ));
        }
        let project = project.flatten();
        let window = args["value"]["window"].as_str().map(str::trim);
        let chosen = match window {
            Some(name) => {
                let found = shown.iter().find(|w| w.name == name).ok_or_else(|| {
                    let all: Vec<&str> = shown.iter().map(|w| w.name.as_str()).collect();
                    if all.is_empty() {
                        specboard_core::text!("agent.screens.noWindowForKey", name)
                    } else {
                        specboard_core::text!("agent.screens.noWindow", name, all = all.join(" / "))
                    }
                })?;
                if let Some(p) = &project
                    && *p != found.project
                {
                    return Err(specboard_core::text!(
                        "agent.screens.windowOtherProject",
                        name,
                        wanted = projects[p].0,
                        open = projects[&found.project].0
                    ));
                }
                found
            }
            None => match &project {
                Some(p) => shown.iter().find(|w| w.project == *p).ok_or_else(|| {
                    if op == "select" {
                        specboard_core::text!(
                            "agent.screens.noWindowToSelect",
                            project = projects[p].0
                        )
                    } else {
                        specboard_core::text!(
                            "agent.screens.noWindowToRead",
                            project = projects[p].0
                        )
                    }
                })?,
                None => shown
                    .first()
                    .ok_or_else(|| specboard_core::text!("agent.screens.noWindowAny"))?,
            },
        };
        let handed = json!({"arguments": arguments, "caller": caller, "window": chosen.number});
        let said = self
            .clients
            .ask(chosen.app, "app", handed, WAIT)
            .ok_or_else(|| specboard_core::text!("agent.screens.appSilent", name = chosen.name))?;
        serde_json::from_str(&said)
            .map_err(|_| specboard_core::text!("agent.screens.appUnreadable", name = chosen.name))
    }
}

impl App for Screens {
    fn app_control(&self, arguments: &str, caller: &str) -> String {
        match self.answer(arguments, caller) {
            Ok(answer) => answer.to_string(),
            Err(problem) => json!({"error": problem}).to_string(),
        }
    }

    /// A person asked on every app it holds: the first answer, as the app answers it for the core it holds itself.
    fn ask(&self, request: &str) -> String {
        if self.clients.all().is_empty() {
            return json!({"error": no_one()}).to_string();
        }
        let seconds = serde_json::from_str::<Value>(request)
            .ok()
            .and_then(|r| r["seconds"].as_u64())
            .unwrap_or(120);
        match self
            .clients
            .ask_all(json!({"request": request}), Duration::from_secs(seconds))
        {
            Some(answer) => answer,
            None => json!({"timeout": true}).to_string(),
        }
    }

    fn picture(&self, request: &str) -> String {
        self.draw(request)
    }
}

impl Screens {
    /// The app a picture is drawn by, and its machine's name: none when no app is attached.
    fn nearest_app(&self) -> Option<(u64, String)> {
        let mut apps: Vec<(bool, u64, u64, String)> = self
            .clients
            .all()
            .into_iter()
            .map(|(app, client)| {
                let seen = client.seen.lock().unwrap_or_else(|e| e.into_inner());
                let front = seen.windows.iter().any(|w| w["front"] == true);
                (front, seen.said_at, app, seen.name.clone())
            })
            .collect();
        apps.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)).then(b.2.cmp(&a.2)));
        apps.into_iter().next().map(|(_, _, app, name)| (app, name))
    }

    /// A picture asked of the nearest app, answered as it answers it.
    fn draw(&self, request: &str) -> String {
        let Some((app, name)) = self.nearest_app() else {
            return json!({"error": specboard_core::text!("picture.noApp")}).to_string();
        };
        self.clients
            .ask(app, "picture", json!({"request": request}), WAIT)
            .unwrap_or_else(|| {
                json!({"error": specboard_core::text!("agent.screens.pictureSilent", name, seconds = WAIT.as_secs())})
                    .to_string()
            })
    }
}

/// What a call that asks is told while no app is there to ask on.
fn no_one() -> String {
    specboard_core::text!("agent.screens.noOne")
}

/// The projects it keeps (id → name and slug), and those the caller's key may read: every one for a
/// call with no key. Asked of the core on a thread of its own: this one is in the middle of the
/// call, bound to its key.
#[allow(clippy::type_complexity)]
fn asked_of(
    core: &Arc<Core>,
    caller: &Value,
) -> (HashMap<String, (String, String)>, Option<HashSet<String>>) {
    let core = core.clone();
    let key = caller["id"].as_str().map(str::to_string);
    std::thread::spawn(move || {
        let call = |request: Value| -> Value {
            serde_json::from_str::<Value>(&core.call(&request.to_string()))
                .map(|v| v["result"].clone())
                .unwrap_or(Value::Null)
        };
        let projects: HashMap<String, (String, String)> =
            call(json!({"op": "projects"}))["projects"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|p| {
                    Some((
                        p["id"].as_str()?.to_string(),
                        (
                            p["name"].as_str().unwrap_or("").to_string(),
                            p["slug"].as_str().unwrap_or("").to_string(),
                        ),
                    ))
                })
                .collect();
        let readable = key.map(|key| {
            let mut seen: HashSet<String> = call(json!({"op": "project_public"}))["projects"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|p| p.as_str().map(str::to_string))
                .collect();
            if let Some(found) = call(json!({"op": "access_keys"}))["keys"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|k| k["id"] == key.as_str())
                && let Some(grants) = found["grants"].as_object()
            {
                seen.extend(grants.keys().cloned());
            }
            seen
        });
        (projects, readable)
    })
    .join()
    // Not read: no project, and none seen.
    .unwrap_or_else(|_| (HashMap::new(), Some(HashSet::new())))
}
