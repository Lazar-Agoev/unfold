# Visual QA pass (V1–V4, run in parallel)

The screens are built (U1–U6). This pass makes each artboard match its design exactly, in light AND dark, and checks
that every state the artboard draws is reachable in the app with real data.

Read: `brief-common.md` (NO git commands), `brief-screens.md` (ownership rules, `shot.sh`, Paper read-only tools,
gates), `decisions.md` (all; V-n decisions are deliberate deviations — don't undo them), `kit-api.md`,
`core-api.md`, and the specs in `design/` for your artboards.

## Method, per artboard
1. Reach the artboard's state in the app with real data. Seed data deliberately: build boards through the engine
   (`{"invoke": ...}` actions of `screenshot.mjs`, e.g. `new_board`, `save_board` with pages/cards/descriptions,
   `publish_board`, then another `save_board` for unpublished changes); make a description outdated by editing a
   copied playground file after describing it (copies live under `target/visual-<pkg>/`, never edit `testdata/`);
   connect several projects with repeated `--project` (one can be a copy of py-orders whose `[project] name` you
   change so cards become placeholders); use window size `--size 1440x990` for the 990-tall artboards.
2. Screenshot light and dark (`docs/redesign/build/screens/v-<pkg>-<artboard>-<theme>.png`). Look at the Paper
   artboard's screenshot side by side.
3. Compare element by element, not by eye alone: for each region (header, rows, buttons, cards, chips, menus,
   dock, sheets), read `get_computed_styles`/`get_jsx` from Paper and measure the app with `{"log": "...getComputedStyle / getBoundingClientRect..."}`.
   Check: geometry (x, y, width, height, padding, gaps), type (family, size, weight, line-height, letter-spacing,
   color), color tokens (light and dark), borders, radii, shadows, icon size and stroke, alignment lanes.
4. Fix every difference in the files you own. Re-shoot until it matches. Differences that come only from sample data
   (names, counts, how many cards) are fine; differences in layout, type and color are not.
5. Dark: artboard 13 is the only dark artboard; for screens without a dark artboard, the dark theme applies the dark
   tokens (`dark-theme.md`, V-29..V-31, V-40..V-42) — check nothing is unreadable, nothing keeps a light value, and
   contrast holds.

## Also
- Hover, focus and selected states of rows, buttons and tabs look right in both themes.
- Keyboard focus is visible (accent focus ring) and Escape closes every overlay.
- No text clipped, no unintended scrollbars at 1440×900 and 1280×800.

## Report
Per brief-common: for each artboard, the before/after screenshots, what you fixed, and anything that still differs
(with the reason). Gate: `docs/redesign/build/gates.sh js` PASS (re-run once if another package's files break it,
then report).
