//! Who does what, by the actor sections (`specboard://SpecBoard/Docs/Design:Overview` §9
//! 「アクターのセクション」 and §11「シーケンス図の生命線と帯」): a flow's actor is the nearest actor
//! section around it, kept nowhere, so moving a flow never leaves one stale; a store is the actor's
//! that holds a ghost of it; and a sequence diagram draws each one at the depth it is seen from — the
//! flow opened, or the actor whose chart it is — unless the actors to draw (`lifelines`) are given.

use super::Project;
use crate::model::Card;
use std::collections::HashMap;

/// The actor sections of a project's tree, looked up by section.
pub struct Actors<'p, 'a> {
    project: &'p Project<'a>,
    /// Each section's parent, by id.
    parent: HashMap<&'a str, &'a str>,
    /// Each section's place in the tree's order (parents before children, siblings in order).
    order: HashMap<&'a str, usize>,
}

impl<'p, 'a> Actors<'p, 'a> {
    pub fn new(project: &'p Project<'a>) -> Actors<'p, 'a> {
        let sections = &project.detail.sections;
        Actors {
            project,
            parent: sections
                .iter()
                .filter_map(|s| Some((s.id.as_str(), s.parent_id.as_deref()?)))
                .collect(),
            order: sections
                .iter()
                .enumerate()
                .map(|(i, s)| (s.id.as_str(), i))
                .collect(),
        }
    }

    /// Whether the section `id` is an actor.
    pub fn is_actor(&self, id: &str) -> bool {
        self.project.section_kind(id) == Some(crate::kinds::ACTOR)
    }

    /// The sections from the root down to `section`, itself last.
    fn line_of(&self, section: &str) -> Vec<String> {
        let mut out = vec![section.to_string()];
        let mut at = section;
        while let Some(&up) = self.parent.get(at) {
            if out.iter().any(|s| s == up) || out.len() > 256 {
                break;
            }
            out.push(up.to_string());
            at = up;
        }
        out.reverse();
        out
    }

    /// The actor sections around `section`, from the top down, itself last when it is one.
    pub fn chain(&self, section: &str) -> Vec<String> {
        self.line_of(section)
            .into_iter()
            .filter(|s| self.is_actor(s))
            .collect()
    }

    /// The nearest actor section around `section` (itself, when it is one).
    pub fn nearest(&self, section: &str) -> Option<String> {
        self.chain(section).pop()
    }

    /// A card's actor: the nearest actor section around it — a node's, its flow's.
    pub fn of_card(&self, card: &Card) -> Option<String> {
        let mut head = card;
        let mut seen = 0;
        while let Some(container) = self.project.container_of(head)
            && seen < 32
        {
            head = container;
            seen += 1;
        }
        self.nearest(&self.project.section_of(head))
    }

    /// Whether `inner` is the section `outer` or one inside it.
    pub fn contains(&self, outer: &str, inner: &str) -> bool {
        self.line_of(inner).iter().any(|s| s == outer)
    }

    /// The nearest actor above `actor`, not itself.
    pub fn parent_actor(&self, actor: &str) -> Option<String> {
        let mut chain = self.chain(actor);
        if chain.last().map(String::as_str) == Some(actor) {
            chain.pop();
        }
        chain.pop()
    }

    /// A section's place in the tree's order.
    pub fn order(&self, section: &str) -> usize {
        self.order.get(section).copied().unwrap_or(usize::MAX)
    }

    /// The actors that hold a ghost of `store`, each once, in the tree's order.
    pub fn store_holders(&self, store: &Card) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for ghost in &self.project.detail.ghosts {
            if self.project.ghost_card(ghost).map(|c| c.id.as_str()) != Some(store.id.as_str()) {
                continue;
            }
            if let Some(actor) = self.nearest(&self.project.section_of_ghost(ghost))
                && !out.contains(&actor)
            {
                out.push(actor);
            }
        }
        out.sort_by_key(|a| self.order(a));
        out
    }

    /// The actor whose `store` is, as seen from `near` (the flow's actor): of those that hold a
    /// ghost of it, the one whose path shares the most with `near`'s, the first in the tree's order
    /// of those that share as much. None when no actor holds one.
    pub fn store_actor(&self, store: &Card, near: Option<&str>) -> Option<String> {
        let holders = self.store_holders(store);
        let from = near.map(|n| self.line_of(n)).unwrap_or_default();
        let shared = |actor: &str| {
            self.line_of(actor)
                .iter()
                .zip(from.iter())
                .take_while(|(a, b)| a == b)
                .count()
        };
        let most = holders.iter().map(|h| shared(h)).max()?;
        holders.into_iter().find(|h| shared(h) == most)
    }

    /// The lifeline `actor` is drawn on, seen from `seen` (the flow opened, or the actor whose chart
    /// it is): of `chosen`, the deepest that holds it; else the first actor around it, from the top,
    /// that does not hold `seen` — a sibling inside the one parent, the outside one outside it — and
    /// itself when every one does.
    pub fn lifeline(&self, actor: &str, seen: Option<&str>, chosen: &[String]) -> String {
        let chain = self.chain(actor);
        if let Some(deepest) = chain.iter().rev().find(|a| chosen.contains(a)) {
            return deepest.clone();
        }
        chain
            .iter()
            .find(|a| seen.is_none_or(|s| !self.contains(a, s)))
            .cloned()
            .unwrap_or_else(|| actor.to_string())
    }
}
