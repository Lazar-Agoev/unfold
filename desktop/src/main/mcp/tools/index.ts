/**
 * Every tool the server offers, in the order agents see them. A new tool is a
 * `defineTool` in this folder, listed here.
 */

import type { Tool } from "../tool";
import { createBoard, editPages, organizeBoards, show } from "./boards";
import { addCards, arrangePage, removeItems, setView } from "./build";
import { readCall } from "./calls";
import { shapeCard } from "./card";
import { getRelations, readCode } from "./code";
import { removeEntrance, setApp, setEntrance } from "./entrances";
import { readFile } from "./files";
import { finishWork, repair } from "./finish";
import { getGuide } from "./guide";
import { listBoards, readPage } from "./pages";
import { getProject, listProjects } from "./projects";
import { check, getScreenshot } from "./review";
import { listEntrances, search } from "./search";
import { addProject, compileProject, updateProjectSettings } from "./setup";
import { addNotes, addResources, connect, describe, frameCards, propose } from "./shape";
import { getCurrentView } from "./view";

export const TOOLS: readonly Tool[] = [
  getGuide,
  getCurrentView,
  listProjects,
  getProject,
  addProject,
  compileProject,
  updateProjectSettings,
  setApp,
  setEntrance,
  removeEntrance,
  createBoard,
  organizeBoards,
  editPages,
  show,
  search,
  listEntrances,
  readCode,
  readCall,
  readFile,
  getRelations,
  listBoards,
  readPage,
  check,
  getScreenshot,
  addCards,
  setView,
  arrangePage,
  removeItems,
  shapeCard,
  propose,
  frameCards,
  addResources,
  connect,
  addNotes,
  describe,
  repair,
  finishWork,
];
