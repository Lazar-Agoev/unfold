/**
 * The UI's client for the rest of the app: engine commands, streams, and events typed by
 * the generated module; window commands and shell events typed by the desktop shell; and the query
 * cache with its keys and event-driven invalidation. Screens import from here.
 */

export { queryClient } from "./client";
export { call, onEngineEvent } from "./commands";
export type * from "./generated";
export {
  engineKey,
  engineQueryOptions,
  installEngineInvalidation,
  useEngineQuery,
} from "./queries";
export { createLatestStream, type LatestStream } from "./stream";
export {
  addDroppedFolder,
  onShellEvent,
  toUnfoldError,
  windowCommand,
  type DroppedFile,
  type MenuCommand,
} from "./unfold";
