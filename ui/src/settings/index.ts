/**
 * Project settings in the UI: the manual entrance editor and its remove confirmation, and Copy
 * Diagnostics. Every change goes through Rust's validated `update_project_config`, which keeps
 * `.unfold/config.toml`'s comments and formatting.
 */

export { copyDiagnostics } from "./diagnostics";
export { EntranceEditorDialog, RemoveEntranceDialog } from "./EntranceEditor";
