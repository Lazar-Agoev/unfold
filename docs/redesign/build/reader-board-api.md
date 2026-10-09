# `unfold_reader::board` API for the board commands (R3 → R5)

What `crates/unfold-reader` offers the workspace's board commands after R3 (contract §6). Everything is
in `unfold_reader::board` (files in `board::files`) and `unfold_reader::mermaid`. Rustdoc on each item is
the reference; this page is the map and the call recipes.

## Types

| Type | What |
| --- | --- |
| `Board { title, folder: Option<String>, projects: Vec<ProjectName>, pages: Vec<Page> }` | The document (§6.2) without `format`/`version` (add them in the IPC wrapper, `FORMAT`/`VERSION`) and without the copy framing. `projects` is derived: `Board::used_projects()`, set by `with_pages`, `parse`, `parse_edit`, `prepare_save`, `refresh_projects()`; the writer always writes the derived set |
| `Page { id, title, default_view, cards, groups, labels: Vec<LabelEntry>, guards: Vec<GuardEntry>, links: Vec<Link> }` | One page. Lookups: `card`, `group`, `link`, `label_entry(project, key)`, `label(project, key)` (text), `guard(project, key)`, `group_project(group)`, `view_of(card)`, `references(project)` |
| `CardRef::Block { block, project } \| CardRef::Entrance { entrance, project }` | Serializes as `{block\|entrance, project?}`. `CardRef::block(key, project)`, `::entrance(key, project)`, `.key()`, `.block_key()`, `.project()` (`None` = the board's own project) |
| `LabelEntry { project, key, label: Description }`, `GuardEntry { project, key, kind, owner, signature }`, `Link { id, from, to, label: Option<Description> }` | Per (project, key) / drawn link (`label` flattened into the entry on the wire) |
| `ProjectName` | Validated shared name (`ProjectName::parse`, `as_str`); TS `string` |
| `Health { items, missing, outdated, broken_groups, review, not_connected, broken_links }` | Page or board health. `HealthItem` has `page`, `kind`, `target`, `project?`, `key?`, `message`, `repairs`. `Health::label()` is `BoardSummary.label` ("1 missing · 2 outdated · 1 not connected · 2 broken links"), `of_page(id)`, `append(other)`, `is_healthy()` |
| `Target::{Card(id), Group(id), Label(ProjectKey), Link(id)}`, `ProjectKey { project?, key }` | Serializes as `{card}`, `{group}`, `{label: {project?, key}}`, `{link}` = contract `TargetSpec` |
| `Repair::{Reconnect {project, from, to}, RemoveCard, RemoveLabel(ProjectKey), Ungroup, RemoveLink, RemoveDescription(Target), Resign(Target), Accept(ProjectKey)}` | What `apply_repair` does; `RepairKind` (camelCase) lists what an item offers |
| `PageOutline { page, rows: Vec<OutlineRow>, unlabeled: Vec<Unlabeled> }` | Contract `PageOutline` |
| `BoardGraph` | One project's slice of the graph (unchanged except `load` takes keys) |
| `PageGraphs<'a>` | `PageGraphs::new(&own, &others: &BTreeMap<ProjectName, BoardGraph>)`, `PageGraphs::own_only(&own)`, `.get(project)`. A project absent from `others` is **not connected** |
| `files::{BoardFile, Place, BoardDirs, Loaded { board, revision, base }, Revision, Change}` | Files; see below |
| `BoardError` | `Syntax`, `Invalid {path, message}`, `Problem {page, target, message}` (gained `page`), `Unverified`, `InvalidName`, `NameTaken`, `Io`. The V1 code mapping still fits |

All payload types derive `ts_rs::TS` under the `ts` feature (as V1).

## Functions

| Function | Use |
| --- | --- |
| `parse(text) -> Board` | A draft or published file (refuses `publishes`/`baseRevision`) |
| `parse_edit(text, folder) -> Board` | A `BoardEdit` (`{title, pages}`; JSON pointers relative to it). Pass the session's folder; `projects` is derived |
| `serialize(&board)`, `serialize_changes(&board, published_name, base: Revision)` | Canonical text of a draft/published file, or of the changes copy of `boards/<published_name>.json` |
| `BoardGraph::load(&reader, root, &board.references(project))` | One project's graph for a board (or `page.references(project)` for one page) |
| `health(&page, &graphs)`, `board_health(&board, &graphs)` | Per page / all pages in page order |
| `prepare_save(board, previous: Option<(&Board, &Health)>, &graphs) -> (Board, Health)` | Board-level save: pages equal to the previous version's keep their health unchecked; every other page is signed (descriptions, labels, groups of connected projects; link labels from both ends' guards), guards kept/added/restored per (project, key), and refused on a **new** missing reference, broken group, held reference, or broken link (never on outdated text or not-connected cards). Refreshes `projects` |
| `apply_repair(&mut page, &graphs, &repair)` | One repair on one page (`board.page_mut(&spec.page)`); then `prepare_save` |
| `outline(&page, &graphs)`, `card_links(&page, &graphs)` | Outline (graph calls, then drawn links); graph follow links between same-project cards (thumbnail `edges`) |
| `changes(&working, &published) -> u32` | Contract §6.6 count (guards not counted); 0 = equal |
| `current_guard(&graph, project, key)` | A fresh guard (rarely needed: `prepare_save`/`Accept` do it) |
| `is_folder_path(path)` | Contract §1.4 folder rule (`invalid_folder`) |
| `mermaid::page(&board, page_id, &graphs) -> Option<String>` | Page export (drawn links dashed with labels, "From <name>, not connected" placeholders) |
| `mermaid::entrance(&graph, key)`, `mermaid::entrance_board(&graph, key)` | Entrance export |

## Files: `BoardDirs`

`BoardDirs::new(workspace.root(), &workspace.layout().drafts())` — published files under the folder
(link-checked), drafts and copies in the layout's drafts directory (fallback location honored).

| Method | Notes |
| --- | --- |
| `load(&file) -> Loaded` | A copy's `publishes` must match its name; `Loaded.base` = its `baseRevision` |
| `change(&file, revision) -> Change` | `Unchanged` / `Changed(rev)` / `Missing` |
| `save_draft(&file, text) -> Revision` | Drafts and copies only (refuses a published file). Creates the drafts dir; the workspace still refuses before the first Compile |
| `create_published(name, text)` | Never overwrites: `NameTaken {path, suggestion}` |
| `overwrite_published(&published, text)` | Atomic replace (or recreate): Publish of a copy, keep mine |
| `remove(&file)` | Gone is fine |
| `list(Place::Published)`, `list(Place::Draft)` | Drafts never include copies |
| `changed()` | Published boards that have a copy (even when the published file is gone) |
| `free_draft(base)` | `base`, `base-2`, …; never a `.changes` name |
| `free_name(name)` | Free published name (what `NameTaken` suggests) |
| `path(&file)` | For messages and mtime (`updatedAtMs`) |

`BoardFile::published(name)`, `BoardFile::draft(name)` (refuses `*.changes`), `BoardFile::changes_of(&published)`,
`file.publishes()` (copy → its published file: map watcher paths `<drafts>/x.changes.json` to the id
`.unfold/boards/x.json`), `file.relative()` (the id for drafts and published files). `files::slug(title)`,
`files::write_atomic` as V1.

## How a session should call it

**Graphs.** For a board of project P (own name `own`):

```rust
let own_graph = BoardGraph::load(&reader_of(P), root_of(P), &board.references(None))?; // BoardGraph::new() before the first Compile
let mut others = BTreeMap::new();
for name in &board.projects {
    if let Some(q) = resolve(name) {   // D-3: first connected project with that name, P included; never wakes q
        others.insert(name.clone(), BoardGraph::load(&reader_of(q), root_of(q), &board.references(Some(name)))?);
    }                                  // unresolved = not connected: placeholder, never an error
}
let graphs = PageGraphs::new(&own_graph, &others);
```

**Open / reload.** Working file = `BoardFile::changes_of(&published)` when it exists, else the published file
(drafts: the draft). `let loaded = dirs.load(&working)?;` then `board_health(&loaded.board, &graphs)` and one
`outline` per page. `BoardState.published` for a published board: `changes(&working, &dirs.load(&published)?.board)`,
`baseChanged` = published revision ≠ `loaded.base`.

**Save.** `let board = parse_edit(&args.board.to_string(), session.folder.clone())?;` Refuse with `invalid_args`
("omit project for the board's own project") when `board.projects.contains(&own)` — the reader does not know
P's name. Build graphs for the *new* board, then
`let (signed, health) = prepare_save(board, Some((&session.board, &session.health)), &graphs)?;` and write:

- draft: `dirs.save_draft(&draft, &serialize(&signed))`;
- published: `n = changes(&signed, &published_board)`; `n == 0` → remove the copy if any (session back on the
  published file); else `dirs.save_draft(&copy, &serialize_changes(&signed, &published.name, base))` with
  `base` = the copy's `Loaded.base`, or for a first copy the published revision the session loaded (the one on
  disk now after `force`).

**Publish.** Draft: `prepare_save` → `dirs.create_published(name.unwrap_or(slug(&title)), &serialize(&signed))`
(`NameTaken` → `name_taken`) → `dirs.remove(&draft)`. Copy: `dirs.change(&published, base)`; `Unchanged` (or
`keep_mine`) → `dirs.overwrite_published(&published, &serialize(&board))` then `dirs.remove(&copy)`;
otherwise `conflict` (`Missing` → `theirsRevision: null`). No copy → `done`.

**Discard / unpublish / delete / duplicate.** Discard: `dirs.remove(&copy)`, reload the published file.
Unpublish: `let draft = dirs.free_draft(&published.name)?; dirs.save_draft(&draft, &serialize(&working))`,
then remove the published file and the copy. Delete: remove the draft, or the published file and its copy.
Duplicate: `free_draft(&format!("board-{ms}"))`, title "<title> copy", same folder.

**Folders.** `is_folder_path` validates. `set_board_folder`/`rename_folder` set `board.folder`; for a published
board that is an unpublished change (write/update the copy as in Save; `changes` counts `folder`).

**Repairs.** `RepairSpec` → `Repair` (the page id selects `board.page_mut(..)`, `invalid_args` when absent),
`apply_repair(page, &graphs, &repair)`, then the Save path.

**Lists, thumbnails, refs.** `dirs.list(..)`, `dirs.changed()`, `changes(..)` for `BoardSummary.changes`,
`health.label()` for `label` (null when resting). Thumbnail: first page, `card_links(page, &graphs)` for
`edges`, `page.links` for `links`, `foreign = card.reference.project().is_some()`, `lines` from
`graph.block(key)`. `board_refs`: a card's project is `card.reference.project()` resolved by name, or the
board's own project when `None`.

**Export.** Page: `mermaid::page(&board, &page_id, &graphs)`. Entrance: `BoardGraph::load(&reader, root, &[key])`
→ `mermaid::entrance_board(&graph, key)` → reload with `board.references(None)` → `mermaid::entrance(&graph, key)`.

## Removed from V1

`files::{save, load, load_path, change, change_path, list, publish, unpublish, rename, free_name, remove}` (now
`BoardDirs` methods or gone: rename and unpublish move to the workspace), `board::{check, sign, describe_card,
describe_group, set_label, Guard, BoardOutline}`, the V1 `Board` shape and its methods, `mermaid::board`.
