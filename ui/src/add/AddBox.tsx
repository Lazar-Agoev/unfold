/**
 * The window's Add box (UI plan §7.2, R-13), mounted once: it registers the `add.*` actions (`A`, ⌘K, `/`;
 * ⇧⌘P commands; ⌘P files; ⌘T functions) and shows the box while an Add request is open. Each request
 * starts a fresh box.
 */

import type { JSX } from "react";
import { ActionIds, useRegisterActions, type ActionSpec } from "../keyboard";
import { openAdd, useAddRequest, type AddRequest } from "../state/overlays";
import { AddLayer } from "./AddLayer";

const ADD_ACTIONS: readonly ActionSpec[] = [
  { id: ActionIds.add, title: "Add…", category: "Go", icon: "plus", run: () => { openAdd(); } },
  { id: ActionIds.addCommands, title: "Commands…", category: "Go", run: () => { openAdd({ mode: "commands" }); } },
  { id: ActionIds.addFiles, title: "Go to File…", category: "Go", icon: "page", run: () => { openAdd({ mode: "find", kind: "files" }); } },
  { id: ActionIds.addFunctions, title: "Go to Function…", category: "Go", icon: "search", run: () => { openAdd({ mode: "find", kind: "functions" }); } },
];

const sessions = new WeakMap<AddRequest, number>();
let lastSession = 0;

/** A number per request object, so a new request remounts the box. */
function sessionOf(request: AddRequest): number {
  let session = sessions.get(request);
  if (session === undefined) {
    lastSession += 1;
    session = lastSession;
    sessions.set(request, session);
  }
  return session;
}

export function AddBox(): JSX.Element | null {
  useRegisterActions(ADD_ACTIONS);
  const request = useAddRequest();
  return request ? <AddLayer key={sessionOf(request)} request={request} /> : null;
}
