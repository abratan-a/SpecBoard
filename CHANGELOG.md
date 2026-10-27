<!-- SpecBoard がプロジェクト「SpecBoard」から作ったファイルです。書き換えはプロジェクトで -->

# Changelog

Changes in each public release of SpecBoard that people using it can see (newest first).

## 0.3.29

- Actors: a new kind of section for something that provides and uses things (a system, a person, or something external). It holds the flows it carries out and the actors inside it, and opens as a Map or a Sequence of the flows others use. Any section can have its own icon and color, shown in the navigator, tabs and section ghosts.
- Sequence diagrams: lifelines are only the ones that act, seen from the flow's depth; open an actor to see the ones inside, or fold them into their parent. Steps are named bands, and calls to the same actor's flows show their contents in nested frames. A start with no caller written, or a write with no store written, is drawn as an orange dashed gap instead of being guessed; a start can take calls from anyone (MCP `anyCaller`). The heads stay pinned at the top when you scroll. Branches and loops become alt and loop frames, and a diagram too large to draw says so instead of hanging the agent.
- Inspection: finds partners written as text that name a flow or an actor, and stores that are not written (with the candidate stores when more than one is reachable).
- Groups: named frames on section maps and goal maps, with a corner, color and fill of their own. Moving a group moves the cards inside it. MCP reads and writes them as `groups`.
- Arrange: a button on the map tidies overlaps, or arranges cards along their lines (referenced cards to the left, callers to the right, groups as one block). The result is the same each time and does not freeze the screen. MCP `section_update` has `tidy` and `arrange`.
- Data cards: a shape can give a list face, so each element of an array is one row with a mark, a note and contents that open. MCP names array elements by their primary key and can add or delete one. Merge compares data cards field by field and element by element.
- Kinds: project settings edit kinds with a form as well as JSON, with a live preview of a card of that kind. Built-in kinds open in the same form, read-only.
- Built-in kinds: line names now say the same thing as their labels (`notes` → `rules`, `of` → `shapeOf`, `branch` → `condition`, `event` → `awaits`, `peer` → `partner`), and every line that can appear in the same diagram looks different. The screen takes the words for sections, card badges, fields, lines and flow exits from the kind definitions. Goal status and priority are stored in English and shown in the app's language. Existing projects move to the new names when they are opened.
- Goal cards show their status in the head only, and other fields as rows only when they are written. Completed and cancelled sections have their own icons.
- Notes are Markdown and are drawn as Markdown on the map.
- Enumerations: "open" (not exhaustive) can be switched in the property panel. Sections of fields the screen cannot write say they are written over MCP.
- Mention lines (references from a body to other cards) are hidden at first and can be turned on from the line toggles; MCP diagrams include them only when asked.
- Formatted documents: right-click a row to copy its path or a `[[…]]` link. Brackets in the margin show the nesting of items.
- Cards with a fixed height keep a half-row strip above and below their rows for the "more" counts, and opening a row in such a card scrolls it into view.
- Navigator: parent rows stay at the top while you scroll, sections list cards, ghosts and then child sections, the change dot on an open row is only for that row itself, and large trees no longer rebuild on every selection.
- Maps with many lines stay fast when you drag cards, click, or come back to a section seen before.
- Updates: choose whether new versions download automatically (off by default), turn periodic checks on or off per source, and find new versions in notifications instead of a window. "Update Now" checks again and takes a newer version if there is one.
- Language: places that showed Japanese with the app in English now follow the app's language, including the menu bar and open and save panels.
- MCP: `kind_list` says what each kind is for (`purpose`) and which kinds only the core creates (`systemOnly`); answers leave out empty and repeated values; partial table cell edits answer with only the rows changed; ghosts can be created with a position; the whole-card key for merge conflicts is `(whole)`.
- Security: unknown or revoked access keys are refused with 401 before the request body is read; the agent has time limits for slow connections; a project name or slug that clashes with one the key cannot see is refused without naming it; update sources read from settings are checked again, and a Forgejo token is only sent over https.
- Fixes: a crash when a picture over MCP was asked for an area far past the map (now refused beyond ±1,000,000 cells); a crash when opening a section whose groups, read from a project file, were too large (they are now brought within range); a crash when the SSH connection to an agent host was lost; a crash when opening a section while a font could not be loaded; Swift files with `"#` inside raw strings losing their functions and highlighting; moving or resizing a card on a branch losing its size; diagrams that used up the agent's memory on densely linked projects.
- Projects are saved in a newer format, and older versions of the app cannot open them. Update the app and its agents together, and reconnect AI sessions to MCP to get the new tools.

## 0.3.2

- Update sources: when you have not chosen one (or the one you chose was removed), the app uses the first source you added, and this repository's releases only when you have added none. Adding a source no longer changes the one in use.
- The Code of Conduct gives an e-mail address for reports: specboard@moapp-lab.org.

## 0.3.0

The first public release of SpecBoard.

- A project is a board of cards: entities, enums and value objects with their fields, lines between cards, flows that describe how a system behaves, documents (formatted, free, stacked), tables, data, and SVG or HTML cards, in sections with tags.
- Branches and merge requests: plan a change on a branch, review the difference, and merge it into main. History, undo, and an inspection that finds broken paths and mismatches.
- A board of goals and tasks to plan the work, linked to the spec.
- Tabs for what you open: the section's map first, then cards, Connections maps and settings, remembered per project.
- MCP for AI agents through `specboard-agent`, installed by the app on macOS (Apple silicon) and Linux (x86_64) hosts over SSH. Access keys choose which projects an agent may read or write.
- Exports to TypeScript, Zod, JSON Schema, Mermaid and Markdown, into the folder given at each export.
- Working directories: link a folder and read its code (21 languages) next to the design.
- English and Japanese.
- Signed with Developer ID and notarized; updates come from this repository's GitHub Releases.
