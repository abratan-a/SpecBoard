//! 言語ごとの文の表（`specboard://SpecBoard/Docs/Design:Localization` §1・§3）。
//!
//! コードは呼び名と差し込む値だけを書き（`text!(<呼び名>)`・`text!(<呼び名>, id = id)`・`text!(<呼び名>, id)`。
//! 呼び名は `部位.もの.こと` の英語の camelCase）、文は `<言語>/<部位>.json`（呼び名 → 文）から、今の言語で作る。
//! 部位ごとのファイルに分けるのは、いくつものセッションが同時に文を足しても同じファイルを直さないため（ファイルは
//! `build.rs` が見つけて埋め込むので、部位を足すのはファイルを置くことだけ）。表はどの言語も同じ呼び名と同じ差し込む
//! 名前を持つ（テストが見張る）ので、言語を足すのは `languages!` に 1 つ足してその言語のフォルダを置くこと。
//!
//! 差し込みは `{name}`（名前は ASCII の英数字と `_`）。それ以外の `{`（`{{パス}}` の差し込みの書き方や JSON の例）は
//! 文のまま出す。`{{` で始まる所は差し込みとして読まない（英語の文の `{{path}}` を名前と取り違えないように）。
//!
//! 今の言語は呼び出しごとにスレッドへ結び付ける（`bind`）。呼び出しは取ったスレッドで最後まで走るので、中で呼ぶ
//! ものにも同じ言語が届く。結び付けが無ければプロセスの既定（`set_default`。エージェントはその機械の言語）、それも
//! 無ければ日本語（前からの文のまま）。
//!
//! 残す文（履歴の 1 行）と、言い方でなく中身で見分ける文（点検の件）は、作った時に文にせず呼び名と値のまま持つ
//! （`keyed!` の `Keyed`）。読む時に、読む人の言語で文にする。

use serde_json::{Map, Value};
use std::cell::Cell;
use std::collections::HashMap;
use std::fmt;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

macro_rules! languages {
    ($($code:literal),+ $(,)?) => {
        /// 文を出せる言語。並びは画面の選びの並び（アプリの `Language.supported` と同じ）。
        pub const LANGUAGES: &[&str] = &[$($code),+];
    };
}

languages!("ja", "en");

// The tables of every language folder, by `build.rs`: `SOURCES`.
include!(concat!(env!("OUT_DIR"), "/text_tables.rs"));

/// 結び付けの無い呼び出しの言語: 言語を言わない頼み（前からの画面・テスト）は前からの文のまま答える。
pub const UNBOUND: &str = "ja";

/// `system` の並びに表のある言語が 1 つも無いときの言語: 知らない言語の人にも読める見込みが一番高い。
pub const FALLBACK: &str = "en";

/// 言語の書き方（`ja`・`ja-JP`・`en_US.UTF-8`・`EN`）から、表のある言語。表の無い言語と `C`・`POSIX` は無し。
pub fn supported(tag: &str) -> Option<&'static str> {
    let base = tag
        .split(['-', '_', '.', '@'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    LANGUAGES.iter().copied().find(|code| *code == base)
}

/// `system` の言語: その機械が優先する言語の並びの先頭から、表のある言語を探す。無ければ `FALLBACK`。
pub fn resolve_system<'a>(preferred: impl IntoIterator<Item = &'a str>) -> &'static str {
    preferred
        .into_iter()
        .find_map(supported)
        .unwrap_or(FALLBACK)
}

/// macOS 以外の機械（エージェントの Linux など）の言語: `LC_ALL`・`LC_MESSAGES`・`LANG` のうち、値のある最初の
/// もの（POSIX がメッセージの言語を決める順）。どれにも値が無ければ無し。
pub fn system_from_env(var: impl Fn(&str) -> Option<String>) -> Option<&'static str> {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .find_map(|name| var(name).filter(|v| !v.is_empty()))
        .map(|value| resolve_system([value.as_str()]))
}

// MARK: - The language of the call

thread_local! {
    /// The language the call running on this thread answers in (an index into `LANGUAGES`);
    /// none when nothing was bound.
    static CURRENT: Cell<Option<usize>> = const { Cell::new(None) };
}

/// The language of what no call binds, for the whole process: none set (`usize::MAX`), `UNBOUND`.
static DEFAULT: AtomicUsize = AtomicUsize::new(usize::MAX);

/// What was bound before, put back when the call is over — however it ends.
#[must_use = "the language is bound only while this is held"]
pub struct Bound(Option<usize>);

impl Drop for Bound {
    fn drop(&mut self) {
        CURRENT.with(|c| c.set(self.0));
    }
}

/// Binds `lang` to this thread until what is returned is dropped. A language without a table, or
/// none, leaves what is bound as it is.
pub fn bind(lang: Option<&str>) -> Bound {
    let before = CURRENT.with(Cell::get);
    if let Some(index) = lang.and_then(index_of_supported) {
        CURRENT.with(|c| c.set(Some(index)));
    }
    Bound(before)
}

/// The language of everything no call binds in this process (a program of its own, such as the
/// agent, says its own lines in its machine's language). A language without a table changes nothing.
pub fn set_default(lang: &str) {
    if let Some(index) = index_of_supported(lang) {
        DEFAULT.store(index, Ordering::Relaxed);
    }
}

fn index_of_supported(lang: &str) -> Option<usize> {
    supported(lang).and_then(|code| LANGUAGES.iter().position(|l| *l == code))
}

/// The language the call running on this thread answers in.
pub fn current() -> &'static str {
    LANGUAGES[current_index()]
}

/// The place in `LANGUAGES` of the language the call running on this thread answers in: what a
/// table kept for each language is looked up by.
pub(crate) fn current_index() -> usize {
    CURRENT.with(Cell::get).unwrap_or_else(|| {
        let set = DEFAULT.load(Ordering::Relaxed);
        if set < LANGUAGES.len() {
            set
        } else {
            index_of(UNBOUND)
        }
    })
}

fn index_of(code: &str) -> usize {
    LANGUAGES
        .iter()
        .position(|l| *l == code)
        .expect("a language of the list")
}

// MARK: - The tables

type Table = HashMap<String, String>;

static TABLES: [OnceLock<Table>; LANGUAGES.len()] = [const { OnceLock::new() }; LANGUAGES.len()];

/// The area files of a language, as embedded (`build.rs`): none for a language with no folder.
fn sources(code: &str) -> &'static [(&'static str, &'static str)] {
    SOURCES
        .iter()
        .find(|(c, _)| *c == code)
        .map(|(_, areas)| *areas)
        .unwrap_or(&[])
}

/// The table of `LANGUAGES[index]`, every area of it in one, read the first time a text of that
/// language is made.
fn table(index: usize) -> &'static Table {
    TABLES[index].get_or_init(|| {
        let code = LANGUAGES[index];
        let mut out = Table::new();
        for (area, source) in sources(code) {
            for (key, text) in parse(code, area, source) {
                out.insert(key, text);
            }
        }
        out
    })
}

fn parse(code: &str, area: &str, source: &str) -> Vec<(String, String)> {
    let map: Map<String, Value> = serde_json::from_str(source)
        .unwrap_or_else(|e| panic!("text/{code}/{area}.json is not a JSON object: {e}"));
    map.into_iter()
        .map(|(key, value)| match value {
            Value::String(text) => (key, text),
            other => panic!("text/{code}/{area}.json: {key} is not a string: {other}"),
        })
        .collect()
}

/// Whether some table has `key`: a key kept from before (a history line) may be one no table has
/// any more.
pub fn has(key: &str) -> bool {
    table(index_of(UNBOUND)).contains_key(key)
}

/// The template of `key` in the language of the call, else in `FALLBACK`.
fn template(key: &str) -> Option<&'static String> {
    table(current_index())
        .get(key)
        .or_else(|| table(index_of(FALLBACK)).get(key))
}

/// The text of `key` in the language of the call, with `values` put into its `{name}`s.
///
/// A key no table has, or a name with no value, stops debug builds (the tests) on the spot; a
/// release build answers with the key or leaves the `{name}`, so a slip never takes a call down.
pub fn render(key: &str, values: &[(&str, &dyn fmt::Display)]) -> String {
    let Some(template) = template(key) else {
        debug_assert!(false, "no text for the key {key}");
        return key.to_string();
    };
    fill(template, |name| {
        let found = values.iter().find(|(n, _)| *n == name);
        if found.is_none() {
            debug_assert!(false, "no value for {{{name}}} of the key {key}");
        }
        found.map(|(_, value)| value.to_string())
    })
}

/// A template with each `{name}` put in by `value`; one with none left as written.
fn fill(template: &str, value: impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(template.len());
    for piece in pieces(template) {
        match piece {
            Piece::Text(text) => out.push_str(text),
            Piece::Name(name) => match value(name) {
                Some(said) => out.push_str(&said),
                None => {
                    out.push('{');
                    out.push_str(name);
                    out.push('}');
                }
            },
        }
    }
    out
}

/// Whether `said` holds the text of `key` in the language of the call, whatever was put into its
/// `{name}`s and whatever comes before or after it: how code that is handed a text (a warning
/// among others, one said after a card's path) knows which it is, in any language.
pub fn is_said(key: &str, said: &str) -> bool {
    let Some(template) = template(key) else {
        debug_assert!(false, "no text for the key {key}");
        return false;
    };
    let mut rest = said;
    for piece in pieces(template) {
        if let Piece::Text(text) = piece {
            match rest.find(text) {
                Some(at) => rest = &rest[at + text.len()..],
                None => return false,
            }
        }
    }
    true
}

/// The text of `key` in the language of the call, with named values: `text!(<key>, id = id)`, or
/// `text!(<key>, id)` for a value named as it is put in. The key is a literal, so a test can find
/// every key the code uses and check the tables have it. Exported for the agent, which says the
/// core's texts too.
#[macro_export]
macro_rules! text {
    ($key:literal $(, $name:ident $(= $value:expr)?)* $(,)?) => {
        $crate::text::render(
            $key,
            &[$((
                stringify!($name),
                &$crate::__text_value!($name $(, $value)?) as &dyn ::std::fmt::Display,
            )),*],
        )
    };
}

/// A text kept by its key and values, said when it is read: `keyed!(<key>, id = id)`, as `text!`.
/// A value that is itself `Keyed` (or a `List`) is kept as one, so it is said in the reader's
/// language too.
#[macro_export]
macro_rules! keyed {
    ($key:literal $(, $name:ident $(= $value:expr)?)* $(,)?) => {{
        #[allow(unused_imports)]
        use $crate::text::{ArgOfDisplay as _, ArgOfKept as _};
        $crate::text::Keyed::new(
            $key,
            vec![$((
                stringify!($name),
                (&$crate::text::Wrap(&$crate::__text_value!($name $(, $value)?))).arg(),
            )),*],
        )
    }};
}

#[doc(hidden)]
#[macro_export]
macro_rules! __text_value {
    ($name:ident) => {
        $name
    };
    ($name:ident, $value:expr) => {
        $value
    };
}

// MARK: - Texts over the Japanese ones

/// A text of another language over a Japanese one that is data rather than a table's (the built-in
/// kinds' words, the MCP tools' descriptions): the Japanese it was made from, and itself. It is said
/// only while the Japanese is still what it was made from, so a Japanese text changed since is said
/// as it is now rather than by words that no longer say it (and the tests name it).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Over {
    pub from: String,
    pub text: String,
}

/// Texts over Japanese ones by their places: an object of `{"<place>": {"from": …, "text": …}}`.
pub fn overs(source: &Value, name: &str) -> HashMap<String, Over> {
    let Some(places) = source.as_object() else {
        panic!("{name} is not a JSON object");
    };
    places
        .iter()
        .map(|(place, entry)| {
            let said = |key: &str| {
                entry[key]
                    .as_str()
                    .unwrap_or_else(|| panic!("{name}: {place} has no {key}"))
                    .to_string()
            };
            (
                place.clone(),
                Over {
                    from: said("from"),
                    text: said("text"),
                },
            )
        })
        .collect()
}

/// The text over `japanese` at `place`, when there is one made from it.
pub fn over<'a>(overs: &'a HashMap<String, Over>, place: &str, japanese: &str) -> Option<&'a str> {
    overs
        .get(place)
        .filter(|o| o.from == japanese)
        .map(|o| o.text.as_str())
}

/// Whether `text` has a letter of Japanese: kana, kanji, the full-width forms, the brackets and
/// stops Japanese is written with.
pub fn has_japanese(text: &str) -> bool {
    text.chars().any(|c| {
        matches!(c, '\u{3000}'..='\u{30ff}' | '\u{4e00}'..='\u{9fff}' | '\u{ff00}'..='\u{ffef}')
    })
}

// MARK: - Kept texts

/// A text by its key and the values put in, said in the language of whoever reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Keyed {
    key: String,
    values: Vec<(String, Arg)>,
}

/// A value put into a kept text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Arg {
    /// Said as it is in every language: a name, a path, a number.
    Plain(String),
    /// Items joined as the language joins a list, each said as it is or in the language.
    List(Vec<Arg>),
    /// A text of its own, said in the same language.
    Keyed(Keyed),
}

impl Keyed {
    #[doc(hidden)]
    pub fn new(key: &'static str, values: Vec<(&'static str, Arg)>) -> Keyed {
        Keyed {
            key: key.to_string(),
            values: values
                .into_iter()
                .map(|(n, v)| (n.to_string(), v))
                .collect(),
        }
    }

    /// Its key in the text tables.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// The value put in as `name`, if one is.
    pub fn value(&self, name: &str) -> Option<&Arg> {
        self.values.iter().find(|(n, _)| n == name).map(|(_, v)| v)
    }

    /// Said in the language of the call. Kept from before, a key no table has now, or a name no
    /// value is kept for, is said as best it can be rather than stop.
    pub fn render(&self) -> String {
        let Some(template) = template(&self.key) else {
            return self.key.clone();
        };
        fill(template, |name| self.value(name).map(Arg::render))
    }

    /// Said in `lang`, whatever the call is in.
    pub fn render_in(&self, lang: &str) -> String {
        let _lang = bind(Some(lang));
        self.render()
    }

    /// As JSON, to keep and to digest: `{"key": …, "values": {<name>: <value>}}` — a plain value a
    /// string, a list an array of strings, a kept text an object of the same form.
    pub fn json(&self) -> Value {
        let mut out = Map::new();
        out.insert("key".into(), self.key.clone().into());
        if !self.values.is_empty() {
            out.insert("values".into(), self.values_json());
        }
        Value::Object(out)
    }

    /// Its values alone as JSON (`json`'s `values`).
    pub fn values_json(&self) -> Value {
        Value::Object(
            self.values
                .iter()
                .map(|(name, value)| (name.clone(), value.json()))
                .collect(),
        )
    }

    /// Read back from `json`'s form; none when it is not that.
    pub fn from_json(value: &Value) -> Option<Keyed> {
        let key = value.get("key")?.as_str()?;
        Keyed::from_parts(key, value.get("values"))
    }

    /// Read back from a key and `values_json`'s form (none, or an object).
    pub fn from_parts(key: &str, values: Option<&Value>) -> Option<Keyed> {
        let values = match values {
            None | Some(Value::Null) => Vec::new(),
            Some(Value::Object(o)) => o
                .iter()
                .map(|(name, value)| Some((name.clone(), Arg::from_json(value)?)))
                .collect::<Option<Vec<_>>>()?,
            Some(_) => return None,
        };
        Some(Keyed {
            key: key.to_string(),
            values,
        })
    }
}

impl Arg {
    fn render(&self) -> String {
        match self {
            Arg::Plain(text) => text.clone(),
            Arg::List(items) => list(&items.iter().map(Arg::render).collect::<Vec<_>>()),
            Arg::Keyed(keyed) => keyed.render(),
        }
    }

    fn json(&self) -> Value {
        match self {
            Arg::Plain(text) => text.clone().into(),
            Arg::List(items) => Value::Array(items.iter().map(Arg::json).collect()),
            Arg::Keyed(keyed) => keyed.json(),
        }
    }

    fn from_json(value: &Value) -> Option<Arg> {
        match value {
            Value::String(text) => Some(Arg::Plain(text.clone())),
            Value::Array(items) => items
                .iter()
                .map(Arg::from_json)
                .collect::<Option<Vec<_>>>()
                .map(Arg::List),
            Value::Object(_) => Keyed::from_json(value).map(Arg::Keyed),
            _ => None,
        }
    }
}

impl fmt::Display for Keyed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.render())
    }
}

/// Said in the language of the call: a refusal kept as a `Keyed` goes out through `?` as text.
impl From<Keyed> for String {
    fn from(keyed: Keyed) -> String {
        keyed.render()
    }
}

/// Items said as a list in the language of the call (`・` in Japanese, `, ` in English): put into
/// `text!` as it is, kept as a list by `keyed!`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct List(pub Vec<Arg>);

impl List {
    /// Items said as they are (names, paths).
    pub fn of<T: fmt::Display>(items: impl IntoIterator<Item = T>) -> List {
        List(
            items
                .into_iter()
                .map(|i| Arg::Plain(i.to_string()))
                .collect(),
        )
    }

    /// Items that are texts of their own, each said in the language too.
    pub fn of_keyed(items: impl IntoIterator<Item = Keyed>) -> List {
        List(items.into_iter().map(Arg::Keyed).collect())
    }
}

impl fmt::Display for List {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let items: Vec<String> = self.0.iter().map(Arg::render).collect();
        f.write_str(&list(&items))
    }
}

/// Items joined as the language of the call joins a list (`・` in Japanese).
pub fn list<T: AsRef<str>>(items: &[T]) -> String {
    joined(items, "common.list.separator")
}

/// Items joined as the language of the call joins a run of them in a sentence (`、` in Japanese).
pub fn list_comma<T: AsRef<str>>(items: &[T]) -> String {
    joined(items, "common.list.comma")
}

fn joined<T: AsRef<str>>(items: &[T], key: &str) -> String {
    let separator = render(key, &[]);
    items
        .iter()
        .map(AsRef::as_ref)
        .collect::<Vec<_>>()
        .join(&separator)
}

// How `keyed!` keeps each value: a `Keyed` or a `List` as one (found first, on the value's own
// type), anything else said as text (found through one more reference).

#[doc(hidden)]
pub struct Wrap<'a, T: ?Sized>(pub &'a T);

#[doc(hidden)]
pub trait ArgOfKept {
    fn arg(&self) -> Arg;
}

impl ArgOfKept for Wrap<'_, Keyed> {
    fn arg(&self) -> Arg {
        Arg::Keyed(self.0.clone())
    }
}

impl ArgOfKept for Wrap<'_, &Keyed> {
    fn arg(&self) -> Arg {
        Arg::Keyed((*self.0).clone())
    }
}

impl ArgOfKept for Wrap<'_, List> {
    fn arg(&self) -> Arg {
        Arg::List(self.0.0.clone())
    }
}

impl ArgOfKept for Wrap<'_, &List> {
    fn arg(&self) -> Arg {
        Arg::List(self.0.0.clone())
    }
}

#[doc(hidden)]
pub trait ArgOfDisplay {
    fn arg(&self) -> Arg;
}

impl<T: fmt::Display + ?Sized> ArgOfDisplay for &Wrap<'_, T> {
    fn arg(&self) -> Arg {
        Arg::Plain(self.0.to_string())
    }
}

// MARK: - Templates

enum Piece<'a> {
    Text(&'a str),
    Name(&'a str),
}

/// A template cut into text and `{name}`s.
fn pieces(template: &str) -> Vec<Piece<'_>> {
    let mut out = Vec::new();
    let bytes = template.as_bytes();
    let (mut start, mut i) = (0, 0);
    while i < bytes.len() {
        if bytes[i] == b'{' {
            if bytes.get(i + 1) == Some(&b'{') {
                // `{{…}}` is the embed syntax quoted, never a name.
                i += 2;
                continue;
            }
            let name_end = bytes[i + 1..]
                .iter()
                .position(|b| !(b.is_ascii_alphanumeric() || *b == b'_'))
                .map(|n| i + 1 + n);
            if let Some(end) = name_end
                && end > i + 1
                && bytes[end] == b'}'
            {
                if start < i {
                    out.push(Piece::Text(&template[start..i]));
                }
                out.push(Piece::Name(&template[i + 1..end]));
                i = end + 1;
                start = i;
                continue;
            }
        }
        i += 1;
    }
    if start < bytes.len() {
        out.push(Piece::Text(&template[start..]));
    }
    out
}

#[cfg(test)]
mod tests;
