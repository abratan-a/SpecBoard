//! Who is called what, section by section (`SchemaDirectory` in the Swift app's Model):
//! each section's path, and the card a name in a path refers to, read from a section.

use crate::pathtext;
use std::collections::{HashMap, HashSet};

/// A section as the directory needs it.
pub struct SectionRef {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
}

/// A card as the directory needs it.
pub struct CardRef {
    pub id: String,
    pub name: String,
    pub section_id: Option<String>,
    /// The container it is inside (a card's id): its name is its own there, and its path
    /// goes on from the container's (`Behavior:Checkout.review`).
    pub inside: Option<String>,
}

/// A card ghost as the directory needs it: a name in its section, and what it points at
/// by id (a card, or another ghost).
pub struct GhostRef {
    pub id: String,
    pub name: String,
    pub section_id: Option<String>,
    pub target: Option<String>,
}

/// What a name in a section is: a card, or a card ghost — each by its index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Entry {
    Card(usize),
    Ghost(usize),
}

/// How far a chain of ghosts is followed before it is taken for one that never ends.
const CHAIN: usize = 64;

pub struct Directory {
    /// Main's root: the first section, or `""` with none.
    pub home: String,
    /// The root names are written from.
    context: String,
    sections: Vec<String>,
    by_id: HashMap<String, (String, Option<String>)>,
    full_paths: HashMap<String, String>,
    roots: HashMap<String, String>,
    by_path: HashMap<String, String>,
    by_key: HashMap<(String, String), usize>,
    by_section: HashMap<String, Vec<usize>>,
    /// The cards inside each container, by the container's id.
    by_container: HashMap<String, Vec<usize>>,
    card_list: Vec<String>,
    /// The cards by id.
    card_ids: HashMap<String, usize>,
    /// The card ghosts: by their name in their section, by section, by id, and what each
    /// points at.
    ghost_key: HashMap<(String, String), usize>,
    ghosts_by_section: HashMap<String, Vec<usize>>,
    ghost_ids: HashMap<String, usize>,
    ghost_targets: Vec<Option<String>>,
    ghost_names: Vec<String>,
    ghost_list: Vec<String>,
}

impl Directory {
    pub fn new(sections: &[SectionRef], cards: &[CardRef], ghosts: &[GhostRef]) -> Directory {
        let home = sections.first().map(|s| s.id.clone()).unwrap_or_default();
        let by_id: HashMap<String, (String, Option<String>)> = sections
            .iter()
            .map(|s| (s.id.clone(), (s.name.clone(), s.parent_id.clone())))
            .collect();
        let mut full_paths = HashMap::new();
        let mut roots = HashMap::new();
        let mut by_path = HashMap::new();
        for section in sections {
            let mut names = vec![section.name.clone()];
            let mut root = section.id.clone();
            let mut parent = section.parent_id.clone();
            let mut seen: HashSet<String> = HashSet::from([section.id.clone()]);
            while let Some(id) = parent {
                let Some((name, up)) = by_id.get(&id) else {
                    break;
                };
                if !seen.insert(id.clone()) {
                    break;
                }
                names.insert(0, name.clone());
                root = id;
                parent = up.clone();
            }
            let path = pathtext::join_sections(names.iter().map(String::as_str));
            by_path.insert(pathtext::fold(&path), section.id.clone());
            full_paths.insert(section.id.clone(), path);
            roots.insert(section.id.clone(), root);
        }
        let mut by_key = HashMap::new();
        let mut by_section: HashMap<String, Vec<usize>> = HashMap::new();
        let mut by_container: HashMap<String, Vec<usize>> = HashMap::new();
        let mut card_ids = HashMap::new();
        for (index, card) in cards.iter().enumerate() {
            let section = card.section_id.clone().unwrap_or_else(|| home.clone());
            card_ids.insert(card.id.clone(), index);
            match card.inside.as_ref().filter(|c| !c.is_empty()) {
                Some(container) => {
                    by_key.insert((container.clone(), card.name.clone()), index);
                    by_container
                        .entry(container.clone())
                        .or_default()
                        .push(index);
                }
                None => {
                    by_key.insert((section.clone(), card.name.clone()), index);
                    by_section.entry(section).or_default().push(index);
                }
            }
        }
        let mut ghost_key = HashMap::new();
        let mut ghosts_by_section: HashMap<String, Vec<usize>> = HashMap::new();
        let mut ghost_ids = HashMap::new();
        for (index, ghost) in ghosts.iter().enumerate() {
            let section = ghost.section_id.clone().unwrap_or_else(|| home.clone());
            ghost_key.insert((section.clone(), ghost.name.clone()), index);
            ghosts_by_section.entry(section).or_default().push(index);
            ghost_ids.insert(ghost.id.clone(), index);
        }
        Directory {
            context: home.clone(),
            home,
            sections: sections.iter().map(|s| s.id.clone()).collect(),
            by_id,
            full_paths,
            roots,
            by_path,
            by_key,
            by_section,
            by_container,
            card_list: cards.iter().map(|c| c.id.clone()).collect(),
            card_ids,
            ghost_key,
            ghosts_by_section,
            ghost_ids,
            ghost_targets: ghosts.iter().map(|g| g.target.clone()).collect(),
            ghost_names: ghosts.iter().map(|g| g.name.clone()).collect(),
            ghost_list: ghosts.iter().map(|g| g.id.clone()).collect(),
        }
    }

    /// A ghost's id, by its index.
    pub fn ghost_id(&self, ghost: usize) -> Option<String> {
        self.ghost_list.get(ghost).cloned()
    }

    /// What a name as written is in a section — or in a container, by its id: a card, else a
    /// card ghost. Names of names (`Checkout.review`) go on into the container the first leads
    /// to.
    pub fn entry(&self, section: &str, written: &str) -> Option<Entry> {
        let steps = pathtext::dots(written);
        let mut scope = section.to_string();
        for (i, step) in steps.iter().enumerate() {
            let found = self.entry_named(&scope, &pathtext::unquote(step))?;
            if i == steps.len() - 1 {
                return Some(found);
            }
            let container = self.follow(found)?;
            scope = self.card_id(container)?;
        }
        None
    }

    /// What a name (as it is, not written in a path) is in a section or a container.
    pub fn entry_named(&self, scope: &str, name: &str) -> Option<Entry> {
        let key = (scope.to_string(), name.to_string());
        self.by_key
            .get(&key)
            .map(|&i| Entry::Card(i))
            .or_else(|| self.ghost_key.get(&key).map(|&i| Entry::Ghost(i)))
    }

    /// A card's id, by its index.
    pub fn card_id(&self, card: usize) -> Option<String> {
        self.card_list.get(card).cloned()
    }

    /// The cards inside a container, by its id, in project order.
    pub fn inside(&self, container: &str) -> &[usize] {
        self.by_container
            .get(container)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// What an id is: a card, a card ghost, or neither.
    pub fn entry_of(&self, id: &str) -> Option<Entry> {
        self.card_ids
            .get(id)
            .map(|&i| Entry::Card(i))
            .or_else(|| self.ghost_ids.get(id).map(|&i| Entry::Ghost(i)))
    }

    /// The card an entry leads to: itself, or the card at the end of a ghost's chain.
    /// None when the chain ends at nothing (a card removed) or goes round.
    pub fn follow(&self, entry: Entry) -> Option<usize> {
        let mut at = entry;
        for _ in 0..CHAIN {
            match at {
                Entry::Card(i) => return Some(i),
                Entry::Ghost(g) => {
                    at = self.entry_of(self.ghost_targets[g].as_deref()?)?;
                }
            }
        }
        None
    }

    /// The ghosts a ghost's chain passes through, itself first, up to where it ends.
    pub fn chain(&self, ghost: usize) -> Vec<usize> {
        let mut out = vec![ghost];
        let mut at = ghost;
        while out.len() < CHAIN {
            match self.ghost_targets[at]
                .as_deref()
                .and_then(|t| self.entry_of(t))
            {
                Some(Entry::Ghost(next)) if !out.contains(&next) => {
                    out.push(next);
                    at = next;
                }
                _ => break,
            }
        }
        out
    }

    /// Whether a ghost's chain goes round: it comes back to a ghost it passed.
    pub fn goes_round(&self, ghost: usize) -> bool {
        let chain = self.chain(ghost);
        let last = chain[chain.len() - 1];
        matches!(
            self.ghost_targets[last].as_deref().and_then(|t| self.entry_of(t)),
            Some(Entry::Ghost(next)) if chain.contains(&next)
        )
    }

    /// The card ghosts of one section, by their index.
    pub fn ghosts_in(&self, section: &str) -> &[usize] {
        self.ghosts_by_section
            .get(section)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// The names in a section, the cards' (by `cards`, every card's name by index) then
    /// the ghosts'.
    fn names_in(&self, section: &str, cards: &[String]) -> Vec<String> {
        self.members(section)
            .iter()
            .map(|&i| cards[i].clone())
            .chain(
                self.ghosts_in(section)
                    .iter()
                    .map(|&g| self.ghost_names[g].clone()),
            )
            .collect()
    }

    /// Whether a section has something of this name: a card or a ghost.
    fn has_name(&self, section: &str, name: &str) -> bool {
        let key = (section.to_string(), name.to_string());
        self.by_key.contains_key(&key) || self.ghost_key.contains_key(&key)
    }

    fn root_of(&self, section: &str) -> String {
        self.roots
            .get(section)
            .cloned()
            .unwrap_or_else(|| self.home.clone())
    }

    /// A section's path under its tree's root (`Store/Ordering`); the root itself is `""`.
    fn local_path(&self, section: &str) -> String {
        let root = self.root_of(section);
        let (Some(full), Some(root_path)) =
            (self.full_paths.get(section), self.full_paths.get(&root))
        else {
            return String::new();
        };
        if section == root || full.chars().count() <= root_path.chars().count() {
            return String::new();
        }
        full.chars().skip(root_path.chars().count() + 1).collect()
    }

    /// Whether the tree read has the section `id`.
    pub fn has_section(&self, id: &str) -> bool {
        self.by_id.contains_key(id)
    }

    /// In full, as stored and as MCP answers: `Store/Ordering`; a root by its own name.
    pub fn full_section(&self, section: &str) -> String {
        match self.by_id.get(section) {
            Some((name, None)) => name.clone(),
            Some(_) => self.local_path(section),
            None => "Main".to_string(),
        }
    }

    /// The cards of one section, in project order, by their index.
    pub fn members(&self, section: &str) -> &[usize] {
        self.by_section
            .get(section)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    fn in_tree(&self, root: &str) -> usize {
        self.sections
            .iter()
            .filter(|s| self.root_of(s) == root)
            .count()
    }

    fn is_root(&self, key: &str, root: &str) -> bool {
        self.by_id
            .get(root)
            .map(|(name, _)| pathtext::fold(&pathtext::quote(name)) == key)
            .unwrap_or(false)
    }

    fn relative(&self, key: &str, root: &str) -> Option<String> {
        let root_path = self.full_paths.get(root)?;
        self.by_path
            .get(&format!("{}/{}", pathtext::fold(root_path), key))
            .cloned()
    }

    /// A section's path as written, as the directory looks it up: each name quoted only where it
    /// needs it, in any ASCII case.
    fn key(written: &str) -> String {
        let names: Vec<std::borrow::Cow<str>> = pathtext::slashes(written)
            .into_iter()
            .map(pathtext::unquote)
            .collect();
        pathtext::fold(&pathtext::join_sections(names.iter().map(|n| n.as_ref())))
    }

    /// A section named in a path written in `reader`'s tree, or from the context.
    pub fn section_named(&self, name: &str, reader: Option<&str>) -> Option<String> {
        let raw = name.trim();
        if let Some((tree, rest)) = pathtext::tree_prefix(raw) {
            let rest = rest.trim();
            if !pathtext::same(&pathtext::unquote(tree), "main") {
                return None;
            }
            return if rest.is_empty() {
                Some(self.home.clone())
            } else {
                self.relative(&Directory::key(rest), &self.home)
            };
        }
        let key = Directory::key(raw);
        let tree = match reader {
            Some(r) => self.root_of(r),
            None => self.context.clone(),
        };
        if self.is_root(&key, &tree) {
            return Some(tree);
        }
        self.relative(&key, &tree)
    }

    /// A section named in a path written in `reader`'s tree.
    pub fn section_from(&self, name: &str, reader: &str) -> Option<String> {
        self.section_named(name, Some(reader))
    }

    /// The card a name in a path written in a card refers to: held to the trees its
    /// reader sees (`find(_, from:, seeing: true)`). A card ghost leads to its card.
    pub fn find_seeing(&self, raw: &str, base: &str) -> Option<usize> {
        self.follow(self.find_entry_seeing(raw, base)?)
    }

    /// What a name in a path written in a card names, a card ghost as itself.
    pub fn find_entry_seeing(&self, raw: &str, base: &str) -> Option<Entry> {
        let raw = raw.trim();
        if let (Some(section), name) = pathtext::split_section(raw) {
            let section = self.section_named(section.trim(), Some(base))?;
            return self.entry(&section, name);
        }
        self.find_entry(raw, Some(base))
    }

    /// The sections directly inside one, the root's in their fixed order.
    pub fn sections_in(&self, parent: &str) -> Vec<String> {
        self.sections
            .iter()
            .filter(|s| {
                self.by_id
                    .get(*s)
                    .and_then(|(_, p)| p.as_deref())
                    .filter(|p| self.by_id.contains_key(*p))
                    == Some(parent)
            })
            .cloned()
            .collect()
    }

    /// Whether names need their section to be told apart: the tree has more than one.
    pub fn is_sectioned(&self) -> bool {
        self.in_tree(&self.context) > 1
    }

    /// Why a name in a path written in a card, read from `base`, finds no card — in
    /// words to act on (`SchemaDirectory.find(_, from:, seeing: true)`'s failure). A name
    /// another section has is told with its section, and that a card ghost would let it be
    /// written by its name alone.
    pub fn find_problem(&self, raw: &str, base: &str, cards: &[String]) -> String {
        let raw = raw.trim();
        if let (Some(section_name), name) = pathtext::split_section(raw) {
            let section_name = section_name.trim();
            let Some(section) = self.section_named(section_name, Some(base)) else {
                return self.section_problem(section_name, base);
            };
            if let Some(entry) = self.entry(&section, name) {
                return self.dead_end(entry, raw);
            }
            let members = self.names_in(&section, cards);
            return missing_card(name, &members, Some(&self.full_section(&section)))
                + &self.elsewhere(&pathtext::unquote(name), &section);
        }
        if let Some((_, rest)) = pathtext::tree_prefix(raw) {
            let rest = rest.trim();
            return text!(
                "directory.problem.noSectionBeforeCard",
                raw,
                example = self.example_section(),
                rest
            );
        }
        if let Some(entry) = self.entry(base, raw) {
            return self.dead_end(entry, raw);
        }
        let members = self.names_in(base, cards);
        let elsewhere = self.elsewhere(&pathtext::unquote(raw), base);
        let hint = if elsewhere.is_empty() {
            String::new()
        } else {
            text!(
                "directory.problem.elsewhereHint",
                example = self
                    .elsewhere_sections(&pathtext::unquote(raw), base)
                    .first()
                    .map(|s| self.full_section(s))
                    .unwrap_or_default(),
                raw,
                section = self.full_section(base)
            )
        };
        missing_card(raw, &members, Some(&self.full_section(base))) + &elsewhere + &hint
    }

    /// Why a name that is there leads to no card: a ghost whose chain ends at nothing, or
    /// goes round.
    pub fn dead_end(&self, entry: Entry, raw: &str) -> String {
        match entry {
            Entry::Ghost(g) if self.goes_round(g) => {
                text!("directory.problem.ghostRound", raw)
            }
            Entry::Ghost(_) => text!("directory.problem.ghostEmpty", raw),
            Entry::Card(_) => text!("directory.problem.isCard", raw),
        }
    }

    fn example_section(&self) -> String {
        self.sections
            .iter()
            .find(|s| self.by_id.get(*s).and_then(|(_, p)| p.as_ref()) == Some(&self.home))
            .map(|s| self.local_path(s))
            .unwrap_or_else(|| "Schemas".to_string())
    }

    /// The other sections that have something of this name: a card or a card ghost.
    fn elsewhere_sections(&self, name: &str, except: &str) -> Vec<&String> {
        self.sections
            .iter()
            .filter(|s| s.as_str() != except && self.has_name(s, name))
            .collect()
    }

    fn elsewhere(&self, name: &str, except: &str) -> String {
        let found: Vec<String> = self
            .elsewhere_sections(name, except)
            .into_iter()
            .map(|s| pathtext::card_path(&self.full_section(s), name))
            .collect();
        if found.is_empty() {
            String::new()
        } else {
            text!("directory.problem.elsewhere", found = found.join(" / "))
        }
    }

    fn section_list(&self, root: &str) -> String {
        let listed: Vec<String> = self
            .sections
            .iter()
            .filter(|s| self.root_of(s) == root && s.as_str() != root)
            .map(|s| self.full_section(s))
            .collect();
        if listed.is_empty() {
            text!("directory.part.noSections")
        } else {
            listed.join(" / ")
        }
    }

    /// Why a section name found nothing (`SchemaDirectory.sectionProblem`).
    pub fn section_problem(&self, name: &str, reader: &str) -> String {
        let raw = name.trim();
        let tree_prefix = pathtext::tree_prefix(raw);
        let parts = pathtext::slashes(raw);
        if tree_prefix.is_none() && parts.len() > 1 {
            let head = pathtext::unquote(parts[0]);
            let rest = &raw[parts[0].len() + 1..];
            if self
                .sections
                .iter()
                .any(|s| matches!(self.by_id.get(s), Some((n, None)) if pathtext::same(n, &head)))
            {
                return text!("directory.problem.oldSectionPath", name, rest);
            }
        }
        if let Some((tree, rest)) = tree_prefix {
            let tree = tree.trim();
            let rest = rest.trim();
            if !pathtext::same(&pathtext::unquote(tree), "main") {
                return text!("directory.problem.treeInPath", tree);
            }
            return text!(
                "directory.problem.noSection",
                name = rest,
                sections = self.section_list(&self.home)
            );
        }
        let single = parts.len() == 1;
        let leaf = pathtext::unquote(raw);
        let tree = self.root_of(reader);
        let leaves: Vec<&String> = self
            .sections
            .iter()
            .filter(|s| self.root_of(s) == tree && **s != tree)
            .filter(|s| {
                single
                    && self
                        .by_id
                        .get(*s)
                        .map(|(n, _)| pathtext::same(n, &leaf))
                        .unwrap_or(false)
            })
            .collect();
        if leaves.len() > 1 {
            return text!(
                "directory.problem.manySections",
                name,
                count = leaves.len(),
                sections = leaves
                    .iter()
                    .map(|s| self.full_section(s))
                    .collect::<Vec<_>>()
                    .join(" / ")
            );
        }
        if let Some(only) = leaves.first() {
            return text!(
                "directory.problem.sectionFromTop",
                path = self.full_section(only)
            );
        }
        text!(
            "directory.problem.noSection",
            name,
            sections = self.section_list(&tree)
        )
    }

    /// Every card in full, section by section (`allQualified`).
    #[allow(dead_code)]
    pub fn all_qualified(&self, cards: &[String]) -> Vec<String> {
        if self.sections.is_empty() {
            return self
                .members(&self.home)
                .iter()
                .map(|&i| cards[i].clone())
                .collect();
        }
        self.sections
            .iter()
            .flat_map(|s| {
                self.members(s)
                    .iter()
                    .map(|&i| pathtext::card_path(&self.full_section(s), &cards[i]))
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// Why a name read from `base` (none: the context) finds no card, as a call is told
    /// (`SchemaDirectory.find(_, from:)`'s failure). `cards`: every card's name, by index.
    #[allow(dead_code)]
    pub fn find_refusal(&self, raw: &str, base: Option<&str>, cards: &[String]) -> String {
        let raw = raw.trim();
        if let (Some(section_name), name) = pathtext::split_section(raw) {
            let section_name = section_name.trim();
            let section = base
                .and_then(|b| self.section_named(section_name, Some(b)))
                .or_else(|| self.section_named(section_name, None));
            let Some(section) = section else {
                return self.section_problem(section_name, &self.context);
            };
            if let Some(entry) = self.entry(&section, name) {
                return self.dead_end(entry, raw);
            }
            let members = self.names_in(&section, cards);
            return missing_card(name, &members, Some(&self.full_section(&section)))
                + &self.elsewhere(&pathtext::unquote(name), &section);
        }
        if let Some((_, rest)) = pathtext::tree_prefix(raw) {
            let rest = rest.trim();
            return text!(
                "directory.problem.noSectionBeforeCard",
                raw,
                example = self.example_section(),
                rest
            );
        }
        let single = (self.in_tree(&self.context) == 1).then(|| self.context.clone());
        let base = match base {
            Some(b) => Some(b.to_string()),
            None if self.sections.is_empty() => Some(self.home.clone()),
            None => single,
        };
        let Some(base) = base else {
            let name = pathtext::unquote(raw);
            let found: Vec<String> = self
                .sections
                .iter()
                .filter(|s| self.has_name(s, &name))
                .map(|s| pathtext::card_path(&self.full_section(s), &name))
                .collect();
            return text!(
                "directory.problem.writeWithSection",
                example = self.example_section(),
                raw,
                detail = if found.is_empty() {
                    let all = self.all_qualified(cards);
                    missing_card(raw, &all, None)
                } else {
                    text!(
                        "directory.problem.foundIn",
                        raw,
                        sections = found.join(" / ")
                    )
                }
            );
        };
        if let Some(entry) = self.entry(&base, raw) {
            return self.dead_end(entry, raw);
        }
        let members = self.names_in(&base, cards);
        missing_card(raw, &members, Some(&self.full_section(&base)))
            + &self.elsewhere(&pathtext::unquote(raw), &base)
    }

    /// A path as what it starts in — a card, a card inside a container through the
    /// container's names, or a card ghost — and the names after it there as written (`[]`
    /// kept); none when it names no card. Read from `base` as `find_entry` reads.
    pub fn split_entry(&self, raw: &str, base: Option<&str>) -> Option<(Entry, String)> {
        let raw = raw.trim();
        let (section, rest) = match pathtext::colon(raw) {
            Some(c) => (&raw[..=c], &raw[c + 1..]),
            None => ("", raw),
        };
        let parts: Vec<&str> = pathtext::dots(rest);
        for k in (1..=parts.len()).rev() {
            let head: Vec<&str> = parts[..k]
                .iter()
                .map(|p| pathtext::without_all(p))
                .collect();
            if head.iter().any(|h| h.is_empty()) {
                continue;
            }
            if let Some(entry) = self.find_entry(&format!("{section}{}", head.join(".")), base) {
                return Some((entry, parts[k..].join(".")));
            }
        }
        None
    }

    /// The card a name refers to, read from `base` (`SchemaDirectory.find`), by index. A
    /// card ghost leads to the card at the end of its chain.
    pub fn find(&self, raw: &str, base: Option<&str>) -> Option<usize> {
        self.follow(self.find_entry(raw, base)?)
    }

    /// What a name refers to, read from `base`: a card, or a card ghost as itself.
    pub fn find_entry(&self, raw: &str, base: Option<&str>) -> Option<Entry> {
        let raw = raw.trim();
        if let (Some(section_name), name) = pathtext::split_section(raw) {
            let section_name = section_name.trim();
            let section = base
                .and_then(|b| self.section_named(section_name, Some(b)))
                .or_else(|| self.section_named(section_name, None))?;
            return self.entry(&section, name);
        }
        if pathtext::tree_prefix(raw).is_some() {
            return None;
        }
        let single = if self.in_tree(&self.context) == 1 {
            Some(self.context.clone())
        } else {
            None
        };
        let base = match base {
            Some(b) => b.to_string(),
            None if self.sections.is_empty() => self.home.clone(),
            None => single?,
        };
        self.entry(&base, raw)
    }
}

/// A name as it is held up to another to see whether they look alike: in any ASCII case, and
/// with only its letters and digits (`CardKinds_md` looks like `CardKinds.md`).
fn loose(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase()
}

/// Whether a name looks like the one wanted: one starts with the other or holds it, as it is or
/// loosely (`loose`).
fn looks_like(name: &str, wanted: &str) -> bool {
    let alike = |a: &str, b: &str| {
        !a.is_empty() && !b.is_empty() && (a.starts_with(b) || b.starts_with(a) || a.contains(b))
    };
    alike(&pathtext::fold(name), &pathtext::fold(wanted)) || alike(&loose(name), &loose(wanted))
}

/// How every caller says "no such card", with the names that look like it (as a path writes
/// them). `wanted`: the name as it was written.
pub fn missing_card(wanted: &str, names: &[String], section: Option<&str>) -> String {
    let needle = pathtext::unquote(wanted);
    let close: Vec<String> = names
        .iter()
        .filter(|n| looks_like(n, &needle))
        .map(|n| pathtext::quote(n).into_owned())
        .collect();
    let place = section
        .map(|s| text!("directory.part.placeSection", section = s))
        .unwrap_or_else(|| text!("directory.part.placeProject"));
    let total = names.len();
    let tail = if close.is_empty() {
        text!("directory.problem.cardCount", place, total)
    } else {
        let shown: Vec<&str> = close.iter().take(12).map(|s| s.as_str()).collect();
        let more = if close.len() > 12 {
            text!("directory.part.more", count = close.len() - 12)
        } else {
            String::new()
        };
        text!(
            "directory.problem.closeCards",
            shown = shown.join(" / "),
            more,
            place,
            total
        )
    };
    text!("directory.problem.noCard", wanted, tail)
}
