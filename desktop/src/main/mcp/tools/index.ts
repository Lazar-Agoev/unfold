/**
 * Every tool and prompt the server offers (design §6), in the order agents see them. A new tool is a
 * `defineTool` in this folder, listed here; a new prompt, a `definePrompt`, listed in `PROMPTS`.
 */

import type { Prompt } from "../prompt";
import type { Tool } from "../tool";
import { createBoard, editPages, organizeBoards, show } from "./boards";
import { addCards, arrangePage, removeItems, setView } from "./build";
import { readCall } from "./calls";
import { getRelations, readCode } from "./code";
import { readFile } from "./files";
import { finishWork, repair } from "./finish";
import { getGuide } from "./guide";
import { listBoards, readPage } from "./pages";
import { dataModel, explainEntrance, fixBoards, serviceMap, simplifyFunction, tour } from "./prompts";
import { getProject, listProjects } from "./projects";
import { check, getScreenshot } from "./review";
import { listEntrances, search } from "./search";
import { addProject, compileProject, updateProjectSettings } from "./setup";
import { addNotes, addResources, connect, describe, frameCards, groupSteps, labelSteps, propose } from "./shape";
import { getCurrentView } from "./view";

export const TOOLS: readonly Tool[] = [
  getGuide,
  getCurrentView,
  listProjects,
  getProject,
  addProject,
  compileProject,
  updateProjectSettings,
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
  groupSteps,
  frameCards,
  addResources,
  connect,
  addNotes,
  describe,
  labelSteps,
  propose,
  repair,
  finishWork,
];

export const PROMPTS: readonly Prompt[] = [explainEntrance, simplifyFunction, serviceMap, dataModel, tour, fixBoards];
