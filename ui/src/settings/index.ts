/**
 * Project settings in the UI: the Project settings dialog and Copy Diagnostics. Every change goes
 * through Rust's validated `update_project_config`, which keeps `unfold.toml`'s comments and formatting.
 */

export { copyDiagnostics } from "./diagnostics";
export { ProjectSettingsDialog } from "./ProjectSettingsDialog";
