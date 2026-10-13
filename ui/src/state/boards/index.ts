/**
 * The open boards: sessions keyed by `BoardRef`, pages, edits as undo steps across pages, autosave of
 * `BoardEdit`, view state per page, cards, frames, resource cards, connections, notes, health and
 * repairs, and file operations. Screens import from here.
 */

export {
  addBlock,
  addLink,
  describeCard,
  describeLink,
  describeResource,
  displayedRects,
  editFrame,
  editGroup,
  editNote,
  entranceCardText,
  entranceTitle,
  focusCard,
  focusFrame,
  focusItem,
  labelNode,
  openEntrance,
  removeCard,
  removeFrame,
  removeItem,
  removeLink,
  setLinkKind,
  setLinkLabel,
  traceEntrance,
  ungroup,
  usePageKeys,
} from "./api";
export { CONNECTION_KINDS } from "./connections";
export { NOTE_MAX, findNote, noteLength, placeNote } from "./notes";
export {
  PROPOSAL_REASON_MAX,
  PROPOSAL_TITLE_MAX,
  addProposal,
  findProposal,
  placementProblem,
  placesAfter,
  proposalReason,
  proposalTitle,
  proposalsOf,
  removeProposal,
  updateProposal,
  type NewProposal,
  type ProposalChange,
} from "./proposals";
export { findResource } from "./resources";
export {
  MAX_EXPANSION_DEPTH,
  addCard,
  addGroup,
  callExpansions,
  cardKey,
  findCard,
  findCardByKey,
  findPage,
  keyDisplayName,
  labelTexts,
  moveItems,
  isCallExpanded,
  setCardExpanded,
  setCardView,
  toggleCallExpansion,
  updateGroup,
  type GroupChange,
} from "./document";
export { MAX_FRAME_DEPTH, findFrame, frameItems, frameOf, moveFrame } from "./frames";
export { installAgentChannel, setAgentChannelReady } from "./agent/channel";
export { useWorkingAgents, useWorkingItems, workingText } from "./agent/working";
export { installBoardEvents } from "./events";
export { deleteBoard, deleteFolder, exportPagePng, renameBoard, renameFolder, repairBoard, setBoardFolder } from "./files";
export { itemsFor, kindWord, repairEffect, repairLabel, repairSpec } from "./health";
export { boardKey, cardRefKey, pageKey, sameBoard } from "./keys";
export { lineOffsetIn, followCall, type FollowRequest } from "./navigation";
export { closeBoard, closeBoardsOfProject, newBoard, openBoard, reopenBoards } from "./open";
export { addPage, deletePage, movePage, renamePage } from "./pages";
export {
  canUndo,
  editBoard,
  cardExpanded,
  editPage,
  flushAllBoards,
  flushBoard,
  getOpenBoard,
  getPageView,
  isDirty,
  redo,
  requestCardFocus,
  saveBoard,
  select,
  setActivePage,
  setCamera,
  setLinking,
  pageLens,
  setPageLens,
  shownView,
  type Lens,
  undo,
  updatePageView,
  useBoards,
  useOpenBoard,
  usePageView,
  type EditOptions,
} from "./store";
export type {
  BoardId,
  BoardRef,
  Camera,
  CardSize,
  FocusRequest,
  OpenBoard,
  PageId,
  PageTarget,
  PageView,
  SelectedCalls,
  SelectedLine,
  Selection,
} from "./types";
