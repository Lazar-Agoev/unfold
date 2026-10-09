/**
 * The UI's client for the rest of the app (UI plan §4): engine commands, streams, and events typed by
 * the generated module; window commands and shell events typed by the desktop shell; and the query
 * cache with its keys and event-driven invalidation. Screens import from here.
 */

export { queryClient } from "./client";
export { call, callStream, onEngineEvent, type TypedStream } from "./commands";
export type * from "./generated";
export { useConnectionState } from "./hooks";
export {
  engineKey,
  engineQueryOptions,
  installEngineInvalidation,
  resetEngineQueries,
  useEngineQuery,
  type EngineKey,
  type EngineQueryOptions,
} from "./queries";
export { createLatestStream, type LatestStream } from "./stream";
export {
  UnfoldError,
  addDroppedFolder,
  onShellEvent,
  toUnfoldError,
  windowCommand,
  type ConnectionState,
  type DroppedFile,
  type MenuCommand,
  type ShellEvent,
  type WindowInfo,
} from "./unfold";
