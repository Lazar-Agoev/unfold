# Follow-ups (owner: orchestrator; assign to a later package)

- F-1 (U6) Quit: the main process kills the engine host 5 s after `shutdown`, but Rust's detach waits up to 15 s for
  closing writes. Align: wait up to 16 s for the engine host's shutdown before killing it.
- F-2 (U6) Add Project failures (menu, argv, open-file, second-instance) are only logged. Send a shell event
  `project_add_failed {path, message}` to the focused window and show it as a toast.
- F-3 (e2e) `e2e/src/window.ts` still lists `folder_changed`; rewritten with the journeys.
- F-4 (R4) `SettingsEdit` is tagged `op` (not `kind`): `{op: "set_project_name", name}`. The UI uses `op`.
- F-5 (polish, Rust+U4) `search` FileHit lacks a definitions count ("6 definitions" in 04); add `definitions: number`
  (indexed files; null otherwise) and show it. **Done:** one grouped store query per project for the page.
- F-6 (polish, Rust+U4) entrance SearchHits lack their application's name ("storefront-api · worker" in 04); add
  `application: string | null` and show `project · application`. **Done:** one grouped query per project; a launch
  names the application it starts; a row without one shows the declaring file.
- F-7 (polish, U4) preview shows 10 lines, the design 5; match the design. **Done:** block previews show 5 lines
  and `preview` returns at most 5; file previews keep V-63's 12.
- F-8 (polish) screenshot.mjs takes one --project; cross-project screenshots need a repeatable --project.
- F-9 (polish, Rust+U3) WorkspaceView.removed lacks removedAtMs for first launch's "last opened" column; add it and show it. **Done.**
- F-10 (polish, U3) state/compile.ts module state leaks between tests (U3 note); check when sweeping.
