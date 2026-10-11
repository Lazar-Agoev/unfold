/**
 * Where the journeys exercise each row of `docs/ui-shell.md`'s "Where V1's parts went" table and its
 * keyboard table: the row's first column (markdown removed) → the spec, test, and step that use the
 * V1 part's new home or run the action. `journeys/coverage.spec.ts` checks it against the document and
 * the spec sources.
 */

export interface Home {
  /** The spec file under `e2e/journeys/`. */
  readonly spec: string;
  /** The test's title, exactly. */
  readonly test: string;
  /** The step's name, exactly. */
  readonly step: string;
}

const FIRST_LAUNCH = "first-launch.spec.ts";
const EMPTY = "an empty workspace; Add project, ⌘O, a dropped folder, a dropped file, and a second launch";
const NOT_COMPILED = "the not-compiled Home: what it reads and skips, the scope tree, and the first Compile";

const HOME = "home.spec.ts";
const COMPILED_HOME = "a compiled project's Home: header, How it starts, Trace, Boards, the sidebar tree, Recents and Drafts";
const HEALTH = "Health and activity: a file that doesn't parse, the Health list, and the Activity log";

const BOARD = "board.spec.ts";
const READING = "follow a call, On this page, Selected, the trail with Back and Forward, the lens, and ⌥-click inline";
const AUTHORING = "pages, descriptions, groups, Flow labels, and undo and redo across pages";
const CANVAS = "Show callers, a drawn link, Arrange, the zoom and Fit, and the canvas and card menus";

const BOARDS = "boards.spec.ts";
const BOARD_MENU = "the board menu: rename in place, Duplicate, Share as Mermaid, Fold all, and Delete";
const OUTSIDE = "a board's file edited outside the app: a clean board reloads, a deleted file shows the banner; a failed save offers Retry";

const PUBLISH = "publish.spec.ts";
const PUBLISHING = "a draft, its first Publish, unpublished changes, Discard changes, and Move back to drafts";

const ADD = "add.spec.ts";
const ADD_ON_BOARD = "on a board: A, ⌘K and / open it, the map, Enter and ⌘Enter add, Escape closes";
const ADD_SEARCH = "search across two projects: kind tabs and counts, the preview, Look in, and Enter from Home";
const ADD_TEXT = "text search: match case, whole word, and regular expressions";
const ADD_COMMANDS = "commands: `>`, ⇧⌘P, ⌘P on Files, ⌘T on Functions, and Copy Diagnostics";

const PEEK = "peek.spec.ts";
const PEEK_FILE = "a file from Add: folds, ⊕ and call links add to the page, the pin, the menu, and Escape";

const SETTINGS = "settings.spec.ts";
const PROJECT_SETTINGS = "Scope excludes a folder, Detectors turn one off, and General renames the project";
const ENTRANCES = "add a manual entrance from the project menu, edit and remove it from Home, and start one from a card";
const PREFERENCES = "the theme applies at once and is saved; a key binding is stored by action id";

const PROJECT_MENU = "project-menu.spec.ts";
const MENU_ITEMS = "Compile again, Rebuild from scratch, Health and activity, Settings…, Add entrance…, Reveal in Finder, Copy diagnostics";
const CANCEL = "Cancel stops a Rebuild from scratch before it commits";
const PAUSE = "Pause background updates holds an edit back until Resume";
const REMOVE = "Remove from workspace… asks, keeps .unfold/, and the folder can be added back";

const BACKGROUND = "background.spec.ts";
const SAVED = "a saved file updates the graph and the open card; a described card asks for review";
const READ_ONLY = "a read-only folder keeps its graph in the fallback location and everything works";

const WINDOWS = "windows.spec.ts";
const NEW_WINDOW = "New Window; two windows with their own boards both update; a held board focuses its window";
const MOVE_WINDOW = "Open in new window, from a tab and from the board menu, keeps the board's undo history";
const RESTORE = "quitting with two windows and relaunching restores both, with their tabs";
const RESTART = "a killed engine host reconnects every window without losing an edit";

const KEYBOARD = "keyboard.spec.ts";
const SHORTCUTS = "every shortcut of the keyboard map, once";
const APP_MENU = "the application menu's File, View, and Go items";

export const MAP_HOMES: Readonly<Record<string, readonly Home[]>> = {
  "Title bar folder switcher (recent folders, Open Folder, New Window, Folder Settings, Close Folder, Preferences)": [
    { spec: FIRST_LAUNCH, test: EMPTY, step: "Add project… adds the chosen folder and shows it, not compiled, writing nothing" },
    { spec: KEYBOARD, test: APP_MENU, step: "File › Add Project… adds the chosen folder" },
    { spec: HOME, test: COMPILED_HOME, step: "a project row shows the project's overview on Home" },
    { spec: HOME, test: COMPILED_HOME, step: "Recents lists the boards opened lately, across projects" },
    { spec: PROJECT_MENU, test: MENU_ITEMS, step: "the menu on Home's header lists every item, with ⇧⌘B on Compile again and Cancel disabled" },
    { spec: WINDOWS, test: NEW_WINDOW, step: "⇧⌘N opens another window on the same workspace and project" },
    { spec: PROJECT_MENU, test: REMOVE, step: "answering Remove takes it out of the workspace; its .unfold/ stays; Home shows the other project" },
    { spec: SETTINGS, test: PREFERENCES, step: "the sidebar's gear opens Preferences; Dark applies at once and Save keeps it" },
    { spec: SETTINGS, test: PREFERENCES, step: "⌘, opens Preferences; Light previews, Cancel returns to the saved theme" },
    { spec: SETTINGS, test: PREFERENCES, step: "Keyboard: Toggle Sidebar rebound to ⇧⌘Y is stored under view.toggleSidebar" },
  ],
  "Title bar board title, Code | Flow, Compile, Publish": [
    { spec: BOARDS, test: BOARD_MENU, step: "double-clicking the title in the sidebar header renames the board in place" },
    { spec: BOARD, test: READING, step: "the lens in the top bar shows every card as a Diagram node, in Flow, or in Code, and ⌘1 / ⌘2 / ⌘3 switch it" },
    { spec: FIRST_LAUNCH, test: NOT_COMPILED, step: "Compile writes the scope, shows progress, and fills Home in" },
    { spec: PROJECT_MENU, test: MENU_ITEMS, step: "Compile again runs a compile that the Activity log records" },
    { spec: PROJECT_MENU, test: MENU_ITEMS, step: "Rebuild from scratch rebuilds the graph" },
    { spec: PROJECT_MENU, test: CANCEL, step: "Cancel returns the project to its graph, and nothing is committed" },
    { spec: PUBLISH, test: PUBLISHING, step: "Publish (top bar) writes the changes over the published file" },
  ],
  "Inspector, and its toggle": [
    { spec: BOARD, test: READING, step: "Selected shows the card's name, location, Called by, Reached from, and calls not opened" },
    { spec: BOARD, test: AUTHORING, step: "a description written in Selected shows on the card and is saved in the draft" },
    { spec: BOARD, test: AUTHORING, step: "double-clicking the description on the card edits it in place" },
    { spec: BOARD, test: AUTHORING, step: "a group selected from On this page shows in Selected, where Ungroup removes it" },
    { spec: BOARD, test: AUTHORING, step: "Label them steps to an unlabeled Flow node and focuses its label in Selected" },
    { spec: KEYBOARD, test: SHORTCUTS, step: "⌘B hides and shows the sidebar" },
    { spec: KEYBOARD, test: APP_MENU, step: "View › Home shows Home; View › Toggle Sidebar hides and shows the sidebar" },
  ],
  "Board menu ⋯": [
    { spec: BOARDS, test: BOARD_MENU, step: "Rename… in the board menu renames it in the header too" },
    { spec: BOARDS, test: BOARD_MENU, step: "Share › Export Mermaid… writes the page through the save dialog" },
    { spec: BOARDS, test: BOARD_MENU, step: "Share › Copy Mermaid puts the same page on the clipboard" },
    { spec: BOARDS, test: BOARD_MENU, step: "Share › Export PNG… draws the page in the render window and writes it through the save dialog" },
    { spec: BOARDS, test: BOARD_MENU, step: "Fold all cards and Unfold all cards" },
    { spec: BOARD, test: CANVAS, step: "the canvas menu folds and unfolds every card" },
    { spec: BOARD, test: CANVAS, step: "the card menu folds a card and removes it from the page" },
    { spec: BOARD, test: CANVAS, step: "Draw link… joins two cards with a drawn link, labeled in Selected" },
    { spec: WINDOWS, test: MOVE_WINDOW, step: "the board menu's Open in new window moves it on, with its history" },
    { spec: PUBLISH, test: PUBLISHING, step: "Move back to drafts makes it a draft again under a new id" },
    { spec: BOARDS, test: BOARD_MENU, step: "Delete… asks, then deletes the file and closes the tab" },
  ],
  "Activity bar": [{ spec: FIRST_LAUNCH, test: EMPTY, step: "the window is tabs, one sidebar, and the content: no activity bar, inspector, panel, or status bar" }],
  "Files view (the disk tree and its marks and menu)": [
    { spec: ADD, test: ADD_COMMANDS, step: "⌘P opens Add on Files" },
    { spec: PEEK, test: PEEK_FILE, step: "a file chosen in Add opens in the sheet, whole and read-only" },
    { spec: SETTINGS, test: PROJECT_SETTINGS, step: "Scope: unchecking a folder in a lazily loaded level stages an exclude rule" },
    { spec: PEEK, test: PEEK_FILE, step: "the sheet's menu offers Open in editor and Reveal in Finder" },
    { spec: BOARD, test: CANVAS, step: "Show callers on a card adds the cards that call it" },
  ],
  "Entrances view": [
    { spec: HOME, test: COMPILED_HOME, step: "How it starts groups the entrances by application" },
    { spec: ADD, test: ADD_ON_BOARD, step: "the map shows each application's entrances and marks the one on the page" },
    { spec: SETTINGS, test: ENTRANCES, step: "project menu › Add entrance… opens the editor; a command entrance is saved to config.toml" },
    { spec: SETTINGS, test: ENTRANCES, step: "Home's Add entrance opens the editor too" },
    { spec: SETTINGS, test: ENTRANCES, step: "its row's menu Edit… changes it" },
    { spec: SETTINGS, test: ENTRANCES, step: "its row's menu Remove… removes it after asking" },
    { spec: SETTINGS, test: ENTRANCES, step: "a card's Add as entrance… prefills the handler with the card's definition" },
  ],
  "Search view (and match case, whole word, regex)": [
    { spec: ADD, test: ADD_SEARCH, step: "typing searches every project, with counts per kind and each row's project" },
    { spec: ADD, test: ADD_SEARCH, step: "Tab and ⇧Tab switch the kind" },
    { spec: ADD, test: ADD_TEXT, step: "Match case keeps only the lines with Retry as typed" },
    { spec: ADD, test: ADD_TEXT, step: "Match whole word drops retryFailed" },
    { spec: ADD, test: ADD_TEXT, step: "a regular expression matches by pattern" },
  ],
  "Boards view (boards, drafts, outline, the Flow-label stepper, groups, health and repairs)": [
    { spec: HOME, test: COMPILED_HOME, step: "Home's Boards shows the board as a card with a thumbnail of its first page" },
    { spec: HOME, test: COMPILED_HOME, step: "the sidebar lists the project's boards, and a board row opens its board" },
    { spec: PUBLISH, test: PUBLISHING, step: "Drafts in the sidebar lists it" },
    { spec: BOARD, test: READING, step: "On this page lists the traced cards in follow order" },
    { spec: BOARD, test: AUTHORING, step: "Label them steps to an unlabeled Flow node and focuses its label in Selected" },
    { spec: BOARD, test: AUTHORING, step: "a group selected from On this page shows in Selected, where Ungroup removes it" },
    { spec: BACKGROUND, test: SAVED, step: "its code changing shows the review banner, the row's mark, and the page's mark" },
    { spec: BACKGROUND, test: SAVED, step: "Review explains the problem and a repair clears it" },
    { spec: HOME, test: HEALTH, step: "Health and activity lists the file that doesn't parse, with Open" },
    { spec: BOARDS, test: OUTSIDE, step: "deleting the open board's file shows the banner at the top of the canvas" },
    { spec: BOARDS, test: OUTSIDE, step: "Keep mine on the banner writes the board again" },
    { spec: BOARDS, test: OUTSIDE, step: "a save that fails shows Not saved with Retry, and Retry saves once it can" },
  ],
  "Editor tabs; source tabs": [
    { spec: HOME, test: COMPILED_HOME, step: "Trace opens an entrance on a new page of a new draft board" },
    { spec: KEYBOARD, test: SHORTCUTS, step: "⌃Tab and ⌃⇧Tab, ⌘⇧] and ⌘⇧[ move between tabs" },
    { spec: WINDOWS, test: MOVE_WINDOW, step: "the tab's Open in new window moves the board to a new window" },
    { spec: PEEK, test: PEEK_FILE, step: "a file chosen in Add opens in the sheet, whole and read-only" },
    { spec: PEEK, test: PEEK_FILE, step: "pinned, the sheet narrows the canvas instead of covering it, and Escape still closes it" },
  ],
  "The board bar (Back, trail, zoom, Fit, Arrange)": [
    { spec: BOARD, test: READING, step: "Back and Forward in the dock move along the trail" },
    { spec: BOARD, test: CANVAS, step: "the dock's zoom buttons change the zoom, and Fit returns to the whole page" },
    { spec: BOARD, test: CANVAS, step: "Arrange orders the cards by call depth, with Undo in its toast" },
  ],
  "Bottom panel: Problems, Index log, References": [
    { spec: HOME, test: HEALTH, step: "the Health line sums the problems up, each with its action" },
    { spec: HOME, test: HEALTH, step: "Health and activity lists the file that doesn't parse, with Open" },
    { spec: HOME, test: HEALTH, step: "Activity shows the index log with the Compile" },
    { spec: BOARD, test: READING, step: "Selected shows the card's name, location, Called by, Reached from, and calls not opened" },
    { spec: BOARD, test: CANVAS, step: "Show callers on a card adds the cards that call it" },
  ],
  "Status bar (graph state, notes, connection, problem count, zoom) and its menu": [
    { spec: HOME, test: COMPILED_HOME, step: "the header shows the path, the name, the branch, and the graph's state in words" },
    { spec: HOME, test: HEALTH, step: "the project row and the state line say a file doesn't parse" },
    { spec: BACKGROUND, test: READ_ONLY, step: "the Compile works and Home's state line says it uses the fallback location" },
    { spec: WINDOWS, test: RESTART, step: "an edit is waiting to be saved when the engine host dies" },
    { spec: BOARD, test: CANVAS, step: "the dock's zoom buttons change the zoom, and Fit returns to the whole page" },
    { spec: PROJECT_MENU, test: MENU_ITEMS, step: "Compile again runs a compile that the Activity log records" },
    { spec: PROJECT_MENU, test: MENU_ITEMS, step: "Rebuild from scratch rebuilds the graph" },
    { spec: PROJECT_MENU, test: CANCEL, step: "Cancel returns the project to its graph, and nothing is committed" },
    { spec: PROJECT_MENU, test: PAUSE, step: "Pause background updates: the row shows the pause glyph and Home says so" },
    { spec: PROJECT_MENU, test: PAUSE, step: "the project menu now offers Resume background updates; Resume compiles the edit" },
    { spec: PROJECT_MENU, test: MENU_ITEMS, step: "Health and activity opens Home's Health view; Activity lists the rebuild" },
    { spec: PROJECT_MENU, test: MENU_ITEMS, step: "Copy diagnostics copies a report with versions and counts, and no paths" },
    { spec: PROJECT_MENU, test: MENU_ITEMS, step: "Help › Copy Diagnostics in the application menu copies it too" },
    { spec: ADD, test: ADD_COMMANDS, step: "Copy Diagnostics is one of Add's commands" },
  ],
  "Welcome screen; first-Compile screen": [
    { spec: FIRST_LAUNCH, test: EMPTY, step: "the first launch shows an empty workspace" },
    { spec: FIRST_LAUNCH, test: EMPTY, step: "a folder dropped on the window is added" },
    { spec: PROJECT_MENU, test: REMOVE, step: "first launch lists both folders under Opened before; Add brings one back, still compiled" },
    { spec: FIRST_LAUNCH, test: NOT_COMPILED, step: "a folder on the command line is added and shown, not compiled" },
    { spec: FIRST_LAUNCH, test: NOT_COMPILED, step: "Skipped says what isn't read and why" },
    { spec: FIRST_LAUNCH, test: NOT_COMPILED, step: "unchecking a top-level folder in the scope tree leaves it out" },
    { spec: FIRST_LAUNCH, test: NOT_COMPILED, step: "Compile writes the scope, shows progress, and fills Home in" },
  ],
  "Close Folder": [
    { spec: PROJECT_MENU, test: REMOVE, step: "answering Cancel keeps the project" },
    { spec: PROJECT_MENU, test: REMOVE, step: "answering Remove takes it out of the workspace; its .unfold/ stays; Home shows the other project" },
    { spec: KEYBOARD, test: APP_MENU, step: "File › Remove Project asks, then removes the project Home shows" },
  ],
  "Folder Settings dialog": [
    { spec: SETTINGS, test: PROJECT_SETTINGS, step: "Home's Settings opens the settings dialog on General" },
    { spec: SETTINGS, test: PROJECT_SETTINGS, step: "Save writes the rule to config.toml, keeping its comments, and a background update drops the folder" },
    { spec: SETTINGS, test: PROJECT_SETTINGS, step: "Detectors: turning node-cron schedules off removes the schedules from How it starts" },
    { spec: SETTINGS, test: PROJECT_SETTINGS, step: "General: a new shared name renames the project everywhere and writes [project] name" },
  ],
  "Command palette, Go to File, Go to Symbol": [
    { spec: ADD, test: ADD_COMMANDS, step: "typing > switches Add to commands, grouped" },
    { spec: ADD, test: ADD_COMMANDS, step: "⇧⌘P opens Add in commands mode" },
    { spec: ADD, test: ADD_COMMANDS, step: "⌘P opens Add on Files" },
    { spec: ADD, test: ADD_COMMANDS, step: "⌘T opens Add on Functions" },
    { spec: KEYBOARD, test: APP_MENU, step: "Go › Add opens Add" },
  ],
  "Several windows on one folder": [
    { spec: WINDOWS, test: NEW_WINDOW, step: "⇧⌘N opens another window on the same workspace and project" },
    { spec: WINDOWS, test: NEW_WINDOW, step: "opening a board another window holds focuses that window and its tab" },
    { spec: WINDOWS, test: MOVE_WINDOW, step: "⌘Z in the new window undoes the edit made in the first" },
    { spec: WINDOWS, test: RESTORE, step: "each window's tabs and active tab come back, and its boards reopen" },
  ],
};

/** The steps that run each keyboard action: its default shortcut, or, with none, its menu item or Add's command. */
export const KEYBOARD_HOMES: Readonly<Record<string, readonly Home[]>> = {
  "Add…": [{ spec: KEYBOARD, test: SHORTCUTS, step: "A, ⌘K, and / open Add; ⇧⌘P opens it on commands; ⌘P on Files; ⌘T on Functions" }],
  "Commands… (Add with >)": [{ spec: KEYBOARD, test: SHORTCUTS, step: "A, ⌘K, and / open Add; ⇧⌘P opens it on commands; ⌘P on Files; ⌘T on Functions" }],
  "Go to File…, Go to Function…": [{ spec: KEYBOARD, test: SHORTCUTS, step: "A, ⌘K, and / open Add; ⇧⌘P opens it on commands; ⌘P on Files; ⌘T on Functions" }],
  Home: [{ spec: KEYBOARD, test: SHORTCUTS, step: "⌘N makes a new board; ⌘0 shows Home; ⌘W closes a tab" }],
  "Next Tab, Previous Tab": [{ spec: KEYBOARD, test: SHORTCUTS, step: "⌃Tab and ⌃⇧Tab, ⌘⇧] and ⌘⇧[ move between tabs" }],
  "Close Tab": [{ spec: KEYBOARD, test: SHORTCUTS, step: "⌘N makes a new board; ⌘0 shows Home; ⌘W closes a tab" }],
  "New Board": [{ spec: KEYBOARD, test: SHORTCUTS, step: "⌘N makes a new board; ⌘0 shows Home; ⌘W closes a tab" }],
  "New Page": [{ spec: KEYBOARD, test: SHORTCUTS, step: "⌥⌘N adds a page; ⌘Z and ⇧⌘Z undo and redo it" }],
  "New Window": [{ spec: KEYBOARD, test: SHORTCUTS, step: "⇧⌘N opens a new window" }],
  "Close Window": [{ spec: KEYBOARD, test: SHORTCUTS, step: "⇧⌘W closes the new window" }],
  "Add Project…": [{ spec: KEYBOARD, test: SHORTCUTS, step: "⌘O adds a project; ⇧⌘B compiles it from its not-compiled Home" }],
  "Compile (the selected project, or the active board's)": [
    { spec: KEYBOARD, test: SHORTCUTS, step: "⌘O adds a project; ⇧⌘B compiles it from its not-compiled Home" },
    { spec: KEYBOARD, test: SHORTCUTS, step: "⇧⌘B compiles the board's project" },
  ],
  "Diagram, Flow, Code (the page's lens)": [{ spec: KEYBOARD, test: SHORTCUTS, step: "⌘1, ⌘2, and ⌘3 set the page's lens to Diagram, Flow, and Code" }],
  "Toggle Sidebar": [{ spec: KEYBOARD, test: SHORTCUTS, step: "⌘B hides and shows the sidebar" }],
  "Back, Forward": [{ spec: KEYBOARD, test: SHORTCUTS, step: "⌘[ and ⌘] go back and forward" }],
  Fit: [{ spec: KEYBOARD, test: SHORTCUTS, step: "⇧1 fits; ⌘= and ⌘- zoom in and out" }],
  "Zoom In, Zoom Out": [{ spec: KEYBOARD, test: SHORTCUTS, step: "⇧1 fits; ⌘= and ⌘- zoom in and out" }],
  "Arrange by Call Depth": [{ spec: KEYBOARD, test: SHORTCUTS, step: "⇧A arranges the page by call depth" }],
  "Undo, Redo (board edit)": [{ spec: KEYBOARD, test: SHORTCUTS, step: "⌥⌘N adds a page; ⌘Z and ⇧⌘Z undo and redo it" }],
  "Preferences…": [{ spec: KEYBOARD, test: SHORTCUTS, step: "⇧⌘E opens Publish; ⌘, opens Preferences" }],
  Publish: [{ spec: KEYBOARD, test: SHORTCUTS, step: "⇧⌘E opens Publish; ⌘, opens Preferences" }],
  "Remove Project from Workspace…": [
    { spec: KEYBOARD, test: APP_MENU, step: "File › Remove Project asks, then removes the project Home shows" },
    { spec: PROJECT_MENU, test: REMOVE, step: "answering Remove takes it out of the workspace; its .unfold/ stays; Home shows the other project" },
  ],
  "Copy Diagnostics": [
    { spec: PROJECT_MENU, test: MENU_ITEMS, step: "Help › Copy Diagnostics in the application menu copies it too" },
    { spec: ADD, test: ADD_COMMANDS, step: "Copy Diagnostics is one of Add's commands" },
  ],
};
