# Rules for the screen packages U2–U5 (run in parallel)

Read, in order: `brief-common.md` (hard rules: NO git commands of any kind), `ui-plan.md` (all of it; your package's
rows in §1, §7.2, §7.3, §10.2, and the frozen §10.3 interfaces), `decisions.md` (all V-n, A-n), `kit-api.md` (U1a
components — use them; don't edit `components/`), `core-api.md` (U1b state/ipc/keyboard — frozen; use it as is),
`contract.md` §4–§6 for the commands you call, and the design specs in `design/` for your artboards (exact values).
V1's UI is at `docs/redesign/build/v1-ui/src` (read-only) for porting logic worth keeping.

## Ownership
Edit only the paths your package owns (ui-plan §10.2). The frame (`app/`), kit (`theme/`, `icons/`, `components/`,
`lib/`) and core (`ipc/`, `keyboard/`, `state/` except what your row grants) are frozen. If you need something
from a frozen module, build it inside your package and say so in your report; if a frozen API is genuinely wrong,
stop and report it instead of editing it. Your package's entry file (stubbed by U1c with the §10.3 signature) is
yours to fill in; keep the signature.

## Done means
1. Every artboard your package owns is implemented with the design's exact values (sizes, spacing, colors via the
   theme tokens, type, radii, shadows), in light and dark, with all states the spec lists, and every row of the
   redesign §6 table assigned to your package (ui-plan §7.3) has a working home.
2. Behaviour is wired to the real engine commands (no mock data), with loading/empty/error states.
3. Efficient: virtualize long lists, memoize, no per-row IPC calls, invalidation via the core's project events.
4. Unit tests for your components and logic.
5. **Visual check.** Build and screenshot your screens in light and dark with
   `docs/redesign/build/shot.sh <screenshot.mjs options>` (it serializes builds/launches across parallel agents; see the
   option list at the top of `screenshot.mjs`; use `--project testdata/playgrounds/ts-orders` and/or `py-orders`,
   and `--actions` to reach the state). Save PNGs to `docs/redesign/build/screens/<pkg>-<screen>-<theme>.png`. Compare
   each with its artboard: read the artboard with read-only Paper tools (ToolSearch
   "select:mcp__paper__get_guide,mcp__paper__get_screenshot,mcp__paper__get_jsx,mcp__paper__get_computed_styles,mcp__paper__get_tree_summary";
   `get_guide({topic:"paper-mcp-instructions"})` first; fileId `01M465BN8EP0ES7QTYPPZ0GFHJ`; NEVER call a mutating
   Paper tool) and with the `{"log": ...}` action to measure geometry in the app. Fix differences. The artboards'
   sample data (storefront-api…) is illustrative; layout, type and color must match, content comes from testdata.
6. Gates: `docs/redesign/build/gates.sh js` must PASS. Other packages edit other folders at the same time; if the
   gate fails only because of files you don't own, re-run once after a minute, then report the failing files
   instead of editing them. Also run eslint and vitest scoped to your folders.
7. No dead code in your folders: delete V1 leftovers in your scope (e.g. U2 removes `canvas/BoardTab`/`BoardBar` V1
   chrome and `lucide-react`), no unused exports, no unused CSS rules.

## Report
Per brief-common, plus: the screenshots you took and what still differs from the artboard (with reason), every
decision you made where the specs are silent (append them to `decisions.md` under a heading for your package as
V-n numbers starting at the next free number — read the file right before appending, others append too), and
anything you needed from a frozen module.
