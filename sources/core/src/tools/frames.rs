//! Frames (`frame`; `specboard://SpecBoard/Docs/Design:CardKinds`「UI のセクションとはめ込み」): a card of
//! a ui section that puts another ui section's mock into its box. What it points at is a section,
//! held by the section's id in its field `target`, so it follows a rename or a move as a card ghost
//! follows its card. A frame is refused when it points at a section that is not a ui one, at its own
//! section, or at one whose frames lead back to its own section: a mock that holds itself has no
//! end to draw.

use super::Seen;
use super::cards::Scope;
use crate::export::Project;
use crate::model::Card;
use crate::write::Tx;

/// The kind of section a frame is put in and points at.
pub(crate) const UI: &str = "ui";

/// The section a frame points at, by id: none when nothing is written.
pub(crate) fn target_of(card: &Card) -> Option<&str> {
    card.form
        .as_ref()?
        .get("fields")?
        .get("target")?
        .as_str()
        .filter(|t| !t.is_empty())
}

/// The frames directly in `section`.
pub(crate) fn frames_in<'a>(project: &Project<'a>, section: &str) -> Vec<&'a Card> {
    project
        .directory
        .members(section)
        .iter()
        .map(|&i| project.nth(i))
        .filter(|c| c.kind == crate::kinds::FRAME)
        .collect()
}

/// Why the frame `card` cannot point where it does: at a section that is not a ui one, at its own,
/// or at one whose frames lead back to its own (the sections passed, in order). A target that is
/// gone is not refused here: a section removed leaves the frame pointing nowhere, which the check
/// tells of (`broken`).
pub(crate) fn problem(project: &Project, card: &Card) -> Option<String> {
    let target = target_of(card)?;
    if !project.directory.has_section(target) {
        return None;
    }
    let own = project.section_of(card);
    let path = project.directory.full_section(target);
    if project.section_kind(target) != Some(UI) {
        let kind = project
            .section_kind(target)
            .map(|k| project.defs.section_label(k))
            .unwrap_or_default();
        return Some(text!("frames.target.notUi", path, kind));
    }
    if target == own {
        return Some(text!("frames.target.self", path));
    }
    // Each section reached through frames, with the way it was reached, until the frame's own.
    let mut chain: Vec<Vec<String>> = vec![vec![own.clone(), target.to_string()]];
    let mut passed: Vec<String> = vec![target.to_string()];
    while let Some(way) = chain.pop() {
        let at = way.last().cloned().unwrap_or_default();
        for frame in frames_in(project, &at) {
            let Some(next) = target_of(frame) else {
                continue;
            };
            let mut further = way.clone();
            further.push(next.to_string());
            if next == own {
                let said: Vec<String> = further
                    .iter()
                    .map(|s| project.directory.full_section(s))
                    .collect();
                return Some(text!("frames.target.circle", chain = said.join(" → ")));
            }
            if !passed.iter().any(|p| p == next) {
                passed.push(next.to_string());
                chain.push(further);
            }
        }
    }
    None
}

/// The frames among `ids`, as the tree holds them now, refused with why when one points where it
/// cannot (`problem`): what a write that made, changed or moved them is held to.
pub(crate) fn refuse(tx: &Tx, scope: &Scope, ids: &[&str]) -> Result<(), String> {
    let seen = Seen::in_tx(tx, scope)?;
    let project = seen.project();
    for id in ids {
        let Some(card) = project.card(id) else {
            continue;
        };
        if card.kind != crate::kinds::FRAME {
            continue;
        }
        if let Some(why) = problem(&project, card) {
            return Err(text!("frames.refused", card = project.full(card), why));
        }
    }
    Ok(())
}

/// A frame that leads nowhere, as the check says it: its target as written (empty when the section
/// is gone) and why — what it pointed at was removed, or is no ui section now.
pub(crate) fn broken(project: &Project, card: &Card) -> Option<(String, String)> {
    if card.kind != crate::kinds::FRAME {
        return None;
    }
    let target = target_of(card)?;
    if !project.directory.has_section(target) {
        return Some((String::new(), text!("check.frame.gone")));
    }
    let path = project.directory.full_section(target);
    (project.section_kind(target) != Some(UI)).then(|| {
        let why = text!("check.frame.notUi", path = path.clone());
        (path, why)
    })
}
