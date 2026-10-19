//! The projects the core's own tests start from (`tests/fixtures/projects/`): project files of the core's
//! form, each read into a store of its own.

use crate::Core;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// A project file of the fixtures, by its name (`sample`, `large`, `datamodel`, `small`).
pub(crate) fn project_file(name: &str) -> String {
    std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/projects")
            .join(format!("{name}.json")),
    )
    .unwrap()
}

/// The id of a fixture's project.
pub(crate) fn project_id(name: &str) -> String {
    let file: Value = serde_json::from_str(&project_file(name)).unwrap();
    file["project"]["id"].as_str().unwrap().to_string()
}

/// The folder this run of the tests keeps its places in (`sb-tests-<process>` in the temporary
/// folder), taken away with all in it — the stores, their `-wal`・`-shm` and the copies kept beside
/// them — when the run ends, whether the tests passed or not: the harness ends the process through
/// `exit`, after the last test, and `exit` calls what `atexit` was given. One of the same name is
/// left only by a run of a process numbered the same that was killed, and is taken away first. (The
/// integration tests keep theirs as these do: `tests/common`.)
fn root() -> &'static Path {
    static ROOT: OnceLock<PathBuf> = OnceLock::new();
    unsafe extern "C" {
        fn atexit(callback: extern "C" fn()) -> std::ffi::c_int;
    }
    extern "C" fn sweep() {
        if let Some(root) = ROOT.get() {
            let _ = std::fs::remove_dir_all(root);
        }
    }
    ROOT.get_or_init(|| {
        let root = std::env::temp_dir().join(format!("sb-tests-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        // SAFETY: `sweep` is a plain function that touches only what is set before it is given.
        unsafe { atexit(sweep) };
        root
    })
}

/// A place of its own for a store, not there yet, in the run's folder (`root`): tests run side by
/// side.
pub(crate) fn place(tag: &str) -> PathBuf {
    static COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = COUNT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    root().join(format!("sb-{tag}-{n}.sqlite"))
}

/// A folder of its own, made, in the run's folder.
pub(crate) fn folder(tag: &str) -> PathBuf {
    let made = place(tag).with_extension("");
    std::fs::create_dir_all(&made).unwrap();
    made
}

/// A store of its own with the fixtures `names` read into it, as the window reads a project file.
pub(crate) fn store_with(names: &[&str]) -> PathBuf {
    let to = place(&names.join("-"));
    let core = Core::create(&to).unwrap();
    for name in names {
        let reply: Value = serde_json::from_str(
            &core.call(&json!({"op": "project_file_read", "text": project_file(name)}).to_string()),
        )
        .unwrap();
        assert!(reply.get("error").is_none(), "{name}: {reply}");
    }
    core.close();
    to
}

/// A store of its own with the sample project in it.
pub(crate) fn sample() -> PathBuf {
    store_with(&["sample"])
}

/// A store with the sample project in it, made once in a run for the tests that only read it.
pub(crate) fn read_only_sample() -> &'static PathBuf {
    static SAMPLE: OnceLock<PathBuf> = OnceLock::new();
    SAMPLE.get_or_init(sample)
}

/// The answer a test keeps to compare with (`tests/fixtures/kept/<name>`), as the integration
/// tests keep theirs (`tests/common`'s `kept`): with `SPECBOARD_KEEP=1` what the core says now is
/// kept instead.
pub(crate) fn kept(name: &str, got: &str) {
    let place = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/kept")
        .join(name);
    if std::env::var("SPECBOARD_KEEP").as_deref() == Ok("1") {
        std::fs::create_dir_all(place.parent().unwrap()).unwrap();
        std::fs::write(&place, got).unwrap();
        return;
    }
    let want = std::fs::read_to_string(&place)
        .unwrap_or_else(|_| panic!("{name} の見本がありません（SPECBOARD_KEEP=1 で取る）"));
    assert!(
        want == got,
        "{name} が見本と違う（変えたつもりなら SPECBOARD_KEEP=1 で取り直す）\n今:\n{got}"
    );
}
