//! `section_read`'s `as: mock`: a ui section's parts put together into one page (`export::mock`),
//! with its size and what it holds. The page is given whole — cut, an HTML page no longer reads.
//! Its parts' notes and review marks are laid over it as the screen's map shows them, unless the
//! value says `notes: false`. With `faces: true`, each part's own page too
//! (`export::mock::faces`), which the screen's map draws the parts from.

use super::args::Args;
use super::{Seen, reads};
use crate::export::Project;
use crate::export::mock::{Face, Mock};
use crate::mcp::Output;
use crate::store::Store;
use crate::tools::grid::Layout;
use serde_json::{Map, Value, json};

/// The ui section `path` names, by id: one of another kind refused with the reads it has.
pub(crate) fn ui_section(project: &Project, path: &str) -> Result<String, String> {
    let id = super::section_id(project, path)?;
    if project.section_kind(&id) != Some(super::frames::UI) {
        let kind = project
            .section_kind(&id)
            .map(|k| project.defs.section_label(k))
            .unwrap_or_default();
        return Err(text!(
            "mocks.notUi",
            path = project.directory.full_section(&id),
            kind
        ));
    }
    Ok(id)
}

/// A ui section's mock as read: its path, its page, and its parts' faces when asked.
pub(crate) struct Read {
    pub path: String,
    pub mock: Mock,
    pub faces: Option<Vec<Face>>,
}

/// The mock of the section `path` in the tree read, laid out on the project's grid: with its
/// parts' notes and review marks over it when `notes`, and each part's face when `faces`.
pub(crate) fn mock_of(
    store: &Store,
    seen: &Seen,
    path: &str,
    notes: bool,
    faces: bool,
) -> Result<Read, String> {
    reads::refuse_settings_as_section(seen, path)?;
    let project = seen.project();
    let id = ui_section(&project, path)?;
    let layout = Layout::read(&store.conn, &seen.summary.id)?;
    let review = if notes {
        Some(super::review::marks(
            &store.conn,
            &seen.summary.id,
            &seen.named.tree,
            &project,
            None,
        )?)
    } else {
        None
    };
    let overlay = crate::export::mock::Overlay {
        notes,
        review: review.as_ref(),
    };
    let mock = crate::export::mock::assemble_over(&project, &layout, &id, overlay);
    let faces = faces.then(|| crate::export::mock::faces(&project, &layout, &id));
    Ok(Read {
        path: project.directory.full_section(&id),
        mock,
        faces,
    })
}

/// Whether a read's value lays the notes over the mock: unless it says `notes: false`.
pub(crate) fn with_notes(given: &Map<String, Value>) -> bool {
    given.get("notes").and_then(Value::as_bool) != Some(false)
}

/// Whether a read's value asks for each part's face: only when it says `faces: true`.
fn with_faces(given: &Map<String, Value>) -> bool {
    given.get("faces").and_then(Value::as_bool) == Some(true)
}

/// `{project, branch, path, html, width, height, parts, frames}`, and `faces:
/// [{path, kind, width, height, html}]` when asked.
pub(crate) fn mock_read(store: &Store, args: &Args) -> Result<Output, String> {
    let given = args.value()?;
    super::check(&given, "section_read", "section（as: mock）")?;
    let seen = Seen::read(
        store,
        args.string("project")?,
        args.optional_string("branch"),
    )?;
    let Read { path, mock, faces } = mock_of(
        store,
        &seen,
        &args.target()?,
        with_notes(&given),
        with_faces(&given),
    )?;
    let mut out = Map::new();
    out.insert("project".into(), seen.summary.name.clone().into());
    out.insert("path".into(), path.into());
    out.insert("html".into(), mock.html.into());
    out.insert("width".into(), mock.width.into());
    out.insert("height".into(), mock.height.into());
    out.insert("parts".into(), mock.parts.into());
    out.insert("frames".into(), mock.frames.into());
    if let Some(faces) = faces {
        let listed: Vec<Value> = faces
            .into_iter()
            .map(|f| {
                json!({"path": f.path, "kind": f.kind, "width": f.width, "height": f.height, "html": f.html})
            })
            .collect();
        out.insert("faces".into(), listed.into());
    }
    Ok(seen.headed(Output::Json(Value::Object(out))))
}

/// Whether a read asks for a mock (its value's `as`).
pub(crate) fn asked(arguments: &Map<String, Value>) -> bool {
    arguments
        .get("value")
        .and_then(|v| v.get("as"))
        .and_then(Value::as_str)
        == Some("mock")
}
