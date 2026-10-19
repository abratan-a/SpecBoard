//! The languages a working directory's files are read in: one table, every grammar in it built in
//! (and, on the agent, those a machine adds of its own: `loader`). A file's language comes from its
//! name — the working directory's own `parsers` first, then the table's — so it is known before the
//! file is read.

use std::collections::BTreeMap;
use std::sync::{OnceLock, RwLock};
use tree_sitter::Query;
use tree_sitter_highlight::HighlightConfiguration;

/// Where a language's grammar comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    Bundled,
    /// Added on this machine, from the folder named (`loader`): the agent's alone.
    #[cfg_attr(not(feature = "dynamic-grammars"), allow(dead_code))]
    Loaded(String),
}

impl Source {
    pub fn word(&self) -> &'static str {
        match self {
            Source::Bundled => "bundled",
            Source::Loaded(_) => "loaded",
        }
    }
}

/// How a language's definitions are framed: the siblings before one that belong to it (its doc
/// comments, Rust's attributes), and the nodes around one to take in (`export`, decorators,
/// `template<…>`).
#[derive(Clone, Debug, Default)]
pub struct Rules {
    pub leading: Vec<String>,
    pub wrappers: Vec<String>,
}

/// One language.
pub struct Language {
    pub id: String,
    pub name: String,
    pub extensions: Vec<String>,
    pub file_names: Vec<String>,
    pub source: Source,
    pub grammar: tree_sitter::Language,
    pub rules: Rules,
    /// Only read inside another (Markdown's inline text): no file is of it.
    pub inner: bool,
    symbols_text: Option<String>,
    highlights_text: Option<String>,
    injections_text: String,
    symbols: OnceLock<Result<Query, String>>,
    highlight: OnceLock<Result<HighlightConfiguration, String>>,
}

impl std::fmt::Debug for Language {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Language").field("id", &self.id).finish()
    }
}

impl Language {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: &str,
        name: &str,
        extensions: &[&str],
        file_names: &[&str],
        source: Source,
        grammar: tree_sitter::Language,
        rules: Rules,
        symbols: Option<String>,
        highlights: Option<String>,
        injections: String,
    ) -> Language {
        Language {
            id: id.to_string(),
            name: name.to_string(),
            extensions: extensions.iter().map(|e| e.to_string()).collect(),
            file_names: file_names.iter().map(|e| e.to_string()).collect(),
            source,
            grammar,
            rules,
            inner: false,
            symbols_text: symbols,
            highlights_text: highlights,
            injections_text: injections,
            symbols: OnceLock::new(),
            highlight: OnceLock::new(),
        }
    }

    /// Whether its functions and types are read.
    pub fn has_symbols(&self) -> bool {
        self.symbols_text.is_some()
    }

    pub fn has_highlight(&self) -> bool {
        self.highlights_text.is_some()
    }

    /// The query its definitions are found by, made the first time it is asked for: a query takes
    /// tens of milliseconds to make, and most calls read no code.
    pub fn symbols_query(&self) -> Option<Result<&Query, String>> {
        let text = self.symbols_text.as_ref()?;
        Some(
            self.symbols
                .get_or_init(|| Query::new(&self.grammar, text).map_err(|e| query_error(&e)))
                .as_ref()
                .map_err(Clone::clone),
        )
    }

    /// How its text is coloured, made the first time it is asked for.
    pub fn highlight_config(&self) -> Option<Result<&HighlightConfiguration, String>> {
        let text = self.highlights_text.as_ref()?;
        Some(
            self.highlight
                .get_or_init(|| {
                    let mut config = HighlightConfiguration::new(
                        self.grammar.clone(),
                        self.id.clone(),
                        text,
                        &self.injections_text,
                        "",
                    )
                    .map_err(|e| query_error(&e))?;
                    config.configure(&super::highlight::RECOGNIZED);
                    Ok(config)
                })
                .as_ref()
                .map_err(Clone::clone),
        )
    }

    /// Both queries made now, saying what is wrong with either (a grammar a machine adds).
    #[cfg_attr(not(any(test, feature = "dynamic-grammars")), allow(dead_code))]
    pub fn check_queries(&self) -> Result<(), String> {
        if let Some(Err(e)) = self.symbols_query() {
            return Err(format!("symbols.scm: {e}"));
        }
        if let Some(Err(e)) = self.highlight_config() {
            return Err(format!("highlights.scm: {e}"));
        }
        Ok(())
    }
}

/// A query's error with its row and column, from 1.
fn query_error(e: &tree_sitter::QueryError) -> String {
    format!(
        "{} {}:{} {}",
        kind_word(&e.kind),
        e.row + 1,
        e.column + 1,
        e.message
    )
}

fn kind_word(kind: &tree_sitter::QueryErrorKind) -> &'static str {
    use tree_sitter::QueryErrorKind as K;
    match kind {
        K::Syntax => "syntax",
        K::NodeType => "node type",
        K::Field => "field",
        K::Capture => "capture",
        K::Predicate => "predicate",
        K::Structure => "structure",
        K::Language => "language",
    }
}

fn rules(leading: &[&str], wrappers: &[&str]) -> Rules {
    Rules {
        leading: leading.iter().map(|s| s.to_string()).collect(),
        wrappers: wrappers.iter().map(|s| s.to_string()).collect(),
    }
}

macro_rules! ours {
    ($lang:literal) => {
        Some(include_str!(concat!("queries/", $lang, "/symbols.scm")).to_string())
    };
}

/// Highlight queries put together, earlier ones first (they win). A node captured twice in one
/// pattern is coloured by neither (`@comment @spell`): the spell checker's captures are taken out.
fn joined(parts: &[&str]) -> Option<String> {
    Some(
        parts
            .join("\n")
            .replace(" @nospell", "")
            .replace(" @spell", ""),
    )
}

/// The languages built in, in the table's order.
fn bundled() -> Vec<Language> {
    let c_comments = ["comment"];
    let mut out = vec![
        Language::new(
            "rust",
            "Rust",
            &[".rs"],
            &[],
            Source::Bundled,
            tree_sitter_rust::LANGUAGE.into(),
            rules(&["line_comment", "block_comment", "attribute_item"], &[]),
            ours!("rust"),
            joined(&[tree_sitter_rust::HIGHLIGHTS_QUERY]),
            tree_sitter_rust::INJECTIONS_QUERY.to_string(),
        ),
        Language::new(
            "swift",
            "Swift",
            &[".swift"],
            &[],
            Source::Bundled,
            tree_sitter_swift::LANGUAGE.into(),
            rules(&["comment", "multiline_comment"], &[]),
            ours!("swift"),
            joined(&[tree_sitter_swift::HIGHLIGHTS_QUERY]),
            tree_sitter_swift::INJECTIONS_QUERY.to_string(),
        ),
        Language::new(
            "typescript",
            "TypeScript",
            &[".ts"],
            &[],
            Source::Bundled,
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            rules(&c_comments, &["export_statement", "expression_statement"]),
            ours!("typescript"),
            joined(&[
                tree_sitter_typescript::HIGHLIGHTS_QUERY,
                tree_sitter_javascript::HIGHLIGHT_QUERY,
            ]),
            tree_sitter_javascript::INJECTIONS_QUERY.to_string(),
        ),
        Language::new(
            "tsx",
            "TSX",
            &[".tsx"],
            &[],
            Source::Bundled,
            tree_sitter_typescript::LANGUAGE_TSX.into(),
            rules(&c_comments, &["export_statement", "expression_statement"]),
            ours!("tsx"),
            joined(&[
                tree_sitter_typescript::HIGHLIGHTS_QUERY,
                tree_sitter_javascript::JSX_HIGHLIGHT_QUERY,
                tree_sitter_javascript::HIGHLIGHT_QUERY,
            ]),
            tree_sitter_javascript::INJECTIONS_QUERY.to_string(),
        ),
        Language::new(
            "javascript",
            "JavaScript",
            &[".js", ".mjs", ".cjs", ".jsx"],
            &[],
            Source::Bundled,
            tree_sitter_javascript::LANGUAGE.into(),
            rules(&c_comments, &["export_statement"]),
            ours!("javascript"),
            joined(&[
                tree_sitter_javascript::JSX_HIGHLIGHT_QUERY,
                tree_sitter_javascript::HIGHLIGHT_QUERY,
            ]),
            tree_sitter_javascript::INJECTIONS_QUERY.to_string(),
        ),
        Language::new(
            "python",
            "Python",
            &[".py"],
            &[],
            Source::Bundled,
            tree_sitter_python::LANGUAGE.into(),
            rules(&c_comments, &["decorated_definition"]),
            ours!("python"),
            joined(&[tree_sitter_python::HIGHLIGHTS_QUERY]),
            String::new(),
        ),
        Language::new(
            "go",
            "Go",
            &[".go"],
            &[],
            Source::Bundled,
            tree_sitter_go::LANGUAGE.into(),
            rules(&c_comments, &["type_declaration", "const_declaration"]),
            ours!("go"),
            joined(&[tree_sitter_go::HIGHLIGHTS_QUERY]),
            String::new(),
        ),
        Language::new(
            "c",
            "C",
            &[".c", ".h"],
            &[],
            Source::Bundled,
            tree_sitter_c::LANGUAGE.into(),
            rules(&c_comments, &[]),
            ours!("c"),
            joined(&[tree_sitter_c::HIGHLIGHT_QUERY]),
            String::new(),
        ),
        Language::new(
            "cpp",
            "C++",
            &[".cc", ".cpp", ".hpp", ".hh"],
            &[],
            Source::Bundled,
            tree_sitter_cpp::LANGUAGE.into(),
            rules(&c_comments, &["template_declaration"]),
            ours!("cpp"),
            joined(&[
                tree_sitter_cpp::HIGHLIGHT_QUERY,
                tree_sitter_c::HIGHLIGHT_QUERY,
            ]),
            String::new(),
        ),
        Language::new(
            "java",
            "Java",
            &[".java"],
            &[],
            Source::Bundled,
            tree_sitter_java::LANGUAGE.into(),
            rules(&["line_comment", "block_comment"], &[]),
            ours!("java"),
            joined(&[tree_sitter_java::HIGHLIGHTS_QUERY]),
            String::new(),
        ),
        Language::new(
            "csharp",
            "C#",
            &[".cs"],
            &[],
            Source::Bundled,
            tree_sitter_c_sharp::LANGUAGE.into(),
            rules(&c_comments, &[]),
            ours!("csharp"),
            joined(&[tree_sitter_c_sharp::HIGHLIGHTS_QUERY]),
            String::new(),
        ),
        Language::new(
            "ruby",
            "Ruby",
            &[".rb"],
            &[],
            Source::Bundled,
            tree_sitter_ruby::LANGUAGE.into(),
            rules(&c_comments, &[]),
            ours!("ruby"),
            joined(&[tree_sitter_ruby::HIGHLIGHTS_QUERY]),
            String::new(),
        ),
        Language::new(
            "php",
            "PHP",
            &[".php"],
            &[],
            Source::Bundled,
            tree_sitter_php::LANGUAGE_PHP.into(),
            rules(&c_comments, &[]),
            ours!("php"),
            joined(&[tree_sitter_php::HIGHLIGHTS_QUERY]),
            String::new(),
        ),
        Language::new(
            "kotlin",
            "Kotlin",
            &[".kt", ".kts"],
            &[],
            Source::Bundled,
            tree_sitter_kotlin_ng::LANGUAGE.into(),
            rules(&["line_comment", "block_comment"], &[]),
            ours!("kotlin"),
            Some(include_str!("queries/kotlin/highlights.scm").to_string()),
            String::new(),
        ),
        Language::new(
            "sql",
            "SQL",
            &[".sql"],
            &[],
            Source::Bundled,
            tree_sitter_sequel::LANGUAGE.into(),
            rules(&["comment", "marginalia"], &["statement"]),
            ours!("sql"),
            joined(&[tree_sitter_sequel::HIGHLIGHTS_QUERY]),
            String::new(),
        ),
        Language::new(
            "bash",
            "Bash",
            &[".sh", ".bash"],
            &[],
            Source::Bundled,
            tree_sitter_bash::LANGUAGE.into(),
            rules(&c_comments, &[]),
            ours!("bash"),
            joined(&[tree_sitter_bash::HIGHLIGHT_QUERY]),
            String::new(),
        ),
        Language::new(
            "json",
            "JSON",
            &[".json"],
            &[],
            Source::Bundled,
            tree_sitter_json::LANGUAGE.into(),
            Rules::default(),
            None,
            joined(&[tree_sitter_json::HIGHLIGHTS_QUERY]),
            String::new(),
        ),
        Language::new(
            "yaml",
            "YAML",
            &[".yml", ".yaml"],
            &[],
            Source::Bundled,
            tree_sitter_yaml::LANGUAGE.into(),
            Rules::default(),
            None,
            joined(&[tree_sitter_yaml::HIGHLIGHTS_QUERY]),
            String::new(),
        ),
        Language::new(
            "toml",
            "TOML",
            &[".toml"],
            &[],
            Source::Bundled,
            tree_sitter_toml_ng::LANGUAGE.into(),
            Rules::default(),
            None,
            joined(&[tree_sitter_toml_ng::HIGHLIGHTS_QUERY]),
            String::new(),
        ),
        Language::new(
            "html",
            "HTML",
            &[".html"],
            &[],
            Source::Bundled,
            tree_sitter_html::LANGUAGE.into(),
            Rules::default(),
            None,
            joined(&[tree_sitter_html::HIGHLIGHTS_QUERY]),
            tree_sitter_html::INJECTIONS_QUERY.to_string(),
        ),
        Language::new(
            "css",
            "CSS",
            &[".css"],
            &[],
            Source::Bundled,
            tree_sitter_css::LANGUAGE.into(),
            Rules::default(),
            None,
            joined(&[tree_sitter_css::HIGHLIGHTS_QUERY]),
            String::new(),
        ),
        Language::new(
            "markdown",
            "Markdown",
            &[".md"],
            &[],
            Source::Bundled,
            tree_sitter_md::LANGUAGE.into(),
            Rules::default(),
            None,
            joined(&[tree_sitter_md::HIGHLIGHT_QUERY_BLOCK]),
            tree_sitter_md::INJECTION_QUERY_BLOCK.to_string(),
        ),
    ];
    let mut inline = Language::new(
        "markdown_inline",
        "Markdown (inline)",
        &[],
        &[],
        Source::Bundled,
        tree_sitter_md::INLINE_LANGUAGE.into(),
        Rules::default(),
        None,
        joined(&[tree_sitter_md::HIGHLIGHT_QUERY_INLINE]),
        tree_sitter_md::INJECTION_QUERY_INLINE.to_string(),
    );
    inline.inner = true;
    out.push(inline);
    out
}

/// The languages: those built in, then those added on this machine.
pub struct Registry {
    bundled: Vec<&'static Language>,
    loaded: RwLock<Vec<&'static Language>>,
}

pub fn registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(|| Registry {
        bundled: bundled()
            .into_iter()
            .map(|l| &*Box::leak(Box::new(l)))
            .collect(),
        loaded: RwLock::new(Vec::new()),
    })
}

impl Registry {
    /// Every language a file can be of, built in first.
    pub fn all(&self) -> Vec<&'static Language> {
        let mut out: Vec<&'static Language> = self.bundled.clone();
        out.extend(self.loaded.read().unwrap().iter().copied());
        out.retain(|l| !l.inner);
        out
    }

    pub fn find(&self, id: &str) -> Option<&'static Language> {
        self.bundled
            .iter()
            .find(|l| l.id == id)
            .copied()
            .or_else(|| {
                self.loaded
                    .read()
                    .unwrap()
                    .iter()
                    .find(|l| l.id == id)
                    .copied()
            })
    }

    /// The language an injection names (`"js"`, `"html"`, a code fence's word), by its id, its name
    /// or one of its extensions.
    pub fn named(&self, said: &str) -> Option<&'static Language> {
        let said = said.trim().to_ascii_lowercase();
        let alias = match said.as_str() {
            "js" => "javascript",
            "ts" => "typescript",
            "py" => "python",
            "sh" | "shell" | "zsh" => "bash",
            "c++" => "cpp",
            "cs" | "c#" => "csharp",
            "rb" => "ruby",
            "kt" => "kotlin",
            "yml" => "yaml",
            "md" => "markdown",
            other => other,
        };
        let dotted = format!(".{alias}");
        self.find(alias).or_else(|| {
            self.bundled
                .iter()
                .chain(self.loaded.read().unwrap().iter())
                .find(|l| l.name.eq_ignore_ascii_case(alias) || l.extensions.contains(&dotted))
                .copied()
        })
    }

    #[cfg_attr(not(feature = "dynamic-grammars"), allow(dead_code))]
    pub fn is_bundled(&self, id: &str) -> bool {
        self.bundled.iter().any(|l| l.id == id)
    }

    /// The languages added on this machine set to those read now. Each is the process's for good
    /// (`Box::leak`): its grammar's library is never let go, and what was made of it stays good.
    #[cfg_attr(not(feature = "dynamic-grammars"), allow(dead_code))]
    pub fn set_loaded(&self, now: Vec<&'static Language>) {
        *self.loaded.write().unwrap() = now;
    }

    #[cfg_attr(not(feature = "dynamic-grammars"), allow(dead_code))]
    pub fn loaded(&self) -> Vec<&'static Language> {
        self.loaded.read().unwrap().clone()
    }
}

/// How a working directory maps its files to languages besides the table's (`parsers`): by an
/// extension (`.h`) or a file's name (`Makefile`), to a language's id or to none.
pub type Parsers = BTreeMap<String, Option<String>>;

/// Whether a key of `parsers` is of a form it takes: an extension (`.h`, `.d.ts`) or a file's name
/// (`Makefile`), with no `/`.
pub fn parsers_key_ok(key: &str) -> bool {
    !key.is_empty()
        && !key.contains('/')
        && !key.chars().any(char::is_control)
        && key != "."
        && key.trim() == key
}

/// The language of a file by its name (the last part of its path): the working directory's own
/// mapping first — its name, then its endings from the longest — then the table's. `None` when
/// none is, or when the mapping says none; `Err` names a language the mapping gives that this
/// machine does not have.
pub fn language_of(name: &str, parsers: &Parsers) -> Result<Option<&'static Language>, String> {
    let name = name.rsplit('/').next().unwrap_or(name);
    let endings: Vec<String> = name
        .char_indices()
        .filter(|(i, c)| *c == '.' && *i > 0)
        .map(|(i, _)| name[i..].to_ascii_lowercase())
        .collect();
    let given = parsers.get(name).or_else(|| {
        endings.iter().find_map(|e| {
            parsers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(e))
                .map(|(_, v)| v)
        })
    });
    let registry = registry();
    if let Some(given) = given {
        return match given {
            None => Ok(None),
            Some(id) => registry
                .find(id)
                .filter(|l| !l.inner)
                .map(Some)
                .ok_or(id.clone()),
        };
    }
    let all = registry.all();
    if let Some(found) = all.iter().find(|l| l.file_names.iter().any(|f| f == name)) {
        return Ok(Some(*found));
    }
    for ending in &endings {
        if let Some(found) = all.iter().find(|l| l.extensions.contains(ending)) {
            return Ok(Some(*found));
        }
    }
    Ok(None)
}

/// The table's mapping as a working directory sees it: each extension and file's name with its
/// language, those `parsers` sets marked so, and what `parsers` maps to none.
pub fn effective(parsers: &Parsers) -> Vec<(String, Option<String>, bool)> {
    let mut out: BTreeMap<String, (Option<String>, bool)> = BTreeMap::new();
    for l in registry().all() {
        for key in l.extensions.iter().chain(l.file_names.iter()) {
            out.entry(key.clone())
                .or_insert((Some(l.id.clone()), false));
        }
    }
    for (key, id) in parsers {
        out.insert(key.clone(), (id.clone(), true));
    }
    out.into_iter().map(|(k, (v, o))| (k, v, o)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_s_language_is_its_working_directory_s_mapping_then_the_table_s() {
        let none = Parsers::new();
        let id = |name: &str, p: &Parsers| language_of(name, p).map(|l| l.map(|l| l.id.clone()));
        assert_eq!(id("src/a.rs", &none), Ok(Some("rust".into())));
        assert_eq!(id("A.SWIFT", &none), Ok(Some("swift".into())));
        assert_eq!(id("x.h", &none), Ok(Some("c".into())));
        assert_eq!(id("Makefile", &none), Ok(None));
        assert_eq!(id(".gitignore", &none), Ok(None));
        let mut set = Parsers::new();
        set.insert(".h".into(), Some("cpp".into()));
        set.insert(".d.ts".into(), None);
        set.insert("Makefile".into(), Some("bash".into()));
        set.insert(".zig".into(), Some("zig".into()));
        assert_eq!(id("x.h", &set), Ok(Some("cpp".into())));
        assert_eq!(id("index.d.ts", &set), Ok(None));
        assert_eq!(id("index.ts", &set), Ok(Some("typescript".into())));
        assert_eq!(id("Makefile", &set), Ok(Some("bash".into())));
        assert_eq!(id("a.zig", &set), Err("zig".into()));
        assert!(parsers_key_ok(".h") && parsers_key_ok("Makefile"));
        assert!(!parsers_key_ok("src/x") && !parsers_key_ok("") && !parsers_key_ok(" .h"));
    }

    /// Every grammar built in reads with this tree-sitter, and every query built in is made: a
    /// grammar's node names change between its versions, and a query naming one gone fails here
    /// before it fails a reader.
    #[test]
    fn every_language_built_in_reads_and_its_queries_are_made() {
        let mut parser = tree_sitter::Parser::new();
        for l in &registry().bundled {
            parser
                .set_language(&l.grammar)
                .unwrap_or_else(|e| panic!("{}: {e}", l.id));
            let abi = l.grammar.abi_version();
            assert!(
                (tree_sitter::MIN_COMPATIBLE_LANGUAGE_VERSION..=tree_sitter::LANGUAGE_VERSION)
                    .contains(&abi),
                "{}: {abi}",
                l.id
            );
            l.check_queries()
                .unwrap_or_else(|e| panic!("{}: {e}", l.id));
            assert!(l.has_highlight(), "{}", l.id);
        }
        let symbols: Vec<&str> = registry()
            .bundled
            .iter()
            .filter(|l| l.has_symbols())
            .map(|l| l.id.as_str())
            .collect();
        assert_eq!(
            symbols,
            [
                "rust",
                "swift",
                "typescript",
                "tsx",
                "javascript",
                "python",
                "go",
                "c",
                "cpp",
                "java",
                "csharp",
                "ruby",
                "php",
                "kotlin",
                "sql",
                "bash"
            ]
        );
    }
}
