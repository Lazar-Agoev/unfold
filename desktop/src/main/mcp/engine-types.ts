/**
 * The engine's command, argument, result, and payload types, as `cargo xtask gen-ipc` generates them
 * from Rust for the UI (`ui/src/ipc/generated/`). The MCP server's tools import them from here, as
 * types only: nothing of the UI is bundled into the main process.
 */

export type * from "../../../../ui/src/ipc/generated";
