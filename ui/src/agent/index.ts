/**
 * Agents in the window, apart from any one board: the agent mark and bylines, Copy link, and the view
 * reports that tell the main process what this window shows. Agent edits of a board live with the
 * board sessions (`state/boards/agent/`).
 */

export { AgentMark, Author } from "./AgentMark";
export { copyLink } from "./links";
export { installViewReports } from "./viewReport";
