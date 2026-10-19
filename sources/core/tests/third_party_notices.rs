//! The licenses of what the app and its agent are built from are written down for every crate they can link.
//!
//! `tools/notices.py` collects them (from `cargo metadata`, which needs the crates' sources) into
//! `ThirdPartyNotices.json` for the app's page and `AgentThirdPartyNotices.txt` for the agent; what
//! Cargo.lock lists under their trees but no target links (build tools, other platforms' crates) goes into
//! `tools/notices/unlinked.json`. These tests read Cargo.lock, which needs no network, and fail when a crate
//! was added or its version changed without the tool being run again.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde_json::Value;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The "name version" of every registry crate Cargo.lock reaches from `root`, whatever the kind of
/// dependency or the target (a crate of this workspace has no source and is left out).
fn lock_closure(root: &str) -> BTreeSet<String> {
    struct Entry {
        name: String,
        version: String,
        registry: bool,
        deps: Vec<String>,
    }
    let text = std::fs::read_to_string(repo().join("sources/core/Cargo.lock")).unwrap();
    let mut entries: Vec<Entry> = Vec::new();
    for line in text.lines() {
        let quoted = || line.split('"').nth(1).unwrap_or_default().to_string();
        if line == "[[package]]" {
            entries.push(Entry {
                name: String::new(),
                version: String::new(),
                registry: false,
                deps: Vec::new(),
            });
        } else if let Some(entry) = entries.last_mut() {
            if line.starts_with("name = ") {
                entry.name = quoted();
            } else if line.starts_with("version = ") {
                entry.version = quoted();
            } else if line.starts_with("source = ") {
                entry.registry = true;
            } else if line.starts_with(" \"") {
                entry.deps.push(quoted());
            }
        }
    }
    let mut seen: BTreeSet<usize> = BTreeSet::new();
    let mut stack: Vec<usize> = (0..entries.len())
        .filter(|&i| entries[i].name == root)
        .collect();
    assert!(!stack.is_empty(), "{root} is not in Cargo.lock");
    while let Some(i) = stack.pop() {
        if !seen.insert(i) {
            continue;
        }
        for dep in &entries[i].deps {
            let mut parts = dep.split(' ');
            let name = parts.next().unwrap();
            let version = parts.next();
            stack.extend((0..entries.len()).filter(|&j| {
                entries[j].name == name && version.is_none_or(|v| entries[j].version == v)
            }));
        }
    }
    seen.into_iter()
        .filter(|&i| entries[i].registry)
        .map(|i| format!("{} {}", entries[i].name, entries[i].version))
        .collect()
}

fn json(path: &str) -> Value {
    let text = std::fs::read_to_string(repo().join(path)).unwrap();
    serde_json::from_str(&text).unwrap()
}

fn strings(value: &Value) -> BTreeSet<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect()
}

const FIX: &str = "run `python3 tools/notices.py` in the repository (the crates must be downloaded: `cargo fetch` in sources/core) and commit what it writes";

/// What `lock` names and `written` and `unlinked` do not: crates to be written down; and what they name
/// and `lock` does not: entries of a version no longer used.
fn compare(
    what: &str,
    lock: &BTreeSet<String>,
    written: &BTreeSet<String>,
    unlinked: &BTreeSet<String>,
) {
    let known: BTreeSet<&String> = written.iter().chain(unlinked.iter()).collect();
    let missing: Vec<&String> = lock.iter().filter(|c| !known.contains(c)).collect();
    assert!(
        missing.is_empty(),
        "{what}: crates in Cargo.lock with no license notice (added, or another version): {missing:?}. {FIX}"
    );
    let stale: Vec<&String> = known
        .iter()
        .filter(|c| !lock.contains(**c))
        .cloned()
        .collect();
    assert!(
        stale.is_empty(),
        "{what}: license notices of crates Cargo.lock no longer has: {stale:?}. {FIX}"
    );
    let both: Vec<&String> = written.intersection(unlinked).collect();
    assert!(
        both.is_empty(),
        "{what}: crates both written down and listed as not linked: {both:?}. {FIX}"
    );
}

#[test]
fn every_crate_the_app_can_link_has_its_license_written_down() {
    let notices = json("sources/SpecBoard/Resources/ThirdPartyNotices.json");
    let written: BTreeSet<String> = notices["packages"]
        .as_array()
        .unwrap()
        .iter()
        // The standard library is written in by hand (`builtin`) and is not a crate of Cargo.lock.
        .filter(|p| p["source"] == "rust" && p.get("builtin").is_none())
        .map(|p| {
            format!(
                "{} {}",
                p["name"].as_str().unwrap(),
                p["version"].as_str().unwrap()
            )
        })
        .collect();
    let unlinked = strings(&json("tools/notices/unlinked.json")["app"]);
    compare(
        "the app",
        &lock_closure("specboard-ffi"),
        &written,
        &unlinked,
    );
}

#[test]
fn every_crate_the_agent_can_link_has_its_license_written_down() {
    let text = std::fs::read_to_string(
        repo().join("sources/SpecBoard/Resources/AgentThirdPartyNotices.txt"),
    )
    .unwrap();
    // A package line is "name version - homepage" at the start of its line. What is not a crate (the Rust
    // standard library, musl libc) has a name of several words, so it never reads as "name version".
    let written: BTreeSet<String> = text
        .lines()
        .filter(|l| !l.starts_with(' ') && l.contains(" - "))
        .filter_map(|l| {
            let head = l.split(" - ").next()?;
            let mut parts = head.split(' ');
            let (name, version) = (parts.next()?, parts.next()?);
            (parts.next().is_none() && version.chars().next()?.is_ascii_digit())
                .then(|| format!("{name} {version}"))
        })
        .collect();
    let unlinked = strings(&json("tools/notices/unlinked.json")["agent"]);
    compare(
        "the agent",
        &lock_closure("specboard-agent"),
        &written,
        &unlinked,
    );
}

#[test]
fn every_license_text_a_package_names_is_there_and_every_text_is_used() {
    let notices = json("sources/SpecBoard/Resources/ThirdPartyNotices.json");
    let texts = notices["texts"].as_object().unwrap();
    let mut used: BTreeMap<&str, usize> = texts.keys().map(|k| (k.as_str(), 0)).collect();
    for package in notices["packages"].as_array().unwrap() {
        let name = package["name"].as_str().unwrap();
        let ids = package["texts"].as_array().unwrap();
        assert!(!ids.is_empty(), "{name} has no license text. {FIX}");
        for id in ids {
            let id = id.as_str().unwrap();
            match used.get_mut(id) {
                Some(count) => *count += 1,
                None => panic!("{name} names the text {id}, which is not in the file. {FIX}"),
            }
        }
    }
    for (id, text) in texts {
        assert!(
            !text["body"].as_str().unwrap().trim().is_empty(),
            "the text {id} is empty. {FIX}"
        );
    }
    let unused: Vec<&&str> = used
        .iter()
        .filter(|(_, n)| **n == 0)
        .map(|(k, _)| k)
        .collect();
    assert!(
        unused.is_empty(),
        "texts no package uses: {unused:?}. {FIX}"
    );
}
