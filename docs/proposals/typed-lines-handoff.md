# Typed lines: handoff

Status of the typed-lines build on branch `branch-workshop-798907a4`
(worktree `~/Desktop/repos/worktrees/unfold/branch-workshop-798907a4`). The spec is
[typed-lines.md](typed-lines.md); its "Decisions made here" (§9, items 1–30) records every call made so
far. Delete this file in the final commit.

## The task

Board lines between cards show the types flowing through each call: inputs, return, and throws. A
label sits at rest on the line, and clicking it opens the call's contract in place. Written types are
clickable and open their definitions inline. The design is final in Paper file
`01M465BN8EP0ES7QTYPPZ0GFHJ`, page "Upfold App 2", on these artboards:

- "Typed lines · board"
- "Typed lines · type opened"
- "Typed lines · states and sources"

Read exact values with Paper's `get_jsx` / `get_computed_styles`, never from screenshots. §1 of the
spec tables them.

## Where it stands

| Part | State |
| --- | --- |
| Compiler, store, reader, commands (commit `2e8da93`) | Done and verified: `just check`, `just test` 820/0, `cargo xtask gen-ipc --check`, the four parity commands, and the oracle check on in-repo corpora (precision 100%, recall unchanged) |
| UI, first pass | Built and verified before the last fixes started: `just js-check` (UI 670, desktop 101), `just e2e` 43/43 including `e2e/journeys/typed-lines.spec.ts`, `just check`, `just test` |
| UI review fixes | **In progress, committed as WIP: `pnpm --filter @unfold/ui typecheck` fails** (see below) |

The app matches Paper well in light and dark: the at-rest label, the contract (Sends / Returns /
Throws, with "catch" when the call sits in a caught `try`), the inline `interface Cart` definition with
nested clickable types, the "Used in 6 places" popover, the callee-card marks, and the caller's
selected line.

## What's left

Three fixes from my review of the running app, plus finishing the one that was interrupted.

1. **Labels never cover a card.** This is the one that was in progress. After Arrange, cards sit close
   together, and labels drawn over them hid card titles:
   - TS: `(CartItem[]) → Promise<Hold>` hid the `reserveInventory` card's title and its whole line.
   - Python: `(str, retries=int) → Gateway` covered `Gateway.__init__`, and
     `(object, order=str) → None` covered `log`.

   The interrupted design:
   - `ui/src/canvas/lines/placeLabels.ts`: pure and deterministic. It tries points along the curve
     (t = 0.5, then 0.4/0.6, 0.3/0.7, …), then small steps above and below into free canvas, and
     takes the first spot that meets no card box and no placed label. If none fits, it tries shorter
     forms (`(+2) → Gateway`, then `→ Gateway`). If even those don't fit, the label is not drawn at
     rest; the line takes the click itself, and its aria name stays reachable.
   - `labelLayout.ts`: new; a per-canvas context that collects curves and card rects and hands back
     placements.
   - `labelText.ts`: gains `labelForms`.
   - `LineLabel.tsx`: now takes `curve` and `placement` instead of `x`/`y`.

   **Still to do:**
   - Wire `ArrowEdge.tsx`. At about line 58 it still passes `x`/`y` to `LineLabel`; that's the
     typecheck error. It needs the curve's control points (match `getBezierPath`'s curvature), the
     `LabelLayoutContext` provider in `BoardCanvas.tsx` with card rects from React Flow's nodes,
     `useLabelPlacement`, and a click target on the line when the label isn't drawn.
   - Unit tests in `placeLabels.test.ts`: a short gap between cards, two stacked cards, crossing
     lines, and determinism.
   - An e2e assertion in the typed-lines journey: after Arrange, no visible label's box intersects
     any card's box.

2. **Definition members align.** In `ui/src/code/TypeDefinition.tsx` and `types.css`, the type column
   must line up across rows. Make the name column max(84px, widest name in that definition), and draw
   `readonly` as a muted prefix inside the name cell. Today `readonly createdAt Date` pushes its type
   out of line.

3. **Contract placement.** In `LineContract.tsx`, when the contract opens, prefer a spot that covers
   neither the caller's selected line nor the callee's marked lines: shift along or below the line.
   Fall back to centering on the label, as it does now.

For each fix, record the decision in the spec's §9 (items 31+) and keep `docs/ui-shell.md` ("Lines
between cards") current.

## Known limitations already accepted (keep them in the final report)

- `fastapi-template` isn't available locally, so the Python builtin-exception change is unchecked
  against it.
- `@types/node` global aliases aren't in the alias table, so they are `unknown_name`.
- Not linked: TS `extends`, `implements`, `as`, `satisfies`, and call type arguments; Python
  `type X = …` values.
- External type names show their package on hover/focus, not on click.
- Merged TS interfaces resolve as `ambiguous`.
- No labels on entrance-to-handler arrows, and no marks on Flow cards.

## Ground rules (from the user)

- Don't stop to ask questions: make the call and note it in the spec.
- Work through one Opus implementation sub-agent and verify its work yourself. Sub-agents never run
  git. You may commit at verified checkpoints. No pushes or PRs.
- Commit messages end with `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>`.
- Use only the worktree's `target/` (no other `CARGO_TARGET_DIR`), and delete any extra target dirs
  at the end.
- The store is a cache: no database migrations. `SCHEMA_VERSION` is already 2.
- Docs describe today and what's proposed: no release versions, no migration plans.
- Run `source "$HOME/.cargo/env"` before cargo or just.

## Verify

```sh
source "$HOME/.cargo/env"
pnpm --filter @unfold/ui typecheck
just js-check          # typecheck, lint, unit tests
just e2e               # full suite (builds the release addon first)
just check && just test
cargo xtask gen-ipc --check
```

Then drive the built app on `testdata/fixtures/typed-lines/` and compare with the Paper artboards.
Playwright is the easiest way:

1. Put a temporary spec in `e2e/journeys/`. Don't add it while `just e2e` is running.
2. Use `sandbox.fixture("typed-lines/ts", …)` for TypeScript and `sandbox.fixture("typed-lines", …)`
   for both projects.
3. Add `createOrder` / `create_order`, follow the calls (`follow(card, "reserveInventory")`; the link
   buttons are named `Follow <text>`), press Arrange, and save `boardView(page).screenshot({ path })`.
   Capture the board, a clicked label (the contract), a clicked `Cart` (the definition), "Used in",
   and dark mode (`data-theme="dark"`).
4. Run it with `pnpm --filter @unfold/e2e exec playwright test --project=journeys journeys/<file>`,
   then delete the spec.

Also check the Python contract: `gateway.charge(cart, customer, currency="eur")` should list
`CardDeclined` under Throws.

## Finish

1. Commit the verified UI checkpoint, and delete this handoff file in that commit.
2. Confirm that the only Cargo target dir is the worktree's `target/` (and no `fuzz/target`).
3. Give the user a short report: what was built, the gates and their results, screenshots, and
   anything deferred.
