/**
 * Command scopes route a command to the part of the window the person is working in. ⌘Z undoes a
 * board edit when the board has focus (or was clicked last), while a focused text field keeps its own
 * native undo. A scope is a DOM element plus the commands it handles; the innermost scope around the
 * focused element (or, with nothing focused, the last element clicked) wins.
 */

import { useEffect, useRef, type RefObject } from "react";

/** Commands routed to scopes. Strings so views can route their own. */
export type ScopedCommand = "undo" | "redo" | (string & {});

export type ScopeHandlers = Readonly<Partial<Record<ScopedCommand, (() => void) | undefined>>>;

interface Scope {
  readonly element: HTMLElement;
  readonly handlers: () => ScopeHandlers;
}

const scopes = new Set<Scope>();
let lastPointerTarget: Node | null = null;
let tracking = false;

function trackPointer(): void {
  if (tracking || typeof document === "undefined") {
    return;
  }
  tracking = true;
  document.addEventListener(
    "pointerdown",
    (event) => {
      lastPointerTarget = event.target instanceof Node ? event.target : null;
    },
    { capture: true },
  );
}

/** Where the person is working: the focused element, or the last thing clicked when focus is on body. */
function workingElement(): Node | null {
  const active = document.activeElement;
  if (active && active !== document.body) {
    return active;
  }
  return lastPointerTarget?.isConnected ? lastPointerTarget : null;
}

/** The handler for `command` in the innermost scope around where the person is working. */
export function scopedHandler(command: ScopedCommand): (() => void) | null {
  const target = workingElement();
  if (!target) {
    return null;
  }
  let best: Scope | null = null;
  for (const scope of scopes) {
    if (scope.element.contains(target) && (!best || best.element.contains(scope.element))) {
      best = scope;
    }
  }
  return best?.handlers()[command] ?? null;
}

/** Runs `command` in the working scope. Returns whether a scope handled it. */
export function runScoped(command: ScopedCommand): boolean {
  const handler = scopedHandler(command);
  handler?.();
  return handler !== null;
}

/** Registers a DOM element as a scope (for code outside React). Returns the unregister function. */
export function registerScope(element: HTMLElement, handlers: () => ScopeHandlers): () => void {
  trackPointer();
  const scope: Scope = { element, handlers };
  scopes.add(scope);
  return () => {
    scopes.delete(scope);
  };
}

/**
 * Makes the element behind `ref` a command scope while mounted. Handlers are read when the command
 * runs, so they may close over current state; a missing handler means the scope can't run it now.
 */
export function useCommandScope(ref: RefObject<HTMLElement | null>, handlers: ScopeHandlers): void {
  const latest = useRef(handlers);
  useEffect(() => {
    latest.current = handlers;
  });
  useEffect(() => {
    const element = ref.current;
    return element ? registerScope(element, () => latest.current) : undefined;
  }, [ref]);
}
