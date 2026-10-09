/**
 * The engine commands and events the board store, canvas, and cards use. A seam over the typed
 * client (`ipc/commands`), so tests can script the engine by replacing this one module.
 */

export { call, onEngineEvent } from "../../ipc/commands";
