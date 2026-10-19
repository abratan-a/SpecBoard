//! `section_read`'s and `card_read`'s `as: picture`: a map drawn as the screen draws it, as a
//! picture (`specboard://SpecBoard/Docs/Design:CoreApi` tools.pictures). Only the app can draw it,
//! so the core does what it does for any read — the key may read the project, the value fits the
//! form — then works out which map of what (a section's section map — a ui section's mock, the page
//! its parts make, unless the map is asked for —, a flow's node diagram, a
//! goal's or container's content map, any other card's connections map) and hands that to the app
//! (`crate::App::picture`). No store lock is held while the app draws: it reads the project through
//! the core itself. When there is no app, it does not answer in time or it says why it cannot, the
//! refusal offers the same call as `as: diagram` (a mock's, `as: mock`), which the core answers alone.

use super::args::Args;
use super::walk::{Target, locate};
use super::{Seen, reads};
use crate::Core;
use crate::export::Project;
use crate::mcp::Output;
use crate::store::Store;
use serde_json::{Map, Value, json};

/// The longest side when the call says none: as `app_control`'s capture.
const SIZE: i64 = 1600;
const SIZES: std::ops::RangeInclusive<i64> = 400..=3000;
/// Grid units around `cards` or `area` when the call says none: room for the lines that leave them.
const MARGIN: i64 = 2;
const MARGINS: std::ops::RangeInclusive<i64> = 0..=100;
/// Below this, a card's words are smaller than half the screen's and hard to read.
const READABLE: f64 = 0.5;

/// What the app says of a picture, in the order the note says it.
const NOTE_KEYS: [&str; 7] = [
    "picture",
    "width",
    "height",
    "scale",
    "area",
    "cards",
    "documentsPending",
];

/// The keys only a picture takes: left out of the `as: diagram` call a refusal offers.
const PICTURE_KEYS: [&str; 5] = ["maxSize", "cards", "area", "margin", "view"];

/// Whether a read asks for a picture (its value's `as`).
pub(crate) fn asked(arguments: &Map<String, Value>) -> bool {
    arguments
        .get("value")
        .and_then(|v| v.get("as"))
        .and_then(Value::as_str)
        == Some("picture")
}

/// What the app is asked while there is none to ask: a core opened without the window (the agent
/// with no app connected, a test).
pub(crate) fn no_app() -> String {
    text!("picture.noApp")
}

/// Which map is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Picture {
    SectionMap,
    ConnectionsMap,
    FlowDiagram,
    ContentMap,
    /// A ui section's mock: the page its parts make (`export::mock`), drawn as the screen shows it.
    Mock,
}

impl Picture {
    fn word(self) -> &'static str {
        match self {
            Picture::SectionMap => "sectionMap",
            Picture::ConnectionsMap => "connectionsMap",
            Picture::FlowDiagram => "flowDiagram",
            Picture::ContentMap => "contentMap",
            Picture::Mock => "mock",
        }
    }

    fn said(self) -> String {
        match self {
            Picture::SectionMap => text!("picture.map.sectionMap"),
            Picture::ConnectionsMap => text!("picture.map.connectionsMap"),
            Picture::FlowDiagram => text!("picture.map.flowDiagram"),
            Picture::ContentMap => text!("picture.map.contentMap"),
            Picture::Mock => text!("picture.map.mock"),
        }
    }

    /// Whether it is cut to some of it (`cards`, `area`, `margin`): a map whose boxes a person
    /// places, which can grow past what one picture reads well.
    fn takes_range(self) -> bool {
        matches!(self, Picture::SectionMap | Picture::ContentMap)
    }

    /// Whether it takes `kinds`: a flow's chart draws its own lines (so does its `as: diagram`).
    fn takes_kinds(self) -> bool {
        !matches!(self, Picture::FlowDiagram | Picture::Mock)
    }
}

/// A picture worked out: what the app is asked, and what the answer is headed with.
struct Asked {
    picture: Picture,
    request: Value,
    head: Map<String, Value>,
}

impl Core {
    /// `section_read` or `card_read` with `as: picture`, checked, worked out and handed to the app.
    pub(crate) fn picture(
        &self,
        tool: &str,
        arguments: &Map<String, Value>,
    ) -> Result<Output, String> {
        let args = Args::new(arguments);
        let asked = {
            let store = self.reading()?;
            resolve(&store, tool, &args)?
        };
        let said = match self.app() {
            Some(app) => app.picture(&asked.request.to_string()),
            None => json!({ "error": no_app() }).to_string(),
        };
        answer(&said, asked, tool, arguments)
    }
}

/// The call's target and value as the request the app draws from: refused when the target is not
/// one card or one section, or the value says what that map does not take.
fn resolve(store: &Store, tool: &str, args: &Args) -> Result<Asked, String> {
    let given = args.value()?;
    let form = if tool == "section_read" {
        "section（as: picture）"
    } else {
        "as: picture"
    };
    super::check(&given, tool, form)?;
    let seen = Seen::read(
        store,
        args.string("project")?,
        args.optional_string("branch"),
    )?;
    let project = seen.project();
    let base = reads::base(&seen, &given)?;
    let mut head = Map::new();
    head.insert("project".into(), seen.summary.name.clone().into());
    head.insert("branch".into(), seen.named.name.clone().into());
    let (picture, of, id) = if tool == "section_read" {
        let path = args.target()?;
        reads::refuse_settings_as_section(&seen, &path)?;
        let id = super::section_id(&project, &path)?;
        head.insert("path".into(), project.directory.full_section(&id).into());
        // A ui section is drawn as its mock unless the map is asked for; another has no mock.
        let ui = project.section_kind(&id) == Some(super::frames::UI);
        let picture = match given.get("view").and_then(Value::as_str) {
            Some("map") => Picture::SectionMap,
            Some("mock") if !ui => {
                return Err(super::mocks::ui_section(&project, &path).unwrap_err());
            }
            _ if ui => Picture::Mock,
            _ => Picture::SectionMap,
        };
        (picture, "section", id)
    } else {
        let list = args
            .targets()?
            .ok_or_else(|| text!("reads.target.needed"))?;
        let [one] = list.as_slice() else {
            return Err(text!("picture.target.many", count = list.len()));
        };
        let card = one_card(&project, args, one, base.as_deref())?;
        head.insert("path".into(), project.full(card).into());
        if let Some(rev) = seen.rev_at(one, base.as_deref()) {
            head.insert("rev".into(), rev.into());
        }
        let picture = if card.kind == "flow" {
            Picture::FlowDiagram
        } else if project.is_container(&card.kind) {
            Picture::ContentMap
        } else {
            Picture::ConnectionsMap
        };
        (picture, "card", card.id.clone())
    };
    let path = head["path"].as_str().unwrap_or_default().to_string();
    refuse_keys(&given, picture, &path)?;
    let a = Args::new(&given);
    let mut request = Map::new();
    request.insert("picture".into(), picture.word().into());
    request.insert("project".into(), seen.summary.id.clone().into());
    request.insert("tree".into(), seen.named.tree.clone().into());
    request.insert(of.into(), id.into());
    if picture == Picture::Mock {
        let notes = super::mocks::with_notes(&given);
        let mock = super::mocks::mock_of(store, &seen, &args.target()?, notes, false)?.mock;
        request.insert("html".into(), mock.html.into());
        request.insert("width".into(), mock.width.into());
        request.insert("height".into(), mock.height.into());
    }
    if picture == Picture::ConnectionsMap {
        let depth = a.int("depth", 1);
        if !(0..=2).contains(&depth) {
            return Err(text!("picture.depth", given = depth));
        }
        request.insert("depth".into(), depth.into());
    }
    if let Some(kinds) = a.strings("kinds") {
        let kept = super::diagram::kinds_asked(&project, &kinds)?;
        if !kept.is_empty() {
            let said: Vec<Value> = kept
                .iter()
                .map(|k| crate::export::mermaid::said_kind(k).into())
                .collect();
            request.insert("kinds".into(), Value::Array(said));
        }
    }
    if picture.takes_range() {
        let cards = a.strings("cards").unwrap_or_default();
        let area = given.get("area");
        if !cards.is_empty() && area.is_some() {
            return Err(text!("picture.cardsAndArea"));
        }
        if !cards.is_empty() {
            // A section map's bare names are read in the section drawn.
            let from = match of {
                "section" => Some(request[of].as_str().unwrap_or_default().to_string()),
                _ => base.clone(),
            };
            let ids: Vec<Value> = cards
                .iter()
                .map(|c| {
                    super::walk::find_card(&project, c, from.as_deref())
                        .map(|c| c.id.clone().into())
                })
                .collect::<Result<_, _>>()?;
            request.insert("cards".into(), Value::Array(ids));
        }
        if let Some(area) = area {
            request.insert("area".into(), checked_area(area)?);
        }
        let margin = a.int("margin", MARGIN);
        if !MARGINS.contains(&margin) {
            return Err(text!("picture.margin", given = margin));
        }
        request.insert("margin".into(), margin.into());
    }
    let size = a.int("maxSize", SIZE);
    if !SIZES.contains(&size) {
        return Err(text!("picture.maxSize", given = size));
    }
    request.insert("maxSize".into(), size.into());
    request.insert("lang".into(), crate::text::current().into());
    Ok(Asked {
        picture,
        request: Value::Object(request),
        head,
    })
}

/// The card `path` names, as a picture is drawn of it: a field, a row, a node, an enum's value or a
/// step is refused with the way it is read, a card ghost with the call that draws the card it leads
/// to.
fn one_card<'a>(
    project: &Project<'a>,
    args: &Args,
    path: &str,
    base: Option<&str>,
) -> Result<&'a crate::model::Card, String> {
    let refused = |what: String, how: String| text!("picture.notCard", path, what, how);
    match locate(project, path, base)? {
        Target::Symbol(..) => Err(text!("code.symbol.noPicture", path)),
        Target::Card(card) => match project.container_of(card) {
            Some(flow) if flow.kind == "flow" => {
                let flow = project.full(flow);
                Err(refused(
                    text!("picture.what.node", flow = flow),
                    text!("picture.how.flow", flow = flow),
                ))
            }
            _ => Ok(card),
        },
        Target::Ghost(ghost) => {
            let Some(card) = project.ghost_card(ghost) else {
                return Err(project
                    .directory
                    .entry_of(&ghost.id)
                    .map(|e| project.directory.dead_end(e, &project.full_ghost(ghost)))
                    .unwrap_or_default());
            };
            let card = project.full(card);
            let mut arguments = args.raw.clone();
            arguments.insert("target".into(), card.clone().into());
            if let Some(Value::Object(value)) = arguments.get_mut("value") {
                // The card's full path needs no section to be read from.
                value.shift_remove("section");
            }
            Err(crate::mcp::refusal(
                text!("picture.ghost", path, card),
                json!({ "use": { "tool": "card_read", "arguments": arguments } }),
            ))
        }
        Target::Field(card, ..) => {
            let card = project.full(card);
            Err(refused(
                text!("picture.what.field", card = card),
                text!("picture.how.field", card = card),
            ))
        }
        Target::FormatRow(card, _) | Target::CustomRow(card, _) => {
            let card = project.full(card);
            Err(refused(
                text!("picture.what.row", card = card),
                text!("picture.how.row", card = card),
            ))
        }
        Target::EnumValue(card, _) => {
            let card = project.full(card);
            Err(refused(
                text!("picture.what.enumValue", card = card),
                text!("picture.how.enumValue", card = card),
            ))
        }
        Target::Step(node, _) => {
            let flow = project
                .container_of(node)
                .map_or_else(|| project.full(node), |f| project.full(f));
            Err(refused(
                text!("picture.what.step", node = project.full(node)),
                text!("picture.how.flow", flow = flow),
            ))
        }
    }
}

/// Keys the form takes that this map does not: `depth` is a connections map's alone, the range
/// (`cards`, `area`, `margin`) a section map's and a content map's.
fn refuse_keys(given: &Map<String, Value>, picture: Picture, path: &str) -> Result<(), String> {
    if picture != Picture::ConnectionsMap && given.contains_key("depth") {
        return Err(text!("picture.key.depth", path, map = picture.said()));
    }
    if !picture.takes_kinds() && given.contains_key("kinds") {
        return Err(text!("picture.key.kinds", path, map = picture.said()));
    }
    if !picture.takes_range() {
        let range: Vec<&str> = ["cards", "area", "margin"]
            .into_iter()
            .filter(|k| given.contains_key(*k))
            .collect();
        if !range.is_empty() {
            return Err(text!(
                "picture.key.range",
                keys = crate::text::List::of(&range),
                path,
                map = picture.said()
            ));
        }
    }
    Ok(())
}

/// `area` as the app is given it: its corners in whole grid units, the top left above and left of
/// the bottom right.
fn checked_area(area: &Value) -> Result<Value, String> {
    let corner = |name: &str| -> Option<(f64, f64)> {
        let at = area.get(name)?;
        Some((at.get("x")?.as_f64()?, at.get("y")?.as_f64()?))
    };
    let (Some(top_left), Some(bottom_right)) = (corner("topLeft"), corner("bottomRight")) else {
        return Err(text!("picture.area"));
    };
    if top_left.0 > bottom_right.0 || top_left.1 > bottom_right.1 {
        return Err(text!("picture.area"));
    }
    let point = |(x, y): (f64, f64)| json!({ "x": x.floor() as i64, "y": y.floor() as i64 });
    Ok(json!({ "topLeft": point(top_left), "bottomRight": point(bottom_right) }))
}

/// The app's answer as the tool's: the picture and what it is, headed with the project, the tree,
/// the path (and a card's rev) — or, whatever kept it from drawing, a refusal offering
/// `as: diagram`.
/// The area as it is asked for (`topLeft` before `bottomRight`, `x` before `y`), whatever order
/// the app wrote its dictionaries in.
fn area_in_order(area: Value) -> Value {
    fn ordered(value: Value, keys: &[&str], inner: fn(Value) -> Value) -> Value {
        let Value::Object(mut from) = value else {
            return value;
        };
        let mut out = Map::new();
        for key in keys {
            if let Some(v) = from.shift_remove(*key) {
                out.insert((*key).into(), inner(v));
            }
        }
        out.extend(from);
        Value::Object(out)
    }
    ordered(area, &["topLeft", "bottomRight"], |corner| {
        ordered(corner, &["x", "y"], |v| v)
    })
}

fn answer(
    said: &str,
    asked: Asked,
    tool: &str,
    arguments: &Map<String, Value>,
) -> Result<Output, String> {
    let picture = asked.picture;
    let unreadable = || {
        let head: String = said.chars().take(200).collect();
        text!("picture.unreadable", head)
    };
    let mut answer = match serde_json::from_str::<Value>(said) {
        Ok(Value::Object(answer)) => answer,
        _ => return Err(not_drawn(&unreadable(), picture, tool, arguments)),
    };
    if let Some(error) = answer.remove("error") {
        let why = match error {
            Value::String(text) => text,
            other => other.to_string(),
        };
        return Err(not_drawn(&why, picture, tool, arguments));
    }
    let image = answer.remove("image");
    let (Some(data), Some(mime_type)) = (
        image.as_ref().and_then(|i| i["data"].as_str()),
        image.as_ref().and_then(|i| i["mimeType"].as_str()),
    ) else {
        return Err(not_drawn(&unreadable(), picture, tool, arguments));
    };
    let mut note = asked.head;
    if let Some(Value::Object(mut said)) = answer.remove("note") {
        // In the order the answer is read in, whatever order the app wrote them in.
        for key in NOTE_KEYS {
            if let Some(value) = said.shift_remove(key) {
                let value = if key == "area" {
                    area_in_order(value)
                } else {
                    value
                };
                note.insert(key.into(), value);
            }
        }
        for (key, value) in said {
            if !note.contains_key(&key) {
                note.insert(key, value);
            }
        }
    }
    if !note.contains_key("hint")
        && let Some(scale) = note.get("scale").and_then(Value::as_f64)
        && scale < READABLE
    {
        let ways = if asked.picture.takes_range() {
            text!("picture.hint.narrow")
        } else {
            text!("picture.hint.larger")
        };
        let percent = (scale * 100.0).round() as i64;
        note.insert("hint".into(), text!("picture.hint", percent, ways).into());
    }
    Ok(Output::Image {
        data: data.to_string(),
        mime_type: mime_type.to_string(),
        note: Value::Object(note),
    })
}

/// No picture, and why: with the same call as `as: diagram` (what that takes of the value), which
/// reads the same map as text — a mock's, as `as: mock`, the page itself as text.
fn not_drawn(why: &str, picture: Picture, tool: &str, arguments: &Map<String, Value>) -> String {
    let mut arguments = arguments.clone();
    if let Some(Value::Object(value)) = arguments.get_mut("value") {
        if picture == Picture::Mock {
            value.clear();
            value.insert("as".into(), "mock".into());
        } else {
            value.insert("as".into(), "diagram".into());
        }
        for key in PICTURE_KEYS {
            value.shift_remove(key);
        }
    }
    crate::mcp::refusal(
        text!("picture.notDrawn", why),
        json!({ "use": { "tool": tool, "arguments": arguments } }),
    )
}
