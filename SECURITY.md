# Security policy

## Reporting a vulnerability

Please report vulnerabilities privately through GitHub:
open the **Security** tab of this repository and choose **Report a vulnerability**
([direct link](https://github.com/abratan-a/SpecBoard/security/advisories/new)).
Do not open a public issue for a vulnerability.

You can write in English or Japanese. Please include the version of SpecBoard (SpecBoard > About SpecBoard), your macOS version, and the steps to reproduce. Do not attach real
project data; a small made-up project is enough.

The maintainer works on SpecBoard alone and in their own time, so there is no fixed response time. You will
get a reply in the advisory when the report has been read, and the fix will be released as a new version
with a note in the changelog.

## Supported versions

Only the latest release gets security fixes. The app updates itself from this repository's Releases.

## Scope

Things worth reporting include, for example:

- the MCP endpoint of the agent (`specboard-agent`): it should listen only on `127.0.0.1`, refuse requests
  without a valid access key, refuse requests from web pages (an `Origin` header, a `Host` that is not
  loopback), and let a key reach only the projects it was given;
- how the app installs and starts the agent on a host over SSH;
- the self-update: a downloaded DMG must not be installed unless it is SpecBoard, signed by the same team
  as the running app;
- exports writing outside the folder they were given;
- Markdown and diagrams shown in the app running scripts or reaching the network.

Builds you made yourself from source are in scope when the problem is in the source.
