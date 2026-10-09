/**
 * The project the code inside a card (or the peek sheet) belongs to, for what it reads by key: a call
 * link's hover asks `peek {project, key}`. Each card provides its resolved project.
 */

import { createContext, useContext } from "react";
import type { ProjectId } from "../ipc/generated";

export const ProjectScope = createContext<ProjectId | null>(null);

/** The enclosing card's project, or null outside one. */
export function useScopeProject(): ProjectId | null {
  return useContext(ProjectScope);
}
