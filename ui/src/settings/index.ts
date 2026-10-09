/**
 * Folder settings in the UI: the manual entrance editor, the Folder Settings dialog (Python
 * projects, detectors, scope), and Copy Diagnostics. Every change goes through Rust's validated
 * `update_folder_config`, which keeps `.unfold/config.toml`'s comments and formatting.
 */

export { addAsEntrance, addAsEntranceItem } from "./actions";
export { copyDiagnostics } from "./diagnostics";
export { manualTableOf } from "./entrance";
export { type DefinitionRef } from "./locators";
export { SettingsDialogs } from "./SettingsDialogs";
export { confirmRemoveEntrance, openEntranceEditor, openFolderSettings, type ManualRow } from "./store";
