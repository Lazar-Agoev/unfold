/**
 * Every tool and prompt the server offers (design §6), in the order agents see them. A new tool is a
 * `defineTool` in this folder, listed here; a new prompt, a `definePrompt`, listed in `PROMPTS`.
 */

import type { Prompt } from "../prompt";
import type { Tool } from "../tool";
import { createBoard, editPages, organizeBoards, show } from "./boards";
import { readCall } from "./calls";
import { getRelations, readCode } from "./code";
import { readFile } from "./files";
import { getGuide } from "./guide";
import { getProject, listProjects } from "./projects";
import { listEntrances, search } from "./search";
import { addProject, compileProject, updateProjectSettings } from "./setup";
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
];

export const PROMPTS: readonly Prompt[] = [];
