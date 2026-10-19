//! The binary's own list: a file of `tests/` that `integration.rs` does not name as a module is
//! not built, not run and not formatted, and nothing else would say so.

use std::collections::BTreeSet;
use std::path::PathBuf;

fn tests_folder() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests")
}

/// The modules `integration.rs` names (`mod x;`), besides the shared helpers.
fn mounted() -> BTreeSet<String> {
    let text = std::fs::read_to_string(tests_folder().join("integration.rs")).unwrap();
    text.lines()
        .filter_map(|line| line.strip_prefix("mod ")?.strip_suffix(';'))
        .filter(|name| *name != "common")
        .map(str::to_string)
        .collect()
}

/// The files of `tests/` that are test modules: every `.rs` in it but the binary's root.
fn files() -> BTreeSet<String> {
    std::fs::read_dir(tests_folder())
        .unwrap()
        .filter_map(|entry| {
            let name = entry.unwrap().file_name().into_string().unwrap();
            let stem = name.strip_suffix(".rs")?;
            (stem != "integration").then(|| stem.to_string())
        })
        .collect()
}

#[test]
fn every_test_file_is_a_module_of_the_binary_and_every_module_is_a_file() {
    let (mounted, files) = (mounted(), files());
    let loose: Vec<_> = files.difference(&mounted).collect();
    let missing: Vec<_> = mounted.difference(&files).collect();
    assert!(
        loose.is_empty(),
        "not named in tests/integration.rs, so never run (add `mod <name>;`): {loose:?}"
    );
    assert!(
        missing.is_empty(),
        "named in tests/integration.rs but there is no such file in tests/: {missing:?}"
    );
}
