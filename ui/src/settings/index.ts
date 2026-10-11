/**
 * Project settings in the UI: the Project settings dialog, the manual entrance editor
 * and its remove confirmation, and Copy Diagnostics. Every change goes through Rust's validated
 * `update_project_config`, which keeps `unfold.toml`'s comments and formatting.
 */

export { copyDiagnostics } from "./diagnostics";
export { EntranceEditorDialog, RemoveEntranceDialog } from "./EntranceEditor";
export { ProjectSettingsDialog } from "./ProjectSettingsDialog";
