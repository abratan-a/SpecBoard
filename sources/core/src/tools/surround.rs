//! What a section's map draws of the other sections as section ghosts (the screen's
//! `SectionGhosts.fold`, counted the same way): one card for each other section its lines reach,
//! holding a row for each card a line comes from or goes to, and under a card the rows down to the
//! field a line lands on (a row for each name on the way; a file's function or type a row of its
//! own). Counted with no kind of line hidden and no outside card switched off, so the boxes the
//! core tidies and checks are as tall as the screen stacks them whatever a filter shows
//! (`specboard://SpecBoard/Docs/Design:Overview` §12). The same lines, each between the boxes it is
//! drawn to (`ends`), are what arranging along the lines lays the map out by.

use crate::export::Project;
use crate::model::Card;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};

/// A field pointing at this many cards or more states a rule, and the screen draws none of its
/// lines (`wideFanThreshold`): the lines it would have drawn reach no section ghost either.
const WIDE_FAN: usize = 4;

/// The project's lines as the screen reads them (`relations`), read once for every map that asks.
#[derive(Default)]
pub(crate) struct Lines(std::cell::OnceCell<Vec<Value>>);

impl Lines {
    fn of(&self, project: &Project) -> &[Value] {
        self.0
            .get_or_init(|| crate::screen_detail::relations(project))
    }
}

/// The rows of each section ghost `section`'s map draws, by the id of the section it sums up.
/// `placed_here`: the other sections' cards put on the map for a look, drawn whole, which lines
/// keep landing on.
pub(crate) fn ghost_rows(
    project: &Project,
    section: &str,
    placed_here: &HashSet<String>,
    lines: &Lines,
) -> BTreeMap<String, usize> {
    let map = Map::of(project, section, placed_here);
    let mut rows: BTreeMap<String, HashSet<String>> = BTreeMap::new();
    for line in map.drawn_lines(lines) {
        map.fold(&line, &mut rows);
    }
    rows.into_iter()
        .map(|(section, rows)| (section, rows.len()))
        .filter(|(_, n)| *n > 0)
        .collect()
}

/// A box a line is drawn to on a section's map: a card drawn whole (its own, a card ghost, another
/// section's card put there), or the section ghost summing up the section a far card is in.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum End {
    Card(String),
    Section(String),
}

/// The lines `section`'s map draws, each between the boxes it is drawn to, with its kind of line —
/// the lines `ghost_rows` counts rows for, and those between two boxes drawn whole.
pub(crate) fn ends(
    project: &Project,
    section: &str,
    placed_here: &HashSet<String>,
    lines: &Lines,
) -> Vec<(End, End, String)> {
    let map = Map::of(project, section, placed_here);
    let mut out = Vec::new();
    for line in map.drawn_lines(lines) {
        if let Some((from, to)) = map.ends(&line) {
            out.push((from, to, line.line));
        }
    }
    out
}

/// A line put on the map: the boxes it is drawn between, the row it leaves, and where it lands
/// in the card it reaches (its names there; a node's row lands on its flow's head).
struct Placed {
    line: String,
    from_box: String,
    from_row: String,
    to_box: String,
    landing: Landing,
    /// Of a kind that fans out (`fans` on screen): not an embedding, not what a text mentions, and
    /// leaving a row rather than the card's head.
    fans: bool,
}

/// Where a line lands in the card it reaches.
enum Landing {
    /// The card's head.
    Head,
    /// The names down to a field or row of it.
    Names(Vec<String>),
    /// A function or type of a file, a row of its own.
    Symbol(String),
}

/// What the map of a section has, to put the project's lines on it.
struct Map<'p> {
    project: &'p Project<'p>,
    section: String,
    /// Its own cards drawn as cards (a value object or an enum folded into what holds it is not).
    own: HashSet<String>,
    /// Its card ghosts, by the card each leads to (the first for a card).
    stand_ins: HashMap<String, String>,
    /// What is drawn whole and folds into no section ghost: its card ghosts, and the other
    /// sections' cards put there for a look.
    whole: HashSet<String>,
    /// For a section with sections inside: each card under one of them, by the child it is in.
    children: HashMap<String, String>,
    /// The cards and card ghosts of the section: a line between two of them is about it.
    named: HashSet<String>,
}

impl<'p> Map<'p> {
    fn of(project: &'p Project<'p>, section: &str, placed_here: &HashSet<String>) -> Map<'p> {
        let folded = super::grid::folded(project, section);
        let own: HashSet<String> = project
            .directory
            .members(section)
            .iter()
            .map(|&i| project.nth(i))
            .filter(|c| !folded.contains(&c.id))
            .map(|c| c.id.clone())
            .collect();
        let mut stand_ins = HashMap::new();
        let mut whole: HashSet<String> = placed_here.clone();
        for ghost in project.ghosts_in(section) {
            whole.insert(ghost.id.clone());
            if let Some(card) = project.ghost_card(ghost)
                && !own.contains(&card.id)
            {
                stand_ins
                    .entry(card.id.clone())
                    .or_insert_with(|| ghost.id.clone());
            }
        }
        let mut named = own.clone();
        named.extend(project.ghosts_in(section).iter().map(|g| g.id.clone()));
        let children = children_of(project, section);
        Map {
            project,
            section: section.to_string(),
            own,
            stand_ins,
            whole,
            children,
            named,
        }
    }

    /// A card by id, among the tree's cards and the working directories' files.
    fn card(&self, id: &str) -> Option<&'p Card> {
        self.project
            .card(id)
            .or_else(|| self.project.file_card(id))
            .or_else(|| self.project.outside_card(id))
    }

    /// The box a card is drawn in on the map, and, for one inside a container, where in it: the
    /// card itself when it is drawn (its own, one put there for a look, another section's that is
    /// not a value object or an enum), the container holding it, or a card ghost of it.
    fn drawn(&self, id: &str) -> Option<(String, Option<Landing>)> {
        if self.own.contains(id) || self.whole.contains(id) {
            return Some((id.to_string(), None));
        }
        let card = self.card(id)?;
        if let Some(container) = card.inside.as_deref().and_then(|c| self.project.card(c)) {
            let (held, _) = self.drawn(&container.id)?;
            // A flow's node is drawn as a row of the flow; its section ghost has no row for it.
            let landing = if container.kind == "flow" {
                Landing::Head
            } else {
                Landing::Names(vec![card.name.clone()])
            };
            return Some((held, Some(landing)));
        }
        if let Some(ghost) = self.stand_ins.get(id) {
            return Some((ghost.clone(), None));
        }
        // One of its own not drawn as a card (folded into what holds it) is drawn nowhere.
        let here = self
            .project
            .card(id)
            .is_some_and(|c| self.project.section_of(c) == self.section);
        if here || card.kind == "value" || card.kind == "enum" {
            return None;
        }
        Some((id.to_string(), None))
    }

    /// A line of the project put on the map: none when either end is not drawn on it, or both are
    /// one box.
    fn place(&self, relation: &Value) -> Option<Placed> {
        let said = |key: &str| relation.get(key).and_then(Value::as_str).unwrap_or("");
        let (from, to) = (said("fromSchemaId"), said("toSchemaId"));
        let via = said("viaGhostId");
        let (from_box, _) = self.drawn(from)?;
        let (to_box, held) = if !via.is_empty() && self.named.contains(via) {
            (via.to_string(), None)
        } else {
            self.drawn(to)?
        };
        if from_box == to_box {
            return None;
        }
        let line = said("line");
        let from_row = said("from").to_string();
        let landing = match held {
            Some(landing) => landing,
            None => {
                let at = said("at");
                if self.project.card(to).is_none()
                    && self.project.file_card(to).is_some()
                    && !at.is_empty()
                    && !at.starts_with('#')
                {
                    Landing::Symbol(at.to_string())
                } else if at.is_empty() || at.starts_with('#') {
                    Landing::Head
                } else {
                    Landing::Names(
                        crate::pathtext::names(at)
                            .into_iter()
                            .filter(|n| !n.is_empty())
                            .collect(),
                    )
                }
            }
        };
        Some(Placed {
            fans: line != "embed" && line != "mentions" && !from_row.is_empty(),
            line: line.to_string(),
            from_box,
            from_row,
            to_box,
            landing,
        })
    }

    /// The project's lines the map draws: put on it, about the section, and not among a field's
    /// lines that fan out wide.
    fn drawn_lines(&self, lines: &Lines) -> Vec<Placed> {
        let placed: Vec<Placed> = lines
            .of(self.project)
            .iter()
            .filter_map(|relation| self.place(relation))
            .collect();
        let mut fans: HashMap<(String, String), usize> = HashMap::new();
        for line in placed.iter().filter(|l| l.fans) {
            *fans
                .entry((line.from_box.clone(), line.from_row.clone()))
                .or_default() += 1;
        }
        placed
            .into_iter()
            .filter(|line| {
                !(line.fans
                    && fans
                        .get(&(line.from_box.clone(), line.from_row.clone()))
                        .is_some_and(|n| *n >= WIDE_FAN))
            })
            .filter(|line| self.about_here(&line.from_box, &line.to_box))
            .collect()
    }

    /// The boxes a drawn line goes between, as `fold` puts it on the section ghosts: none for a
    /// line no box of the map is drawn for at either end.
    fn ends(&self, line: &Placed) -> Option<(End, End)> {
        let whole = |id: &str| self.own.contains(id) || self.whole.contains(id);
        let ghost = |id: &str| self.card(id).map(|card| End::Section(self.ghost_of(card)));
        let from_own = self.own.contains(&line.from_box);
        let to_own = self.own.contains(&line.to_box);
        let child = |id: &str| self.children.get(id);
        if whole(&line.from_box) && whole(&line.to_box) {
            Some((
                End::Card(line.from_box.clone()),
                End::Card(line.to_box.clone()),
            ))
        } else if from_own && !whole(&line.to_box) {
            Some((End::Card(line.from_box.clone()), ghost(&line.to_box)?))
        } else if to_own && !whole(&line.from_box) {
            Some((ghost(&line.from_box)?, End::Card(line.to_box.clone())))
        } else if !from_own
            && !to_own
            && matches!((child(&line.from_box), child(&line.to_box)), (Some(a), Some(b)) if a != b)
        {
            Some((ghost(&line.from_box)?, ghost(&line.to_box)?))
        } else {
            None
        }
    }

    /// Whether a line is about the section: one end its own card, both its cards or card ghosts,
    /// or between two of its children.
    fn about_here(&self, from: &str, to: &str) -> bool {
        if self.own.contains(from) || self.own.contains(to) {
            return true;
        }
        if self.named.contains(from) && self.named.contains(to) {
            return true;
        }
        matches!((self.children.get(from), self.children.get(to)), (Some(a), Some(b)) if a != b)
    }

    /// The section ghost a card goes into: the child of the section it is under, or its own
    /// section.
    fn ghost_of(&self, card: &Card) -> String {
        self.children
            .get(&card.id)
            .cloned()
            .unwrap_or_else(|| self.project.section_of(card))
    }

    /// The rows a line puts on the section ghosts.
    fn fold(&self, line: &Placed, rows: &mut BTreeMap<String, HashSet<String>>) {
        let from_own = self.own.contains(&line.from_box);
        let to_own = self.own.contains(&line.to_box);
        let child = |id: &str| self.children.get(id);
        if from_own && !to_own && !self.whole.contains(&line.to_box) {
            self.land(&line.to_box, &line.landing, rows);
        } else if to_own && !from_own && !self.whole.contains(&line.from_box) {
            self.land(&line.from_box, &Landing::Head, rows);
        } else if !from_own
            && !to_own
            && matches!((child(&line.from_box), child(&line.to_box)), (Some(a), Some(b)) if a != b)
        {
            self.land(&line.to_box, &line.landing, rows);
            self.land(&line.from_box, &Landing::Head, rows);
        }
    }

    /// The rows for `landing` in the card `id` on its section ghost: the card's, then one for each
    /// name down; a file's symbol a row of its own.
    fn land(&self, id: &str, landing: &Landing, rows: &mut BTreeMap<String, HashSet<String>>) {
        let Some(card) = self.card(id) else {
            return;
        };
        let on = rows.entry(self.ghost_of(card)).or_default();
        if let Landing::Symbol(symbol) = landing {
            on.insert(format!("s\u{1}{}\u{1}{symbol}", card.id));
            return;
        }
        on.insert(format!("c\u{1}{}", card.id));
        if let Landing::Names(names) = landing {
            let mut key = format!("p\u{1}{}", card.id);
            for name in names {
                key.push('\u{1}');
                key.push_str(name);
                on.insert(key.clone());
            }
        }
    }
}

/// For a section with sections inside: each card under one of them — not one of its own, not one
/// inside a container — by the child of the section it is under.
fn children_of(project: &Project, section: &str) -> HashMap<String, String> {
    let parents: HashMap<&str, Option<&str>> = project
        .detail
        .sections
        .iter()
        .map(|s| (s.id.as_str(), s.parent_id.as_deref()))
        .collect();
    let children: HashSet<&str> = parents
        .iter()
        .filter(|(_, p)| **p == Some(section))
        .map(|(id, _)| *id)
        .collect();
    let mut out = HashMap::new();
    if children.is_empty() {
        return out;
    }
    for card in project.detail.schemas.iter().filter(|c| c.inside.is_none()) {
        let mut at = card.section_id.as_deref();
        let mut steps = 0;
        while let Some(id) = at {
            if children.contains(id) {
                out.insert(card.id.clone(), id.to_string());
                break;
            }
            at = parents.get(id).copied().flatten();
            steps += 1;
            if steps > parents.len() {
                break;
            }
        }
    }
    out
}
