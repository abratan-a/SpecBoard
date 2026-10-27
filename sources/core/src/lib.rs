//! SpecBoard のコア: プロジェクトを読み書きする窓口。
//!
//! 画面もエージェントも、同じ 4 つで話す: 開く（`Core::open`）・呼ぶ（`Core::call`。頼みの JSON を渡すと
//! 答えの JSON が返る）・知らせを受ける（`Core::listen`）・閉じる（`Core::close`）。違うのは道（同じプロセスの
//! 中か、ネットワーク越しか）だけなので、頼みと答えはどちらも文字列の JSON で受け渡す。アプリはもう 1 つ、
//! 画面を動かすツール `app_control` に答える口（`App`）を渡す（`Core::set_app`）。
//!
//! 答えは、通れば `{"result": …}`、断れば `{"error": "<断る文>"}`。頼み（`op`）は:
//!
//! - `ping`: コアの版。同じ中身を知らせにも流す（窓口の道を通しで確かめる）
//! - `project_detail`: プロジェクト（`project` に id）を、`branch`（省くか `""` で main、branch の id、`~board` でボード）
//!   から見たとおりに丸ごと（セクション・カードの全部・入れ物の中のカード・全部の線・種類の定義）。画面が描く
//!   のはこれから（`screen_detail`。`specboard://SpecBoard/Docs/Design:CoreApi`「読む」）。項目ごとのハッシュ（`hashes`）と
//!   その digest を添え、`since` に読み手が持つ digest があれば、変わった項目だけで答える（`deltas`）
//! - `export_files`: プロジェクト（`project` に id）の main を、書き出しの設定どおりに書き出したファイル
//!   （`files` に「作業ディレクトリの名前/パス」ごとの中身）と、設定の問題（`problems`）。書き込みはしない
//! - `mcp`: MCP の JSON-RPC の 1 通（`message`。HTTP の頭は `headers` に小文字の名前で）への答え（`status` と `body`）。
//!   `authorization` のアクセスキーで誰かを確かめ（無い・知らない・取り消したキーは 401 と文の `body`）、キーに許した
//!   プロジェクトだけを読み書きする（`access`）。`app_control` はアプリの口（`App`）に頼む。2026 年の
//!   `subscriptions/listen` は確かめたうえで `listen` を付けて返し、知らせの流れは HTTP の側が開く
//! - アクセスキーと公開（`access`。画面の頼み）: `access_keys`・`access_key_create`・`access_key_rename`・
//!   `access_key_permit`・`access_key_revoke`・`access_key_regenerate`・`access_grant`・
//!   `project_public`・`project_public_set`
//! - MCP からの書き込みの許可（`tools::mcp_writes`。プロジェクトごと。画面の頼み）: `mcp_writes`・`mcp_writes_set`
//! - `project_file`: プロジェクト（`project` に id）のファイル（今の版。`projectfile::VERSION`）の文字
//! - 履歴と押さえ（`ops`。画面の頼み）: `commits`（木の履歴）・`revert`（戻す）・`undo_commit`（1 つのコミットだけ取り消す。かち合えば書かずに、かち合った所を答える）・`cut`（足を切る）・
//!   `name_commit`・`main_lock`・`group_begin`/`group_end`/`group_abandon`（まとめて直す）・`holds`
//! - プロジェクト丸ごと（`ops`。画面の頼み）: `project_file_peek`・`project_file_read`（ファイルから読み込む）・
//!   `project_archive_copy`・`project_archive`・`project_restore`・`project_delete`
//! - merge と MR（`tools::requests`。画面の頼み）: `merge_preview`・`merge`・`merge_requests`・
//!   `merge_request_approve`・`merge_request_reject`
//! - 画面が読み書きする口（`tools::screen`。id で読み書きする）: 読む `projects`・`branches`・`branch_marks`・
//!   `layout`・`workdir_detail`（作業ディレクトリの中身と設定）・`file_text`（ファイルの中身。`highlight` で色付け、`symbol` で関数と型の範囲）・`file_outline`（ファイルの関数と型）・`writing_rules`・`commit_changes`・`cut_cost`・`project_size`・
//!   `section_positions`・`embed_diagram`（文書の `![[…]]` の図）・`inserted_values`（文書の `{{…}}` の値）・`document_parts`（いくつもの文書の `[[…]]`・`{{…}}`・`![[…]]` を 1 回の読みで）・`kind_preview`（書いている種類の定義を、書いたとしたときの形で。何も書かない）、書く `tool`（MCP のツールを画面の立場で）・`project_rename`・
//!   `writing_rules_set`・`places`・`place_unplaced`・`prune`・
//!   `release_screen_holds`
//!
//! 書いた頼みは、書いた木ごとに知らせる（`{"op": "changed", "project", "branch", "rev"}`）。
//!
//! どの頼みにも `lang`（`text::LANGUAGES` のどれか）を添えられ、コアの作る文はその言語で答える（画面は画面の言語、
//! エージェントの `mcp` はいつも英語）。添えなければ日本語（`text::UNBOUND`）。

// The macro `text!` is used throughout, so the module comes before every other.
#[macro_use]
pub mod text;

pub mod access;
mod board_move;
mod code;
mod container_move;
mod definition_move;
mod deltas;
mod detail;
#[allow(dead_code)]
mod diff;
mod directory;
mod export;
mod export_move;
#[cfg(test)]
mod fixtures;
mod ghost_move;
mod ids;
mod key_move;
pub mod kinds;
mod line_move;
pub mod mcp;
mod model;
mod name_move;
mod nodes;
mod notetext;
mod ojson;
mod ops;
mod pathtext;
pub(crate) mod projectfile;
mod rows;
pub mod schema;
mod screen_detail;
mod store;
// MCP のツールを移している途中は、まだ繋いでいない所がある（移し終えたら外す）
#[allow(dead_code)]
mod tools;
mod tree_move;
mod wire;
mod workdir;
mod workdir_move;
pub mod write;

use serde_json::{Value, json};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

pub use store::BOARD;

/// The most one MCP request may carry in its body, and the most of files one call may read into
/// what it writes (`bodyFrom`, `stepsFrom`): a file never lets a call carry more than it could
/// inline.
pub const MAX_REQUEST_BYTES: usize = 8 << 20;

/// コアの版: アプリの版（リポの `VERSION`）。`ping` の答えと MCP の `serverInfo` に入る。
pub const VERSION: &str = include_str!("../../../VERSION").trim_ascii();

/// The grammars this machine adds of its own, looked for in `folder` (the agent's
/// `~/.specboard/grammars/`) from now on, and loaded now: a native library each, so the agent alone
/// loads them (`code::loader`).
#[cfg(feature = "dynamic-grammars")]
pub fn load_grammars(folder: PathBuf) {
    code::loader::set_dir(folder);
}

/// 何かが変わったときに知らせを受けるもの。知らせは JSON の文字列。
pub trait Listener: Send + Sync {
    fn changed(&self, notice: &str);
}

/// 画面を動かすツール `app_control` に答え、人に許可を求めるもの: アプリ。
///
/// `app_control`: コアは呼び出しを確かめてから（op に要るキーの項目（state・select・view・windows は app_control、
/// capture は app_control.capture）を許可か、人が今回だけ許したか、引数と op ごとの value の形）頼み、答えをツールの答えに
/// する（`tools::app`）。`arguments` は呼び出しの引数（JSON。書かれた順のまま）、`caller` は呼んだキー（`{"id", "name"}`。
/// キー無し（画面の頼み）なら `null`）。答えは JSON で、`{"json": 値}`・`{"blocks": [文…]}`・
/// `{"image": {"data": <base64>, "mimeType": …}, "note": 値}`・`{"error": 文}` のどれか。
///
/// `ask`: 都度承認の項目に当たる呼び出しが `request: true` を
/// 付けたとき、人に許可を求める。`request` は `{"tool", "key"（キーの名前）, "items": [{id, name, whose（project・key）,
/// whoseId, whoseName, whoseSlug（project のときだけ）}]（呼び出しの言語で）, "languages"（言語ごとの {"items": […]}。窓は画面の
/// 言語のものを出す）, "what"（何を。呼び出しの引数か、バッチの手の一覧の文）, "seconds"（待つ秒）}`。人が答えるまで
/// （`seconds` まで）待って、`{"answer": "once" | "always" | "deny" | "never", "client": 答えた Mac}`・`{"timeout": true}`・
/// `{"error": 文}` のどれかを返す。待ち切ったら求めを取り下げる。答える口が無いもの（テストなど）は、既定のとおり断る。
///
/// `picture`: `section_read`・`card_read` の `as: picture`。コアが確かめて何の図かを決めた頼み `request`（JSON。
/// `{"picture": "sectionMap" | "connectionsMap" | "flowDiagram" | "contentMap" | "mock", "project": <id>, "tree": <木>,
/// "section" | "card": <id>, "depth"?, "kinds"?, "cards"?: [<id>], "area"?, "margin"?, "html"?, "width"?, "height"?,
/// "maxSize", "lang"}`）を、画面と同じ描き方で描く（`tools::picture`）。`mock` は UI のセクションのモックで、コアが組み上げた
/// ページ `html`（幅 `width`・高さ `height` ピクセル）を、そのまま画面の外で撮る。答えは `{"image": {"data", "mimeType"}, "note": {picture, width, height,
/// scale, area?, cards, documentsPending?}}` か `{"error": 文}`。描く口の無いもの（本体にアプリが繋いでいない・テストなど）は、
/// 既定のとおり断る。
///
/// どれも呼ぶのは MCP の要求を受けたスレッドで、置き場の鍵は持たない（アプリが答えるあいだに、ほかのスレッドからコアを
/// 呼んでよい）。
pub trait App: Send + Sync {
    fn app_control(&self, arguments: &str, caller: &str) -> String;

    fn ask(&self, request: &str) -> String {
        let _ = request;
        serde_json::json!({ "error": tools::asking::no_screen() }).to_string()
    }

    fn picture(&self, request: &str) -> String {
        let _ = request;
        serde_json::json!({ "error": tools::picture::no_app() }).to_string()
    }
}

/// 開けなかったわけ。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenError {
    /// 置き場が空の文字列だった。
    NoPlace,
    /// データベースを開けない・形が違う。
    Store(String),
}

impl fmt::Display for OpenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OpenError::NoPlace => write!(f, "{}", text!("core.error.noPlace")),
            OpenError::Store(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for OpenError {}

/// 開いたコア。データベースのファイルごとに 1 つ。
///
/// 書くのは 1 本の接続で 1 つずつ（`store`）。読むだけの頼みは、書き込みと別の接続で、書き終えた（コミットした）形だけを
/// 読み、書いている間も待たない（`reading`）。
pub struct Core {
    place: PathBuf,
    store: Mutex<store::Store>,
    /// The connections that read apart from the writes, not in use now (`reading`); none when the
    /// store keeps no write-ahead log (`Store::logged_ahead`): then a read takes the writes' lock.
    readers: Option<Mutex<Vec<store::Store>>>,
    /// What the working directories read, shared by the connections (`workdir::Shared`).
    work: workdir::Shared,
    /// The access keys a request came by while a write held the store, to note as used once it is
    /// free (`Core::caller`).
    used: Mutex<Vec<String>>,
    listeners: Mutex<Vec<Arc<dyn Listener>>>,
    /// What answers `app_control`; none until the app hands it over.
    app: Mutex<Option<Arc<dyn App>>>,
    closed: AtomicBool,
    /// The mark of the last change told about: one more for each tree a write changed.
    rev: AtomicU64,
    /// What each reader was sent of the trees it read (`project_detail`), to answer it next with
    /// what changed.
    sent: deltas::Sent,
}

/// How many times a write on this thread worked the whole project out of the documents so far
/// (`Seen::in_tx`: asked again with no row changed between, it is not). For the tests that hold
/// what a write costs; nothing else needs it.
#[doc(hidden)]
pub fn project_rebuilds_on_this_thread() -> u64 {
    tools::rebuilds_on_this_thread()
}

/// How many times a write on this thread brought the project it had worked out up to the cards
/// made, let go of or put elsewhere since, going over every card (`Seen::brought_up_whole`): less
/// than working it out whole, but the project's size each time. For the tests that hold what a
/// write costs; nothing else needs it.
#[doc(hidden)]
pub fn project_bring_ups_on_this_thread() -> u64 {
    tools::brought_whole_on_this_thread()
}

/// How many projects worked out whole are held on this thread now: none once a call is done — what
/// a write keeps worked out is let go with its transaction.
/// For the tests; nothing else needs it.
#[doc(hidden)]
pub fn projects_held_on_this_thread() -> i64 {
    tools::held_on_this_thread()
}

/// Where arranging along lines puts boxes of `sizes` (cells), each pair in `pointers` a line from a
/// card to the card one of its fields points at, `margin` cells round each: the computation alone,
/// for the tests that hold what arranging costs. Nothing else needs it.
#[doc(hidden)]
pub fn arranged_by_pointers(
    sizes: &[(i64, i64)],
    pointers: &[(usize, usize)],
    margin: i64,
) -> Vec<(i64, i64)> {
    tools::arrange::pointed(sizes, pointers, margin)
}

impl Core {
    /// データベースのファイルを開く。コアの形のもの（`schema`）だけを開ける。
    pub fn open(place: impl AsRef<Path>) -> Result<Core, OpenError> {
        let place = place.as_ref();
        if place.as_os_str().is_empty() {
            return Err(OpenError::NoPlace);
        }
        let store = store::Store::open(place).map_err(OpenError::Store)?;
        // Read apart only where the store keeps a write-ahead log: what a write has not committed
        // is in it, not in the file, and the reading connections do not see it.
        let readers = store
            .logged_ahead()
            .unwrap_or(false)
            .then(|| Mutex::new(Vec::new()));
        Ok(Core {
            place: place.to_path_buf(),
            store: Mutex::new(store),
            readers,
            work: workdir::Shared::default(),
            used: Mutex::new(Vec::new()),
            listeners: Mutex::new(Vec::new()),
            app: Mutex::new(None),
            closed: AtomicBool::new(false),
            rev: AtomicU64::new(0),
            sent: deltas::Sent::default(),
        })
    }

    /// 新しい空のデータベース（コアの形）を作って開く。もうあれば断る。
    pub fn create(place: impl AsRef<Path>) -> Result<Core, OpenError> {
        let path = place.as_ref();
        if path.as_os_str().is_empty() {
            return Err(OpenError::NoPlace);
        }
        store::Store::create(path).map_err(OpenError::Store)?;
        Core::open(path)
    }

    /// 開いているデータベースのファイル。
    pub fn place(&self) -> &Path {
        &self.place
    }

    /// 頼みの JSON を受けて、答えの JSON を返す。断るときも答えは返す（`{"error": …}`）。
    pub fn call(&self, request: &str) -> String {
        answer(self.handle(request))
    }

    /// 知らせを受けるものを足す。閉じるまで、変わるたびに知らせる。
    pub fn listen(&self, listener: Arc<dyn Listener>) {
        self.listeners
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(listener);
    }

    /// Lets go of what was sent to the reader `connection` (`project_detail`'s `connection`): the
    /// agent's, when an app's connection goes.
    pub fn forget_reader(&self, connection: &str) {
        self.sent.forget(connection);
    }

    /// `app_control` に答えるものを渡す。あとから渡したものが答える。
    pub fn set_app(&self, app: Arc<dyn App>) {
        *self.app.lock().unwrap_or_else(|e| e.into_inner()) = Some(app);
    }

    /// 閉じる。閉じたあとの頼みは断り、知らせも止める（`app_control` に答えるものも放す）。
    pub fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
        self.listeners
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        *self.app.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }

    /// What answers `app_control` now, if anything does: taken out, so it is not held
    /// while the app answers.
    pub(crate) fn app(&self) -> Option<Arc<dyn App>> {
        self.app.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn handle(&self, request: &str) -> Result<Value, String> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(text!("core.error.closed"));
        }
        let request: Value =
            serde_json::from_str(request).map_err(|e| text!("core.error.badRequest", e))?;
        // Everything the call says, down to the tools a batch runs, is said in the language it
        // asked for.
        let _lang = text::bind(request.get("lang").and_then(Value::as_str));
        let op = request
            .get("op")
            .and_then(Value::as_str)
            .ok_or_else(|| text!("core.error.noOp"))?;
        match op {
            "ping" => {
                let result = json!({ "core": VERSION });
                self.tell(&json!({ "op": "ping", "core": VERSION }));
                Ok(result)
            }
            "project_detail" => {
                let project = request
                    .get("project")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        text!(
                            "ops.error.needs",
                            op = "project_detail",
                            key = "project",
                            what = text!("ops.what.projectId")
                        )
                    })?;
                let branch = request
                    .get("branch")
                    .and_then(Value::as_str)
                    .filter(|b| !b.is_empty());
                let detail = {
                    let store = self.reading()?;
                    screen_detail::read(&store.reader(), project, branch)?
                };
                // The agent's connection the request came by; the app that holds the core is
                // the one reader of its own.
                let reader = match request.get("connection") {
                    Some(Value::String(connection)) => connection.clone(),
                    Some(other) => other.to_string(),
                    None => String::new(),
                };
                let since = request.get("since").and_then(Value::as_str);
                Ok(self
                    .sent
                    .answer(&reader, project, branch.unwrap_or(""), detail, since))
            }
            "export_files" => {
                let project = request
                    .get("project")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        text!(
                            "ops.error.needs",
                            op = "export_files",
                            key = "project",
                            what = text!("ops.what.projectId")
                        )
                    })?;
                let store = self.reading()?;
                let detail = model::typed(&detail::read(&store.reader(), project, None)?)?;
                let exports = store.exports(project)?;
                let grid = crate::tools::grid::Layout::read(&store.conn, project)?;
                let built = export::build(&detail, &exports, &grid);
                let names: std::collections::HashMap<&str, &str> = exports
                    .iter()
                    .map(|e| (e.id.as_str(), e.name.as_str()))
                    .collect();
                // By "<export>/<path>", whatever the ids of the exports are.
                let mut by_name: std::collections::BTreeMap<String, &String> =
                    std::collections::BTreeMap::new();
                for (export, by_path) in &built.files {
                    for (path, content) in by_path {
                        let name = names.get(export.as_str()).copied().unwrap_or(export);
                        by_name.insert(format!("{name}/{path}"), content);
                    }
                }
                let mut files = serde_json::Map::new();
                for (key, content) in by_name {
                    files.insert(key, content.clone().into());
                }
                Ok(json!({
                    "files": files,
                    "problems": crate::export::Problem::texts(&built.problems),
                }))
            }
            "mcp" => {
                let message = request.get("message").cloned().unwrap_or(Value::Null);
                let headers = request
                    .get("headers")
                    .and_then(Value::as_object)
                    .cloned()
                    .unwrap_or_default();
                // Who is asking, by the access key the request carries; turned away, before
                // anything is read, without one of this store's.
                let Some(caller) = self.caller(&headers)? else {
                    return Ok(json!({ "status": 401, "body": access::key_refused(&headers) }));
                };
                let who = write::Who::mcp(Some(&caller.name));
                let run = |name: &str,
                           arguments: &serde_json::Map<String, Value>|
                 -> Result<mcp::Output, String> {
                    access::as_caller(&caller, name, || self.tool(&who, name, arguments))
                };
                let reply = mcp::answer(&message, &headers, &run);
                let mut out = json!({ "status": reply.status, "body": reply.body });
                // A stream to open: the HTTP side's, told the key and the message passed.
                if reply.listen {
                    out["listen"] = true.into();
                }
                Ok(out)
            }
            "project_file" => {
                let project = request
                    .get("project")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        text!(
                            "ops.error.needs",
                            op = "project_file",
                            key = "project",
                            what = text!("ops.what.projectId")
                        )
                    })?;
                let store = self.reading()?;
                projectfile::write(&store.conn, project).map(|text| json!({ "text": text }))
            }
            other => self
                .history_op(other, &request)
                .or_else(|| self.merge_op(other, &request))
                .or_else(|| self.access_op(other, &request))
                .or_else(|| self.screen_op(other, &request))
                .unwrap_or_else(|| Err(text!("core.error.notYet", other))),
        }
    }

    /// The store, to write: one at a time. What the working directories read that others gave out
    /// is taken in first, and what this reads anew of them is given out when it is let go
    /// (`workdir::Shared`; a transaction undone undoes it too).
    fn lock(&self) -> Locked<'_> {
        let store = self.store.lock().unwrap_or_else(|e| e.into_inner());
        // Not taken in, it is read again where it is asked for.
        let _ = workdir::take_in(&store, &self.work);
        Locked { core: self, store }
    }

    /// The store, to read what was committed: on a connection apart from the writes when the store
    /// keeps a write-ahead log, in a transaction of its own (what it reads is all of one moment,
    /// and never what a write has not committed yet), not waiting for a write going on. Else under
    /// the writes' lock, as before.
    pub(crate) fn reading(&self) -> Result<Reading<'_>, String> {
        let Some(idle) = &self.readers else {
            return Ok(Reading::Locked(self.lock()));
        };
        let kept = idle.lock().unwrap_or_else(|e| e.into_inner()).pop();
        let store = match kept {
            Some(store) => store,
            None => match store::Store::open_reader(&self.place) {
                Ok(store) => store,
                Err(_) => return Ok(Reading::Locked(self.lock())),
            },
        };
        let _ = workdir::take_in(&store, &self.work);
        store
            .conn
            .execute_batch("BEGIN")
            .map_err(|e| text!("store.error.read", e))?;
        Ok(Reading::Apart {
            core: self,
            store: Some(store),
        })
    }

    fn next_rev(&self) -> u64 {
        self.rev.fetch_add(1, Ordering::SeqCst) + 1
    }

    fn tell(&self, notice: &Value) {
        let text = notice.to_string();
        let listeners = self
            .listeners
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        for listener in listeners {
            listener.changed(&text);
        }
    }
}

/// The store held to write (`Core::lock`).
pub(crate) struct Locked<'c> {
    core: &'c Core,
    store: std::sync::MutexGuard<'c, store::Store>,
}

impl std::ops::Deref for Locked<'_> {
    type Target = store::Store;
    fn deref(&self) -> &store::Store {
        &self.store
    }
}

impl std::ops::DerefMut for Locked<'_> {
    fn deref_mut(&mut self) -> &mut store::Store {
        &mut self.store
    }
}

impl Drop for Locked<'_> {
    fn drop(&mut self) {
        // What a write read anew of the working directories and committed, for the others.
        let _ = workdir::give_out(&self.store, &self.core.work, true);
        // The keys that came while the store was held, noted as used now.
        let used = std::mem::take(&mut *self.core.used.lock().unwrap_or_else(|e| e.into_inner()));
        for key in used {
            access::note_used(&self.store.conn, &key);
        }
    }
}

/// The store held to read (`Core::reading`).
pub(crate) enum Reading<'c> {
    /// On a connection of its own, in a transaction of its own; given back to the idle ones when
    /// done.
    Apart {
        core: &'c Core,
        store: Option<store::Store>,
    },
    /// Under the writes' lock (a store without a write-ahead log).
    Locked(Locked<'c>),
}

impl std::ops::Deref for Reading<'_> {
    type Target = store::Store;
    fn deref(&self) -> &store::Store {
        match self {
            Reading::Apart { store, .. } => store
                .as_ref()
                .expect("a reading kept apart holds its store"),
            Reading::Locked(locked) => locked,
        }
    }
}

/// How many reading connections are kept for the next reads, at most.
const IDLE_READERS: usize = 4;

impl Drop for Reading<'_> {
    fn drop(&mut self) {
        let Reading::Apart { core, store } = self else {
            return;
        };
        let Some(store) = store.take() else {
            return;
        };
        // Done: what it read the working directories as is kept (`temp`), and given out.
        if store.conn.execute_batch("COMMIT").is_err()
            && store.conn.execute_batch("ROLLBACK").is_err()
        {
            return;
        }
        let _ = workdir::give_out(&store, &core.work, false);
        if let Some(idle) = &core.readers {
            let mut idle = idle.lock().unwrap_or_else(|e| e.into_inner());
            if idle.len() < IDLE_READERS {
                idle.push(store);
            }
        }
    }
}

fn answer(outcome: Result<Value, String>) -> String {
    match outcome {
        Ok(result) => json!({ "result": result }),
        // What a refusal says beside its words is MCP's (`mcp::refusal`): the window reads the words.
        Err(message) => json!({ "error": mcp::refusal_words(&message) }),
    }
    .to_string()
}

/// The sample project and its branch: their ids, as its project file keeps them
/// (`tests/fixtures/projects/sample.json`).
#[cfg(test)]
pub(crate) fn built_ids() -> (String, String) {
    let file: Value = serde_json::from_str(&fixtures::project_file("sample")).unwrap();
    let project = file["project"]["id"].as_str().unwrap().to_string();
    let branch = file["branches"][0]["id"].as_str().unwrap().to_string();
    (project, branch)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Heard(Mutex<Vec<String>>);

    impl Listener for Heard {
        fn changed(&self, notice: &str) {
            self.0.lock().unwrap().push(notice.to_string());
        }
    }

    fn parse(text: &str) -> Value {
        serde_json::from_str(text).unwrap()
    }

    /// A store of the core's form with the sample project, read once for every test here.
    fn core() -> Core {
        Core::open(fixtures::read_only_sample()).unwrap()
    }

    #[test]
    fn ping_answers_with_the_version_and_tells_listeners() {
        let core = core();
        let heard = Arc::new(Heard(Mutex::new(Vec::new())));
        core.listen(heard.clone());
        let reply = parse(&core.call(r#"{"op":"ping"}"#));
        assert_eq!(reply, json!({ "result": { "core": VERSION } }));
        let notices = heard.0.lock().unwrap().clone();
        assert_eq!(notices.len(), 1);
        assert_eq!(parse(&notices[0]), json!({ "op": "ping", "core": VERSION }));
    }

    #[test]
    fn requests_it_cannot_take_are_refused_with_a_reason() {
        let core = core();
        assert_eq!(
            parse(&core.call(r#"{"op":"card_read"}"#)),
            json!({ "error": "「card_read」はまだ使えません" })
        );
        assert_eq!(
            parse(&core.call(r#"{}"#)),
            json!({ "error": "頼みに op がありません" })
        );
        let broken = parse(&core.call("{"));
        assert!(
            broken["error"]
                .as_str()
                .unwrap()
                .starts_with("頼みの JSON が読めません")
        );
        assert_eq!(
            parse(&core.call(r#"{"op":"project_detail","project":"none"}"#)),
            json!({ "error": "プロジェクト「none」がありません" })
        );
    }

    #[test]
    fn a_closed_core_refuses_and_stops_telling() {
        let core = core();
        let heard = Arc::new(Heard(Mutex::new(Vec::new())));
        core.listen(heard.clone());
        core.close();
        assert_eq!(
            parse(&core.call(r#"{"op":"ping"}"#)),
            json!({ "error": "閉じています" })
        );
        assert!(heard.0.lock().unwrap().is_empty());
    }

    #[test]
    fn a_new_store_is_made_empty_and_opened_and_not_made_twice() {
        let place = fixtures::place("core-create");
        let core = Core::create(&place).unwrap();
        assert_eq!(
            parse(&core.call(r#"{"op":"projects"}"#)),
            json!({ "result": { "projects": [] } })
        );
        core.close();
        let again = Core::create(&place).err().unwrap();
        assert!(again.to_string().starts_with("データベースがもうあります"));
        assert!(Core::open(&place).is_ok());
    }

    #[test]
    fn what_does_not_open() {
        assert_eq!(Core::open("").err(), Some(OpenError::NoPlace));
        let missing = Core::open("/nonexistent/specboard.db").err().unwrap();
        assert!(missing.to_string().starts_with("データベースがありません"));
        // A database of another form is not opened.
        let other = fixtures::place("other");
        rusqlite::Connection::open(&other)
            .unwrap()
            .execute_batch("CREATE TABLE projects (id TEXT PRIMARY KEY, name TEXT)")
            .unwrap();
        let of_another_form = Core::open(&other).err().unwrap();
        assert!(
            of_another_form
                .to_string()
                .starts_with("コアの形のデータベースではありません")
        );
    }

    /// A store of the core's form of its own, for a test that writes.
    fn written() -> Core {
        Core::open(fixtures::sample()).unwrap()
    }

    #[test]
    fn main_locked_and_let_go_is_in_its_history_and_told() {
        let (project, _) = built_ids();
        let project = project.as_str();
        let core = written();
        let heard = Arc::new(Heard(Mutex::new(Vec::new())));
        core.listen(heard.clone());
        let call = |request: Value| parse(&core.call(&request.to_string()));
        assert_eq!(
            call(json!({"op": "main_lock", "project": project, "on": true})),
            json!({ "result": {} })
        );
        // Put right in one go in a branch of the app's own; given up, nothing in main's history.
        let begun = call(json!({"op": "group_begin", "project": project}));
        assert!(begun["result"]["branch"].is_string(), "{begun}");
        call(json!({"op": "group_abandon", "project": project}));
        call(json!({"op": "main_lock", "project": project, "on": false}));
        let commits =
            call(json!({"op": "commits", "project": project}))["result"]["commits"].clone();
        let lines: Vec<&str> = commits
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["lines"][0]["summary"].as_str().unwrap())
            .collect();
        // After the project file read in.
        assert_eq!(
            lines,
            [
                "main のロックを外した",
                "main をロックした",
                "プロジェクトのファイル（版 33）から読み込んだ"
            ]
        );
        let heard = heard.0.lock().unwrap();
        // Locked, main and the group's branch held and let go, unlocked.
        assert_eq!(heard.len(), 6);
        assert_eq!(
            parse(&heard[0]),
            json!({ "op": "changed", "project": project, "branch": "", "rev": 1 })
        );
    }

    #[test]
    fn the_window_holds_a_tree_and_lets_it_go() {
        let (project, _) = built_ids();
        let project = project.as_str();
        let core = written();
        let call = |request: Value| parse(&core.call(&request.to_string()));
        call(json!({"op": "group_begin", "project": project, "branch": BOARD}));
        let holds = call(json!({"op": "holds", "project": project, "branch": BOARD}));
        assert_eq!(holds["result"]["holds"][0]["holder"], "screen");
        // The board's history: the project file read in.
        assert_eq!(holds["result"]["holds"][0]["fromSeq"], 1);
        assert_eq!(
            call(json!({"op": "group_abandon", "project": project, "branch": BOARD})),
            json!({ "result": { "undone": 0 } })
        );
        assert_eq!(
            call(json!({"op": "holds", "project": project, "branch": BOARD}))["result"]["holds"],
            json!([])
        );
        assert_eq!(
            call(json!({"op": "revert", "project": project, "to": 9}))["error"],
            "コミット 9 はありません（無い番号か、足を切ったより前です）"
        );
        assert_eq!(
            call(json!({"op": "name_commit", "project": project, "seq": 1, "name": "はじまり"})),
            json!({ "result": {} })
        );
        // The first: the project file read in, named.
        let commits =
            call(json!({"op": "commits", "project": project}))["result"]["commits"].clone();
        assert_eq!(commits[0]["seq"], 1);
        assert_eq!(commits[0]["name"], "はじまり");
        assert_eq!(
            call(json!({"op": "commits", "project": project, "branch": "nope"}))["error"],
            "branch「nope」が見つかりません"
        );
        assert_eq!(
            call(json!({"op": "commits", "project": "nope"}))["error"],
            "プロジェクトが見つかりません"
        );
    }

    /// The sample project's note `Free:Memo` as the window reads it (`project_detail`): its id and
    /// label.
    fn first_card(core: &Core, project: &str) -> (String, Value) {
        let read =
            parse(&core.call(&json!({"op": "project_detail", "project": project}).to_string()));
        let card = read["result"]["schemas"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == "Memo")
            .unwrap();
        (
            card["id"].as_str().unwrap().to_string(),
            card["label"].clone(),
        )
    }

    /// The card `id` of the sample project's main given the label `label`, in `tx`, not committed.
    fn relabelled(tx: &mut write::Tx, project: &str, id: &str, label: &str) {
        let mut doc = tx
            .seen(write::Table::Cards, project, "", id)
            .unwrap()
            .unwrap()
            .doc()
            .unwrap();
        doc.entry("fields")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .unwrap()
            .insert("label".into(), label.into());
        tx.put_card(project, "", &doc).unwrap();
    }

    /// A key that writes the sample project, for MCP: its id and its value.
    fn key(core: &Core, project: &str) -> (String, String) {
        let call = |request: Value| parse(&core.call(&request.to_string()));
        let made = call(json!({"op": "access_key_create", "name": "読む"}));
        let id = made["result"]["key"]["id"].as_str().unwrap().to_string();
        let given =
            call(json!({"op": "access_grant", "key": id, "project": project, "level": "write"}));
        assert!(given.get("error").is_none(), "{given}");
        (id, made["result"]["value"].as_str().unwrap().to_string())
    }

    /// A read — the window's, and MCP's — answers while a write holds the store, with what was
    /// committed; what the write has not committed is not read, and once it is, it is. The
    /// order is held, not the time: the write
    /// is left open until the reads are done (were a read to wait for it, it would never be).
    #[test]
    fn a_read_does_not_wait_for_a_write_and_reads_only_what_was_committed() {
        let (project, _) = built_ids();
        let project = project.as_str();
        let core = written();
        let (id, before) = first_card(&core, project);
        let (key_id, key) = key(&core, project);
        let card_read = || -> Value {
            let message = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
                "params": {"name": "card_read", "arguments": {"project": project, "target": "Free:Memo"}}});
            let reply = parse(&core.call(
                &json!({"op": "mcp", "message": message, "headers": {"authorization": format!("Bearer {key}")}})
                    .to_string(),
            ));
            let text = reply["result"]["body"]["result"]["content"][0]["text"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            serde_json::from_str(&text).unwrap_or(Value::String(text))
        };
        let (mcp_before, label_of) = (card_read(), |v: &Value| v.get("label").cloned());
        let (held, holding) = std::sync::mpsc::channel();
        let (go, going) = std::sync::mpsc::channel::<()>();
        let (core, id, card_read) = (&core, &id, &card_read);
        std::thread::scope(|s| {
            s.spawn(move || {
                let mut store = core.lock();
                let mut tx = store.write(write::Who::screen()).unwrap();
                relabelled(&mut tx, project, id, "書きかけ");
                held.send(()).unwrap();
                going.recv().unwrap();
                tx.commit().unwrap();
            });
            holding.recv().unwrap();
            let (read, reading) = std::sync::mpsc::channel();
            s.spawn(move || {
                let screen = first_card(core, project);
                let mcp = card_read();
                read.send((screen, mcp)).unwrap();
            });
            let answered = reading.recv_timeout(std::time::Duration::from_secs(120));
            // Let the write go whatever came: a read that waited for it ends then, and is said.
            go.send(()).unwrap();
            let ((read_id, label), mcp) = answered.expect("読みが、書いている間待った");
            assert_eq!(read_id, *id);
            assert_eq!(label, before, "書きかけを読んだ");
            assert_eq!(
                label_of(&mcp),
                label_of(&mcp_before),
                "書きかけを読んだ: {mcp}"
            );
        });
        // Committed: read.
        assert_eq!(first_card(core, project).1, json!("書きかけ"));
        let after = card_read();
        assert_eq!(label_of(&after), Some(json!("書きかけ")), "{after}");
        // The key the reads came by while the write held the store is noted as used once it let go.
        let keys = parse(&core.call(r#"{"op":"access_keys"}"#));
        let noted = keys["result"]["keys"]
            .as_array()
            .unwrap()
            .iter()
            .find(|k| k["id"] == key_id.as_str())
            .unwrap()["usedAt"]
            .clone();
        assert!(noted.is_string(), "{keys}");
    }

    /// One reading is of one moment: what is committed while it goes on is read by the next.
    #[test]
    fn a_reading_reads_one_moment_and_the_next_reads_what_was_committed_since() {
        let (project, _) = built_ids();
        let project = project.as_str();
        let core = written();
        let (id, before) = first_card(&core, project);
        let label = |store: &store::Store| -> Value {
            let read = detail::read(&store.reader(), project, None).unwrap();
            read["schemas"]
                .as_array()
                .unwrap()
                .iter()
                .find(|c| c["id"] == id.as_str())
                .unwrap()["label"]
                .clone()
        };
        let reading = core.reading().unwrap();
        assert!(matches!(reading, Reading::Apart { .. }));
        assert_eq!(label(&reading), before);
        {
            let mut store = core.lock();
            let mut tx = store.write(write::Who::screen()).unwrap();
            relabelled(&mut tx, project, &id, "その間に");
            tx.commit().unwrap();
        }
        assert_eq!(label(&reading), before, "読んでいる間に変わった");
        drop(reading);
        assert_eq!(label(&core.reading().unwrap()), json!("その間に"));
    }

    /// A store is opened keeping a write-ahead log, and what reads apart from the writes cannot write
    /// it: only what the working directories read is kept on it (`temp`).
    #[test]
    fn what_reads_apart_cannot_write_the_store() {
        let core = written();
        let mode: String = rusqlite::Connection::open(core.place())
            .unwrap()
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .unwrap();
        assert_eq!(mode, "wal");
        let reading = core.reading().unwrap();
        for sql in [
            "UPDATE projects SET name = 'x'",
            "DELETE FROM card_docs",
            "INSERT INTO _migrations (name) VALUES ('x')",
            "CREATE TABLE x (a)",
        ] {
            assert!(reading.conn.execute(sql, []).is_err(), "{sql} が通った");
        }
        reading
            .conn
            .execute_batch("CREATE TEMP TABLE IF NOT EXISTS t (a); INSERT INTO temp.t VALUES (1);")
            .unwrap();
    }

    #[test]
    fn a_project_file_read_over_its_project_keeps_an_archived_copy_first() {
        let (project, _) = built_ids();
        let project = project.as_str();
        let core = written();
        let call = |request: Value| parse(&core.call(&request.to_string()));
        let text =
            call(json!({"op": "project_file", "project": project}))["result"]["text"].clone();
        let peeked = call(json!({"op": "project_file_peek", "text": text}));
        assert_eq!(peeked["result"]["id"], project);
        assert_eq!(peeked["result"]["there"], true);
        let read = call(json!({"op": "project_file_read", "text": text, "archiveFirst": true}));
        assert_eq!(read["result"]["project"], project);
        let archived = read["result"]["archived"].as_str().unwrap().to_string();
        assert!(
            call(json!({"op": "main_lock", "project": archived, "on": true}))["error"]
                .as_str()
                .unwrap()
                .ends_with("はアーカイブです（読むだけ）")
        );
        assert_eq!(
            call(json!({"op": "project_delete", "project": archived})),
            json!({ "result": {} })
        );
        assert_eq!(
            call(json!({"op": "project_archive", "project": archived}))["error"],
            "プロジェクトが見つかりません"
        );
        let copy = call(
            json!({"op": "project_file_read", "text": text, "landing": "copy", "name": "写し"}),
        );
        assert!(copy["result"]["project"].is_string(), "{copy}");
        assert_ne!(copy["result"]["project"], project);
    }
}
