/**
 * Every tool and prompt the server offers (design §6), in the order agents see them. A new tool is a
 * `defineTool` in this folder, listed here; a new prompt, a `definePrompt`, listed in `PROMPTS`.
 */

import type { Prompt } from "../prompt";
import type { Tool } from "../tool";
import { getGuide } from "./guide";
import { getProject, listProjects } from "./projects";
import { getCurrentView } from "./view";

export const TOOLS: readonly Tool[] = [getGuide, getCurrentView, listProjects, getProject];

export const PROMPTS: readonly Prompt[] = [];
