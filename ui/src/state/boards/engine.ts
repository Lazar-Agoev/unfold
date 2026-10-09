/**
 * The engine and window commands the board store uses. A seam over the typed client, so tests can
 * script the engine by replacing this one module.
 */

export { call, onEngineEvent } from "../../ipc/commands";
export { windowCommand } from "../../ipc/unfold";
