//! The core's own database form: the tables a project is kept in once it has been
//! moved out of the Swift app's form (`migrate`).
//!
//! Most tables are as the Swift app had them. What changed: a card's document is in the
//! kind's form (`kinds`, `specboard://SpecBoard/Docs/Design:CardKinds`), its lines among it (`connections`); a
//! project's own kinds of card, section and line are a column of its row (`projects.kinds`);
//! how a card looks is apart from it (`card_looks`); and every box on a diagram — a card,
//! a section's card standing for another section's cards, a flow's node — is one row of
//! `places`, its place and its size together.
//!
//! Kept from the start, though nothing writes it yet: a branch set aside
//! (`branches.archived_at`). The change notes people were to write (`changelog`) went, never
//! written (`NO_CHANGELOG`).
//!
//! The columns added once the form was in use (`MCP_WRITES`, `MR_PLACES`, `CONNECTIONS`) sit
//! where a store made before gets them — last, `can_control` in the place of the `can_drive`
//! it was renamed from — so both are the same form, column by column (a project's row goes into
//! its file that way).

/// The mark in `_migrations` that says a database is in this form.
pub const MARK: &str = "core_0001_kinds";

/// The mark of the first change to the form once it was in use: what MCP may write in each
/// project (`projects.mcp_writes`), and an access key's three permissions in place of the one
/// "may move the window" (`access_keys.can_control`, `can_capture`, `can_create`). A store
/// made before it is brought up to it once, when it is opened (`store::upgrade`). The key's
/// columns keep a state each — 1 allowed, 0 asked of a person each time, -1 denied
/// (`mcp_writes::State`) — and the project's `mcp_writes` its items by the tools' names; what
/// was kept before reads as it was meant.
pub const MCP_WRITES: &str = "core_0002_mcp_writes";

/// The mark of the card ghosts made cards of their own: a section's
/// `ghosts` moved into cards of the kind `ghost` and a value's or an enum's `standalone`
/// (`ghost_move`). Nothing of the tables changes; the documents do, once.
pub const CARD_GHOSTS: &str = "core_0003_card_ghosts";

/// The mark of a request to merge into main asking for the boxes the branch moved too
/// (`specboard://SpecBoard/Docs/Design:DesignBranches` §8): the requests' `places` (the table was
/// `pull_requests` then). A request made before asked for none.
pub const MR_PLACES: &str = "core_0004_pr_places";

/// The mark of a card's lines as connections: a project's own kinds of card, section and line in its row
/// (`projects.kinds`, in place of the tables `card_kinds` and `card_fields` there were), and
/// the lines a card's fields held moved into its `connections` (`line_move`).
pub const CONNECTIONS: &str = "core_0005_connections";

/// The mark of what a container holds moved inside it: a flow's nodes made cards inside the flow, their ways and paths
/// the flow's lines, a goal's items cards inside the goal (`container_move`). Nothing of the
/// tables changes; a card's `section_id` holds the container it is inside, where its name is
/// its own.
pub const CONTAINERS: &str = "core_0006_containers";

/// The mark of a designating line landing inside the card it reaches: a line moved before whose landing is in
/// another card is split in two, the line to its card and a `link` to where its values are
/// (`line_move::split_all`). Nothing of the tables changes; the documents do, once.
pub const LANDINGS: &str = "core_0007_landings";

/// The mark of a goal's fields named in English (Japanese keys were confusing to use): `状態`・`優先度`・`期限` moved to `status`・`priority`・`due`, their values as
/// they were (`key_move`). What the board's kinds no longer have (a goal's and a task's `担当`, a
/// task's `優先度`, `branch` and `PR`) stays with a card that has a value in it, 「定義に無い欄」.
/// Nothing of the tables changes; the documents do, once.
pub const BOARD_KEYS: &str = "core_0008_board_keys";

/// The mark of a request to merge into main called a merge request:
/// the table `pull_requests` named `merge_requests`, a goal's field `PR` named `MR` and the kind
/// of field `pr` named `mr`, in a project's own kinds too (`key_move::move_mr`). The documents
/// change once, and the table's name.
pub const MERGE_REQUESTS: &str = "core_0009_merge_requests";

/// The mark of what is directly under each root written into the project:
/// `projects.kinds`' `project_tree` from each root's sections as they
/// were, a section a branch made itself there moved into main (`tree_move`). Nothing of the tables
/// changes; a project's kinds do, and the order of each root's sections.
pub const PROJECT_TREE: &str = "core_0010_plan_tree";

/// The mark of a goal's `branch` named `workBranch`: MCP's answers head with `branch`, the tree they are of, which hid
/// the goal's own (`key_move::move_work_branch_all`). Nothing of the tables changes; the documents
/// do, once.
pub const WORK_BRANCH: &str = "core_0011_work_branch";

/// The mark of the window's holds told apart by the Mac it is on: an agent's core is written by the apps of several
/// Macs, and one's "まとめて直す" is another's to wait for. `holds.client`, empty for the app that
/// holds the store itself.
pub const HOLD_CLIENTS: &str = "core_0012_hold_clients";

/// The mark of a branch that sees main as it was made: what main has changed since, kept for the branches that have
/// not written it (`branch_frozen`), until they take main in; a branch merged into main set aside
/// (`branches.archived_at`, named `~aside-…`) and what a revert brings back (`branches.reverts`);
/// whether a merge request keeps its branch (`merge_requests.keep`). The branches there were see
/// main as it is when the store is brought up.
pub const BRANCH_SNAPSHOTS: &str = "core_0013_branch_snapshots";

/// The mark of a plan called a project:
/// the table `plans` named `projects`, `plan_export_root` and `plan_public` named `project_export_root`
/// and `project_public`, every column `plan_id` named `project_id`; what a history's line says it
/// touched (`target_type` `plan`, a kind `plan.…`), the kinds' `plan_tree`, the items of what MCP
/// may write and the places of the writing rules (`plan.…`) named by the project. Done first,
/// before any other change to the form, so that each of them reads the tables by the names of now.
pub const PROJECTS: &str = "core_0014_projects";

/// The mark of the exports: the
/// project's own (`projects.exports`) in place of the cards' and sections' export settings, which
/// every tree's documents let go of (`export_move`).
pub const EXPORTS: &str = "core_0015_exports";

/// The mark of an export holding no working directory: where it is written is given when it is written, and each Mac keeps
/// one default folder for it (`export_default_folders`). The folder an export's working directory
/// was bound to on this Mac is put there, and the exports let go of `dir` (`export_move::carry_all`).
pub const EXPORT_TARGETS: &str = "core_0016_export_targets";

/// The table of the default folders, made where a store has none (`core_0016_export_targets`).
pub const EXPORT_DEFAULT_FOLDERS: &str = "
CREATE TABLE IF NOT EXISTS export_default_folders (
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  export_id  TEXT NOT NULL,
  folder     TEXT NOT NULL,
  PRIMARY KEY (project_id, export_id)
);";

/// The mark of the change notes gone:
/// the table `changelog`, kept from the start and never written by a tool or
/// the window, dropped. A project's history and the stacked documents keep what changed.
pub const NO_CHANGELOG: &str = "core_0017_no_changelog";

/// The mark of the check's marks: an item of a tree's check said to be as designed
/// (`tools::inspection`), one table for the project — not a tree's content, so no history keeps it.
pub const INSPECTION_MARKS: &str = "core_0018_inspection_marks";

/// The table of the check's marks, made where a store has none (`core_0018_inspection_marks`).
pub const INSPECTION_MARKS_TABLE: &str = "
CREATE TABLE IF NOT EXISTS inspection_marks (
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  id         TEXT NOT NULL,
  grp        TEXT NOT NULL,
  print      TEXT NOT NULL,
  reason     TEXT NOT NULL,
  by         TEXT NOT NULL,
  at         TEXT NOT NULL,
  PRIMARY KEY (project_id, id)
);";

/// The mark of the history's lines kept by key: a line's text kept as the key of the text tables
/// and the values put in (`text_key`, `text_values`), said in the language of whoever reads it;
/// `summary` keeps the sentence too, as an app of before reads it. A line from before has no key
/// and is said as it was kept.
pub const COMMIT_KEYS: &str = "core_0019_commit_keys";

/// The columns `core_0019_commit_keys` adds to the history's lines.
pub const COMMIT_KEYS_COLUMNS: [&str; 2] = ["text_key", "text_values"];

/// The mark of the project's export language (`projects.export_language`: `ja` or `en`): the
/// language the words an export, a diagram and a document make of their own are said in, one for
/// the project so that whoever writes it out gets the same files. A project of before is `ja`, the
/// language those words were in.
pub const EXPORT_LANGUAGE: &str = "core_0020_export_language";

/// The mark of the built-in kind of section `instances` replaced by `definition`, which holds the
/// shapes beside their data: every tree's sections of the kind of before, the history's and a
/// branch's frozen ones, and the project tree go by the kind of now, and a project's own kind named
/// `definition` is let go of or named anew (`definition_move`). Nothing of the tables changes; the
/// documents do, once.
pub const DEFINITION_KIND: &str = "core_0021_definition_kind";

/// The mark of the board's sections the card actions move cards into: each goals section directly
/// under the board's root holds one of the kind `complete` and one of `cancelled`, a goals section of
/// before named for one turned into it (`board_move`). Nothing of the tables changes; the board's
/// documents do, once.
pub const BOARD_SECTIONS: &str = "core_0022_board_sections";

/// The mark of the working directories as sections: each directly under main's root and every
/// branch's, listed in the project tree by its id, its section's document in main
/// (`workdir_move`). Nothing of the tables changes; the documents and the project's row do, once.
pub const WORKDIR_SECTIONS: &str = "core_0023_workdir_sections";

/// The mark of the code an implementation's line was recorded at (`tools::code_review`): one
/// table for the project, by the line's id — not a tree's content, so no history keeps it, a
/// locked main takes it, and a branch and main share a line's record.
pub const CODE_MARKS: &str = "core_0024_code_marks";

/// The table of the lines' recorded code, made where a store has none (`core_0024_code_marks`).
pub const CODE_MARKS_TABLE: &str = "
CREATE TABLE IF NOT EXISTS code_marks (
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  line_id    TEXT NOT NULL,
  file_id    TEXT NOT NULL,
  symbol     TEXT NOT NULL DEFAULT '',
  digest     TEXT NOT NULL,
  by         TEXT NOT NULL,
  at         TEXT NOT NULL,
  PRIMARY KEY (project_id, line_id)
);";

/// The mark of the groups on a diagram (`tools::groups`): `places` made anew so a row can be a
/// group's (`item_kind` `group`, its name the `item`) and hold how it looks (`look`). SQLite cannot
/// change a CHECK, so the table is made again under another name, its rows copied as they are, and
/// named back.
pub const MAP_GROUPS: &str = "core_0025_map_groups";

/// The mark of the built-in names of now (`name_move`): the kinds of line named as the screen
/// says them, an enum's value keyed by `name`, a goal's values in English.
pub const BUILTIN_NAMES: &str = "core_0026_builtin_names";

/// `places` as `core_0025_map_groups` has it, made under the name `places_new` to take the rows of
/// the one of before.
pub const PLACES_NEW: &str = "
CREATE TABLE places_new (
  project_id   TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  branch_id TEXT NOT NULL DEFAULT '',
  view      TEXT NOT NULL DEFAULT '',
  item_kind TEXT NOT NULL DEFAULT 'card' CHECK (item_kind IN ('card', 'section', 'node', 'group')),
  item      TEXT NOT NULL,
  x         INTEGER,
  y         INTEGER,
  w         INTEGER,
  h         INTEGER,
  pinned    INTEGER NOT NULL DEFAULT 0,
  look      TEXT,
  PRIMARY KEY (project_id, branch_id, view, item_kind, item)
);
INSERT INTO places_new (project_id, branch_id, view, item_kind, item, x, y, w, h, pinned)
  SELECT project_id, branch_id, view, item_kind, item, x, y, w, h, pinned FROM places;
DROP TABLE places;
ALTER TABLE places_new RENAME TO places;";

/// Every mark a store of the form has now, in the order they came.
pub const MARKS: [&str; 26] = [
    MARK,
    MCP_WRITES,
    CARD_GHOSTS,
    MR_PLACES,
    CONNECTIONS,
    CONTAINERS,
    LANDINGS,
    BOARD_KEYS,
    MERGE_REQUESTS,
    PROJECT_TREE,
    WORK_BRANCH,
    HOLD_CLIENTS,
    BRANCH_SNAPSHOTS,
    PROJECTS,
    EXPORTS,
    EXPORT_TARGETS,
    NO_CHANGELOG,
    INSPECTION_MARKS,
    COMMIT_KEYS,
    EXPORT_LANGUAGE,
    DEFINITION_KIND,
    BOARD_SECTIONS,
    WORKDIR_SECTIONS,
    CODE_MARKS,
    MAP_GROUPS,
    BUILTIN_NAMES,
];

/// Every table of the form, created empty.
pub const DDL: &str = "
CREATE TABLE projects (
  id          TEXT PRIMARY KEY,
  name        TEXT NOT NULL,
  slug        TEXT NOT NULL UNIQUE,
  description TEXT NOT NULL DEFAULT '',
  created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  tag_groups  TEXT,
  rules       TEXT,
  export_dirs TEXT,
  archived_at TEXT,
  locked_at   TEXT,
  mcp_writes  TEXT,
  kinds       TEXT,
  ai_merge    INTEGER NOT NULL DEFAULT 0,
  exports     TEXT,
  export_language TEXT NOT NULL DEFAULT 'ja'
);
CREATE TABLE _migrations (
  name       TEXT PRIMARY KEY,
  applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE TABLE default_view (
  project_id   TEXT PRIMARY KEY REFERENCES projects(id) ON DELETE CASCADE,
  nested    INTEGER NOT NULL DEFAULT 0,
  doc_view  TEXT,
  placement TEXT,
  rows      INTEGER,
  width     TEXT,
  gap       REAL,
  grid_gap  INTEGER
);
CREATE TABLE project_export_root (
  project_id        TEXT PRIMARY KEY REFERENCES projects(id) ON DELETE CASCADE,
  folder         TEXT NOT NULL,
  with_project_file INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE export_dir_folders (
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  dir_id  TEXT NOT NULL,
  folder  TEXT NOT NULL,
  PRIMARY KEY (project_id, dir_id)
);
CREATE TABLE export_default_folders (
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  export_id  TEXT NOT NULL,
  folder     TEXT NOT NULL,
  PRIMARY KEY (project_id, export_id)
);
CREATE TABLE access_keys (
  id          TEXT PRIMARY KEY,
  name        TEXT NOT NULL,
  digest      TEXT NOT NULL UNIQUE,
  can_control INTEGER NOT NULL DEFAULT 0,
  created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  used_at     TEXT,
  can_capture INTEGER NOT NULL DEFAULT 0,
  can_create  INTEGER NOT NULL DEFAULT 1
);
CREATE TABLE access_grants (
  key_id  TEXT NOT NULL REFERENCES access_keys(id) ON DELETE CASCADE,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  level   TEXT NOT NULL CHECK (level IN ('read', 'write')),
  PRIMARY KEY (key_id, project_id)
);
CREATE TABLE project_public (
  project_id TEXT PRIMARY KEY REFERENCES projects(id) ON DELETE CASCADE
);
CREATE TABLE batches (
  token      TEXT PRIMARY KEY,
  project_id    TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  branch_id  TEXT,
  created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  checked_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE TABLE batch_steps (
  token     TEXT NOT NULL REFERENCES batches(token) ON DELETE CASCADE,
  idx       INTEGER NOT NULL,
  tool      TEXT NOT NULL,
  arguments TEXT NOT NULL,
  PRIMARY KEY (token, idx)
);
CREATE TABLE branches (
  id         TEXT PRIMARY KEY,
  project_id    TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  name       TEXT NOT NULL COLLATE NOCASE,
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  archived_at TEXT,
  reverts    TEXT,
  UNIQUE (project_id, name)
);
CREATE TABLE card_docs (
  project_id    TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  branch_id  TEXT NOT NULL DEFAULT '',
  id         TEXT NOT NULL,
  section_id TEXT NOT NULL,
  name       TEXT NOT NULL COLLATE NOCASE,
  kind       TEXT NOT NULL,
  removed    INTEGER NOT NULL DEFAULT 0,
  doc        TEXT,
  base       TEXT,
  based_at   TEXT,
  PRIMARY KEY (project_id, branch_id, id)
);
CREATE UNIQUE INDEX card_docs_name ON card_docs(project_id, branch_id, section_id, name) WHERE removed = 0;
CREATE INDEX card_docs_id ON card_docs(id);
CREATE TABLE section_docs (
  project_id   TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  branch_id TEXT NOT NULL DEFAULT '',
  id        TEXT NOT NULL,
  parent_id TEXT,
  name      TEXT NOT NULL COLLATE NOCASE,
  removed   INTEGER NOT NULL DEFAULT 0,
  doc       TEXT,
  base      TEXT,
  based_at  TEXT,
  PRIMARY KEY (project_id, branch_id, id)
);
CREATE UNIQUE INDEX section_docs_name ON section_docs(project_id, branch_id, ifnull(parent_id, ''), name) WHERE removed = 0;
CREATE INDEX section_docs_id ON section_docs(id);
CREATE TABLE card_looks (
  project_id   TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  branch_id TEXT NOT NULL DEFAULT '',
  card_id   TEXT NOT NULL,
  row_path  TEXT NOT NULL DEFAULT '',
  field     TEXT NOT NULL,
  value     TEXT NOT NULL,
  PRIMARY KEY (project_id, branch_id, card_id, row_path, field)
);
CREATE TABLE places (
  project_id   TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  branch_id TEXT NOT NULL DEFAULT '',
  view      TEXT NOT NULL DEFAULT '',
  item_kind TEXT NOT NULL DEFAULT 'card' CHECK (item_kind IN ('card', 'section', 'node', 'group')),
  item      TEXT NOT NULL,
  x         INTEGER,
  y         INTEGER,
  w         INTEGER,
  h         INTEGER,
  pinned    INTEGER NOT NULL DEFAULT 0,
  look      TEXT,
  PRIMARY KEY (project_id, branch_id, view, item_kind, item)
);
CREATE TABLE commits (
  id          TEXT PRIMARY KEY,
  project_id     TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  branch_id   TEXT NOT NULL DEFAULT '',
  seq         INTEGER NOT NULL,
  at          TEXT NOT NULL,
  author_kind TEXT NOT NULL,
  author      TEXT,
  kind        TEXT NOT NULL DEFAULT 'edit',
  name        TEXT,
  reverted    INTEGER NOT NULL DEFAULT 0
);
CREATE UNIQUE INDEX commits_seq ON commits(project_id, branch_id, seq);
CREATE TABLE commit_lines (
  commit_id   TEXT NOT NULL REFERENCES commits(id) ON DELETE CASCADE,
  idx         INTEGER NOT NULL,
  kind        TEXT NOT NULL,
  target_type TEXT NOT NULL,
  target_id   TEXT,
  summary     TEXT NOT NULL,
  text_key    TEXT,
  text_values TEXT,
  PRIMARY KEY (commit_id, idx)
);
CREATE TABLE commit_rows (
  commit_id TEXT NOT NULL REFERENCES commits(id) ON DELETE CASCADE,
  tbl       TEXT NOT NULL,
  row_id    TEXT NOT NULL,
  before    TEXT,
  PRIMARY KEY (commit_id, tbl, row_id)
);
CREATE TABLE merge_requests (
  id          TEXT PRIMARY KEY,
  project_id     TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  number      INTEGER NOT NULL,
  branch_id   TEXT NOT NULL,
  branch_name TEXT NOT NULL,
  title       TEXT NOT NULL,
  description TEXT NOT NULL DEFAULT '',
  author      TEXT,
  at          TEXT NOT NULL,
  take        TEXT NOT NULL,
  result      TEXT NOT NULL,
  print       TEXT NOT NULL,
  state       TEXT NOT NULL DEFAULT 'open',
  reason      TEXT,
  decided_at  TEXT,
  merge_seq   INTEGER,
  places      INTEGER NOT NULL DEFAULT 0,
  keep        INTEGER NOT NULL DEFAULT 0
);
CREATE UNIQUE INDEX merge_requests_number ON merge_requests(project_id, number);
CREATE TABLE holds (
  project_id   TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  branch_id TEXT NOT NULL DEFAULT '',
  holder    TEXT NOT NULL,
  token     TEXT NOT NULL DEFAULT '',
  since     TEXT NOT NULL,
  from_seq  INTEGER NOT NULL DEFAULT 0,
  commit_id TEXT,
  client    TEXT NOT NULL DEFAULT '',
  PRIMARY KEY (project_id, branch_id, holder, token)
);
CREATE TABLE branch_prints (
  branch_id TEXT NOT NULL REFERENCES branches(id) ON DELETE CASCADE,
  card_id   TEXT NOT NULL,
  print     TEXT NOT NULL,
  path      TEXT NOT NULL DEFAULT '',
  PRIMARY KEY (branch_id, card_id)
);
CREATE TABLE branch_frozen (
  project_id   TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  branch_id TEXT NOT NULL REFERENCES branches(id) ON DELETE CASCADE,
  tbl       TEXT NOT NULL,
  id        TEXT NOT NULL,
  doc       TEXT,
  PRIMARY KEY (branch_id, tbl, id)
);
CREATE TABLE inspection_marks (
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  id         TEXT NOT NULL,
  grp        TEXT NOT NULL,
  print      TEXT NOT NULL,
  reason     TEXT NOT NULL,
  by         TEXT NOT NULL,
  at         TEXT NOT NULL,
  PRIMARY KEY (project_id, id)
);
CREATE TABLE code_marks (
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  line_id    TEXT NOT NULL,
  file_id    TEXT NOT NULL,
  symbol     TEXT NOT NULL DEFAULT '',
  digest     TEXT NOT NULL,
  by         TEXT NOT NULL,
  at         TEXT NOT NULL,
  PRIMARY KEY (project_id, line_id)
);
";

/// A document's print: what a branch remembers of a card of main, to tell whether main
/// has changed it since (`specboard://SpecBoard/Docs/Design:DesignBranches` §11). The digest of its text.
pub fn print(text: &str) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
