# Contributing

Thank you for your interest in SpecBoard.

## Issues: welcome

Bug reports, questions and ideas are welcome as issues, **in English or Japanese**
(issue は日本語でも英語でも受け付けます).

- Search the existing issues first.
- For a bug, include the version of SpecBoard, your macOS version, what you did, what you expected and what
  happened. A screenshot or the text of an error helps.
- Do not paste real project data or access keys. A small made-up project is enough to show a problem.
- Vulnerabilities go through [SECURITY.md](SECURITY.md), not issues.

## Pull requests: not accepted

This repository receives one commit per release from the maintainer's own development repository, so a pull
request cannot be merged here, and pull requests are closed without review. If you have a fix, describe it in
an issue (a short diff in the issue is fine). If it is taken, it is reworked in the development repository and
comes out in a later release.

By posting code in an issue you agree that it may be used under the project's licenses (MIT OR Apache-2.0).

## Building and testing

See "Build from source" in the [README](README.md). The core's tests run with `cargo test --workspace` in
`sources/core`.

## Code of conduct

Please follow the [Code of Conduct](CODE_OF_CONDUCT.md).
