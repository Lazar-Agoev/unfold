/**
 * Launches and configured entrances in the UI: the launch editor (add, edit, describe a detected
 * launch, review a deploy-file import) and the confirmation before removing one. Every change goes
 * through Rust's validated `update_project_config`, which keeps `unfold.toml`'s comments and
 * formatting. Screens open them with `openDialog({ kind: "launchEditor" | "removeEntrance", … })`.
 */

export { canBeHandler } from "./locators";
export { LaunchEditorDialog } from "./LaunchEditor";
export { RemoveEntranceDialog } from "./RemoveEntranceDialog";
