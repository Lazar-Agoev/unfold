# Build decisions (where the doc and designs are silent or disagree)

Each line: the choice, and why. V-n are visual/design resolutions; the contract's own choices are D-n in contract.md §10; process choices are P-n. These are folded into the decision log (U-entries) at the end.

## Design resolutions

- **V-1 Text search options in Add.** Shown as three toggle buttons (Match case `Aa`, Whole word `ab`, Regex `.*`) at
  the right end of Add's input row, only while the Text tab is active. R-13 and artboard 00 promise them; no artboard
  draws them; this is the least-intrusive placement consistent with the input row.
- **V-2 Look in.** Only on the map (before typing), as drawn. Typing searches every connected project; each result row
  carries its project monogram.
- **V-3 Kind tabs.** Map: Entrances, Functions, Files, Text without counts. Search: All plus each kind with counts. As drawn.
- **V-4 Match highlighting.** As drawn: text matches get the tinted bold run; command names bold the match; entrance,
  function and file names show no highlight.
- **V-5 On-page mark.** On map chips (as drawn) and in the hover popover ("Already on <board> › <page>"). Search rows: none.
- **V-6 Preview code.** Uses the reader's syntax colors (the preview renders the same Code model the cards use); the
  artboard's plain text is a drawing shortcut.
- **V-7 Publish wording.** One command "Publish" (it never closes the board); "Publish changes and keep editing" is not a
  separate command. "Move back to drafts" everywhere.
- **V-8 Hover.** Rows that the designs only draw selected get a hover fill one step lighter than selection
  (a chrome token `--color-hover`).
- **V-9 Overlay placement.** Add is window-level (centered on the window, scrim over tabs and sidebar), as drawn.
  Board-level dialogs (Publish, Discard, Delete, Rename) are centered on the canvas with a canvas-only scrim, as drawn.
  Sheets (peek, project settings) slide over the canvas. Preferences is window-level.
- **V-10 Publish variants.** The plain first-publish dialog, Discard changes confirmation and Move to folder submenu,
  which aren't drawn, reuse the drawn dialog and menu styles.
- **V-11 Destructive red.** One red for chrome: `#DC2626` (Paper `--color-failure`) for Delete and the DELETE method; the
  off-token `#B42318` is not used.
- **V-12 Unpublished-changes dot.** Indigo (accent), as drawn: it marks board state, not a warning.
- **V-13 Secondary text.** As drawn, `#656974` (the code's `--color-muted`); the doc's §7 mention of `--color-subtle`
  describes the contrast floor, not the secondary-text color.
- **V-14 Untokened values.** The designs' monogram colors, selection tints, scrim (ink 18%), floating shadows, radii
  5/8/12px and 16px input text become named chrome tokens in `ui/src/theme/`, with dark values.
- **V-15 Branch and worktree switcher.** Not drawn: a popover in the drawn menu style listing the worktrees (path,
  branch, current one checked) with the footnote "Drafts and the graph are per worktree." (R-4).
- **V-16 Project settings.** As drawn: an 880×700 dialog centered over a window scrim (not a sliding sheet). General,
  Detectors and Python projects are built in the drawn Scope section's style; the switch is a small accent toggle.
- **V-17 Tooltips.** One small tooltip component in the popover style (problem count on the amber dot, fallback location
  and scanning notes on Home's state line).
- **V-18 Project row menu.** `⋯` appears in the row's trailing slot on hover and while the row is selected; the state
  indicator keeps its own slot. The full project menu is shown in every state, with inapplicable items disabled.
- **V-19 Entrances per application.** As the doc says: the first eight, then "N more" and a filter. Routes are ordered by
  first path segment (no group headers), as drawn.
- **V-20 Resting project.** The moon glyph with an undimmed name, as drawn (the glyph already says it).
- **V-21 Opened before.** First launch lists up to five folders that were removed from the workspace ("Opened before",
  as drawn), kept in `workspace.json`; clicking one adds it back.
- **V-22 Icons.** One custom 16px icon set (the one most artboards draw); the Lucide-style icons of 08 and 11 are redrawn
  in it.
- **V-23 Check boxes.** 16px everywhere (06's box); folder paths in mono.
- **V-24 Monogram.** One fill `#E6E8EC` / letter `#3F4451` (tabs included).
- **V-25 State line.** "Graph current · N files · <languages> · updated <when>"; branch names in mono; the problem phrase
  comes from the actual counts. Every link in it 6px after the text, weight 500.
- **V-26 Health | Activity.** A segmented switch showing one list at a time.
- **V-27 Board review items** are amber everywhere.
- **V-28 Chrome colors.** The chrome uses the designs' values (amber `#D97706`, green `#16A34A`, red `#DC2626`, and the
  untokened greys) as named chrome tokens with dark values; the reader's and cards' tokens are unchanged.
- **V-29 Surfaces per theme.** As drawn: light sidebar = canvas `#F7F8FA` with a white top bar and tab strip
  `#F0F1F4`; dark sidebar and top bar `#191B1F` over a `#151619` canvas. Chrome tokens carry both.
- **V-30 Dark values.** Artboard 13's values are the dark chrome tokens (designed, not inverted); primary buttons in dark
  are light indigo `#A0AAFF` with dark text. Code's old dark chrome values are replaced.
- **V-31 Hairlines.** Light `#E0E2E7` everywhere (as drawn; the doc's `#E2E8F0` loses). Dark: shell edges `#2B2E34`,
  controls, dock and cards `#353840`.
- **V-32 Active tab** joins the top bar in both themes (no rule under it), as dark draws it.
- **V-33 Tab close.** `×` in a fixed slot on the active and hovered tab; no new-tab button (New board is ⌘N).
- **V-34 Cards restyled as drawn:** lighter border, no shadow, selected = 1.5px accent border + 12% accent ring, 20px
  code rows at 12px, full-width review banner under the header, entrance card heading "Handlers, in order" without the
  accent border, gray entrance→handler arrow. Card internals (render models) are unchanged.
- **V-35 Placeholder card copy.** As drawn: "From <project> · not connected", "<author> added this card. Connect the
  project to read it.", button "Connect project".
- **V-36 On this page across projects.** Depth indentation is kept; a project-initial column appears only when the page
  holds cards from more than one project.
- **V-37 Calls-only card in 10** is a drawing shortcut, not a new card mode.
- **V-38 Peek sheet and floating chrome.** The lens and the dock stay visible and center on the uncovered canvas while a
  sheet is open.
- **V-39 Small mismatches.** Amber `#D97706` everywhere; the Selected label column is 88px for cards and links; `⋯` is
  the filled-dots glyph everywhere; a plain "Published" state uses the drawn state text without the dot.

## Contract amendments (orchestrator review)

- **A-1 Opened before.** `workspace.json` gains `"removed": [{path, name, removedAtMs}]` (≤ 5, most recent first; a
  path leaves the list when added again). `WorkspaceView` gains `removed: {path: string; name: string}[]`. New engine
  command `readd_project {path}` (untrusted, L —): accepts only a path currently in `removed` (else `not_found`), then
  behaves like `add_project`. Implements V-21 without letting a window name arbitrary folders.

## Process

- **P-1 No U0 bridge.** Step 2 does not adapt the V1 shell to the new IPC: the V1 UI is deleted and rebuilt by the
  shell step. Rust and desktop gates pass at the end of steps 2–3; `just js-check` for `ui/` is red from the moment
  the generated IPC changes until the new shell lands (step 4), and green at every step after that.

## UI plan decisions

From [ui-plan.md](ui-plan.md), where the doc, the contract, and the designs are silent.

- **V-40 Dark token scope.** Artboard 13's values apply to every token 13 draws: chrome, card shell, authored text,
  review, step rows, line numbers, arrows. Syntax and Flow node tokens (and `--color-failure`, `--color-decision`) keep
  their current values (V-28). External call names get `--color-syntax-external` (`#DB2777`, dark `#D59BE8`) as drawn.
- **V-41 Hover.** `--color-hover` is an indigo tint one step lighter than selection: light `#F3F5FD`, dark `#22263A`.
- **V-42 Home surface.** Home's main area is `--color-home`: white in light (as drawn), the canvas tone `#151619` in
  dark, so the dark sidebar stays one tone above it as on the board.
- **V-43 One menu style.** Every menu and submenu: radius 10, padding 4, 28px items, 9px separators in
  `--color-border`, highlight `--color-selection`, danger label `--color-danger`. Widths: board 220, project 264,
  peek 208, share 196. Items show their shortcut when the action has one.
- **V-44 Tabs.** A tab is as wide as its label up to 220px (ellipsis), shrinking to 96px; past that the strip
  scrolls horizontally with the active tab kept in view. Middle-click closes; the context menu has Close, Close other
  tabs, Open in new window. ⌘W on Home closes the window when Home is the only tab, and does nothing otherwise.
- **V-45 No footer in board mode.** The board sidebar has no Add project/gear footer (as drawn); Preferences stay on
  ⌘, and the app menu.
- **V-46 One mounted board.** Only the active board tab is mounted; camera, sizes, and selection live in the store,
  so switching back restores it without re-measuring.
- **V-47 Home views.** Home's main area shows one of: overview, health, activity, recents, drafts; remembered per
  window. Health and Activity follow V-26 (one list at a time, no activity preview under Health); "Show all" is
  dropped, Copy diagnostics sits in the Activity header.
- **V-48 Applications on Home.** Cards wrap (`flex: 1 1 280px`); the application with the most entrances grows 1.4.
  "N more · Filter" expands that card in place: a filter field and every entrance (≤ 500, scrolling inside the card,
  max 420px). The two collapsed sections expand into the same entrance rows; launches show their command and path.
- **V-49 Boards on Home.** Filter pills All/Published/Drafts; sort Last edited or Title; a responsive grid
  (`minmax(220px, 1fr)`, four columns at full width); the right-click menu is the board menu; empty state "No boards
  yet" with New board.
- **V-50 Thumbnails follow the theme.** Drawn with theme tokens (card fill `--color-surface`, strokes
  `--color-border`, accent line `--color-accent`), not light in dark; geometry from `Thumbnail` with the canvas's
  fallback sizes.
- **V-51 Sidebar tree.** Folders first, then boards, each by name (case-insensitive, natural order). Folder menu: New
  board here, New folder…, Rename…, Delete (only when empty). No drag and drop; Move to folder is the board menu's
  submenu. Expanded folders are remembered per window.
- **V-52 Project rows.** A click selects the project and shows its Home overview (activating the Home tab); the
  chevron only expands or collapses; a board row opens its board.
- **V-53 Page operations.** A page row's menu: Rename (also double-click), Duplicate page, Move up, Move down,
  Delete page… (disabled for the only page). New pages are titled "Page N".
- **V-54 Undo per board.** One history per board across its pages (≤ 200 steps, moves with Open in new window); each
  step remembers the page it changed, and undo or redo shows that page.
- **V-55 Lens per page.** The top bar lens is the active page's `BoardView` lens (saved in `state.json`), shown as
  `lens ?? defaultView`; ⌘1/⌘2 set it; it never goes into `windows.json`. A card's own switch while the lens is set is
  an exception, as in V1.
- **V-56 `card` without `board`.** The UI always calls `card {project, key}` and applies a page's Flow labels itself
  (V1 rule), so one card query serves every page. The contract's optional `board` argument is then unused (flag for
  the contract owner).
- **V-57 Card widths.** Block cards size to content between 280 and 880px; entrance cards between 200 and 320px
  (V-34's narrower cards). Layout constants and tests follow.
- **V-58 Drawn links.** Created from a card's menu (Draw link…): the next click on another card of the page makes
  the link, Escape cancels; Selected then focuses the link's Label (optional). Edit label in Selected or by
  double-clicking the chip; Remove link in Selected or the link's menu. Any two cards may be linked.
- **V-59 Link rows.** In On this page a link row follows its `from` card's row (after that card's group rows), one
  level deeper; an unlabeled link reads "link to <card>".
- **V-60 Selected.** Card: name, location, description, then Called by and Reached from (up to 3 each, then "N more ·
  Show callers"), Not opened (up to 5 calls not on the page, each "+ Open"), and the card's groups. Flow node: label,
  the code name it replaces, its paths. Group: title, description, Ungroup. Link: as drawn in 10. The section takes at
  most half the sidebar's height and scrolls; collapsed state is remembered per window.
- **V-61 Placeholder author.** "<author> added this card" uses the card's description author; without one the line
  reads "This card is from <project>. Connect the project to read it." Connect project opens the Add project picker.
- **V-62 Same-name projects.** A card from another project whose shared name equals the board's own project's name
  can't be added (the row is disabled with "Rename one of the projects in its settings"), since such a ref can't be
  saved.
- **V-63 Add behaviour.** Query debounce 120 ms; name search `limit` 50 per kind on All, 500 on a kind tab; text
  search starts at 2 characters; All shows up to 4 rows per kind, then "N more <kind>" switches to that tab; preview
  after 150 ms (files: first 12 lines; text: ±5 lines around the match). From Home, Enter (and Trace) make a new draft
  in the result's own project. ⌘N makes a draft in the selected project on Home, in the active board's project in a
  board tab.
- **V-64 Add map.** 12 entrances per application, then a "+N more" chip that expands the lane (≤ 500). Look in counts
  sum the map's totals (no waking). Before typing, the Functions, Files and Text tabs show a one-line hint. The hover
  popover appears after 300 ms on hover or keyboard focus.
- **V-65 Recents and Drafts.** Their sidebar rows open the list in Home's main area as a board grid with thumbnails
  (each board's project monogram on its card); the window's selected project (and its liveness) is unchanged. The
  Drafts count shows "500+" at the cap.
- **V-66 Peek sheet scope.** One sheet per window, kept across tab switches; it covers Home's main area too. On Home,
  `⊕` makes a new draft in the file's project. Opened from Add, Health's Open, a card's Open source, and a card
  header's location. Escape closes it, pinned or not.
- **V-67 Toasts.** Bottom right, 24px from the edges, 10px apart, newest at the bottom, at most 4. Info toasts hide
  after 5 s, errors after 8 s; the engine toast stays until reconnected; × shows on hover. "<project> compiled" shows
  only in the window that started the Compile, and only when its Home isn't showing that project.
- **V-68 Progress.** The row's progress ring rotates (indeterminate); Home's state line gives "done of total files ·
  stage" from `compile_progress`.
- **V-69 First Compile scope.** Check box changes on the not-compiled Home are kept in memory and written with
  `update_project_config` just before `compile` (nothing is written before Compile). `compile_preview`'s problems show
  as Health rows between Skipped and the scope tree.
- **V-70 Settings scope tree.** Levels load lazily from `scope_dir`. A folder is mixed when a saved or staged exclude
  pattern lies under it. The count column shows discovered source files ignoring scope (the contract has no included
  count; flag). Rules show patterns exactly as Rust writes them (the folder path); staged removals are struck through.
- **V-71 Settings General.** The name is staged with Save; the background-updates switch applies at once (it isn't in
  `config.toml`); storage notes come from `project_summary.status`. Closing with staged edits asks "Discard changes?".
- **V-72 Publish variants.** First publish of a draft: "Publish <board>", the file name field (slug, edited on
  `name_taken`), the destination line, no list. A published board with changes: the drawn dialog with "N changes" as
  a count (the contract returns only `changes: number`, so no per-change list; flag). Conflict: as drawn; choosing
  "Take theirs and drop my changes" relabels the button "Discard my changes" in the danger style.
- **V-73 Confirmations.** Discard changes and Delete ask first in a 400px canvas dialog; Move back to drafts doesn't.
- **V-74 Rename inline.** Board titles rename in place in the sidebar header (double-click or Rename…); page titles in
  the Pages row. Enter commits, Escape or an empty title reverts.
- **V-75 Keyboard details.** `[ ] = - ,` match by physical key when Shift changes the character; `Ctrl+Tab` works on
  macOS; every §8 action is rebindable, Preferences included.
- **V-76 Remove and switch.** Remove from workspace and Switch worktree first close this window's tabs of the project
  (saving them); `boards_open` from another window shows "A board of <project> is open in another window."
- **V-77 Health actions.** Open shows the file in the peek sheet at its line; Settings opens Project settings on the
  section the problem is about (Python projects for Python version errors, Scope for scope errors, else General);
  Review opens the board on the page of its first item with that card selected.
- **V-78 Engine toast.** "Reconnecting to the engine…" / "Your boards stay open. Edits are saved once it's back.",
  with the progress ring; dismissed after the reconnect sequence finishes.
- **V-79 Window layout.** `windows.json` `layout` holds sidebar width and visibility, the Selected collapse, the peek
  pin, Home's view, boards filter and sort, and expanded sidebar folders; each tab records its active page.
- **V-80 Worktree switcher.** `git_info` is fetched when the popover opens (never cached); rows show branch (mono),
  path (muted), a check on the current one, unavailable worktrees disabled; the V-15 footnote closes the list.

## Kit decisions (U1a review)

- **V-81 Contrast exceptions.** The design's amber review dot on the sidebar (2.98:1) and on selection (2.80:1), the
  unchecked box border in dark (2.74:1), and the shortcut hint on primary buttons (3.95:1) keep the design's values:
  each is a mark or a secondary hint repeated by text elsewhere, not body text. `--color-faint` is never text (§7), so
  its pairs are tested only as marks. The contrast test encodes exactly these exceptions.
