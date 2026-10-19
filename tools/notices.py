#!/usr/bin/env python3
"""Collects the licenses of what the app and its agent are built from, and writes them down.

    python3 tools/notices.py            # write the files below
    python3 tools/notices.py --check    # write nothing; exit 1 if a file would change

Writes, from the dependency trees in sources/core and the checked-in tools/notices/bundled.json:

  sources/SpecBoard/Resources/ThirdPartyNotices.json
      What the app shows in Settings > Licenses: every package (Rust crates the core links, npm packages
      mermaid bundles, cmark-gfm) with its license, homepage and copyright lines, and the license texts,
      each distinct text once (`texts`; a package names its texts by id).
  sources/SpecBoard/Resources/AgentThirdPartyNotices.txt
      The same for the agent (specboard-agent), as plain text grouped by license. The agent binaries ship
      in the app bundle's resources, so this file ships beside them.
  tools/notices/unlinked.json
      The crates Cargo.lock lists under the app's and the agent's trees that are not linked on any target
      we build (build scripts' tools, other platforms' crates). The core test
      (sources/core/tests/third_party_notices.rs) reads it, with the JSON, to see that every crate in
      Cargo.lock's trees is either written down here or known not to be linked.

When it has to be run again: a crate is added or its version changes (the core test fails and says so), or
mermaid's version changes.

Rust crates: `cargo metadata` for the app's targets (aarch64 macOS; tools/build-core-lib.sh) from
specboard-ffi, and for the agent's targets (those two and x86_64 Linux musl; tools/build-agent.sh) from
specboard-agent. Only normal dependencies count (a build script's or a test's tools are not in the
binary). The license texts and copyright lines are read from the crate's own LICENSE*/COPYING*/NOTICE files in
the cargo registry (so the crates have to be downloaded: `cargo fetch` in sources/core). A crate without
a copyright line in its files gets its Cargo.toml authors, marked so (`copyrightFrom`). A crate that ships
no license file at all gets its text from tools/notices/bundled.json: its own entry under `crates` (the text
and copyright lines of its repository's LICENSE at that version, as for the tree-sitter grammars), or else the
text of that license another bundled package carries (the MPL-2.0 ones). An entry under `crates` may also name
a file inside the crate whose notice it carries besides its own (`also`: tree-sitter's ICU headers).

mermaid and cmark-gfm do not change with cargo: tools/notices/bundled.json holds them (name, version, source,
license, homepage, copyright lines, and the texts as the packages ship them). To refresh the npm part for a
new mermaid version:

    mkdir /tmp/mermaid-notices && cd /tmp/mermaid-notices && npm init -y && npm install mermaid@<version>
    python3 tools/notices.py --refresh-npm /tmp/mermaid-notices/node_modules
    python3 tools/notices.py

--refresh-npm rewrites the npm entries of bundled.json from the installed tree (the cmark-gfm entry is kept) and
needs npm and the network. What is inside mermaid.min.js is read from its source maps, not only from the tree:
the packages named by the pnpm paths in `mermaid.min.js.map`, and in the maps of the `@mermaid-js/parser` chunks
it includes (the parser is prebundled, so langium, chevrotain, vscode-languageserver and the others are not in
node_modules). A package at the version a map names that the tree does not hold is fetched with `npm pack`; a
package of the tree at another version than the one inside the bundle is dropped. webpack-bundled code names
no version (vscode-uri and its path-browserify): bundled.json's `mapless` says which package declares each, and
the version is what that declaration resolves to. What was set by hand in an entry stays: dompurify's two texts
(its file holds the MPL and the Apache text in one), the placeholder of a package that ships no license file
(fastdom, strictdom), a copyright line that does not start with "(c)" or a year (lodash-es), and the license
of one whose package.json has none (khroma). Check the diff of bundled.json.

Things no dependency tree shows are written in as well: the Rust standard library (the toolchain version of
rust-toolchain.toml; in both lists) and, for the agent only, the musl libc that zig links into the Linux
binary (bundled.json, `agentOnly`).

`--check` also fails on: a license that is not on the allowlist (ALLOWED), the mermaid version in
mermaid.min.js or the swift-cmark version in Package.resolved differing from the entry of the list, and (when
zig is installed) the musl version of its lib dir differing from the entry. It needs `cargo fetch` and no npm.

The output has no date and is sorted, so running it again gives no diff. Python 3, no third-party modules.
"""

import hashlib
import json
import os
import re
import subprocess
import sys
import tarfile
import tempfile

TOOL_STAMP = "notices.py 1"

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CORE = os.path.join(ROOT, "sources", "core")
OUT_JSON = os.path.join(ROOT, "sources", "SpecBoard", "Resources", "ThirdPartyNotices.json")
OUT_AGENT = os.path.join(ROOT, "sources", "SpecBoard", "Resources", "AgentThirdPartyNotices.txt")
BUNDLED = os.path.join(ROOT, "tools", "notices", "bundled.json")
UNLINKED = os.path.join(ROOT, "tools", "notices", "unlinked.json")

APP_TARGETS = ["aarch64-apple-darwin"]
AGENT_TARGETS = ["aarch64-apple-darwin", "x86_64-unknown-linux-musl"]

COPYRIGHT = re.compile(r"^(Copyright|\(c\)|©)\s*(\(c\)|©|\d)", re.I)
MIT_TITLE = re.compile(r"^(the )?mit license( \(mit\))?$", re.I)
LICENSE_FILE = re.compile(r"^(LICEN[CS]E|COPYING|UNLICENSE|NOTICE)", re.I)
APACHE = ("Apache-2.0", "Apache-2.0 WITH LLVM-exception")
ORDER = {"MIT": 0, "Apache-2.0": 1}

MERMAID_JS = os.path.join(ROOT, "sources", "SpecBoard", "Resources", "mermaid.min.js")
PACKAGE_RESOLVED = os.path.join(
    ROOT, "sources", "SpecBoard.xcodeproj", "project.xcworkspace", "xcshareddata", "swiftpm", "Package.resolved"
)
TOOLCHAIN = os.path.join(ROOT, "rust-toolchain.toml")

# The licenses a package may be under (SPDX names; `blessing` is SQLite's public-domain dedication). An expression
# passes when each license it names is here, whether joined by OR, AND or `WITH LLVM-exception`.
ALLOWED = {
    "MIT",
    "Apache-2.0",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "Zlib",
    "Unlicense",
    "Unicode-3.0",
    "Unicode-DFS-2016",
    "MPL-2.0",
    "blessing",
}


# ---- license expressions ------------------------------------------------------------------------------


def canonical(expression):
    """The spelling the page groups by: `MIT/Apache-2.0` and `Apache-2.0 OR MIT` are one license."""
    e = expression.strip().replace("/", " OR ")
    if e.startswith("(") and e.endswith(")") and e.count("(") == 1:
        e = e[1:-1]
    if "(" in e or " AND " in e:
        return e
    return " OR ".join(sorted({a.strip() for a in e.split(" OR ")}, key=atom_order))


def atom_order(atom):
    base = atom.split(" WITH ")[0]
    return (ORDER.get(base, 9), atom != base, atom)


def atoms(expression):
    """The licenses an expression names, in the order of its canonical spelling."""
    out = []
    for a in re.split(r"\s+(?:OR|AND)\s+|[()]", expression):
        a = a.strip()
        if a and a not in out:
            out.append(a)
    return out


def classify(body):
    """Which license a file's text is, from what it says; None for a file that only points elsewhere."""
    low = body.lower()
    if "apache license" in low and "terms and conditions for use, reproduction, and distribution" in low:
        return "Apache-2.0 WITH LLVM-exception" if "llvm exceptions to the apache 2.0 license" in low else "Apache-2.0"
    if "mozilla public license" in low and "version 2.0" in low:
        return "MPL-2.0"
    if "free and unencumbered software released into the public domain" in low:
        return "Unlicense"
    if "unicode license" in low or "unicode data files" in low:
        return "Unicode-3.0"
    if "permission is hereby granted, free of charge" in low:
        return "MIT"
    if "redistribution and use in source and binary forms" in low:
        return "BSD-3-Clause" if "neither the name" in low else "BSD-2-Clause"
    if "permission to use, copy, modify, and/or distribute this software for any purpose" in low:
        return "ISC"
    if "provided 'as-is'" in low or 'provided "as-is"' in low:
        return "Zlib"
    return None


# ---- texts --------------------------------------------------------------------------------------------


def copyright_lines(text):
    out = []
    for line in text.split("\n"):
        s = line.strip()
        if COPYRIGHT.match(s) and len(s) < 160 and s not in out:
            out.append(s)
    return out


def clean(raw):
    """The text without its copyright lines (they differ per package and are listed beside it)."""
    out, blank = [], True
    for line in raw.replace("\r\n", "\n").split("\n"):
        line = line.rstrip()
        s = line.strip()
        if COPYRIGHT.match(s) and len(s) < 160:
            continue
        if not s:
            if blank:
                continue
            blank = True
        else:
            blank = False
        out.append(line)
    return "\n".join(out).strip("\n")


def same_text_key(body):
    """Texts that differ only in wrapping, the title, the copyright lines or the Apache appendix are one text."""
    lines = [l for l in body.split("\n") if not MIT_TITLE.match(l.strip())]
    key = " ".join(" ".join(lines).lower().split())
    # The Apache text's appendix is a template for one's own files, which crates trim or keep.
    end = key.find("end of terms and conditions")
    return key if end < 0 else key[: end + len("end of terms and conditions")]


class Texts:
    def __init__(self):
        self.by_id = {}
        self.label_of_first = {}

    def add(self, label, raw):
        body = clean(raw)
        digest = hashlib.sha256((label + "\0" + same_text_key(body)).encode()).hexdigest()[:8]
        slug = re.sub(r"[^a-z0-9]+", "-", label.lower()).strip("-")
        text_id = "%s-%s" % (slug, digest)
        if text_id not in self.by_id:
            self.by_id[text_id] = {"license": label, "body": body}
        self.label_of_first.setdefault(label, text_id)
        return text_id


# ---- Rust crates --------------------------------------------------------------------------------------


def cargo():
    return os.environ.get("CARGO") or ("/usr/local/bin/cargo" if os.path.exists("/usr/local/bin/cargo") else "cargo")


def metadata(target):
    run = subprocess.run(
        [cargo(), "metadata", "--format-version", "1", "--filter-platform", target, "--locked", "--offline"],
        cwd=CORE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        universal_newlines=True,
    )
    if run.returncode != 0:
        sys.exit("cargo metadata failed (run `cargo fetch` in sources/core first):\n" + run.stderr)
    return json.loads(run.stdout)


def linked_crates(root_name, targets):
    """{(name, version): package} of the registry crates linked into `root_name` on any of the targets."""
    out = {}
    for target in targets:
        meta = metadata(target)
        pk = {p["id"]: p for p in meta["packages"]}
        nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
        roots = [i for i, p in pk.items() if p["name"] == root_name]
        seen, stack = set(), list(roots)
        while stack:
            i = stack.pop()
            if i in seen:
                continue
            seen.add(i)
            for d in nodes[i]["deps"]:
                if any(k["kind"] is None for k in d["dep_kinds"]):
                    stack.append(d["pkg"])
        for i in seen:
            p = pk[i]
            if p["source"] is not None:
                out[(p["name"], p["version"])] = p
    return out


def lock_closure(root_name):
    """The "name version" of every registry crate Cargo.lock reaches from `root_name`, whatever the kind or target."""
    packages, current = [], None
    for line in open(os.path.join(CORE, "Cargo.lock"), encoding="utf-8"):
        line = line.rstrip("\n")
        if line == "[[package]]":
            current = {"deps": []}
            packages.append(current)
        elif current is None:
            continue
        elif line.startswith("name = "):
            current["name"] = line.split('"')[1]
        elif line.startswith("version = "):
            current["version"] = line.split('"')[1]
        elif line.startswith("source = "):
            current["source"] = True
        elif line.startswith(' "'):
            current["deps"].append(line.strip().strip(",").strip('"'))
    by_name = {}
    for p in packages:
        by_name.setdefault(p["name"], []).append(p)
    seen, stack = set(), [p for p in packages if p["name"] == root_name]
    while stack:
        p = stack.pop()
        key = (p["name"], p["version"])
        if key in seen:
            continue
        seen.add(key)
        for d in p["deps"]:
            parts = d.split(" ")
            candidates = by_name[parts[0]]
            if len(parts) > 1:
                candidates = [c for c in candidates if c["version"] == parts[1]]
            stack.extend(candidates)
    registry = {(p["name"], p["version"]) for p in packages if p.get("source")}
    return {"%s %s" % k for k in seen if k in registry}


def crate_files(manifest):
    folder = os.path.dirname(manifest)
    out = []
    for name in sorted(os.listdir(folder)):
        path = os.path.join(folder, name)
        if LICENSE_FILE.match(name) and os.path.isfile(path):
            with open(path, encoding="utf-8", errors="replace") as f:
                out.append((name, f.read()))
    return out


def rust_package(p, texts, fallback, overrides):
    expression = p["license"]
    if not expression:
        sys.exit("%s %s has no license in Cargo.toml" % (p["name"], p["version"]))
    files = [(n, t, classify(t)) for n, t in crate_files(p["manifest_path"])]
    own = overrides.get(p["name"], {})
    given = {t["license"]: t["body"] for t in own.get("texts", [])}
    ids = []
    for atom in atoms(canonical(expression)):
        mine = [f for f in files if f[2] == atom]
        mine.sort(key=lambda f: (re.split(r"[- ]", atom)[0].upper() not in f[0].upper(), f[0]))
        if mine:
            ids.append(texts.add(atom, mine[0][1]))
        elif atom in given:
            ids.append(texts.add(atom, given[atom]))
        elif atom in fallback:
            ids.append(texts.add(atom, fallback[atom]))
        else:
            sys.exit("%s %s: no text for %s (its files: %s)" % (p["name"], p["version"], atom, [f[0] for f in files]))
    lines = []
    for name, text, kind in files:
        if kind not in APACHE:
            for line in copyright_lines(text):
                if line not in lines:
                    lines.append(line)
    if not files:
        lines = list(own.get("copyright", []))
    # Notices of what a crate carries inside it under another license (tree-sitter's ICU headers).
    for also in own.get("also", []):
        path = os.path.join(os.path.dirname(p["manifest_path"]), also["file"])
        with open(path, encoding="utf-8", errors="replace") as f:
            body = f.read()
        ids.append(texts.add(also["license"], body))
        for line in copyright_lines(body)[:1]:
            if line not in lines:
                lines.append(line)
    package = {
        "name": p["name"],
        "version": p["version"],
        "source": "rust",
        "license": canonical(expression),
        "homepage": p.get("homepage") or p.get("repository") or "https://crates.io/crates/" + p["name"],
        "copyright": lines[:6],
        "texts": ids,
    }
    if not lines:
        authors = [re.sub(r"\s*<.*?>", "", a).strip() for a in p.get("authors") or []]
        authors = [a for a in authors if a]
        if authors:
            package["copyright"] = ["Copyright (c) " + ", ".join(authors)]
            package["copyrightFrom"] = "authors"
    return package


# ---- what does not change with cargo ------------------------------------------------------------------


def bundled_packages(texts, agent=False):
    """The packages of bundled.json for the app's list, or (agent=True) for the agent's: the ones marked `agentOnly`."""
    out = []
    for b in json.load(open(BUNDLED, encoding="utf-8"))["packages"]:
        if bool(b.get("agentOnly")) != agent:
            continue
        package = {
            "name": b["name"],
            "version": b["version"],
            "source": b["source"],
            "license": canonical(b["license"]),
            "homepage": b["homepage"],
            "copyright": b["copyright"],
            "texts": [texts.add(t["license"], t["body"]) for t in b["texts"]],
        }
        for key in ("copyrightFrom", "builtin"):
            if b.get(key):
                package[key] = b[key]
        out.append(package)
    return out


def rust_toolchain():
    with open(TOOLCHAIN, encoding="utf-8") as f:
        for line in f:
            m = re.match(r'\s*channel\s*=\s*"([^"]+)"', line)
            if m:
                return m.group(1)
    sys.exit("no channel in rust-toolchain.toml")


def rust_std(texts, fallback):
    """The Rust standard library every binary carries; the toolchain version cannot drift from rust-toolchain.toml."""
    version = rust_toolchain()
    return {
        "name": "Rust standard library",
        "version": version,
        "source": "rust",
        "license": "MIT OR Apache-2.0",
        "homepage": "https://github.com/rust-lang/rust/tree/%s/library" % version,
        "copyright": [
            "Copyright (c) The Rust Project Contributors",
            "Includes the crates the standard library bundles (compiler_builtins, hashbrown, addr2line, gimli, object, "
            "miniz_oxide, adler2, rustc-demangle, memchr, libc, unwind and others) under their own licenses; "
            "see COPYRIGHT in the repository",
        ],
        "texts": [texts.add(atom, fallback[atom]) for atom in ("MIT", "Apache-2.0")],
        "builtin": True,
    }


def crate_overrides():
    """What bundled.json says of crates beyond their own files: the license text of one that ships none
    (copied from its repository at that version), and notices of what one carries under another license."""
    return {c["name"]: c for c in json.load(open(BUNDLED, encoding="utf-8")).get("crates", [])}


def bundled_fallback():
    """The text of each license the bundled packages carry, for crates that ship none of their own."""
    out = {}
    for b in json.load(open(BUNDLED, encoding="utf-8"))["packages"]:
        for t in b["texts"]:
            out.setdefault(t["license"], t["body"])
    return out


PNPM_PATH = re.compile(r"node_modules/\.pnpm/(.+?)/node_modules/((?:@[^/]+/)?[^/]+)/")


def pnpm_package(source):
    """(name, version) of the package a source-map path inside pnpm's store names; None for any other path."""
    m = PNPM_PATH.search(source)
    if not m:
        return None
    name = m.group(2)
    prefix = name.replace("/", "+") + "@"
    if not m.group(1).startswith(prefix):
        return None
    # `chevrotain-allstar@0.3.1_chevrotain@11.1.2`: what follows the underscore is its peers.
    return name, m.group(1)[len(prefix) :].split("_")[0]


def map_sources(path):
    if not os.path.exists(path):
        sys.exit("%s is missing: the installed package has no source map to read the bundled packages from" % path)
    with open(path, encoding="utf-8") as f:
        return json.load(f).get("sources", [])


def npm_json(*args):
    run = subprocess.run(["npm", *args, "--json"], stdout=subprocess.PIPE, stderr=subprocess.PIPE, universal_newlines=True)
    if run.returncode != 0:
        sys.exit("npm %s failed:\n%s" % (" ".join(args), run.stderr))
    return json.loads(run.stdout) if run.stdout.strip() else None


def version_key(version):
    return [int(n) if n.isdigit() else 0 for n in re.split(r"[.+-]", version)]


def resolve_range(name, spec):
    versions = npm_json("view", "%s@%s" % (name, spec), "version")
    versions = [versions] if isinstance(versions, str) else versions
    return max(versions, key=version_key)


def bundled_in_mermaid(modules, mapless):
    """{(name, version)} of the npm packages whose code is inside mermaid.min.js, from its source maps."""
    found, parser_maps = set(), set()
    for source in map_sources(os.path.join(modules, "mermaid", "dist", "mermaid.min.js.map")):
        package = pnpm_package(source)
        if package:
            found.add(package)
        if "/parser/dist/" in source:
            parser_maps.add(os.path.join(modules, "@mermaid-js", "parser", "dist", source.split("/parser/dist/", 1)[1] + ".map"))
    unnamed = set()
    for path in sorted(parser_maps):
        for source in map_sources(path):
            package = pnpm_package(source)
            if package:
                found.add(package)
            elif source.startswith("webpack://") and "/node_modules/" in source:
                unnamed.add(source.split("/node_modules/", 1)[1].split("/")[0])
    missing = sorted(unnamed - set(mapless))
    if missing:
        sys.exit("webpack-bundled packages with no version in any map; add them to `mapless` in bundled.json: %s" % missing)
    # Their version is what the package that bundles them declares.
    versions = {name: version for name, version in found}
    for name in sorted(mapless, key=lambda n: mapless[n] in mapless):
        via = mapless[name]
        declared = {}
        for field in ("devDependencies", "dependencies"):
            declared.update(npm_json("view", "%s@%s" % (via, versions[via]), field) or {})
        spec = declared.get(name)
        if not spec:
            sys.exit("%s@%s does not declare %s (bundled.json `mapless`)" % (via, versions[via], name))
        versions[name] = resolve_range(name, spec)
        found.add((name, versions[name]))
    return found


def read_package(folder):
    """(name, version, license, license text) of an installed package, or None for a folder that is not one."""
    with open(os.path.join(folder, "package.json"), encoding="utf-8") as f:
        info = json.load(f)
    if not info.get("name") or not info.get("version"):
        return None
    license_ = info.get("license")
    if isinstance(license_, dict):
        license_ = license_.get("type")
    if not license_ and info.get("licenses"):
        license_ = " OR ".join(l.get("type", "") for l in info["licenses"])
    text = ""
    for name in sorted(os.listdir(folder)):
        if LICENSE_FILE.match(name) and os.path.isfile(os.path.join(folder, name)):
            with open(os.path.join(folder, name), encoding="utf-8", errors="replace") as f:
                text += f.read().strip("\n") + "\n\n"
    return info["name"], info["version"], license_ or "UNKNOWN", text.strip()


def fetch_package(name, version, scratch):
    """Downloads one exact version with `npm pack` and reads it."""
    packed = npm_json("pack", "%s@%s" % (name, version), "--pack-destination", scratch)
    target = os.path.join(scratch, "%s@%s" % (name.replace("/", "+"), version))
    with tarfile.open(os.path.join(scratch, packed[0]["filename"])) as tar:
        tar.extractall(target)
    return read_package(os.path.join(target, "package"))


def refresh_npm(modules):
    """Rewrites the npm entries of bundled.json from an installed node_modules tree and mermaid's source maps."""
    data = json.load(open(BUNDLED, encoding="utf-8"))
    found = {}
    for folder, _, names in os.walk(modules):
        if "package.json" in names:
            package = read_package(folder)
            if package:
                found[(package[0], package[1])] = package[2:]
    inside = bundled_in_mermaid(modules, data.get("mapless", {}))
    with tempfile.TemporaryDirectory() as scratch:
        for name, version in sorted(inside - set(found)):
            package = fetch_package(name, version, scratch)
            if package is None or (package[0], package[1]) != (name, version):
                sys.exit("could not read %s@%s with npm pack" % (name, version))
            found[(name, version)] = package[2:]
    # A tree package at another version than the one inside the bundle is not what the app carries.
    names_inside = {name for name, _ in inside}
    found = {k: v for k, v in found.items() if k[0] not in names_inside or k in inside}
    keep = [p for p in data["packages"] if p["source"] != "npm"]
    old = {p["name"]: p for p in data["packages"] if p["source"] == "npm"}
    for (name, version), (license_, text) in sorted(found.items()):
        before = old.get(name)
        label = before["license"] if license_ == "UNKNOWN" and before else license_
        texts = [{"license": canonical(label), "body": text}]
        lines = copyright_lines(text)[:3]
        entry = {
            "name": name,
            "version": version,
            "source": "npm",
            "license": canonical(label),
            "homepage": "https://www.npmjs.com/package/" + name,
        }
        # What was set by hand (a package that ships two texts in one file, or none) stays.
        if before:
            if len(before["texts"]) > 1 and canonical(before["license"]) == canonical(label):
                texts = before["texts"]
            if not text:
                texts = before["texts"]
                if before.get("copyrightFrom"):
                    entry["copyrightFrom"] = before["copyrightFrom"]
            if not lines:
                lines = before["copyright"]
        entry["copyright"] = lines
        entry["texts"] = texts
        keep.append(entry)
    keep.sort(key=lambda p: (p["source"], p["name"].lower(), p["version"]))
    data["packages"] = keep
    with open(BUNDLED, "w", encoding="utf-8") as f:
        json.dump(data, f, ensure_ascii=False, indent=1)
        f.write("\n")
    print("bundled.json: %d npm packages, %d of them from mermaid's maps" % (len([p for p in keep if p["source"] == "npm"]), len(inside)))


# ---- output -------------------------------------------------------------------------------------------


def sort_key(p):
    return (p["name"].lower(), p["version"], p["source"])


def json_text(packages, texts):
    lines = ["{", ' "generated": %s,' % json.dumps(TOOL_STAMP), ' "packages": [']
    rows = [" " + json.dumps(p, ensure_ascii=False, separators=(",", ":")) for p in sorted(packages, key=sort_key)]
    lines.append(",\n".join(rows))
    lines += [" ],", ' "texts": {']
    rows = [
        " %s: %s" % (json.dumps(k), json.dumps(v, ensure_ascii=False, separators=(",", ":")))
        for k, v in sorted(texts.by_id.items())
    ]
    lines.append(",\n".join(rows))
    lines += [" }", "}", ""]
    return "\n".join(lines)


def groups(packages):
    out = {}
    for p in packages:
        out.setdefault(p["license"], []).append(p)
    return sorted(out.items(), key=lambda kv: (-len(kv[1]), kv[0]))


def agent_text(packages, texts):
    rule = "=" * 72
    lines = [
        "SpecBoard agent (specboard-agent) - third-party notices",
        "",
        "The agent is built from the Rust crates and libraries below. Each is distributed under its own license, reproduced",
        "here as the license requires: the packages of each license come first with their copyright lines,",
        "then the text of the license once. Where a package offers a choice (MIT OR Apache-2.0), you may use",
        "it under either.",
        "",
    ]
    for license_, members in groups(packages):
        lines += [rule, "%s  (%d package%s)" % (license_, len(members), "" if len(members) == 1 else "s"), rule, ""]
        for p in sorted(members, key=sort_key):
            lines.append("%s %s - %s" % (p["name"], p["version"], p["homepage"]))
            for c in p["copyright"]:
                lines.append("    " + c)
            if p.get("copyrightFrom") == "authors":
                lines.append("    (from the crate's authors; its license file has no copyright line)")
        used = []
        for p in sorted(members, key=sort_key):
            for t in p["texts"]:
                if t not in used:
                    used.append(t)
        for t in used:
            lines += ["", "--- %s ---" % texts.by_id[t]["license"], "", texts.by_id[t]["body"]]
        lines.append("")
    return "\n".join(lines)


def collect():
    texts = Texts()
    fallback = bundled_fallback()
    overrides = crate_overrides()
    app = linked_crates("specboard-ffi", APP_TARGETS)
    agent = linked_crates("specboard-agent", AGENT_TARGETS)
    # One record per crate, whichever tree it is in, so the same crate has the same texts in both.
    records = {}
    for key, p in sorted({**app, **agent}.items()):
        records[key] = rust_package(p, texts, fallback, overrides)
    std = rust_std(texts, fallback)
    app_packages = [records[k] for k in app] + bundled_packages(texts) + [std]
    agent_packages = [records[k] for k in agent] + bundled_packages(texts, agent=True) + [std]
    unlinked = {
        "app": sorted(lock_closure("specboard-ffi") - {"%s %s" % k for k in app}),
        "agent": sorted(lock_closure("specboard-agent") - {"%s %s" % k for k in agent}),
    }
    unlinked_text = json.dumps(unlinked, indent=1) + "\n"
    return {
        OUT_JSON: json_text(app_packages, texts),
        OUT_AGENT: agent_text(agent_packages, texts),
        UNLINKED: unlinked_text,
    }, (app_packages, agent_packages, texts)


# ---- checks -------------------------------------------------------------------------------------------


def allowed(atom):
    if " WITH " in atom:
        base, exception = atom.split(" WITH ", 1)
        return base in ALLOWED and exception == "LLVM-exception"
    return atom in ALLOWED


def zig_musl_version():
    """The musl version of the zig that links the Linux agent, or None when zig is not installed here."""
    zig = os.environ.get("ZIG") or ("/opt/homebrew/bin/zig" if os.path.exists("/opt/homebrew/bin/zig") else "zig")
    try:
        env = subprocess.run([zig, "env"], stdout=subprocess.PIPE, stderr=subprocess.PIPE, universal_newlines=True)
        lib_dir = re.search(r'\.lib_dir = "([^"]+)"', env.stdout)
        with open(os.path.join(lib_dir.group(1), "libc", "musl", "src", "internal", "version.h"), encoding="utf-8") as f:
            return re.search(r'VERSION "([^"]+)"', f.read()).group(1)
    except (OSError, AttributeError):
        return None


def problems(app_packages, agent_packages):
    """What is wrong with the lists, said by package: a license off the allowlist, or a version that drifted."""
    out = []
    seen = set()
    for p in app_packages + agent_packages:
        if (p["name"], p["version"]) in seen:
            continue
        seen.add((p["name"], p["version"]))
        bad = [a for a in atoms(p["license"]) if not allowed(a)]
        if bad:
            out.append("%s %s: license %s is not on the allowlist (%s)" % (p["name"], p["version"], bad, p["license"]))

    def entry(name):
        found = [p["version"] for p in app_packages if p["name"] == name]
        return found[0] if found else None

    with open(MERMAID_JS, encoding="utf-8") as f:
        m = re.search(r'\{version:"(\d+\.\d+\.\d+)"\},[\w$]+=[\w$]+\(\(\)=>[\w$]+\.version,"getVersion"\)', f.read())
    if not m:
        out.append("mermaid.min.js: its version was not found (the pattern in notices.py no longer fits)")
    elif entry("mermaid") != m.group(1):
        out.append("mermaid: mermaid.min.js is %s but the list says %s (run --refresh-npm)" % (m.group(1), entry("mermaid")))
    with open(PACKAGE_RESOLVED, encoding="utf-8") as f:
        pins = {pin["identity"]: pin["state"].get("version") for pin in json.load(f)["pins"]}
    if entry("cmark-gfm") != pins.get("swift-cmark"):
        out.append("cmark-gfm: Package.resolved pins swift-cmark %s but the list says %s" % (pins.get("swift-cmark"), entry("cmark-gfm")))
    zig = zig_musl_version()
    musl = [p["version"] for p in agent_packages if p["name"] == "musl libc"]
    if zig is not None and musl != [zig]:
        out.append("musl libc: zig links %s but the agent list says %s (tools/notices/bundled.json)" % (zig, musl))
    return out


def main(argv):
    if "--refresh-npm" in argv:
        refresh_npm(argv[argv.index("--refresh-npm") + 1])
        return 0
    files, (packages, agent_packages, texts) = collect()
    wrong = problems(packages, agent_packages)
    for w in wrong:
        print("error: " + w)
    if wrong:
        return 1
    changed = []
    for path, content in files.items():
        old = open(path, encoding="utf-8").read() if os.path.exists(path) else None
        if old != content:
            changed.append(os.path.relpath(path, ROOT))
            if "--check" not in argv:
                with open(path, "w", encoding="utf-8") as f:
                    f.write(content)
    licenses = {p["license"] for p in packages}
    print("%d packages in %d licenses, %d distinct texts" % (len(packages), len(licenses), len(texts.by_id)))
    for c in changed:
        print(("would change " if "--check" in argv else "wrote ") + c)
    return 1 if changed and "--check" in argv else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
