//! The text tables (`src/text/<language>/<area>.json`), embedded: every area file of every language
//! folder, found here so that adding an area is adding its files, with no list to edit beside them.
//! So are the other languages' texts of the built-in kinds (`src/kinds/text/<language>.json`) and of
//! the MCP tools (`src/mcp/text/<language>.json`): adding a language is putting its files there.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    println!("cargo:rerun-if-changed=build.rs");
    tables(&manifest.join("src/text"), &out_dir.join("text_tables.rs"));
    overlays(
        &manifest.join("src/kinds/text"),
        &out_dir.join("kinds_overlays.rs"),
    );
    overlays(
        &manifest.join("src/mcp/text"),
        &out_dir.join("mcp_overlays.rs"),
    );
}

/// The files of `dir` with the extension `json`, in name order.
fn json_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.expect("a folder's entry").path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();
    files
}

fn tables(root: &Path, target: &Path) {
    println!("cargo:rerun-if-changed={}", root.display());
    let mut languages: Vec<_> = std::fs::read_dir(root)
        .expect("src/text")
        .map(|e| e.expect("src/text").path())
        .filter(|p| p.is_dir())
        .collect();
    languages.sort();
    let mut out = String::from(
        "/// Each language's tables: its code and, for each area, the area's name and its JSON.\n\
         const SOURCES: &[(&str, &[(&str, &str)])] = &[\n",
    );
    for folder in languages {
        println!("cargo:rerun-if-changed={}", folder.display());
        let code = folder.file_name().unwrap().to_string_lossy().into_owned();
        writeln!(out, "    ({code:?}, &[").unwrap();
        for area in json_files(&folder) {
            let name = area.file_stem().unwrap().to_string_lossy().into_owned();
            let path = area.to_string_lossy().into_owned();
            writeln!(out, "        ({name:?}, include_str!({path:?})),").unwrap();
        }
        out.push_str("    ]),\n");
    }
    out.push_str("];\n");
    std::fs::write(target, out).unwrap();
}

/// `OVERLAYS`: each language's file of `dir`, by its code (the file's name).
fn overlays(dir: &Path, target: &Path) {
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut out = String::from(
        "/// Each language's texts over the Japanese ones: its code and its JSON.\n\
         const OVERLAYS: &[(&str, &str)] = &[\n",
    );
    for file in json_files(dir) {
        let code = file.file_stem().unwrap().to_string_lossy().into_owned();
        let path = file.to_string_lossy().into_owned();
        writeln!(out, "    ({code:?}, include_str!({path:?})),").unwrap();
    }
    out.push_str("];\n");
    std::fs::write(target, out).unwrap();
}
