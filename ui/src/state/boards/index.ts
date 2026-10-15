/**
 * The open boards: sessions keyed by `BoardRef`, pages, edits as undo steps across pages, autosave of
 * `BoardEdit`, view state per page, cards, frames, resource cards, connections, notes, health and
 * repairs, and file operations. Screens import from here.
 */

export {
  addBlock,
  addBlockWithoutFocus,
  addRef,
  describeCard,
  describeLink,
  describeResource,
  displayedRects,
  editCombined,
  editFrame,
  editNote,
  focusCard,
  focusFrame,
  focusItem,
  nameBlock,
  removeCard,
  removeFrame,
  removeItem,
  removeLink,
  setLinkLabel,
  uncombine,
  usePageKeys,
} from "./api";
export { CONNECTION_KINDS, CONNECTION_WORDS } from "./connections";
export { addResourceCard, combineSteps, drawLink, proposeStep, setLinkKind, setRelationship, type RelationshipChange } from "./explain";
export { cancelLink, clickItem, startLink } from "./linking";
export { NOTE_MAX, findNote, noteLength, placeNote } from "./notes";
export {
  PROPOSAL_REASON_MAX,
  PROPOSAL_TITLE_MAX,
  findProposal,
  proposalReason,
  proposalTitle,
  removeProposal,
  updateProposal,
  type ProposalPlace,
} from "./proposals";
export { findResource } from "./resources";
export {
  addCard,
  cardKey,
  cardView,
  findCard,
  findCardByKey,
  findCombined,
  findPage,
  isOpen,
  moveItems,
  setCardView,
  toggleOpen,
} from "./document";
export { MAX_FRAME_DEPTH, findFrame, frameItems, frameOf, moveFrame } from "./frames";
export { installAgentChannel, setAgentChannelReady } from "./agent/channel";
export { useWorkingAgents, useWorkingItems, workingText } from "./agent/working";
export { installBoardEvents } from "./events";
export { deleteBoard, deleteFolder, exportPagePng, renameBoard, renameFolder, repairBoard, setBoardFolder } from "./files";
export { itemsFor, kindWord, repairEffect, repairLabel, repairSpec } from "./health";
export { boardKey, cardRefKey } from "./keys";
export { closeBoardsOfProject, newBoard, openBoard, reopenBoards } from "./open";
export { addPage, deletePage, movePage, renamePage } from "./pages";
export {
  canUndo,
  editPage,
  getOpenBoard,
  getPageView,
  keepMine,
  loadFile,
  redo,
  requestCardFocus,
  saveBoard,
  select,
  setActivePage,
  setCamera,
  undo,
  updatePageView,
  useBoards,
  useOpenBoard,
  usePageView,
} from "./store";
export type {
  BoardRef,
  Camera,
  PageId,
  PageTarget,
  SelectedCalls,
  SelectedLine,
  Selection,
} from "./types";
