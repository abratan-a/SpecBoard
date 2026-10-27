//! Grammars a machine adds of its own, on the agent alone: each a folder in the agent's `grammars/`
//! (`~/.specboard/grammars/<language>/`) holding `grammar.json`, the grammar's library and its
//! queries (`symbols.scm`, `highlights.scm`; either may be left out). A library runs with the rights
//! of whoever loads it, so only one the agent's user owns and nobody else can write is loaded — the
//! rule ssh holds a key's folder to — and nothing is ever fetched. A library loaded is never let go:
//! what was made of it points into it. A grammar that cannot be loaded leaves out its language alone
//! and says why (`problems`), kept by its key: each reader is told it in its own language.

use super::registry::{Language, Rules, Source, registry};
use crate::text::Keyed;
use serde_json::Value;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// Where the grammars are looked for, once the agent said so (`set_dir`).
fn dir() -> &'static Mutex<Option<PathBuf>> {
    static DIR: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();
    DIR.get_or_init(|| Mutex::new(None))
}

/// Why each grammar not loaded was not: its folder's name, and why.
fn kept_problems() -> &'static Mutex<Vec<(String, Keyed)>> {
    static PROBLEMS: OnceLock<Mutex<Vec<(String, Keyed)>>> = OnceLock::new();
    PROBLEMS.get_or_init(|| Mutex::new(Vec::new()))
}

/// The grammars looked for in `folder` from now on, and loaded now.
pub fn set_dir(folder: PathBuf) {
    *dir().lock().unwrap() = Some(folder);
    reload();
}

/// Why each grammar of the folder was not loaded, as last looked: its folder's name, and why.
pub fn problems() -> Vec<(String, Keyed)> {
    kept_problems().lock().unwrap().clone()
}

/// The grammars of the folder looked at again: those not loaded yet are loaded; one loaded before
/// stays as it was.
pub fn reload() {
    let Some(folder) = dir().lock().unwrap().clone() else {
        return;
    };
    let (loaded, problems) = load_from(&folder, &registry().loaded());
    registry().set_loaded(loaded);
    *kept_problems().lock().unwrap() = problems;
}

/// The grammars in `folder`, those in `before` kept as they are: what is loaded, and why each other
/// one is not.
pub fn load_from(
    folder: &Path,
    before: &[&'static Language],
) -> (Vec<&'static Language>, Vec<(String, Keyed)>) {
    let mut loaded: Vec<&'static Language> = before.to_vec();
    let mut problems = Vec::new();
    let Ok(entries) = std::fs::read_dir(folder) else {
        return (loaded, problems);
    };
    if let Err(why) = owned_alone(folder) {
        problems.push((folder.display().to_string(), why));
        return (loaded, problems);
    }
    let mut folders: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    folders.sort();
    for one in folders {
        let name = one
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if before
            .iter()
            .any(|l| l.source == Source::Loaded(one.display().to_string()))
        {
            continue;
        }
        match load_one(&one) {
            Ok(language) => {
                if loaded.iter().any(|l| l.id == language.id) {
                    problems.push((name, keyed!("code.loader.taken", id = language.id)));
                    continue;
                }
                loaded.push(Box::leak(Box::new(language)));
            }
            Err(why) => problems.push((name, why)),
        }
    }
    (loaded, problems)
}

/// Whether a file or a folder is the agent's user's alone: owned by it, and written by nobody else.
fn owned_alone(path: &Path) -> Result<(), Keyed> {
    let meta = std::fs::symlink_metadata(path)
        .map_err(|_| keyed!("code.loader.missing", path = path.display()))?;
    // SAFETY: `getuid` reads the process's user and cannot fail.
    let me = unsafe { libc::getuid() };
    if meta.uid() != me {
        return Err(keyed!("code.loader.owner", path = path.display()));
    }
    if meta.permissions().mode() & 0o022 != 0 {
        return Err(keyed!("code.loader.writable", path = path.display()));
    }
    if meta.file_type().is_symlink() {
        return Err(keyed!("code.loader.link", path = path.display()));
    }
    Ok(())
}

fn strings(manifest: &Value, key: &str) -> Vec<String> {
    manifest
        .get(key)
        .and_then(Value::as_array)
        .map(|l| {
            l.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// One grammar's folder loaded: its manifest read, its files checked, its library opened and its
/// queries made.
fn load_one(folder: &Path) -> Result<Language, Keyed> {
    owned_alone(folder)?;
    let manifest_path = folder.join("grammar.json");
    owned_alone(&manifest_path)?;
    let manifest: Value = serde_json::from_slice(
        &std::fs::read(&manifest_path)
            .map_err(|_| keyed!("code.loader.missing", path = manifest_path.display()))?,
    )
    .map_err(|e| keyed!("code.loader.manifest", e))?;
    let field = |key: &str| -> Result<String, Keyed> {
        manifest
            .get(key)
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .map(str::to_string)
            .ok_or_else(|| keyed!("code.loader.field", key))
    };
    let id = field("id")?;
    if !id
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
    {
        return Err(keyed!("code.loader.id", id));
    }
    if registry().is_bundled(&id) {
        return Err(keyed!("code.loader.bundled", id));
    }
    let name = manifest
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or(&id)
        .to_string();
    let symbol = field("symbol")?;
    let library = field("library")?;
    if library.contains('/') {
        return Err(keyed!("code.loader.libraryName", library));
    }
    let extensions = strings(&manifest, "extensions");
    let file_names = strings(&manifest, "fileNames");
    let library_path = folder.join(&library);
    owned_alone(&library_path)?;
    let query = |file: &str| -> Result<Option<String>, Keyed> {
        let path = folder.join(file);
        if !path.exists() {
            return Ok(None);
        }
        owned_alone(&path)?;
        std::fs::read_to_string(&path)
            .map(Some)
            .map_err(|_| keyed!("code.loader.missing", path = path.display()))
    };
    let symbols = query("symbols.scm")?;
    let highlights = query("highlights.scm")?;
    let injections = query("injections.scm")?.unwrap_or_default();
    // SAFETY: the library is the agent's user's own and written by nobody else (checked above); it
    // runs as that user, as a program of theirs would.
    let lib = unsafe { libloading::Library::new(&library_path) }
        .map_err(|e| keyed!("code.loader.open", library, e))?;
    // SAFETY: a grammar's library exports its language as `const TSLanguage *tree_sitter_<name>(void)`.
    let made: unsafe extern "C" fn() -> *const () = unsafe {
        *lib.get::<unsafe extern "C" fn() -> *const ()>(symbol.as_bytes())
            .map_err(|_| keyed!("code.loader.symbol", symbol, library))?
    };
    // SAFETY: the function is the grammar's, as asked for by its name.
    let grammar =
        tree_sitter::Language::new(unsafe { tree_sitter_language::LanguageFn::from_raw(made) });
    let abi = grammar.abi_version();
    let takes = tree_sitter::MIN_COMPATIBLE_LANGUAGE_VERSION..=tree_sitter::LANGUAGE_VERSION;
    if !takes.contains(&abi) {
        return Err(keyed!(
            "code.loader.abi",
            abi,
            min = takes.start(),
            max = takes.end()
        ));
    }
    // The language points into the library from now on.
    std::mem::forget(lib);
    let mut language = Language::new(
        &id,
        &name,
        &extensions.iter().map(String::as_str).collect::<Vec<_>>(),
        &file_names.iter().map(String::as_str).collect::<Vec<_>>(),
        Source::Loaded(folder.display().to_string()),
        grammar,
        Rules {
            leading: strings(&manifest, "leading"),
            wrappers: strings(&manifest, "wrappers"),
        },
        symbols,
        highlights,
        injections,
    );
    language
        .check_queries()
        .map_err(|why| keyed!("code.loader.queries", why))?;
    language.inner = false;
    Ok(language)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(tag: &str) -> PathBuf {
        let made = crate::fixtures::folder(tag);
        std::fs::set_permissions(&made, std::fs::Permissions::from_mode(0o755)).unwrap();
        made
    }

    fn grammar(root: &Path, name: &str, manifest: &str) -> PathBuf {
        let one = root.join(name);
        std::fs::create_dir_all(&one).unwrap();
        std::fs::set_permissions(&one, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(one.join("grammar.json"), manifest).unwrap();
        std::fs::set_permissions(
            one.join("grammar.json"),
            std::fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        one
    }

    fn why(root: &Path) -> Vec<(String, String)> {
        let _lang = crate::text::bind(Some("en"));
        load_from(root, &[])
            .1
            .into_iter()
            .map(|(name, why)| (name, why.render()))
            .collect()
    }

    /// A grammar that cannot be loaded leaves out its language alone and says why: a manifest that
    /// is not one, a language built in, a file others can write, a library not there.
    #[test]
    fn a_grammar_that_cannot_be_loaded_says_why_and_is_left_out() {
        let root = folder("grammars");
        grammar(&root, "a-bad", "{");
        grammar(
            &root,
            "b-rust",
            r#"{"id": "rust", "symbol": "tree_sitter_rust", "library": "librust.so"}"#,
        );
        let open = grammar(
            &root,
            "c-open",
            r#"{"id": "zig", "symbol": "tree_sitter_zig", "library": "libzig.so"}"#,
        );
        std::fs::write(open.join("libzig.so"), "").unwrap();
        std::fs::set_permissions(
            open.join("libzig.so"),
            std::fs::Permissions::from_mode(0o666),
        )
        .unwrap();
        grammar(
            &root,
            "d-gone",
            r#"{"id": "odin", "symbol": "tree_sitter_odin", "library": "libodin.so"}"#,
        );
        let fake = grammar(
            &root,
            "e-fake",
            r#"{"id": "fake", "symbol": "tree_sitter_fake", "library": "libfake.so"}"#,
        );
        std::fs::write(fake.join("libfake.so"), "not a library").unwrap();
        std::fs::set_permissions(
            fake.join("libfake.so"),
            std::fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        let said = why(&root);
        let of = |name: &str| {
            said.iter()
                .find(|(n, _)| n == name)
                .map(|(_, w)| w.clone())
                .unwrap_or_else(|| panic!("{name}: {said:?}"))
        };
        assert!(of("a-bad").contains("grammar.json"), "{said:?}");
        assert!(of("b-rust").contains("built in"), "{said:?}");
        assert!(of("c-open").contains("others can write"), "{said:?}");
        assert!(of("d-gone").contains("libodin.so"), "{said:?}");
        assert!(of("e-fake").contains("cannot be opened"), "{said:?}");
        let (loaded, _) = load_from(&root, &[]);
        assert!(loaded.is_empty());
        // The folder itself, written by others: nothing in it is loaded.
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o777)).unwrap();
        let said = why(&root);
        assert_eq!(said.len(), 1, "{said:?}");
        assert!(said[0].1.contains("others can write"), "{said:?}");
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}
