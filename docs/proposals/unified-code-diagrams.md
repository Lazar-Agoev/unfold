# Unified code diagrams

Status: revised proposal · 2026-10-11

Product direction: [the fixed Unfold product vision](../product-vision.md).
Design target: Paper, "Lazar's Playground" ›
[Unfold 4](https://app.paper.design/file/01M465BN8EP0ES7QTYPPZ0GFHJ/p-G-0).

Unfold should let a person see how their actual code works, expand the parts they care about,
summarize consecutive steps, and open the source behind each block without leaving the diagram.
People and MCP agents arrange and explain the compiler's blocks, add useful architectural
connections, and clearly mark intended changes as proposals.

The same compiled source and saved diagram choices produce the same code-derived structure;
agent summaries never supply that structure. Existing background updates refresh the real blocks
and their reference health as code changes.

This is a visual layer over the existing compiler, entrance logic, and workspace. The product
vision is fixed; this document specifies the small first implementation and the corresponding
changes to the existing Paper design.

## 1. Scope and existing foundations

### What to change

| Change | Result |
| --- | --- |
| One Diagram canvas | Replace the Diagram/Flow distinction with expansion on individual cards; keep a page-wide Code lens. |
| Expand a call in place | Reveal a resolved callee's diagram inside its caller while retaining its source identity. |
| Combine | Give consecutive steps of one function a concise name using existing step groups. |
| View code | Open the selected card, step, or combined block's actual source in the existing peek sheet, with a precise highlight. |
| Simple proposed steps | Add, edit, and remove visibly proposed behavior at a real insertion point. |
| Matching MCP actions | External coding agents use these same capabilities on a page targeted by its copied link. |

### What to preserve

- Projects, folders, boards, pages, Home, and their existing creation and navigation flows.
- Window chrome, tab strip, sidebar width and section order, dock, and the existing visual style.
- Share, Copy link, Publish, draft handling, undo, agent attribution, and working indicators.
- Frames, resources, notes, drawn links, typed connections, and existing source/evidence links.
- Type cards, ERD tables and relationships, and the current Code and Flow renderers as the basis
  of the new presentation.
- Compiler extraction, resolution, discovery, launches, application membership, and uncertainty.

The sidebar keeps Board, Pages, On this page, and Selected. Its additions are proposal rows and
the controls for the selected step, combined block, or proposal. The top bar changes only its
lens options. Existing Add actions remain the way to add compiled cards and other supported items;
there is no new diagram-creation wizard.

### Entrance logic is an existing contract

The recently merged entrance work is a foundation for these diagrams. Use its existing reader
and command responses; do not replace it with a visual hierarchy or a second reachability model.
The rules live in [derived views](../reader.md#derived-views),
[manual entrances](../engine.md#manual-entrances-and-locators), and
[entrance detectors](../engine.md#entrance-detectors).

- Preserve entrance identity, kind, method/trigger, trigger certainty, declaration evidence,
  and target order. Configured entrances remain supported.
- Preserve the distinction between a launch's **startup** target and an entrance's **handler**
  targets. Use those returned roles for visual wording.
- Preserve resolved, external, unresolved, missing, and source-unavailable states. A target is
  followable only when it is resolved; expansion must not turn uncertainty into a guessed function.
- Preserve application membership, including proven versus declared membership, mounted routes,
  shared entrances, and entrances not reached from a launch. Existing categories and counts stay
  under the current logic.
- Authored frames and architecture connections do not change application membership,
  Called by, Reached from, or compiler arrows.

An entrance's source action opens its registration or startup/configuration evidence. Its target
links open the separate startup or handler code. Handler registration order is not a new inferred
control-flow graph, and static reachability is not live execution tracking.

### Scope limits

No new service hierarchy, runtime telemetry, branch/snapshot manager, in-app assistant, agent
review queue, source inspector, cross-function Combine model, or proposal-completion workflow.
There is no automatic ERD generation or new table/column editor in this proposal. Separate pages
can continue to explain different systems or versions using the existing workspace model.
Proposed additions cover function steps only; code sketches, replacement/removal proposals,
and proposed architecture connections are outside this implementation.

## 2. One Diagram canvas

The page lens becomes **Diagram (⌘1) | Code (⌘2)**. Diagram is the default for new pages.
Code retains the current source-oriented view
and each card kind's existing behavior there.

In Diagram, a function card has one expansion chevron instead of Diagram | Flow | Code buttons:

- **Collapsed:** the existing compact diagram body, name, location, and description.
- **Expanded:** the existing Flow body, with written steps, branch labels, decisions, start/end
  markers, labels, and combined blocks. Keep "Paths from written structure" where it is used today.

Preserve per-kind bodies rather than making every card a compact function node:

| Card kind | Diagram presentation |
| --- | --- |
| Function/block | Compact summary or expanded written flow. |
| Entrance | Compact entrance identity or expanded declaration evidence and ordered target rows. |
| Interface, alias, enum | Existing fields, written type, or values and their row links. |
| Class | Existing fields and supported member/detail presentation. |
| Resource/table | Existing resource body, columns, key marks, and relationships. |

Type and resource cards do not lose their fields when the lens changes. Offer expansion only
where the existing kind has meaningful additional detail. Existing header-only folding remains
available through its current actions; do not add a second set of bulk Expand/Collapse controls.

Function and entrance expansion reuse the card's saved `view`: `diagram` is compact, `flow`
is expanded, and a legacy `code` card appears expanded in Diagram. For a card without its own
view, use its original page default to determine that detail. The page lens remains existing
local view state, separate from shared card presentation. Remove per-card lens exceptions.
Opening Code and returning to Diagram restores the same expansions, selection, and camera.

Accept old boards and tool inputs: a saved Flow lens maps to Diagram; cards inheriting a Flow
default remain expanded. A saved Code lens/default still opens Code. Do not rewrite a board
merely on read. Update menus, shortcuts, guides, and page checks to use the new public terminology.

Compiler arrows retain their meaning: compact cards attach them to the card edge, and expanded
cards attach them to the actual call or target row. Expansion changes presentation, never graph facts.

## 3. Expand a call in place

A resolved call step or callback chip in an expanded function gets an expansion chevron.
Expanding reveals the callee's written flow in a lightweight inset below the call, with its
function name and `file:line`. Keep the call visible and avoid repeating a full card header,
description, and toolbar at every nesting level.

The existing `→ callee` link and callback-follow action still add/open a separate callee card.
The inset title opens the callee's source; View code on the calling step opens the caller's
call expression. Selection and source inspection always carry the actual owner and project.

A one-call callback may expand directly to its proven callee, provided the caption retains the
callback and wrapper context. For example, `withRetry(() => sendEmail(job.data), 3)` can show
sendEmail's steps below the retry step. It must still show that this is through a callback;
the display must not invent a direct synchronous edge or erase the wrapper.

- Save expanded call occurrences on the containing card using actual occurrence keys, and the
  displayed expansion path where needed. Several calls to the same function remain distinct.
- Allow up to three inline levels. Stop recursion at a repeated ancestor and retain its follow link.
- External and unresolved targets retain their current explanations without an expansion control.
- Route step connections through their real owner/occurrence, including steps displayed inside an inset.
- Keep the Code lens's current inline-source interaction; synchronizing it with saved Diagram
  expansions is outside this implementation.

Labels already refer to source keys on the page and can appear in either presentation. A callee's
combined blocks come from its own card on that page, when present; they do not become groups owned
by the caller. To Combine inside an inline callee, follow it to its separate card and Combine
there. Do not create hidden owner cards or a new inline composition model.

## 4. Combine consecutive steps

**Combine** is the public name for creating an existing step group. It summarizes what a run
of code does while retaining the original steps and source.

1. In an expanded function, select the first step and Shift-select the last consecutive step.
2. Choose **Combine…** from the selection's context menu.
3. Reuse the existing group naming dialog with the members preselected and a required concise
   title. An optional description can be edited in Selected.
4. Show the new group collapsed as one block, with its title, step count, expansion chevron,
   and View code action. Expanding reveals the original steps in order.

The selection and MCP `group_steps` must use the same existing outline validation: whole
consecutive sibling statements of one owner, within one parent/branch. Reject gaps, overlaps,
nested groups, function headers, nested definitions, and cross-function selections using the
existing actionable reason. An entrance's target list is not a set of combinable function steps.

Keep source-line grouping as another entry to the same operation. Selected lets a person edit
the title/description, View code, and Ungroup. Ungroup restores the underlying steps; it never
changes code. Existing groups keep their saved collapsed state, authorship, signatures, and guards.

A combined block may contain a complete control statement when existing validation permits it.
It must preserve its branches and terminal behavior when expanded, and existing return/throw
indicators when collapsed. Connections to contained steps attach to the summary while collapsed
and to those same real steps when expanded. Combine never invents or merges compiler edges.

## 5. View the actual code

Use the existing source peek beside the diagram. Preserve its chrome, pin, folds, source
actions, and close behavior; add precise range highlighting and following of diagram selection.

| Selected item | Source target |
| --- | --- |
| Function card | Its definition. |
| Real step | Its condition, call expression, or statement, using recorded source ranges. |
| Combined block | The span of its actual member statements, in its owning function. |
| Call step with an inline expansion | The caller's call expression. |
| Inline region title or an inline callee step | The callee's definition or that step's own range. |
| Entrance | Its declaration, registration, or launch/configuration evidence. |
| Resolved entrance target | That startup or handler's definition, separately from the entrance evidence. |
| Type/table/resource | Its existing definition or cited source anchors, where available. |
| Proposed step | **View placement code** opens the real anchor; the proposal has no implementation source. |

Show a small `</>` action on a hovered or selected source-backed step and on combined blocks.
Keep the card's existing source action, and make the location in Selected a link to the same
source. Register `C` for View code through the current action/shortcut system, active only on
canvas selection and never while typing. Proposal wording must distinguish placement from implementation.

The peek highlights the relevant lines and precise expression where available. Selecting
another real step updates the file and highlight without losing the diagram's selection,
expansion, or camera. For combined blocks, selection stays on the group rather than jumping
to an arbitrary child.

Expose focus ranges in the reader's Flow/card response and MCP `read_code`, using existing
recorded condition/call ranges and otherwise the statement range. Preserve UTF-8 graph ranges
and the current UTF-16 rendering conversion. This is a reader/presentation change, not new
compiler inference. Source must pass the existing facts-hash verification before it is shown;
changed or missing source uses the existing unavailable state, never an old highlight on
unverified current disk text. Retain configuration evidence text and pointers where supplied.

## 6. Architectural connections and ERD remain part of the diagram

Not every useful system connection can be proven from a call graph. A person or MCP agent
can still add backend → queue → worker or code → database connections using the existing
resource and typed-connection model. These are intentional explanations, with attribution
and source/evidence links under the current rules in [grounding](../reader.md#grounding).
An agent can cite the producer, channel, and worker evidence without pretending the compiler
proved an end-to-end edge.

Retain the visual distinction between compiler arrows and authored connections. Do not demand
a newly inferred compiler edge for every architecture connection or relax the existing
grounding rules to draw contradictory calls. Frames remain authored organization and add no
new service semantics.

### Preserve current ERD support

ERD is supported through the existing table resource and connection logic, even without
automatic ERD extraction. Keep:

- Type-backed columns from written fields, including supported inherited fields, and tables
  with their own authored columns and cited schema/model evidence.
- Primary, foreign, and unique key marks; column endpoints; cardinality/crow's-foot relationships.
- Reads and writes connections between code steps and tables, and source links on both.
- Existing layout, folding, descriptions, link actions, authorship, and reference health.
- MCP table/column authoring through `add_resources` and relationships through `connect`, as
  documented in [data models](../../desktop/src/main/mcp/guide/data-models.md).

The current UI has no table/column authoring form. Preserve the current MCP authoring and
manual layout/description/connection actions; do not add that form here. Changing the lens or
expanding nearby code must not turn a table into a compact function node, drop columns, lose
cardinality, or duplicate existing compiled type arrows with authored relationships.

## 7. Simple proposed steps

A proposed step is a clearly marked intended addition: a title, optional reason, author, and
where it would go. It is not a compiler fact or an automatic implementation plan.

### Manual and MCP interaction

- Select a real step on its owning function card and choose **Add proposed step after…**.
  Expand a combined block first to choose its actual anchor. For an inline callee, follow its
  separate card before adding a proposal there.
- If the selected decision has several outgoing paths, choose one of its actual path labels.
  Do not offer insertion after a terminal return/throw with no continuation.
- Create the proposal in place and edit its title and optional **Why** in Selected. Selected
  also shows attribution, a **Where** link to the placement code, Copy link, and Remove proposal.
- List it under its owning card in On this page; a compact card can show a proposal count.
- People and agents can edit or remove it through the normal diagram actions, undo, and MCP.
  Existing protection for a person's text applies to agent edits.

Use a dashed green outline and a visible **PROPOSED** type row, with the existing success
palette and readable tint. The title is explanatory text, not fabricated source. Any connectors
showing proposed placement are dashed green and visually separate from compiled arrows.
The current compiled path must remain inspectable; adding a proposal never replaces the
underlying code path or makes it appear implemented.

### Persistence and changes to code

Store proposals as an optional page collection, omitted when empty. Each holds an id, owning
card, real anchor key, optional branch/path, title, optional reason, and author. Protect the
anchor's identity through the existing project/guard mechanisms. Proposed words do not claim
that current code implements them and do not need a fictional implementation signature.

If the anchor disappears or becomes held, retain the text and show the existing missing/held
reference treatment with a placement explanation. The person or agent can explicitly choose
a valid new anchor or remove the proposal. Never silently reattach by a similar name or shifted
ordinal key. Reuse existing health/repair infrastructure, without creating proposal statuses.

There is no Would call binding, Mark built, automatic completion detection, conversion into
a label, or proposal status machine. When someone implements a change, they can remove the
proposal and label/combine the new real steps with the ordinary actions. Proposal removal
does not edit code.

Proposals follow the board's existing draft/Publish rules. Page images include their visible
proposed styling. Mermaid export omits proposed steps; do not redesign exports in this work.

## 8. External agent workflow

The person creates a page, uses its existing **Copy link**, and pastes the link into Claude Code
or Codex: "Use this page to show how my email service works." The agent uses MCP from that
coding environment. There is no in-app assistant or new setup flow.

1. Resolve the copied page link with the current project/board/page targeting and read that page.
   The pasted target takes precedence over whichever page happens to be active.
2. Search/read compiled entrances, functions, types, and their source. Add the actual cards.
3. Expand relevant cards/calls; label and Combine their real steps through existing operations.
4. Add useful frames, resources, ERD tables, and architectural connections with current evidence rules.
5. Arrange and check the page, then finish through the existing agent working/attribution flow.
6. Add proposed steps when the person requests intended changes. Explaining existing code does
   not automatically add proposals or invent missing behavior.

### Bounded tool changes

| Tool/area | Required change |
| --- | --- |
| `set_view` | Expose Diagram/Code page presentation, per-card expansion, and expanded call-occurrence keys; retain compatible legacy inputs and existing fold/group/frame controls. |
| `group_steps` | Reuse unchanged grouping semantics for Combine; new concise summaries can request `collapsed: true`. |
| `read_code` | Expose real step focus ranges while retaining existing source-state and entrance evidence/target data. |
| New `propose` operation | Create/update proposed steps by owning card and guarded insertion point, including explicit placement repair. |
| `remove_items` | Remove proposals along with its existing supported items. |
| Page/current-view reads | Include saved expansions, proposal items, and their selection/placement information. |
| Guides, prompts, checks | Teach expand/Combine/View code, optional proposed changes, copied-link targeting, and preserved ERD. Count expanded function detail where checks currently count Flow. |

Use the existing holding-window edit protocol, validation, undo, attribution, and text-replacement
rules. Exact request fields belong to the implementation's current schemas; extend them rather
than introducing a parallel diagram document, tool suite, or save lifecycle.

## 9. Grounded example for the design

Use the real [orders playground](../../testdata/playgrounds/ts-orders/src) to explain current
behavior, while clearly separating authored wording and connections:

1. The HTTP entrance opens its route registration evidence. Follow its registered handlers to
   [`createOrder`](../../testdata/playgrounds/ts-orders/src/routes/orders.ts). Its condition is
   `req.header("prefer") === "respond-async"`: label it **Async response requested?** Yes queues
   the job and responds 202; no places the order, posts to Slack, and responds 201. Replace the
   old mockup's misleading "Client asked to wait?" label without changing the compiled branches.
2. An authored connection explains the orders queue and its worker.
   [`processOrder`](../../testdata/playgrounds/ts-orders/src/jobs/process-order.ts) validates,
   places the order, and queues a confirmation. The diagram keeps each real call inspectable.
3. The [email worker](../../testdata/playgrounds/ts-orders/src/queues/email-worker.ts) calls
   `withRetry` with a callback to
   [`sendEmail`](../../testdata/playgrounds/ts-orders/src/jobs/send-email.ts). Expand that
   callback in place, preserving the wrapper context.
4. On sendEmail's owning card, Combine its consecutive simulated-delay and logging statements
   into **Simulate delivery**. The inline rendition can reuse that group. Expanding it reveals
   those two statements; the recipient check and outbox mutations remain separate real steps.
5. If asked to suggest a change, add **Email a confirmation** after placement on createOrder's
   direct path, with a reason explaining the current difference from the queued path. Keep
   that intention visibly separate from the currently compiled behavior.

This fixture stores orders in a Map and the outbox in a Set. Do not present either as a
production database or reinterpret the outbox as a broker. Use the existing grounded table
support for an ERD example, and distinguish real persistence evidence from authored conceptual
relationships. Key/cardinality marks are authored metadata unless established by the cited schema.
If the ERD mockup needs illustrative data absent from this fixture, identify it as an authored
schema example; do not claim it is the playground's extracted database.

## 10. Paper design brief

**Edit the existing Unfold 4 page in place.** It already demonstrates the intended canvas,
peek sheet, and existing app chrome. Reuse those screens and add only the missing interaction
states; a blank redesign would reopen settled workspace decisions.

Before editing, read this proposal, the fixed vision, and the relevant current entrance/type/table
references. The previous Paper drawings are a starting point; current code/reference semantics
take precedence over outdated mockup details. Keep screen annotations below the app window.
Show the final usable interface, with authored, compiled, and proposed content distinguishable
on the real screens rather than adding a separate provenance dashboard.

| Artboard | Required design changes |
| --- | --- |
| **00 · What changes, what stays** | Update the scope to expansion, Combine, source inspection, simple proposals, and matching MCP. Show entrances and ERD as preserved foundations. Remove automatic completion/lifecycle claims. |
| **01 · Expand in place** | Keep the existing orders canvas and chrome. Show Diagram/Code plus one card expansion control. Correct the preference decision to Async response requested? with the branches in §9. Include compact and expanded entrance states using registration evidence and actual ordered target roles; keep type/resource bodies intact. |
| **02 · View code on a step** | Keep the diagram beside the existing peek. Show the selected condition's precise highlight, its `</>` action and Selected source link. Add a small entrance-evidence versus resolved-handler-source state so those actions are clearly different. |
| **03 · Expand a call in place** | Keep the worker/queue example. Simplify nested chrome into lightweight callee insets; retain function/file identity and retry/callback context. Preserve follow-to-separate-card and correct source actions. |
| **04 · A proposed step** | Show manual Add proposed step after, branch placement where relevant, and the green PROPOSED node. Selected contains title, optional Why, author, Where, Copy link, and Remove. Remove Status, Would call, Mark built, and conversion/review lifecycle explanations. Keep the real compiled path visible. |
| **05 · Ask your agent** | Begin with the existing Copy link and paste into an external coding agent. Target that page, use compiled blocks and MCP, and show existing working marks. A proposal is an optional requested change, not the default final step. |
| **06 · Combine** — add | Show consecutive-step selection → concise naming → collapsed combined block, plus its expanded members, View code, and Ungroup. Use the real Simulate delivery statements and identify their owning function. |
| **07 · Existing ERD support** — add | Show the current table/type rendering, columns, PK/FK/unique marks, a column relationship with cardinality, and a code reads/writes connection. Mark authored relationships and show cited-source inspection. Show preserved MCP authoring; add no table editor or automatic-generation control. |

Small alternate states may sit beside/below their parent artboard rather than becoming new
product screens. The home/project UX, style system, and app chrome need no redesign.
Keep bulk management, lifecycle controls, and speculative future features out of the drawings.

## 11. Implementation boundaries and validation

This section directs the later implementation session; no application code is changed by this document.

| Owning area | Work |
| --- | --- |
| [Canvas cards](../../ui/src/canvas/cards), [Flow](../../ui/src/flow), [board selection](../../ui/src/board/selected) | Expansion controls, inline callees, step selection/Combine, proposal presentation, and source actions. Preserve per-kind rendering and entrance target ports/roles. |
| [Source peek](../../ui/src/peek), [board state](../../ui/src/state/boards) | Focus highlighting, saved Diagram expansions, proposal edits, and existing view/undo behavior. Reuse group and reference validation. |
| [Reader Flow](../../crates/unfold-reader/src/flow), [board contracts](../../crates/unfold-reader/src/board) | Expose recorded focus ranges; add optional expansion/proposal metadata with bounded payloads and existing guard/source rules. |
| [Workspace commands](../../crates/unfold-workspace/src/commands), [MCP](../../desktop/src/main/mcp) | Carry the affected responses/edits, compatible tool inputs, read targeting, guide/check updates, and proposal operations through existing adapters. |
| Top bar and existing action/menu registration | Two lens options, canvas-only View code shortcut, and Combine/proposal actions. |

Do not change engine detectors, launch parsing, resolution, application projection, database
schema, workspace organization, ERD contracts, or publishing ownership. No new cache, composition,
or versioning subsystem is needed. Generated IPC types must follow the existing generation path.
Shared board metadata additions must round-trip old documents and be omitted when empty;
saved view compatibility must be deliberate rather than a blanket card-kind conversion.

Use the focused checks appropriate to these later changes:

- Reader/board checks for correct focus ranges, owner/project identity, guards, and old/new
  board round trips. Non-ASCII source must use the existing byte/render conversion correctly.
- UI/agent checks for Combine validity, inline limits/recursion, view restoration, proposals
  and removal/repair, and unavailable-source behavior.
- Verify route and launch entrance expansion preserves evidence, target order/roles, uncertain
  states, and membership. The visual change must not recompute entrance facts.
- Verify type/table rows, column endpoints, cardinality, reads/writes, and existing MCP table
  creation survive lens/expansion changes.
- Use a matching integrated journey only if required by the changed interaction, under the
  [testing selection policy](../testing.md#choose-checks-for-a-local-change). Documentation/Paper
  work alone needs no app build.

### Acceptance criteria

- A person manually builds a page from actual code, follows an entrance to its handlers,
  expands a resolved call, and understands the written flow on one canvas.
- Combine creates one concise block from valid consecutive steps, expands back to the original
  steps, and opens their verified source in one action.
- View code on a condition, call, inline callee, or entrance opens the correct owner's source
  or evidence while retaining the diagram. Uncertainty never turns into fabricated source.
- People and agents can retain useful architectural connections that the compiler cannot
  infer, without changing proven calls or entrance/application logic.
- Existing ERD diagrams and MCP authoring remain usable with their fields and relationships intact.
- A proposed step is visibly future behavior and can be added, edited, removed, or explicitly
  repositioned without any completion lifecycle or rewriting the real code path.
- A copied page link lets an external MCP agent build that targeted diagram from compiled blocks,
  preserving the person's work and the current workspace/agent workflow.

The remaining design choices are spacing, exact placement of compact controls, and inset styling
within the current app style. They must satisfy these interactions without adding product scope.
