# SpecBoard

[日本語](README.ja.md)

SpecBoard is a macOS app for schema-first, AI-driven development. A project is a board of cards that holds
the shape of your data (entities, enums, value objects) and how your system behaves (flows, documents),
and it is the single source that people and AI agents both read and write.

- **People** look, check and review on a canvas of cards and lines.
- **AI agents** read and write the same project through MCP: plan a change on a branch, check what it
  affects, and read the spec while they implement it.
- **Types, diagrams and documents are made from the project**: TypeScript, Zod, JSON Schema, Mermaid
  and Markdown. Nothing is drawn by hand, so it can be diffed and checked.

Nearly all of the code and the app icon were made by AI (Claude and Codex), directed and reviewed by the maintainer.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset=".github/assets/data-dark.png">
  <img alt="The data of a made-up online shop: entities, lines between them, and the properties of Order" src=".github/assets/data-light.png">
</picture>

<picture>
  <source media="(prefers-color-scheme: dark)" srcset=".github/assets/flow-dark.png">
  <img alt="A flow that describes how the shop takes an order" src=".github/assets/flow-light.png">
</picture>

## Install

Download the latest `SpecBoard-<version>.dmg` from
[Releases](https://github.com/abratan-a/SpecBoard/releases/latest), open it and drag SpecBoard to
Applications. The app is signed with Developer ID and notarized, and it updates itself from the Releases
of this repository.

Requirements: macOS 26.5 or later on Apple silicon.

## Use it with an AI agent (MCP)

SpecBoard serves MCP from an **agent** (`specboard-agent`), not from the app window. Projects that an AI
should read live on an agent:

1. In the app, add a host where your AI sessions run (macOS on Apple silicon or Linux x86_64, reached over
   SSH). The app installs the agent there and starts it.
2. Create a project on that host, or move one there.
3. Create an access key in the app's settings and choose which projects it may read or write.
4. Copy the command the app shows and run it on that host, for example with Claude Code:

   ```
   claude mcp add --transport http --scope user specboard http://127.0.0.1:8787/mcp --header "Authorization: Bearer <key>"
   ```

The agent listens only on `127.0.0.1` of its host (port 8787 by default) and refuses requests without a
key. The tool `guide` explains the other tools to the agent.

## Build from source

Requirements:

- macOS 26.5 or later on Apple silicon
- Xcode 26.5 or later
- Rust through [rustup](https://rustup.rs) (the version is pinned by `rust-toolchain.toml` and installed on
  the first build)
- Optional: [zig](https://ziglang.org) and [cargo-zigbuild](https://github.com/rust-cross/cargo-zigbuild)
  (`brew install zig cargo-zigbuild`) to build the Linux agent. Without them the app is built without it,
  and it cannot install an agent on Linux hosts.

```
git clone https://github.com/abratan-a/SpecBoard.git
cd SpecBoard
xcodebuild -project sources/SpecBoard.xcodeproj -scheme SpecBoard -configuration Release \
  -destination 'platform=macOS' -derivedDataPath build build
open build/Build/Products/Release/SpecBoard.app
```

Or open `sources/SpecBoard.xcodeproj` in Xcode and run the `SpecBoard` scheme.

The build is signed "to run locally" (no team). To sign with your own team, put `DEVELOPMENT_TEAM` and
`CODE_SIGN_IDENTITY` in `sources/Config/Signing.local.xcconfig` (see `sources/Config/Signing.xcconfig`).
An app you build yourself does not update itself: updates are accepted only when they are signed by the
same team as the running app.

The core (Rust) and its tests:

```
cd sources/core
cargo test --workspace
```

`tools/` holds the scripts the Xcode build runs (`build-core-lib.sh`, `build-agent.sh`), the script that
regenerates the Swift bindings of the core (`build-core-swift.sh`, checked by `swift-check.sh`) and the one
that collects third-party notices (`notices.py`). `tools/build-dmg.sh` and `tools/publish-github.sh` are what
the maintainer uses to sign, notarize and publish a release; you do not need them.

Code comments point to design documents as `specboard://SpecBoard/Docs/Design:<name>`: these are the maintainer's design documents, which are not public; read them as the names of design topics.

## Contributing

Issues are welcome, in English or Japanese. Pull requests are not accepted. See
[CONTRIBUTING.md](CONTRIBUTING.md). To report a vulnerability, see [SECURITY.md](SECURITY.md).

## License

SpecBoard is licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option. See [NOTICE](NOTICE) for the parts that come from UniFFI's templates (MPL-2.0) and where
the third-party notices are.

The name "SpecBoard" and the app icon are not covered by these licenses. If you distribute a modified
version, please give it a different name and icon.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work
by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms
or conditions.
