/**
 * The window's open boards (UI plan §3.4–§3.7, §3.10): sessions keyed by `BoardRef`, pages, edits as
 * undo steps across pages, autosave of `BoardEdit`, view state per page, cards from any connected
 * project, health and repairs, file operations, and Move to New Window. Screens import from here.
 */

export {
  addBlock,
  addLink,
  describeCard,
  displayedRects,
  editGroup,
  entranceTitle,
  focusCard,
  foldAll,
  labelNode,
  openEntrance,
  removeCard,
  removeLink,
  setLinkLabel,
  traceEntrance,
  ungroup,
  usePageKeys,
  useResolvedCardProject,
} from "./api";
export {
  addCard,
  addGroup,
  cardKey,
  findCard,
  findCardByRef,
  findPage,
  keyDisplayName,
  labelsFor,
  moveCards,
  setCardView,
  setCardsFolded,
  updateGroup,
  type GroupChange,
} from "./document";
export { installBoardEvents } from "./events";
export {
  copyPageMermaid,
  createFolder,
  deleteBoard,
  deleteFolder,
  duplicateBoard,
  exportPageMermaid,
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
  amendPage,
  canUndo,
  discardChanges,
  editBoard,
  editPage,
  effectiveView,
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
  setPageLens,
  setPendingGroup,
  setPendingPlacement,
  travel,
  undo,
  updatePageView,
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
  PendingPlacement,
  Selection,
  TrailItem,
  WindowId,
} from "./types";
