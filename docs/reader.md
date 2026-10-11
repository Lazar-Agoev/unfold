# Reader and boards

Read this for derived graph views, Code/Flow/types, board persistence, health, or repairs.
Start at [reader lib.rs](../crates/unfold-reader/src/lib.rs). Exact board shapes are owned by
[board/model.rs](../crates/unfold-reader/src/board/model.rs) and the
[board JSON Schema](schemas/unfold-board.schema.json); engine command shapes belong to [IPC](ipc.md).

## Derived views

| View | Rule / code owner |
| --- | --- |
| Owner | Nearest activating file/class/function body context; `index.rs` and `file_owners` |
| Definition catalog | Display names and selectable definitions, not every syntax block; `catalog.rs` |
| Closure | Conservative static reachability from seeds; `closure.rs`, `reader_index.rs` |
| Applications | Distinct resolved process startup targets and entrance membership; `applications.rs` |
| Relations | Callers/reached-from and outgoing relationships; `relations.rs` |

Occurrences own edges; definitions own activation. Python class headers and nested function
bodies require the existing owner rules. Reachability is a lower bound, not a runtime order
or a promise that every dependency is known. External/unresolved relationships remain visible.

`project(&graph)` computes definitions/applications/memberships for the store after updates.
`ReaderCache` holds an index for one committed graph version and caches closures by seed.
Per-file view requests read stored facts and verified source; do not parse source in the reader
or leak a whole graph into a UI response.

## Code view

[code/](../crates/unfold-reader/src/code/mod.rs) builds rendered source, outlines, folds,
groups, highlighting, and occurrence links. [source.rs](../crates/unfold-reader/src/source.rs)
checks bytes against the facts hash before returning code. Changed/unavailable source gives
an explicit state, never current disk text labeled as old facts.

Render offsets are UTF-16, converted from graph byte ranges. A card uses the exact verified
slice; outlines/folds derive from written structure. Do not fabricate code with AST printing.
Highlighting comes through the language's `Highlighter` trait.

Links occur at written call/new/import ranges. Resolved links point to their permitted target;
external/unresolved links retain module/reason text. Keep occurrence identity distinct from
its target, especially when several calls reach the same card. Pagination and file limits
belong to command implementations.

## Typed lines

[contracts/](../crates/unfold-reader/src/contracts/mod.rs) derives line labels, call contracts,
type definitions, uses, and direct throws from stored written records and verified bytes.
It adds presentation, not inferred types or new argument-binding evidence.

- A line label shows written parameter/return types, with bounded text and declared/external/
  builtin/unresolved type spans. The selected line can open its detailed contract.
- A call contract uses the occurrence's proven argument pairings and bound count. Keep spread,
  missing signature, unknown receiver, and excess arguments explicit. Report unfilled
  parameters only when ambiguous arguments cannot have filled them; omit bound/`this` takers.
- Direct throws are owned construction throws outside a catching try body. Nested callables,
  rethrows, arbitrary thrown variables, and transitive callee exceptions are not added.
- Type definitions show written fields, aliases, and inherited groups. Uses come from the
  store's reverse type-link index; order and paginate them by source location.

### Inherited members

Follow only `extends`, not `implements`, depth-first in written order. Visit each base once,
stop cycles and the configured depth bound, and do not traverse past external/unresolved
bases. Display generic members as written (`T` remains `T`); preserve overridden/repeated
written fields rather than synthesizing a checker-computed shape.

## Flow view

[flow/](../crates/unfold-reader/src/flow/mod.rs) renders written control structure and calls:
branches, loops, exceptions, returns, and owned steps. Preserve source order, branch labels,
and step grouping; it does not infer runtime execution paths. Layout is TypeScript's job.
Groups must refer to permitted ordered steps of one owner, not to arbitrary overlapping ranges.

## Boards

A board is an `unfold-board` v1 JSON document containing titled pages, optional folder/project
references, and publishing metadata. A page contains cards, groups, labels, drawn links or
connections, frames, resources, notes, and guards. Exact fields, ID syntax, enum values,
limits, omission rules, and parsing belong to the schema and board modules, not this guide.

Cards reference a block/type or entrance. Missing references remain placeholders. Description
and label text records its author and, where source-backed, its signature. Page-level edits
share one undo history for the board; changing a view/lens should not invent a content edit.

### Cards from other projects

A reference without `project` belongs to the board's project. Named references resolve through
the board's project declarations to connected shared project names. Never treat an unknown
project as the current project or silently change a card's origin. Saved views are local;
shared references and explanations belong to the board.

### Signatures

Sign only verified bytes: card/guard source or entrance evidence; group member span; label
block; resource type/anchors; connection step/anchors; frame member-card signatures. The
board implementation owns exact concatenation and ordering.

Changed bytes or failed verification make source-backed text/claims outdated. Pure drawn-link
labels describe their ends rather than a code assertion and are not reported outdated; claims
without code have the specific unsigned rules implemented in `board/save.rs` and `claims.rs`.
Saving must not mark old explanations reviewed just because new bytes became available.

### Guards and reconnecting

A guard remembers kind, owner, and signature for each used `(project, key)`. After updates:

- A surviving compatible key reconnects; changed source can make its text outdated.
- A missing key remains a placeholder.
- Changed kind/owner or changed source behind an ordinal key holds the reference for review.
- Reconnect repairs update uses throughout the page within the original project; a card's
  connections carry supported relative steps to the new key with fresh guards.

Do not automatically describe held references or silently reattach them by similar names.
Evidence-file line anchors use input hashes and line existence rather than structural guards;
reconnect them explicitly to new lines or keys.

### Grounding

[board/claims.rs](../crates/unfold-reader/src/board/claims.rs) validates new or changed claims
against their referenced graph and verified source. Positions alone do not change a claim.

- Type-backed tables mark actual fields, including supported inherited fields; relations use
  existing columns. Agent resource names/columns must occur in cited bytes.
- Agent resources other than actors need anchors. Agent HTTP/RPC/queue/event/calls connections
  need supported evidence; person-authored claims have their documented allowances.
- Agent channels must be grounded in the cited code, or derived from a target entrance.
  Carried types must be used in the step's owner or cited block.
- An agent `calls` connection explains a call/new with an unresolved or external occurrence;
  do not overlay a contradictory invented target on a resolved call.
- Claims involving disconnected projects cannot be grounded. Refuse them as `board_problem`
  with an actionable reason, without recording a partial edit or undo step.

Agent-authored text remains visibly attributed. Re-sign only the permitted agent work;
overwriting a person's text requires the explicit replacement option. Preserve unchanged text
and authorship. Agent edits run through the holding window as one undo step; see [agent IPC](ipc.md#agents).

### Health and repairs

[board/check.rs](../crates/unfold-reader/src/board/check.rs) reports missing, held, outdated,
disconnected, and invalid references/claims. Outline and repair actions use the same project
and source rules. A repair is explicit Review/Reconnect/remove work, not silent content cleanup.
Refresh health after graph/project changes and keep it distinct from persisted board edits.

### Files, drafts, Publish, and unpublished changes

[board/files.rs](../crates/unfold-reader/src/board/files.rs), workspace board commands, and
UI board state own the lifecycle:

- New boards are local drafts. Publish writes a shared file under `.unfold/boards/`.
- Edits to published boards are local unpublished changes until Publish. Discard reloads
  the published version; unpublish moves the board back to drafts under its rules.
- External file changes must not overwrite local dirty work. Surface conflicts and use
  explicit keep/reload choices. Failed saves preserve work and offer retry.
- A board is held by one window; its session/undo state moves through the claim rules.
  Quit/detach must allow closing writes to finish before releasing the claim.
- Folder names are board metadata, not a parallel disk hierarchy. Shared project references,
  signatures, and guards survive publish and round trips.

## Mermaid export

[mermaid.rs](../crates/unfold-reader/src/mermaid.rs) exports a page, flow, or entrance using
verified graph content and board health. Preserve labels, placeholders, and uncertainty;
export never resolves missing links by guessing. Frozen `.mmd` fixtures check exact output,
and the Mermaid parser check verifies syntax. PNG/page checks use the desktop render window.

## Checks

Start with local reader/board tests, matching store or workspace command tests when persistence
changes, and the affected schema/fixture check. Changes to Publish/restore/holding-window
behavior may need a focused journey under [testing's selection policy](testing.md#choose-checks-for-a-local-change).
