/**
 * The window's open boards: sessions keyed by `BoardRef`, pages, edits as
 * undo steps across pages, autosave of `BoardEdit`, view state per page, cards from any connected
 * project, frames, resource cards, connections, notes, health and repairs, file operations, and Move to
 * New Window. Screens import from here.
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
  foldAll,
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
  useResolvedCardProject,
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
  findCardByRef,
  findPage,
  keyDisplayName,
  labelsFor,
  moveItems,
  isCallExpanded,
  setCardExpanded,
  setCardView,
  setCardsFolded,
  toggleCallExpansion,
  updateGroup,
  type GroupChange,
} from "./document";
export { MAX_FRAME_DEPTH, findFrame, frameItems, frameOf, moveFrame } from "./frames";
export { installAgentChannel, setAgentChannelReady } from "./agent/channel";
export { useWorkingAgents, useWorkingItems, workingText } from "./agent/working";
export { installBoardEvents } from "./events";
export {
  copyPageMermaid,
  createFolder,
  deleteBoard,
  deleteFolder,
  duplicateBoard,
  exportPageMermaid,
  exportPagePng,
  isValidName,
  publishBoard,
  publishedPath,
  renameBoard,
  renameFolder,
  repairBoard,
  setBoardFolder,
  slugOf,
  unpublishBoard,
} from "./files";
export { itemsFor, kindWord, repairEffect, repairLabel, repairSpec } from "./health";
export { boardKey, cardRefKey, pageKey, sameBoard } from "./keys";
export { lineOffsetIn, followCall, type FollowRequest } from "./navigation";
export { closeBoard, closeBoardsOfProject, newBoard, openBoard, reclaimBoards, type OpenOutcome } from "./open";
export { addPage, deletePage, duplicatePage, movePage, renamePage } from "./pages";
export {
  canUndo,
  discardChanges,
  editBoard,
  cardExpanded,
  editPage,
  flushAllBoards,
  flushBoard,
  flushBoardView,
  getOpenBoard,
  getPageView,
  isDirty,
  pushFocusMove,
  redo,
  requestCardFocus,
  resolveConflict,
  saveBoard,
  select,
  setActivePage,
  setCamera,
  setLinking,
  pageLens,
  setPageLens,
  shownView,
  type Lens,
  setPendingGroup,
  toggleOpenType,
  toggledOpenTypes,
  travel,
  undo,
  updatePageView,
  useBoards,
  useOpenBoard,
  usePageView,
  type EditOptions,
} from "./store";
export { installTransferReceiver, moveBoardToNewWindow, setTransfersReady } from "./transfer";
export type {
  BoardId,
  BoardRef,
  BoardSession,
  Camera,
  CardSize,
  FocusMove,
  FocusRequest,
  OpenBoard,
  PageId,
  PageTarget,
  PageView,
  PendingGroup,
  SelectedCalls,
  SelectedLine,
  Selection,
  TrailItem,
  WindowId,
} from "./types";
