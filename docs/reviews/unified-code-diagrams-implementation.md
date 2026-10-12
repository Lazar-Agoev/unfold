# Unified code diagrams implementation review

Reviewed on 2026-10-12 against the [fixed product vision](../product-vision.md) and
[unified code diagrams proposal](../proposals/unified-code-diagrams.md).

The original review identified seven functional defects and one test correction. Follow-up
verification on 2026-10-12 confirms that the fixes address those findings, with no remaining
blockers found in the reviewed scope. Sections 1 through 8 retain the original reproductions
and required behavior for reference; they describe the defects before the fixes.

Verification after the fixes:

- All 219 focused UI tests and 62 desktop tests pass, including the previously failing type-card test.
- All 122 reader tests pass; both UI/desktop typechecks and the generated IPC check pass.
- The existing Electron diagrams journey passes on outputs newer than the affected sources.
- Two additional Electron checks pass: inserting ten lines and recompiling moves the selected
  condition's source highlight from line 27 to 37; following a call inside an inline callee
  produces the compiler arrow from that inset without adding its parent as a separate card.
- Temporary readiness probes were removed after verification.

This verifies the agreed diagram workflow and the reported fixes. It does not constitute a full
repository audit or packaged release validation.

P1 means a core code-understanding workflow gives misleading or incomplete information. P2 means
a supported interaction or contract needs a bounded correction. These fixes fit the agreed scope;
they do not require another diagram workflow, proposal lifecycle, or compiler inference system.

## 1 P1 Source highlights become wrong after recompilation

Select the condition in `createOrder` on the orders playground and press C. Its source highlight
starts on line 27. Insert ten blank lines at the beginning of the file and wait for recompilation.
Selected correctly reports the condition at line 37, and the peek displays the refreshed source,
but the highlight remains on line 27 over unrelated code.

[useFollowSelection](../../ui/src/board/viewCode.ts) reacts to the selection and page target, but
does not react to updated card models. Its resolver reads the query cache without subscribing to
the relevant data. [PeekSheet](../../ui/src/peek/PeekSheet.tsx) accepts the saved focus whenever the
newly fetched file is verified; it does not establish that the focus came from those same facts.
Consequently, verifying the new text does not make the old range correct.

Re-resolve the selected item's focus when its compiled model changes, and withhold a focus until
it matches the source being displayed. Preserve the existing source verification and selection
behavior. The regression check should move the selected statement during a real compile and
assert that the highlight moves with it.

## 2 P1 Expanded callees lose their outgoing connections

Trace POST `/orders`, expand `OrderService.place` inside `createOrder`, and follow the inset's
proven `validateOrder` call to its own card. There is no compiler arrow from that inline call to
the new card. The inset has no connection handles. The separate `validateBody` middleware can
still connect to `validateOrder`, which makes checking only for any incoming arrow insufficient.

[useBoardArrows](../../ui/src/canvas/useBoardArrows.ts) gathers models for the page's standalone
cards. [CallInsets](../../ui/src/canvas/cards/CallInsets.tsx) renders nested Flow without ports,
and [flowPortNodes](../../ui/src/flow/FlowView.tsx) searches only the supplied layout's own nodes.
The inset's displayed steps therefore cannot supply or receive their required step connections.

Include rendered inline owners and occurrences in the existing arrow and port routing. Repeated
calls to the same function must retain distinct displayed occurrences and project identity. Route
only proven compiler links and existing authored connections; a callback inset must not invent
a direct synchronous call. Test the inline source specifically, including an authored connection
to an inline step and a followed callee that has no separate parent card.

## 3 P2 Class cards lose their supported detail and Code views

[cardBody](../../ui/src/code/typeKinds.ts) now returns `fields` for every type kind, including
classes, in both Flow and Code. [CardNode](../../ui/src/canvas/cards/CardNode.tsx) consequently
always renders a class as a TypeCard. MCP [set_view](../../ui/src/state/boards/agent/layout.ts)
also classifies classes as inert type cards and rejects expansion and legacy Flow/Code inputs.

This contradicts the proposal's requirement to preserve classes' supported member/detail
presentation. The module's existing description also distinguishes a class's fields in Diagram
from its methods in Flow. A probe confirmed that a class requested in either Flow or Code is
forced into fields.

Restore the distinction between declarations that only have fields and classes with existing
detail/source presentation. Preserve the two public lenses and existing compatible inputs.
Verify a saved class card with legacy views as well as the equivalent MCP operation; this does
not call for building a new class explorer.

## 4 P2 Invalid proposal placements can be saved as healthy

Using the actual compiled orders graph, the Rust board save path accepted each of these new
proposals and returned healthy board status:

- A step after a `return`.
- A step after a decision without choosing a branch.
- A step on a decision's nonexistent `each` branch.
- A call-occurrence key instead of a selectable Flow step.

[placement_problem](../../crates/unfold-reader/src/board/check.rs) checks only that the anchor
exists under the card's key prefix. It does not check the actual Flow owner, terminal behavior,
or branch/path. [same_place](../../crates/unfold-reader/src/board/save.rs) also omits the proposal's
path when deciding whether a placement changed, so a branch-only edit bypasses placement checking.
The UI/MCP checks are stronger, but cannot protect the persisted board contract or Rust repairs.

Apply the same insertion rules at the reader/save/repair boundary: an actual step owned by the
card, a permitted nonterminal insertion, and a valid path where required. Include path changes
in placement comparison. Keep existing missing or held proposals readable and repairable through
the current health machinery. Check both newly saved invalid placements and a path-only edit.

## 5 P2 MCP can quote the wrong source expression

For a callback written as `const run = () => sendEmail(job.data);`, the callback's CodeText begins
at `() =>`, partway through the file's line. Focus columns are correctly measured from the start
of the full file line. MCP [fileLines and focusText](../../desktop/src/main/mcp/tools/code.ts)
apply those absolute columns to the truncated block slice, however. A focused `read_code` test
reported the expression as `b.data)` instead of `sendEmail(job.data)`.

The reader's [line contract](../../crates/unfold-reader/src/code/lines.rs) explicitly allows
partial first lines. Compensate for the block slice's origin using existing source metadata, or
omit the quotation when the available text cannot support it. Keep the authoritative focus
coordinates. Test a callback or method beginning partway through a line, including non-ASCII
text before the expression.

## 6 P2 Canvas and MCP disagree about recursive callback expansion

For `f() { once(() => f()) }`, the canvas allows the callback to expand to its own written flow,
then stops before expanding `f` again. MCP rejects that same callback path outright as recursion.

[planInsets](../../ui/src/flow/insets.ts) falls back to the callback when a one-call callback's
callee is already an ancestor. MCP [leadOf and checkCallPath](../../ui/src/state/boards/agent/expansions.ts)
always replace that callback with its callee before checking ancestry. A probe using the same
root and callback models confirmed this disagreement.

Share the fallback and recursion rule between the two paths. The callback's own flow can remain
available while the repeated ancestor stays unexpanded and followable. Verify that a valid
manual expansion can be requested through MCP and that the three-level limit still holds.

## 7 P2 Combine visually moves a proposal past later statements

Create a proposal after `createOrder`'s order-placement statement. Combine that statement,
the following Slack announcement, and the reply into one collapsed block named
`Place order and reply 201`. The proposal still stores its original anchor, but appears after
the entire combined block, with no indication that its true placement is inside it.

[planProposals](../../ui/src/flow/layout.ts) maps every hidden member's proposal to the collapsed
group's `after` slot, losing the visible distinction between an internal insertion and an
insertion after the final member. A layout probe confirmed the proposal sits below the whole
group and carries no placement note. The existing layout test explicitly expects this behavior.

Keep the original anchor and show its location honestly. A small explanation that the insertion
is inside the combined block, with an action to reveal its members, is sufficient; alternatively,
show the members while presenting the placement. Avoid drawing the insertion as though it occurs
after later code. Test both an internal anchor and an anchor genuinely after the group's final
member, without replacing the real compiled path.

## 8 The affected UI test gate currently fails

[DiagramView.test.tsx](../../ui/src/board/DiagramView.test.tsx), in the type-card field/navigation
test, queries the CartItem buttons before ReactFlow makes the card accessible. It consistently
failed in the focused suite and in an isolated rerun. The buttons exist under an initially hidden
ancestor. A temporary copy that waits for their visibility before clicking passes.

Correct the visibility wait while retaining the actual navigation assertion. This observation
does not establish that the production type link is broken, but the affected suite needs to pass
before completion can be claimed.

## Validation and scope of fixes

The existing focused UI suite had **198 passing tests and one failure across 16 files**. Desktop
MCP tests passed **61 tests across five files**. UI and desktop typechecks passed, the complete
`unfold-reader` crate tests passed, and `cargo xtask gen-ipc --check` passed. The existing diagrams
Electron journey passed; additional app reproductions confirmed findings 1 and 2. Focused
diagnostic probes confirmed the remaining findings and were removed after the review.

Use these reproductions as regression checks for the fixes. Preserve entrance evidence, ordered
targets and roles, application membership, uncertainty, and the current ERD contracts. Keep the
work confined to the affected presentation, source focus, proposal validation, and matching MCP
behavior. No production fixes were applied during this review.
