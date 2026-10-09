# Model and decisions (artboard v2 · 14)

Transcription of the Paper artboard `v2 · 14 · Model and decisions` (file *Lazar's Playground*, page **Unfold App**,
1440 × 1826). It is a **reference board**, not a screen. The copy below is verbatim; differences from
[the redesign doc](../../studio-redesign.md) §2 (model), §3 (principles), §9 (decisions), and §12 are listed at the
end. **Where the two disagree, the doc is newer and wins** (it says Unfold has not shipped, so there is no migration).

## 1. The model

- Eyebrow: `The model`
- Headline: `Projects hold code. Boards hold stories.`

Six columns, left to right (name · description · storage):

| Thing | Description | Stored (mono line) |
| --- | --- | --- |
| Workspace | Everything you've connected on this machine. One per person. Home lives here. | `app data · workspace.json` |
| Project | A folder with its own graph, usually a repository. Shows its branch and switches worktrees in place. | `the folder · .unfold/` |
| Folder | Yours to organize boards inside a project, like folders in Paper. Not the disk layout. | `a field on each board` |
| Board | A story about the code, such as "Order lifecycle". A draft or published. Can hold cards from other projects. | `boards/ or local/drafts/` |
| Page | One canvas inside a board: one trace, such as "Placing an order". What V1 calls a board. | `inside the board file` |
| Card | An entrance or a function, in Code or Flow. Unchanged from V1. | `inside a page` |

## 2. Principles

1. **One sidebar, and it follows you.** On Home it holds projects and their boards. Inside a board it holds pages, the
   cards on the page, and the selected card. Never two at once, and no right-hand inspector.
2. **State lives on the thing it describes.** Graph state on the project, health on the card, publish state on the
   board. No status bar.
3. **Finding code is adding it.** One Add finds entrances, functions, files, and text in every connected project and
   puts what you pick on a page. The same box runs commands.
4. **Home answers "how does it start?"** A project opens on its entrances, grouped by application, and then the
   boards people made about it.
5. **A trace doesn't stop at a repository.** A page can show the web app's checkout handler next to the API route it
   calls.
6. **Nothing covers cards for long.** The dock and the lens float at the edges. Sheets and dialogs cover the canvas
   only while you use them, and Escape always closes them.
7. **The engine guarantees don't change.** Unfold reads files only. It never runs the code it reads, installs nothing,
   and writes only inside .unfold/ and its own app data.

## 3. Before implementation

Intro: `Each of these changes the contract an implementation session builds against, so it needs an answer first.`

| # | ID | Question |
| --- | --- | --- |
| 1 | R-1 | What a story is called. |
| 2 | R-2 | One window shows the whole workspace, and commands name their project. |
| 3 | R-6 | Unpublished changes on published boards. |
| 4 | R-7 | Whether the first release has cross-project boards, and with drawn links. |
| 5 | R-4 | Worktrees: one row with a switcher, or a row each. |
| 6 | | A yes, or changes, to the proposed defaults (R-3, R-5, R-8 to R-16) and to the designs in Paper. |
| 7 | | Scope of the first release: all of the build order at once, or steps 2–7 first, with cross-project boards (step 8) after. |

## 4. Decisions

Title: `Decisions` · `16 decisions · 5 need your call · 11 proposed`

Note (right): `Proposed defaults are what the designs already follow. They need a yes, or a change.`

| ID | Decision | Recommendation | Status |
| --- | --- | --- | --- |
| R-1 | What a story is called | Board, holding pages. | Needs your call |
| R-2 | How a window relates to projects | A window shows the whole workspace; commands name their project. | Needs your call |
| R-3 | What a project is, and its shared name | A folder with its own .unfold/; [project] name in config.toml. | Proposed |
| R-4 | Branches and worktrees | Read .git files, never run Git; one project row with a worktree switcher. | Needs your call |
| R-5 | The board format | unfold-board v2 with pages, a folder, and project-qualified refs; v1 migrates on open. | Proposed |
| R-6 | Edits to a published board | Kept as unpublished changes until Publish. | Needs your call |
| R-7 | Boards that span projects | Allowed, with drawn links; automatic links later. | Needs your call |
| R-8 | Which projects are watched | Projects with an open tab or selected on Home; the rest rest and catch up when opened. | Proposed |
| R-9 | Where workspace state is stored | App data workspace.json and windows.json; per-project state stays in .unfold/local/. | Proposed |
| R-10 | Folders | A folder field on the board; empty folders kept in .unfold/local/state.json. | Proposed |
| R-11 | Where the selected card's details go | The sidebar's Selected section; no right inspector. | Proposed |
| R-12 | Whole source files | A peek sheet that can be pinned. | Proposed |
| R-13 | Finding and commands | One Add box, with > for commands. | Proposed |
| R-14 | Drafts | Stored per project, listed together in the sidebar. | Proposed |
| R-15 | Can anything cover the canvas | Floating dock and lens; sheets and dialogs only while open. | Proposed |
| R-16 | Which V1 decisions change | Supersede U-26; amend U-16, U-29, U-30, U-37. | Proposed |

## 5. Board styling (for reference only)

Not built. Artboard 1440px wide, height fit-content, padding 72px / 80px, flex column, gap 56px, fill `#FFFFFF`.

- Eyebrow Inter 11px / 14px weight 600, `0.12em`, uppercase, `#5862C0`. Headline Inter 36px / 40px (`--text-2xl`,
  `--leading-2xl`) weight 800 (`--font-weight-heavy`), `-0.03em` (`--tracking-tight`), `#202126`.
- Model columns: top border 1px `#202126`; columns separated by 1px `#E0E2E7`; padding 16px top, 16px inline, 4px
  bottom; gap 6px. Name Inter 15px / 18px 700; description Inter 13px / 19px `#656974`; storage JetBrains Mono
  11px / 16px `#656974`, 6px above.
- Principles column (flex-grow 1.2) and "Before implementation" panel (flex-grow 1, fill `#F7F8FA`, radius 12px,
  padding 24px, gap 14px), gap 64px between. Principle number Inter 15px / 18px weight 800 `#5862C0` in a 24px column;
  title Inter 15px / 18px 600 `#202126`; body Inter 13px / 20px `#656974`. Question rows padding-block 10px, gap 12px,
  dividers `#E0E2E7`: number JetBrains Mono 12px / 20px `#656974` (20px wide), ID JetBrains Mono 12px / 20px 600
  `#202126` (44px wide), text Inter 13px / 20px `#202126`.
- Decisions table: columns 48px / 300px / flex / 130px, gap 32px; head row caps labels with a 1px `#202126` bottom
  border; rows padding-block 12px with `#E0E2E7` dividers. ID JetBrains Mono 12px / 20px 600; decision Inter 13px /
  20px 600; recommendation Inter 13px / 20px.
- Status pills (Inter 11px / 14px 600, radius 999px, padding 3px 8px):
  - **Needs your call:** fill `#FFF3D6` (`--color-reader-light-warn-fill`), 6px dot `#D97706` (`--color-decision`) with
    6px gap, text `#8A5A00` (`--color-reader-light-warn`).
  - **Proposed:** fill `#F1F2F4` (no token), text `#3F4451` (no token).

## 6. Differences from the redesign doc

### §2 The model

The six things and their descriptions match the doc word for word. The storage column is abbreviated on the board:

| Thing | Board | Doc |
| --- | --- | --- |
| Workspace | `app data · workspace.json` | App data: `workspace.json` |
| Project | `the folder · .unfold/` | The folder itself, with its `.unfold/` |
| Board | `boards/ or local/drafts/` | `.unfold/boards/` (published) or `.unfold/local/drafts/` |

The doc also explains why "board" and not "file" (R-1); the board doesn't.

### §3 Principles

All seven principles match the doc word for word.

### §9 Decisions

Titles, statuses, and the "5 need your call · 11 proposed" split match. Two recommendations **conflict** with the
doc; the doc wins:

| ID | Board | Doc §9 | Resolution |
| --- | --- | --- | --- |
| **R-5** | `unfold-board v2 with pages, a folder, and project-qualified refs; v1 migrates on open.` | "`unfold-board` with pages, a folder, and project-qualified refs; replaces the V1 format, no migration" (and R-5's text: "one supported `version`, no reader for the old shape, and no migration") | **No migration, no v1 reader.** The board is stale |
| **R-16** | `Supersede U-26; amend U-16, U-29, U-30, U-37.` | "Revise U-26, U-16, U-29, U-30, U-37 in place" ("revised in place, since nothing has shipped") | **Revise all five in place**; nothing is superseded. The board is stale |

Smaller wording differences, same meaning: R-1 to R-15 end with a period on the board; the doc's R-5 has no "v2".

### §12 Before implementation starts

The seven items match, except item 7 says "all of the build order" where the doc says "all of §11".

The **framing differs**: the board says each question "needs an answer first" and marks five decisions "Needs your
call". The doc's status line and §12 say the questions are settled by default: "Each takes its §9 recommendation
unless the build's brief overrides it." Build against the doc: every decision takes its recommendation.
