# Follow-ups (owner: orchestrator; assign to a later package)

- F-1 (U6) Quit: the main process kills the engine host 5 s after `shutdown`, but Rust's detach waits up to 15 s for
  closing writes. Align: wait up to 16 s for the engine host's shutdown before killing it.
- F-2 (U6) Add Project failures (menu, argv, open-file, second-instance) are only logged. Send a shell event
  `project_add_failed {path, message}` to the focused window and show it as a toast.
- F-3 (e2e) `e2e/src/window.ts` still lists `folder_changed`; rewritten with the journeys.
- F-4 (R4) `SettingsEdit` is tagged `op` (not `kind`): `{op: "set_project_name", name}`. The UI uses `op`.
